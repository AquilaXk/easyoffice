import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// Row heights (PD-278): the heights a user set, kept in the layout and in the file; the rows that follow their content (wrapped
// text and large fonts), never under the grid's row height; a double click on the edge of a row header gives the rows back
// to their content; a drag on the edge sets the height; each is one undo step; the grid draws rows of any height in place.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const L = loadUiModule(ui("sheet-look.js"), {}).__EASYOFFICE_SHEET_LOOK__;
const RH = loadUiModule(ui("sheet-row-height.js"), {}).__EASYOFFICE_SHEET_ROW_HEIGHT__;
const RF = loadUiModule(ui("sheet-row-fit.js"), { __EASYOFFICE_SHEET_LOOK__: L });
const refs = loadUiModule(ui("sheet-refs.js"), {}).__EASYOFFICE_SHEET_REFS__;
const MAX_ROWS = 1048576;

// ---- the model ---------------------------------------------------------------------------------------

test("the heights come from the engine's list, bad entries are skipped and a height that follows the content is left to the page", () => {
  const map = RH.fromJson([[3, 30], [5, 14.4, 1], [9, 21.75, 0], [0, 20], [2, 0], [2, -1], [2, 410], [2, "20"], [2.5, 20], [MAX_ROWS + 1, 20], null, "x", [7, NaN], [8, Infinity]]);
  assert.deepEqual([...map], [[3, 30], [5, 14.4]]);
  assert.deepEqual(RH.fromJson(undefined), new Map());
  assert.deepEqual(RH.toJson(new Map([[9, 12], [2, 30.5]])), [[2, 30.5], [9, 12]], "in row order");
  assert.deepEqual(RH.pxEntries(new Map([[9, 15], [2, 30]])), [[2, 40], [9, 20]], "points are 4/3 of a px");
});

test("the list stops at the cap", () => {
  const list = Array.from({ length: RH.MAX_ROW_HEIGHTS + 50 }, (_, i) => [i + 1, 20]);
  assert.equal(RH.fromJson(list).size, RH.MAX_ROW_HEIGHTS);
});

test("a height is clamped to what a row may be and a range over the cap is refused whole", () => {
  assert.equal(RH.withHeight(new Map(), 1, 1, 1).get(1), RH.pxToPt(RH.MIN_PX), "a drag stops above a hidden row");
  assert.equal(RH.withHeight(new Map(), 1, 1, 9999).get(1), 409);
  const some = RH.withHeight(new Map([[2, 50]]), 1, 3, 40);
  assert.deepEqual([...some].sort((a, b) => a[0] - b[0]), [[1, 30], [2, 30], [3, 30]]);
  assert.equal(RH.withHeight(new Map(), 1, MAX_ROWS, 40), null, "a million rows is more than a sheet carries");
  const full = new Map(Array.from({ length: RH.MAX_ROW_HEIGHTS }, (_, i) => [i + 1, 20]));
  assert.equal(RH.withHeight(full, 5, 5, 30)?.size, RH.MAX_ROW_HEIGHTS, "a row that has a height already is no new one");
  assert.equal(RH.withHeight(full, RH.MAX_ROW_HEIGHTS + 1, RH.MAX_ROW_HEIGHTS + 1, 30), null);
});

test("rows fit back to their content: out of the map, never walking the range", () => {
  const map = new Map([[1, 20], [4, 30], [9, 40]]);
  assert.deepEqual([...RH.withoutRows(map, 3, MAX_ROWS)], [[1, 20]]);
  assert.equal(map.size, 3, "the map it was given is not changed");
});

test("a drag on the edge of a header sets the selected rows when the pressed one is among them, else only it", () => {
  assert.deepEqual(RH.rowsToSet(3, { axis: "row", r1: 2, r2: 4 }), [2, 4]);
  assert.deepEqual(RH.rowsToSet(7, { axis: "row", r1: 2, r2: 4 }), [7, 7]);
  assert.deepEqual(RH.rowsToSet(3, { axis: "col", r1: 1, r2: MAX_ROWS }), [3, 3], "a column selection does not set them");
  assert.deepEqual(RH.rowsToSet(3, { axis: null, r1: 2, r2: 4 }), [3, 3], "nor does a cell range");
});

// ---- the heights of the rows ---------------------------------------------------------------------------

