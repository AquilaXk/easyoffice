import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// 편집 > 지우기 (모두·서식·내용·메모·하이퍼링크) and 수식 > 수식 표시 (Ctrl+`) of the sheet window (PD-400).

// `values`: the answers the engine gives for formula cells, by cell.
async function openSheet(values = {}) {
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const action = JSON.parse(args.payload).sessionAction;
        if (action === "structure") return { session: "s1", formulas: [], changed: [], circular: false, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules: {} };
        return { session: "s1", changed: Object.entries(values).map(([cell, value]) => ({ sheet: "Sheet1", cell, value })), circular: false };
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return harness;
}

async function type(h, ref, text) {
  const td = h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  assert.ok(td, "the cell " + ref + " is in the window");
  td.focus();
  for (const ch of text) {
    const active = h.document.activeElement;
    active.dispatchEvent({ type: "keydown", key: ch, target: active });
    active.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: active });
    active.textContent += ch;
    active.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: active });
  }
  td.dispatchEvent({ type: "keydown", key: "Tab", target: td });
  await flushAsyncTurns(15);
}

const get = (h, id) => h.document.getElementById(id);
const cellAt = (h, ref) => h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const act = async (h, name) => {
  h.userClick(h.document.querySelector(`.md-item[data-sheet-act="${name}"]`));
  await flushAsyncTurns(25);
};
const select = async (h, text) => {
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="goto"]'));
  await flushAsyncTurns(6);
  get(h, "sheet-goto-ref").value = text;
  h.userClick(get(h, "btn-sheet-goto-ok"));
  await flushAsyncTurns(10);
};

test("the 지우기 list has the five items in the spreadsheet program's order", async () => {
  const h = await openSheet();
  const sub = h.document.querySelectorAll(".md-sub").find((el) => el.querySelector(":scope > .md-label")?.textContent === "지우기");
  assert.deepEqual(
    sub.querySelectorAll(".md-item").map((el) => el.querySelector(".md-label").textContent),
    ["모두 지우기", "서식 지우기", "내용 지우기", "메모 지우기", "하이퍼링크 지우기"],
  );
});

test("서식 지우기 takes the format and keeps the text, 내용 지우기 keeps the format, 모두 지우기 takes everything, each in one undo step", async () => {
  const h = await openSheet();
  await type(h, "A1", "alpha");
  await type(h, "B1", "beta");
  cellAt(h, "A1").style.fontWeight = "700";
  cellAt(h, "B1").style.fontWeight = "700";
  await select(h, "A1:B1");

  await act(h, "clear-contents");
  assert.equal(cellAt(h, "A1").textContent, "", "the text is gone");
  assert.equal(cellAt(h, "A1").style.fontWeight, "700", "the format stays");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(25);
  assert.equal(cellAt(h, "A1").textContent, "alpha");

  await act(h, "clear-formats");
  assert.equal(cellAt(h, "A1").textContent, "alpha", "the text stays");
  assert.equal(cellAt(h, "A1").style.fontWeight, "");
  assert.equal(cellAt(h, "B1").style.fontWeight, "");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(25);
  assert.equal(cellAt(h, "B1").style.fontWeight, "700", "one undo brings both formats back");

  await act(h, "clear-all");
  assert.equal(cellAt(h, "A1").textContent, "");
  assert.equal(cellAt(h, "B1").style.fontWeight, "");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(25);
  assert.equal(cellAt(h, "B1").textContent, "beta");
  assert.equal(cellAt(h, "B1").style.fontWeight, "700");
});

test("메모 지우기 and 하이퍼링크 지우기 take only the notes and the links of the selection", async () => {
  const h = await openSheet();
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const sheet = h.document.querySelector(".sheet-tab.active, .sheet-tab[aria-selected='true']")?.textContent || "Sheet1";
  await type(h, "A1", "text");
  await type(h, "A2", "https://example.com/page");
  layout.setNotes(sheet, [{ ref: "A1", author: "a", text: "inside", visible: false }, { ref: "C5", author: "a", text: "outside", visible: false }]);
  await select(h, "A1:A3");
  assert.ok(cellAt(h, "A2").dataset.link, "the typed address became a link");
  await act(h, "clear-links");
  assert.equal(cellAt(h, "A2").dataset.link, undefined, "the link is gone");
  assert.equal(cellAt(h, "A2").textContent, "https://example.com/page", "the text stays");
  await act(h, "clear-notes");
  assert.deepEqual(layout.notesOf(sheet).map((n) => n.ref), ["C5"], "the note outside the selection stays");
  await act(h, "clear-notes");
  assert.equal(get(h, "status-command-feedback").textContent, "지울 내용이 없습니다");
});

