import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";
import { fakeSortEngine } from "./test-support/fake-sort-engine.mjs";

// Cell fill and font colors (PD-227): the model keys, the toolbar buttons and their palettes, the 글꼴 and 채우기
// panes of the 셀 서식 dialog, the paint of the grid, the save and open payloads, and the commands that carry a
// cell's format (undo, paste, fill, sort, row insert, sheet copy, find and replace, protection).

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const L = loadUiModule(UI("sheet-look.js"), {}).__EASYOFFICE_SHEET_LOOK__;

test("a color is six lower case digits, and a style with only a color is a look", () => {
  assert.equal(L.normalizeColor("#FFCC00"), "ffcc00");
  assert.equal(L.normalizeColor("fc0"), "ffcc00");
  assert.equal(L.normalizeColor("red"), null);
  assert.equal(L.hasLook({ fill: "ffcc00" }), true);
  assert.equal(L.hasLook({ fontColor: "123456" }), true);
  assert.equal(L.hasLook({ fill: "" }), false);
  assert.notEqual(L.lookKey({ fill: "ffcc00" }), L.lookKey({ fontColor: "ffcc00" }), "the fill and the font color are different looks");
  assert.equal(L.lookKey({ fill: "ffcc00" }), L.lookKey({ fill: "ffcc00" }));
  assert.ok(L.LOOK_KEYS.includes("fill") && L.LOOK_KEYS.includes("fontColor"), "paste, fill and the model copy follow the look keys");
});

test("text with no color is white on a dark fill and black on any other, and an explicit color is kept", () => {
  assert.equal(L.autoTextColor("000000"), "ffffff");
  assert.equal(L.autoTextColor("0000ff"), "ffffff", "pure blue is dark");
  assert.equal(L.autoTextColor("ffff00"), "000000");
  assert.equal(L.autoTextColor("808080"), "000000", "a middle gray is not dark");
  assert.equal(L.textColorOf({}), "", "no fill, no color: the theme's text");
  assert.equal(L.textColorOf({ fill: "000000" }), "ffffff");
  assert.equal(L.textColorOf({ fill: "000000", fontColor: "ff0000" }), "ff0000");
});

