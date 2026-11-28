//! Per-kind menu bar specifications and lookups for macOS (`PD-190`, `PD-334`).

#[cfg(test)]
use super::{
    cf_submenu, sheet_submenu, ALIGN_LINES, BORDER_LINES, CALC_OPTIONS_LINES, CF_LINES,
    EXPORT_BASE_LINES, EXPORT_BOARD_LINES, EXPORT_DOC_LINES, EXPORT_PDF_LINES, EXPORT_SHEET_LINES,
    EXPORT_SLIDE_LINES, NEW_DOC_IDS, NUMFMT_LINES, QUIT_ID, SEPARATOR, SLIDE_ALIGN_LINES,
    THEME_IDS, USE_IN_FORMULA_LINES,
};
use super::{
    menu, MenuKind, MenuSpec, ALIGN, APP, BORDER, CALC_OPTIONS, CF, EXPORT_BASE, EXPORT_BOARD,
    EXPORT_DOC, EXPORT_PDF, EXPORT_SHEET, EXPORT_SLIDE, HELP, NEW_DOCS, NUMFMT, PASTE, S,
    SELECT_ALL, SHEET_COL, SHEET_FREEZE, SHEET_ROW, SHEET_TAB, SLIDE_ALIGN, THEME, USE_IN_FORMULA,
    WINDOW,
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
        "file:save-as-hwpx",
        S,
        "file:page-setup",
        "file:export-pdf",
        "file:print",
    ],
);
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
        EXPORT_DOC,
        S,
        "file:doc-paper",
        "file:print-preview",
        "file:print",
    ],
);
const FILE_BASE: MenuSpec = menu(
    "파일",
    &[
        "file:new",
        NEW_DOCS,
        S,
        "file:open",
        "file:open-recent",
        "file:save",
        "file:save-as",
        EXPORT_BASE,
        "file:base-import",
        S,
        "file:print-preview",
        "file:print",
    ],
);
const FILE_BOARD: MenuSpec = menu(
    "파일",
    &[
        "file:new",
        NEW_DOCS,
        S,
        "file:open",
        "file:open-recent",
        "file:save",
        "file:save-as",
        EXPORT_BOARD,
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
        EXPORT_SLIDE,
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
        EXPORT_SHEET,
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
        EXPORT_PDF,
        S,
        "file:print-preview",
        "file:print",
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
        "edit:goto",
    ],
);
const EDIT_PDF: MenuSpec = menu(
    "편집",
    &["edit:undo", "edit:redo", S, SELECT_ALL, S, "edit:find"],
);
const EDIT_WORKSPACE: MenuSpec = menu(
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
const EDIT_FIND_ONLY: MenuSpec = menu(
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
        "view:toolbar-labels",
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
        S,
        "view:toolbar-labels",
    ],
);
/// The spreadsheet's 보기 menu: the plain view menu with 틀 고정 after the zoom group.
const VIEW_SHEET: MenuSpec = menu(
    "보기",
    &[
        "view:zoom-in",
        "view:zoom-out",
        "view:zoom-100",
        S,
        "view:zoom-fit-page",
        "view:zoom-fit-width",
        S,
        "sheet:view-formula-bar",
        "sheet:view-headings",
        SHEET_FREEZE,
        S,
        "view:split",
        "view:outline",
        S,
        THEME,
        "view:theme-toggle",
        S,
        "view:toolbar-labels",
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
        S,
        "view:toolbar-labels",
    ],
);

