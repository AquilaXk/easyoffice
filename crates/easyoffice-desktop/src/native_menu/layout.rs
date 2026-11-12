//! Which menus the native menu bar has for each kind of window (PD-190).
//!
//! The page draws one menu row that follows the window's kind (`menu-kinds.js`); the native bar
//! follows it too: a document, spreadsheet, presentation or PDF window shows only the menus and
//! items that mean something there, and the Hangul editor window keeps the full bar it always had
//! (the editor's own commands are in it). Every line is an id of the item table
//! ([`super::ITEMS`]), a separator or a token of a system item or submenu the builder knows.
//! The menu bar is data only: `build.rs` turns it into Tauri menus, and the tests read it as it is.

use super::QUIT_ID;

/// The kind of a window as far as its menu bar goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuKind {
    Doc,
    Hwp,
    Sheet,
    Slide,
    Pdf,
}

impl MenuKind {
    pub const ALL: [MenuKind; 5] = [
        MenuKind::Doc,
        MenuKind::Hwp,
        MenuKind::Sheet,
        MenuKind::Slide,
        MenuKind::Pdf,
    ];

    /// The name the page reports (`menuKindOf` in `menu-kinds.js`).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "DOC" => Some(Self::Doc),
            "HWP" => Some(Self::Hwp),
            "SHEET" => Some(Self::Sheet),
            "SLIDE" => Some(Self::Slide),
            "PDF" => Some(Self::Pdf),
            _ => None,
        }
    }
}

/// One menu of the bar: its title and its lines.
pub struct MenuSpec {
    pub title: &'static str,
    pub lines: &'static [&'static str],
}

