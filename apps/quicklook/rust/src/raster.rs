//! Pixel limits for the pictures inside a page SVG (PD-145). The page SVG of a file Finder hands to
//! the extension is untrusted, and the decoders allocate by the size in a picture's header, so the
//! header is read first and a picture over the limits is left out (the same limits as the
//! thumbnails of the desktop shell, whose module is included here: one source of truth).

use easyoffice_formats::checked_svg_text;
use std::sync::Arc;

#[path = "../../../../crates/easyoffice-desktop/src/raster_budget.rs"]
#[allow(dead_code)]
mod budget;

#[path = "../../../../crates/easyoffice-desktop/src/svg_budget.rs"]
#[allow(dead_code)]
pub mod svg_budget;

pub use budget::{is_raster_mime, RasterBudget};

/// usvg's image resolver, which reads data URLs only, with every raster picture checked against
/// `budget` first. A path or `file:` href is never read. An SVG picture goes through
/// [`checked_svg_text`] first (usvg expands entity declarations and inflates gzip without a bound);
/// one it refuses is left out.
pub fn bounded_image_resolver(budget: RasterBudget) -> usvg::ImageHrefResolver<'static> {
    let budget = Arc::new(budget);
    let svg_budget = Arc::new(svg_budget::NestedSvgBudget::new());
    let default_data = usvg::ImageHrefResolver::default_data_resolver();
    usvg::ImageHrefResolver {
        resolve_data: Box::new(move |mime, data, options| {
            if mime.eq_ignore_ascii_case("image/svg+xml") {
                let guard = svg_budget.enter(data.len())?;
                let text = checked_svg_text(&data, svg_budget::MAX_NESTED_SVG_BYTES).ok()?;
                guard.admit_bytes(text.len())?;
                svg_budget::check_svg_structure(&text).ok()?;
                return usvg::Tree::from_str(&text, options)
                    .ok()
                    .map(usvg::ImageKind::SVG);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_svg_depth_is_bounded_by_resolver() {
        let resolver = bounded_image_resolver(RasterBudget::for_page());
        let opt = usvg::Options {
            image_href_resolver: resolver,
            ..usvg::Options::default()
        };

        // Recursive nested SVG data URLs
        let make_nested = |depth: usize| {
            let mut s = String::from(
                r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1"/></svg>"#,
            );
            for _ in 0..depth {
                let encoded = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    s.as_bytes(),
                );
                s = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="data:image/svg+xml;base64,{}" width="1" height="1"/></svg>"#,
                    encoded
                );
            }
            s
        };

        // Depth 3 succeeds
        let shallow = make_nested(2);
        assert!(usvg::Tree::from_str(&shallow, &opt).is_ok());

        // Depth > 4 is dropped safely
        let deep = make_nested(6);
        let tree = usvg::Tree::from_str(&deep, &opt);
        assert!(
            tree.is_ok(),
            "Tree parses safely even with deeply nested dropped images"
        );
    }
}
