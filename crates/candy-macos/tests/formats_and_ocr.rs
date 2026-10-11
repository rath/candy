use candy_core::{Operation, PixelRect};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn all_five_formats_decode_and_webp_roundtrips() {
    for ext in ["png", "jpg", "heic", "avif"] {
        let image = candy_macos::load(&fixture(&format!("sample.{ext}"))).unwrap();
        assert_eq!(image.dimensions(), (1000, 660), "{ext}");
    }
    let original = candy_macos::load(&fixture("sample.png")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.webp");
    candy_core::save_atomic(&original, &path, false).unwrap();
    assert_eq!(candy_macos::load(&path).unwrap(), original);
}

#[test]
fn exif_orientation_is_applied_before_edit_coordinates() {
    let original = candy_macos::load(&fixture("sample.png")).unwrap();
    let rotated = candy_macos::load(&fixture("orientation6.jpg")).unwrap();
    assert_eq!(rotated.dimensions(), (660, 1000));
    let expected = image::imageops::rotate90(&original);
    for (x, y) in [(20, 20), (500, 900), (360, 780)] {
        for channel in 0..3 {
            assert!(
                rotated.get_pixel(x, y)[channel].abs_diff(expected.get_pixel(x, y)[channel]) < 15
            );
        }
    }
}

#[test]
fn detects_english_and_korean_and_mosaics_reported_regions() {
    let image = candy_macos::load(&fixture("sample.png")).unwrap();
    let regions = candy_macos::detect_text(&image).unwrap();
    assert!(regions.len() >= 6, "{regions:?}");
    assert!(
        regions
            .iter()
            .any(|r| r.text.contains("sample@example.com")),
        "{regions:?}"
    );
    assert!(
        regions
            .iter()
            .any(|r| r.text.chars().any(|c| ('가'..='힣').contains(&c))),
        "{regions:?}"
    );
    let boxes: Vec<PixelRect> = regions.iter().map(|r| r.rect).collect();
    for rect in &boxes {
        rect.validate(image.width(), image.height()).unwrap();
    }
    let result = candy_core::apply(
        &image,
        &Operation::Mosaic {
            regions: boxes.clone(),
            block: 12,
        },
    )
    .unwrap();
    let mut changed = 0;
    for (x, y, pixel) in image.enumerate_pixels() {
        if result.get_pixel(x, y) != pixel {
            changed += 1;
            assert!(
                boxes
                    .iter()
                    .any(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
            );
        }
    }
    assert!(changed > 1000);
}
