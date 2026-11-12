import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { build, createSelection, p, rangeOf } from "./test-support/doc-run-harness.mjs";

// 표 도구 (doc-table.js, doc-table-grid.js): the table menu acts on the table the caret is in.

const row = (...cells) => ({ tag: "tr", children: cells.map((text) => ({ tag: "td", children: text === "" ? [{ tag: "br" }] : [text] })) });
const rowsOf = (table) => table.querySelectorAll("tr");
const cellAt = (table, r, c) => rowsOf(table)[r].children[c];
const textsOf = (table) => rowsOf(table).map((tr) => tr.children.map((td) => td.textContent).join("|"));

async function openWithTable(rows) {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection });
  switchModule(h.document, "DOC");
  await flushAsyncTurns(10);
  const editor = h.document.getElementById("doc-editor");
  editor.replaceChildren();
  build(h.document, editor, p("앞"));
  const table = build(h.document, editor, { tag: "table", children: [{ tag: "tbody", children: rows.map((r) => row(...r)) }] });
  table.className = "doc-inline-table";
  build(h.document, editor, p("뒤"));
  const caretIn = (r, c, endR = r, endC = c) => {
    const start = cellAt(table, r, c);
    const end = cellAt(table, endR, endC);
    selection.addRange(rangeOf(start, 0, end, 0));
  };
  const menu = h.document.querySelector('.menu-item[data-menu="table"]');
  const run = async (cmd) => {
    menu.dispatchEvent({ type: "mouseenter" });
    h.userClick(studioItem(h.document, cmd));
    await flushAsyncTurns(10);
  };
  return { ...h, selection, editor, table, caretIn, run, menu };
}

const GRID = [["a", "b", "c"], ["d", "e", "f"], ["g", "h", "i"]];

test("rows and columns are added above, below, left and right of the caret's cell", async () => {
  const t = await openWithTable(GRID);
  t.caretIn(1, 1);
  await t.run("table-row-above");
  assert.deepEqual(textsOf(t.table), ["a|b|c", "||", "d|e|f", "g|h|i"]);
  t.caretIn(3, 0);
  await t.run("table-add-row");
  assert.deepEqual(textsOf(t.table), ["a|b|c", "||", "d|e|f", "g|h|i", "||"]);
  t.caretIn(0, 0);
  await t.run("table-col-left");
  await t.run("table-col-right");
  assert.deepEqual(textsOf(t.table)[0], "||a|b|c");
  assert.deepEqual(rowsOf(t.table).map((tr) => tr.children.length), [5, 5, 5, 5, 5]);
});

test("a row added inside a rowspan grows the span instead of adding a cell", async () => {
  const t = await openWithTable(GRID);
  t.caretIn(0, 0, 1, 0);
  await t.run("table-merge");
  assert.equal(cellAt(t.table, 0, 0).rowSpan, 2);
  t.caretIn(1, 0);
  await t.run("table-row-above");
  assert.equal(cellAt(t.table, 0, 0).rowSpan, 3);
  assert.deepEqual(rowsOf(t.table).map((tr) => tr.children.length), [3, 2, 2, 3]);
});

test("a selection of cells merges into one cell with colspan and rowspan that the save payload carries, and splits back", async () => {
  const t = await openWithTable(GRID);
  t.caretIn(0, 0, 1, 1);
  await t.run("table-merge");
  const merged = cellAt(t.table, 0, 0);
  assert.equal(merged.colSpan, 2);
  assert.equal(merged.rowSpan, 2);
  assert.equal(merged.textContent, "abde");
  assert.deepEqual(rowsOf(t.table).map((tr) => tr.children.length), [2, 1, 3]);
  const block = t.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(t.editor).find((b) => b.type === "table");
  assert.deepEqual(block.merges, [{ row: 0, col: 0, rowSpan: 2, colSpan: 2 }]);
  t.caretIn(0, 0);
  await t.run("table-split");
  assert.equal(merged.colSpan, 1);
  assert.equal(merged.rowSpan, 1);
  assert.deepEqual(rowsOf(t.table).map((tr) => tr.children.length), [3, 3, 3]);
});

test("merge and split with nothing to do change nothing and say why", async () => {
  const t = await openWithTable(GRID);
  const before = textsOf(t.table);
  t.caretIn(1, 1);
  await t.run("table-merge");
  await t.run("table-split");
  assert.deepEqual(textsOf(t.table), before);
  assert.equal(t.document.getElementById("status-command-feedback").textContent, "나눌 셀 없음");
});

