import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The sheet tabs (PD-209): insert, delete, rename, copy, move, hide and show again, each one undo
// step; the references to a renamed or deleted sheet come from the engine (`sheetRefs`, stood in
// for here by a small rewriter); the order and the hidden flag reach the save request.

const SHEET = "[A-Za-z_][A-Za-z0-9_]*";
const quoted = (name) => (new RegExp(`^${SHEET}$`).test(name) && !/^[A-Za-z]{1,3}\d+$/.test(name) ? name : `'${name.replaceAll("'", "''")}'`);

// What the engine does with a rename or a deletion, for the references these tests write.
function rewrite(raw, op, sheet, newName) {
  const old = `(?:'${sheet.replaceAll("'", "''")}'|${sheet})`;
  const unit = String.raw`\$?[A-Z]+\$?\d+(?::\$?[A-Z]+\$?\d+)?`;
  if (op === "delete") return raw.replace(new RegExp(`${old}!${unit}`, "g"), "#REF!");
  return raw.replace(new RegExp(`${old}!`, "g"), `${quoted(newName)}!`);
}

function fakeEngine() {
  const requests = [];
  const state = { refuse: "" };
  const cells = new Map();
  let sessions = 0;
  async function evaluate(body) {
    requests.push(body);
    switch (body.sessionAction) {
      case "open":
        cells.clear();
        sessions += 1;
        return { session: "s" + sessions };
      case "apply":
        for (const c of body.changes) {
          const key = c.sheet + "!" + c.cell;
          if (c.value === "") cells.delete(key);
          else cells.set(key, c.value);
        }
        return { session: body.session, changed: [], circular: false };
      case "sheetRefs": {
        if (state.refuse) throw new Error(state.refuse);
        if (body.op === "rename" && /[\\/?*[\]:]/.test(body.newName)) throw new Error("시트 이름에 쓸 수 없는 글자입니다");
        const formulas = [];
        for (const [key, raw] of cells) {
          const [sheet, cell] = key.split("!");
          if (body.op === "delete" && sheet === body.sheet) continue;
          if (!raw.startsWith("=")) continue;
          const next = rewrite(raw, body.op, body.sheet, body.newName);
          if (next !== raw) formulas.push({ sheet, cell, value: next });
        }
        const rules = {};
        for (const [sheet, r] of Object.entries(body.rules || {})) {
          const text = JSON.stringify(r);
          const next = rewrite(text, body.op, body.sheet, body.newName);
          if (next !== text) rules[sheet] = JSON.parse(next);
        }
        return { session: body.session, formulas, rules };
      }
      default:
        return {};
    }
  }
  return { requests, evaluate, state };
}

async function open() {
  const engine = fakeEngine();
  const converts = [];
  let model = { sheets: [] };
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") return engine.evaluate(JSON.parse(args.payload));
      if (command === "convert_document_format") {
        const body = JSON.parse(args.payload);
        if (body.action === "decode") {
          return JSON.stringify({ status: "OK", module: "SHEET", format: "xlsx", kind: "workbook", itemCount: model.sheets.length, warnings: [], ...model });
        }
        converts.push(body);
        return JSON.stringify({ status: "OK", convertedBase64: "", fileName: "x.xlsx" });
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const { document, userClick } = harness;
  // The demo page carries a second tab of its own; the app's page has one tab.
  harness.host.__EASYOFFICE_SHEET_TAB_STRIP__.render({ document, keys: ["Sheet1"], active: "Sheet1" });
  const tabs = () => [...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")];
  const names = () => tabs().map((t) => t.dataset.sheetTab);
  const labels = () => tabs().map((t) => t.textContent);
  const active = () => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab;
  const tab = (name) => tabs().find((t) => t.dataset.sheetTab === name);
  const status = () => document.getElementById("status-command-feedback").textContent;
  const menu = () => document.getElementById("sheet-tab-menu");
  const history = async (cmd) => {
    userClick(studioItem(document, cmd));
    await flushAsyncTurns(30);
  };
  // The tab's menu item of `act`.
  const tabMenu = async (name, act) => {
    const t = tab(name);
    t.dispatchEvent({ type: "contextmenu", target: t, clientX: 40, clientY: 700 });
    const item = menu().querySelector(`[data-tab-act="${act}"]`);
    userClick(item);
    await flushAsyncTurns(30);
    return item;
  };
  const add = async () => {
    userClick(document.getElementById("btn-add-sheet-tab"));
    await flushAsyncTurns(30);
  };
  // Opens a workbook of `sheets` (`{ name, cells: { ref: raw }, ...layout }`) as a file would.
  const openBook = async (sheets) => {
    model = { sheets: sheets.map(({ name, cells, ...rest }) => ({ name, cells: Object.entries(cells || {}).map(([ref, raw]) => ({ ref, raw })), ...rest })) };
    const bytes = new TextEncoder().encode("book");
    const result = await harness.sandbox.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("book.xlsx", bytes);
    await flushAsyncTurns(40);
    assert.equal(result.ok, true);
  };
  // What a save would send: the sheets in tab order with their cells as `{ ref: raw }`.
  const saved = async () => {
    userClick(studioItem(document, "studio-save"));
    await flushAsyncTurns(40);
    return converts.at(-1).sheets.map((sheet) => ({ ...sheet, cells: Object.fromEntries(sheet.cells.map((c) => [c.ref, c.raw])) }));
  };
  const book = async () => Object.fromEntries((await saved()).map((sheet) => [sheet.name, sheet]));
  return { ...harness, engine, converts, tabs, names, labels, active, tab, status, menu, tabMenu, add, history, openBook, saved, book };
}

test("a new sheet is named after the numbers in use and goes after the last tab", async () => {
  const h = await open();
  await h.add();
  await h.add();
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2", "Sheet3"]);
  assert.deepEqual(h.labels(), ["시트 1", "시트 2", "시트 3"]);
  assert.equal(h.active(), "Sheet3");
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2"]);
  assert.equal(h.active(), "Sheet2");
  await h.history("studio-redo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2", "Sheet3"]);
  // Deleting the middle one leaves "Sheet3" in use, so the next one is "Sheet4".
  await h.tabMenu("Sheet2", "delete");
  userClickOk(h, "sheet-tab-delete-dialog");
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet1", "Sheet3"]);
  await h.add();
  assert.deepEqual(h.names(), ["Sheet1", "Sheet3", "Sheet4"]);
});

function userClickOk(h, dialogId) {
  const dialog = h.document.getElementById(dialogId);
  h.userClick(dialog.querySelector(".dialog-btn-primary"));
}

test("the menu inserts a sheet before the tab it was opened on and shows the new one", async () => {
  const h = await open();
  await h.add();
  await h.add();
  await h.tabMenu("Sheet2", "insert");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet4", "Sheet2", "Sheet3"]);
  assert.equal(h.active(), "Sheet4");
  assert.equal(h.menu().hidden, true);
  assert.match(h.status(), /시트 삽입: 시트 4/);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2", "Sheet3"]);
  assert.equal(h.active(), "Sheet3");
});

test("a sheet is not inserted past the number a save takes", async () => {
  const h = await open();
  await h.openBook(Array.from({ length: 256 }, (_, i) => ({ name: "S" + (i + 1), cells: {} })));
  assert.equal(h.tabs().length, 256);
  await h.add();
  assert.equal(h.tabs().length, 256);
  assert.match(h.status(), /시트는 256개까지/);
  const copy = await h.tabMenu("S1", "copy");
  assert.equal(copy.getAttribute("aria-disabled"), "true");
  const insert = await h.tabMenu("S1", "insert");
  assert.equal(insert.getAttribute("aria-disabled"), "true");
});

test("deleting asks first, turns the references to the sheet into #REF!, and one undo brings it all back", async () => {
  const h = await open();
  await h.openBook([
    { name: "첫째", cells: { A1: "7", B1: "=둘째!A1+1" }, tabColor: "3366cc" },
    { name: "둘째", cells: { A1: "5", B1: "=SUM(첫째!A1:A3)" }, tabColor: "cc3333", freeze: { rows: 1, cols: 0 } },
  ]);
  await h.tabMenu("둘째", "delete");
  const dialog = h.document.getElementById("sheet-tab-delete-dialog");
  assert.equal(dialog.open, true);
  assert.equal(h.document.getElementById("sheet-tab-delete-message").textContent, "'둘째' 시트를 삭제하시겠습니까?");
  // Cancel changes nothing.
  h.userClick(dialog.querySelector("[data-dialog-close].dialog-btn"));
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["첫째", "둘째"]);
  await h.tabMenu("둘째", "delete");
  userClickOk(h, "sheet-tab-delete-dialog");
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["첫째"]);
  assert.equal(h.active(), "첫째");
  assert.match(h.status(), /시트 삭제: 둘째/);
  let book = await h.book();
  assert.deepEqual(Object.keys(book), ["첫째"], "the sheet and every store of it are gone");
  assert.equal(book["첫째"].cells.B1, "=#REF!+1");
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.tabColorOf("둘째"), null);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["첫째", "둘째"]);
  book = await h.book();
  assert.equal(book["첫째"].cells.B1, "=둘째!A1+1");
  assert.equal(book["둘째"].cells.B1, "=SUM(첫째!A1:A3)");
  assert.equal(book["둘째"].tabColor, "cc3333");
  assert.deepEqual(book["둘째"].freeze, { rows: 1, cols: 0 });
  await h.history("studio-redo");
  assert.deepEqual(h.names(), ["첫째"]);
});

