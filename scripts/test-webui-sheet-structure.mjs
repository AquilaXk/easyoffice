import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

async function layoutApi() {
  const { host } = await createWorkspaceHarness();
  return host.__EASYOFFICE_SHEET_LAYOUT__;
}

test("hidden row runs map between rows and visible positions", async () => {
  const layout = await layoutApi();
  const runs = [[3, 4], [10, 10]];
  assert.equal(layout.hiddenBefore(runs, 3), 0);
  assert.equal(layout.hiddenBefore(runs, 6), 2);
  assert.equal(layout.hiddenBefore(runs, 11), 3);
  assert.equal(layout.hiddenTotal(runs), 3);
  // Visible positions 0.. are rows 1, 2, 5, 6, ... and skip row 10.
  assert.deepEqual([0, 1, 2, 3, 7, 8].map((v) => layout.visibleToIndex(runs, v)), [1, 2, 5, 6, 11, 12]);
  assert.equal(layout.runContains(runs, 4), true);
  assert.equal(layout.runContains(runs, 5), false);
});

test("hiding and unhiding keep the runs sorted and joined", async () => {
  const layout = await layoutApi();
  layout.setHidden("S", "row", 5, 6, true);
  layout.setHidden("S", "row", 2, 3, true);
  layout.setHidden("S", "row", 4, 4, true);
  assert.deepEqual(layout.get("S").hiddenRows, [[2, 6]]);
  layout.setHidden("S", "row", 4, 5, false);
  assert.deepEqual(layout.get("S").hiddenRows, [[2, 3], [6, 6]]);
  assert.equal(layout.rowHidden("S", 3), true);
  assert.equal(layout.rowHidden("S", 4), false);
  layout.setHidden("S", "col", 2, 2, true);
  assert.equal(layout.colHidden("S", 2), true);
  assert.deepEqual(layout.toJson(layout.get("S")), { hiddenRows: [[2, 3], [6, 6]], hiddenCols: [[2, 2]] });
});

test("merges are found by any covered cell and replaced when overlapped", async () => {
  const layout = await layoutApi();
  layout.addMerge("S", 2, 2, 4, 3);
  assert.deepEqual({ ...layout.mergeAt("S", 3, 3) }, { r1: 2, c1: 2, r2: 4, c2: 3 });
  assert.equal(layout.mergeAt("S", 5, 2), null);
  assert.equal(layout.mergeAt("S", 1, 1), null);
  layout.addMerge("S", 4, 3, 5, 5);
  assert.equal(layout.get("S").merges.length, 1);
  assert.equal(layout.mergeAt("S", 3, 3), null);
  assert.deepEqual(layout.toJson(layout.get("S")).merges, ["C4:E5"]);
  assert.equal(layout.removeMerges("S", 1, 1, 100, 100), 1);
  assert.equal(layout.get("S").merges.length, 0);
  // A merge that spans a million rows is still found.
  layout.addMerge("T", 1, 1, 1048576, 2);
  assert.ok(layout.mergeAt("T", 999999, 2));
});

test("the layout round-trips through its JSON and the engine's answer", async () => {
  const layout = await layoutApi();
  layout.replaceAll({ S: { merges: ["A1:B2"], hiddenRows: [[3, 4]], hiddenCols: [[2, 2]] } });
  assert.deepEqual(layout.getAll(), { S: { merges: ["A1:B2"], hiddenRows: [[3, 4]], hiddenCols: [[2, 2]] } });
  layout.setSheet("S", { merges: [], hiddenRows: [], hiddenCols: [] });
  assert.deepEqual(layout.getAll(), {});
});

test("remapRef moves a reference like the engine moves a cell", async () => {
  const layout = await layoutApi();
  const ins = { axis: "row", op: "insert", at: 3, count: 2 };
  assert.deepEqual(["A2", "A3", "B9"].map((r) => layout.remapRef(ins, r)), ["A2", "A5", "B11"]);
  const del = { axis: "row", op: "delete", at: 3, count: 2 };
  assert.deepEqual(["A2", "A3", "A4", "A5", "B9"].map((r) => layout.remapRef(del, r)), ["A2", null, null, "A3", "B7"]);
  const col = { axis: "col", op: "insert", at: 2, count: 1 };
  assert.deepEqual(["A1", "B1", "C4"].map((r) => layout.remapRef(col, r)), ["A1", "C1", "D4"]);
});

// A stand-in engine that answers `structure` with fixed rewrites.
function createFakeEngine() {
  const calls = [];
  async function invokeIpc(command, payload) {
    const body = JSON.parse(payload);
    calls.push(body);
    if (body.sessionAction === "open") return { session: "s1" };
    if (body.sessionAction === "apply") {
      return { session: "s1", changed: (body.changes || []).filter((c) => c.value.startsWith("=")).map((c) => ({ sheet: c.sheet, cell: c.cell, value: "v" })), circular: false };
    }
    if (body.sessionAction === "structure") {
      return {
        session: "s1",
        formulas: [{ sheet: "Sheet1", cell: "B3", value: "=A3+1" }],
        changed: [],
        circular: false,
        merges: ["A5:B6"],
        hiddenRows: [],
        hiddenCols: [],
        widths: [],
        rules: {},
      };
    }
    if (body.sessionAction === "close") return { closed: true };
    throw new Error("unexpected " + command);
  }
  return { invokeIpc, calls };
}

test("an edit moves the session's mirror, so the next evaluation sends nothing", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const engine = createFakeEngine();
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: engine.invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(true);
  const sheets = { Sheet1: { A1: { value: "1", formula: "" }, B1: { value: "", formula: "=A1+1" } } };
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  const result = await store.editAxis(sheets, { sheet: "Sheet1", axis: "row", op: "insert", at: 1, count: 2, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules: {} });
  assert.equal(result.ok, true);
  assert.equal(engine.calls.filter((c) => c.sessionAction === "structure").length, 1);
  // The page applies the move: A1 -> A3, B1 -> B3 (formula rewritten).
  const moved = { Sheet1: { A3: { value: "1", formula: "" }, B3: { value: "", formula: "=A3+1" } } };
  const before = engine.calls.length;
  await store.evaluateLiveFormulaTargets(moved, "Sheet1");
  const applies = engine.calls.slice(before).filter((c) => c.sessionAction === "apply");
  assert.equal(applies.length, 1);
  assert.deepEqual(applies[0].changes, []);
});

test("a refused or failing edit reports why and changes nothing", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const invokeIpc = async (command, payload) => {
    const body = JSON.parse(payload);
    if (body.sessionAction === "open") return { session: "s1" };
    if (body.sessionAction === "apply") return { session: "s1", changed: [], circular: false };
    if (body.sessionAction === "structure") return { refused: "structure" };
    return {};
  };
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(true);
  const refused = await store.editAxis({ S: { A1: { value: "1", formula: "" } } }, { sheet: "S", axis: "row", op: "insert", at: 1, count: 1 });
  assert.deepEqual(refused, { ok: false, reason: "structure" });
  store.enableSheetSessions(false);
  assert.equal((await store.editAxis({ S: {} }, { sheet: "S", axis: "row", op: "insert", at: 1, count: 1 })).ok, false);
});
