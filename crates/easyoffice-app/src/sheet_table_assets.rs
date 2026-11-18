//! The scripts of the sheet tables (PD-231), served by name: the built-in style set, the style layer of the grid, the plans
//! of the table commands, the style gallery, the totals row's function list, the table look kept by 범위로 변환, the session's hidden rows (PD-232) and the commands' own module.

const PRESETS: &str = include_str!("../ui/sheet-table-presets.js");
const STYLE: &str = include_str!("../ui/sheet-table-style.js");
const OPS: &str = include_str!("../ui/sheet-table-ops.js");
const GALLERY: &str = include_str!("../ui/sheet-table-gallery.js");
const HIDDEN_ROWS: &str = include_str!("../ui/sheet-hidden-rows.js");
const BAKE: &str = include_str!("../ui/sheet-table-bake.js");
const TOTALS: &str = include_str!("../ui/sheet-table-totals.js");
const STYLE_UI: &str = include_str!("../ui/sheet-table-style-ui.js");
const DEDUPE_UI: &str = include_str!("../ui/sheet-table-dedupe.js");
const SLICER_UI: &str = include_str!("../ui/sheet-slicer.js");
const UI: &str = include_str!("../ui/sheet-table-ui.js");

/// The text of the script a request for `route` (`/sheet-table-ops.js`) names.
pub(crate) fn script(route: &str) -> Option<&'static str> {
    match route {
        "/sheet-table-presets.js" => Some(PRESETS),
        "/sheet-table-style.js" => Some(STYLE),
        "/sheet-table-ops.js" => Some(OPS),
        "/sheet-table-gallery.js" => Some(GALLERY),
        "/sheet-hidden-rows.js" => Some(HIDDEN_ROWS),
        "/sheet-table-bake.js" => Some(BAKE),
        "/sheet-table-totals.js" => Some(TOTALS),
        "/sheet-table-style-ui.js" => Some(STYLE_UI),
        "/sheet-table-dedupe.js" => Some(DEDUPE_UI),
        "/sheet-slicer.js" => Some(SLICER_UI),
        "/sheet-table-ui.js" => Some(UI),
        _ => None,
    }
}
