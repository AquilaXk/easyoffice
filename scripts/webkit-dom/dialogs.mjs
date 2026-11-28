import { analyzeDialog, measureDialog } from "./dialog-layout.mjs";
import { paste } from "./support.mjs";
import { KNOWN_DIALOG_VIOLATIONS } from "./dialog-known.mjs";

// The dialog layout steps of the WebKit suite (docs/design-system.md, "대화상자 부품 키트"): each step opens one dialog (or one
// tab of it) through the menu command a user would use, measures it (dialog-layout.mjs) and fails with the dialog, the
// element and the measured values. What a dialog already breaks is listed per step in dialog-known.mjs; that list only shrinks.
//
// A dialog that needs a selection or an object gets it in its opening steps (a pasted table, a pasted picture, a made table).
// The slide, PDF and base windows have their own lists. The base import dialog (it opens after a file is chosen), the
// slide and PDF dialogs that need a loaded file, and the pivot and chart sub-dialogs are not covered yet.

const click = (selector) => (page) => page.evaluate((target) => document.querySelector(target).click(), selector);
const choose = (selector, value) => (page) => page.evaluate(([target, picked]) => {
  const select = document.querySelector(target);
  select.value = picked;
  select.dispatchEvent(new Event("change", { bubbles: true }));
}, [selector, value]);

/** Three numbers in A1:A3, so the sort dialog has rows to sort. */
async function typeColumn(page) {
  await page.click('#pane-sheet td[data-cell="A1"]');
  for (const value of ["3", "1", "2"]) {
    await page.keyboard.type(value);
    await page.keyboard.press("Enter");
  }
  await page.click('#pane-sheet td[data-cell="A1"]');
}

const wait = (selector) => (page) => page.waitForSelector(selector);
/** A real pointer click, for the buttons that a script `click()` does not reach (the table tab's). */
const press = (selector) => (page) => page.click(selector);
/** The filter panel of A1 (a column with a header), then its "조건 필터…" row. */
const CUSTOM_FILTER = [typeColumn, click('[data-sheet-act="filter-toggle"]'), (page) => page.keyboard.press("Alt+ArrowDown"), wait("[data-filter-act=custom]"), click("[data-filter-act=custom]")];
/** A table over A1:A3 (the create dialog confirmed), so the table tab and its slicer and style dialogs exist. */
const MAKE_TABLE = [typeColumn, click('[data-studio-menu-cmd="insert-table"]'), wait("#sheet-table-dialog[open]"), click("#btn-sheet-table-ok")];
/** A 1x1 picture in the sheet, opened for its size and alternative text. */
async function openPicture(page) {
  await page.evaluate(async () => {
    const png = Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="), (c) => c.charCodeAt(0));
    const media = window.__EASYOFFICE_SHEET_MEDIA__;
    const res = await media.insertPictureFromBytes(png, "image/png");
    media.openImageDialog(res.id ?? res.image?.id ?? media.getSelectedImageId());
  });
}

const CELL_FORMAT = click('[data-sheet-numfmt="dialog"]');
const PAGE_SETUP = click('[data-sheet-act="page-setup"]');

