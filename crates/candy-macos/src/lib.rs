//! Small macOS boundary. Image and Vision objects stay on the calling worker
//! thread. Shared by the CLI and the Swift app through candy-ffi.
use anyhow::{Context, Result, bail, ensure};
use candy_core::{PixelRect, validate_dimensions};
use image::{DynamicImage, RgbaImage};
use objc2::{AnyThread, rc::autoreleasepool};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString,
    CFType, CGPoint, CGRect, CGSize, ConcreteType,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGContext,
    CGDataProvider, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_image_io::{
    CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
    kCGImagePropertyColorModel, kCGImagePropertyDPIHeight, kCGImagePropertyDPIWidth,
    kCGImagePropertyDepth, kCGImagePropertyHasAlpha, kCGImagePropertyIsFloat,
    kCGImagePropertyIsIndexed, kCGImagePropertyOrientation, kCGImagePropertyPixelHeight,
    kCGImagePropertyPixelWidth, kCGImagePropertyProfileName, kCGImagePropertyTIFFCompression,
    kCGImagePropertyTIFFDictionary,
};
use objc2_vision::{VNDetectTextRectanglesRequest, VNImageRequestHandler, VNRequest};
use std::path::Path;

/// Display names for common Image I/O type identifiers. Candy opens anything
/// Image I/O decodes; this list only names the formats people meet most.
const FORMATS: [(&str, &str); 19] = [
    ("public.png", "PNG"),
    ("public.jpeg", "JPEG"),
    ("org.webmproject.webp", "WebP"),
    ("public.heic", "HEIC"),
    ("public.heif", "HEIF"),
    ("public.avif", "AVIF"),
    ("com.compuserve.gif", "GIF"),
    ("public.tiff", "TIFF"),
    ("com.microsoft.bmp", "BMP"),
    ("public.jpeg-2000", "JPEG 2000"),
    ("public.jpeg-xl", "JPEG XL"),
    ("com.adobe.photoshop-image", "PSD"),
    ("com.truevision.tga-image", "TGA"),
    ("com.microsoft.ico", "ICO"),
    ("com.apple.icns", "ICNS"),
    ("com.ilm.openexr-image", "OpenEXR"),
    ("public.radiance", "Radiance HDR"),
    ("public.pbm", "PBM"),
    ("com.adobe.raw-image", "DNG"),
];

fn format_name(uti: &str) -> Option<&'static str> {
    FORMATS
        .iter()
        .find(|(kind, _)| *kind == uti)
        .map(|(_, name)| *name)
        .or_else(|| uti.ends_with("-raw-image").then_some("RAW"))
}

/// Output formats encoded by Image I/O, keyed by lowercase extension, with the
/// destination type identifier and display name. Everything else goes through
/// `candy_core::export_format`.
fn image_io_format(extension: &str) -> Option<(&'static str, &'static str)> {
    Some(match extension {
        "heic" | "heif" => ("public.heic", "HEIC"),
        "avif" => ("public.avif", "AVIF"),
        "tif" | "tiff" => ("public.tiff", "TIFF"),
        // Image I/O cannot read the alpha-capable BMP header the image crate
        // writes, so BMP is encoded here too.
        "bmp" => ("com.microsoft.bmp", "BMP"),
        "jp2" | "j2k" | "jpf" | "jpx" => ("public.jpeg-2000", "JPEG 2000"),
        _ => return None,
    })
}

/// Image I/O quality for HEIC, AVIF and JPEG 2000 exports, on its 0–1 scale.
pub const LOSSY_QUALITY: f64 = 0.85;

