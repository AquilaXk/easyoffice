import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// The sheet's auto filter (PD-198): how a condition in the dialog maps to the engine's condition,
// the range a filter is created for, the layout lookups over the rows the user and the filter
// hide, the commands with their undo steps and the panel on screen.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const core = loadUiModule(UI("sheet-filter.js"), {}).__EASYOFFICE_SHEET_FILTER_CORE__;

test("every condition of the dialog maps to the engine's form and back", () => {
  const forms = [
    [{ op: "equals", text: "a" }, { text: "a" }],
    [{ op: "notEquals", text: "a" }, { op: "notEqual", text: "a" }],
    [{ op: "greaterThan", text: "5" }, { op: "greaterThan", text: "5" }],
    [{ op: "greaterThanOrEqual", text: "5" }, { op: "greaterThanOrEqual", text: "5" }],
    [{ op: "lessThan", text: "5" }, { op: "lessThan", text: "5" }],
    [{ op: "lessThanOrEqual", text: "5" }, { op: "lessThanOrEqual", text: "5" }],
    [{ op: "contains", text: "ab" }, { text: "*ab*" }],
    [{ op: "notContains", text: "ab" }, { op: "notEqual", text: "*ab*" }],
    [{ op: "startsWith", text: "ab" }, { text: "ab*" }],
    [{ op: "endsWith", text: "ab" }, { text: "*ab" }],
    [{ op: "empty", text: "" }, { text: "" }],
    [{ op: "notEmpty", text: "" }, { op: "notEqual", text: " " }],
  ];
  for (const [form, condition] of forms) {
    assert.deepEqual(core.toCondition(form), condition, form.op);
    assert.deepEqual(core.toForm(condition), form, form.op);
  }
  assert.equal(core.toCondition({ op: "none", text: "" }), null);
  // A condition from a file: an explicit `equal` is the form's "equals".
  assert.deepEqual(core.toForm({ op: "equal", text: "7" }), { op: "equals", text: "7" });
  // The dialog's operator list is the list of the page.
  const html = fs.readFileSync(UI("index.html"), "utf8");
  for (const id of ["sheet-filter-op1", "sheet-filter-op2"]) {
    const select = new RegExp(`<select id="${id}"[^>]*>(.*?)</select>`).exec(html)[1];
    const options = [...select.matchAll(/<option value="([^"]+)">([^<]+)<\/option>/g)].map((m) => [m[1], m[2]]);
    assert.deepEqual(options, core.OPERATORS.map(([v, label]) => [v, label]), id);
  }
});

test("two rows of the dialog become one or two conditions, AND or OR", () => {
  const two = core.criteriaFromForms(true, [{ op: "greaterThanOrEqual", text: "10" }, { op: "lessThanOrEqual", text: "20" }]);
  assert.deepEqual(two.criteria, {
    and: true,
    conditions: [{ op: "greaterThanOrEqual", text: "10" }, { op: "lessThanOrEqual", text: "20" }],
  });
  const or = core.criteriaFromForms(false, [{ op: "startsWith", text: "a" }, { op: "endsWith", text: "z" }]);
  assert.deepEqual(or.criteria, { conditions: [{ text: "a*" }, { text: "*z" }] });
  // A single condition carries no `and`, and an empty first row falls to the second.
  assert.deepEqual(core.criteriaFromForms(true, [{ op: "none", text: "" }, { op: "empty", text: "" }]).criteria, { conditions: [{ text: "" }] });
  assert.equal(core.criteriaFromForms(true, [{ op: "none", text: "" }, { op: "none", text: "" }]).criteria, null);
  assert.ok(core.criteriaFromForms(true, [{ op: "equals", text: "" }, { op: "none", text: "" }]).error);
  assert.ok(core.criteriaFromForms(true, [{ op: "contains", text: "x".repeat(254) }, { op: "none", text: "" }]).error, "the wildcards count");
  // The dialog shows a stored column as it was entered.
  const forms = core.formsFromCriteria(two.criteria);
  assert.equal(forms.and, true);
  assert.deepEqual(forms.rows.map((r) => r.op), ["greaterThanOrEqual", "lessThanOrEqual"]);
  assert.deepEqual(core.formsFromCriteria(null).rows.map((r) => r.op), ["none", "none"]);
});

test("a filter range is the region around a cell, the rows below a row or the selection itself", () => {
  const filled = new Set(["B2", "C2", "D2", "B3", "C3", "D3", "B4", "C4", "D4", "B5", "D5"]);
  const col = (c) => String.fromCharCode(64 + c);
  const has = (r, c) => filled.has(col(c) + r);
  const extent = { rows: 20, cols: 8 };
  assert.deepEqual(core.regionFrom({ r1: 3, c1: 3, r2: 3, c2: 3 }, has, extent), { r1: 2, c1: 2, r2: 5, c2: 4 });
  assert.deepEqual(core.regionFrom({ r1: 2, c1: 2, r2: 2, c2: 3 }, has, extent), { r1: 2, c1: 2, r2: 5, c2: 3 }, "one row reaches down");
  assert.deepEqual(core.regionFrom({ r1: 2, c1: 2, r2: 3, c2: 3 }, has, extent), { r1: 2, c1: 2, r2: 3, c2: 3 }, "a block stays");
  assert.deepEqual(core.regionFrom({ r1: 9, c1: 9, r2: 9, c2: 9 }, has, extent), { r1: 9, c1: 9, r2: 9, c2: 9 }, "an empty cell stays alone");
  assert.deepEqual(core.rangeOfAxisSelection({ axis: "col", ac: 3, cc: 2 }, { rows: 7, cols: 5 }), { r1: 1, c1: 2, r2: 7, c2: 3 });
  assert.deepEqual(core.rangeOfAxisSelection({ axis: "row", ar: 4, cr: 4 }, { rows: 7, cols: 5 }), { r1: 4, c1: 1, r2: 4, c2: 5 });
});

async function layoutApi() {
  const { host } = await createWorkspaceHarness();
  return host.__EASYOFFICE_SHEET_LAYOUT__;
}

// The rows hidden by `user` and `filter` runs, one by one.
function hiddenSet(user, filter) {
  const set = new Set();
  for (const [a, b] of [...user, ...filter]) for (let r = a; r <= b; r += 1) set.add(r);
  return set;
}

