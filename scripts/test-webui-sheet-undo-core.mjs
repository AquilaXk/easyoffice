import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The cells an undo step keeps (`sheet-delta.js`), the record of a sheet change (`sheet-history.js`) and the bounded
// stacks (`workspace-history.js`), apart from the page (PD-221).

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("sheet-refs.js"), env);
loadUiModule(UI("sheet-delta.js"), env);
const D = env.__EASYOFFICE_SHEET_DELTA__;

const byText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const e = (value, extra = {}) => ({ value, formula: "", ...extra });
const FROZEN = Object.freeze({ fontWeight: "700" });

test("a delta keeps cells, absent cells and every part of an entry, and applying it gives the delta that leads back", () => {
  const map = {
    A1: e("1"),
    B2: { value: "", formula: "=A1+1", style: FROZEN },
    C3: e("t", { link: Object.freeze({ target: "#A1" }) }),
    D4: e("kid", { spillOf: "A1" }),
    E5: { value: "no formula key" },
  };
  const before = structuredClone(map);
  const delta = D.captureRefs(map, ["A1", "B2", "C3", "D4", "E5", "Z9"]);
  assert.equal(delta.n, 6);
  assert.equal(delta.packed instanceof Float64Array, true, "the addresses are one packed array");
  assert.equal(delta.flags instanceof Uint8Array, true, "one flag byte a cell");
  assert.equal(delta.styles[1], FROZEN, "a frozen style is shared, not copied");
  // The page changes the cells in place and by replacement; the delta is not the cells it was taken from.
  map.A1.value = "changed";
  map.B2 = e("new");
  delete map.C3;
  map.Z9 = e("made");
  const redo = D.apply(map, delta);
  assert.deepEqual(map, before, "applying the delta restores the cells, the absent one is deleted");
  assert.deepEqual([...D.cells(redo)].map(([ref, entry]) => [ref, entry?.value]), [["A1", "changed"], ["B2", "new"], ["C3", undefined], ["D4", "kid"], ["E5", "no formula key"], ["Z9", "made"]]);
  D.apply(map, redo);
  assert.equal(map.A1.value, "changed");
  assert.equal(map.Z9.value, "made");
  assert.equal("C3" in map, false);
});

test("a key that is no canonical address survives as text", () => {
  const map = { a1: e("low"), "$B$2": e("abs") };
  const delta = D.captureRefs(map, ["a1", "$B$2", "A1"]);
  assert.deepEqual(D.refsOf(delta), ["a1", "$B$2", "A1"]);
  const edited = { a1: e("x"), "$B$2": e("y"), A1: e("z") };
  D.apply(edited, delta);
  assert.deepEqual(edited, { a1: e("low"), "$B$2": e("abs") });
});

test("reconcile drops the cells a change left as they were and names the cells it made", () => {
  const map = { A1: e("1"), A2: e("2"), A3: e("3") };
  const delta = D.captureRange(map, { r1: 1, c1: 1, r2: 5, c2: 2 });
  assert.deepEqual(D.refsOf(delta).sort(byText), ["A1", "A2", "A3", "A4", "A5", "B1", "B2", "B3", "B4", "B5"].sort(byText), "a small range is read by address");
  map.A2 = e("two");
  map.B4 = e("new");
  const kept = D.reconcile(delta, map);
  assert.deepEqual(D.refsOf(kept).sort(byText), ["A2", "B4"]);
  // A large range is read from the keys, so a cell the change made inside it is found by a scan of the model.
  const wide = {};
  for (let r = 1; r <= 5000; r += 1) wide["A" + r] = e(String(r));
  const rg = { r1: 1, c1: 1, r2: 5000, c2: 3 };
  const big = D.captureRange(wide, rg);
  assert.equal(big.n, 5000, "only the cells that exist are captured");
  wide.A10 = e("ten");
  wide.C7 = e("made");
  const bigKept = D.reconcile(big, wide, { scan: (row, col) => row >= rg.r1 && row <= rg.r2 && col >= rg.c1 && col <= rg.c2 });
  assert.deepEqual(D.refsOf(bigKept).sort(byText), ["A10", "C7"]);
  assert.equal(D.entryAt(bigKept, D.refsOf(bigKept).indexOf("C7")), undefined, "the made cell was absent");
});

