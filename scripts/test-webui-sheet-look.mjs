import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// Cell alignment, wrapping and borders (PD-207): the edit plans, the commands of the 서식 menu, the dialog
// panes, the paint of the grid, the save and open payloads, and the paste and fill that carry the look.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const L = loadUiModule(UI("sheet-look.js"), {}).__EASYOFFICE_SHEET_LOOK__;

const MAX = 1048576;
const plan = (rg, kind, edge, grid = {}) => {
  const out = L.planBorders({ r1: 1, c1: 1, r2: 1, c2: 1, ...rg }, kind, edge, (r, c) => grid[r + "," + c], MAX, 16384);
  return Object.fromEntries(out.map((x) => [x.r + "," + x.c, x.border]));
};
const THIN = "thin:112233";
const byText = (a, b) => a.localeCompare(b);

test("an edge text needs a known line and a six digit color", () => {
  assert.equal(L.edgeText("thin", "#ABCDEF"), "thin:abcdef");
  assert.equal(L.edgeText("double", "fa0"), "double:ffaa00");
  assert.equal(L.edgeText("wavy", "000000"), null);
  assert.equal(L.edgeText("thin", "red"), null);
  assert.equal(L.parseEdge("thick:zzzzzz"), null);
  assert.deepEqual(L.parseEdge("dotted:0a0b0c"), { line: "dotted", color: "0a0b0c" });
  assert.equal(L.freezeBorder({ t: "thin:12345", r: "bad" }), "", "no valid edge leaves no border");
  assert.deepEqual({ ...L.freezeBorder({ t: THIN, x: THIN }) }, { t: THIN });
});

test("outer borders write the ring cells' facing sides and an inner edge needs two cells", () => {
  const out = plan({ r1: 2, c1: 2, r2: 3, c2: 3 }, "outer", THIN);
  assert.deepEqual(Object.keys(out).sort(byText), ["1,2", "1,3", "2,1", "2,2", "2,3", "2,4", "3,1", "3,2", "3,3", "3,4", "4,2", "4,3"]);
  assert.equal(out["1,2"].b, THIN, "the cell above gets its bottom");
  assert.equal(out["2,2"].t, THIN);
  assert.equal(out["2,2"].l, THIN);
  assert.equal(out["2,2"].r, undefined, "the inner side is not an outer edge");
  assert.deepEqual(plan({}, "inner", THIN), {}, "one cell has no inner edge");
  const inner = plan({ r1: 1, c1: 1, r2: 1, c2: 2 }, "inner", THIN);
  assert.deepEqual(Object.keys(inner).sort(byText), ["1,1", "1,2"]);
  assert.equal(inner["1,1"].r, THIN);
  assert.equal(inner["1,2"].l, THIN);
  const all = plan({ r1: 1, c1: 1, r2: 2, c2: 2 }, "all", THIN);
  assert.equal(all["1,1"].r, THIN);
  assert.equal(all["1,1"].b, THIN);
  assert.equal(all["2,2"].t, THIN);
});

test("the first row and column have no ring and the sheet's last row and column are not exceeded", () => {
  const out = L.planBorders({ r1: 1, c1: 1, r2: 1, c2: 1 }, "outer", THIN, () => undefined, 1, 1);
  assert.deepEqual(out.map((x) => x.r + "," + x.c), ["1,1"]);
});

test("none clears the cells' sides and the ring's facing sides and leaves other cells alone", () => {
  const grid = {
    "2,2": L.freezeBorder({ t: THIN, b: THIN }),
    "1,2": L.freezeBorder({ b: THIN, l: THIN }),
    "5,5": L.freezeBorder({ t: THIN }),
  };
  const out = plan({ r1: 2, c1: 2, r2: 2, c2: 2 }, "none", null, grid);
  assert.deepEqual(Object.keys(out).sort(byText), ["1,2", "2,2"]);
  assert.equal(out["2,2"], "", "no edge is left");
  assert.deepEqual({ ...out["1,2"] }, { l: THIN }, "the ring cell keeps its other side");
});

test("an edge that is already there plans no change", () => {
  const grid = { "1,1": L.freezeBorder({ t: THIN, b: THIN, l: THIN, r: THIN }), "1,2": L.freezeBorder({ l: THIN }), "2,1": L.freezeBorder({ t: THIN }) };
  assert.deepEqual(plan({}, "outer", THIN, grid), {});
});

