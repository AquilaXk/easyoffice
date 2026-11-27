import assert from "node:assert/strict";
import test from "node:test";
import { open, answer } from "./test-support/copilot-open.mjs";

// A copilot table operation that names a table (`tableName`) or a range that no table has fails; it does not act on the
// first table of the sheet instead.

async function sheetWithTwoTables() {
  const h = await open({ module: "SHEET", run: () => answer([]) });
  const ops = h.host.__EASYOFFICE_COPILOT_SHEET_OPS__;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const sheetName = "표 시트";
  const store = { [sheetName]: Object.create(null) };
  const rows = { [sheetName]: 12 };
  const put = (ref, value) => { store[sheetName][ref] = { value, formula: "" }; };
  for (const [ref, value] of [["A1", "지역"], ["B1", "매출"], ["A2", "서울"], ["B2", "1"], ["D1", "품목"], ["E1", "수량"], ["D2", "펜"], ["E2", "2"]]) put(ref, value);
  const run = (op) => {
    const locations = [];
    ops.executeExtendedSheetOp("sheet.table", op, { sheetName, sheetDataStore: store, sheetRowCounts: rows, locations });
    return locations;
  };
  run({ action: "create", range: "A1:B2", tableName: "첫표" });
  run({ action: "create", range: "D1:E2", tableName: "둘째표" });
  return { run, tables: () => layout.tablesOf(sheetName) };
}

test("a table named in the operation is the one that changes", async () => {
  const t = await sheetWithTwoTables();
  const locations = t.run({ action: "style", tableName: "둘째표", style: "TableStyleLight9" });
  assert.equal(locations.at(-1).label, "둘째표");
  assert.equal(t.tables().find((x) => x.name === "둘째표").style, "TableStyleLight9");
  assert.notEqual(t.tables().find((x) => x.name === "첫표").style, "TableStyleLight9");
});

test("a table range in the operation picks the table with that range", async () => {
  const t = await sheetWithTwoTables();
  const locations = t.run({ action: "style", range: "d1:e2", style: "TableStyleMedium3" });
  assert.equal(locations.at(-1).label, "둘째표");
});

test("a name or range that no table has fails and leaves every table alone", async () => {
  const t = await sheetWithTwoTables();
  const before = JSON.stringify(t.tables());
  assert.throws(() => t.run({ action: "style", tableName: "없는표", style: "TableStyleMedium2" }), /찾을 수 없습니다/);
  assert.throws(() => t.run({ action: "total_row", tableName: "없는표" }), /찾을 수 없습니다/);
  assert.throws(() => t.run({ action: "to_range", range: "H1:I9" }), /찾을 수 없습니다/);
  assert.throws(() => t.run({ action: "style", tableName: "없는표", range: "A1:B2", style: "TableStyleMedium2" }), /찾을 수 없습니다/, "a range that fits does not excuse a name that does not");
  assert.equal(JSON.stringify(t.tables()), before);
});

test("an operation with no name and no range still acts on the first table", async () => {
  const t = await sheetWithTwoTables();
  const locations = t.run({ action: "style", style: "TableStyleMedium4" });
  assert.equal(locations.at(-1).label, "첫표");
});
