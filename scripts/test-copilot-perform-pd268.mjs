import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadPerformParts, loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-268: where a stopped performance cuts the operations. A cell that reads as a value is written whole or not at all.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

function performer(withSheetUtils) {
  const env = {};
  if (withSheetUtils) {
    loadUiModule(UI("sheet-refs.js"), env);
    loadUiModule(UI("sheet-utils.js"), env);
  }
  loadPerformParts(env);
  loadUiModule(UI("copilot-perform.js"), env);
  return env.__EASYOFFICE_CREATE_COPILOT_PERFORMER__({ document: {}, getActiveSlideIndex: () => 0, getActiveSheetName: () => "Sheet1", blockText: (x) => String(x?.text ?? x) });
}

const write = (...raws) => [{ op: "sheet.write", cells: raws.map((raw, i) => ({ coord: "A" + (i + 1), raw })) }];

function cutInside(perf, text) {
  const ops = write("앞 셀", text, "뒤 셀");
  const targets = perf.collectTargets("SHEET", ops);
  const cut = perf.cutOperations(ops, targets, { target: targets[1], typed: 2 });
  return cut[0].cells.map((c) => c.raw);
}

for (const withSheetUtils of [false, true]) {
  test(`a cell that reads as a value is not cut in the middle (${withSheetUtils ? "with" : "without"} the sheet's own check)`, () => {
    const perf = performer(withSheetUtils);
    for (const text of ["10:30", "10:30:15", "2026-11-20", "2026/11/20 09:30", "(1,234)", "-12.5", "12.5%", "₩1,000", "$5.00", "1,234,567", "=SUM(A1:A3)", "TRUE", "false", "3:45 PM"]) {
      assert.deepEqual(cutInside(perf, text), ["앞 셀"], text + " is dropped whole");
    }
    assert.deepEqual(cutInside(perf, "합계 행"), ["앞 셀", "합계"], "a plain text keeps what was typed");
    assert.deepEqual(cutInside(perf, "A열 합계"), ["앞 셀", "A열"], "text that starts like a name is still text");
  });
}

test("a text cell keeps nothing when the stop came before its first letter", () => {
  const perf = performer(false);
  const ops = write("앞 셀", "합계", "뒤 셀");
  const targets = perf.collectTargets("SHEET", ops);
  assert.deepEqual(perf.cutOperations(ops, targets, { target: targets[1], typed: 0 })[0].cells.map((c) => c.raw), ["앞 셀"]);
});
