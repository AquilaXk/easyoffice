//! Pixel limits for the pictures inside a page SVG (PD-145). The page SVG of a file Finder hands to
//! the extension is untrusted, and the decoders allocate by the size in a picture's header, so the
//! header is read first and a picture over the limits is left out (the same limits as the
//! thumbnails of the desktop shell, whose module is included here: one source of truth).

use std::sync::Arc;

#[path = "../../../../crates/easyoffice-desktop/src/raster_budget.rs"]
#[allow(dead_code)]
mod budget;

pub use budget::RasterBudget;

/// usvg's image resolver, which reads data URLs only, with every raster picture checked against
/// `budget` first. A path or `file:` href is never read.
pub fn bounded_image_resolver(budget: RasterBudget) -> usvg::ImageHrefResolver<'static> {
    let budget = Arc::new(budget);
    let default_data = usvg::ImageHrefResolver::default_data_resolver();
    usvg::ImageHrefResolver {
        resolve_data: Box::new(move |mime, data, options| {
            if mime != "image/svg+xml" && !budget.admit(&data) {
                return None;
            }
            default_data(mime, data, options)
        }),
        resolve_string: Box::new(|_, _| None),
    }
}
