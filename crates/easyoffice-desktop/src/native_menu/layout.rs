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
/// The 맞춤 submenu of the spreadsheet's 서식 menu ([`ALIGN_LINES`]).
pub const ALIGN: &str = "@align";
/// The 테두리 submenu of the spreadsheet's 서식 menu ([`BORDER_LINES`]).
pub const BORDER: &str = "@border";
/// The 조건부 서식 submenu of the spreadsheet's 서식 menu ([`CF_LINES`]; its own submenus are [`CF_SUBMENUS`]).
pub const CF: &str = "@cf";
pub const PASTE: &str = "@paste";
pub const SELECT_ALL: &str = "@select-all";
pub const SERVICES: &str = "@services";
pub const HIDE: &str = "@hide";
pub const HIDE_OTHERS: &str = "@hide-others";
pub const SHOW_ALL: &str = "@show-all";
pub const MINIMIZE: &str = "@minimize";
pub const CLOSE_WINDOW: &str = "@close-window";

/// The items of the 새 문서 submenu.
pub const NEW_DOC_IDS: [&str; 8] = [
    "file:new-doc",
    "file:new-hwp",
    "file:new-sheet",
    "file:new-slide",
    "file:new-base",
    "file:new-board",
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
/// The lines of the 맞춤 submenu.
pub const ALIGN_LINES: [&str; 7] = [
    "format:align-left",
    "format:align-center",
    "format:align-right",
    SEPARATOR,
    "format:valign-top",
    "format:valign-middle",
    "format:valign-bottom",
];
/// The lines of the 테두리 submenu.
pub const BORDER_LINES: [&str; 16] = [
    "format:border-all",
    "format:border-outer",
    "format:border-inner",
    "format:border-top",
    "format:border-bottom",
    "format:border-left",
    "format:border-right",
    "format:border-none",
    SEPARATOR,
    "format:border-line-thin",
    "format:border-line-thick",
    "format:border-line-dashed",
    "format:border-line-dotted",
    "format:border-line-double",
    SEPARATOR,
    "format:border-color",
];
/// The lines of the 조건부 서식 submenu: five submenus, then new rule, clear rules and manage rules.
pub const CF_LINES: [&str; 9] = [
    "@cf-highlight",
    "@cf-top",
    "@cf-bars",
    "@cf-scales",
    "@cf-icons",
    SEPARATOR,
    "format:cf-new",
    "@cf-clear",
    "format:cf-manage",
];

/// A submenu of the 조건부 서식 menu: the token its parent lists, its title and its lines.
pub struct CfSubmenu {
    pub token: &'static str,
    pub title: &'static str,
    pub lines: &'static [&'static str],
}

pub const CF_SUBMENUS: [CfSubmenu; 6] = [
    CfSubmenu {
        token: "@cf-highlight",
        title: "셀 강조 규칙",
        lines: &[
            "format:cf-quick-greater",
            "format:cf-quick-less",
            "format:cf-quick-between",
            "format:cf-quick-equal",
            "format:cf-quick-text",
            "format:cf-quick-date",
            "format:cf-quick-dup",
            SEPARATOR,
            "format:cf-more-highlight",
        ],
    },
    CfSubmenu {
        token: "@cf-top",
        title: "상위/하위 규칙",
        lines: &[
            "format:cf-quick-top",
            "format:cf-quick-top-percent",
            "format:cf-quick-bottom",
            "format:cf-quick-bottom-percent",
            "format:cf-quick-above",
            "format:cf-quick-below",
            SEPARATOR,
            "format:cf-more-top",
        ],
    },
    CfSubmenu {
        token: "@cf-bars",
        title: "데이터 막대",
        lines: &[
            "format:cf-bar-grad-blue",
            "format:cf-bar-grad-green",
            "format:cf-bar-grad-red",
            "format:cf-bar-grad-orange",
            "format:cf-bar-grad-lightblue",
            "format:cf-bar-grad-purple",
            SEPARATOR,
            "format:cf-bar-solid-blue",
            "format:cf-bar-solid-green",
            "format:cf-bar-solid-red",
            "format:cf-bar-solid-orange",
            "format:cf-bar-solid-lightblue",
            "format:cf-bar-solid-purple",
            SEPARATOR,
            "format:cf-more-bar",
        ],
    },
    CfSubmenu {
        token: "@cf-scales",
        title: "색조",
        lines: &[
            "format:cf-scale-gyr",
            "format:cf-scale-ryg",
            "format:cf-scale-gwr",
            "format:cf-scale-rwg",
            "format:cf-scale-bwr",
            "format:cf-scale-rwb",
            "format:cf-scale-wr",
            "format:cf-scale-rw",
            "format:cf-scale-gw",
            "format:cf-scale-wg",
            "format:cf-scale-gy",
            "format:cf-scale-yg",
            SEPARATOR,
            "format:cf-more-scale",
        ],
    },
    CfSubmenu {
        token: "@cf-icons",
        title: "아이콘 집합",
        lines: &[
            "format:cf-icons-3arrows",
            "format:cf-icons-3arrowsgray",
            "format:cf-icons-3triangles",
            "format:cf-icons-4arrowsgray",
            "format:cf-icons-4arrows",
            "format:cf-icons-5arrowsgray",
            "format:cf-icons-5arrows",
            SEPARATOR,
            "format:cf-icons-3trafficlights1",
            "format:cf-icons-3trafficlights2",
            "format:cf-icons-3signs",
            "format:cf-icons-4trafficlights",
            "format:cf-icons-4redtoblack",
            SEPARATOR,
            "format:cf-icons-3symbols",
            "format:cf-icons-3symbols2",
            "format:cf-icons-3flags",
            SEPARATOR,
            "format:cf-icons-3stars",
            "format:cf-icons-4rating",
            "format:cf-icons-5rating",
            "format:cf-icons-5quarters",
            "format:cf-icons-5boxes",
            SEPARATOR,
            "format:cf-more-icons",
        ],
    },
    CfSubmenu {
        token: "@cf-clear",
        title: "규칙 지우기",
        lines: &["format:cf-clear-selection", "format:cf-clear-sheet"],
    },
];

