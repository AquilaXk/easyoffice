//! Pixel limits for the pictures inside a page SVG (PD-145). The page SVG of a file Finder hands to
//! the extension is untrusted, and the decoders allocate by the size in a picture's header, so the
//! header is read first and a picture over the limits is left out (the same limits as the
//! thumbnails of the desktop shell, whose module is included here: one source of truth).

use easyoffice_formats::checked_svg_text;
use std::borrow::Cow;
use std::sync::Arc;

#[path = "../../../../crates/easyoffice-desktop/src/raster_budget.rs"]
#[allow(dead_code)]
mod budget;

pub use budget::{is_raster_mime, RasterBudget};

/// Largest SVG picture inside a page once inflated (bytes).
const MAX_NESTED_SVG_BYTES: usize = 16 * 1_048_576;

/// usvg's image resolver, which reads data URLs only, with every raster picture checked against
/// `budget` first. A path or `file:` href is never read. An SVG picture goes through
/// [`checked_svg_text`] first (usvg expands entity declarations and inflates gzip without a bound);
/// one it refuses is left out.
pub fn bounded_image_resolver(budget: RasterBudget) -> usvg::ImageHrefResolver<'static> {
    let budget = Arc::new(budget);
    let default_data = usvg::ImageHrefResolver::default_data_resolver();
    usvg::ImageHrefResolver {
        resolve_data: Box::new(move |mime, data, options| {
            if mime.eq_ignore_ascii_case("image/svg+xml") {
                let plain = match checked_svg_text(&data, MAX_NESTED_SVG_BYTES).ok()? {
                    Cow::Borrowed(_) => data,
                    Cow::Owned(text) => Arc::new(text.into_bytes()),
                };
                return default_data("image/svg+xml", plain, options);
            }
            // Only a raster type goes on: usvg's resolver reads `text/plain` (and `data:,`) as a
            // picture of any format, an SVG with entity declarations included.
            if !is_raster_mime(mime) || !budget.admit(&data) {
                return None;
            }
            default_data(&mime.to_ascii_lowercase(), data, options)
        }),
        resolve_string: Box::new(|_, _| None),
    }
}