test("the last shown sheet cannot be deleted or hidden", async () => {
  const h = await open();
  for (const act of ["delete", "hide"]) {
    const item = await h.tabMenu("Sheet1", act);
    assert.equal(item.getAttribute("aria-disabled"), "true", act);
  }
  assert.deepEqual(h.names(), ["Sheet1"]);
  assert.notEqual(h.document.getElementById("sheet-tab-delete-dialog").open, true);
  // The sheet menu's command says why.
  userClickMenuAct(h, "tab-delete");
  await flushAsyncTurns(30);
  assert.match(h.status(), /마지막 시트는 삭제할 수 없음/);
  userClickMenuAct(h, "tab-hide");
  await flushAsyncTurns(30);
  assert.match(h.status(), /모든 시트를 숨길 수 없음/);
});

function userClickMenuAct(h, act) {
  h.userClick(h.document.querySelector(`.md-item[data-sheet-act="${act}"]`));
}

test("renaming types the name in the tab, rewrites the formulas that name it and follows every store", async () => {
  const h = await open();
  await h.openBook([
    { name: "Sheet1", cells: { A1: "7", B1: "=Sheet2!A1+Sheet2!B1" } },
    { name: "Sheet2", cells: { A1: "5" }, tabColor: "3366cc", freeze: { rows: 1, cols: 0 }, columns: [{ index: 2, widthPx: 140 }] },
  ]);
  await h.tabMenu("Sheet1", "insert");
  await h.history("studio-undo");
  const { document, host } = h;
  const t = h.tab("Sheet2");
  t.dispatchEvent({ type: "dblclick", target: t });
  const input = document.querySelector("#sheet-tab-bar .sheet-tab-edit");
  assert.equal(input.value, "시트 2", "the tab shows the page's label, which is what is edited");
  assert.equal(input.maxLength, 31);
  assert.equal(t.hidden, true);
  input.value = "월별 합계";
  input.dispatchEvent({ type: "keydown", key: "Enter", target: input });
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet1", "월별 합계"]);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-edit"), null);
  let book = await h.book();
  assert.equal(book.Sheet1.cells.B1, "='월별 합계'!A1+'월별 합계'!B1");
  assert.equal(book["월별 합계"].cells.A1, "5");
  assert.equal(book.Sheet2, undefined);
  assert.equal(book["월별 합계"].tabColor, "3366cc");
  assert.deepEqual(book["월별 합계"].freeze, { rows: 1, cols: 0 });
  assert.deepEqual(book["월별 합계"].columns, [{ index: 2, widthPx: 140 }]);
  assert.equal(h.active(), "Sheet1");
  assert.deepEqual(h.engine.requests.filter((r) => r.sessionAction === "sheetRefs").map((r) => [r.op, r.sheet, r.newName]), [["rename", "Sheet2", "월별 합계"]]);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2"]);
  book = await h.book();
  assert.equal(book.Sheet1.cells.B1, "=Sheet2!A1+Sheet2!B1");
  assert.equal(book.Sheet2.tabColor, "3366cc");
  await h.history("studio-redo");
  assert.deepEqual(h.names(), ["Sheet1", "월별 합계"]);
  assert.ok(host);
});

test("the active sheet keeps being active under its new name", async () => {
  const h = await open();
  await h.add();
  assert.equal(h.active(), "Sheet2");
  const t = h.tab("Sheet2");
  t.dispatchEvent({ type: "dblclick", target: t });
  const input = h.document.querySelector("#sheet-tab-bar .sheet-tab-edit");
  input.value = "둘째";
  input.dispatchEvent({ type: "keydown", key: "Enter", target: input });
  await flushAsyncTurns(30);
  assert.equal(h.active(), "둘째");
  assert.equal(h.document.activeElement, h.tab("둘째"), "the focus goes to the renamed tab");
});

