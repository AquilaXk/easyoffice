import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { fakeSortEngine } from "./test-support/fake-sort-engine.mjs";

// The sheet sort (PD-203): which merges a sort accepts, which rows may move, how the keys go to
// the engine, how the cells follow the answer, the header guess and the dialog with its undo step.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const loaded = loadUiModule(UI("sheet-sort.js"), {});
const core = loaded.__EASYOFFICE_SHEET_SORT_CORE__;
const colName = (c) => String.fromCharCode(64 + c);
const gridUtils = { refOf: (r, c) => colName(c) + r };

const m = (r1, c1, r2, c2) => ({ r1, c1, r2, c2 });

test("merges are accepted only as equal blocks that tile the whole range", () => {
  const range = m(2, 1, 7, 2);
  assert.deepEqual(core.unitOf([], range), { h: 1, w: 1 });
  assert.deepEqual(core.unitOf([m(2, 1, 3, 1), m(2, 2, 3, 2), m(4, 1, 5, 1), m(4, 2, 5, 2), m(6, 1, 7, 1), m(6, 2, 7, 2)], range), { h: 2, w: 1 });
  assert.deepEqual(core.unitOf([m(2, 1, 3, 2), m(4, 1, 5, 2), m(6, 1, 7, 2)], range), { h: 2, w: 2 });
  // One merge of another size, a gap, or a merge that sticks out refuse the sort.
  assert.deepEqual(core.unitOf([m(2, 1, 3, 2), m(4, 1, 6, 2)], range), { error: "mergeSize" });
  assert.deepEqual(core.unitOf([m(2, 1, 3, 2), m(4, 1, 5, 2)], range), { error: "mergeSize" });
  assert.deepEqual(core.unitOf([m(1, 1, 3, 2)], range), { error: "merge" });
  assert.deepEqual(core.unitOf([m(2, 1, 3, 3)], range), { error: "merge" });
});

test("rows hidden by the user or a filter stay out of the sort", () => {
  const hidden = new Set([3, 4, 8]);
  assert.deepEqual(core.unitRows(m(2, 1, 9, 1), 1, (r) => hidden.has(r)), [2, 5, 6, 7, 9]);
  // A block is hidden when its first row is.
  assert.deepEqual(core.unitRows(m(2, 1, 7, 1), 2, (r) => r === 4), [2, 6]);
  assert.deepEqual(core.unitRows(m(2, 1, 6, 1), 2, () => false), [2, 4], "an incomplete last block is left out");
});

test("a key is blank, a number (formatted ones count) or the trimmed text", () => {
  const sandbox = { __EASYOFFICE_SHEET_UTILS__: { normalizeFormattedNumericCell: (v) => String(v ?? "").trim().replaceAll(",", "") } };
  const { numberOf, keyValue } = loadUiModule(UI("sheet-sort.js"), sandbox).__EASYOFFICE_SHEET_SORT_CORE__;
  assert.equal(keyValue(undefined, numberOf), null);
  assert.equal(keyValue({ value: "  " }, numberOf), null);
  assert.equal(keyValue({ value: "1,250" }, numberOf), 1250);
  assert.equal(keyValue({ value: " 가나 " }, numberOf), "가나");
  assert.equal(keyValue({ value: "0" }, numberOf), 0);
});

test("a header row is a text row over numbers or a bold row over plain ones", () => {
  const refOf = gridUtils.refOf;
  const numberOf = (e) => (e && /^-?\d+(\.\d+)?$/.test(String(e.value ?? "").trim()) ? Number(e.value) : null);
  const sheet = (rows) => Object.fromEntries(rows.flatMap((row, r) => row.map((v, c) => [refOf(r + 1, c + 1), typeof v === "object" ? v : { value: v }])));
  const range = (rows, cols) => m(1, 1, rows, cols);
  assert.equal(core.detectHeader(sheet([["부서", "매출"], ["영업", "10"], ["개발", "20"]]), range(3, 2), refOf, numberOf), true);
  assert.equal(core.detectHeader(sheet([["영업", "10"], ["개발", "20"]]), range(2, 2), refOf, numberOf), false, "a number in the first row");
  assert.equal(core.detectHeader(sheet([["가", "나"], ["다", "라"]]), range(2, 2), refOf, numberOf), false, "all text, nothing tells");
  const bold = { value: "가", style: { fontWeight: "600" } };
  assert.equal(core.detectHeader(sheet([[bold, { value: "나", style: { fontWeight: "bold" } }], ["다", "라"]]), range(2, 2), refOf, numberOf), true);
  assert.equal(core.detectHeader(sheet([[bold], [bold]]), range(2, 1), refOf, numberOf), false, "the row below is bold too");
  assert.equal(core.detectHeader(sheet([["a"]]), range(1, 1), refOf, numberOf), false);
  assert.equal(core.detectHeader(sheet([["", ""], ["1", "2"]]), range(2, 2), refOf, numberOf), false, "a blank row");
});

