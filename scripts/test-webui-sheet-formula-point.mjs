import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// PD-279: point mode of the formula editor, the way the spreadsheet program the sheet follows works it: while a formula is
// typed, a click, a drag or an arrow writes the reference, F2 turns entry and edit, F4 cycles the `$`, and the references
// wear colours. The pure rules are in test-sheet-formula-point.mjs; here the keys, the mouse, the status and the sheets.

async function openSheet() {
  const h = await createWorkspaceHarness();
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  return h;
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const mode = (d) => d.getElementById("status-sheet-mode");
const bar = (d) => d.getElementById("formula-input");
const boxes = (d) => [...d.querySelectorAll("#sheet-viewport .sheet-ref-box")];

// What the browser does for each character typed in the focused cell.
function typeText(d, text) {
  for (const ch of text) {
    const td = d.activeElement;
    assert.equal(key(td, ch), true, "printable keys are not cancelled");
    td.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: td });
    td.textContent += ch;
    td.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: td });
  }
}

// Types into the formula bar.
function typeBar(d, text) {
  const input = bar(d);
  input.focus();
  for (const ch of text) {
    key(input, ch);
    input.value += ch;
    input.dispatchEvent({ type: "input", target: input });
  }
}

// A press and release on a cell; `false` when the page took it as a pointing (the focus stays where it is).
function click(d, ref, extra = {}) {
  const td = cell(d, ref);
  const kept = td.dispatchEvent({ type: "mousedown", target: td, button: 0, ...extra }) === false;
  td.dispatchEvent({ type: "mouseup", target: td });
  return kept;
}

function headerClick(d, selector, extra = {}) {
  const th = d.querySelector(selector);
  const kept = th.dispatchEvent({ type: "mousedown", target: th, button: 0, ...extra }) === false;
  th.dispatchEvent({ type: "mouseup", target: th });
  return kept;
}

// A tab press the way the browser takes it: a press that is not cancelled moves the focus to the tab.
function clickTab(d, name, extra = {}) {
  const tab = d.querySelector(`#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="${name}"]`);
  if (tab.dispatchEvent({ type: "mousedown", target: tab, button: 0, ...extra })) d.activeElement?.blur?.();
  tab.click();
}

test("a click on a cell after = writes its reference and a click after an operator adds the next", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=");
  assert.equal(mode(document).textContent, "입력");
  assert.equal(click(document, "C3"), true, "the press is kept in the cell being edited");
  assert.equal(b9.textContent, "=C3");
  assert.equal(document.activeElement, b9);
  assert.equal(mode(document).textContent, "참조");
  typeText(document, "+");
  assert.equal(mode(document).textContent, "입력", "typing ends the pointing");
  click(document, "D3");
  assert.equal(b9.textContent, "=C3+D3");
  key(b9, "Enter");
  await flushAsyncTurns(12);
  assert.equal(cell(document, "B9").dataset.formula, "=C3+D3");
  assert.equal(cell(document, "B9").textContent, String(980 + 1040));
  assert.equal(mode(document).hidden, true, "no mode shows once the cell is entered");
});

test("a second click while pointing replaces the reference, and a click after ) enters the cell and selects the one clicked", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  click(document, "C3");
  click(document, "C4");
  assert.equal(b9.textContent, "=SUM(C4");
  typeText(document, ")");
  click(document, "C5");
  await flushAsyncTurns(12);
  assert.equal(document.getElementById("active-cell-ref").textContent, "C5");
  assert.equal(cell(document, "B9").dataset.formula, "=SUM(C4)");
});

test("a drag writes a range, Shift+click widens it from the pointed cell, and a merged or header press has the usual forms", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  const c3 = cell(document, "C3");
  const e5 = cell(document, "E5");
  assert.equal(c3.dispatchEvent({ type: "mousedown", target: c3, button: 0 }), false);
  e5.dispatchEvent({ type: "mouseover", target: e5 });
  assert.equal(b9.textContent, "=SUM(C3:E5)".replace(")", ""), "the drag writes the range while it moves");
  e5.dispatchEvent({ type: "mouseup", target: e5 });
  click(document, "F6", { shiftKey: true });
  assert.equal(b9.textContent, "=SUM(C3:F6", "Shift+click keeps the first cell and moves the far corner");
  headerClick(document, '#pane-sheet th[data-col="D"]');
  assert.equal(b9.textContent, "=SUM(D:D");
  headerClick(document, '#pane-sheet th[data-col="F"]', { shiftKey: true });
  assert.equal(b9.textContent, "=SUM(D:F");
  headerClick(document, '#pane-sheet th[data-row="4"]');
  assert.equal(b9.textContent, "=SUM(4:4");
});

test("arrow keys write the cell next to the edited one, move it, widen it with Shift and jump with Ctrl", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=");
  key(b9, "ArrowRight");
  assert.equal(b9.textContent, "=C9", "the first arrow writes the cell it reaches from the edited cell");
  assert.equal(document.getElementById("active-cell-ref").textContent, "B9", "the cell being edited stays selected");
  key(b9, "ArrowUp");
  key(b9, "ArrowUp");
  assert.equal(b9.textContent, "=C7");
  key(b9, "ArrowRight", { shiftKey: true });
  key(b9, "ArrowUp", { shiftKey: true });
  assert.equal(b9.textContent, "=C6:D7", "Shift keeps the first cell (C7) and widens to the far one (D6)");
  key(b9, "ArrowLeft");
  assert.equal(b9.textContent, "=C6", "an arrow without Shift moves the far cell and leaves a single cell");
  key(b9, "ArrowUp", { ctrlKey: true });
  assert.equal(b9.textContent, "=C1", "Ctrl jumps to the edge of the data");
  assert.equal(mode(document).textContent, "참조");
});