test("a name that breaks a rule keeps the tab in editing with the reason, Escape leaves it and blur drops it", async () => {
  const h = await open();
  await h.add();
  const { document } = h;
  const start = () => {
    const t = h.tab("Sheet2");
    t.focus();
    t.dispatchEvent({ type: "keydown", key: "F2", target: t });
    return document.querySelector("#sheet-tab-bar .sheet-tab-edit");
  };
  const press = (input, key) => input.dispatchEvent({ type: "keydown", key, target: input });
  let input = start();
  const bad = [
    ["", /시트 이름을 입력/],
    ["가".repeat(32), /31자까지/],
    ["a/b", /쓸 수 없는 글자: \//],
    ["'a", /작은따옴표/],
    ["history", /예약된 시트 이름/],
    ["시트 1", /이미 있는 시트 이름/],
    ["SHEET1", /이미 있는 시트 이름/],
  ];
  for (const [text, reason] of bad) {
    input.value = text;
    press(input, "Enter");
    await flushAsyncTurns(10);
    assert.match(h.status(), reason, JSON.stringify(text));
    assert.equal(input.getAttribute("aria-invalid"), "true");
    assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-edit"), input, "still editing");
  }
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2"]);
  press(input, "Escape");
  await flushAsyncTurns(10);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-edit"), null);
  assert.equal(h.tab("Sheet2").hidden, false);
  // Blur with a bad name drops the edit and says why; with the unchanged label it changes nothing.
  input = start();
  input.value = "a?b";
  input.blur();
  await flushAsyncTurns(10);
  assert.equal(document.querySelector("#sheet-tab-bar .sheet-tab-edit"), null);
  assert.match(h.status(), /쓸 수 없는 글자/);
  input = start();
  press(input, "Enter");
  await flushAsyncTurns(10);
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2"]);
  assert.equal(h.engine.requests.some((r) => r.sessionAction === "sheetRefs"), false);
  // A name another sheet shows as its label is taken: "시트 1" is Sheet1's.
  input = start();
  input.value = "시트 1";
  press(input, "Enter");
  assert.match(h.status(), /이미 있는 시트 이름/);
});

test("a rename needs no engine when nothing names a sheet, and what the engine refuses changes nothing", async () => {
  const h = await open();
  await h.openBook([{ name: "Sheet1", cells: { A1: "x" } }]);
  await h.add();
  const t = h.tab("Sheet2");
  t.dispatchEvent({ type: "dblclick", target: t });
  const input = h.document.querySelector("#sheet-tab-bar .sheet-tab-edit");
  input.value = "둘째";
  input.dispatchEvent({ type: "keydown", key: "Enter", target: input });
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet1", "둘째"]);
  assert.equal(h.engine.requests.some((r) => r.sessionAction === "sheetRefs"), false);
});

test("with a formula in the book the engine is asked, and a name it refuses leaves the sheets as they were", async () => {
  const h = await open();
  await h.openBook([{ name: "A", cells: { A1: "=B!A1" } }, { name: "B", cells: {} }]);
  const rename = async (name) => {
    const t = h.tab("B");
    t.dispatchEvent({ type: "dblclick", target: t });
    const edit = h.document.querySelector("#sheet-tab-bar .sheet-tab-edit");
    edit.value = name;
    edit.dispatchEvent({ type: "keydown", key: "Enter", target: edit });
    await flushAsyncTurns(30);
  };
  h.engine.state.refuse = "이미 있는 시트 이름입니다.";
  await rename("fine");
  assert.equal(h.engine.requests.filter((r) => r.sessionAction === "sheetRefs").length, 1);
  assert.match(h.status(), /이미 있는 시트 이름입니다/);
  assert.deepEqual(h.names(), ["A", "B"]);
  assert.equal(h.document.querySelector("#sheet-tab-bar .sheet-tab-edit"), null);
  assert.deepEqual((await h.saved()).map((sheet) => sheet.cells.A1), ["=B!A1", undefined]);
  h.engine.state.refuse = "";
  await rename("fine");
  assert.deepEqual(h.names(), ["A", "fine"]);
  assert.equal((await h.saved())[0].cells.A1, "=fine!A1");
});

test("a copy goes right after its source, keeps formulas as written and takes the layout and the rules", async () => {
  const h = await open();
  await h.openBook([
    { name: "Sheet1", cells: { A1: "7", B1: "=A1+Sheet2!A1+Sheet1!A1" }, tabColor: "3366cc", merges: ["A3:B4"] },
    { name: "Sheet2", cells: { A1: "5" } },
  ]);
  await h.tabMenu("Sheet1", "copy");
  assert.deepEqual(h.names(), ["Sheet1", "시트 1 (2)", "Sheet2"]);
  assert.equal(h.active(), "시트 1 (2)");
  const book = await h.book();
  assert.equal(book["시트 1 (2)"].cells.B1, "=A1+Sheet2!A1+Sheet1!A1", "text kept: the bare reference now points into the copy, the named ones where they did");
  assert.deepEqual(book.Sheet1.cells, { A1: "7", B1: "=A1+Sheet2!A1+Sheet1!A1" });
  assert.equal(book["시트 1 (2)"].tabColor, "3366cc");
  assert.deepEqual(book["시트 1 (2)"].merges, ["A3:B4"]);
  await h.tabMenu("Sheet1", "copy");
  assert.deepEqual(h.names(), ["Sheet1", "시트 1 (3)", "시트 1 (2)", "Sheet2"]);
  await h.history("studio-undo");
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2"]);
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.get("시트 1 (2)"), null);
});

test("a copy takes the tables of its source under names of their own and a save sends all of them (PD-224)", async () => {
  const h = await open();
  const table = (name, range) => ({ name, range, header: true, totals: false, columns: ["a", "b"], rowStripes: true, columnStripes: false, firstColumn: false, lastColumn: false });
  await h.openBook([
    { name: "Sheet1", cells: { A1: "a", B1: "b" }, tables: [table("Sales", "A1:B4")] },
    { name: "Sheet2", cells: {}, tables: [table("Other", "C1:D3")] },
  ]);
  let book = await h.book();
  assert.deepEqual(book.Sheet1.tables.map((t) => t.name), ["Sales"]);
  assert.deepEqual(book.Sheet2.tables.map((t) => t.name), ["Other"]);
  await h.tabMenu("Sheet1", "copy");
  book = await h.book();
  assert.deepEqual(book["시트 1 (2)"].tables.map((t) => [t.name, t.range]), [["Sales_2", "A1:B4"]]);
  assert.deepEqual(book.Sheet1.tables.map((t) => t.name), ["Sales"], "the source keeps its name");
  await h.history("studio-undo");
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.get("시트 1 (2)"), null);
  assert.deepEqual((await h.book()).Sheet1.tables.map((t) => t.name), ["Sales"]);
  // Deleting a sheet takes its tables with it; undo brings them back.
  await h.tabMenu("Sheet2", "delete");
  userClickOk(h, "sheet-tab-delete-dialog");
  await flushAsyncTurns(30);
  assert.equal((await h.book()).Sheet2, undefined);
  await h.history("studio-undo");
  assert.deepEqual((await h.book()).Sheet2.tables.map((t) => t.name), ["Other"]);
});

test("a copy of a long name is cut to fit the name length", async () => {
  const h = await open();
  const long = "가".repeat(31);
  await h.openBook([{ name: long, cells: { A1: "x" } }, { name: "다른", cells: {} }]);
  await h.tabMenu(long, "copy");
  const copyName = h.names()[1];
  assert.equal(h.names()[0], long);
  assert.ok(copyName.length <= 31 && copyName.endsWith(" (2)"), copyName);
  assert.equal(copyName, "가".repeat(27) + " (2)");
  await h.tabMenu(long, "copy");
  assert.equal(h.names()[1], "가".repeat(27) + " (3)");
});

