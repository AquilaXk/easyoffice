import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// The default row height of a sheet (PD-286): the height of every row that has none of its own. A file that sets one
// (`sheetFormatPr customHeight`) draws the rows at it; setting a height with every row selected changes it and replaces the
// heights of the rows; a default that follows the font stays at the grid's row height; a value that is not a height is no default.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const L = loadUiModule(ui("sheet-look.js"), {}).__EASYOFFICE_SHEET_LOOK__;
const RH = loadUiModule(ui("sheet-row-height.js"), {}).__EASYOFFICE_SHEET_ROW_HEIGHT__;
const RF = loadUiModule(ui("sheet-row-fit.js"), { __EASYOFFICE_SHEET_LOOK__: L });
const refs = loadUiModule(ui("sheet-refs.js"), {}).__EASYOFFICE_SHEET_REFS__;
const MAX_ROWS = 1048576;

// ---- the model ---------------------------------------------------------------------------------------

test("only a default a user set is kept, and a value that is not a height is none", () => {
  assert.equal(RH.defaultFromJson([30]), 30);
  assert.equal(RH.defaultFromJson([14.4, 1]), 14.4);
  assert.equal(RH.defaultFromJson([409]), 409);
  assert.equal(RH.defaultFromJson([14.4, 0]), null, "a default that follows the font is the grid's");
  for (const bad of [undefined, null, 20, "20", [], [0], [-1], [409.01], [NaN], [Infinity], [1e308], ["20"], [null], [20, 0, 1], { 0: 20 }]) {
    assert.equal(RH.defaultFromJson(bad), null, JSON.stringify(bad));
  }
  assert.deepEqual(RH.defaultToJson(30), [30]);
  for (const bad of [null, undefined, 0, -3, NaN, 410, "30"]) assert.equal(RH.defaultToJson(bad), null);
});

test("the grid's row height is the default in px when there is one", () => {
  assert.equal(RH.baseOf(null, 24), 24);
  assert.equal(RH.baseOf(30, 24), 40);
  assert.equal(RH.baseOf(15, 24), 20);
  assert.equal(RH.baseOf(NaN, 24), 24);
  assert.equal(RH.baseOf(1e9, 24), 24, "a value over a row's limit is not a default");
});

test("a dragged default is clamped to what a row may be", () => {
  assert.equal(RH.defaultPtOf(40), 30);
  assert.equal(RH.defaultPtOf(1), RH.pxToPt(RH.MIN_PX));
  assert.equal(RH.defaultPtOf(99999), 409);
  assert.equal(RH.defaultPtOf(-5), RH.pxToPt(RH.MIN_PX));
});

test("every row is selected by Ctrl+A or by a row selection from the first row to the last", () => {
  assert.equal(RH.coversAllRows({ whole: true, axis: null, r1: 1, r2: 9 }), true);
  assert.equal(RH.coversAllRows({ axis: "row", r1: 1, r2: MAX_ROWS }), true);
  assert.equal(RH.coversAllRows({ axis: "row", r1: 2, r2: MAX_ROWS }), false);
  assert.equal(RH.coversAllRows({ axis: "row", r1: 1, r2: MAX_ROWS - 1 }), false);
  assert.equal(RH.coversAllRows({ axis: "col", r1: 1, r2: MAX_ROWS }), false, "every row of a column is not every row");
  assert.equal(RH.coversAllRows({ axis: null, r1: 1, r2: MAX_ROWS }), false);
  assert.equal(RH.coversAllRows(null), false);
});

// ---- the heights of the rows ---------------------------------------------------------------------------

function fit(map, base, extra = {}) {
  return RF.__EASYOFFICE_CREATE_ROW_FIT__({
    rowH: () => base.px,
    maxRows: MAX_ROWS,
    getSheetName: () => "S",
    getMap: () => map,
    columnWidth: () => 113,
    isRowHidden: () => false,
    isMerged: () => false,
    measure: (text, width) => Math.ceil((text.length * 10) / width),
    packRef: refs.packRef,
    ...extra,
  });
}

