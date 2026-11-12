import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { readText } from "./test-support/read-text.mjs";
import { fileURLToPath } from "node:url";

// 골라 붙이기 (PD-206): the menu item, the dialog, the modes, 행/열 바꿈, and the paste that is
// refused.

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const text = (d, refs) => refs.map((ref) => cell(d, ref)?.textContent);
const feedback = (d) => d.getElementById("status-command-feedback").textContent;
const dialog = (d) => d.getElementById("sheet-paste-dialog");

const MODEL = {
  sheets: [
    {
      name: "붙여넣기",
      cells: [
        { ref: "A1", raw: "10", style: { bold: true } },
        { ref: "B1", raw: "20" },
        { ref: "C1", raw: "=A1+B1", style: { italic: true } },
        { ref: "A2", raw: "가" },
        { ref: "A3", raw: "다" },
        { ref: "B2", raw: "=A1*2" },
        { ref: "E5", raw: "남음", style: { underline: true } },
        { ref: "F5", raw: "=E5" },
      ],
      columns: [{ index: 1, widthPx: 150 }, { index: 2, widthPx: 60 }],
    },
  ],
};

async function open() {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(h.document, makeFixtureFile("붙여넣기.xlsx", MODEL));
  await flushAsyncTurns(12);
  return h;
}

async function select(d, from, moves = []) {
  const start = cell(d, from);
  start.dispatchEvent({ type: "mousedown", target: start, button: 0, bubbles: true });
  start.dispatchEvent({ type: "mouseup", target: start, bubbles: true });
  for (const [k, n] of moves) for (let i = 0; i < n; i += 1) key(d.activeElement, k, { shiftKey: true });
  await flushAsyncTurns(2);
}

// Copies the selection into the app's own clipboard through the copy event; the system clipboard
// of a test page is empty, so the menu paste reads the in-app copy.
function copy(d) {
  const store = {};
  const target = d.activeElement;
  target.dispatchEvent({ type: "copy", target, clipboardData: { setData: (t, v) => (store[t] = v), getData: (t) => store[t] || "" } });
  return store["text/plain"];
}

async function special(h, mode, { transpose = false } = {}) {
  const { document } = h;
  h.userClick(document.querySelector('.md-item[data-sheet-paste="special"]'));
  const select = document.getElementById("sheet-paste-kind-select");
  select.value = mode;
  document.getElementById("sheet-paste-transpose").checked = transpose;
  h.userClick(document.getElementById("btn-sheet-paste-apply"));
  await flushAsyncTurns(15);
}

test("the edit menu of a sheet window has 골라 붙이기… after 붙이기, other windows do not", async () => {
  const { document } = await open();
  const items = [...document.querySelectorAll('.md-item[data-sheet-paste="special"]')];
  assert.equal(items.length, 1);
  assert.equal(items[0].querySelector(".md-label").textContent, "골라 붙이기…");
  assert.equal(items[0].querySelector(".md-shortcut").textContent, "Ctrl+Alt+V");
  assert.equal(items[0].dataset.kinds, "SHEET");
  const paste = document.querySelector('.md-item[data-studio-menu-cmd="clip-paste"]');
  const menu = [...paste.parentElement.querySelectorAll(".md-item")];
  assert.equal(menu.indexOf(items[0]), menu.indexOf(paste) + 1);
});

test("the dialog is the shared tool dialog: combo, checkbox, focus on 확인", async () => {
  const h = await open();
  const { document } = h;
  h.userClick(document.querySelector('.md-item[data-sheet-paste="special"]'));
  const dlg = dialog(document);
  assert.equal(dlg.open, true);
  assert.ok(dlg.classList.contains("tool-dialog"));
  assert.equal(dlg.querySelector(".dialog-title span").textContent, "골라 붙이기");
  const options = [...dlg.querySelectorAll("#sheet-paste-kind-select option")].map((o) => [o.value, o.textContent]);
  assert.deepEqual(options, [["all", "모두"], ["values", "값"], ["formats", "서식"], ["formulas", "수식"], ["widths", "열 너비"]]);
  assert.equal(dlg.querySelector('label[for="sheet-paste-kind-select"]').textContent, "붙이기");
  assert.equal(dlg.querySelector("#sheet-paste-transpose").parentElement.textContent, "행/열 바꿈");
  assert.equal(dlg.querySelector("#sheet-paste-transpose").checked, false);
  assert.equal(document.activeElement, dlg.querySelector(".dialog-btn-primary"), "the focus starts on 확인");
  assert.equal(dlg.querySelector(".dialog-btn-primary").textContent, "확인");
  assert.equal(dlg.querySelector(".dialog-footer .dialog-btn:not(.dialog-btn-primary)").textContent, "취소");
});

