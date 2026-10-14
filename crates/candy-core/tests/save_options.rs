use candy_core::{SaveOptions, WebpMode, save_atomic, save_atomic_with_options};
use image::{Rgba, RgbaImage};

fn texture() -> RgbaImage {
    RgbaImage::from_fn(128, 96, |x, y| {
        Rgba([
            ((x * 13 + y * 7) % 256) as u8,
            ((x * 3 + y * 17) % 256) as u8,
            ((x * 11 + y * 5) % 256) as u8,
            ((x * 7 + y * 3) % 256) as u8,
        ])
    })
}

fn webp_chunks(bytes: &[u8]) -> Vec<&[u8]> {
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    let mut chunks = Vec::new();
    let mut offset = 12;
    while offset < bytes.len() {
        chunks.push(&bytes[offset..offset + 4]);
        let length = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        offset += 8 + length + length % 2;
    }
    assert_eq!(offset, bytes.len());
    chunks
}

#[test]
fn webp_modes_and_quality_reach_the_encoder_and_preserve_alpha() {
    let dir = tempfile::tempdir().unwrap();
    let image = texture();
    let path = dir.path().join("image.webp");
    save_atomic(&image, &path, false).unwrap();
    let default = std::fs::read(&path).unwrap();
    assert!(webp_chunks(&default).contains(&b"VP8L".as_slice()));
    assert_eq!(image::open(&path).unwrap().into_rgba8(), image);
    save_atomic_with_options(
        &image,
        &path,
        true,
        SaveOptions {
            quality: None,
            webp_mode: Some(WebpMode::Lossless),
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), default);

    let mut sizes = Vec::new();
    for quality in [1, 80, 100] {
        save_atomic_with_options(
            &image,
            &path,
            true,
            SaveOptions {
                quality: Some(quality),
                webp_mode: Some(WebpMode::Lossy),
            },
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(webp_chunks(&bytes).contains(&b"VP8 ".as_slice()));
        sizes.push(bytes.len());
        let decoded = image::open(&path).unwrap().into_rgba8();
        assert_eq!(decoded.dimensions(), image.dimensions());
        assert!(
            decoded
                .pixels()
                .zip(image.pixels())
                .all(|(a, b)| a[3] == b[3])
        );
    }
    assert!(sizes[0] < sizes[1] && sizes[1] < sizes[2]);
}

#[test]
fn jpeg_quality_changes_output_and_defaults_stay_at_92() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.JPEG");
    let image = texture();
    save_atomic(&image, &path, false).unwrap();
    let default = std::fs::read(&path).unwrap();
    let mut sizes = Vec::new();
    for quality in [1, 92, 100] {
        save_atomic_with_options(
            &image,
            &path,
            true,
            SaveOptions {
                quality: Some(quality),
                webp_mode: None,
            },
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        if quality == 92 {
            assert_eq!(bytes, default);
        }
        sizes.push(bytes.len());
        assert_eq!(
            image::open(&path).unwrap().into_rgba8().dimensions(),
            image.dimensions()
        );
    }
    assert!(sizes[0] < sizes[1] && sizes[1] < sizes[2]);
    let transparent = RgbaImage::from_pixel(16, 16, Rgba([255, 0, 0, 0]));
    save_atomic_with_options(
        &transparent,
        &path,
        true,
        SaveOptions {
            quality: Some(100),
            webp_mode: None,
        },
    )
    .unwrap();
    assert!(
        image::open(&path)
            .unwrap()
            .to_rgb8()
            .pixels()
            .all(|p| p.0.iter().all(|c| *c >= 254))
    );
}

#[test]
fn invalid_options_and_encoder_failures_preserve_existing_output() {
    let dir = tempfile::tempdir().unwrap();
    for (ext, options) in [
        (
            "png",
            SaveOptions {
                quality: Some(80),
                webp_mode: None,
            },
        ),
        (
            "jpg",
            SaveOptions {
                quality: Some(0),
                webp_mode: None,
            },
        ),
        (
            "jpg",
            SaveOptions {
                quality: Some(101),
                webp_mode: None,
            },
        ),
        (
            "jpg",
            SaveOptions {
                quality: None,
                webp_mode: Some(WebpMode::Lossy),
            },
        ),
        (
            "webp",
            SaveOptions {
                quality: Some(80),
                webp_mode: None,
            },
        ),
        (
            "webp",
            SaveOptions {
                quality: Some(80),
                webp_mode: Some(WebpMode::Lossless),
            },
        ),
    ] {
        let path = dir.path().join(format!("image.{ext}"));
        std::fs::write(&path, b"existing output").unwrap();
        assert!(save_atomic_with_options(&texture(), &path, true, options).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing output");
    }
    let path = dir.path().join("large.webp");
    std::fs::write(&path, b"existing output").unwrap();
    let image = RgbaImage::new(16_384, 1);
    let count = std::fs::read_dir(dir.path()).unwrap().count();
    assert!(
        save_atomic_with_options(
            &image,
            &path,
            true,
            SaveOptions {
                quality: None,
                webp_mode: Some(WebpMode::Lossy),
            }
        )
        .unwrap_err()
        .to_string()
        .contains("16383")
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"existing output");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), count);
}
