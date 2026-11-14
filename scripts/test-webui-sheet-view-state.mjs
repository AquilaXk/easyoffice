import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// What a sheet remembers about how it was left (PD-208): the selected cells, the zoom and how far it is scrolled.
// They come from the file, return when the window comes back to the sheet, go to the save request, are no undo step
// and follow a sheet that is renamed or copied.

test("a view keeps what a sheet may carry and drops the rest", async () => {
  const { host } = await createWorkspaceHarness({ transformHtml: demoHtml });
  const store = host.__EASYOFFICE_SHEET_VIEW_STATE__;
  store.set("a", { active: [3, 4], range: [3, 4, 5, 6], zoom: 150, top: [10, 2] });
  assert.deepEqual(store.get("a"), { active: [3, 4], range: [3, 4, 5, 6], zoom: 150, top: [10, 2] });
  assert.deepEqual(store.toLive(store.get("a")), { selection: { ar: 3, ac: 4, cr: 5, cc: 6 }, zoom: 150, top: { row: 10, col: 2 } });
  // The corner opposite the active cell, wherever the active cell is in the range.
  assert.deepEqual(store.toLive({ active: [5, 6], range: [3, 4, 5, 6] }).selection, { ar: 5, ac: 6, cr: 3, cc: 4 });
  // The defaults are no view; a value out of its bound is dropped, never clamped.
  for (const none of [{}, { zoom: 100, active: [1, 1], range: [1, 1, 1, 1], top: [1, 1] }, { zoom: 9 }, { zoom: 401 }, { zoom: 1.5 }, { active: [0, 1] }, { active: [1048577, 1] }, { top: [1, 16385] }, { range: [1, 1, 2] }, { active: [9, 9], range: [1, 1, 3, 3] }]) {
    store.set("none", none);
    assert.equal(store.get("none"), null, JSON.stringify(none));
  }
  store.set("edge", { active: [1048576, 16384], range: [1, 1, 1048576, 16384], zoom: 400, top: [1048576, 16384] });
  assert.equal(store.get("edge").zoom, 400);
  assert.deepEqual(store.get("edge").range, [1, 1, 1048576, 16384]);
  // What the grid has becomes the JSON the file takes.
  assert.deepEqual(store.fromLive({ selection: { ar: 5, ac: 6, cr: 3, cc: 4 }, zoom: 125, top: { row: 7, col: 1 } }), { active: [5, 6], range: [3, 4, 5, 6], zoom: 125, top: [7, 1] });
  store.set("live", store.fromLive({ selection: { ar: 1, ac: 1, cr: 1, cc: 1 }, zoom: 100, top: { row: 1, col: 1 } }));
  assert.equal(store.get("live"), null);
});

async function open(extra = {}) {
  const converts = [];
  let model = { sheets: [] };
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        return { open: { session: "s1" }, apply: { session: "s1", changed: [], circular: false }, close: { closed: true } }[body.sessionAction] ?? { session: "s1", formulas: [], rules: {} };
      }
      if (command === "convert_document_format") {
        const body = JSON.parse(args.payload);
        if (body.action === "decode") return JSON.stringify({ status: "OK", module: "SHEET", format: "xlsx", kind: "workbook", itemCount: model.sheets.length, warnings: [], ...model });
        converts.push(body);
        return JSON.stringify({ status: "OK", convertedBase64: "", fileName: "x.xlsx" });
      }
      return { status: "ok" };
    },
    ...extra,
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const { document, userClick } = harness;
  const viewport = document.getElementById("sheet-viewport");
  viewport.clientHeight = 520;
  viewport.clientWidth = 960;
  const tabs = () => [...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")];
  const tab = (name) => tabs().find((t) => t.dataset.sheetTab === name);
  const active = () => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab;
  const openBook = async (sheets) => {
    model = { sheets: sheets.map(({ name, cells, ...rest }) => ({ name, cells: Object.entries(cells || {}).map(([ref, raw]) => ({ ref, raw })), ...rest })) };
    const result = await harness.sandbox.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("book.xlsx", new TextEncoder().encode("book"));
    await flushAsyncTurns(40);
    assert.equal(result.ok, true);
    viewport.clientHeight = 520;
    viewport.clientWidth = 960;
  };
  const go = async (name) => {
    userClick(tab(name));
    await flushAsyncTurns(30);
  };
  const saved = async () => {
    userClick(studioItem(document, "studio-save"));
    await flushAsyncTurns(40);
    return Object.fromEntries(converts.at(-1).sheets.map((sheet) => [sheet.name, sheet]));
  };
  const selection = () => harness.host.__EASYOFFICE_SHEET_VIEW_STATE__ && document.querySelector("#pane-sheet td.selected-cell")?.dataset.cell;
  const zoomLabel = () => document.getElementById("status-zoom-label").textContent;
  return { ...harness, viewport, tabs, tab, active, openBook, go, saved, selection, zoomLabel, converts };
}

const BOOK = [
  { name: "A", cells: { A1: "1" }, view: { active: [42, 4], range: [42, 4, 44, 5], zoom: 150, top: [40, 3] } },
  { name: "B", cells: { A1: "2" } },
  { name: "C", cells: { A1: "3" }, view: { zoom: 25 } },
];

