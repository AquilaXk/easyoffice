import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// The defined names of the sheet window (PD-214): the name box of the formula bar (a name selects what it refers to,
// an address selects that range, a new valid name names the selection, the arrow opens the list), the 이름 정의 and
// 이름 관리 dialogs, one undo step each, and the list sent to the engine session and saved with the file.

const CHECK_MESSAGES = { duplicate: "이미 있는 이름", reserved: "함수나 시트 이름은 쓸 수 없음", name: "이름이 올바르지 않음" };

async function open(options = {}) {
  const calls = [];
  const harness = await createWorkspaceHarness({
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        calls.push(body);
        if (body.sessionAction === "nameCheck" && body.candidates) {
          const seen = new Set(body.names.map((n) => n.name.toLowerCase()));
          return { problems: body.candidates.map((c) => { const k = c.name.toLowerCase(); if (!/^[^\d\s][\w가-힣]*$/.test(c.name) || /^[A-Za-z]{1,3}\d+$/.test(c.name)) return "name"; if (seen.has(k)) return "duplicate"; seen.add(k); return null; }) };
        }
        if (body.sessionAction === "nameValues") return { session: "s1", values: body.names ? [] : (options.values ?? []) };
        if (body.sessionAction === "nameCheck") {
          const { candidate, names, replace } = body;
          const same = (a, b) => a.name.toLowerCase() === b.name.toLowerCase() && (a.scope ?? "") === (b.scope ?? "");
          if (/^[A-Za-z]{1,3}\d+$/.test(candidate.name) || /\s/.test(candidate.name)) return { problem: "name", detail: "cellAddress" };
          if (candidate.name.toUpperCase() === "SUM") return { problem: "reserved" };
          if (names.some((n) => same(n, candidate) && !(replace && same(n, replace)))) return { problem: "duplicate" };
          return { problem: null };
        }
        return options.answer?.(body) ?? { open: { session: "s1" }, apply: { session: "s1", changed: [], circular: false }, names: { session: "s1", changed: [], circular: false }, close: { closed: true } }[body.sessionAction] ?? {};
      }
      return { status: "ok" };
    },
  });
  await openDemoWorkbook(harness.document);
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const { document, userClick, host } = harness;
  const viewport = document.getElementById("sheet-viewport");
  viewport.clientHeight = 520;
  viewport.clientWidth = 960;
  await flushAsyncTurns(5);
  const names = host.__EASYOFFICE_SHEET_NAMES__;
  const act = async (name) => {
    userClick(document.querySelector(`.md-item[data-sheet-act="${name}"]`));
    await flushAsyncTurns(20);
  };
  const box = document.getElementById("active-cell-ref");
  const typeInBox = async (text) => {
    box.focus();
    box.textContent = text;
    box.dispatchEvent({ type: "input", target: box });
    box.dispatchEvent({ type: "keydown", key: "Enter", target: box });
    await flushAsyncTurns(30);
  };
  const field = (id) => document.getElementById("sheet-name-edit-" + id);
  const undo = async () => {
    userClick(studioItem(document, "studio-undo"));
    await flushAsyncTurns(20);
  };
  const cell = (ref) => document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  return { ...harness, calls, names, act, box, typeInBox, field, undo, cell, say: () => document.getElementById("status-command-feedback").textContent };
}

const select = (t, ref) => t.cell(ref).focus();

test("the 수식 menu has 이름 정의 and 이름 관리, and both dialogs use the shared frame", async () => {
  const t = await open();
  const menu = [...t.document.querySelectorAll('.menu-item[data-menu="formula"] .md-label')].map((el) => el.textContent);
  assert.deepEqual(menu, ["이름 정의…", "이름 관리…", "선택 영역에서 이름 만들기…"]);
  for (const id of ["sheet-names-dialog", "sheet-name-edit-dialog"]) {
    const dialog = t.document.getElementById(id);
    assert.match(dialog.className, /tool-dialog/);
    assert.ok(dialog.querySelector(".dialog-wrap .dialog-title .dialog-close"), id);
  }
  assert.deepEqual(
    [...t.document.querySelectorAll("#sheet-name-edit-dialog .dialog-label")].map((el) => el.textContent),
    ["이름", "범위", "설명", "참조 영역"]
  );
});

