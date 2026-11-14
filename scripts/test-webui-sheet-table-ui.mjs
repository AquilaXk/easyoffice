import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// The table commands in the page (PD-230): the 표 만들기 dialog, the 표 디자인 bar (name, size, options, style gallery, 범위로
// 변환), the growth of a table over what is typed, each as one undo step, and a protected sheet refusing them.

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const $ = (h, id) => h.document.getElementById(id);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });

function typeText(d, text) {
  for (const ch of text) {
    const td = d.activeElement;
    key(td, ch);
    td.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: td });
    td.textContent = td.textContent + ch;
    td.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: td });
  }
}

// `answers` maps a session action to what the stand-in engine answers (a function of the request body).
async function openSheet(answers = {}) {
  let testIpc;
  if (Object.keys(answers).length > 0) {
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
    testIpc = async (command, payload) => {
      const body = typeof payload === "string" && payload.startsWith("{") ? JSON.parse(payload) : null;
      if (command === "evaluate_sheet" && body && answers[body.sessionAction]) return answers[body.sessionAction](body);
      return offline[command](payload);
    };
  }
  const h = await createWorkspaceHarness({ transformHtml: demoHtml, ...(testIpc ? { testIpc } : {}) });
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  return h;
}

const tables = (h, sheet = "Sheet1") => h.host.__EASYOFFICE_SHEET_LAYOUT__.tablesOf(sheet);
const undo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
};
const redo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(14);
};

async function makeTable(h, range = "A1:H8", header = true) {
  cell(h.document, "B3").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-insert-table"));
  await flushAsyncTurns(4);
  $(h, "sheet-table-range").value = range;
  $(h, "sheet-table-header").checked = header;
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(16);
}

test("the 표 만들기 dialog opens on the region around the cell with the header found, and its 확인 makes the table", async () => {
  const h = await openSheet();
  cell(h.document, "B3").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-insert-table"));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-table-dialog").open, true);
  assert.equal($(h, "sheet-table-dialog-title").textContent, "표 만들기");
  assert.equal($(h, "sheet-table-range").value, "$A$1:$H$8");
  assert.equal($(h, "sheet-table-header").checked, true, "a first row of text over rows of numbers is a header");
  assert.equal(h.document.getElementById("sheet-table-bar").hidden, true, "no table yet");
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(16);
  assert.equal($(h, "sheet-table-dialog").open, false);
  const [made] = tables(h);
  assert.equal(made.name, "표1");
  assert.equal(made.range, "A1:H8");
  assert.equal(made.style, "TableStyleMedium2");
  assert.equal(made.columns[0], "부문");
  assert.equal($(h, "sheet-table-bar").hidden, false, "the bar shows while the active cell is in the table");
  assert.equal($(h, "sheet-table-name").value, "표1");
  assert.equal(cell(h.document, "A1").dataset.tsF, "1", "the header is painted in the style");
  assert.match(cell(h.document, "A1").style.getPropertyValue("--ts-fill"), /^#[0-9a-f]{6}$/);
  assert.match($(h, "status-command-feedback").textContent, /^표 만들기: 표1$/);
});

test("making a table is one undo step and redo makes it again", async () => {
  const h = await openSheet();
  await makeTable(h);
  assert.equal(tables(h).length, 1);
  await undo(h);
  assert.equal(tables(h).length, 0);
  assert.equal($(h, "sheet-table-bar").hidden, true);
  assert.equal(cell(h.document, "A1").dataset.tsF, undefined);
  await redo(h);
  assert.equal(tables(h)[0]?.name, "표1");
});

test("a range that overlaps a table stays in the dialog with the reason; another sheet's range is refused", async () => {
  const h = await openSheet();
  await makeTable(h, "A1:C4");
  cell(h.document, "F6").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-insert-table"));
  await flushAsyncTurns(4);
  $(h, "sheet-table-range").value = "B3:D6";
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-table-dialog").open, true);
  assert.equal($(h, "sheet-table-status").textContent, "다른 표와 겹치는 범위에는 표를 만들 수 없음");
  $(h, "sheet-table-range").value = "Sheet2!A1:B2";
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-table-status").textContent, "현재 시트의 범위만 가능");
  $(h, "sheet-table-range").value = "nonsense";
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-table-status").textContent, "범위가 올바르지 않음");
  assert.equal(tables(h).length, 1);
});

