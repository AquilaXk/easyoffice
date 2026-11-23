import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("sheet-utils.js"), env);
loadUiModule(UI("sheet-rules.js"), env);
loadUiModule(UI("sheet-refs.js"), env);
loadUiModule(UI("sheet-table-ops.js"), env);
loadUiModule(UI("sheet-pivot-model.js"), env);

const cellType = env.__EASYOFFICE_SHEET_CELL_TYPE__;
const tableOps = env.__EASYOFFICE_SHEET_TABLE_OPS__;
const pivotModel = env.__EASYOFFICE_SHEET_PIVOT_MODEL__;

test("sheet-utils inferTag treats #CONNECT! and #BLOCKED! as error literals", () => {
  assert.equal(cellType.inferTag("#CONNECT!"), "e");
  assert.equal(cellType.inferTag("#BLOCKED!"), "e");
  assert.equal(cellType.inferTag("#connect!"), "s");
  assert.equal(cellType.inferTag("#BLOCKED! "), "s");
});

test("sheet-table-ops dedupe distinguishes #CONNECT!/#BLOCKED! error cells from explicit text", () => {
  assert.deepEqual(tableOps.cellDedupeKey({ value: "#CONNECT!" }), [4, "#CONNECT!"]);
  assert.deepEqual(tableOps.cellDedupeKey({ value: "#blocked!", t: "e" }), [4, "#BLOCKED!"]);
  assert.deepEqual(tableOps.cellDedupeKey({ value: "#BLOCKED!", t: "s" }), [2, "#blocked!"]);

  const grid = {
    "1,0": { value: "오류" },
    "2,0": { value: "#BLOCKED!", t: "e" },
    "3,0": { value: "#blocked!" },
    "4,0": { value: "#BLOCKED!", t: "s" },
    "5,0": { value: "#CONNECT!" },
    "6,0": { value: "#CONNECT!", t: "e" },
  };
  const table = { name: "표1", range: "A1:A6", header: true, totals: false, columns: ["오류"] };
  const plan = tableOps.planDedupe({
    table,
    cols: [0],
    ctx: {
      boxOf: () => ({ r1: 1, c1: 1, r2: 6, c2: 1 }),
      cellAt: (r, c) => grid[`${r},${c - 1}`] || { value: "" },
    },
  });
  assert.equal(plan.removed, 2);
  assert.equal(plan.remaining, 3);
  assert.deepEqual(plan.rowMoves, [[4, 3], [5, 4]]);
});

test("sheet-pivot-model propagates #CONNECT! and #BLOCKED! error literals in aggregates", () => {
  for (const errCode of ["#CONNECT!", "#BLOCKED!"]) {
    assert.equal(pivotModel.numberText(errCode), errCode);
    const reader = (r, c) => {
      if (r === 0) return { kind: "text", text: c === 0 ? "Key" : "Val", display: c === 0 ? "Key" : "Val" };
      if (r === 1) return c === 0 ? { kind: "text", text: "k1", display: "k1" } : { kind: "number", num: 10, display: "10" };
      return c === 0 ? { kind: "text", text: "k1", display: "k1" } : { kind: "error", display: errCode };
    };
    const res = pivotModel.computePivotWithReader(
      { range: "A1:B3", header: true, row: 0, value: 1, agg: "sum" },
      reader,
    );
    assert.equal(res.matrix[0][0], errCode);
    assert.equal(res.rowTotals[0], errCode);
    assert.equal(res.grandTotal, errCode);
  }
});
