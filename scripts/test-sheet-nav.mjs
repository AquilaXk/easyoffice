import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The movement rules of the sheet selection (`sheet-nav.js`, PD-280): Ctrl+Arrow over runs and gaps, Enter and Tab inside a
// range, the Enter after a row of Tabs, Home, Ctrl+End and the page keys, with hidden rows and columns and merged ranges.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(ui("sheet-nav.js"), env);
const nav = env.__EASYOFFICE_SHEET_NAV__;

const A = (ref) => ({ r: Number(ref.slice(1)), c: ref.charCodeAt(0) - 64 });
const ref = ({ r, c }) => String.fromCharCode(64 + c) + r;

// A sheet of 100 rows and 20 columns whose cells with a value, merges ("B3:B4"), hidden rows and columns are given.
function sheet({ cells = [], merges = [], hiddenRows = [], hiddenCols = [] } = {}) {
  const valued = new Set(cells);
  const ranges = merges.map((m) => {
    const [a, b] = m.split(":").map(A);
    return { r1: a.r, c1: a.c, r2: b.r, c2: b.c };
  });
  const all = cells.map(A);
  return {
    maxRows: 100,
    maxCols: 20,
    dataRows: Math.max(0, ...all.map((p) => p.r)),
    dataCols: Math.max(0, ...all.map((p) => p.c)),
    hasValue: (r, c) => valued.has(ref({ r, c })),
    mergeAt: (r, c) => ranges.find((m) => r >= m.r1 && r <= m.r2 && c >= m.c1 && c <= m.c2) || null,
    rowHidden: (r) => hiddenRows.includes(r),
    colHidden: (c) => hiddenCols.includes(c),
  };
}

const jump = (view, from, dr, dc) => ref(nav.jump(view, A(from), dr, dc));
const COLUMN = sheet({ cells: ["B2", "B3", "B4", "B7", "B8"] });

test("Ctrl+Down from a run goes to its last cell, and from there to the next run, then to the last row", () => {
  assert.equal(jump(COLUMN, "B2", 1, 0), "B4");
  assert.equal(jump(COLUMN, "B3", 1, 0), "B4");
  assert.equal(jump(COLUMN, "B4", 1, 0), "B7");
  assert.equal(jump(COLUMN, "B7", 1, 0), "B8");
  assert.equal(jump(COLUMN, "B8", 1, 0), "B100");
});

test("Ctrl+Down from an empty cell goes to the next value, or to the last row when there is none", () => {
  assert.equal(jump(COLUMN, "B1", 1, 0), "B2");
  assert.equal(jump(COLUMN, "B5", 1, 0), "B7");
  assert.equal(jump(COLUMN, "B6", 1, 0), "B7");
  assert.equal(jump(COLUMN, "D5", 1, 0), "D100");
  assert.equal(jump(COLUMN, "B60", 1, 0), "B100");
});

test("Ctrl+Up goes the same way back, and from far below the data it comes back to the last used row", () => {
  assert.equal(jump(COLUMN, "B8", -1, 0), "B7");
  assert.equal(jump(COLUMN, "B7", -1, 0), "B4");
  assert.equal(jump(COLUMN, "B4", -1, 0), "B2");
  assert.equal(jump(COLUMN, "B2", -1, 0), "B1");
  assert.equal(jump(COLUMN, "B50", -1, 0), "B8");
  assert.equal(jump(COLUMN, "D50", -1, 0), "D1");
  assert.equal(jump(COLUMN, "B1", -1, 0), "B1", "the first row has nowhere to go");
});

test("Ctrl+Right and Ctrl+Left walk a row", () => {
  const row = sheet({ cells: ["B2", "C2", "D2", "G2"] });
  assert.equal(jump(row, "B2", 0, 1), "D2");
  assert.equal(jump(row, "D2", 0, 1), "G2");
  assert.equal(jump(row, "G2", 0, 1), "T2");
  assert.equal(jump(row, "G2", 0, -1), "D2");
  assert.equal(jump(row, "B2", 0, -1), "A2");
  assert.equal(jump(row, "S2", 0, -1), "G2");
});

test("a hidden row is not a cell of the walk: the run goes on over it and an empty one over it is not a gap", () => {
  assert.equal(jump(sheet({ cells: ["B2", "B4"], hiddenRows: [3] }), "B2", 1, 0), "B4");
  assert.equal(jump(sheet({ cells: ["B2", "B3", "B6"], hiddenRows: [4, 5] }), "B3", 1, 0), "B6");
  assert.equal(jump(sheet({ cells: ["B2", "B6"], hiddenRows: [3, 4] }), "B2", 1, 0), "B6");
  assert.equal(jump(sheet({ cells: ["B2", "C2", "F2"], hiddenCols: [3] }), "B2", 0, 1), "F2");
});

test("a merged range is one cell with the value of its top-left cell, left past its far edge", () => {
  const view = sheet({ cells: ["B2", "B3", "B6"], merges: ["B3:B4"] });
  assert.equal(jump(view, "B2", 1, 0), "B4", "the run goes through the merged range");
  assert.equal(jump(view, "B4", -1, 0), "B2");
  assert.equal(jump(view, "B3", 1, 0), "B6", "from the merged range the far edge is where the walk starts");
  assert.equal(jump(sheet({ cells: ["B2"], merges: ["B3:C5"] }), "B2", 1, 0), "B100", "an empty merged range is a gap");
});