test("an arrow also moves the reference that was typed, and commits where no reference is awaited", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=C3");
  key(b9, "ArrowDown");
  assert.equal(b9.textContent, "=C4", "a reference at the caret is the one the arrow moves");
  typeText(document, "+1)");
  key(b9, "ArrowDown");
  await flushAsyncTurns(12);
  assert.equal(document.getElementById("active-cell-ref").textContent, "B10", "after ) the arrow commits and moves as before");
  assert.equal(cell(document, "B9").dataset.formula, "=C4+1)");
});

test("in the edit mode arrows move the caret, and F2 turns the entry and the edit", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=");
  assert.equal(mode(document).textContent, "입력");
  key(b9, "F2");
  assert.equal(mode(document).textContent, "편집");
  assert.equal(b9.dataset.editMode, "edit");
  assert.equal(key(b9, "ArrowRight"), true, "the arrow is left to the caret");
  assert.equal(b9.textContent, "=");
  key(b9, "F2");
  assert.equal(mode(document).textContent, "입력");
  key(b9, "ArrowRight");
  assert.equal(b9.textContent, "=C9");
  key(b9, "F2");
  assert.equal(mode(document).textContent, "편집", "from the pointing F2 goes to the edit");
  assert.equal(key(b9, "ArrowRight"), true);
  assert.equal(b9.textContent, "=C9");
  click(document, "D4");
  assert.equal(mode(document).textContent, "참조", "a click points in the edit mode too");
  key(b9, "ArrowRight");
  assert.equal(b9.textContent, "=E4", "and the arrows go on moving that reference");
});

test("F4 cycles the reference at the caret in the cell and in the formula bar", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=C3+D4");
  const seen = [];
  for (let i = 0; i < 4; i += 1) {
    assert.equal(key(b9, "F4"), false, "F4 is taken while a cell is edited");
    seen.push(b9.textContent);
  }
  assert.deepEqual(seen, ["=C3+$D$4", "=C3+D$4", "=C3+$D4", "=C3+D4"]);
  assert.equal(b9.dataset.formula, "=C3+D4");
  key(b9, "Escape");

  typeBar(document, "=B2");
  key(bar(document), "F4");
  assert.equal(bar(document).value, "=$B$2");
  key(bar(document), "F4");
  assert.equal(bar(document).value, "=B$2");
});

test("pointing works in the formula bar and Enter there enters the formula", async () => {
  const { document } = await openSheet();
  cell(document, "B9").focus();
  typeBar(document, "=SUM(");
  assert.equal(mode(document).textContent, "편집");
  assert.equal(click(document, "C3"), true, "the bar keeps the focus");
  click(document, "C5", { shiftKey: true });
  assert.equal(bar(document).value, "=SUM(C3:C5");
  assert.equal(mode(document).textContent, "참조");
  typeBar(document, ")");
  key(bar(document), "Enter");
  await flushAsyncTurns(12);
  assert.equal(cell(document, "B9").dataset.formula, "=SUM(C3:C5)");
  assert.equal(cell(document, "B9").textContent, String(980 + 720 + 890));
  assert.equal(mode(document).hidden, true);
});

test("Esc drops the formula with its references, boxes and mode", async () => {
  const { document } = await openSheet();
  const c3 = cell(document, "C3");
  const before = c3.textContent;
  c3.focus();
  typeText(document, "=");
  click(document, "B4");
  assert.equal(c3.textContent, "=B4");
  assert.equal(boxes(document).length, 1);
  key(c3, "Escape");
  await flushAsyncTurns(12);
  assert.equal(cell(document, "C3").textContent, before, "Esc drops the formula");
  assert.equal(cell(document, "C3").dataset.formula, undefined);
  assert.equal(boxes(document).length, 0);
  assert.equal(mode(document).hidden, true);
});

test("one undo takes back a formula entered by pointing", async () => {
  const { document, sandbox } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=");
  click(document, "C3");
  typeText(document, "*");
  click(document, "D3");
  key(b9, "Enter");
  await flushAsyncTurns(12);
  assert.equal(cell(document, "B9").dataset.formula, "=C3*D3");
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(12);
  assert.equal(cell(document, "B9").dataset.formula, undefined);
  assert.equal(cell(document, "B9").textContent, "");
});

test("each reference of the formula gets a box in its colour, the same text keeps its colour", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=C3+D4:E5+C3");
  assert.deepEqual(boxes(document).map((b) => b.dataset.color), ["0", "1", "0"]);
  const [first, second] = boxes(document);
  assert.match(first.style.width, /px$/);
  assert.notEqual(first.style.left, second.style.left);
  assert.equal(bar(document).classList.contains("colored"), true, "the bar shows the colours too");
  const text = [...document.querySelectorAll("#sheet-formula-colors span[data-color]")].map((s) => s.dataset.color + s.textContent);
  assert.deepEqual(text, ["0C3", "1D4:E5", "0C3"]);
  key(b9, "Escape");
  assert.equal(boxes(document).length, 0, "the boxes go with the edit");
  assert.equal(bar(document).classList.contains("colored"), false);
});

test("the colours are the series colours of the charts", async () => {
  const { document, host } = await openSheet();
  const palette = host.__EASYOFFICE_SHEET_CHART_SVG__.CHART_PALETTE;
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=C3");
  const pane = document.getElementById("pane-sheet");
  palette.forEach((color, i) => assert.equal(pane.style.getPropertyValue("--sheet-ref-" + i), color));
});

