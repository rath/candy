//! Shared, deterministic image operations. Coordinates are always image pixels.
use anyhow::{Context, Result, bail, ensure};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage, imageops};
use std::{fs, path::Path, sync::Arc};

pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;

pub fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    ensure!(
        width > 0 && height > 0,
        "Image dimensions must be greater than zero"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "Image exceeds the 64-megapixel limit"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl PixelRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    pub fn validate(self, width: u32, height: u32) -> Result<Self> {
        ensure!(
            self.width > 0 && self.height > 0,
            "Select a non-empty rectangle"
        );
        ensure!(
            u64::from(self.x) + u64::from(self.width) <= u64::from(width)
                && u64::from(self.y) + u64::from(self.height) <= u64::from(height),
            "Rectangle extends beyond the image"
        );
        Ok(self)
    }
    pub fn from_points(a: [f32; 2], b: [f32; 2], width: u32, height: u32) -> Option<Self> {
        if a.iter().chain(b.iter()).any(|v| !v.is_finite()) {
            return None;
        }
        let x = a[0].min(b[0]).floor().clamp(0.0, width as f32) as u32;
        let y = a[1].min(b[1]).floor().clamp(0.0, height as f32) as u32;
        let right = a[0].max(b[0]).ceil().clamp(0.0, width as f32) as u32;
        let bottom = a[1].max(b[1]).ceil().clamp(0.0, height as f32) as u32;
        (right > x && bottom > y).then(|| Self::new(x, y, right - x, bottom - y))
    }
}
impl std::str::FromStr for PixelRect {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let n: Vec<u32> = s
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()
            .context("Use x,y,width,height with non-negative integer pixels")?;
        ensure!(n.len() == 4, "Use x,y,width,height");
        ensure!(
            n[2] > 0 && n[3] > 0,
            "Rectangle dimensions must be positive"
        );
        Ok(Self::new(n[0], n[1], n[2], n[3]))
    }
}

#[derive(Clone, Debug)]
pub enum Operation {
    Crop(PixelRect),
    Mosaic {
        regions: Vec<PixelRect>,
        block: u32,
    },
    Highlight {
        points: Vec<[f32; 2]>,
        width: f32,
        color: [u8; 4],
    },
    Resize {
        width: u32,
        height: u32,
    },
    Flatten([u8; 3]),
}

