import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// Cell number formats (PD-193): presets, decimal places, the cell format dialog, the display of
// formatted cells, and the save and open payloads.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const N = loadUiModule(UI("sheet-numfmt.js"), {}).__EASYOFFICE_SHEET_NUMFMT__;

test("the presets keep the reference menu order and codes", () => {
  assert.deepEqual(
    N.PRESETS.map((p) => p.id),
    ["general", "text", "number", "percent", "scientific", "accounting", "financial", "currency", "rounding-currency", "date", "time", "datetime", "duration"]
  );
  assert.equal(N.presetById("percent").code, "0.00%");
  assert.equal(N.presetById("currency").code, '"₩"#,##0.00_);[Red]("₩"#,##0.00)');
  assert.equal(N.presetById("accounting").code, '"₩" #,##0.00_);[Red]("₩"#,##0.00)');
  assert.equal(N.currencyFormat("$", 0), '"$"#,##0_);[Red]("$"#,##0)');
});

test("decimal places are set per section and leave quoted text alone", () => {
  assert.equal(N.setPatternDecimal("0.00", 3), "0.000");
  assert.equal(N.setPatternDecimal("0", 2), "0.00");
  assert.equal(N.setPatternDecimal("#,##0.00_);[Red](#,##0.00)", 0), "#,##0_);[Red](#,##0)");
  assert.equal(N.setPatternDecimal('0.0"x.y;z"', 2), '0.00"x.y;z"');
  assert.equal(N.setPatternDecimal("[Red]0", 1), "[Red]0.0", "digits inside a bracket section are not placeholders");
  assert.equal(N.decimalsOf("#,##0.000"), 3);
  assert.equal(N.decimalsOf('0"a.bc"'), 0);
  assert.equal(N.getDecimalString(99).length, N.MAX_DECIMALS);
});

test("adding and removing a decimal place follows the reference commands", () => {
  const plan = (cells, delta) => N.planDecimalChange(cells, delta);
  assert.deepEqual(plan([{ code: "", raw: "3.14159" }, { code: "0.00", raw: "1" }, { code: "", raw: "abc" }, { code: "", raw: "5" }], 1), ["0.000000", "0.000", undefined, "0.000000"]);
  assert.deepEqual(plan([{ code: "0.00", raw: "3" }, { code: "#,##0", raw: "9" }], -1), ["0.0", undefined]);
  assert.deepEqual(plan([{ code: "", raw: "1.5e-7" }], 1), ["0.000E+00"], "a General scientific number switches to a scientific code");
  assert.deepEqual(plan([{ code: "yyyy-mm-dd", raw: "45000" }, { code: "@", raw: "1" }], 1), [undefined, undefined], "dates and text keep their code");
});

test("a code is sorted into the dialog categories", () => {
  const kind = (code) => N.patternKind(code);
  assert.deepEqual(
    ["", "@", "0.00", "#,##0", "0.0%", "0.00E+00", "# ?/?", "yyyy-mm-dd", "yyyy-mm-dd;@", "h:mm:ss", "[h]:mm", 'm"월" d"일"', '"₩"#,##0', '"₩" #,##0.00_);[Red]("₩"#,##0.00)', '0.0"x"', "[Red]0"].map(kind),
    ["general", "text", "number", "number", "percent", "scientific", "fraction", "date", "date", "time", "time", "date", "currency", "accounting", "number", "number"]
  );
  assert.equal(N.buildCode("number", { decimals: 2, thousands: true, negative: "red-paren" }), "#,##0.00_);[Red](#,##0.00)");
  assert.equal(N.buildCode("currency", { decimals: 0, negative: "paren" }), '"₩"#,##0_);("₩"#,##0)');
  assert.equal(N.buildCode("percent", { decimals: 1 }), "0.0%");
  assert.equal(N.buildCode("general", {}), "");
});

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
  const formats = [];
  const harness = await createWorkspaceHarness({
    testIpc: async (command, payload) => {
      const body = typeof payload === "string" && payload.startsWith("{") ? JSON.parse(payload) : null;
      if (command === "convert_document_format") converts.push(body);
      if (body?.sessionAction === "format") formats.push(body);
      return offline[command](payload);
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return { ...harness, converts, formats };
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const action = (d, name) => d.querySelector(`[data-sheet-numfmt="${name}"]`);

async function selectRange(d, from, keys) {
  cell(d, from).focus();
  for (const [k, n] of keys) for (let i = 0; i < n; i += 1) key(d.activeElement, k, { shiftKey: true });
  await flushAsyncTurns(2);
}

const MODEL = {
  sheets: [
    {
      name: "표",
      cells: [
        { ref: "A1", raw: "1234.5" },
        { ref: "A2", raw: "-0.256" },
        { ref: "A3", raw: "3.14159" },
        { ref: "A4", raw: "-1234.6" },
        { ref: "B1", raw: "글자" },
      ],
    },
  ],
};

async function openModel(h, model = MODEL) {
  await pickFile(h.document, makeFixtureFile("서식.xlsx", model));
  await flushAsyncTurns(8);
}

async function click(h, el) {
  h.userClick(el);
  await flushAsyncTurns(14);
}

test("a preset from the 표시 형식 menu formats every cell of the selection and one undo reverts it", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowDown", 2]]);
  await click(h, action(h.document, "number"));
  assert.equal(cell(h.document, "A1").textContent, "1235");
  assert.equal(cell(h.document, "A1").dataset.raw, "1234.5", "the stored value stays as typed");
  assert.equal(cell(h.document, "A3").textContent, "3");
  assert.equal(cell(h.document, "B1").textContent, "글자", "a cell outside the selection is untouched");
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.equal(cell(h.document, "A1").textContent, "1234.5");
  assert.equal(cell(h.document, "A1").dataset.raw, undefined);
});

test("the percent and decimal buttons of the toolbar edit the code", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  await click(h, action(h.document, "percent"));
  assert.equal(cell(h.document, "A2").textContent, "-25.60%");
  await click(h, action(h.document, "decimal-down"));
  assert.equal(cell(h.document, "A2").textContent, "-25.6%");
  await click(h, action(h.document, "decimal-up"));
  await click(h, action(h.document, "decimal-up"));
  assert.equal(cell(h.document, "A2").textContent, "-25.600%");
  assert.equal(cell(h.document, "A3").textContent, "3.14159", "a cell outside the selection stays");
});

test("a General cell gains decimal places from its own digits", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "A3").focus();
  await flushAsyncTurns(2);
  await click(h, action(h.document, "decimal-up"));
  assert.equal(cell(h.document, "A3").dataset.raw, "3.14159");
  assert.equal(cell(h.document, "A3").textContent, "3.141590");
});