test("printing draws the fill and the text color of a cell", () => {
  const td = { style: {} };
  L.printStyle(td, { fill: "ffeb9c", fontColor: "9c5700" });
  assert.equal(td.style.backgroundColor, "#ffeb9c");
  assert.equal(td.style.color, "#9c5700");
  const dark = { style: {} };
  L.printStyle(dark, { fill: "000000" });
  assert.equal(dark.style.color, "#ffffff");
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
const $ = (h, id) => h.document.getElementById(id);

async function selectRange(d, from, keys) {
  cell(d, from).focus();
  for (const [k, n] of keys) for (let i = 0; i < n; i += 1) key(d.activeElement, k, { shiftKey: true });
  await flushAsyncTurns(2);
}

const MODEL = {
  sheets: [{ name: "표", cells: [{ ref: "A1", raw: "12" }, { ref: "B1", raw: "글자" }, { ref: "A2", raw: "가" }, { ref: "B2", raw: "x" }, { ref: "A3", raw: "나" }] }],
};

async function openModel(h, model = MODEL) {
  await pickFile(h.document, makeFixtureFile("색.xlsx", model));
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
const redo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(14);
};

const fillOf = (h, ref) => cell(h.document, ref).style.getPropertyValue("--cell-fill");
const textOf = (h, ref) => cell(h.document, ref).style.getPropertyValue("--cell-fg");
const autoOf = (h, ref) => cell(h.document, ref).style.getPropertyValue("--cell-auto-fg");
const swatch = (h, paletteId, color) => [...$(h, paletteId).querySelectorAll(".sb-hl-swatch")].find((b) => b.title === color);

test("the toolbar has a 채우기 색 and a 글꼴 색 button with the reference palette and the spreadsheet program's clear words", async () => {
  const h = await createHarness();
  for (const [button, label] of [["btn-sheet-fill", "채우기 색"], ["btn-sheet-font-color", "글꼴 색"]]) {
    assert.equal($(h, button).getAttribute("title"), label);
    assert.ok($(h, button).getAttribute("aria-label"));
  }
  const clearLabel = (palette) => [...$(h, palette).querySelectorAll(".sb-hl-palette-actions button")].map((b) => b.textContent);
  assert.deepEqual(clearLabel("sheet-fill-palette"), ["채우기 없음", "다른 색..."]);
  assert.deepEqual(clearLabel("sheet-font-color-palette"), ["자동", "다른 색..."]);
  assert.ok($(h, "sheet-fill-palette").querySelector(".sb-hl-palette-row"), "the rows of the reference palette");
  assert.match($(h, "sheet-fill-palette").textContent, /최근에 사용한 색/);
});

test("the fill button applies its color to the selection, the palette picks another, one undo reverts each", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowRight", 1]]);
  await click(h, $(h, "btn-sheet-fill"));
  assert.equal(fillOf(h, "A1"), "#ffff00", "the button's own color");
  assert.equal(fillOf(h, "B1"), "#ffff00");
  assert.equal(fillOf(h, "A2"), "", "a cell outside stays");
  assert.equal(cell(h.document, "A1").dataset.fill, "1");
  assert.equal(textOf(h, "A1"), "", "no text color of its own");
  assert.equal(autoOf(h, "A1"), "#000000", "automatic text on a light fill is black, whatever the theme");
  await click(h, swatch(h, "sheet-fill-palette", "#0000ff"));
  assert.equal(fillOf(h, "A1"), "#0000ff");
  assert.equal(autoOf(h, "A1"), "#ffffff", "automatic text on a dark fill is white");
  assert.equal($(h, "btn-sheet-fill").style.getPropertyValue("--bar-color"), "#0000ff", "the button shows the color it applies");
  await undo(h);
  assert.equal(fillOf(h, "A1"), "#ffff00", "one undo steps back one color");
  await undo(h);
  assert.equal(fillOf(h, "A1"), "");
  assert.equal(cell(h.document, "A1").dataset.fill, undefined);
  assert.equal(textOf(h, "A1"), "");
  await redo(h);
  assert.equal(fillOf(h, "B1"), "#ffff00");
});

test("the font color button colors the text and 자동 gives the automatic color back", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  await click(h, $(h, "btn-sheet-font-color"));
  assert.equal(textOf(h, "A2"), "#ff0000");
  assert.equal(fillOf(h, "A2"), "");
  assert.equal(cell(h.document, "A2").dataset.fg, "1");
  await click(h, swatch(h, "sheet-font-color-palette", "#00ff00"));
  assert.equal(textOf(h, "A2"), "#00ff00");
  const auto = [...$(h, "sheet-font-color-palette").querySelectorAll(".sb-hl-palette-actions button")].find((b) => b.textContent === "자동");
  await click(h, auto);
  assert.equal(textOf(h, "A2"), "");
  assert.equal(cell(h.document, "A2").dataset.fg, undefined);
  await undo(h);
  assert.equal(textOf(h, "A2"), "#00ff00");
});

test("black text on a cell with no fill is marked automatic for the dark theme and black text on a fill is kept", async () => {
  const h = await createHarness();
  const cells = [
    { ref: "A1", raw: "a", style: { fc: "000000" } },
    { ref: "A2", raw: "b", style: { fc: "000000", fill: "ffff00" } },
    { ref: "A3", raw: "c", style: { fill: "000000" } },
  ];
  await openModel(h, { sheets: [{ name: "표", cells }] });
  assert.equal(cell(h.document, "A1").dataset.fg, "black");
  assert.equal(cell(h.document, "A2").dataset.fg, "1");
  assert.equal(cell(h.document, "A3").dataset.fg, undefined, "no text color of its own: the automatic one is by the fill");
  assert.equal(autoOf(h, "A3"), "#ffffff");
});

test("a protected sheet refuses a color unless it allows 셀 서식", async () => {
  const h = await createHarness();
  await openModel(h);
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const guard = h.host.__EASYOFFICE_SHEET_PROTECT__;
  const base = guard.FLAG_NAMES.filter((f) => f !== "selectLockedCells" && f !== "selectUnlockedCells");
  layout.setProtect("표", { on: true, deny: base });
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, $(h, "btn-sheet-fill"));
  assert.equal(fillOf(h, "A1"), "", "formatCells is not allowed");
  await click(h, $(h, "btn-sheet-numfmt-apply")?.closest("dialog") ? $(h, "btn-sheet-font-color") : $(h, "btn-sheet-font-color"));
  assert.equal(textOf(h, "A1"), "");
  layout.setProtect("표", { on: true, deny: base.filter((f) => f !== "formatCells") });
  await click(h, $(h, "btn-sheet-fill"));
  assert.equal(fillOf(h, "A1"), "#ffff00", "formatCells is allowed");
});

// ---- the dialog -----------------------------------------------------------------------------------

