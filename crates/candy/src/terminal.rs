use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat, RgbaImage, imageops::FilterType};
use std::{
    fs::OpenOptions,
    io::{Cursor, Read, Write},
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

const QUERY_ID: u32 = 73921;

struct TerminalGuard {
    fd: libc::c_int,
    termios: libc::termios,
    signals: libc::sigset_t,
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Restore terminal settings before allowing a pending termination signal.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.termios);
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.signals, std::ptr::null_mut());
        }
    }
}

pub fn supported() -> bool {
    if std::env::var_os("TMUX").is_some()
        || std::env::var("TERM").is_ok_and(|t| t.starts_with("tmux") || t.starts_with("screen"))
    {
        return false;
    }
    probe().unwrap_or(false)
}

fn probe() -> Result<bool> {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let fd = tty.as_raw_fd();
    // SAFETY: All structures are initialized by libc before use, the file owns
    // fd throughout the guard, and every path restores both tty and signal state.
    let _guard = unsafe {
        let mut original: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut original) != 0 {
            return Ok(false);
        }
        let mut mask: libc::sigset_t = std::mem::zeroed();
        let mut previous: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        for signal in [
            libc::SIGINT,
            libc::SIGTERM,
            libc::SIGHUP,
            libc::SIGQUIT,
            libc::SIGTSTP,
        ] {
            libc::sigaddset(&mut mask, signal);
        }
        if libc::pthread_sigmask(libc::SIG_BLOCK, &mask, &mut previous) != 0 {
            return Ok(false);
        }
        let guard = TerminalGuard {
            fd,
            termios: original,
            signals: previous,
        };
        let mut raw = original;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(fd, libc::TCSANOW, &raw) != 0 {
            return Ok(false);
        }
        guard
    };
    write!(tty, "\x1b_Ga=q,i={QUERY_ID},s=1,v=1,t=d,f=24;AAAA\x1b\\")?;
    tty.flush()?;
    let deadline = Instant::now() + Duration::from_millis(180);
    let mut response = Vec::new();
    while Instant::now() < deadline && response.len() < 4096 {
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(180) as i32;
        if unsafe { libc::poll(&mut poll, 1, timeout) } <= 0 {
            break;
        }
        let mut bytes = [0_u8; 256];
        let count = tty.read(&mut bytes)?;
        response.extend_from_slice(&bytes[..count]);
        if response.ends_with(b"\x1b\\") {
            break;
        }
    }
    Ok(query_succeeded(&response))
}

fn query_succeeded(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    text.contains(&format!("\x1b_Gi={QUERY_ID};OK\x1b\\"))
}

pub fn display(image: &RgbaImage) -> Result<()> {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe {
        libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size);
    }
    let cols = u32::from(size.ws_col).max(20);
    let rows = u32::from(size.ws_row).max(10).saturating_sub(3);
    let pixel_width = if size.ws_xpixel > 0 {
        u32::from(size.ws_xpixel)
    } else {
        cols * 8
    };
    let pixel_height = if size.ws_ypixel > 0 {
        u32::from(size.ws_ypixel) * rows / u32::from(size.ws_row).max(1)
    } else {
        rows * 16
    };
    let preview = if image.width() > pixel_width || image.height() > pixel_height {
        DynamicImage::ImageRgba8(image.clone())
            .resize(pixel_width, pixel_height, FilterType::Triangle)
            .into_rgba8()
    } else {
        image.clone()
    };
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(preview).write_to(&mut png, ImageFormat::Png)?;
    let mut stdout = std::io::stdout().lock();
    write_graphics(&mut stdout, png.get_ref()).context("Cannot write image to terminal")?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}

fn write_graphics(mut output: impl Write, png: &[u8]) -> std::io::Result<()> {
    let data = STANDARD.encode(png);
    let chunks = data.as_bytes().chunks(4096);
    let total = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        let more = u8::from(index + 1 < total);
        if index == 0 {
            write!(output, "\x1b_Ga=T,t=d,f=100,q=2,m={more};")?;
        } else {
            write!(output, "\x1b_Gm={more};")?;
        }
        output.write_all(chunk)?;
        output.write_all(b"\x1b\\")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_matching_positive_query_is_accepted() {
        assert!(query_succeeded(b"\x1b_Gi=73921;OK\x1b\\"));
        assert!(!query_succeeded(b"\x1b_Gi=73921;ENOTSUP\x1b\\"));
        assert!(!query_succeeded(b"\x1b_Gi=42;OK\x1b\\"));
    }
    #[test]
    fn graphics_chunks_roundtrip_payload() {
        let data: Vec<u8> = (0..12000).map(|i| (i % 251) as u8).collect();
        let mut encoded = Vec::new();
        write_graphics(&mut encoded, &data).unwrap();
        let text = String::from_utf8(encoded).unwrap();
        let chunks: Vec<_> = text.split("\x1b\\").filter(|s| !s.is_empty()).collect();
        assert!(chunks.len() > 1);
        assert!(chunks.last().unwrap().starts_with("\x1b_Gm=0;"));
        let payload = chunks
            .iter()
            .map(|s| s.split_once(';').unwrap().1)
            .collect::<String>();
        assert_eq!(STANDARD.decode(payload).unwrap(), data);
    }
}
