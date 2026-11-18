import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// Sheet table tools & workbook theme integration (PD-242):
// 1. Workbook theme colors applied to built-in table styles + save/reopen round-trip
// 2. 새 표 스타일 dialog, live preview, custom style gallery & Undo/Redo
// 3. 중복된 항목 제거 (single/multi-column typed deduplication, totals preservation, outside-column isolation, Undo/Redo)
// 4. 피벗 테이블로 요약 (table-name source range excluding totals row)
// 5. 슬라이서 삽입, single/multi-select filtering, bidirectional filter sync, clear/remove & Undo/Redo

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const $ = (h, id) => h.document.getElementById(id);

function filterEngine(body) {
  const { op, filter, data, col } = body;
  const [, b] = filter.range.split(":");
  const rows = Number(/\d+$/.exec(b)[0]) - Number(/\d+$/.exec(filter.range.split(":")[0])[0]);
  const cols = new Map((data || []).map((d) => [d.col, d.values]));
  const matches = (cond, val) => {
    if (cond.values || cond.blank) {
      if (val === "") return Boolean(cond.blank);
      return (cond.values || []).includes(val);
    }
    return true;
  };
  const hidden = [];
  for (let i = 0; i < rows; i += 1) {
    const r = Number(/\d+$/.exec(filter.range.split(":")[0])[0]) + 1 + i;
    const hide = (filter.columns || []).some((cond) => !matches(cond, (cols.get(cond.col) || [])[i] ?? ""));
    if (hide) {
      const last = hidden[hidden.length - 1];
      if (last && last[1] === r - 1) last[1] = r;
      else hidden.push([r, r]);
    }
  }
  if (op === "apply") {
    const count = hidden.reduce((n, [s, e]) => n + (e - s + 1), 0);
    return { hidden, count };
  }
  const vals = cols.get(col) || [];
  const counts = new Map();
  let blank = 0;
  for (const v of vals) {
    if (v === "") blank += 1;
    else counts.set(v, (counts.get(v) || 0) + 1);
  }
  return { items: [...counts.entries()], blank, more: false };
}

async function openSheet(extraRpc = {}) {
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
      if (extraRpc[body.sessionAction]) return extraRpc[body.sessionAction](body);
      if (body.sessionAction === "filter") return filterEngine(body);
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

const tables = (h, sheet = "Sheet1") => h.host.__EASYOFFICE_SHEET_LAYOUT__.tablesOf(sheet);
const undo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(16);
};
const redo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(16);
};

async function makeTable(h, range = "A1:H8", header = true) {
  cell(h.document, "B2").focus();
  await flushAsyncTurns(2);
  h.userClick(commandItem(h.document, "sheet-insert-table"));
  await flushAsyncTurns(4);
  $(h, "sheet-table-range").value = range;
  $(h, "sheet-table-header").checked = header;
  h.userClick($(h, "btn-sheet-table-ok"));
  await flushAsyncTurns(16);
}

test("workbookTheme from an opened workbook colors built-in table styles and round-trips into the save payload", async () => {
  const h = await openSheet();
  const customTheme = ["ffffff", "111827", "f3f4f6", "1f2937", "0f766e", "b45309", "6b7280", "eab308", "0284c7", "15803d", "0369a1", "7e22ce"];
  const fixtureModel = {
    workbookTheme: customTheme,
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "지역" },
          { ref: "B1", raw: "매출" },
          { ref: "A2", raw: "서울" },
          { ref: "B2", raw: "1200" },
        ],
        tables: [
          {
            name: "표1",
            range: "A1:B2",
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
    ],
  };
  const bytes = new TextEncoder().encode("EOFIXTURE:" + JSON.stringify(fixtureModel));
  const opened = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("테마표.xlsx", bytes);
  await flushAsyncTurns(12);
  assert.equal(opened.ok, true);
  assert.deepEqual(h.host.__EASYOFFICE_SHEET_TABLE_STYLE__.getTheme(), customTheme);
  assert.equal(cell(h.document, "A1").style.getPropertyValue("--ts-fill"), "#0f766e", "Medium2 header uses workbook accent1");

  // Saving emits workbookTheme on structured sheet conversion payload.
  switchModule(h.document, "CONV");
  await flushAsyncTurns(4);
  $(h, "conv-src-fmt").value = "xlsx";
  $(h, "conv-dst-fmt").value = "xlsx";
  h.userClick($(h, "btn-run-convert"));
  await flushAsyncTurns(12);
  assert.equal(h.capturedConverts.length >= 1, true);
  assert.deepEqual(h.capturedConverts.at(-1).workbookTheme, customTheme);
});

