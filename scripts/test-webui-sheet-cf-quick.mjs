import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The conditional formatting submenu (PD-282): the quick dialogs of the cell highlight and top/bottom rules with their live
// preview, the galleries, clearing rules, and the preview and scope of the rule manager.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const source = (name) => fs.readFileSync(ui(name), "utf8");

const env = {};
loadUiModule(ui("sheet-look.js"), env);
loadUiModule(ui("sheet-cf-presets.js"), env);
loadUiModule(ui("sheet-cf-preview.js"), env);
const P = env.__EASYOFFICE_SHEET_CF_PRESETS__;
const PV = env.__EASYOFFICE_SHEET_CF_PREVIEW__;

const box = (text) => {
  const [a, b = a] = text.split(":");
  const at = (ref) => ({ row: Number(ref.slice(1)) - 1, col: ref.charCodeAt(0) - 65 });
  const p = at(a);
  const q = at(b);
  return { row0: p.row, col0: p.col, row1: q.row, col1: q.col };
};
const rangesOf = (text) => String(text).split(" ").map(box);
const textOf = (boxes) => boxes.map((b) => `${String.fromCharCode(65 + b.col0)}${b.row0 + 1}:${String.fromCharCode(65 + b.col1)}${b.row1 + 1}`).join(" ");

// ---- presets and rule builders ------------------------------------------------------------------------------------

test("a quick rule is the rule body of its kind with the colors of the chosen format", () => {
  const greater = P.quickRule("greater", { a: " 10 " }, P.FORMATS[0], "B2:C5").rule;
  assert.deepEqual(greater, { r: "B2:C5", t: "cellIs", op: "greaterThan", a: "10", b: "", stop: false, fl: 0, bg: "#ffc7ce", fg: "#9c0006" });
  const between = P.quickRule("between", { a: "1", b: "9" }, P.FORMATS[1], "A1").rule;
  assert.deepEqual([between.op, between.a, between.b, between.bg, between.fg], ["between", "1", "9", "#ffeb9c", "#9c5700"]);
  assert.deepEqual(P.quickRule("text", { text: "abc" }, P.FORMATS[3], "A1").rule, { r: "A1", t: "text", op: "contains", text: "abc", stop: false, fl: 0, bg: "#ffc7ce" });
  const redText = P.quickRule("date", { period: "last7Days" }, P.FORMATS[4], "A1").rule;
  assert.deepEqual([redText.t, redText.period, redText.bg, redText.fg], ["period", "last7Days", undefined, "#9c0006"]);
  assert.equal(P.quickRule("dup", { unique: "unique" }, P.FORMATS[2], "A1").rule.unique, true);
  assert.equal(P.quickRule("dup", { unique: "dup" }, P.FORMATS[2], "A1").rule.unique, false);
  const bottom = P.quickRule("bottom-percent", { n: "15" }, P.FORMATS[0], "A1").rule;
  assert.deepEqual([bottom.t, bottom.n, bottom.percent, bottom.bottom], ["top", 15, true, true]);
  assert.deepEqual(P.quickRule("below", {}, P.FORMATS[0], "A1").rule, { r: "A1", t: "average", above: false, equal: false, std: 0, stop: false, fl: 0, bg: "#ffc7ce", fg: "#9c0006" });
  const custom = P.quickRule("equal", { a: "x" }, P.CUSTOM_FORMAT, "A1").rule;
  assert.equal(custom.bg, "#ffc7ce", "a custom format starts from the first format's colors");
});

test("a quick rule is refused while a value is missing or out of range", () => {
  for (const [kind, values] of [
    ["greater", { a: "" }], ["between", { a: "1", b: " " }], ["text", { text: "" }], ["top", { n: "0" }], ["top", { n: "1001" }],
    ["top", { n: "2.5" }], ["bottom-percent", { n: "101" }], ["top-percent", { n: "abc" }],
  ]) {
    assert.match(P.quickRule(kind, values, P.FORMATS[0], "A1").error, /입력하세요/, `${kind} ${JSON.stringify(values)}`);
  }
  assert.ok(P.quickRule("top", { n: "1000" }, P.FORMATS[0], "A1").rule);
  assert.ok(P.quickRule("top-percent", { n: "100" }, P.FORMATS[0], "A1").rule);
  assert.match(P.quickRule("nothing", {}, P.FORMATS[0], "A1").error, /알 수 없는/);
});

test("the galleries hold the presets of the spreadsheet program: 12 bars, 12 color scales, 20 icon sets in four groups", () => {
  assert.equal(P.BARS.length, 12);
  assert.deepEqual(P.BARS.map((b) => b.id).slice(0, 2), ["grad-blue", "grad-green"]);
  assert.equal(P.BARS.filter((b) => b.gradient).length, 6);
  assert.deepEqual(P.barRule("A1:A9", P.BARS[9]), { r: "A1:A9", t: "bar", min: { k: "min", v: "" }, max: { k: "max", v: "" }, pos: "#ffb628", neg: "#ff0000", grad: false, show: true, stop: false });

  assert.equal(P.SCALES.length, 12);
  assert.equal(P.SCALES.filter((s) => s.colors.length === 3).length, 6);
  const gyr = P.scaleRule("A1", P.SCALES[0]);
  assert.deepEqual(gyr.stops.map((s) => [s.k, s.v, s.c]), [["min", "", "#f8696b"], ["percentile", "50", "#ffeb84"], ["max", "", "#63be7b"]], "green is the highest, red the lowest");
  const wr = P.scaleRule("A1", P.SCALES.find((s) => s.id === "wr"));
  assert.deepEqual(wr.stops.map((s) => [s.k, s.c]), [["min", "#f8696b"], ["max", "#fcfcff"]], "a two color scale has no middle");

  assert.deepEqual(P.ICON_GROUPS.map((g) => [g.label, g.sets.length]), [["방향", 7], ["도형", 5], ["표시기", 3], ["등급", 5]]);
  assert.deepEqual(P.iconRule("A1", "3Arrows").th.map((t) => t.v), ["0", "33", "67"]);
  assert.deepEqual(P.iconRule("A1", "4Rating").th.map((t) => t.v), ["0", "25", "50", "75"]);
  assert.deepEqual(P.iconRule("A1", "5Boxes").th.map((t) => t.v), ["0", "20", "40", "60", "80"]);
  assert.equal(P.iconRule("A1", "3Flags").th.every((t) => t.k === "percent"), true);
});

test("clearing cells cuts them out of each rule that meets them and drops a rule with nothing left", () => {
  const byName = (a, b) => a.localeCompare(b);
  assert.deepEqual(P.subtract(box("A1:C3"), box("B2")).map((b) => textOf([b])).sort(byName), ["A1:C1", "A2:A2", "A3:C3", "C2:C2"].sort(byName));
  assert.deepEqual(P.subtract(box("A1:C3"), box("D1:D2")), [box("A1:C3")], "a cut that misses leaves the box");
  assert.deepEqual(P.subtract(box("A1:C3"), box("A1:C3")), []);
  assert.deepEqual(P.subtractAll([box("A1:A5")], [box("A2"), box("A4")]).map((b) => textOf([b])), ["A1:A1", "A3:A3", "A5:A5"]);

  const parse = (text) => rangesOf(text);
  const cf = [
    { r: "A1:C3", t: "dup" },
    { r: "B2", t: "top" },
    { r: "E1:E4", t: "bar" },
    { r: "A1:A9 D1", t: "scale" },
  ];
  const kept = P.clearRange(cf, [box("B2")], parse, textOf);
  assert.deepEqual(kept.map((r) => r.t), ["dup", "bar", "scale"], "a rule only on the cleared cell is gone");
  assert.equal(kept[0].r.split(" ").length, 4);
  assert.equal(kept[1], cf[2], "a rule the cut misses is the same object");
  assert.deepEqual(P.clearRange(cf, [box("A1:E9")], parse, textOf), [], "a cut over everything leaves no rule");
});

test("the manager shows the rules of the sheet, or those that meet the selection", () => {
  const cf = [{ r: "A1:A3" }, { r: "C5" }, { r: "A9 B2:B3" }];
  assert.deepEqual(P.rulesInScope(cf, "sheet", [box("Z1")], rangesOf), [0, 1, 2]);
  assert.deepEqual(P.rulesInScope(cf, "selection", [box("A2:B2")], rangesOf), [0, 2]);
  assert.deepEqual(P.rulesInScope(cf, "selection", [box("D1")], rangesOf), []);
});

// ---- preview -------------------------------------------------------------------------------------------------------

const ICON_SETS = { "3Arrows": [["down", "#d93025"], ["right", "#f9ab00"], ["up", "#1e8e3e"]] };

test("a rule's preview is described without looking at the cells", () => {
  assert.deepEqual(PV.previewOf({ t: "bar", pos: "#638ec6", neg: "#ff0000", grad: true }, ICON_SETS), { kind: "bar", neg: "#ff0000", pos: "#638ec6", gradient: true });
  assert.deepEqual(PV.previewOf({ t: "bar", pos: "#638ec6", neg: "", grad: false }, ICON_SETS), { kind: "bar", neg: "", pos: "#638ec6", gradient: false });
  const scale = PV.previewOf({ t: "scale", stops: [{ c: "#000000" }, { c: "#ffffff" }] }, ICON_SETS);
  assert.deepEqual(scale.colors, ["#000000", "#404040", "#808080", "#bfbfbf", "#ffffff"], "five samples between the stops");
  const three = PV.previewOf({ t: "scale", stops: [{ c: "#ff0000" }, { c: "#00ff00" }, { c: "#0000ff" }] }, ICON_SETS);
  assert.deepEqual([three.colors[0], three.colors[2], three.colors[4]], ["#ff0000", "#00ff00", "#0000ff"]);
  assert.deepEqual(PV.previewOf({ t: "icons", set: "3Arrows" }, ICON_SETS).icons.map((i) => i[0]), ["up", "right", "down"], "highest first");
  assert.equal(PV.previewOf({ t: "icons", set: "none" }, ICON_SETS), null);
  assert.deepEqual(PV.previewOf({ t: "cellIs", bg: "#ffc7ce", fg: "#9c0006", fl: 5 }, ICON_SETS), { kind: "style", text: "123", bg: "#ffc7ce", fg: "#9c0006", flags: 5 });
  assert.equal(PV.previewOf(null, ICON_SETS), null);
  assert.equal(PV.previewOf({ t: "scale", stops: [{ c: "red" }] }, ICON_SETS), null, "a color that is not a hex code is not drawn");
});

