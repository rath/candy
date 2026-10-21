//! SVG rasterization with resvg. A vector document has no pixel size of its
//! own, so Candy renders it at its intrinsic CSS-pixel size, enlarged so the
//! longer side is at least `MIN_SIDE` and reduced to fit `MAX_PIXELS`.
//!
//! resvg is built without its `raster-images` feature: bitmaps embedded in or
//! linked from an SVG are skipped, which keeps the decoders and their weight
//! out of the binaries. Nested SVG images still render.
use crate::{MAX_PIXELS, validate_dimensions};
use anyhow::{Context, Result, anyhow};
use image::RgbaImage;
use resvg::{tiny_skia, usvg};
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

/// Smallest longer side of a rasterized SVG. An icon drawn in a 24-pixel
/// viewBox renders crisp at this size instead of becoming a 24-pixel bitmap.
pub const MIN_SIDE: u32 = 1024;

/// Uniform type identifier Candy reports for SVG, matching Launch Services.
pub const UTI: &str = "public.svg-image";

/// Whether `bytes`, read from `path`, are an SVG document. The `svg` and
/// `svgz` extensions are trusted; otherwise the text must open with an XML tag
/// and name an `<svg` root within its first kibibyte, since Image I/O cannot
/// sniff SVG itself.
pub fn is_svg(path: &Path, bytes: &[u8]) -> bool {
    match crate::extension(path).as_str() {
        "svg" | "svgz" => true,
        _ => {
            let head = &bytes[..bytes.len().min(1024)];
            let head = String::from_utf8_lossy(head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head));
            let text = head.trim_start();
            text.starts_with('<') && text.contains("<svg")
        }
    }
}

/// Pixel size `render` produces for `bytes`.
pub fn dimensions(bytes: &[u8], resources_dir: Option<&Path>) -> Result<(u32, u32)> {
    raster_size(parse(bytes, resources_dir)?.size())
}

/// Rasterizes `bytes` as straight-alpha sRGB pixels. `resources_dir` resolves
/// relative `<image href>` references to other SVG files, normally the SVG
/// file's directory; raster images are skipped.
pub fn render(bytes: &[u8], resources_dir: Option<&Path>) -> Result<RgbaImage> {
    let tree = parse(bytes, resources_dir)?;
    let (width, height) = raster_size(tree.size())?;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).context("Cannot allocate the SVG raster")?;
    // Scale each axis onto the exact integer size so the drawing fills the
    // bitmap without a transparent fringe from rounding.
    let transform = tiny_skia::Transform::from_scale(
        width as f32 / tree.size().width(),
        height as f32 / tree.size().height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    RgbaImage::from_raw(width, height, pixmap.take_demultiplied())
        .context("Invalid SVG raster buffer")
}

fn parse(bytes: &[u8], resources_dir: Option<&Path>) -> Result<usvg::Tree> {
    let decompressed;
    let data = if bytes.starts_with(&[0x1f, 0x8b]) {
        decompressed =
            usvg::decompress_svgz(bytes).map_err(|e| anyhow!("Cannot decompress SVGZ: {e}"))?;
        decompressed.as_slice()
    } else {
        bytes
    };
    let text = std::str::from_utf8(data).context("SVG text must be UTF-8")?;
    let mut options = usvg::Options {
        resources_dir: resources_dir.map(Path::to_path_buf),
        ..usvg::Options::default()
    };
    // usvg converts text to paths while parsing, so fonts must be present
    // now. Scanning the system font directories costs real time; skip it for
    // the many documents that draw no text at all.
    if text.contains("<text") {
        options.fontdb = system_fonts();
    }
    usvg::Tree::from_str(text, &options).map_err(|e| anyhow!("Cannot parse this SVG: {e}"))
}

/// System fonts, scanned once per process and shared between renders.
fn system_fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut database = usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