test("이름 정의 opens with the selection as the reference, 확인 defines the name as one undo step", async () => {
  const t = await open();
  select(t, "B2");
  await t.act("name-define");
  assert.equal(t.document.getElementById("sheet-name-edit-dialog").open, true);
  assert.equal(t.field("refers").value, "Sheet1!$B$2");
  assert.equal(t.field("scope").value, "");
  t.field("name").value = "Rate_";
  t.field("comment").value = "세율";
  t.document.getElementById("btn-sheet-name-edit-apply").click();
  await flushAsyncTurns(30);
  assert.equal(t.document.getElementById("sheet-name-edit-dialog").open, false);
  assert.deepEqual(t.names.getAll(), [{ name: "Rate_", refersTo: "Sheet1!$B$2", comment: "세율" }]);
  assert.equal(t.say(), "이름 정의: Rate_");
  await t.undo();
  assert.deepEqual(t.names.getAll(), []);
});

test("a name the engine refuses keeps the dialog open with the reason", async () => {
  const t = await open();
  select(t, "A1");
  await t.act("name-define");
  for (const [name, message] of [["A1", "이름이 올바르지 않음"], ["two words", "이름이 올바르지 않음"], ["sum", "함수나 시트 이름은 쓸 수 없음"], ["", "이름을 입력"]]) {
    t.field("name").value = name;
    t.document.getElementById("btn-sheet-name-edit-apply").click();
    await flushAsyncTurns(30);
    assert.equal(t.document.getElementById("sheet-name-edit-status").textContent, message, name);
    assert.equal(t.document.getElementById("sheet-name-edit-dialog").open, true);
  }
  t.field("name").value = "Fine";
  t.field("refers").value = "";
  t.document.getElementById("btn-sheet-name-edit-apply").click();
  await flushAsyncTurns(30);
  assert.equal(t.document.getElementById("sheet-name-edit-status").textContent, "참조 영역을 입력");
  assert.deepEqual(t.names.getAll(), []);
});

test("이름 관리 lists the names with their scope and reference, edits one and deletes one", async () => {
  const t = await open();
  t.names.replaceAll([
    { name: "Prices", refersTo: "Sheet1!$A$1:$A$3" },
    { name: "Local", scope: "Sheet1", refersTo: "Sheet1!$B$1", comment: "메모" },
    { name: "Secret", refersTo: "1", hidden: true },
  ], true);
  await t.act("name-manage");
  const rows = [...t.document.querySelectorAll("#sheet-names-list li")].map((li) => [...li.querySelectorAll("span")].map((s) => s.textContent));
  assert.deepEqual(rows, [["Prices", "", "Sheet1!$A$1:$A$3", "통합 문서", ""], ["Local", "", "Sheet1!$B$1", "Sheet1", "메모"]]);
  assert.deepEqual([...t.document.querySelectorAll("#sheet-names-dialog .sheet-names-head span")].map((s) => s.textContent), ["이름", "값", "참조 영역", "범위", "설명"]);
  assert.equal(t.document.getElementById("btn-sheet-names-edit").disabled, true);
  t.userClick(t.document.querySelector('#sheet-names-list li[data-index="1"]'));
  assert.equal(t.document.getElementById("btn-sheet-names-edit").disabled, false);
  t.document.getElementById("btn-sheet-names-edit").click();
  await flushAsyncTurns(10);
  assert.equal(t.field("name").value, "Local");
  assert.equal(t.field("scope").value, "Sheet1");
  assert.equal(t.field("comment").value, "메모");
  t.field("refers").value = "Sheet1!$C$1";
  t.document.getElementById("btn-sheet-name-edit-apply").click();
  await flushAsyncTurns(30);
  assert.equal(t.names.getAll()[1].refersTo, "Sheet1!$C$1");
  assert.equal(t.names.getAll().length, 3, "the hidden name stays");
  t.userClick(t.document.querySelector('#sheet-names-list li[data-index="0"]'));
  t.document.getElementById("btn-sheet-names-delete").click();
  await flushAsyncTurns(30);
  const confirm = t.document.getElementById("sheet-name-delete-dialog");
  assert.equal(confirm.open, true, "deleting asks first");
  assert.equal(t.document.getElementById("sheet-name-delete-message").textContent, "Prices 이름을 지울까요?");
  assert.deepEqual([...confirm.querySelectorAll(".dialog-footer .dialog-btn")].map((b) => b.textContent), ["지움", "취소"]);
  confirm.querySelector(".dialog-footer [data-dialog-close]").click();
  await flushAsyncTurns(20);
  assert.equal(t.names.getAll().length, 3, "취소 keeps the name");
  t.document.getElementById("btn-sheet-names-delete").click();
  await flushAsyncTurns(30);
  t.document.getElementById("btn-sheet-name-delete-ok").click();
  await flushAsyncTurns(30);
  assert.deepEqual(t.names.getAll().map((n) => n.name), ["Local", "Secret"]);
  await t.undo();
  assert.deepEqual(t.names.getAll().map((n) => n.name), ["Prices", "Local", "Secret"]);
  assert.equal(t.names.getAll()[1].refersTo, "Sheet1!$C$1", "one undo step undoes the delete only");
});

