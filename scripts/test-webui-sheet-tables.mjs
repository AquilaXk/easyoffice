import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// The page's side of the sheet tables (PD-224): the layout keeps the engine's JSON, the engine is told the ranges of the
// tables before the cells (an array is not placed in or across one), a copied sheet's tables get names of their own,
// and the sheet's filter keeps out of a table.

const entry = (value, formula = "") => ({ value, formula });
const sales = (name = "Sales", range = "A1:C4") => ({
  name,
  range,
  header: true,
  totals: false,
  columns: ["Region", "Q1", "Q2"],
  style: "TableStyleMedium2",
  firstColumn: false,
  lastColumn: false,
  rowStripes: true,
  columnStripes: false,
});

test("the layout keeps the tables as the engine sent them and an empty one is no layout", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ S: { tables: [sales()] } });
  assert.deepEqual(layout.getAll(), { S: { tables: [sales()] } });
  assert.deepEqual(layout.tablesOf("S"), [sales()]);
  assert.deepEqual(layout.tablesOf("T"), []);
  // The engine's answer to a row or column edit replaces them.
  layout.setSheet("S", { tables: [sales("Sales", "A3:C6")] });
  assert.equal(layout.tablesOf("S")[0].range, "A3:C6");
  layout.setSheet("S", { tables: [] });
  assert.equal(layout.get("S"), null, "a sheet with nothing is dropped");
  layout.replaceAll({});
});

test("a copy of the tables gets the next free names and keeps the rest", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  const all = { A: { tables: [sales("Sales"), sales("sales_2", "E1:G4")] } };
  const copy = layout.copiedTables(all.A.tables, all);
  assert.deepEqual(copy.map((t) => t.name), ["Sales_3", "sales_2_2"]);
  assert.equal(copy[0].range, "A1:C4");
  assert.equal(copy[0].style, "TableStyleMedium2");
  assert.deepEqual(all.A.tables.map((t) => t.name), ["Sales", "sales_2"], "the source is not touched");
});

const payload = (name, range) => ({ name, range, header: true, totals: false, columns: ["Region", "Q1", "Q2"] });

test("the tables of a sheet go to the engine with their names and columns, next to the merges", async () => {
  const { host } = await createWorkspaceHarness();
  const spill = host.__EASYOFFICE_SHEET_SPILL__;
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ S: { merges: ["E1:F1"], tables: [sales("A", "A1:C4"), sales("B", "H2:J5")] } });
  assert.deepEqual(spill.blockerPayload("tables", "S", layout), [payload("A", "A1:C4"), payload("B", "H2:J5")]);
  assert.deepEqual(spill.blockerPayload("merges", "S", layout), ["E1:F1"]);
  assert.deepEqual(spill.mergesField(["S", "T"], layout), {
    merges: { S: ["E1:F1"] },
    tables: { S: [payload("A", "A1:C4"), payload("B", "H2:J5")] },
  });
  assert.deepEqual(spill.mergesField(["T"], layout), {});
  layout.replaceAll({});
});

function sessionStore(sandbox, answers) {
  const calls = [];
  const invokeIpc = async (command, payload) => {
    const body = JSON.parse(payload);
    calls.push(body);
    if (body.sessionAction === "open") return { session: "s1" };
    if (body.sessionAction === "close") return { closed: true };
    if (body.sessionAction === "tables") return answers.tables(body);
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

test("a session is told the tables before the cells, once per change, and folds the blocked array into the model", async () => {
  const { host, sandbox } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  let blocked = false;
  const { store, calls } = sessionStore(sandbox, {
    tables: () => ({
      session: "s1",
      changed: blocked
        ? [{ sheet: "Sheet1", cell: "B1", value: "#SPILL!" }, { sheet: "Sheet1", cell: "B2", value: "" }]
        : [{ sheet: "Sheet1", cell: "B1", value: "1" }],
      spills: [{ sheet: "Sheet1", cell: "B1", rows: blocked ? 2 : 0, cols: 1, blocked }],
      circular: false,
    }),
  });
  const sheets = { Sheet1: { B1: entry("1", "=SEQUENCE(2)"), B2: { value: "2", formula: "", spillOf: "B1" } } };
  sheets.Sheet1.B1.spillRows = 2;
  sheets.Sheet1.B1.spillCols = 1;
  layout.replaceAll({ Sheet1: { tables: [sales("T", "A2:C5")] } });
  blocked = true;
  const live = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(calls.map((c) => c.sessionAction).slice(0, 3), ["open", "tables", "apply"], "tables go before the cells");
  assert.deepEqual(calls[1].tables, [payload("T", "A2:C5")]);
  assert.equal(live.spillsChanged, true);
  assert.equal(sheets.Sheet1.B2, undefined, "the blocked array gave its cell up");
  assert.equal(sheets.Sheet1.B1.value, "#SPILL!");
  calls.length = 0;
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(calls.map((c) => c.sessionAction), ["apply"], "nothing changed: not sent again");
  // A table that moves is sent at its new place; one that goes is sent as an empty list.
  layout.setSheet("Sheet1", { tables: [sales("T", "A4:C7")] });
  blocked = false;
  calls.length = 0;
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(calls.map((c) => c.sessionAction), ["tables", "apply"]);
  assert.deepEqual(calls[0].tables, [payload("T", "A4:C7")]);
  layout.setSheet("Sheet1", { tables: [] });
  calls.length = 0;
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(calls[0].tables, []);
  layout.replaceAll({});
});

// A column of a table follows its header cell (PD-225).

const named = (columns) => ({ ...sales("Sales", "A1:C4"), columns });

test("a header cell with new text renames its column once, after the first look", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ S: { tables: [named(["Region", "Q1", "Q2"])] } });
  const cells = { A1: entry("Region"), B1: entry("Q1"), C1: entry("Q2") };
  assert.deepEqual(layout.tableColumnRenames({ S: cells }), [], "the first look is the baseline");
  cells.B1 = entry("Sales Q1");
  assert.deepEqual(layout.tableColumnRenames({ S: cells }), [{ sheet: "S", table: "Sales", index: 1, from: "Q1", to: "Sales Q1" }]);
  // The same text is not a new rename, whether or not the page has given the column the name yet.
  assert.deepEqual(layout.tableColumnRenames({ S: cells }), []);
  assert.equal(layout.setTableColumn("S", "Sales", 1, "Sales Q1"), true);
  assert.deepEqual(layout.tablesOf("S")[0].columns, ["Region", "Sales Q1", "Q2"]);
  assert.equal(layout.setTableColumn("S", "Sales", 1, "Sales Q1"), false);
  assert.equal(layout.setTableColumn("S", "Nope", 1, "x"), false);
  // Back to the old text (an undo) is a rename back.
  cells.B1 = entry("Q1");
  assert.deepEqual(layout.tableColumnRenames({ S: cells }).map((r) => [r.from, r.to]), [["Sales Q1", "Q1"]]);
  layout.replaceAll({});
});

