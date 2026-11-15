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

  let activeSheet = "toString";
  let activeRef = "A1";
  // Create a minimal sheet controller to test scanExtent and small-range selection/toolbar queries
  const controller = host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__({
    document,
    sheetDataStore,
    sheetRowCounts,
    sheetStoreHelpers: helpers({
      invokeIpc: async () => ({}),
      normalizeFormattedNumericCell: (v) => String(v),
      tokenizeFormulaStringLiterals: () => [],
    }),
    getActiveModule: () => "SHEET",
    getActiveSheetName: () => activeSheet,
    setActiveSheetName: (next) => {
      activeSheet = next;
    },
    getActiveSheetCellRef: () => activeRef,
    setActiveSheetCellRef: (next) => {
      activeRef = next;
    },
    enqueueWorkspaceHistoryOp: (fn) => Promise.resolve().then(fn),
    restoreWorkspaceSnapshot: () => {},
    sheetHistory: null,
    onSheetSwitchedOutline: () => {},
  });

  for (const name of specialNames) {
    const count = controller.getSheetRowCount(name);
    assert.ok(typeof count === "number" && count >= 5, `sheet "${name}" row count is >= 5`);
  }

  // Deterministic O(1) proof on 20,000 cells: small-range selection aggregate & format toolbar sync
  // (area <= 4096) must address target cells directly without enumerating Object.keys(map).
  const benchRaw = Object.create(null);
  for (let r = 1; r <= 200; r++) {
    for (let c = 1; c <= 100; c++) {
      benchRaw[gridUtils.refOf(r, c)] = { value: r === 10 && c === 5 ? "42" : "1", formula: "" };
    }
  }
  let fullKeyScans = 0;
  let directCellReads = 0;
  const trackedBench = new Proxy(benchRaw, {
    ownKeys(target) {
      fullKeyScans += 1;
      return Reflect.ownKeys(target);
    },
    get(target, prop, receiver) {
      if (typeof prop === "string") directCellReads += 1;
      return Reflect.get(target, prop, receiver);
    },
  });
  sheetDataStore.bench = trackedBench;
  activeSheet = "bench";

  // Prime extent cache once (initial extent scan legitimately enumerates keys once), then reset counters.
  assert.equal(controller.getSheetRowCount("bench"), 200);
  assert.equal(fullKeyScans, 1, "initial extent scan enumerates keys once");
  fullKeyScans = 0;
  directCellReads = 0;

  controller.updateSheetSelectionAndAggregates("E10");
  assert.equal(fullKeyScans, 0, "initial 1x1 selection + viewport render must not call Object.keys on 20,000-cell sheet");

  directCellReads = 0;
  controller.syncSheetFormatToolbar();
  assert.equal(fullKeyScans, 0, "1x1 toolbar sync must not call Object.keys on 20,000-cell sheet");
  assert.equal(directCellReads, 1, "1x1 toolbar sync reads only the single selected cell");

  controller.updateSheetSelectionAndAggregates("F12");
  assert.equal(activeRef, "F12");
  assert.equal(fullKeyScans, 0, "subsequent 1x1 selection move must not call Object.keys on 20,000-cell sheet");
});

test("sheet store diff against mirror skips untouched sheet references without key or cell scans", async () => {
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
  const lastRef = gridUtils.refOf(100, 100);
  for (let r = 1; r <= 100; r++) {
    for (let c = 1; c <= 100; c++) {
      const ref = gridUtils.refOf(r, c);
      sheet1[ref] = { value: "10", formula: "" };
      sheet2[ref] = { value: "20", formula: "" };
    }
  }

  const initialMap = { Sheet2: sheet2, Sheet1: sheet1 };
  await helper.callSheetAction({ sessionAction: "open", sheets: ["Sheet2", "Sheet1"] });

  // First full sync establishes the snapshot and mirror
  const fullSync = await helper.computeEvaluatedSheetStore(initialMap, "Sheet1");
  assert.ok(fullSync, "first sync completed");

  let untouchedKeyScans = 0;
  let untouchedCellReads = 0;
  const trackedSheet2 = new Proxy(sheet2, {
    ownKeys(target) {
      untouchedKeyScans += 1;
      return Reflect.ownKeys(target);
    },
    get(target, prop, receiver) {
      if (typeof prop === "string") untouchedCellReads += 1;
      return Reflect.get(target, prop, receiver);
    },
  });

  const baseMap = { Sheet2: trackedSheet2, Sheet1: sheet1 };
  const unchangedMap = { Sheet2: trackedSheet2, Sheet1: sheet1 };
  const modifiedSheet1 = Object.assign(Object.create(null), sheet1, {
    [lastRef]: { value: "99", formula: "" },
  });
  const partialMap = { Sheet2: trackedSheet2, Sheet1: modifiedSheet1 };

  assert.equal(helper.sheetCellDataChanged(baseMap, unchangedMap, false), false);
  assert.equal(helper.sheetCellDataChanged(baseMap, partialMap, false), true);
  assert.equal(untouchedKeyScans, 0, "untouched shared-reference sheet must not trigger ownKeys scan");
  assert.equal(untouchedCellReads, 0, "untouched shared-reference sheet must not trigger per-cell reads");
});

