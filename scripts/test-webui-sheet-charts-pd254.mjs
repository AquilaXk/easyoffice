import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

function setCell(document, coord, text) {
  const td = document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
  assert.ok(td, `missing cell ${coord}`);
  td.focus();
  td.textContent = text;
  td.dispatchEvent({ type: "input", target: td });
  td.blur();
}

async function seedQuarterlySheet(document) {
  const entries = [
    ["A1", "분기"], ["B1", "매출"], ["C1", "달성률"],
    ["A2", "1분기"], ["B2", "120"], ["C2", "85.0%"],
    ["A3", "2분기"], ["B3", "240"], ["C3", "92.9%"],
    ["A4", "3분기"], ["B4", "-60"], ["C4", "40.0%"],
    ["A5", "4분기"], ["B5", "=B2+B3"], ["C5", "110.0%"],
    ["E1", "투입"], ["F1", "산출"],
    ["E2", "10"], ["F2", "25"],
    ["E3", "20"], ["F3", "55"],
    ["E4", "35"], ["F4", "90"],
  ];
  for (const [coord, text] of entries) {
    setCell(document, coord, text);
    await flushAsyncTurns(2);
  }
  document.querySelector('#pane-sheet td[data-cell="A1"]').focus();
  await flushAsyncTurns(4);
}

function menuCommand(document, cmd) {
  const item = document.querySelector(`#menu-bar [data-studio-menu-cmd="${cmd}"]`);
  assert.ok(item, `missing menu item ${cmd}`);
  return item;
}

