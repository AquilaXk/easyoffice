import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// 삽입 > 페이지 나누기 삽입, 제거 and 모든 페이지 나누기 원래대로 (PD-391): the breaks of the page setup, and the print cut that follows them.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const { __EASYOFFICE_SHEET_PAGE_PRINT__: pagePrint } = loadUiModule(ui("sheet-page-print.js"), {});
const { __EASYOFFICE_SHEET_PAGE_CORE__: pageCore } = loadUiModule(ui("sheet-page.js"), { __EASYOFFICE_SHEET_REFS__: {} });

test("a manual break starts a page even when more lines would fit, and a break outside the range is none", () => {
  const sizes = new Float64Array([10, 10, 10, 10, 10, 10]);
  const spans = (breaks, budget = 100) => pagePrint.cut(sizes, budget, null, breaks).groups.map((g) => [g.first, g.last]);
  assert.deepEqual(spans([]), [[0, 5]]);
  assert.deepEqual(spans([2, 4]), [[0, 1], [2, 3], [4, 5]]);
  assert.deepEqual(spans([3], 25), [[0, 1], [2, 2], [3, 4], [5, 5]]);
  assert.deepEqual(pagePrint.breaksIn([1, 5, 9, 12], 5, 10), [4, 7], "relative to the first line of the range; the first line and the lines past the range are no break");
});

test("the page setup dialog keeps the breaks it does not show", () => {
  const original = { rowBreaks: [11, 21], colBreaks: [4], landscape: true };
  const fields = pageCore.toFields(original);
  const { page } = pageCore.fromFields(fields, original);
  assert.deepEqual(page.rowBreaks, [11, 21]);
  assert.deepEqual(page.colBreaks, [4]);
  assert.equal(pageCore.fromFields(pageCore.toFields({}), {}).page.rowBreaks, undefined);
});

async function openSheet() {
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") return { session: "s1", formulas: [], changed: [], circular: false };
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return harness;
}

test("the three commands set, take away and clear the breaks of the active sheet, each as one undo step", async () => {
  const h = await openSheet();
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const act = async (name) => {
    h.userClick(h.document.querySelector(`.md-item[data-sheet-act="${name}"]`));
    await flushAsyncTurns(15);
  };
  const select = async (text) => {
    h.userClick(h.document.querySelector('.md-item[data-sheet-act="goto"]'));
    await flushAsyncTurns(6);
    h.document.getElementById("sheet-goto-ref").value = text;
    h.userClick(h.document.getElementById("btn-sheet-goto-ok"));
    await flushAsyncTurns(10);
  };
  const sheet = () => Object.keys(layout.getAll())[0];
  const page = () => layout.pageOf(sheet()) ?? {};
  await select("C5");
  await act("break-insert");
  assert.deepEqual([page().rowBreaks, page().colBreaks], [[5], [3]], "a cell breaks above its row and left of its column");
  await select("A8");
  await act("break-insert");
  assert.deepEqual([page().rowBreaks, page().colBreaks], [[5, 8], [3]], "column A has no line to break before");
  await select("A1");
  await act("break-insert");
  assert.equal(h.document.getElementById("status-command-feedback").textContent, "첫 행이나 첫 열 앞에는 쪽을 나눌 수 없음");
  await select("C5");
  await act("break-remove");
  assert.deepEqual([page().rowBreaks, page().colBreaks], [[8], undefined]);
  await act("break-remove");
  assert.equal(h.document.getElementById("status-command-feedback").textContent, "제거할 쪽 나누기가 없음");
  await act("break-reset");
  assert.deepEqual(page(), {}, "nothing is left of the setup");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(15);
  assert.deepEqual(page().rowBreaks, [8], "one undo step brings the breaks back");
});
