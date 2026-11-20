import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// A pivot value field that holds an error shows that error in Sum, Average, Min and Max, and Count counts it
// (the same rule as the engine), and the page writes the error literal into the cell.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("sheet-rules.js"), env);
loadUiModule(UI("sheet-pivot-model.js"), env);
const model = env.__EASYOFFICE_SHEET_PIVOT_MODEL__;

const rows = [
  ["a", { kind: "number", num: 10, display: "10" }],
  ["a", { kind: "error", display: "#DIV/0!" }],
  ["b", { kind: "number", num: 5, display: "5" }],
  ["b", { kind: "text", text: "text", display: "text" }],
];
const reader = (r, c) => {
  if (r === 0) return { kind: "text", text: c === 0 ? "Key" : "Val", display: c === 0 ? "Key" : "Val" };
  const [key, value] = rows[r - 1];
  return c === 0 ? { kind: "text", text: key, display: key } : value;
};
const pivot = (agg) =>
  model.computePivotWithReader({ range: "A1:B5", header: true, row: 0, value: 1, agg }, reader);

test("Sum, Average, Min and Max answer the error of the group, its row total and the grand total", () => {
  for (const agg of ["sum", "average", "min", "max"]) {
    const p = pivot(agg);
    const a = p.rowKeys.indexOf("a");
    const b = p.rowKeys.indexOf("b");
    assert.equal(p.matrix[a][0], "#DIV/0!", agg);
    assert.equal(p.rowTotals[a], "#DIV/0!", agg);
    assert.equal(typeof p.matrix[b][0], "number", agg);
    assert.equal(p.grandTotal, "#DIV/0!", agg);
  }
});

test("Count counts the error and the text", () => {
  const p = pivot("count");
  assert.deepEqual(p.matrix.map((r) => r[0]), [2, 2]);
  assert.equal(p.grandTotal, 4);
});

test("the sheet cells of the answer hold the error literal", () => {
  const built = model.buildPivotSheetCells(pivot("sum"), {
    fieldNames: ["Key", "Val"],
    rowFields: [0],
    colFields: [],
    filterFields: [],
    valueFields: [{ col: 1, agg: "sum", name: "합계: Val" }],
    filters: [],
  });
  assert.ok(Object.values(built.cells).some((c) => c.value === "#DIV/0!"), JSON.stringify(built.cells));
});