test("the cell after wins a shared edge and the first row and column draw their own outer edge", () => {
  const at = { "1,1": { t: "thin:000000", r: "thin:111111" }, "1,2": { l: "thick:222222" }, "2,1": { t: "dashed:333333" } };
  const edges = L.resolveEdges(1, 1, (r, c) => at[r + "," + c]);
  assert.equal(edges.r, "thick:222222");
  assert.equal(edges.b, "dashed:333333");
  assert.equal(edges.t, "thin:000000", "row 1");
  assert.equal(edges.l, null);
  const inner = L.resolveEdges(2, 2, () => undefined);
  assert.deepEqual(inner, { t: null, l: null, r: null, b: null });
  const merged = L.resolveEdges(1, 1, (r, c) => ({ "1,3": { r: "double:000000" }, "1,4": { l: "thin:444444" } })[r + "," + c], { r2: 1, c2: 3 });
  assert.equal(merged.r, "thin:444444", "a merged range ends at its last column");
});

// ---- the page -------------------------------------------------------------------------------------

async function createHarness() {
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
  const harness = await createWorkspaceHarness({
    testIpc: async (command, payload) => {
      const body = typeof payload === "string" && payload.startsWith("{") ? JSON.parse(payload) : null;
      if (command === "convert_document_format") converts.push(body);
      return offline[command](payload);
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return { ...harness, converts };
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const look = (d, name) => d.querySelector(`[data-sheet-look="${name}"]`);

async function selectRange(d, from, keys) {
  cell(d, from).focus();
  for (const [k, n] of keys) for (let i = 0; i < n; i += 1) key(d.activeElement, k, { shiftKey: true });
  await flushAsyncTurns(2);
}

const MODEL = {
  sheets: [{ name: "표", cells: [{ ref: "A1", raw: "12" }, { ref: "B1", raw: "글자" }, { ref: "A2", raw: "긴 글 ".repeat(30) }, { ref: "B2", raw: "x" }] }],
};

async function openModel(h, model = MODEL) {
  await pickFile(h.document, makeFixtureFile("서식.xlsx", model));
  await flushAsyncTurns(8);
}

async function click(h, el) {
  h.userClick(el);
  await flushAsyncTurns(14);
}

const undo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
};

test("an alignment command sets every cell of the selection, the same command again clears it, one undo reverts it", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowRight", 1]]);
  await click(h, look(h.document, "align-center"));
  assert.equal(cell(h.document, "A1").style.textAlign, "center");
  assert.equal(cell(h.document, "B1").style.textAlign, "center");
  assert.equal(cell(h.document, "A2").style.textAlign, "", "a cell outside stays");
  assert.equal(h.document.querySelector('.tb-btn[data-sheet-look="align-center"]').getAttribute("aria-pressed"), "true");
  assert.equal(look(h.document, "align-center").getAttribute("aria-checked"), "true");
  await click(h, look(h.document, "align-center"));
  assert.equal(cell(h.document, "A1").style.textAlign, "", "the same alignment again returns to the default");
  await click(h, look(h.document, "align-right"));
  await undo(h);
  assert.equal(cell(h.document, "A1").style.textAlign, "");
});

test("vertical alignment and the bottom default", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, look(h.document, "valign-top"));
  assert.equal(cell(h.document, "A1").style.verticalAlign, "top");
  await click(h, look(h.document, "valign-middle"));
  assert.equal(cell(h.document, "A1").style.verticalAlign, "middle");
  await click(h, look(h.document, "valign-bottom"));
  assert.equal(cell(h.document, "A1").style.verticalAlign, "", "the bottom is the default and has no key");
});

test("wrapping sets the cell's white space and a tall row; clearing it restores the plain cell", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  await click(h, look(h.document, "wrap"));
  const td = cell(h.document, "A2");
  assert.equal(td.style.whiteSpace, "pre-wrap");
  assert.equal(td.style.overflowWrap, "anywhere");
  await click(h, look(h.document, "wrap"));
  assert.equal(cell(h.document, "A2").style.whiteSpace, "");
  assert.equal(cell(h.document, "A2").style.paddingTop, "");
});

