/// Embedded WebUI asset checks: `(asset text, required marker)`.
pub(super) const WEBUI_ASSET_MARKERS: &[(&str, &str)] = &[
    (include_str!("../ui/index.html"), "EasyOffice"),
    (include_str!("../ui/tokens.css"), "--ui-menu-open"),
    (include_str!("../ui/styles.css"), "var(--ui-text)"),
    (include_str!("../ui/shell.css"), "var(--ui-bg-light)"),
    (include_str!("../ui/content.css"), "--doc-paper"),
    (include_str!("../ui/theme.js"), "__EASYOFFICE_THEME__"),
    (include_str!("../ui/home.css"), ".home-screen"),
    (include_str!("../ui/menu-bar.css"), ".md-sub-panel"),
    (include_str!("../ui/toolbar.css"), "#icon-toolbar"),
    (
        include_str!("../ui/home-screen.js"),
        "__EASYOFFICE_CREATE_HOME_SCREEN__",
    ),
    (
        include_str!("../ui/recent-documents-dialog.js"),
        "__EASYOFFICE_RECENT_DIALOG__",
    ),
    (
        include_str!("../ui/update-notice.js"),
        "__EASYOFFICE_CREATE_UPDATE_NOTICE__",
    ),
    (include_str!("../ui/icons.js"), "__EASYOFFICE_ICONS__"),
    (
        include_str!("../ui/window-bridge.js"),
        "__EASYOFFICE_WINDOW_BRIDGE__",
    ),
    (
        include_str!("../ui/ipc-transport.js"),
        "__EASYOFFICE_CREATE_IPC_TRANSPORT__",
    ),
    (
        include_str!("../ui/menu-keyboard.js"),
        "__EASYOFFICE_CREATE_MENU_KEYBOARD__",
    ),
    (
        include_str!("../ui/menu-kinds.js"),
        "__EASYOFFICE_MENU_KINDS__",
    ),
    (include_str!("../ui/index.html"), "./shell.css"),
    (include_str!("../ui/index.html"), "./tokens.css"),
    (include_str!("../ui/index.html"), "./content.css"),
    (include_str!("../ui/index.html"), "./theme.js"),
    (include_str!("../ui/index.html"), "./home.css"),
    (include_str!("../ui/index.html"), "./menu-bar.css"),
    (include_str!("../ui/index.html"), "./toolbar.css"),
    (include_str!("../ui/index.html"), "./home-screen.js"),
    (
        include_str!("../ui/index.html"),
        "./recent-documents-dialog.js",
    ),
    (include_str!("../ui/index.html"), "./update-notice.js"),
    (include_str!("../ui/index.html"), "./icons.js"),
    (include_str!("../ui/index.html"), "./window-bridge.js"),
    (include_str!("../ui/index.html"), "./ipc-transport.js"),
    (include_str!("../ui/index.html"), "./menu-keyboard.js"),
    (include_str!("../ui/index.html"), "./menu-kinds.js"),
    (
        include_str!("../ui/sheet-utils.js"),
        "__EASYOFFICE_SHEET_UTILS__",
    ),
    (
        include_str!("../ui/pii-dom-scrubber.js"),
        "__EASYOFFICE_CREATE_PII_SCRUBBER__",
    ),
    (
        include_str!("../ui/sheet-store.js"),
        "__EASYOFFICE_CREATE_SHEET_STORE__",
    ),
    (
        include_str!("../ui/ribbon-catalog.js"),
        "__EASYOFFICE_RIBBON_CATALOG__",
    ),
    (
        include_str!("../ui/save-payload.js"),
        "__EASYOFFICE_SAVE_PAYLOAD__",
    ),
    (
        include_str!("../ui/desktop-bridge.js"),
        "__EASYOFFICE_DESKTOP_BRIDGE__",
    ),
    (
        include_str!("../ui/custom-select.js"),
        "__EASYOFFICE_CUSTOM_SELECT__",
    ),
    (
        include_str!("../ui/color-picker.js"),
        "__EASYOFFICE_COLOR_PICKER__",
    ),
    (
        include_str!("../ui/doc-symbol-dialog.js"),
        "__EASYOFFICE_CREATE_DOC_SYMBOL_DIALOG__",
    ),
    (
        include_str!("../ui/doc-dialogs-ext.js"),
        "__EASYOFFICE_CREATE_DOC_DIALOGS_EXT__",
    ),
    (
        include_str!("../ui/workspace-scrub.js"),
        "__EASYOFFICE_CREATE_WORKSPACE_SCRUB__",
    ),
    (
        include_str!("../ui/workspace-tools.js"),
        "__EASYOFFICE_CREATE_WORKSPACE_TOOLS__",
    ),
    (
        include_str!("../ui/workspace-panels.js"),
        "__EASYOFFICE_CREATE_WORKSPACE_PANELS__",
    ),
    (
        include_str!("../ui/sheet-grid.js"),
        "__EASYOFFICE_CREATE_SHEET_GRID__",
    ),
    (
        include_str!("../ui/sheet-clipboard.js"),
        "__EASYOFFICE_CREATE_SHEET_CLIPBOARD__",
    ),
    (
        include_str!("../ui/sheet-input.js"),
        "__EASYOFFICE_CREATE_SHEET_INPUT__",
    ),
    (
        include_str!("../ui/sheet-sort.js"),
        "__EASYOFFICE_CREATE_SHEET_SORT__",
    ),
    (
        include_str!("../ui/sheet-sort-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_SORT_UI__",
    ),
    (
        include_str!("../ui/sheet-ops.js"),
        "__EASYOFFICE_CREATE_SHEET_OPS__",
    ),
    (
        include_str!("../ui/sheet-layout.js"),
        "__EASYOFFICE_SHEET_LAYOUT__",
    ),
    (
        include_str!("../ui/sheet-table-presets.js"),
        "__EASYOFFICE_SHEET_TABLE_PRESETS__",
    ),
    (
        include_str!("../ui/sheet-table-style.js"),
        "__EASYOFFICE_SHEET_TABLE_STYLE__",
    ),
    (
        include_str!("../ui/sheet-table-ops.js"),
        "__EASYOFFICE_SHEET_TABLE_OPS__",
    ),
    (
        include_str!("../ui/sheet-table-gallery.js"),
        "__EASYOFFICE_CREATE_SHEET_TABLE_GALLERY__",
    ),
    (
        include_str!("../ui/sheet-hidden-rows.js"),
        "__EASYOFFICE_CREATE_SHEET_HIDDEN_ROWS_SYNC__",
    ),
    (
        include_str!("../ui/sheet-table-bake.js"),
        "__EASYOFFICE_SHEET_TABLE_BAKE__",
    ),
    (
        include_str!("../ui/sheet-table-totals.js"),
        "__EASYOFFICE_SHEET_TABLE_TOTALS__",
    ),
    (
        include_str!("../ui/sheet-table-style-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_TABLE_STYLE_UI__",
    ),
    (
        include_str!("../ui/sheet-table-dedupe.js"),
        "__EASYOFFICE_CREATE_SHEET_TABLE_DEDUPE_UI__",
    ),
    (
        include_str!("../ui/sheet-slicer.js"),
        "__EASYOFFICE_CREATE_SHEET_SLICER_UI__",
    ),
    (
        include_str!("../ui/sheet-table-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_TABLE_UI__",
    ),
    (
        include_str!("../ui/sheet-filter.js"),
        "__EASYOFFICE_CREATE_SHEET_FILTER__",
    ),
    (
        include_str!("../ui/sheet-filter-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_FILTER_UI__",
    ),
    (
        include_str!("../ui/sheet-outline.js"),
        "__EASYOFFICE_SHEET_OUTLINE__",
    ),
    (
        include_str!("../ui/sheet-freeze.js"),
        "__EASYOFFICE_CREATE_SHEET_FREEZE__",
    ),
    (
        include_str!("../ui/sheet-view-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_VIEW__",
    ),
    (
        include_str!("../ui/sheet-outline-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_OUTLINE_UI__",
    ),
    (
        include_str!("../ui/sheet-tab-names.js"),
        "__EASYOFFICE_SHEET_TAB_NAMES__",
    ),
    (
        include_str!("../ui/sheet-tab-strip.js"),
        "__EASYOFFICE_SHEET_TAB_STRIP__",
    ),
    (
        include_str!("../ui/sheet-tabs.js"),
        "__EASYOFFICE_CREATE_SHEET_TABS__",
    ),
    (
        include_str!("../ui/sheet-tab-ui.js"),
        "__EASYOFFICE_CREATE_SHEET_TAB_UI__",
    ),
    (
        include_str!("../ui/sheet-tab-menu.js"),
        "__EASYOFFICE_CREATE_SHEET_TAB_MENU__",
    ),
    (
        include_str!("../ui/sheet-structure.js"),
        "__EASYOFFICE_CREATE_SHEET_STRUCTURE__",
    ),
    (
        include_str!("../ui/sheet-format.js"),
        "__EASYOFFICE_CREATE_SHEET_FORMAT__",
    ),
    (
        include_str!("../ui/sheet-view-state.js"),
        "__EASYOFFICE_SHEET_VIEW_STATE__",
    ),
    (
        include_str!("../ui/sheet-view-live.js"),
        "__EASYOFFICE_CREATE_SHEET_VIEW_LIVE__",
    ),
    (
        include_str!("../ui/zoom-range.js"),
        "__EASYOFFICE_ZOOM_RANGE__",
    ),
    (
        include_str!("../ui/sheet-controller.js"),
        "__EASYOFFICE_CREATE_SHEET_CONTROLLER__",
    ),
    (
        include_str!("../ui/ribbon-history.js"),
        "__EASYOFFICE_CREATE_RIBBON_HISTORY__",
    ),
    (
        include_str!("../ui/workspace-session.js"),
        "__EASYOFFICE_CREATE_WORKSPACE_SESSION__",
    ),
    (
        include_str!("../ui/file-open.js"),
        "__EASYOFFICE_CREATE_FILE_OPEN__",
    ),
    (
        include_str!("../ui/pdf-edit.js"),
        "__EASYOFFICE_CREATE_PDF_EDIT__",
    ),
    (
        include_str!("../ui/slide-studio.js"),
        "__EASYOFFICE_CREATE_SLIDE_STUDIO__",
    ),
    (
        include_str!("../ui/doc-open-type.js"),
        "__EASYOFFICE_DOC_OPEN_TYPE__",
    ),
    (include_str!("../ui/doc-runs.js"), "__EASYOFFICE_DOC_RUNS__"),
    (
        include_str!("../ui/doc-runs-ui.js"),
        "__EASYOFFICE_CREATE_DOC_RUNS_UI__",
    ),
    (
        include_str!("../ui/hwp-host.js"),
        "__EASYOFFICE_CREATE_HWP_HOST__",
    ),
    (include_str!("../ui/index.html"), "id=\"pane-hwp\""),
    (
        include_str!("../ui/hwp-session.js"),
        "__EASYOFFICE_CREATE_HWP_SESSION__",
    ),
    (
        include_str!("../ui/hwp-print.js"),
        "__EASYOFFICE_CREATE_HWP_PRINT__",
    ),
    (include_str!("../ui/index.html"), "./hwp-host.js"),
    (include_str!("../ui/index.html"), "./hwp-print.js"),
    (include_str!("../ui/index.html"), "./hwp-session.js"),
    (include_str!("../ui/doc-ime.js"), "compositionstart"),
    (include_str!("../ui/app.js"), "CREATE_DOC_IME"),
    (
        include_str!("../ui/command-history.js"),
        "CREATE_COMMAND_HISTORY",
    ),
    (include_str!("../ui/doc-paste.js"), "CREATE_DOC_PASTE"),
];
