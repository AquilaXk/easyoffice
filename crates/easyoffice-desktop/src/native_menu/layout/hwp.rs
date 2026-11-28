//! The Hangul editor window's menu bar (PD-190, PD-396).
//!
//! The menus follow the official 한글 menu pages (파일, 편집, 보기, 입력, 서식, 쪽, 보안, 검토, 도구, 표) and
//! name the commands the editor's own menu row has (`rhwp-studio`, MIT); the page sends each one over the
//! embed port. On macOS this bar is the window's one menu: the editor hides its own row there.

use super::{
    menu, MenuSpec, TokenSubmenu, APP, HELP, NEW_DOCS, PASTE, S, SELECT_ALL, SEPARATOR, THEME,
    WINDOW,
};

pub const HWP_TOOLBOX: &str = "@hwp-toolbox";
pub const HWP_OBJECT: &str = "@hwp-object";
pub const HWP_CAPTION: &str = "@hwp-caption";
pub const HWP_ROTATE: &str = "@hwp-rotate";
pub const HWP_HEADER_FOOTER: &str = "@hwp-header-footer";
pub const HWP_CELL_BORDER: &str = "@hwp-cell-border";
pub const HWP_MANUSCRIPT: &str = "@hwp-manuscript";

/// The submenus of the bar below.
pub const HWP_SUBMENUS: [TokenSubmenu; 7] = [
    TokenSubmenu {
        token: HWP_TOOLBOX,
        title: "도구 상자",
        lines: &["view:toolbox-basic", "view:toolbox-format"],
    },
    TokenSubmenu {
        token: HWP_OBJECT,
        title: "개체",
        lines: &["insert:textbox", "insert:shape", "insert:equation"],
    },
    TokenSubmenu {
        token: HWP_CAPTION,
        title: "캡션 넣기",
        lines: &[
            "insert:caption-top",
            "insert:caption-lt",
            "insert:caption-lm",
            "insert:caption-lb",
            "insert:caption-rt",
            "insert:caption-rm",
            "insert:caption-rb",
            "insert:caption-bottom",
            "insert:caption-none",
        ],
    },
    TokenSubmenu {
        token: HWP_ROTATE,
        title: "회전/대칭",
        lines: &[
            "insert:rotate-cw",
            "insert:rotate-ccw",
            SEPARATOR,
            "insert:flip-horz",
            "insert:flip-vert",
        ],
    },
    TokenSubmenu {
        token: HWP_HEADER_FOOTER,
        title: "머리말/꼬리말",
        lines: &["page:header", "page:footer"],
    },
    TokenSubmenu {
        token: HWP_CELL_BORDER,
        title: "셀 테두리/배경",
        lines: &["table:border-bg", "table:border-one"],
    },
    TokenSubmenu {
        token: HWP_MANUSCRIPT,
        title: "원고지",
        lines: &["page:manuscript"],
    },
];

/// The submenu a token of the Hangul bar stands for.
pub fn hwp_submenu(token: &str) -> Option<&'static TokenSubmenu> {
    HWP_SUBMENUS.iter().find(|sub| sub.token == token)
}

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
        "file:save-as-hwpx",
        "file:export-pdf",
        S,
        "file:page-setup",
        "file:print",
    ],
);
const EDIT_HWP: MenuSpec = menu(
    "편집",
    &[
        "edit:undo",
        "edit:redo",
        S,
        "edit:cut",
        "edit:copy",
        PASTE,
        "edit:format-copy",
        S,
        "edit:delete",
        SELECT_ALL,
        S,
        "edit:find",
        "edit:replace",
        "edit:find-again",
        "edit:goto",
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
        "view:ctrl-marks",
        "view:para-marks",
        "view:border-transparent",
        "view:clip",
        "view:doc-grid",
        "view:grid-settings",
        S,
        HWP_TOOLBOX,
        "view:toolbar-labels",
        "view:form-mode",
    ],
);
const INSERT_HWP: MenuSpec = menu(
    "입력",
    &[
        "insert:image",
        HWP_OBJECT,
        HWP_CAPTION,
        S,
        "insert:symbol",
        S,
        "insert:footnote",
        "insert:endnote",
        "insert:endnote-shape",
        S,
        "insert:bookmark",
        "insert:link",
        "insert:field",
        S,
        HWP_ROTATE,
        "insert:picture-delete",
    ],
);
const FORMAT_HWP: MenuSpec = menu(
    "서식",
    &[
        "format:hwp-char-shape",
        "format:hwp-para-shape",
        S,
        "format:style",
        S,
        "format:para-numbering",
        "format:para-bullet",
        "format:indent-in",
        "format:indent-out",
        S,
        "format:bold",
        "format:italic",
        "format:underline",
        "format:strike",
        "format:superscript",
        "format:subscript",
        S,
        "format:hwp-font-up",
        "format:hwp-font-down",
        S,
        "format:hwp-align-left",
        "format:hwp-align-center",
        "format:hwp-align-right",
        "format:justify",
        "format:hwp-align-distribute",
        S,
        "format:hwp-line-up",
        "format:hwp-line-down",
        S,
        "format:hwp-object-props",
    ],
);
const PAGE_HWP: MenuSpec = menu(
    "쪽",
    &[
        "page:paper",
        "page:page-border",
        HWP_HEADER_FOOTER,
        "page:hwp-header",
        S,
        "page:new-number",
        "page:hide-current",
        S,
        "page:break",
        "page:column-break",
        "page:columns",
        S,
        "page:section-settings",
        HWP_MANUSCRIPT,
    ],
);
const SECURITY_HWP: MenuSpec = menu(
    "보안",
    &[
        "file:set-password",
        S,
        "tool:scrub-pii",
        "tool:review-verify",
    ],
);
const REVIEW_HWP: MenuSpec = menu(
    "검토",
    &[
        "tool:review-track",
        "tool:review-comment",
        S,
        "tool:review-history",
    ],
);
const TOOL_HWP: MenuSpec = menu(
    "도구",
    &[
        "tool:copilot",
        "tool:diagnose",
        S,
        "tool:review-stats",
        S,
        "tool:palette",
        S,
        "tool:options",
        THEME,
        "view:theme-toggle",
    ],
);
const TABLE_HWP: MenuSpec = menu(
    "표",
    &[
        "table:insert",
        "table:cell-props",
        HWP_CELL_BORDER,
        S,
        "table:insert-row-col",
        "table:delete-row-col",
        S,
        "table:cell-select",
        "table:merge",
        "table:cell-split",
        "table:hwp-split",
        "table:row-equal",
        "table:col-equal",
        "table:transpose-copy",
        "table:transpose-paste",
        S,
        "table:block-formula",
        "table:sum",
        "table:block-avg",
        "table:block-product",
        S,
        "table:thousand-sep",
        "table:decimal-add",
        "table:decimal-remove",
        S,
        "table:hwp-border",
    ],
);

/// The bar, in the order of the official menus (표 is the last one there).
pub const HWP: [&MenuSpec; 13] = [
    &APP,
    &FILE_HWP,
    &EDIT_HWP,
    &VIEW_HWP,
    &INSERT_HWP,
    &FORMAT_HWP,
    &PAGE_HWP,
    &SECURITY_HWP,
    &REVIEW_HWP,
    &TOOL_HWP,
    &TABLE_HWP,
    &WINDOW,
    &HELP,
];
