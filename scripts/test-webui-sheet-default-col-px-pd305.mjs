import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

// PD-305, PD-306: a column a sheet says nothing about is 72 px (8 characters of the 8 px digit of the standard default font,
// Malgun Gothic 11 pt, up to 8 px), or the width the engine told for the workbook's own default font. A sheet whose file says
// (`defaultColWidth`, `baseColWidth`) has the width the engine worked out (`defaultColPx`), which the layout carries with it.

async function open() {
  const { sandbox: boot } = await createWebUiDom();
  const cat = boot.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = boot.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const calls = [];
  const harness = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-sheet-default-col-px",
    testIpc: async (command, payload) => {
      if (command === "get_workspace_bootstrap") return { ...offline.get_workspace_bootstrap(payload), sheetSessions: true };
      if (command === "evaluate_sheet") {
        const body = JSON.parse(payload);
        if (body.sessionAction === "structure") calls.push(body);
      }
      return offline[command](payload);
    },
  });
  await flushAsyncTurns(10);
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns(4);
  return { ...harness, layoutApi: harness.host.__EASYOFFICE_SHEET_LAYOUT__, calls };
}

const headWidth = (document, col) => document.querySelector(`#pane-sheet th[data-col="${col}"]`).style.width;

test("a sheet that says nothing has 72 px columns, in the header and in the positions the grid hands out", async () => {
  const { document, layoutApi } = await open();
  assert.equal(layoutApi.DEFAULT_COL_PX, 72);
  assert.equal(layoutApi.defaultColPxOf("Sheet1"), 72);
  assert.equal(layoutApi.defaultColPxOf("no such sheet"), 72);
  assert.deepEqual(["A", "B", "H"].map((c) => headWidth(document, c)), ["72px", "72px", "72px"]);
});

test("the layout carries the width the engine worked out and hands it back with the width in characters", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ S: { defaultColWidth: { width: 20.7109375 }, defaultColPx: 145 }, T: { defaultColWidth: { base: 10 }, defaultColPx: 80 } });
  assert.equal(layout.defaultColPxOf("S"), 145);
  assert.equal(layout.defaultColPxOf("T"), 80);
  assert.equal(layout.defaultColPxOf("U"), 72, "a sheet the layout does not know");
  assert.deepEqual(layout.getAll(), { S: { defaultColWidth: { width: 20.7109375 }, defaultColPx: 145 }, T: { defaultColWidth: { base: 10 }, defaultColPx: 80 } });
  // The width of a sheet that has none in characters is not carried: it would be a width without a source.
  layout.replaceAll({ S: { defaultColPx: 145, tabColor: "336699" } });
  assert.equal(layout.defaultColPxOf("S"), 72);
  assert.deepEqual(layout.getAll(), { S: { tabColor: "336699" } });
});

test("a width that is not one the engine can mean is not taken", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  const bad = [7, 2001, 0, -64, 64.5, "145", null, NaN, Infinity, 1e308, {}, [], true];
  layout.replaceAll(Object.fromEntries(bad.map((px, i) => [`S${i}`, { defaultColWidth: { width: 20 }, defaultColPx: px }])));
  for (let i = 0; i < bad.length; i += 1) assert.equal(layout.defaultColPxOf(`S${i}`), 72, String(bad[i]));
  // The two ends of what a sheet may say: the narrowest column and the widest one.
  layout.replaceAll({ A: { defaultColWidth: { base: 0 }, defaultColPx: 8 }, B: { defaultColWidth: { base: 255 }, defaultColPx: 2000 } });
  assert.deepEqual([layout.defaultColPxOf("A"), layout.defaultColPxOf("B")], [8, 2000]);
});

