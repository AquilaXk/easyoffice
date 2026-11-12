import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The rules of row and column groups (PD-208): grouping and ungrouping by level, folding, the
// level buttons, which groups a fold command acts on, and the view in the layout store.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = { __EASYOFFICE_SHEET_GRID_UTILS__: { refOf: (r, c) => "A" + r + c, parseRef: () => null } };
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