test("the row lookups over the user's and the filter's hidden rows agree with a row-by-row scan", async () => {
  const layout = await layoutApi();
  let seed = 12345;
  const random = (n) => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed % n;
  };
  const runs = (count, span) => {
    const out = [];
    let at = 1 + random(5);
    for (let i = 0; i < count; i += 1) {
      const len = 1 + random(span);
      out.push([at, at + len - 1]);
      at += len + 1 + random(span);
    }
    return out;
  };
  for (const [uc, fc, span] of [[0, 0, 3], [3, 0, 4], [0, 40, 3], [20, 200, 5], [1, 1, 1]]) {
    const user = runs(uc, span);
    const filter = runs(fc, span);
    layout.replaceAll({ S: { hiddenRows: user, filter: { range: "A1:B9999", columns: [], hidden: filter } } });
    const set = hiddenSet(user, filter);
    let hidden = 0;
    const visible = [];
    for (let r = 1; r <= 1200; r += 1) {
      assert.equal(layout.rowsHiddenBefore("S", r), hidden, `hidden before ${r} (${uc},${fc})`);
      assert.equal(layout.rowHidden("S", r), set.has(r), `row ${r}`);
      if (set.has(r)) hidden += 1;
      else visible.push(r);
    }
    visible.slice(0, 400).forEach((row, v) => assert.equal(layout.rowAtVisible("S", v), row, `visible ${v}`));
    assert.equal(layout.rowsHiddenTotal("S"), set.size);
  }
  // A sheet without a layout hides nothing.
  assert.equal(layout.rowsHiddenBefore("None", 50), 0);
  assert.equal(layout.rowAtVisible("None", 7), 8);
});

test("the filter's hidden rows stay apart from the user's through the layout JSON", async () => {
  const layout = await layoutApi();
  const json = {
    hiddenRows: [[5, 5]],
    filter: { range: "A1:C9", columns: [{ col: 2, values: ["a"], blank: true }], hidden: [[3, 4], [7, 8]] },
  };
  layout.replaceAll({ S: json });
  assert.deepEqual(layout.getAll(), { S: json });
  assert.deepEqual(layout.get("S").hiddenRows, [[5, 5]]);
  assert.deepEqual(layout.filterOf("S").range, { r1: 1, c1: 1, r2: 9, c2: 3 });
  // Unhiding the user's row leaves the filter's rows hidden.
  layout.setHidden("S", "row", 5, 5, false);
  assert.equal(layout.rowHidden("S", 5), false);
  assert.equal(layout.rowHidden("S", 4), true);
  // Removing the filter shows its rows and empties a layout that has nothing else.
  layout.setFilter("S", null);
  assert.equal(layout.rowHidden("S", 4), false);
  assert.deepEqual(layout.getAll(), {});
});

// ---- the commands and the panel on a sheet -------------------------------------------------------

const MODEL = {
  sheets: [
    {
      name: "표",
      cells: [
        { ref: "A1", raw: "부서" },
        { ref: "B1", raw: "매출" },
        ...[
          ["영업", "10"],
          ["개발", "20"],
          ["영업", "30"],
          ["기획", "40"],
          ["영업", "50"],
          ["개발", ""],
        ].flatMap(([dept, sales], i) => [
          { ref: "A" + (i + 2), raw: dept },
          ...(sales ? [{ ref: "B" + (i + 2), raw: sales }] : []),
        ]),
      ],
    },
  ],
};

// A stand-in for the engine's `filter` action: values lists, `greaterThan` and `*x*` conditions.
function judge(criteria, text) {
  if (criteria.values) return text === "" ? criteria.blank === true : criteria.values.includes(text);
  const one = (c) => {
    if (c.op === "greaterThan") return text !== "" && Number(text) > Number(c.text);
    if (c.op === "lessThan") return text !== "" && Number(text) < Number(c.text);
    return new RegExp("^" + c.text.replaceAll("*", ".*") + "$", "i").test(text);
  };
  const [a, b] = criteria.conditions;
  return b ? (criteria.and ? one(a) && one(b) : one(a) || one(b)) : one(a);
}

function fakeFilterEngine() {
  return (body) => {
    const rows = body.data[0]?.values.length ?? 0;
    const column = (col) => body.data.find((d) => d.col === col).values;
    const hiddenBy = (skip) => {
      const hidden = new Set();
      for (const c of body.filter.columns) {
        if (c.col === skip) continue;
        column(c.col).forEach((text, i) => {
          if (!judge(c, text)) hidden.add(i);
        });
      }
      return hidden;
    };
    if (body.op === "apply") {
      const hidden = hiddenBy(-1);
      const base = Number(/^[A-Z]+(\d+)/.exec(body.filter.range)[1]) + 1;
      const runs = [];
      for (let i = 0; i < rows; i += 1) {
        if (!hidden.has(i)) continue;
        const last = runs[runs.length - 1];
        if (last && last[1] === base + i - 1) last[1] = base + i;
        else runs.push([base + i, base + i]);
      }
      return { hidden: runs, count: hidden.size };
    }
    const excluded = hiddenBy(body.col);
    const counts = new Map();
    let blank = 0;
    column(body.col).forEach((text, i) => {
      if (excluded.has(i)) return;
      if (text === "") blank += 1;
      else if (!body.query || text.toLowerCase().includes(body.query.toLowerCase())) counts.set(text, (counts.get(text) || 0) + 1);
    });
    const items = [...counts].sort((a, b) => a[0].localeCompare(b[0], "ko"));
    return { items, blank: body.query ? 0 : blank, more: false };
  };
}

