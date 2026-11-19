import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// Dynamic PivotTable WebUI suite (PD-255):
// 1. Multi-row, column, report-filter, and multi-value pivot creation + `pivots` layout persistence + `#sheet-pivot-bar`
// 2. Source sheet edits & table (`표1`) row expansion -> `새로 고침` (`Alt+F5`) and `모두 새로 고침`
// 3. `데이터 원본 변경` in-place rebuild (no new sheet), `삭제`, and single-step Undo/Redo
// 4. Pivot `슬라이서 삽입` -> single/multi-select/clear filtering + shared `cacheId` multi-pivot live sync + Undo/Redo
// 5. `save-payload` (`sheets[i].pivots`) & `file-open` round-trip with live refresh/slicer + Fail-Closed bounds

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const $ = (h, id) => h.document.getElementById(id);

async function openSheet() {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const capturedConverts = [];
  const capturedSheetActions = [];
  const testIpc = async (command, payload) => {
    const body = typeof payload === "string" && payload.trimStart().startsWith("{") ? JSON.parse(payload) : null;
    if (command === "convert_document_format" && body && !body.action) {
      capturedConverts.push(body);
    }
    if (command === "evaluate_sheet" && body?.sessionAction) {
      capturedSheetActions.push(body);
    }
    return offline[command](payload);
  };
  const h = await createWorkspaceHarness({ transformHtml: demoHtml, testIpc });
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(6);
  h.capturedConverts = capturedConverts;
  h.capturedSheetActions = capturedSheetActions;
  return h;
}

async function loadFixture(h, model, fileName = "피벗원본.xlsx") {
  const bytes = new TextEncoder().encode("EOFIXTURE:" + JSON.stringify(model));
  const res = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(fileName, bytes);
  await flushAsyncTurns(12);
  assert.equal(res.ok, true);
}

function choose(h, id, value) {
  const el = $(h, id);
  assert.ok(el, `missing #${id}`);
  el.value = String(value);
  el.dispatchEvent({ type: "change", target: el, bubbles: true });
}

async function switchTab(h, name) {
  const tab = h.document.querySelector(`#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="${name}"]`);
  assert.ok(tab, `missing sheet tab ${name}`);
  h.userClick(tab);
  await flushAsyncTurns(10);
}

async function typeCell(h, ref, text) {
  const td = cell(h.document, ref);
  assert.ok(td, `missing cell ${ref}`);
  td.focus();
  await flushAsyncTurns(2);
  td.textContent = text;
  td.dispatchEvent({ type: "input", target: td });
  td.dispatchEvent({ type: "keydown", key: "Enter", target: td });
  await flushAsyncTurns(12);
}

const pivots = (h, sheet) => h.host.__EASYOFFICE_SHEET_LAYOUT__.pivotsOf(sheet);
const tables = (h, sheet = "Sheet1") => h.host.__EASYOFFICE_SHEET_LAYOUT__.tablesOf(sheet);

const undo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(16);
};
const redo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(16);
};