/// Opens an image file and reads the first image's header properties.
fn open(path: &Path) -> Result<(CFRetained<CGImageSource>, String, CFRetained<CFDictionary>)> {
    let bytes = std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
    let data = CFData::from_bytes(&bytes);
    // SAFETY: All options are absent, and retained CF objects outlive each native call.
    let source =
        unsafe { CGImageSource::with_data(&data, None) }.context("Cannot decode this image")?;
    let kind = unsafe { source.r#type() }
        .context("Unknown image format")?
        .to_string();
    let props =
        unsafe { source.properties_at_index(0, None) }.context("Cannot read image properties")?;
    Ok((source, kind, props))
}

pub fn load(path: &Path) -> Result<RgbaImage> {
    autoreleasepool(|_| {
        let (source, _, props) = open(path)?;
        let width = property_number(&props, unsafe { kCGImagePropertyPixelWidth })
            .context("Missing image width")?;
        let height = property_number(&props, unsafe { kCGImagePropertyPixelHeight })
            .context("Missing image height")?;
        validate_dimensions(width.try_into()?, height.try_into()?)?;
        let orientation =
            property_number(&props, unsafe { kCGImagePropertyOrientation }).unwrap_or(1);
        let cg_image = unsafe { source.image_at_index(0, None) }
            .context("Image I/O could not decode the image")?;
        let w = CGImage::width(Some(&cg_image));
        let h = CGImage::height(Some(&cg_image));
        validate_dimensions(w.try_into()?, h.try_into()?)?;
        let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
            .context("Cannot create sRGB color space")?;
        let mut pixels = vec![0_u8; w * h * 4];
        // SAFETY: The backing buffer is w*h*4 bytes, remains allocated and is not
        // accessed while the context is alive. Big-endian + premultiplied-last
        // specifies byte order R,G,B,A independently of the host endianness.
        let context = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast(),
                w,
                h,
                8,
                w * 4,
                Some(&space),
                0x4001,
            )
        }
        .context("Cannot create image conversion context")?;
        CGContext::draw_image(
            Some(&context),
            CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as f64, h as f64)),
            Some(&cg_image),
        );
        drop(context);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            for c in 0..3 {
                pixel[c] = if pixel[3] == 0 {
                    0
                } else {
                    (u32::from(pixel[c]) * 255 / u32::from(pixel[3])).min(255) as u8
                };
            }
        }
        let image = RgbaImage::from_raw(w as u32, h as u32, pixels)
            .context("Invalid decoded pixel buffer")?;
        let mut image = DynamicImage::ImageRgba8(image);
        if let Some(orientation) = image::metadata::Orientation::from_exif(orientation as u8) {
            image.apply_orientation(orientation);
        }
        Ok(image.into_rgba8())
    })
}

fn property<'a, T: ConcreteType>(props: &'a CFDictionary, key: &CFString) -> Option<&'a T> {
    // SAFETY: Image I/O property values are CF objects that the dictionary
    // retains throughout this borrow. Null is checked, and `downcast_ref`
    // verifies the concrete type before the reference is used as `T`.
    let ptr = unsafe { props.value((key as *const CFString).cast()) };
    unsafe { ptr.cast::<CFType>().as_ref() }.and_then(CFType::downcast_ref)
}

fn property_number(props: &CFDictionary, key: &CFString) -> Option<i64> {
    property::<CFNumber>(props, key).and_then(CFNumber::as_i64)
}

fn property_f64(props: &CFDictionary, key: &CFString) -> Option<f64> {
    property::<CFNumber>(props, key).and_then(CFNumber::as_f64)
}

fn property_bool(props: &CFDictionary, key: &CFString) -> bool {
    // Image I/O includes these flags only when they are set, as CFBoolean; a
    // numeric 0/1 is accepted too.
    property::<CFBoolean>(props, key)
        .map(CFBoolean::as_bool)
        .or_else(|| property_number(props, key).map(|n| n != 0))
        .unwrap_or(false)
}

fn property_string(props: &CFDictionary, key: &CFString) -> Option<String> {
    property::<CFString>(props, key).map(ToString::to_string)
}