test("수식 표시 draws the formula in place of the value, keeps the value for the model, and is one setting of the sheet", async () => {
  const h = await openSheet({ A1: "3" });
  await type(h, "A1", "=1+2");
  const td = cellAt(h, "A1");
  assert.equal(td.dataset.formula, "=1+2");
  assert.notEqual(td.textContent, "=1+2", "the value is shown first");
  const item = h.document.querySelector('.md-item[data-sheet-act="view-formulas"]');
  assert.equal(item.getAttribute("aria-checked"), "false");
  await act(h, "view-formulas");
  assert.equal(cellAt(h, "A1").textContent, "=1+2");
  assert.equal(item.getAttribute("aria-checked"), "true");
  assert.equal(cellAt(h, "A1").classList.contains("num"), false);
  assert.equal(h.host.__EASYOFFICE_SHEET_GRID_TEXT__.readTdValue(cellAt(h, "A1")), td.dataset.raw ?? "", "the value is what the model reads");
  await act(h, "view-formulas");
  assert.notEqual(cellAt(h, "A1").textContent, "=1+2", "the value is back");
  assert.equal(item.getAttribute("aria-checked"), "false");
  assert.equal(h.host.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS.find((b) => b.action === "sheet-view-formulas").keys, "Ctrl+`");
});

test("서식 복사 copies the format of the selection to the next one and keeps the text there; Esc drops it; Ctrl+Shift+V pastes", async () => {
  const h = await openSheet();
  await type(h, "A1", "src");
  await type(h, "C3", "keep");
  await type(h, "D3", "also");
  await select(h, "A1");
  h.userClick(studioItem(h.document, "home-bold"));
  await flushAsyncTurns(25);
  assert.notEqual(cellAt(h, "A1").style.fontWeight, "", "the source cell is bold");
  const bold = cellAt(h, "A1").style.fontWeight;
  const brush = h.document.querySelector('.tb-btn[data-sheet-act="format-painter"]');
  assert.ok(brush, "the brush is in the toolbar");
  h.userClick(brush);
  await flushAsyncTurns(10);
  assert.equal(brush.getAttribute("aria-pressed"), "true");
  assert.equal(get(h, "status-command-feedback").textContent, "서식을 적용할 셀을 선택하세요");
  await select(h, "C3");
  get(h, "sheet-viewport").dispatchEvent({ type: "mouseup", target: get(h, "sheet-viewport") });
  await flushAsyncTurns(25);
  assert.equal(cellAt(h, "C3").style.fontWeight, bold, "the format is on the next selection");
  assert.equal(cellAt(h, "C3").textContent, "keep", "the text stays");
  assert.equal(cellAt(h, "A1").textContent, "src");
  assert.equal(brush.getAttribute("aria-pressed"), "false", "it is used once");
  assert.equal(cellAt(h, "D3").style.fontWeight, "");

  // Esc drops it before it is used.
  await select(h, "A1");
  h.userClick(brush);
  await flushAsyncTurns(10);
  h.document.dispatchEvent({ type: "keydown", key: "Escape", target: h.document.body });
  await flushAsyncTurns(5);
  assert.equal(brush.getAttribute("aria-pressed"), "false");
  assert.equal(get(h, "status-command-feedback").textContent, "서식 복사를 취소했습니다");

  // Ctrl+Shift+V pastes the copied format on the selection without the brush.
  await select(h, "A1");
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="format-painter"]'));
  await flushAsyncTurns(10);
  h.document.dispatchEvent({ type: "keydown", key: "Escape", target: h.document.body });
  await select(h, "D3");
  cellAt(h, "D3").dispatchEvent({ type: "keydown", key: "V", code: "KeyV", ctrlKey: true, shiftKey: true, target: cellAt(h, "D3") });
  await flushAsyncTurns(25);
  assert.equal(cellAt(h, "D3").style.fontWeight, bold);
  assert.equal(cellAt(h, "D3").textContent, "also");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(25);
  assert.equal(cellAt(h, "D3").style.fontWeight, "", "one undo step");
});

test("Ctrl+` switches the formula view, and a cell typed in while it is on is shown as a formula", async () => {
  const h = await openSheet({ A1: "6", B1: "7" });
  await type(h, "A1", "=2*3");
  cellAt(h, "A1").focus();
  const key = { type: "keydown", key: "`", code: "Backquote", ctrlKey: true, target: cellAt(h, "A1") };
  cellAt(h, "A1").dispatchEvent(key);
  await flushAsyncTurns(15);
  assert.equal(cellAt(h, "A1").textContent, "=2*3");
  await type(h, "B1", "=A1+1");
  assert.equal(cellAt(h, "B1").textContent, "=A1+1");
  cellAt(h, "B1").dispatchEvent({ ...key, target: cellAt(h, "B1") });
  await flushAsyncTurns(15);
  assert.notEqual(cellAt(h, "B1").textContent, "=A1+1");
});