// ---- the page ------------------------------------------------------------------------------------------------------

async function pageHarness({ cells = {}, selection = { r1: 2, c1: 2, r2: 5, c2: 3 }, sheetRules = null } = {}) {
  const harness = await createWorkspaceHarness();
  const win = harness.document.defaultView;
  const rules = win.__EASYOFFICE_SHEET_RULES__;
  rules.replaceAll(sheetRules || {});
  const pushed = [];
  const opened = [];
  const stored = { Sheet1: cells };
  const state = { selection };
  const rulesUi = win.__EASYOFFICE_CREATE_SHEET_RULES_UI__({
    document: harness.document,
    sheetDataStore: stored,
    sheetRowCounts: {},
    getActiveSheetName: () => "Sheet1",
    getRange: () => state.selection,
    isFormulaStr: (t) => String(t).startsWith("="),
    sheetStoreHelpers: { validateEntry: async () => ({ ok: true }), callSheetAction: async () => ({}) },
    enqueueWorkspaceHistoryOp: (fn) => fn(),
    drainInFlightSheetWork: async () => {},
    sheetHistory: { begin: () => ({ snapshot: true }), commit: (record) => pushed.push(record) },
    flushFocusedSheetCellBeforeSwitch: async () => {},
    performSwitchSheetTabAsync: async () => {},
    applyCellEdits: async () => true,
    readTdValue: (td) => td.textContent,
  });
  return { ...harness, win, rules, rulesUi, pushed, opened, state, doc: harness.document };
}

const field = (document, id, value) => {
  const el = document.getElementById(id);
  assert.ok(el, id);
  if (typeof value === "boolean") el.checked = value;
  else el.value = value;
  el.dispatchEvent({ type: "input", target: el, bubbles: true });
  el.dispatchEvent({ type: "change", target: el, bubbles: true });
  return el;
};
// Escape closes the dialog and the browser then fires `close` (the harness's `close()` does not).
function escape(document) {
  const dialog = document.getElementById("sheet-cfq-dialog");
  dialog.close();
  dialog.dispatchEvent({ type: "close", target: dialog });
}
const kinds = (rules) => rules.get("Sheet1").cf.map((r) => r.t);

test("the submenu has the items of the spreadsheet program's conditional formatting menu", async () => {
  const { doc } = await pageHarness();
  const labels = (selector) => Array.from(doc.querySelectorAll(selector), (el) => el.querySelector(".md-label").textContent);
  const panel = (label) => doc.querySelector(`.md-sub-panel[aria-label="${label}"]`);
  const direct = (el) => Array.from(el.querySelectorAll(":scope > .md-item, :scope > .md-sub"), (item) => item.querySelector(".md-label").textContent);
  assert.deepEqual(direct(panel("조건부 서식")), ["셀 강조 규칙", "상위/하위 규칙", "데이터 막대", "색조", "아이콘 집합", "새 규칙…", "규칙 지우기", "규칙 관리…"]);
  assert.deepEqual(direct(panel("셀 강조 규칙")), ["보다 큼…", "보다 작음…", "다음 값의 사이에 있음…", "같음…", "텍스트 포함…", "발생 날짜…", "중복 값…", "기타 규칙…"]);
  assert.deepEqual(direct(panel("상위/하위 규칙")), ["상위 10개 항목…", "상위 10%…", "하위 10개 항목…", "하위 10%…", "평균 초과…", "평균 미만…", "기타 규칙…"]);
  assert.deepEqual(direct(panel("규칙 지우기")), ["선택한 셀에서 규칙 지우기", "전체 시트에서 규칙 지우기", "이 표에서 규칙 지우기", "이 피벗 테이블에서 규칙 지우기"]);
  assert.equal(doc.querySelectorAll(".cfg-tile").length, 44);
  assert.deepEqual(Array.from(doc.querySelectorAll("#sheet-cf-gallery-bar .cfg-title"), (t) => t.textContent), ["그라데이션 채우기", "단색 채우기"]);
  assert.deepEqual(Array.from(doc.querySelectorAll("#sheet-cf-gallery-icons .cfg-title"), (t) => t.textContent), ["방향", "도형", "표시기", "등급"]);
  assert.equal(labels("#sheet-cf-gallery-scale .cfg-tile")[0], "녹색 - 노랑 - 빨강 색조");
  const tile = doc.querySelector('[data-sheet-cf="bar-grad-blue"]');
  assert.equal(tile.title, "파랑 그라데이션 데이터 막대");
  assert.equal(tile.querySelector(".cfp").dataset.kind, "bar");
  assert.equal(doc.querySelector('[data-sheet-cf="icons-3arrows"]').querySelectorAll(".cfp-icon").length, 3);
});

test("a quick dialog fills the value from the active cell and shows the rule on the sheet while its inputs change", async () => {
  const { rulesUi, rules, doc, pushed } = await pageHarness({ cells: { B2: { value: "42", formula: "" } } });
  await rulesUi.runCfAction("quick-greater");
  const dialog = doc.getElementById("sheet-cfq-dialog");
  assert.equal(dialog.open, true);
  assert.equal(doc.getElementById("sheet-cfq-title").textContent, "보다 큼");
  assert.equal(doc.getElementById("sheet-cfq-a").value, "42", "the value of the active cell");
  assert.equal(doc.getElementById("btn-sheet-cfq-ok").disabled, false);
  assert.deepEqual(kinds(rules), ["cellIs"], "the rule is on the sheet before it is accepted");
  assert.equal(rules.get("Sheet1").cf[0].r, "B2:C5");
  assert.equal(rules.get("Sheet1").cf[0].bg, "#ffc7ce");
  assert.equal(pushed.length, 0, "a preview is no undo step");

  field(doc, "sheet-cfq-format-select", "green-fill-dark-green");
  assert.deepEqual([rules.get("Sheet1").cf[0].bg, rules.get("Sheet1").cf[0].fg], ["#c6efce", "#006100"]);
  field(doc, "sheet-cfq-a", "");
  assert.deepEqual(kinds(rules), [], "an empty value takes the preview off");
  assert.equal(doc.getElementById("btn-sheet-cfq-ok").disabled, true);
  field(doc, "sheet-cfq-a", "7");
  assert.equal(rules.get("Sheet1").cf[0].a, "7");
  assert.equal(doc.getElementById("sheet-cfq-sample").dataset.kind, "style");
});

test("cancelling a quick dialog puts the rules back and leaves no undo step", async () => {
  const before = { cf: [{ r: "A1", t: "dup", bg: "#ffc7ce" }], dv: [{ r: "D1", t: "whole", op: "between", a: "1", b: "2" }] };
  const { rulesUi, rules, doc, pushed } = await pageHarness({ sheetRules: { Sheet1: before } });
  await rulesUi.runCfAction("quick-top");
  assert.deepEqual(kinds(rules), ["top", "dup"], "the preview goes before the rules that are there");
  assert.equal(doc.getElementById("sheet-cfq-n").value, "10");
  doc.querySelector("#sheet-cfq-dialog .dialog-footer [data-dialog-close]").click();
  assert.deepEqual(rules.get("Sheet1"), before);
  assert.equal(pushed.length, 0);
  assert.equal(doc.getElementById("sheet-cfq-dialog").open, false);

  await rulesUi.runCfAction("quick-below");
  assert.deepEqual(kinds(rules), ["average", "dup"]);
  escape(doc);
  assert.deepEqual(rules.get("Sheet1"), before, "closing the window another way (Escape) cancels as well");
  assert.equal(pushed.length, 0);
});

test("accepting a quick dialog is one undo step that keeps the rule first in the list", async () => {
  const { rulesUi, rules, doc, pushed } = await pageHarness({ sheetRules: { Sheet1: { cf: [{ r: "A1", t: "dup", bg: "#ffc7ce" }], dv: [] } } });
  await rulesUi.runCfAction("quick-between");
  assert.equal(doc.getElementById("btn-sheet-cfq-ok").disabled, true, "both values are needed");
  field(doc, "sheet-cfq-a", "1");
  assert.equal(doc.getElementById("btn-sheet-cfq-ok").disabled, true);
  field(doc, "sheet-cfq-b", "9");
  assert.equal(doc.getElementById("btn-sheet-cfq-ok").disabled, false);
  let changes = 0;
  rules.on("changed", () => (changes += 1));
  doc.getElementById("btn-sheet-cfq-ok").click();
  await flushAsyncTurns();
  assert.equal(changes, 0, "the rules the preview put in are not set again (the session would not send them twice and the sheet would lose its look)");
  assert.equal(pushed.length, 1);
  assert.deepEqual(kinds(rules), ["cellIs", "dup"]);
  assert.deepEqual(rules.get("Sheet1").cf[0].op, "between");
  assert.equal(doc.getElementById("sheet-cfq-dialog").open, false);
  assert.equal(JSON.parse(rules.payloadFor("Sheet1")).cf[0].p, 1, "the new rule has the highest priority");
});