test("PD-254: 차트 삽입 대화상자에서 5종 벡터 SVG 차트(막대·가로막대·꺾은선·원형·분산형)와 제목·범례·행열전환을 렌더링한다", async () => {
  const { document, userClick, host } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-chart-types" });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  await seedQuarterlySheet(document);

  userClick(menuCommand(document, "insert-chart"));
  await flushAsyncTurns(6);

  assert.match(
    document.getElementById("sheet-eval-result").textContent,
    /차트 데이터 요약 · 합계/,
    "기존 차트 데이터 요약 출력 호환성 유지"
  );

  const dialog = document.getElementById("sheet-chart-dialog");
  assert.ok(dialog, "#sheet-chart-dialog exists");
  assert.equal(dialog.open, true, "chart dialog opened");
  assert.equal(document.getElementById("sheet-chart-title").textContent, "차트 삽입");
  assert.equal(document.getElementById("btn-sheet-chart-delete").hidden, true);

  const rangeInput = document.getElementById("sheet-chart-range");
  assert.match(rangeInput.value, /A1:C5/, "인접 비어있지 않은 셀 블록 자동 감지");

  rangeInput.value = "A1:B5";
  const headingInput = document.getElementById("sheet-chart-heading");
  headingInput.value = "분기별 매출";
  const typeSelect = document.getElementById("sheet-chart-type");
  const legendSelect = document.getElementById("sheet-chart-legend");
  typeSelect.value = "column";
  typeSelect.dispatchEvent({ type: "change", target: typeSelect });
  legendSelect.value = "right";
  legendSelect.dispatchEvent({ type: "change", target: legendSelect });

  userClick(document.getElementById("btn-sheet-chart-ok"));
  await flushAsyncTurns(6);
  assert.equal(dialog.open, false);

  const layer = document.getElementById("sheet-chart-layer");
  assert.ok(layer && !layer.hidden, "#sheet-chart-layer visible");
  const card = layer.querySelector(".sheet-chart-card");
  assert.ok(card, "floating chart card rendered");

  const colBars = card.querySelectorAll(".eo-chart-bar");
  assert.equal(colBars.length, 4, "4 quarterly column bars");
  const h0 = Number(colBars[0].getAttribute("height"));
  const h1 = Number(colBars[1].getAttribute("height"));
  const h2 = Number(colBars[2].getAttribute("height"));
  const h3 = Number(colBars[3].getAttribute("height"));
  assert.ok(h3 > h1 && h1 > h0 && h2 > 0, `bar heights scale with values (${h0}, ${h1}, ${h2}, ${h3})`);
  const y1 = Number(colBars[1].getAttribute("y"));
  const y2 = Number(colBars[2].getAttribute("y"));
  assert.ok(y2 > y1, "음수 막대(-60)는 0 기준선 아래에서 시작한다");
  assert.equal(card.querySelector(".eo-chart-title")?.textContent, "분기별 매출");
  assert.ok(card.querySelector(".eo-chart-legend"), "범례 렌더링");

  const editCard = async (patchFn) => {
    card.dispatchEvent({ type: "dblclick", target: card });
    await flushAsyncTurns(4);
    assert.equal(dialog.open, true);
    assert.equal(document.getElementById("sheet-chart-title").textContent, "차트 데이터 편집");
    patchFn();
    userClick(document.getElementById("btn-sheet-chart-ok"));
    await flushAsyncTurns(6);
    return layer.querySelector(".sheet-chart-card");
  };

  const barCard = await editCard(() => {
    typeSelect.value = "bar";
    typeSelect.dispatchEvent({ type: "change", target: typeSelect });
    legendSelect.value = "bottom";
    legendSelect.dispatchEvent({ type: "change", target: legendSelect });
  });
  const horizBars = barCard.querySelectorAll(".eo-chart-bar");
  assert.equal(horizBars.length, 4);
  assert.ok(
    Number(horizBars[3].getAttribute("width")) > Number(horizBars[0].getAttribute("width")),
    "가로 막대 폭이 수치 크기에 비례한다"
  );

  const lineCard = await editCard(() => {
    rangeInput.value = "A1:C5";
    typeSelect.value = "line";
    typeSelect.dispatchEvent({ type: "change", target: typeSelect });
    legendSelect.value = "top";
    legendSelect.dispatchEvent({ type: "change", target: legendSelect });
  });
  assert.equal(lineCard.querySelectorAll(".eo-chart-line").length, 2, "매출·달성률 2개 계열 꺾은선");
  assert.equal(lineCard.querySelectorAll(".eo-chart-marker").length, 8, "계열당 4개 표식");

  const pieCard = await editCard(() => {
    rangeInput.value = "A1:B3";
    typeSelect.value = "pie";
    typeSelect.dispatchEvent({ type: "change", target: typeSelect });
  });
  const pieSlices = pieCard.querySelectorAll(".eo-chart-slice");
  assert.equal(pieSlices.length, 2, "양수 2개 항목 부채꼴 조각");
  assert.equal(pieSlices[0].tagName, "PATH");
  assert.match(pieSlices[0].getAttribute("d") || "", /A\s/);

  const singlePieCard = await editCard(() => {
    rangeInput.value = "A1:B2";
    typeSelect.value = "pie";
    typeSelect.dispatchEvent({ type: "change", target: typeSelect });
  });
  const singleSlices = singlePieCard.querySelectorAll(".eo-chart-slice");
  assert.equal(singleSlices.length, 1);
  assert.equal(singleSlices[0].tagName, "CIRCLE", "단일 조각 100% 원형 차트는 <circle>로 렌더링");

  const scatterCard = await editCard(() => {
    rangeInput.value = "E1:F4";
    typeSelect.value = "scatter";
    typeSelect.dispatchEvent({ type: "change", target: typeSelect });
    legendSelect.value = "none";
    legendSelect.dispatchEvent({ type: "change", target: legendSelect });
  });
  const pts = scatterCard.querySelectorAll(".eo-chart-point");
  assert.equal(pts.length, 3, "분산형 점 3개");
  assert.ok(
    Number(pts[2].getAttribute("cx")) > Number(pts[0].getAttribute("cx")) &&
      Number(pts[2].getAttribute("cy")) < Number(pts[0].getAttribute("cy")),
    "분산형 X/Y 수치 축 좌표 비례 검증"
  );
  assert.equal(scatterCard.querySelector(".eo-chart-legend"), null, "범례 없음(none) 적용");

  const switchedCard = await editCard(() => {
    rangeInput.value = "A1:C3";
    typeSelect.value = "column";
    typeSelect.dispatchEvent({ type: "change", target: typeSelect });
    legendSelect.value = "right";
    legendSelect.dispatchEvent({ type: "change", target: legendSelect });
    document.getElementById("sheet-chart-switch").checked = true;
  });
  assert.equal(switchedCard.querySelectorAll(".eo-chart-bar").length, 4);
  assert.equal(host.__EASYOFFICE_SHEET_LAYOUT__.chartsOf("Sheet1")[0].switchRowCol, true);
});

