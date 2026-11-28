//! The Pages document window's menu bar (PD-190, PD-399).
//!
//! The order follows the official menu bar of the reference Pages app: 파일, 편집, 삽입, 포맷, 정렬, 보기, 윈도우, 도움말
//! (the page's own 도구 menu sits before 윈도우). Every item is a command the document window already runs: the date and
//! time, the bookmark, the watermark and the picture commands are the word processor's (`doc-word-*.js`), and the
//! highlight and the capitals are the run commands of the font dialog. What `.pages` cannot hold is counted by the save
//! warnings (`doc_save_loss`, `pages_write_loss`). The 표 menu the bar had is the 표 submenu of 포맷.

use super::{
    menu, MenuSpec, TokenSubmenu, APP, EXPORT_DOC, HELP, NEW_DOCS, PASTE, S, SELECT_ALL, THEME,
    WINDOW,
};

pub const PAGES_TABLE: &str = "@pages-table";
pub const PAGES_CASE: &str = "@pages-case";
pub const PAGES_WRAP: &str = "@pages-wrap";

/// The submenus of the bar below.
pub const PAGES_SUBMENUS: [TokenSubmenu; 3] = [
    TokenSubmenu {
        token: PAGES_TABLE,
        title: "표",
        lines: &[
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
            "table:sort",
        ],
    },
    TokenSubmenu {
        token: PAGES_CASE,
        title: "대문자",
        lines: &[
            "format:case-none",
            "format:case-all",
            "format:case-small",
            "format:case-title",
        ],
    },
    TokenSubmenu {
        token: PAGES_WRAP,
        title: "텍스트 줄 바꿈",
        lines: &[
            "arrange:wrap-inline",
            "arrange:wrap-square",
            "arrange:wrap-top-bottom",
            "arrange:wrap-behind",
            "arrange:wrap-front",
        ],
    },
];

/// The submenu a token of the Pages bar stands for.
pub fn pages_submenu(token: &str) -> Option<&'static TokenSubmenu> {
    PAGES_SUBMENUS.iter().find(|sub| sub.token == token)
}

const FILE_PAGES: MenuSpec = menu(
    "파일",
    &[
        "file:new",
        NEW_DOCS,
        S,
        "file:open",
        "file:open-recent",
        "file:save",
        "file:save-as",
        EXPORT_DOC,
        S,
        "page:pages-paper",
        "file:print-preview",
        "file:print",
    ],
);
const EDIT_PAGES: MenuSpec = menu(
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
const INSERT_PAGES: MenuSpec = menu(
    "삽입",
    &[
        "insert:shape",
        "insert:image",
        "insert:chart",
        "table:insert",
        S,
        "insert:link",
        "insert:pages-bookmark",
        "insert:datetime",
        S,
        "page:break",
        S,
        "insert:footnote",
        "insert:endnote",
        S,
        "insert:toc",
        "insert:toc-update",
        "page:header",
        "page:footer",
    ],
);
const FORMAT_PAGES: MenuSpec = menu(
    "포맷",
    &[
        "format:bold",
        "format:italic",
        "format:underline",
        "format:strike",
        "format:superscript",
        "format:subscript",
        S,
        "format:font-dialog",
        "format:highlight",
        PAGES_CASE,
        S,
        "format:style",
        "format:pages-justify",
        S,
        "format:para-checklist",
        "format:indent-in",
        "format:indent-out",
        S,
        PAGES_TABLE,
    ],
);
const ARRANGE_PAGES: MenuSpec = menu(
    "정렬",
    &[
        PAGES_WRAP,
        S,
        "arrange:crop",
        "arrange:rotate-right",
        "arrange:rotate-left",
        S,
        "arrange:watermark",
    ],
);
const VIEW_PAGES: MenuSpec = menu(
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
        "view:pages-inspector",
        S,
        THEME,
        "view:theme-toggle",
        S,
        "view:toolbar-labels",
        "view:para-marks",
        "tool:review-stats",
    ],
);
/// 도구 is the app's own menu (코파일럿, 문장 진단, 보안 검사): the reference Pages app has none. The engine diagnostic is
/// left to the command search.
const TOOL_PAGES: MenuSpec = menu(
    "도구",
    &[
        "tool:copilot",
        "tool:diagnose",
        "tool:scrub-pii",
        "tool:review-verify",
        "tool:doc-inspect",
        S,
        "tool:review-track",
        "tool:review-accept-all",
        "tool:review-reject-all",
        "tool:review-comment",
        S,
        "tool:palette",
    ],
);

pub const PAGES: [&MenuSpec; 10] = [
    &APP,
    &FILE_PAGES,
    &EDIT_PAGES,
    &INSERT_PAGES,
    &FORMAT_PAGES,
    &ARRANGE_PAGES,
    &VIEW_PAGES,
    &TOOL_PAGES,
    &WINDOW,
    &HELP,
];