test("each dialog shows the inputs of its kind, with the text, period, duplicate and rank rules", async () => {
  const { rulesUi, rules, doc } = await pageHarness({ cells: { B2: { value: "word", formula: "" } } });
  const shown = () => Array.from(doc.querySelectorAll("#sheet-cfq-dialog [data-cfq-row]")).filter((row) => !row.hidden).map((row) => row.dataset.cfqRow);
  const leads = () => Array.from(doc.querySelectorAll('#sheet-cfq-dialog [data-cfq-row]:not([hidden]) [data-cfq-lead]'), (el) => el.textContent);
  const cancel = () => escape(doc);
  await rulesUi.runCfAction("quick-text");
  assert.deepEqual(shown(), ["text"]);
  assert.deepEqual(leads(), ["다음 텍스트를 포함하는 셀"]);
  assert.equal(doc.getElementById("sheet-cfq-text").value, "word");
  field(doc, "sheet-cfq-text", "ab");
  assert.deepEqual([rules.get("Sheet1").cf[0].t, rules.get("Sheet1").cf[0].op, rules.get("Sheet1").cf[0].text], ["text", "contains", "ab"]);
  cancel();
  await rulesUi.runCfAction("quick-date");
  assert.deepEqual(shown(), ["period"]);
  assert.deepEqual(Array.from(doc.getElementById("sheet-cfq-period-select").querySelectorAll("option"), (o) => o.textContent).slice(0, 4), ["어제", "오늘", "내일", "지난 7일"]);
  field(doc, "sheet-cfq-period-select", "nextMonth");
  assert.equal(rules.get("Sheet1").cf[0].period, "nextMonth");
  cancel();
  await rulesUi.runCfAction("quick-dup");
  field(doc, "sheet-cfq-dup-select", "unique");
  assert.equal(rules.get("Sheet1").cf[0].unique, true);
  cancel();
  await rulesUi.runCfAction("quick-bottom-percent");
  assert.deepEqual(shown(), ["n"]);
  assert.equal(doc.getElementById("sheet-cfq-unit").textContent, "%");
  assert.equal(doc.getElementById("btn-sheet-cfq-ok").disabled, false);
  field(doc, "sheet-cfq-n", "101");
  assert.equal(doc.getElementById("btn-sheet-cfq-ok").disabled, true);
  field(doc, "sheet-cfq-n", "20");
  assert.deepEqual([rules.get("Sheet1").cf[0].n, rules.get("Sheet1").cf[0].percent, rules.get("Sheet1").cf[0].bottom], [20, true, true]);
  cancel();
  await rulesUi.runCfAction("quick-above");
  assert.deepEqual(shown(), ["none"]);
  assert.equal(doc.getElementById("btn-sheet-cfq-ok").disabled, false);
  assert.deepEqual([rules.get("Sheet1").cf[0].t, rules.get("Sheet1").cf[0].above], ["average", true], "a rule without an input shows at once");
  cancel();
  assert.deepEqual(kinds(rules), []);
});

test("the custom format opens the rule manager on the rule, and the preview is taken off", async () => {
  const { rulesUi, rules, doc, pushed } = await pageHarness();
  await rulesUi.runCfAction("quick-equal");
  field(doc, "sheet-cfq-a", "5");
  field(doc, "sheet-cfq-format-select", "custom");
  assert.deepEqual(kinds(rules), [], "a custom format has no preview");
  doc.getElementById("btn-sheet-cfq-ok").click();
  await flushAsyncTurns();
  assert.equal(doc.getElementById("sheet-cfq-dialog").open, false);
  assert.equal(doc.getElementById("sheet-cf-dialog").open, true);
  assert.equal(doc.getElementById("sheet-cf-type-select").value, "cellIs");
  assert.equal(doc.getElementById("sheet-cf-cmp-select").value, "equal");
  assert.equal(doc.getElementById("sheet-cf-a").value, "5");
  assert.equal(doc.getElementById("sheet-cf-range").value, "B2:C5");
  assert.deepEqual(kinds(rules), [], "nothing is stored until 적용");
  assert.equal(pushed.length, 0);
});

test("a gallery item adds its rule at once as one undo step, and the same item twice adds two rules", async () => {
  const { rulesUi, rules, pushed } = await pageHarness();
  await rulesUi.runCfAction("bar-solid-green");
  await flushAsyncTurns();
  assert.equal(pushed.length, 1);
  const [bar] = rules.get("Sheet1").cf;
  assert.deepEqual([bar.t, bar.r, bar.pos, bar.grad, bar.show], ["bar", "B2:C5", "#63c384", false, true]);
  await rulesUi.runCfAction("scale-bwr");
  await rulesUi.runCfAction("icons-5quarters");
  await rulesUi.runCfAction("icons-5quarters");
  await flushAsyncTurns();
  assert.deepEqual(kinds(rules), ["icons", "icons", "scale", "bar"]);
  assert.equal(pushed.length, 4);
  assert.equal(rules.get("Sheet1").cf[0].th.length, 5);
});

test("clearing rules from the selection cuts them out, from the sheet drops them all, and neither makes a step when nothing changes", async () => {
  const { rulesUi, rules, pushed, state } = await pageHarness({ sheetRules: { Sheet1: { cf: [{ r: "A1:C9", t: "dup", bg: "#ffc7ce" }, { r: "E1", t: "top", bg: "#ffc7ce" }, { r: "B2", t: "bar" }], dv: [{ r: "B2", t: "whole", op: "between", a: "1", b: "2" }] } } });
  await rulesUi.runCfAction("clear-selection");
  await flushAsyncTurns();
  assert.equal(pushed.length, 1);
  assert.deepEqual(rules.get("Sheet1").cf.map((r) => r.t), ["dup", "top"], "the rule that lay only on the selection is gone");
  assert.equal(rules.get("Sheet1").cf[0].r.includes("B2"), false);
  assert.equal(rules.get("Sheet1").dv.length, 1, "validation is not conditional formatting");
  state.selection = { r1: 20, c1: 20, r2: 20, c2: 20 };
  await rulesUi.runCfAction("clear-selection");
  assert.equal(pushed.length, 1, "no rule meets the selection");
  await rulesUi.runCfAction("clear-sheet");
  await flushAsyncTurns();
  assert.deepEqual(rules.get("Sheet1").cf, []);
  assert.equal(pushed.length, 2);
  await rulesUi.runCfAction("clear-sheet");
  assert.equal(pushed.length, 2, "an empty sheet has nothing to clear");
});

test("the 기타 규칙 items open the manager on a new rule of their kind", async () => {
  const { rulesUi, doc } = await pageHarness();
  for (const [action, type] of [["more-highlight", "cellIs"], ["more-top", "top"], ["more-bar", "bar"], ["more-scale", "scale"], ["more-icons", "icons"]]) {
    await rulesUi.runCfAction(action);
    assert.equal(doc.getElementById("sheet-cf-dialog").open, true, action);
    assert.equal(doc.getElementById("sheet-cf-type-select").value, type, action);
    assert.equal(doc.getElementById("sheet-cf-range").value, "B2:C5", action);
    doc.getElementById("sheet-cf-dialog").close();
  }
  await rulesUi.runCfAction("new");
  assert.equal(doc.getElementById("sheet-cf-type-select").value, "cellIs");
  doc.getElementById("sheet-cf-dialog").close();
  await rulesUi.runCfAction("manage");
  assert.equal(doc.getElementById("sheet-cf-dialog").open, true);
});

test("the manager lists the rules for the selection or for the sheet, each with a preview of its look", async () => {
  const cf = [
    { r: "B2:B3", t: "cellIs", op: "greaterThan", a: "1", b: "", bg: "#ffc7ce", fg: "#9c0006", fl: 1 },
    { r: "H9", t: "bar", min: { k: "min", v: "" }, max: { k: "max", v: "" }, pos: "#638ec6", neg: "#ff0000", grad: true, show: true },
    { r: "C4", t: "icons", set: "3Arrows", th: [], rev: false, show: true },
    { r: "B5", t: "scale", stops: [{ k: "min", v: "", c: "#f8696b" }, { k: "max", v: "", c: "#63be7b" }] },
  ];
  const { rulesUi, rules, doc } = await pageHarness({ sheetRules: { Sheet1: { cf, dv: [] } } });
  const rows = () => Array.from(doc.querySelectorAll("#sheet-cf-list li"));
  rulesUi.openConditionalFormatDialog();
  assert.equal(doc.getElementById("sheet-cf-scope-select").value, "selection");
  assert.deepEqual(rows().map((li) => li.dataset.index), ["0", "2", "3"], "H9 is outside the selection B2:C5");
  assert.deepEqual(rows().map((li) => li.querySelector(".cfp").dataset.kind), ["style", "icons", "scale"]);
  assert.equal(rows()[0].querySelector(".cfp-bold") !== null, true, "the font of the rule shows in the preview");
  assert.equal(rows()[1].querySelectorAll(".cfp-icon").length, 3);
  assert.equal(rows()[2].querySelectorAll(".cfp-seg").length, 5);
  assert.match(rows()[1].querySelector(".cf-row-text").textContent, /^3\. C4/);

  field(doc, "sheet-cf-scope-select", "sheet");
  assert.deepEqual(rows().map((li) => li.dataset.index), ["0", "1", "2", "3"]);
  assert.equal(rows()[1].querySelector(".cfp").dataset.kind, "bar");

  field(doc, "sheet-cf-scope-select", "selection");
  rows()[1].click();
  assert.equal(doc.getElementById("sheet-cf-icon-set-select").value, "3Arrows");
  doc.getElementById("btn-sheet-cf-down").click();
  await flushAsyncTurns();
  assert.deepEqual(rules.get("Sheet1").cf.map((r) => r.r), ["B2:B3", "H9", "B5", "C4"], "a move passes the neighbour that is shown");
  field(doc, "sheet-cf-scope-select", "sheet");
  rows()[3].click();
  field(doc, "sheet-cf-scope-select", "selection");
  assert.equal(rows().filter((li) => li.getAttribute("aria-selected") === "true").length, 1);
  rules.replaceAll({});
});

test("a rule applied outside what the list shows switches the list to the whole sheet", async () => {
  const { rulesUi, rules, doc } = await pageHarness();
  rulesUi.openConditionalFormatDialog();
  field(doc, "sheet-cf-range", "H1:H9");
  field(doc, "sheet-cf-a", "3");
  doc.getElementById("btn-sheet-cf-apply").click();
  await flushAsyncTurns();
  assert.equal(doc.getElementById("sheet-cf-scope-select").value, "sheet");
  assert.equal(doc.querySelectorAll("#sheet-cf-list li").length, 1);
  assert.equal(rules.get("Sheet1").cf[0].r, "H1:H9");
});