pub fn apply(input: &RgbaImage, op: &Operation) -> Result<RgbaImage> {
    match op {
        Operation::Crop(rect) => {
            rect.validate(input.width(), input.height())?;
            Ok(imageops::crop_imm(input, rect.x, rect.y, rect.width, rect.height).to_image())
        }
        Operation::Resize { width, height } => {
            validate_dimensions(*width, *height)?;
            // Filter premultiplied values to avoid dark fringes around transparent pixels.
            let mut premultiplied = input.clone();
            for p in premultiplied.pixels_mut() {
                for c in 0..3 {
                    p[c] = ((u16::from(p[c]) * u16::from(p[3]) + 127) / 255) as u8;
                }
            }
            let mut out = imageops::resize(
                &premultiplied,
                *width,
                *height,
                imageops::FilterType::Lanczos3,
            );
            for p in out.pixels_mut() {
                for c in 0..3 {
                    p[c] = if p[3] == 0 {
                        0
                    } else {
                        (u32::from(p[c]) * 255 / u32::from(p[3])).min(255) as u8
                    };
                }
            }
            Ok(out)
        }
        Operation::Flatten(background) => {
            let mut out = input.clone();
            for p in out.pixels_mut() {
                let alpha = u32::from(p[3]);
                for c in 0..3 {
                    p[c] =
                        ((u32::from(p[c]) * alpha + u32::from(background[c]) * (255 - alpha) + 127)
                            / 255) as u8;
                }
                p[3] = 255;
            }
            Ok(out)
        }
        Operation::Mosaic { regions, block } => {
            ensure!(*block >= 2, "Mosaic blocks must be at least 2 pixels");
            let mut out = input.clone();
            for rect in regions {
                rect.validate(input.width(), input.height())?;
                for y in (rect.y..rect.y + rect.height).step_by(*block as usize) {
                    for x in (rect.x..rect.x + rect.width).step_by(*block as usize) {
                        let right = x.saturating_add(*block).min(rect.x + rect.width);
                        let bottom = y.saturating_add(*block).min(rect.y + rect.height);
                        let mut sum = [0_u64; 4];
                        for py in y..bottom {
                            for px in x..right {
                                let p = input.get_pixel(px, py);
                                for c in 0..3 {
                                    sum[c] += u64::from(p[c]) * u64::from(p[3]);
                                }
                                sum[3] += u64::from(p[3]);
                            }
                        }
                        let count = u64::from(right - x) * u64::from(bottom - y);
                        let mut color = [0; 4];
                        for c in 0..3 {
                            color[c] = sum[c].checked_div(sum[3]).unwrap_or(0) as u8;
                        }
                        color[3] = (sum[3] / count) as u8;
                        for py in y..bottom {
                            for px in x..right {
                                out.put_pixel(px, py, Rgba(color));
                            }
                        }
                    }
                }
            }
            Ok(out)
        }
        Operation::Highlight {
            points,
            width,
            color,
        } => {
            ensure!(
                width.is_finite() && *width > 0.0,
                "Brush width must be positive"
            );
            ensure!(
                points.iter().flatten().all(|v| v.is_finite()),
                "Invalid brush coordinates"
            );
            let mut out = input.clone();
            if points.is_empty() {
                return Ok(out);
            }
            // Coverage is accumulated once per stroke, so joins never become darker.
            let radius = *width / 2.0;
            let min_x = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min) - radius - 1.0;
            let min_y = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min) - radius - 1.0;
            let max_x = points
                .iter()
                .map(|p| p[0])
                .fold(f32::NEG_INFINITY, f32::max)
                + radius
                + 1.0;
            let max_y = points
                .iter()
                .map(|p| p[1])
                .fold(f32::NEG_INFINITY, f32::max)
                + radius
                + 1.0;
            let x0 = min_x.clamp(0.0, input.width() as f32) as u32;
            let y0 = min_y.clamp(0.0, input.height() as f32) as u32;
            let x1 = max_x.ceil().clamp(0.0, input.width() as f32) as u32;
            let y1 = max_y.ceil().clamp(0.0, input.height() as f32) as u32;
            let mask_width = (x1 - x0) as usize;
            let mut coverage = vec![0.0_f32; mask_width * (y1 - y0) as usize];
            for i in 0..points.len() {
                let a = points[i.saturating_sub(1)];
                let b = points[i];
                let sx0 = (a[0].min(b[0]) - radius - 1.0).clamp(x0 as f32, x1 as f32) as u32;
                let sy0 = (a[1].min(b[1]) - radius - 1.0).clamp(y0 as f32, y1 as f32) as u32;
                let sx1 = (a[0].max(b[0]) + radius + 1.0)
                    .ceil()
                    .clamp(x0 as f32, x1 as f32) as u32;
                let sy1 = (a[1].max(b[1]) + radius + 1.0)
                    .ceil()
                    .clamp(y0 as f32, y1 as f32) as u32;
                for y in sy0..sy1 {
                    for x in sx0..sx1 {
                        let distance = segment_distance([x as f32 + 0.5, y as f32 + 0.5], a, b);
                        let index = (y - y0) as usize * mask_width + (x - x0) as usize;
                        coverage[index] =
                            coverage[index].max((radius + 0.5 - distance).clamp(0.0, 1.0));
                    }
                }
            }
            for y in y0..y1 {
                for x in x0..x1 {
                    let alpha = coverage[(y - y0) as usize * mask_width + (x - x0) as usize]
                        * f32::from(color[3])
                        / 255.0;
                    if alpha <= 0.0 {
                        continue;
                    }
                    let p = out.get_pixel_mut(x, y);
                    let old_alpha = f32::from(p[3]) / 255.0;
                    let new_alpha = alpha + old_alpha * (1.0 - alpha);
                    for c in 0..3 {
                        p[c] = ((f32::from(color[c]) * alpha
                            + f32::from(p[c]) * old_alpha * (1.0 - alpha))
                            / new_alpha)
                            .round() as u8;
                    }
                    p[3] = (new_alpha * 255.0).round() as u8;
                }
            }
            Ok(out)
        }
    }
}

fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0] * d[0] + d[1] * d[1];
    let t = if len == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len).clamp(0.0, 1.0)
    };
    ((p[0] - a[0] - t * d[0]).powi(2) + (p[1] - a[1] - t * d[1]).powi(2)).sqrt()
}

pub fn resize_dimensions(
    width: u32,
    height: u32,
    new_width: Option<u32>,
    new_height: Option<u32>,
) -> Result<(u32, u32)> {
    validate_dimensions(width, height)?;
    let (w, h) = match (new_width, new_height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (
            w,
            ((u64::from(height) * u64::from(w) + u64::from(width) / 2) / u64::from(width))
                .max(1)
                .try_into()?,
        ),
        (None, Some(h)) => (
            ((u64::from(width) * u64::from(h) + u64::from(height) / 2) / u64::from(height))
                .max(1)
                .try_into()?,
            h,
        ),
        _ => bail!("Specify --width or --height"),
    };
    validate_dimensions(w, h)?;
    Ok((w, h))
}