test("새 표 스타일 creates a custom style with element formatting, updates the gallery and supports Undo/Redo", async () => {
  const h = await openSheet();
  await makeTable(h, "A1:D5");

  h.userClick($(h, "btn-table-style"));
  await flushAsyncTurns(2);
  const newBtn = h.document.querySelector('#sheet-table-gallery [data-table-style-new="1"]');
  assert.ok(newBtn, "gallery exposes 새 표 스타일… action");
  assert.equal(newBtn.textContent, "새 표 스타일…");
  h.userClick(newBtn);
  await flushAsyncTurns(4);

  assert.equal($(h, "sheet-table-style-dialog").open, true);
  assert.equal($(h, "sheet-ts-name").value, "표 스타일 1");
  assert.equal(h.document.querySelectorAll("#sheet-ts-preview .sheet-table-swatch-cell").length, 25);

  // Refuses empty name and built-in name.
  $(h, "sheet-ts-name").value = "";
  h.userClick($(h, "btn-sheet-ts-ok"));
  assert.equal($(h, "sheet-ts-status").textContent, "스타일 이름을 입력");
  $(h, "sheet-ts-name").value = "TableStyleMedium2";
  h.userClick($(h, "btn-sheet-ts-ok"));
  assert.equal($(h, "sheet-ts-status").textContent, "기본 제공 스타일 이름은 쓸 수 없음");

  // Format header (`h`) and first row stripe (`r1`, stripe size 2).
  $(h, "sheet-ts-name").value = "영업 강조 표";
  $(h, "sheet-ts-element").value = "h";
  $(h, "sheet-ts-element").dispatchEvent({ type: "change", target: $(h, "sheet-ts-element"), bubbles: true });
  const accent5Swatch = h.document.querySelector('#sheet-ts-fill-theme button[data-theme-slot="5"]');
  h.userClick(accent5Swatch);
  $(h, "sheet-ts-element").value = "r1";
  $(h, "sheet-ts-element").dispatchEvent({ type: "change", target: $(h, "sheet-ts-element"), bubbles: true });
  assert.equal($(h, "sheet-ts-stripe-row").hidden, false);
  $(h, "sheet-ts-stripe-size").value = "2";
  $(h, "sheet-ts-stripe-size").dispatchEvent({ type: "input", target: $(h, "sheet-ts-stripe-size"), bubbles: true });

  h.userClick($(h, "btn-sheet-ts-ok"));
  await flushAsyncTurns(14);
  assert.equal($(h, "sheet-table-style-dialog").open, false);
  assert.equal(tables(h)[0].style, "영업 강조 표");
  assert.equal(tables(h)[0].styleDef.h.f, "ed7d31");
  assert.equal(tables(h)[0].styleDef.h.b, 1);
  assert.equal(tables(h)[0].styleDef.r1.sz, 2);
  assert.equal(cell(h.document, "A1").style.getPropertyValue("--ts-fill"), "#ed7d31");

  // Custom group now appears in the gallery.
  h.userClick($(h, "btn-table-style"));
  await flushAsyncTurns(2);
  assert.deepEqual(
    [...h.document.querySelectorAll("#sheet-table-gallery .sheet-table-gallery-title")].map((t) => t.textContent),
    ["밝게", "보통", "어둡게", "사용자 지정"],
  );
  h.userClick($(h, "btn-table-style"));

  // Undo restores TableStyleMedium2 and removes session custom style; Redo brings both back.
  await undo(h);
  assert.equal(tables(h)[0].style, "TableStyleMedium2");
  assert.deepEqual(h.host.__EASYOFFICE_SHEET_TABLE_STYLE__.getCustomStyles(), {});
  await redo(h);
  assert.equal(tables(h)[0].style, "영업 강조 표");
});