test("rows with no height of their own take the default, and a row a user set stays", () => {
  const base = { px: 40 };
  const f = fit({}, base, { customHeights: () => [[3, 60], [4, 40], [5, 20]] });
  f.sync();
  assert.equal(f.heightOf(1), 40);
  assert.equal(f.heightOf(3), 60);
  assert.equal(f.heightOf(4), 40, "a row at the default is a plain row");
  assert.equal(f.heightOf(5), 20);
  assert.equal(f.extraBefore(3), 0);
  assert.equal(f.extraBefore(4), 20);
  assert.equal(f.extraBefore(6), 20 - 20, "row 3 is 20 over the default and row 5 is 20 under it");
  assert.equal(f.minHeight(), 20);
});

test("a large font asks for a taller row than a short default has, and not for one the default holds", () => {
  const map = { A1: { value: "가", style: { fontSize: "18px" } }, A2: { value: "가", style: { fontSize: "40px" } } };
  const base = { px: 20 };
  const f = fit(map, base);
  f.sync();
  assert.equal(f.heightOf(1), Math.ceil(18 * 1.275), "23 px does not fit a 20 px row");
  assert.equal(f.heightOf(2), Math.ceil(40 * 1.275));
  base.px = 60;
  assert.equal(f.sync(), true, "a new default is a change even when no row has an own height");
  assert.equal(f.any(), false, "both fonts fit 60 px");
  assert.equal(f.heightOf(2), 60);
});

test("a default that already fills the scrolled box does not drop the taller rows", () => {
  const base = { px: 300 };
  const f = fit({}, base, { customHeights: () => [[1, 545], [9, 400]] });
  f.sync();
  assert.equal(f.heightOf(1), 545);
  assert.equal(f.heightOf(9), 400);
  assert.equal(f.extraBefore(10), 245 + 100);
});

// ---- the layout ------------------------------------------------------------------------------------------

function layoutApi() {
  return loadUiModule(ui("sheet-layout.js"), {
    __EASYOFFICE_SHEET_ROW_HEIGHT__: RH,
    __EASYOFFICE_SHEET_GRID_UTILS__: refs,
  }).__EASYOFFICE_SHEET_LAYOUT__;
}

test("the layout carries the default row height through its JSON, and a bad one is dropped", () => {
  const api = layoutApi();
  api.replaceAll({ S: { defaultRowHeight: [30] }, T: { defaultRowHeight: [14.4, 0] }, U: { defaultRowHeight: [-5] }, V: { defaultRowHeight: 30 } });
  assert.deepEqual(api.getAll(), { S: { defaultRowHeight: [30] } });
  assert.equal(api.defaultRowHeightOf("S"), 30);
  assert.equal(api.defaultRowHeightOf("T"), null);
  api.setDefaultRowHeight("S", 50);
  assert.deepEqual(api.getAll().S, { defaultRowHeight: [50] });
  api.setDefaultRowHeight("S", NaN);
  assert.deepEqual(api.getAll(), {}, "a sheet with nothing else is empty again");
  api.setDefaultRowHeight("S", 1e9);
  assert.equal(api.defaultRowHeightOf("S"), null);
});

test("the engine's answer to a row edit keeps the default of the sheet", () => {
  const api = layoutApi();
  api.replaceAll({ S: { defaultRowHeight: [30], rowHeights: [[2, 15]] } });
  api.setSheet("S", { merges: [], rowHeights: [[3, 15]] });
  assert.equal(api.defaultRowHeightOf("S"), 30);
  assert.deepEqual(api.getAll().S.rowHeights, [[3, 15]]);
  api.setSheet("S", { rowHeights: [], defaultRowHeight: [40] });
  assert.equal(api.defaultRowHeightOf("S"), 40, "an answer that has one replaces it");
});

// ---- the page --------------------------------------------------------------------------------------------

const cells = ["머리", "일", "삼", "사", "오"].map((raw, i) => ({ ref: "A" + (i + 1), raw }));