/// Statistics of Candy's working pixels: 8-bit sRGB after EXIF orientation.
#[derive(Clone, Debug, PartialEq)]
pub struct PixelStats {
    pub opaque: u64,
    pub translucent: u64,
    pub transparent: u64,
    /// Alpha-weighted mean of the non-transparent pixels; `None` when every
    /// pixel is transparent.
    pub average: Option<[u8; 3]>,
    pub mean_alpha: f64,
    /// Distinct RGB values among the non-transparent pixels.
    pub colors: u64,
}

impl PixelStats {
    pub fn total(&self) -> u64 {
        self.opaque + self.translucent + self.transparent
    }
}

pub fn pixel_stats(image: &RgbaImage) -> PixelStats {
    let mut stats = PixelStats {
        opaque: 0,
        translucent: 0,
        transparent: 0,
        average: None,
        mean_alpha: 0.0,
        colors: 0,
    };
    // One bit per 24-bit colour: 2 MiB regardless of image size, where a set
    // of a 64-megapixel image's colours could need gigabytes.
    let mut seen = vec![0_u64; 1 << 18];
    let mut sum = [0_u64; 3];
    let mut weight = 0_u64;
    let mut alpha = 0_u64;
    for &Rgba([r, g, b, a]) in image.pixels() {
        alpha += u64::from(a);
        match a {
            0 => {
                stats.transparent += 1;
                continue;
            }
            255 => stats.opaque += 1,
            _ => stats.translucent += 1,
        }
        for (total, channel) in sum.iter_mut().zip([r, g, b]) {
            *total += u64::from(channel) * u64::from(a);
        }
        weight += u64::from(a);
        let key = usize::from(r) << 16 | usize::from(g) << 8 | usize::from(b);
        let bit = 1 << (key % 64);
        if seen[key / 64] & bit == 0 {
            seen[key / 64] |= bit;
            stats.colors += 1;
        }
    }
    if weight > 0 {
        stats.average = Some(sum.map(|total| ((total + weight / 2) / weight) as u8));
    }
    if stats.total() > 0 {
        stats.mean_alpha = alpha as f64 / stats.total() as f64;
    }
    stats
}

#[derive(Clone)]
pub struct History {
    pub original: Arc<RgbaImage>,
    entries: Vec<(u64, Operation)>,
    cursor: usize,
    next_id: u64,
}
impl History {
    pub fn new(image: RgbaImage) -> Self {
        Self {
            original: Arc::new(image),
            entries: Vec::new(),
            cursor: 0,
            next_id: 1,
        }
    }
    pub fn state_id(&self) -> u64 {
        self.cursor.checked_sub(1).map_or(0, |i| self.entries[i].0)
    }
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }
    pub fn push(&mut self, op: Operation) {
        self.entries.truncate(self.cursor);
        self.entries.push((self.next_id, op));
        self.next_id += 1;
        self.cursor += 1;
    }
    pub fn undo(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn redo(&mut self) {
        self.cursor = (self.cursor + 1).min(self.entries.len());
    }
    pub fn render(&self) -> Result<RgbaImage> {
        let mut image = (*self.original).clone();
        for (_, op) in &self.entries[..self.cursor] {
            image = apply(&image, op)?;
        }
        Ok(image)
    }
}

pub fn parse_background(s: &str) -> Result<[u8; 3]> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    ensure!(
        hex.len() == 6 && hex.is_ascii(),
        "Use a six-digit color, such as #ffffff"
    );
    Ok([
        u8::from_str_radix(&hex[0..2], 16)?,
        u8::from_str_radix(&hex[2..4], 16)?,
        u8::from_str_radix(&hex[4..6], 16)?,
    ])
}

/// Reject aliases too: a symlink or hard link must not bypass original protection.
pub fn protect_original(source: &Path, destination: &Path) -> Result<()> {
    let source = source
        .canonicalize()
        .context("Cannot resolve original image")?;
    if let Ok(destination) = destination.canonicalize() {
        ensure!(
            source != destination,
            "Choose a different output file to preserve the original"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let a = fs::metadata(&source)?;
            let b = fs::metadata(&destination)?;
            ensure!(
                a.dev() != b.dev() || a.ino() != b.ino(),
                "Output is a hard link to the original image"
            );
        }
    }
    Ok(())
}