test("the 글꼴 and 채우기 panes write the colors in one undo step with the other panes", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowRight", 1]]);
  h.userClick(h.document.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(6);
  assert.deepEqual([...h.document.querySelectorAll("#sheet-numfmt-dialog [data-fmt-tab]")].map((t) => t.textContent), ["표시 형식", "맞춤", "글꼴", "테두리", "채우기", "보호"]);
  h.document.querySelector('[data-fmt-tab="fill"]').click();
  assert.equal(h.document.querySelector('[data-fmt-pane="fill"]').hidden, false);
  await click(h, swatch(h, "sheet-bgcolor-palette", "#ff9999"));
  assert.equal($(h, "sheet-fill-sample").style.getPropertyValue("--sample-fill"), "#ff9999");
  h.document.querySelector('[data-fmt-tab="font"]').click();
  await click(h, swatch(h, "sheet-fcolor-palette", "#0000ff"));
  assert.equal($(h, "sheet-font-sample").style.getPropertyValue("--sample-text"), "#0000ff");
  await click(h, $(h, "btn-sheet-numfmt-apply"));
  assert.equal($(h, "sheet-numfmt-dialog").open, false);
  assert.equal(fillOf(h, "A1"), "#ff9999");
  assert.equal(fillOf(h, "B1"), "#ff9999");
  assert.equal(textOf(h, "B1"), "#0000ff");
  await undo(h);
  assert.equal(fillOf(h, "A1"), "");
  assert.equal(textOf(h, "A1"), "");
});

test("the dialog shows the colors of the selection, a mixed selection as none, and 채우기 없음 removes the fill", async () => {
  const h = await createHarness();
  const cells = [{ ref: "A1", raw: "a", style: { fill: "ff0000" } }, { ref: "B1", raw: "b", style: { fill: "ff0000" } }, { ref: "A2", raw: "c", style: { fill: "00ff00" } }];
  await openModel(h, { sheets: [{ name: "표", cells }] });
  await selectRange(h.document, "A1", [["ArrowRight", 1]]);
  h.userClick(h.document.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(6);
  const chip = () => $(h, "sheet-bgcolor-host").querySelector(".dialog-swatch-chip").style.getPropertyValue("--dialog-swatch");
  assert.equal(chip(), "#ff0000", "both cells have it");
  const none = [...$(h, "sheet-bgcolor-palette").querySelectorAll(".sb-hl-palette-actions button")].find((b) => b.textContent === "채우기 없음");
  await click(h, none);
  assert.equal(chip(), "");
  await click(h, $(h, "btn-sheet-numfmt-apply"));
  assert.equal(fillOf(h, "A1"), "");
  assert.equal(fillOf(h, "B1"), "");
  assert.equal(fillOf(h, "A2"), "#00ff00", "a cell outside the selection stays");
  await selectRange(h.document, "A1", [["ArrowDown", 1]]);
  h.userClick(h.document.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(6);
  assert.equal(chip(), "", "A1 has none and A2 has one: mixed shows no chip");
  await click(h, $(h, "btn-sheet-numfmt-apply"));
  assert.equal(fillOf(h, "A2"), "#00ff00", "an untouched 확인 changes nothing");
});

// ---- the payloads ---------------------------------------------------------------------------------

test("the save payload carries fill and fc, and a reopened sheet shows them", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowRight", 1]]);
  await click(h, $(h, "btn-sheet-fill"));
  await click(h, $(h, "btn-sheet-font-color"));
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="studio-save"]'));
  await flushAsyncTurns(20);
  const sheet = h.converts.at(-1).sheets[0];
  const a1 = sheet.cells.find((c) => c.ref === "A1").style;
  assert.equal(a1.fill, "ffff00");
  assert.equal(a1.fc, "ff0000");
  assert.equal(sheet.cells.find((c) => c.ref === "A2").style, undefined, "a plain cell has no style");
  const reopened = await createHarness();
  await openModel(reopened, { sheets: [{ name: "표", cells: sheet.cells }] });
  assert.equal(fillOf(reopened, "A1"), "#ffff00");
  assert.equal(textOf(reopened, "A1"), "#ff0000");
});

test("a payload color that is not six digits is ignored on open", async () => {
  const h = await createHarness();
  const cells = [{ ref: "A1", raw: "a", style: { fill: "red", fc: "12345" } }, { ref: "A2", raw: "b", style: { fill: "ABCDEF" } }];
  await openModel(h, { sheets: [{ name: "표", cells }] });
  assert.equal(fillOf(h, "A1"), "");
  assert.equal(fillOf(h, "A2"), "#abcdef");
});

// ---- the commands that carry a cell's format --------------------------------------------------------