test("moving a tab left or right swaps it with the shown tab next to it; the ends cannot move further", async () => {
  const h = await open();
  await h.add();
  await h.add();
  assert.equal((await h.tabMenu("Sheet1", "move-left")).getAttribute("aria-disabled"), "true");
  assert.equal((await h.tabMenu("Sheet3", "move-right")).getAttribute("aria-disabled"), "true");
  await h.tabMenu("Sheet1", "move-right");
  assert.deepEqual(h.names(), ["Sheet2", "Sheet1", "Sheet3"]);
  assert.equal(h.active(), "Sheet3", "moving does not change the active sheet");
  await h.tabMenu("Sheet3", "move-left");
  assert.deepEqual(h.names(), ["Sheet2", "Sheet3", "Sheet1"]);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet2", "Sheet1", "Sheet3"]);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2", "Sheet3"]);
  // A name that looks like a number keeps its place (an object's keys would sort it first).
  await h.tabMenu("Sheet3", "move-left");
  const t = h.tab("Sheet1");
  t.dispatchEvent({ type: "dblclick", target: t });
  const input = h.document.querySelector("#sheet-tab-bar .sheet-tab-edit");
  input.value = "2024";
  input.dispatchEvent({ type: "keydown", key: "Enter", target: input });
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["2024", "Sheet3", "Sheet2"]);
});

test("dragging a tab past its neighbours moves the sheet, and the click that ends the drag does not switch tabs", async () => {
  const h = await open();
  await h.add();
  await h.add();
  const { document } = h;
  // Three tabs of 80px side by side.
  h.tabs().forEach((t, i) => {
    t.getBoundingClientRect = () => ({ left: 10 + i * 80, top: 0, width: 80, height: 24 });
  });
  const first = h.tab("Sheet1");
  first.dispatchEvent({ type: "mousedown", target: first, clientX: 40, button: 0 });
  // Under the threshold it is a click, not a drag.
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 42 });
  assert.equal(first.classList.contains("dragging"), false);
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 150 });
  assert.equal(first.classList.contains("dragging"), true);
  assert.equal(first.style.transform, "translateX(110px)");
  assert.equal(h.tab("Sheet2").style.transform, "translateX(-80px)", "the tab it passed makes room");
  assert.equal(h.tab("Sheet3").style.transform, "", "the tab it did not reach stays");
  document.dispatchEvent({ type: "mouseup", target: document.body, clientX: 150 });
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet2", "Sheet1", "Sheet3"]);
  assert.equal(h.active(), "Sheet3");
  assert.ok(h.tabs().every((t) => t.style.transform === "" && !t.classList.contains("dragging")));
  // The click that ends a drag is swallowed once.
  h.tab("Sheet1").click();
  await flushAsyncTurns(30);
  assert.equal(h.active(), "Sheet3");
  h.tab("Sheet1").click();
  await flushAsyncTurns(30);
  assert.equal(h.active(), "Sheet1");
  // A drag that ends where it began moves nothing and adds no undo step.
  h.tabs().forEach((t, i) => {
    t.getBoundingClientRect = () => ({ left: 10 + i * 80, top: 0, width: 80, height: 24 });
  });
  const before = h.names();
  const second = h.tab("Sheet2");
  second.dispatchEvent({ type: "mousedown", target: second, clientX: 40, button: 0 });
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 60 });
  document.dispatchEvent({ type: "mouseup", target: document.body, clientX: 60 });
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), before);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2", "Sheet3"], "the one undo step is the first drag's");
});

test("hiding takes the tab away and keeps the sheet; the list brings it back, and each is one undo step", async () => {
  const h = await open();
  await h.openBook([{ name: "하나", cells: {} }, { name: "둘", cells: { A1: "hidden data" } }, { name: "셋", cells: {} }]);
  await h.tabMenu("둘", "hide");
  assert.deepEqual(h.names(), ["하나", "셋"]);
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.hiddenOf("둘"), true);
  assert.match(h.status(), /시트 숨김: 둘/);
  // The order keeps the hidden sheet where it was, and the save request carries both.
  const sent = await h.saved();
  assert.deepEqual(sent.map((s) => s.name), ["하나", "둘", "셋"]);
  assert.deepEqual(sent.map((s) => s.hidden === true), [false, true, false]);
  assert.equal(sent[1].cells.A1, "hidden data");
  // The list shows what is hidden by its label; the first row is picked at first.
  assert.equal((await h.tabMenu("하나", "unhide")).getAttribute("aria-disabled"), null);
  const dialog = h.document.getElementById("sheet-tab-unhide-dialog");
  assert.equal(dialog.open, true);
  const rows = [...h.document.querySelectorAll("#sheet-tab-unhide-list li")];
  assert.deepEqual(rows.map((r) => r.textContent), ["둘"]);
  assert.equal(rows[0].getAttribute("aria-selected"), "true");
  h.userClick(dialog.querySelector(".dialog-btn-primary"));
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["하나", "둘", "셋"]);
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.hiddenOf("둘"), false);
  assert.equal((await h.tabMenu("하나", "unhide")).getAttribute("aria-disabled"), "true", "nothing is hidden now");
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["하나", "셋"]);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["하나", "둘", "셋"]);
});

function userClickStudioSave(h) {
  h.userClick(studioItem(h.document, "studio-save"));
}

test("hiding the active sheet shows its neighbour, hidden sheets are skipped by moves, and a list can bring back several", async () => {
  const h = await open();
  await h.openBook(["하나", "둘", "셋", "넷"].map((name) => ({ name, cells: {} })));
  await h.tabMenu("넷", "hide");
  await h.tabMenu("하나", "move-right");
  assert.deepEqual(h.names(), ["둘", "하나", "셋"]);
  await h.tabMenu("둘", "hide");
  assert.deepEqual(h.names(), ["하나", "셋"]);
  assert.equal(h.active(), "하나");
  // Moving 셋 left passes the hidden 둘 and lands before 하나.
  await h.tabMenu("셋", "move-left");
  assert.deepEqual(h.names(), ["셋", "하나"]);
  assert.deepEqual((await h.saved()).map((s) => s.name), ["둘", "셋", "하나", "넷"], "the hidden sheets stay where they were");
  await h.tabMenu("하나", "unhide");
  const rows = [...h.document.querySelectorAll("#sheet-tab-unhide-list li")];
  assert.deepEqual(rows.map((r) => r.textContent).sort((a, b) => a.localeCompare(b)), ["넷", "둘"]);
  rows[1].dispatchEvent({ type: "click", target: rows[1], ctrlKey: true });
  assert.deepEqual(rows.map((r) => r.getAttribute("aria-selected")), ["true", "true"]);
  h.userClick(h.document.querySelector("#sheet-tab-unhide-dialog .dialog-btn-primary"));
  await flushAsyncTurns(30);
  assert.equal(h.tabs().length, 4);
  assert.match(h.status(), /시트 숨기기 취소: /);
});