async function open(sheet = {}) {
  const h = await createWorkspaceHarness();
  const d = h.document;
  const make = d.createElement.bind(d);
  d.createElement = (tag) => {
    const el = make(tag);
    if (tag === "canvas") el.getContext = () => ({ font: "", measureText(t) { return { width: [...t].length * 8 }; } });
    return el;
  };
  d.defaultView.getComputedStyle = () => ({ fontFamily: "sans-serif", fontSize: "13px" });
  switchModule(d, "SHEET");
  await flushAsyncTurns();
  const viewport = d.getElementById("sheet-viewport");
  viewport.clientHeight = 520;
  viewport.clientWidth = 960;
  viewport.scrollTop = 0;
  viewport.scrollLeft = 0;
  await pickFile(d, makeFixtureFile("기본.xlsx", { sheets: [{ name: "기본", cells, ...sheet }] }));
  await flushAsyncTurns(12);
  h.press = (key, extra = {}) => {
    const td = d.activeElement;
    td.dispatchEvent({ type: "keydown", key, target: td, ...extra });
  };
  return { ...h, viewport, layout: () => h.host.__EASYOFFICE_SHEET_LAYOUT__ };
}

const tr = (d, row) => d.querySelector(`#pane-sheet tbody tr[data-row="${row}"]`);
const edge = (d, row) => [...d.querySelectorAll("#pane-sheet .row-resizer")].find((el) => el.parentElement.dataset.row === String(row));
const press = (el, extra = {}) => el.dispatchEvent({ type: "mousedown", target: el, button: 0, bubbles: true, ...extra });
const move = (d, clientY) => d.dispatchEvent({ type: "mousemove", target: d, clientY, bubbles: true });
const release = (d) => d.dispatchEvent({ type: "mouseup", target: d, bubbles: true });
const dragEdge = async (d, row, from, to) => {
  press(edge(d, row), { clientY: from });
  move(d, to);
  release(d);
  await flushAsyncTurns(8);
};
const undo = async (h) => {
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(10);
};
const saved = (h) => h.layout().getAll()["기본"] ?? {};

test("a file's default row height draws every row at it, and the window and the spacer follow", async () => {
  const h = await open({ defaultRowHeight: [30] });
  const d = h.document;
  assert.equal(tr(d, 1).style.height, "40px", "30 pt");
  assert.equal(tr(d, 5).style.height, "40px");
  assert.equal(tr(d, 1).classList.contains("row-short"), false);
  assert.deepEqual(saved(h), { defaultRowHeight: [30] }, "kept in the layout, saved from it");
  const rows = [...d.querySelectorAll("#pane-sheet tbody tr[data-row]")].map((r) => Number(r.dataset.row));
  const last = Math.max(...rows);
  const spacer = parseInt(d.querySelector("#pane-sheet .sheet-spacer-bottom td").style.height, 10);
  assert.equal(spacer, (MAX_ROWS - last) * 40, "the rows below the window are as tall as the default");
  assert.ok(rows.length <= Math.ceil(520 / 40) + 30, "the window holds the rows the viewport shows at 40 px, not 24: " + rows.length);
  h.viewport.scrollTop = 40 * 1000;
  h.viewport.dispatchEvent({ type: "scroll" });
  await flushAsyncTurns();
  const far = [...d.querySelectorAll("#pane-sheet tbody tr[data-row]")].map((r) => Number(r.dataset.row));
  assert.ok(far[0] > 975 && far[0] <= 1001, "the window follows the scroll by the default height: " + far[0]);
});

test("a short default and a row of a user beside it", async () => {
  const h = await open({ defaultRowHeight: [15], rowHeights: [[2, 30]] });
  const d = h.document;
  assert.equal(tr(d, 1).style.height, "20px");
  assert.equal(tr(d, 1).classList.contains("row-short"), true);
  assert.equal(tr(d, 2).style.height, "40px");
  assert.equal(tr(d, 3).style.height, "20px");
});

