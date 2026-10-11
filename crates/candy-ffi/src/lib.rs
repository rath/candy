//! Synchronous C boundary for the native app. See candy.h for ownership and
//! threading contracts. No AppKit calls, callbacks, or UI-thread work lives here.
// All exported functions share the safety contract documented in candy.h.
#![allow(clippy::missing_safety_doc)]

use anyhow::{Context, Result, ensure};
use candy_core::{History, Operation, PixelRect};
use image::RgbaImage;
use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    ptr,
    sync::Arc,
};

pub struct CandyDocument {
    history: History,
    image: Arc<RgbaImage>,
    source: Option<PathBuf>,
    saved: Option<u64>,
    revision: u64,
}
pub struct CandyImage(Arc<RgbaImage>);

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CandyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl From<CandyRect> for PixelRect {
    fn from(r: CandyRect) -> Self {
        Self::new(r.x, r.y, r.width, r.height)
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CandyPoint {
    pub x: f32,
    pub y: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CandyState {
    pub width: u32,
    pub height: u32,
    pub state_id: u64,
    pub revision: u64,
    pub dirty: u8,
    pub can_undo: u8,
    pub can_redo: u8,
}
#[repr(C)]
pub struct CandyPreview {
    pub width: u32,
    pub height: u32,
    pub length: usize,
    pub data: *mut u8,
}
#[repr(C)]
pub struct CandyRegions {
    pub count: usize,
    pub data: *mut CandyRect,
}

// Errors are owned, per-call values; no global or thread-local error state.
fn guarded<T: Default>(error: *mut *mut c_char, f: impl FnOnce() -> Result<T>) -> T {
    if !error.is_null() {
        unsafe {
            *error = ptr::null_mut();
        }
    }
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(anyhow::anyhow!(
            "The image engine encountered an unexpected error"
        ))
    });
    match result {
        Ok(value) => value,
        Err(e) => {
            if !error.is_null() {
                let message = CString::new(format!("{e:#}").replace('\0', " ")).unwrap_or_default();
                unsafe {
                    *error = message.into_raw();
                }
            }
            T::default()
        }
    }
}
unsafe fn path(value: *const c_char) -> Result<PathBuf> {
    ensure!(!value.is_null(), "Missing file path");
    use std::os::unix::ffi::OsStrExt;
    Ok(std::ffi::OsStr::from_bytes(unsafe { CStr::from_ptr(value) }.to_bytes()).into())
}
unsafe fn doc<'a>(value: *mut CandyDocument) -> Result<&'a mut CandyDocument> {
    unsafe { value.as_mut() }.context("Missing document")
}
unsafe fn items<'a, T>(value: *const T, count: usize) -> Result<&'a [T]> {
    ensure!(
        count <= isize::MAX as usize / std::mem::size_of::<T>(),
        "Invalid buffer length"
    );
    if count == 0 {
        return Ok(&[]);
    }
    ensure!(!value.is_null(), "Missing buffer");
    Ok(unsafe { std::slice::from_raw_parts(value, count) })
}
impl CandyDocument {
    fn new(image: RgbaImage, source: Option<PathBuf>, dirty: bool) -> Self {
        let history = History::new(image);
        Self {
            image: history.original.clone(),
            history,
            source,
            saved: if dirty { None } else { Some(0) },
            revision: 0,
        }
    }
    fn commit(&mut self, history: History) -> Result<()> {
        let image = history.render()?;
        self.image = Arc::new(image);
        self.history = history;
        self.revision += 1;
        Ok(())
    }
    fn apply(&mut self, operation: Operation) -> Result<u8> {
        let mut history = self.history.clone();
        history.push(operation);
        self.commit(history)?;
        Ok(1)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_error_free(error: *mut c_char) {
    if !error.is_null() {
        drop(unsafe { CString::from_raw(error) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_open(
    file: *const c_char,
    error: *mut *mut c_char,
) -> *mut CandyDocument {
    guarded(error, || {
        let file = unsafe { path(file)? }.canonicalize()?;
        let image = candy_macos::load(&file)?;
        Ok(Box::into_raw(Box::new(CandyDocument::new(
            image,
            Some(file),
            false,
        ))))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_free(document: *mut CandyDocument) {
    if !document.is_null() {
        drop(unsafe { Box::from_raw(document) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_state(
    document: *mut CandyDocument,
    error: *mut *mut c_char,
) -> CandyState {
    guarded(error, || {
        let d = unsafe { doc(document)? };
        Ok(CandyState {
            width: d.image.width(),
            height: d.image.height(),
            state_id: d.history.state_id(),
            revision: d.revision,
            dirty: u8::from(d.saved != Some(d.history.state_id())),
            can_undo: u8::from(d.history.can_undo()),
            can_redo: u8::from(d.history.can_redo()),
        })
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_history(
    document: *mut CandyDocument,
    redo: u8,
    error: *mut *mut c_char,
) -> u8 {
    guarded(error, || {
        let d = unsafe { doc(document)? };
        let mut history = d.history.clone();
        if redo != 0 {
            history.redo();
        } else {
            history.undo();
        }
        d.commit(history)?;
        Ok(1)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_resize(
    document: *mut CandyDocument,
    width: u32,
    height: u32,
    error: *mut *mut c_char,
) -> u8 {
    guarded(error, || {
        unsafe { doc(document)? }.apply(Operation::Resize { width, height })
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_flatten(
    document: *mut CandyDocument,
    r: u8,
    g: u8,
    b: u8,
    error: *mut *mut c_char,
) -> u8 {
    guarded(error, || {
        unsafe { doc(document)? }.apply(Operation::Flatten([r, g, b]))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_mosaic(
    document: *mut CandyDocument,
    rects: *const CandyRect,
    count: usize,
    block: u32,
    error: *mut *mut c_char,
) -> u8 {
    guarded(error, || {
        let regions = unsafe { items(rects, count)? }
            .iter()
            .copied()
            .map(Into::into)
            .collect();
        unsafe { doc(document)? }.apply(Operation::Mosaic { regions, block })
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_highlight(
    document: *mut CandyDocument,
    points: *const CandyPoint,
    count: usize,
    width: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    error: *mut *mut c_char,
) -> u8 {
    guarded(error, || {
        let points = unsafe { items(points, count)? }
            .iter()
            .map(|p| [p.x, p.y])
            .collect();
        unsafe { doc(document)? }.apply(Operation::Highlight {
            points,
            width,
            color: [r, g, b, a],
        })
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_extract(
    document: *mut CandyDocument,
    rect: CandyRect,
    error: *mut *mut c_char,
) -> *mut CandyDocument {
    guarded(error, || {
        let d = unsafe { doc(document)? };
        let image = candy_core::apply(&d.image, &Operation::Crop(rect.into()))?;
        // Derived images retain protection of the input they came from.
        Ok(Box::into_raw(Box::new(CandyDocument::new(
            image,
            d.source.clone(),
            true,
        ))))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_save(
    document: *mut CandyDocument,
    file: *const c_char,
    error: *mut *mut c_char,
) -> u8 {
    guarded(error, || {
        let d = unsafe { doc(document)? };
        let file = unsafe { path(file)? };
        if let Some(source) = &d.source {
            candy_core::protect_original(source, &file)?;
        }
        candy_core::save_atomic(&d.image, &file, true)?;
        d.saved = Some(d.history.state_id());
        Ok(1)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_preview(
    document: *mut CandyDocument,
    error: *mut *mut c_char,
) -> *mut CandyPreview {
    guarded(error, || {
        let d = unsafe { doc(document)? };
        let mut preview = if d.image.width() > 4096 || d.image.height() > 4096 {
            image::DynamicImage::ImageRgba8((*d.image).clone())
                .resize(4096, 4096, image::imageops::FilterType::Triangle)
                .into_rgba8()
        } else {
            (*d.image).clone()
        };
        let (width, height) = preview.dimensions();
        for pixel in preview.pixels_mut() {
            for channel in 0..3 {
                pixel[channel] =
                    ((u16::from(pixel[channel]) * u16::from(pixel[3]) + 127) / 255) as u8;
            }
        }
        let bytes = preview.into_raw().into_boxed_slice();
        let length = bytes.len();
        let data = Box::into_raw(bytes).cast::<u8>();
        Ok(Box::into_raw(Box::new(CandyPreview {
            width,
            height,
            length,
            data,
        })))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_preview_free(preview: *mut CandyPreview) {
    if !preview.is_null() {
        let p = unsafe { Box::from_raw(preview) };
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(p.data, p.length)) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_document_snapshot(
    document: *mut CandyDocument,
    error: *mut *mut c_char,
) -> *mut CandyImage {
    guarded(error, || {
        Ok(Box::into_raw(Box::new(CandyImage(
            unsafe { doc(document)? }.image.clone(),
        ))))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_image_free(image: *mut CandyImage) {
    if !image.is_null() {
        drop(unsafe { Box::from_raw(image) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_image_detect(
    image: *const CandyImage,
    error: *mut *mut c_char,
) -> *mut CandyRegions {
    guarded(error, || {
        let image = unsafe { image.as_ref() }.context("Missing image")?;
        let regions: Box<[_]> = candy_macos::detect_text(&image.0)?
            .into_iter()
            .map(|r| CandyRect {
                x: r.rect.x,
                y: r.rect.y,
                width: r.rect.width,
                height: r.rect.height,
            })
            .collect();
        let count = regions.len();
        let data = Box::into_raw(regions).cast::<CandyRect>();
        Ok(Box::into_raw(Box::new(CandyRegions { count, data })))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn candy_regions_free(regions: *mut CandyRegions) {
    if !regions.is_null() {
        let r = unsafe { Box::from_raw(regions) };
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(r.data, r.count)) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> CandyDocument {
        CandyDocument::new(
            RgbaImage::from_pixel(8, 6, image::Rgba([100, 200, 50, 128])),
            None,
            false,
        )
    }
    #[test]
    fn failed_edit_does_not_commit_history_or_pixels() {
        let mut d = document();
        let before = d.image.clone();
        assert!(
            d.apply(Operation::Resize {
                width: 0,
                height: 1
            })
            .is_err()
        );
        assert_eq!(*d.image, *before);
        assert!(!d.history.can_undo());
        assert_eq!(d.revision, 0);
    }
    #[test]
    fn ffi_errors_and_preview_ownership_are_explicit() {
        let mut error = ptr::null_mut();
        unsafe {
            assert_eq!(candy_document_resize(ptr::null_mut(), 1, 1, &mut error), 0);
            assert!(!error.is_null());
            candy_error_free(error);
            let mut d = document();
            let preview = candy_document_preview(&mut d, &mut error);
            assert!(error.is_null());
            assert_eq!((*preview).length, 8 * 6 * 4);
            assert_eq!(
                std::slice::from_raw_parts((*preview).data, 4),
                &[50, 100, 25, 128]
            );
            candy_preview_free(preview);
            assert_eq!(
                candy_document_mosaic(&mut d, ptr::null(), 1, 12, &mut error),
                0
            );
            assert!(!error.is_null());
            candy_error_free(error);
            candy_preview_free(ptr::null_mut());
        }
    }
    #[test]
    fn snapshots_survive_edits_and_document_destruction() {
        let mut d = document();
        let mut error = ptr::null_mut();
        unsafe {
            let snapshot = candy_document_snapshot(&mut d, &mut error);
            d.apply(Operation::Resize {
                width: 2,
                height: 2,
            })
            .unwrap();
            drop(d);
            assert_eq!((*snapshot).0.dimensions(), (8, 6));
            candy_image_free(snapshot);
        }
    }
    #[test]
    fn saves_track_history_branches_and_protect_derived_originals() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("original.png");
        let copy = temp.path().join("copy.png");
        let mut d = document();
        candy_core::save_atomic(&d.image, &source, false).unwrap();
        d.source = Some(source.clone());
        let source_c = CString::new(source.to_str().unwrap()).unwrap();
        let copy_c = CString::new(copy.to_str().unwrap()).unwrap();
        let mut error = ptr::null_mut();
        unsafe {
            d.apply(Operation::Flatten([255; 3])).unwrap();
            assert_eq!(candy_document_save(&mut d, copy_c.as_ptr(), &mut error), 1);
            assert_eq!(candy_document_state(&mut d, &mut error).dirty, 0);
            candy_document_history(&mut d, 0, &mut error);
            d.apply(Operation::Flatten([0; 3])).unwrap();
            assert_eq!(candy_document_state(&mut d, &mut error).dirty, 1);
            let crop = candy_document_extract(
                &mut d,
                CandyRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                &mut error,
            );
            assert_eq!(candy_document_save(crop, source_c.as_ptr(), &mut error), 0);
            candy_error_free(error);
            candy_document_free(crop);
            let alias = temp.path().join("alias.png");
            std::fs::hard_link(&source, &alias).unwrap();
            let alias_c = CString::new(alias.to_str().unwrap()).unwrap();
            assert_eq!(candy_document_save(&mut d, alias_c.as_ptr(), &mut error), 0);
            candy_error_free(error);
        }
    }
}
