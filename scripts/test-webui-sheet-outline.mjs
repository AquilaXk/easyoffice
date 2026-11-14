import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The rules of row and column groups (PD-208): grouping and ungrouping by level, folding, the
// level buttons, which groups a fold command acts on, and the view in the layout store.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const parseRef = (text) => {
  const m = /^([A-Z]+)(\d+)$/.exec(String(text));
  return m ? { col: [...m[1]].reduce((n, ch) => n * 26 + ch.charCodeAt(0) - 64, 0), row: Number(m[2]) } : null;
};
const refOf = (r, c) => String.fromCharCode(64 + c) + r;
const env = { __EASYOFFICE_SHEET_GRID_UTILS__: { refOf, parseRef } };
loadUiModule(UI("sheet-layout.js"), env);
loadUiModule(UI("sheet-outline.js"), env);
const O = env.__EASYOFFICE_SHEET_OUTLINE__;
const L = env.__EASYOFFICE_SHEET_LAYOUT__;

test("grouping adds a level, splits runs and refuses past level 7", () => {
  let o = O.group({}, "row", 3, 9).outline;
  assert.deepEqual(o.rows, [[3, 9, 1]]);
  o = O.group(o, "row", 5, 6).outline;
  assert.deepEqual(o.rows, [[3, 4, 1], [5, 6, 2], [7, 9, 1]]);
  o = O.group(o, "row", 1, 4).outline;
  assert.deepEqual(o.rows, [[1, 2, 1], [3, 6, 2], [7, 9, 1]]);
  let deep = {};
  for (let i = 0; i < 7; i += 1) deep = O.group(deep, "col", 2, 3).outline;
  assert.deepEqual(deep.cols, [[2, 3, 7]]);
  assert.equal(O.group(deep, "col", 3, 5).error, "level");
  assert.equal(O.group({}, "row", 1, 100001).error, "limit");
});

test("ungrouping takes rows one level out and trims the folds", () => {
  let o = O.group(O.group({}, "row", 3, 9).outline, "row", 5, 6).outline;
  o = O.fold(o, "row", 5, 6, true).outline;
  assert.deepEqual(o.foldedRows, [[5, 6]]);
  o = O.ungroup(o, "row", 5, 6).outline;
  assert.deepEqual(o.rows, [[3, 9, 1]]);
  assert.deepEqual(o.foldedRows, [[5, 6]], "still in a group, still folded");
  o = O.ungroup(o, "row", 3, 9).outline;
  assert.deepEqual(o, {});
  assert.equal(O.ungroup(o, "row", 1, 2).error, "none");
});

test("a group is a stretch of level k or deeper, summed up below or above it", () => {
  const o = { rows: [[3, 4, 1], [5, 6, 2], [7, 9, 1]], cols: [[2, 3, 1]] };
  assert.deepEqual(
    O.groups(o, "row").map((g) => [g.level, g.first, g.last, g.summary]),
    [[1, 3, 9, 10], [2, 5, 6, 7]]
  );
  const above = { ...o, summaryAbove: true };
  assert.deepEqual(O.groups(above, "row").map((g) => g.summary), [2, 4]);
  assert.deepEqual(O.groups(above, "col").map((g) => g.summary), [4], "columns follow summaryLeft");
});

test("fold hides a group, unfold shows it, the level buttons fold by depth", () => {
  const o = { rows: [[3, 4, 1], [5, 6, 2], [7, 9, 1]] };
  const inner = O.fold(o, "row", 7, 7, true);
  assert.deepEqual(inner.outline.foldedRows, [[5, 6]], "the summary row 7 folds the group it sums up");
  const outer = O.fold(o, "row", 10, 10, true).outline;
  assert.deepEqual(outer.foldedRows, [[3, 9]]);
  assert.deepEqual(O.fold(outer, "row", 10, 10, false).outline.foldedRows, undefined);
  assert.equal(O.fold(o, "row", 10, 10, false).error, "none", "nothing to show");
  // Rows inside a group, not its summary: the innermost group holding the selection.
  assert.deepEqual(O.fold(o, "row", 5, 5, true).outline.foldedRows, [[5, 6]]);
  assert.equal(O.fold(o, "row", 1, 1, true).error, "none");
  // Level 1 hides every grouped row, level 2 only the deeper ones, level 3 shows all.
  assert.deepEqual(O.showLevel(o, "row", 1).foldedRows, [[3, 9]]);
  assert.deepEqual(O.showLevel(o, "row", 2).foldedRows, [[5, 6]]);
  assert.equal(O.showLevel(o, "row", 3).foldedRows, undefined);
});

test("clearing the outline keeps the side of the summaries", () => {
  const o = { rows: [[3, 4, 1]], foldedRows: [[3, 4]], summaryAbove: true };
  assert.deepEqual(O.clear(o), { summaryAbove: true });
  assert.deepEqual(O.clear({ rows: [[1, 2, 1]] }), {});
});

