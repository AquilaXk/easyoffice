import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { fakeFilterEngine } from "./test-support/fake-filter-engine.mjs";

// The filter buttons of a table (PD-226, PD-231): the header cells of a table that has them carry the button, the panel
// works on the table's own conditions and hidden rows, `필터 단추` and the filter command turn the buttons on and off, and
// each change is one undo step.

const modelWith = (tables, filter = null) => ({
  sheets: [
    {
      name: "표",
      tables,
      ...(filter ? { filter } : {}),
      cells: [
        { ref: "A1", raw: "부서" },
        { ref: "B1", raw: "매출" },
        ...[["영업", "10"], ["개발", "20"], ["영업", "30"], ["기획", "40"], ["영업", "50"]].flatMap(([dept, sales], i) => [
          { ref: "A" + (i + 2), raw: dept },
          { ref: "B" + (i + 2), raw: sales },
        ]),
        { ref: "D1", raw: "단독" },
        { ref: "D2", raw: "1" },
        { ref: "D3", raw: "2" },
      ],
    },
  ],
});

const table = (extra = {}) => ({ name: "표1", range: "A1:B6", header: true, totals: false, columns: ["부서", "매출"], style: "TableStyleMedium2", rowStripes: true, ...extra });

async function createHarness(tables = [], filter = null) {
  const requests = [];
  const fake = fakeFilterEngine();
  const cat = (await createWorkspaceHarness()).sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = (await createWorkspaceHarness()).sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const harness = await createWorkspaceHarness({
    testIpc: async (command, payload) => {
      const body = typeof payload === "string" && payload.startsWith("{") ? JSON.parse(payload) : null;
      if (body?.sessionAction === "filter") {
        requests.push(body);
        return fake(body);
      }
      return offline[command](payload);
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(harness.document, makeFixtureFile("표필터.xlsx", modelWith(tables, filter)));
  await flushAsyncTurns(8);
  return { ...harness, requests };
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const act = (d, name) => d.querySelector(`.md-item[data-sheet-act="${name}"]`);
const undo = (d) => d.querySelector('.md-item[data-studio-menu-cmd="studio-undo"]');
const feedback = (d) => d.getElementById("status-command-feedback").textContent;
const layoutOf = (h) => h.host.__EASYOFFICE_SHEET_LAYOUT__;
const tableOf = (h) => layoutOf(h).tablesOf("표")[0];

async function click(h, el) {
  h.userClick(el);
  await flushAsyncTurns(14);
}

const withTable = (extra) => createHarness([table(extra)]);

async function pressButton(h, ref) {
  const td = cell(h.document, ref);
  td.getBoundingClientRect = () => ({ left: 0, right: 100, top: 0, bottom: 24 });
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, clientX: 95, clientY: 20, bubbles: true });
  await flushAsyncTurns(10);
}

const shownRows = (d) => [...d.querySelectorAll("#sheet-filter-rows .sheet-filter-row")].filter((r) => !r.hidden);
const labels = (d) => shownRows(d).map((r) => r.children[2].textContent);
const hiddenRows = (d, last = 6) => {
  const drawn = new Set([...d.querySelectorAll("#pane-sheet tbody tr")].map((tr) => Number(tr.dataset.row)));
  return Array.from({ length: last }, (_, i) => i + 1).filter((r) => !drawn.has(r));
};

test("only the header cells of a table with filter buttons carry the button", async () => {
  const h = await withTable({ filter: [] });
  const d = h.document;
  assert.ok(cell(d, "A1").classList.contains("filter-head") && cell(d, "B1").classList.contains("filter-head"));
  assert.equal(cell(d, "A2").classList.contains("filter-head"), false, "not a data cell");
  assert.equal(cell(d, "D1").classList.contains("filter-head"), false, "not a cell outside the table");
  assert.equal(cell(d, "A1").classList.contains("filter-on"), false);
  const bare = await withTable({});
  assert.equal(cell(bare.document, "A1").classList.contains("filter-head"), false, "a table without buttons has none");
  const headerless = await withTable({ header: false, filter: [] });
  assert.equal(cell(headerless.document, "A1").classList.contains("filter-head"), false, "a table without a header row has none");
});

test("the panel of a table lists its column and a pick filters the rows of the table", async () => {
  const h = await withTable({ filter: [] });
  const d = h.document;
  await pressButton(h, "A1");
  assert.equal(d.getElementById("sheet-filter-panel").hidden, false);
  assert.deepEqual(labels(d), ["(모두 선택)", "개발", "기획", "영업"]);
  const request = h.requests.at(-1);
  assert.equal(request.op, "values");
  assert.equal(request.filter.range, "A1:B6", "the table's range is the filter's");
  // Uncheck 개발 and 기획, then confirm.
  const rows = shownRows(d);
  for (const label of ["개발", "기획"]) h.userClick(rows.find((r) => r.children[2].textContent === label));
  await flushAsyncTurns(4);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  const t = tableOf(h);
  assert.deepEqual(t.filter, [{ col: 1, values: ["영업"] }]);
  assert.deepEqual(t.filterHidden, [[3, 3], [5, 5]]);
  assert.equal(layoutOf(h).filterOf("표"), null, "the sheet has no filter of its own");
  assert.deepEqual(hiddenRows(d), [3, 5]);
  assert.ok(cell(d, "A1").classList.contains("filter-on"));
  assert.equal(cell(d, "B1").classList.contains("filter-on"), false);
  assert.equal(feedback(d), "필터: 5행 중 3행 표시");
});

test("one undo takes the table's condition and its hidden rows back, redo applies them again", async () => {
  const h = await withTable({ filter: [] });
  const d = h.document;
  const before = layoutOf(h).getAll();
  await pressButton(h, "A1");
  h.userClick(shownRows(d).find((r) => r.children[2].textContent === "영업"));
  await flushAsyncTurns(4);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.ok(tableOf(h).filterHidden.length > 0);
  await click(h, undo(d));
  assert.deepEqual(layoutOf(h).getAll(), before);
  assert.deepEqual(hiddenRows(d), []);
  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.deepEqual(tableOf(h).filter?.[0]?.col, 1);
});

test("the sort rows of the table's panel sort the data rows and judge the filter again", async () => {
  const h = await withTable({ filter: [{ col: 1, values: ["영업"] }], filterHidden: [[3, 3], [5, 5]] });
  const d = h.document;
  await pressButton(h, "B1");
  await click(h, d.querySelector('[data-filter-act="sort-desc"]'));
  assert.equal(feedback(d), "내림차순 정렬");
  const values = [2, 3, 4, 5, 6].map((r) => cell(d, "B" + r)?.textContent);
  assert.deepEqual(values, ["50", undefined, "30", undefined, "10"], "the rows the filter hides are not drawn");
  assert.equal(h.requests.at(-1).op, "apply", "the rows moved, so the conditions are judged again");
});

test("the clear row of the panel drops the column's condition and shows its rows", async () => {
  const h = await withTable({ filter: [{ col: 1, values: ["영업"] }], filterHidden: [[3, 3], [5, 5]] });
  const d = h.document;
  await pressButton(h, "A1");
  await click(h, d.querySelector('[data-filter-act="clear-column"]'));
  assert.deepEqual(tableOf(h).filter, []);
  assert.equal(tableOf(h).filterHidden, undefined);
  assert.deepEqual(hiddenRows(d), []);
});

test("필터 지우기 clears the table's conditions and keeps its buttons", async () => {
  const h = await withTable({ filter: [{ col: 1, values: ["영업"] }], filterHidden: [[3, 3], [5, 5]] });
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-clear"));
  assert.deepEqual(tableOf(h).filter, []);
  assert.ok(cell(d, "A1").classList.contains("filter-head"));
  assert.deepEqual(hiddenRows(d), []);
});

test("the filter command inside a table turns its buttons off and on, one undo step each", async () => {
  const h = await withTable({ filter: [] });
  const d = h.document;
  cell(d, "A2").focus();
  await flushAsyncTurns(2);
  assert.equal(act(d, "filter-toggle").getAttribute("aria-checked"), "true");
  await click(h, act(d, "filter-toggle"));
  assert.equal(tableOf(h).filter, undefined);
  assert.equal(cell(d, "A1").classList.contains("filter-head"), false);
  assert.equal(feedback(d), "필터 해제");
  await click(h, act(d, "filter-toggle"));
  assert.deepEqual(tableOf(h).filter, []);
  assert.ok(cell(d, "A1").classList.contains("filter-head"));
  await click(h, undo(d));
  assert.equal(tableOf(h).filter, undefined);
  assert.equal(layoutOf(h).filterOf("표"), null, "the sheet got no filter of its own");
});

test("the 필터 단추 option of the table bar follows the buttons; off removes the conditions with their rows", async () => {
  const h = await withTable({ filter: [{ col: 1, values: ["영업"] }], filterHidden: [[3, 3], [5, 5]] });
  const d = h.document;
  cell(d, "B2").focus();
  await flushAsyncTurns(2);
  const box = d.querySelector('[data-table-opt="filterButton"]');
  assert.equal(box.checked, true);
  assert.equal(box.disabled, false);
  box.checked = false;
  box.dispatchEvent({ type: "change", target: box, bubbles: true });
  await flushAsyncTurns(14);
  assert.equal(tableOf(h).filter, undefined);
  assert.equal(tableOf(h).filterHidden, undefined);
  assert.deepEqual(hiddenRows(d), []);
  assert.equal(d.querySelector('[data-table-opt="filterButton"]').checked, false);
  await click(h, undo(d));
  assert.deepEqual(tableOf(h).filter, [{ col: 1, values: ["영업"] }]);
  assert.deepEqual(hiddenRows(d), [3, 5]);
});

test("a header row switched off disables the option; switched on again the buttons are back", async () => {
  const h = await withTable({ filter: [] });
  const d = h.document;
  cell(d, "B2").focus();
  await flushAsyncTurns(2);
  const header = d.querySelector('[data-table-opt="header"]');
  header.checked = false;
  header.dispatchEvent({ type: "change", target: header, bubbles: true });
  await flushAsyncTurns(14);
  assert.equal(tableOf(h).header, false);
  assert.equal(tableOf(h).filter, undefined);
  cell(d, "B3").focus();
  await flushAsyncTurns(2);
  assert.equal(d.querySelector('[data-table-opt="filterButton"]').disabled, true);
  const on = d.querySelector('[data-table-opt="header"]');
  on.checked = true;
  on.dispatchEvent({ type: "change", target: on, bubbles: true });
  await flushAsyncTurns(14);
  assert.deepEqual(tableOf(h).filter, [], "the header row is back with its buttons");
});

async function protectedSheet(deny) {
  const h = await createHarness([table({ filter: [] })]);
  const layout = layoutOf(h);
  layout.setProtect("표", { on: true, deny, ranges: [] });
  await flushAsyncTurns(2);
  return h;
}

test("a protected sheet that allows 자동 필터 사용 lets the table's buttons filter but not be turned off (PD-216)", async () => {
  const h = await protectedSheet([]);
  const d = h.document;
  await pressButton(h, "A1");
  assert.equal(d.getElementById("sheet-filter-panel").hidden, false);
  h.userClick(shownRows(d).find((r) => r.children[2].textContent === "영업"));
  await flushAsyncTurns(4);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.ok(tableOf(h).filterHidden?.length > 0, "the condition applied");
  cell(d, "A3").focus();
  await click(h, act(d, "filter-toggle"));
  assert.ok(Array.isArray(tableOf(h).filter), "the buttons stay");
  const option = d.querySelector('[data-table-opt="filterButton"]');
  assert.equal(option.disabled, true);
});

test("a protected sheet that denies 자동 필터 사용 keeps the table's panel shut", async () => {
  const h = await protectedSheet(["autoFilter"]);
  const d = h.document;
  await pressButton(h, "A1");
  const before = tableOf(h).filter;
  const rows = shownRows(d);
  if (!d.getElementById("sheet-filter-panel").hidden && rows.length > 0) {
    h.userClick(rows.find((r) => r.children[2].textContent === "영업"));
    await flushAsyncTurns(4);
    await click(h, d.getElementById("btn-sheet-filter-ok"));
  }
  assert.deepEqual(tableOf(h).filter, before, "no condition went in");
  assert.equal(tableOf(h).filterHidden, undefined);
});

test("a sheet filter that shares its header cells with a table without buttons keeps its button, and the press opens its panel", async () => {
  const h = await createHarness([table({})], { range: "A1:B6", columns: [], hidden: [] });
  const d = h.document;
  assert.ok(cell(d, "A1").classList.contains("filter-head"), "the button is drawn");
  assert.equal(layoutOf(h).filterOwnerAt("표", 1, 1), null, "by the sheet's filter");
  await pressButton(h, "A1");
  assert.equal(d.getElementById("sheet-filter-panel").hidden, false, "the panel opens by the answer the grid drew by");
  assert.equal(h.requests.at(-1).filter.range, "A1:B6");
});

test("where a table with buttons and a sheet filter share the header cells the table owns the button", async () => {
  const h = await createHarness([table({ filter: [] })], { range: "A1:B6", columns: [], hidden: [] });
  const d = h.document;
  assert.equal(layoutOf(h).filterOwnerAt("표", 1, 2), "표1");
  assert.ok(cell(d, "B1").classList.contains("filter-head"));
  await pressButton(h, "B1");
  assert.equal(d.getElementById("sheet-filter-panel").hidden, false);
  assert.equal(h.requests.at(-1).op, "values");
  assert.equal(layoutOf(h).filterOwnerAt("표", 2, 2), undefined, "a data cell has none");
  assert.equal(layoutOf(h).filterOwnerAt("표", 1, 9), undefined, "nor a cell outside both");
});

test("hiddenButtons hides the filter button on those columns only while keeping other columns interactive (PD-237)", async () => {
  const h = await createHarness([table({ filter: [], hiddenButtons: [2] })]);
  const d = h.document;
  assert.equal(layoutOf(h).filterOwnerAt("표", 1, 1), "표1");
  assert.equal(layoutOf(h).filterOwnerAt("표", 1, 2), undefined, "column B has hiddenButton=1");
  assert.ok(cell(d, "A1").classList.contains("filter-head"));
  assert.equal(cell(d, "B1").classList.contains("filter-head"), false);
  await pressButton(h, "B1");
  assert.equal(d.getElementById("sheet-filter-panel").hidden, true, "pressing hidden-button header does not open the panel");
  await pressButton(h, "A1");
  assert.equal(d.getElementById("sheet-filter-panel").hidden, false, "visible-button column opens normally");

  // Sheet autoFilter hiddenButtons works the same way and round-trips through filterToJson.
  const h2 = await createHarness([], { range: "A1:C6", columns: [], hidden: [], hiddenButtons: [1, 3] });
  assert.equal(layoutOf(h2).filterOwnerAt("표", 1, 1), undefined);
  assert.equal(layoutOf(h2).filterOwnerAt("표", 1, 2), null);
  assert.equal(layoutOf(h2).filterOwnerAt("표", 1, 3), undefined);
  assert.deepEqual(layoutOf(h2).toJson(layoutOf(h2).get("표")).filter.hiddenButtons, [1, 3]);
});
