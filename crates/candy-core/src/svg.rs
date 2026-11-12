//! SVG rasterization with resvg. A vector document has no pixel size of its
//! own, so Candy renders it at its intrinsic CSS-pixel size, enlarged so the
//! longer side is at least `MIN_SIDE` and reduced to fit `MAX_PIXELS`.
//!
//! resvg is built without its `raster-images` feature: bitmaps embedded in or
//! linked from an SVG are skipped, which keeps the decoders and their weight
//! out of the binaries. Nested SVG images still render.
use crate::{
    MAX_PIXELS,
    input::{self, InputTooLarge},
    validate_dimensions,
};
use anyhow::{Context, Result, anyhow};
use image::RgbaImage;
use resvg::{tiny_skia, usvg};
use std::{
    borrow::Cow,
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

/// Smallest longer side of a rasterized SVG. An icon drawn in a 24-pixel
/// viewBox renders crisp at this size instead of becoming a 24-pixel bitmap.
pub const MIN_SIDE: u32 = 1024;

/// Total decoded XML bytes, including referenced and embedded SVG documents.
pub const MAX_SVG_BYTES: usize = 32 * 1024 * 1024;

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
    parse_with_limit(bytes, resources_dir, MAX_SVG_BYTES)
}

fn svg_data(bytes: &[u8], limit: usize) -> Result<Cow<'_, [u8]>> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        input::check_size(bytes.len() as u64, MAX_SVG_BYTES)?;
        input::read_limited(flate2::read::GzDecoder::new(bytes), limit)
            .map(Cow::Owned)
            .context("Cannot decompress SVGZ within the decoded-data limit")
    } else {
        input::check_size(bytes.len() as u64, limit)?;
        Ok(Cow::Borrowed(bytes))
    }
}

struct SvgBudget {
    remaining: AtomicUsize,
    exceeded: AtomicBool,
}

impl SvgBudget {
    fn checked<T>(&self, result: Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                if error.is::<InputTooLarge>() {
                    self.exceeded.store(true, Ordering::Relaxed);
                }
                None
            }
        }
    }

    fn image(&self, bytes: &[u8], options: &usvg::Options<'_>) -> Option<usvg::ImageKind> {
        let data = self.checked(svg_data(bytes, self.remaining.load(Ordering::Relaxed)))?;
        let mut remaining = self.remaining.load(Ordering::Relaxed);
        loop {
            let Some(next) = remaining.checked_sub(data.len()) else {
                self.exceeded.store(true, Ordering::Relaxed);
                return None;
            };
            match self.remaining.compare_exchange_weak(
                remaining,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => remaining = current,
            }
        }
        // Preserve SVG's nested-image rules (external references are disabled),
        // while decompressing here so usvg cannot bypass the shared byte budget.
        usvg::Tree::from_data_nested(&data, options)
            .ok()
            .map(usvg::ImageKind::SVG)
    }
}

