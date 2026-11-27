import { analyzeDialog, measureDialog } from "./dialog-layout.mjs";
import { KNOWN_DIALOG_VIOLATIONS } from "./dialog-known.mjs";

// The dialog layout steps of the WebKit suite (docs/design-system.md, "대화상자 부품 키트"): each step opens one dialog (or one
// tab of it) through the menu command a user would use, measures it (dialog-layout.mjs) and fails with the dialog, the
// element and the measured values. What a dialog already breaks is listed per step in dialog-known.mjs; that list only shrinks.
//
// Not covered yet (TODO.md): the hyperlink, table properties and image dialogs (they need a selection first), and the dialogs of the
// slide, PDF and base windows.

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
  ["데이터 유효성 검사", "sheet-dv-dialog", [click('[data-studio-menu-cmd="data-validation"]')]],
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
  ["이름 관리", "sheet-names-dialog", [click('[data-sheet-act="name-manage"]')]],
  ["중복된 항목 제거", "sheet-table-dedupe-dialog", [typeColumn, click('[data-sheet-act="dedupe-dialog"]')]],
  ["텍스트 나누기", "sheet-text-columns-dialog", [typeColumn, click('[data-sheet-act="text-to-columns"]')]],
  ["피벗 테이블", "sheet-pivot-dialog", [typeColumn, click('[data-studio-menu-cmd="sheet-mod-pivot"]')]],
  ["차트", "sheet-chart-dialog", [typeColumn, click('[data-studio-menu-cmd="insert-chart"]')]],
  ["하이퍼링크", "sheet-link-dialog", [click('[data-studio-menu-cmd="insert-link"]')]],
  ["메모", "sheet-note-dialog", [click('[data-sheet-act="note-insert"]')]],
];

/** Some text with a selection in the document, so the font dialog opens on a run. */
async function selectDocText(page) {
  await page.click("#doc-editor p");
  await page.keyboard.type("가나다라 AaBb");
  await page.keyboard.press("Shift+Home");
}

const FONT_DIALOG = click('[data-studio-menu-cmd="font-dialog"]');

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

export const sheetDialogSteps = SHEET_DIALOGS.map(layoutStep);
export const docDialogSteps = DOC_DIALOGS.map(layoutStep);
