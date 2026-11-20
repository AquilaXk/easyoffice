import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// A text value that starts with `=`, `+`, `-`, `@`, a tab or a carriage return is written with a leading
// apostrophe in CSV and TSV, so a spreadsheet program does not run it as a formula (OWASP CSV injection).
// A number keeps its sign.

const env = {};
loadUiModule(fileURLToPath(new URL("../crates/easyoffice-app/ui/base-model.js", import.meta.url)), env);
const M = env.__EASYOFFICE_BASE_MODEL__;

function table(values, header = "제목") {
  const state = M.createInitialBaseState();
  const tbl = state.tables[0];
  M.renameField?.(state, tbl.id, "fld-1", header);
  for (const v of values) M.addRecord(state, tbl.id, { "fld-1": v, "fld-4": typeof v === "string" ? 0 : v });
  return { state, tbl };
}

test("text that starts like a formula gets a leading apostrophe in CSV and TSV", () => {
  const triggers = ["=1+1", "+1+1", "-1+1", "@SUM(A1)", "\tcmd", "\rcmd"];
  const { state, tbl } = table(triggers);
  const csv = M.toDelimitedText(state, tbl.id, ",").replace("﻿", "");
  const rows = csv.split("\r\n").slice(1);
  assert.equal(rows[0].split(",")[0], "'=1+1");
  assert.equal(rows[1].split(",")[0], "'+1+1");
  assert.equal(rows[2].split(",")[0], "'-1+1");
  assert.equal(rows[3].split(",")[0], "'@SUM(A1)");
  assert.ok(csv.includes("'\tcmd"));
  assert.ok(csv.includes('"\'\rcmd"'));
  const tsv = M.toDelimitedText(state, tbl.id, "\t");
  assert.ok(tsv.includes("'=1+1\t"));
});

test("a negative number keeps its sign and ordinary text is untouched", () => {
  const { state, tbl } = table(["보통 글", "a=b"]);
  M.addRecord(state, tbl.id, { "fld-1": "숫자", "fld-4": -5 });
  const csv = M.toDelimitedText(state, tbl.id, ",");
  assert.ok(csv.includes("보통 글,"), csv);
  assert.ok(csv.includes("a=b,"));
  assert.ok(/숫자,,,-5(\r|$)/.test(csv), csv);
});

test("a field name that starts like a formula is neutralised in the header too", () => {
  const state = M.createInitialBaseState();
  const tbl = state.tables[0];
  M.addField(state, tbl.id, { name: "=HYPERLINK(\"x\")", type: "text" });
  const header = M.toDelimitedText(state, tbl.id, ",").split("\r\n")[0];
  assert.ok(header.includes("\"'=HYPERLINK(\"\"x\"\")\""), header);
});