test("a protected sheet refuses the table command", async () => {
  const h = await openSheet();
  h.host.__EASYOFFICE_SHEET_LAYOUT__.setProtect("Sheet1", { on: true, deny: [] });
  cell(h.document, "B3").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-insert-table"));
  await flushAsyncTurns(4);
  assert.notEqual($(h, "sheet-table-dialog").open, true);
  assert.equal(tables(h).length, 0);
});

test("the options of the bar change the table, each as one undo step", async () => {
  const h = await openSheet();
  await makeTable(h);
  const box = (opt) => h.document.querySelector(`#sheet-table-bar [data-table-opt="${opt}"]`);
  assert.deepEqual(["header", "totals", "rowStripes", "firstColumn", "lastColumn", "columnStripes"].map((o) => box(o).checked), [true, false, true, false, false, false]);
  box("firstColumn").checked = true;
  box("firstColumn").dispatchEvent({ type: "change", target: box("firstColumn"), bubbles: true });
  await flushAsyncTurns(12);
  assert.equal(tables(h)[0].firstColumn, true);
  assert.equal(cell(h.document, "A3").dataset.tsB, "1", "the first column is bold in Medium2");
  box("rowStripes").checked = false;
  box("rowStripes").dispatchEvent({ type: "change", target: box("rowStripes"), bubbles: true });
  await flushAsyncTurns(12);
  assert.equal(tables(h)[0].rowStripes, false);
  assert.equal(cell(h.document, "B3").dataset.tsF, undefined, "no band without stripes");
  await undo(h);
  assert.equal(tables(h)[0].rowStripes, true);
  assert.equal(tables(h)[0].firstColumn, true);
  await undo(h);
  assert.equal(tables(h)[0].firstColumn, false);
});

test("the gallery lists the 60 styles in three groups, picks one and clears it", async () => {
  const h = await openSheet();
  await makeTable(h);
  h.userClick($(h, "btn-table-style"));
  await flushAsyncTurns(2);
  const swatches = [...h.document.querySelectorAll("#sheet-table-gallery .sheet-table-swatch")];
  assert.equal(swatches.length, 60);
  assert.deepEqual([...h.document.querySelectorAll("#sheet-table-gallery .sheet-table-gallery-title")].map((t) => t.textContent), ["밝게", "보통", "어둡게"]);
  assert.equal(swatches.find((s) => s.dataset.tableStyle === "TableStyleMedium2").classList.contains("selected"), true);
  assert.equal(swatches.find((s) => s.dataset.tableStyle === "TableStyleDark3").title, "표 스타일 어둡게 3");
  assert.equal(swatches[0].querySelectorAll(".sheet-table-swatch-cell").length, 25);
  h.userClick(swatches.find((s) => s.dataset.tableStyle === "TableStyleDark3"));
  await flushAsyncTurns(12);
  assert.equal(tables(h)[0].style, "TableStyleDark3");
  assert.equal($(h, "sheet-table-gallery").hidden, true);
  h.userClick($(h, "btn-table-style"));
  await flushAsyncTurns(2);
  const clear = [...h.document.querySelectorAll("#sheet-table-gallery .sb-hl-palette-actions button")].find((b) => b.textContent === "지우기");
  h.userClick(clear);
  await flushAsyncTurns(12);
  assert.equal(tables(h)[0].style, undefined);
  assert.equal(cell(h.document, "A1").dataset.tsF, undefined, "no style, no look");
  await undo(h);
  assert.equal(tables(h)[0].style, "TableStyleDark3");
});

