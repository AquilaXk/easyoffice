//! Picture model of [`crate::RichDocument`]: validated PNG/JPEG bytes, pixel size, display size
//! in EMU and alternative text. Construction is the only way to obtain a [`RichImage`], so every
//! value in a document passed the same format, size and pixel-count checks (fail closed).

use std::fmt;
use std::sync::Arc;

use crate::rich_document::{RichBlock, RichDocument};
use crate::rich_image_probe::{probe_jpeg, probe_png, sniff_format};
use crate::{EasyOfficeError, RichImageFormat};

/// Largest accepted single picture (encoded bytes).
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// Largest number of pictures in one document.
pub const MAX_IMAGES_PER_DOCUMENT: usize = 256;
/// Largest total of encoded picture bytes in one document.
pub const MAX_TOTAL_IMAGE_BYTES: usize = 128 * 1024 * 1024;
/// Largest pixel count of one picture (decompression-bomb guard for viewers and converters).
pub const MAX_IMAGE_PIXELS: u64 = 50_000_000;
/// Largest alternative text, in bytes.
pub const MAX_ALT_TEXT_BYTES: usize = 4_096;
/// Text used where a picture cannot be embedded and has no alternative text.
pub const IMAGE_PLACEHOLDER_TEXT: &str = "[그림]";

/// EMU per pixel at 96 dpi.
const EMU_PER_PIXEL: u64 = 9_525;
/// EMU per typographic point.
pub(crate) const EMU_PER_POINT: u64 = 12_700;
/// Widest displayed picture: A4 text width with 25 mm margins.
const MAX_DISPLAY_WIDTH_EMU: u64 = 5_731_510;
/// Tallest displayed picture: A4 text height with 25 mm margins.
const MAX_DISPLAY_HEIGHT_EMU: u64 = 8_640_000;
/// Largest display side a caller may request before fitting (200 inches).
const MAX_REQUESTED_SIDE_EMU: u64 = 182_880_000;

/// A validated PNG or JPEG picture placed as its own block.
#[derive(Clone, PartialEq, Eq)]
pub struct RichImage {
    format: RichImageFormat,
    /// Shared so that a picture referenced again (a clone) does not copy its bytes.
    bytes: Arc<Vec<u8>>,
    width_px: u32,
    height_px: u32,
    width_emu: u64,
    height_emu: u64,
    alt: String,
}

impl fmt::Debug for RichImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RichImage")
            .field("format", &self.format)
            .field("bytes", &self.bytes.len())
            .field("size_px", &(self.width_px, self.height_px))
            .field("alt", &self.alt)
            .finish()
    }
}

/// Scales `(w, h)` down, keeping the aspect ratio, until it fits `(max_w, max_h)`; sides stay >= 1.
pub(crate) fn fit_within(w: u64, h: u64, max_w: u64, max_h: u64) -> (u64, u64) {
    let (w, h) = (u128::from(w.max(1)), u128::from(h.max(1)));
    let (max_w, max_h) = (u128::from(max_w), u128::from(max_h));
    if w <= max_w && h <= max_h {
        return (w as u64, h as u64);
    }
    let (nw, nh) = if max_w * h <= max_h * w {
        (max_w, h * max_w / w)
    } else {
        (w * max_h / h, max_h)
    };
    (nw.max(1) as u64, nh.max(1) as u64)
}