test("a tab pressed while a reference is awaited keeps the entry, the other sheet is pointed with its name and Enter comes back", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet2");
  assert.equal(bar(document).value, "=", "the entry goes on in the formula bar");
  assert.equal(document.activeElement, bar(document));
  assert.equal(click(document, "B1"), true);
  assert.equal(bar(document).value, "=Sheet2!B1");
  assert.equal(boxes(document).length, 1, "the reference to this sheet is boxed here");
  typeBar(document, "+");
  click(document, "A1");
  assert.equal(bar(document).value, "=Sheet2!B1+Sheet2!A1");
  key(bar(document), "Enter");
  await flushAsyncTurns(40);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet1", "Enter goes back to the sheet of the entry");
  assert.equal(cell(document, "B9").dataset.formula, "=Sheet2!B1+Sheet2!A1");
  assert.equal(cell(document, "B9").textContent, "350");
  assert.equal(document.getElementById("active-cell-ref").textContent, "B10");
});

test("a sheet name with a blank or a sign is quoted, Esc from the other sheet drops the entry", async () => {
  const { document } = await openSheet();
  const book = document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]');
  assert.ok(book);
  const b9 = cell(document, "B9");
  const before = b9.textContent;
  b9.focus();
  typeText(document, "=SUM(");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  click(document, "A1");
  click(document, "B1", { shiftKey: true });
  assert.equal(bar(document).value, "=SUM(Sheet2!A1:B1");
  key(bar(document), "Escape");
  await flushAsyncTurns(40);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet1");
  assert.equal(cell(document, "B9").textContent, before);
  assert.equal(cell(document, "B9").dataset.formula, undefined);
  assert.equal(mode(document).hidden, true);
});

test("a tab pressed where no reference is awaited enters the cell as before", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=1+2");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet2");
  clickTab(document, "Sheet1");
  await flushAsyncTurns(20);
  assert.equal(cell(document, "B9").dataset.formula, "=1+2");
});

test("an entry that started in the formula bar goes on over another sheet and comes back to the bar", async () => {
  const { document } = await openSheet();
  cell(document, "B9").focus();
  typeBar(document, "=SUM(");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  assert.equal(bar(document).value, "=SUM(");
  click(document, "A1");
  click(document, "B1", { shiftKey: true });
  assert.equal(bar(document).value, "=SUM(Sheet2!A1:B1");
  typeBar(document, ")");
  key(bar(document), "Enter");
  await flushAsyncTurns(40);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet1");
  assert.equal(cell(document, "B9").dataset.formula, "=SUM(Sheet2!A1:B1)");
  assert.equal(cell(document, "B9").textContent, "350");
});

test("while the entry is out on another sheet a click after a finished formula enters it on its own sheet and selects the cell clicked", async () => {
  const { document } = await openSheet();
  cell(document, "B9").focus();
  typeBar(document, "=SUM(");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  click(document, "A1");
  typeBar(document, ")");
  assert.equal(mode(document).textContent, "편집");
  assert.equal(click(document, "B1"), true, "the press is taken");
  await flushAsyncTurns(60);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet2", "the sheet clicked stays on show");
  assert.equal(document.getElementById("active-cell-ref").textContent, "B1", "and the cell clicked is the one selected");
  assert.equal(mode(document).hidden, true, "the entry is over");
  assert.equal(boxes(document).length, 0);
  clickTab(document, "Sheet1");
  await flushAsyncTurns(40);
  assert.equal(cell(document, "B9").dataset.formula, "=SUM(Sheet2!A1)", "the cell of the entry took the formula");
  assert.equal(cell(document, "B9").textContent, "100");
});

test("a header press after a finished formula on another sheet enters the formula and selects that column", async () => {
  const { document } = await openSheet();
  cell(document, "B9").focus();
  typeBar(document, "=SUM(");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  click(document, "A1");
  typeBar(document, ")");
  headerClick(document, '#pane-sheet th[data-col="B"]');
  await flushAsyncTurns(60);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet2");
  assert.equal(document.getElementById("active-cell-ref").textContent, "B1");
  assert.equal(mode(document).hidden, true, "the entry is over");
  assert.notEqual(document.activeElement, bar(document), "and the bar lets go of the focus");});

const head = (d, axis, index) => d.querySelector(axis === "col" ? `#pane-sheet th[data-col="${index}"]` : `#pane-sheet th[data-row="${index}"]`);
const over = (el) => el.dispatchEvent({ type: "mouseover", target: el });
const press = (el, extra = {}) => el.dispatchEvent({ type: "mousedown", target: el, button: 0, ...extra });
const release = (el) => el.dispatchEvent({ type: "mouseup", target: el });

test("a drag over the column headers writes the columns, a drag over the row headers the rows", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  assert.equal(press(head(document, "col", "B")), false, "the press is kept in the cell being edited");
  assert.equal(b9.textContent, "=SUM(B:B");
  over(head(document, "col", "D"));
  assert.equal(b9.textContent, "=SUM(B:D", "the drag widens to the header it is over");
  over(cell(document, "F6"));
  assert.equal(b9.textContent, "=SUM(B:D", "a cell is not a column header");
  over(head(document, "col", "C"));
  assert.equal(b9.textContent, "=SUM(B:C", "and it narrows again");
  release(head(document, "col", "C"));
  over(head(document, "col", "F"));
  assert.equal(b9.textContent, "=SUM(B:C", "no drag goes on after the release");
  press(head(document, "row", "2"));
  over(head(document, "row", "4"));
  assert.equal(b9.textContent, "=SUM(2:4");
  release(head(document, "row", "4"));
  assert.equal(boxes(document).length, 1);
  typeText(document, ")");
  key(b9, "Enter");
  await flushAsyncTurns(12);
  assert.equal(cell(document, "B9").dataset.formula, "=SUM(2:4)");
});

