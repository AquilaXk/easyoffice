import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// A sheet name that reads as a cell in the A1 or the R1C1 style is quoted in front of `!`, so a
// chart or link reference to it points at the sheet.

const env = {};
loadUiModule(fileURLToPath(new URL("../crates/easyoffice-app/ui/sheet-utils.js", import.meta.url)), env);
const quote = env.__EASYOFFICE_SHEET_UTILS__.quoteFormulaSheetName;

test("names that read as cells are quoted", () => {
  for (const name of ["A1", "XFD10", "R1C1", "r1c1", "R", "C", "RC", "R12", "C3", "RC5", "R2C", "TRUE", "My Sheet", "1Q"]) {
    assert.equal(quote(name), `'${name}'`, name);
  }
});

test("plain names stay bare", () => {
  for (const name of ["Sheet1", "Report", "Rows", "Col", "Data_2026", "RCAA1"]) {
    assert.equal(quote(name), name, name);
  }
});