test("borders: a range's outer edge is drawn once per shared slot and 없음 removes it", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowRight", 1], ["ArrowDown", 1]]);
  await click(h, look(h.document, "line-double"));
  await click(h, look(h.document, "border-outer"));
  assert.equal(cell(h.document, "A1").style.borderTopStyle, "double", "the first row draws its own top");
  assert.equal(cell(h.document, "A1").style.borderLeftStyle, "double");
  assert.equal(cell(h.document, "B2").style.borderRightStyle, "double");
  assert.equal(cell(h.document, "B2").style.borderBottomStyle, "double");
  assert.equal(cell(h.document, "A1").style.borderRightStyle, "", "no inner edge");
  await click(h, look(h.document, "border-none"));
  assert.equal(cell(h.document, "A1").style.borderTopStyle, "");
  assert.equal(cell(h.document, "B2").style.borderBottomStyle, "");
});

test("a border on the ring's facing side shows in the cell before the range", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "B2").focus();
  await flushAsyncTurns(2);
  await click(h, look(h.document, "line-thick"));
  await click(h, look(h.document, "border-top"));
  assert.equal(cell(h.document, "B1").style.borderBottomWidth, "2px", "B1's bottom slot draws B2's top");
  assert.equal(cell(h.document, "B1").style.borderBottomStyle, "solid");
});

test("the save payload carries the look and a reopened sheet shows it", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowRight", 1]]);
  await click(h, look(h.document, "align-right"));
  await click(h, look(h.document, "valign-middle"));
  await click(h, look(h.document, "wrap"));
  await click(h, look(h.document, "border-all"));
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="studio-save"]'));
  await flushAsyncTurns(20);
  const sheet = h.converts.at(-1).sheets[0];
  const a1 = sheet.cells.find((c) => c.ref === "A1").style;
  assert.equal(a1.ha, "right");
  assert.equal(a1.va, "middle");
  assert.equal(a1.wrap, true);
  assert.deepEqual(a1.bd, { t: "thin:000000", r: "thin:000000", b: "thin:000000", l: "thin:000000" });
  const reopened = await createHarness();
  await openModel(reopened, { sheets: [{ name: "표", cells: sheet.cells }] });
  assert.equal(cell(reopened.document, "A1").style.textAlign, "right");
  assert.equal(cell(reopened.document, "A1").style.borderTopStyle, "solid");
  assert.equal(cell(reopened.document, "A1").style.whiteSpace, "pre-wrap");
});

test("a look survives a typed edit of the cell", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, look(h.document, "align-center"));
  const td = cell(h.document, "A1");
  td.focus();
  key(td, "F2");
  td.textContent = "99";
  key(td, "Enter");
  await flushAsyncTurns(6);
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="studio-save"]'));
  await flushAsyncTurns(20);
  const a1 = h.converts.at(-1).sheets[0].cells.find((c) => c.ref === "A1");
  assert.equal(a1.raw, "99");
  assert.equal(a1.style.ha, "center");
});

test("the dialog's 맞춤 pane writes alignment and wrapping in one undo step and leaves the number format alone", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowRight", 1]]);
  await click(h, look(h.document, "dialog-border").closest("[data-sheet-look]"));
  const $ = (id) => h.document.getElementById(id);
  assert.equal($("sheet-numfmt-dialog").open, true);
  h.document.querySelector('[data-fmt-tab="align"]').click();
  assert.equal(h.document.querySelector('[data-fmt-pane="align"]').hidden, false);
  assert.equal(h.document.querySelector('[data-fmt-pane="numfmt"]').hidden, true);
  $("sheet-align-h-select").value = "center";
  $("sheet-align-h-select").dispatchEvent({ type: "change", target: $("sheet-align-h-select") });
  $("sheet-align-wrap").checked = true;
  await click(h, $("btn-sheet-numfmt-apply"));
  assert.equal($("sheet-numfmt-dialog").open, false);
  assert.equal(cell(h.document, "A1").style.textAlign, "center");
  assert.equal(cell(h.document, "B1").style.textAlign, "center");
  assert.equal(cell(h.document, "A1").style.whiteSpace, "pre-wrap");
  assert.equal(cell(h.document, "A1").dataset.raw, undefined, "no number format was written");
  await undo(h);
  assert.equal(cell(h.document, "A1").style.textAlign, "");
  assert.equal(cell(h.document, "A1").style.whiteSpace, "");
});