async function createHarness(override = () => undefined, model = MODEL) {
  const requests = [];
  const fake = fakeFilterEngine();
  const engine = (body) => {
    requests.push(body);
    return override(body) ?? fake(body);
  };
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
      if (body?.sessionAction === "filter") return engine(body);
      return offline[command](payload);
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(harness.document, makeFixtureFile("필터.xlsx", model));
  await flushAsyncTurns(8);
  return { ...harness, requests };
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const act = (d, name) => d.querySelector(`.md-item[data-sheet-act="${name}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const undo = (d) => d.querySelector('.md-item[data-studio-menu-cmd="studio-undo"]');
const feedback = (d) => d.getElementById("status-command-feedback").textContent;
// The rows of the sheet that have no row in the document: a hidden row is not drawn at all.
const hiddenRows = (d, last = 7) => {
  const drawn = new Set([...d.querySelectorAll("#pane-sheet tbody tr")].map((tr) => Number(tr.dataset.row)));
  return Array.from({ length: last }, (_, i) => i + 1).filter((r) => !drawn.has(r));
};

async function click(h, el) {
  h.userClick(el);
  await flushAsyncTurns(14);
}

test("the 필터 menu item turns the filter on over the region, marks the header cells and turns it off", async () => {
  const h = await createHarness();
  const d = h.document;
  assert.equal(act(d, "filter-toggle").getAttribute("aria-checked"), "false");
  await click(h, act(d, "filter-reapply"));
  assert.equal(feedback(d), "켜진 필터 없음");
  await click(h, act(d, "filter-clear"));
  assert.equal(feedback(d), "지울 필터 조건 없음");
  cell(d, "A3").focus();
  await click(h, act(d, "filter-toggle"));
  assert.equal(feedback(d), "필터 켬");
  assert.equal(act(d, "filter-toggle").getAttribute("aria-checked"), "true");
  assert.ok(cell(d, "A1").classList.contains("filter-head") && cell(d, "B1").classList.contains("filter-head"));
  assert.equal(cell(d, "A2").classList.contains("filter-head"), false, "only the header row");
  assert.equal(cell(d, "A1").classList.contains("filter-on"), false);
  await click(h, act(d, "filter-toggle"));
  assert.equal(feedback(d), "필터 해제");
  assert.equal(cell(d, "A1").classList.contains("filter-head"), false);
  // One undo brings the filter back, a second takes it away again.
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  assert.equal(layout.filterOf("표"), null);
  await click(h, undo(d));
  assert.deepEqual(layout.filterOf("표").range, { r1: 1, c1: 1, r2: 7, c2: 2 });
  await click(h, undo(d));
  assert.equal(layout.filterOf("표"), null);
  assert.equal(act(d, "filter-toggle").getAttribute("aria-checked"), "false");
});

test("a value condition hides rows apart from the user's, shows on the header and one undo reverts it", async () => {
  const h = await createHarness();
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  // The user hides row 7 (개발, no sales) by hand first.
  layout.setHidden("표", "row", 7, 7, true);
  const before = layout.getAll();
  const panelOpen = async (col) => {
    cell(d, col + "1").focus();
    const td = cell(d, col + "1");
    td.getBoundingClientRect = () => ({ left: 0, right: 100, top: 0, bottom: 24 });
    td.dispatchEvent({ type: "mousedown", target: td, button: 0, clientX: 95, clientY: 20, bubbles: true });
    await flushAsyncTurns(10);
  };
  await panelOpen("A");
  const panel = d.getElementById("sheet-filter-panel");
  assert.equal(panel.hidden, false);
  assert.equal(d.getElementById("sheet-filter-count").textContent, "3 / 3", "영업, 개발, 기획");
  const rows = () => [...d.querySelectorAll("#sheet-filter-rows .sheet-filter-row")].filter((r) => !r.hidden);
  assert.deepEqual(rows().map((r) => r.children[2].textContent), ["(모두 선택)", "개발", "기획", "영업"]);
  assert.deepEqual(rows().map((r) => r.children[3].textContent), ["", "2", "1", "3"]);
  // Uncheck 개발 and 기획, then confirm: only 영업 stays.
  await click(h, rows()[1]);
  await click(h, rows()[2]);
  assert.equal(d.getElementById("sheet-filter-count").textContent, "1 / 3");
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.equal(panel.hidden, true);
  assert.deepEqual(layout.filterOf("표").hidden, [[3, 3], [5, 5], [7, 7]]);
  assert.deepEqual(layout.get("표").hiddenRows, [[7, 7]], "the user's row stays the user's");
  assert.deepEqual(hiddenRows(d), [3, 5, 7]);
  assert.ok(cell(d, "A1").classList.contains("filter-on"));
  assert.equal(feedback(d), "필터: 6행 중 3행 표시");
  // Row numbers of the filtered range show in the accent color; the header row's own number does not.
  const rowHead = (r) => d.querySelector(`#pane-sheet tbody tr[data-row="${r}"] th`);
  assert.ok(rowHead(2).classList.contains("filter-row") && rowHead(4).classList.contains("filter-row"));
  assert.equal(rowHead(1).classList.contains("filter-row"), false);
  assert.equal(d.getElementById("sheet-filter-dialog-title").textContent, d.querySelector('[data-filter-act="custom"] .md-label').textContent.replace("…", ""), "the dialog is named like its menu row");
  const apply = h.requests.filter((r) => r.op === "apply").at(-1);
  assert.deepEqual(apply.filter.columns, [{ col: 1, values: ["영업"] }]);
  assert.equal(apply.data.length, 1, "only the column that has a condition is sent");
  // The list of the other column leaves out the rows column A hides.
  await panelOpen("B");
  assert.deepEqual(rows().map((r) => r.children[2].textContent), ["(모두 선택)", "10", "30", "50"]);
  d.getElementById("sheet-filter-panel").hidden = true;
  // Editing a cell does not move the rows; "다시 적용" judges the cells as they are now.
  await click(h, act(d, "filter-reapply"));
  assert.equal(feedback(d), "필터: 6행 중 3행 표시");
  // One undo brings every hidden row back except the user's, and drops the condition.
  await click(h, undo(d));
  assert.deepEqual(layout.filterOf("표").columns, []);
  assert.deepEqual(layout.filterOf("표").hidden, []);
  assert.deepEqual(layout.get("표").hiddenRows, [[7, 7]]);
  assert.deepEqual(hiddenRows(d), [7]);
  assert.deepEqual(layout.getAll(), before, "the undo restored the state from before the condition");
});

test("the sheet's filter is not turned on over a table, and is over the cells next to one (PD-224)", async () => {
  const h = await createHarness();
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const before = layout.getAll();
  layout.replaceAll({ ...before, 표: { ...before["표"], tables: [{ name: "T", range: "A1:B7", header: true, totals: false, columns: ["a", "b"] }] } });
  cell(d, "A3").focus();
  await click(h, act(d, "filter-toggle"));
  assert.equal(feedback(d), "표와 겹치는 범위에는 필터를 켤 수 없음");
  assert.equal(layout.filterOf("표"), null);
  assert.equal(act(d, "filter-toggle").getAttribute("aria-checked"), "false");
  layout.replaceAll(before);
});

test("the custom condition dialog sets a condition from two rows and the column shows it again", async () => {
  const h = await createHarness();
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  cell(d, "B2").focus();
  await click(h, act(d, "filter-toggle"));
  const td = cell(d, "B1");
  td.getBoundingClientRect = () => ({ left: 0, right: 100, top: 0, bottom: 24 });
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, clientX: 95, clientY: 20, bubbles: true });
  await flushAsyncTurns(10);
  await click(h, d.querySelector('[data-filter-act="custom"]'));
  const dialog = d.getElementById("sheet-filter-dialog");
  assert.equal(dialog.open, true);
  const set = (id, value) => {
    d.getElementById(id).value = value;
    d.getElementById(id).dispatchEvent({ type: "change", target: d.getElementById(id), bubbles: true });
  };
  set("sheet-filter-op1", "greaterThan");
  set("sheet-filter-text1", "15");
  set("sheet-filter-op2", "lessThan");
  set("sheet-filter-text2", "45");
  d.getElementById("sheet-filter-and").checked = true;
  await click(h, d.getElementById("btn-sheet-filter-apply"));
  assert.equal(dialog.open, false);
  assert.deepEqual(layout.filterOf("표").columns, [
    { col: 2, and: true, conditions: [{ op: "greaterThan", text: "15" }, { op: "lessThan", text: "45" }] },
  ]);
  // 10 and 50 and the empty cell fall outside (15, 45).
  assert.deepEqual(layout.filterOf("표").hidden, [[2, 2], [6, 7]]);
  assert.ok(cell(d, "B1").classList.contains("filter-on"));
  // The dialog shows the stored condition again.
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, clientX: 95, clientY: 20, bubbles: true });
  await flushAsyncTurns(10);
  await click(h, d.querySelector('[data-filter-act="custom"]'));
  assert.equal(d.getElementById("sheet-filter-op1").value, "greaterThan");
  assert.equal(d.getElementById("sheet-filter-text2").value, "45");
  assert.equal(d.getElementById("sheet-filter-and").checked, true);
});

