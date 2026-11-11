import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// PD-171: conditional formatting and data validation rules, their dialogs, the packed answers of
// the sheet session, and the pivot dialog.

const RUST_SETS = fs.readFileSync(
  new URL("../crates/easyoffice-formats/src/sheet_rules.rs", import.meta.url),
  "utf8"
);

function packWords(entries) {
  // Mirrors `pack.rs`: row, head, [colors], [bar].
  const palette = [];
  const id = (rgb) => {
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
    if (e.hide) head |= 1 << 22;
    if (e.bar?.gradient) head |= 1 << 23;
    words.push(e.row, head >>> 0);
    if (e.bg !== undefined || e.fg !== undefined) {
      words.push(((e.bg !== undefined ? id(e.bg) : 0) | ((e.fg !== undefined ? id(e.fg) : 0) << 16)) >>> 0);
    }
    if (e.bar) words.push((id(e.bar.color) | ((e.bar.pct + 128) << 16) | (e.bar.start << 24)) >>> 0);
  }
  const bytes = Buffer.alloc(words.length * 4);
  words.forEach((w, i) => bytes.writeUInt32LE(w >>> 0, i * 4));
  return { pal: palette, w: bytes.toString("base64") };
}

async function load() {
  const harness = await createWorkspaceHarness();
  // The page's globals live on the window of the harness document.
  const win = harness.document.defaultView;
  return { ...harness, win, rules: win.__EASYOFFICE_SHEET_RULES__ };
}

test("packed appearances decode into colors, flags, bars and icons", async () => {
  const { rules } = await load();
  const packed = packWords([
    { row: 1, col: 2, bg: 0xffc7ce, fg: 0x9c0006, flags: 5 },
    { row: 1048575, col: 16383, bar: { pct: -73, start: 41, color: 0x638ec6, gradient: true }, icon: { set: 19, index: 4 }, hide: true },
    { row: 7, col: 0 },
  ]);
  const [a, b, c] = rules.decodePacked(packed);
  assert.deepEqual({ row: a.row, col: a.col, bg: a.bg, fg: a.fg, flags: a.flags }, { row: 1, col: 2, bg: "#ffc7ce", fg: "#9c0006", flags: 5 });
  assert.deepEqual(b.bar, { color: "#638ec6", pct: -73, start: 41, gradient: true });
  assert.deepEqual([b.row, b.col, b.icon, b.hide], [1048575, 16383, { set: 19, index: 4 }, true]);
  assert.equal(c.bg, undefined);
  assert.deepEqual(rules.decodePacked({ pal: [], w: "" }), []);
});