async function openFile(sheet) {
  const h = await createWorkspaceHarness();
  const d = h.document;
  const make = d.createElement.bind(d);
  d.createElement = (tag) => {
    const el = make(tag);
    if (tag === "canvas") el.getContext = () => ({ font: "", measureText: (t) => ({ width: [...t].length * 8 }) });
    return el;
  };
  d.defaultView.getComputedStyle = () => ({ fontFamily: "'맑은 고딕', 'Malgun Gothic', sans-serif", fontSize: "11pt" });
  switchModule(d, "SHEET");
  await flushAsyncTurns();
  const viewport = d.getElementById("sheet-viewport");
  Object.assign(viewport, { clientHeight: 520, clientWidth: 960, scrollTop: 0, scrollLeft: 0 });
  await pickFile(d, makeFixtureFile("기본.xlsx", { sheets: [{ name: "기본", cells: [{ ref: "A1", raw: "x" }], ...sheet }] }));
  await flushAsyncTurns(12);
  return { ...h, viewport, layout: () => h.host.__EASYOFFICE_SHEET_LAYOUT__ };
}

test("the grid draws the columns of a sheet at the width of its sheet and keeps the widths written down", async () => {
  const h = await openFile({ defaultColWidth: { width: 20.7109375 }, defaultColPx: 145, columns: [{ index: 2, widthPx: 300 }, { index: 4, widthPx: 145 }] });
  const d = h.document;
  assert.deepEqual(["A", "B", "C", "D", "E"].map((c) => headWidth(d, c)), ["145px", "300px", "145px", "145px", "145px"]);
  assert.equal(h.layout().defaultColPxOf("기본"), 145);
  // A drag to the width of the sheet is no width of its own, and a drag to another one is.
  const drag = async (col, dx) => {
    const resizer = d.querySelector(`#pane-sheet th[data-col="${col}"] .col-resizer`);
    resizer.dispatchEvent({ type: "mousedown", target: resizer, button: 0, clientX: 500, bubbles: true });
    d.dispatchEvent({ type: "mousemove", target: d, clientX: 500 + dx, bubbles: true });
    d.dispatchEvent({ type: "mouseup", target: d, bubbles: true });
    await flushAsyncTurns(8);
  };
  await drag("B", -155);
  assert.equal(headWidth(d, "B"), "145px");
  await drag("C", 55);
  assert.equal(headWidth(d, "C"), "200px");
});

test("a sheet with no width of its own is 72 px wide whatever the file before it said", async () => {
  const h = await openFile({ defaultColWidth: { base: 10 }, defaultColPx: 80 });
  assert.equal(headWidth(h.document, "A"), "80px");
  await pickFile(h.document, makeFixtureFile("다른.xlsx", { sheets: [{ name: "기본", cells: [{ ref: "A1", raw: "새" }] }] }));
  assert.equal(headWidth(h.document, "A"), "72px");
});

test("a column or row edit sends the width of the sheet to the engine and keeps it afterwards", async () => {
  const { document, userClick, layoutApi, calls } = await open();
  layoutApi.setSheet("Sheet1", { defaultColWidth: { width: 20.7109375, base: 10 }, defaultColPx: 145 });
  await flushAsyncTurns(4);
  const td = document.querySelector('#pane-sheet td[data-cell="A1"]');
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
  td.focus();
  userClick(document.querySelector('.md-item[data-sheet-act="col-insert-left"]'));
  await flushAsyncTurns(20);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0].defaultColWidth, { width: 20.7109375, base: 10 });
  assert.equal(layoutApi.defaultColPxOf("Sheet1"), 145, "the answer does not carry the width: the sheet keeps its own");
  assert.deepEqual(layoutApi.getAll().Sheet1.defaultColWidth, { width: 20.7109375, base: 10 });
  assert.equal("defaultFont" in calls[0], false, "the standard font is no value of the request");
});

