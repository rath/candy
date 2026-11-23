use anyhow::{Context, Result, ensure};
use ratatui::text::Line;
use std::{
    cmp::Ordering,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(super) struct Entry {
    pub path: PathBuf,
    pub name: String,
}

pub(super) struct Listing {
    pub directory: PathBuf,
    pub files: Vec<Entry>,
    pub longest_name: u16,
}

impl Listing {
    pub fn open(path: &Path) -> Result<(Self, usize)> {
        let context = || format!("Cannot open {}", path.display());
        let metadata = fs::metadata(path).with_context(context)?;
        let directory = if metadata.is_dir() {
            path
        } else {
            ensure!(metadata.is_file(), "Choose an image file or directory");
            fs::File::open(path).with_context(context)?;
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
        };
        let directory = directory.canonicalize().with_context(context)?;
        let listing = Self::scan(&directory)?;
        ensure!(
            !listing.files.is_empty(),
            "No images in {}",
            directory.display()
        );
        let selected = if metadata.is_dir() {
            0
        } else {
            let name = path.file_name().context("Missing filename")?;
            ensure!(
                visible_image(path),
                "Image is not in the browser listing: {}",
                path.display()
            );
            let index = listing
                .files
                .iter()
                .position(|entry| entry.path.file_name() == Some(name));
            // Case-insensitive and normalization-insensitive filesystems can
            // return a different spelling. Compare links themselves so opening
            // an alias still selects that directory entry.
            index
                .or_else(|| {
                    let wanted = fs::symlink_metadata(path).ok()?;
                    listing.files.iter().position(|entry| {
                        fs::symlink_metadata(&entry.path).is_ok_and(|actual| {
                            (actual.dev(), actual.ino()) == (wanted.dev(), wanted.ino())
                        })
                    })
                })
                .with_context(|| {
                    format!("Image is not in the browser listing: {}", path.display())
                })?
        };
        Ok((listing, selected))
    }

    pub fn scan(directory: &Path) -> Result<Self> {
        let context = || format!("Cannot open {}", directory.display());
        let mut files = Vec::new();
        let mut longest_name = 0;
        for entry in fs::read_dir(directory).with_context(context)? {
            let entry = entry.with_context(context)?;
            let path = entry.path();
            if !visible_image(&path) || !path.is_file() {
                continue;
            }
            let name = display_text(&entry.file_name().to_string_lossy());
            longest_name =
                longest_name.max(u16::try_from(Line::raw(&name).width()).unwrap_or(u16::MAX));
            files.push((name.to_lowercase(), Entry { path, name }));
        }
        files.sort_unstable_by(|(a, left), (b, right)| {
            natural_cmp(a.as_bytes(), b.as_bytes()).then_with(|| left.path.cmp(&right.path))
        });
        Ok(Self {
            directory: directory.to_owned(),
            files: files.into_iter().map(|(_, entry)| entry).collect(),
            longest_name,
        })
    }
}

fn visible_image(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| !name.as_encoded_bytes().starts_with(b"."))
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(candy_macos::is_image_extension)
}

// Filenames and diagnostics are terminal text. Preserve the original path for I/O.
pub(super) fn display_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}

fn digit_run(bytes: &[u8]) -> (&[u8], &[u8]) {
    let end = bytes
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(bytes.len());
    let (digits, rest) = bytes.split_at(end);
    let start = digits
        .iter()
        .position(|&c| c != b'0')
        .unwrap_or(digits.len());
    (&digits[start..], rest)
}

fn natural_cmp(mut left: &[u8], mut right: &[u8]) -> Ordering {
    while let (Some(&a), Some(&b)) = (left.first(), right.first()) {
        let order = if a.is_ascii_digit() && b.is_ascii_digit() {
            let (ad, rest_a) = digit_run(left);
            let (bd, rest_b) = digit_run(right);
            left = rest_a;
            right = rest_b;
            ad.len().cmp(&bd.len()).then_with(|| ad.cmp(bd))
        } else {
            left = &left[1..];
            right = &right[1..];
            a.cmp(&b)
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn scans_visible_image_files_and_selects_the_named_alias() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        for name in [
            "img10.png",
            "img2.JPG",
            "IMG02.jpg",
            "extra.txt",
            ".secret.heic",
            "東京.svg",
            "한글.png",
        ] {
            fs::write(dir.join(name), []).unwrap();
        }
        fs::create_dir(dir.join("nested.png")).unwrap();
        symlink(dir.join("img2.JPG"), dir.join("alias.jpg")).unwrap();
        fs::hard_link(dir.join("img2.JPG"), dir.join(".alias.jpg")).unwrap();
        assert!(Listing::open(&dir.join(".alias.jpg")).is_err());
        symlink(dir.join("missing"), dir.join("dangling.jpg")).unwrap();
        let (listing, selected) = Listing::open(&dir.join("alias.jpg")).unwrap();
        let names: Vec<_> = listing
            .files
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "alias.jpg",
                "IMG02.jpg",
                "img2.JPG",
                "img10.png",
                "한글.png",
                "東京.svg"
            ]
        );
        assert_eq!(
            listing.files[selected].path.file_name().unwrap(),
            "alias.jpg"
        );
        assert_eq!(Listing::open(&dir.join("img10.png")).unwrap().1, 3);
    }

    #[test]
    fn decomposed_hangul_names_keep_their_width_and_open_from_either_form() {
        // Files copied from other systems often keep NFD names; APFS opens
        // them by the NFC spelling a shell or terminal usually produces.
        let decomposed = "\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}.png";
        let composed = "\u{d55c}\u{ae00}.png";
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(decomposed), []).unwrap();
        fs::write(temp.path().join("abc.png"), []).unwrap();
        let (listing, selected) = Listing::open(&temp.path().join(composed)).unwrap();
        let entry = &listing.files[selected];
        assert_eq!(entry.name, decomposed);
        assert_eq!(entry.path.file_name().unwrap(), decomposed);
        assert_eq!(
            Line::raw(decomposed).width(),
            Line::raw(composed).width(),
            "jamo must not widen the list column"
        );
        // Two wide syllables and ".png", not one column per jamo.
        assert_eq!(listing.longest_name, 8);
    }

    #[test]
    fn natural_numbers_have_no_integer_limit() {
        assert!(natural_cmp(b"x99999999999999999999", b"x100000000000000000000").is_lt());
        assert_eq!(natural_cmp(b"a00b002", b"a0b2"), Ordering::Equal);
        assert!(natural_cmp(b"a02z", b"a2a").is_gt());
    }
}
