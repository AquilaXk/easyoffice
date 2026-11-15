import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The function list of the totals row (PD-232): the `SUBTOTAL` each choice writes, the function a cell stands for, the menu
// that opens from the selected totals-row cell, and each choice as one undo step.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const api = loadUiModule(UI("sheet-table-totals.js"), {}).__EASYOFFICE_SHEET_TABLE_TOTALS__;
const T = { name: "표1", columns: ["부문", "매출", "분기 합계", "a.b", "it's"] };

test("each function writes the SUBTOTAL that skips filtered rows, and the cell reads back as that function", () => {
  const expected = { average: 101, count: 103, countNums: 102, max: 104, min: 105, sum: 109, stdDev: 107, var: 110 };
  for (const [id, code] of Object.entries(expected)) {
    const formula = api.formulaFor(T, 1, id);
    assert.equal(formula, `=SUBTOTAL(${code},표1[매출])`);
    assert.equal(api.functionOf(T, 1, { formula }), id);
  }
  assert.equal(api.formulaFor(T, 1, "none"), "");
  assert.equal(api.formulaFor(T, 1, "custom"), "");
});

test("a column with a special character is written in double brackets and quoted as the references do", () => {
  assert.equal(api.formulaFor(T, 2, "sum"), "=SUBTOTAL(109,표1[분기 합계])");
  assert.equal(api.formulaFor(T, 3, "sum"), "=SUBTOTAL(109,표1[[a.b]])");
  assert.equal(api.formulaFor(T, 4, "sum"), "=SUBTOTAL(109,표1[[it''s]])");
  for (const index of [2, 3, 4]) assert.equal(api.functionOf(T, index, { formula: api.formulaFor(T, index, "max") }), "max");
});

test("a cell that holds another formula is custom, a constant or an empty cell is none", () => {
  assert.equal(api.functionOf(T, 1, { formula: "=SUM(표1[매출])" }), "custom");
  assert.equal(api.functionOf(T, 1, { formula: "=SUBTOTAL(109,표1[부문])" }), "custom", "another column");
  assert.equal(api.functionOf(T, 1, { formula: "=SUBTOTAL(999,표1[매출])" }), "custom", "not a function number");
  assert.equal(api.functionOf(T, 1, { formula: "=SUBTOTAL(9,표1[매출])" }), "custom", "9 counts hidden rows");
  assert.equal(api.functionOf(T, 1, { value: "요약" }), "none");
  assert.equal(api.functionOf(T, 1, undefined), "none");
  // The file's own forms: the column without the table's name and the data-only form.
  assert.equal(api.functionOf(T, 1, { formula: "=SUBTOTAL(109,[매출])" }), "sum");
  assert.equal(api.functionOf(T, 1, { formula: "=subtotal( 109 , 표1[[#Data],[매출]] )" }), "sum");
});

test("a new totals row sums a column of numbers and counts any other", () => {
  assert.equal(api.defaultFunction(["1", "2.5", " -3 "]), "sum");
  assert.equal(api.defaultFunction(["1", "a"]), "count");
  assert.equal(api.defaultFunction([]), "count");
});

// ---- on the sheet ---------------------------------------------------------------------------------------------------------------

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const $ = (h, id) => h.document.getElementById(id);
const tables = (h) => h.host.__EASYOFFICE_SHEET_LAYOUT__.tablesOf("Sheet1");

async function openSheet() {
  const h = await createWorkspaceHarness({ transformHtml: demoHtml });
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  return h;
}

const undo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
};

async function tableWithTotals() {
  const h = await openSheet();
  cell(h.document, "B3").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-insert-table"));
  await flushAsyncTurns(4);
  $(h, "sheet-table-range").value = "A1:H8";
  $(h, "sheet-table-header").checked = true;
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(16);
  cell(h.document, "B3").focus();
  await flushAsyncTurns(2);
  const box = h.document.querySelector('#sheet-table-bar [data-table-opt="totals"]');
  box.checked = true;
  box.dispatchEvent({ type: "change", target: box, bubbles: true });
  await flushAsyncTurns(16);
  return h;
}

async function pressArrow(h, ref) {
  const td = cell(h.document, ref);
  td.focus();
  await flushAsyncTurns(2);
  td.getBoundingClientRect = () => ({ left: 100, right: 200, top: 0, bottom: 24 });
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, clientX: 195, clientY: 10, bubbles: true });
  await flushAsyncTurns(4);
}