test("a row edit moves the cells: they are compared where the edit puts them and the deleted ones are kept", () => {
  const map = { A1: e("a"), A2: e("b"), A3: e("c"), B3: e("", { formula: "=A3" }) };
  const delta = D.captureAll(map);
  const del = { axis: "row", op: "delete", at: 2, count: 1 };
  const moved = D.shiftMap(map, del);
  assert.deepEqual(Object.keys(moved.map), ["A1", "A2", "B2"]);
  assert.deepEqual(D.refsOf(moved.gone), ["A2"]);
  moved.map.B2 = e("", { formula: "=A2" });
  const kept = D.reconcile(delta, moved.map, { edit: del, scan: () => true });
  assert.deepEqual(D.refsOf(kept).sort(byText), ["A2", "B3"], "the deleted cell and the rewritten formula, nothing else");
  assert.equal(D.shiftedRef(del, "A3"), "A2");
  assert.equal(D.shiftedRef(del, "A2"), null);
  assert.equal(D.shiftedRef({ axis: "col", op: "insert", at: 1, count: 2 }, "A1"), "C1");
  assert.deepEqual(D.inverseEdit(del), { ...del, op: "insert" });
});

test("the bytes of a delta count the strings it holds and stay far below a whole-entry copy", () => {
  const map = {};
  for (let r = 1; r <= 10000; r += 1) map["A" + r] = e("value " + r);
  const delta = D.captureAll(map);
  const whole = D.bytesOfEntries(10000);
  assert.ok(delta.bytes > 10000 * 25, "at least the address, flag and pointers of each cell");
  assert.ok(delta.bytes < whole, "less than the same cells as entries (" + delta.bytes + " < " + whole + ")");
  assert.equal(D.emptyDelta().bytes, 0);
});

test("the extent change tells the written and the removed addresses", () => {
  const delta = D.captureRefs({ B2: e("x"), C3: e("") }, ["B2", "C3", "D4"]);
  const { written, removed } = D.extentChange(delta);
  assert.deepEqual(written, [D.packCoord(2, 2)]);
  assert.deepEqual(removed.sort(byText), [D.packCoord(3, 3), D.packCoord(4, 4)].sort(byText));
});

// ---- the record of a change ------------------------------------------------------------------------------------

function pageOf(sheets) {
  const pushed = [];
  const host = {};
  loadUiModule(UI("sheet-refs.js"), host);
  loadUiModule(UI("sheet-delta.js"), host);
  const layout = { value: {} };
  host.__EASYOFFICE_SHEET_LAYOUT__ = { getAll: () => structuredClone(layout.value) };
  host.__EASYOFFICE_SHEET_NAMES__ = { getAll: () => [] };
  host.__EASYOFFICE_SHEET_TAB_STRIP__ = { reconcile: (keys) => keys };
  const protectedSheets = new Set();
  host.__EASYOFFICE_SHEET_PROTECT__ = { isOn: (name) => protectedSheets.has(name), cellEditable: (_n, _r, _c, style) => !style?.locked };
  loadUiModule(UI("sheet-history.js"), host);
  const store = sheets;
  const counts = {};
  const history = host.__EASYOFFICE_CREATE_SHEET_HISTORY__({
    sheetDataStore: store,
    sheetRowCounts: counts,
    getActiveModule: () => "SHEET",
    getActiveSheetName: () => "S",
    getActiveSheetCellRef: () => "A1",
    getController: () => ({ snapshotDomSheetCells: () => store.S, getColumnWidths: () => ({}), getSheetRules: () => ({}) }),
    push: (record) => pushed.push(record),
  });
  return { history, store, pushed, layout, protectedSheets, D: host.__EASYOFFICE_SHEET_DELTA__ };
}

test("a record of cells undoes and redoes through the swap, the redo holding what the undo found", () => {
  const { history, store, pushed } = pageOf({ S: { A1: e("1"), A2: e("2") } });
  const change = history.begin({ refs: ["A1", "B1"] });
  store.S.A1 = e("one");
  store.S.B1 = e("made");
  const record = history.commit(change);
  assert.equal(pushed[0], record);
  assert.deepEqual(record.sheets.map((entry) => [entry.name, entry.delta.n]), [["S", 2]]);
  const { inverse } = history.swap(record);
  assert.deepEqual(store.S, { A1: e("1"), A2: e("2") });
  history.swap(inverse);
  assert.deepEqual(store.S, { A1: e("one"), A2: e("2"), B1: e("made") });
});

