import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

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
  assert.deepEqual(direct(panel("규칙 지우기")), ["선택한 셀에서 규칙 지우기", "전체 시트에서 규칙 지우기"]);
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
  assert.deepEqual(P.borderFromControls({ on: true, line: "double", color: "#0000ff", sides }), { t: "double:0000ff", r: "double:0000ff", b: "double:0000ff", l: "double:0000ff" });
  assert.deepEqual(P.borderFromControls({ on: true, line: "dotted", color: "#112233", sides: ["l", "t"] }), { t: "dotted:112233", l: "dotted:112233" });
  assert.equal(P.borderFromControls({ on: false, line: "thin", color: "#000000", sides }), null);
  assert.equal(P.borderFromControls({ on: true, line: "thin", color: "#000000", sides: [] }), null);
  assert.equal(P.borderFromControls({ on: true, line: "wavy", color: "#000000", sides }), null);
  assert.equal(P.borderFromControls({ on: true, line: "thin", color: "red", sides }), null);
  assert.deepEqual(P.controlsFromBorder({ r: "thick:00ff00", b: "thin:ff0000" }), { on: true, line: "thick", color: "#00ff00", sides: ["r", "b"] });
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
  assert.equal(doc.getElementById("sheet-cf-bd-line").value, "double", "the line of the first edge");
  assert.equal(doc.getElementById("sheet-cf-bd").value, "#0000ff");
  assert.deepEqual(sideBoxes(doc), [true, false, false, true]);

  await apply();
  assert.deepEqual(rules.get("Sheet1").cf[0].bd, mixed, "an untouched row keeps the rule's own edges, one line kind per side included");

  field(doc, "sheet-cf-bd-line", "thick");
  await apply();
  assert.deepEqual(rules.get("Sheet1").cf[0].bd, { t: "thick:0000ff", l: "thick:0000ff" }, "a changed row writes its inputs on the ticked sides");

  field(doc, "sheet-cf-bd-r", true);
  field(doc, "sheet-cf-bd", "#ff0000");
  await apply();
  assert.deepEqual(rules.get("Sheet1").cf[0].bd, { t: "thick:ff0000", r: "thick:ff0000", l: "thick:ff0000" });

  for (const side of ["t", "r", "b", "l"]) field(doc, "sheet-cf-bd-" + side, false);
  await apply();
  assert.match(doc.getElementById("sheet-cf-status").textContent, /변을 하나 이상/);
  assert.equal(rules.get("Sheet1").cf[0].bd.t, "thick:ff0000", "a refused form stores nothing");

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
  for (const name of ["sheet-cf-presets.js", "sheet-cf-preview.js", "sheet-cf-quick-ui.js", "sheet-rules-ui.js"]) {
    const text = source(name);
    assert.doesNotMatch(text, /querySelectorAll\([^)]*\)\.(map|filter|find|some|every|reduce|flatMap|slice|indexOf|includes)\b/, `${name}: querySelectorAll`);
    assert.doesNotMatch(text, /\.(childNodes|children)\.(map|filter|find|some|every|reduce|flatMap|slice|indexOf|includes|at)\b/, `${name}: childNodes or children`);
  }
});