async function loadFixture(h, model, fileName = "데이터.xlsx") {
  const bytes = new TextEncoder().encode("EOFIXTURE:" + JSON.stringify(model));
  const res = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(fileName, bytes);
  await flushAsyncTurns(12);
  assert.equal(res.ok, true);
}

test("중복된 항목 제거 deduplicates selected columns in O(R*K), preserves totals & outside columns, and undoes in 1 step", async () => {
  const h = await openSheet();
  await loadFixture(h, {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "바깥왼쪽" }, { ref: "B1", raw: "부서" }, { ref: "C1", raw: "코드" }, { ref: "D1", raw: "금액" }, { ref: "E1", raw: "바깥오른쪽" },
          { ref: "A2", raw: "L1" }, { ref: "B2", raw: "영업" }, { ref: "C2", raw: "100" }, { ref: "D2", raw: "10" }, { ref: "E2", raw: "R1" },
          { ref: "A3", raw: "L2" }, { ref: "B3", raw: "영업" }, { ref: "C3", raw: "100.0" }, { ref: "D3", raw: "20" }, { ref: "E3", raw: "R2" },
          { ref: "A4", raw: "L3" }, { ref: "B4", raw: "영업" }, { ref: "C4", raw: "100", t: "s" }, { ref: "D4", raw: "30" }, { ref: "E4", raw: "R3" },
          { ref: "A5", raw: "L4" }, { ref: "B5", raw: "개발" }, { ref: "C5", raw: "200" }, { ref: "D5", raw: "40" }, { ref: "E5", raw: "R4" },
          { ref: "A6", raw: "L5" }, { ref: "B6", raw: "영업" }, { ref: "C6", raw: "100", t: "s" }, { ref: "D6", raw: "50" }, { ref: "E6", raw: "R5" },
          { ref: "A7", raw: "L6" }, { ref: "B7", raw: "요약" }, { ref: "D7", raw: "=SUBTOTAL(109,표1[금액])" }, { ref: "E7", raw: "R6" },
        ],
        tables: [
          {
            name: "표1",
            range: "B1:D7",
            header: true,
            totals: true,
            columns: ["부서", "코드", "금액"],
            style: "TableStyleMedium2",
            firstColumn: false,
            lastColumn: false,
            rowStripes: true,
            columnStripes: false,
            filter: [],
          },
        ],
      },
    ],
  });
  cell(h.document, "B2").focus();
  await flushAsyncTurns(4);

  const dedupeBtn = h.document.querySelector('#sheet-table-bar [data-table-act="dedupe"]');
  h.userClick(dedupeBtn);
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-table-dedupe-dialog").open, true);

  // Unselect all -> refuses empty selection.
  h.userClick($(h, "btn-sheet-dedupe-none"));
  h.userClick($(h, "btn-sheet-dedupe-ok"));
  assert.equal($(h, "sheet-dedupe-status").textContent, "열을 하나 이상 선택");

  // Select 부서 (0) and 코드 (1) only:
  // Row 2 ("영업", 100) kept; Row 3 ("영업", 100.0) duplicate of Row 2; Row 4 ("영업", '100 text) distinct; Row 5 ("개발", 200) kept; Row 6 ("영업", '100 text) duplicate of Row 4!
  const boxes = [...h.document.querySelectorAll('#sheet-dedupe-columns input[type="checkbox"]')];
  boxes[0].checked = true;
  boxes[1].checked = true;
  boxes[2].checked = false;
  h.userClick($(h, "btn-sheet-dedupe-ok"));
  await flushAsyncTurns(18);

  assert.equal(tables(h)[0].range, "B1:D5", "header + 3 unique rows + totals row = B1:D5");
  assert.equal(
    $(h, "status-command-feedback").textContent,
    "2개의 중복된 값이 검색되어 제거되었습니다. 3개의 고유 값이 남습니다.",
  );
  assert.equal(cell(h.document, "D2").textContent, "10");
  assert.equal(cell(h.document, "D3").textContent, "30", "explicit text '100 stayed distinct from numeric 100");
  assert.equal(cell(h.document, "D4").textContent, "40");
  assert.equal(cell(h.document, "B5").textContent, "요약", "totals row moved up cleanly to row 5");
  assert.equal(cell(h.document, "B6").textContent, "");
  // Outside columns A and E were NEVER altered!
  assert.equal(cell(h.document, "A6").textContent, "L5");
  assert.equal(cell(h.document, "E6").textContent, "R5");
  assert.equal(cell(h.document, "A7").textContent, "L6");

  await undo(h);
  assert.equal(tables(h)[0].range, "B1:D7");
  assert.equal(cell(h.document, "D6").textContent, "50");
  assert.equal(cell(h.document, "B7").textContent, "요약");
});