test("copying and pasting formats carries the colors, and so does the fill handle", async () => {
  const h = await createHarness();
  await openModel(h);
  const { document } = h;
  cell(document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, $(h, "btn-sheet-fill"));
  await click(h, $(h, "btn-sheet-font-color"));
  const store = {};
  cell(document, "A1").dispatchEvent({ type: "copy", target: cell(document, "A1"), clipboardData: { setData: (t, v) => (store[t] = v), getData: (t) => store[t] || "" } });
  cell(document, "A4").focus();
  await flushAsyncTurns(2);
  h.userClick(document.querySelector('.md-item[data-sheet-paste="special"]'));
  document.getElementById("sheet-paste-kind-select").value = "formats";
  h.userClick(document.getElementById("btn-sheet-paste-apply"));
  await flushAsyncTurns(15);
  assert.equal(fillOf(h, "A4"), "#ffff00");
  assert.equal(textOf(h, "A4"), "#ff0000");
  assert.equal(cell(document, "A4").textContent, "", "formats only: the value of A1 did not come along");
  cell(document, "A1").focus();
  await flushAsyncTurns(2);
  const handle = document.querySelector("#sheet-viewport .sheet-fill-handle");
  handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
  const target = cell(document, "A3");
  target.dispatchEvent({ type: "mouseover", target, bubbles: true });
  target.dispatchEvent({ type: "mouseup", target, bubbles: true });
  await flushAsyncTurns(15);
  assert.equal(fillOf(h, "A3"), "#ffff00");
  assert.equal(textOf(h, "A3"), "#ff0000");
  await undo(h);
  assert.equal(fillOf(h, "A3"), "", "one undo reverts the fill");
});

test("a color survives a typed edit of the cell and a find and replace of its text", async () => {
  const h = await createHarness();
  await openModel(h);
  const { document } = h;
  cell(document, "A2").focus();
  await flushAsyncTurns(2);
  await click(h, $(h, "btn-sheet-fill"));
  const td = cell(document, "A2");
  td.focus();
  key(td, "F2");
  td.textContent = "99";
  key(td, "Enter");
  await flushAsyncTurns(6);
  assert.equal(fillOf(h, "A2"), "#ffff00", "the typed cell keeps its fill");
  h.sandbox.dispatchWindowKeydown({ key: "f", ctrlKey: true });
  h.sandbox.dispatchWindowKeydown({ key: "F2", ctrlKey: true });
  await flushAsyncTurns(3);
  for (const [id, value] of [["find-query", "99"], ["find-replacement", "77"]]) {
    $(h, id).value = value;
    $(h, id).dispatchEvent({ type: "input", target: $(h, id) });
  }
  $(h, "btn-find-replace-all").click();
  await flushAsyncTurns(10);
  assert.equal(cell(document, "A2").textContent, "77");
  assert.equal(fillOf(h, "A2"), "#ffff00", "a replaced text keeps the format");
});

test("a sheet copy takes the colors of its source", async () => {
  const h = await createHarness();
  await openModel(h);
  h.host.__EASYOFFICE_SHEET_TAB_STRIP__.render({ document: h.document, keys: ["표"], active: "표" });
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, $(h, "btn-sheet-fill"));
  const tab = h.document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="표"]');
  tab.dispatchEvent({ type: "contextmenu", target: tab, clientX: 40, clientY: 700 });
  h.userClick(h.document.querySelector('#sheet-tab-menu [data-tab-act="copy"]'));
  await flushAsyncTurns(30);
  assert.notEqual(h.document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab, "표", "the copy is the active sheet");
  assert.equal(fillOf(h, "A1"), "#ffff00");
  assert.equal(cell(h.document, "A1").dataset.fill, "1");
});

