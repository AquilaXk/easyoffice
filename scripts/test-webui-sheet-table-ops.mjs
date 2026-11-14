import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// The plans of the table commands (PD-230): making a table of a range, the names, giving a table a new range, and growing a
// table over what is typed under or beside it.

async function ops() {
  const { host } = await createWorkspaceHarness();
  const api = host.__EASYOFFICE_SHEET_TABLE_OPS__;
  const refOf = host.__EASYOFFICE_SHEET_REFS__.refOf;
  const box = (text) => api.parseRange(text, "Sheet1");
  const sheet = (cells = {}, extra = {}) => {
    const tables = extra.tables || [];
    const parse = host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef;
    return {
      tables,
      merges: extra.merges || [],
      filter: extra.filter || null,
      cellAt: (row, col) => (cells[refOf(row, col)] === undefined ? undefined : { value: cells[refOf(row, col)], formula: "" }),
      boxOf: (t) => {
        const [a, b] = t.range.split(":").map((r) => parse(r));
        return { r1: a.row, c1: a.col, r2: b.row, c2: b.col };
      },
    };
  };
  return { api, box, sheet };
}

const table = (name, range, extra = {}) => ({ name, range, header: true, totals: false, columns: ["a", "b", "c"].slice(0, extra.width ?? 3), style: "TableStyleMedium2", rowStripes: true, firstColumn: false, lastColumn: false, columnStripes: false, ...extra });

test("a range is read with dollars, a sheet name or one cell, and another sheet's range is told apart", async () => {
  const { api } = await ops();
  assert.deepEqual(api.parseRange("$A$1:$C$5", "Sheet1"), { r1: 1, c1: 1, r2: 5, c2: 3 });
  assert.deepEqual(api.parseRange("c5:a1", "Sheet1"), { r1: 1, c1: 1, r2: 5, c2: 3 });
  assert.deepEqual(api.parseRange("Sheet1!B2", "Sheet1"), { r1: 2, c1: 2, r2: 2, c2: 2 });
  assert.deepEqual(api.parseRange("'My ''s'!A1:B2", "My 's"), { r1: 1, c1: 1, r2: 2, c2: 2 });
  assert.equal(api.parseRange("Other!A1:B2", "Sheet1"), "sheet");
  for (const bad of ["", "A", "1:2", "A1:B2:C3", "A0:B2", "ZZZZ1:A2"]) assert.equal(api.parseRange(bad, "Sheet1"), null, bad);
});

test("the default names are 표N past the tables and Column<n>, and a repeated header gets a number", async () => {
  const { api } = await ops();
  assert.equal(api.defaultName([]), "표1");
  assert.equal(api.defaultName(["표1", "표2", "Sales"], 3), "표4", "one past the tables");
  assert.equal(api.defaultName(["표1", "표7"], 2), "표8", "and past the biggest number in use");
  assert.equal(api.defaultName(["표5"], 0), "표6");
  assert.equal(api.defaultName(["Sales"], 1), "표2");
  const used = new Set();
  assert.deepEqual(api.headerNames(["Region", "", "region", "Region", "Column2"], used), ["Region", "Column2", "region2", "Region3", "Column22"]);
});

test("a table is made of the range with its header row, banded rows and the default style, named 표1", async () => {
  const { api, box, sheet } = await ops();
  const cells = { A1: "Region", B1: "Q1", A2: "East", B2: "10" };
  const plan = api.planCreate({ box: box("A1:B2"), header: true, ctx: sheet(cells) });
  assert.equal(plan.error, undefined);
  assert.equal(plan.insertRow, false);
  assert.deepEqual(plan.table, { name: "표1", range: "A1:B2", header: true, totals: false, columns: ["Region", "Q1"], style: "TableStyleMedium2", firstColumn: false, lastColumn: false, rowStripes: true, columnStripes: false });
  assert.deepEqual(plan.headerWrites, [], "the header cells already say their names");
});