fn parse_with_limit(
    bytes: &[u8],
    resources_dir: Option<&Path>,
    limit: usize,
) -> Result<usvg::Tree> {
    let data = svg_data(bytes, limit)?;
    let text = std::str::from_utf8(&data).context("SVG text must be UTF-8")?;
    let budget = Arc::new(SvgBudget {
        remaining: AtomicUsize::new(limit - data.len()),
        exceeded: AtomicBool::new(false),
    });
    let data_budget = budget.clone();
    let file_budget = budget.clone();
    let select_font = usvg::FontResolver::default_font_selector();
    let options = usvg::Options {
        resources_dir: resources_dir.map(Path::to_path_buf),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(move |mime, bytes, options| {
                if matches!(mime, "image/svg+xml" | "text/plain") {
                    data_budget.image(&bytes, options)
                } else {
                    None
                } // Raster images are intentionally skipped.
            }),
            resolve_string: Box::new(move |href, options| {
                let path = options.get_abs_path(Path::new(href));
                if !matches!(crate::extension(&path).as_str(), "svg" | "svgz") {
                    return None;
                }
                let bytes = file_budget.checked(input::read_file(&path, MAX_SVG_BYTES))?;
                file_budget.image(&bytes, options)
            }),
        },
        font_resolver: usvg::FontResolver {
            // Load on the first real text node, including namespaced text and
            // text inside nested SVGs. Vector-only documents never scan fonts.
            select_font: Box::new(move |font, database| {
                if database.is_empty() {
                    *database = system_fonts();
                }
                select_font(font, database)
            }),
            ..usvg::FontResolver::default()
        },
        ..usvg::Options::default()
    };
    let tree =
        usvg::Tree::from_str(text, &options).map_err(|e| anyhow!("Cannot parse this SVG: {e}"))?;
    if budget.exceeded.load(Ordering::Relaxed) {
        return Err(InputTooLarge { limit }.into());
    }
    Ok(tree)
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
    fn namespaced_and_nested_text_render_like_direct_text() {
        let direct = r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100"><text x="10" y="70" font-family="sans-serif" font-size="60">Candy</text></svg>"#;
        let expected = render(direct.as_bytes(), None).unwrap();
        assert!(expected.pixels().any(|p| p[3] == 255));
        let prefixed = direct
            .replace("<svg ", "<s:svg ")
            .replace("xmlns=", "xmlns:s=")
            .replace("</svg>", "</s:svg>")
            .replace("<text ", "<s:text ")
            .replace("</text>", "</s:text>");
        assert_eq!(render(prefixed.as_bytes(), None).unwrap(), expected);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("inner.svg"), direct).unwrap();
        let nested = r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100"><image width="300" height="100" href="inner.svg"/></svg>"#;
        assert_eq!(
            render(nested.as_bytes(), Some(dir.path())).unwrap(),
            expected
        );
        // Comments mentioning text must not populate a vector-only tree's database.
        let vector = HALF_RED.replace("<rect", "<!-- <text --> <rect");
        assert!(parse(vector.as_bytes(), None).unwrap().fontdb().is_empty());
    }

    #[test]
    fn svg_limits_cover_plain_gzip_referenced_and_embedded_documents() {
        fn gzip(bytes: &[u8]) -> Vec<u8> {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut encoder, bytes).unwrap();
            encoder.finish().unwrap()
        }
        let child = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><!--{}--><rect width="10" height="10"/></svg>"#,
            "x".repeat(4096)
        );
        let compressed = gzip(child.as_bytes());
        assert!(compressed.len() < 512);
        for bytes in [child.as_bytes(), compressed.as_slice()] {
            assert!(parse_with_limit(bytes, None, child.len()).is_ok());
            assert!(
                parse_with_limit(bytes, None, child.len() - 1)
                    .unwrap_err()
                    .is::<InputTooLarge>()
            );
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("inner.svgz"), &compressed).unwrap();
        let wrapper = |href: &str| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><image width="10" height="10" href="{href}"/></svg>"#
            )
        };
        let encoded: String = compressed.iter().map(|b| format!("%{b:02X}")).collect();
        for outer in [
            wrapper("inner.svgz"),
            wrapper(&format!("data:image/svg+xml,{encoded}")),
        ] {
            assert!(
                parse_with_limit(
                    outer.as_bytes(),
                    Some(dir.path()),
                    outer.len() + child.len()
                )
                .is_ok()
            );
            assert!(
                parse_with_limit(outer.as_bytes(), Some(dir.path()), outer.len() + 512)
                    .unwrap_err()
                    .is::<InputTooLarge>()
            );
        }
        std::fs::write(dir.path().join("another.svgz"), &compressed).unwrap();
        let repeated = wrapper("inner.svgz").replace(
            "</svg>",
            r#"<image width="10" height="10" href="another.svgz"/></svg>"#,
        );
        assert!(
            parse_with_limit(
                repeated.as_bytes(),
                Some(dir.path()),
                repeated.len() + child.len() * 2
            )
            .is_ok()
        );
        assert!(
            parse_with_limit(
                repeated.as_bytes(),
                Some(dir.path()),
                repeated.len() + child.len() * 2 - 1
            )
            .unwrap_err()
            .is::<InputTooLarge>()
        );
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
