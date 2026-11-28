import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// 편집 > 이동 and 이동 옵션 (PD-391): the reference field, the names list, the options dialog and the keys.

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

const byId = (document, id) => document.getElementById(id);

async function openGoto(h) {
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="goto"]'));
  await flushAsyncTurns(6);
  return byId(h.document, "sheet-goto-dialog");
}

async function goTo(h, text) {
  const dialog = await openGoto(h);
  const field = byId(h.document, "sheet-goto-ref");
  field.value = text;
  h.userClick(byId(h.document, "btn-sheet-goto-ok"));
  await flushAsyncTurns(10);
  return dialog;
}

test("the 편집 menu has 이동… with the Control key shortcut, and the dialog is the names list, the 참조 field and 옵션…", async () => {
  const h = await openSheet();
  const item = h.document.querySelector('.md-item[data-sheet-act="goto"]');
  assert.equal(item.querySelector(".md-label").textContent, "이동…");
  const dialog = await openGoto(h);
  assert.equal(dialog.open, true);
  assert.deepEqual([...dialog.querySelectorAll(".dialog-footer button")].map((b) => b.textContent), ["옵션…", "확인", "취소"]);
  assert.deepEqual([...dialog.querySelectorAll("label.dialog-label")].map((l) => l.textContent), ["참조:"]);
});

test("a reference selects its range and closes; a bad one keeps the dialog open and says so", async () => {
  const h = await openSheet();
  let dialog = await goTo(h, "b2:c3");
  assert.equal(dialog.open, false);
  assert.equal(byId(h.document, "active-cell-ref").textContent, "B2");
  assert.equal(h.document.querySelector('#pane-sheet td[data-cell="C3"]').classList.contains("in-range"), true);
  dialog = await goTo(h, "not a ref!");
  assert.equal(dialog.open, true);
  assert.equal(byId(h.document, "sheet-goto-status").textContent, "참조가 올바르지 않습니다");
  byId(h.document, "sheet-goto-ref").value = "D1,A5";
  h.userClick(byId(h.document, "btn-sheet-goto-ok"));
  await flushAsyncTurns(10);
  assert.equal(dialog.open, false, "ranges apart with commas are taken");
  assert.equal(h.document.querySelector('#pane-sheet td[data-cell="A5"]').classList.contains("in-range"), true);
  assert.equal(byId(h.document, "sheet-goto-ref").value, "D1,A5", "the reference of the last go is shown again");
});

test("the dialog lists the names of the sheet, and a name picked goes to what it refers to", async () => {
  const h = await openSheet();
  const names = h.host.__EASYOFFICE_SHEET_NAMES__;
  const sheet = Object.keys(h.host.__EASYOFFICE_SHEET_LAYOUT__.getAll())[0] ?? "Sheet1";
  names.replaceAll([{ name: "Block", refersTo: "'" + (h.document.querySelector("#sheet-tab-bar .sheet-tab-btn")?.dataset.sheetTab || sheet) + "'!$B$2:$C$3" }]);
  await openGoto(h);
  const items = [...byId(h.document, "sheet-goto-list").children];
  assert.deepEqual(items.map((li) => li.textContent), ["Block"]);
  h.userClick(items[0]);
  assert.equal(byId(h.document, "sheet-goto-ref").value, "Block");
  h.userClick(byId(h.document, "btn-sheet-goto-ok"));
  await flushAsyncTurns(10);
  assert.equal(byId(h.document, "active-cell-ref").textContent, "Block");
});

test("이동 옵션 selects the cells of a kind, says when there are none, and the key opens the dialog", async () => {
  const h = await openSheet();
  await openGoto(h);
  h.userClick(byId(h.document, "btn-sheet-goto-special"));
  await flushAsyncTurns(6);
  const special = byId(h.document, "sheet-special-dialog");
  assert.equal(special.open, true);
  assert.deepEqual([...special.querySelectorAll('input[name="sheet-special-kind"]')].map((r) => r.value), ["notes", "constants", "formulas", "blanks", "region", "array", "last", "visible"]);
  const kind = (value) => special.querySelector(`input[name="sheet-special-kind"][value="${value}"]`);
  kind("notes").checked = true;
  kind("constants").checked = false;
  h.userClick(byId(h.document, "btn-sheet-special-ok"));
  await flushAsyncTurns(10);
  assert.equal(special.open, true, "no note, the dialog stays");
  assert.equal(byId(h.document, "sheet-special-status").textContent, "해당 셀을 찾을 수 없습니다");
  kind("notes").checked = false;
  kind("last").checked = true;
  h.userClick(byId(h.document, "btn-sheet-special-ok"));
  await flushAsyncTurns(10);
  assert.equal(special.open, false, byId(h.document, "sheet-special-status").textContent);
  assert.equal(byId(h.document, "sheet-goto-dialog").open, false, "a search that selected closes 이동 too");
  assert.equal(byId(h.document, "active-cell-ref").textContent, "H8", "the last cell of the used range is the active cell");

  h.sandbox.dispatchWindowKeydown({ key: "g", ctrlKey: true });
  await flushAsyncTurns(6);
  assert.equal(byId(h.document, "sheet-goto-dialog").open, true, "Ctrl+G opens 이동");
});
