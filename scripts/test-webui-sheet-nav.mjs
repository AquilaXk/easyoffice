import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// Keyboard movement of the sheet selection by the spreadsheet application's rules (PD-280): Ctrl+Arrow over runs and gaps,
// Ctrl+Shift+Arrow, Enter and Tab inside a range, the Enter after a row of Tabs, Home, Ctrl+Home, Ctrl+End, the page keys,
// merged ranges and hidden rows, through the real input handler of the sheet.

const SHEET = "이동";
const CELLS = { B2: "1", B3: "2", B4: "3", B7: "4", B8: "5", C2: "x", D2: "y", G2: "z" };

// The helpers hang on the harness so each test reads as the keys a user presses.
function withKeys(h) {
  const d = h.document;
  h.cell = (ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  h.active = () => d.getElementById("active-cell-ref").textContent;
  h.press = (key, extra = {}) => {
    const td = d.activeElement;
    td.dispatchEvent({ type: "keydown", key, target: td, ...extra });
  };
  h.go = (ref) => h.cell(ref).focus();
  h.range = () => [...d.querySelectorAll("#pane-sheet td[data-cell]")].filter((td) => td.classList.contains("in-range") || td.classList.contains("selected-cell")).map((td) => td.dataset.cell).sort((a, b) => a.localeCompare(b));
  return h;
}

async function open() {
  const h = await createWorkspaceHarness();
  withKeys(h);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  const model = { sheets: [{ name: SHEET, cells: Object.entries(CELLS).map(([ref, raw]) => ({ ref, raw })) }] };
  await pickFile(h.document, makeFixtureFile("이동.xlsx", model));
  h.layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  h.utils = h.host.__EASYOFFICE_SHEET_GRID_UTILS__;
  h.go("A1");
  return h;
}

const CTRL = { ctrlKey: true };

test("Ctrl+Down walks a run to its end, crosses the gap to the next run, and ends at the last row", async () => {
  const h = await open();
  h.go("B2");
  const seen = [];
  for (let i = 0; i < 4; i += 1) {
    h.press("ArrowDown", CTRL);
    seen.push(h.active());
  }
  assert.deepEqual(seen, ["B4", "B7", "B8", "B" + h.utils.MAX_ROWS]);
  h.press("ArrowUp", CTRL);
  assert.equal(h.active(), "B8", "up from the last row comes back to the last used row");
});

test("Ctrl+Right and Ctrl+Left walk a row the same way", async () => {
  const h = await open();
  h.go("B2");
  const seen = [];
  for (let i = 0; i < 3; i += 1) {
    h.press("ArrowRight", CTRL);
    seen.push(h.active());
  }
  assert.deepEqual(seen, ["D2", "G2", h.utils.colName(h.utils.MAX_COLS) + "2"]);
  h.press("ArrowLeft", CTRL);
  assert.equal(h.active(), "G2");
});

test("Ctrl+Shift+Down extends the range to the end of the run and on to the next run", async () => {
  const h = await open();
  h.go("B2");
  h.press("ArrowDown", { ...CTRL, shiftKey: true });
  assert.deepEqual(h.range(), ["B2", "B3", "B4"]);
  assert.equal(h.active(), "B2", "the active cell stays where the range started");
  h.press("ArrowDown", { ...CTRL, shiftKey: true });
  assert.deepEqual(h.range(), ["B2", "B3", "B4", "B5", "B6", "B7"]);
});

// B2:C3 selected with the active cell in B2.
function selectBlock(h) {
  h.go("B2");
  h.press("ArrowDown", { shiftKey: true });
  h.press("ArrowRight", { shiftKey: true });
  assert.deepEqual(h.range(), BLOCK);
}

const BLOCK = ["B2", "B3", "C2", "C3"];
const walk = (h, key, extra, times) => Array.from({ length: times }, () => (h.press(key, extra), h.active()));

test("Enter walks the active cell down a range, to the next column, and from the last cell to the first, and keeps the range", async () => {
  const h = await open();
  selectBlock(h);
  assert.deepEqual(walk(h, "Enter", {}, 5), ["B3", "C2", "C3", "B2", "B3"]);
  assert.deepEqual(h.range(), BLOCK, "the range is still selected");
  selectBlock(h);
  assert.deepEqual(walk(h, "Enter", { shiftKey: true }, 3), ["C3", "C2", "B3"], "Shift+Enter walks back, from the first cell to the last");
  assert.deepEqual(h.range(), BLOCK);
});

test("Tab walks the active cell along a range, to the next row, and from the last cell to the first, and keeps the range", async () => {
  const h = await open();
  selectBlock(h);
  assert.deepEqual(walk(h, "Tab", {}, 5), ["C2", "B3", "C3", "B2", "C2"]);
  assert.deepEqual(h.range(), BLOCK);
  selectBlock(h);
  assert.deepEqual(walk(h, "Tab", { shiftKey: true }, 3), ["C3", "B3", "C2"]);
});

test("an arrow key after walking a range selects from the active cell", async () => {
  const h = await open();
  selectBlock(h);
  walk(h, "Enter", {}, 2);
  assert.equal(h.active(), "C2");
  h.press("ArrowDown");
  assert.equal(h.active(), "C3");
  assert.deepEqual(h.range(), ["C3"]);
});

test("Enter after typing in a range commits the cell and moves on inside the range", async () => {
  const h = await open();
  h.go("B6");
  h.press("ArrowDown", { shiftKey: true });
  h.press("ArrowRight", { shiftKey: true });
  assert.deepEqual(h.range(), ["B6", "B7", "C6", "C7"]);
  const td = h.cell("B6");
  td.dispatchEvent({ type: "keydown", key: "9", target: td });
  td.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: "9", target: td });
  td.textContent = "9";
  td.dispatchEvent({ type: "input", inputType: "insertText", data: "9", target: td });
  h.press("Enter");
  await flushAsyncTurns();
  assert.equal(h.cell("B6").textContent, "9");
  assert.equal(h.active(), "B7");
  assert.deepEqual(h.range(), ["B6", "B7", "C6", "C7"]);
});