// PD-306: the default font of the file (the unit of the widths in characters) comes from the engine's answer, goes back with
// every request that has widths in characters, and is the standard font again for the next file.
test("a file with another default font gives the sheets that say nothing the engine's width, and the request carries the font", async () => {
  const h = await openFile({});
  const font = h.host.__EASYOFFICE_SHEET_DEFAULT_FONT__;
  assert.equal(font.get(), null);
  assert.deepEqual(font.field(), {});
  const file = makeFixtureFile("글꼴.xlsx", { sheets: [{ name: "기본", cells: [{ ref: "A1", raw: "x" }] }], defaultFont: { name: "Arial", size: 12 }, defaultColPx: 80 });
  await pickFile(h.document, file);
  assert.deepEqual(font.get(), { name: "Arial", size: 12, colPx: 80 });
  assert.deepEqual(font.field(), { defaultFont: { name: "Arial", size: 12 } });
  assert.equal(headWidth(h.document, "A"), "80px", "the sheet says nothing: the width of the font");
  assert.equal(h.layout().defaultColPxOf("a sheet added afterwards"), 80);
  // The next file without a font is the standard one again.
  await pickFile(h.document, makeFixtureFile("기본.xlsx", { sheets: [{ name: "기본", cells: [{ ref: "A1", raw: "새" }] }] }));
  assert.equal(font.get(), null);
  assert.equal(headWidth(h.document, "A"), "72px");
  assert.equal(h.layout().defaultColPxOf("a sheet added afterwards"), 72);
});

test("the default font of a snapshot is taken back, and a font that is not a bounded font is the standard one", async () => {
  const { host } = await createWorkspaceHarness();
  const font = host.__EASYOFFICE_SHEET_DEFAULT_FONT__;
  font.set({ name: "Calibri", size: 11 }, 64);
  const snap = font.get();
  assert.deepEqual(snap, { name: "Calibri", size: 11, colPx: 64 });
  font.set(null);
  assert.equal(font.colPx(), 72);
  font.restore(snap);
  assert.deepEqual(font.get(), snap);
  assert.equal(host.__EASYOFFICE_SHEET_LAYOUT__.defaultColPxOf("S"), 64);
  font.restore(null);
  assert.equal(font.get(), null);
  const bad = [
    { name: "F".repeat(65), size: 11 },
    { name: "Bad;Font", size: 11 },
    { name: "Back\\slash", size: 11 },
    { name: "A\nB", size: 11 },
    { name: "", size: 11 },
    { name: " Arial", size: 11 },
    { name: 12, size: 11 },
    { name: "Arial", size: 0 },
    { name: "Arial", size: -1 },
    { name: "Arial", size: 409.5 },
    { name: "Arial", size: NaN },
    { name: "Arial", size: Infinity },
    { name: "Arial", size: "11" },
    { name: "Arial" },
    "Arial",
    [],
    42,
    true,
  ];
  for (const next of bad) {
    font.set(next, 80);
    assert.equal(font.get(), null, JSON.stringify(next));
    assert.equal(font.colPx(), 72);
  }
  // The largest bounded font and the widths a column may have.
  font.set({ name: "F".repeat(64), size: 409 }, 2000);
  assert.equal(font.colPx(), 2000);
  for (const px of [7, 2001, 64.5, "80", null, NaN]) {
    font.set({ name: "Arial", size: 12 }, px);
    assert.equal(font.colPx(), 72, String(px));
  }
});

test("a column edit sends the default font of the file", async () => {
  const { document, userClick, layoutApi, calls, host } = await open();
  host.__EASYOFFICE_SHEET_DEFAULT_FONT__.set({ name: "Arial", size: 12 }, 80);
  layoutApi.setSheet("Sheet1", { defaultColWidth: { width: 20 }, defaultColPx: 180 });
  await flushAsyncTurns(4);
  const td = document.querySelector('#pane-sheet td[data-cell="A1"]');
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
  td.focus();
  userClick(document.querySelector('.md-item[data-sheet-act="col-insert-left"]'));
  await flushAsyncTurns(20);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0].defaultFont, { name: "Arial", size: 12 });
  assert.deepEqual(calls[0].defaultColWidth, { width: 20 });
});