// ---- the border of a format (PD-284) --------------------------------------------------------------------------------------

const RED_EDGE = "thin:ff0000";
const RED_OUTLINE = { t: RED_EDGE, r: RED_EDGE, b: RED_EDGE, l: RED_EDGE };
const redBorder = () => P.FORMATS.find((f) => f.id === "red-border");

test("the format list ends with the red border, a thin red outline that sets nothing else", () => {
  assert.deepEqual(P.FORMATS.map((f) => f.label), [
    "진한 빨강 텍스트가 있는 연한 빨강 채우기", "진한 노랑 텍스트가 있는 노랑 채우기", "진한 녹색 텍스트가 있는 녹색 채우기", "연한 빨강 채우기", "빨강 텍스트", "빨강 테두리",
  ]);
  const rule = P.quickRule("greater", { a: "1" }, redBorder(), "A1:B2").rule;
  assert.deepEqual(rule, { r: "A1:B2", t: "cellIs", op: "greaterThan", a: "1", b: "", stop: false, fl: 0, bd: RED_OUTLINE });
  rule.bd.t = "double:000000";
  assert.equal(redBorder().bd.t, RED_EDGE, "a rule does not share the format's edges");
  assert.equal(P.quickRule("dup", { unique: "dup" }, P.FORMATS[0], "A1").rule.bd, undefined, "the other formats draw no edge");
});

test("the border row of the custom format: inputs make edges, edges fill inputs, and nothing ticked is no border", () => {
  const sides = ["t", "r", "b", "l"];
  assert.deepEqual(P.BORDER_LINES, ["thin", "dashed", "dotted"], "the thin family, as the spreadsheet program's dialog offers");
  assert.deepEqual(P.borderFromControls({ on: true, line: "dashed", color: "#0000ff", sides }), { t: "dashed:0000ff", r: "dashed:0000ff", b: "dashed:0000ff", l: "dashed:0000ff" });
  assert.equal(P.borderFromControls({ on: true, line: "double", color: "#0000ff", sides }), null, "no thick or double line for a conditional format");
  assert.equal(P.borderFromControls({ on: true, line: "thick", color: "#0000ff", sides }), null);
  assert.deepEqual(P.borderFromControls({ on: true, line: "dotted", color: "#112233", sides: ["l", "t"] }), { t: "dotted:112233", l: "dotted:112233" });
  assert.equal(P.borderFromControls({ on: false, line: "thin", color: "#000000", sides }), null);
  assert.equal(P.borderFromControls({ on: true, line: "thin", color: "#000000", sides: [] }), null);
  assert.equal(P.borderFromControls({ on: true, line: "wavy", color: "#000000", sides }), null);
  assert.equal(P.borderFromControls({ on: true, line: "thin", color: "red", sides }), null);
  assert.deepEqual(P.controlsFromBorder({ r: "dashed:00ff00", b: "thin:ff0000" }), { on: true, line: "dashed", color: "#00ff00", sides: ["r", "b"] });
  assert.equal(P.controlsFromBorder({ r: "thick:00ff00" }).line, "thin", "a line the row does not offer shows as thin");
  assert.equal(P.controlsFromBorder({ r: "double:00ff00" }).line, "thin");
  assert.deepEqual(P.controlsFromBorder(undefined), { on: false, line: "thin", color: "#000000", sides });
  assert.deepEqual(P.controlsFromBorder({ t: "nonsense" }).on, false, "an edge that is not one is no edge");
});

test("a rule's preview draws its edges as custom properties of the sample", async () => {
  assert.deepEqual(PV.previewOf({ t: "cellIs", bd: RED_OUTLINE }, ICON_SETS), { kind: "style", text: "123", bg: "", fg: "", flags: 0, bd: RED_OUTLINE });
  const { doc, win } = await pageHarness();
  const el = doc.createElement("div");
  win.__EASYOFFICE_SHEET_CF_PREVIEW__.paint(el, { t: "cellIs", bd: { t: RED_EDGE, b: "double:000000" } }, ICON_SETS, () => "");
  const sample = el.querySelector(".cfp-text");
  assert.equal(sample.style.getPropertyValue("--cfp-bt"), "1px solid #ff0000");
  assert.equal(sample.style.getPropertyValue("--cfp-bb"), "3px double var(--color-text)", "black is the theme's text color");
  assert.equal(sample.style.getPropertyValue("--cfp-br"), "");
});

test("choosing the red border in a quick dialog shows it on the sheet and accepting it is one undo step", async () => {
  const { rulesUi, rules, doc, pushed } = await pageHarness();
  await rulesUi.runCfAction("quick-greater");
  const options = Array.from(doc.getElementById("sheet-cfq-format-select").querySelectorAll("option"), (o) => o.textContent);
  assert.deepEqual(options.slice(-2), ["빨강 테두리", "사용자 지정 형식…"]);
  field(doc, "sheet-cfq-a", "3");
  field(doc, "sheet-cfq-format-select", "red-border");
  const [rule] = rules.get("Sheet1").cf;
  assert.deepEqual([rule.bd, rule.bg, rule.fg], [RED_OUTLINE, undefined, undefined], "the preview is on the sheet");
  assert.equal(doc.getElementById("sheet-cfq-sample").querySelector(".cfp-text").style.getPropertyValue("--cfp-bl"), "1px solid #ff0000");
  assert.equal(pushed.length, 0);
  doc.getElementById("btn-sheet-cfq-ok").click();
  await flushAsyncTurns();
  assert.equal(pushed.length, 1);
  assert.deepEqual(rules.get("Sheet1").cf[0].bd, RED_OUTLINE);
  assert.match(rules.payloadFor("Sheet1"), /"bd":\{"t":"thin:ff0000","r":"thin:ff0000","b":"thin:ff0000","l":"thin:ff0000"\}/, "the session is told the edges");
  rules.replaceAll({});
});

const mixed = { t: "double:0000ff", l: "dotted:00ff00" };
const sideBoxes = (doc) => ["t", "r", "b", "l"].map((s) => doc.getElementById("sheet-cf-bd-" + s).checked);

test("the manager's border row shows a rule's edges, keeps edges it cannot show and writes the ones it makes", async () => {
  const cf = [{ r: "B2:B3", t: "cellIs", op: "greaterThan", a: "1", b: "", bg: "#ffc7ce", fl: 0, bd: mixed }, { r: "C4", t: "dup", fl: 0, bg: "#ffc7ce" }];
  const { rulesUi, rules, doc } = await pageHarness({ sheetRules: { Sheet1: { cf, dv: [] } } });
  const apply = async () => {
    doc.getElementById("btn-sheet-cf-apply").click();
    await flushAsyncTurns();
  };
  rulesUi.openConditionalFormatDialog();
  doc.querySelector('#sheet-cf-list li[data-index="0"]').click();
  assert.equal(doc.getElementById("sheet-cf-bd-on").checked, true);
  assert.equal(doc.getElementById("sheet-cf-bd-line").value, "thin", "a double line is not one the row offers: it shows as thin");
  assert.equal(doc.getElementById("sheet-cf-bd").value, "#0000ff", "the color of the first edge");
  assert.deepEqual(sideBoxes(doc), [true, false, false, true]);
  assert.deepEqual(Array.from(doc.getElementById("sheet-cf-bd-line").querySelectorAll("option"), (o) => o.value), ["thin", "dashed", "dotted"]);

  await apply();
  assert.deepEqual(rules.get("Sheet1").cf[0].bd, mixed, "an untouched row keeps the rule's own edges, one line kind per side included");

  field(doc, "sheet-cf-bd-line", "dashed");
  await apply();
  assert.deepEqual(rules.get("Sheet1").cf[0].bd, { t: "dashed:0000ff", l: "dashed:0000ff" }, "a changed row writes its inputs on the ticked sides");

  field(doc, "sheet-cf-bd-r", true);
  field(doc, "sheet-cf-bd", "#ff0000");
  await apply();
  assert.deepEqual(rules.get("Sheet1").cf[0].bd, { t: "dashed:ff0000", r: "dashed:ff0000", l: "dashed:ff0000" });

  for (const side of ["t", "r", "b", "l"]) field(doc, "sheet-cf-bd-" + side, false);
  await apply();
  assert.match(doc.getElementById("sheet-cf-status").textContent, /변을 하나 이상/);
  assert.equal(rules.get("Sheet1").cf[0].bd.t, "dashed:ff0000", "a refused form stores nothing");

  field(doc, "sheet-cf-bd-on", false);
  await apply();
  assert.equal(rules.get("Sheet1").cf[0].bd, undefined, "the row off draws none");
  assert.equal(rules.get("Sheet1").cf[0].bg, "#ffc7ce");

  // A border alone is a format; no format at all is not.
  doc.querySelector('#sheet-cf-list li[data-index="1"]').click();
  assert.equal(doc.getElementById("sheet-cf-bd-on").checked, false);
  assert.deepEqual(sideBoxes(doc), [true, true, true, true], "turning the row on draws the whole outline");
  field(doc, "sheet-cf-bg-on", false);
  await apply();
  assert.match(doc.getElementById("sheet-cf-status").textContent, /테두리 중 하나/);
  field(doc, "sheet-cf-bd-on", true);
  await apply();
  assert.deepEqual(rules.get("Sheet1").cf[1].bd, { t: "thin:000000", r: "thin:000000", b: "thin:000000", l: "thin:000000" });
  assert.equal(rules.get("Sheet1").cf[1].bg, undefined);
  rules.replaceAll({});
});

test("no array method is called on a list the page returns (a NodeList or an HTMLCollection has none)", () => {
  for (const name of ["sheet-cf-presets.js", "sheet-cf-preview.js", "sheet-cf-quick-ui.js", "sheet-cf-bar-ui.js", "sheet-rules-ui.js"]) {
    const text = source(name);
    assert.doesNotMatch(text, /querySelectorAll\([^)]*\)\.(map|filter|find|some|every|reduce|flatMap|slice|indexOf|includes)\b/, `${name}: querySelectorAll`);
    assert.doesNotMatch(text, /\.(childNodes|children)\.(map|filter|find|some|every|reduce|flatMap|slice|indexOf|includes|at)\b/, `${name}: childNodes or children`);
  }
});

