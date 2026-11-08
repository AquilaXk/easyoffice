//! Status codes and the C structs of the extension boundary (`EasyOfficeQuickLookFFI.h` mirrors them).

use std::ptr;

pub const STATUS_OK: i32 = 0;
pub const STATUS_INVALID_INPUT: i32 = 1;
pub const STATUS_PARSE_FAILED: i32 = 2;
pub const STATUS_EMPTY_DOCUMENT: i32 = 3;
pub const STATUS_RENDER_FAILED: i32 = 4;
pub const STATUS_NO_THUMBNAIL: i32 = 5;
pub const STATUS_TOO_LARGE: i32 = 6;
pub const STATUS_TIMEOUT: i32 = 7;

/// A buffer allocated by this crate; the caller returns it with `eo_ql_free_bytes`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bytes {
    pub ptr: *mut u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderResult {
    pub status: i32,
    pub bytes: Bytes,
    pub page_count: u32,
    /// First page size in CSS pixels (96 per inch).
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ThumbnailResult {
    pub status: i32,
    pub bytes: Bytes,
    pub width: u32,
    pub height: u32,
}

impl Bytes {
    pub const EMPTY: Bytes = Bytes {
        ptr: ptr::null_mut(),
        len: 0,
    };

    /// Hands the vector to the caller; `free` is the only way back.
    pub fn from_vec(bytes: Vec<u8>) -> Bytes {
        let raw = Box::into_raw(bytes.into_boxed_slice());
        Bytes {
            ptr: raw.cast::<u8>(),
            len: raw.len(),
        }
    }

    /// # Safety
    ///
    /// `ptr` and `len` must come from [`Bytes::from_vec`] and be freed at most once.
    pub unsafe fn free(ptr: *mut u8, len: usize) {
        if ptr.is_null() || len == 0 {
            return;
        }
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(ptr, len)));
    }
}

impl RenderResult {
    pub fn failed(status: i32) -> Self {
        Self {
            status,
            bytes: Bytes::EMPTY,
            page_count: 0,
            width: 0.0,
            height: 0.0,
        }
    }
}

impl ThumbnailResult {
    pub fn failed(status: i32) -> Self {
        Self {
            status,
            bytes: Bytes::EMPTY,
            width: 0,
            height: 0,
        }
    }
}
