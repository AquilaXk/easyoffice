//! C ABI of the macOS Quick Look extensions (preview and thumbnail) for HWP/HWPX files.
//!
//! The Swift side reads the file, calls one of the functions below and gets a status plus a buffer
//! this crate allocated (`eo_ql_free_bytes` returns it). Finder passes arbitrary files, so every
//! call is bounded (input size, page SVG size, pixel size, page count, PDF size, wall-clock time
//! and the number of requests in flight) and fails closed: any problem is a status code and no bytes, and the
//! extension then answers with the system's default icon. Nothing unwinds across the C boundary.
//! No path, document text or log line leaves this crate.

#[cfg(test)]
mod bench_tests;
mod bounded;
mod fonts;
mod raster;
mod render;
mod status;
#[cfg(test)]
mod tests;

use status::{
    Bytes, RenderResult, ThumbnailResult, STATUS_INVALID_INPUT, STATUS_NO_THUMBNAIL, STATUS_OK,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

/// A thumbnail is drawn while Finder waits; a preview may take longer.
const THUMBNAIL_DEADLINE: Duration = Duration::from_secs(15);
const PREVIEW_DEADLINE: Duration = Duration::from_secs(30);
/// The embedded picture is a stream read, no layout.
const EMBEDDED_DEADLINE: Duration = Duration::from_secs(5);

/// The bytes at `data` after the checks that need no copy.
///
/// # Safety
///
/// `data` must be null or point to `len` readable bytes, which stay readable for the lifetime `'a`.
unsafe fn checked_input<'a>(data: *const u8, len: usize) -> Result<&'a [u8], i32> {
    if data.is_null() || len == 0 {
        return Err(STATUS_INVALID_INPUT);
    }
    // Refuse before copying: a hostile length must not cost a huge allocation.
    if len > render::MAX_INPUT_BYTES {
        return Err(status::STATUS_TOO_LARGE);
    }
    Ok(std::slice::from_raw_parts(data, len))
}

/// The result `fallback` makes when `body` panics: a panic never unwinds into the caller (the
/// extension would be killed), also when it comes from outside the worker.
fn guarded<R>(fallback: impl FnOnce(i32) -> R, body: impl FnOnce() -> R) -> R {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| fallback(status::STATUS_RENDER_FAILED))
}

/// All pages (at most 64) of the document as one PDF, for the Quick Look preview.
///
/// # Safety
///
/// `data` must be null or point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn eo_ql_render_preview_pdf(data: *const u8, len: usize) -> RenderResult {
    guarded(RenderResult::failed, || {
        let input = match checked_input(data, len) {
            Ok(input) => input,
            Err(code) => return RenderResult::failed(code),
        };
        let outcome = bounded::run_with(
            PREVIEW_DEADLINE,
            || Ok(input.to_vec()),
            |bytes| {
                let (core, page_count) = render::open(&bytes)?;
                let (pdf, first) = render::pdf(&core, page_count)?;
                Ok((pdf, page_count, first.width, first.height))
            },
        );
        match outcome {
            Ok((pdf, page_count, width, height)) => RenderResult {
                status: STATUS_OK,
                bytes: Bytes::from_vec(pdf),
                page_count,
                width: f64::from(width),
                height: f64::from(height),
            },
            Err(code) => RenderResult::failed(code),
        }
    })
}

/// The first page as a PNG whose longer side is `max_dimension` pixels (0 means the largest size).
///
/// # Safety
///
/// `data` must be null or point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn eo_ql_render_first_page_png(
    data: *const u8,
    len: usize,
    max_dimension: u32,
) -> RenderResult {
    guarded(RenderResult::failed, || {
        let input = match checked_input(data, len) {
            Ok(input) => input,
            Err(code) => return RenderResult::failed(code),
        };
        let side = if max_dimension == 0 {
            render::MAX_PNG_SIDE
        } else {
            max_dimension
        };
        let outcome = bounded::run_with(
            THUMBNAIL_DEADLINE,
            || Ok(input.to_vec()),
            move |bytes| {
                let (core, page_count) = render::open(&bytes)?;
                let first = render::page(&core, 0)?;
                let png = render::png(&first, side)?;
                Ok((png, page_count, first.width, first.height))
            },
        );
        match outcome {
            Ok((png, page_count, width, height)) => RenderResult {
                status: STATUS_OK,
                bytes: Bytes::from_vec(png),
                page_count,
                width: f64::from(width),
                height: f64::from(height),
            },
            Err(code) => RenderResult::failed(code),
        }
    })
}

/// The preview picture stored in the file (`PrvImage`), if it has one. The parser's own size limit
/// applies and so does the pixel limit of a thumbnail (4096 x 4096, read from the picture's
/// header); the picture is returned as stored (PNG, BMP or GIF) for the system image decoder.
///
/// # Safety
///
/// `data` must be null or point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn eo_ql_extract_embedded_thumbnail(
    data: *const u8,
    len: usize,
) -> ThumbnailResult {
    guarded(ThumbnailResult::failed, || {
        let input = match checked_input(data, len) {
            Ok(input) => input,
            Err(code) => return ThumbnailResult::failed(code),
        };
        let outcome = bounded::run_with(
            EMBEDDED_DEADLINE,
            || Ok(input.to_vec()),
            |bytes| {
                rhwp::parser::extract_thumbnail_only(&bytes)
                    .filter(|found| !found.data.is_empty())
                    // The system decoder allocates by the size in the picture's header, which the
                    // file controls: a picture over the thumbnail pixel limit (or with no readable
                    // header) is not handed over (PD-175).
                    .filter(|found| raster::RasterBudget::for_page().admit(&found.data))
                    .ok_or(STATUS_NO_THUMBNAIL)
            },
        );
        match outcome {
            Ok(found) => ThumbnailResult {
                status: STATUS_OK,
                width: found.width,
                height: found.height,
                bytes: Bytes::from_vec(found.data),
            },
            Err(code) => ThumbnailResult::failed(code),
        }
    })
}

/// Returns a buffer this crate handed out.
///
/// # Safety
///
/// `ptr` and `len` must be a buffer returned by this crate and not yet freed (null is accepted).
#[no_mangle]
pub unsafe extern "C" fn eo_ql_free_bytes(ptr: *mut u8, len: usize) {
    Bytes::free(ptr, len);
}
