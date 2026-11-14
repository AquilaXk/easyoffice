//! The scripts of the sheet tables (PD-230), served by name: the built-in style set, the style layer of the grid, the plans
//! of the table commands, the style gallery and the commands' own module.

const PRESETS: &str = include_str!("../ui/sheet-table-presets.js");
const STYLE: &str = include_str!("../ui/sheet-table-style.js");
const OPS: &str = include_str!("../ui/sheet-table-ops.js");
const GALLERY: &str = include_str!("../ui/sheet-table-gallery.js");
const UI: &str = include_str!("../ui/sheet-table-ui.js");

/// The text of the script a request for `route` (`/sheet-table-ops.js`) names.
pub(crate) fn script(route: &str) -> Option<&'static str> {
    match route {
        "/sheet-table-presets.js" => Some(PRESETS),
        "/sheet-table-style.js" => Some(STYLE),
        "/sheet-table-ops.js" => Some(OPS),
        "/sheet-table-gallery.js" => Some(GALLERY),
        "/sheet-table-ui.js" => Some(UI),
        _ => None,
    }
}
