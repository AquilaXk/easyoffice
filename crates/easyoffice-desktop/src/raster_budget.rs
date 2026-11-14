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

/// Whether a data-URL media type names a raster picture the decoders read. It is the only kind a
/// resolver hands to usvg's default one: that resolver reads `text/plain` (and so `data:,`) as a
/// picture of any format, an SVG included, parsed with entity declarations allowed.
pub fn is_raster_mime(mime: &str) -> bool {
    [
        "image/png",
        "image/jpeg",
        "image/jpg",
        "image/gif",
        "image/webp",
    ]
    .iter()
    .any(|known| mime.eq_ignore_ascii_case(known))
}

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
        let mut pixels = (size.width as u64).saturating_mul(size.height as u64);
        if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            let Ok(Some(frame_pixels)) = inspect_gif_frames(data) else {
                return false;
            };
            if frame_pixels > self.per_image {
                return false;
            }
            pixels = pixels.max(frame_pixels);
        }
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

/// Inspects GIF frames following Global Color Table and Extension blocks to find the maximum
/// frame dimension, and rejects malformed blocks before decoders allocate.
fn inspect_gif_frames(data: &[u8]) -> Result<Option<u64>, ()> {
    if !data.starts_with(b"GIF87a") && !data.starts_with(b"GIF89a") {
        return Ok(None);
    }
    if data.len() < 13 {
        return Err(());
    }
    let packed = data[10];
    let has_gct = (packed & 0x80) != 0;
    let gct_size = if has_gct {
        3usize.checked_mul(1 << ((packed & 0x07) + 1)).ok_or(())?
    } else {
        0
    };
    let mut offset = 13usize.checked_add(gct_size).ok_or(())?;
    if offset > data.len() {
        return Err(());
    }

    let mut max_pixels = 0u64;
    let mut loop_count = 0usize;

    while offset < data.len() {
        loop_count += 1;
        if loop_count > 4096 {
            return Err(());
        }
        let block_type = data[offset];
        if block_type == 0x3B {
            // GIF Trailer
            break;
        }
        if block_type == 0x21 {
            // Extension block: 0x21 + label + subblocks
            offset = offset.checked_add(2).ok_or(())?;
            while offset < data.len() {
                let sub_len = data[offset] as usize;
                offset = offset.checked_add(1).ok_or(())?;
                if sub_len == 0 {
                    break;
                }
                offset = offset.checked_add(sub_len).ok_or(())?;
                if offset > data.len() {
                    return Err(());
                }
            }
        } else if block_type == 0x2C {
            // Image Descriptor: 10 bytes header
            if offset + 10 > data.len() {
                return Err(());
            }
            let w = u16::from_le_bytes([data[offset + 5], data[offset + 6]]) as u64;
            let h = u16::from_le_bytes([data[offset + 7], data[offset + 8]]) as u64;
            let frame_pixels = w.saturating_mul(h);
            max_pixels = max_pixels.max(frame_pixels);

            let img_packed = data[offset + 9];
            let has_lct = (img_packed & 0x80) != 0;
            let lct_size = if has_lct {
                3usize
                    .checked_mul(1 << ((img_packed & 0x07) + 1))
                    .ok_or(())?
            } else {
                0
            };
            offset = offset.checked_add(10 + lct_size).ok_or(())?;
            if offset >= data.len() {
                return Err(());
            }
            // Skip LZW minimum code size
            offset = offset.checked_add(1).ok_or(())?;
            // Skip image data sub-blocks
            while offset < data.len() {
                let sub_len = data[offset] as usize;
                offset = offset.checked_add(1).ok_or(())?;
                if sub_len == 0 {
                    break;
                }
                offset = offset.checked_add(sub_len).ok_or(())?;
                if offset > data.len() {
                    return Err(());
                }
            }
        } else {
            // Unknown/malformed block
            return Err(());
        }
    }

    Ok(Some(max_pixels))
}

#[cfg(test)]
#[path = "raster_budget_tests.rs"]
mod tests;
