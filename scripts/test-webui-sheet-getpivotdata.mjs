import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The pivot tables `GETPIVOTDATA` reads (PD-292): what the page tells the engine of them (the payload of a sheet, the
// field of a request without a session), when the session is told (a create, an edit, a delete and an undo) and what
// is not sent (a change that leaves the pivot tables as they were).

const PIVOT = {
  name: "피벗 테이블1",
  cacheId: 1,
  sourceSheet: "Sheet1",
  sourceRange: "A1:C7",
  sourceTable: null,
  hasHeader: true,
  location: "F3:I6",
  fieldNames: ["지역", "분기", "매출"],
  rowFields: [0],
  colFields: [1],
  filterFields: [],
  valueFields: [{ col: 2, agg: "sum", name: "합계: 매출" }],
  filters: [{ col: 0, values: ["East"], blank: false }],
  slicers: [{ col: 0, caption: "지역", left: 300, top: 40, width: 184, height: 210, multiSelect: false }],
};

test("the engine is told what it reads of a pivot table and nothing the page keeps for itself", async () => {
  const h = await createWorkspaceHarness();
  const { __EASYOFFICE_SHEET_SPILL__: spill, __EASYOFFICE_SHEET_LAYOUT__: layout } = h.host;
  layout.setPivots("S", [PIVOT]);
  const [sent] = spill.pivotPayload("S", layout);
  const byName = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
  assert.deepEqual(Object.keys(sent).sort(byName), ["colFields", "filters", "hasHeader", "location", "rowFields", "sourceRange", "sourceSheet", "valueFields"]);
  assert.equal(sent.location, "F3:I6");
  assert.deepEqual(sent.valueFields, [{ col: 2, agg: "sum", name: "합계: 매출" }]);
  assert.deepEqual(spill.pivotPayload("None", layout), []);
  assert.deepEqual(spill.blockerPayload("pivots", "S", layout), spill.pivotPayload("S", layout));
});

test("a long name and long filter items: the name is not sent, and a table with no value field is left out", async () => {
  const h = await createWorkspaceHarness();
  const { __EASYOFFICE_SHEET_SPILL__: spill, __EASYOFFICE_SHEET_LAYOUT__: layout } = h.host;
  const long = "가".repeat(255);
  layout.setPivots("S", [
    { ...PIVOT, name: long, filters: [{ col: 0, values: [long.repeat(3)], blank: false }] },
    { ...PIVOT, name: "값 없음", location: "A20:B21", valueFields: [] },
  ]);
  const sent = spill.pivotPayload("S", layout);
  assert.equal(sent.length, 1, "only the table that can answer");
  assert.equal(sent[0].name, undefined, "the name is not sent");
  assert.equal(sent[0].filters[0].values[0], long.repeat(3), "the filter items are the engine's to bound, they are sent as they are");
});

test("a pivot table whose content the engine bounds does not stop the evaluation of the other formulas", async () => {
  const requests = [];
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: false, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        requests.push(body);
        return { targetValues: body.targets.map((t) => (t.cell === "H9" ? "3" : "")), circular: false };
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const long = "가".repeat(255);
  harness.host.__EASYOFFICE_SHEET_LAYOUT__.setPivots("Sheet1", [{ ...PIVOT, name: long, filters: [{ col: 0, values: [long.repeat(3)], blank: false }] }]);
  await type({ ...harness, requests }, "H9", "=1+2");
  const sent = requests.filter((r) => !r.sessionAction && Array.isArray(r.targets)).at(-1);
  assert.ok(sent, "a request was sent");
  assert.equal(sent.pivots.Sheet1[0].name, undefined, "the long name does not travel");
  assert.equal(harness.document.querySelector('#pane-sheet td[data-cell="H9"]').textContent, "3", "the other formula has its value");
});

test("a request without a session carries the pivot tables of the sheets that have some", async () => {
  const h = await createWorkspaceHarness();
  const { __EASYOFFICE_SHEET_SPILL__: spill, __EASYOFFICE_SHEET_LAYOUT__: layout } = h.host;
  layout.setPivots("S", [PIVOT]);
  const field = spill.mergesField(["S", "T"], layout);
  assert.deepEqual(Object.keys(field), ["pivots"], "no merge and no table, nothing of them");
  assert.deepEqual(Object.keys(field.pivots), ["S"]);
  assert.equal(field.pivots.S[0].location, "F3:I6");
  assert.deepEqual(spill.mergesField(["T"], layout), {});
});

// Types into a cell, which opens the session (nothing is sent before a formula is calculated).
async function type(t, ref, text) {
  const td = t.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  td.focus();
  for (const ch of text) {
    const active = t.document.activeElement;
    active.dispatchEvent({ type: "keydown", key: ch, target: active });
    active.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: active });
    active.textContent += ch;
    active.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: active });
  }
  td.dispatchEvent({ type: "keydown", key: "Tab", target: td });
  await flushAsyncTurns(15);
}