async function openPanel(h, col) {
  const td = cell(h.document, col + "1");
  td.getBoundingClientRect = () => ({ left: 0, right: 100, top: 0, bottom: 24 });
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, clientX: 95, clientY: 20, bubbles: true });
  await flushAsyncTurns(10);
}

const shownRows = (d) => [...d.querySelectorAll("#sheet-filter-rows .sheet-filter-row")].filter((r) => !r.hidden);
const labels = (d) => shownRows(d).map((r) => r.children[2].textContent);

test("a press away from the corner of a header cell selects the cell and does not open the panel", async () => {
  const h = await createHarness();
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  const td = cell(d, "A1");
  td.getBoundingClientRect = () => ({ left: 0, right: 100, top: 0, bottom: 24 });
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, clientX: 20, clientY: 10, bubbles: true });
  await flushAsyncTurns(6);
  assert.equal(d.getElementById("sheet-filter-panel").hidden, true);
});

test("the list keeps only the rows in view, however many values a column has", async () => {
  const many = 6000;
  const h = await createHarness((body) =>
    body.op === "values" ? { items: Array.from({ length: many }, (_, i) => ["값" + String(i).padStart(5, "0"), 1]), blank: 4, more: false } : undefined
  );
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "A");
  assert.equal(d.getElementById("sheet-filter-count").textContent, many + 1 + " / " + (many + 1), "the blank row counts");
  assert.ok(shownRows(d).length <= 20, "rows in the document: " + shownRows(d).length);
  assert.equal(labels(d)[0], "(모두 선택)");
  assert.equal(d.getElementById("sheet-filter-rows").style.height, (many + 2) * 22 + "px");
  // Scrolling moves the window: the first row now shown is far down the list.
  const list = d.getElementById("sheet-filter-list");
  list.scrollTop = 3000 * 22;
  list.dispatchEvent({ type: "scroll", target: list });
  assert.ok(shownRows(d).length <= 20);
  assert.ok(labels(d).includes("값02999") || labels(d).includes("값02998"), labels(d).join());
  assert.ok(!labels(d).includes("(모두 선택)"));
});

test("a list cut at its limit is read-only until a search narrows it", async () => {
  const h = await createHarness((body) =>
    body.op === "values"
      ? body.query
        ? { items: [["가1", 2], ["가2", 1]], blank: 0, more: false }
        : { items: [["가1", 2], ["가2", 1], ["나", 1]], blank: 0, more: true }
      : undefined
  );
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "A");
  assert.equal(d.getElementById("sheet-filter-count").textContent, "3 / 3+");
  assert.equal(shownRows(d)[1].getAttribute("aria-disabled"), "true");
  assert.equal(d.getElementById("btn-sheet-filter-ok").disabled, true);
  await click(h, shownRows(d)[1]);
  assert.equal(d.getElementById("sheet-filter-count").textContent, "3 / 3+", "a click on a locked row does nothing");
  // The search lists what matches; its rows are checked and the list can be confirmed.
  const ui = d.getElementById("sheet-filter-search");
  ui.value = "가";
  ui.dispatchEvent({ type: "input", target: ui });
  h.advanceTimers(300);
  await flushAsyncTurns(10);
  assert.equal(d.getElementById("sheet-filter-count").textContent, "2 / 2");
  assert.equal(d.getElementById("btn-sheet-filter-ok").disabled, false);
  assert.equal(h.requests.at(-1).query, "가");
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  assert.deepEqual(layout.filterOf("표").columns, [{ col: 1, values: ["가1", "가2"] }]);
});

test("the panel is driven from the keyboard: Alt+Down opens it, arrows and space check, Escape closes", async () => {
  const h = await createHarness();
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  cell(d, "A1").focus();
  key(cell(d, "A1"), "ArrowDown", { altKey: true });
  await flushAsyncTurns(10);
  const panel = d.getElementById("sheet-filter-panel");
  assert.equal(panel.hidden, false);
  assert.equal(d.activeElement.id, "sheet-filter-search", "the search field takes the focus");
  // Down from the search field moves into the list, on "(모두 선택)".
  key(d.getElementById("sheet-filter-search"), "ArrowDown");
  assert.equal(d.activeElement.id, "sheet-filter-list");
  assert.equal(d.getElementById("sheet-filter-list").getAttribute("aria-activedescendant"), "sheet-filter-row-0");
  key(d.getElementById("sheet-filter-list"), "ArrowDown");
  assert.equal(d.getElementById("sheet-filter-list").getAttribute("aria-activedescendant"), "sheet-filter-row-1");
  key(d.getElementById("sheet-filter-list"), " ");
  assert.equal(d.getElementById("sheet-filter-count").textContent, "2 / 3", "개발 is unchecked");
  // Up from the first row goes back to the search field, up from there to the last menu row.
  key(d.getElementById("sheet-filter-list"), "ArrowUp");
  key(d.getElementById("sheet-filter-list"), "ArrowUp");
  assert.equal(d.activeElement.id, "sheet-filter-search");
  key(d.getElementById("sheet-filter-search"), "ArrowUp");
  assert.equal(d.activeElement.dataset.filterAct, "custom", "the clear row is disabled while the column has no condition");
  key(d.activeElement, "ArrowUp");
  assert.equal(d.activeElement.dataset.filterAct, "sort-desc");
  key(d.activeElement, "ArrowDown");
  key(d.activeElement, "ArrowDown");
  assert.equal(d.activeElement.id, "sheet-filter-search");
  key(d.getElementById("sheet-filter-search"), "Enter");
  await flushAsyncTurns(10);
  assert.equal(panel.hidden, true);
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  assert.deepEqual(layout.filterOf("표").columns, [{ col: 1, values: ["기획", "영업"] }]);
  // Escape closes without changing anything.
  const before = layout.getAll();
  key(cell(d, "A1"), "ArrowDown", { altKey: true });
  await flushAsyncTurns(10);
  key(d.getElementById("sheet-filter-search"), "Escape");
  assert.equal(panel.hidden, true);
  assert.deepEqual(layout.getAll(), before);
});

