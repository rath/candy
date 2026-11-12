//! Byte limits before image decoding or SVG parsing allocates native buffers.
use anyhow::{Context, Result};
use std::{fmt, fs::File, io::Read, path::Path};

/// Encoded input limit, independent of the decoded pixel limit.
pub const MAX_INPUT_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug)]
pub struct InputTooLarge {
    pub limit: usize,
}
impl fmt::Display for InputTooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Input exceeds the {} byte limit", self.limit)
    }
}
impl std::error::Error for InputTooLarge {}

pub fn check_size(size: u64, limit: usize) -> Result<()> {
    if size > limit as u64 {
        return Err(InputTooLarge { limit }.into());
    }
    Ok(())
}

/// Read at most `limit + 1` bytes, including for pipes or files growing while read.
/// Check the extra byte before extending the buffer, avoiding a large final growth.
pub fn read_limited(mut reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        let count = chunk.len().min((limit - bytes.len()).saturating_add(1));
        let read = match reader.read(&mut chunk[..count]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            return Ok(bytes);
        }
        check_size((bytes.len() + read) as u64, limit)?;
        bytes
            .try_reserve(read)
            .context("Cannot allocate image input buffer")?;
        bytes.extend_from_slice(&chunk[..read]);
    }
}

pub fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = File::open(path).with_context(|| format!("Cannot read {}", path.display()))?;
    check_size(file.metadata()?.len(), limit)?;
    read_limited(file, limit).with_context(|| format!("Cannot read {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    #[test]
    fn reads_boundaries_and_stops_infinite_input_after_the_extra_byte() {
        assert_eq!(read_limited(&b"abcd"[..], 4).unwrap(), b"abcd");
        assert!(read_limited(&b""[..], 0).unwrap().is_empty());
        let mut input = Cursor::new(b"abcde rest should stay unread");
        assert!(
            read_limited(&mut input, 4)
                .unwrap_err()
                .is::<InputTooLarge>()
        );
        assert_eq!(input.position(), 5);
        assert!(
            read_limited(io::repeat(0), 128)
                .unwrap_err()
                .is::<InputTooLarge>()
        );
    }

    #[test]
    fn handles_interrupted_and_failed_readers_and_rejects_large_files() {
        struct Interrupted(bool);
        impl Read for Interrupted {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    Err(io::ErrorKind::Interrupted.into())
                } else {
                    Err(io::Error::other("broken input"))
                }
            }
        }
        assert!(
            read_limited(Interrupted(false), 4)
                .unwrap_err()
                .to_string()
                .contains("broken input")
        );
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().set_len(MAX_INPUT_BYTES as u64 + 1).unwrap();
        assert!(
            read_file(file.path(), MAX_INPUT_BYTES)
                .unwrap_err()
                .is::<InputTooLarge>()
        );
    }
}