/** `[step name, dialog id, ways to open it]`. */
const SHEET_DIALOGS = [
  ["셀 서식 표시 형식", "sheet-numfmt-dialog", [CELL_FORMAT]],
  ["셀 서식 맞춤", "sheet-numfmt-dialog", [CELL_FORMAT, click('[data-fmt-tab="align"]')]],
  ["셀 서식 글꼴", "sheet-numfmt-dialog", [CELL_FORMAT, click('[data-fmt-tab="font"]')]],
  ["셀 서식 테두리", "sheet-numfmt-dialog", [CELL_FORMAT, click('[data-fmt-tab="border"]')]],
  ["셀 서식 채우기", "sheet-numfmt-dialog", [CELL_FORMAT, click('[data-fmt-tab="fill"]')]],
  ["셀 서식 보호", "sheet-numfmt-dialog", [CELL_FORMAT, click('[data-fmt-tab="protect"]')]],
  ["데이터 유효성 검사 설정", "sheet-dv-dialog", [click('[data-studio-menu-cmd="data-validation"]')]],
  ["데이터 유효성 검사 설명 메시지", "sheet-dv-dialog", [click('[data-studio-menu-cmd="data-validation"]'), click('[data-dv-tab="prompt"]')]],
  ["데이터 유효성 검사 오류 메시지", "sheet-dv-dialog", [click('[data-studio-menu-cmd="data-validation"]'), click('[data-dv-tab="error"]')]],
  ["조건부 서식", "sheet-cf-dialog", [click('[data-studio-menu-cmd="sheet-mod-condfmt"]')]],
  ["페이지 설정 페이지", "sheet-page-dialog", [PAGE_SETUP]],
  ["페이지 설정 여백", "sheet-page-dialog", [PAGE_SETUP, click('[data-sp-tab="margin"]')]],
  ["페이지 설정 시트", "sheet-page-dialog", [PAGE_SETUP, click('[data-sp-tab="sheet"]')]],
  ["찾기 및 바꾸기", "find-dialog", [click('[data-studio-menu-cmd="replace"]')]],
  ["행 높이", "sheet-size-dialog", [click('[data-sheet-act="row-height"]')]],
  ["열 너비", "sheet-size-dialog", [click('[data-sheet-act="col-width"]')]],
  ["시트 보호", "sheet-protect-dialog", [click('[data-sheet-act="sheet-protect"]')]],
  ["통합 문서 보호", "sheet-wb-protect-dialog", [click('[data-sheet-act="workbook-protect"]')]],
  ["정렬", "sheet-sort-dialog", [typeColumn, click('[data-sheet-act="sort-dialog"]')]],
  ["선택하여 붙여넣기", "sheet-paste-dialog", [click('[data-sheet-paste="special"]')]],
  ["이름 관리자", "sheet-names-dialog", [click('[data-sheet-act="name-manage"]')]],
  ["이동", "sheet-goto-dialog", [click('[data-sheet-act="goto"]')]],
  ["이동 옵션", "sheet-special-dialog", [click('[data-sheet-act="goto"]'), click("#btn-sheet-goto-special")]],
  ["부분합", "sheet-subtotal-dialog", [typeColumn, click('[data-sheet-act="subtotal"]')]],
  ["중복된 항목 제거", "sheet-table-dedupe-dialog", [typeColumn, click('[data-sheet-act="dedupe-dialog"]')]],
  ["텍스트 나누기", "sheet-text-columns-dialog", [typeColumn, click('[data-sheet-act="text-to-columns"]')]],
  ["피벗 테이블", "sheet-pivot-dialog", [typeColumn, click('[data-studio-menu-cmd="sheet-mod-pivot"]')]],
  ["차트", "sheet-chart-dialog", [typeColumn, click('[data-studio-menu-cmd="insert-chart"]')]],
  ["하이퍼링크", "sheet-link-dialog", [click('[data-studio-menu-cmd="insert-link"]')]],
  ["메모", "sheet-note-dialog", [click('[data-sheet-act="note-insert"]')]],
  ["이름 정의", "sheet-name-edit-dialog", [click('[data-sheet-act="name-define"]')]],
  ["선택 영역에서 이름 만들기", "sheet-name-create-dialog", [typeColumn, (page) => page.click('#pane-sheet td[data-cell="A3"]', { modifiers: ["Shift"] }), click('[data-sheet-act="name-create"]')]],
  ["시트 이동/복사", "sheet-tab-move-dialog", [click('[data-sheet-act="tab-move-copy"]')]],
  ["조건 필터", "sheet-filter-dialog", CUSTOM_FILTER],
  ["표 만들기", "sheet-table-dialog", [typeColumn, click('[data-studio-menu-cmd="insert-table"]')]],
  ["슬라이서", "sheet-slicer-dialog", [...MAKE_TABLE, press('[data-table-act="slicer"]')]],
  ["새 표 스타일", "sheet-table-style-dialog", [...MAKE_TABLE, press("#btn-table-style"), press("[data-table-style-new]")]],
  ["그림", "sheet-image-dialog", [openPicture]],
];

/** Some text with a selection in the document, so the font dialog opens on a run. */
async function selectDocText(page) {
  await page.click("#doc-editor p");
  await page.keyboard.type("가나다라 AaBb");
  await page.keyboard.press("Shift+Home");
}

const FONT_DIALOG = click('[data-studio-menu-cmd="font-dialog"]');
const DOC_MENU = (cmd) => click(`[data-studio-menu-cmd="${cmd}"]`);

/** A two-by-two table with the caret in its first cell, so the table dialogs have a table to act on. */
async function pasteTable(page) {
  await page.click("#doc-editor p");
  await page.keyboard.type("앞");
  await paste(page, { "text/html": "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>", "text/plain": "a\tb\nc\td" });
  await page.waitForSelector("#doc-editor table td");
  await page.click("#doc-editor table tr:first-child td:first-child");
}

/** A 1x1 image in the document, opened for its size through a double click. */
async function pasteImage(page) {
  await page.click("#doc-editor p");
  await page.keyboard.type("앞");
  const gif1x1 = "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
  await paste(page, { "text/html": `<p><img src="data:image/gif;base64,${gif1x1}"></p>`, "text/plain": "" });
  await page.waitForSelector("#doc-editor img.doc-inline-image");
  await page.evaluate(() => document.querySelector("#doc-editor img.doc-inline-image").dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));
}