const rows = (h) => [...$(h, "sheet-totals-menu").querySelectorAll(".md-item")].map((r) => r.querySelector(".md-label").textContent);
const checked = (h) => [...$(h, "sheet-totals-menu").querySelectorAll('[aria-checked="true"]')].map((r) => r.dataset.totalsFn);

test("the selected totals-row cell carries the arrow class and the menu lists the functions, the cell's own checked", async () => {
  const h = await tableWithTotals();
  assert.ok(cell(h.document, "H9").classList.contains("totals-cell"));
  assert.equal(cell(h.document, "H2").classList.contains("totals-cell"), false, "not a data cell");
  await pressArrow(h, "H9");
  assert.equal($(h, "sheet-totals-menu").hidden, false);
  assert.deepEqual(rows(h), ["없음", "평균", "개수", "숫자 개수", "최대값", "최소값", "합계", "표본 표준 편차", "표본 분산", "함수 더 보기..."]);
  assert.deepEqual(checked(h), ["count"], "a column of text was counted when the row was turned on");
});

test("a press away from the arrow, and on a data cell, opens nothing", async () => {
  const h = await tableWithTotals();
  const td = cell(h.document, "H9");
  td.focus();
  await flushAsyncTurns(2);
  td.getBoundingClientRect = () => ({ left: 100, right: 200, top: 0, bottom: 24 });
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, clientX: 120, clientY: 10, bubbles: true });
  await flushAsyncTurns(2);
  assert.ok(!$(h, "sheet-totals-menu") || $(h, "sheet-totals-menu").hidden);
  await pressArrow(h, "C3");
  assert.ok($(h, "sheet-totals-menu").hidden);
});

test("a choice writes the SUBTOTAL into the cell as one undo step, and none empties it", async () => {
  const h = await tableWithTotals();
  const pick = async (id) => {
    $(h, "sheet-totals-menu").querySelector(`[data-totals-fn="${id}"]`).click();
    await flushAsyncTurns(16);
  };
  await pressArrow(h, "C9");
  await pick("max");
  const column = tables(h)[0].columns[2];
  assert.equal(cell(h.document, "C9").dataset.formula, `=SUBTOTAL(104,표1[${column}])`);
  assert.equal($(h, "sheet-totals-menu").hidden, true, "the menu closes");
  await pressArrow(h, "C9");
  assert.deepEqual(checked(h), ["max"]);
  await pick("none");
  assert.equal(cell(h.document, "C9").dataset.formula || "", "");
  assert.equal(cell(h.document, "C9").textContent, "");
  await undo(h);
  assert.equal(cell(h.document, "C9").dataset.formula, `=SUBTOTAL(104,표1[${column}])`, "one undo gives the function back");
  await undo(h);
  assert.equal(cell(h.document, "C9").dataset.formula || "", "", "a second undo takes the choice away");
});

test("함수 더 보기 selects the cell and starts a formula in the formula bar", async () => {
  const h = await tableWithTotals();
  await pressArrow(h, "D9");
  $(h, "sheet-totals-menu").querySelector('[data-totals-fn="more"]').click();
  await flushAsyncTurns(8);
  assert.equal($(h, "formula-input").value, "=");
  assert.equal(tables(h)[0].totals, true);
});

test("Alt+Down on the totals-row cell opens the menu, Escape closes it, Enter picks the focused row", async () => {
  const h = await tableWithTotals();
  const td = cell(h.document, "B9");
  td.focus();
  await flushAsyncTurns(2);
  td.dispatchEvent({ type: "keydown", key: "ArrowDown", altKey: true, target: td, bubbles: true });
  await flushAsyncTurns(2);
  assert.equal($(h, "sheet-totals-menu").hidden, false);
  const menu = $(h, "sheet-totals-menu");
  menu.dispatchEvent({ type: "keydown", key: "Escape", target: menu, bubbles: true });
  assert.equal(menu.hidden, true);
});

test("a protected sheet keeps the menu shut", async () => {
  const h = await tableWithTotals();
  h.host.__EASYOFFICE_SHEET_LAYOUT__.setProtect("Sheet1", { on: true, deny: [], ranges: [] });
  await flushAsyncTurns(2);
  await pressArrow(h, "C9");
  assert.equal($(h, "sheet-totals-menu").hidden, true);
});