test("the name box: a typed name selects its range, an address selects that range, Escape gives the address back", async () => {
  const t = await open();
  t.names.replaceAll([{ name: "Block", refersTo: "Sheet1!$B$2:$C$4" }], true);
  select(t, "A1");
  assert.equal(t.box.textContent, "A1");
  await t.typeInBox("block");
  assert.equal(t.box.textContent, "Block", "the selection is exactly the named range");
  assert.equal(t.document.getElementById("status-sheet-agg") !== null, true);
  await t.typeInBox("D5");
  assert.equal(t.box.textContent, "D5");
  await t.typeInBox("e1:f3");
  assert.equal(t.box.textContent, "E1");
  t.box.focus();
  t.box.textContent = "garbage text";
  t.box.dispatchEvent({ type: "keydown", key: "Escape", target: t.box });
  await flushAsyncTurns(10);
  assert.equal(t.box.textContent, "E1");
});

test("the name box shows the name of a definition that refers to exactly the selection", async () => {
  const t = await open();
  t.names.replaceAll([{ name: "Corner", refersTo: "Sheet1!$B$2" }], true);
  await t.typeInBox("B2");
  assert.equal(t.box.textContent, "Corner");
  await t.typeInBox("A2");
  assert.equal(t.box.textContent, "A2");
});

test("a new valid name typed in the name box names the selection as one undo step; a bad one changes nothing", async () => {
  const t = await open();
  select(t, "B2");
  await t.typeInBox("Total_Row");
  assert.deepEqual(t.names.getAll(), [{ name: "Total_Row", refersTo: "Sheet1!$B$2" }]);
  assert.equal(t.box.textContent, "Total_Row");
  await t.undo();
  assert.deepEqual(t.names.getAll(), []);
  await t.typeInBox("B2");
  await t.typeInBox("SUM");
  assert.equal(t.say(), CHECK_MESSAGES.reserved);
  await t.typeInBox("not valid");
  assert.equal(t.say(), CHECK_MESSAGES.name);
  assert.deepEqual(t.names.getAll(), []);
});