test("PD-254: 원본 셀·수식 값 변경 시 차트 SVG가 실시간 갱신되고 다른 시트 참조도 반영한다", async () => {
  const { document, userClick, host } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-chart-live" });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  await seedQuarterlySheet(document);

  userClick(menuCommand(document, "insert-chart"));
  await flushAsyncTurns(6);
  document.getElementById("sheet-chart-range").value = "A1:B5";
  document.getElementById("sheet-chart-heading").value = "실시간 매출";
  userClick(document.getElementById("btn-sheet-chart-ok"));
  await flushAsyncTurns(6);

  const layer = document.getElementById("sheet-chart-layer");
  const heightsOf = () =>
    Array.from(layer.querySelectorAll(".eo-chart-bar")).map((el) => Number(el.getAttribute("height")));
  const before = heightsOf();
  assert.equal(before.length, 4);
  assert.ok(before[3] > before[0], "초기 B5(=B2+B3=360) 막대가 B2(120)보다 큼");

  setCell(document, "B2", "600");
  await flushAsyncTurns(8);

  const after = heightsOf();
  assert.equal(after.length, 4);
  assert.ok(after[0] > after[1], "B2(600) 변경 즉시 첫 막대가 B3(240)보다 커짐");
  assert.ok(after[3] > after[0], "수식 셀 B5(=600+240=840) 막대도 함께 갱신됨");

  host.__EASYOFFICE_SHEET_LAYOUT__.setCharts("Sheet1", [
    {
      id: "chart-cross",
      chartType: "column",
      title: "교차 시트",
      dataRange: "Sheet1!E1:F4",
      switchRowCol: false,
      legend: "right",
      anchor: { fromCol: 4, fromRow: 1, toCol: 9, toRow: 12, left: 360, top: 30, width: 380, height: 240 },
      series: [
        {
          name: "산출",
          nameRef: "Sheet1!$F$1",
          categoriesRef: "Sheet1!$E$2:$E$4",
          valuesRef: "Sheet1!$F$2:$F$4",
        },
      ],
    },
  ]);
  await flushAsyncTurns(4);
  assert.equal(layer.querySelectorAll(".eo-chart-bar").length, 3, "명시 series 참조 렌더링");
});

