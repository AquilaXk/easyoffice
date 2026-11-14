import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// The page's side of the gaps next to arrays that spill (PD-222): the format a filled cell keeps, the merges the engine is
// told, and the clipboard text that may not be written over a part of an array.

const entry = (value, formula = "") => ({ value, formula });
const locked = { unlocked: true };

test("an array fills a cell that holds only a format and the cell keeps the format when the array goes", async () => {
  const { host } = await createWorkspaceHarness();
  const spill = host.__EASYOFFICE_SHEET_SPILL__;
  const map = { A1: entry("", "=SEQUENCE(3)"), A2: { value: "", formula: "", style: locked }, A3: entry("typed") };
  const values = { A2: "2", A3: "3", A4: "4" };
  spill.applyInfo(map, { cell: "A1", rows: 4, cols: 1, blocked: false }, (ref) => values[ref]);
  assert.deepEqual(map.A2, { value: "2", formula: "", spillOf: "A1", style: locked });
  assert.equal(map.A3.value, "typed", "a cell with a value is the user's");
  assert.deepEqual(map.A4, { value: "4", formula: "", spillOf: "A1" });
  // the area shrinks to the formula's own cell: the formatted cell is empty again with its format, the plain one is gone
  spill.applyInfo(map, { cell: "A1", rows: 0, cols: 0, blocked: false }, () => "");
  assert.deepEqual(map.A2, { value: "", formula: "", style: locked });
  assert.equal(map.A4, undefined);
});

test("a hyperlink on an empty cell survives the array that fills it", async () => {
  const { host } = await createWorkspaceHarness();
  const spill = host.__EASYOFFICE_SHEET_SPILL__;
  const link = { target: "https://example.com" };
  const map = { A1: entry("", "=SEQUENCE(2)"), A2: { value: "", formula: "", link } };
  spill.applyInfo(map, { cell: "A1", rows: 2, cols: 1, blocked: false }, () => "2");
  assert.deepEqual(map.A2.link, link);
  spill.applyInfo(map, { cell: "A1", rows: 0, cols: 0, blocked: false }, () => "");
  assert.deepEqual(map.A2, { value: "", formula: "", link });
});

test("a stateless answer keeps the format of the cells it fills and of the ones it frees", async () => {
  const { host } = await createWorkspaceHarness();
  const spill = host.__EASYOFFICE_SHEET_SPILL__;
  const sheets = { S: { A1: entry("", "=SEQUENCE(2)"), A2: { value: "", formula: "", style: locked } } };
  spill.replaceAll(sheets, [{ sheet: "S", cell: "A1", rows: 2, cols: 1, blocked: false, values: ["1", "2"] }]);
  assert.deepEqual(sheets.S.A2, { value: "2", formula: "", spillOf: "A1", style: locked });
  spill.replaceAll(sheets, []);
  assert.deepEqual(sheets.S.A2, { value: "", formula: "", style: locked });
});

test("the merges of a sheet go to the engine as ranges, and only the sheets that have some are named", async () => {
  const { host } = await createWorkspaceHarness();
  const spill = host.__EASYOFFICE_SHEET_SPILL__;
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.addMerge("S", 2, 1, 3, 2);
  layout.addMerge("S", 7, 4, 7, 5);
  assert.deepEqual(spill.mergePayload("S", layout), ["A2:B3", "D7:E7"]);
  assert.deepEqual(spill.mergesField(["S", "T"], layout), { merges: { S: ["A2:B3", "D7:E7"] } });
  assert.deepEqual(spill.mergesField(["T"], layout), {});
  layout.replaceAll({});
});

function sessionStore(host, sandbox, answers) {
  const calls = [];
  const invokeIpc = async (command, payload) => {
    const body = JSON.parse(payload);
    calls.push(body);
    if (body.sessionAction === "open") return { session: "s1" };
    if (body.sessionAction === "close") return { closed: true };
    if (body.sessionAction === "merges") return answers.merges(body);
    if (body.sessionAction === "apply") return { session: "s1", changed: [], circular: false, recomputed: 0 };
    throw new Error("unexpected " + body.sessionAction);
  };
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(true);
  return { store, calls };
}