test("a column drag keeps its first column when the press had Shift and does not start from a cell", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  press(head(document, "col", "C"));
  release(head(document, "col", "C"));
  press(head(document, "col", "F"), { shiftKey: true });
  over(head(document, "col", "G"));
  assert.equal(b9.textContent, "=SUM(C:G");
  release(head(document, "col", "G"));
  press(cell(document, "C3"));
  over(head(document, "col", "H"));
  assert.equal(b9.textContent, "=SUM(C3", "a drag from a cell is no column drag");
});

test("a merged range is pointed as a whole, by a click on any of its cells and by a drag that reaches it", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_LAYOUT__.addMerge("Sheet1", 8, 3, 9, 4);
  const b12 = cell(document, "B12");
  b12.focus();
  typeText(document, "=");
  click(document, "C8");
  assert.equal(b12.textContent, "=C8:D9", "the whole range, from the cell at its top left");
  click(document, "D9");
  assert.equal(b12.textContent, "=C8:D9", "from the cell at its bottom right too");
  typeText(document, "+");
  press(cell(document, "B8"));
  over(cell(document, "C8"));
  assert.equal(b12.textContent, "=C8:D9+B8:D9", "a drag that reaches the merged range takes all of it");
  release(cell(document, "C8"));
  typeText(document, "+");
  click(document, "B10");
  click(document, "C9", { shiftKey: true });
  assert.equal(b12.textContent, "=C8:D9+B8:D9+B8:D10", "Shift from B10 to a cell of the merged range takes the range whole");
});

// The table `표1` over A1:G6 of the sample sheet: a header on row 1, data on rows 2..6.
const COLUMNS = ["부문", "1분기", "2분기", "3분기", "4분기", "연간합계", "목표대비"];
const TABLE = { name: "표1", range: "A1:G6", header: true, totals: false, columns: COLUMNS, style: "TableStyleMedium2", rowStripes: true };

async function openTable() {
  const h = await openSheet();
  h.host.__EASYOFFICE_SHEET_LAYOUT__.setTables("Sheet1", [TABLE]);
  return h;
}

test("the data of a table column is pointed as the structured reference of the column", async () => {
  const { document } = await openTable();
  const h10 = cell(document, "H10");
  h10.focus();
  typeText(document, "=SUM(");
  press(cell(document, "C2"));
  over(cell(document, "C6"));
  assert.equal(h10.textContent, "=SUM(표1[2분기]");
  over(cell(document, "D6"));
  assert.equal(h10.textContent, "=SUM(표1[[2분기]:[3분기]]");
  release(cell(document, "D6"));
  assert.equal(boxes(document).length, 1, "the pointed cells are boxed");
  assert.equal(boxes(document)[0].dataset.color, "0");
  click(document, "B3");
  assert.equal(h10.textContent, "=SUM(B3", "a cell of another row is an A1 reference");
  press(cell(document, "B2"));
  over(cell(document, "B5"));
  assert.equal(h10.textContent, "=SUM(B2:B5", "part of a column is an A1 range");
  release(cell(document, "B5"));
  press(cell(document, "B1"));
  over(cell(document, "B6"));
  assert.equal(h10.textContent, "=SUM(표1[[#All],[1분기]]", "header plus all data in a table without totals becomes #All");
  release(cell(document, "B6"));
  press(cell(document, "B1"));
  over(cell(document, "B5"));
  assert.equal(h10.textContent, "=SUM(B1:B5", "header plus partial data stays an A1 range");
  release(cell(document, "B5"));
  press(cell(document, "B2"));
  over(cell(document, "B6"));
  typeText(document, ")");
  assert.equal(h10.textContent, "=SUM(표1[1분기])");
  key(h10, "Enter");
  await flushAsyncTurns(12);
  assert.equal(cell(document, "H10").dataset.formula, "=SUM(표1[1분기])");
});

test("a second click replaces a structured reference, Shift widens it, and the status says reference", async () => {
  const { document } = await openTable();
  const h10 = cell(document, "H10");
  h10.focus();
  typeText(document, "=");
  press(cell(document, "C2"));
  over(cell(document, "C6"));
  release(cell(document, "C6"));
  assert.equal(mode(document).textContent, "참조");
  click(document, "F4");
  assert.equal(h10.textContent, "=F4", "a click replaces the structured reference at the caret");
  press(cell(document, "C2"));
  over(cell(document, "C6"));
  release(cell(document, "C6"));
  click(document, "E6", { shiftKey: true });
  assert.equal(h10.textContent, "=표1[[2분기]:[4분기]]", "Shift keeps the first cell of the reference and moves the far one");
  key(h10, "ArrowLeft", { shiftKey: true });
  assert.equal(h10.textContent, "=표1[[2분기]:[3분기]]", "Shift and an arrow move the far cell the reference was pointed to");
});