test("Scenario 1: multi-row, column, report-filter, and multi-value pivot builds tabular report, persists pivots layout, and toggles #sheet-pivot-bar", async () => {
  const h = await openSheet();
  await loadFixture(h, {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "지역" }, { ref: "B1", raw: "품목" }, { ref: "C1", raw: "수량" }, { ref: "D1", raw: "단가" }, { ref: "E1", raw: "월" }, { ref: "F1", raw: "구분" },
          { ref: "A2", raw: "서울" }, { ref: "B2", raw: "노트북" }, { ref: "C2", raw: "10" }, { ref: "D2", raw: "1000" }, { ref: "E2", raw: "7월" }, { ref: "F2", raw: "온라인" },
          { ref: "A3", raw: "서울" }, { ref: "B3", raw: "노트북" }, { ref: "C3", raw: "20" }, { ref: "D3", raw: "1200" }, { ref: "E3", raw: "8월" }, { ref: "F3", raw: "오프라인" },
          { ref: "A4", raw: "서울" }, { ref: "B4", raw: "마우스" }, { ref: "C4", raw: "30" }, { ref: "D4", raw: "50" }, { ref: "E4", raw: "7월" }, { ref: "F4", raw: "온라인" },
          { ref: "A5", raw: "부산" }, { ref: "B5", raw: "노트북" }, { ref: "C5", raw: "15" }, { ref: "D5", raw: "1100" }, { ref: "E5", raw: "7월" }, { ref: "F5", raw: "온라인" },
          { ref: "A6", raw: "부산" }, { ref: "B6", raw: "노트북" }, { ref: "C6", raw: "25" }, { ref: "D6", raw: "900" }, { ref: "E6", raw: "7월" }, { ref: "F6", raw: "오프라인" },
          { ref: "A7", raw: "부산" }, { ref: "B7", raw: "마우스" }, { ref: "C7", raw: "40" }, { ref: "D7", raw: "60" }, { ref: "E7", raw: "8월" }, { ref: "F7", raw: "온라인" },
        ],
      },
    ],
  });

  assert.equal($(h, "sheet-pivot-bar").hidden, true);
  h.userClick(commandItem(h.document, "sheet-mod-pivot"));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-pivot-dialog").open, true);
  assert.equal($(h, "btn-sheet-pivot-apply").textContent, "만들기");
  assert.equal($(h, "sheet-pivot-range").value, "A1:F7");

  choose(h, "sheet-pivot-filter-select", "5"); // 구분
  choose(h, "sheet-pivot-row-select", "0"); // 지역
  choose(h, "sheet-pivot-row2-select", "1"); // 품목
  choose(h, "sheet-pivot-col-select", "4"); // 월
  choose(h, "sheet-pivot-value-select", "2"); // 수량
  choose(h, "sheet-pivot-agg-select", "sum");
  choose(h, "sheet-pivot-value2-select", "3"); // 단가
  choose(h, "sheet-pivot-agg2-select", "average");

  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);

  assert.equal($(h, "sheet-pivot-dialog").open, false);
  assert.equal(h.document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab, "피벗1");

  const pivotCall = h.capturedSheetActions.find((c) => c.sessionAction === "pivot");
  assert.ok(pivotCall);
  assert.deepEqual(pivotCall.rowFields, [0, 1]);
  assert.deepEqual(pivotCall.colFields, [4]);
  assert.deepEqual(pivotCall.filterFields, [5]);
  assert.deepEqual(pivotCall.valueFields, [
    { col: 2, agg: "sum", name: "합계: 수량" },
    { col: 3, agg: "average", name: "평균: 단가" },
  ]);

  // Report filter summary row + blank row + 2 header rows + 4 data rows + grand total row = A1:H9
  assert.equal(cell(h.document, "A1").textContent, "구분");
  assert.equal(cell(h.document, "B1").textContent, "(모두)");
  assert.equal(cell(h.document, "A2").textContent, "");
  assert.equal(cell(h.document, "C3").textContent, "7월");
  assert.equal(cell(h.document, "E3").textContent, "8월");
  assert.equal(cell(h.document, "G3").textContent, "합계");
  assert.equal(cell(h.document, "A4").textContent, "지역");
  assert.equal(cell(h.document, "B4").textContent, "품목");
  assert.equal(cell(h.document, "C4").textContent, "합계: 수량");
  assert.equal(cell(h.document, "D4").textContent, "평균: 단가");

  // 서울 / 노트북: 7월 (10, 1000), 8월 (20, 1200), 합계 (30, 1100)
  assert.equal(cell(h.document, "A5").textContent, "서울");
  assert.equal(cell(h.document, "B5").textContent, "노트북");
  assert.equal(cell(h.document, "C5").textContent, "10");
  assert.equal(cell(h.document, "D5").textContent, "1000");
  assert.equal(cell(h.document, "E5").textContent, "20");
  assert.equal(cell(h.document, "F5").textContent, "1200");
  assert.equal(cell(h.document, "G5").textContent, "30");
  assert.equal(cell(h.document, "H5").textContent, "1100");

  // 부산 / 노트북: 7월 (15+25=40, avg(1100,900)=1000)
  assert.equal(cell(h.document, "A7").textContent, "부산");
  assert.equal(cell(h.document, "B7").textContent, "노트북");
  assert.equal(cell(h.document, "C7").textContent, "40");
  assert.equal(cell(h.document, "D7").textContent, "1000");

  // Grand totals row (A9:H9): 7월 sum=80, avg=(1000+50+1100+900)/4=762.5; 8월 sum=60, avg=(1200+60)/2=630; grand sum=140
  assert.equal(cell(h.document, "A9").textContent, "합계");
  assert.equal(cell(h.document, "C9").textContent, "80");
  assert.equal(cell(h.document, "D9").textContent, "762.5");
  assert.equal(cell(h.document, "E9").textContent, "60");
  assert.equal(cell(h.document, "F9").textContent, "630");
  assert.equal(cell(h.document, "G9").textContent, "140");

  const storedPivots = pivots(h, "피벗1");
  assert.equal(storedPivots.length, 1);
  assert.deepEqual(storedPivots[0], {
    name: "피벗 테이블1",
    cacheId: 1,
    sourceSheet: "Sheet1",
    sourceRange: "A1:F7",
    sourceTable: null,
    hasHeader: true,
    location: "A1:H9",
    fieldNames: ["지역", "품목", "수량", "단가", "월", "구분"],
    rowFields: [0, 1],
    colFields: [4],
    filterFields: [5],
    valueFields: [
      { col: 2, agg: "sum", name: "합계: 수량" },
      { col: 3, agg: "average", name: "평균: 단가" },
    ],
    filters: [],
    slicers: [],
  });

  // Contextual `#sheet-pivot-bar` visibility follows active cell inside/outside `location`
  assert.equal($(h, "sheet-pivot-bar").hidden, false);
  assert.equal($(h, "sheet-pivot-name").value, "피벗 테이블1");
  cell(h.document, "A11").focus();
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-pivot-bar").hidden, true);
  cell(h.document, "C5").focus();
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-pivot-bar").hidden, false);

  // Rename pivot via `#sheet-pivot-name`
  const nameInput = $(h, "sheet-pivot-name");
  nameInput.focus();
  nameInput.value = "월별 지역 피벗";
  nameInput.dispatchEvent({ type: "keydown", key: "Enter", target: nameInput });
  await flushAsyncTurns(10);
  assert.equal(pivots(h, "피벗1")[0].name, "월별 지역 피벗");
  await undo(h);
  assert.equal(pivots(h, "피벗1")[0].name, "피벗 테이블1");
});