test("the hiding of the active sheet shows the sheet after it, else the one before", async () => {
  const h = await open();
  await h.add();
  await h.add();
  await h.tabMenu("Sheet3", "hide");
  assert.equal(h.active(), "Sheet2");
  await h.tabMenu("Sheet2", "hide");
  assert.equal(h.active(), "Sheet1");
});

test("closing the hidden-sheet list any other way picks nothing", async () => {
  const h = await open();
  await h.add();
  await h.tabMenu("Sheet1", "hide");
  await h.tabMenu("Sheet2", "unhide");
  const dialog = h.document.getElementById("sheet-tab-unhide-dialog");
  h.userClick(dialog.querySelector("[data-dialog-close].dialog-btn"));
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet2"]);
  assert.equal(dialog.open, false);
});

test("the sheet menu runs the same commands on the active sheet", async () => {
  const h = await open();
  userClickMenuAct(h, "tab-insert");
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet2", "Sheet1"], "inserted before the active sheet");
  userClickMenuAct(h, "tab-copy");
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet2", "Sheet1", "시트 1 (2)"].length === 3 ? h.names() : []);
  assert.equal(h.tabs().length, 3);
  userClickMenuAct(h, "tab-left");
  await flushAsyncTurns(30);
  userClickMenuAct(h, "tab-hide");
  await flushAsyncTurns(30);
  assert.equal(h.tabs().length, 2);
  userClickMenuAct(h, "tab-rename");
  await flushAsyncTurns(30);
  assert.ok(h.document.querySelector("#sheet-tab-bar .sheet-tab-edit"));
});

test("the menu is greyed per tab and its keys skip what cannot run", async () => {
  const h = await open();
  await h.add();
  const t = h.tab("Sheet1");
  t.dispatchEvent({ type: "contextmenu", target: t, clientX: 40, clientY: 700 });
  const off = [...h.menu().querySelectorAll(".md-item")].filter((el) => el.getAttribute("aria-disabled") === "true").map((el) => el.dataset.tabAct);
  assert.deepEqual(off, ["move-left", "unhide"]);
  const focused = () => h.document.activeElement.dataset.tabAct;
  assert.equal(focused(), "insert");
  const press = (key) => h.menu().dispatchEvent({ type: "keydown", key, target: h.document.activeElement });
  press("End");
  assert.equal(focused(), "hide", "unhide is greyed and skipped");
  press("ArrowUp");
  assert.equal(focused(), "move-right");
  press("ArrowUp");
  assert.equal(h.document.activeElement.dataset.tabSub, "color", "move-left is skipped");
  press("ArrowUp");
  assert.equal(focused(), "move-copy");
  press("Escape");
  assert.equal(h.menu().hidden, true);
});

test("a file opens in its own order on its first shown sheet, and the order and the hidden flag come back out of a save", async () => {
  const h = await open();
  await h.openBook([
    { name: "숨김", cells: { A1: "h" }, hidden: true },
    { name: "2024", cells: { A1: "a" } },
    { name: "10", cells: { A1: "b" } },
    { name: "가", cells: { A1: "c" }, hidden: true },
    { name: "Sheet9", cells: { A1: "d" } },
  ]);
  assert.deepEqual(h.names(), ["2024", "10", "Sheet9"], "numeric names keep the file's order, not an object's");
  assert.deepEqual(h.labels(), ["2024", "10", "시트 9"]);
  assert.equal(h.active(), "2024", "the hidden first sheet is not the one shown");
  const sent = await h.saved();
  assert.deepEqual(sent.map((s) => [s.name, s.hidden === true]), [["숨김", true], ["2024", false], ["10", false], ["가", true], ["Sheet9", false]]);
  assert.deepEqual(sent.map((s) => s.cells.A1), ["h", "a", "b", "c", "d"]);
});

// A step of a sheet's name or existence puts the sheet back with everything its layout holds, and a protection (with the
// digest of its password) is never lost or lifted by it (PD-216, PD-221).
test("renaming and deleting a protected sheet, then undo and redo, keep its protection, password digest and layout", async () => {
  const h = await open();
  await h.add();
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const pwd = { kind: "both", alg: "SHA-512", hash: "OFFLINE-1-1", salt: "S", spin: 1, key: "K" };
  const protect = { on: true, deny: ["formatCells"], ranges: [{ name: "r", sqref: ["B2:B3"], pwd }], pwd };
  layout.setProtect("Sheet2", protect);
  layout.setFreeze("Sheet2", { rows: 1, cols: 0 });
  layout.setPage("Sheet2", { orientation: "landscape" });
  layout.setHidden("Sheet2", "row", 3, 4, true);
  layout.addMerge("Sheet2", 1, 1, 2, 2);
  layout.setFilter("Sheet2", { range: { r1: 5, c1: 1, r2: 9, c2: 3 }, columns: [], hidden: [] });
  const kept = structuredClone(layout.getAll());
  assert.deepEqual(layout.protectOf("Sheet2"), protect);
  const same = (label) => {
    assert.deepEqual(layout.getAll(), kept, label);
    assert.deepEqual(layout.protectOf("Sheet2"), protect, label + ": the protection and the digest of the password");
  };
  const t = h.tab("Sheet2");
  t.dispatchEvent({ type: "dblclick", target: t });
  const input = h.document.querySelector("#sheet-tab-bar .sheet-tab-edit");
  input.value = "둘째";
  input.dispatchEvent({ type: "keydown", key: "Enter", target: input });
  await flushAsyncTurns(30);
  assert.deepEqual(layout.protectOf("둘째"), protect, "the rename carries the protection");
  assert.equal(layout.protectOf("Sheet2"), null);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2"]);
  same("undo of the rename");
  await h.history("studio-redo");
  assert.deepEqual(layout.protectOf("둘째"), protect, "redo of the rename");
  assert.equal(layout.protectOf("Sheet2"), null);
  await h.history("studio-undo");
  same("and the undo again");

  await h.tabMenu("Sheet2", "delete");
  userClickOk(h, "sheet-tab-delete-dialog");
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet1"]);
  assert.equal(layout.protectOf("Sheet2"), null, "the deleted sheet has none");
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2"]);
  same("undo of the delete");
  await h.history("studio-redo");
  assert.deepEqual(h.names(), ["Sheet1"]);
  await h.history("studio-undo");
  same("and the undo of the delete again");
});

