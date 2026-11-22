import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// PD-302: a row or column edit carries the charts of the sheet, the answer moves their anchors, and one undo gives them back.

const EMU = { colOffEmu: 0, rowOffEmu: 0, toColOffEmu: 0, toRowOffEmu: 0 };
// 192 x 72 px over C3:E5 on the 96 x 24 px grid.
const chart = (id, editAs) => ({
  id,
  chartType: "column",
  title: id,
  dataRange: "A1:B3",
  switchRowCol: false,
  legend: "right",
  anchor: { fromCol: 2, fromRow: 2, toCol: 4, toRow: 5, ...EMU, left: 192, top: 48, width: 192, height: 72, editAs },
});

function selectCell(document, coord) {
  const td = document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
  assert.ok(td, `missing cell ${coord}`);
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
  td.focus();
}

async function open() {
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
    search: "?module=SHEET&win=w-sheet-structure-charts",
    testIpc: async (command, payload) => {
      if (command === "get_workspace_bootstrap") return { ...offline.get_workspace_bootstrap(payload), sheetSessions: true };
      if (command === "evaluate_sheet") {
        const body = JSON.parse(payload);
        if (body.sessionAction === "structure") calls.push(body);
      }
      return offline[command](payload);
    },
  });
  await flushAsyncTurns(10);
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns(4);
  const layoutApi = harness.host.__EASYOFFICE_SHEET_LAYOUT__;
  layoutApi.setCharts("Sheet1", [chart("two", "twoCell"), chart("one", "oneCell"), chart("abs", "absolute")]);
  await flushAsyncTurns(4);
  const rows = () => Object.fromEntries(layoutApi.chartsOf("Sheet1").map((c) => [c.id, [c.anchor.fromRow, c.anchor.toRow]]));
  const cols = () => Object.fromEntries(layoutApi.chartsOf("Sheet1").map((c) => [c.id, [c.anchor.fromCol, c.anchor.toCol]]));
  return { ...harness, layoutApi, calls, rows, cols };
}

test("PD-302: 행 삽입 요청에 차트가 실리고 종류에 따라 앵커가 옮겨지며 실행 취소 한 번으로 돌아온다", async () => {
  const { document, sandbox, userClick, layoutApi, calls, rows } = await open();
  selectCell(document, "A2");
  userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0].charts.map((c) => [c.id, c.anchor.editAs]), [["two", "twoCell"], ["one", "oneCell"], ["abs", "absolute"]]);
  assert.deepEqual(rows(), { two: [3, 6], one: [3, 6], abs: [2, 5] }, "두 칸·한 칸은 옮겨지고 절대는 그대로");
  assert.equal(layoutApi.chartsOf("Sheet1")[0].anchor.top, 72);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.deepEqual(rows(), { two: [2, 5], one: [2, 5], abs: [2, 5] }, "실행 취소 한 번으로 모두 돌아옴");
  assert.equal(layoutApi.chartsOf("Sheet1")[0].anchor.top, 48);
});

test("PD-302: 열 삭제는 차트의 시작·끝 칸을 당기고 지운 칸 위의 차트는 자리를 지킨다", async () => {
  const { document, sandbox, userClick, calls, cols } = await open();
  selectCell(document, "A1");
  userClick(document.querySelector('.md-item[data-sheet-act="col-delete"]'));
  await flushAsyncTurns(20);
  assert.equal(calls.length, 1);
  assert.deepEqual(cols(), { two: [1, 3], one: [1, 3], abs: [2, 4] });

  // 지금 B열(0부터 1)을 지워도 차트는 남는다: 시작·끝 칸이 한 칸으로 모인다.
  selectCell(document, "B1");
  userClick(document.querySelector('.md-item[data-sheet-act="col-delete"]'));
  await flushAsyncTurns(20);
  assert.deepEqual(cols().two, [1, 2]);
  assert.equal(calls[1].charts.length, 3);
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.deepEqual(cols(), { two: [1, 3], one: [1, 3], abs: [2, 4] });
});