test("clear-table and clear-pivot enable only inside their ranges and cut only that table or pivot in one undo step", async () => {
  const { win, rulesUi, rules, doc, pushed, state } = await pageHarness({
    selection: { r1: 1, c1: 1, r2: 1, c2: 1 },
    sheetRules: {
      Sheet1: {
        cf: [{ r: "A1:E6", t: "dup", bg: "#ffc7ce" }],
        dv: [{ r: "B2", t: "whole", op: "between", a: "1", b: "2" }],
      },
    },
  });
  const layout = win.__EASYOFFICE_SHEET_LAYOUT__;
  layout.setTables("Sheet1", [{ name: "Table1", range: "B2:C4", columns: ["A", "B"] }]);
  layout.setPivots("Sheet1", [{ name: "PivotTable1", sourceSheet: "Sheet1", sourceRange: "A1:B2", location: "E1:F3", fieldNames: ["A", "B"], rowFields: [0] }]);
  const disabledOf = (act) => doc.querySelector(`.md-item[data-sheet-cf="${act}"]`).getAttribute("aria-disabled");

  // Selection on A1 is outside both Table1 and PivotTable1.
  rulesUi.syncClearMenu();
  assert.equal(disabledOf("clear-table"), "true");
  assert.equal(disabledOf("clear-pivot"), "true");
  await rulesUi.runCfAction("clear-table");
  await rulesUi.runCfAction("clear-pivot");
  assert.equal(pushed.length, 0);

  // Inside Table1 (C3 -> row 3, col 3).
  state.selection = { r1: 3, c1: 3, r2: 3, c2: 3 };
  rulesUi.syncClearMenu();
  assert.equal(disabledOf("clear-table"), "false");
  assert.equal(disabledOf("clear-pivot"), "true");
  await rulesUi.runCfAction("clear-table");
  await flushAsyncTurns();
  assert.equal(pushed.length, 1);
  const afterTable = rules.parseRanges(rules.get("Sheet1").cf[0].r);
  assert.ok(!afterTable.some((b) => b.row0 <= 2 && b.row1 >= 2 && b.col0 <= 2 && b.col1 >= 2), "C3 is cut out");
  assert.ok(afterTable.some((b) => b.row0 <= 0 && b.col0 <= 0), "A1 outside Table1 stays");
  assert.ok(afterTable.some((b) => b.col1 >= 4), "E1:E6 in pivot stays");
  assert.equal(rules.get("Sheet1").dv.length, 1);

  // Inside PivotTable1 (E2 -> row 2, col 5).
  state.selection = { r1: 2, c1: 5, r2: 2, c2: 5 };
  rulesUi.syncClearMenu();
  assert.equal(disabledOf("clear-table"), "true");
  assert.equal(disabledOf("clear-pivot"), "false");
  await rulesUi.runCfAction("clear-pivot");
  await flushAsyncTurns();
  assert.equal(pushed.length, 2);
  const afterPivot = rules.parseRanges(rules.get("Sheet1").cf[0].r);
  assert.ok(!afterPivot.some((b) => b.col1 >= 4 && b.row0 <= 2), "E1:E3 in PivotTable1 is cut out");
  assert.ok(afterPivot.some((b) => b.row0 === 0 && b.col0 === 0), "A1 outside both stays");
  // Repeating clear-pivot when nothing intersects makes no undo step.
  await rulesUi.runCfAction("clear-pivot");
  assert.equal(pushed.length, 2);

  // Reused preview nodes must not retain stale data-rtl when repainted with non-RTL / non-bar rules.
  const swatch = doc.createElement("span");
  win.__EASYOFFICE_SHEET_CF_PREVIEW__.paint(
    swatch,
    { r: "A1", t: "bar", pos: "#638ec6", neg: "#ff555a", dir: "rightToLeft", min: { k: "min" }, max: { k: "max" } },
    [],
  );
  assert.equal(swatch.dataset.rtl, "true");
  win.__EASYOFFICE_SHEET_CF_PREVIEW__.paint(swatch, { r: "A1", t: "dup", bg: "#ffc7ce" }, []);
  assert.equal(swatch.dataset.rtl, undefined);

  layout.replaceAll({});
  rules.replaceAll({});
});

// ---- PD-351: gallery pointer & keyboard live preview --------------------------------------------------------

function packTestCf(entries) {
  const palette = [];
  const colorId = (rgb) => {
    let i = palette.indexOf(rgb);
    if (i < 0) i = palette.push(rgb) - 1;
    return i;
  };
  const words = [];
  for (const e of entries) {
    let head = e.col | ((e.flags || 0) << 14);
    if (e.bg !== undefined) head |= 1 << 18;
    if (e.fg !== undefined) head |= 1 << 19;
    if (e.bar) head |= 1 << 20;
    if (e.icon) head |= (1 << 21) | (((e.icon.set << 3) | e.icon.index) << 24);
    words.push(e.row >>> 0, head >>> 0);
    if (e.bg !== undefined || e.fg !== undefined) {
      words.push(((e.bg !== undefined ? colorId(e.bg) : 0) | ((e.fg !== undefined ? colorId(e.fg) : 0) << 16)) >>> 0);
    }
    if (e.bar) words.push((colorId(e.bar.color) | ((e.bar.pct + 128) << 16) | (e.bar.start << 24)) >>> 0);
  }
  const bytes = Buffer.alloc(words.length * 4);
  words.forEach((w, i) => bytes.writeUInt32LE(w >>> 0, i * 4));
  return { pal: palette, w: bytes.toString("base64") };
}

function firstCellOfRule(rule) {
  const first = String(rule?.r || "A1").split(/[\s:]+/)[0];
  const m = /^([A-Za-z]+)(\d+)$/.exec(first);
  if (!m) return { row: 0, col: 0 };
  let col = 0;
  for (const ch of m[1].toUpperCase()) col = col * 26 + ch.charCodeAt(0) - 64;
  return { row: Math.max(0, Number(m[2]) - 1), col: Math.max(0, col - 1) };
}

function defaultCfHandler(body) {
  const cf = body.rules?.cf || [];
  if (cf.length === 0) return { pal: [], w: "" };
  const rule = cf[0];
  const { row, col } = firstCellOfRule(rule);
  if (rule.t === "bar") {
    const rgb = parseInt((rule.pos || "#638ec6").slice(1), 16);
    return packTestCf([{ row, col, bar: { color: rgb, pct: 80, start: 0 } }]);
  }
  if (rule.t === "scale") {
    const rgb = parseInt((rule.stops?.[0]?.c || "#f8696b").slice(1), 16);
    return packTestCf([{ row, col, bg: rgb }]);
  }
  if (rule.t === "icons") {
    return packTestCf([{ row, col, icon: { set: 0, index: 2 } }]);
  }
  return packTestCf([{ row, col, bg: 0xffc7ce }]);
}

// MiniElement.focus() dispatches "focus" only; mirror real-browser focusout/focusin on the format menu for menu-keyboard tests.
function bridgeFormatMenuFocus(doc) {
  const wrap = (el) => {
    if (!el || el._eoFocusBridged) return;
    el._eoFocusBridged = true;
    const orig = el.focus.bind(el);
    el.focus = () => {
      const prev = doc.activeElement;
      if (prev === el) return;
      orig();
      if (prev && prev !== doc.body) prev.dispatchEvent({ type: "focusout", target: prev, relatedTarget: el });
      el.dispatchEvent({ type: "focusin", target: el, relatedTarget: prev || null });
    };
  };
  const fmt = doc.querySelector('.menu-item[data-menu="format"]');
  wrap(fmt.querySelector(".menu-title"));
  for (const node of Array.from(fmt.querySelectorAll(".md-item, .md-sub"))) wrap(node);
}