test("PD-254: 부동 차트 카드 드래그 이동·리사이즈(anchor 동기화)·편집·Delete 삭제·Undo/Redo가 동작한다", async () => {
  const { document, sandbox, userClick, host } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-chart-ops" });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  await seedQuarterlySheet(document);

  userClick(menuCommand(document, "insert-chart"));
  await flushAsyncTurns(6);
  document.getElementById("sheet-chart-range").value = "A1:B4";
  document.getElementById("sheet-chart-heading").value = "초기 제목";
  userClick(document.getElementById("btn-sheet-chart-ok"));
  await flushAsyncTurns(6);

  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const layer = document.getElementById("sheet-chart-layer");
  let card = layer.querySelector(".sheet-chart-card");
  const initialAnchor = structuredClone(layoutApi.chartsOf("Sheet1")[0].anchor);

  const head = card.querySelector(".sheet-chart-head");
  head.dispatchEvent({ type: "mousedown", button: 0, clientX: 400, clientY: 50 });
  document.dispatchEvent({ type: "mousemove", clientX: 592, clientY: 122 });
  document.dispatchEvent({ type: "mouseup", clientX: 592, clientY: 122 });
  await flushAsyncTurns(6);

  const movedAnchor = structuredClone(layoutApi.chartsOf("Sheet1")[0].anchor);
  assert.equal(movedAnchor.left, initialAnchor.left + 192);
  assert.equal(movedAnchor.top, initialAnchor.top + 72);
  assert.ok(movedAnchor.fromCol > initialAnchor.fromCol, "이동 후 fromCol 그리드 좌표 동기화");
  assert.ok(movedAnchor.fromRow > initialAnchor.fromRow, "이동 후 fromRow 그리드 좌표 동기화");

  card = layer.querySelector(".sheet-chart-card");
  const resizeGrip = card.querySelector(".sheet-chart-resize");
  assert.ok(resizeGrip, "리사이즈 핸들 존재");
  resizeGrip.dispatchEvent({ type: "mousedown", button: 0, clientX: 700, clientY: 300 });
  document.dispatchEvent({ type: "mousemove", clientX: 892, clientY: 396 });
  document.dispatchEvent({ type: "mouseup", clientX: 892, clientY: 396 });
  await flushAsyncTurns(6);

  const resizedAnchor = structuredClone(layoutApi.chartsOf("Sheet1")[0].anchor);
  assert.equal(resizedAnchor.width, movedAnchor.width + 192);
  assert.equal(resizedAnchor.height, movedAnchor.height + 96);
  assert.ok(resizedAnchor.toCol > movedAnchor.toCol, "리사이즈 후 toCol 그리드 좌표 동기화");
  assert.ok(resizedAnchor.toRow > movedAnchor.toRow, "리사이즈 후 toRow 그리드 좌표 동기화");

  card = layer.querySelector(".sheet-chart-card");
  userClick(card.querySelector('[data-chart-act="edit"]'));
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("sheet-chart-title").textContent, "차트 데이터 편집");
  assert.equal(document.getElementById("btn-sheet-chart-delete").hidden, false);
  document.getElementById("sheet-chart-heading").value = "수정된 제목";
  userClick(document.getElementById("btn-sheet-chart-ok"));
  await flushAsyncTurns(6);
  assert.equal(layoutApi.chartsOf("Sheet1")[0].title, "수정된 제목");

  card = layer.querySelector(".sheet-chart-card");
  card.focus();
  card.dispatchEvent({ type: "keydown", key: "Delete", target: card });
  await flushAsyncTurns(6);
  assert.equal(layoutApi.chartsOf("Sheet1").length, 0, "Delete 키로 차트 삭제");
  assert.equal(layer.hidden, true);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.chartsOf("Sheet1").length, 1, "Undo로 삭제 복원");
  assert.equal(layoutApi.chartsOf("Sheet1")[0].title, "수정된 제목");
  assert.equal(layer.hidden, false);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.chartsOf("Sheet1")[0].title, "초기 제목", "Undo로 제목 편집 복원");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.chartsOf("Sheet1")[0].anchor.width, movedAnchor.width, "Undo로 리사이즈 복원");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.chartsOf("Sheet1")[0].anchor.left, initialAnchor.left, "Undo로 이동 복원");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.chartsOf("Sheet1")[0].anchor.left, movedAnchor.left, "Redo로 이동 재실행");
});

test("PD-254: save-payload 직렬화·file-open 재적재 복원 및 office-print 벡터 SVG 인쇄 오버레이를 검증한다", async () => {
  class FakeStyleSheet {
    constructor() { this.cssRules = []; }
    insertRule(text, at) {
      this.cssRules.splice(at, 0, { cssText: text, style: { props: {}, setProperty(k, v) { this.props[k] = v; } } });
      return at;
    }
  }
  const printedRoots = [];
  const { document, sandbox, userClick, host } = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-chart-io-print",
  });
  host.CSSStyleSheet = FakeStyleSheet;
  host.requestAnimationFrame = (cb) => cb();
  host.getComputedStyle = () => ({ getPropertyValue: (p) => (p === "font-size" ? "11pt" : "") });
  host.print = () => { printedRoots.push(document.getElementById("eo-print-root")); };
  document.adoptedStyleSheets = [];
  await flushAsyncTurns(10);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  await seedQuarterlySheet(document);

  userClick(menuCommand(document, "insert-chart"));
  await flushAsyncTurns(6);
  document.getElementById("sheet-chart-range").value = "A1:B5";
  document.getElementById("sheet-chart-heading").value = "인쇄·저장 차트";
  userClick(document.getElementById("btn-sheet-chart-ok"));
  await flushAsyncTurns(6);

  const saveApi = host.__EASYOFFICE_SAVE_PAYLOAD__;
  const layoutBySheet = host.__EASYOFFICE_SHEET_LAYOUT__.getAll();
  const sheetsPayload = saveApi.buildSheetsPayload(
    ["Sheet1"],
    { Sheet1: { A1: { value: "분기", formula: "" }, B1: { value: "매출", formula: "" }, A2: { value: "1분기", formula: "" }, B2: { value: "120", formula: "" } } },
    {},
    {},
    layoutBySheet
  );
  assert.equal(Array.isArray(sheetsPayload[0].charts), true, "save-payload에 charts 배열 포함");
  assert.equal(sheetsPayload[0].charts.length, 1);
  assert.equal(sheetsPayload[0].charts[0].title, "인쇄·저장 차트");
  assert.equal(sheetsPayload[0].charts[0].chartType, "column");

  document.defaultView.__EASYOFFICE_NATIVE_MENU__("file:print");
  await flushAsyncTurns(10);
  assert.equal(printedRoots.length, 1, "인쇄 명령 실행됨");
  const printRoot = printedRoots[0];
  assert.ok(printRoot, "#eo-print-root 생성됨");
  const printChartSvg = printRoot.querySelector(".eo-print-sheet-box .eo-print-chart svg");
  assert.ok(printChartSvg, "#eo-print-root 안에 벡터 <svg> 차트 오버레이 렌더링");
  assert.equal(printChartSvg.querySelectorAll(".eo-chart-bar").length, 4);
  assert.equal(printChartSvg.querySelector(".eo-chart-title")?.textContent, "인쇄·저장 차트");
  sandbox.dispatchWindowEvent("afterprint");

  const reopened = await createWorkspaceHarness();
  await pickFile(reopened.document, makeFixtureFile("sales.xlsx", { sheets: sheetsPayload }), 30);
  assert.equal(reopened.host.__EASYOFFICE_SHEET_LAYOUT__.chartsOf("Sheet1").length, 1, "file-open 후 차트 복원");
  assert.equal(reopened.document.getElementById("sheet-chart-layer").hidden, false);
  assert.equal(
    reopened.document.querySelector("#sheet-chart-layer .eo-chart-title")?.textContent,
    "인쇄·저장 차트",
    "복원 직후 차트 레이어 SVG 즉시 렌더링"
  );
});