impl RichImage {
    /// Validates `bytes` (PNG or JPEG by magic bytes) and builds a picture shown at 96 dpi,
    /// scaled down to fit the page. `alt` is truncated at a character boundary to
    /// [`MAX_ALT_TEXT_BYTES`].
    pub fn new(bytes: Vec<u8>, alt: impl Into<String>) -> Result<Self, EasyOfficeError> {
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "그림 크기({}바이트)가 한도({MAX_IMAGE_BYTES}바이트)를 넘었습니다.",
                bytes.len()
            )));
        }
        let format = sniff_format(&bytes).ok_or_else(|| {
            EasyOfficeError::InvalidArgument("PNG 또는 JPEG 그림만 사용할 수 있습니다.".to_string())
        })?;
        let (width_px, height_px) = match format {
            RichImageFormat::Png => {
                let h = probe_png(&bytes)?;
                (h.width, h.height)
            }
            RichImageFormat::Jpeg => {
                let h = probe_jpeg(&bytes)?;
                (h.width, h.height)
            }
        };
        if width_px == 0 || height_px == 0 {
            return Err(EasyOfficeError::CorruptedContainer(
                "그림 크기가 0입니다.".to_string(),
            ));
        }
        if u64::from(width_px) * u64::from(height_px) > MAX_IMAGE_PIXELS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "그림 화소 수가 한도({MAX_IMAGE_PIXELS})를 넘었습니다."
            )));
        }
        let (width_emu, height_emu) = fit_within(
            u64::from(width_px) * EMU_PER_PIXEL,
            u64::from(height_px) * EMU_PER_PIXEL,
            MAX_DISPLAY_WIDTH_EMU,
            MAX_DISPLAY_HEIGHT_EMU,
        );
        Ok(Self {
            format,
            bytes: Arc::new(bytes),
            width_px,
            height_px,
            width_emu,
            height_emu,
            alt: truncate_alt(alt.into()),
        })
    }

    /// The same picture (bytes shared, already validated) with other alternative text.
    pub(crate) fn with_alt(mut self, alt: &str) -> Self {
        self.alt = truncate_alt(alt.to_string());
        self
    }

    /// Replaces the display size (EMU). The size is rejected when a side is zero or absurdly
    /// large, and is otherwise scaled down to fit the page with its aspect ratio kept.
    pub fn set_display_emu(&mut self, width: u64, height: u64) -> Result<(), EasyOfficeError> {
        let valid = 1..=MAX_REQUESTED_SIDE_EMU;
        if !valid.contains(&width) || !valid.contains(&height) {
            return Err(EasyOfficeError::InvalidArgument(
                "그림 표시 크기가 허용 범위를 벗어났습니다.".to_string(),
            ));
        }
        (self.width_emu, self.height_emu) =
            fit_within(width, height, MAX_DISPLAY_WIDTH_EMU, MAX_DISPLAY_HEIGHT_EMU);
        Ok(())
    }

    pub fn format(&self) -> RichImageFormat {
        self.format
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    /// Display size in EMU (914,400 per inch).
    pub fn display_emu(&self) -> (u64, u64) {
        (self.width_emu, self.height_emu)
    }

    pub fn alt(&self) -> &str {
        &self.alt
    }

    /// Text that stands in for the picture in formats without picture support.
    pub fn fallback_text(&self) -> &str {
        if self.alt.trim().is_empty() {
            IMAGE_PLACEHOLDER_TEXT
        } else {
            &self.alt
        }
    }
}

/// Replaces control characters with spaces and caps the text at [`MAX_ALT_TEXT_BYTES`].
pub(crate) fn truncate_alt(mut alt: String) -> String {
    alt = alt
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if alt.len() > MAX_ALT_TEXT_BYTES {
        let mut cut = MAX_ALT_TEXT_BYTES;
        while !alt.is_char_boundary(cut) {
            cut -= 1;
        }
        alt.truncate(cut);
    }
    alt
}

/// Enforces the per-document picture count and total byte limits (fail closed).
pub(crate) fn check_document_images(doc: &RichDocument) -> Result<(), EasyOfficeError> {
    let mut count = 0usize;
    let mut total = 0usize;
    for block in &doc.blocks {
        if let RichBlock::Image(img) = block {
            count += 1;
            total = total.saturating_add(img.bytes.len());
        }
    }
    if count > MAX_IMAGES_PER_DOCUMENT || total > MAX_TOTAL_IMAGE_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "문서의 그림이 한도(개수 {MAX_IMAGES_PER_DOCUMENT}, 합계 {MAX_TOTAL_IMAGE_BYTES}바이트)를 넘었습니다."
        )));
    }
    Ok(())
}

/// Running picture count and byte total of a document being read (PD-096 limits).
#[derive(Default)]
pub(crate) struct ImageBudget {
    count: usize,
    bytes: usize,
}

impl ImageBudget {
    /// Counts one picture of `len` bytes; fails closed when a document limit is exceeded.
    pub(crate) fn charge(&mut self, len: usize) -> Result<(), EasyOfficeError> {
        self.count += 1;
        self.bytes = self.bytes.saturating_add(len);
        if self.count > MAX_IMAGES_PER_DOCUMENT || self.bytes > MAX_TOTAL_IMAGE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "문서의 그림이 한도(개수·합계 크기)를 넘었습니다.".to_string(),
            ));
        }
        Ok(())
    }
}

/// A package-relative part path from a document reference, or `None` when it is absolute, has a
/// scheme (external link), a backslash or any `.`/`..` segment.
pub(crate) fn safe_package_path(href: &str) -> Option<String> {
    let path = href.trim().strip_prefix("./").unwrap_or(href.trim());
    let bad = path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path
            .split('/')
            .any(|seg| seg.is_empty() || seg == "." || seg == "..");
    (!bad).then(|| path.to_string())
}