test("Ctrl+Alt+V, the native bar item and the command search open the same dialog", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "A1");
  const a1 = cell(document, "A1");
  key(a1, "v", { ctrlKey: true, altKey: true });
  assert.equal(dialog(document).open, true, "the key");
  dialog(document).close();
  assert.equal(document.defaultView.__EASYOFFICE_NATIVE_MENU__("edit:paste-special"), true);
  assert.equal(dialog(document).open, true, "the native item");
  dialog(document).close();
  h.userClick(document.getElementById("btn-command-palette"));
  const input = document.getElementById("cmd-search-input");
  input.value = "골라 붙이기";
  input.dispatchEvent({ type: "input" });
  const [found] = [...document.querySelectorAll("#cmd-palette-list .cp-item")];
  assert.equal(found.querySelector(".cp-item-label").textContent, "골라 붙이기…");
  assert.equal(found.querySelector(".cp-item-shortcut").textContent, "Ctrl+Alt+V");
  found.click();
  assert.equal(dialog(document).open, true, "the command search");
});

test("Ctrl+V of the app's own copy brings the formats; 모두 does the same", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "A1", [["ArrowRight", 2]]);
  assert.equal(copy(document), "10\t20\t=A1+B1");
  await select(document, "A4");
  const a4 = cell(document, "A4");
  const clip = { getData: (t) => (t === "text/plain" ? "10\t20\t=A1+B1" : ""), setData() {} };
  a4.dispatchEvent({ type: "paste", target: a4, clipboardData: clip });
  await flushAsyncTurns(15);
  assert.deepEqual(text(document, ["A4", "B4", "C4"]), ["10", "20", "30"]);
  assert.equal(cell(document, "C4").dataset.formula, "=A4+B4", "references move by the offset");
  assert.equal(cell(document, "A4").style.fontWeight, "600");
  assert.equal(cell(document, "C4").style.fontStyle, "italic");
  assert.equal(feedback(document), "붙여넣기: 3칸");
  await select(document, "A6");
  await special(h, "all");
  assert.equal(cell(document, "A6").style.fontWeight, "600");
  assert.equal(feedback(document), "붙여넣기: 3칸");
});

test("값 pastes the shown value of a formula, 수식 the formula, 서식 only the format; each is one undo step", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "A1", [["ArrowRight", 2]]);
  copy(document);
  await select(document, "A4");
  await special(h, "values");
  assert.deepEqual(text(document, ["A4", "B4", "C4"]), ["10", "20", "30"]);
  assert.equal(cell(document, "C4").dataset.formula ?? "", "", "no formula");
  assert.equal(cell(document, "A4").style.fontWeight, "", "no format");
  assert.equal(feedback(document), "골라 붙이기: 3칸");
  h.userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(15);
  assert.deepEqual(text(document, ["A4", "B4", "C4"]), ["", "", ""], "one undo step");

  await select(document, "A4");
  await special(h, "formulas");
  assert.equal(cell(document, "C4").dataset.formula, "=A4+B4");
  assert.equal(cell(document, "A4").style.fontWeight, "", "no format");

  await select(document, "A6");
  await special(h, "formats");
  assert.deepEqual(text(document, ["A6", "B6", "C6"]), ["", "", ""], "no content");
  assert.equal(cell(document, "A6").style.fontWeight, "600");
  assert.equal(cell(document, "C6").style.fontStyle, "italic");
  assert.equal(cell(document, "B6").style.fontWeight, "");

  await select(document, "E5");
  await special(h, "formats");
  assert.equal(cell(document, "E5").textContent, "남음", "the content of the target stays");
  assert.equal(cell(document, "E5").style.fontWeight, "600");
  assert.equal(cell(document, "E5").style.textDecoration, "", "the old format is replaced");
});

