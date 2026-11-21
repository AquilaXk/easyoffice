import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { openDemoWorkbook } from "./test-support/demo-fixtures.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The edit box of a cell grows with its text (PD-280): right over the cells beside it, down once the view's edge is
// reached, down for a wrapped cell, and it goes back to the cell when the edit ends. The cases of the pure rule follow the
// reference's own sizing (`cell-editor-resize.service.ts`, `editor.js`).

const env = {};
loadUiModule(fileURLToPath(new URL("../crates/easyoffice-app/ui/sheet-edit-box.js", import.meta.url)), env);
const { fitEditBox, GAP, BORDER } = env.__EASYOFFICE_SHEET_EDIT_BOX__;

const base = { cellW: 96, cellH: 24, naturalW: 50, availW: 600, wrap: false, heightAt: () => 999 };

test("text that fits the cell keeps the cell as it is", () => {
  assert.deepEqual(fitEditBox(base), { grow: false, wrap: false, w: 96, h: 24 });
  assert.equal(fitEditBox({ ...base, naturalW: 96 - GAP }).grow, false, "the gap after the last glyph counts");
});

test("text longer than the cell widens the box to the text and a gap, up to the room to the right", () => {
  assert.deepEqual(fitEditBox({ ...base, naturalW: 200 }), { grow: true, wrap: false, w: 200 + GAP, h: 24 });
  assert.deepEqual(fitEditBox({ ...base, naturalW: 595 - BORDER * 2 - GAP }), { grow: true, wrap: false, w: 595 - BORDER * 2, h: 24 });
});

test("text past the room to the right wraps and the box grows down by the lines (x-spreadsheet resetTextareaSize)", () => {
  const box = fitEditBox({ ...base, naturalW: 1500, heightAt: (w) => Math.ceil(1500 / (w - 12)) * 23 + 1 });
  assert.equal(box.grow && box.wrap, true);
  assert.equal(box.w, 600 - BORDER * 2);
  assert.equal(box.h, Math.ceil(1500 / (box.w - 12)) * 23 + 1);
});

test("the box is never narrower than the cell, even where the cell is nearer to the edge than the text", () => {
  const box = fitEditBox({ ...base, cellW: 300, naturalW: 400, availW: 120, heightAt: () => 47 });
  assert.deepEqual(box, { grow: true, wrap: true, w: 300, h: 47 });
});

test("a wrapped cell keeps its width and grows down only when its lines need more than the cell", () => {
  assert.equal(fitEditBox({ ...base, wrap: true, naturalW: 900, heightAt: () => 24 }).grow, false);
  assert.deepEqual(fitEditBox({ ...base, wrap: true, naturalW: 900, heightAt: () => 70.2 }), { grow: true, wrap: true, w: 96, h: 71 });
});

// A page without layout: the cell is measured by a model of it. One glyph is 8px, a cell has 12px of padding and a line is 23px.
const GLYPH = 8;
const PAD = 12;
const LINE = 23;

function lay(td, { cellW = 96, cellH = 24 } = {}) {
  const var_ = (name) => td.style.getPropertyValue(name);
  const textW = () => td.textContent.length * GLYPH + PAD;
  const width = () => {
    if (td.classList.contains("edit-measure")) return textW();
    return td.classList.contains("edit-grow") ? Number.parseFloat(var_("--ew")) : cellW;
  };
  td.getBoundingClientRect = () => ({ left: 0, right: cellW, width: cellW, top: 0, height: cellH });
  Object.defineProperty(td, "offsetWidth", { configurable: true, get: width });
  Object.defineProperty(td, "offsetHeight", {
    configurable: true,
    get: () => {
      if (var_("--eh")) return Number.parseFloat(var_("--eh"));
      if (!td.classList.contains("edit-wrap")) return cellH;
      return Math.max(1, Math.ceil((td.textContent.length * GLYPH) / (width() - PAD))) * LINE + 1;
    },
  });
}

async function openSheet() {
  const h = await createWorkspaceHarness();
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  // The view is 640px wide and the cells start at its left edge.
  h.document.getElementById("sheet-viewport").getBoundingClientRect = () => ({ left: 0, right: 640, width: 640, top: 0, height: 360 });
  return h;
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const type = (td, text) => {
  td.dispatchEvent({ type: "keydown", key: text[0], target: td });
  td.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: text[0], target: td });
  td.textContent = text;
  td.dispatchEvent({ type: "input", inputType: "insertText", data: text, target: td });
};
const vars = (td) => ["--ew", "--eh", "--ch"].map((name) => td.style.getPropertyValue(name));