test("a cell of the formula's own row is #This Row outside the table and @ inside it", async () => {
  const { document } = await openTable();
  const h3 = cell(document, "H3");
  h3.focus();
  typeText(document, "=");
  click(document, "C3");
  assert.equal(h3.textContent, "=표1[[#This Row],[2분기]]");
  typeText(document, "*");
  press(cell(document, "B3"));
  over(cell(document, "C3"));
  assert.equal(h3.textContent, "=표1[[#This Row],[2분기]]*표1[[#This Row],[1분기]:[2분기]]");
  key(h3, "Escape");
  const g3 = cell(document, "G3");
  g3.focus();
  typeText(document, "=");
  click(document, "E3");
  assert.equal(g3.textContent, "=[@4분기]");
  typeText(document, "+");
  click(document, "F3");
  assert.equal(g3.textContent, "=[@4분기]+[@연간합계]");
  click(document, "F4");
  assert.equal(g3.textContent, "=[@4분기]+F4", "another row of the table is an A1 reference");
});

test("a table of another sheet is pointed by its name without the sheet's", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_LAYOUT__.setTables("Sheet2", [{ ...TABLE, name: "표2", range: "A1:B2", columns: ["가", "나 다"] }]);
  cell(document, "B9").focus();
  typeText(document, "=SUM(");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  click(document, "B2");
  assert.equal(bar(document).value, "=SUM(표2[나 다]");
  assert.equal(boxes(document).length, 1, "boxed on its own sheet");
  click(document, "A1");
  assert.equal(bar(document).value, "=SUM(표2[[#Headers],[가]]", "pointing a header cell writes [#Headers] with its box");
  assert.equal(boxes(document).length, 1);
  click(document, "C1");
  assert.equal(bar(document).value, "=SUM(Sheet2!C1", "a cell outside the table is a sheet-prefixed A1 reference");
});

test("a name defined for exactly the cells pointed is not written in their place", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_NAMES__.replaceAll([{ name: "목표", refersTo: "Sheet1!$C$3:$C$5" }], true);
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  press(cell(document, "C3"));
  over(cell(document, "C5"));
  assert.equal(b9.textContent, "=SUM(C3:C5");
});

test("Shift+click on a sheet tab writes a 3D sheet span before or after clicking a cell, marks spanned tabs, and evaluates across sheets", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  click(document, "B2");
  assert.equal(b9.textContent, "=SUM(B2");

  // Upgrading a pointed cell on Sheet1 to Sheet1:Sheet2 via Shift+click on Sheet2
  clickTab(document, "Sheet2", { shiftKey: true });
  await flushAsyncTurns(20);
  assert.equal(bar(document).value, "=SUM(Sheet1:Sheet2!B2");
  assert.equal(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]').dataset.pointSpan, "true");
  assert.equal(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]').dataset.pointSpan, "true");
  assert.equal(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]').dataset.tabGrouped, undefined, "formula point span does not mark tabs as grouped");
  assert.equal(boxes(document).length, 1, "3D reference shows a ref box on Sheet2 as well");

  typeBar(document, ")");
  key(bar(document), "Enter");
  await flushAsyncTurns(40);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet1");
  assert.equal(cell(document, "B9").dataset.formula, "=SUM(Sheet1:Sheet2!B2)");
  assert.equal(cell(document, "B9").textContent, String(1150 + 1200));
  assert.equal(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]').dataset.pointSpan, undefined);
});

test("Shift+click on another sheet tab before pointing a cell uses quoted 3D sheet span on Korean tab names", async () => {
  const { document } = await openSheet();
  const { makeFixtureFile, pickFile } = await import("./test-support/fixture-files.mjs");
  await pickFile(
    document,
    makeFixtureFile("삼차원.xlsx", {
      sheets: [
        { name: "1월", cells: [{ ref: "A1", raw: "10" }, { ref: "B1", raw: "20" }] },
        { name: "2월", cells: [{ ref: "A1", raw: "30" }, { ref: "B1", raw: "40" }] },
        { name: "3월", cells: [{ ref: "A1", raw: "50" }, { ref: "B1", raw: "60" }] },
      ],
    })
  );
  switchModule(document, "SHEET");
  await flushAsyncTurns(20);

  const b4 = cell(document, "B4");
  b4.focus();
  typeText(document, "=SUM(");
  clickTab(document, "3월", { shiftKey: true });
  await flushAsyncTurns(20);
  click(document, "A1");
  click(document, "B1", { shiftKey: true });
  assert.equal(bar(document).value, "=SUM('1월:3월'!A1:B1");
  assert.equal(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="2월"]').dataset.pointSpan, "true");

  typeBar(document, ")");
  key(bar(document), "Enter");
  await flushAsyncTurns(40);
  assert.equal(cell(document, "B4").dataset.formula, "=SUM('1월:3월'!A1:B1)");
  assert.equal(cell(document, "B4").textContent, "210");
});

test("pointing table headers, totals row, combined sections and whole table inserts structured references with boxes", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_LAYOUT__.setTables("Sheet1", [
    { ...TABLE, range: "A1:G7", totals: true },
  ]);
  const h10 = cell(document, "H10");
  h10.focus();
  typeText(document, "=");

  click(document, "C1");
  assert.equal(h10.textContent, "=표1[[#Headers],[2분기]]");
  assert.equal(boxes(document).length, 1);

  press(cell(document, "A1"));
  over(cell(document, "G1"));
  release(cell(document, "G1"));
  assert.equal(h10.textContent, "=표1[#Headers]");
  assert.equal(boxes(document).length, 1);

  click(document, "C7");
  assert.equal(h10.textContent, "=표1[[#Totals],[2분기]]");
  assert.equal(boxes(document).length, 1);

  press(cell(document, "A7"));
  over(cell(document, "G7"));
  release(cell(document, "G7"));
  assert.equal(h10.textContent, "=표1[#Totals]");

  press(cell(document, "A1"));
  over(cell(document, "G7"));
  release(cell(document, "G7"));
  assert.equal(h10.textContent, "=표1[#All]");
  assert.equal(boxes(document).length, 1);

  press(cell(document, "C1"));
  over(cell(document, "C6"));
  release(cell(document, "C6"));
  assert.equal(h10.textContent, "=표1[[#Headers],[#Data],[2분기]]");

  press(cell(document, "C2"));
  over(cell(document, "C7"));
  release(cell(document, "C7"));
  assert.equal(h10.textContent, "=표1[[#Data],[#Totals],[2분기]]");
});