test("what is typed under or beside the table is taken in; one undo takes back the growth and leaves the text", async () => {
  const h = await openSheet();
  await makeTable(h);
  const a9 = cell(h.document, "A9");
  a9.focus();
  typeText(h.document, "신규");
  key(a9, "Enter");
  await flushAsyncTurns(24);
  assert.equal(tables(h)[0].range, "A1:H9");
  assert.equal(cell(h.document, "A9").textContent, "신규");
  await undo(h);
  assert.equal(tables(h)[0].range, "A1:H8", "the growth alone is undone");
  assert.equal(cell(h.document, "A9").textContent, "신규", "the typed text stays");
  await redo(h);
  assert.equal(tables(h)[0].range, "A1:H9");
});

test("what is typed beside the table becomes a column with a header of its own", async () => {
  const h = await openSheet();
  await makeTable(h, "A1:C4");
  const d3 = cell(h.document, "D3");
  d3.focus();
  typeText(h.document, "x");
  key(d3, "Enter");
  await flushAsyncTurns(24);
  assert.equal(tables(h)[0].range, "A1:D4");
  assert.equal(tables(h)[0].columns.at(-1), "Column4");
  assert.equal(cell(h.document, "D1").textContent, "Column4", "the header of the new column");
  await undo(h);
  assert.equal(tables(h)[0].range, "A1:C4");
  assert.equal(cell(h.document, "D3").textContent, "x");
});

test("a protected sheet's table does not grow", async () => {
  const h = await openSheet();
  await makeTable(h);
  h.host.__EASYOFFICE_SHEET_LAYOUT__.setProtect("Sheet1", { on: true, deny: [], ranges: [] });
  assert.equal(await h.host.__EASYOFFICE_SHEET_TABLE_BAR__.expandAfter("Sheet1", ["A9"]), false);
  assert.equal(tables(h)[0].range, "A1:H8");
});

const SALES = [
  { name: "Sheet1", cells: [{ ref: "A1", raw: "Region" }, { ref: "B1", raw: "Q1" }, { ref: "A2", raw: "East" }, { ref: "B2", raw: "10" }, { ref: "A3", raw: "West" }, { ref: "B3", raw: "20" }, { ref: "D1", raw: "=SUM(Sales[Q1])" }, { ref: "D2", raw: "=표9[Q1]" }] },
];

test("renaming the table writes the new name into the formulas and names, as one undo step; a taken or bad name is refused", async () => {
  const requests = [];
  const h = await openSheet({
    nameCheck: (body) => ({ problem: body.candidate.name === "Bad Name" ? "name" : null }),
    tableColumn: (body) => {
      requests.push(body);
      return {
        rewrites: body.formulas.filter((f) => f.text.includes(body.table + "[")).map((f) => ({ sheet: f.sheet, cell: f.cell, text: f.text.replaceAll(body.table + "[", body.to + "[") })),
        names: body.names.length ? body.names.map((n) => ({ ...n, refersTo: n.refersTo.replaceAll(body.table + "[", body.to + "[") })) : null,
      };
    },
  });
  const { pickFile } = await import("./test-support/fixture-files.mjs");
  const { makeFixtureFile } = await import("./test-support/fixture-files.mjs");
  await pickFile(h.document, makeFixtureFile("s.xlsx", { sheets: SALES.map((s) => ({ ...s, tables: [{ name: "Sales", range: "A1:B3", header: true, totals: false, columns: ["Region", "Q1"], style: "TableStyleMedium2", firstColumn: false, lastColumn: false, rowStripes: true, columnStripes: false }] })) }));
  await flushAsyncTurns(8);
  h.host.__EASYOFFICE_SHEET_NAMES__.replaceAll([{ name: "Qs", refersTo: "Sales[Q1]" }]);
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  const input = $(h, "sheet-table-name");
  assert.equal(input.value, "Sales");
  input.value = "Orders";
  key(input, "Enter");
  await flushAsyncTurns(16);
  assert.equal(tables(h)[0].name, "Orders");
  assert.equal(h.host.__EASYOFFICE_SHEET_NAMES__.getAll()[0].refersTo, "Orders[Q1]");
  assert.equal(requests.at(-1).op, "rename");
  assert.equal(requests.at(-1).table, "Sales");
  assert.equal(h.document.querySelector('#pane-sheet td[data-cell="D1"]').dataset.formula, "=SUM(Orders[Q1])");
  await undo(h);
  assert.equal(tables(h)[0].name, "Sales");
  assert.equal(h.host.__EASYOFFICE_SHEET_NAMES__.getAll()[0].refersTo, "Sales[Q1]");
  // A bad name keeps the old one and says why.
  input.value = "Bad Name";
  key(input, "Enter");
  await flushAsyncTurns(16);
  assert.equal(tables(h)[0].name, "Sales");
  assert.equal($(h, "status-command-feedback").textContent, "표 이름이 올바르지 않음");
  assert.equal(input.value, "Sales");
  // A name another table has (without case) is refused before the engine is asked.
  h.host.__EASYOFFICE_SHEET_LAYOUT__.setTables("Sheet2", [{ name: "Other", range: "A1:B2", header: true, totals: false, columns: ["a", "b"] }]);
  input.value = "other";
  key(input, "Enter");
  await flushAsyncTurns(16);
  assert.equal(tables(h)[0].name, "Sales");
  assert.equal($(h, "status-command-feedback").textContent, "이미 있는 이름");
});