test("Scenario 2: editing source cells and appending rows to table 표1 auto-updates pivot via 새로 고침 (Alt+F5) and 모두 새로 고침", async () => {
  const h = await openSheet();
  await loadFixture(h, {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "지역" }, { ref: "B1", raw: "매출" },
          { ref: "A2", raw: "서울" }, { ref: "B2", raw: "100" },
          { ref: "A3", raw: "부산" }, { ref: "B3", raw: "200" },
        ],
        tables: [
          {
            name: "표1",
            range: "A1:B3",
            header: true,
            totals: false,
            columns: ["지역", "매출"],
            style: "TableStyleMedium2",
            firstColumn: false,
            lastColumn: false,
            rowStripes: true,
            columnStripes: false,
            filter: [],
          },
        ],
      },
      {
        name: "Sheet2",
        cells: [
          { ref: "A1", raw: "부서" }, { ref: "B1", raw: "비용" },
          { ref: "A2", raw: "개발" }, { ref: "B2", raw: "50" },
          { ref: "A3", raw: "영업" }, { ref: "B3", raw: "70" },
        ],
      },
    ],
  });

  // Create Pivot 1 from table `표1`
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-table-bar [data-table-act="pivot"]'));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-pivot-range").value, "표1");
  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);
  assert.equal(pivots(h, "피벗1")[0].sourceTable, "표1");
  assert.equal(pivots(h, "피벗1")[0].sourceRange, "A1:B3");
  assert.equal(cell(h.document, "B2").textContent, "100");
  assert.equal(cell(h.document, "B4").textContent, "300");

  // Create Pivot 2 from `Sheet2!A1:B3`
  await switchTab(h, "Sheet2");
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-mod-pivot"));
  await flushAsyncTurns(4);
  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);
  assert.equal(h.document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab, "피벗2");
  assert.equal(cell(h.document, "B2").textContent, "50");

  // Modify Sheet1 (`B2` 100 -> 150, append row 4 `대전` / `250` expanding `표1` to A1:B4) and Sheet2 (`B2` 50 -> 90)
  await switchTab(h, "Sheet1");
  await typeCell(h, "B2", "150");
  await typeCell(h, "A4", "대전");
  await typeCell(h, "B4", "250");
  assert.equal(tables(h, "Sheet1")[0].range, "A1:B4", "typing under 표1 expanded the table to row 4");

  await switchTab(h, "Sheet2");
  await typeCell(h, "B2", "90");

  // Switch to 피벗1 and press Alt+F5 -> refreshes 피벗1 (picking up expanded 표1 A1:B4!)
  await switchTab(h, "피벗1");
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  h.sandbox.dispatchWindowKeydown({ key: "F5", altKey: true });
  await flushAsyncTurns(16);

  assert.equal(pivots(h, "피벗1")[0].sourceRange, "A1:B4");
  assert.equal(pivots(h, "피벗1")[0].location, "A1:B5");
  assert.equal(cell(h.document, "B2").textContent, "150"); // 서울
  assert.equal(cell(h.document, "A4").textContent, "대전");
  assert.equal(cell(h.document, "B4").textContent, "250"); // 대전
  assert.equal(cell(h.document, "B5").textContent, "600"); // 150 + 200 + 250

  // Undo reverts Alt+F5 refresh in 1 step; Redo re-applies it
  await undo(h);
  assert.equal(pivots(h, "피벗1")[0].sourceRange, "A1:B3");
  assert.equal(cell(h.document, "B4").textContent, "300");
  await redo(h);
  assert.equal(cell(h.document, "B5").textContent, "600");

  // 피벗2 still has old 50 until `모두 새로 고침` runs from 피벗1's bar
  h.userClick(h.document.querySelector('#sheet-pivot-bar [data-pivot-act="refresh-all"]'));
  await flushAsyncTurns(16);
  await switchTab(h, "피벗2");
  assert.equal(cell(h.document, "B2").textContent, "90");
  assert.equal(cell(h.document, "B4").textContent, "160"); // 90 + 70
});