test("the formulas of a cell with a conditional format keep the cell's own colors as the fallback", async () => {
  const fs = await import("node:fs");
  const css = fs.readFileSync(UI("styles.css"), "utf8");
  assert.match(css, /td\[data-cf\]:not\(\.selected-cell\):not\(\.in-range\) \{ background-color: var\(--cf-bg, var\(--cell-fill, transparent\)\); \}/);
  assert.match(css, /td\[data-cf\] \{ color: var\(--cf-fg, var\(--cell-fg, inherit\)\)/);
  assert.match(css, /td\[data-fill\]:where\(:not\(\[data-fg\]\):not\(\[data-link\]\)\) \{ color: var\(--cell-auto-fg/);
  // The cell's own colors come first in the sheet, so the number format color and the conditional format win.
  assert.ok(css.indexOf("td[data-fg] { color: var(--cell-fg, inherit); }") < css.indexOf("td.nf-colored"), "the cell text color is below the number format color");
  assert.ok(css.indexOf("td[data-fg] { color: var(--cell-fg, inherit); }") < css.indexOf("td[data-cf] { color:"), "and below the conditional format");
});

// A page with a session engine that answers every structure request with no rewrites (a stand-in for the real one).
async function openWithEngine() {
  const answers = {
    open: { session: "s1" },
    apply: { session: "s1", changed: [], circular: false },
    structure: { session: "s1", formulas: [], changed: [], circular: false, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules: {} },
    close: { closed: true },
  };
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") return answers[JSON.parse(args.payload).sessionAction] ?? {};
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return harness;
}

test("an inserted row or column takes the colors of the one before it and moves the rest; one undo reverts both", async () => {
  const h = await openWithEngine();
  const { document } = h;
  await selectRange(document, "A2", [["ArrowRight", 1]]);
  await click(h, $(h, "btn-sheet-fill"));
  assert.equal(fillOf(h, "A2"), "#ffff00");
  const text = (ref) => cell(document, ref).textContent;
  const before3 = text("A3");
  cell(document, "A2").focus();
  await flushAsyncTurns(2);
  h.userClick(document.querySelector('.md-item[data-sheet-act="row-insert-below"]'));
  await flushAsyncTurns(20);
  assert.equal(text("A3"), "", "the new row holds no content");
  assert.equal(fillOf(h, "A3"), "#ffff00", "and the format of the row above");
  assert.equal(fillOf(h, "B3"), "#ffff00");
  assert.equal(fillOf(h, "A2"), "#ffff00");
  assert.equal(text("A4"), before3, "the old third row moved down");
  assert.equal(fillOf(h, "A4"), "", "and has no fill");
  h.userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(20);
  assert.equal(text("A3"), before3);
  assert.equal(fillOf(h, "A3"), "", "undo takes the inherited format away");
  assert.equal(fillOf(h, "A2"), "#ffff00");
  cell(document, "B2").focus();
  await flushAsyncTurns(2);
  h.userClick(document.querySelector('.md-item[data-sheet-act="col-insert-right"]'));
  await flushAsyncTurns(20);
  assert.equal(fillOf(h, "C2"), "#ffff00", "a column inserted after B takes B's format");
  assert.equal(fillOf(h, "C3"), "", "B3 had none");
});

test("the first row has nothing before it, so a row inserted above it is plain", async () => {
  const h = await openWithEngine();
  const { document } = h;
  cell(document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, $(h, "btn-sheet-fill"));
  h.userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  assert.equal(fillOf(h, "A1"), "", "the new first row");
  assert.equal(fillOf(h, "A2"), "#ffff00", "the old first row moved down with its fill");
});

test("a sort moves the colors with their rows", async () => {
  const colName = (c) => String.fromCharCode(64 + c);
  const gridUtils = { refOf: (r, c) => colName(c) + r };
  const sandbox = { __EASYOFFICE_SHEET_UTILS__: { normalizeFormattedNumericCell: (v) => String(v ?? "").trim() } };
  const host = loadUiModule(UI("sheet-sort.js"), sandbox);
  const run = host.__EASYOFFICE_CREATE_SHEET_SORT__({
    gridUtils,
    call: async (body) => fakeSortEngine(body),
    layoutApi: { mergesIn: () => [], rowHidden: () => false },
    cloneStyle: (st) => (st ? { ...st } : undefined),
  }).run;
  const red = { fill: "ff0000" };
  const blue = { fontColor: "0000ff" };
  const map = {
    A1: { value: "c", formula: "", style: red },
    B1: { value: "x", formula: "" },
    A2: { value: "a", formula: "" },
    B2: { value: "y", formula: "", style: blue },
    A3: { value: "b", formula: "", style: blue },
    B3: { value: "z", formula: "" },
  };
  const result = await run("S", map, { range: { r1: 1, c1: 1, r2: 3, c2: 2 }, keys: [{ col: 1, asc: true }], hasHeader: false, caseSensitive: false });
  assert.equal(result.ok, true);
  assert.deepEqual([map.A1.value, map.A2.value, map.A3.value], ["a", "b", "c"]);
  assert.equal(map.A3.style.fill, "ff0000", "the red fill went down with c");
  assert.equal(map.B1.style.fontColor, "0000ff", "the blue text went up with a");
  assert.equal(map.A2.style.fontColor, "0000ff");
  assert.equal(map.A1.style, undefined);
});