test("choosing every value clears the column's condition", async () => {
  const h = await createHarness();
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "A");
  await click(h, shownRows(d)[1]);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.equal(layout.filterOf("표").columns.length, 1);
  await openPanel(h, "A");
  assert.equal(d.getElementById("sheet-filter-count").textContent, "2 / 3");
  await click(h, shownRows(d)[0]);
  assert.equal(d.getElementById("sheet-filter-count").textContent, "3 / 3");
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.deepEqual(layout.filterOf("표").columns, []);
  assert.deepEqual(layout.filterOf("표").hidden, []);
  // "필터 지우기" removes the conditions and shows every row again.
  await openPanel(h, "A");
  await click(h, shownRows(d)[1]);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  await click(h, act(d, "filter-clear"));
  assert.deepEqual(layout.filterOf("표").columns, []);
  assert.equal(feedback(d), "필터 지움");
});

test("sorting from the panel sorts the data rows below the header and applies the filter again", async () => {
  const h = await createHarness();
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "A");
  await click(h, shownRows(d)[1]);
  await click(h, shownRows(d)[2]);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.deepEqual(layout.filterOf("표").hidden, [[3, 3], [5, 5], [7, 7]], "영업 only: rows 2, 4 and 6 stay");
  await openPanel(h, "B");
  await click(h, d.querySelector('[data-filter-act="sort-desc"]'));
  assert.equal(feedback(d), "내림차순 정렬");
  assert.equal(h.requests.at(-1).op, "apply", "the filter was judged again after the sort");
  assert.deepEqual(layout.filterOf("표").hidden, [[3, 3], [5, 5], [7, 7]], "영업 is now on rows 2, 4 and 6");
  assert.deepEqual([2, 4, 6].map((r) => cell(d, "B" + r).textContent), ["50", "30", "10"], "the visible rows are sorted");
  // With the filter cleared every row is drawn: the visible rows moved whole, the hidden rows
  // (3, 5 and 7) kept their place and the header stayed.
  await click(h, act(d, "filter-clear"));
  const column = (c) => [2, 3, 4, 5, 6, 7].map((r) => cell(d, c + r).textContent);
  assert.deepEqual(column("B"), ["50", "20", "30", "40", "10", ""]);
  assert.deepEqual(column("A"), ["영업", "개발", "영업", "기획", "영업", "개발"], "the visible rows moved whole");
  assert.equal(cell(d, "A1").textContent, "부서", "the header row stays");
});

test("undo steps of a filter change keep the cells as they are and a sort step puts the rows back", async () => {
  const h = await createHarness();
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "B");
  await click(h, d.querySelector('[data-filter-act="sort-desc"]'));
  const column = (c) => [2, 3, 4, 5, 6, 7].map((r) => cell(d, c + r).textContent);
  assert.deepEqual(column("B"), ["50", "40", "30", "20", "10", ""]);
  await openPanel(h, "A");
  await click(h, shownRows(d)[1]);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.equal(layout.filterOf("표").columns.length, 1);
  await click(h, undo(d));
  assert.deepEqual(layout.filterOf("표").columns, []);
  assert.deepEqual(column("B"), ["50", "40", "30", "20", "10", ""], "the sort stays");
  await click(h, undo(d));
  assert.deepEqual(column("B"), ["10", "20", "30", "40", "50", ""], "the sort is undone");
  assert.notEqual(layout.filterOf("표"), null, "the filter is still on");
  await click(h, undo(d));
  assert.equal(layout.filterOf("표"), null);
  // Redo walks forward again.
  const redo = d.querySelector('.md-item[data-studio-menu-cmd="studio-redo"]');
  await click(h, redo);
  await click(h, redo);
  assert.deepEqual(column("B"), ["50", "40", "30", "20", "10", ""]);
  await click(h, redo);
  assert.equal(layout.filterOf("표").columns.length, 1);
});

// ---- the conditions of PD-226: top and average in the dialog, the range that grows, a table's own filter ----------------

test("the statistic operators of the dialog are the whole criterion of a column", () => {
  const top = core.criteriaFromForms(true, [{ op: "top", text: "10" }, { op: "none", text: "" }]);
  assert.deepEqual(top.criteria, { top: { count: 10 } });
  assert.deepEqual(core.criteriaFromForms(true, [{ op: "none", text: "" }, { op: "bottomPercent", text: " 25 " }]).criteria, {
    top: { bottom: true, percent: true, count: 25 },
  });
  assert.deepEqual(core.criteriaFromForms(true, [{ op: "topPercent", text: "100" }, { op: "none", text: "" }]).criteria, { top: { percent: true, count: 100 } });
  assert.deepEqual(core.criteriaFromForms(false, [{ op: "aboveAverage", text: "" }, { op: "none", text: "" }]).criteria, { average: { above: true } });
  assert.deepEqual(core.criteriaFromForms(false, [{ op: "none", text: "" }, { op: "belowAverage", text: "x" }]).criteria, { average: { above: false } });
  // A count must be a whole number in range, and a statistic is never one of two conditions.
  for (const text of ["", "0", "-1", "1.5", "abc", "1000001"]) {
    assert.ok(core.criteriaFromForms(true, [{ op: "top", text }, { op: "none", text: "" }]).error, "items " + text);
  }
  assert.ok(core.criteriaFromForms(true, [{ op: "topPercent", text: "101" }, { op: "none", text: "" }]).error);
  assert.ok(core.criteriaFromForms(true, [{ op: "top", text: "3" }, { op: "greaterThan", text: "1" }]).error);
  assert.ok(core.criteriaFromForms(true, [{ op: "aboveAverage", text: "" }, { op: "belowAverage", text: "" }]).error);
  // The dialog shows a stored statistic as it was entered, and keeps the second row empty.
  for (const [criteria, row] of [
    [{ top: { count: 7 } }, { op: "top", text: "7" }],
    [{ top: { bottom: true, count: 7 } }, { op: "bottom", text: "7" }],
    [{ top: { percent: true, count: 5 } }, { op: "topPercent", text: "5" }],
    [{ top: { bottom: true, percent: true, count: 5 } }, { op: "bottomPercent", text: "5" }],
    [{ average: { above: true } }, { op: "aboveAverage", text: "" }],
    [{ average: { above: false } }, { op: "belowAverage", text: "" }],
  ]) {
    const forms = core.formsFromCriteria(criteria);
    assert.deepEqual(forms.rows, [row, { op: "none", text: "" }]);
    assert.deepEqual(core.criteriaFromForms(forms.and, forms.rows).criteria, criteria);
  }
  assert.deepEqual([core.paramsOf("top"), core.paramsOf("bottomPercent"), core.paramsOf("aboveAverage")], [1, 1, 0]);
});