async function liveSheetHarness({ onRules, onApply, onConvert, model } = {}) {
  const { sandbox: proto } = await createWorkspaceHarness();
  const cat = proto.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = proto.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const rulesCalls = [];
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    testIpc: async (command, payload) => {
      const body = typeof payload === "string" && payload.trimStart().startsWith("{") ? JSON.parse(payload) : null;
      if (command === "get_workspace_bootstrap") {
        return { ...offline.get_workspace_bootstrap(), sheetSessions: true };
      }
      if (command === "convert_document_format" && body && !body.action && onConvert) {
        onConvert(body);
      }
      if (command === "evaluate_sheet" && body?.sessionAction) {
        if (body.sessionAction === "open") return { session: "s1" };
        if (body.sessionAction === "close") return { closed: true };
        if (body.sessionAction === "apply") {
          const extra = onApply ? await onApply(body) : null;
          return { session: "s1", changed: [], spills: [], circular: false, ...(extra || {}) };
        }
        if (body.sessionAction === "rules") {
          rulesCalls.push(body);
          const cf = onRules ? await onRules(body) : defaultCfHandler(body);
          return { session: "s1", sheet: body.sheet, cf };
        }
        return { session: "s1", changed: [], spills: [], circular: false };
      }
      return offline[command](payload);
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns(8);
  if (model) {
    const bytes = new TextEncoder().encode("EOFIXTURE:" + JSON.stringify(model));
    const res = await harness.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("live.xlsx", bytes);
    assert.equal(res.ok, true);
    await flushAsyncTurns(10);
  }
  const win = harness.document.defaultView;
  const rules = win.__EASYOFFICE_SHEET_RULES__;
  const historySizes = () => win.__EASYOFFICE_WORKSPACE_HISTORY__.sizes();
  const tdAt = (ref) => harness.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  const selectRange = (r1, c1, r2, c2) => {
    const start = tdAt(rules.refOf(r1 - 1, c1 - 1));
    const end = tdAt(rules.refOf(r2 - 1, c2 - 1));
    start.dispatchEvent({ type: "mousedown", target: start, button: 0, bubbles: true });
    if (r1 !== r2 || c1 !== c2) {
      end.dispatchEvent({ type: "mousedown", target: end, button: 0, shiftKey: true, bubbles: true });
    }
  };
  return { ...harness, doc: harness.document, win, rules, rulesCalls, historySizes, tdAt, selectRange };
}

test("gallery pointer hover previews bar, scale and icon rules, survives 4px grid gaps, preserves existing rules, and clicks as one undo step", async () => {
  const before = {
    cf: [{ r: "A1", t: "dup", bg: "#ffc7ce" }],
    dv: [{ r: "D1", t: "whole", op: "between", a: "1", b: "2" }],
  };
  const h = await liveSheetHarness();
  const { doc, win, rules, historySizes, tdAt, selectRange, advanceTimers, userClick } = h;
  rules.replaceAll({ Sheet1: before });
  await flushAsyncTurns(8);
  selectRange(2, 2, 5, 3); // B2:C5
  const baseUndo = historySizes().undo;

  const tileBarBlue = doc.querySelector('.cfg-tile[data-sheet-cf="bar-grad-blue"]');
  const tileBarGreen = doc.querySelector('.cfg-tile[data-sheet-cf="bar-solid-green"]');
  const tileScaleGyr = doc.querySelector('.cfg-tile[data-sheet-cf="scale-gyr"]');
  const tileIcons3 = doc.querySelector('.cfg-tile[data-sheet-cf="icons-3arrows"]');
  const barPanel = doc.getElementById("sheet-cf-gallery-bar");
  const moreIcons = doc.querySelector('[data-sheet-cf="more-icons"]');

  // Hover Data Bar tile: previews as highest-priority rule, keeps existing rules & dv, 0 undo steps, paints fx on B2.
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  await flushAsyncTurns(8);
  assert.deepEqual(rules.get("Sheet1").cf.map((r) => [r.t, r.r]), [["bar", "B2:C5"], ["dup", "A1"]]);
  assert.deepEqual(rules.get("Sheet1").dv, before.dv);
  assert.equal(historySizes().undo, baseUndo);
  assert.match(tdAt("B2").style.getPropertyValue("--cf-bar"), /#638ec6/);

  // Leave across 4px gap inside .cfg-panel -> 48ms timer holds preview, switching to another tile cancels timer.
  tileBarBlue.dispatchEvent({ type: "mouseleave", target: tileBarBlue, relatedTarget: barPanel });
  advanceTimers(20);
  assert.equal(rules.get("Sheet1").cf.length, 2, "preview stays across gap before 48ms");
  tileBarGreen.dispatchEvent({ type: "mouseenter", target: tileBarGreen });
  advanceTimers(60);
  await flushAsyncTurns(8);
  assert.equal(rules.get("Sheet1").cf[0].pos, "#63c384", "new tile cancelled the leave timer");
  assert.match(tdAt("B2").style.getPropertyValue("--cf-bar"), /#63c384/);
  assert.equal(historySizes().undo, baseUndo);

  // Leave into panel gap and stay >= 48ms -> reverts cleanly to before.
  tileBarGreen.dispatchEvent({ type: "mouseleave", target: tileBarGreen, relatedTarget: barPanel });
  advanceTimers(48);
  await flushAsyncTurns(8);
  assert.deepEqual(rules.get("Sheet1"), before);
  assert.equal(tdAt("B2").dataset.cf, undefined);
  assert.equal(historySizes().undo, baseUndo);

  // Color Scale hover -> direct mouseleave outside panel reverts synchronously.
  tileScaleGyr.dispatchEvent({ type: "mouseenter", target: tileScaleGyr });
  await flushAsyncTurns(8);
  assert.equal(rules.get("Sheet1").cf[0].t, "scale");
  assert.equal(tdAt("B2").style.getPropertyValue("--cf-bg"), "#f8696b");
  tileScaleGyr.dispatchEvent({ type: "mouseleave", target: tileScaleGyr, relatedTarget: null });
  assert.deepEqual(rules.get("Sheet1").cf, before.cf);
  await flushAsyncTurns(8);
  assert.equal(tdAt("B2").dataset.cf, undefined);

  // Icon Set hover -> moving onto `기타 규칙…` reverts preview immediately.
  tileIcons3.dispatchEvent({ type: "mouseenter", target: tileIcons3 });
  await flushAsyncTurns(8);
  assert.equal(rules.get("Sheet1").cf[0].t, "icons");
  assert.equal(tdAt("B2").dataset.cfIcon, "up");
  moreIcons.dispatchEvent({ type: "mouseenter", target: moreIcons });
  assert.deepEqual(rules.get("Sheet1").cf, before.cf);
  await flushAsyncTurns(8);
  assert.equal(tdAt("B2").dataset.cfIcon, undefined);

  // Hover then click -> commits as 1 undo step without duplicating the rule; Ctrl+Z undoes back to before.
  const tileBarOrange = doc.querySelector('.cfg-tile[data-sheet-cf="bar-grad-orange"]');
  tileBarOrange.dispatchEvent({ type: "mouseenter", target: tileBarOrange });
  await flushAsyncTurns(8);
  userClick(tileBarOrange);
  await flushAsyncTurns(10);
  assert.equal(rules.get("Sheet1").cf.length, 2, "rule was not duplicated on click");
  assert.equal(rules.get("Sheet1").cf[0].pos, "#ffb628");
  assert.equal(historySizes().undo, baseUndo + 1);

  userClick(studioItem(doc, "studio-undo"));
  await flushAsyncTurns(10);
  assert.deepEqual(rules.get("Sheet1"), before);
  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(rules.get("Sheet1").cf.length, 2);
  assert.equal(rules.get("Sheet1").cf[0].pos, "#ffb628");

  // Switching top-level menu while hovering a tile closes the menu and reverts the preview via onMenusClosed.
  const formatMenu = doc.querySelector('.menu-item[data-menu="format"]');
  const toolMenu = doc.querySelector('.menu-item[data-menu="tool"]');
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  await flushAsyncTurns(8);
  assert.equal(rules.get("Sheet1").cf.length, 3);
  toolMenu.dispatchEvent({ type: "mouseenter", target: toolMenu });
  await flushAsyncTurns(8);
  assert.equal(rules.get("Sheet1").cf.length, 2, "switching menu reverts preview");

  // Leaving .cfg-panel and clicking outside menubar both revert preview.
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 3);
  barPanel.dispatchEvent({ type: "mouseleave", target: barPanel });
  assert.equal(rules.get("Sheet1").cf.length, 2, "mouseleave on .cfg-panel reverts immediately");
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 3);
  doc.dispatchEvent({ type: "click", target: tdAt("B2") });
  assert.equal(rules.get("Sheet1").cf.length, 2, "outside click closes menu and reverts preview");
  win.__EASYOFFICE_CLOSE_MENUS__?.(doc);
});

test("gallery keyboard focus previews, submenu Escape and Tab revert, and Enter/Space activate without duplicating", async () => {
  const h = await liveSheetHarness();
  const { doc, win, rules, historySizes, selectRange, sandbox, userClick } = h;
  bridgeFormatMenuFocus(doc);
  rules.replaceAll({});
  await flushAsyncTurns(6);
  selectRange(2, 2, 3, 3);
  const press = (el, key) => el.dispatchEvent({ type: "keydown", key, target: el, bubbles: true });
  const fmtTitle = doc.querySelector('.menu-item[data-menu="format"] > .menu-title');
  const cfSub = Array.from(doc.querySelectorAll('.menu-item[data-menu="format"] .md-sub')).find((s) => s.querySelector(".md-label")?.textContent === "조건부 서식");
  const barSub = Array.from(cfSub.querySelectorAll(".md-sub")).find((s) => s.querySelector(".md-label")?.textContent === "데이터 막대");
  const scaleSub = Array.from(cfSub.querySelectorAll(".md-sub")).find((s) => s.querySelector(".md-label")?.textContent === "색조");
  const iconSub = Array.from(cfSub.querySelectorAll(".md-sub")).find((s) => s.querySelector(".md-label")?.textContent === "아이콘 집합");

  // Open format menu and walk keyboard focus into Data Bar gallery.
  fmtTitle.focus();
  press(fmtTitle, "ArrowDown");
  cfSub.focus();
  press(cfSub, "ArrowRight");
  barSub.focus();
  press(barSub, "ArrowRight");
  const firstBar = doc.activeElement;
  assert.equal(firstBar.dataset.sheetCf, "bar-grad-blue");
  await flushAsyncTurns(8);
  assert.equal(rules.get("Sheet1").cf.length, 1);
  assert.equal(rules.get("Sheet1").cf[0].pos, "#638ec6");
  assert.equal(historySizes().undo, 0);

  // ArrowDown moves to second tile (relatedTarget is still .cfg-tile -> keeps preview and switches).
  press(firstBar, "ArrowDown");
  const secondBar = doc.activeElement;
  assert.equal(secondBar.dataset.sheetCf, "bar-grad-green");
  await flushAsyncTurns(8);
  assert.equal(rules.get("Sheet1").cf.length, 1);
  assert.equal(rules.get("Sheet1").cf[0].pos, "#63c384");

  // Escape inside submenu returns focus to parent .md-sub and reverts preview synchronously.
  sandbox.dispatchWindowKeydown({ key: "Escape", target: secondBar });
  assert.equal(doc.activeElement, barSub);
  assert.deepEqual(rules.get("Sheet1").cf, []);
  assert.equal(historySizes().undo, 0);

  // Re-enter tile then Tab out -> closes menu and reverts preview via onMenusClosed.
  press(barSub, "ArrowRight");
  assert.equal(rules.get("Sheet1").cf.length, 1);
  press(doc.activeElement, "Tab");
  assert.equal(Boolean(doc.querySelector(".menu-item.open")), false);
  assert.deepEqual(rules.get("Sheet1").cf, []);

  // Re-open and activate Color Scale tile with Enter -> preserves preview into click, 1 undo step, no duplication.
  fmtTitle.focus();
  press(fmtTitle, "ArrowDown");
  scaleSub.focus();
  press(scaleSub, "ArrowRight");
  assert.equal(doc.activeElement.dataset.sheetCf, "scale-gyr");
  assert.equal(rules.get("Sheet1").cf.length, 1);
  press(doc.activeElement, "Enter");
  await flushAsyncTurns(10);
  assert.equal(Boolean(doc.querySelector(".menu-item.open")), false);
  assert.equal(doc.activeElement, fmtTitle);
  assert.equal(rules.get("Sheet1").cf.length, 1);
  assert.equal(rules.get("Sheet1").cf[0].t, "scale");
  assert.equal(historySizes().undo, 1);

  // Activate Icon Set tile with Space -> adds 1 more rule in 1 undo step.
  fmtTitle.focus();
  press(fmtTitle, "ArrowDown");
  iconSub.focus();
  press(iconSub, "ArrowRight");
  assert.equal(rules.get("Sheet1").cf.length, 2);
  press(doc.activeElement, " ");
  await flushAsyncTurns(10);
  assert.equal(rules.get("Sheet1").cf.length, 2);
  assert.equal(rules.get("Sheet1").cf[0].t, "icons");
  assert.equal(historySizes().undo, 2);

  // Mixed case: hover a gallery tile with mouse, then press Enter on another menu item -> reverts hovered tile.
  userClick(fmtTitle);
  firstBar.dispatchEvent({ type: "mouseenter", target: firstBar });
  assert.equal(rules.get("Sheet1").cf.length, 3);
  const boldItem = doc.querySelector('.menu-item[data-menu="format"] .md-item[data-studio-menu-cmd="home-bold"]');
  boldItem.focus();
  press(boldItem, "Enter");
  await flushAsyncTurns(8);
  assert.equal(rules.get("Sheet1").cf.length, 2, "activating another menu item reverts unrelated hovered tile");

  // Duplicate onMenusClosed registration fires callback only once per close.
  let closeHits = 0;
  const countClose = () => { closeHits += 1; };
  win.__EASYOFFICE_ON_MENUS_CLOSED__(doc, countClose);
  win.__EASYOFFICE_ON_MENUS_CLOSED__(doc, countClose);
  win.__EASYOFFICE_CLOSE_MENUS__(doc);
  assert.equal(closeHits, 1);
});

test("protected sheet denying formatCells blocks hover preview silently and blocks click with alert and 0 undo steps", async () => {
  const h = await liveSheetHarness();
  const { doc, win, rules, historySizes, userClick } = h;
  rules.replaceAll({});
  await flushAsyncTurns(6);
  win.__EASYOFFICE_SHEET_LAYOUT__.setProtect("Sheet1", { on: true, deny: ["formatCells"], ranges: [] });

  const tile = doc.querySelector('.cfg-tile[data-sheet-cf="bar-grad-blue"]');
  const alertDlg = doc.getElementById("sheet-protect-alert-dialog");
  tile.dispatchEvent({ type: "mouseenter", target: tile });
  tile.dispatchEvent({ type: "focusin", target: tile });
  await flushAsyncTurns(6);
  assert.deepEqual(rules.get("Sheet1").cf, [], "no preview rule added on protected sheet");
  assert.equal(alertDlg.open, false, "hovering never opens protection alert");
  assert.equal(historySizes().undo, 0);

  userClick(tile);
  await flushAsyncTurns(10);
  assert.equal(alertDlg.open, true, "clicking protected gallery tile shows protection alert");
  assert.deepEqual(rules.get("Sheet1").cf, []);
  assert.equal(historySizes().undo, 0);
  alertDlg.close();
  win.__EASYOFFICE_SHEET_LAYOUT__.setProtect("Sheet1", null);
});

test("rapid hover-then-revert async syncRules race never paints stale fx or drops session rule sync", async () => {
  const h = await createWorkspaceHarness();
  const doc = h.document;
  const rules = doc.defaultView.__EASYOFFICE_SHEET_RULES__;
  const pending = [];
  const sentPayloads = [];
  const store = doc.defaultView.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: async (_cmd, payload) => {
      const body = JSON.parse(payload);
      if (body.sessionAction === "open") return { session: "s1" };
      if (body.sessionAction === "apply") return { session: "s1", changed: [], circular: false };
      if (body.sessionAction === "rules") {
        sentPayloads.push(body.rules);
        return new Promise((resolve) => pending.push({ body, resolve }));
      }
      return {};
    },
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(true);
  const sheets = { Sheet1: { A1: { value: "5", formula: "" } } };
  const barRule = { r: "A1", t: "bar", min: { k: "min", v: "" }, max: { k: "max", v: "" }, pos: "#638ec6", neg: "#ff0000", grad: true, show: true, stop: false };
  const scaleRule = { r: "A1", t: "scale", stops: [{ k: "min", v: "", c: "#f8696b" }, { k: "max", v: "", c: "#63be7b" }], stop: false };

  // Hover bar rule -> in-flight IPC 1; revert to empty BEFORE IPC 1 resolves.
  rules.set("Sheet1", { cf: [barRule], dv: [] });
  const eval1 = store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  await flushAsyncTurns(4);
  assert.equal(pending.length, 1);
  rules.set("Sheet1", { cf: [], dv: [] });
  const eval2 = store.evaluateLiveFormulaTargets(sheets, "Sheet1");

  // Resolve IPC 1 with bar fx: stale fx must NOT paint onto Sheet1.
  pending.shift().resolve({ session: "s1", sheet: "Sheet1", cf: packTestCf([{ row: 0, col: 0, bar: { color: 0x638ec6, pct: 100, start: 0 } }]) });
  await eval1;
  assert.equal(rules.fxAt("Sheet1", "A1"), undefined, "superseded response must not paint stale fx after revert");
  assert.equal(rules.hasFx("Sheet1"), false);

  // Queued eval2 must send empty rules because session.rules preserved that barRule was sent.
  await flushAsyncTurns(4);
  assert.equal(pending.length, 1, "engine session must be told the rules reverted to empty");
  assert.deepEqual(sentPayloads[1].cf, []);
  pending.shift().resolve({ session: "s1", sheet: "Sheet1", cf: { pal: [], w: "" } });
  await eval2;
  assert.equal(rules.fxAt("Sheet1", "A1"), undefined);

  // Switch tile 1 -> tile 2 while tile 1 is in flight: stale tile 1 does not flash, tile 2 paints cleanly.
  rules.set("Sheet1", { cf: [barRule], dv: [] });
  const sw1 = store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  await flushAsyncTurns(4);
  rules.set("Sheet1", { cf: [scaleRule], dv: [] });
  const sw2 = store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  pending.shift().resolve({ session: "s1", sheet: "Sheet1", cf: packTestCf([{ row: 0, col: 0, bar: { color: 0x638ec6, pct: 100, start: 0 } }]) });
  await sw1;
  assert.equal(rules.fxAt("Sheet1", "A1"), undefined, "superseded tile 1 response must not paint");
  await flushAsyncTurns(4);
  pending.shift().resolve({ session: "s1", sheet: "Sheet1", cf: packTestCf([{ row: 0, col: 0, bg: 0xf8696b }]) });
  await sw2;
  assert.equal(rules.fxAt("Sheet1", "A1")?.bg, "#f8696b");
  rules.replaceAll({});
});

test("manual calculation mode still paints gallery CF preview without overwriting unedited formula values", async () => {
  let a1Val = "10";
  const h = await liveSheetHarness({
    model: {
      sheets: [
        {
          name: "Sheet1",
          cells: [
            { ref: "A1", raw: "10" },
            { ref: "B1", raw: "=A1*2" },
            { ref: "B2", raw: "5" },
            { ref: "B3", raw: "15" },
          ],
        },
      ],
    },
    onApply: (body) => {
      const a1 = body.changes?.find((c) => c.cell === "A1")?.value;
      if (a1 !== undefined) a1Val = a1;
      return { changed: [{ sheet: "Sheet1", cell: "B1", value: String(Number(a1Val) * 2) }] };
    },
  });
  const { doc, win, tdAt, selectRange, userClick } = h;
  const calcApi = win.__EASYOFFICE_SHEET_CALC_OPTIONS__;
  assert.equal(tdAt("B1").textContent, "20");

  // Switch to manual calculation mode via menu.
  userClick(doc.querySelector('.md-item[data-sheet-act="calc-manual"]'));
  await flushAsyncTurns(10);
  assert.equal(calcApi.isManual(), true);
  assert.equal(tdAt("B1").textContent, "20");

  // Hover preview before any cell edit paints CF and does not falsely mark formulas pending.
  selectRange(2, 2, 3, 2); // B2:B3
  const tile = doc.querySelector('.cfg-tile[data-sheet-cf="bar-grad-blue"]');
  tile.dispatchEvent({ type: "mouseenter", target: tile });
  await flushAsyncTurns(10);
  assert.equal(tdAt("B2").dataset.cf, "1");
  assert.match(tdAt("B2").style.getPropertyValue("--cf-bar"), /#638ec6/);
  assert.equal(calcApi.isPending(), false);
  tile.dispatchEvent({ type: "mouseleave", target: tile, relatedTarget: null });
  await flushAsyncTurns(10);
  assert.equal(tdAt("B2").dataset.cf, undefined);

  // Edit A1 to 30 in manual mode -> B1 stays stale at 20, status shows "계산".
  const a1 = tdAt("A1");
  a1.focus();
  a1.textContent = "30";
  a1.dispatchEvent({ type: "input", target: a1 });
  a1.dispatchEvent({ type: "keydown", key: "Enter", target: a1 });
  await flushAsyncTurns(10);
  assert.equal(tdAt("B1").textContent, "20");
  assert.equal(calcApi.isPending(), true);
  assert.equal(doc.getElementById("status-command-feedback").textContent, "계산");

  // Now hover scale-gyr in manual mode: CF preview paints, B1 stays "20" (not overwritten with 60), pending stays intact.
  selectRange(2, 2, 3, 2);
  const scaleTile = doc.querySelector('.cfg-tile[data-sheet-cf="scale-gyr"]');
  scaleTile.dispatchEvent({ type: "mouseenter", target: scaleTile });
  await flushAsyncTurns(10);
  assert.equal(tdAt("B2").dataset.cf, "1");
  assert.equal(tdAt("B2").style.getPropertyValue("--cf-bg"), "#f8696b");
  assert.equal(tdAt("B1").textContent, "20", "unedited formula cell must stay stale in manual mode");
  assert.equal(calcApi.isPending(), true);
  assert.equal(doc.getElementById("status-command-feedback").textContent, "계산");

  scaleTile.dispatchEvent({ type: "mouseleave", target: scaleTile, relatedTarget: null });
  await flushAsyncTurns(10);
  assert.equal(tdAt("B2").dataset.cf, undefined);
  assert.equal(tdAt("B1").textContent, "20");
});

test("hover preview reverts before Ctrl+Z/Ctrl+S/native menu and never resurrects stale rules after external replaceAll", async () => {
  const savedBodies = [];
  const h = await liveSheetHarness({ onConvert: (body) => savedBodies.push(body) });
  const { doc, win, rules, historySizes, selectRange, sandbox, userClick } = h;
  selectRange(2, 2, 3, 3);
  const tileBarBlue = doc.querySelector('.cfg-tile[data-sheet-cf="bar-grad-blue"]');
  const tileScaleGyr = doc.querySelector('.cfg-tile[data-sheet-cf="scale-gyr"]');
  const formatMenu = doc.querySelector('.menu-item[data-menu="format"]');

  // Apply 1 real rule so undo stack has 1 step.
  userClick(tileScaleGyr);
  await flushAsyncTurns(10);
  assert.equal(rules.get("Sheet1").cf.length, 1);
  assert.equal(historySizes().undo, 1);

  // Open menu and hover bar-grad-blue, then press Ctrl+S -> menu closes & preview reverts BEFORE saving.
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 2);
  sandbox.dispatchWindowKeydown({ key: "s", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(Boolean(doc.querySelector(".menu-item.open")), false, "Ctrl+S closes open menu");
  assert.equal(rules.get("Sheet1").cf.length, 1, "preview reverted before Ctrl+S saved");
  assert.equal(savedBodies.at(-1)?.sheets?.[0]?.rules?.cf?.length ?? 0, 1, "saved file excludes uncommitted preview rule");

  // Hover bar-grad-blue, then press Ctrl+Z -> closes menu & reverts preview, then undoes the 1 real step cleanly.
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 2);
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.deepEqual(rules.get("Sheet1").cf, [], "Ctrl+Z undid real rule after reverting preview");
  assert.equal(historySizes().undo, 0);

  // Even if rules.replaceAll happens externally while hover is active, leaving tile must NOT resurrect stale before.cf.
  userClick(tileScaleGyr);
  await flushAsyncTurns(10);
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 2);
  rules.replaceAll({});
  assert.deepEqual(rules.get("Sheet1").cf, []);
  tileBarBlue.dispatchEvent({ type: "mouseleave", target: tileBarBlue, relatedTarget: null });
  assert.deepEqual(rules.get("Sheet1").cf, [], "mouseleave after external replaceAll must not resurrect stale rule");
  win.__EASYOFFICE_CLOSE_MENUS__?.(doc);

  // Native menu command while hovering closes menu and reverts preview before saving/undoing.
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 1);
  win.__EASYOFFICE_NATIVE_MENU__("file:save");
  await flushAsyncTurns(10);
  assert.equal(Boolean(doc.querySelector(".menu-item.open")), false);
  assert.deepEqual(rules.get("Sheet1").cf, []);
  assert.equal(savedBodies.at(-1)?.sheets?.[0]?.rules, undefined, "native save excludes reverted hover preview");
});