/// File extension of `path`, lowercased and without the dot.
pub fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Output formats this crate encodes itself, keyed by lowercase extension.
/// HEIC, AVIF, TIFF, BMP and JPEG 2000 are encoded by Image I/O in `candy-macos`.
pub fn export_format(extension: &str) -> Option<ImageFormat> {
    Some(match extension {
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        "gif" => ImageFormat::Gif,
        "tga" => ImageFormat::Tga,
        _ => return None,
    })
}

pub fn save_atomic(image: &RgbaImage, path: &Path, overwrite: bool) -> Result<()> {
    let format = export_format(&extension(path)).context("Save as PNG, JPEG, WebP, GIF or TGA")?;
    write_atomic(path, overwrite, |file| encode(image, format, file))
}

/// Writes `path` through a temporary file in the same directory, so a failed
/// or interrupted encode never leaves a partial image behind.
pub fn write_atomic(
    path: &Path,
    overwrite: bool,
    write: impl FnOnce(&mut fs::File) -> Result<()>,
) -> Result<()> {
    ensure!(
        overwrite || !path.exists(),
        "Output already exists; choose another path or use --force"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).context("Cannot create a temporary output file")?;
    write(temp.as_file_mut())?;
    temp.as_file().sync_all()?;
    if overwrite {
        temp.persist(path).map_err(|e| e.error)?;
    } else {
        temp.persist_noclobber(path).map_err(|e| e.error)?;
    }
    Ok(())
}

