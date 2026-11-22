import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// PD-303: a row or column edit of one sheet sends the references of the charts of the other sheets, the answer moves the ones
// that point at the edited sheet and leaves the rest of those charts as they were, and one undo gives them back.

const anchor = { fromCol: 2, fromRow: 2, toCol: 4, toRow: 5, colOffEmu: 0, rowOffEmu: 0, toColOffEmu: 0, toRowOffEmu: 0, left: 192, top: 48, width: 192, height: 72, editAs: "twoCell" };
const sheet2Chart = {
  id: "far",
  chartType: "column",
  title: "다른 시트의 차트",
  dataRange: "Sheet1!$A$1:$B$5",
  switchRowCol: false,
  legend: "bottom",
  anchor,
  series: [{ name: "매출", nameRef: "Sheet1!$B$1", categoriesRef: "Sheet1!$A$2:$A$5", valuesRef: "Sheet1!$B$2:$B$5" }],
};
const ownChart = { ...sheet2Chart, id: "own", title: "제 시트 차트", dataRange: "A1:B3", series: [{ name: "x", valuesRef: "B2:B3" }] };

function selectCell(document, coord) {
  const td = document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
  assert.ok(td, `missing cell ${coord}`);
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
  td.focus();
}

// `answer(res, far)` may change the engine's answer to a structure request (a test double of a mismatch).
async function open({ answer = null } = {}) {
  let farName = null;
  const { sandbox: boot } = await createWebUiDom();
  const cat = boot.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = boot.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const calls = [];
  const harness = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-sheet-structure-chart-refs",
    testIpc: async (command, payload) => {
      if (command === "get_workspace_bootstrap") return { ...offline.get_workspace_bootstrap(payload), sheetSessions: true };
      if (command === "evaluate_sheet") {
        const body = JSON.parse(payload);
        if (body.sessionAction === "structure") calls.push(body);
      }
      const res = await offline[command](payload);
      if (answer && command === "evaluate_sheet" && JSON.parse(payload).sessionAction === "structure") return answer(res, farName);
      return res;
    },
  });
  await flushAsyncTurns(10);
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns(4);
  harness.userClick(harness.document.getElementById("btn-add-sheet-tab"));
  await flushAsyncTurns(10);
  const names = [...harness.document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn")].map((b) => b.dataset.sheetTab);
  assert.ok(names.includes("Sheet1") && names.length >= 2, `two sheets: ${names}`);
  const far = names.find((n) => n !== "Sheet1");
  farName = far;
  harness.userClick(harness.document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns(10);
  const layoutApi = harness.host.__EASYOFFICE_SHEET_LAYOUT__;
  layoutApi.setCharts(far, [sheet2Chart]);
  layoutApi.setCharts("Sheet1", [ownChart]);
  await flushAsyncTurns(4);
  return { ...harness, layoutApi, calls, far };
}

const refsOf = (layoutApi, sheet) => {
  const [c] = layoutApi.chartsOf(sheet);
  return [c.dataRange, c.series[0].nameRef, c.series[0].categoriesRef, c.series[0].valuesRef];
};

test("PD-303: 행 삽입 요청에 다른 시트 차트의 참조만 실리고, 답이 그 참조를 옮기며, 실행 취소 한 번으로 돌아온다", async () => {
  const { document, sandbox, userClick, layoutApi, calls, far } = await open();
  selectCell(document, "A2");
  userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  assert.equal(calls.length, 1);
  assert.deepEqual(Object.keys(calls[0].chartRefs), [far], "편집한 시트가 아니라 다른 시트의 차트");
  assert.deepEqual(calls[0].chartRefs[far], [
    { id: "far", dataRange: "Sheet1!$A$1:$B$5", switchRowCol: false, series: [{ nameRef: "Sheet1!$B$1", categoriesRef: "Sheet1!$A$2:$A$5", valuesRef: "Sheet1!$B$2:$B$5" }] },
  ]);
  assert.deepEqual(refsOf(layoutApi, far), ["Sheet1!$A$1:$B$6", "Sheet1!$B$1", "Sheet1!$A$3:$A$6", "Sheet1!$B$3:$B$6"], "A2 위에 한 행: 2행부터 아래로");
  const moved = layoutApi.chartsOf(far)[0];
  assert.equal(moved.title, "다른 시트의 차트");
  assert.equal(moved.legend, "bottom");
  assert.deepEqual([moved.anchor.fromRow, moved.anchor.toRow, moved.anchor.top], [2, 5, 48], "그 시트의 칸은 그대로");
  assert.equal(moved.series[0].name, "매출");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.deepEqual(refsOf(layoutApi, far), ["Sheet1!$A$1:$B$5", "Sheet1!$B$1", "Sheet1!$A$2:$A$5", "Sheet1!$B$2:$B$5"], "실행 취소 한 번으로 돌아옴");
});

test("PD-303: id가 같은 차트 둘도 각자 자기 참조를 받고, 실행 취소가 다른 시트의 차트를 되돌린다", async () => {
  const { document, sandbox, userClick, layoutApi, calls, far } = await open();
  const one = { ...sheet2Chart, id: "same", dataRange: "Sheet1!$A$1:$B$5", series: [{ name: "a", valuesRef: "Sheet1!$B$2:$B$5" }] };
  const two = { ...sheet2Chart, id: "same", dataRange: "Sheet1!$A$1:$C$5", series: [{ name: "b", valuesRef: "Sheet1!$C$2:$C$5" }] };
  layoutApi.setCharts(far, [one, two]);
  await flushAsyncTurns(4);
  const values = () => layoutApi.chartsOf(far).map((c) => [c.dataRange, c.series[0].valuesRef]);
  selectCell(document, "A2");
  userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  assert.equal(calls.length, 1);
  assert.deepEqual(values(), [["Sheet1!$A$1:$B$6", "Sheet1!$B$3:$B$6"], ["Sheet1!$A$1:$C$6", "Sheet1!$C$3:$C$6"]], "각 차트가 제 참조를 옮겨 받음");
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.deepEqual(values(), [["Sheet1!$A$1:$B$5", "Sheet1!$B$2:$B$5"], ["Sheet1!$A$1:$C$5", "Sheet1!$C$2:$C$5"]], "실행 취소 한 번에 다른 시트의 차트 둘 다 돌아옴");
});

test("PD-303: 답의 차트 수나 id가 페이지와 다르면 그 시트는 건너뛴다", async () => {
  const { document, userClick, layoutApi, far } = await open({ answer: (res, far) => ({ ...res, chartRefs: { [far]: res.chartRefs[far].slice(1) } }) });
  const a = { ...sheet2Chart, id: "a" };
  const b = { ...sheet2Chart, id: "b" };
  layoutApi.setCharts(far, [a, b]);
  await flushAsyncTurns(4);
  selectCell(document, "A2");
  userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  assert.deepEqual(layoutApi.chartsOf(far).map((c) => c.dataRange), ["Sheet1!$A$1:$B$5", "Sheet1!$A$1:$B$5"], "길이가 다르면 아무것도 덮지 않는다");
});

test("PD-303: 행 삽입과 실행 취소 뒤에도 시트의 기본 열 너비 값이 레이아웃에 남는다", async () => {
  const { document, sandbox, userClick, layoutApi } = await open();
  layoutApi.replaceAll({ ...layoutApi.getAll(), Sheet1: { ...layoutApi.getAll().Sheet1, defaultColWidth: { width: 20.7109375, base: 10 } } });
  selectCell(document, "A2");
  userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  assert.deepEqual(layoutApi.getAll().Sheet1.defaultColWidth, { width: 20.7109375, base: 10 }, "편집한 시트");
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.deepEqual(layoutApi.getAll().Sheet1.defaultColWidth, { width: 20.7109375, base: 10 }, "실행 취소 뒤");
});

test("PD-303: 차트가 없는 시트는 요청에 실리지 않고, 다른 시트 차트의 참조가 바뀌지 않으면 그 차트를 건드리지 않는다", async () => {
  const { document, userClick, layoutApi, calls, far } = await open();
  layoutApi.setCharts(far, []);
  await flushAsyncTurns(4);
  selectCell(document, "A2");
  userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0].chartRefs, {}, "다른 시트에 차트가 없으면 비어 있다");
  assert.equal(calls[0].charts.length, 1, "편집한 시트의 차트는 charts로 간다");

  // The chart of the other sheet points at a third sheet: the edit does not change it and the answer does not name the sheet.
  const stays = { ...sheet2Chart, dataRange: "Other!$A$1:$B$5", series: [{ name: "x", valuesRef: "Other!$B$2:$B$5" }] };
  layoutApi.setCharts(far, [stays]);
  await flushAsyncTurns(4);
  selectCell(document, "A2");
  userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  assert.equal(calls.length, 2);
  assert.deepEqual(Object.keys(calls[1].chartRefs), [far]);
  assert.deepEqual([layoutApi.chartsOf(far)[0].dataRange, layoutApi.chartsOf(far)[0].series[0].valuesRef], ["Other!$A$1:$B$5", "Other!$B$2:$B$5"]);
});
