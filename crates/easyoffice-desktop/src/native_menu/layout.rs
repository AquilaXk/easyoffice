//! Which menus the native menu bar has for each kind of window (PD-190).
//!
//! The page draws one menu row that follows the window's kind (`menu-kinds.js`); the native bar
//! follows it too: a document, spreadsheet, presentation or PDF window shows only the menus and
//! items that mean something there, and the Hangul editor window keeps the full bar it always had
//! (the editor's own commands are in it). Every line is an id of the item table
//! ([`super::ITEMS`]), a separator or a token of a system item or submenu the builder knows.
//! The menu bar is data only: `build.rs` turns it into Tauri menus, and the tests read it as it is.

use super::QUIT_ID;

mod bars;
pub use bars::layout;
#[cfg(test)]
pub use bars::table_ids;

/// The kind of a window as far as its menu bar goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuKind {
    Doc,
    Pages,
    Hwp,
    Sheet,
    Slide,
    Pdf,
    Base,
    Board,
    Start,
    Conv,
}

impl MenuKind {
    pub const ALL: [MenuKind; 10] = [
        MenuKind::Doc,
        MenuKind::Pages,
        MenuKind::Hwp,
        MenuKind::Sheet,
        MenuKind::Slide,
        MenuKind::Pdf,
        MenuKind::Base,
        MenuKind::Board,
        MenuKind::Start,
        MenuKind::Conv,
    ];

    /// The name the page reports (`menuKindOf` in `menu-kinds.js`).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "DOC" => Some(Self::Doc),
            "PAGES" => Some(Self::Pages),
            "HWP" => Some(Self::Hwp),
            "SHEET" => Some(Self::Sheet),
            "SLIDE" => Some(Self::Slide),
            "PDF" => Some(Self::Pdf),
            "BASE" => Some(Self::Base),
            "BOARD" => Some(Self::Board),
            "START" => Some(Self::Start),
            "CONV" => Some(Self::Conv),
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
/// Per-window-kind 내보내기 submenus.
pub const EXPORT_DOC: &str = "@export-doc";
pub const EXPORT_SHEET: &str = "@export-sheet";
pub const EXPORT_SLIDE: &str = "@export-slide";
pub const EXPORT_BASE: &str = "@export-base";
pub const EXPORT_BOARD: &str = "@export-board";
pub const EXPORT_PDF: &str = "@export-pdf";
/// The 테마 submenu (the three theme check items).
pub const THEME: &str = "@theme";
/// The 표시 형식 submenu of the spreadsheet's 서식 menu ([`NUMFMT_LINES`]).
pub const NUMFMT: &str = "@numfmt";
/// The 맞춤 submenu of the spreadsheet's 서식 menu ([`ALIGN_LINES`]).
pub const ALIGN: &str = "@align";
/// The 맞춤 submenu of the presentation's 삽입 menu ([`SLIDE_ALIGN_LINES`]).
pub const SLIDE_ALIGN: &str = "@slide-align";
/// The 테두리 submenu of the spreadsheet's 서식 menu ([`BORDER_LINES`]).
pub const BORDER: &str = "@border";
/// The 조건부 서식 submenu of the spreadsheet's 서식 menu ([`CF_LINES`]; its own submenus are [`CF_SUBMENUS`]).
pub const CF: &str = "@cf";
/// The 수식에서 사용 submenu of the spreadsheet's 수식 menu ([`USE_IN_FORMULA_LINES`]).
pub const USE_IN_FORMULA: &str = "@name-use";
/// The 계산 옵션 submenu of the spreadsheet's 수식 menu ([`CALC_OPTIONS_LINES`]).
pub const CALC_OPTIONS: &str = "@calc-options";
pub const PASTE: &str = "@paste";
pub const SELECT_ALL: &str = "@select-all";
pub const SERVICES: &str = "@services";
pub const HIDE: &str = "@hide";
pub const HIDE_OTHERS: &str = "@hide-others";
pub const SHOW_ALL: &str = "@show-all";
pub const MINIMIZE: &str = "@minimize";
pub const CLOSE_WINDOW: &str = "@close-window";

/// The static lines of the 수식에서 사용 submenu (dynamic defined names live in the WebUI menu).
pub const USE_IN_FORMULA_LINES: [&str; 1] = ["sheet:name-paste"];
/// The lines of the 계산 옵션 submenu.
pub const CALC_OPTIONS_LINES: [&str; 3] = [
    "sheet:calc-auto",
    "sheet:calc-auto-no-table",
    "sheet:calc-manual",
];
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
pub const EXPORT_DOC_LINES: [&str; 9] = [
    "file:exp-docx",
    "file:exp-odt",
    "file:exp-rtf",
    "file:exp-pages",
    "file:exp-md",
    "file:exp-txt",
    "file:exp-pdf",
    SEPARATOR,
    "file:exp-dlg",
];
pub const EXPORT_SHEET_LINES: [&str; 7] = [
    "file:exp-xlsx",
    "file:exp-ods",
    "file:exp-csv",
    "file:exp-tsv",
    "file:exp-pdf",
    SEPARATOR,
    "file:exp-dlg",
];
pub const EXPORT_SLIDE_LINES: [&str; 5] = [
    "file:exp-pptx",
    "file:exp-odp",
    "file:exp-pdf",
    SEPARATOR,
    "file:exp-dlg",
];
pub const EXPORT_BASE_LINES: [&str; 6] = [
    "file:exp-base",
    "file:exp-xlsx",
    "file:exp-csv",
    "file:exp-tsv",
    SEPARATOR,
    "file:exp-dlg",
];
pub const EXPORT_BOARD_LINES: [&str; 4] =
    ["file:exp-board", "file:exp-pdf", SEPARATOR, "file:exp-dlg"];
pub const EXPORT_PDF_LINES: [&str; 5] = [
    "file:exp-pdf",
    "file:exp-docx",
    "file:exp-txt",
    SEPARATOR,
    "file:exp-dlg",
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
/// The lines of the presentation 맞춤 submenu.
pub const SLIDE_ALIGN_LINES: [&str; 10] = [
    "slide:align-left",
    "slide:align-center",
    "slide:align-right",
    SEPARATOR,
    "slide:align-top",
    "slide:align-middle",
    "slide:align-bottom",
    SEPARATOR,
    "slide:distribute-h",
    "slide:distribute-v",
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
        lines: &[
            "format:cf-clear-selection",
            "format:cf-clear-sheet",
            "format:cf-clear-table",
            "format:cf-clear-pivot",
        ],
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
