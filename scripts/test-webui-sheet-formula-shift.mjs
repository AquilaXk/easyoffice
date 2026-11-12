import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

async function shifter() {
  const { document } = await createWorkspaceHarness();
  const win = document.defaultView;
  const grid = win.__EASYOFFICE_SHEET_GRID_UTILS__;
  const clip = win.__EASYOFFICE_CREATE_SHEET_CLIPBOARD__({
    grid,
    isFormulaStr: (s) => String(s).startsWith("="),
    getEntry: () => undefined,
    applyCellEdits: async () => {},
    clearExisting: async () => {},
  });
  return clip.shiftFormula;
}

test("relative references shift by the row and column offset, $ anchors stay", async () => {
  const shift = await shifter();
  assert.equal(shift("=A1+B2", 3, 0), "=A4+B5");
  assert.equal(shift("=A1+B2", 0, 2), "=C1+D2");
  assert.equal(shift("=$A$1+$A2+B$2", 3, 1), "=$A$1+$A5+C$2");
  assert.equal(shift("=SUM(A1:B2)", 2, 0), "=SUM(A3:B4)");
  assert.equal(shift("=$A$1:A5", 3, 0), "=$A$1:A8", "only the relative end of a mixed range moves");
});

test("a sheet prefix and its range move as one unit", async () => {
  const shift = await shifter();
  assert.equal(shift("=SUM(Sheet2!A1:A5)", 3, 0), "=SUM(Sheet2!A4:A8)");
  assert.equal(shift("='a b'!C3:C4+1", 2, 0), "='a b'!C5:C6+1");
  assert.equal(shift("='it''s'!B2", 1, 0), "='it''s'!B3");
  assert.equal(shift("=매출!B4*2", 1, 0), "=매출!B5*2");
  assert.equal(shift("=Sheet2!$A$1:A5", 3, 0), "=Sheet2!$A$1:A8");
});

test("column ranges and row ranges shift on their own axis", async () => {
  const shift = await shifter();
  assert.equal(shift("=SUM(A:A)", 5, 1), "=SUM(B:B)", "a column range ignores the row offset");
  assert.equal(shift("=SUM($A:B)", 0, 2), "=SUM($A:D)");
  assert.equal(shift("=SUM(1:1)", 4, 3), "=SUM(5:5)", "a row range ignores the column offset");
  assert.equal(shift("=SUM($2:3)", 2, 0), "=SUM($2:5)");
  assert.equal(shift("=SUM(Sheet2!A:A)", 0, 1), "=SUM(Sheet2!B:B)");
});

test("string literals, function names and names that merely look like references stay", async () => {
  const shift = await shifter();
  assert.equal(shift('="A1"&A1', 1, 0), '="A1"&A2');
  assert.equal(shift('=IF(A1>0,"B2 ""C3""",A1)', 1, 0), '=IF(A2>0,"B2 ""C3""",A2)');
  assert.equal(shift("=LOG10(A1)", 1, 0), "=LOG10(A2)");
  assert.equal(shift("=A1B+A1", 1, 0), "=A1B+A2");
  assert.equal(shift("=my_name1+A1", 1, 0), "=my_name1+A2");
  assert.equal(shift("=Sheet2.v1!A1", 1, 0), "=Sheet2.v1!A2");
});

test("references pushed outside the grid become #REF!", async () => {
  const shift = await shifter();
  assert.equal(shift("=A2+B5", -2, 0), "=#REF!+B3");
  assert.equal(shift("=SUM(A1:A5)", -1, 0), "=SUM(#REF!)");
  assert.equal(shift("=Sheet2!A1", -1, 0), "=Sheet2!#REF!");
  assert.equal(shift("=A1", 0, -1), "=#REF!");
  assert.equal(shift("=$A$1+A1", -1, -1), "=$A$1+#REF!");
});

test("a turned-over paste swaps the offsets of references that are relative in row and column, and leaves the rest as written", async () => {
  const shift = await shifter();
  const turn = { fromRow: 1, fromCol: 3, toRow: 10, toCol: 1 };
  assert.equal(shift("=A1+B1", 9, -2, turn), "=A8+A9", "offsets (0,-2) and (0,-1) become (-2,0) and (-1,0) from A10");
  assert.equal(shift("=C1", 9, -2, turn), "=A10", "a reference to the formula's own cell stays on it");
  assert.equal(shift("=SUM(A1:B2)", 9, -2, turn), "=SUM(A8:B9)", "both ends of a relative range swap");
  assert.equal(shift("=Sheet2!A1", 9, -2, turn), "=Sheet2!A8");
  assert.equal(shift("=$A1+A$1+$A$1", 9, -2, turn), "=$A1+A$1+$A$1", "a $ part keeps the reference as written");
  assert.equal(shift("=SUM($A1:B2)", 9, -2, turn), "=SUM($A1:B2)", "a range turns only when both ends are fully relative");
  assert.equal(shift("=SUM(A:A)+SUM(1:1)", 9, -2, turn), "=SUM(A:A)+SUM(1:1)", "a whole row or column stays");
  assert.equal(shift("=Z99", 9, -2, turn), "=CU33", "a reference outside the block turns the same way: offset (98,23) becomes (23,98)");
  assert.equal(shift('="A1"&LOG10(A1)', 9, -2, turn), '="A1"&LOG10(A8)');
  assert.equal(shift("=A1", 0, -2, { fromRow: 1, fromCol: 3, toRow: 1, toCol: 1 }), "=#REF!", "a swapped offset past the grid edge");
});
