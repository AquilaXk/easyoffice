//! Pixel limits for the pictures inside a page that is drawn to a bitmap (PD-145).
//!
//! The thumbnail of a saved document and the start-window card decode every JPEG, PNG, GIF and
//! WebP of the first page, and a page SVG is untrusted. The decoders allocate by the size in the
//! picture's header, so the header is read first (`imagesize`, the reader usvg itself uses) and a
//! picture that is too large, or one that would take the page over its total, is dropped: the page
//! is drawn without it. A picture that is not a readable raster image is dropped too.

use std::sync::atomic::{AtomicU64, Ordering};

/// Largest picture decoded, in pixels (4096 x 4096; 64 MiB as RGBA).
pub const MAX_IMAGE_PIXELS: u64 = 4096 * 4096;
/// Largest sum of the pictures decoded for one page, in pixels (four of the largest).
pub const MAX_PAGE_PIXELS: u64 = 4 * MAX_IMAGE_PIXELS;

/// What one drawn page may still decode.
#[derive(Debug)]
pub struct RasterBudget {
    per_image: u64,
    remaining: AtomicU64,
}

impl RasterBudget {
    pub fn new(per_image: u64, page: u64) -> Self {
        Self {
            per_image,
            remaining: AtomicU64::new(page),
        }
    }

    /// The limits of a thumbnail or card ([`MAX_IMAGE_PIXELS`], [`MAX_PAGE_PIXELS`]).
    pub fn for_page() -> Self {
        Self::new(MAX_IMAGE_PIXELS, MAX_PAGE_PIXELS)
    }

    /// Whether the picture in `data` may be decoded; when it may, its pixels are taken from the
    /// page total. The size is the header's; a picture whose header cannot be read is refused.
    pub fn admit(&self, data: &[u8]) -> bool {
        let Ok(size) = imagesize::blob_size(data) else {
            return false;
        };
        let pixels = (size.width as u64).saturating_mul(size.height as u64);
        if pixels == 0 || pixels > self.per_image {
            return false;
        }
        self.remaining
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                left.checked_sub(pixels)
            })
            .is_ok()
    }
}

#[cfg(test)]
#[path = "raster_budget_tests.rs"]
mod tests;
