//! Small macOS boundary. Image and Vision objects stay on the calling worker
//! thread; the menu bar lives on the main thread.
pub mod menu;
use anyhow::{Context, Result, ensure};
use candy_core::{PixelRect, validate_dimensions};
use image::{DynamicImage, ImageFormat, RgbaImage};
use objc2::{AnyThread, rc::autoreleasepool};
use objc2_core_foundation::{CFData, CFDictionary, CFNumber, CFString, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use objc2_image_io::{
    CGImageSource, kCGImagePropertyOrientation, kCGImagePropertyPixelHeight,
    kCGImagePropertyPixelWidth,
};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};
use std::{io::Cursor, path::Path};

pub fn load(path: &Path) -> Result<RgbaImage> {
    autoreleasepool(|_| {
        let bytes =
            std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
        let data = CFData::from_bytes(&bytes);
        // SAFETY: All options are absent, and retained CF objects outlive each native call.
        let source =
            unsafe { CGImageSource::with_data(&data, None) }.context("Cannot decode this image")?;
        let kind = unsafe { source.r#type() }
            .context("Unknown image format")?
            .to_string();
        ensure!(
            [
                "public.png",
                "public.jpeg",
                "org.webmproject.webp",
                "public.heic",
                "public.heif",
                "public.avif"
            ]
            .contains(&kind.as_str()),
            "Supported image formats are PNG, JPEG, WebP, HEIC and AVIF (found {kind})"
        );
        let props = unsafe { source.properties_at_index(0, None) }
            .context("Cannot read image dimensions")?;
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

fn property_number(props: &CFDictionary, key: &CFString) -> Option<i64> {
    // SAFETY: Image I/O documents these three property keys as CFNumber values.
    // The dictionary retains them throughout this borrow; null is checked.
    let ptr = unsafe { props.value((key as *const CFString).cast()) };
    unsafe { ptr.cast::<CFNumber>().as_ref() }.and_then(CFNumber::as_i64)
}

#[derive(Clone, Debug)]
pub struct TextRegion {
    pub rect: PixelRect,
    pub text: String,
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

pub fn detect_text(image: &RgbaImage) -> Result<Vec<TextRegion>> {
    autoreleasepool(|_| {
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image.clone()).write_to(&mut encoded, ImageFormat::Png)?;
        let data = NSData::with_bytes(encoded.get_ref());
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &data,
            &NSDictionary::new(),
        );
        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setAutomaticallyDetectsLanguage(true);
        // Restrict hints to languages the installed Vision revision supports.
        let supported = unsafe { request.supportedRecognitionLanguagesAndReturnError() }
            .map_err(|e| anyhow::anyhow!("Cannot query OCR languages: {e}"))?;
        let hints: Vec<_> = ["ko-KR", "en-US"]
            .iter()
            .filter(|lang| supported.iter().any(|v| v.to_string() == **lang))
            .map(|lang| NSString::from_str(lang))
            .collect();
        if !hints.is_empty() {
            request.setRecognitionLanguages(&NSArray::from_retained_slice(&hints));
        }
        let requests = NSArray::<VNRequest>::from_slice(&[&request]);
        handler
            .performRequests_error(&requests)
            .map_err(|e| anyhow::anyhow!("Text detection failed: {e}"))?;
        let mut regions = Vec::new();
        if let Some(results) = request.results() {
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
                    let text = observation
                        .topCandidates(1)
                        .firstObject()
                        .map(|c| c.string().to_string())
                        .unwrap_or_default();
                    regions.push(TextRegion { rect, text });
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