test("a very hidden sheet has no tab, is not offered by 숨기기 취소, and stays very hidden through a save (PD-209)", async () => {
  const h = await open();
  await h.openBook([
    { name: "보임", cells: { A1: "a" } },
    { name: "숨김", cells: { A1: "h" }, hidden: true },
    { name: "깊은", cells: { A1: "v" }, hidden: true, veryHidden: true },
  ]);
  assert.deepEqual(h.names(), ["보임"]);
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  assert.equal(layout.veryHiddenOf("깊은"), true);
  assert.equal(layout.veryHiddenOf("숨김"), false);
  await h.tabMenu("보임", "unhide");
  const rows = [...h.document.querySelectorAll("#sheet-tab-unhide-list li")];
  assert.deepEqual(rows.map((r) => r.textContent), ["숨김"], "the list holds the hidden sheet only");
  h.userClick(h.document.querySelector("#sheet-tab-unhide-dialog .dialog-btn-primary"));
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["보임", "숨김"]);
  // With only a very hidden sheet left, the command is greyed.
  const t = h.tab("보임");
  t.dispatchEvent({ type: "contextmenu", target: t, clientX: 40, clientY: 700 });
  assert.equal(h.menu().querySelector('[data-tab-act="unhide"]').getAttribute("aria-disabled"), "true", "only a very hidden sheet is left");
  const sent = await h.saved();
  assert.deepEqual(sent.map((s) => [s.name, s.hidden === true, s.veryHidden === true]), [["보임", false, false], ["숨김", false, false], ["깊은", true, true]]);
  // A sheet that was hidden by the menu goes out as hidden, not as very hidden.
  await h.tabMenu("숨김", "hide");
  const again = await h.saved();
  assert.deepEqual(again.map((s) => [s.name, s.hidden === true, s.veryHidden === true]), [["보임", false, false], ["숨김", true, false], ["깊은", true, true]]);
});

test("a drag with Ctrl held copies the sheet before the tab the pointer is at, and shows the slot (PD-209)", async () => {
  const h = await open();
  await h.openBook([
    { name: "Sheet1", cells: { A1: "1" } },
    { name: "Sheet2", cells: { A1: "2" } },
    { name: "Sheet3", cells: { A1: "3" } },
  ]);
  const { document } = h;
  const place = () => h.tabs().forEach((t, i) => {
    t.getBoundingClientRect = () => ({ left: 10 + i * 80, top: 0, width: 80, height: 24 });
  });
  place();
  const first = h.tab("Sheet1");
  first.dispatchEvent({ type: "mousedown", target: first, clientX: 40, button: 0 });
  // Between the middles of the second and third tab: the slot is before Sheet3.
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 200, ctrlKey: true });
  assert.equal(document.getElementById("sheet-tab-bar").classList.contains("copying"), true);
  assert.deepEqual(h.tabs().map((t) => t.classList.contains("drop-before")), [false, false, true]);
  assert.ok(h.tabs().every((t) => t.style.transform === ""), "the tabs stay where they are");
  document.dispatchEvent({ type: "mouseup", target: document.body, clientX: 200, ctrlKey: true });
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2", "시트 1 (2)", "Sheet3"]);
  assert.equal(h.active(), "시트 1 (2)");
  assert.ok(h.tabs().every((t) => !t.classList.contains("drop-before") && !t.classList.contains("drop-end")));
  assert.equal(document.getElementById("sheet-tab-bar").classList.contains("copying"), false);
  const book = await h.book();
  assert.equal(book["시트 1 (2)"].cells.A1, "1");
  // Past the last tab the copy goes to the end; the mark is on the right edge of the last tab.
  place();
  const second = h.tab("Sheet2");
  second.dispatchEvent({ type: "mousedown", target: second, clientX: 100, button: 0 });
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 500, ctrlKey: true });
  assert.equal(h.tabs().at(-1).classList.contains("drop-end"), true);
  document.dispatchEvent({ type: "mouseup", target: document.body, clientX: 500, ctrlKey: true });
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2", "시트 1 (2)", "Sheet3", "시트 2 (2)"]);
  // Letting go of Ctrl before the drop is a move again.
  place();
  const third = h.tab("Sheet3");
  third.dispatchEvent({ type: "mousedown", target: third, clientX: 270, button: 0 });
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 200, ctrlKey: true });
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 90 });
  assert.equal(document.getElementById("sheet-tab-bar").classList.contains("copying"), false);
  document.dispatchEvent({ type: "mouseup", target: document.body, clientX: 90 });
  await flushAsyncTurns(30);
  assert.equal(h.names().length, 5, "no sheet was added");
  assert.equal(h.names().indexOf("Sheet3") < 3, true);
  // One undo step takes a copy back.
  await h.history("studio-undo");
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["Sheet1", "Sheet2", "시트 1 (2)", "Sheet3", "시트 2 (2)"].filter((n) => n !== "시트 2 (2)"));
});

test("이동/복사 lists the sheets with a tab and 끝으로 이동, moves or copies before the one picked, and cancelling does nothing (PD-209)", async () => {
  const h = await open();
  await h.openBook([
    { name: "A", cells: { A1: "a" } },
    { name: "B", cells: { A1: "b" }, hidden: true },
    { name: "C", cells: { A1: "c" } },
    { name: "D", cells: { A1: "d" } },
  ]);
  const { document, userClick } = h;
  const dialog = () => document.getElementById("sheet-tab-move-dialog");
  const rows = () => [...document.querySelectorAll("#sheet-tab-move-list li")];
  const pick = async (label, copy) => {
    userClick(rows().find((r) => r.textContent === label));
    document.getElementById("sheet-tab-move-copy").checked = copy;
    userClick(document.getElementById("btn-sheet-tab-move-ok"));
    await flushAsyncTurns(30);
  };
  await h.tabMenu("A", "move-copy");
  assert.equal(dialog().open, true);
  assert.equal(document.getElementById("sheet-tab-move-title").textContent, "이동/복사");
  assert.deepEqual(rows().map((r) => r.textContent), ["A", "C", "D", "(끝으로 이동)"], "a hidden sheet is not listed");
  assert.equal(rows().find((r) => r.getAttribute("aria-selected") === "true").textContent, "C", "the sheet after this one is picked first");
  assert.equal(document.getElementById("sheet-tab-move-copy").checked, false);
  // 취소 changes nothing.
  userClick(dialog().querySelector("[data-dialog-close].dialog-btn"));
  await flushAsyncTurns(20);
  assert.deepEqual(h.names(), ["A", "C", "D"]);
  // Move A to the end.
  await h.tabMenu("A", "move-copy");
  await pick("(끝으로 이동)", false);
  assert.deepEqual(h.names(), ["C", "D", "A"]);
  assert.equal(h.active(), "A");
  // Copy D before C.
  await h.tabMenu("D", "move-copy");
  await pick("C", true);
  assert.deepEqual(h.names(), ["D (2)", "C", "D", "A"]);
  assert.equal(h.active(), "D (2)");
  // A move before itself is nothing and adds no step.
  await h.tabMenu("C", "move-copy");
  await pick("C", false);
  assert.deepEqual(h.names(), ["D (2)", "C", "D", "A"]);
  // Each of the two steps is one undo step; the hidden sheet kept its place in the order.
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["C", "D", "A"]);
  await h.history("studio-undo");
  assert.deepEqual(h.names(), ["A", "C", "D"]);
  const sent = await h.saved();
  assert.deepEqual(sent.map((s) => s.name), ["A", "B", "C", "D"]);
});