test("Scenario 3: 데이터 원본 변경 rebuilds pivot in-place without adding a sheet, supports 삭제, and restores via Undo/Redo", async () => {
  const h = await openSheet();
  await loadFixture(h, {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "지역" }, { ref: "B1", raw: "분기" }, { ref: "C1", raw: "매출" },
          { ref: "A2", raw: "서울" }, { ref: "B2", raw: "1분기" }, { ref: "C2", raw: "100" },
          { ref: "A3", raw: "서울" }, { ref: "B3", raw: "2분기" }, { ref: "C3", raw: "300" },
          { ref: "A4", raw: "부산" }, { ref: "B4", raw: "1분기" }, { ref: "C4", raw: "200" },
          { ref: "A5", raw: "부산" }, { ref: "B5", raw: "2분기" }, { ref: "C5", raw: "400" },
        ],
      },
    ],
  });

  // Initial pivot: only A1:C4, row=지역(0), col=(none), value=매출(2, sum)
  h.userClick(commandItem(h.document, "sheet-mod-pivot"));
  await flushAsyncTurns(4);
  $(h, "sheet-pivot-range").value = "A1:C4";
  $(h, "sheet-pivot-range").dispatchEvent({ type: "change", target: $(h, "sheet-pivot-range"), bubbles: true });
  choose(h, "sheet-pivot-row-select", "0");
  choose(h, "sheet-pivot-col-select", "");
  choose(h, "sheet-pivot-value-select", "2");
  choose(h, "sheet-pivot-agg-select", "sum");
  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);

  const sheetCountAfterCreate = h.document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]").length;
  assert.equal(cell(h.document, "B2").textContent, "400"); // 서울: 100+300
  assert.equal(cell(h.document, "B3").textContent, "200"); // 부산: 200
  assert.equal(pivots(h, "피벗1")[0].location, "A1:B4");

  // Click `데이터 원본 변경` -> button says "확인", range pre-filled with "Sheet1!A1:C4"
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-pivot-bar [data-pivot-act="source"]'));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-pivot-dialog").open, true);
  assert.equal($(h, "btn-sheet-pivot-apply").textContent, "확인");
  assert.equal($(h, "sheet-pivot-range").value, "Sheet1!A1:C4");

  // Expand range to Sheet1!A1:C5, add column field 분기(1), switch agg to average
  $(h, "sheet-pivot-range").value = "Sheet1!A1:C5";
  $(h, "sheet-pivot-range").dispatchEvent({ type: "change", target: $(h, "sheet-pivot-range"), bubbles: true });
  choose(h, "sheet-pivot-row-select", "0");
  choose(h, "sheet-pivot-col-select", "1");
  choose(h, "sheet-pivot-value-select", "2");
  choose(h, "sheet-pivot-agg-select", "average");
  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);

  assert.equal($(h, "sheet-pivot-dialog").open, false);
  assert.equal(
    h.document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]").length,
    sheetCountAfterCreate,
    "reconfiguring source updated in-place without adding a sheet"
  );
  assert.equal(pivots(h, "피벗1")[0].location, "A1:D4");
  assert.equal(cell(h.document, "B1").textContent, "1분기");
  assert.equal(cell(h.document, "C1").textContent, "2분기");
  assert.equal(cell(h.document, "D2").textContent, "200"); // 서울 avg(100,300)
  assert.equal(cell(h.document, "D3").textContent, "300"); // 부산 avg(200,400)
  assert.equal(cell(h.document, "D4").textContent, "250"); // grand avg(100,300,200,400)

  // Undo restores 1-way sum A1:B4 and clears columns C & D; Redo brings back 2-way average A1:D4
  await undo(h);
  assert.equal(pivots(h, "피벗1")[0].location, "A1:B4");
  assert.equal(cell(h.document, "B2").textContent, "400");
  assert.equal(cell(h.document, "C1").textContent, "");
  assert.equal(cell(h.document, "D4").textContent, "");

  await redo(h);
  assert.equal(pivots(h, "피벗1")[0].location, "A1:D4");
  assert.equal(cell(h.document, "D4").textContent, "250");

  // Delete pivot removes cells + layout in 1 Undo step
  cell(h.document, "B2").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-pivot-bar [data-pivot-act="delete"]'));
  await flushAsyncTurns(14);
  assert.equal(pivots(h, "피벗1").length, 0);
  assert.equal($(h, "sheet-pivot-bar").hidden, true);
  assert.equal(cell(h.document, "A1").textContent, "");
  assert.equal(cell(h.document, "D4").textContent, "");

  await undo(h);
  assert.equal(pivots(h, "피벗1").length, 1);
  assert.equal(cell(h.document, "D4").textContent, "250");
});

