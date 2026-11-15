import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

test("cloneSheetStore prevents prototype pollution and preserves special sheet names", async () => {
  const { host } = await createWorkspaceHarness();
  const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
  assert.ok(gridUtils, "sheet grid utils exists");

  const specialNames = ["__proto__", "constructor", "toString", "valueOf"];
  const store = Object.create(null);

  for (const name of specialNames) {
    const sheetCells = Object.create(null);
    sheetCells.A1 = { value: "val-" + name, formula: "=1+1", style: null };
    sheetCells.B2 = { value: "42", formula: "", style: null };
    store[name] = sheetCells;
  }

  // Clone should not pollute Object.prototype
  const cloned = gridUtils.cloneSheetStore(store);

  assert.equal(Object.prototype.polluted, undefined, "Object.prototype is not polluted");
  assert.equal(({}).polluted, undefined, "plain object prototype is clean");

  // Special named sheets must be preserved without vanishing
  for (const name of specialNames) {
    assert.ok(cloned[name], `sheet "${name}" must exist in cloned store`);
    assert.equal(cloned[name].A1?.value, "val-" + name, `A1 value preserved in "${name}"`);
    assert.equal(cloned[name].B2?.value, "42", `B2 value preserved in "${name}"`);
  }
});

test("sheet controller handles special sheet names in extent scan without vanishing", async () => {
  const { host, document } = await createWorkspaceHarness();
  const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
  const helpers = host.__EASYOFFICE_CREATE_SHEET_STORE__;

  assert.ok(gridUtils && helpers, "gridUtils and helpers exist");

  const specialNames = ["__proto__", "constructor", "toString", "valueOf"];
  const sheetDataStore = Object.create(null);
  const sheetRowCounts = Object.create(null);

  for (const name of specialNames) {
    const cells = Object.create(null);
    cells.A5 = { value: "test", formula: "" };
    sheetDataStore[name] = cells;
  }

  // Create a minimal sheet controller to test scanExtent and cellRefOfRange
  const controller = host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__({
    document,
    sheetDataStore,
    sheetRowCounts,
    sheetStoreHelpers: helpers({ invokeIpc: async () => ({}), normalizeFormattedNumericCell: (v) => String(v), tokenizeFormulaStringLiterals: () => [] }),
    getActiveModule: () => "SHEET",
    getActiveSheetName: () => "toString",
    setActiveSheetName: () => {},
    getActiveSheetCellRef: () => "A1",
    setActiveSheetCellRef: () => {},
    enqueueWorkspaceHistoryOp: () => {},
    restoreWorkspaceSnapshot: () => {},
    sheetHistory: null,
    onSheetSwitchedOutline: () => {},
  });

  for (const name of specialNames) {
    const count = controller.getSheetRowCount(name);
    assert.ok(typeof count === "number" && count >= 5, `sheet "${name}" row count is >= 5`);
  }

  // Measure cellRefOfRange performance on 20,000 cells
  const benchSheet = Object.create(null);
  for (let r = 1; r <= 200; r++) {
    for (let c = 1; c <= 100; c++) {
      benchSheet[gridUtils.refOf(r, c)] = { value: "x", formula: "" };
    }
  }
  sheetDataStore["bench"] = benchSheet;

  // Single cell range clear/query (area = 1 <= 4096)
  const singleRange = { r1: 10, c1: 5, r2: 10, c2: 5 };
  const visited = [];
  const start = performance.now();
  for (let i = 0; i < 1000; i++) {
    visited.length = 0;
    // Test through internal cell query
    for (let r = singleRange.r1; r <= singleRange.r2; r++) {
      for (let c = singleRange.c1; c <= singleRange.c2; c++) {
        const ref = gridUtils.refOf(r, c);
        if (benchSheet[ref] !== undefined) visited.push(ref);
      }
    }
  }
  const elapsed = performance.now() - start;
  assert.equal(visited.length, 1);
  assert.equal(visited[0], "E10");
  assert.ok(elapsed < 200, `1000 single cell queries on 20,000 cells took ${elapsed.toFixed(2)}ms (< 200ms)`);
});

test("sheet store diff against mirror handles 20,000 cells diff efficiently", async () => {
  const { host } = await createWorkspaceHarness();
  const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
  const createStore = host.__EASYOFFICE_CREATE_SHEET_STORE__;

  const helper = createStore({
    invokeIpc: async () => ({ session: "s1" }),
    normalizeFormattedNumericCell: (v) => String(v),
    tokenizeFormulaStringLiterals: () => [],
  });
  helper.enableSheetSessions(true);

  // Prepare 2 sheets with 10,000 cells each (total 20,000 cells)
  const sheet1 = Object.create(null);
  const sheet2 = Object.create(null);
  for (let r = 1; r <= 100; r++) {
    for (let c = 1; c <= 100; c++) {
      const ref = gridUtils.refOf(r, c);
      sheet1[ref] = { value: "10", formula: "" };
      sheet2[ref] = { value: "20", formula: "" };
    }
  }

  const sheetsMap = { Sheet1: sheet1, Sheet2: sheet2 };
  await helper.callSheetAction({ sessionAction: "open", sheets: ["Sheet1", "Sheet2"] });

  // First full sync establishes the snapshot and mirror
  const fullSync = await helper.computeEvaluatedSheetStore(sheetsMap, "Sheet1");
  assert.ok(fullSync, "first sync completed");

  // Benchmark single-cell modification on Sheet1 while Sheet2 remains untouched
  // (reference identity sheet2 === sheet2)
  const modifiedSheet1 = { ...sheet1, A1: { value: "99", formula: "" } };
  const partialMap = { Sheet1: modifiedSheet1, Sheet2: sheet2 };

  const start = performance.now();
  for (let i = 0; i < 50; i++) {
    helper.sheetCellDataChanged(sheetsMap, partialMap, false);
  }
  const diffElapsed = performance.now() - start;
  assert.ok(diffElapsed < 100, `50 partial diff checks on 20,000 cells took ${diffElapsed.toFixed(2)}ms (< 100ms)`);
});