test("48ms gap timer, non-CF format items, menu-keyboard outside click, and syncRules synchronous throws behave cleanly", async () => {
  const h = await liveSheetHarness({
    model: { sheets: [{ name: "Sheet1", cells: [{ ref: "A1", raw: "10" }] }, { name: "Sheet2", cells: [{ ref: "A1", raw: "20" }] }] },
  });
  const { doc, win, rules, advanceTimers, userClick } = h;
  bridgeFormatMenuFocus(doc);
  const tileBarBlue = doc.querySelector('.cfg-tile[data-sheet-cf="bar-grad-blue"]');
  const barPanel = doc.getElementById("sheet-cf-gallery-bar");
  const formatMenu = doc.querySelector('.menu-item[data-menu="format"]');
  const editMenu = doc.querySelector('.menu-item[data-menu="edit"]');

  // Pending 48ms timer + __EASYOFFICE_CLOSE_MENUS__ (window-bridge path): reverts immediately & timer becomes no-op.
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 1);
  tileBarBlue.dispatchEvent({ type: "mouseleave", target: tileBarBlue, relatedTarget: barPanel });
  assert.equal(rules.get("Sheet1").cf.length, 1);
  win.__EASYOFFICE_CLOSE_MENUS__?.(doc);
  assert.deepEqual(rules.get("Sheet1").cf, []);
  advanceTimers(60);
  assert.deepEqual(rules.get("Sheet1").cf, []);

  // Pending 48ms timer + top menu mouseenter switch: reverts immediately.
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  tileBarBlue.dispatchEvent({ type: "mouseleave", target: tileBarBlue, relatedTarget: barPanel });
  editMenu.dispatchEvent({ type: "mouseenter", target: editMenu });
  assert.deepEqual(rules.get("Sheet1").cf, []);
  advanceTimers(60);

  // Pending 48ms timer + clicking sheet tab outside menubar: closes menu & reverts immediately.
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  tileBarBlue.dispatchEvent({ type: "mouseleave", target: tileBarBlue, relatedTarget: barPanel });
  userClick(doc.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]'));
  await flushAsyncTurns(10);
  assert.deepEqual(rules.get("Sheet1").cf, []);
  advanceTimers(60);
  userClick(doc.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns(10);

  // Non-CF items in formatMenu (e.g. 진하게, 셀 서식…) revert hover preview on hover/focus.
  userClick(formatMenu.querySelector(".menu-title"));
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 1);
  const numfmtDialogItem = formatMenu.querySelector('.md-item[data-sheet-numfmt="dialog"]');
  numfmtDialogItem.dispatchEvent({ type: "mouseenter", target: numfmtDialogItem });
  assert.deepEqual(rules.get("Sheet1").cf, [], "non-CF format item mouseenter reverts hover");
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 1);
  const boldItem = formatMenu.querySelector('.md-item[data-studio-menu-cmd="home-bold"]');
  boldItem.focus();
  assert.deepEqual(rules.get("Sheet1").cf, [], "non-CF format item focusin reverts hover");

  // Outside click closes menu even when .open was added without createMenuKeyboard.open().
  win.__EASYOFFICE_CLOSE_MENUS__?.(doc);
  formatMenu.classList.add("open");
  tileBarBlue.dispatchEvent({ type: "mouseenter", target: tileBarBlue });
  assert.equal(rules.get("Sheet1").cf.length, 1);
  doc.dispatchEvent({ type: "click", target: doc.getElementById("pane-sheet") });
  assert.equal(formatMenu.classList.contains("open"), false, "outside click closes .menu-item.open");
  assert.deepEqual(rules.get("Sheet1").cf, []);
});