test("a section color of the code reaches the cell", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A3", [["ArrowDown", 1]]);
  await click(h, action(h.document, "dialog"));
  const code = h.document.getElementById("sheet-numfmt-code");
  code.value = "#,##0;[Red]-#,##0";
  code.dispatchEvent({ type: "input", target: code });
  await flushAsyncTurns(6);
  assert.equal(h.document.getElementById("sheet-numfmt-kind-select").value, "custom");
  await click(h, h.document.getElementById("btn-sheet-numfmt-apply"));
  const positive = cell(h.document, "A3");
  const negative = cell(h.document, "A4");
  assert.equal(positive.textContent, "3");
  assert.equal(positive.classList.contains("nf-colored"), false);
  assert.equal(negative.textContent, "-1,235");
  assert.equal(negative.classList.contains("nf-colored"), true);
  assert.equal(negative.style["--nf-color"], "#ff0000");
  assert.equal(negative.style.color, "", "the color is not an inline font property the model would read back");
});

test("the cell format dialog loads the anchor cell's code, builds codes from its fields and shows a sample", async () => {
  const h = await createHarness();
  await openModel(h, { sheets: [{ name: "표", numFmts: ["0.0%"], cells: [{ ref: "A1", raw: "0.256", style: { nf: 1 } }, { ref: "A2", raw: "9" }] }] });
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, action(h.document, "dialog"));
  const $ = (id) => h.document.getElementById(id);
  assert.equal($("sheet-numfmt-kind-select").value, "percent");
  assert.equal($("sheet-numfmt-code").value, "0.0%");
  assert.equal($("sheet-numfmt-decimals").value, "1");
  assert.equal($("sheet-numfmt-sample").textContent, "25.6%");
  assert.equal(h.document.querySelector('[data-numfmt-field="decimals"]').hidden, false);
  assert.equal(h.document.querySelector('[data-numfmt-field="symbol"]').hidden, true);
  $("sheet-numfmt-decimals").value = "3";
  $("sheet-numfmt-decimals").dispatchEvent({ type: "change", target: $("sheet-numfmt-decimals") });
  await flushAsyncTurns(6);
  assert.equal($("sheet-numfmt-code").value, "0.000%");
  assert.equal($("sheet-numfmt-sample").textContent, "25.600%");
  await click(h, $("btn-sheet-numfmt-apply"));
  assert.equal(cell(h.document, "A1").textContent, "25.600%");
  assert.equal($("sheet-numfmt-dialog").open, false, "the dialog closes after 확인");
});

test("a code over the bounds is refused in the dialog and the cell keeps its format", async () => {
  const h = await createHarness();
  await openModel(h);
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  await click(h, action(h.document, "dialog"));
  const code = h.document.getElementById("sheet-numfmt-code");
  code.value = "0".repeat(300);
  code.dispatchEvent({ type: "input", target: code });
  await click(h, h.document.getElementById("btn-sheet-numfmt-apply"));
  assert.notEqual(h.document.getElementById("sheet-numfmt-status").textContent, "");
  assert.equal(h.document.getElementById("sheet-numfmt-dialog").open, true);
  assert.equal(cell(h.document, "A1").textContent, "1234.5");
});