test("a row of Tabs sends the Enter after it back to the column it started in; an arrow or Shift+Tab forgets it", async () => {
  const h = await open();
  h.go("B10");
  h.press("Tab");
  h.press("Tab");
  assert.equal(h.active(), "D10");
  h.press("Enter");
  assert.equal(h.active(), "B11", "below the cell the Tabs started from");
  h.go("B10");
  h.press("Tab");
  h.press("ArrowDown");
  assert.equal(h.active(), "C11");
  h.press("Enter");
  assert.equal(h.active(), "C12", "an arrow key ended the row of Tabs");
  h.go("B10");
  h.press("Tab");
  h.press("Tab");
  h.press("Tab", { shiftKey: true });
  h.press("Enter");
  assert.equal(h.active(), "C11", "Shift+Tab ended the row of Tabs");
  h.go("B9");
  h.press("Tab");
  h.press("Enter", { shiftKey: true });
  assert.equal(h.active(), "C8", "Shift+Enter goes up from where the active cell is");
});

test("Home, Ctrl+Home and Ctrl+End, alone and with Shift", async () => {
  const h = await open();
  h.go("D5");
  h.press("Home");
  assert.equal(h.active(), "A5");
  h.go("D5");
  h.press("Home", CTRL);
  assert.equal(h.active(), "A1");
  h.press("End", CTRL);
  assert.equal(h.active(), "G8", "the last row and the last column that hold data");
  h.go("B2");
  h.press("End", { ...CTRL, shiftKey: true });
  assert.equal(h.range().length, 6 * 7, "B2 to G8");
  h.go("E5");
  h.press("Home", { shiftKey: true });
  assert.deepEqual(h.range(), ["A5", "B5", "C5", "D5", "E5"]);
});

test("PageDown and PageUp move a screen of rows, Alt+PageDown and Alt+PageUp a screen of columns, Shift extends", async () => {
  const h = await open();
  h.go("B2");
  h.press("PageDown");
  assert.equal(h.active(), "B22", "a screen is 20 rows where the page has no layout");
  h.press("PageUp");
  assert.equal(h.active(), "B2");
  h.press("PageUp");
  assert.equal(h.active(), "B1", "the first row is the edge");
  h.press("PageDown", { altKey: true });
  assert.equal(h.active(), "J1", "a screen is 8 columns where the page has no layout");
  h.press("PageUp", { altKey: true });
  assert.equal(h.active(), "B1");
  h.go("B2");
  h.press("PageDown", { shiftKey: true });
  assert.match(h.document.getElementById("status-sheet-agg").textContent, /개수 5 /, "the range runs from B2 down past B8");
  h.press("Home", CTRL);
  h.go("B2");
  h.press("PageDown", CTRL);
  assert.equal(h.active(), "B2", "Ctrl+PageDown is not a page move");
});

test("a merged range is entered at its top-left cell and left past its far edge by every key", async () => {
  const h = await open();
  h.layout.addMerge(SHEET, 3, 2, 4, 2);
  h.go("B2");
  h.press("Enter");
  assert.equal(h.active(), "B3", "the merged range is one cell");
  h.press("Enter");
  assert.equal(h.active(), "B5", "Enter does not stay in the merged range");
  h.press("ArrowUp");
  assert.equal(h.active(), "B3");
  h.go("B2");
  h.press("ArrowDown", CTRL);
  assert.equal(h.active(), "B3", "the run reaches the merged range and its far edge selects its anchor");
  h.press("ArrowDown", CTRL);
  assert.equal(h.active(), "B7");
  h.layout.addMerge(SHEET, 12, 3, 12, 4);
  h.go("B12");
  h.press("Tab");
  assert.equal(h.active(), "C12");
  h.press("Tab");
  assert.equal(h.active(), "E12", "Tab goes past the merged range");
  h.press("Tab", { shiftKey: true });
  assert.equal(h.active(), "C12");
});

test("hidden rows are stepped over by the arrows, Enter and Ctrl+Arrow, and not counted by PageDown", async () => {
  const h = await open();
  h.layout.setHidden(SHEET, "row", 3, 3, true);
  h.go("B2");
  h.press("ArrowDown");
  assert.equal(h.active(), "B4");
  h.go("B2");
  h.press("Enter");
  assert.equal(h.active(), "B4");
  h.go("B2");
  h.press("ArrowDown", CTRL);
  assert.equal(h.active(), "B4", "a hidden row of a run does not end it");
  h.layout.setHidden(SHEET, "row", 5, 6, true);
  h.press("ArrowDown", CTRL);
  assert.equal(h.active(), "B8", "hidden empty rows are not a gap: the run of B4, B7 and B8 goes on");
  h.go("B2");
  h.press("PageDown");
  assert.equal(h.active(), "B25", "20 shown rows below B2: rows 3, 5 and 6 are hidden");
});