/// Header properties of an image file, read without decoding its pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Metadata {
    /// Uniform type identifier, such as `public.heic`.
    pub uti: String,
    /// Display name of common formats; `None` for rarer types Image I/O reads,
    /// which Candy still opens.
    pub format: Option<&'static str>,
    /// Stored pixel dimensions, before EXIF orientation.
    pub width: u32,
    pub height: u32,
    /// EXIF orientation 1–8; 1 when absent or invalid.
    pub orientation: u8,
    pub color_model: Option<String>,
    /// Bits per channel.
    pub depth: Option<u32>,
    /// Image I/O reports these flags only when set; absent means false.
    pub has_alpha: bool,
    pub is_float: bool,
    pub is_indexed: bool,
    /// Embedded colour profile name, such as `Display P3`.
    pub profile: Option<String>,
    pub dpi: Option<(f64, f64)>,
    /// Number of images or frames in the file.
    pub images: usize,
}

impl Metadata {
    /// Dimensions after orientation, as `load` returns them.
    pub fn display_dimensions(&self) -> (u32, u32) {
        if self.orientation >= 5 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

pub fn inspect(path: &Path) -> Result<Metadata> {
    autoreleasepool(|_| {
        let (source, uti, props) = open(path)?;
        let width = property_number(&props, unsafe { kCGImagePropertyPixelWidth })
            .context("Missing image width")?;
        let height = property_number(&props, unsafe { kCGImagePropertyPixelHeight })
            .context("Missing image height")?;
        let orientation = property_number(&props, unsafe { kCGImagePropertyOrientation })
            .filter(|o| (1..=8).contains(o))
            .map_or(1, |o| o as u8);
        let dpi = property_f64(&props, unsafe { kCGImagePropertyDPIWidth })
            .zip(property_f64(&props, unsafe { kCGImagePropertyDPIHeight }));
        Ok(Metadata {
            format: format_name(&uti),
            uti,
            width: width.try_into().context("Invalid image width")?,
            height: height.try_into().context("Invalid image height")?,
            orientation,
            color_model: property_string(&props, unsafe { kCGImagePropertyColorModel }),
            depth: property_number(&props, unsafe { kCGImagePropertyDepth })
                .and_then(|depth| depth.try_into().ok()),
            has_alpha: property_bool(&props, unsafe { kCGImagePropertyHasAlpha }),
            is_float: property_bool(&props, unsafe { kCGImagePropertyIsFloat }),
            is_indexed: property_bool(&props, unsafe { kCGImagePropertyIsIndexed }),
            profile: property_string(&props, unsafe { kCGImagePropertyProfileName }),
            dpi,
            // SAFETY: The retained source outlives this call.
            images: unsafe { source.count() },
        })
    })
}

/// Vision uses normalized, bottom-left coordinates; Candy uses top-left pixels.
pub fn vision_rect(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    image_width: u32,
    image_height: u32,
) -> Option<PixelRect> {
    if [x, y, width, height].iter().any(|v| !v.is_finite()) || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let padding = (height * f64::from(image_height) * 0.06).max(2.0);
    PixelRect::from_points(
        [
            (x * f64::from(image_width) - padding) as f32,
            ((1.0 - y - height) * f64::from(image_height) - padding) as f32,
        ],
        [
            ((x + width) * f64::from(image_width) + padding) as f32,
            ((1.0 - y) * f64::from(image_height) + padding) as f32,
        ],
        image_width,
        image_height,
    )
}

/// Saves `image` at `path`; the extension selects the format. HEIC, AVIF, TIFF
/// (LZW), BMP and JPEG 2000 go through Image I/O, the rest through `candy_core`.
pub fn save(image: &RgbaImage, path: &Path, overwrite: bool) -> Result<()> {
    let extension = candy_core::extension(path);
    if let Some((uti, name)) = image_io_format(&extension) {
        ensure!(
            can_encode(uti),
            "This version of macOS cannot encode {name}; save as PNG, JPEG, WebP or HEIC"
        );
        let bytes = encode(image, uti)?;
        candy_core::write_atomic(path, overwrite, |file| {
            std::io::Write::write_all(file, &bytes).context("Cannot write the output file")
        })
    } else if candy_core::export_format(&extension).is_some() {
        candy_core::save_atomic(image, path, overwrite)
    } else {
        bail!("Save as PNG, JPEG, WebP, HEIC, AVIF, TIFF, GIF, BMP, JPEG 2000 or TGA")
    }
}

/// Whether this macOS version's Image I/O can write `uti`. AVIF encoding, for
/// example, arrived after macOS 14.
pub fn can_encode(uti: &str) -> bool {
    // SAFETY: Image I/O returns a retained array of CFStrings.
    let identifiers = unsafe { CGImageDestination::type_identifiers() };
    let identifiers: &CFArray<CFString> = unsafe { identifiers.cast_unchecked() };
    identifiers.iter().any(|kind| kind.to_string() == uti)
}

/// Encodes `image` with Image I/O as `uti` and returns the file bytes. Lossy
/// formats use `LOSSY_QUALITY`; TIFF uses LZW compression.
pub fn encode(image: &RgbaImage, uti: &str) -> Result<Vec<u8>> {
    autoreleasepool(|_| {
        let cg_image = rgba_image(image)?;
        let data = CFMutableData::new(None, 0).context("Cannot allocate the output buffer")?;
        let kind = CFString::from_str(uti);
        // SAFETY: All options are absent; the retained destination, image and
        // property dictionaries outlive each call.
        let destination = unsafe { CGImageDestination::with_data(&data, &kind, 1, None) }
            .with_context(|| format!("Image I/O cannot encode {uti}"))?;
        let quality = CFNumber::new_f64(LOSSY_QUALITY);
        let lzw = CFNumber::new_i32(5);
        let tiff = CFDictionary::<CFString, CFType>::from_slices(
            &[unsafe { kCGImagePropertyTIFFCompression }],
            &[&lzw],
        );
        let properties = CFDictionary::<CFString, CFType>::from_slices(
            &[
                unsafe { kCGImageDestinationLossyCompressionQuality },
                unsafe { kCGImagePropertyTIFFDictionary },
            ],
            &[&quality, &tiff],
        );
        unsafe { destination.add_image(&cg_image, Some(properties.as_opaque())) };
        ensure!(
            unsafe { destination.finalize() },
            "Image I/O could not encode {uti}"
        );
        Ok(data.to_vec())
    })
}

// Pass owned RGBA pixels directly, avoiding a PNG encode/decode round trip.
fn rgba_image(image: &RgbaImage) -> Result<CFRetained<CGImage>> {
    validate_dimensions(image.width(), image.height())?;
    let data = CFData::from_bytes(image.as_raw());
    let provider = CGDataProvider::with_cf_data(Some(&data))
        .context("Cannot create the pixel data provider")?;
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
        .context("Cannot create sRGB color space")?;
    // SAFETY: CFData copies the validated w*h*4 bytes. The provider and image
    // retain their backing storage. Candy uses straight (not premultiplied)
    // RGBA; explicit big-endian byte order keeps that layout on either CPU.
    unsafe {
        CGImage::new(
            image.width() as usize,
            image.height() as usize,
            8,
            32,
            image.width() as usize * 4,
            Some(&space),
            CGBitmapInfo::from_bits_retain(
                CGImageAlphaInfo::Last.0 | CGImageByteOrderInfo::Order32Big.0,
            ),
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .context("Cannot create the image")
}

/// Detect candidate text bounds without transcribing their contents.
/// Recognition is unnecessary for mosaic and can spend tens of seconds compiling
/// Neural Engine language models, even when an image contains no text.
pub fn detect_text(image: &RgbaImage) -> Result<Vec<PixelRect>> {
    autoreleasepool(|_| {
        let started = std::time::Instant::now();
        let cg_image = rgba_image(image)?;
        // SAFETY: The CGImage owns its pixel data and outlives the synchronous request.
        let handler = unsafe {
            VNImageRequestHandler::initWithCGImage_options(
                VNImageRequestHandler::alloc(),
                &cg_image,
                &NSDictionary::new(),
            )
        };
        let prepared = started.elapsed();
        // SAFETY: This request is created and used on this worker thread.
        let request = unsafe { VNDetectTextRectanglesRequest::new() };
        let requests = NSArray::<VNRequest>::from_slice(&[&request]);
        let configured = started.elapsed();
        let timing = std::env::var_os("CANDY_OCR_TIMING").is_some();
        if timing {
            eprintln!(
                "Text detection {}x{}: prepare={prepared:?}, configure={:?}; starting Vision",
                image.width(),
                image.height(),
                configured - prepared
            );
        }
        let result = handler.performRequests_error(&requests);
        if timing {
            eprintln!(
                "Text detection Vision={:?}, success={}",
                started.elapsed() - configured,
                result.is_ok()
            );
        }
        result.map_err(|e| anyhow::anyhow!("Text detection failed: {e}"))?;
        let mut regions = Vec::new();
        // SAFETY: The request has completed; its retained observations stay local.
        if let Some(results) = unsafe { request.results() } {
            for observation in results.iter() {
                // SAFETY: The observation is a retained Vision rectangle observation.
                let bounds = unsafe { observation.boundingBox() };
                if let Some(rect) = vision_rect(
                    bounds.origin.x,
                    bounds.origin.y,
                    bounds.size.width,
                    bounds.size.height,
                    image.width(),
                    image.height(),
                ) {
                    regions.push(rect);
                }
            }
        }
        Ok(regions)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    #[test]
    fn rgba_image_owns_straight_rgba_pixels() {
        let source = RgbaImage::from_raw(
            2,
            2,
            vec![
                200, 50, 100, 128, 10, 240, 30, 255, 100, 200, 250, 0, 80, 120, 200, 64,
            ],
        )
        .unwrap();
        let image = rgba_image(&source).unwrap();
        drop(source);
        let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).unwrap();
        let mut pixels = [0_u8; 16];
        // SAFETY: The 2x2 RGBA buffer stays alive and untouched until context release.
        let context = unsafe {
            CGBitmapContextCreate(pixels.as_mut_ptr().cast(), 2, 2, 8, 8, Some(&space), 0x4001)
        }
        .unwrap();
        CGContext::draw_image(
            Some(&context),
            CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(2.0, 2.0)),
            Some(&image),
        );
        drop(context);
        let expected = [
            100_u8, 25, 50, 128, 10, 240, 30, 255, 0, 0, 0, 0, 20, 30, 50, 64,
        ];
        for (actual, expected) in pixels.into_iter().zip(expected) {
            assert!(actual.abs_diff(expected) <= 1, "{pixels:?}");
        }
        assert!(rgba_image(&RgbaImage::new(0, 1)).is_err());
    }

    #[test]
    fn native_decode_preserves_orientation_and_channels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("한글 sample.png");
        let image = RgbaImage::from_fn(7, 5, |x, y| {
            Rgba([(x * 30) as u8, (y * 40) as u8, 110, 255])
        });
        candy_core::save_atomic(&image, &path, false).unwrap();
        assert_eq!(load(&path).unwrap(), image);
    }
    #[test]
    fn normalized_regions_flip_y_and_include_padding() {
        let rect = vision_rect(0.1, 0.6, 0.3, 0.2, 100, 100).unwrap();
        assert_eq!(rect, PixelRect::new(8, 18, 34, 24));
        assert_eq!(
            vision_rect(0.0, 0.0, 1.0, 1.0, 100, 100),
            Some(PixelRect::new(0, 0, 100, 100))
        );
    }
    #[test]
    fn corrupt_files_return_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.png");
        std::fs::write(&path, b"not an image").unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn blank_image_has_no_text_regions() {
        let image = RgbaImage::from_pixel(300, 150, Rgba([255; 4]));
        assert!(detect_text(&image).unwrap().is_empty());
    }
}
