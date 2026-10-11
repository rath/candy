//! Run with CANDY_OCR_TIMING=1 cargo run --release -p candy-macos --example ocr-timing.
use std::{path::Path, time::Instant};

fn main() -> anyhow::Result<()> {
    let source = candy_macos::load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/sample.png"
    )))?;
    let blank = image::RgbaImage::from_pixel(4000, 2640, image::Rgba([255; 4]));
    let shapes = image::RgbaImage::from_fn(4000, 2640, |x, y| {
        if (x as i64 - 2000).pow(2) + (y as i64 - 1320).pow(2) < 800_i64.pow(2) {
            image::Rgba([215, 90, 55, 255])
        } else {
            image::Rgba([220, 230, 240, 255])
        }
    });
    let large = image::imageops::resize(&source, 4000, 2640, image::imageops::FilterType::Nearest);
    for (name, image) in [
        ("blank", blank),
        ("shapes", shapes),
        ("text", source),
        ("large text", large),
    ] {
        for run in 1..=3 {
            let start = Instant::now();
            let regions = candy_macos::detect_text(&image)?;
            println!(
                "{name} {}x{} run {run}: {:?}, {} regions",
                image.width(),
                image.height(),
                start.elapsed(),
                regions.len()
            );
        }
    }
    Ok(())
}