test("a session is told the merges before the cells, once per change, and folds the blocked array into the model", async () => {
  const { host, sandbox } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  let blocked = false;
  const { store, calls } = sessionStore(host, sandbox, {
    merges: (body) => ({
      session: "s1",
      changed: blocked
        ? [{ sheet: "Sheet1", cell: "B1", value: "#SPILL!" }, { sheet: "Sheet1", cell: "B2", value: "" }]
        : [{ sheet: "Sheet1", cell: "B1", value: "1" }],
      spills: [{ sheet: "Sheet1", cell: "B1", rows: blocked ? 2 : 0, cols: 1, blocked }],
      circular: false,
      merges: body.merges,
    }),
  });
  const sheets = { Sheet1: { B1: entry("1", "=SEQUENCE(2)"), B2: { value: "2", formula: "", spillOf: "B1" } } };
  sheets.Sheet1.B1.spillRows = 2;
  sheets.Sheet1.B1.spillCols = 1;
  layout.addMerge("Sheet1", 2, 2, 2, 3);
  blocked = true;
  const live = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  const order = calls.map((c) => c.sessionAction);
  assert.deepEqual(order.slice(0, 3), ["open", "merges", "apply"], "merges go before the cells");
  assert.deepEqual(calls[1].merges, ["B2:C2"]);
  assert.equal(live.spillsChanged, true);
  assert.equal(sheets.Sheet1.B2, undefined, "the blocked array gave its cell up");
  assert.equal(sheets.Sheet1.B1.value, "#SPILL!");
  // nothing changed: not sent again
  calls.length = 0;
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(calls.map((c) => c.sessionAction), ["apply"]);
  // unmerged: the engine is told an empty list
  layout.removeMerges("Sheet1", 2, 2, 2, 3);
  blocked = false;
  calls.length = 0;
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(calls.map((c) => c.sessionAction), ["merges", "apply"]);
  assert.deepEqual(calls[0].merges, []);
  layout.replaceAll({});
});

function clipboard(host, entries, feedback) {
  const grid = host.__EASYOFFICE_SHEET_GRID_UTILS__;
  const edits = [];
  const clip = host.__EASYOFFICE_CREATE_SHEET_CLIPBOARD__({
    grid: { ...grid, colWidth: () => 80 },
    isFormulaStr: (s) => String(s).startsWith("="),
    getEntry: (ref) => entries[ref],
    getSheetName: () => "S",
    feedback,
    applyCellEdits: async (list) => {
      edits.push(...list);
      return true;
    },
    clearExisting: () => [],
  });
  return { clip, edits };
}

test("clipboard text is not written over a part of an array, but over the whole of it or its own cell it is", async () => {
  const { host } = await createWorkspaceHarness();
  const said = [];
  const entries = {
    A1: { value: "1", formula: "=SEQUENCE(3)", spillRows: 3, spillCols: 1 },
    A2: { value: "2", formula: "", spillOf: "A1" },
    A3: { value: "3", formula: "", spillOf: "A1" },
  };
  const { clip, edits } = clipboard(host, entries, (t) => said.push(t));
  // a part of the area without the formula's cell
  assert.equal(await clip.pasteText("x\ty", { r1: 2, c1: 1, r2: 2, c2: 1 }), null);
  assert.deepEqual(said, ["배열의 일부분을 변경할 수 없음"]);
  assert.deepEqual(edits, []);
  // the block reaches the cell of the formula: the array is replaced, nothing is cut
  assert.notEqual(await clip.pasteText("x\ny\nz", { r1: 1, c1: 1, r2: 1, c2: 1 }), null);
  assert.ok(edits.length >= 3);
  // beside the area: free
  edits.length = 0;
  assert.notEqual(await clip.pasteText("x", { r1: 2, c1: 2, r2: 2, c2: 2 }), null);
  assert.equal(edits.length, 1);
});