test("syncRules synchronous throw clears rulesPending and non-empty before.cf recovers cleanly from race", async () => {
  const h = await createWorkspaceHarness();
  const doc = h.document;
  const rules = doc.defaultView.__EASYOFFICE_SHEET_RULES__;
  let throwSync = true;
  const pending = [];
  const store = doc.defaultView.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: (_cmd, payload) => {
      const body = JSON.parse(payload);
      if (body.sessionAction === "open") return Promise.resolve({ session: "s1" });
      if (body.sessionAction === "apply") return Promise.resolve({ session: "s1", changed: [], circular: false });
      if (body.sessionAction === "rules") {
        if (throwSync) throw new Error("sync failure");
        return new Promise((resolve) => pending.push({ body, resolve }));
      }
      return Promise.resolve({});
    },
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(true);
  const sheets = { Sheet1: { A1: { value: "5", formula: "" } } };
  const barRule = { r: "A1", t: "bar", min: { k: "min", v: "" }, max: { k: "max", v: "" }, pos: "#638ec6", neg: "#ff0000", grad: true, show: true, stop: false };
  const beforeRule = { r: "A1", t: "dup", bg: "#ffc7ce" };

  // Synchronous throw during rules sync must NOT leave rulesPending > 0.
  rules.set("Sheet1", { cf: [barRule], dv: [] });
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  rules.set("Sheet1", { cf: [], dv: [] });
  const afterClear = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(afterClear, null, "rulesInPlay must be false once rules are cleared after synchronous throw");

  // Non-empty beforeRule -> hover barRule -> revert before IPC 1 resolves -> restores beforeRule fx cleanly.
  throwSync = false;
  rules.set("Sheet1", { cf: [beforeRule], dv: [] });
  const initEval = store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  await flushAsyncTurns(4);
  pending.shift().resolve({ session: "s1", sheet: "Sheet1", cf: packTestCf([{ row: 0, col: 0, bg: 0xffc7ce }]) });
  await initEval;
  assert.equal(rules.fxAt("Sheet1", "A1")?.bg, "#ffc7ce");

  rules.set("Sheet1", { cf: [barRule, beforeRule], dv: [] });
  const hoverEval = store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  await flushAsyncTurns(4);
  rules.set("Sheet1", { cf: [beforeRule], dv: [] });
  const revertEval = store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  pending.shift().resolve({ session: "s1", sheet: "Sheet1", cf: packTestCf([{ row: 0, col: 0, bar: { color: 0x638ec6, pct: 100, start: 0 } }]) });
  await hoverEval;
  assert.equal(rules.fxAt("Sheet1", "A1"), undefined, "stale bar fx must not paint after revert to beforeRule");
  await flushAsyncTurns(4);
  pending.shift().resolve({ session: "s1", sheet: "Sheet1", cf: packTestCf([{ row: 0, col: 0, bg: 0xffc7ce }]) });
  await revertEval;
  assert.equal(rules.fxAt("Sheet1", "A1")?.bg, "#ffc7ce", "beforeRule fx is restored after revert");
  rules.replaceAll({});
});