test("a header cell that is empty or repeated is written anew; a formula header is turned into its text", async () => {
  const { api, box, sheet } = await ops();
  const ctx = sheet({ A1: "x", B1: "", C1: "X", A2: "1" });
  const plan = api.planCreate({ box: box("A1:C2"), header: true, ctx });
  assert.deepEqual(plan.table.columns, ["x", "Column2", "X2"]);
  assert.deepEqual(plan.headerWrites, [{ row: 1, col: 2, text: "Column2" }, { row: 1, col: 3, text: "X2" }]);
  const withFormula = { ...ctx, cellAt: (r, c) => (r === 1 && c === 1 ? { value: "7", formula: "=3+4" } : ctx.cellAt(r, c)) };
  const again = api.planCreate({ box: box("A1:C2"), header: true, ctx: withFormula });
  assert.equal(again.headerWrites[0].text, "7");
});

test("without a header row a row opens above the range for Column1.., and the table covers both", async () => {
  const { api, box, sheet } = await ops();
  const plan = api.planCreate({ box: box("B3:C4"), header: false, ctx: sheet({ B3: "1", C3: "2", B4: "3" }) });
  assert.equal(plan.insertRow, true);
  assert.equal(plan.table.range, "B3:C5");
  assert.deepEqual(plan.table.columns, ["Column1", "Column2"]);
  assert.deepEqual(plan.headerWrites, [{ row: 3, col: 2, text: "Column1" }, { row: 3, col: 3, text: "Column2" }]);
});

test("one row with a header gets a data row below it", async () => {
  const { api, box, sheet } = await ops();
  const plan = api.planCreate({ box: box("A1:B1"), header: true, ctx: sheet({ A1: "a", B1: "b" }) });
  assert.equal(plan.table.range, "A1:B2");
});

test("a range that meets another table, a merge, the filter or the bound is refused with its reason", async () => {
  const { api, box, sheet } = await ops();
  const other = table("Other", "D2:F4");
  const make = (ctx, text = "C1:E3", header = true) => api.planCreate({ box: box(text), header, ctx });
  assert.equal(make(sheet({}, { tables: [other] })).error, api.MESSAGES.tables);
  assert.equal(make(sheet({}, { merges: [{ r1: 2, c1: 3, r2: 3, c2: 3 }] })).error, api.MESSAGES.merges);
  assert.equal(make(sheet({}, { filter: { r1: 1, c1: 1, r2: 9, c2: 3 } })).error, api.MESSAGES.filter);
  assert.equal(make(sheet(), "A1048576:B1048576").error, api.MESSAGES.bottom, "a header row with no row for data");
  assert.equal(make(sheet(), "A1048576:B1048576", false).error, api.MESSAGES.bottom, "and no room for the opened row");
  assert.equal(api.planCreate({ box: box("A1:B2"), header: true, ctx: sheet(), totalTables: 1000 }).error, api.MESSAGES.limit);
  assert.equal(api.planCreate({ box: box("A1:C2"), header: true, ctx: sheet(), totalColumns: 65535 }).error, api.MESSAGES.columns);
  // Beside another table is fine; and the row for a table with no header is where the range starts, so a table above blocks it only when it touches.
  assert.equal(make(sheet({}, { tables: [other] }), "A1:B3").error, undefined);
});