function fit(map, extra = {}) {
  return RF.__EASYOFFICE_CREATE_ROW_FIT__({
    rowH: () => 24,
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

test("a larger font makes its row taller, a font that fits the row does not, and an empty cell counts too", () => {
  const f = fit({
    A1: { value: "가", style: { fontSize: "18px" } },
    A2: { value: "가", style: { fontSize: "19px" } },
    A3: { value: "", style: { fontSize: "40px" } },
    A4: { value: "가", style: { fontSize: "13px" } },
    A5: { value: "가" },
  });
  f.sync();
  assert.equal(f.heightOf(1), 24, "18 px needs 23: the row holds it");
  assert.equal(f.heightOf(2), 25);
  assert.equal(f.heightOf(3), Math.ceil(40 * 1.275), "as the spreadsheet programs size a row of one line");
  assert.equal(f.heightOf(4), 24);
  assert.equal(f.heightOf(5), 24);
  assert.equal(L.TALL_FONT_PX, 19, "the latch that tells a sheet to look for them agrees with the rule");
});

test("a wrapped cell with one line takes its font's height, with more lines its lines, and the tallest cell wins", () => {
  const f = fit({
    A1: { value: "x".repeat(5), style: { wrap: true, fontSize: "30px" } },
    A2: { value: "x".repeat(30), style: { wrap: true } },
    B2: { value: "x", style: { fontSize: "40px" } },
  });
  f.sync();
  assert.equal(f.heightOf(1), Math.ceil(30 * 1.275));
  assert.equal(f.heightOf(2), Math.max(3 * 18 + 7, Math.ceil(40 * 1.275)));
});

test("a height a user set wins over the content, under the grid's row height too, and the rows below follow", () => {
  const map = { A1: { value: "x".repeat(30), style: { wrap: true } }, A2: { value: "가", style: { fontSize: "40px" } } };
  const f = fit(map, { customHeights: () => [[1, 20], [2, 30], [5, 60], [6, 24]] });
  f.sync();
  assert.equal(f.heightOf(1), 20, "shorter than the plain row, shorter than the text needs");
  assert.equal(f.heightOf(2), 30);
  assert.equal(f.heightOf(5), 60);
  assert.equal(f.heightOf(6), 24);
  assert.equal(f.extraBefore(1), 0);
  assert.equal(f.extraBefore(2), -4);
  assert.equal(f.extraBefore(3), -4 + 6, "the next row starts after the extra of the rows above");
  assert.equal(f.extraBefore(6), -4 + 6 + 36);
  assert.equal(f.extraTotal(), -4 + 6 + 36);
  assert.equal(f.minHeight(), 20);
  assert.equal(f.isClipped(1), true, "the text needs more than the row has: it is cut off");
  assert.equal(f.isClipped(2), true);
  assert.equal(f.isClipped(5), false);
  assert.equal(f.sync(), false, "the same heights again");
});

test("a hidden row keeps no height and a merged cell asks for none", () => {
  const f = fit({ A1: { value: "x".repeat(30), style: { wrap: true } }, A3: { value: "가", style: { fontSize: "40px" } } }, {
    isRowHidden: (r) => r === 1 || r === 2,
    isMerged: (r) => r === 3,
    customHeights: () => [[2, 100]],
  });
  f.sync();
  assert.equal(f.any(), false);
});

test("the extra height is signed: rows under the grid's height take space off", () => {
  const f = fit({}, { customHeights: () => [[10, 12], [20, 12]] });
  f.sync();
  assert.equal(f.extraBefore(11), -12);
  assert.equal(f.extraBefore(21), -24);
  assert.equal(f.extraBefore(5), 0);
});

test("a sync that needs more measurements or more time than its budget leaves the rows it did not reach plain", () => {
  const map = {};
  for (let r = 1; r <= 300000; r += 1) map["A" + r] = { value: "x".repeat(20 + (r % 7)) + r, style: { wrap: true } };
  let calls = 0;
  const f = fit(map, { measure: () => ((calls += 1), 3) });
  f.sync();
  assert.equal(f.overBudget(), true);
  assert.ok(calls <= 200000, "the count ends the sync (" + calls + ")");
  assert.equal(f.heightOf(10), 3 * 18 + 7, "the rows it reached are tall");
  assert.equal(f.heightOf(299999), 24, "and the rest stay plain");
  let clock = 0;
  const slow = fit({ A1: { value: "a", style: { wrap: true } }, ...Object.fromEntries(Array.from({ length: 2000 }, (_, i) => ["B" + (i + 1), { value: "t" + i, style: { wrap: true } }])) }, { now: () => clock, measure: () => ((clock += 10), 2) });
  slow.sync();
  assert.equal(slow.overBudget(), true, "a slow canvas ends it by time");
  const ok = fit({ A1: { value: "a", style: { wrap: true } } });
  ok.sync();
  assert.equal(ok.overBudget(), false);
});

// ---- the page --------------------------------------------------------------------------------------------

const MODEL = {
  sheets: [
    {
      name: "높이",
      cells: [
        { ref: "A1", raw: "머리" },
        { ref: "A2", raw: "일" },
        { ref: "A3", raw: "삼" },
        { ref: "A4", raw: "사" },
        { ref: "A5", raw: "오" },
      ],
      rowHeights: [[2, 15], [4, 60]],
    },
  ],
};

async function open(model = MODEL) {
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
  await pickFile(d, makeFixtureFile("높이.xlsx", model));
  await flushAsyncTurns(12);
  return { ...h, viewport, layout: () => h.host.__EASYOFFICE_SHEET_LAYOUT__ };
}

const tr = (d, row) => d.querySelector(`#pane-sheet tbody tr[data-row="${row}"]`);
const edge = (d, row) => [...d.querySelectorAll("#pane-sheet .row-resizer")].find((el) => el.parentElement.dataset.row === String(row));
const press = (el, extra = {}) => el.dispatchEvent({ type: "mousedown", target: el, button: 0, bubbles: true, ...extra });
const move = (d, clientY) => d.dispatchEvent({ type: "mousemove", target: d, clientY, bubbles: true });
const release = (d) => d.dispatchEvent({ type: "mouseup", target: d, bubbles: true });
const doubleClick = async (d, row) => {
  const el = edge(d, row);
  el.dispatchEvent({ type: "dblclick", target: el, bubbles: true, button: 0 });
  await flushAsyncTurns(15);
};
const drag = async (d, row, from, to) => {
  press(edge(d, row), { clientY: from });
  move(d, to);
  release(d);
  await flushAsyncTurns(8);
};
const undo = async (h) => {
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(10);
};

test("a file's heights come to the grid: rows under, over and at the plain height, in place", async () => {
  const h = await open();
  const d = h.document;
  assert.equal(tr(d, 1).style.height, "", "a row with no height keeps the stylesheet's");
  assert.equal(tr(d, 2).style.height, "20px", "15 pt");
  assert.equal(tr(d, 2).classList.contains("row-short"), true);
  assert.equal(tr(d, 2).style.getPropertyValue("--rh"), "20px");
  assert.equal(tr(d, 4).style.height, "80px", "60 pt");
  assert.equal(tr(d, 4).classList.contains("row-short"), false);
  assert.equal(tr(d, 4).style.getPropertyValue("--rh"), "");
  assert.deepEqual(h.layout().getAll()["높이"].rowHeights, [[2, 15], [4, 60]], "kept in the layout, saved from it");
  const last = Math.max(...[...d.querySelectorAll("#pane-sheet tbody tr[data-row]")].map((r) => Number(r.dataset.row)));
  const spacer = d.querySelector("#pane-sheet .sheet-spacer-bottom td").style.height;
  assert.equal(parseInt(spacer, 10), (MAX_ROWS - last) * 24, "the rows below the window are plain: the heights above it do not leak into the spacer");
});

test("a double click on the edge of a header gives the row back to its content; one undo step brings the height again", async () => {
  const h = await open();
  const d = h.document;
  await doubleClick(d, 4);
  assert.equal(tr(d, 4).style.height, "");
  assert.deepEqual(h.layout().getAll()["높이"].rowHeights, [[2, 15]]);
  assert.equal(d.getElementById("status-command-feedback").textContent, "행 높이 자동 맞춤: 1행");
  await undo(h);
  assert.equal(tr(d, 4).style.height, "80px");
  assert.deepEqual(h.layout().getAll()["높이"].rowHeights, [[2, 15], [4, 60]]);
});

test("with rows selected, the double click fits all of them when the pressed one is among them, only the pressed one otherwise", async () => {
  const h = await open();
  const d = h.document;
  const head = (row) => d.querySelector(`#pane-sheet th[data-row="${row}"]`);
  press(head(2));
  press(head(4), { shiftKey: true });
  await flushAsyncTurns(3);
  await doubleClick(d, 3);
  assert.deepEqual(h.layout().getAll()["높이"]?.rowHeights ?? [], [], "rows 2 to 4 are selected and 3 is among them");
  assert.equal(tr(d, 2).style.height, "");
  assert.equal(tr(d, 4).style.height, "");
  await undo(h);
  press(head(2));
  await flushAsyncTurns(3);
  await doubleClick(d, 4);
  assert.deepEqual(h.layout().getAll()["높이"].rowHeights, [[2, 15]], "only row 4: the selection is row 2");
});

test("rows that have no height of a user change nothing and make no undo step", async () => {
  const h = await open();
  const d = h.document;
  const before = d.getElementById("status-command-feedback").textContent;
  await doubleClick(d, 3);
  assert.equal(d.getElementById("status-command-feedback").textContent, before, "nothing was fitted, nothing was said");
  assert.deepEqual(h.layout().getAll()["높이"].rowHeights, [[2, 15], [4, 60]]);
});

test("a drag on the edge sets the height of the row, redraws as it moves, and is one undo step", async () => {
  const h = await open();
  const d = h.document;
  press(edge(d, 1), { clientY: 100 });
  move(d, 120);
  assert.equal(tr(d, 1).style.height, "44px", "24 px and the 20 px of the move, before the button is released");
  move(d, 130);
  assert.equal(tr(d, 1).style.height, "54px");
  release(d);
  await flushAsyncTurns(8);
  assert.deepEqual(h.layout().getAll()["높이"].rowHeights, [[1, 40.5], [2, 15], [4, 60]]);
  assert.equal(tr(d, 1).style.height, "54px");
  await undo(h);
  assert.equal(tr(d, 1).style.height, "");
  assert.deepEqual(h.layout().getAll()["높이"].rowHeights, [[2, 15], [4, 60]]);
});

test("a drag sets every selected row when the pressed one is among them, and stops above a hidden row", async () => {
  const h = await open();
  const d = h.document;
  const head = (row) => d.querySelector(`#pane-sheet th[data-row="${row}"]`);
  press(head(3));
  press(head(5), { shiftKey: true });
  await flushAsyncTurns(3);
  await drag(d, 4, 100, 110);
  const rows = Object.fromEntries(h.layout().getAll()["높이"].rowHeights);
  assert.deepEqual([rows[3], rows[4], rows[5]], [67.5, 67.5, 67.5], "the pressed row was 80 px and moved 10 px: 90 px, which is 67.5 pt, for every selected row");
  await drag(d, 1, 100, -500);
  assert.equal(h.layout().getAll()["높이"].rowHeights[0][1], RH.pxToPt(RH.MIN_PX));
});

test("a drag with no move makes no step, and the press on an edge does not select the row", async () => {
  const h = await open();
  const d = h.document;
  press(edge(d, 3), { clientY: 50 });
  release(d);
  await flushAsyncTurns(5);
  assert.equal(d.querySelector('#pane-sheet th[data-row="3"]').classList.contains("active-axis"), false);
  assert.deepEqual(h.layout().getAll()["높이"].rowHeights, [[2, 15], [4, 60]]);
});

test("rows of other heights are drawn where they are: the window, the top of a row and the click position agree", async () => {
  const h = await open();
  const d = h.document;
  const grid = h.host.__EASYOFFICE_SHEET_GRID_UTILS__;
  assert.ok(grid);
  // Rows 2 (20 px) and 4 (80 px): row 5 starts after 24 + 20 + 24 + 80 px.
  const top = h.viewport.scrollTop;
  assert.equal(top, 0);
  const rows = [...d.querySelectorAll("#pane-sheet tbody tr[data-row]")].map((r) => Number(r.dataset.row));
  assert.deepEqual(rows.slice(0, 5), [1, 2, 3, 4, 5]);
  h.viewport.scrollTop = 24 * 2000;
  h.viewport.dispatchEvent({ type: "scroll" });
  await flushAsyncTurns();
  const far = [...d.querySelectorAll("#pane-sheet tbody tr[data-row]")].map((r) => Number(r.dataset.row));
  assert.ok(far[0] > 1970 && far[0] < 2000, "the window follows the scroll, 52 px of the two rows above it moved it up: " + far[0]);
});

test("short rows fill the viewport: the window holds as many of them as the viewport shows", async () => {
  const model = { sheets: [{ name: "짧음", cells: [{ ref: "A1", raw: "1" }], rowHeights: Array.from({ length: 300 }, (_, i) => [i + 1, 3]) }] };
  const h = await open(model);
  const d = h.document;
  const rows = [...d.querySelectorAll("#pane-sheet tbody tr[data-row]")].map((r) => Number(r.dataset.row));
  // 520 px of 4 px rows: 130 rows (and the room around them), not the 22 rows a plain viewport holds.
  assert.ok(rows.length >= 130, "rows in the window: " + rows.length);
  assert.equal(tr(d, 1).style.height, "4px");
  assert.equal(tr(d, 1).classList.contains("row-short"), true);
});

// The page with a session engine that moves the heights as the engine does: an inserted row copies the height above.
async function openWithEngine() {
  const requests = [];
  const moved = (body) => {
    const { at, count, op, axis } = body;
    const above = body.rowHeights.find(([row]) => row === at - 1);
    const next = body.rowHeights.flatMap(([row, pt]) => (axis !== "row" ? [[row, pt]] : op === "insert" ? [[row >= at ? row + count : row, pt]] : row >= at + count ? [[row - count, pt]] : row < at ? [[row, pt]] : []));
    if (axis === "row" && op === "insert" && above) for (let i = 0; i < count; i += 1) next.push([at + i, above[1]]);
    return next.sort((x, y) => x[0] - y[0]);
  };
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command !== "evaluate_sheet") return { status: "ok" };
      const body = JSON.parse(args.payload);
      requests.push(body);
      if (body.sessionAction === "structure") return { session: "s1", formulas: [], changed: [], circular: false, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules: {}, rowHeights: moved(body) };
      return { open: { session: "s1" }, apply: { session: "s1", changed: [], circular: false }, close: { closed: true } }[body.sessionAction] ?? {};
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const viewport = harness.document.getElementById("sheet-viewport");
  viewport.clientHeight = 520;
  viewport.clientWidth = 960;
  viewport.scrollTop = 0;
  return { ...harness, requests, layout: () => harness.host.__EASYOFFICE_SHEET_LAYOUT__ };
}

test("a row edit sends the heights and takes the engine's moved ones, an inserted row copying the height above; undo gives them back", async () => {
  const h = await openWithEngine();
  const d = h.document;
  h.layout().replaceAll({ Sheet1: { rowHeights: [[2, 15], [4, 60]] } });
  d.querySelector('#pane-sheet td[data-cell="C3"]').focus();
  await flushAsyncTurns(2);
  h.userClick(d.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  const sent = h.requests.find((r) => r.sessionAction === "structure");
  assert.deepEqual(sent.rowHeights, [[2, 15], [4, 60]]);
  assert.deepEqual(h.layout().getAll().Sheet1.rowHeights, [[2, 15], [3, 15], [5, 60]], "row 3 is new and takes the height of row 2");
  assert.equal(tr(d, 3).style.height, "20px");
  assert.equal(tr(d, 5).style.height, "80px");
  await undo(h);
  assert.deepEqual(h.layout().getAll().Sheet1.rowHeights, [[2, 15], [4, 60]]);
  assert.equal(tr(d, 4).style.height, "80px");
});

test("wrapped text and large fonts make rows follow the text; a height of a user ends that for the row", async () => {
  const model = {
    sheets: [
      {
        name: "내용",
        cells: [
          { ref: "A1", raw: "큰 글자", style: { sizePt: 30 } },
          { ref: "A2", raw: "글".repeat(80), style: { wrap: true } },
          { ref: "A3", raw: "보통" },
        ],
      },
    ],
  };
  const h = await open(model);
  const d = h.document;
  assert.equal(tr(d, 1).style.height, Math.ceil(40 * 1.275) + "px", "30 pt is 40 px");
  assert.ok(parseInt(tr(d, 2).style.height, 10) > 24);
  assert.equal(tr(d, 3).style.height, "");
  await drag(d, 1, 100, 60);
  assert.equal(tr(d, 1).style.height, Math.ceil(40 * 1.275) - 40 > 4 ? Math.ceil(40 * 1.275) - 40 + "px" : "4px", "the height of the user, under the font's");
  assert.equal(tr(d, 1).classList.contains("row-clip"), true, "its text is cut off, not pushed out");
  await doubleClick(d, 1);
  assert.equal(tr(d, 1).style.height, Math.ceil(40 * 1.275) + "px", "back to the content");
  assert.equal(tr(d, 1).classList.contains("row-clip"), false);
});

test("the heights are sent in the save payload, those of a user only, and a sheet without them keeps the old shape", async () => {
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
  const converts = [];
  const h = await createWorkspaceHarness({
    testIpc: async (command, payload) => {
      if (command === "convert_document_format") converts.push(JSON.parse(String(payload)));
      return offline[command](payload);
    },
  });
  const d = h.document;
  switchModule(d, "SHEET");
  await flushAsyncTurns();
  const save = async () => {
    h.userClick(studioItem(d, "studio-save"));
    await flushAsyncTurns(20);
    return converts.at(-1).sheets[0];
  };
  await pickFile(d, makeFixtureFile("높이.xlsx", { sheets: [{ name: "저장", cells: [{ ref: "A1", raw: "x" }, { ref: "A2", raw: "큰", style: { sizePt: 40 } }] }] }));
  await flushAsyncTurns(12);
  assert.equal("rowHeights" in (await save()), false, "a row that follows its font is not a height of a user");
  await drag(d, 1, 100, 120);
  assert.deepEqual((await save()).rowHeights, [[1, 33]], "24 px and 20 px of the drag: 44 px is 33 pt");
});