test("a default that follows the font and values that are not heights leave the grid's row height", async () => {
  for (const defaultRowHeight of [[14.4, 0], [0], [-3], [1e9], [NaN], "30", [409.5]]) {
    const h = await open({ defaultRowHeight });
    assert.equal(tr(h.document, 1).style.height, "", JSON.stringify(defaultRowHeight));
    assert.deepEqual(saved(h), {}, JSON.stringify(defaultRowHeight));
  }
});

test("a drag on an edge with every row selected sets the default and replaces the rows' heights; one undo step brings them back", async () => {
  const h = await open({ rowHeights: [[2, 15], [4, 60]] });
  const d = h.document;
  d.querySelector('#pane-sheet td[data-cell="A1"]').focus();
  h.press("a", { ctrlKey: true });
  await flushAsyncTurns(3);
  await dragEdge(d, 1, 100, 116);
  assert.deepEqual(saved(h), { defaultRowHeight: [28.5] }, "22 px and 16 px of move: 38 px, which is 28.5 pt, and no row of its own");
  assert.equal(tr(d, 1).style.height, "38px");
  assert.equal(tr(d, 2).style.height, "38px", "the row that had 15 pt takes the default too");
  assert.equal(tr(d, 4).style.height, "38px");
  await undo(h);
  assert.deepEqual(saved(h), { rowHeights: [[2, 15], [4, 60]] });
  assert.equal(tr(d, 1).style.height, "");
  assert.equal(tr(d, 2).style.height, "20px");
  assert.equal(tr(d, 4).style.height, "80px");
});

test("a drag on an edge without every row selected sets that row only", async () => {
  const h = await open({ defaultRowHeight: [30] });
  const d = h.document;
  await dragEdge(d, 3, 100, 120);
  assert.deepEqual(saved(h), { defaultRowHeight: [30], rowHeights: [[3, 45]] }, "40 px and the 20 px of the move");
  assert.equal(tr(d, 2).style.height, "40px");
});

test("a row selection from the first row to the last is every row too", async () => {
  const h = await open({ rowHeights: [[4, 60]] });
  const d = h.document;
  const box = d.getElementById("active-cell-ref");
  box.focus();
  box.textContent = "A1:XFD" + MAX_ROWS;
  box.dispatchEvent({ type: "input", target: box });
  box.dispatchEvent({ type: "keydown", key: "Enter", target: box });
  await flushAsyncTurns(30);
  await dragEdge(d, 2, 100, 90);
  assert.deepEqual(saved(h), { defaultRowHeight: [9] }, "22 px less 10: 12 px");
  assert.equal(tr(d, 4).style.height, "12px");
});

test("a double click on an edge with every row selected gives every row back to its content and drops the default", async () => {
  const h = await open({ defaultRowHeight: [30], rowHeights: [[2, 15]] });
  const d = h.document;
  d.querySelector('#pane-sheet td[data-cell="A1"]').focus();
  h.press("a", { ctrlKey: true });
  await flushAsyncTurns(3);
  const el = edge(d, 3);
  el.dispatchEvent({ type: "dblclick", target: el, bubbles: true, button: 0 });
  await flushAsyncTurns(15);
  assert.deepEqual(saved(h), {});
  assert.equal(tr(d, 1).style.height, "");
  assert.equal(tr(d, 2).style.height, "");
  await undo(h);
  assert.deepEqual(saved(h), { defaultRowHeight: [30], rowHeights: [[2, 15]] });
  assert.equal(tr(d, 1).style.height, "40px");
});

test("an edge pressed with a cell range selected, not the whole sheet, leaves the default alone", async () => {
  const h = await open({ defaultRowHeight: [30] });
  const d = h.document;
  d.querySelector('#pane-sheet td[data-cell="A1"]').focus();
  h.press("ArrowDown", { shiftKey: true });
  await dragEdge(d, 1, 100, 110);
  assert.equal(saved(h).defaultRowHeight[0], 30);
  assert.deepEqual(saved(h).rowHeights, [[1, 37.5]]);
});