test("the table is resized by a range over its own: its header stays in its row, nothing else is overlapped", async () => {
  const { api, box, sheet } = await ops();
  const t = table("Sales", "A1:C4", { filter: [{ col: 2, values: ["x"] }, { col: 3, blank: true }], filterHidden: [[3, 3]] });
  const ctx = sheet({ A1: "a", B1: "b", C1: "c", D1: "Fresh", E1: "" }, { tables: [t, table("Next", "A8:B9", { width: 2 })] });
  const grown = api.planResize({ table: t, box: box("A1:E6"), ctx });
  assert.equal(grown.error, undefined);
  assert.equal(grown.table.range, "A1:E6");
  assert.deepEqual(grown.table.columns, ["a", "b", "c", "Fresh", "Column5"]);
  assert.deepEqual(grown.headerWrites, [{ row: 1, col: 5, text: "Column5" }]);
  assert.equal(grown.table.filterHidden, undefined, "the rows to hide are judged again");
  const shrunk = api.planResize({ table: t, box: box("A1:B3"), ctx });
  assert.deepEqual(shrunk.table.columns, ["a", "b"]);
  assert.deepEqual(shrunk.table.filter, [{ col: 2, values: ["x"] }], "a column that left takes its condition along");
  assert.equal(api.planResize({ table: t, box: box("A2:C4"), ctx }).error, api.MESSAGES.header);
  assert.equal(api.planResize({ table: t, box: box("A1:C9"), ctx }).error, api.MESSAGES.resizeTables);
  assert.equal(api.planResize({ table: t, box: box("E1:F4"), ctx }).error, api.MESSAGES.apart);
  assert.equal(api.planResize({ table: t, box: box("A1:C1"), ctx }).error, api.MESSAGES.small);
  assert.equal(api.planResize({ table: { ...t, totals: true }, box: box("A1:C2"), ctx }).error, api.MESSAGES.small);
  assert.equal(api.planResize({ table: t, box: box("A1:C5"), ctx: sheet({}, { tables: [t], merges: [{ r1: 5, c1: 1, r2: 5, c2: 2 }] }) }).error, api.MESSAGES.resizeMerges);
});

test("a cell typed right under a table takes its row in, a cell beside it takes a column in; not under a totals row", async () => {
  const { api, sheet } = await ops();
  const t = table("Sales", "A1:C4");
  const ctx = sheet({ A5: "East", D2: "x", D1: "Extra", A9: "far" }, { tables: [t] });
  const below = api.planExpand({ row: 5, col: 1, ctx });
  assert.equal(below.table.range, "A1:C5");
  assert.equal(below.to, "row");
  const beside = api.planExpand({ row: 2, col: 4, ctx });
  assert.equal(beside.table.range, "A1:D4");
  assert.deepEqual(beside.table.columns, ["a", "b", "c", "Column4"]);
  assert.deepEqual(beside.headerWrites, [{ row: 1, col: 4, text: "Column4" }]);
  // Typed in the header row beside the table: the text names the new column.
  const named = api.planExpand({ row: 1, col: 4, ctx });
  assert.deepEqual(named.table.columns, ["a", "b", "c", "Extra"]);
  assert.deepEqual(named.headerWrites, []);
  assert.equal(api.planExpand({ row: 9, col: 1, ctx }), null, "a gap is not adjacent");
  assert.equal(api.planExpand({ row: 5, col: 5, ctx }), null, "nor the corner");
  assert.equal(api.planExpand({ row: 5, col: 2, ctx: sheet({}, { tables: [t] }) }), null, "an empty cell is not content");
  assert.equal(api.planExpand({ row: 6, col: 1, ctx: sheet({ A6: "x" }, { tables: [{ ...t, totals: true }] }) }), null);
  assert.equal(api.planExpand({ row: 5, col: 1, ctx: sheet({ A5: "x" }, { tables: [{ ...t, totals: true, range: "A1:C4" }] }) }), null, "under a totals row");
  assert.equal(api.planExpand({ row: 5, col: 1, ctx: sheet({ A5: "x" }, { tables: [t], merges: [{ r1: 5, c1: 1, r2: 5, c2: 2 }] }) }), null);
  assert.equal(api.planExpand({ row: 5, col: 1, ctx: sheet({ A5: "x" }, { tables: [t, table("Under", "A5:B6", { width: 2 })] }) }), null);
  assert.equal(api.planExpand({ row: 5, col: 1, ctx: sheet({ A5: "x" }, { tables: [t], filter: { r1: 5, c1: 1, r2: 7, c2: 3 } }) }), null);
});