function sorter({ merges = [], hidden = [], engine = fakeSortEngine } = {}) {
  const requests = [];
  const layoutApi = {
    mergesIn: (name, r1, c1, r2, c2) => merges.filter((g) => g.r1 <= r2 && g.r2 >= r1 && g.c1 <= c2 && g.c2 >= c1),
    rowHidden: (name, r) => hidden.includes(r),
  };
  const sandbox = { __EASYOFFICE_SHEET_UTILS__: { normalizeFormattedNumericCell: (v) => String(v ?? "").trim() } };
  const host = loadUiModule(UI("sheet-sort.js"), sandbox);
  const run = host.__EASYOFFICE_CREATE_SHEET_SORT__({
    gridUtils,
    call: async (body) => {
      requests.push(body);
      return engine(body);
    },
    layoutApi,
    cloneStyle: (s) => (s ? { ...s } : undefined),
  }).run;
  return { run, requests };
}

const sheetOf = (rows) =>
  Object.fromEntries(rows.flatMap((row, r) => row.map((v, c) => [colName(c + 1) + (r + 1), typeof v === "object" ? v : { value: String(v), formula: "" }])));
const column = (map, col, rows) => Array.from({ length: rows }, (_, i) => map[col + (i + 1)]?.value ?? "");

test("a sort by two keys moves whole rows, keeps ties in order and puts blanks last", async () => {
  const map = sheetOf([["영업", 5, "a"], ["개발", 9, "b"], ["영업", 5, "c"], ["", 1, "d"], ["개발", 2, "e"], ["영업", 7, "f"]]);
  const { run, requests } = sorter();
  const result = await run("S", map, { range: m(1, 1, 6, 3), keys: [{ col: 1, asc: true }, { col: 2, asc: false }], hasHeader: false, caseSensitive: false });
  assert.deepEqual(result, { ok: true, moved: 6 });
  assert.deepEqual(column(map, "A", 6), ["개발", "개발", "영업", "영업", "영업", ""]);
  assert.deepEqual(column(map, "C", 6), ["b", "e", "f", "a", "c", "d"]);
  assert.deepEqual(requests[0].keys[0], { asc: true, values: ["영업", "개발", "영업", null, "개발", "영업"] });
  assert.deepEqual(requests[0].keys[1].values, [5, 9, 5, 1, 2, 7]);
  assert.equal(requests[0].sessionAction, "sort");
  assert.deepEqual(requests[0].range, { r1: 1, r2: 6 });
});

test("a header row stays, case counts only on request and a sorted range reports no move", async () => {
  const map = sheetOf([["이름"], ["b"], ["B"], ["a"], ["A"]]);
  const { run, requests } = sorter();
  const spec = { range: m(1, 1, 5, 1), keys: [{ col: 1, asc: true }], hasHeader: true, caseSensitive: false };
  await run("S", map, spec);
  assert.deepEqual(column(map, "A", 5), ["이름", "a", "A", "b", "B"]);
  assert.deepEqual(requests[0].rows, [2, 3, 4, 5]);
  assert.equal((await run("S", map, spec)).moved, 0, "already sorted");
  await run("S", map, { ...spec, keys: [{ col: 1, asc: false }], caseSensitive: true });
  assert.deepEqual(column(map, "A", 5), ["이름", "B", "b", "A", "a"]);
  assert.equal(requests[2].caseSensitive, true);
});

test("hidden rows keep their place while the visible rows trade places", async () => {
  const map = sheetOf([["h"], ["3"], ["1"], ["2"], ["0"]]);
  const { run, requests } = sorter({ hidden: [3] });
  await run("S", map, { range: m(1, 1, 5, 1), keys: [{ col: 1, asc: true }], hasHeader: true, caseSensitive: false });
  assert.deepEqual(requests[0].rows, [2, 4, 5]);
  // Row 3 ("1") did not move; 3, 2, 0 sorted among rows 2, 4 and 5.
  assert.deepEqual(column(map, "A", 5), ["h", "0", "1", "2", "3"]);
});

