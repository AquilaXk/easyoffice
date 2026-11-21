import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// A copied block repeats over a selection that is a whole multiple of it (rows and columns both),
// otherwise it is pasted once at the top left, and the merges of the block travel with it.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(ui("sheet-paste-core.js"), env);
const paste = env.__EASYOFFICE_SHEET_PASTE_CORE__;

const refOf = (r, c) => String.fromCodePoint(64 + c) + r;
const GRID = { MAX_ROWS: 1048576, MAX_COLS: 16384, DEFAULT_COL_W: 96, refOf };
const cellOf = (v) => ({ value: v, formula: "" });
const block = (rows, extra = {}) => ({ rows: rows.map((_, i) => i + 1), c1: 1, cols: rows[0].length, cells: rows.map((r) => r.map(cellOf)), widths: [], ...extra });
const plan = (snap, target, extra = {}) =>
  paste.planPaste({ snap, mode: "all", transpose: false, target, getEntry: () => undefined, shiftFormula: (f) => f, isFormulaStr: () => false, grid: GRID, ...extra });
const refs = (out) => out.edits.map((e) => e.ref + "=" + e.value);

test("a block repeats over a selection that is a whole multiple of it", () => {
  const two = block([["a", "b"], ["c", "d"]]);
  const out = plan(two, { r1: 1, c1: 1, r2: 4, c2: 6 });
  assert.equal(out.edits.length, 24, "2 x 3 copies of four cells");
  assert.deepEqual(refs(out).slice(0, 4), ["A1=a", "B1=b", "A2=c", "B2=d"]);
  assert.ok(refs(out).includes("E3=a") && refs(out).includes("F4=d"));
  assert.deepEqual(paste.areaOf(two, false, { r1: 1, c1: 1, r2: 4, c2: 6 }), { r1: 1, c1: 1, r2: 4, c2: 6 });
});

test("a selection that is not a whole multiple takes one copy at its top left", () => {
  const two = block([["a", "b"], ["c", "d"]]);
  assert.equal(plan(two, { r1: 2, c1: 2, r2: 4, c2: 4 }).edits.length, 4, "3 x 3 is no multiple of 2 x 2");
  assert.equal(plan(two, { r1: 2, c1: 2, r2: 5, c2: 5 }).edits.length, 16, "4 x 4 is");
  const row = block([["x", "y", "z"]]);
  assert.equal(plan(row, { r1: 1, c1: 1, r2: 5, c2: 3 }).edits.length, 15, "one row down five rows");
  assert.equal(plan(row, { r1: 1, c1: 1, r2: 5, c2: 4 }).edits.length, 3, "the columns do not divide: one copy, not a partial repeat");
  assert.deepEqual(paste.areaOf(row, false, { r1: 1, c1: 1, r2: 5, c2: 4 }), { r1: 1, c1: 1, r2: 1, c2: 3 });
  assert.equal(plan(two, { r1: 1, c1: 1, r2: 1, c2: 1 }).edits.length, 4, "a selection smaller than the block takes the block");
});

test("each copy moves the references of a copied formula by its own distance", () => {
  const f = { rows: [3], c1: 2, cols: 1, cells: [[{ value: "", formula: "=B3" }]], widths: [] };
  const out = plan(f, { r1: 1, c1: 1, r2: 3, c2: 1 }, { shiftFormula: (formula, dr, dc) => formula + "@" + dr + "," + dc });
  assert.deepEqual(out.edits.map((e) => [e.ref, e.formula]), [["A1", "=B3@-2,-1"], ["A2", "=B3@-1,-1"], ["A3", "=B3@0,-1"]]);
});

test("the merges of the block are written once per copy, turned over with the block, and only for the modes that carry formats", () => {
  const merged = block([["a", ""], ["", ""]], { merges: [{ r1: 0, c1: 0, r2: 0, c2: 1 }] });
  assert.equal(paste.planMerges({ snap: { ...merged, rows: null }, mode: "all", transpose: false, target: { r1: 1, c1: 1, r2: 1, c2: 1 }, grid: GRID }), null, "text from outside");
  const planned = paste.planMerges({ snap: merged, mode: "all", transpose: false, target: { r1: 1, c1: 1, r2: 4, c2: 2 }, grid: GRID });
  assert.deepEqual(planned.add, [{ r1: 1, c1: 1, r2: 1, c2: 2 }, { r1: 3, c1: 1, r2: 3, c2: 2 }]);
  assert.deepEqual(planned.clear, { r1: 1, c1: 1, r2: 4, c2: 2 });
  const turned = paste.planMerges({ snap: merged, mode: "formats", transpose: true, target: { r1: 5, c1: 5, r2: 5, c2: 5 }, grid: GRID });
  assert.deepEqual(turned.add, [{ r1: 5, c1: 5, r2: 6, c2: 5 }]);
  for (const mode of ["values", "formulas", "widths"]) {
    assert.equal(paste.planMerges({ snap: merged, mode, transpose: false, target: { r1: 1, c1: 1, r2: 1, c2: 1 }, grid: GRID }), null, mode);
  }
  const edge = paste.planMerges({ snap: merged, mode: "all", transpose: false, target: { r1: 1, c1: 16383, r2: 1, c2: 16383 }, grid: GRID });
  assert.deepEqual(edge.add, [{ r1: 1, c1: 16383, r2: 1, c2: 16384 }]);
  const past = paste.planMerges({ snap: merged, mode: "all", transpose: false, target: { r1: 1, c1: 16384, r2: 1, c2: 16384 }, grid: GRID });
  assert.deepEqual(past.add, [], "a merge that leaves the grid is not written");
});

