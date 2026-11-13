//! The page scripts of the sheet's view and tabs that the local server answers by path.

/// A view script by its file name: its request path and its text.
macro_rules! view_script {
    ($file:literal) => {
        (concat!("/", $file), include_str!(concat!("../ui/", $file)))
    };
}

/// The scripts of the sheet's view (PD-208) and tabs (PD-209) by request path.
pub(crate) const SHEET_VIEW_SCRIPTS: [(&str, &str); 10] = [
    ("/sheet-outline.js", include_str!("../ui/sheet-outline.js")),
    ("/sheet-freeze.js", include_str!("../ui/sheet-freeze.js")),
    ("/sheet-view-ui.js", include_str!("../ui/sheet-view-ui.js")),
    (
        "/sheet-outline-ui.js",
        include_str!("../ui/sheet-outline-ui.js"),
    ),
    (
        "/sheet-tab-color.js",
        include_str!("../ui/sheet-tab-color.js"),
    ),
    view_script!("sheet-tab-names.js"),
    view_script!("sheet-tab-strip.js"),
    view_script!("sheet-tabs.js"),
    view_script!("sheet-tab-ui.js"),
    view_script!("sheet-tab-menu.js"),
];