test("merged blocks of equal size move together and the merges stay", async () => {
  const merges = [m(1, 1, 2, 2), m(3, 1, 4, 2), m(5, 1, 6, 2)];
  const map = sheetOf([["c", "1"], ["", ""], ["a", "3"], ["", ""], ["b", "5"], ["", ""]]);
  const { run, requests } = sorter({ merges });
  // Column 2 is inside the merges: the key is read from the merge's own cell.
  const result = await run("S", map, { range: m(1, 1, 6, 2), keys: [{ col: 2, asc: true }], hasHeader: false, caseSensitive: false });
  assert.deepEqual(result, { ok: true, moved: 3 });
  assert.deepEqual(requests[0].rows, [1, 3, 5]);
  assert.equal(requests[0].height, 2);
  assert.deepEqual(requests[0].keys[0].values, ["c", "a", "b"]);
  assert.deepEqual(column(map, "A", 6), ["a", "", "b", "", "c", ""]);
  assert.deepEqual(column(map, "B", 6), ["3", "", "5", "", "1", ""]);
  const bad = await sorter({ merges: [m(1, 1, 2, 1), m(3, 1, 3, 2)] }).run("S", sheetOf([["a"], ["b"], ["c"]]), { range: m(1, 1, 3, 2), keys: [{ col: 1, asc: true }], hasHeader: false, caseSensitive: false });
  assert.deepEqual(bad, { ok: false, why: "mergeSize" });
  assert.equal(core.FAILURES.mergeSize, "크기가 다른 병합 셀이 있어 정렬할 수 없음");
});

test("formulas come back from the engine and styles travel with the cells", async () => {
  const map = sheetOf([[{ value: "2", formula: "", style: { fontWeight: "600" } }, { value: "4", formula: "=A1*2" }], [{ value: "1", formula: "" }, { value: "2", formula: "=A2*2" }]]);
  const { run, requests } = sorter();
  await run("S", map, { range: m(1, 1, 2, 2), keys: [{ col: 1, asc: true }], hasHeader: false, caseSensitive: false });
  assert.deepEqual(requests[0].formulas, [[0, "=A1*2"], [1, "=A2*2"]]);
  assert.equal(map.A2.style.fontWeight, "600");
  assert.equal(map.B1.formula, "=A1*2");
  assert.equal(map.B2.formula, "=A2*2");
  assert.equal(map.B2.value, "4");
});

test("a failing or malformed engine answer leaves the cells alone", async () => {
  for (const engine of [() => { throw new Error("x"); }, () => ({ order: [0] }), () => ({ order: [1, 0], formulas: [1] })]) {
    const map = sheetOf([["b", { value: "", formula: "=A1" }], ["a", { value: "", formula: "=A2" }]]);
    const before = JSON.stringify(map);
    const result = await sorter({ engine }).run("S", map, { range: m(1, 1, 2, 2), keys: [{ col: 1, asc: true }], hasHeader: false, caseSensitive: false });
    assert.deepEqual(result, { ok: false, why: "engine" });
    assert.equal(JSON.stringify(map), before);
  }
  const tooMany = await sorter().run("S", sheetOf([["a"], ["b"]]), { range: m(1, 1, 2, 1), keys: Array.from({ length: 65 }, () => ({ col: 1, asc: true })), hasHeader: false });
  assert.deepEqual(tooMany, { ok: true, moved: 0 }, "more keys than the limit are not sent");
});

// ---- the dialog on a sheet -------------------------------------------------------------------------

const MODEL = {
  sheets: [
    {
      name: "표",
      cells: [
        { ref: "A1", raw: "부서" },
        { ref: "B1", raw: "매출" },
        ...[["영업", "30"], ["개발", "20"], ["영업", "10"], ["기획", "40"]].flatMap(([dept, sales], i) => [
          { ref: "A" + (i + 2), raw: dept },
          { ref: "B" + (i + 2), raw: sales },
        ]),
      ],
    },
  ],
};