test("the arrow opens the list of the names a sheet sees, a click selects, the last entry opens 이름 관리", async () => {
  const t = await open();
  t.names.replaceAll([
    { name: "Block", refersTo: "Sheet1!$B$2:$C$4" },
    { name: "Elsewhere", scope: "Other", refersTo: "Other!$A$1" },
    { name: "Hidden_", refersTo: "Sheet1!$A$1", hidden: true },
    { name: "Single", refersTo: "Sheet1!$E$7" },
  ], true);
  const list = t.document.getElementById("sheet-name-box-list");
  assert.equal(list.hidden, true);
  t.document.getElementById("btn-name-box-list").dispatchEvent({ type: "mousedown", target: t.document.getElementById("btn-name-box-list") });
  assert.equal(list.hidden, false);
  const items = [...list.querySelectorAll("li")];
  assert.deepEqual(items.map((li) => li.querySelector("span")?.textContent ?? li.textContent), ["Block", "Single", "이름 관리…"]);
  items[1].dispatchEvent({ type: "mousedown", target: items[1], bubbles: true });
  await flushAsyncTurns(20);
  assert.equal(list.hidden, true);
  assert.equal(t.box.textContent, "Single");
  t.box.focus();
  t.document.getElementById("btn-name-box-list").dispatchEvent({ type: "mousedown", target: t.document.getElementById("btn-name-box-list") });
  const manage = list.querySelector("li[data-manage]");
  manage.dispatchEvent({ type: "mousedown", target: manage, bubbles: true });
  assert.equal(t.document.getElementById("sheet-names-dialog").open, true);
});

test("a name that is no range says so and selects nothing", async () => {
  const t = await open();
  t.names.replaceAll([{ name: "Pi_", refersTo: "3.14" }], true);
  select(t, "A1");
  await t.typeInBox("Pi_");
  assert.equal(t.say(), "참조 영역이 아닌 이름");
});

// A stand-in for the engine's session commands that keeps what `names` and `apply` send.
function createFakeEngine() {
  const calls = [];
  let nextId = 1;
  const live = new Set();
  async function invokeIpc(command, payload) {
    const body = JSON.parse(payload);
    calls.push(body);
    switch (body.sessionAction) {
      case "open": {
        const id = "s" + nextId++;
        live.add(id);
        return { session: id };
      }
      case "close":
        return { closed: live.delete(body.session) };
      case "names":
        if (!live.has(body.session)) return { expired: true };
        return { session: body.session, changed: [{ sheet: "Sheet1", cell: "B1", value: "6" }], circular: false };
      case "apply":
        if (!live.has(body.session)) return { expired: true };
        return { session: body.session, changed: [], circular: false };
      default:
        return { targetValues: [], values: {}, circular: false };
    }
  }
  return { invokeIpc, calls, live, only: (action) => calls.filter((c) => c.sessionAction === action), order: () => calls.map((c) => c.sessionAction ?? "evaluate") };
}

async function createStore(engine) {
  const { sandbox, host } = await createWorkspaceHarness();
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({ invokeIpc: engine.invokeIpc, normalizeFormattedNumericCell: (v) => v, tokenizeFormulaStringLiterals: () => [] });
  store.enableSheetSessions(true);
  return { store, names: host.__EASYOFFICE_SHEET_NAMES__ };
}

const entry = (value, formula = "") => ({ value, formula });

test("the list goes to the engine session before the cells, once, and again only when it changes", async () => {
  const engine = createFakeEngine();
  const { store, names } = await createStore(engine);
  names.replaceAll([{ name: "Prices", refersTo: "Sheet1!$A$1:$A$3" }], true);
  const sheets = { Sheet1: { A1: entry("1"), B1: entry("", "=SUM(Prices)") } };
  const live = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(engine.order(), ["open", "names", "apply"]);
  assert.deepEqual(engine.only("names")[0].names, [{ name: "Prices", refersTo: "Sheet1!$A$1:$A$3" }]);
  assert.deepEqual(live.targetValues, ["6"], "the value the names answer reports reaches the formula cell");
  sheets.Sheet1.A1 = entry("2");
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.only("names").length, 1, "an unchanged list is not sent again");
  names.replaceAll([{ name: "Prices", refersTo: "Sheet1!$A$1:$A$2" }], true);
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.only("names").length, 2);
  names.replaceAll([], true);
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(engine.only("names")[2].names, [], "a cleared list is sent too, once the session has one");
});

test("a workbook without names never sends the action, and a new session gets the list again", async () => {
  const engine = createFakeEngine();
  const { store, names } = await createStore(engine);
  const sheets = { Sheet1: { A1: entry("1"), B1: entry("", "=A1+1") } };
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.only("names").length, 0);
  names.replaceAll([{ name: "Rate_", refersTo: "0.5" }], true);
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.only("names").length, 1);
  const first = engine.only("open").length;
  engine.live.clear();
  names.replaceAll([{ name: "Rate_", refersTo: "0.5" }, { name: "Two_", refersTo: "2" }], true);
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.only("open").length, first + 1, "the expired session was opened again");
  assert.equal(engine.only("names").at(-1).names.length, 2);
});