const DOC_DIALOGS = [
  ["글꼴 글꼴 탭", "doc-font-dialog", [selectDocText, FONT_DIALOG]],
  ["글꼴 고급 탭", "doc-font-dialog", [selectDocText, FONT_DIALOG, click("#doc-font-tab-advanced")]],
  ["용지 설정", "doc-page-setup-dialog", [click('[data-studio-menu-cmd="layout-paper"]')]],
  ["찾기 및 바꾸기", "find-dialog", [click('[data-studio-menu-cmd="replace"]')]],
  ["목차", "doc-toc-dialog", [click('[data-studio-menu-cmd="doc-mod-toc"]')]],
  ["문서 통계", "doc-stats-dialog", [click('[data-studio-menu-cmd="review-stats"]')]],
  ["기호", "symbol-picker-dialog", [click('[data-studio-menu-cmd="mod-symbol"]')]],
  ["단락 줄 간격", "doc-paragraph-dialog", [choose("#doc-linespacing-select", "options")]],
  ["편지 병합", "doc-mailmerge-dialog", [click('[data-studio-menu-cmd="doc-mod-mailmerge"]')]],
  ["메모", "doc-comment-dialog", [click('[data-studio-menu-cmd="review-comment"]')]],
  ["차트", "doc-chart-dialog", [click('[data-studio-menu-cmd="insert-chart"]')]],
  ["하이퍼링크", "doc-link-dialog", [selectDocText, DOC_MENU("insert-link")]],
  ["표 속성", "doc-table-props-dialog", [pasteTable, DOC_MENU("table-cell-props")]],
  ["셀 테두리", "doc-cell-border-dialog", [pasteTable, DOC_MENU("table-border-bg")]],
  ["셀 배경", "doc-cell-border-dialog", [pasteTable, DOC_MENU("table-border-bg"), click('[data-doc-cb-tab="bg"]')]],
  ["그림 크기", "doc-image-size-dialog", [pasteImage]],
];

/** Violations the step may still have (`KNOWN_DIALOG_VIOLATIONS`), minus what the browser's fonts decide: a clipped box is listed but its absence is not an error. */
export function judge(stepName, found, known = KNOWN_DIALOG_VIOLATIONS[stepName] ?? []) {
  const keys = found.map((v) => v.key);
  const added = found.filter((v) => !known.includes(v.key));
  const stale = known.filter((key) => !keys.includes(key) && !key.includes(" clip "));
  const lines = [
    ...added.map((v) => `  ${v.message}`),
    ...stale.map((key) => `  fixed, remove from dialog-known.mjs: ${key}`),
  ];
  if (lines.length) throw new Error(`${stepName}: ${added.length} new, ${stale.length} fixed\n${lines.join("\n")}`);
}

function layoutStep([name, dialogId, opens]) {
  return [`dialog layout: ${name}`, async (page) => {
    for (const open of opens) await open(page);
    await page.waitForSelector(`#${dialogId}[open]`);
    judge(name, analyzeDialog(await measureDialog(page, dialogId)));
  }];
}

/** A slide dialog through the opener the double click and the ribbon use (`slide-dialogs.js`), with the element it edits. */
const slideDialog = (opener, element) => (page) => page.evaluate(([name, spec]) => {
  const canvas = window.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.dialogs[name](spec ? canvas.addElement(spec) : null);
}, [opener, element]);
const PNG_1X1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

const SLIDE_DIALOGS = [
  ["마스터 레이아웃", "slide-layout-dialog", [click('[data-studio-menu-cmd="slide-mod-master"]')]],
  ["도형", "slide-shape-dialog", [slideDialog("openShapeDialog")]],
  ["표", "slide-table-dialog", [slideDialog("openTableDialog")]],
  ["차트", "slide-chart-dialog", [slideDialog("openChartDialog")]],
  ["그림", "slide-image-dialog", [slideDialog("openImageDialog", { kind: "image", x: 100, y: 100, w: 240, h: 160, data: PNG_1X1, mime: "image/png" })]],
];

const PDF_DIALOGS = [
  ["형광펜·밑줄 주석", "pdf-highlight-dialog", [click('[data-studio-menu-cmd="pdf-mod-highlight"]')]],
  ["페이지 분할·병합", "pdf-splitmerge-dialog", [(page) => page.evaluate(() => document.getElementById("pdf-splitmerge-dialog").showModal())]],
  ["문서 구조", "outline-dialog", [click('[data-studio-menu-cmd="view-outline"]')]],
  ["개인정보 지우기", "pii-dialog", [click('[data-studio-menu-cmd="scrub-pii"]')]],
];

const BASE_DIALOGS = [
  ["필드 추가", "base-field-dialog", [press("#btn-base-add-field")]],
  ["필드 설정", "base-field-dialog", [press(".base-field-edit-btn")]],
];

export const sheetDialogSteps = SHEET_DIALOGS.map(layoutStep);
export const docDialogSteps = DOC_DIALOGS.map(layoutStep);
export const slideDialogSteps = SLIDE_DIALOGS.map(layoutStep);
export const pdfDialogSteps = PDF_DIALOGS.map(layoutStep);
export const baseDialogSteps = BASE_DIALOGS.map(layoutStep);