test("manually typed structured references draw grid boxes in cell, formula bar and F2 edit mode, and follow sheet switches and Shift+arrow", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_LAYOUT__.setTables("Sheet1", [TABLE]);
  host.__EASYOFFICE_SHEET_LAYOUT__.setTables("Sheet2", [
    { ...TABLE, name: "표2", range: "A1:B2", columns: ["가", "나 다"] },
  ]);
  const g3 = cell(document, "G3");
  g3.focus();
  typeText(document, "=SUM(표1[1분기])+표1[[#Headers],[2분기]]+[@3분기]");
  assert.deepEqual(boxes(document).map((b) => b.dataset.color), ["0", "1", "2"]);
  const [colBox, hdrBox, rowBox] = boxes(document);
  assert.equal(hdrBox.style.top, "26px");
  assert.equal(colBox.style.left, "128px");
  assert.ok(parseFloat(colBox.style.height) > parseFloat(hdrBox.style.height) * 4);
  assert.ok(parseFloat(rowBox.style.top) > parseFloat(hdrBox.style.top));
  assert.ok(parseFloat(rowBox.style.left) > parseFloat(hdrBox.style.left));
  assert.ok(parseFloat(hdrBox.style.left) > parseFloat(colBox.style.left));

  // Re-entering an existing typed structured-ref formula via F2 draws boxes immediately.
  key(g3, "Enter");
  await flushAsyncTurns(12);
  assert.equal(boxes(document).length, 0);
  g3.focus();
  key(g3, "F2");
  await flushAsyncTurns(12);
  assert.deepEqual(boxes(document).map((b) => b.dataset.color), ["0", "1", "2"]);
  key(g3, "Escape");

  // Formula bar typing + cross-sheet box isolation (`[@3분기]` belongs to Sheet1 and never paints on Sheet2).
  cell(document, "B9").focus();
  typeBar(document, "=SUM(표2[나 다])+");
  assert.equal(boxes(document).length, 0, "Sheet2 table does not draw a box while Sheet1 is active");
  typeBar(document, "표1[[#Headers],[2분기]]+[@3분기]");
  assert.equal(boxes(document).length, 1, "Sheet1 header structured ref typed in formula bar draws a box");
  typeBar(document, "+");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  assert.deepEqual(
    boxes(document).map((b) => b.dataset.color),
    ["0"],
    "switching to Sheet2 shows Sheet2 table box only and hides Sheet1 boxes"
  );
  key(bar(document), "Escape");
  await flushAsyncTurns(40);

  // Typed structured reference in enter mode widens with Shift+ArrowRight.
  const h10 = cell(document, "H10");
  h10.focus();
  typeText(document, "=표1[[#Headers],[2분기]]");
  assert.equal(boxes(document).length, 1);
  key(h10, "ArrowRight", { shiftKey: true });
  assert.equal(h10.textContent, "=표1[[#Headers],[2분기]:[3분기]]");
});

test("F3 (이름 붙여넣기) inserts defined names without committing incomplete formulas, restores on cancel, and starts =name from idle", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_NAMES__.replaceAll([
    { name: "목표", refersTo: "Sheet1!$C$3:$C$5" },
    { name: "비율", refersTo: "2" },
  ], true);

  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  assert.equal(key(b9, "F3"), false, "F3 opens 이름 붙여넣기 while editing cell formula");
  await flushAsyncTurns(10);
  const pasteDlg = document.getElementById("sheet-name-paste-dialog");
  assert.equal(pasteDlg.open, true);
  assert.equal(b9.dataset.editMode, "enter", "opening modal does not commit partial formula");
  assert.deepEqual(
    [...document.querySelectorAll("#sheet-name-paste-list li")].map((li) => li.querySelector("span").textContent),
    ["목표", "비율"]
  );

  // Cancel preserves partial formula and cursor
  pasteDlg.querySelector(".dialog-close").click();
  await flushAsyncTurns(10);
  assert.equal(pasteDlg.open, false);
  assert.equal(b9.textContent, "=SUM(");
  assert.equal(b9.dataset.editMode, "enter");

  // Re-open with F3, pick second entry with ArrowDown + 확인, color + ref box + evaluate
  key(b9, "F3");
  await flushAsyncTurns(10);
  assert.equal(pasteDlg.open, true);
  key(pasteDlg, "ArrowDown");
  assert.equal(document.querySelector('#sheet-name-paste-list li[aria-selected="true"]').dataset.index, "1");
  key(pasteDlg, "ArrowUp");
  document.getElementById("btn-sheet-name-paste-ok").click();
  await flushAsyncTurns(10);
  assert.equal(b9.textContent, "=SUM(목표");
  assert.equal(boxes(document).length, 1, "inserted defined name draws its range box");
  assert.deepEqual([...document.querySelectorAll("#sheet-formula-colors span[data-color]")].map((s) => s.textContent), ["목표"]);
  typeText(document, ")");
  key(b9, "Enter");
  await flushAsyncTurns(30);
  assert.equal(cell(document, "B9").dataset.formula, "=SUM(목표)");

  // Starting from idle cell via F3 begins `=비율` edit mode (and Esc reverts cell cleanly)
  const c9 = cell(document, "C9");
  c9.focus();
  key(c9, "F3");
  await flushAsyncTurns(10);
  key(pasteDlg, "ArrowDown");
  key(pasteDlg, "Enter");
  await flushAsyncTurns(10);
  assert.equal(c9.textContent, "=비율");
  assert.equal(c9.dataset.editMode, "edit");
  assert.equal(boxes(document).length, 0, "constant defined name has no grid range box");
  assert.deepEqual([...document.querySelectorAll("#sheet-formula-colors span[data-color]")].map((s) => s.textContent), ["비율"]);
  key(c9, "Escape");
  await flushAsyncTurns(10);
  assert.equal(cell(document, "C9").textContent, "");
});