fn encode(image: &RgbaImage, format: ImageFormat, file: &mut fs::File) -> Result<()> {
    match format {
        ImageFormat::Jpeg => {
            let flat = apply(image, &Operation::Flatten([255; 3]))?;
            let rgb = DynamicImage::ImageRgba8(flat).to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(file, 92).encode_image(&rgb)?;
        }
        ImageFormat::Gif => {
            // GIF has one transparent index: pixels under 50% alpha drop out,
            // the rest keep their own colour. Speed 10 samples the palette
            // quickly; speed 1 would take minutes on large images.
            let mut frame = image.clone();
            for pixel in frame.pixels_mut() {
                pixel[3] = if pixel[3] < 128 { 0 } else { 255 };
            }
            image::codecs::gif::GifEncoder::new_with_speed(file, 10)
                .encode(
                    &frame,
                    frame.width(),
                    frame.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .context("GIF encoding failed")?;
        }
        _ => DynamicImage::ImageRgba8(image.clone())
            .write_to(file, format)
            .context("Image encoding failed")?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn grid() -> RgbaImage {
        RgbaImage::from_fn(8, 6, |x, y| Rgba([(x * 25) as u8, (y * 35) as u8, 80, 255]))
    }
    #[test]
    fn crop_checks_bounds_and_preserves_pixels() {
        let image = grid();
        let rect = PixelRect::new(2, 1, 3, 2);
        let crop = apply(&image, &Operation::Crop(rect)).unwrap();
        assert_eq!(crop.dimensions(), (3, 2));
        assert_eq!(crop.get_pixel(0, 0), image.get_pixel(2, 1));
        assert!(apply(&image, &Operation::Crop(PixelRect::new(u32::MAX, 0, 2, 2))).is_err());
    }
    #[test]
    fn mosaic_preserves_every_pixel_outside_region() {
        let image = grid();
        let result = apply(
            &image,
            &Operation::Mosaic {
                regions: vec![PixelRect::new(2, 1, 3, 3)],
                block: 2,
            },
        )
        .unwrap();
        for (x, y, p) in image.enumerate_pixels() {
            if !(2..5).contains(&x) || !(1..4).contains(&y) {
                assert_eq!(p, result.get_pixel(x, y));
            }
        }
        assert_eq!(result.get_pixel(2, 1), result.get_pixel(3, 2));
        assert_ne!(result, image);
    }
    #[test]
    fn alpha_is_composited_not_just_discarded() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 128]));
        assert_eq!(
            apply(&image, &Operation::Flatten([255; 3]))
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [255, 127, 127, 255]
        );
    }
    #[test]
    fn brush_joins_do_not_double_blend() {
        let image = RgbaImage::from_pixel(10, 10, Rgba([255; 4]));
        let result = apply(
            &image,
            &Operation::Highlight {
                points: vec![[2.5, 4.5], [4.5, 4.5], [6.5, 4.5]],
                width: 3.0,
                color: [255, 200, 0, 100],
            },
        )
        .unwrap();
        assert_eq!(result.get_pixel(3, 4), result.get_pixel(4, 4));
        assert_eq!(result.get_pixel(0, 0), image.get_pixel(0, 0));
    }
    #[test]
    fn history_branches_have_distinct_save_states() {
        let mut history = History::new(grid());
        history.push(Operation::Flatten([255; 3]));
        let saved = history.state_id();
        history.undo();
        assert_eq!(history.render().unwrap(), grid());
        history.redo();
        assert_eq!(history.state_id(), saved);
        history.undo();
        history.push(Operation::Crop(PixelRect::new(0, 0, 2, 2)));
        assert_ne!(history.state_id(), saved);
        assert!(!history.can_redo());
    }
    #[test]
    fn resize_preserves_ratio_and_rejects_excessive_allocations() {
        assert_eq!(
            resize_dimensions(400, 200, Some(100), None).unwrap(),
            (100, 50)
        );
        assert!(resize_dimensions(400, 200, Some(0), None).is_err());
        assert!(validate_dimensions(u32::MAX, u32::MAX).is_err());
    }
    #[test]
    fn save_roundtrips_and_never_clobbers_by_default() {
        let dir = tempfile::tempdir().unwrap();
        for ext in ["png", "jpg", "webp", "gif", "tga"] {
            let path = dir.path().join(format!("image.{ext}"));
            save_atomic(&grid(), &path, false).unwrap();
            let decoded = image::open(&path).unwrap();
            assert_eq!(decoded.dimensions(), (8, 6), "{ext}");
            if matches!(ext, "png" | "webp" | "tga") {
                assert_eq!(decoded.into_rgba8(), grid(), "{ext} is lossless");
            }
            let bytes = fs::read(&path).unwrap();
            assert!(save_atomic(&grid(), &path, false).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert!(protect_original(&path, &path).is_err());
        }
        assert!(export_format("tif").is_none(), "TIFF belongs to Image I/O");
        assert!(export_format("bmp").is_none(), "BMP belongs to Image I/O");
        let error = save_atomic(&grid(), &dir.path().join("image.xyz"), false).unwrap_err();
        assert!(error.to_string().starts_with("Save as PNG"), "{error}");
    }
    #[test]
    fn gif_keeps_binary_transparency_and_the_pixel_colours() {
        let image = RgbaImage::from_fn(4, 1, |x, _| {
            Rgba([200, 40, 40, [0, 127, 128, 255][x as usize]])
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.gif");
        save_atomic(&image, &path, false).unwrap();
        let decoded = image::open(&path).unwrap().into_rgba8();
        assert_eq!(decoded.get_pixel(0, 0)[3], 0);
        assert_eq!(decoded.get_pixel(1, 0)[3], 0);
        assert_eq!(decoded.get_pixel(2, 0), &Rgba([200, 40, 40, 255]));
        assert_eq!(decoded.get_pixel(3, 0), &Rgba([200, 40, 40, 255]));
    }
    #[test]
    fn pixel_stats_weight_by_alpha_and_count_distinct_colours() {
        let image = RgbaImage::from_raw(
            3,
            2,
            vec![
                255, 0, 0, 255, // opaque red
                255, 0, 0, 255, // the same colour again
                0, 0, 255, 128, // translucent blue
                9, 9, 9, 0, // transparent: excluded from colours and the average
                0, 255, 0, 255, // opaque green
                0, 0, 255, 128, // translucent blue again
            ],
        )
        .unwrap();
        let stats = pixel_stats(&image);
        assert_eq!(
            (stats.opaque, stats.translucent, stats.transparent),
            (3, 2, 1)
        );
        assert_eq!(stats.total(), 6);
        assert_eq!(stats.colors, 3);
        // Weights: 255 × 3 opaque + 128 × 2 translucent = 1021.
        assert_eq!(stats.average, Some([127, 64, 64]));
        assert!((stats.mean_alpha - 1021.0 / 6.0).abs() < 1e-9);
        let clear = pixel_stats(&RgbaImage::from_pixel(2, 2, Rgba([30, 40, 50, 0])));
        assert_eq!((clear.transparent, clear.colors), (4, 0));
        assert_eq!(clear.average, None);
        assert_eq!(clear.mean_alpha, 0.0);
        assert_eq!(pixel_stats(&RgbaImage::new(0, 0)).total(), 0);
    }
    #[test]
    fn reverse_drag_clamps_and_rejects_empty_rectangles() {
        assert_eq!(
            PixelRect::from_points([6.0, 5.0], [-1.0, 2.0], 8, 6),
            Some(PixelRect::new(0, 2, 6, 3))
        );
        assert!(PixelRect::from_points([2.0; 2], [2.0; 2], 8, 6).is_none());
    }
    use image::GenericImageView;
}