const fn menu(title: &'static str, lines: &'static [&'static str]) -> MenuSpec {
    MenuSpec { title, lines }
}

pub const SEPARATOR: &str = "-";
/// The 새 문서 submenu (the six `file:new-*` items).
pub const NEW_DOCS: &str = "@new-docs";
/// The 테마 submenu (the three theme check items).
pub const THEME: &str = "@theme";
/// The 표시 형식 submenu of the spreadsheet's 서식 menu ([`NUMFMT_LINES`]).
pub const NUMFMT: &str = "@numfmt";
pub const PASTE: &str = "@paste";
pub const SELECT_ALL: &str = "@select-all";
pub const SERVICES: &str = "@services";
pub const HIDE: &str = "@hide";
pub const HIDE_OTHERS: &str = "@hide-others";
pub const SHOW_ALL: &str = "@show-all";
pub const MINIMIZE: &str = "@minimize";
pub const CLOSE_WINDOW: &str = "@close-window";

/// The items of the 새 문서 submenu.
pub const NEW_DOC_IDS: [&str; 6] = [
    "file:new-doc",
    "file:new-hwp",
    "file:new-sheet",
    "file:new-slide",
    "file:new-pdf",
    "file:new-conv",
];
/// The lines of the 표시 형식 submenu.
pub const NUMFMT_LINES: [&str; 11] = [
    "format:numfmt-general",
    "format:numfmt-number",
    "format:numfmt-currency",
    "format:numfmt-accounting",
    "format:numfmt-percent",
    "format:numfmt-date",
    "format:numfmt-time",
    "format:numfmt-text",
    SEPARATOR,
    "format:numfmt-decimal-up",
    "format:numfmt-decimal-down",
];
/// The items of the 테마 submenu.
pub const THEME_IDS: [&str; 3] = ["view:theme-system", "view:theme-light", "view:theme-dark"];

const S: &str = SEPARATOR;

const APP: MenuSpec = menu(
    "EasyOffice",
    &[
        "app:about",
        S,
        SERVICES,
        S,
        HIDE,
        HIDE_OTHERS,
        SHOW_ALL,
        S,
        QUIT_ID,
    ],
);
const WINDOW: MenuSpec = menu("윈도우", &[MINIMIZE, CLOSE_WINDOW]);
/// 도움말: the About item lives in the application menu.
const HELP: MenuSpec = menu("도움말", &["help:update-check"]);

const FILE_HWP: MenuSpec = menu(
    "파일",
    &[
        "file:new",
        NEW_DOCS,
        S,
        "file:open",
        "file:open-recent",
        "file:save",
        "file:save-as",
        S,
        "file:export-pdf",
        "file:print",
    ],
);
const FILE_DOC: MenuSpec = menu(
    "파일",
    &[
        "file:new",
        NEW_DOCS,
        S,
        "file:open",
        "file:open-recent",
        "file:save",
        "file:save-as",
        S,
        "file:print",
    ],
);
/// 편집 용지 is a 파일 item in the spreadsheet and presentation windows (they have no 쪽 menu).
const FILE_PAGE_SETUP: MenuSpec = menu(
    "파일",
    &[
        "file:new",
        NEW_DOCS,
        S,
        "file:open",
        "file:open-recent",
        "file:save",
        "file:save-as",
        S,
        "page:paper",
        "file:print",
    ],
);
const FILE_PDF: MenuSpec = menu(
    "파일",
    &[
        "file:new",
        NEW_DOCS,
        S,
        "file:open",
        "file:open-recent",
        "file:save",
        "file:save-as",
    ],
);

/// The standard text-editing group: it stays in every window's bar because a bound key never
/// reaches a text field (the window then needs the menu's cut, copy and paste).
const EDIT_FULL: MenuSpec = menu(
    "편집",
    &[
        "edit:undo",
        "edit:redo",
        S,
        "edit:cut",
        "edit:copy",
        PASTE,
        SELECT_ALL,
        S,
        "edit:find",
        "edit:replace",
    ],
);
const EDIT_NO_FIND: MenuSpec = menu(
    "편집",
    &[
        "edit:undo",
        "edit:redo",
        S,
        "edit:cut",
        "edit:copy",
        PASTE,
        SELECT_ALL,
    ],
);

const VIEW_HWP: MenuSpec = menu(
    "보기",
    &[
        "view:zoom-in",
        "view:zoom-out",
        "view:zoom-100",
        S,
        "view:zoom-fit-page",
        "view:zoom-fit-width",
        "view:zoom-dialog",
        S,
        "view:split",
        "view:outline",
        S,
        THEME,
        "view:theme-toggle",
        S,
        "view:toolbar-labels",
        "view:form-mode",
        "view:ctrl-marks",
        "view:para-marks",
        "view:doc-grid",
    ],
);
const VIEW_DOC: MenuSpec = menu(
    "보기",
    &[
        "view:zoom-in",
        "view:zoom-out",
        "view:zoom-100",
        S,
        "view:zoom-fit-page",
        "view:zoom-fit-width",
        S,
        "view:split",
        "view:outline",
        S,
        THEME,
        "view:theme-toggle",
        S,
        "view:ctrl-marks",
        "view:para-marks",
        "view:doc-grid",
    ],
);
const VIEW_PLAIN: MenuSpec = menu(
    "보기",
    &[
        "view:zoom-in",
        "view:zoom-out",
        "view:zoom-100",
        S,
        "view:zoom-fit-page",
        "view:zoom-fit-width",
        S,
        "view:split",
        "view:outline",
        S,
        THEME,
        "view:theme-toggle",
    ],
);

const INSERT_HWP: MenuSpec = menu(
    "입력",
    &[
        "insert:shape",
        "insert:chart",
        "insert:footnote",
        "insert:symbol",
        "insert:link",
        "insert:toc",
        "insert:mailmerge",
        "insert:field",
    ],
);
const INSERT_DOC: MenuSpec = menu(
    "입력",
    &[
        "insert:shape",
        "insert:chart",
        S,
        "insert:footnote",
        "insert:symbol",
        "insert:link",
        S,
        "insert:toc",
        "insert:toc-update",
        "insert:mailmerge",
    ],
);
const INSERT_SHEET: MenuSpec = menu(
    "삽입",
    &[
        "table:insert",
        "insert:shape",
        "insert:chart",
        "table:sheet-chart",
        "insert:symbol",
        "insert:link",
        S,
        "table:fn-browser",
        "table:autosum",
    ],
);
const INSERT_SLIDE: MenuSpec = menu(
    "삽입",
    &[
        "table:insert",
        "insert:shape",
        "insert:chart",
        "insert:symbol",
        "insert:link",
    ],
);
const ANNOTATE_PDF: MenuSpec = menu("주석", &["tool:pdf-highlight", "tool:review-comment"]);

const FORMAT_TEXT: MenuSpec = menu(
    "서식",
    &[
        "format:bold",
        "format:italic",
        "format:underline",
        S,
        "format:style",
        "format:justify",
        "format:para-numbering",
        "format:para-bullet",
        "format:indent-in",
        "format:indent-out",
    ],
);
const FORMAT_CELL: MenuSpec = menu(
    "서식",
    &[
        "format:bold",
        "format:italic",
        "format:underline",
        S,
        "format:style",
    ],
);

const FORMAT_SHEET: MenuSpec = menu(
    "서식",
    &[
        "format:bold",
        "format:italic",
        "format:underline",
        S,
        "format:style",
        S,
        NUMFMT,
        "format:cell-format",
    ],
);

const PAGE_HWP: MenuSpec = menu(
    "쪽",
    &[
        "page:paper",
        "page:orientation",
        "page:margins",
        "page:header",
        "page:columns",
        S,
        "page:hwp-header",
        "page:manuscript",
        S,
        "page:slide-master",
        "page:slide-snap",
        "page:slide-transition",
        "page:slide-presenter",
    ],
);
const PAGE_DOC: MenuSpec = menu(
    "쪽",
    &[
        "page:paper",
        "page:orientation",
        "page:margins",
        S,
        "page:header",
        "page:columns",
        "page:break",
    ],
);
const PAGE_PDF: MenuSpec = menu("쪽", &["tool:pdf-rotate", "tool:pdf-splitmerge"]);
const SHEET_SHEET: MenuSpec = menu(
    "시트",
    &[
        "table:sheet-freeze",
        S,
        "sheet:row-insert-above",
        "sheet:row-insert-below",
        "sheet:col-insert-left",
        "sheet:col-insert-right",
        "sheet:row-delete",
        "sheet:col-delete",
        S,
        "sheet:hide",
        "sheet:unhide",
        S,
        "sheet:merge",
        "sheet:unmerge",
        S,
        "page:orientation",
        "page:margins",
    ],
);
const SLIDE_SLIDE: MenuSpec = menu(
    "슬라이드",
    &[
        "page:slide-master",
        "page:slide-snap",
        S,
        "page:orientation",
        "page:margins",
    ],
);
const SLIDE_SHOW: MenuSpec = menu(
    "슬라이드 쇼",
    &["page:slide-transition", "page:slide-presenter"],
);

const TABLE_HWP: MenuSpec = menu(
    "표",
    &[
        "table:insert",
        "table:row-above",
        "table:add-row",
        "table:col-left",
        "table:col-right",
        "table:row-delete",
        "table:col-delete",
        S,
        "table:cell-select",
        "table:merge",
        "table:cell-split",
        S,
        "table:sum",
        "table:doc-formula",
        "table:hwp-border",
        S,
        "table:autosum",
        "table:fn-browser",
        "table:sort",
        "table:validation",
        "table:sheet-pivot",
        "table:sheet-chart",
        "table:sheet-condfmt",
        "table:sheet-freeze",
    ],
);
const TABLE_DOC: MenuSpec = menu(
    "표",
    &[
        "table:insert",
        "table:row-above",
        "table:add-row",
        "table:col-left",
        "table:col-right",
        "table:row-delete",
        "table:col-delete",
        S,
        "table:split",
        "table:merge",
        S,
        "table:sum",
        "table:doc-formula",
        S,
        "table:autosum",
        "table:fn-browser",
        "table:sort",
        "table:validation",
    ],
);
const DATA_SHEET: MenuSpec = menu(
    "데이터",
    &[
        "table:sort",
        "sheet:sort",
        "table:validation",
        S,
        "sheet:filter",
        "sheet:filter-reapply",
        "sheet:filter-clear",
        S,
        "table:sheet-pivot",
        "table:sheet-condfmt",
    ],
);

const TOOL_HWP: MenuSpec = menu(
    "도구",
    &[
        "tool:diagnose",
        "tool:scrub-pii",
        "tool:security-audit",
        "tool:bench-run",
        S,
        "tool:review-track",
        "tool:review-comment",
        "tool:review-verify",
        "tool:review-stats",
        S,
        "tool:pdf-highlight",
        "tool:pdf-rotate",
        "tool:pdf-splitmerge",
        "tool:pdf-tooffice",
        S,
        "tool:palette",
    ],
);
const TOOL_OFFICE: MenuSpec = menu(
    "도구",
    &[
        "tool:diagnose",
        "tool:scrub-pii",
        "tool:security-audit",
        "tool:bench-run",
        S,
        "tool:review-track",
        "tool:review-comment",
        "tool:review-verify",
        "tool:review-stats",
        S,
        "tool:palette",
    ],
);
const TOOL_PDF: MenuSpec = menu(
    "도구",
    &[
        "tool:diagnose",
        "tool:scrub-pii",
        "tool:security-audit",
        "tool:bench-run",
        S,
        "tool:review-verify",
        "tool:review-stats",
        S,
        "tool:pdf-tooffice",
        S,
        "tool:palette",
    ],
);

const DOC: [&MenuSpec; 11] = [
    &APP,
    &FILE_DOC,
    &EDIT_FULL,
    &VIEW_DOC,
    &INSERT_DOC,
    &FORMAT_TEXT,
    &PAGE_DOC,
    &TABLE_DOC,
    &TOOL_OFFICE,
    &WINDOW,
    &HELP,
];
/// The Hangul editor window keeps the bar it had before the bars followed the window kind.
const HWP: [&MenuSpec; 11] = [
    &APP,
    &FILE_HWP,
    &EDIT_FULL,
    &VIEW_HWP,
    &INSERT_HWP,
    &FORMAT_TEXT,
    &PAGE_HWP,
    &TABLE_HWP,
    &TOOL_HWP,
    &WINDOW,
    &HELP,
];
const SHEET: [&MenuSpec; 11] = [
    &APP,
    &FILE_PAGE_SETUP,
    &EDIT_FULL,
    &VIEW_PLAIN,
    &INSERT_SHEET,
    &FORMAT_SHEET,
    &SHEET_SHEET,
    &DATA_SHEET,
    &TOOL_OFFICE,
    &WINDOW,
    &HELP,
];
const SLIDE: [&MenuSpec; 11] = [
    &APP,
    &FILE_PAGE_SETUP,
    &EDIT_NO_FIND,
    &VIEW_PLAIN,
    &INSERT_SLIDE,
    &FORMAT_CELL,
    &SLIDE_SLIDE,
    &SLIDE_SHOW,
    &TOOL_OFFICE,
    &WINDOW,
    &HELP,
];
const PDF: [&MenuSpec; 9] = [
    &APP,
    &FILE_PDF,
    &EDIT_NO_FIND,
    &VIEW_PLAIN,
    &ANNOTATE_PDF,
    &PAGE_PDF,
    &TOOL_PDF,
    &WINDOW,
    &HELP,
];

/// The menus of the bar for a kind of window, in order.
pub fn layout(kind: MenuKind) -> &'static [&'static MenuSpec] {
    match kind {
        MenuKind::Doc => &DOC,
        MenuKind::Hwp => &HWP,
        MenuKind::Sheet => &SHEET,
        MenuKind::Slide => &SLIDE,
        MenuKind::Pdf => &PDF,
    }
}

/// Every id of the table a kind's bar holds (the submenus and system items expanded; the quit
/// item is not one of the table).
#[cfg(test)]
pub fn table_ids(kind: MenuKind) -> Vec<&'static str> {
    let mut ids = Vec::new();
    for spec in layout(kind) {
        for line in spec.lines {
            match *line {
                NEW_DOCS => ids.extend(NEW_DOC_IDS),
                THEME => ids.extend(THEME_IDS),
                NUMFMT => ids.extend(NUMFMT_LINES.iter().filter(|line| **line != SEPARATOR)),
                QUIT_ID => {}
                line if line == SEPARATOR || line.starts_with('@') => {}
                id => ids.push(id),
            }
        }
    }
    ids
}