test("PD-254: 코파일럿 sheet.chart 연동, 시트 보호 편집 거부, 차트 64개·계열 32개 상한 Fail-Closed를 검증한다", async () => {
  const { document, userClick, host } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-chart-guard" });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  await seedQuarterlySheet(document);

  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const copilotActions = host.__EASYOFFICE_CREATE_COPILOT_ACTIONS__({
    document,
    sheetDataStore: { Sheet1: { A1: { value: "분기", formula: "" }, B1: { value: "매출", formula: "" }, A2: { value: "1분기", formula: "" }, B2: { value: "120", formula: "" } } },
    sheetRowCounts: { Sheet1: 12 },
    slideDeckState: [],
    getActiveModule: () => "SHEET",
    getActiveSheetName: () => "Sheet1",
    getActiveSlideIndex: () => 0,
    getSheetController: () => null,
    getWorkspacePanels: () => null,
    getPdfEdit: () => null,
    activatePane: async () => {},
    captureWorkspaceSnapshot: () => ({ sheetLayout: layoutApi.getAll() }),
    restoreWorkspaceSnapshot: async (snap) => {
      layoutApi.replaceAll(snap.sheetLayout || {});
    },
    commitDeferredUndoSnapshot: () => {},
    noteActivity: () => {},
    extractEditableDocumentText: () => "",
    localScrubPii: (s) => ({ scrubbedText: s, totalMasked: 0 }),
  });

  const copilotRes = await copilotActions.applyOperations(
    "SHEET",
    "edit",
    [{ op: "sheet.chart", chartType: "line", dataRange: "A1:B2", title: "코파일럿 차트", legend: "bottom" }],
    "turn-chart-1"
  );
  assert.equal(copilotRes.applied, true);
  assert.equal(layoutApi.chartsOf("Sheet1").length, 1);
  assert.equal(layoutApi.chartsOf("Sheet1")[0].title, "코파일럿 차트");
  await copilotActions.undoLastTurn("turn-chart-1");
  assert.equal(layoutApi.chartsOf("Sheet1").length, 0, "코파일럿 차트 Undo 복원");

  userClick(menuCommand(document, "insert-chart"));
  await flushAsyncTurns(6);
  document.getElementById("sheet-chart-range").value = "A1:B4";
  document.getElementById("sheet-chart-heading").value = "보호 대상 차트";
  userClick(document.getElementById("btn-sheet-chart-ok"));
  await flushAsyncTurns(6);
  assert.equal(layoutApi.chartsOf("Sheet1").length, 1);

  const beforeProtected = structuredClone(layoutApi.chartsOf("Sheet1")[0]);
  layoutApi.setProtect("Sheet1", { on: true, deny: ["objects"] });
  await flushAsyncTurns(4);

  const card = document.querySelector("#sheet-chart-layer .sheet-chart-card");
  const head = card.querySelector(".sheet-chart-head");
  head.dispatchEvent({ type: "mousedown", button: 0, clientX: 400, clientY: 50 });
  document.dispatchEvent({ type: "mousemove", clientX: 600, clientY: 150 });
  document.dispatchEvent({ type: "mouseup", clientX: 600, clientY: 150 });
  await flushAsyncTurns(4);
  assert.equal(layoutApi.chartsOf("Sheet1")[0].anchor.left, beforeProtected.anchor.left, "보호 상태에서 드래그 이동 거부");

  const resizeGrip = card.querySelector(".sheet-chart-resize");
  resizeGrip.dispatchEvent({ type: "mousedown", button: 0, clientX: 700, clientY: 300 });
  document.dispatchEvent({ type: "mousemove", clientX: 900, clientY: 450 });
  document.dispatchEvent({ type: "mouseup", clientX: 900, clientY: 450 });
  await flushAsyncTurns(4);
  assert.equal(layoutApi.chartsOf("Sheet1")[0].anchor.width, beforeProtected.anchor.width, "보호 상태에서 리사이즈 거부");

  card.dispatchEvent({ type: "keydown", key: "Delete", target: card });
  userClick(card.querySelector('[data-chart-act="delete"]'));
  await flushAsyncTurns(4);
  assert.equal(layoutApi.chartsOf("Sheet1").length, 1, "보호 상태에서 삭제 거부");

  card.dispatchEvent({ type: "dblclick", target: card });
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("sheet-chart-dialog").open, false, "보호 상태에서 차트 편집 대화상자 오픈 거부");

  await assert.rejects(
    () =>
      copilotActions.applyOperations(
        "SHEET",
        "edit",
        [{ op: "sheet.chart", chartType: "column", dataRange: "A1:B2", title: "거부됨" }],
        "turn-chart-blocked"
      ),
    /보호/
  );

  layoutApi.setProtect("Sheet1", null);
  await flushAsyncTurns(4);

  const maxCharts = Array.from({ length: 64 }, (_, i) => ({
    id: `chart-${i + 1}`,
    chartType: "column",
    title: `차트 ${i + 1}`,
    dataRange: "A1:B3",
    switchRowCol: false,
    legend: "right",
    anchor: { fromCol: 4, fromRow: 1, toCol: 8, toRow: 10, left: 360, top: 30, width: 320, height: 200 },
  }));
  layoutApi.setCharts("Sheet1", maxCharts);
  assert.equal(layoutApi.chartsOf("Sheet1").length, 64);

  assert.throws(
    () => layoutApi.setCharts("Sheet1", [...maxCharts, { ...maxCharts[0], id: "chart-65" }]),
    /64/,
    "setCharts 64개 초과 Fail-Closed"
  );
  assert.equal(layoutApi.chartsOf("Sheet1").length, 64, "초과 거부 후 기존 64개 유지");

  assert.throws(
    () =>
      layoutApi.setCharts("Sheet1", [
        {
          ...maxCharts[0],
          series: Array.from({ length: 33 }, (_, k) => ({ name: `S${k}`, valuesRef: "Sheet1!$B$2:$B$3" })),
        },
      ]),
    /32/,
    "계열 32개 초과 Fail-Closed"
  );

  assert.throws(
    () => layoutApi.setCharts("Sheet1", [{ ...maxCharts[0], chartType: "radar" }]),
    /차트 종류/,
    "미지원 chartType Fail-Closed"
  );

  userClick(menuCommand(document, "insert-chart"));
  await flushAsyncTurns(6);
  if (document.getElementById("sheet-chart-dialog").open) {
    document.getElementById("sheet-chart-range").value = "A1:B3";
    userClick(document.getElementById("btn-sheet-chart-ok"));
    await flushAsyncTurns(6);
  }
  assert.equal(layoutApi.chartsOf("Sheet1").length, 64, "대화상자에서도 64개 상한 초과 생성 거부");
});