test("Scenario 4: pivot slicers filter pivot aggregates in real time and synchronize across pivots sharing cacheId", async () => {
  const h = await openSheet();
  await loadFixture(h, {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "지역" }, { ref: "B1", raw: "품목" }, { ref: "C1", raw: "수량" },
          { ref: "A2", raw: "서울" }, { ref: "B2", raw: "사과" }, { ref: "C2", raw: "10" },
          { ref: "A3", raw: "서울" }, { ref: "B3", raw: "배" }, { ref: "C3", raw: "20" },
          { ref: "A4", raw: "부산" }, { ref: "B4", raw: "사과" }, { ref: "C4", raw: "30" },
          { ref: "A5", raw: "대전" }, { ref: "B5", raw: "포도" }, { ref: "C5", raw: "40" },
        ],
      },
    ],
  });

  // Pivot 1 on 피벗1: row = 지역(0), value = 수량(2, sum)
  h.userClick(commandItem(h.document, "sheet-mod-pivot"));
  await flushAsyncTurns(4);
  choose(h, "sheet-pivot-row-select", "0");
  choose(h, "sheet-pivot-value-select", "2");
  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);

  // Pivot 2 on 피벗2 from exact same source Sheet1!A1:C5: row = 품목(1), value = 수량(2, sum)
  await switchTab(h, "Sheet1");
  cell(h.document, "A1").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-mod-pivot"));
  await flushAsyncTurns(4);
  choose(h, "sheet-pivot-row-select", "1");
  choose(h, "sheet-pivot-value-select", "2");
  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);

  assert.equal(pivots(h, "피벗1")[0].cacheId, 1);
  assert.equal(pivots(h, "피벗2")[0].cacheId, 1, "both pivots share cacheId 1 for Sheet1!A1:C5");
  assert.equal(cell(h.document, "B2").textContent, "40"); // 사과: 10+30 on 피벗2
  assert.equal(cell(h.document, "B5").textContent, "100"); // grand total on 피벗2

  // Switch to 피벗1 and insert slicers for 지역(0) and 품목(1)
  await switchTab(h, "피벗1");
  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-pivot-bar [data-pivot-act="slicer"]'));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-slicer-dialog").open, true);

  h.document.querySelector('#sheet-slicer-columns input[data-slicer-col="0"]').checked = true;
  h.document.querySelector('#sheet-slicer-columns input[data-slicer-col="1"]').checked = true;
  h.userClick($(h, "btn-sheet-slicer-ok"));
  await flushAsyncTurns(14);

  assert.equal($(h, "sheet-slicer-layer").hidden, false);
  const regionCardSelector = '#sheet-slicer-layer .sheet-slicer-card[data-slicer-pivot="피벗 테이블1"][data-slicer-col="0"]';
  const itemCardSelector = '#sheet-slicer-layer .sheet-slicer-card[data-slicer-pivot="피벗 테이블1"][data-slicer-col="1"]';
  const slicerItems = (sel) => [...h.document.querySelectorAll(`${sel} .sheet-slicer-item`)];
  assert.deepEqual(slicerItems(regionCardSelector).map((b) => b.textContent), ["대전", "부산", "서울"]);

  // Single-click "서울" -> filters 피벗1 to 서울 (30), mutes "포도" in 품목 slicer, AND updates 피벗2 (사과=10, 배=20, 합계=30)!
  h.userClick(slicerItems(regionCardSelector).find((b) => b.textContent === "서울"));
  await flushAsyncTurns(18);

  assert.equal(cell(h.document, "A2").textContent, "서울");
  assert.equal(cell(h.document, "B2").textContent, "30");
  assert.equal(cell(h.document, "A3").textContent, "합계");
  assert.equal(cell(h.document, "B3").textContent, "30");
  assert.deepEqual(
    slicerItems(itemCardSelector).filter((b) => b.classList.contains("is-muted")).map((b) => b.textContent),
    ["포도"]
  );

  // Shared cacheId check on 피벗2!
  await switchTab(h, "피벗2");
  assert.deepEqual(pivots(h, "피벗2")[0].filters, [{ col: 0, values: ["서울"], blank: false }]);
  assert.equal(cell(h.document, "A2").textContent, "사과");
  assert.equal(cell(h.document, "B2").textContent, "10");
  assert.equal(cell(h.document, "A3").textContent, "배");
  assert.equal(cell(h.document, "B3").textContent, "20");
  assert.equal(cell(h.document, "B4").textContent, "30");

  // Switch back to 피벗1, enable multi-select, add "부산" -> 서울(30) + 부산(30) = 60, 피벗2 사과 becomes 40!
  await switchTab(h, "피벗1");
  h.userClick(h.document.querySelector(`${regionCardSelector} [data-slicer-act="multi"]`));
  await flushAsyncTurns(10);
  h.userClick(slicerItems(regionCardSelector).find((b) => b.textContent === "부산"));
  await flushAsyncTurns(18);

  assert.equal(cell(h.document, "B4").textContent, "60");
  await switchTab(h, "피벗2");
  assert.equal(cell(h.document, "B2").textContent, "40"); // 사과 10 + 30
  assert.equal(cell(h.document, "B4").textContent, "60");

  // Clear filter on 피벗1 -> restores grand total 100 on both pivots
  await switchTab(h, "피벗1");
  h.userClick(h.document.querySelector(`${regionCardSelector} [data-slicer-act="clear"]`));
  await flushAsyncTurns(18);
  assert.deepEqual(pivots(h, "피벗1")[0].filters, []);
  assert.deepEqual(pivots(h, "피벗2")[0].filters, []);
  assert.equal(cell(h.document, "B5").textContent, "100");

  // Undo restores filtered state (60) across both pivots + slicer selection; Redo clears again (100)
  await undo(h);
  assert.equal(pivots(h, "피벗1")[0].filters.length, 1);
  assert.equal(pivots(h, "피벗2")[0].filters.length, 1);
  assert.equal(cell(h.document, "B4").textContent, "60");
  await redo(h);
  assert.equal(cell(h.document, "B5").textContent, "100");
});