test("an arrow step leaves a merged range past its far edge and over hidden rows and columns, and stops at the sheet edge", () => {
  const view = sheet({ merges: ["B3:C4"], hiddenRows: [6], hiddenCols: [5] });
  const step = (from, dr, dc) => ref(nav.step(view, A(from), dr, dc));
  assert.equal(step("B3", 1, 0), "B5");
  assert.equal(step("B3", 0, 1), "D3");
  assert.equal(step("B5", -1, 0), "B3", "stepping into a merged range selects its anchor");
  assert.equal(step("D4", 0, -1), "B3");
  assert.equal(step("B5", 1, 0), "B7", "row 6 is hidden");
  assert.equal(step("D1", 0, 1), "F1", "column E is hidden");
  assert.equal(step("A1", -1, 0), "A1");
  assert.equal(step("A1", 0, -1), "A1");
  assert.equal(ref(nav.step(view, { r: 100, c: 20 }, 1, 0)), "T100");
});

test("Enter in a range goes down it, wraps to the next column, and from the last cell back to the first", () => {
  const view = sheet();
  const range = { r1: 2, c1: 2, r2: 3, c2: 3 };
  const enter = (from, dr = 1) => ref(nav.cycle(view, range, A(from), dr, 0));
  assert.deepEqual(["B2", "B3", "C2", "C3"].map((from) => enter(from)), ["B3", "C2", "C3", "B2"]);
  assert.deepEqual(["B2", "C3", "C2", "B3"].map((from) => enter(from, -1)), ["C3", "C2", "B3", "B2"]);
});

test("Tab in a range goes along it, wraps to the next row, and Shift+Tab goes back", () => {
  const view = sheet();
  const range = { r1: 2, c1: 2, r2: 3, c2: 4 };
  const tab = (from, dc = 1) => ref(nav.cycle(view, range, A(from), 0, dc));
  assert.deepEqual(["B2", "C2", "D2", "B3", "C3", "D3"].map((from) => tab(from)), ["C2", "D2", "B3", "C3", "D3", "B2"]);
  assert.deepEqual(["B2", "D3", "B3"].map((from) => tab(from, -1)), ["D3", "C3", "D2"]);
});

test("a one-column range is walked down and a one-row range along, wrapping to the start", () => {
  const view = sheet();
  assert.deepEqual(["B2", "B3", "B4"].map((from) => ref(nav.cycle(view, { r1: 2, c1: 2, r2: 4, c2: 2 }, A(from), 1, 0))), ["B3", "B4", "B2"]);
  assert.deepEqual(["B2", "C2", "D2"].map((from) => ref(nav.cycle(view, { r1: 2, c1: 2, r2: 2, c2: 4 }, A(from), 1, 0))), ["C2", "D2", "B2"], "Enter goes right in a range of one row");
});

test("the walk through a range skips hidden rows and columns and leaves a merged range past its far edge", () => {
  const hidden = sheet({ hiddenRows: [3], hiddenCols: [3] });
  const range = { r1: 2, c1: 2, r2: 4, c2: 4 };
  assert.equal(ref(nav.cycle(hidden, range, A("B2"), 1, 0)), "B4", "row 3 is hidden");
  assert.equal(ref(nav.cycle(hidden, range, A("B4"), 1, 0)), "D2", "column C is hidden");
  assert.equal(ref(nav.cycle(hidden, range, A("D4"), 1, 0)), "B2");
  const merged = sheet({ merges: ["B2:B3"] });
  assert.equal(ref(nav.cycle(merged, range, A("B2"), 1, 0)), "B4");
  assert.equal(ref(nav.cycle(merged, range, A("B4"), -1, 0)), "B2", "the anchor of the merged range");
});

test("the Enter after a row of Tabs goes below the cell the row started in", () => {
  const view = sheet();
  assert.equal(ref(nav.afterTabs(view, A("B2"), 1)), "B3");
  assert.equal(ref(nav.afterTabs(view, A("B2"), -1)), "B1");
  assert.equal(ref(nav.afterTabs(sheet({ merges: ["B2:B3"] }), A("B2"), 1)), "B4");
});

test("PageDown and PageUp move a screen of shown rows, Alt+Page a screen of columns, and stop at the edge", () => {
  const view = sheet({ hiddenRows: [4, 5], hiddenCols: [3] });
  const page = (from, dr, dc, span) => ref(nav.page(view, A(from), dr, dc, span));
  assert.equal(page("B2", 1, 0, 5), "B9", "rows 4 and 5 are not counted");
  assert.equal(page("B9", -1, 0, 5), "B2");
  assert.equal(page("B2", -1, 0, 5), "B1");
  assert.equal(page("B98", 1, 0, 5), "B100");
  assert.equal(page("B2", 0, 1, 3), "F2", "column C is not counted");
  assert.equal(page("F2", 0, -1, 9), "A2");
});

test("Home goes to the first shown column, Ctrl+Home to the first shown cell, Ctrl+End to the last used cell", () => {
  const view = sheet({ cells: ["C2", "F9"], hiddenRows: [1], hiddenCols: [1] });
  assert.equal(ref(nav.home(view, A("E5"), false)), "B5");
  assert.equal(ref(nav.home(view, A("E5"), true)), "B2");
  assert.equal(ref(nav.end(view)), "F9");
  assert.equal(ref(nav.end(sheet())), "A1", "an empty sheet ends at A1");
});

test("a sheet with nothing in it: Ctrl+Down goes to the last row, Ctrl+Up to the first, without visiting the empty cells", () => {
  let reads = 0;
  const view = { ...sheet(), hasValue: () => { reads += 1; return false; } };
  assert.equal(jump(view, "C50", 1, 0), "C100");
  assert.equal(jump(view, "C50", -1, 0), "C1");
  assert.equal(jump(view, "T50", 0, 1), "T50");
  assert.ok(reads < 10, `the empty rows were read ${reads} times`);
});