test("turning the table into a range removes it and writes its references as cells, in one undo step", async () => {
  const h = await openSheet({
    tableColumn: (body) => ({
      rewrites: body.formulas.filter((f) => f.text.includes(body.table + "[Q1]")).map((f) => ({ sheet: f.sheet, cell: f.cell, text: f.text.replaceAll(body.table + "[Q1]", "$B$2:$B$3") })),
      names: null,
    }),
  });
  const { pickFile, makeFixtureFile } = await import("./test-support/fixture-files.mjs");
  await pickFile(h.document, makeFixtureFile("s.xlsx", { sheets: SALES.map((s) => ({ ...s, tables: [{ name: "Sales", range: "A1:B3", header: true, totals: false, columns: ["Region", "Q1"], style: "TableStyleMedium2", firstColumn: false, lastColumn: false, rowStripes: true, columnStripes: false }] })) }));
  await flushAsyncTurns(8);
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-table-bar [data-table-act="to-range"]'));
  await flushAsyncTurns(16);
  assert.equal(tables(h).length, 0);
  assert.equal($(h, "sheet-table-bar").hidden, true);
  assert.equal(h.document.querySelector('#pane-sheet td[data-cell="D1"]').dataset.formula, "=SUM($B$2:$B$3)");
  await undo(h);
  assert.equal(tables(h).length, 1);
  assert.equal(h.document.querySelector('#pane-sheet td[data-cell="D1"]').dataset.formula, "=SUM(Sales[Q1])");
});

test("the size of the table is changed in the dialog from the bar, in one undo step", async () => {
  const h = await openSheet();
  await makeTable(h, "A1:C4");
  cell(h.document, "B2").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-table-bar [data-table-act="resize"]'));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-table-dialog-title").textContent, "표 크기 조정");
  assert.equal($(h, "sheet-table-range").value, "$A$1:$C$4");
  assert.equal($(h, "sheet-table-header-row").hidden, true);
  $(h, "sheet-table-range").value = "A1:E6";
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(16);
  assert.equal(tables(h)[0].range, "A1:E6");
  assert.deepEqual(tables(h)[0].columns.length, 5);
  await undo(h);
  assert.equal(tables(h)[0].range, "A1:C4");
  h.userClick(h.document.querySelector('#sheet-table-bar [data-table-act="resize"]'));
  await flushAsyncTurns(4);
  $(h, "sheet-table-range").value = "A2:C4";
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-table-status").textContent, "머리글 행은 같은 행에 있어야 함");
});
