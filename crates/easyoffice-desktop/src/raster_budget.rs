//! Pixel limits for the pictures inside a page SVG that is decoded (PD-145).
//!
//! The thumbnail of a saved document, the start-window card, the PDF export and the Quick Look
//! extensions decode every JPEG, PNG, GIF and WebP of a page, and a page SVG is untrusted. The
//! limits differ by caller: a bitmap of 256 pixels needs far less than a PDF that keeps the photo.
//! The Quick Look crate includes this file by path (`apps/quicklook/rust/src/raster.rs`). The decoders allocate by the size in the
//! picture's header, so the header is read first (`imagesize`, the reader usvg itself uses) and a
//! picture that is too large, or one that would take the page over its total, is dropped: the page
//! is drawn without it. A picture that is not a readable raster image is dropped too.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Largest picture decoded for a bitmap, in pixels (4096 x 4096; 64 MiB as RGBA).
pub const MAX_IMAGE_PIXELS: u64 = 4096 * 4096;
/// Largest sum of the pictures decoded for one bitmap page, in pixels (four of the largest).
pub const MAX_PAGE_PIXELS: u64 = 4 * MAX_IMAGE_PIXELS;
/// Largest picture the PDF export keeps, in pixels. A 50 MP camera original (8192 x 6144) and a
/// 61 MP full-frame one (9504 x 6336) pass; 64 million pixels are 512 MB as 16-bit RGBA, the
/// 512 MiB allocation limit the `image` crate applies to every decode, so what is admitted can
/// always be decoded.
pub const MAX_EXPORT_IMAGE_PIXELS: u64 = 64_000_000;
/// Largest sum of the pictures one exported page keeps, in pixels (four of the largest).
pub const MAX_EXPORT_PAGE_PIXELS: u64 = 4 * MAX_EXPORT_IMAGE_PIXELS;

/// What one drawn page may still decode.
#[derive(Debug)]
pub struct RasterBudget {
    per_image: u64,
    remaining: AtomicU64,
    refused: AtomicU32,
}

impl RasterBudget {
    pub fn new(per_image: u64, page: u64) -> Self {
        Self {
            per_image,
            remaining: AtomicU64::new(page),
            refused: AtomicU32::new(0),
        }
    }

    /// The limits of a thumbnail or card ([`MAX_IMAGE_PIXELS`], [`MAX_PAGE_PIXELS`]).
    pub fn for_page() -> Self {
        Self::new(MAX_IMAGE_PIXELS, MAX_PAGE_PIXELS)
    }

    /// The limits of one exported PDF page ([`MAX_EXPORT_IMAGE_PIXELS`], [`MAX_EXPORT_PAGE_PIXELS`]).
    pub fn for_export() -> Self {
        Self::new(MAX_EXPORT_IMAGE_PIXELS, MAX_EXPORT_PAGE_PIXELS)
    }

    /// How many pictures [`admit`](Self::admit) has refused.
    pub fn refused(&self) -> u32 {
        self.refused.load(Ordering::Relaxed)
    }

    /// Whether the picture in `data` may be decoded; when it may, its pixels are taken from the
    /// page total. The size is the header's; a picture whose header cannot be read is refused.
    pub fn admit(&self, data: &[u8]) -> bool {
        let admitted = self.try_take(data);
        if !admitted {
            self.refused.fetch_add(1, Ordering::Relaxed);
        }
        admitted
    }

    fn try_take(&self, data: &[u8]) -> bool {
        let Ok(size) = imagesize::blob_size(data) else {
            return false;
        };
        let pixels = (size.width as u64).saturating_mul(size.height as u64);
        if pixels == 0 || pixels > self.per_image {
            return false;
        }
        // A compare-exchange loop rather than `fetch_update`/`try_update`: the latter was renamed
        // between toolchains, and this form compiles warning-free on the MSRV and on current stable.
        let mut left = self.remaining.load(Ordering::Relaxed);
        loop {
            let Some(rest) = left.checked_sub(pixels) else {
                return false;
            };
            match self.remaining.compare_exchange_weak(
                left,
                rest,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(current) => left = current,
            }
        }
    }
}

#[cfg(test)]
#[path = "raster_budget_tests.rs"]
mod tests;