test("the stateless command carries the names when there is no session", async () => {
  const engine = createFakeEngine();
  const { sandbox, host } = await createWorkspaceHarness();
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({ invokeIpc: engine.invokeIpc, normalizeFormattedNumericCell: (v) => v, tokenizeFormulaStringLiterals: () => [] });
  store.enableSheetSessions(false);
  host.__EASYOFFICE_SHEET_NAMES__.replaceAll([{ name: "Rate_", refersTo: "0.5" }], true);
  await store.evaluateLiveFormulaTargets({ Sheet1: { A1: entry("1"), B1: entry("", "=A1*Rate_") } }, "Sheet1");
  assert.deepEqual(engine.calls[0].names, [{ name: "Rate_", refersTo: "0.5" }]);
});

test("the save payload carries the names and a reopened file brings them back", async () => {
  const t = await open();
  t.names.replaceAll([{ name: "Prices", refersTo: "Sheet1!$A$1:$A$3", comment: "c" }], true);
  const payload = t.host.__EASYOFFICE_SAVE_PAYLOAD__.buildSheetsPayload(["Sheet1"], { Sheet1: {} }, {}, {}, {});
  assert.ok(Array.isArray(payload));
  const snap = JSON.stringify(t.names.getAll());
  t.names.replaceAll([], true);
  assert.deepEqual(t.names.getAll(), []);
  t.names.replaceAll(JSON.parse(snap), true);
  assert.equal(t.names.payload(), '[{"name":"Prices","refersTo":"Sheet1!$A$1:$A$3","comment":"c"}]');
});

test("the core reads the ranges a name can refer to and writes them back absolute", async () => {
  const t = await open();
  const n = t.names;
  assert.deepEqual(n.parseRange("Sheet1!$A$1:$B$2"), { sheet: "Sheet1", r1: 1, c1: 1, r2: 2, c2: 2 });
  assert.deepEqual(n.parseRange("'My ''Data'''!C3"), { sheet: "My 'Data'", r1: 3, c1: 3, r2: 3, c2: 3 });
  assert.equal(n.parseRange("SUM(Sheet1!A1)"), null);
  assert.equal(n.parseRange("Sheet1!A1,Sheet1!B2"), null);
  assert.equal(n.parseRange("3.14"), null);
  assert.equal(n.refersToOf("My Data", { r1: 2, c1: 2, r2: 4, c2: 3 }), "'My Data'!$B$2:$C$4");
  assert.deepEqual(n.parseAddress("$B$2"), { r1: 2, c1: 2, r2: 2, c2: 2 });
  assert.deepEqual(n.parseAddress("3:5"), { r1: 3, c1: 1, r2: 5, c2: 16384 });
  assert.deepEqual(n.parseAddress("B:D"), { r1: 1, c1: 2, r2: 1048576, c2: 4 });
  assert.equal(n.parseAddress("Total"), null);
  assert.equal(n.parseAddress("SUM"), null, "a lone word is a name, not a column");
  assert.equal(n.limitProblem({ name: "a".repeat(256), refersTo: "1" }), "tooLong");
  assert.equal(n.limitProblem({ name: "a", refersTo: "1", comment: "c".repeat(256) }), "comment");
});

test("이름 관리 shows the values the engine answers, in the order of the list", async () => {
  const t = await open({ values: ["10", "{1,2;3,4}", "1"] });
  t.names.replaceAll([{ name: "A_", refersTo: "Sheet1!$A$1" }, { name: "B_", refersTo: "Sheet1!$A$1:$B$2" }, { name: "H_", refersTo: "1", hidden: true }], true);
  await t.act("name-manage");
  await flushAsyncTurns(30);
  const rows = [...t.document.querySelectorAll("#sheet-names-list li")].map((li) => [...li.querySelectorAll("span")].map((s) => s.textContent));
  assert.deepEqual(rows.map((r) => [r[0], r[1]]), [["A_", "10"], ["B_", "{1,2;3,4}"]]);
  assert.ok(t.calls.some((c) => c.sessionAction === "nameValues"));
});