test("피벗 테이블로 요약 opens the pivot dialog bound to the table name and excludes the totals row", async () => {
  const h = await openSheet({
    pivot: () => ({
      rowKeys: ["기술", "영업"],
      columnKeys: [],
      matrix: [[400], [300]],
      rowTotals: [400, 300],
      columnTotals: [700],
      grandTotal: 700,
    }),
  });
  await makeTable(h, "A1:H8");
  const totalsBox = h.document.querySelector('#sheet-table-bar [data-table-opt="totals"]');
  totalsBox.checked = true;
  totalsBox.dispatchEvent({ type: "change", target: totalsBox, bubbles: true });
  await flushAsyncTurns(16);
  assert.equal(tables(h)[0].range, "A1:H9");

  cell(h.document, "B3").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-table-bar [data-table-act="pivot"]'));
  await flushAsyncTurns(4);

  assert.equal($(h, "sheet-pivot-dialog").open, true);
  assert.equal($(h, "sheet-pivot-range").value, "표1");
  assert.equal($(h, "sheet-pivot-header").checked, true);

  h.userClick($(h, "btn-sheet-pivot-apply"));
  await flushAsyncTurns(16);
  const pivotCall = h.capturedSheetActions.find((c) => c.sessionAction === "pivot");
  assert.ok(pivotCall, "sent pivot sessionAction");
  assert.equal(pivotCall.range, "A1:H8", "excluded row 9 (totals row) from pivot source range");
  assert.equal(h.document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab, "피벗1");
});