test("a record of sheets: added, deleted and renamed ones go back and forth", () => {
  const { history, store } = pageOf({ S: { A1: e("1") }, T: { A1: e("t") } });
  const change = history.begin({ sheets: "all", parts: "all" });
  store.U = { A1: e("new") };
  delete store.T;
  store.R = store.S;
  delete store.S;
  const record = history.commit(change, { renames: { S: "R" } });
  assert.deepEqual(record.sheets.map((entry) => entry.name).sort(byText), ["S", "T", "U"]);
  assert.equal(record.sheets.find((entry) => entry.name === "S").alias, "R");
  assert.equal(record.sheets.find((entry) => entry.name === "U").absent, true);
  const { inverse } = history.swap(record);
  assert.deepEqual(store, { S: { A1: e("1") }, T: { A1: e("t") } });
  history.swap(inverse);
  assert.deepEqual(store, { R: { A1: e("1") }, U: { A1: e("new") } });
});

test("a record of a row edit holds only the cells the edit changed, and moves the rest back", () => {
  const { history, store } = pageOf({ S: { A1: e("a"), A2: e("b"), A3: e("c"), B3: e("", { formula: "=A3" }), C9: e("far") }, T: { A1: e("", { formula: "=S!A3" }) } });
  const edit = { sheet: "S", axis: "row", op: "delete", at: 2, count: 1 };
  const change = history.begin({ sheets: "all", shift: edit });
  // What the engine and the page do: the cells move, the formulas of both sheets are rewritten.
  const shifted = { A1: e("a"), A2: e("c"), B2: e("", { formula: "=A2" }), C8: e("far") };
  store.S = shifted;
  store.T = { A1: e("", { formula: "=S!A2" }) };
  const record = history.commit(change);
  const held = Object.fromEntries(record.sheets.map((entry) => [entry.name, entry.delta.n]));
  assert.deepEqual(held, { S: 2, T: 1 }, "the deleted cell and the rewritten formula, and the other sheet's rewrite");
  assert.deepEqual(record.shift, { ...edit, op: "insert" });
  const { inverse } = history.swap(record);
  assert.deepEqual(store, { S: { A1: e("a"), A2: e("b"), A3: e("c"), B3: e("", { formula: "=A3" }), C9: e("far") }, T: { A1: e("", { formula: "=S!A3" }) } });
  history.swap(inverse);
  assert.deepEqual(store, { S: shifted, T: { A1: e("", { formula: "=S!A2" }) } });
});

test("a record of an insert puts the cells back by deleting the band, wherever the cells are now", () => {
  const { history, store } = pageOf({ S: { A1: e("a"), A2: e("b"), B2: e("", { formula: "=A2" }) } });
  const edit = { sheet: "S", axis: "row", op: "insert", at: 2, count: 2 };
  const change = history.begin({ sheets: "all", shift: edit });
  store.S = { A1: e("a"), A4: e("b"), B4: e("", { formula: "=A4" }) };
  const record = history.commit(change);
  assert.deepEqual(record.sheets.map((entry) => [entry.name, entry.delta.n]), [["S", 1]], "the places, and the one formula the edit rewrote");
  history.swap(record);
  assert.deepEqual(store.S, { A1: e("a"), A2: e("b"), B2: e("", { formula: "=A2" }) });
});