test("a table's filter hides rows apart from the user's, through the layout JSON", async () => {
  const layout = await layoutApi();
  const json = {
    hiddenRows: [[5, 5]],
    tables: [{ name: "T", range: "A1:C9", header: true, columns: ["a", "b", "c"], filter: [{ col: 1, values: ["x"] }], filterHidden: [[3, 4], [7, 8]] }],
  };
  layout.replaceAll({ S: json });
  assert.deepEqual(layout.getAll(), { S: json });
  assert.deepEqual([3, 4, 5, 6, 7, 8].map((r) => layout.rowHidden("S", r)), [true, true, true, false, true, true]);
  // Unhiding the user's row leaves the table's rows hidden; clearing the table's rows shows them.
  layout.setHidden("S", "row", 5, 5, false);
  assert.equal(layout.rowHidden("S", 5), false);
  assert.equal(layout.rowHidden("S", 4), true);
  assert.equal(layout.setTableFilterHidden("S", "T", []), true);
  assert.equal(layout.rowHidden("S", 4), false);
  assert.equal(layout.tablesOf("S")[0].filterHidden, undefined);
  assert.equal(layout.setTableFilterHidden("S", "Nope", [[2, 2]]), false);
  assert.equal(layout.setTableFilterHidden("S", "T", [[6, 6]]), true);
  assert.equal(layout.rowHidden("S", 6), true);
});

const GROWN = {
  sheets: [
    {
      name: "표",
      cells: [
        { ref: "A1", raw: "부서" },
        { ref: "B1", raw: "매출" },
        ...[["영업", "10"], ["개발", "20"], ["영업", "30"], ["기획", "40"], ["영업", "50"], ["개발", "60"]].flatMap(([dept, sales], i) => [
          { ref: "A" + (i + 2), raw: dept },
          { ref: "B" + (i + 2), raw: sales },
        ]),
        // Rows written below the range: 8 and 9 are next to it, 11 is after an empty row.
        { ref: "A8", raw: "영업" },
        { ref: "B8", raw: "70" },
        { ref: "A9", raw: "기획" },
        { ref: "A11", raw: "영업" },
      ],
    },
  ],
};

test("applying again takes in the rows written right below the range, up to an empty row", async () => {
  const h = await createHarness(undefined, GROWN);
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.setFilter("표", { range: { r1: 1, c1: 1, r2: 7, c2: 2 }, columns: [{ col: 1, values: ["영업"] }], hidden: [] });
  await click(h, act(d, "filter-reapply"));
  assert.deepEqual(layout.filterOf("표").range, { r1: 1, c1: 1, r2: 9, c2: 2 });
  const sent = h.requests.at(-1);
  assert.equal(sent.filter.range, "A1:B9");
  assert.equal(sent.data[0].values.length, 8, "rows 2 to 9");
  assert.deepEqual(layout.filterOf("표").hidden, [[3, 3], [5, 5], [7, 7], [9, 9]], "the row 9 of 기획 is judged too");
  // Nothing below grows it again.
  await click(h, act(d, "filter-reapply"));
  assert.deepEqual(layout.filterOf("표").range, { r1: 1, c1: 1, r2: 9, c2: 2 });
});

test("choosing a condition also takes in the rows written below, and one undo gives the range back", async () => {
  const h = await createHarness(undefined, GROWN);
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.setFilter("표", { range: { r1: 1, c1: 1, r2: 7, c2: 2 }, columns: [], hidden: [] });
  cell(d, "A1").focus();
  key(cell(d, "A1"), "ArrowDown", { altKey: true });
  await flushAsyncTurns(10);
  await click(h, shownRows(d)[1]);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.equal(layout.filterOf("표").range.r2, 9);
  await click(h, undo(d));
  assert.equal(layout.filterOf("표").range.r2, 7);
});

test("the range does not grow into a table", async () => {
  const h = await createHarness(undefined, GROWN);
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({
    표: {
      filter: { range: "A1:B7", columns: [{ col: 1, values: ["영업"] }] },
      tables: [{ name: "T", range: "A9:B10", header: true, columns: ["a", "b"] }],
    },
  });
  await click(h, act(d, "filter-reapply"));
  assert.equal(layout.filterOf("표").range.r2, 8, "row 9 belongs to the table");
});

test("applying again judges the conditions of a table's filter and keeps its rows apart from the user's", async () => {
  const h = await createHarness(undefined, GROWN);
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({
    표: {
      hiddenRows: [[2, 2]],
      tables: [{ name: "T", range: "A1:B7", header: true, columns: ["부서", "매출"], filter: [{ col: 1, values: ["영업"] }] }],
    },
  });
  await click(h, act(d, "filter-reapply"));
  const table = layout.tablesOf("표")[0];
  assert.deepEqual(table.filterHidden, [[3, 3], [5, 5], [7, 7]]);
  assert.equal(h.requests.at(-1).filter.range, "A1:B7");
  assert.deepEqual(layout.get("표").hiddenRows, [[2, 2]], "the user's row stays the user's");
  assert.deepEqual([2, 3, 4, 5].map((r) => layout.rowHidden("표", r)), [true, true, false, true]);
  // Conditions of the table are what a later apply reads: change them and judge again.
  table.filter = [{ col: 1, values: ["개발"] }];
  await click(h, act(d, "filter-reapply"));
  assert.deepEqual(layout.tablesOf("표")[0].filterHidden, [[2, 2], [4, 6]]);
  // A table whose filter has no condition, or no header row, is not judged.
  layout.replaceAll({ 표: { tables: [{ name: "U", range: "A1:B7", header: false, columns: ["a", "b"], filter: [{ col: 1, values: ["x"] }] }] } });
  const before = h.requests.length;
  await click(h, act(d, "filter-reapply"));
  assert.equal(feedback(d), "켜진 필터 없음");
  assert.equal(h.requests.length, before);
});

// Packed appearances as the engine sends them (`pack.rs`): rows and heads are little-endian words.
function packedFill(cells, rgb) {
  const words = cells.flatMap(([row, col]) => [row, col | (1 << 18), 0]);
  return { pal: [rgb], w: Buffer.from(new Uint8Array(new Uint32Array(words).buffer)).toString("base64") };
}

