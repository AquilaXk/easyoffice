import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// 데이터 > 개요 > 부분합 (PD-391): the plan of the rows, then the dialog and the step on a sheet.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const { __EASYOFFICE_SHEET_SUBTOTAL__: subtotal } = loadUiModule(ui("sheet-subtotal.js"), {});

const colName = (col) => String.fromCharCode(64 + col);
const cell = (value, formula = "") => ({ value, formula });
const LIST = [
  [cell("항목"), cell("금액")],
  [cell("가"), cell("1")],
  [cell("가"), cell("2")],
  [cell("나"), cell("5")],
];
const options = (extra = {}) => ({ firstRow: 12, firstCol: 1, groupCol: 0, calcCols: [1], fn: subtotal.functionOf("sum"), colName, ...extra });
const texts = (rows) => rows.map((cells) => cells.map((entry) => (entry ? entry.formula || entry.value : "")));

test("the subtotal rows go below each group and a grand total below the list, with the groups that fold them", () => {
  const result = subtotal.plan(LIST, options());
  assert.deepEqual(texts(result.rows), [
    ["항목", "금액"],
    ["가", "1"],
    ["가", "2"],
    ["가 합계", "=SUBTOTAL(9,B13:B14)"],
    ["나", "5"],
    ["나 합계", "=SUBTOTAL(9,B16:B16)"],
    ["총합계", "=SUBTOTAL(9,B13:B17)"],
  ]);
  assert.deepEqual(result.groups, [[13, 17], [13, 14], [16, 16]]);
  assert.equal(result.rows[3][0].style.bold, true);
});

test("with the summaries above, each subtotal sits over its group and the grand total under the heading", () => {
  const result = subtotal.plan(LIST, options({ below: false, fn: subtotal.functionOf("average") }));
  assert.deepEqual(texts(result.rows), [
    ["항목", "금액"],
    ["총합계", "=SUBTOTAL(1,B14:B18)"],
    ["가 평균", "=SUBTOTAL(1,B15:B16)"],
    ["가", "1"],
    ["가", "2"],
    ["나 평균", "=SUBTOTAL(1,B18:B18)"],
    ["나", "5"],
  ]);
  assert.deepEqual(result.groups, [[14, 18], [15, 16], [18, 18]]);
});

test("old subtotals are replaced and 모두 제거 takes them out; a list without them has nothing to remove", () => {
  const first = subtotal.plan(LIST, options());
  const again = subtotal.plan(first.rows, options({ fn: subtotal.functionOf("count") }));
  assert.equal(again.rowCount, first.rowCount, "replaced, not added to");
  assert.equal(again.rows[3][1].formula, "=SUBTOTAL(3,B13:B14)");
  assert.equal(subtotal.plan(first.rows, options({ replace: false })).error, "kept");
  assert.deepEqual(texts(subtotal.removeAll(first.rows).rows), texts(LIST));
  assert.equal(subtotal.removeAll(LIST).error, "none");
});

test("a list with formulas, no data, or no chosen column is refused", () => {
  assert.equal(subtotal.plan([LIST[0], [cell("가"), cell("3", "=1+2")]], options()).error, "formulas");
  assert.equal(subtotal.plan([LIST[0]], options()).error, "rows");
  assert.equal(subtotal.plan(LIST, options({ calcCols: [] })).error, "columns");
  assert.equal(subtotal.plan(LIST, options({ groupCol: 5 })).error, "group");
  assert.equal(subtotal.plan(LIST, options({ fn: null })).error, "function");
});

async function openSheet() {
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const action = JSON.parse(args.payload).sessionAction;
        if (action === "structure") return { session: "s1", formulas: [], changed: [], circular: false, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules: {} };
        return { session: "s1", changed: [], circular: false };
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return harness;
}