const INSERT_HWP: MenuSpec = menu(
    "입력",
    &[
        "insert:shape",
        "insert:image",
        S,
        "insert:footnote",
        "insert:endnote",
        "insert:symbol",
        "insert:link",
        S,
        "insert:field",
    ],
);
const INSERT_DOC: MenuSpec = menu(
    "삽입",
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
const INSERT_PAGES: MenuSpec = menu(
    "삽입",
    &[
        "insert:shape",
        "insert:image",
        "insert:chart",
        S,
        "insert:footnote",
        "insert:endnote",
        "insert:link",
        S,
        "insert:toc",
        "insert:toc-update",
        "page:header",
        "page:footer",
    ],
);
const INSERT_SHEET: MenuSpec = menu(
    "삽입",
    &[
        "sheet:row-insert-above",
        "sheet:col-insert-left",
        "sheet:tab-insert",
        S,
        "table:insert",
        "sheet:insert-picture",
        "insert:shape",
        "insert:chart",
        "insert:symbol",
        "insert:link",
        "sheet:link-remove",
        S,
        "sheet:note-insert",
        "sheet:note-toggle-all",
        S,
        "table:fn-browser",
        "table:autosum",
    ],
);
const INSERT_SLIDE: MenuSpec = menu(
    "삽입",
    &[
        "slide:insert-textbox",
        "table:insert",
        "slide:insert-picture",
        "insert:shape",
        "insert:chart",
        "insert:symbol",
        "insert:link",
    ],
);
const ANNOTATE_PDF: MenuSpec = menu("주석", &["tool:pdf-highlight", "tool:review-comment"]);

const FORMAT_DOC: MenuSpec = menu(
    "서식",
    &[
        "format:bold",
        "format:italic",
        "format:underline",
        "format:strike",
        "format:superscript",
        "format:subscript",
        "format:font-dialog",
        S,
        "format:style",
        "format:doc-justify",
        S,
        "format:para-numbering",
        "format:para-bullet",
        "format:para-checklist",
        "format:indent-in",
        "format:indent-out",
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
        "format:style",
        "format:pages-justify",
        S,
        "format:para-checklist",
        "format:indent-in",
        "format:indent-out",
    ],
);
const FORMAT_HWP: MenuSpec = menu(
    "서식",
    &[
        "format:bold",
        "format:italic",
        "format:underline",
        "format:strike",
        "format:superscript",
        "format:subscript",
        S,
        "format:hwp-char-shape",
        "format:hwp-para-shape",
        S,
        "format:style",
        "format:justify",
        S,
        "format:para-numbering",
        "format:para-bullet",
        "format:indent-in",
        "format:indent-out",
    ],
);
const FORMAT_SLIDE: MenuSpec = menu(
    "서식",
    &[
        "format:bold",
        "format:italic",
        "format:underline",
        S,
        "format:style",
        S,
        "slide:bring-front",
        "slide:bring-forward",
        "slide:send-backward",
        "slide:send-back",
        SLIDE_ALIGN,
        "slide:delete-element",
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
        "sheet:merge",
        "sheet:unmerge",
        BORDER,
        CF,
        S,
        SHEET_ROW,
        SHEET_COL,
        SHEET_TAB,
        S,
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
    "레이아웃",
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
const SLIDE_SLIDE: MenuSpec = menu(
    "슬라이드",
    &[
        "page:slide-master",
        "page:slide-snap",
        S,
        "page:orientation",
    ],
);
const SLIDE_SHOW: MenuSpec = menu(
    "슬라이드 쇼",
    &["page:slide-transition", "page:slide-presenter"],
);

const TABLE_PAGES: MenuSpec = menu(
    "표",
    &[
        "table:insert",
        S,
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
        "table:hwp-split",
        "table:col-equal",
        S,
        "table:border-bg",
        "table:cell-props",
        S,
        "table:sum",
        "table:hwp-border",
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
        "table:doc-formula",
        "table:sort",
    ],
);
/// The spreadsheet's 수식 menu: defined names, calculation options, and manual/sheet recalculation.
const FORMULA_SHEET: MenuSpec = menu(
    "수식",
    &[
        "sheet:name-define",
        "sheet:name-manage",
        "sheet:name-create",
        USE_IN_FORMULA,
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
        "sheet:remove-duplicates",
        "sheet:text-to-columns",
        S,
        "table:sheet-pivot",
        "table:sheet-condfmt",
    ],
);

const TOOL_HWP: MenuSpec = menu(
    "도구",
    &[
        "tool:copilot",
        "tool:diagnose",
        "tool:scrub-pii",
        "tool:review-verify",
        "tool:bench-run",
        S,
        "tool:review-track",
        "tool:review-comment",
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
        "tool:review-verify",
        "tool:doc-inspect",
        "tool:bench-run",
        S,
        "tool:review-track",
        "tool:review-accept-all",
        "tool:review-reject-all",
        "tool:review-comment",
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
        "tool:review-verify",
        "tool:doc-inspect",
        "tool:bench-run",
        S,
        "tool:review-comment",
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
        "tool:review-verify",
        "tool:bench-run",
        S,
        "tool:review-track",
        "tool:review-comment",
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
        "tool:review-verify",
        "tool:bench-run",
        S,
        "tool:review-stats",
        S,
        "tool:pdf-tooffice",
        S,
        "tool:palette",
    ],
);
const TOOL_WORKSPACE: MenuSpec = menu("도구", &["tool:copilot", S, "tool:palette"]);
const TOOL_MINIMAL: MenuSpec = menu("도구", &["tool:palette"]);

const DOC: [&MenuSpec; 11] = [
    &APP,
    &FILE_DOC,
    &EDIT_FULL,
    &VIEW_DOC,
    &INSERT_DOC,
    &FORMAT_DOC,
    &PAGE_DOC,
    &TABLE_DOC,
    &TOOL_DOC,
    &WINDOW,
    &HELP,
];
const PAGES: [&MenuSpec; 10] = [
    &APP,
    &FILE_PAGES,
    &EDIT_FULL,
    &VIEW_PAGES,
    &INSERT_PAGES,
    &FORMAT_PAGES,
    &TABLE_PAGES,
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
    &FORMAT_HWP,
    &PAGE_HWP,
    &TABLE_HWP,
    &TOOL_HWP,
    &WINDOW,
    &HELP,
];
const SHEET: [&MenuSpec; 11] = [
    &APP,
    &FILE_SHEET,
    &EDIT_SHEET,
    &VIEW_SHEET,
    &INSERT_SHEET,
    &FORMAT_SHEET,
    &FORMULA_SHEET,
    &DATA_SHEET,
    &TOOL_SHEET,
    &WINDOW,
    &HELP,
];
const SLIDE: [&MenuSpec; 11] = [
    &APP,
    &FILE_PAGE_SETUP,
    &EDIT_FIND_ONLY,
    &VIEW_PLAIN,
    &INSERT_SLIDE,
    &FORMAT_SLIDE,
    &SLIDE_SLIDE,
    &SLIDE_SHOW,
    &TOOL_OFFICE,
    &WINDOW,
    &HELP,
];
const PDF: [&MenuSpec; 9] = [
    &APP,
    &FILE_PDF,
    &EDIT_PDF,
    &VIEW_PLAIN,
    &ANNOTATE_PDF,
    &PAGE_PDF,
    &TOOL_PDF,
    &WINDOW,
    &HELP,
];
const BASE: [&MenuSpec; 7] = [
    &APP,
    &FILE_BASE,
    &EDIT_FULL,
    &VIEW_WORKSPACE,
    &TOOL_WORKSPACE,
    &WINDOW,
    &HELP,
];
const BOARD: [&MenuSpec; 7] = [
    &APP,
    &FILE_BOARD,
    &EDIT_FIND_ONLY,
    &VIEW_WORKSPACE,
    &TOOL_WORKSPACE,
    &WINDOW,
    &HELP,
];
const START: [&MenuSpec; 7] = [
    &APP,
    &FILE_WORKSPACE,
    &EDIT_WORKSPACE,
    &VIEW_WORKSPACE,
    &TOOL_MINIMAL,
    &WINDOW,
    &HELP,
];
const CONV: [&MenuSpec; 7] = [
    &APP,
    &FILE_WORKSPACE,
    &EDIT_WORKSPACE,
    &VIEW_WORKSPACE,
    &TOOL_MINIMAL,
    &WINDOW,
    &HELP,
];

/// The menus of the bar for a kind of window, in order.
pub fn layout(kind: MenuKind) -> &'static [&'static MenuSpec] {
    match kind {
        MenuKind::Doc => &DOC,
        MenuKind::Pages => &PAGES,
        MenuKind::Hwp => &HWP,
        MenuKind::Sheet => &SHEET,
        MenuKind::Slide => &SLIDE,
        MenuKind::Pdf => &PDF,
        MenuKind::Base => &BASE,
        MenuKind::Board => &BOARD,
        MenuKind::Start => &START,
        MenuKind::Conv => &CONV,
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
                EXPORT_DOC => {
                    ids.extend(EXPORT_DOC_LINES.iter().filter(|line| **line != SEPARATOR))
                }
                EXPORT_SHEET => {
                    ids.extend(EXPORT_SHEET_LINES.iter().filter(|line| **line != SEPARATOR))
                }
                EXPORT_SLIDE => {
                    ids.extend(EXPORT_SLIDE_LINES.iter().filter(|line| **line != SEPARATOR))
                }
                EXPORT_BASE => {
                    ids.extend(EXPORT_BASE_LINES.iter().filter(|line| **line != SEPARATOR))
                }
                EXPORT_BOARD => {
                    ids.extend(EXPORT_BOARD_LINES.iter().filter(|line| **line != SEPARATOR))
                }
                EXPORT_PDF => {
                    ids.extend(EXPORT_PDF_LINES.iter().filter(|line| **line != SEPARATOR))
                }
                THEME => ids.extend(THEME_IDS),
                NUMFMT => ids.extend(NUMFMT_LINES.iter().filter(|line| **line != SEPARATOR)),
                ALIGN => ids.extend(ALIGN_LINES.iter().filter(|line| **line != SEPARATOR)),
                SLIDE_ALIGN => {
                    ids.extend(SLIDE_ALIGN_LINES.iter().filter(|line| **line != SEPARATOR))
                }
                BORDER => ids.extend(BORDER_LINES.iter().filter(|line| **line != SEPARATOR)),
                CF => ids.extend(cf_ids()),
                SHEET_ROW | SHEET_COL | SHEET_TAB | SHEET_FREEZE => {
                    let sub = sheet_submenu(line).expect("a sheet submenu token");
                    ids.extend(sub.lines.iter().filter(|id| **id != SEPARATOR));
                }
                USE_IN_FORMULA => ids.extend(USE_IN_FORMULA_LINES),
                CALC_OPTIONS => ids.extend(CALC_OPTIONS_LINES),
                QUIT_ID => {}
                line if line == SEPARATOR || line.starts_with('@') => {}
                id => ids.push(id),
            }
        }
    }
    ids
}