test("the colours of a column are sent as runs for a colour condition, the rules' over the cell's own, and only then", async () => {
  const h = await createHarness((body) => (body.op === "apply" ? { hidden: [[3, 3]], count: 1 } : undefined));
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const rules = h.host.__EASYOFFICE_SHEET_RULES__;
  // A2, A3 and A5 (0-based rows 1, 2, 4 of column 0) are filled.
  rules.applyPacked("표", packedFill([[1, 0], [2, 0], [4, 0]], 0xffc7ce), true);
  assert.deepEqual(rules.colorRuns("표", 1, 2, 7), { fill: [[0, 1, "ffc7ce"], [3, 3, "ffc7ce"]], font: [] });
  assert.equal(rules.colorRuns("표", 2, 2, 7), null, "no colour in column B");
  layout.replaceAll({ 표: { filter: { range: "A1:B7", columns: [{ col: 1, color: { rgb: "ffc7ce" } }] } } });
  await click(h, act(d, "filter-reapply"));
  const sent = h.requests.at(-1);
  assert.deepEqual(sent.filter.columns, [{ col: 1, color: { rgb: "ffc7ce" } }]);
  assert.deepEqual(sent.data[0].fill, [[0, 1, "ffc7ce"], [3, 3, "ffc7ce"]]);
  assert.deepEqual(sent.data[0].font, [], "both kinds are told: a column with no text colour says so (PD-227)");
  assert.deepEqual(layout.filterOf("표").hidden, [[3, 3]]);

  // A column with a value list has no colour in its request.
  layout.replaceAll({ 표: { filter: { range: "A1:B7", columns: [{ col: 1, values: ["영업"] }] } } });
  await click(h, act(d, "filter-reapply"));
  assert.equal(h.requests.at(-1).data[0].fill, undefined);
});

test("a colour condition and the colour list read the colours of the cells, a rule's colour winning over the cell's (PD-227)", async () => {
  const cells = [
    { ref: "A1", raw: "부서" },
    { ref: "A2", raw: "가", style: { fill: "00ff00", fc: "0000ff" } },
    { ref: "A3", raw: "나", style: { fill: "ff0000" } },
    { ref: "A4", raw: "다" },
    { ref: "A5", raw: "라", style: { fill: "00ff00" } },
    { ref: "A6", raw: "마", style: { fc: "0000ff" } },
  ];
  const h = await createHarness(undefined, { sheets: [{ name: "표", cells }] });
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const rules = h.host.__EASYOFFICE_SHEET_RULES__;
  // A rule gives A3 another fill: the rule's colour is the one the filter sees.
  rules.applyPacked("표", packedFill([[2, 0]], 0xffc7ce), true);
  layout.replaceAll({ 표: { filter: { range: "A1:A6", columns: [{ col: 1, color: { rgb: "00ff00" } }] } } });
  await click(h, act(d, "filter-reapply"));
  const sent = h.requests.at(-1).data[0];
  assert.deepEqual(sent.fill, [[0, 0, "00ff00"], [1, 1, "ffc7ce"], [3, 3, "00ff00"]], "A2, A3 (the rule's), A5");
  assert.deepEqual(sent.font, [[0, 0, "0000ff"], [4, 4, "0000ff"]], "A2 and A6 have a text colour");
  // Only a font condition: both kinds are still told.
  layout.replaceAll({ 표: { filter: { range: "A1:A6", columns: [{ col: 1, color: { font: true } }] } } });
  await click(h, act(d, "filter-reapply"));
  assert.ok(h.requests.at(-1).data[0].fill.length > 0 && h.requests.at(-1).data[0].font.length > 0);
});

test("필터 지우기 clears the filter of the table around the active cell and shows its rows, one undo brings it back", async () => {
  const h = await createHarness(undefined, GROWN);
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({
    표: {
      hiddenRows: [[2, 2]],
      filter: { range: "D1:E7", columns: [{ col: 4, values: ["x"] }], hidden: [[3, 3]] },
      tables: [{ name: "T", range: "A1:B7", header: true, columns: ["부서", "매출"], filter: [{ col: 1, values: ["영업"] }], filterHidden: [[3, 3], [5, 5]] }],
    },
  });
  // The active cell is outside the table: the sheet's filter is what clears.
  cell(d, "D2").focus();
  await click(h, act(d, "filter-clear"));
  assert.deepEqual(layout.filterOf("표").columns, []);
  assert.deepEqual(layout.tablesOf("표")[0].filterHidden, [[3, 3], [5, 5]], "the table keeps its rows");
  // The active cell is in the table: its conditions and rows go, the user's row stays hidden.
  cell(d, "A4").focus();
  await click(h, act(d, "filter-clear"));
  const table = layout.tablesOf("표")[0];
  assert.deepEqual(table.filter, []);
  assert.equal(table.filterHidden, undefined);
  assert.deepEqual([2, 3, 5].map((r) => layout.rowHidden("표", r)), [true, false, false]);
  assert.equal(feedback(d), "필터 지움");
  await click(h, undo(d));
  assert.deepEqual(layout.tablesOf("표")[0].filter, [{ col: 1, values: ["영업"] }]);
  assert.deepEqual(layout.tablesOf("표")[0].filterHidden, [[3, 3], [5, 5]]);
  // Nothing left to clear in the table, and no sheet filter conditions either.
  await click(h, undo(d));
  cell(d, "A4").focus();
  await click(h, act(d, "filter-clear"));
  await click(h, act(d, "filter-clear"));
  assert.equal(feedback(d), "필터 지움", "the second one clears the sheet filter's conditions");
});

// ---- PD-226: the date tree and the colour submenu of the panel -------------------------------------------------

const TREE = { years: [[2023, 3, [[3, 2, [[15, 1], [16, 1]]], [4, 1, [[1, 1]]]]], [2024, 1, [[1, 1, [[5, 1]]]]]], more: false };

function treeEngine(extra = {}) {
  return (body) => {
    if (body.op === "values") return { items: [["x", 1]], blank: 0, more: false, dates: TREE, ...extra };
    if (body.op === "apply") return { hidden: [], count: 0 };
    return undefined;
  };
}

const arrowOf = (row) => row.children[0];