async function type(h, ref, text) {
  const td = h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  assert.ok(td, "the cell " + ref + " is in the window");
  td.focus();
  for (const ch of text) {
    const active = h.document.activeElement;
    active.dispatchEvent({ type: "keydown", key: ch, target: active });
    active.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: active });
    active.textContent += ch;
    active.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: active });
  }
  td.dispatchEvent({ type: "keydown", key: "Tab", target: td });
  await flushAsyncTurns(15);
}

test("부분합 writes the rows over the list in one step with their outline, and 모두 제거 brings the list back", async () => {
  const h = await openSheet();
  const get = (id) => h.document.getElementById(id);
  const cellAt = (ref) => h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  for (const [ref, text] of [["A9", "item"], ["B9", "amount"], ["A10", "x"], ["B10", "1"], ["A11", "x"], ["B11", "2"], ["A12", "y"], ["B12", "5"]]) await type(h, ref, text);
  const select = async (text) => {
    h.userClick(h.document.querySelector('.md-item[data-sheet-act="goto"]'));
    await flushAsyncTurns(6);
    get("sheet-goto-ref").value = text;
    h.userClick(get("btn-sheet-goto-ok"));
    await flushAsyncTurns(10);
  };
  await select("A9:B12");
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="subtotal"]'));
  await flushAsyncTurns(6);
  const dialog = get("sheet-subtotal-dialog");
  assert.equal(dialog.open, true);
  assert.deepEqual([...get("sheet-subtotal-group").querySelectorAll("option")].map((o) => o.textContent), ["item", "amount"]);
  assert.deepEqual([...get("sheet-subtotal-fn").querySelectorAll("option")].map((o) => o.textContent).slice(0, 3), ["합계", "개수", "평균"]);
  assert.deepEqual([...dialog.querySelectorAll("#sheet-subtotal-columns input")].map((box) => box.checked), [false, true], "the last column is checked");
  h.userClick(get("btn-sheet-subtotal-ok"));
  await flushAsyncTurns(30);
  assert.equal(dialog.open, false);
  assert.equal(cellAt("A12").textContent, "x 합계");
  assert.equal(cellAt("B12").dataset.formula, "=SUBTOTAL(9,B10:B11)");
  assert.equal(cellAt("A14").textContent, "y 합계");
  assert.equal(cellAt("A15").textContent, "총합계");
  const sheet = Object.keys(h.host.__EASYOFFICE_SHEET_LAYOUT__.getAll())[0];
  assert.deepEqual(h.host.__EASYOFFICE_SHEET_LAYOUT__.outlineOf(sheet).rows, [[10, 11, 2], [12, 12, 1], [13, 13, 2], [14, 14, 1]]);

  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(30);
  assert.equal(cellAt("A12").textContent, "y", "one undo step takes the subtotal rows and the groups away");
  assert.equal((h.host.__EASYOFFICE_SHEET_LAYOUT__.outlineOf(sheet)?.rows || []).length, 0);
  h.userClick(studioItem(h.document, "studio-redo"));
  await flushAsyncTurns(30);
  assert.equal(cellAt("A12").textContent, "x 합계");

  await select("A9:B15");
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="subtotal"]'));
  await flushAsyncTurns(6);
  h.userClick(get("btn-sheet-subtotal-remove"));
  await flushAsyncTurns(30);
  assert.equal(cellAt("A12").textContent, "y");
  assert.equal(cellAt("B12").textContent, "5");
  assert.equal(cellAt("A13").textContent, "");
  assert.equal((h.host.__EASYOFFICE_SHEET_LAYOUT__.outlineOf(sheet)?.rows || []).length, 0, "the groups go with the subtotals");
});