test("선택 영역에서 이름 만들기 names the rows or columns by their labels, skips what the engine refuses, one undo step", async () => {
  const t = await open();
  const labels = { A1: "수량", B1: "단가", C1: "1x", A2: "1", B2: "2", C2: "3", A3: "4", B3: "5", C3: "6" };
  const store = t.host.__EASYOFFICE_SHEET_NAMES__;
  const out = store.namesFromSelection("Sheet1", { r1: 1, c1: 1, r2: 3, c2: 3 }, { top: true, bottom: false, left: false, right: false }, (r, c) => labels["ABC"[c - 1] + r]);
  assert.deepEqual(out.map((n) => [n.name, n.refersTo]), [["수량", "Sheet1!$A$2:$A$3"], ["단가", "Sheet1!$B$2:$B$3"], ["1x", "Sheet1!$C$2:$C$3"]]);
  const both = store.namesFromSelection("Sheet1", { r1: 1, c1: 1, r2: 3, c2: 3 }, { top: true, bottom: false, left: true, right: false }, (r, c) => labels["ABC"[c - 1] + r]);
  assert.deepEqual(both.map((n) => n.refersTo), ["Sheet1!$B$2:$B$3", "Sheet1!$C$2:$C$3", "Sheet1!$B$2:$C$2", "Sheet1!$B$3:$C$3"]);
  assert.deepEqual(store.namesFromSelection("Sheet1", { r1: 1, c1: 1, r2: 1, c2: 3 }, { top: true, bottom: false, left: false, right: false }, () => "x"), [], "no rows below the labels");
  await t.typeInBox("A1:C2");
  await t.act("name-create");
  const dialog = t.document.getElementById("sheet-name-create-dialog");
  assert.equal(dialog.open, true);
  assert.deepEqual([...dialog.querySelectorAll(".dialog-checkbox")].map((l) => l.textContent.trim()), ["첫 행", "마지막 행", "왼쪽 열", "오른쪽 열"]);
  assert.equal(t.document.getElementById("sheet-name-create-top").checked, true, "a first row is the guess");
  t.document.getElementById("sheet-name-create-top").checked = false;
  t.document.getElementById("btn-sheet-name-create-apply").click();
  await flushAsyncTurns(30);
  assert.equal(dialog.open, true, "no edge chosen, no name to make");
  assert.equal(t.document.getElementById("sheet-name-create-status").textContent, "만들 이름이 없음");
  assert.deepEqual(t.names.getAll(), []);
});

test("the buttons and titles are the spreadsheet help's: 이름 추가, 이름 편집, 이름 삭제, 닫기; 이름 정의 from the menu", async () => {
  const t = await open();
  t.names.replaceAll([{ name: "A_", refersTo: "Sheet1!$A$1" }], true);
  await t.act("name-manage");
  const dialog = t.document.getElementById("sheet-names-dialog");
  assert.deepEqual([...dialog.querySelectorAll(".dialog-footer .dialog-btn")].map((b) => b.textContent), ["이름 추가", "이름 편집", "이름 삭제", "닫기"]);
  const title = () => t.document.getElementById("sheet-name-edit-dialog-title").textContent;
  t.document.getElementById("btn-sheet-names-new").click();
  await flushAsyncTurns(10);
  assert.equal(title(), "이름 추가");
  t.document.getElementById("sheet-name-edit-dialog").close();
  t.userClick(t.document.querySelector('#sheet-names-list li[data-index="0"]'));
  t.document.getElementById("btn-sheet-names-edit").click();
  await flushAsyncTurns(10);
  assert.equal(title(), "이름 편집");
  t.document.getElementById("sheet-name-edit-dialog").close();
  await t.act("name-define");
  assert.equal(title(), "이름 정의");
  assert.equal(t.document.getElementById("btn-sheet-name-edit-apply").textContent, "확인");
});
