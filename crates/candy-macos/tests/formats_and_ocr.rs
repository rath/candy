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
    assert_text_coverage(&image, &regions);
    let boxes = regions;
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

// The fixture has eight known text lines, including Korean on the seventh line.
// Check actual dark glyph pixels, not recognition strings or exact box counts:
// Vision may split a line, but mosaic candidates must still cover its letters.
fn assert_text_coverage(image: &image::RgbaImage, boxes: &[PixelRect]) {
    let scale = image.width() / 1000;
    for (top, bottom) in [
        (60, 90),
        (110, 165),
        (170, 205),
        (260, 305),
        (315, 360),
        (370, 410),
        (430, 480),
        (555, 600),
    ] {
        let mut ink = 0;
        let mut covered = 0;
        for y in top * scale..bottom * scale {
            for x in 45 * scale..610 * scale {
                let pixel = image.get_pixel(x, y);
                if pixel[0] < 150 && pixel[1] < 150 && pixel[2] < 150 {
                    ink += 1;
                    if boxes
                        .iter()
                        .any(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
                    {
                        covered += 1;
                    }
                }
            }
        }
        assert!(ink > 100, "Missing fixture text at y={top}");
        assert!(
            covered as f64 / ink as f64 > 0.99,
            "Uncovered text at y={top}: {covered}/{ink}; {boxes:?}"
        );
    }
}

#[test]
fn large_text_regions_cover_original_image_pixels() {
    let source = candy_macos::load(&fixture("sample.png")).unwrap();
    let image = image::imageops::resize(&source, 4000, 2640, image::imageops::FilterType::Nearest);
    let boxes = candy_macos::detect_text(&image).unwrap();
    assert_text_coverage(&image, &boxes);
    for rect in boxes {
        rect.validate(image.width(), image.height()).unwrap();
    }
}

#[test]
fn large_blank_image_has_no_text_regions() {
    let image = image::RgbaImage::from_pixel(4000, 2640, image::Rgba([255; 4]));
    assert!(candy_macos::detect_text(&image).unwrap().is_empty());
}