test("the layout keeps folded rows apart from hidden ones and in the undo form", () => {
  L.replaceAll({
    S: {
      hiddenRows: [[2, 2]],
      freeze: { rows: 2, cols: 1 },
      tabColor: "3366CC",
      outline: { rows: [[5, 8, 1]], foldedRows: [[5, 8]], cols: [[3, 4, 1]], foldedCols: [[3, 4]] },
    },
  });
  assert.deepEqual(L.freezeOf("S"), { rows: 2, cols: 1 });
  assert.equal(L.tabColorOf("S"), "3366cc");
  assert.equal(L.rowHidden("S", 2) && L.rowHidden("S", 6) && !L.rowHidden("S", 9), true);
  assert.equal(L.rowsHiddenBefore("S", 10), 5);
  assert.equal(L.colHidden("S", 3) && !L.colHidden("S", 5), true);
  assert.deepEqual(L.get("S").hiddenRows, [[2, 2]], "the user's runs stay their own");
  assert.deepEqual(L.hiddenColRuns("S"), [[3, 4]]);
  const saved = L.getAll();
  assert.equal(saved.S.outline.foldedRows[0][1], 8);
  L.setOutline("S", O.clear(L.outlineOf("S")));
  assert.equal(L.rowHidden("S", 6), false);
  L.setFreeze("S", { rows: 0, cols: 0 });
  assert.equal(L.freezeOf("S"), null);
  L.replaceAll(saved);
  assert.equal(L.rowHidden("S", 6), true);
  L.replaceAll({ S: { freeze: { rows: -1, cols: 0 }, tabColor: "red" } });
  assert.equal(L.get("S"), null, "a bad view is dropped");
});

test("folding never shows the rows a filter or the user hid, and unfolding gives back only the group's rows", () => {
  L.replaceAll({ S: { hiddenRows: [[9, 9]], filter: { range: "A1:B20", columns: [], hidden: [[10, 12]] } } });
  const folded = O.fold({ rows: [[8, 14, 1]] }, "row", 15, 15, true).outline;
  L.setOutline("S", folded);
  assert.deepEqual([7, 8, 9, 10, 12, 14, 15].map((r) => L.rowHidden("S", r)), [false, true, true, true, true, true, false]);
  const shown = O.fold(folded, "row", 15, 15, false).outline;
  L.setOutline("S", shown);
  // The user's row 9 and the filter's rows 10-12 are still hidden; the other rows of the group are back.
  assert.deepEqual([7, 8, 9, 10, 12, 13, 14, 15].map((r) => L.rowHidden("S", r)), [false, false, true, true, true, false, false, false]);
  assert.equal(L.rowsHiddenTotal("S"), 4);
  assert.deepEqual(L.get("S").hiddenRows, [[9, 9]]);
  assert.deepEqual(L.get("S").filter.hidden, [[10, 12]]);
});

test("an inner group keeps its own state when the outer group folds and opens again (PD-208)", () => {
  // Rows 3-9 are the outer group, rows 5-6 an inner one; summaries below (rows 10 and 7).
  const base = O.group(O.group({}, "row", 3, 9).outline, "row", 5, 6).outline;
  const outer = O.groups(base, "row").find((g) => g.level === 1);
  const inner = O.groups(base, "row").find((g) => g.level === 2);
  // Fold the inner group, then the outer: both are folded and nothing is left open.
  let o = O.setFolded(base, "row", inner, true);
  assert.deepEqual(o.foldedRows, [[5, 6]]);
  o = O.setFolded(o, "row", outer, true);
  assert.deepEqual(o.foldedRows, [[3, 9]]);
  assert.equal(o.openRows, undefined);
  // Opening the outer one shows its rows but keeps the inner one folded.
  const reopened = O.setFolded(o, "row", outer, false);
  assert.deepEqual(reopened.foldedRows, [[5, 6]]);
  assert.equal(O.isFolded(reopened, "row", inner), true);
  // With the inner group open, folding the outer hides it and opening the outer shows it open.
  const folded = O.setFolded(base, "row", outer, true);
  assert.deepEqual(folded.foldedRows, [[3, 9]]);
  assert.deepEqual(folded.openRows, [[5, 6, 2]], "the inner group is open inside the folded rows");
  assert.equal(O.isFolded(folded, "row", inner), false);
  assert.equal(O.isFolded(folded, "row", outer), true);
  const shown = O.setFolded(folded, "row", outer, false);
  assert.equal(shown.foldedRows, undefined);
  assert.equal(shown.openRows, undefined);
  // The level buttons fold every group from a level down and leave the ones above open.
  const level2 = O.showLevel(base, "row", 2);
  assert.deepEqual(level2.foldedRows, [[5, 6]]);
  assert.equal(level2.openRows, undefined);
  const level1 = O.showLevel(base, "row", 1);
  assert.deepEqual(level1.foldedRows, [[3, 9]]);
  assert.equal(level1.openRows, undefined, "the inner group is folded as well");
  // Ungrouping drops an open mark whose group is gone.
  const gone = O.ungroup(folded, "row", 5, 6).outline;
  assert.equal(gone.openRows, undefined);
  assert.deepEqual(gone.foldedRows, [[3, 9]]);
});