test("a column's dates are a year, month, day tree that folds, and a checked node is a date group", async () => {
  const h = await createHarness(treeEngine());
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "A");
  const rows = () => shownRows(d);
  assert.deepEqual(labels(d), ["(모두 선택)", "2023", "2024", "x"], "folded: the years, then the other texts");
  assert.deepEqual(rows().map((r) => r.children[3].textContent), ["", "3", "1", "1"]);
  assert.equal(d.getElementById("sheet-filter-count").textContent, "5 / 5", "the days and the text");
  assert.equal(rows()[1].getAttribute("aria-expanded"), "false");
  assert.equal(arrowOf(rows()[3]).hidden, true, "a text has no chevron");
  assert.equal(arrowOf(rows()[1]).hidden, false);
  assert.equal(arrowOf(rows()[1]).dataset.icon, "chevron-right");
  // Unfold 2023, then March.
  await click(h, arrowOf(rows()[1]));
  assert.deepEqual(labels(d), ["(모두 선택)", "2023", "3월", "4월", "2024", "x"]);
  assert.equal(arrowOf(rows()[1]).dataset.icon, "chevron-down");
  assert.equal(rows()[1].getAttribute("aria-expanded"), "true");
  await click(h, arrowOf(rows()[2]));
  assert.deepEqual(labels(d), ["(모두 선택)", "2023", "3월", "15", "16", "4월", "2024", "x"]);
  assert.equal(rows()[3].style.paddingLeft, "40px", "a day is two levels in");
  // Uncheck the 16th: its month and year turn mixed. Uncheck the 2024 node as a whole.
  await click(h, rows()[4]);
  assert.equal(rows()[2]._box.indeterminate, true);
  assert.equal(rows()[1]._box.indeterminate, true);
  assert.equal(d.getElementById("sheet-filter-count").textContent, "4 / 5");
  await click(h, rows()[6]);
  assert.equal(rows()[6]._box.checked, false);
  await click(h, rows()[6]);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  const apply = h.requests.filter((r) => r.op === "apply").at(-1);
  // A whole year is a year group, a whole month a month group, the rest single days.
  assert.deepEqual(apply.filter.columns, [
    { col: 1, values: ["x"], dates: [{ unit: "day", year: 2023, month: 3, day: 15 }, { unit: "month", year: 2023, month: 4 }, { unit: "year", year: 2024 }] },
  ]);
  // The panel shows the saved groups again.
  await openPanel(h, "A");
  await click(h, arrowOf(shownRows(d)[1]));
  await click(h, arrowOf(shownRows(d)[2]));
  assert.deepEqual(shownRows(d).map((r) => [r.children[2].textContent, r._box.checked, r._box.indeterminate]), [
    ["(모두 선택)", false, true],
    ["2023", false, true],
    ["3월", false, true],
    ["15", true, false],
    ["16", false, false],
    ["4월", true, false],
    ["2024", true, false],
    ["x", true, false],
  ]);
});

test("choosing every value, dates included, clears the condition, and a search lists the texts flat", async () => {
  const h = await createHarness(treeEngine());
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "A");
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.deepEqual(h.host.__EASYOFFICE_SHEET_LAYOUT__.filterOf("표").columns, [], "nothing unchecked");
  // Uncheck "(모두 선택)" and check the 2024 year only.
  await openPanel(h, "A");
  await click(h, shownRows(d)[0]);
  assert.equal(d.getElementById("btn-sheet-filter-ok").disabled, true, "nothing checked");
  await click(h, shownRows(d)[2]);
  await click(h, d.getElementById("btn-sheet-filter-ok"));
  assert.deepEqual(h.host.__EASYOFFICE_SHEET_LAYOUT__.filterOf("표").columns, [{ col: 1, values: [], dates: [{ unit: "year", year: 2024 }] }]);
});

test("a condition from a file that has date groups and no texts shows its groups checked", async () => {
  const h = await createHarness(treeEngine());
  const d = h.document;
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  layout.setFilter("표", { ...layout.filterOf("표"), columns: [{ col: 1, values: [], dates: [{ unit: "month", year: 2023, month: 3 }] }] });
  cell(d, "A1").focus();
  key(cell(d, "A1"), "ArrowDown", { altKey: true });
  await flushAsyncTurns(10);
  await click(h, arrowOf(shownRows(d)[1]));
  await click(h, arrowOf(shownRows(d)[2]));
  assert.deepEqual(shownRows(d).map((r) => [r.children[2].textContent, r._box.checked]), [
    ["(모두 선택)", false], ["2023", false], ["3월", true], ["15", true], ["16", true], ["4월", false], ["2024", false], ["x", false],
  ]);
  // The keyboard folds with the arrow keys.
  const list = d.getElementById("sheet-filter-list");
  list.focus();
  const at = (k) => key(list, k);
  // The cursor is on March, where the last fold left it: down and up again, then fold it.
  at("ArrowDown");
  at("ArrowUp");
  at("ArrowLeft");
  assert.deepEqual(labels(d).slice(0, 4), ["(모두 선택)", "2023", "3월", "4월"], "March folded");
  at("ArrowRight");
  assert.equal(labels(d)[3], "15");
});

test("the 색 기준 필터 submenu lists the colours of the column and sets a colour condition", async () => {
  const colors = { fill: [["ffc7ce", 2], ["00ff00", 1]], font: [["9c0006", 1]], noFill: 3, noFont: 5 };
  const h = await createHarness((body) => (body.op === "values" ? { items: [["x", 1]], blank: 0, more: false, colors } : body.op === "apply" ? { hidden: [], count: 0 } : undefined));
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "A");
  const sub = d.getElementById("sheet-filter-color");
  assert.equal(sub.getAttribute("aria-disabled"), null, "the column has two kinds of fill");
  const menu = d.getElementById("sheet-filter-color-menu");
  const text = () => [...menu.children].map((el) => el.querySelector?.(".md-label")?.textContent ?? el.className);
  // Fills: two colours and "채우기 없음". Text colours: one colour and "자동" is two kinds as well.
  assert.deepEqual(text(), ["셀 색 기준 필터", "sb-hl-palette-row", "채우기 없음", "글꼴 색 기준 필터", "sb-hl-palette-row", "자동"]);
  const swatches = [...menu.querySelectorAll(".sb-hl-swatch")];
  assert.deepEqual(swatches.map((s) => s.dataset.rgb), ["ffc7ce", "00ff00", "9c0006"]);
  assert.equal(swatches[0].title, "#ffc7ce (2)");
  await click(h, swatches[0]);
  assert.equal(d.getElementById("sheet-filter-panel").hidden, true);
  const apply = () => h.requests.filter((r) => r.op === "apply").at(-1).filter.columns;
  assert.deepEqual(apply(), [{ col: 1, color: { rgb: "ffc7ce" } }]);
  // The colour is marked in the list the next time; "채우기 없음" and a text colour are conditions too.
  await openPanel(h, "A");
  assert.ok([...d.querySelectorAll("#sheet-filter-color-menu .sb-hl-swatch")][0].classList.contains("selected"));
  await click(h, [...d.querySelectorAll("#sheet-filter-color-menu .md-item")].find((el) => el.querySelector(".md-label").textContent === "채우기 없음"));
  assert.deepEqual(apply(), [{ col: 1, color: {} }]);
  await openPanel(h, "A");
  await click(h, [...d.querySelectorAll("#sheet-filter-color-menu .sb-hl-swatch")][2]);
  assert.deepEqual(apply(), [{ col: 1, color: { font: true, rgb: "9c0006" } }]);
});

test("the 색 기준 필터 submenu is off when the column has fewer than two kinds of colour", async () => {
  const h = await createHarness((body) => (body.op === "values" ? { items: [["x", 1]], blank: 0, more: false, colors: { fill: [["ffc7ce", 4]], font: [], noFill: 0, noFont: 4 } } : undefined));
  const d = h.document;
  cell(d, "A2").focus();
  await click(h, act(d, "filter-toggle"));
  await openPanel(h, "A");
  assert.equal(d.getElementById("sheet-filter-color").getAttribute("aria-disabled"), "true");
  assert.equal(d.getElementById("sheet-filter-color-menu").children.length, 0);
});