test("an undo that would change a locked cell of a protected sheet is refused, an edit's own moves are not compared", () => {
  const { history, store, protectedSheets } = pageOf({ S: { A1: e("1", { style: { locked: true } }), B1: e("2") } });
  const change = history.begin({ refs: ["A1", "B1"] });
  store.S.A1 = e("9", { style: { locked: true } });
  store.S.B1 = e("8");
  const record = history.commit(change);
  assert.equal(history.wouldChange(record), false, "an unprotected sheet refuses nothing");
  protectedSheets.add("S");
  assert.equal(history.wouldChange(record), true, "A1 is locked and would change");
  const unlocked = pageOf({ S: { B1: e("2") } });
  protectedSheets.clear();
  const open = unlocked.history.begin({ refs: ["B1"] });
  unlocked.store.S.B1 = e("3");
  unlocked.protectedSheets.add("S");
  assert.equal(unlocked.history.wouldChange(unlocked.history.commit(open)), false, "an unlocked cell may change");
  const edit = pageOf({ S: { A1: e("1", { style: { locked: true } }) } });
  const shift = { sheet: "S", axis: "row", op: "insert", at: 1, count: 1 };
  const moved = edit.history.begin({ sheets: "all", shift });
  edit.store.S = { A2: e("1", { style: { locked: true } }) };
  edit.protectedSheets.add("S");
  assert.equal(edit.history.wouldChange(edit.history.commit(moved)), false, "the cells of a row edit are moved, not changed");
});

test("the settings of a record are those that changed, and a no-op change still makes a step", () => {
  const { history, store, layout, pushed } = pageOf({ S: { A1: e("1") } });
  const change = history.begin({ parts: ["sheetLayout", "definedNames"] });
  layout.value = { S: { hiddenRows: [[2, 2]] } };
  const record = history.commit(change);
  assert.deepEqual(Object.keys(record.parts), ["sheetLayout"]);
  assert.deepEqual(record.parts.sheetLayout, {});
  const same = history.begin({ range: { r1: 1, c1: 1, r2: 1, c2: 1 } });
  assert.equal(history.commit(same).sheets.length, 0);
  assert.equal(pushed.length, 2, "both steps are on the stack");
  assert.deepEqual(store.S, { A1: e("1") });
});

// ---- the bounded stacks ----------------------------------------------------------------------------------------

function stacksOf() {
  const loaded = {};
  loadUiModule(UI("sheet-refs.js"), loaded);
  loadUiModule(UI("sheet-delta.js"), loaded);
  loadUiModule(UI("workspace-history.js"), loaded);
  const feedback = { textContent: "" };
  const document = { getElementById: (id) => (id === "status-command-feedback" ? feedback : null) };
  const history = loaded.__EASYOFFICE_CREATE_WORKSPACE_HISTORY__({
    document,
    invokeIpc: async () => ({}),
    sheetDataStore: {},
    sheetStoreHelpers: {},
    ribbonIpcPayload: () => "{}",
    sheetHistory: {},
    getSheetController: () => ({}),
    getActiveSheetName: () => "S",
    captureWorkspaceSnapshot: () => ({}),
    restoreWorkspaceSnapshot: async () => {},
    enqueueWorkspaceHistoryOp: (fn) => fn(),
  });
  return { history, feedback, limits: loaded.__EASYOFFICE_HISTORY_LIMITS__ };
}

test("the stack keeps the last 100 steps", () => {
  const { history, limits } = stacksOf();
  assert.equal(limits.MAX_HISTORY_STEPS, 100);
  for (let i = 0; i < 130; i += 1) history.pushRecord({ kind: "delta", bytes: 100, id: i });
  assert.equal(history.sizes().undo, 100);
});

test("the oldest steps leave when the bytes pass the bound, and a step over the bound alone ends the record", () => {
  const { history, feedback, limits } = stacksOf();
  const quarter = Math.floor(limits.MAX_HISTORY_BYTES / 4) + 1;
  for (let i = 0; i < 10; i += 1) history.pushRecord({ kind: "delta", bytes: quarter });
  assert.equal(history.sizes().undo, 3, "three of them fit");
  assert.ok(history.sizes().bytes <= limits.MAX_HISTORY_BYTES);
  history.pushRecord({ kind: "delta", bytes: limits.MAX_HISTORY_BYTES + 1 });
  assert.equal(history.sizes().undo, 0, "the record of steps ends");
  assert.equal(feedback.textContent, "실행 취소 기록 한도 초과: 기록을 비움");
  history.pushRecord({ kind: "delta", bytes: 10 });
  assert.equal(history.sizes().undo, 1, "and starts again");
});

test("a whole snapshot is weighed by the entries of its sheets", () => {
  const { history } = stacksOf();
  const sheet = {};
  for (let r = 1; r <= 1000; r += 1) sheet["A" + r] = e("x");
  history.pushRecord({ sheets: { S: sheet } });
  assert.ok(history.sizes().bytes >= D.bytesOfEntries(1000));
});
