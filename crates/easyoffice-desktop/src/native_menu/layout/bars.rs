//! Per-kind menu bar specifications and lookups for macOS (`PD-190`, `PD-334`).

#[cfg(test)]
use super::{
    cf_submenu, ALIGN_LINES, BORDER_LINES, CALC_OPTIONS_LINES, CF_LINES, NEW_DOC_IDS, NUMFMT_LINES,
    QUIT_ID, SEPARATOR, THEME_IDS,
};
use super::{
    menu, MenuKind, MenuSpec, ALIGN, APP, BORDER, CALC_OPTIONS, CF, HELP, NEW_DOCS, NUMFMT, PASTE,
    S, SELECT_ALL, THEME, WINDOW,
};

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
const FILE_WORKSPACE: MenuSpec = menu(
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
/// 용지 설정 is a 파일 item in the presentation window (it has no 쪽 menu).
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
        "page:doc-paper",
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
const EDIT_PLAIN: MenuSpec = menu(
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
const VIEW_WORKSPACE: MenuSpec = menu(
    "보기",
    &[
        "view:zoom-in",
        "view:zoom-out",
        "view:zoom-100",
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
        S,
        "insert:footnote",
        "insert:symbol",
        "insert:link",
        S,
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
        S,
        "page:header",
        "page:footer",
        "page:columns",
        S,
        "page:hwp-header",
        "page:manuscript",
    ],
);
const PAGE_DOC: MenuSpec = menu(
    "쪽",
    &[
        "page:doc-paper",
        "page:orientation",
        "page:margins",
        S,
        "page:doc-header",
        "page:doc-footer",
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
        "table:hwp-border",
        S,
        "table:autosum",
        "table:fn-browser",
        "table:sort",
        "table:validation",
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
/// The spreadsheet's 수식 menu: defined names, calculation options, and manual/sheet recalculation.
const FORMULA_SHEET: MenuSpec = menu(
    "수식",
    &[
        "sheet:name-define",
        "sheet:name-manage",
        "sheet:name-create",
        S,
        CALC_OPTIONS,
        "sheet:calc-now",
        "sheet:calc-sheet",
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
const TOOL_WORKSPACE: MenuSpec = menu("도구", &["tool:copilot", S, "tool:palette"]);

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
    &EDIT_PLAIN,
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
    &EDIT_PLAIN,
    &VIEW_PLAIN,
    &ANNOTATE_PDF,
    &PAGE_PDF,
    &TOOL_PDF,
    &WINDOW,
    &HELP,
];
const BASE: [&MenuSpec; 7] = [
    &APP,
    &FILE_WORKSPACE,
    &EDIT_FULL,
    &VIEW_WORKSPACE,
    &TOOL_WORKSPACE,
    &WINDOW,
    &HELP,
];
const BOARD: [&MenuSpec; 7] = [
    &APP,
    &FILE_WORKSPACE,
    &EDIT_PLAIN,
    &VIEW_WORKSPACE,
    &TOOL_WORKSPACE,
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
        MenuKind::Base => &BASE,
        MenuKind::Board => &BOARD,
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
                CALC_OPTIONS => ids.extend(CALC_OPTIONS_LINES),
                QUIT_ID => {}
                line if line == SEPARATOR || line.starts_with('@') => {}
                id => ids.push(id),
            }
        }
    }
    ids
}
