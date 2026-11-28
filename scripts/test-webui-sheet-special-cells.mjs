import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// 이동 옵션 (PD-391): which cells of a selection the kinds of the dialog find, in rectangles the multi-range selection takes.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const { __EASYOFFICE_SHEET_SPECIAL_CELLS__: special } = loadUiModule(ui("sheet-special-cells.js"), {});

const MAX_ROWS = 1048576;
const MAX_COLS = 16384;
const inferTag = (raw) => (raw === "TRUE" || raw === "FALSE" ? "b" : raw.startsWith("#") ? "e" : raw !== "" && !Number.isNaN(Number(raw)) ? "n" : "s");

// A sheet from `{ "row,col": entry }`.
function viewOf(cells, { hiddenRows = [], hiddenCols = [], notes = [], arrays = [] } = {}) {
  const at = (row, col) => cells[row + "," + col];
  const used = Object.keys(cells).map((key) => key.split(",").map(Number));
  return {
    maxRows: MAX_ROWS,
    maxCols: MAX_COLS,
    usedRows: Math.max(0, ...used.map(([row]) => row)),
    usedCols: Math.max(0, ...used.map(([, col]) => col)),
    forEachEntry: (visit) => used.forEach(([row, col]) => visit(row, col, at(row, col))),
    forEachNote: (visit) => notes.forEach(([row, col]) => visit(row, col)),
    isFilled: (row, col) => Boolean(at(row, col)?.formula) || (at(row, col)?.value ?? "") !== "",
    hasNote: (row, col) => notes.some(([r, c]) => r === row && c === col),
    rowHidden: (row) => hiddenRows.includes(row),
    colHidden: (col) => hiddenCols.includes(col),
    arrayAt: (row, col) => arrays.find((a) => row >= a.r0 && row <= a.r1 && col >= a.c0 && col <= a.c1) ?? null,
    inferTag,
  };
}

const SHEET = {
  "1,1": { value: "이름" },
  "1,2": { value: "수량" },
  "2,1": { value: "사과" },
  "2,2": { value: "3" },
  "3,1": { value: "배" },
  "3,2": { value: "5" },
  "4,2": { value: "8", formula: "=B2+B3" },
  "5,1": { value: "TRUE" },
  "5,2": { value: "#DIV/0!", formula: "=1/0" },
};

test("constants and formulas are told apart and filtered by the type of their value", () => {
  const view = viewOf(SHEET);
  assert.deepEqual(special.findSpecial(view, { kind: "constants" }).ranges, [
    { r1: 1, c1: 1, r2: 3, c2: 2 },
    { r1: 5, c1: 1, r2: 5, c2: 1 },
  ]);
  assert.deepEqual(special.findSpecial(view, { kind: "constants", types: ["number"] }).ranges, [{ r1: 2, c1: 2, r2: 3, c2: 2 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "constants", types: ["logical"] }).ranges, [{ r1: 5, c1: 1, r2: 5, c2: 1 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "formulas" }).ranges, [{ r1: 4, c1: 2, r2: 5, c2: 2 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "formulas", types: ["error"] }).ranges, [{ r1: 5, c1: 2, r2: 5, c2: 2 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "formulas", types: ["text"] }).ranges, []);
});

test("a selection of several cells limits the search to itself; the cells that touch join into one rectangle", () => {
  const view = viewOf(SHEET);
  const found = special.findSpecial(view, { kind: "constants", scope: [{ r1: 2, c1: 1, r2: 3, c2: 2 }] });
  assert.deepEqual(found.ranges, [{ r1: 2, c1: 1, r2: 3, c2: 2 }]);
});

test("blanks are the empty cells of the used range or the selection", () => {
  const view = viewOf(SHEET);
  assert.deepEqual(special.findSpecial(view, { kind: "blanks" }).ranges, [{ r1: 4, c1: 1, r2: 4, c2: 1 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "blanks", scope: [{ r1: 4, c1: 1, r2: 6, c2: 3 }] }).ranges.length, 1, "only the used range is looked at");
});