test("a header that is empty, a formula, a taken name or too long is left to the engine", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ S: { tables: [named(["Region", "Q1", "Q2"])] } });
  const cells = { A1: entry("Region"), B1: entry("Q1"), C1: entry("Q2") };
  layout.tableColumnRenames({ S: cells });
  cells.B1 = entry("");
  cells.C1 = entry("region");
  cells.A1 = entry("x".repeat(256));
  assert.deepEqual(layout.tableColumnRenames({ S: cells }), []);
  cells.B1 = entry("3", "=1+2");
  assert.deepEqual(layout.tableColumnRenames({ S: cells }), []);
  // A table with no header row has no header cells.
  layout.replaceAll({ S: { tables: [{ ...named(["a", "b", "c"]), header: false }] } });
  assert.deepEqual(layout.tableColumnRenames({ S: { A1: entry("z") } }), []);
  layout.replaceAll({});
});

test("the page takes the engine's rewrite of the formulas and then names the column", async () => {
  const { host, sandbox } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  const names = host.__EASYOFFICE_SHEET_NAMES__;
  layout.replaceAll({ S: { tables: [named(["Region", "Q1", "Q2"])] } });
  names.replaceAll([{ name: "Qs", refersTo: "Sales[Q1]" }]);
  const calls = [];
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: async (command, payload) => {
      calls.push(JSON.parse(payload));
      return { rewrites: [{ sheet: "S", cell: "E1", text: "=SUM(Sales[[Q 1]])" }], names: [{ name: "Qs", refersTo: "Sales[[Q 1]]" }] };
    },
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  const sheets = { S: { A1: entry("Region"), B1: entry("Q 1"), E1: entry("6", "=SUM(Sales[Q1])"), E2: entry("7", "=A1+1") } };
  const moved = await store.renameTableColumns([{ sheet: "S", table: "Sales", index: 1, from: "Q1", to: "Q 1" }], sheets);
  assert.equal(moved, true);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].sessionAction, "tableColumn");
  assert.deepEqual(calls[0].formulas, [{ sheet: "S", cell: "E1", text: "=SUM(Sales[Q1])" }], "only formulas with a bracket are sent");
  assert.equal(sheets.S.E1.formula, "=SUM(Sales[[Q 1]])");
  assert.equal(sheets.S.E1.value, "6", "the value waits for the recalculation");
  assert.deepEqual(layout.tablesOf("S")[0].columns, ["Region", "Q 1", "Q2"]);
  assert.equal(names.getAll()[0].refersTo, "Sales[[Q 1]]");
  // An answer the engine refused leaves the column and the formulas alone.
  const failing = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: async () => {
      throw new Error("refused");
    },
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  assert.equal(await failing.renameTableColumns([{ sheet: "S", table: "Sales", index: 2, from: "Q2", to: "Q 2" }], sheets), false);
  assert.deepEqual(layout.tablesOf("S")[0].columns, ["Region", "Q 1", "Q2"]);
  layout.replaceAll({});
  names.replaceAll([]);
});