async function open({ sessions }) {
  const requests = [];
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: sessions, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        requests.push(body);
        if (body.sessionAction === "open") return { session: "s1" };
        if (body.sessionAction === "apply") return { session: "s1", changed: [], spills: [], circular: false };
        if (body.sessionAction === "pivots") return { session: "s1", changed: [{ sheet: body.sheet, cell: "H9", value: String(body.pivots.length ? 43 : "#REF!") }], spills: [], circular: false };
        return {};
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return { ...harness, requests };
}

const told = (t) => t.requests.filter((r) => r.sessionAction === "pivots");

test("a create, an edit, a delete and an undo of a pivot table tell the session once each, and nothing else does", async () => {
  const t = await open({ sessions: true });
  const layout = t.host.__EASYOFFICE_SHEET_LAYOUT__;
  await type(t, "H9", '=GETPIVOTDATA("매출",$F$3)');
  assert.ok(t.requests.some((r) => r.sessionAction === "open"), "a session is open");
  assert.equal(told(t).length, 0, "no pivot table, nothing to say");
  const sheet = "Sheet1";

  // A create: the page puts the pivot table in the layout.
  layout.setPivots(sheet, [PIVOT]);
  await flushAsyncTurns(30);
  assert.equal(told(t).length, 1, JSON.stringify(told(t)));
  assert.equal(told(t)[0].sheet, sheet);
  assert.equal(told(t)[0].pivots[0].location, "F3:I6");
  assert.equal(told(t)[0].pivots[0].slicers, undefined);
  assert.equal(t.document.querySelector('#pane-sheet td[data-cell="H9"]').textContent, "43", "the answer of the engine is shown");

  // A change of something else, and of what only the page keeps of the pivot table: nothing is sent.
  layout.setTabColor(sheet, "336699");
  layout.setPivots(sheet, [{ ...PIVOT, slicers: [], fieldNames: ["a", "b", "c"], name: PIVOT.name }]);
  await flushAsyncTurns(20);
  assert.equal(told(t).length, 1);

  // An edit: another source, another filter.
  layout.setPivots(sheet, [{ ...PIVOT, sourceRange: "A1:C9", filters: [] }]);
  await flushAsyncTurns(30);
  assert.equal(told(t).length, 2);
  assert.equal(told(t)[1].pivots[0].sourceRange, "A1:C9");

  // A delete through the page's own command, and its undo.
  const pivot = layout.pivotsOf(sheet)[0];
  await t.host.__EASYOFFICE_SHEET_PIVOT__.deletePivot(pivot);
  await flushAsyncTurns(30);
  assert.equal(told(t).length, 3);
  assert.deepEqual(told(t)[2].pivots, []);
  assert.equal(t.document.querySelector('#pane-sheet td[data-cell="H9"]').textContent, "#REF!");
  t.userClick(studioItem(t.document, "studio-undo"));
  await flushAsyncTurns(40);
  assert.equal(told(t).length, 4, "the undo brings the pivot table back and the session is told");
  assert.equal(told(t)[3].pivots.length, 1);
});

test("a request without a session carries the pivot tables to the engine", async () => {
  const t = await open({ sessions: false });
  const layout = t.host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.setPivots("Sheet1", [PIVOT]);
  await type(t, "H9", '=GETPIVOTDATA("매출",$F$3)');
  const request = t.requests.filter((r) => !r.sessionAction && Array.isArray(r.targets)).at(-1);
  assert.ok(request, "a stateless request was sent");
  assert.equal(request.pivots.Sheet1[0].location, "F3:I6");
  assert.equal(request.pivots.Sheet1[0].sourceRange, "A1:C7");
});