test("a copy brings the names scoped to its source and refers to itself, and the limit of sheets and a protected structure stop it (PD-214, PD-216)", async () => {
  const h = await open();
  await h.openBook([
    { name: "Sheet1", cells: { A1: "1", B1: "=Local" } },
    { name: "Sheet2", cells: { A1: "2" } },
  ]);
  const namesApi = h.host.__EASYOFFICE_SHEET_NAMES__;
  namesApi.replaceAll([
    { name: "Local", scope: "Sheet1", refersTo: "Sheet1!$A$1:Sheet1!$B$2+Sheet2!$A$1+\"Sheet1!\"" },
    { name: "Global", refersTo: "Sheet1!$A$1" },
    { name: "Other", scope: "Sheet2", refersTo: "Sheet2!$A$1" },
  ], true);
  await h.tabMenu("Sheet1", "copy");
  const copy = h.active();
  assert.equal(copy, "시트 1 (2)");
  const all = namesApi.getAll();
  assert.equal(all.length, 4);
  assert.deepEqual(all.find((d) => d.scope === copy), { name: "Local", scope: copy, refersTo: "'시트 1 (2)'!$A$1:'시트 1 (2)'!$B$2+Sheet2!$A$1+\"Sheet1!\"" });
  assert.equal(all.find((d) => d.name === "Local" && d.scope === "Sheet1").refersTo.startsWith("Sheet1!"), true, "the source keeps its own");
  assert.equal(all.find((d) => d.name === "Global").scope, undefined, "a name of the workbook is not copied");
  // The names go with the step: one undo takes the copy and its names back.
  await h.history("studio-undo");
  assert.equal(namesApi.getAll().length, 3);
  // A protected structure refuses the dialog and the copy alike.
  h.host.__EASYOFFICE_SHEET_PROTECT__.setWorkbook({ structure: true, windows: false });
  const t = h.tab("Sheet1");
  t.dispatchEvent({ type: "contextmenu", target: t, clientX: 40, clientY: 700 });
  assert.equal(h.menu().querySelector('[data-tab-act="move-copy"]').getAttribute("aria-disabled"), "true");
  h.host.__EASYOFFICE_SHEET_PROTECT__.setWorkbook(null);
});

test("the key held as the button goes up decides between a copy and a move, and a move to the same place is no step (PD-209)", async () => {
  const h = await open();
  await h.openBook([{ name: "Sheet1", cells: { A1: "1" } }, { name: "Sheet2", cells: { A1: "2" } }, { name: "Sheet3", cells: { A1: "3" } }]);
  const { document } = h;
  const place = () => h.tabs().forEach((t, i) => {
    t.getBoundingClientRect = () => ({ left: 10 + i * 80, top: 0, width: 80, height: 24 });
  });
  // Ctrl went up before the pointer moved on and the button goes up without it: a move, not a copy.
  place();
  const first = h.tab("Sheet1");
  first.dispatchEvent({ type: "mousedown", target: first, clientX: 40, button: 0 });
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 200, ctrlKey: true });
  document.dispatchEvent({ type: "mouseup", target: document.body, clientX: 200 });
  await flushAsyncTurns(30);
  assert.equal(h.names().length, 3, "no copy");
  // The other way round: Ctrl goes down after the last move and is held at the release: a copy at the release point.
  place();
  const second = h.tab("Sheet2");
  second.dispatchEvent({ type: "mousedown", target: second, clientX: 100, button: 0 });
  document.dispatchEvent({ type: "mousemove", target: document.body, clientX: 40 });
  document.dispatchEvent({ type: "mouseup", target: document.body, clientX: 40, ctrlKey: true });
  await flushAsyncTurns(30);
  assert.equal(h.names().length, 4, "one copy");
  // A move before the sheet that already follows it, or before itself, changes nothing.
  const steps = async () => (await h.saved()).length;
  const before = h.names();
  const only = h.tab(before[0]);
  await h.tabMenu(before[0], "move-copy");
  const rows = [...document.querySelectorAll("#sheet-tab-move-list li")];
  h.userClick(rows[1]);
  h.userClick(document.getElementById("btn-sheet-tab-move-ok"));
  await flushAsyncTurns(30);
  assert.deepEqual(h.names(), before);
  await h.history("studio-undo");
  assert.equal(h.names().length, 3, "the one undo step is the copy's, the move before the next tab made none");
  assert.ok(only && (await steps()) === 3);
});

test("a copy of a sheet takes its names with their references moved by the sheet name, never by a part of another name, and is refused when it cannot take them whole (PD-214)", async () => {
  const h = await open();
  await h.openBook([{ name: "시트1", cells: { A1: "1" } }, { name: "Sheet2", cells: { A1: "2" } }]);
  const namesApi = h.host.__EASYOFFICE_SHEET_NAMES__;
  assert.equal(namesApi.retarget("내시트1!A1+[1]시트1!A1+시트1!A1+'시트1'!B2+\"시트1!x\"", "시트1", "시트1 (2)"), "내시트1!A1+[1]시트1!A1+'시트1 (2)'!A1+'시트1 (2)'!B2+\"시트1!x\"");
  assert.equal(namesApi.retarget("Xsheet1!A1+sheet1!A1+Sheet1_2!A1+Sheet1.x!A1", "Sheet1", "Z"), "Xsheet1!A1+Z!A1+Sheet1_2!A1+Sheet1.x!A1");
  // A definition that would pass its length is refused whole, and so is a list that would pass the limit.
  const long = "시트1!$A$1" + "+1".repeat(4100);
  namesApi.replaceAll([{ name: "Long", scope: "시트1", refersTo: long }, { name: "Short", scope: "시트1", refersTo: "시트1!$A$1" }], true);
  assert.equal(namesApi.copiedFor(namesApi.getAll(), "시트1", "시트1 (2)"), null, "a longer name text than the limit");
  namesApi.replaceAll([{ name: "Short", scope: "시트1", refersTo: "시트1!$A$1" }], true);
  assert.equal(namesApi.copiedFor(namesApi.getAll(), "시트1", "시트1 (2)").length, 1);
  const many = Array.from({ length: namesApi.MAX_NAMES - 1 }, (_, i) => ({ name: "N" + i, refersTo: "Sheet2!$A$1" }));
  namesApi.replaceAll([...many, { name: "Own", scope: "시트1", refersTo: "시트1!$A$1" }], true);
  assert.equal(namesApi.copiedFor(namesApi.getAll(), "시트1", "시트1 (2)"), null, "the list would pass its limit");
  await h.tabMenu("시트1", "copy");
  assert.deepEqual(h.names(), ["시트1", "Sheet2"], "no copy was made");
  assert.match(h.status(), /이름 정의가 한도를 넘어/);
});