/// The submenu a token of [`CF_LINES`] stands for.
pub fn cf_submenu(token: &str) -> Option<&'static CfSubmenu> {
    CF_SUBMENUS.iter().find(|sub| sub.token == token)
}

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
        "file:print-preview",
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
        "file:print-preview",
        "file:print",
    ],
);
/// The spreadsheet's 파일 menu: the page setup and the print area sit before 인쇄.
const FILE_SHEET: MenuSpec = menu(
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
        "sheet:page-setup",
        "sheet:print-area-set",
        "sheet:print-area-clear",
        "file:print-preview",
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
/// The spreadsheet's edit menu: the standard group with 골라 붙이기 after the system paste.
const EDIT_SHEET: MenuSpec = menu(
    "편집",
    &[
        "edit:undo",
        "edit:redo",
        S,
        "edit:cut",
        "edit:copy",
        PASTE,
        "edit:paste-special",
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
        "insert:image",
        "insert:chart",
        S,
        "insert:footnote",
        "insert:endnote",
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
        "sheet:link-remove",
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
        "format:para-checklist",
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
        ALIGN,
        "format:wrap",
        BORDER,
        CF,
        "format:cell-format",
    ],
);

const PAGE_HWP: MenuSpec = menu(
    "쪽",
    &[
        "page:paper",
        "page:header",
        "page:footer",
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
        "page:footer",
        "page:columns",
        "page:break",
    ],
);
const PAGE_PDF: MenuSpec = menu("쪽", &["tool:pdf-rotate", "tool:pdf-splitmerge"]);
const SHEET_SHEET: MenuSpec = menu(
    "시트",
    &[
        "sheet:freeze-first-row",
        "sheet:freeze-first-col",
        "table:sheet-freeze",
        "sheet:freeze-clear",
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
        "sheet:tab-insert",
        "sheet:tab-delete",
        "sheet:tab-rename",
        "sheet:tab-copy",
        "sheet:tab-move-copy",
        "sheet:tab-left",
        "sheet:tab-right",
        "sheet:tab-hide",
        "sheet:tab-unhide",
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
        "table:col-equal",
        S,
        "table:border-bg",
        "table:cell-props",
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
/// The spreadsheet's 수식 menu: the defined names (수식 > 이름 > 이름 정의, 이름 관리 of the spreadsheet help).
const FORMULA_SHEET: MenuSpec = menu(
    "수식",
    &[
        "sheet:name-define",
        "sheet:name-manage",
        "sheet:name-create",
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
        "sheet:group",
        "sheet:ungroup",
        "sheet:hide-detail",
        "sheet:show-detail",
        "sheet:outline-clear",
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
const TOOL_DOC: MenuSpec = menu(
    "도구",
    &[
        "tool:copilot",
        "tool:diagnose",
        "tool:scrub-pii",
        "tool:security-audit",
        "tool:doc-inspect",
        "tool:bench-run",
        S,
        "tool:review-track",
        "tool:review-accept-all",
        "tool:review-reject-all",
        "tool:review-comment",
        "tool:review-verify",
        "tool:review-stats",
        S,
        "tool:palette",
    ],
);
const TOOL_OFFICE: MenuSpec = menu(
    "도구",
    &[
        "tool:copilot",
        "tool:diagnose",
        "tool:scrub-pii",
        "tool:security-audit",
        "tool:doc-inspect",
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
/// The spreadsheet's 도구 menu: the common items and the protection commands (도구 > 보호 of the spreadsheet help).
const TOOL_SHEET: MenuSpec = menu(
    "도구",
    &[
        "tool:copilot",
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
        "sheet:protect",
        "sheet:protect-workbook",
        "sheet:protect-ranges",
        S,
        "tool:palette",
    ],
);
const TOOL_PDF: MenuSpec = menu(
    "도구",
    &[
        "tool:copilot",
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
    &TOOL_DOC,
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
const SHEET: [&MenuSpec; 12] = [
    &APP,
    &FILE_SHEET,
    &EDIT_SHEET,
    &VIEW_PLAIN,
    &INSERT_SHEET,
    &FORMAT_SHEET,
    &SHEET_SHEET,
    &FORMULA_SHEET,
    &DATA_SHEET,
    &TOOL_SHEET,
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

/// The ids of the 조건부 서식 submenu and of the submenus in it.
#[cfg(test)]
fn cf_ids() -> Vec<&'static str> {
    let mut ids = Vec::new();
    for line in CF_LINES {
        match cf_submenu(line) {
            Some(sub) => ids.extend(sub.lines.iter().filter(|id| **id != SEPARATOR)),
            None if line != SEPARATOR => ids.push(line),
            None => {}
        }
    }
    ids
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
                ALIGN => ids.extend(ALIGN_LINES.iter().filter(|line| **line != SEPARATOR)),
                BORDER => ids.extend(BORDER_LINES.iter().filter(|line| **line != SEPARATOR)),
                CF => ids.extend(cf_ids()),
                QUIT_ID => {}
                line if line == SEPARATOR || line.starts_with('@') => {}
                id => ids.push(id),
            }
        }
    }
    ids
}