test("수식에서 사용 menu, Name Box dropdown, cross-sheet insertion, and cell replacement work on defined names", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_NAMES__.replaceAll([
    { name: "목표", refersTo: "Sheet1!$C$3:$C$5" },
    { name: "타시트", scope: "Sheet2", refersTo: "Sheet2!$B$2" },
  ], true);

  // Clicking 수식 > 수식에서 사용 > 목표 preserves cell edit focus and inserts into formula
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  const formulaMenu = document.querySelector('.menu-item[data-menu="formula"]');
  assert.equal(formulaMenu.dispatchEvent({ type: "mousedown", target: formulaMenu.querySelector(".menu-title") }), false, "menu press preserves edit focus");
  const useItem = document.querySelector('#sheet-name-use-menu .md-item[data-name-use="목표"]');
  assert.ok(useItem);
  useItem.dispatchEvent({ type: "mousedown", target: useItem, bubbles: true });
  useItem.click();
  assert.equal(b9.textContent, "=SUM(목표");
  assert.equal(boxes(document).length, 1);

  // Clicking a cell while caret sits on a defined name replaces it with the pointed cell
  click(document, "D3");
  assert.equal(b9.textContent, "=SUM(D3");

  // Selecting from Name Box dropdown during active formula edit inserts instead of navigating
  document.getElementById("btn-name-box-list").dispatchEvent({ type: "mousedown", target: document.getElementById("btn-name-box-list") });
  const listItem = document.querySelector('#sheet-name-box-list li[data-name="목표"]');
  assert.ok(listItem);
  listItem.dispatchEvent({ type: "mousedown", target: listItem, bubbles: true });
  assert.equal(b9.textContent, "=SUM(목표");

  // Cross-sheet formula bar edit inserts Sheet2!타시트 when picking a Sheet2-scoped name on Sheet2
  typeText(document, "+");
  clickTab(document, "Sheet2");
  await flushAsyncTurns(20);
  formulaMenu.dispatchEvent({ type: "mousedown", target: formulaMenu.querySelector(".menu-title") });
  const localItem = document.querySelector('#sheet-name-use-menu .md-item[data-name-use="타시트"]');
  assert.ok(localItem);
  localItem.click();
  assert.equal(bar(document).value, "=SUM(목표+Sheet2!타시트");
  assert.equal(boxes(document).length, 1, "only Sheet2 box draws on Sheet2");
  typeBar(document, ")");
  key(bar(document), "Enter");
  await flushAsyncTurns(40);
  assert.equal(cell(document, "B9").dataset.formula, "=SUM(목표+Sheet2!타시트)");
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab, "Sheet1");
});