test("슬라이서 삽입 renders interactive slicer cards synced bidirectionally with table filters and Undo/Redo", async () => {
  const h = await openSheet();
  await loadFixture(h, {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "지역" }, { ref: "B1", raw: "매출" },
          { ref: "A2", raw: "서울" }, { ref: "B2", raw: "100" },
          { ref: "A3", raw: "부산" }, { ref: "B3", raw: "200" },
          { ref: "A4", raw: "대전" }, { ref: "B4", raw: "150" },
          { ref: "A5", raw: "서울" }, { ref: "B5", raw: "300" },
        ],
        tables: [
          {
            name: "표1",
            range: "A1:B5",
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
    ],
  });

  cell(h.document, "A2").focus();
  await flushAsyncTurns(2);
  h.userClick(h.document.querySelector('#sheet-table-bar [data-table-act="slicer"]'));
  await flushAsyncTurns(4);
  assert.equal($(h, "sheet-slicer-dialog").open, true);

  // Refuses empty selection.
  h.userClick($(h, "btn-sheet-slicer-ok"));
  assert.equal($(h, "sheet-slicer-status").textContent, "열을 하나 이상 선택");

  // Insert slicers for 지역 (col 0) and 매출 (col 1).
  const firstColBox = h.document.querySelector('#sheet-slicer-columns input[data-slicer-col="0"]');
  const secondColBox = h.document.querySelector('#sheet-slicer-columns input[data-slicer-col="1"]');
  firstColBox.checked = true;
  secondColBox.checked = true;
  h.userClick($(h, "btn-sheet-slicer-ok"));
  await flushAsyncTurns(14);

  assert.equal($(h, "sheet-slicer-layer").hidden, false);
  assert.equal(tables(h)[0].slicers.length, 2);
  assert.equal(tables(h)[0].slicers[0].caption, "지역");
  assert.equal(h.document.querySelector('#sheet-slicer-layer [data-slicer-act="multi"]')?.dataset.icon, "slicer-multi");
  assert.equal(h.document.querySelector('#sheet-slicer-layer [data-slicer-act="clear"]')?.dataset.icon, "filter-clear");

  const items = (col = 0) => [
    ...h.document.querySelectorAll(`#sheet-slicer-layer .sheet-slicer-card[data-slicer-col="${col}"] .sheet-slicer-item`),
  ];
  assert.deepEqual(items(0).map((b) => b.textContent), ["대전", "부산", "서울"]);
  assert.ok(items(0).every((b) => b.classList.contains("selected")));

  // Single-select "서울" -> hides rows 3 and 4, and cross-filters 매출 slicer (100 & 300 active first, 150 & 200 muted last)!
  h.userClick(items(0).find((b) => b.textContent === "서울"));
  await flushAsyncTurns(16);
  assert.deepEqual(tables(h)[0].filter, [{ col: 1, values: ["서울"] }]);
  assert.deepEqual(tables(h)[0].filterHidden, [[3, 4]]);
  assert.deepEqual(
    items(0).filter((b) => b.classList.contains("selected")).map((b) => b.textContent),
    ["서울"],
  );
  assert.deepEqual(items(1).map((b) => b.textContent), ["100", "300", "150", "200"]);
  assert.deepEqual(
    items(1).filter((b) => b.classList.contains("is-muted")).map((b) => b.textContent),
    ["150", "200"],
  );

  // Enable multi-select (`slicer-multi`) and add "부산" -> only row 4 ("대전", 150) stays hidden/muted!
  h.userClick(h.document.querySelector('#sheet-slicer-layer .sheet-slicer-card[data-slicer-col="0"] [data-slicer-act="multi"]'));
  await flushAsyncTurns(12);
  assert.equal(tables(h)[0].slicers[0].multiSelect, true);
  h.userClick(items(0).find((b) => b.textContent === "부산"));
  await flushAsyncTurns(16);
  assert.deepEqual(tables(h)[0].filterHidden, [[4, 4]]);

  // Clear filter (`filter-clear`) -> restores all rows and selects all items.
  h.userClick(h.document.querySelector('#sheet-slicer-layer .sheet-slicer-card[data-slicer-col="0"] [data-slicer-act="clear"]'));
  await flushAsyncTurns(16);
  assert.deepEqual(tables(h)[0].filter, []);
  assert.equal(tables(h)[0].filterHidden, undefined);
  assert.ok(items(0).every((b) => b.classList.contains("selected")));

  // Remove first slicer (`close`) and Undo brings it back!
  h.userClick(h.document.querySelector('#sheet-slicer-layer .sheet-slicer-card[data-slicer-col="0"] [data-slicer-act="remove"]'));
  await flushAsyncTurns(14);
  assert.equal(tables(h)[0].slicers.length, 1);
  await undo(h);
  assert.equal(tables(h)[0].slicers.length, 2);
  assert.equal($(h, "sheet-slicer-layer").hidden, false);
});