test("refersInto sees a formula, name or chart text that points into the list, by sheet", () => {
  const colIndex = (letters) => [...letters].reduce((n, ch) => n * 26 + ch.charCodeAt(0) - 64, 0);
  const region = { r1: 9, c1: 1, r2: 12, c2: 2 };
  const into = (text, home = "S", sheet = "S") => subtotal.refersInto(text, { sheet, home, region, colIndex });
  assert.equal(into("=SUM(B10:B11)"), true);
  assert.equal(into("=A9*2"), true);
  assert.equal(into("=SUM($A$1:$B$40)"), true, "a range that covers the list");
  assert.equal(into("=SUM(B13:B20)"), false, "below the list");
  assert.equal(into("=SUM(C9:C12)"), false, "beside the list");
  assert.equal(into("=SUM(A:A)"), true, "a whole column");
  assert.equal(into("=SUM(10:11)"), true, "whole rows");
  assert.equal(into("=SUM(2:5)"), false);
  assert.equal(into("=B10", "T"), false, "the unprefixed cell of another sheet is that sheet's");
  assert.equal(into("=T!B10", "S"), false);
  assert.equal(into("='S'!B10", "T"), true);
  assert.equal(into("=S!B10+1", "T"), true);
  assert.equal(into("=LOG10(2)+B1"), false, "a function name is no cell");
});

test("a formula, a table or a name that points into the list keeps 부분합 from moving it", async () => {
  const h = await openSheet();
  const get = (id) => h.document.getElementById(id);
  const say = () => get("status-command-feedback").textContent;
  for (const [ref, text] of [["A9", "item"], ["B9", "amount"], ["A10", "x"], ["B10", "1"], ["A11", "y"], ["B11", "2"], ["D1", "=SUM(B10:B11)"]]) await type(h, ref, text);
  const select = async (range) => {
    h.userClick(h.document.querySelector('.md-item[data-sheet-act="goto"]'));
    await flushAsyncTurns(6);
    get("sheet-goto-ref").value = range;
    h.userClick(get("btn-sheet-goto-ok"));
    await flushAsyncTurns(10);
  };
  await select("A9:B11");
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="subtotal"]'));
  await flushAsyncTurns(10);
  assert.equal(get("sheet-subtotal-dialog").open, false);
  assert.equal(say(), "다른 수식·이름·차트가 이 목록을 참조해 쓸 수 없습니다");
});

test("a whole-sheet selection is cut to the data and a SUBTOTAL row the user built by hand is not taken for one of ours", async () => {
  const h = await openSheet();
  const get = (id) => h.document.getElementById(id);
  const select = async (range) => {
    h.userClick(h.document.querySelector('.md-item[data-sheet-act="goto"]'));
    await flushAsyncTurns(6);
    get("sheet-goto-ref").value = range;
    h.userClick(get("btn-sheet-goto-ok"));
    await flushAsyncTurns(10);
  };
  await select("A1:XFD1048576");
  const started = Date.now();
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="subtotal"]'));
  await flushAsyncTurns(10);
  assert.ok(Date.now() - started < 3000, "Ctrl+A reads the used range, not 17 billion cells");
  const feedback = get("status-command-feedback").textContent;
  assert.ok(get("sheet-subtotal-dialog").open || feedback !== "", "it opened or said why not: " + feedback);
  if (get("sheet-subtotal-dialog").open) get("sheet-subtotal-dialog").close();
  // by hand: a SUBTOTAL formula and a label of the user's own
  const own = [[{ value: "item" }, { value: "amount" }], [{ value: "x" }, { value: "1" }], [{ value: "합계표" }, { value: "", formula: "=SUBTOTAL(9,B2:B2)" }]];
  assert.equal(subtotal.isSubtotalRow(own[2]), false);
  assert.equal(subtotal.plan(own, options()).error, "formulas", "the hand-made row is a row of the list, and a list with formulas is refused");
  assert.equal(subtotal.isSubtotalRow([{ value: "x 합계" }, { value: "", formula: "=SUBTOTAL(9,B2:B3)" }]), true);
  assert.equal(subtotal.isSubtotalRow([{ value: "총합계" }, { value: "", formula: "=SUBTOTAL(9,B2:B3)" }]), true);
});