test("F3 while pointing a cell preserves or replaces the pointed ref, formula bar F3 works, and F2/protection guards hold", async () => {
  const { document, host } = await openSheet();
  host.__EASYOFFICE_SHEET_NAMES__.replaceAll([{ name: "목표", refersTo: "Sheet1!$C$3:$C$5" }], true);
  const pasteDlg = document.getElementById("sheet-name-paste-dialog");

  // 1) Pointing C3 then cancelling F3 keeps point mode ("참조") and pointed C3 intact; confirming replaces C3
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=SUM(");
  click(document, "C3");
  assert.equal(b9.textContent, "=SUM(C3");
  assert.equal(mode(document).textContent, "참조");
  key(b9, "F3");
  await flushAsyncTurns(10);
  assert.equal(pasteDlg.open, true);
  pasteDlg.querySelector(".dialog-close").click();
  await flushAsyncTurns(10);
  assert.equal(b9.textContent, "=SUM(C3");
  assert.equal(mode(document).textContent, "참조");
  assert.equal(boxes(document).length, 1);

  key(b9, "F3");
  await flushAsyncTurns(10);
  document.getElementById("btn-sheet-name-paste-ok").click();
  await flushAsyncTurns(10);
  assert.equal(b9.textContent, "=SUM(목표");
  assert.equal(mode(document).textContent, "편집");
  assert.equal(key(b9, "F4"), false, "F4 does nothing on a defined name");
  assert.equal(b9.textContent, "=SUM(목표");
  key(b9, "Escape");
  await flushAsyncTurns(10);

  // 2) Same-sheet formula bar editing with F3 cancel & confirm
  cell(document, "B9").focus();
  typeBar(document, "=1+");
  key(bar(document), "F3");
  await flushAsyncTurns(10);
  assert.equal(pasteDlg.open, true);
  pasteDlg.querySelector(".dialog-close").click();
  await flushAsyncTurns(10);
  assert.equal(bar(document).value, "=1+");
  key(bar(document), "F3");
  await flushAsyncTurns(10);
  key(pasteDlg, "Enter");
  await flushAsyncTurns(10);
  assert.equal(bar(document).value, "=1+목표");
  assert.equal(boxes(document).length, 1);
  key(bar(document), "Enter");
  await flushAsyncTurns(30);
  assert.equal(cell(document, "B9").dataset.formula, "=1+목표");

  // 3) Re-entering saved formula via F2 shows defined-name box and colors immediately
  cell(document, "B9").focus();
  key(cell(document, "B9"), "F2");
  await flushAsyncTurns(10);
  assert.equal(boxes(document).length, 1);
  key(cell(document, "B9"), "Escape");
  await flushAsyncTurns(10);

  // 4) Idle F3 insert on a locked cell of a protected sheet is refused without altering the cell
  const c9 = cell(document, "C9");
  host.__EASYOFFICE_SHEET_LAYOUT__.setProtect("Sheet1", { on: true, deny: [] });
  c9.focus();
  key(c9, "F3");
  await flushAsyncTurns(10);
  document.getElementById("btn-sheet-name-paste-ok").click();
  await flushAsyncTurns(10);
  assert.equal(document.getElementById("sheet-protect-alert-dialog").open, true);
  assert.equal(cell(document, "C9").textContent, "");
});

test("F3 empty focus, stable row DOM across selection, 3D tabSpan on cancel, outside-click name box, and F3 input guards", async () => {
  const { document, host } = await openSheet();
  const pasteDlg = document.getElementById("sheet-name-paste-dialog");
  const closePaste = async () => { pasteDlg.close(); pasteDlg.dispatchEvent({ type: "close", target: pasteDlg }); await flushAsyncTurns(10); };

  // 1) Empty F3 disables OK and never focuses .dialog-close; clicking rows keeps <li> node identity for dblclick
  cell(document, "B9").focus();
  key(cell(document, "B9"), "F3");
  await flushAsyncTurns(10);
  assert.equal(pasteDlg.open, true);
  assert.equal(document.getElementById("btn-sheet-name-paste-ok").disabled, true);
  assert.equal(document.activeElement?.classList?.contains("dialog-close"), false);
  await closePaste();

  host.__EASYOFFICE_SHEET_NAMES__.replaceAll([{ name: "목표", refersTo: "Sheet1!$C$3:$C$5" }, { name: "예산", refersTo: "Sheet1!$D$3:$D$5" }], true);
  cell(document, "B9").focus();
  key(cell(document, "B9"), "F3");
  await flushAsyncTurns(10);
  const firstLi = document.querySelector('#sheet-name-paste-list li[data-index="0"]');
  const secondLi = document.querySelector('#sheet-name-paste-list li[data-index="1"]');
  secondLi.dispatchEvent({ type: "click", target: secondLi, bubbles: true });
  assert.equal(document.querySelector('#sheet-name-paste-list li[data-index="1"]'), secondLi);
  assert.equal(secondLi.getAttribute("aria-selected"), "true");
  assert.equal(firstLi.getAttribute("aria-selected"), "false");
  await closePaste();

  // 2) Clicking 이름 붙여넣기 from 수식 menu resets aria-expanded="false"
  const formulaMenu = document.querySelector('.menu-item[data-menu="formula"]');
  const title = formulaMenu.querySelector(".menu-title");
  formulaMenu.classList.add("open");
  title.setAttribute("aria-expanded", "true");
  document.querySelector('.md-item[data-sheet-act="name-paste"]').click();
  await flushAsyncTurns(10);
  assert.equal(title.getAttribute("aria-expanded"), "false");
  assert.equal(pasteDlg.open, true);
  await closePaste();

  // 3) Name Box dropdown closes on Escape (without cancelling formula) and on outside mousedown
  const b9 = cell(document, "B9");
  b9.focus();
  typeText(document, "=1+");
  document.getElementById("btn-name-box-list").dispatchEvent({ type: "mousedown", target: document.getElementById("btn-name-box-list") });
  assert.equal(document.getElementById("sheet-name-box-list").hidden, false);
  key(document, "Escape");
  assert.equal(document.getElementById("sheet-name-box-list").hidden, true);
  assert.equal(b9.textContent, "=1+");
  document.getElementById("btn-name-box-list").dispatchEvent({ type: "mousedown", target: document.getElementById("btn-name-box-list") });
  document.dispatchEvent({ type: "mousedown", target: document.body });
  assert.equal(document.getElementById("sheet-name-box-list").hidden, true);

  // 4) Cancelling F3 preserves 3D tabSpan, and F3 stays ignored inside sub-inputs
  clickTab(document, "Sheet2");
  await flushAsyncTurns(15);
  click(document, "C4");
  clickTab(document, "Sheet1", { shiftKey: true });
  await flushAsyncTurns(15);
  assert.equal(bar(document).value, "=1+Sheet2:Sheet1!C4");
  key(bar(document), "F3");
  await flushAsyncTurns(10);
  pasteDlg.querySelector(".dialog-close").click();
  await flushAsyncTurns(10);
  clickTab(document, "Sheet2", { shiftKey: true });
  await flushAsyncTurns(15);
  assert.equal(bar(document).value, "=1+Sheet2!C4", "preserved 3D tabSpan collapses cleanly on Shift+Sheet2");
  key(bar(document), "Escape");
  await flushAsyncTurns(10);
  for (const selector of ["#active-cell-ref", ".sheet-table-name"]) {
    const inp = document.querySelector(selector);
    if (!inp) continue;
    inp.focus();
    key(inp, "F3");
    await flushAsyncTurns(5);
    assert.equal(pasteDlg.open, false, selector);
  }
});
