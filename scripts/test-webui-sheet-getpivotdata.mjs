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

// `omitted`: the locations the engine names as left out for their size (PD-303); then it answers `#REF!` for the formulas.
async function open({ sessions, omitted = null }) {
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
        if (body.sessionAction === "pivots") {
          const answer = omitted && body.pivots.length ? "#REF!" : String(body.pivots.length ? 43 : "#REF!");
          return { session: "s1", changed: [{ sheet: body.sheet, cell: "H9", value: answer }], spills: [], circular: false, ...(omitted && body.pivots.length ? { pivotsOmitted: omitted } : {}) };
        }
        if (!body.sessionAction && Array.isArray(body.targets)) {
          const hasPivots = Boolean(body.pivots?.Sheet1?.length);
          const answer = hasPivots && omitted?.length ? "#REF!" : String(hasPivots ? 43 : "#REF!");
          return {
            targetValues: body.targets.map((target) => (target.cell === "H9" ? answer : "")),
            circular: false,
            ...(hasPivots && omitted?.length ? { pivotsOmitted: { Sheet1: [...omitted] } } : {}),
          };
        }
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

// PD-303: a pivot table the engine left out for its size is named in the answer; a `GETPIVOTDATA` into it says why it is `#REF!`.
const WHY = "피벗 정의가 너무 커서 계산에서 제외됨";

test("the reason shows for a #REF! GETPIVOTDATA whose pivot table reference lies in an omitted table, and for no other cell", async () => {
  const h = await createWorkspaceHarness();
  const spill = h.host.__EASYOFFICE_SHEET_SPILL__;
  const ref = (formula, value = "#REF!") => ({ formula, value });
  spill.setOmittedPivots("Sheet1", ["F3:I6"]);
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",$F$3)')), WHY);
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",H5,"지역","East")')), WHY, "any cell of the table");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=IFERROR(GETPIVOTDATA("매출",Sheet1!$I$6),0)+1')), WHY, "named by its sheet");
  assert.equal(spill.pivotOmittedNote("Sheet2", ref('=GETPIVOTDATA("매출",Sheet1!F3)')), WHY, "from another sheet");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref("=getpivotdata(\"매출\",$F$3)")), WHY, "any case");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",$F$3)', "43")), "", "a value is not an error");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",$A$20)')), "", "a reference outside the table");
  assert.equal(spill.pivotOmittedNote("Sheet2", ref('=GETPIVOTDATA("매출",$F$3)')), "", "the same cell of another sheet");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref("=F3+1")), "", "no GETPIVOTDATA");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("F3",$A$20)')), "", "a text is not a reference");
  assert.equal(spill.pivotOmittedNote("Sheet1", null), "");
  spill.setOmittedPivots("Sheet1", []);
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",$F$3)')), "", "the list of a sheet is replaced by the next answer");
  spill.setOmittedPivots("Sheet1", ["F3:I6", 7, null]);
  spill.clearOmittedPivots();
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",$F$3)')), "");
});

test("the status bar names the reason of a #REF! that the omitted pivot table left, and takes it back when the table is readable", async () => {
  const t = await open({ sessions: true, omitted: ["F3:I6"] });
  const layout = t.host.__EASYOFFICE_SHEET_LAYOUT__;
  const status = () => t.document.getElementById("status-sheet-agg").textContent;
  await type(t, "H9", '=GETPIVOTDATA("매출",$F$3)');
  layout.setPivots("Sheet1", [PIVOT]);
  await flushAsyncTurns(30);
  const cell = t.document.querySelector('#pane-sheet td[data-cell="H9"]');
  assert.equal(cell.textContent, "#REF!");
  cell.focus();
  await flushAsyncTurns(5);
  assert.equal(status(), WHY);
  t.document.querySelector('#pane-sheet td[data-cell="H10"]').focus();
  await flushAsyncTurns(5);
  assert.notEqual(status(), WHY, "another cell has nothing to say about it");
  // The table is edited and the engine reads it now: the reason is gone.
  t.host.__EASYOFFICE_SHEET_SPILL__.setOmittedPivots("Sheet1", []);
  cell.focus();
  await flushAsyncTurns(5);
  assert.notEqual(status(), WHY);
});

test("pivotOmittedNote follows defined names (workbook, sheet-scoped, chained) and skips cycles or text literals", async () => {
  const h = await createWorkspaceHarness();
  const spill = h.host.__EASYOFFICE_SHEET_SPILL__;
  const names = h.host.__EASYOFFICE_SHEET_NAMES__;
  names.replaceAll(
    [
      { name: "피벗시작", refersTo: "Sheet1!$F$3" },
      { name: "피벗영역", refersTo: "Sheet1!$F$3:$I$6" },
      { name: "별칭", refersTo: "=피벗영역" },
      { name: "시트1전용", scope: "Sheet1", refersTo: "$F$3" },
      { name: "시트2다른참조", scope: "Sheet2", refersTo: "Sheet1!$F$3" },
      { name: "밖", refersTo: "Sheet1!$A$20" },
      { name: "순환A", refersTo: "=순환B" },
      { name: "순환B", refersTo: "=순환A" },
    ],
    true
  );
  spill.replaceOmittedPivots({ sheet1: ["F3:I6"] });
  const ref = (formula, value = "#REF!") => ({ formula, value });

  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",피벗시작)')), WHY);
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",피벗영역)')), WHY);
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",별칭)')), WHY);
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",시트1전용)')), WHY);
  assert.equal(spill.pivotOmittedNote("Sheet2", ref('=GETPIVOTDATA("매출",피벗영역)')), WHY);
  assert.equal(spill.pivotOmittedNote("Sheet2", ref('=GETPIVOTDATA("매출",시트2다른참조)')), WHY);

  assert.equal(spill.pivotOmittedNote("Sheet2", ref('=GETPIVOTDATA("매출",시트1전용)')), "");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",밖)')), "");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",순환A)')), "");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("피벗영역",$A$20)')), "");
  assert.equal(spill.pivotOmittedNote("Sheet1", ref('=GETPIVOTDATA("매출",피벗영역)', "43")), "");
});

test("stateless evaluation carries omitted pivots for defined-name GETPIVOTDATA and clears when resolved", async () => {
  const omittedList = ["F3:I6"];
  const t = await open({ sessions: false, omitted: omittedList });
  const layout = t.host.__EASYOFFICE_SHEET_LAYOUT__;
  const names = t.host.__EASYOFFICE_SHEET_NAMES__;
  names.replaceAll([{ name: "피벗영역", refersTo: "Sheet1!$F$3:$I$6" }], true);
  layout.setPivots("Sheet1", [PIVOT]);
  const status = () => t.document.getElementById("status-sheet-agg").textContent;

  await type(t, "H9", '=GETPIVOTDATA("매출",피벗영역)');
  await flushAsyncTurns(30);
  const cell = t.document.querySelector('#pane-sheet td[data-cell="H9"]');
  assert.equal(cell.textContent, "#REF!");
  cell.focus();
  await flushAsyncTurns(5);
  assert.equal(status(), WHY);

  // Pivot returns within budget: next stateless evaluation clears omitted state and status reverts.
  omittedList.length = 0;
  layout.setPivots("Sheet1", [{ ...PIVOT, filters: [] }]);
  cell.focus();
  t.document.getElementById("btn-eval-formula").click();
  await flushAsyncTurns(30);
  assert.equal(cell.textContent, "43");
  cell.focus();
  await flushAsyncTurns(5);
  assert.notEqual(status(), WHY);
});
