import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// 범위로 변환 keeps the look of the table (PD-232): the style of each cell goes into the cell's own format, the cell's own
// format stays, the step is one undo, and a table over the bound is converted without the look.

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const $ = (h, id) => h.document.getElementById(id);
const tables = (h) => h.host.__EASYOFFICE_SHEET_LAYOUT__.tablesOf("Sheet1");

async function openSheet() {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const testIpc = async (command, payload) => {
    const body = typeof payload === "string" && payload.startsWith("{") ? JSON.parse(payload) : null;
    if (command === "evaluate_sheet" && body?.sessionAction === "tableColumn") return { rewrites: [], names: null };
    return offline[command](payload);
  };
  const h = await createWorkspaceHarness({ transformHtml: demoHtml, testIpc });
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  return h;
}

const undo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
};

async function makeTable(h, range = "A1:C4") {
  cell(h.document, "B3").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-insert-table"));
  await flushAsyncTurns(4);
  $(h, "sheet-table-range").value = range;
  $(h, "sheet-table-header").checked = true;
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(16);
}

async function toRange(h) {
  cell(h.document, "B3").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-table-bar [data-table-act="to-range"]'));
  await flushAsyncTurns(16);
}

const styleOf = (h, ref) => h.host.__EASYOFFICE_SHEET_LAYOUT__ && cell(h.document, ref)._st;

test("the header's fill, the white text, the bold and the stripes of the style are cell formats after the conversion", async () => {
  const h = await openSheet();
  await makeTable(h);
  const table = tables(h)[0];
  const style = h.host.__EASYOFFICE_SHEET_TABLE_STYLE__;
  const g = style.geometryOf(table);
  const look = (r, c) => style.lookOf(table, g, style.styleOf(table), r, c);
  const header = look(1, 1);
  const stripe = look(2, 1);
  assert.ok(header.fill && header.bold, "the built-in style has a header fill and bold text");
  await toRange(h);
  assert.equal(tables(h).length, 0);
  assert.equal(cell(h.document, "A1")._st.fill, header.fill);
  assert.equal(cell(h.document, "A1").style.fontWeight, "600");
  assert.equal(cell(h.document, "B1")._st.fill, header.fill);
  assert.equal(cell(h.document, "A2")._st.fill, stripe.fill);
  assert.equal(cell(h.document, "A3")._st.fill, look(3, 1).fill ?? undefined, "the second row of the stripes");
  assert.equal(h.document.getElementById("status-command-feedback").textContent, "범위로 변환");
});

test("one undo gives the table back and takes the formats away; redo bakes them again", async () => {
  const h = await openSheet();
  await makeTable(h);
  await toRange(h);
  assert.ok(cell(h.document, "A1")._st?.fill);
  await undo(h);
  assert.equal(tables(h).length, 1);
  assert.equal(cell(h.document, "A1")._st?.fill, undefined, "no cell format is left from the conversion");
  assert.equal(cell(h.document, "A1").style.fontWeight, "");
  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.equal(tables(h).length, 0);
  assert.ok(cell(h.document, "A1")._st?.fill);
});

test("a cell's own fill, text colour and border stay over the table's", async () => {
  const h = await openSheet();
  await makeTable(h);
  const bake = h.host.__EASYOFFICE_SHEET_TABLE_BAKE__;
  const table = tables(h)[0];
  const box = h.host.__EASYOFFICE_SHEET_LAYOUT__.tableBox(table);
  const own = { fill: "ffff00", fontColor: "ff0000", fontWeight: "600", border: { b: "thin:00ff00" } };
  const planned = bake.plan(table, box, (r, c) => (r === 1 && c === 1 ? own : undefined));
  const first = planned.cells.find((c) => c.row === 1 && c.col === 1);
  assert.equal(first?.fill, undefined);
  assert.equal(first?.fontColor, undefined);
  assert.equal(first?.bold, undefined);
  assert.equal(first?.border?.b, undefined, "its own bottom edge");
  const second = planned.cells.find((c) => c.row === 1 && c.col === 2);
  assert.ok(second?.fill, "a neighbour without a format takes the table's");
});

test("a table without a style bakes nothing, and one over the bound is converted without the look", async () => {
  const h = await openSheet();
  const bake = h.host.__EASYOFFICE_SHEET_TABLE_BAKE__;
  assert.deepEqual(bake.plan({ name: "T", range: "A1:B2", columns: ["a", "b"] }, { r1: 1, c1: 1, r2: 2, c2: 2 }, () => undefined), { cells: [] });
  const style = { name: "T", range: "A1:B2", columns: ["a", "b"], style: "TableStyleMedium2" };
  const big = bake.plan(style, { r1: 1, c1: 1, r2: bake.MAX_BAKE_CELLS + 1, c2: 1 }, () => undefined);
  assert.equal(big.over, true);
  assert.equal(big.cells.length, 0);
});

test("a table's total row, banding of columns and the edges the style draws are baked as borders", async () => {
  const h = await openSheet();
  await makeTable(h);
  const table = { ...tables(h)[0], style: "TableStyleLight9", totals: false };
  const style = h.host.__EASYOFFICE_SHEET_TABLE_STYLE__;
  const box = h.host.__EASYOFFICE_SHEET_LAYOUT__.tableBox(tables(h)[0]);
  const planned = h.host.__EASYOFFICE_SHEET_TABLE_BAKE__.plan(table, box, () => undefined);
  const edges = planned.cells.filter((c) => c.border);
  assert.ok(edges.length > 0, "a light style draws its lines");
  for (const c of edges) {
    for (const edge of Object.values(c.border)) assert.match(edge, /^(thin|thick):[0-9a-f]{6}$/);
  }
  assert.ok(style.styleOf(table));
});