test("formats are sent as a shared numFmts list and come back after open", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowDown", 2]]);
  await click(h, action(h.document, "number"));
  cell(h.document, "B1").focus();
  await flushAsyncTurns(2);
  await click(h, action(h.document, "percent"));
  h.userClick(studioItem(h.document, "studio-save"));
  await flushAsyncTurns(20);
  const sheet = h.converts.at(-1).sheets[0];
  assert.deepEqual(sheet.numFmts, ["0", "0.00%"], "one entry per distinct code");
  const byRef = Object.fromEntries(sheet.cells.map((c) => [c.ref, c]));
  assert.deepEqual(byRef.A1.style, { nf: 1 });
  assert.deepEqual(byRef.A3.style, { nf: 1 });
  assert.deepEqual(byRef.B1.style, { nf: 2 });
  // Opening the same payload shows the formats again.
  const reopened = await createHarness();
  await openModel(reopened, { sheets: [{ name: "표", numFmts: sheet.numFmts, cells: sheet.cells }] });
  assert.equal(cell(reopened.document, "A1").textContent, "1235");
  assert.equal(cell(reopened.document, "A1").dataset.raw, "1234.5");
});

test("an unformatted sheet sends no numFmts", async () => {
  const h = await createHarness();
  await openModel(h);
  h.userClick(studioItem(h.document, "studio-save"));
  await flushAsyncTurns(20);
  assert.equal(h.converts.at(-1).sheets[0].numFmts, undefined);
});

test("a cell is asked of the engine once per code and value", async () => {
  const h = await createHarness();
  await openModel(h);
  await selectRange(h.document, "A1", [["ArrowDown", 2]]);
  await click(h, action(h.document, "number"));
  const asked = () => h.formats.flatMap((body) => body.items.flatMap((item) => item.values.map((v) => item.code + "|" + v)));
  const before = asked().length;
  h.document.defaultView.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__.shown("0", "1234.5");
  await flushAsyncTurns(4);
  assert.equal(asked().length, before, "a cached value is not asked again");
  assert.equal(new Set(asked()).size, asked().length, "no value is sent twice");
});

test("the number format items belong to the sheet window only", async () => {
  const h = await createHarness();
  const items = [...h.document.querySelectorAll("[data-sheet-numfmt]")];
  assert.ok(items.length >= 15, "menu items and toolbar buttons exist");
  for (const el of items) assert.equal(Boolean(el.closest("[data-kinds]")?.hidden ?? false), false, "visible in the sheet window");
  switchModule(h.document, "DOC");
  await flushAsyncTurns();
  assert.equal(h.document.querySelector('.tb-group[data-kinds="SHEET"]').hidden, true);
  assert.equal(h.document.querySelector('.md-sub[data-kinds="SHEET"]').hidden, true);
});

test("the display cache never asks for a value that is already on its way", async () => {
  const env = { __EASYOFFICE_SHEET_UTILS__: { normalizeFormattedNumericCell: (raw) => String(raw).trim().replaceAll(",", "") } };
  const D = loadUiModule(UI("sheet-numfmt-display.js"), env).__EASYOFFICE_SHEET_NUMFMT_DISPLAY__;
  const requests = [];
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  let filled = 0;
  D.attach({
    call: async (body) => {
      requests.push(body);
      await gate;
      return { items: body.items.map((item) => ({ t: item.values.map((v) => "<" + v + ">"), c: {} })) };
    },
    onFilled: () => { filled += 1; },
  });
  assert.equal(D.shown("0.00", "1,234"), null, "the first ask has no answer yet");
  assert.equal(D.shown("0.00", "1,234"), null);
  assert.equal(D.shown("0.00", "abc"), null, "text under a number code is General without asking");
  for (let i = 0; i < 4; i += 1) await Promise.resolve();
  assert.equal(requests.length, 1, "the first request is on its way");
  const again = D.preview("0.00", ["1,234", "7"]);
  assert.equal(D.shown("@", "00123").t, "00123", "the text format shows the stored text and asks nothing");
  release();
  assert.deepEqual((await again).map((x) => x.t), ["<1234>", "<7>"]);
  assert.equal(requests.length, 2, "one request for the first ask, one for the value not yet on its way");
  assert.deepEqual(requests.map((r) => r.items[0].values), [["1234"], ["7"]], "values go out normalized, once");
  assert.equal(D.shown("0.00", "1,234").t, "<1234>");
  assert.ok(filled >= 1);
  assert.equal(requests.length, 2, "an answered value is served from the cache");
});