test("열 너비 pastes the widths of the copied columns and one undo step takes them back", async () => {
  const h = await open();
  const { document } = h;
  const width = (col) => document.querySelector(`#pane-sheet th[data-col="${col}"]`).style.width;
  assert.equal(width("A"), "150px");
  await select(document, "A1", [["ArrowRight", 1]]);
  copy(document);
  await select(document, "D1");
  await special(h, "widths");
  assert.deepEqual([width("D"), width("E")], ["150px", "60px"]);
  assert.equal(feedback(document), "골라 붙이기: 2열");
  assert.deepEqual(text(document, ["D1", "E1"]), ["", ""], "no content");
  h.userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(15);
  assert.deepEqual([width("D"), width("E")], ["96px", "96px"]);
  await select(document, "A1");
  copy(document);
  await select(document, "A3");
  await special(h, "widths");
  assert.equal(width("A"), "150px");
});

test("행/열 바꿈 turns rows and columns over; a formula moves by the distance of its own cell", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "A1", [["ArrowRight", 2], ["ArrowDown", 1]]);
  assert.equal(copy(document), "10\t20\t=A1+B1\n가\t=A1*2\t");
  await select(document, "A8");
  await special(h, "all", { transpose: true });
  assert.deepEqual(text(document, ["A8", "B8", "A9", "B9", "A10", "B10"]), ["10", "가", "20", "20", "30", ""]);
  assert.equal(cell(document, "A10").dataset.formula, "=A8+A9", "=A1+B1 of C1 (now A10): A1 and B1 followed their cells");
  assert.equal(cell(document, "B9").dataset.formula, "=A8*2", "=A1*2 of B2 (now B9)");
  assert.equal(cell(document, "A8").style.fontWeight, "600");
  assert.equal(feedback(document), "골라 붙이기: 6칸");
});

test("text from outside brings content only: 서식 and 열 너비 have nothing to paste", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "A4");
  const a4 = cell(document, "A4");
  const outside = { getData: (t) => (t === "text/plain" ? "x\ty\n1\t2" : ""), setData() {} };
  a4.dispatchEvent({ type: "paste", target: a4, clipboardData: outside });
  await flushAsyncTurns(15);
  assert.deepEqual(text(document, ["A4", "B4", "A5", "B5"]), ["x", "y", "1", "2"]);
  assert.equal(cell(document, "A4").style.fontWeight, "");
});

test("a paste that cuts through a merged range is refused and changes nothing; one that holds it is not", async () => {
  const h = await open();
  const { document } = h;
  const layout = document.defaultView.__EASYOFFICE_SHEET_LAYOUT__;
  layout.addMerge("붙여넣기", 8, 2, 9, 2);
  await select(document, "A1", [["ArrowRight", 1], ["ArrowDown", 1]]);
  copy(document);
  await select(document, "A7");
  await special(h, "all");
  assert.equal(feedback(document), "병합 셀과 겹침", "A7:B8 holds only the first row of B8:B9");
  assert.deepEqual(text(document, ["A7", "B7", "A8"]), ["", "", ""]);
  await select(document, "A8");
  await special(h, "all");
  assert.equal(text(document, ["A8", "B8", "A9"]).join(), "10,20,가", "A8:B9 holds the merged range: it is overwritten");
});

test("rows an auto filter hides are not copied", async () => {
  const h = await open();
  const { document } = h;
  document.defaultView.__EASYOFFICE_SHEET_LAYOUT__.setFilter("붙여넣기", { range: { r1: 1, c1: 1, r2: 2, c2: 3 }, columns: [], hidden: [[2, 2]] });
  await select(document, "A1", [["ArrowDown", 1]]);
  assert.equal(copy(document), "10\n다", "row 2 is filtered out");
  await select(document, "A5");
  await special(h, "all");
  assert.deepEqual(text(document, ["A5", "A6", "A7"]), ["10", "다", ""]);
});

test("one copied cell fills the larger selection it is pasted into, each copy with its own references", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "C1");
  copy(document);
  await select(document, "C3", [["ArrowDown", 1]]);
  await special(h, "formulas");
  assert.equal(cell(document, "C3").dataset.formula, "=A3+B3");
  assert.equal(cell(document, "C4").dataset.formula, "=A4+B4");
});

test("the dialog is 360px wide with the label column of the cell format dialog, and its open list is not cut by the dialog", () => {
  const css = readText(fileURLToPath(new URL("../crates/easyoffice-app/ui/shell.css", import.meta.url)));
  assert.match(css, /#sheet-paste-dialog \{ width: min\(360px, calc\(100vw - 32px\)\); overflow: visible; \}/);
  assert.match(css, /#sheet-numfmt-dialog \.dialog-label, #sheet-paste-dialog \.dialog-label \{ width: 70px;/);
});