test("the dialog's 테두리 pane applies the chosen line, color and positions", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowRight", 1]]);
  await click(h, look(h.document, "dialog-border"));
  const $ = (id) => h.document.getElementById(id);
  h.document.querySelector('[data-fmt-tab="border"]').click();
  h.document.querySelector('[data-border-line="dashed"]').click();
  h.document.querySelector('[data-border-preset="all"]').click();
  assert.equal(h.document.querySelector('[data-border-pos="v"]').getAttribute("aria-pressed"), "true");
  assert.equal($("sheet-border-preview").querySelectorAll("line").length > 0, true, "the preview draws the edges");
  h.document.querySelector('[data-border-pos="b"]').click();
  await click(h, $("btn-sheet-numfmt-apply"));
  assert.equal(cell(h.document, "A1").style.borderTopStyle, "dashed");
  assert.equal(cell(h.document, "A1").style.borderRightStyle, "dashed", "the vertical center line is between A1 and B1");
  assert.equal(cell(h.document, "B1").style.borderBottomStyle, "", "the bottom position was switched off");
});

test("only a dialog edit of the number format writes the format, and an untouched 확인 keeps the old behavior", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(6);
  assert.equal(h.document.querySelector('[data-fmt-tab="numfmt"]').getAttribute("aria-selected"), "true");
  await click(h, h.document.getElementById("btn-sheet-numfmt-apply"));
  assert.equal(h.document.getElementById("sheet-numfmt-dialog").open, false);
});

test("pasting formats and filling carry the look", async () => {
  const h = await createHarness();
  await openModel(h);
  const { document } = h;
  cell(document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, look(document, "align-center"));
  await click(h, look(document, "wrap"));
  await click(h, look(document, "border-bottom"));
  const store = {};
  cell(document, "A1").dispatchEvent({ type: "copy", target: cell(document, "A1"), clipboardData: { setData: (t, v) => (store[t] = v), getData: (t) => store[t] || "" } });
  cell(document, "A4").focus();
  await flushAsyncTurns(2);
  h.userClick(document.querySelector('.md-item[data-sheet-paste="special"]'));
  document.getElementById("sheet-paste-kind-select").value = "formats";
  h.userClick(document.getElementById("btn-sheet-paste-apply"));
  await flushAsyncTurns(15);
  const a4 = cell(document, "A4");
  assert.equal(a4.style.textAlign, "center");
  assert.equal(a4.style.whiteSpace, "pre-wrap");
  assert.equal(a4.style.borderBottomStyle, "solid");
  // The fill handle repeats the look of the one source cell.
  cell(document, "A1").focus();
  await flushAsyncTurns(2);
  const handle = document.querySelector("#sheet-viewport .sheet-fill-handle");
  handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
  const target = cell(document, "A3");
  target.dispatchEvent({ type: "mouseover", target, bubbles: true });
  target.dispatchEvent({ type: "mouseup", target, bubbles: true });
  await flushAsyncTurns(15);
  assert.equal(cell(document, "A3").style.textAlign, "center");
  assert.equal(cell(document, "A3").style.borderBottomStyle, "solid");
});

test("the 서식 menu and the toolbar list the look commands, each with a label and no explanation", async () => {
  const h = await createHarness();
  const items = [...h.document.querySelectorAll(".md-item[data-sheet-look]")];
  const labels = items.map((el) => el.querySelector(".md-label").textContent);
  assert.deepEqual(labels.slice(0, 7), ["왼쪽 정렬", "가운데 정렬", "오른쪽 정렬", "위쪽 맞춤", "가운데 맞춤", "아래쪽 맞춤", "줄 바꿈"]);
  assert.deepEqual(labels.slice(7), ["모두", "바깥쪽", "안쪽", "위쪽", "아래쪽", "왼쪽", "오른쪽", "없음", "가는 실선", "굵은 실선", "파선", "점선", "이중 실선", "테두리 색…"]);
  const buttons = [...h.document.querySelectorAll(".tb-btn[data-sheet-look]")];
  assert.equal(buttons.length, 8);
  for (const b of buttons) assert.ok(b.getAttribute("aria-label"), b.dataset.sheetLook);
});

// ---- wrapped text and row heights ---------------------------------------------------------------

const RF = loadUiModule(UI("sheet-row-fit.js"), { __EASYOFFICE_SHEET_LOOK__: L });