// ---- the page ------------------------------------------------------------------------------------

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const MODEL = { sheets: [{ name: "되풀이", cells: [{ ref: "A1", raw: "가" }, { ref: "B1", raw: "나" }, { ref: "A2", raw: "다" }, { ref: "B2", raw: "라" }] }] };

async function open() {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(h.document, makeFixtureFile("되풀이.xlsx", MODEL));
  await flushAsyncTurns(12);
  return h;
}

async function select(d, from, moves = []) {
  const start = cell(d, from);
  start.dispatchEvent({ type: "mousedown", target: start, button: 0, bubbles: true });
  start.dispatchEvent({ type: "mouseup", target: start, bubbles: true });
  for (const [k, n] of moves) for (let i = 0; i < n; i += 1) d.activeElement.dispatchEvent({ type: "keydown", key: k, target: d.activeElement, shiftKey: true });
  await flushAsyncTurns(2);
}

function copy(d) {
  const store = {};
  const target = d.activeElement;
  target.dispatchEvent({ type: "copy", target, clipboardData: { setData: (t, v) => (store[t] = v), getData: (t) => store[t] || "" } });
  return store["text/plain"];
}

async function pasteText(d, text) {
  const target = d.activeElement;
  target.dispatchEvent({ type: "paste", target, clipboardData: { getData: (t) => (t === "text/plain" ? text : ""), setData() {} } });
  await flushAsyncTurns(15);
}

test("Ctrl+V of a 2 x 2 copy into a 4 x 4 selection repeats it four times, one undo step takes it back", async () => {
  const h = await open();
  const d = h.document;
  await select(d, "A1", [["ArrowRight", 1], ["ArrowDown", 1]]);
  const text = copy(d);
  await select(d, "D1", [["ArrowRight", 3], ["ArrowDown", 3]]);
  await pasteText(d, text);
  const shown = (refs2) => refs2.map((r) => cell(d, r)?.textContent).join("");
  assert.equal(shown(["D1", "E1", "D2", "E2"]), "가나다라");
  assert.equal(shown(["F1", "G1", "F2", "G2"]), "가나다라");
  assert.equal(shown(["D3", "E3", "D4", "E4"]), "가나다라");
  assert.equal(shown(["F3", "G3", "F4", "G4"]), "가나다라");
  assert.equal(d.getElementById("status-command-feedback").textContent, "붙여넣기: 16칸");
  h.userClick(studioItem(d, "studio-undo"));
  await flushAsyncTurns(10);
  assert.equal(shown(["D1", "E1", "F3", "G4"]), "", "one undo step takes all four copies back");
});

test("a merged range copied from the sheet is merged where it is pasted; text from outside leaves a merge as it is", async () => {
  const h = await open();
  const d = h.document;
  const layout = d.defaultView.__EASYOFFICE_SHEET_LAYOUT__;
  layout.addMerge("되풀이", 1, 1, 1, 2);
  await select(d, "A1", [["ArrowRight", 1], ["ArrowDown", 1]]);
  const text = copy(d);
  await select(d, "D5");
  await pasteText(d, text);
  assert.deepEqual(layout.mergesIn("되풀이", 5, 4, 6, 5), [{ r1: 5, c1: 4, r2: 5, c2: 5 }]);
  assert.deepEqual(layout.mergesIn("되풀이", 1, 1, 2, 2), [{ r1: 1, c1: 1, r2: 1, c2: 2 }], "the source keeps its merge");
  h.userClick(studioItem(d, "studio-undo"));
  await flushAsyncTurns(10);
  assert.deepEqual(layout.mergesIn("되풀이", 5, 4, 6, 5), [], "undo takes the pasted merge back with the cells");
  await select(d, "A1", [["ArrowRight", 1]]);
  await pasteText(d, "새로");
  assert.equal(cell(d, "A1").textContent, "새로");
  assert.deepEqual(layout.mergesIn("되풀이", 1, 1, 1, 2), [{ r1: 1, c1: 1, r2: 1, c2: 2 }], "text from outside is no format: the merge stays");
});
