import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
for (const name of ["sheet-paste-core.js"]) loadUiModule(ui(name), env);
const paste = env.__EASYOFFICE_SHEET_PASTE_CORE__;

const cellOf = (v, extra = {}) => ({ value: v, formula: "", ...extra });
const refOf = (r, c) => String.fromCodePoint(64 + c) + r;
const shift = (f, dr, dc) => f + "@" + dr + "," + dc;

// Paste special (PD-206): which cell writes a copied block makes in each mode.
const GRID = { MAX_ROWS: 1048576, MAX_COLS: 16384, DEFAULT_COL_W: 96, refOf };
const bold = Object.freeze({ fontWeight: "600", fontStyle: "", textDecoration: "", fontSize: "" });

function snap(rows, c1, cells, widths = []) {
  return { rows, c1, cols: cells[0].length, cells, widths };
}

function plan(block, mode, extra = {}) {
  return paste.planPaste({
    snap: block,
    mode,
    transpose: false,
    target: { r1: 5, c1: 5, r2: 5, c2: 5 },
    getEntry: () => undefined,
    shiftFormula: shift,
    isFormulaStr: (t) => t.startsWith("="),
    grid: GRID,
    ...extra,
  });
}

const BLOCK = snap([2, 3], 1, [
  [cellOf("10", { style: bold }), { value: "30", formula: "=A2+B2", style: undefined }],
  [null, cellOf("텍스트")],
], [120, 0]);

test("paste modes write the parts of the copied cells", () => {
  const all = plan(BLOCK, "all").edits;
  assert.deepEqual(all.map((e) => [e.ref, e.value, e.formula, e.style]), [
    ["E5", "10", "", bold],
    ["F5", "", "=A2+B2@3,4", null],
    ["E6", "", "", null],
    ["F6", "텍스트", "", null],
  ]);
  const vals = plan(BLOCK, "values").edits;
  assert.deepEqual(vals.map((e) => [e.value, e.formula, "style" in e]), [["10", "", false], ["30", "", false], ["", "", false], ["텍스트", "", false]]);
  const formulas = plan(BLOCK, "formulas").edits;
  assert.deepEqual(formulas.map((e) => [e.value, e.formula, "style" in e]), [["10", "", false], ["", "=A2+B2@3,4", false], ["", "", false], ["텍스트", "", false]]);
  const formats = plan(BLOCK, "formats", { getEntry: (ref) => (ref === "E5" ? { value: "keep", formula: "" } : undefined) }).edits;
  assert.deepEqual(formats.map((e) => [e.ref, e.value, e.style]), [["E5", "keep", bold], ["F5", "", null], ["E6", "", null], ["F6", "", null]]);
  assert.equal(formats[0].style, bold, "a frozen shared style stays shared");
});

test("column widths paste the source widths, a default width resets the target column", () => {
  const out = plan(BLOCK, "widths");
  assert.deepEqual(out.edits, []);
  assert.deepEqual(out.widths, { 5: 120, 6: 96 });
  assert.deepEqual(plan(BLOCK, "widths", { target: { r1: 1, c1: 16384, r2: 1, c2: 16384 } }).widths, { 16384: 120 }, "columns past the grid are dropped");
});

test("rows and columns turn over; each formula moves by the distance of its own cell", () => {
  const t = plan(BLOCK, "all", { transpose: true });
  assert.deepEqual(t.edits.map((e) => [e.ref, e.value || e.formula]), [
    ["E5", "10"],
    ["F5", ""],
    ["E6", "=A2+B2@4,3"],
    ["F6", "텍스트"],
  ]);
  const wide = snap([1], 1, [[cellOf("a"), cellOf("b"), cellOf("c")]]);
  const tall = plan(wide, "values", { transpose: true });
  assert.deepEqual(tall.edits.map((e) => [e.ref, e.value]), [["E5", "a"], ["E6", "b"], ["E7", "c"]]);
  const col = plan(snap([1, 2, 3], 1, [[cellOf("x")], [cellOf("y")], [cellOf("z")]]), "values", { transpose: true });
  assert.deepEqual(col.edits.map((e) => [e.ref, e.value]), [["E5", "x"], ["F5", "y"], ["G5", "z"]]);
  assert.deepEqual(paste.transposed(wide), [[wide.cells[0][0]], [wide.cells[0][1]], [wide.cells[0][2]]]);
});

test("a turned-over paste tells the formula which cell it came from and went to", () => {
  const seen = [];
  plan(BLOCK, "all", { transpose: true, shiftFormula: (f, dr, dc, turn) => (seen.push(turn), f) });
  assert.deepEqual(seen, [{ fromRow: 2, fromCol: 2, toRow: 6, toCol: 5 }], "the formula at row 2, column 2 lands at E6 (the block is pasted at E5)");
  let straight = "unset";
  plan(BLOCK, "all", { shiftFormula: (f, dr, dc, turn) => ((straight = turn), f) });
  assert.equal(straight, undefined, "a plain paste moves references by the distance only");
});

test("one copied cell fills a larger selection, each copy measured from the source", () => {
  const one = snap([2], 3, [[{ value: "", formula: "=A1" }]]);
  const out = plan(one, "all", { target: { r1: 5, c1: 5, r2: 6, c2: 6 } }).edits;
  assert.deepEqual(out.map((e) => [e.ref, e.formula]), [["E5", "=A1@3,2"], ["F5", "=A1@3,3"], ["E6", "=A1@4,2"], ["F6", "=A1@4,3"]]);
  const flipped = plan(one, "all", { transpose: true, target: { r1: 5, c1: 5, r2: 6, c2: 6 } }).edits;
  assert.equal(flipped.length, 4);
  assert.ok(flipped.every((e) => !e.formula.includes("undefined") && !e.formula.includes("NaN")));
});

test("text from outside brings content only, and has no formats or widths to paste", () => {
  const text = { rows: null, c1: 0, cols: 2, cells: [[cellOf("=1+1"), cellOf("b")]], widths: [] };
  assert.deepEqual(plan(text, "all").edits.map((e) => [e.value, e.formula, "style" in e]), [["", "=1+1", false], ["b", "", false]]);
  assert.deepEqual(plan(text, "values").edits.map((e) => [e.value, e.formula]), [["=1+1", ""], ["b", ""]]);
  assert.deepEqual(plan(text, "formats"), { edits: [], widths: null, clipped: 0 });
  assert.deepEqual(plan(text, "widths"), { edits: [], widths: null, clipped: 0 });
});

test("cells past the last row or column are counted, not written", () => {
  const out = plan(BLOCK, "values", { target: { r1: 1048576, c1: 16384, r2: 1048576, c2: 16384 } });
  assert.equal(out.edits.length, 1);
  assert.equal(out.clipped, 3);
});