test("deleting a row or a column keeps the spans of cells that cross it, and the last one removes the table", async () => {
  const t = await openWithTable(GRID);
  t.caretIn(0, 0, 1, 0);
  await t.run("table-merge");
  t.caretIn(1, 1);
  await t.run("table-row-delete");
  assert.equal(cellAt(t.table, 0, 0).rowSpan, 1);
  assert.deepEqual(textsOf(t.table), ["ad|b|c", "g|h|i"]);
  t.caretIn(0, 1);
  await t.run("table-col-delete");
  assert.deepEqual(textsOf(t.table), ["ad|c", "g|i"]);
  t.caretIn(0, 0, 1, 1);
  await t.run("table-col-delete");
  assert.equal(t.editor.querySelector("table"), null);
});

test("a row deleted at the top of a merged cell moves the cell on into the next row", async () => {
  const t = await openWithTable(GRID);
  t.caretIn(0, 1, 1, 1);
  await t.run("table-merge");
  t.caretIn(0, 0);
  await t.run("table-row-delete");
  assert.equal(cellAt(t.table, 0, 1).rowSpan, 1);
  assert.deepEqual(textsOf(t.table), ["d|be|f", "g|h|i"]);
});

test("블록 합계 puts one sum per column under the block, or the row's sum right of it, adding what is missing", async () => {
  const t = await openWithTable([["1", "2"], ["3", "4"]]);
  t.caretIn(0, 0, 1, 1);
  await t.run("table-sum");
  assert.deepEqual(textsOf(t.table), ["1|2", "3|4", "4|6"]);
  t.caretIn(0, 0, 0, 1);
  await t.run("table-sum");
  assert.deepEqual(textsOf(t.table), ["1|2|3", "3|4|", "4|6|"]);
  t.caretIn(1, 1);
  await t.run("table-sum");
  assert.equal(cellAt(t.table, 1, 1).textContent, "2");
});

test("블록 합계 on text only changes nothing", async () => {
  const t = await openWithTable([["가", "나"], ["다", "라"]]);
  t.caretIn(0, 0, 1, 1);
  await t.run("table-sum");
  assert.equal(rowsOf(t.table).length, 2);
  assert.equal(t.document.getElementById("status-command-feedback").textContent, "합할 숫자 없음");
});

test("표 셀 수식 계산 turns the formulas of the caret's table into values in one undo step", async () => {
  const t = await openWithTable([["1", "2"], ["3", "4"], ["=SUM(ABOVE)", "=SUM(ABOVE)"]]);
  t.caretIn(0, 0);
  t.menu.dispatchEvent({ type: "mouseenter" });
  t.userClick(studioItem(t.document, "doc-mod-tableformula"));
  await flushAsyncTurns(20);
  assert.deepEqual(textsOf(t.table), ["1|2", "3|4", "4|6"]);
});

test("each table command is one undo step", async () => {
  const t = await openWithTable(GRID);
  t.caretIn(1, 1);
  await t.run("table-add-row");
  assert.equal(rowsOf(t.editor.querySelector("table")).length, 4);
  t.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(20);
  assert.equal(rowsOf(t.editor.querySelector("table")).length, 3);
});

test("Tab moves to the next cell, Shift+Tab to the previous one, and Tab in the last cell adds a row", async () => {
  const t = await openWithTable(GRID);
  const tab = (shiftKey = false) => t.editor.dispatchEvent({ type: "keydown", key: "Tab", shiftKey });
  t.caretIn(0, 2);
  assert.equal(tab(), false, "Tab is taken by the table");
  assert.equal(t.selection.getRangeAt().startContainer, cellAt(t.table, 1, 0));
  tab(true);
  assert.equal(t.selection.getRangeAt().startContainer, cellAt(t.table, 0, 2));
  t.caretIn(2, 2);
  tab();
  assert.equal(rowsOf(t.table).length, 4);
  assert.equal(t.selection.getRangeAt().startContainer, cellAt(t.table, 3, 0));
});

test("the table-only items are grayed out unless the caret is in a table", async () => {
  const t = await openWithTable(GRID);
  const grayed = () => ["table-row-above", "table-add-row", "table-col-left", "table-col-right", "table-row-delete", "table-col-delete", "table-split", "table-merge", "table-sum", "doc-mod-tableformula"]
    .map((cmd) => studioItem(t.document, cmd).getAttribute("aria-disabled"));
  t.caretIn(0, 0);
  t.menu.dispatchEvent({ type: "mouseenter" });
  assert.deepEqual(new Set(grayed()), new Set(["false"]));
  t.selection.addRange(rangeOf(t.editor.children[0], 0, t.editor.children[0], 0));
  t.menu.dispatchEvent({ type: "mouseenter" });
  assert.deepEqual(new Set(grayed()), new Set(["true"]));
  assert.equal(studioItem(t.document, "insert-table").getAttribute("aria-disabled"), "false");
});