test("lines are counted by words, by characters in a wide word and by explicit breaks", () => {
  const w = (t) => t.length * 10;
  assert.equal(RF.__EASYOFFICE_SHEET_LINE_COUNT__("short", 100, w), 1);
  assert.equal(RF.__EASYOFFICE_SHEET_LINE_COUNT__("aaaa bbbb cccc", 90, w), 2, "two words fit a line");
  assert.equal(RF.__EASYOFFICE_SHEET_LINE_COUNT__("a".repeat(25), 100, w), 3, "a word wider than the line breaks by characters");
  assert.equal(RF.__EASYOFFICE_SHEET_LINE_COUNT__("가나다라마바사아자차", 50, w), 2, "other scripts break anywhere");
  assert.equal(RF.__EASYOFFICE_SHEET_LINE_COUNT__("a\n\nb", 100, w), 3, "an empty line counts");
  assert.equal(RF.__EASYOFFICE_SHEET_LINE_COUNT__("", 100, w), 1);
});

function fit(map, measure = (text, width) => Math.ceil((text.length * 10) / width), widthOf = () => 113) {
  const refs = loadUiModule(UI("sheet-refs.js"), {}).__EASYOFFICE_SHEET_REFS__;
  return RF.__EASYOFFICE_CREATE_ROW_FIT__({
    rowH: 24,
    getSheetName: () => "S",
    getMap: () => map,
    columnWidth: widthOf,
    isRowHidden: () => false,
    isMerged: () => false,
    measure,
    packRef: refs.packRef,
  });
}

test("a row is as tall as its tallest wrapped cell, other rows and unwrapped cells keep the row height", () => {
  const wrapped = { wrap: true };
  const map = {
    A2: { value: "x".repeat(30), style: wrapped },
    B2: { value: "y".repeat(10), style: wrapped },
    A4: { value: "z".repeat(5), style: wrapped },
    A6: { value: "x".repeat(300), style: { fontSize: "13px" } },
  };
  const f = fit(map);
  assert.equal(f.sync(), true);
  // width 100: A2 takes 3 lines (3 * 18 + 6 + 1), A4 one line (not taller than a plain row).
  assert.equal(f.heightOf(2), 3 * 18 + 7);
  assert.equal(f.heightOf(4), 24);
  assert.equal(f.heightOf(6), 24, "no wrap, no growth");
  assert.equal(f.extraBefore(2), 0);
  assert.equal(f.extraBefore(3), 3 * 18 + 7 - 24);
  assert.equal(f.extraTotal(), 3 * 18 + 7 - 24);
  assert.equal(f.sync(), false, "unchanged cells give the same heights");
});

test("a tall row is capped and a column too narrow for text adds nothing", () => {
  const map = { A1: { value: "x".repeat(100000), style: { wrap: true } } };
  const f = fit(map);
  f.sync();
  assert.equal(f.heightOf(1), 545);
  const narrow = fit(map, undefined, () => 20);
  assert.equal(narrow.sync(), false);
  assert.equal(narrow.any(), false);
});

test("measuring is remembered per font, width and text", () => {
  let calls = 0;
  const map = {};
  for (let r = 1; r <= 50; r += 1) map["A" + r] = { value: "same text here", style: { wrap: true } };
  const f = fit(map, () => ((calls += 1), 2));
  f.sync();
  f.sync();
  assert.equal(calls, 1);
});

test("the wrapped rows of a sheet move the rows below them in the grid", async () => {
  const h = await createHarness();
  h.host.getComputedStyle = () => ({ fontFamily: "sans-serif", fontSize: "13px" });
  const canvas = { getContext: () => ({ measureText: (t) => ({ width: t.length * 7 }), font: "" }) };
  const make = h.document.createElement.bind(h.document);
  h.document.createElement = (tag, ...rest) => (tag === "canvas" ? canvas : make(tag, ...rest));
  await openModel(h);
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  await click(h, look(h.document, "wrap"));
  const row = h.document.querySelector('#pane-sheet tbody tr[data-row="2"]');

  assert.match(row.style.height, /^\d+px$/, "the wrapped row has its own height");
  assert.ok(parseInt(row.style.height, 10) > 24);
  assert.equal(h.document.querySelector('#pane-sheet tbody tr[data-row="1"]').style.height, "");
  await undo(h);
  assert.equal(h.document.querySelector('#pane-sheet tbody tr[data-row="2"]').style.height, "", "undo restores the plain row");
});