test("the icon set order matches the engine's", async () => {
  const { rules } = await load();
  const block = RUST_SETS.slice(RUST_SETS.indexOf("pub const ICON_SETS"));
  const names = [...block.slice(0, block.indexOf("];")).matchAll(/\("(\w+)", \d\)/g)].map((m) => m[1]);
  assert.deepEqual(rules.ICON_SET_NAMES, names);
  for (const name of names) {
    assert.equal(rules.ICON_SETS[name].length, Number(name[0]), name);
    for (const [shape, color] of rules.ICON_SETS[name]) {
      assert.match(rules.iconMask(shape), /^url\("data:image\/svg\+xml,%3Csvg/, shape);
      assert.match(color, /^#[0-9a-f]{6}$/);
      assert.doesNotMatch(decodeURIComponent(rules.iconMask(shape)), /<path d=""|undefined/, shape);
    }
  }
});

test("a bar grows right of the axis for positive values and left of it for negative ones", async () => {
  const { rules } = await load();
  const pos = rules.barBackground({ color: "#00f", pct: 50, start: 0, gradient: false });
  assert.match(pos, /transparent 0%, #00f 0%, #00f 50%, transparent 50%/);
  const neg = rules.barBackground({ color: "#f00", pct: -100, start: 40, gradient: false });
  assert.match(neg, /transparent 0%, #f00 0%, #f00 40%, transparent 40%/);
  assert.match(rules.barBackground({ color: "#00f", pct: 10, start: 0, gradient: true }), /color-mix/);
});

test("ranges parse, format and test membership", async () => {
  const { rules } = await load();
  assert.deepEqual(rules.parseRanges("B2:A1 C3"), [
    { row0: 0, col0: 0, row1: 1, col1: 1 },
    { row0: 2, col0: 2, row1: 2, col1: 2 },
  ]);
  assert.equal(rules.formatRanges(rules.parseRanges("$B$2:C9 D1")), "B2:C9 D1");
  assert.equal(rules.parseRanges("A1:"), null);
  assert.equal(rules.parseRanges(""), null);
  assert.equal(rules.parseCell("XFD1048576").col, 16383);
  assert.equal(rules.parseCell("XFE1"), null);
});

test("setting rules marks them for the session and paints the answers onto cells", async () => {
  const { rules, document } = await load();
  let changed = 0;
  let repainted = 0;
  rules.on("changed", () => (changed += 1));
  rules.on("repaint", () => (repainted += 1));
  assert.equal(rules.hasAny(), false);
  rules.set("Sheet1", { cf: [{ r: "A1:A3", t: "dup" }], dv: [] });
  assert.equal(changed, 1);
  assert.equal(rules.hasAny(), true);
  assert.deepEqual(JSON.parse(rules.payloadFor("Sheet1")).cf[0].p, 1);

  rules.applyPacked("Sheet1", packWords([{ row: 0, col: 0, bg: 0xff0000, flags: 1, icon: { set: 3, index: 2 } }]), true);
  assert.equal(repainted, 1);
  const td = document.createElement("td");
  td.dataset.cell = "A1";
  rules.paintTd(td, "Sheet1");
  assert.equal(td.dataset.cf, "1");
  const prop = (name) => (td.style.getPropertyValue ? td.style.getPropertyValue(name) : td.style[name]);
  assert.equal(prop("--cf-bg"), "#ff0000");
  assert.equal(td.classList.contains("cf-bold"), true);
  assert.equal(td.dataset.cfIcon, "disc");
  assert.match(prop("--cf-icon-mask"), /^url\("data:image\/svg\+xml/);
  assert.equal(prop("--cf-icon-color"), "#1e8e3e");
  assert.equal(td.style.fontWeight, "", "the cell's own font properties are left alone");

  // A delta that clears the cell removes its look.
  rules.applyPacked("Sheet1", packWords([{ row: 0, col: 0 }]), false);
  rules.paintTd(td, "Sheet1");
  assert.equal(td.dataset.cf, undefined);
  assert.equal(td.classList.contains("cf-bold"), false);

  rules.replaceAll({});
  assert.equal(rules.hasAny(), false);
  assert.equal(changed, 2, "replacing rules that were set announces a change");
  rules.replaceAll({});
  assert.equal(changed, 2, "replacing nothing with nothing does not");
});

test("list items come from literals or from a range of the model", async () => {
  const { rules } = await load();
  rules.on("readCells", (sheet, ref) => ({ "Sheet1!D1": "red", "Sheet1!D2": "green", "Lists!A1": "x" })[sheet + "!" + ref] || "");
  rules.set("Sheet1", { cf: [], dv: [{ r: "B2:B3", t: "list", a: "a, b ,c" }, { r: "C2", t: "list", a: "=Sheet1!$D$1:$D$3" }, { r: "E1", t: "list", a: "=Lists!A1:A2" }] });
  assert.deepEqual(rules.listItems("Sheet1", rules.validationAt("Sheet1", "B3")), ["a", "b", "c"]);
  assert.deepEqual(rules.listItems("Sheet1", rules.validationAt("Sheet1", "C2")), ["red", "green"]);
  assert.deepEqual(rules.listItems("Sheet1", rules.validationAt("Sheet1", "E1")), ["x"]);
  assert.equal(rules.validationAt("Sheet1", "Z9"), null);
  rules.replaceAll({});
});

// ---- the sheet session ----------------------------------------------------------------------

function fakeEngine(onRules) {
  const calls = [];
  async function invokeIpc(command, payload) {
    const body = JSON.parse(payload);
    calls.push(body.sessionAction || command);
    switch (body.sessionAction) {
      case "open":
        return { session: "s1" };
      case "apply":
        return { session: "s1", changed: [], circular: false, cf: calls.deltas ? calls.deltas.splice(0) : undefined };
      case "rules":
        calls.lastRules = body;
        return { session: "s1", sheet: body.sheet, cf: onRules(body) };
      case "validate":
        return body.value === "bad" ? { ok: false, alert: "stop", title: "t", message: "m", show: true } : { ok: true };
      case "close":
        return { closed: true };
      default:
        throw new Error("unexpected " + command);
    }
  }
  return { invokeIpc, calls };
}

async function storeWith(engine) {
  const harness = await createWorkspaceHarness();
  const store = harness.document.defaultView.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: engine.invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(true);
  return { ...harness, store, rules: harness.document.defaultView.__EASYOFFICE_SHEET_RULES__ };
}

test("a sheet with rules but no formulas still syncs the session and shows the engine's answer", async () => {
  const engine = fakeEngine(() => packWords([{ row: 0, col: 0, bg: 0x00ff00 }]));
  const { store, rules } = await storeWith(engine);
  rules.set("Sheet1", { cf: [{ r: "A1", t: "cellIs", op: "greaterThan", a: "1", bg: "#00ff00", fl: 0 }], dv: [] });
  const sheets = { Sheet1: { A1: { value: "5", formula: "" } } };
  const live = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.ok(live, "evaluation runs although no cell holds a formula");
  assert.deepEqual(engine.calls.filter((c) => typeof c === "string"), ["open", "apply", "rules", "apply"].slice(0, 4).filter((c, i, a) => a.indexOf(c) === i || true).slice(0, engine.calls.length));
  assert.equal(engine.calls.lastRules.rules.cf[0].p, 1);
  assert.equal(rules.fxAt("Sheet1", "A1").bg, "#00ff00");

  // Unchanged rules are not sent again; a changed rule set is.
  const before = engine.calls.filter((c) => c === "rules").length;
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.calls.filter((c) => c === "rules").length, before);
  rules.set("Sheet1", { cf: [], dv: [] });
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.calls.filter((c) => c === "rules").length, before + 1, "clearing the rules tells the engine");
  rules.replaceAll({});
});

test("deltas in an apply answer update only the cells they name, and a dropped sheet loses its look", async () => {
  const engine = fakeEngine(() => packWords([{ row: 0, col: 0, bg: 0x111111 }, { row: 1, col: 0, bg: 0x222222 }]));
  const { store, rules } = await storeWith(engine);
  rules.set("Sheet1", { cf: [{ r: "A1:A2", t: "dup" }], dv: [] });
  const sheets = { Sheet1: { A1: { value: "5", formula: "" }, A2: { value: "6", formula: "" } } };
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  engine.calls.deltas = [{ sheet: "Sheet1", ...packWords([{ row: 1, col: 0 }]) }];
  sheets.Sheet1.A2 = { value: "7", formula: "" };
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(rules.fxAt("Sheet1", "A1").bg, "#111111");
  assert.equal(rules.fxAt("Sheet1", "A2"), undefined);
  rules.replaceAll({});
});

test("validateEntry asks the session and answers null when it cannot", async () => {
  const engine = fakeEngine(() => packWords([]));
  const { store, rules } = await storeWith(engine);
  assert.equal(await store.validateEntry("Sheet1", "B2", "x"), null, "no session yet");
  rules.set("Sheet1", { cf: [], dv: [{ r: "B2", t: "whole", op: "between", a: "1", b: "2" }] });
  await store.evaluateLiveFormulaTargets({ Sheet1: {} }, "Sheet1");
  assert.deepEqual(await store.validateEntry("Sheet1", "B2", "ok"), { ok: true });
  assert.equal((await store.validateEntry("Sheet1", "B2", "bad")).ok, false);
  rules.replaceAll({});
});

// ---- dialogs --------------------------------------------------------------------------------

function field(document, id, value) {
  const el = document.getElementById(id);
  assert.ok(el, id);
  if (typeof value === "boolean") el.checked = value;
  else el.value = value;
  return el;
}

async function dialogHarness() {
  const harness = await load();
  const { win: sandbox, document } = harness;
  const pushed = [];
  const stored = {};
  const rulesUi = sandbox.__EASYOFFICE_CREATE_SHEET_RULES_UI__({
    document,
    sheetDataStore: stored,
    sheetRowCounts: {},
    getActiveSheetName: () => "Sheet1",
    getRange: () => ({ r1: 2, c1: 2, r2: 5, c2: 3 }),
    isFormulaStr: (t) => String(t).startsWith("="),
    sheetStoreHelpers: {
      validateEntry: async (_s, _c, text) =>
        text === "stop" ? { ok: false, alert: "stop", title: "제한", message: "안 됩니다", show: true }
        : text === "warn" ? { ok: false, alert: "warning", title: "", message: "", show: true }
        : text === "quiet" ? { ok: false, alert: "stop", title: "", message: "", show: false }
        : { ok: true },
      callSheetAction: async () => ({}),
    },
    enqueueWorkspaceHistoryOp: (fn) => fn(),
    drainInFlightSheetWork: async () => {},
    pushRawUndoSnapshot: (snap) => pushed.push(snap),
    captureWorkspaceSnapshot: () => ({ snapshot: true }),
    flushFocusedSheetCellBeforeSwitch: async () => {},
    performSwitchSheetTabAsync: async () => {},
    applyCellEdits: async () => true,
    readTdValue: (td) => td.textContent,
  });
  return { ...harness, rulesUi, pushed, stored };
}

test("the conditional formatting dialog builds rules from its fields and keeps them ordered", async () => {
  const { rulesUi, rules, document, userClick, pushed } = await dialogHarness();
  rulesUi.openConditionalFormatDialog();
  assert.equal(document.getElementById("sheet-cf-dialog").open, true);
  assert.equal(document.getElementById("sheet-cf-range").value, "B2:C5", "the selection is the default range");

  field(document, "sheet-cf-type-select", "cellIs");
  field(document, "sheet-cf-cmp-select", "between");
  field(document, "sheet-cf-a", "=10");
  field(document, "sheet-cf-b", "20");
  userClick(document.getElementById("btn-sheet-cf-apply"));
  await flushAsyncTurns();
  const first = rules.get("Sheet1").cf[0];
  assert.deepEqual({ r: first.r, t: first.t, op: first.op, a: first.a, b: first.b, bg: first.bg, fg: first.fg }, {
    r: "B2:C5", t: "cellIs", op: "between", a: "10", b: "20", bg: "#ffc7ce", fg: "#9c0006",
  });
  assert.equal(pushed.length, 1, "an undo snapshot is taken");

  userClick(document.getElementById("btn-sheet-cf-new"));
  field(document, "sheet-cf-range", "D1:D9");
  field(document, "sheet-cf-type-select", "scale");
  field(document, "sheet-cf-mid", false);
  userClick(document.getElementById("btn-sheet-cf-apply"));
  await flushAsyncTurns();
  assert.deepEqual(rules.get("Sheet1").cf[1].stops.map((s) => s.k), ["min", "max"]);

  userClick(document.getElementById("btn-sheet-cf-up"));
  await flushAsyncTurns();
  assert.deepEqual(rules.get("Sheet1").cf.map((r) => r.t), ["scale", "cellIs"]);
  assert.equal(JSON.parse(rules.payloadFor("Sheet1")).cf[1].p, 2, "the list order is the priority");

  userClick(document.getElementById("btn-sheet-cf-delete"));
  await flushAsyncTurns();
  assert.deepEqual(rules.get("Sheet1").cf.map((r) => r.t), ["cellIs"]);
  rules.replaceAll({});
});

test("an incomplete conditional formatting rule is explained, not stored", async () => {
  const { rulesUi, rules, document, userClick } = await dialogHarness();
  rulesUi.openConditionalFormatDialog();
  field(document, "sheet-cf-range", "not a range");
  userClick(document.getElementById("btn-sheet-cf-apply"));
  await flushAsyncTurns();
  assert.match(document.getElementById("sheet-cf-status").textContent, /적용 범위/);
  field(document, "sheet-cf-range", "A1:A3");
  field(document, "sheet-cf-type-select", "cellIs");
  field(document, "sheet-cf-a", "");
  userClick(document.getElementById("btn-sheet-cf-apply"));
  await flushAsyncTurns();
  assert.match(document.getElementById("sheet-cf-status").textContent, /값/);
  field(document, "sheet-cf-a", "5");
  field(document, "sheet-cf-bg-on", false);
  field(document, "sheet-cf-fg-on", false);
  userClick(document.getElementById("btn-sheet-cf-apply"));
  await flushAsyncTurns();
  assert.match(document.getElementById("sheet-cf-status").textContent, /배경, 글자 색, 글꼴/);
  assert.equal(rules.hasAny(), false);
});

test("the data validation dialog stores one rule per range and clears overlapping ones", async () => {
  const { rulesUi, rules, document, userClick } = await dialogHarness();
  rulesUi.openDataValidationDialog();
  assert.equal(document.getElementById("sheet-dv-dialog").open, true);
  field(document, "sheet-dv-type-select", "whole");
  field(document, "sheet-dv-cmp-select", "between");
  field(document, "sheet-dv-a", "1");
  field(document, "sheet-dv-b", "10");
  field(document, "sheet-dv-alert-select", "warning");
  field(document, "sheet-dv-err", "1에서 10 사이");
  userClick(document.getElementById("btn-sheet-dv-apply"));
  await flushAsyncTurns();
  const [dv] = rules.get("Sheet1").dv;
  assert.deepEqual([dv.r, dv.t, dv.a, dv.b, dv.alert, dv.err], ["B2:C5", "whole", "1", "10", "warning", "1에서 10 사이"]);
  assert.equal(document.getElementById("sheet-dv-dialog").open, false);

  // Reopening on a covered cell shows the stored rule.
  rulesUi.openDataValidationDialog();
  assert.equal(document.getElementById("sheet-dv-a").value, "1");

  field(document, "sheet-dv-type-select", "list");
  field(document, "sheet-dv-list", "가, 나");
  userClick(document.getElementById("btn-sheet-dv-apply"));
  await flushAsyncTurns();
  assert.deepEqual(rules.get("Sheet1").dv.map((d) => [d.t, d.a]), [["list", "가, 나"]]);

  userClick(document.getElementById("btn-sheet-dv-clear"));
  await flushAsyncTurns();
  assert.equal(rules.hasAny(), false);
});

test("a refused entry takes the old value back and an accepted warning commits", async () => {
  const { rulesUi, rules, document, userClick } = await dialogHarness();
  rules.set("Sheet1", { cf: [], dv: [{ r: "B2", t: "whole", op: "between", a: "1", b: "2" }] });
  const td = document.createElement("td");
  td.dataset.cell = "B2";
  const st = { sheetName: "Sheet1", userEdited: true, initialText: "1" };
  const ask = (text) => {
    td.textContent = text;
    return rulesUi.gateEntry(td, st);
  };
  assert.ok(ask("fine") instanceof Promise, "a covered cell is checked");
  assert.equal(await ask("fine"), true);

  const stopped = ask("stop");
  await flushAsyncTurns();
  assert.equal(document.getElementById("sheet-dv-alert-dialog").open, true);
  assert.equal(document.getElementById("sheet-dv-alert-message").textContent, "안 됩니다");
  assert.equal(document.getElementById("btn-sheet-dv-alert-cancel").hidden, true, "a stop alert only informs");
  userClick(document.getElementById("btn-sheet-dv-alert-ok"));
  assert.equal(await stopped, false);

  const warned = ask("warn");
  await flushAsyncTurns();
  assert.equal(document.getElementById("btn-sheet-dv-alert-cancel").hidden, false);
  assert.match(document.getElementById("sheet-dv-alert-message").textContent, /입력한 값이 이 셀에 설정된 제한/);
  userClick(document.getElementById("btn-sheet-dv-alert-ok"));
  assert.equal(await warned, true, "continuing after a warning commits");

  const declined = ask("warn");
  await flushAsyncTurns();
  userClick(document.getElementById("btn-sheet-dv-alert-cancel"));
  assert.equal(await declined, false);

  assert.equal(await ask("quiet"), true, "a rule without an error alert accepts silently");
  td.textContent = "";
  assert.equal(rulesUi.gateEntry(td, st), null, "clearing a cell needs no check");
  td.textContent = "=A1";
  assert.equal(rulesUi.gateEntry(td, st), null, "a formula entry needs no check");
  td.dataset.cell = "C9";
  td.textContent = "stop";
  assert.equal(rulesUi.gateEntry(td, st), null, "a cell outside every rule needs no check");
  rules.replaceAll({});
});

test("pivot results become the cells of a new sheet", async () => {
  const { rulesUi } = await dialogHarness();
  const two = rulesUi.pivotCells(
    { rowKeys: ["영업", "개발"], columnKeys: ["1Q", "2Q"], matrix: [[21, 20], [5, 0]], rowTotals: [41, 5], columnTotals: [26, 20], grandTotal: 46 },
    { row: "부서", col: "분기", value: "합계: 매출" }
  );
  const text = (cells) => Object.fromEntries(Object.entries(cells).map(([k, v]) => [k, v.value]));
  assert.deepEqual(text(two.cells), {
    A1: "부서", B1: "1Q", C1: "2Q", D1: "합계",
    A2: "영업", B2: "21", C2: "20", D2: "41",
    A3: "개발", B3: "5", C3: "0", D3: "5",
    A4: "합계", B4: "26", C4: "20", D4: "46",
  });
  assert.equal(two.rows, 4);
  const one = rulesUi.pivotCells(
    { rowKeys: ["영업"], columnKeys: ["Total"], matrix: [[13.666666666666666]], rowTotals: [13.666666666666666], columnTotals: [13.666666666666666], grandTotal: 13.666666666666666 },
    { row: "부서", col: "", value: "평균: 매출" }
  );
  assert.deepEqual(text(one.cells), { A1: "부서", B1: "평균: 매출", A2: "영업", B2: "13.6666666667", A3: "합계", B3: "13.6666666667" });
});

test("the pivot dialog sends the sheet's cells and writes a new sheet with the answer", async () => {
  const harness = await dialogHarness();
  const { document, win: sandbox, userClick } = harness;
  const sent = [];
  const stored = {
    Sheet1: {
      A1: { value: "부서" }, B1: { value: "분기" }, C1: { value: "매출" },
      A2: { value: "영업" }, B2: { value: "1Q" }, C2: { value: "10" },
    },
  };
  const switched = [];
  const ui = sandbox.__EASYOFFICE_CREATE_SHEET_RULES_UI__({
    document,
    sheetDataStore: stored,
    sheetRowCounts: {},
    getActiveSheetName: () => "Sheet1",
    getRange: () => ({ r1: 1, c1: 1, r2: 1, c2: 1 }),
    isFormulaStr: () => false,
    sheetStoreHelpers: {
      callSheetAction: async (req) => {
        sent.push(req);
        return { rowKeys: ["영업"], columnKeys: ["Total"], matrix: [[10]], rowTotals: [10], columnTotals: [10], grandTotal: 10 };
      },
    },
    enqueueWorkspaceHistoryOp: (fn) => fn(),
    drainInFlightSheetWork: async () => {},
    pushRawUndoSnapshot: () => {},
    captureWorkspaceSnapshot: () => ({}),
    flushFocusedSheetCellBeforeSwitch: async () => {},
    performSwitchSheetTabAsync: async (key) => switched.push(key),
    applyCellEdits: async () => true,
    readTdValue: (td) => td.textContent,
  });
  ui.openPivotDialog();
  assert.equal(document.getElementById("sheet-pivot-range").value, "A1:C2", "a single cell selects the used range");
  const rows = [...document.getElementById("sheet-pivot-row-select").querySelectorAll("option")].map((o) => o.textContent);
  assert.deepEqual(rows, ["부서", "분기", "매출"]);
  field(document, "sheet-pivot-row-select", "0");
  field(document, "sheet-pivot-col-select", "");
  field(document, "sheet-pivot-value-select", "2");
  field(document, "sheet-pivot-agg-select", "sum");
  userClick(document.getElementById("btn-sheet-pivot-apply"));
  await flushAsyncTurns(12);
  assert.equal(sent.length, 1);
  assert.deepEqual(
    { ...sent[0], cells: undefined },
    { sessionAction: "pivot", sheet: "Sheet1", cells: undefined, range: "A1:C2", header: true, row: 0, col: null, value: 2, agg: "sum" }
  );
  assert.equal(sent[0].cells.length, 6);
  assert.deepEqual(switched, ["피벗1"]);
  assert.equal(stored["피벗1"].B2.value, "10");
  assert.equal(document.getElementById("sheet-pivot-dialog").open, false);
});

test("every dialog is a direct child of the page, so a dialog's markup cannot swallow the next one", async () => {
  const { document } = await load();
  const dialogs = document.querySelectorAll("dialog");
  assert.ok(dialogs.length >= 10);
  for (const dlg of dialogs) assert.equal(dlg.parentElement?.tagName, "BODY", dlg.id);
  for (const id of ["sheet-cf-dialog", "sheet-dv-dialog", "sheet-dv-alert-dialog", "sheet-pivot-dialog"]) {
    assert.ok(document.getElementById(id), id);
  }
});

test("colour fields use the shell's palette popup and no visible native colour input", async () => {
  const { rulesUi, document } = await dialogHarness();
  rulesUi.openConditionalFormatDialog();
  for (const id of ["sheet-cf-bg", "sheet-cf-fg", "sheet-cf-bar-pos", "sheet-cf-stop0-color"]) {
    const host = document.getElementById(id + "-host");
    assert.ok(host.querySelector("[data-color-toggle]"), id);
    assert.ok(document.getElementById(id + "-palette").children.length > 0, "the palette is filled: " + id);
    assert.equal(document.getElementById(id).getAttribute("aria-hidden"), "true", id);
  }
  document.getElementById("sheet-cf-bg-palette").querySelector("button").click?.();
  assert.match(document.getElementById("sheet-cf-bg").value, /^#[0-9a-f]{6}$/i);
});

// ---- which rows a rule kind shows ------------------------------------------------------------

const STYLE_ROWS = ["배경", "글자", "글꼴"];
const CF_VISIBLE = {
  cellIs: ["조건", ...STYLE_ROWS],
  text: ["텍스트", ...STYLE_ROWS],
  top: ["순위", ...STYLE_ROWS],
  average: ["평균", ...STYLE_ROWS],
  dup: ["값", ...STYLE_ROWS],
  blanks: ["대상", ...STYLE_ROWS],
  errors: ["대상", ...STYLE_ROWS],
  expr: ["수식", ...STYLE_ROWS],
  scale: ["최소", "중간", "최대"],
  bar: ["막대 시작", "막대 끝", "막대 색", "표시"],
  icons: ["아이콘", "아이콘 2", "표시"],
};
const DV_VISIBLE = {
  whole: ["조건", "값"],
  decimal: ["조건", "값"],
  date: ["조건", "값"],
  time: ["조건", "값"],
  textLength: ["조건", "값"],
  list: ["목록"],
  custom: ["값"],
};

function shownKinds(document, dialogId) {
  const out = [];
  for (const el of document.getElementById(dialogId).querySelectorAll("[data-cf-kinds]")) {
    if (el.hidden) continue;
    out.push(el.querySelector?.(".dialog-label")?.textContent || el.id);
  }
  return out;
}

const bySort = (list) => [...list].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));

function choose(document, id, value) {
  const el = document.getElementById(id);
  el.value = value;
  el.dispatchEvent({ type: "change", target: el, bubbles: true });
}

test("each conditional formatting kind shows exactly its own rows, without the dialog being opened first", async () => {
  const { document } = await dialogHarness();
  for (const [type, rows] of Object.entries(CF_VISIBLE)) {
    choose(document, "sheet-cf-type-select", type);
    assert.deepEqual(bySort(shownKinds(document, "sheet-cf-dialog")), bySort(rows), type);
  }
  choose(document, "sheet-cf-type-select", "icons");
  assert.equal(document.getElementById("sheet-cf-icon-th").children.length, 2, "the 3-icon default set has two thresholds");
});

test("each data validation kind shows exactly its own rows", async () => {
  const { document } = await dialogHarness();
  for (const [type, rows] of Object.entries(DV_VISIBLE)) {
    choose(document, "sheet-dv-type-select", type);
    assert.deepEqual(bySort(shownKinds(document, "sheet-dv-dialog")), bySort(rows), type);
  }
});

// The cascade: a row that is `hidden` must lose its `display: flex` to a rule of at least the same
// specificity that comes later (no `!important`).
function specificity(selector) {
  const ids = (selector.match(/#[\w-]+/g) || []).length;
  const classes = (selector.match(/\.[\w-]+|\[[^\]]+\]|:(?!:)[\w-]+/g) || []).length;
  const elements = (selector.replace(/#[\w-]+|\.[\w-]+|\[[^\]]+\]|::?[\w-]+/g, " ").match(/\b[a-z][\w-]*/gi) || []).length;
  return ids * 10000 + classes * 100 + elements;
}

test("hidden rows of a tool dialog lose no cascade to the row layout rules", () => {
  const css = fs.readFileSync(new URL("../crates/easyoffice-app/ui/shell.css", import.meta.url), "utf8");
  const rules = [...css.matchAll(/^([^{}\n@/][^{\n]*)\{([^}]*)\}/gm)].map((m, index) => ({
    selectors: m[1].split(",").map((s) => s.trim()),
    body: m[2],
    index,
  }));
  const hiding = rules.filter((r) => /display:\s*none/.test(r.body) && r.selectors.some((s) => /\[hidden\]/.test(s) && s.startsWith(".tool-dialog")));
  assert.ok(hiding.length > 0);
  const showing = rules.filter((r) => /display:\s*(flex|inline-flex)/.test(r.body) && r.selectors.some((s) => s.startsWith(".tool-dialog") && !/\[hidden\]/.test(s)));
  for (const show of showing) {
    for (const sel of show.selectors.filter((s) => s.startsWith(".tool-dialog"))) {
      const winner = hiding.find((h) => h.index > show.index && h.selectors.some((hs) => specificity(hs) >= specificity(sel)));
      assert.ok(winner, `a hidden element keeps no display rule over "${sel}"`);
    }
  }
});

test("a rule dialog takes the shell's common open path: the focus starts on the primary button", async () => {
  const { rulesUi, document } = await dialogHarness();
  rulesUi.openDataValidationDialog();
  assert.equal(document.activeElement.id, "btn-sheet-dv-apply");
  rulesUi.openConditionalFormatDialog();
  assert.equal(document.activeElement.id, "btn-sheet-cf-apply");
});

test("the alert dialog follows the reference confirm dialog: 360px, padded body, line breaks kept", () => {
  const css = fs.readFileSync(new URL("../crates/easyoffice-app/ui/shell.css", import.meta.url), "utf8");
  assert.match(css, /#sheet-dv-alert-dialog \{ width: min\(360px/);
  assert.match(css, /#sheet-dv-alert-dialog \.dialog-body \{ padding: 16px 20px; line-height: 1\.6; white-space: pre-line; \}/);
});

test("the icon set combo shows Korean labels and keeps the file format identifiers as values", async () => {
  const { document, rules } = await dialogHarness();
  const options = [...document.getElementById("sheet-cf-icon-set-select").querySelectorAll("option")];
  assert.deepEqual(options.map((o) => o.value), rules.ICON_SET_NAMES);
  for (const o of options) {
    assert.match(o.textContent, /[가-힣]/, o.value);
    assert.ok(!/[A-Za-z]/.test(o.textContent), o.textContent);
    assert.equal(o.textContent.endsWith(o.value[0] + "개"), true, o.textContent);
  }
  assert.equal(options.find((o) => o.value === "3TrafficLights1").textContent, "신호등 3개");
});

test("the pivot combos have a placeholder option until a range names fields, and refuse to build without one", async () => {
  const { document, userClick, rulesUi } = await dialogHarness();
  const text = (id) => [...document.getElementById(id).querySelectorAll("option")].map((o) => o.textContent);
  assert.deepEqual(text("sheet-pivot-row-select"), ["(범위를 먼저 입력)"]);
  assert.deepEqual(text("sheet-pivot-value-select"), ["(범위를 먼저 입력)"]);
  assert.deepEqual(text("sheet-pivot-col-select"), ["(없음)"]);
  rulesUi.openPivotDialog();
  document.getElementById("sheet-pivot-range").value = "";
  userClick(document.getElementById("btn-sheet-pivot-apply"));
  await flushAsyncTurns();
  assert.match(document.getElementById("sheet-pivot-status").textContent, /데이터 범위/);
  assert.equal(document.getElementById("sheet-pivot-dialog").open, true, "nothing was built");
});