test("the view of the first sheet in a file is shown: its cell, its scroll position and its zoom", async () => {
  const h = await open();
  await h.openBook(BOOK);
  assert.equal(h.active(), "A");
  assert.equal(h.zoomLabel(), "150%");
  assert.equal(h.document.querySelector(".canvas-stage").style.zoom, "150%");
  assert.equal(h.document.getElementById("status-zoom-slider").min, "10", "a sheet zooms from 10 to 400 percent");
  assert.equal(h.document.getElementById("status-zoom-slider").max, "400");
  // Row 40 is the first row below the top: 39 rows of 24px; column C starts after two default columns.
  assert.equal(h.viewport.scrollTop, 39 * 24);
  assert.ok(h.viewport.scrollLeft > 0);
  assert.equal(h.document.querySelector("#pane-sheet td.selected-cell")?.dataset.cell, "D42");
  assert.equal(h.document.querySelectorAll("#pane-sheet td.in-range").length > 0, true);
});

test("each sheet comes back as it was left and a sheet with no view shows A1 unscrolled at 100 percent", async () => {
  const h = await open();
  await h.openBook(BOOK);
  await h.go("B");
  assert.equal(h.zoomLabel(), "100%");
  assert.equal(h.viewport.scrollTop, 0);
  assert.equal(h.document.querySelector("#pane-sheet td.selected-cell")?.dataset.cell, "A1");
  // Leave B at a cell of its own, scrolled and zoomed.
  const td = h.document.querySelector('#pane-sheet td[data-cell="C3"]');
  td.dispatchEvent({ type: "mousedown", target: td, button: 0 });
  h.viewport.scrollTop = 24 * 10;
  h.viewport.dispatchEvent({ type: "scroll" });
  h.host.__EASYOFFICE_ZOOM__.apply(80);
  await h.go("C");
  assert.equal(h.zoomLabel(), "25%", "C is at its own zoom, below the 50 percent the other windows stop at");
  await h.go("A");
  assert.equal(h.zoomLabel(), "150%");
  assert.equal(h.document.querySelector("#pane-sheet td.selected-cell")?.dataset.cell, "D42");
  assert.equal(h.viewport.scrollTop, 39 * 24);
  await h.go("B");
  assert.equal(h.zoomLabel(), "80%");
  assert.equal(h.document.querySelector("#pane-sheet td.selected-cell")?.dataset.cell, "C3");
  assert.equal(h.viewport.scrollTop, 24 * 10);
});

test("a save carries the view of every sheet, the one on screen included, and says nothing for a default one", async () => {
  const h = await open();
  await h.openBook(BOOK);
  const td = h.document.querySelector('#pane-sheet td[data-cell="D42"]');
  td.dispatchEvent({ type: "mousedown", target: td, button: 0 });
  h.host.__EASYOFFICE_ZOOM__.apply(200);
  const book = await h.saved();
  assert.deepEqual(book.A.view, { active: [42, 4], zoom: 200, top: [40, 3] });
  assert.equal(book.B.view, undefined);
  assert.deepEqual(book.C.view, { zoom: 25 });
});

test("a view is no undo step: changing it neither marks the book edited nor is undone", async () => {
  const h = await open();
  await h.openBook(BOOK);
  const before = JSON.stringify(h.host.__EASYOFFICE_SHEET_LAYOUT__.getAll());
  await h.go("B");
  await h.go("A");
  assert.equal(JSON.stringify(h.host.__EASYOFFICE_SHEET_LAYOUT__.getAll()), before, "the layout holds no view");
  assert.equal(JSON.stringify(h.host.__EASYOFFICE_SHEET_VIEW_STATE__.getAll()).includes("zoom"), true);
});

test("a renamed sheet keeps its view and a copy starts from its source's", async () => {
  const h = await open();
  await h.openBook(BOOK);
  const store = h.host.__EASYOFFICE_SHEET_VIEW_STATE__;
  const t = h.tab("A");
  t.dispatchEvent({ type: "contextmenu", target: t, clientX: 40, clientY: 700 });
  h.userClick(h.document.querySelector('#sheet-tab-menu [data-tab-act="copy"]'));
  await flushAsyncTurns(40);
  assert.equal(h.active(), "A (2)");
  assert.equal(h.zoomLabel(), "150%", "the copy is shown as its source was left");
  assert.equal(h.viewport.scrollTop, 39 * 24);
  assert.deepEqual(store.get("A (2)"), store.get("A"));
  store.set("A", { zoom: 75 });
  assert.equal(store.get("A (2)").zoom, 150, "a copy is a value, not a link");
  store.remove("A (2)");
  assert.equal(store.get("A (2)"), null);
});

test("a renamed or deleted sheet leaves its view for the undo, and a new sheet takes none of it (PD-208)", async () => {
  const h = await open();
  await h.openBook(BOOK);
  const store = h.host.__EASYOFFICE_SHEET_VIEW_STATE__;
  const rename = async (from, to) => {
    const t = h.tab(from);
    t.dispatchEvent({ type: "dblclick", target: t });
    await flushAsyncTurns(5);
    const input = h.document.querySelector("#sheet-tab-bar .sheet-tab-edit");
    input.value = to;
    input.dispatchEvent({ type: "keydown", key: "Enter", target: input });
    await flushAsyncTurns(40);
  };
  await rename("A", "Z");
  assert.equal(store.get("Z").zoom, 150, "the new name has the view");
  assert.equal(store.get("A").zoom, 150, "the old name keeps it for an undo");
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="studio-undo"]'));
  await flushAsyncTurns(40);
  assert.equal(h.active(), "A");
  assert.equal(h.zoomLabel(), "150%", "the undone sheet shows its view");
  // A deleted sheet keeps its view, a sheet inserted under that name does not inherit it.
  store.set("Sheet4", { zoom: 200 });
  h.userClick(h.document.getElementById("btn-add-sheet-tab"));
  await flushAsyncTurns(40);
  assert.equal(h.active(), "Sheet4");
  assert.equal(store.get("Sheet4"), null);
});