test("typing past the cell widens the cell being edited over its neighbours, whether they are empty or not", async () => {
  const { document } = await openSheet();
  for (const ref of ["H12", "A3"]) {
    const td = cell(document, ref);
    lay(td);
    td.focus();
    type(td, "짧다");
    assert.equal(td.classList.contains("edit-grow"), false, "a short text stays in the cell");
    type(td, "열두 글자를 넘는 긴 문장을 입력한다");
    assert.equal(td.classList.contains("edit-grow"), true, ref);
    assert.deepEqual(vars(td), [td.textContent.length * GLYPH + PAD + GAP + "px", "24px", "24px"], "the text, its padding and the gap; the cell's own height is kept");
    assert.equal(td.classList.contains("edit-wrap"), false);
  }
});

test("the box follows the text back: deleting it shrinks the box, and a text that fits releases it", async () => {
  const { document } = await openSheet();
  const td = cell(document, "H12");
  lay(td);
  td.focus();
  type(td, "가".repeat(30));
  const wide = Number.parseFloat(vars(td)[0]);
  type(td, "가".repeat(20));
  assert.ok(Number.parseFloat(vars(td)[0]) < wide);
  type(td, "가나다");
  assert.equal(td.classList.contains("edit-grow"), false);
  assert.deepEqual(vars(td), ["", "", ""]);
});

test("a text longer than the room to the right of the cell wraps in a box as wide as that room and grows down", async () => {
  const { document } = await openSheet();
  const td = cell(document, "H12");
  lay(td);
  td.focus();
  // The harness view is 640px wide from the cell's left edge (0): the room is 640 less the borders.
  type(td, "가".repeat(200));
  assert.equal(td.classList.contains("edit-wrap"), true);
  assert.equal(vars(td)[0], 640 - BORDER * 2 + "px");
  const lines = Math.ceil((200 * GLYPH) / (640 - BORDER * 2 - PAD));
  assert.equal(vars(td)[1], lines * LINE + 1 + "px");
  assert.equal(vars(td)[2], "24px", "the row keeps the height of the cell");
});

test("a wrapping cell in a row that follows its content is left to the row, which grows with the text", async () => {
  const { document } = await openSheet();
  const td = cell(document, "H12");
  lay(td);
  td.focus();
  td._st = { wrap: true };
  type(td, "가".repeat(30));
  assert.equal(td.classList.contains("edit-grow"), false);
});

test("a wrapping cell in a row whose height a user set keeps its width and grows down over the rows below", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_LAYOUT__.setRowHeights("Sheet1", new Map([[12, 18]]));
  const td = cell(document, "H12");
  lay(td);
  td.focus();
  td._st = { wrap: true };
  type(td, "가".repeat(30));
  assert.equal(td.classList.contains("edit-wrap"), true);
  assert.equal(vars(td)[0], "96px");
  assert.equal(vars(td)[1], Math.ceil((30 * GLYPH) / (96 - PAD)) * LINE + 1 + "px");
  type(td, "가나");
  assert.equal(td.classList.contains("edit-grow"), false, "two glyphs fit on the first line");
});

test("Enter, Esc and moving away give the cell back, and a merged range is not grown", async () => {
  const { document } = await openSheet();
  const key = (el, k) => el.dispatchEvent({ type: "keydown", key: k, target: el });
  const a = cell(document, "H12");
  lay(a);
  a.focus();
  type(a, "가".repeat(30));
  assert.equal(a.classList.contains("edit-grow"), true);
  key(a, "Escape");
  assert.equal(a.classList.contains("edit-grow"), false, "Esc");
  assert.deepEqual(vars(a), ["", "", ""]);
  type(a, "가".repeat(30));
  key(a, "Enter");
  await flushAsyncTurns();
  assert.equal(a.classList.contains("edit-grow"), false, "Enter");
  const m = cell(document, "F12");
  lay(m);
  m.focus();
  m.classList.add("merge-anchor");
  type(m, "가".repeat(30));
  assert.equal(m.classList.contains("edit-grow"), false, "a merged range edits inside its own box");
});

test("a page without layout leaves the cell as it is", async () => {
  const { document } = await openSheet();
  const td = cell(document, "H12");
  td.focus();
  type(td, "가".repeat(50));
  assert.equal(td.classList.contains("edit-grow"), false);
});