test("the region grows over touching filled cells, the last cell is the corner of the used range", () => {
  const view = viewOf({ ...SHEET, "9,9": { value: "x" } });
  assert.deepEqual(special.findSpecial(view, { kind: "region", active: { row: 2, col: 2 } }).ranges, [{ r1: 1, c1: 1, r2: 5, c2: 2 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "region", active: { row: 9, col: 9 } }).ranges, [{ r1: 9, c1: 9, r2: 9, c2: 9 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "last" }).ranges, [{ r1: 9, c1: 9, r2: 9, c2: 9 }]);
});

test("notes come from the notes of the sheet even on empty cells, and an array is the area of the active cell", () => {
  const view = viewOf(SHEET, { notes: [[2, 1], [7, 7]], arrays: [{ r0: 4, c0: 2, r1: 6, c1: 3 }] });
  assert.deepEqual(special.findSpecial(view, { kind: "notes" }).ranges, [{ r1: 2, c1: 1, r2: 2, c2: 1 }, { r1: 7, c1: 7, r2: 7, c2: 7 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "array", active: { row: 5, col: 3 } }).ranges, [{ r1: 4, c1: 2, r2: 6, c2: 3 }]);
  assert.deepEqual(special.findSpecial(view, { kind: "array", active: { row: 1, col: 1 } }).ranges, []);
});

test("the shown cells skip the hidden rows and columns of the selection", () => {
  const view = viewOf({ ...SHEET, "4,3": { value: "z" } }, { hiddenRows: [3], hiddenCols: [2] });
  assert.deepEqual(special.findSpecial(view, { kind: "visible", scope: [{ r1: 1, c1: 1, r2: 4, c2: 3 }] }).ranges, [
    { r1: 1, c1: 1, r2: 2, c2: 1 },
    { r1: 1, c1: 3, r2: 2, c2: 3 },
    { r1: 4, c1: 1, r2: 4, c2: 1 },
    { r1: 4, c1: 3, r2: 4, c2: 3 },
  ]);
});

test("the shown cells of a whole-sheet selection stop at the used range and refuse before the product of runs is made", () => {
  // 4000 used rows with every other one hidden, 40 used columns with every other one hidden: 2000 x 20 runs cross to 40000 ranges.
  const cells = {};
  for (let row = 1; row <= 4000; row += 1) cells[row + ",1"] = { value: "r" };
  cells["1,40"] = { value: "c" };
  const hiddenRows = new Set(Array.from({ length: 2000 }, (_, i) => i * 2 + 2));
  const hiddenCols = new Set(Array.from({ length: 20 }, (_, i) => i * 2 + 2));
  const view = { ...viewOf(cells), rowHidden: (row) => hiddenRows.has(row), colHidden: (col) => hiddenCols.has(col) };
  let walked = 0;
  const counting = { ...view, rowHidden: (row) => (walked += 1, view.rowHidden(row)), colHidden: (col) => (walked += 1, view.colHidden(col)) };
  const started = Date.now();
  const found = special.findSpecial(counting, { kind: "visible", scope: [{ r1: 1, c1: 1, r2: MAX_ROWS, c2: MAX_COLS }] });
  assert.equal(found.error, "ranges");
  assert.ok(walked <= 4100, "only the used lines were looked at, not a million: " + walked);
  assert.ok(Date.now() - started < 1000);
  // a few runs are fine: nothing hidden is one rectangle of the used range
  assert.deepEqual(special.findSpecial(viewOf(SHEET), { kind: "visible", scope: [{ r1: 1, c1: 1, r2: MAX_ROWS, c2: MAX_COLS }] }).ranges, [{ r1: 1, c1: 1, r2: 5, c2: 2 }]);
});

test("a result that needs more ranges than the selection holds is refused, and so is a scope that is too large", () => {
  const checker = {};
  for (let row = 1; row <= 200; row += 2) checker[row + ",1"] = { value: String(row) };
  const found = special.findSpecial(viewOf(checker), { kind: "constants" });
  assert.equal(found.error, "ranges");
  assert.equal(found.count, 100);
  const wide = viewOf({ "1,1": { value: "a" }, [MAX_ROWS + ",1"]: { value: "b" } });
  assert.equal(special.findSpecial(wide, { kind: "blanks" }).error, "scope", "a million empty cells are not walked");
  assert.equal(special.findSpecial(viewOf(SHEET), { kind: "nope" }).error, "kind");
});