test("탭 색 colours the tab as the other spreadsheet program does: the colour is the background of a tab that is not chosen, a bar under the chosen one (PD-229)", async () => {
  const h = await open();
  await h.openBook([{ name: "Sheet1", cells: { A1: "1" }, tabColor: "112244" }, { name: "Sheet2", cells: { A1: "2" }, tabColor: "ffcc00" }, { name: "Sheet3", cells: { A1: "3" } }]);
  const style = (name, prop) => h.tab(name).style.getPropertyValue(prop);
  // Sheet1 is chosen: it has the bar value and the text it already had; Sheet2 is the colour with black text on the light colour.
  assert.equal(h.tab("Sheet1").dataset.tabColor, "112244");
  assert.equal(style("Sheet1", "--tab-color"), "#112244");
  assert.equal(style("Sheet1", "--tab-text"), "#ffffff", "white on a dark colour");
  assert.equal(style("Sheet2", "--tab-color"), "#ffcc00");
  assert.equal(style("Sheet2", "--tab-text"), "#000000", "black on a light colour");
  assert.equal(h.tab("Sheet3").dataset.tabColor, undefined);
  assert.equal(h.tab("Sheet3").style.getPropertyValue("--tab-color"), "");
  // Choosing a tab moves nothing: the colour stays on the tab and only the look (CSS) differs.
  h.tab("Sheet2").click();
  await flushAsyncTurns(30);
  assert.equal(h.active(), "Sheet2");
  assert.equal(h.tab("Sheet2").dataset.tabColor, "ffcc00");
});

test("the 탭 색 palette sets the colour of the tab it was opened on, 색 없음 takes it off, each is one undo step and the file gets it back (PD-229)", async () => {
  const h = await open();
  await h.openBook([{ name: "Sheet1", cells: { A1: "1" } }, { name: "Sheet2", cells: { A1: "2" } }]);
  const { document, userClick } = h;
  const openMenu = (name) => {
    const t = h.tab(name);
    t.dispatchEvent({ type: "contextmenu", target: t, clientX: 40, clientY: 700 });
  };
  const swatches = () => [...document.querySelectorAll("#sheet-tab-color-panel .sb-hl-swatch")];
  openMenu("Sheet2");
  assert.equal(document.querySelector('#sheet-tab-menu [data-tab-sub="color"]').getAttribute("aria-disabled"), null);
  const red = swatches().find((s) => s.title === "#ff0000");
  userClick(red);
  await flushAsyncTurns(40);
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.tabColorOf("Sheet2"), "ff0000");
  assert.equal(h.tab("Sheet2").style.getPropertyValue("--tab-color"), "#ff0000");
  assert.equal(h.tab("Sheet1").dataset.tabColor, undefined, "only the tab it was opened on");
  assert.equal(document.getElementById("sheet-tab-menu").hidden, true, "the menu closes");
  assert.equal((await h.saved()).find((s) => s.name === "Sheet2").tabColor, "ff0000");
  // The palette marks the colour in use; the colour of another picker ("다른 색...") goes the same way.
  openMenu("Sheet2");
  assert.equal(swatches().find((s) => s.classList.contains("selected"))?.title, "#ff0000");
  const input = document.getElementById("sheet-tab-color-input");
  input.value = "#123456";
  input.onchange();
  await flushAsyncTurns(40);
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.tabColorOf("Sheet2"), "123456");
  // 색 없음.
  openMenu("Sheet2");
  userClick([...document.querySelectorAll("#sheet-tab-color-panel .sb-hl-palette-actions button")][0]);
  await flushAsyncTurns(40);
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.tabColorOf("Sheet2"), null);
  assert.equal(h.tab("Sheet2").dataset.tabColor, undefined);
  // The same colour again is no step; each of the others is exactly one.
  openMenu("Sheet1");
  userClick(swatches().find((s) => s.title === "#00ff00"));
  await flushAsyncTurns(40);
  openMenu("Sheet1");
  userClick(swatches().find((s) => s.title === "#00ff00"));
  await flushAsyncTurns(40);
  await h.history("studio-undo");
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.tabColorOf("Sheet1"), null, "one undo takes the colour off, as the second press was no step");
  await h.history("studio-undo");
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.tabColorOf("Sheet2"), "123456");
  assert.equal(h.tab("Sheet2").dataset.tabColor, "123456", "the strip follows the undo");
});

test("a protected workbook structure and a protected sheet keep the tab colour from changing (PD-216, PD-229)", async () => {
  const h = await open();
  await h.openBook([{ name: "Sheet1", cells: { A1: "1" } }, { name: "Sheet2", cells: { A1: "2" }, protect: { on: true, deny: [] } }]);
  const { document } = h;
  const sub = () => document.querySelector('#sheet-tab-menu [data-tab-sub="color"]');
  const openMenu = (name) => {
    const t = h.tab(name);
    t.dispatchEvent({ type: "contextmenu", target: t, clientX: 40, clientY: 700 });
  };
  openMenu("Sheet2");
  assert.equal(sub().getAttribute("aria-disabled"), "true", "a protected sheet");
  assert.equal(await h.host.__EASYOFFICE_SHEET_PROTECT__ && true, true);
  openMenu("Sheet1");
  assert.equal(sub().getAttribute("aria-disabled"), null);
  h.host.__EASYOFFICE_SHEET_PROTECT__.setWorkbook({ structure: true, windows: false });
  openMenu("Sheet1");
  assert.equal(sub().getAttribute("aria-disabled"), "true", "a protected structure");
  h.host.__EASYOFFICE_SHEET_PROTECT__.setWorkbook(null);
});

test("the keyboard reaches the 탭 색 palette: Right opens it, the arrows move in it, Left and Escape go back to the item (PD-229)", async () => {
  const h = await open();
  await h.openBook([{ name: "Sheet1", cells: { A1: "1" } }, { name: "Sheet2", cells: { A1: "2" } }]);
  const { document } = h;
  const t = h.tab("Sheet1");
  t.dispatchEvent({ type: "contextmenu", target: t, clientX: 40, clientY: 700 });
  const sub = document.querySelector('#sheet-tab-menu [data-tab-sub="color"]');
  const panel = document.getElementById("sheet-tab-color-panel");
  sub.focus();
  const press = (key) => h.menu().dispatchEvent({ type: "keydown", key, target: document.activeElement });
  press("ArrowRight");
  assert.equal(sub.classList.contains("open"), true);
  assert.equal(panel.contains(document.activeElement), true);
  const first = document.activeElement;
  press("ArrowRight");
  assert.notEqual(document.activeElement, first);
  press("End");
  assert.equal(document.activeElement.textContent, "다른 색...");
  press("ArrowLeft");
  assert.equal(document.activeElement, sub);
  assert.equal(sub.classList.contains("open"), false);
  assert.equal(h.menu().hidden, false, "Left only leaves the palette");
  press("ArrowRight");
  press("Escape");
  assert.equal(document.activeElement, sub);
  assert.equal(h.menu().hidden, false, "Escape in the palette goes back first");
});
