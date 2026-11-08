//! The first page of a Hangul document as a PNG for a start-window card (PD-142).
//!
//! Used when the file carries no preview picture. The page comes from rhwp's own SVG renderer (the
//! engine the editor and the Quick Look extensions use), and the same `usvg`/`resvg` path that draws
//! the preview picture of a saved file (PD-132) rasterizes it. The caller runs this on a bounded
//! worker (`bounded.rs`); this module adds the input limit, `mod.rs` checks the picture it returns.

use super::bounded::RenderError;
use crate::save_thumbnail::render_png_sized;
use rhwp::DocumentCore;
use std::sync::Arc;
use usvg::fontdb::Database;

/// Largest file rendered. Same limit as the Quick Look extensions, which read files of the same kind.
pub const MAX_RENDER_INPUT_BYTES: usize = 50 * 1024 * 1024;
/// Longer side of the card picture in pixels: the card's picture area is about 300 CSS pixels wide,
/// which this covers on a 1.5x screen and nearly on a 2x one.
pub const CARD_SIDE_PX: u32 = 480;

/// The first page of `document` as a PNG whose longer side is [`CARD_SIDE_PX`].
pub fn first_page_png(document: &[u8], fonts: &Arc<Database>) -> Result<Vec<u8>, RenderError> {
    if document.is_empty() || document.len() > MAX_RENDER_INPUT_BYTES {
        return Err(RenderError::TooLarge);
    }
    let core = DocumentCore::from_bytes(document).map_err(|_| RenderError::Unreadable)?;
    if core.page_count() == 0 {
        return Err(RenderError::Empty);
    }
    let svg = core
        .render_page_svg_native(0)
        .map_err(|_| RenderError::Unreadable)?;
    render_png_sized(&svg, fonts, CARD_SIDE_PX).map_err(|_| RenderError::Unreadable)
}