test("Scenario 5: save-payload serializes sheets[i].pivots, file-open restores refresh & slicers, and malformed inputs fail closed", async () => {
  const h = await openSheet();
  await loadFixture(h, {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "지역" }, { ref: "B1", raw: "수량" },
          { ref: "A2", raw: "서울" }, { ref: "B2", raw: "100" },
          { ref: "A3", raw: "부산" }, { ref: "B3", raw: "200" },
        ],
      },
    ],
  });

  // Create pivot + slicer
  h.userClick(commandItem(h.document, "sheet-mod-pivot"));
  await flushAsyncTurns(4);
  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);

  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-pivot-bar [data-pivot-act="slicer"]'));
  await flushAsyncTurns(4);
  h.document.querySelector('#sheet-slicer-columns input[data-slicer-col="0"]').checked = true;
  h.userClick($(h, "btn-sheet-slicer-ok"));
  await flushAsyncTurns(14);

  // Trigger structured save conversion and verify `sheets[i].pivots`
  switchModule(h.document, "CONV");
  await flushAsyncTurns(4);
  $(h, "conv-src-fmt").value = "xlsx";
  $(h, "conv-dst-fmt").value = "xlsx";
  h.userClick($(h, "btn-run-convert"));
  await flushAsyncTurns(12);

  const saved = h.capturedConverts.at(-1);
  assert.ok(saved && Array.isArray(saved.sheets));
  const savedPivotSheet = saved.sheets.find((s) => s.name === "피벗1");
  assert.ok(savedPivotSheet && Array.isArray(savedPivotSheet.pivots));
  assert.equal(savedPivotSheet.pivots.length, 1);
  assert.equal(savedPivotSheet.pivots[0].name, "피벗 테이블1");
  assert.equal(savedPivotSheet.pivots[0].slicers.length, 1);

  // Reopen into a fresh harness via `file-open` and verify live refresh + slicer still work
  const h2 = await openSheet();
  await loadFixture(h2, { sheets: saved.sheets }, "복원피벗.xlsx");
  await switchTab(h2, "Sheet1");
  await typeCell(h2, "B2", "350");

  await switchTab(h2, "피벗1");
  cell(h2.document, "A2").focus();
  await flushAsyncTurns(4);
  assert.equal($(h2, "sheet-pivot-bar").hidden, false);
  assert.equal($(h2, "sheet-slicer-layer").hidden, false);

  // Refresh updates 서울 to 350 (total 550)
  h2.userClick(h2.document.querySelector('#sheet-pivot-bar [data-pivot-act="refresh"]'));
  await flushAsyncTurns(16);
  assert.equal(cell(h2.document, "B2").textContent, "350");
  assert.equal(cell(h2.document, "B4").textContent, "550");

  // Slicer click on restored card filters to 부산 (200)
  const restoredItems = [...h2.document.querySelectorAll('#sheet-slicer-layer .sheet-slicer-card[data-slicer-pivot="피벗 테이블1"] .sheet-slicer-item')];
  h2.userClick(restoredItems.find((b) => b.textContent === "부산"));
  await flushAsyncTurns(16);
  assert.equal(cell(h2.document, "A2").textContent, "부산");
  assert.equal(cell(h2.document, "B2").textContent, "200");
  assert.equal(cell(h2.document, "B3").textContent, "200");

  // Verify exact Rust `sheet_pivot_ipc` (`valuesMatrix` / `valueRowTotals` / `valueColumnTotals` / `valueGrandTotals`) contract
  const model = h2.host.__EASYOFFICE_SHEET_PIVOT_MODEL__;
  const rustShaped = {
    rowKeys: ["서울 / 사과", "부산 / 배"],
    columnKeys: ["7월", "8월"],
    matrix: [[10, 20], [5, 15]],
    rowTotals: [30, 20],
    columnTotals: [15, 35],
    grandTotal: 50,
    rowTuples: [["서울", "사과"], ["부산", "배"]],
    colTuples: [["7월"], ["8월"]],
    valuesMatrix: [
      [[10, 5], [20, 10]],
      [[5, 2.5], [15, 7.5]],
    ],
    valueRowTotals: [[30, 7.5], [20, 5]],
    valueColumnTotals: [[15, 3.75], [35, 8.75]],
    valueGrandTotals: [50, 6.25],
  };
  const rendered = model.buildPivotSheetCells(rustShaped, {
    fieldNames: ["지역", "품목", "수량", "월"],
    rowFields: [0, 1],
    colFields: [3],
    filterFields: [],
    valueFields: [
      { col: 2, agg: "sum", name: "합계: 수량" },
      { col: 2, agg: "average", name: "평균: 수량" },
    ],
    filters: [],
  });
  assert.equal(rendered.location, "A1:H5");
  assert.equal(rendered.cells.D3.value, "5");
  assert.equal(rendered.cells.H3.value, "7.5");
  assert.equal(rendered.cells.G5.value, "50");
  assert.equal(rendered.cells.H5.value, "6.25");

  // Fail-Closed validation on hostile / out-of-bounds pivot payloads
  assert.throws(() => model.pivotsFromJson("not-an-array"), /올바르지 않습니다/);
  assert.throws(
    () => model.pivotsFromJson(Array.from({ length: 33 }, (_, i) => ({
      name: "P" + i, sourceSheet: "S", sourceRange: "A1:B2", location: "A1:B2", rowFields: [0], valueFields: [{ col: 1, agg: "sum" }],
    }))),
    /한도/
  );
  assert.throws(
    () => model.pivotsFromJson([{
      name: "BadAgg", sourceSheet: "S", sourceRange: "A1:B2", location: "A1:B2", rowFields: [0], valueFields: [{ col: 1, agg: "median" }],
    }]),
    /집계/
  );
  assert.throws(
    () => model.pivotsFromJson([{
      name: "EmptyAxes", sourceSheet: "S", sourceRange: "A1:B2", location: "A1:B2", rowFields: [], valueFields: [],
    }]),
    /하나 이상/
  );
  const extPivots = model.pivotsFromJson([
    { name: "ColOnly", sourceSheet: "S", sourceRange: "A1:B3", location: "D1:F2", rowFields: [], colFields: [0], valueFields: [{ col: 1, agg: "sum" }] },
    { name: "RowOnly", sourceSheet: "S", sourceRange: "A1:B3", location: "H1:H3", rowFields: [0], colFields: [], valueFields: [] },
  ]);
  assert.equal(extPivots.length, 2);
  assert.deepEqual(extPivots[0].rowFields, []);
  assert.deepEqual(extPivots[1].valueFields, []);
  assert.throws(
    () => model.computePivotWithReader(
      { range: "A1:C600", header: false, row: 0, col: 1, value: 2, agg: "sum" },
      (r, c) => (c === 2 ? { kind: "number", num: 1, display: "1" } : { kind: "text", text: (c === 0 ? "r" : "c") + r, display: (c === 0 ? "r" : "c") + r })
    ),
    /ResourceBudgetExceeded/
  );
});