/// Integer raster size for a document of `size` CSS pixels: enlarged so the
/// longer side reaches `MIN_SIDE`, then shrunk to stay within `MAX_PIXELS`.
fn raster_size(size: usvg::Size) -> Result<(u32, u32)> {
    let (w, h) = (f64::from(size.width()), f64::from(size.height()));
    let enlarge = (f64::from(MIN_SIDE) / w.max(h)).max(1.0);
    let fit = (MAX_PIXELS as f64 / (w * h)).sqrt();
    let scale = enlarge.min(fit);
    // Rounding up both sides could cross the limit when shrinking to fit.
    let side = |v: f64| {
        let scaled = v * scale;
        let scaled = if scale < 1.0 {
            scaled.floor()
        } else {
            scaled.round()
        };
        scaled.max(1.0) as u32
    };
    let (width, height) = (side(w), side(h));
    validate_dimensions(width, height)?;
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const HALF_RED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
<rect width="100" height="100" fill="#ff0000"/></svg>"##;

    #[test]
    fn detects_svg_by_extension_or_content() {
        let svg = Path::new("drawing.SVG");
        assert!(is_svg(svg, b""));
        assert!(is_svg(Path::new("drawing.svgz"), b"\x1f\x8b"));
        let unknown = Path::new("drawing");
        assert!(is_svg(unknown, HALF_RED.as_bytes()));
        assert!(is_svg(
            unknown,
            "\u{feff}\n<?xml version=\"1.0\"?>\n<svg/>".as_bytes()
        ));
        assert!(!is_svg(unknown, b"<html><body>svg</body></html>"));
        assert!(!is_svg(Path::new("photo.png"), b"\x89PNG\r\n\x1a\n"));
        assert!(!is_svg(unknown, b"\xff\xd8<svg"));
    }

    #[test]
    fn small_documents_render_enlarged_with_straight_alpha() {
        assert_eq!(dimensions(HALF_RED.as_bytes(), None).unwrap(), (1024, 512));
        let image = render(HALF_RED.as_bytes(), None).unwrap();
        assert_eq!(image.dimensions(), (1024, 512));
        assert_eq!(*image.get_pixel(256, 256), Rgba([255, 0, 0, 255]));
        assert_eq!(*image.get_pixel(768, 256), Rgba([0, 0, 0, 0]));
        assert_eq!(*image.get_pixel(1023, 511), Rgba([0, 0, 0, 0]));
        assert_eq!(*image.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn large_documents_keep_their_size_or_shrink_to_the_limit() {
        let size = |w, h| usvg::Size::from_wh(w, h).unwrap();
        assert_eq!(raster_size(size(2048.0, 10.5)).unwrap(), (2048, 11));
        assert_eq!(raster_size(size(1024.0, 1024.0)).unwrap(), (1024, 1024));
        let (w, h) = raster_size(size(20000.0, 30000.0)).unwrap();
        assert!(u64::from(w) * u64::from(h) <= MAX_PIXELS, "{w}x{h}");
        assert!(
            u64::from(w) * u64::from(h) > MAX_PIXELS * 99 / 100,
            "{w}x{h}"
        );
        assert!((f64::from(w) / f64::from(h) - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn nested_svg_images_render_and_raster_images_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("inner.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
<rect width="10" height="10" fill="#0000ff"/></svg>"##,
        )
        .unwrap();
        RgbaImage::from_pixel(2, 2, Rgba([0, 255, 0, 255]))
            .save(dir.path().join("green.png"))
            .unwrap();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100">
<rect width="100" height="100" fill="#ff0000"/>
<image x="100" width="100" height="100" href="inner.svg"/>
<image x="200" width="100" height="100" href="green.png"/></svg>"##;
        let image = render(svg.as_bytes(), Some(dir.path())).unwrap();
        assert_eq!(image.dimensions(), (1024, 341));
        assert_eq!(*image.get_pixel(170, 170), Rgba([255, 0, 0, 255]));
        assert_eq!(*image.get_pixel(512, 170), Rgba([0, 0, 255, 255]));
        assert_eq!(*image.get_pixel(853, 170), Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn svgz_and_text_render_and_bad_input_fails() {
        let text = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100">
<text x="10" y="70" font-family="sans-serif" font-size="60" fill="#000">Candy</text></svg>"##;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, text.as_bytes()).unwrap();
        let gz = encoder.finish().unwrap();
        let image = render(&gz, None).unwrap();
        assert_eq!(image.dimensions(), (1024, 341));
        // Glyphs are only present when a system font was found and shaped.
        assert!(image.pixels().any(|p| p[3] == 255 && p[0] == 0));
        assert!(render(b"<svg xmlns='http://www.w3.org/2000/svg'><rect/>", None).is_err());
        assert!(render(b"\x1f\x8bnot gzip", None).is_err());
        assert!(
            render(
                b"<svg xmlns='http://www.w3.org/2000/svg' width='0' height='5'/>",
                None
            )
            .is_err()
        );
        assert!(render(b"\xff\xfe<svg/>", None).is_err());
    }
}