async function createHarness() {
  const requests = [];
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
      if (body?.sessionAction === "sort") requests.push(body);
      return offline[command](payload);
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(harness.document, makeFixtureFile("정렬.xlsx", MODEL));
  await flushAsyncTurns(8);
  return { ...harness, requests };
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const act = (d, name) => d.querySelector(`.md-item[data-sheet-act="${name}"]`);
const undo = (d) => d.querySelector('.md-item[data-studio-menu-cmd="studio-undo"]');

async function click(h, el) {
  h.userClick(el);
  await flushAsyncTurns(14);
}

test("the 정렬 dialog guesses the header, adds a key row and sorts as one undo step", async () => {
  const h = await createHarness();
  const d = h.document;
  cell(d, "A3").focus();
  await click(h, act(d, "sort-dialog"));
  assert.equal(d.getElementById("sheet-sort-dialog").open, true);
  assert.equal(d.getElementById("sheet-sort-header").checked, true, "text over numbers is a header");
  assert.equal(d.getElementById("sheet-sort-case").checked, false);
  const col = (i) => d.getElementById("sheet-sort-col-" + i);
  assert.deepEqual([...col(0).querySelectorAll("option")].map((o) => o.textContent), ["부서", "매출"], "the header names the columns");
  d.getElementById("sheet-sort-header").checked = false;
  d.getElementById("sheet-sort-header").dispatchEvent({ type: "change", target: d.getElementById("sheet-sort-header"), bubbles: true });
  assert.deepEqual([...col(0).querySelectorAll("option")].map((o) => o.textContent), ["열 A", "열 B"]);
  d.getElementById("sheet-sort-header").checked = true;
  d.getElementById("sheet-sort-header").dispatchEvent({ type: "change", target: d.getElementById("sheet-sort-header"), bubbles: true });
  assert.equal(d.querySelector("#sheet-sort-keys .dialog-label").textContent, "정렬 기준");
  assert.equal(d.querySelector("#sheet-sort-keys [data-sort-remove]").disabled, true, "the only key cannot be removed");
  await click(h, d.getElementById("btn-sheet-sort-add"));
  assert.equal(d.getElementById("sheet-sort-col-1").value, "2", "the next unused column");
  assert.equal(d.querySelectorAll("#sheet-sort-keys .dialog-label")[1].textContent, "다음 기준");
  assert.equal(d.getElementById("btn-sheet-sort-add").disabled, true, "as many keys as columns");
  const order = d.getElementById("sheet-sort-order-1");
  order.value = "desc";
  order.dispatchEvent({ type: "change", target: order, bubbles: true });
  await click(h, d.getElementById("btn-sheet-sort-apply"));
  assert.equal(h.requests.length, 1);
  assert.deepEqual(h.requests[0].keys.map((k) => k.asc), [true, false]);
  assert.deepEqual(h.requests[0].rows, [2, 3, 4, 5], "the header row is not sent");
  assert.equal(d.getElementById("sheet-sort-dialog").open, false);
  assert.deepEqual(["A", "B"].map((c) => [2, 3, 4, 5].map((r) => cell(d, c + r).textContent)), [["개발", "기획", "영업", "영업"], ["20", "40", "30", "10"]]);
  await click(h, undo(d));
  assert.deepEqual([2, 3, 4, 5].map((r) => cell(d, "A" + r).textContent), ["영업", "개발", "영업", "기획"]);
});

test("the same column twice is refused and a range of one row has nothing to sort", async () => {
  const h = await createHarness();
  const d = h.document;
  cell(d, "A3").focus();
  await click(h, act(d, "sort-dialog"));
  await click(h, d.getElementById("btn-sheet-sort-add"));
  const second = d.getElementById("sheet-sort-col-1");
  second.value = "1";
  second.dispatchEvent({ type: "change", target: second, bubbles: true });
  await click(h, d.getElementById("btn-sheet-sort-apply"));
  assert.equal(d.getElementById("sheet-sort-status").textContent, "같은 열을 두 번 고를 수 없음");
  assert.equal(h.requests.length, 0);
  assert.equal(d.getElementById("sheet-sort-dialog").open, true);
  await click(h, d.querySelector("#sheet-sort-keys [data-sort-remove='1']"));
  assert.equal(d.querySelectorAll("#sheet-sort-keys .sheet-sort-row").length, 1);
});

test("the menu, the native bar and the page agree on the 정렬 item", () => {
  const html = fs.readFileSync(UI("index.html"), "utf8");
  assert.match(html, /data-sheet-act="sort-dialog"[^>]*><span class="md-label">정렬…<\/span>/);
  const chrome = fs.readFileSync(UI("shell-chrome.js"), "utf8");
  assert.match(chrome, /"sheet:sort": "sheet-act:sort-dialog"/);
  assert.equal(core.FAILURES.merge, "병합된 셀이 있어 정렬할 수 없음");
});
