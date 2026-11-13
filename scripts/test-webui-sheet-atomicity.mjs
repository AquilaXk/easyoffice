import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { demoHtml, typeDemoSheet2 } from "./test-support/demo-fixtures.mjs";

// 용지 방향 전환 turns the page and 여백 전환 goes to the narrow set (PD-213); 편집 용지 opens its dialog and changes nothing by itself.
function assertTurnedNarrowPage(document, docEditor) {
  const core = document.defaultView.__EASYOFFICE_DOC_PAGE_SETUP_CORE__;
  const setup = core.parse(docEditor.dataset.eoPage);
  assert.ok(setup, "the page has a setup");
  assert.equal(setup.width > setup.height, true, "the page is turned");
  assert.equal(setup.top, 1134, "the narrow margin set");
  assert.equal(docEditor.style.padding, core.layoutFields(setup).padding);
}

function makeStandaloneOfflineRpc(sandbox) {
  return sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    getActiveSheetCellRef: () => "A1",
    ribbonCategoryTools: { DOC: { home: [] } },
    ribbonStyleLabels: { standard: [], hangul: [] },
    contextualToolBadges: {},
    moduleSpecs: {},
    localRibbonState: {},
  });
}

test("offline RPC preserves literal sheet tab identity, single-layer formula unquoting, explicit sheet whitelist, and security audit guards", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const offlineRpc = makeStandaloneOfflineRpc(sandbox);

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "요약",
        sheets: ["요약", "R&D", 'A"B'],
        expr: "='R&D'!A1 + 'A\"B'!A1",
        cells: [
          { sheet: "R&D", cell: "A1", value: 120 },
          { sheet: 'A"B', cell: "A1", value: 30 },
        ],
      })
    ).value,
    "150"
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "요약",
        sheets: ["요약", "R&D"],
        expr: "='R&D'!A1",
        cells: [{ sheet: "R&D", cell: "A1", value: "기획조정실" }],
      })
    ).value,
    "기획조정실"
  );

  const literalWhitespaceAndQuotes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: " Main ",
      sheets: [" Main ", "Main", " 요약 ", "요약", "A'B", "'A'B'", "'A''B'", "Summary", "'Summary'"],
      targets: [
        { sheet: " Main ", cell: "A1" },
        { sheet: " Main ", cell: "A2" },
        { sheet: " Main ", cell: "A3" },
        { sheet: " Main ", cell: "A4" },
        { sheet: " Main ", cell: "A5" },
        { sheet: " Main ", cell: "A6" },
        { sheet: " Main ", cell: "A7" },
      ],
      cells: [
        { sheet: " 요약 ", cell: "A1", value: 101 },
        { sheet: "요약", cell: "A1", value: 202 },
        { sheet: "A'B", cell: "A1", value: 11 },
        { sheet: "'A'B'", cell: "A1", value: 22 },
        { sheet: "'A''B'", cell: "A1", value: 33 },
        { sheet: "Summary", cell: "A1", value: 44 },
        { sheet: "'Summary'", cell: "A1", value: 55 },
        { sheet: "Main", cell: "A1", value: 999 },
        { sheet: " Main ", cell: "A1", value: "=' 요약 '!A1" },
        { sheet: " Main ", cell: "A2", value: "=요약!A1" },
        { sheet: " Main ", cell: "A3", value: "='A''B'!A1" },
        { sheet: " Main ", cell: "A4", value: "='''A''B'''!A1" },
        { sheet: " Main ", cell: "A5", value: "='''A''''B'''!A1" },
        { sheet: " Main ", cell: "A6", value: "='Summary'!A1" },
        { sheet: " Main ", cell: "A7", value: "='''Summary'''!A1" },
      ],
    })
  );
  assert.deepEqual(
    literalWhitespaceAndQuotes.targetValues,
    ["101", "202", "11", "22", "33", "44", "55"]
  );

  assert.throws(
    () =>
      offlineRpc.evaluate_sheet(
        JSON.stringify({
          sheets: ["   "],
          expr: "=1+1",
          cells: [],
        })
      ),
    /InvalidArgument/
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheets: [],
        expr: "=1+1",
        cells: [{ sheet: "Sheet1", cell: "A1", value: 10 }],
      })
    ).value,
    "#REF!"
  );

  const plainErrorTextRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "C1" }, { cell: "C2" }, { cell: "C3" }, { cell: "C4" }],
      cells: [
        { sheet: "Sheet1", cell: "A1", value: "#REF!" },
        { sheet: "Sheet1", cell: "B1", value: 25 },
        { sheet: "Sheet1", cell: "C1", value: "=A1" },
        { sheet: "Sheet1", cell: "C2", value: '="Label: " & A1' },
        { sheet: "Sheet1", cell: "C3", value: "=A1 + 1" },
        { sheet: "Sheet1", cell: "C4", value: "=SUM(A1:B1)" },
      ],
    })
  );
  assert.deepEqual(plainErrorTextRes.targetValues, ["#REF!", "Label: #REF!", "#VALUE!", "25"]);

  const explicitRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheets: ["예산2026"],
      targets: [{ cell: "A2" }, { sheet: "Sheet1", cell: "A1" }, { cell: "A3" }],
      cells: [
        { sheet: "예산2026", cell: "A1", value: 400 },
        { sheet: "예산2026", cell: "A2", value: "=A1+50" },
        { sheet: "예산2026", cell: "A3", value: "=Sheet2!A1+1" },
        { sheet: "Sheet2", cell: "A1", value: 999 },
      ],
    })
  );
  assert.deepEqual(explicitRes.targetValues, ["450", "#REF!", "#REF!"]);

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "ß",
        sheets: ["ß", "SS"],
        expr: "=A1+SS!A1",
        cells: [
          { sheet: "ß", cell: "A1", value: 7 },
          { sheet: "SS", cell: "A1", value: 100 },
        ],
      })
    ).value,
    "107"
  );

  assert.throws(
    () =>
      offlineRpc.audit_security(
        JSON.stringify({
          entries: [{ name: "a.xml", compressed: null, uncompressed: 100, encrypted: false }],
          xml: "<doc/>",
        })
      ),
    /InvalidArgument/
  );
  assert.throws(
    () =>
      offlineRpc.audit_security(
        JSON.stringify({
          entries: [{ name: "a.xml", compressed: 100, uncompressed: 1e25, encrypted: false }],
          xml: "<doc/>",
        })
      ),
    /InvalidArgument/
  );
});

test("offline AST parser and evaluator enforce lazy IF/IFERROR, strict A1 bounds, parser budgets, MAX_EXPANDED_CELL_COUNT, loop MIN/MAX, and OpenFormula/range semantics", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const offlineRpc = makeStandaloneOfflineRpc(sandbox);
  const astApi = sandbox.__EASYOFFICE_FORMULA_AST__;

  assert.equal(astApi.MAX_FORMULA_LEN, 8192);
  assert.equal(astApi.MAX_TOKENS, 2048);
  assert.equal(astApi.MAX_PARSE_DEPTH, 64);
  assert.equal(astApi.MAX_EXPANDED_CELL_COUNT, 100000);

  const lazyIfTrueRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "A1" }, { cell: "B1" }],
      cells: [
        { sheet: "Sheet1", cell: "A1", value: "=IF(TRUE, 10, B1)" },
        { sheet: "Sheet1", cell: "B1", value: "=A1" },
      ],
    })
  );
  assert.deepEqual(lazyIfTrueRes.targetValues, ["10", "10"]);

  const lazyIfFalseRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "A1" }, { cell: "B1" }],
      cells: [
        { sheet: "Sheet1", cell: "A1", value: "=IF(FALSE, B1, 25)" },
        { sheet: "Sheet1", cell: "B1", value: "=A1" },
      ],
    })
  );
  assert.deepEqual(lazyIfFalseRes.targetValues, ["25", "25"]);

  const lazyIfErrorRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "A1" }, { cell: "B1" }, { cell: "C1" }],
      cells: [
        { sheet: "Sheet1", cell: "A1", value: "=IFERROR(50, B1)" },
        { sheet: "Sheet1", cell: "B1", value: "=A1" },
        { sheet: "Sheet1", cell: "C1", value: "=IFERROR(1/0, 99)" },
      ],
    })
  );
  assert.deepEqual(lazyIfErrorRes.targetValues, ["50", "50", "99"]);

  const overLenFormula = "=" + "1+".repeat(4100) + "1";
  assert.equal(
    offlineRpc.evaluate_sheet(JSON.stringify({ sheet: "Sheet1", expr: overLenFormula })).value,
    "#VALUE!"
  );

  const overTokensFormula = "=" + "1+".repeat(1030) + "1";
  assert.equal(
    offlineRpc.evaluate_sheet(JSON.stringify({ sheet: "Sheet1", expr: overTokensFormula })).value,
    "#VALUE!"
  );

  const overDepthFormula = "=" + "(".repeat(66) + "1" + ")".repeat(66);
  assert.equal(
    offlineRpc.evaluate_sheet(JSON.stringify({ sheet: "Sheet1", expr: overDepthFormula })).value,
    "#VALUE!"
  );

  const strictCoordAndBudgetRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [
        { cell: "=A$1$" },
        { cell: "=A0" },
        { cell: "=XFE1" },
        { cell: "=A1000001" },
        { cell: "=$XFD$1000000" },
        { cell: "=SUM(A1:Z1000000)" },
        { cell: "=ROW(MissingSheet!A5)" },
      ],
      cells: [{ sheet: "Sheet1", cell: "XFD1000000", value: 777 }],
    })
  );
  assert.equal(strictCoordAndBudgetRes.targetValues[0], "#NAME?");
  assert.equal(strictCoordAndBudgetRes.targetValues[1], "#REF!");
  assert.equal(strictCoordAndBudgetRes.targetValues[2], "#REF!");
  assert.equal(strictCoordAndBudgetRes.targetValues[3], "#REF!");
  assert.equal(strictCoordAndBudgetRes.targetValues[4], "777");
  // Wide in-bounds ranges are valid references; aggregates stream populated cells only.
  assert.equal(strictCoordAndBudgetRes.targetValues[5], "0");
  assert.equal(strictCoordAndBudgetRes.targetValues[6], "#REF!");

  const syntaxAndRangeFnsRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [
        { cell: "=SUM(10; 20; .5)" },
        { cell: "=.5 + .25" },
        { cell: "of:=SUM([.A1:.A2]; .5)" },
        { cell: "=AND(B1:B2)" },
        { cell: "=OR(B1:B3)" },
        { cell: "=ROW(C5:D8)" },
        { cell: "=COLUMN(C5:D8)" },
        { cell: "=ROWS(C5:D8)" },
        { cell: "=COLUMNS(C5:D8)" },
      ],
      cells: [
        { sheet: "Sheet1", cell: "A1", value: 12 },
        { sheet: "Sheet1", cell: "A2", value: 18 },
        { sheet: "Sheet1", cell: "B1", value: true },
        { sheet: "Sheet1", cell: "B2", value: 1 },
        { sheet: "Sheet1", cell: "B3", value: false },
      ],
    })
  );
  assert.deepEqual(syntaxAndRangeFnsRes.targetValues, [
    "30.5",
    "0.75",
    "30.5",
    "TRUE",
    "TRUE",
    "5",
    "3",
    "4",
    "2",
  ]);

  const largeCells = [];
  for (let r = 1; r <= 70000; r += 1) {
    largeCells.push({ sheet: "Sheet1", cell: "A" + r, value: r });
  }
  const largeMinMaxRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "=MIN(A1:A70000)" }, { cell: "=MAX(A1:A70000)" }],
      cells: largeCells,
    })
  );
  assert.deepEqual(largeMinMaxRes.targetValues, ["1", "70000"]);
});

function createRibbonBackendMock() {
  const mock = {
    failSheetEval: false,
    failSmartStats: false,
    failSecurityAudit: false,
    executeRibbonCalls: 0,
    undoRibbonCalls: 0,
    redoRibbonCalls: 0,
    backendUndoStack: [],
    backendRedoStack: [],
    backendIsBold: false,
    deferredReviewCategoryResolve: null,
  };
  const handlers = {
    get_workspace_bootstrap: (args) => {
      return {
        platform: "Windows",
        shortcutModifier: "Ctrl",
        recentFormatFilters: ["전체"],
        openTabs: [],
      };
    },
    get_ribbon_category_commands: (args) => {
      const payload = String(args.payload || "");
      const isInsertCategory =
        payload.endsWith("|insert") ||
        (payload.startsWith("{") && JSON.parse(payload).category === "insert");
      if (isInsertCategory) {
        return new Promise((resolve) => {
          mock.deferredReviewCategoryResolve = () =>
            resolve({
              commands: [{ id: "sheet-insert-table", title: "지연된 표 삽입", shortcut: "Ctrl+T" }],
            });
        });
      }
      return {
        commands: [
          { id: "sheet-home-bold", title: "굵게", shortcut: "Ctrl+B" },
          { id: "sheet-data-autosum", title: "자동 합계 (SUM)", shortcut: "Alt+=" },
          { id: "sheet-data-validation", title: "데이터 유효성 검사", shortcut: "Alt+V" },
          { id: "sheet-review-verify", title: "문서 구조 보안 검사", shortcut: "Ctrl+Shift+V" },
          { id: "sheet-review-stats", title: "문서 통계·글자 수", shortcut: "Ctrl+Shift+I" },
        ],
      };
    },
    audit_security: (args) => {
      if (mock.failSecurityAudit) {
        return Promise.reject(new Error("SecurityViolation: 허용되지 않은 엔티티"));
      }
      return { status: "SAFE" };
    },
    execute_ribbon_command: (args) => {
      mock.executeRibbonCalls += 1;
      const cmdId = String(JSON.parse(args.payload).commandId || "");
      const prevBold = mock.backendIsBold;
      if (cmdId.endsWith("-home-bold")) {
        mock.backendIsBold = !mock.backendIsBold;
      }
      mock.backendUndoStack.push({ cmdId, prevBold, nextBold: mock.backendIsBold });
      mock.backendRedoStack.length = 0;
      return { commandId: cmdId, summary: cmdId + " 실행됨", isBold: mock.backendIsBold };
    },
    undo_ribbon_command: (args) => {
      mock.undoRibbonCalls += 1;
      const entry = mock.backendUndoStack.pop();
      if (!entry) {
        return Promise.reject(new Error("backend undo stack empty"));
      }
      mock.backendIsBold = entry.prevBold;
      mock.backendRedoStack.push(entry);
      return { commandId: "undo", summary: "실행 취소 완료", isBold: mock.backendIsBold };
    },
    redo_ribbon_command: (args) => {
      mock.redoRibbonCalls += 1;
      const entry = mock.backendRedoStack.pop();
      if (!entry) {
        return Promise.reject(new Error("backend redo stack empty"));
      }
      mock.backendIsBold = entry.nextBold;
      mock.backendUndoStack.push(entry);
      return { commandId: "redo", summary: "다시 실행 완료", isBold: mock.backendIsBold };
    },
    run_smart_assistant: (args) => {
      if (mock.failSmartStats) {
        return Promise.reject(new Error("smart assistant offline"));
      }
      return {
        charCount: 10,
        wordCount: 2,
        sentenceCount: 1,
        readingTimeSeconds: 1,
        summaryCount: 1,
        actionItemCount: 0,
        suggestionCount: 0,
        summaryLines: ["요약"],
        actionItems: [],
      };
    },
    scrub_pii: (args) => {
      const text = String(args.payload || "");
      if (text.includes("010-1234-5678")) {
        return {
          scrubbedText: text.replaceAll("010-1234-5678", "010-****-****"),
          totalMasked: 1,
        };
      }
      return { scrubbedText: text, totalMasked: 0 };
    },
    evaluate_sheet: (args) => {
      if (mock.failSheetEval) {
        return Promise.reject(new Error("sheet engine offline"));
      }
      const req = JSON.parse(args.payload);
      const targetValues = (req.targets || []).map((t) => {
        if (t.cell === "F2") {
          const f2Cell = (req.cells || []).find(
            (c) => c.sheet === "Sheet1" && c.cell === "F2"
          );
          if (f2Cell?.value === "=B2+100") return "1250";
          if (f2Cell?.value === "=B2+999") return "2149";
          return "4950";
        }
        if (t.cell === "A9") {
          const a9Cell = (req.cells || []).find(
            (c) => c.sheet === "Sheet1" && c.cell === "A9"
          );
          if (a9Cell?.value.includes("010-****-****")) {
            return "연락처 010-****-****";
          }
          return "연락처 010-1234-5678";
        }
        return "17890";
      });
      return {
        query: req.targets?.[0]?.cell || "F2",
        value: targetValues[0] || "4950",
        values: {},
        targetValues,
      };
    },
  };
  mock.invoke = (command, args) => {
    const handler = handlers[command];
    return handler ? handler(args) : { status: "ok" };
  };
  return mock;
}

test("failed ribbon async prep (autosum, smart stats, security audit) preserves backend redo stack and renderRibbonCategoryTools discards stale out-of-order responses", async () => {
  const mock = createRibbonBackendMock();

  const { document, sandbox, userClick } = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: (command, args) => mock.invoke(command, args),
  });

  switchModule(document, "SHEET");
  await flushAsyncTurns();

  assert.ok(commandItem(document, "sheet-home-bold"));
  if (mock.deferredReviewCategoryResolve) {
    mock.deferredReviewCategoryResolve();
    await flushAsyncTurns(6);
  }
  assert.ok(commandItem(document, "sheet-home-bold"));

  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const formulaInput = document.getElementById("formula-input");
  f2.focus();
  const feedback = document.getElementById("status-command-feedback");
  const secStatus = document.getElementById("security-status-text");
  const boldBtn = commandItem(document, "sheet-home-bold");
  const autosumBtn = commandItem(document, "sheet-data-autosum");
  const verifyBtn = commandItem(document, "sheet-review-verify");
  const statsBtn = commandItem(document, "sheet-review-stats");

  userClick(boldBtn);
  await flushAsyncTurns(10);
  assert.equal(f2.style.fontWeight, "600");
  assert.equal(mock.executeRibbonCalls, 1);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.style.fontWeight, "");
  assert.equal(mock.undoRibbonCalls, 1);
  assert.equal(mock.backendRedoStack.length, 1);

  mock.failSheetEval = true;
  formulaInput.value = "=B2+999";
  userClick(autosumBtn);
  await flushAsyncTurns(10);

  assert.equal(f2.dataset.formula, "=SUM(B2:E2)");
  assert.equal(f2.textContent, "4950");
  assert.equal(mock.executeRibbonCalls, 1);
  assert.equal(mock.undoRibbonCalls, 1);
  assert.equal(mock.backendRedoStack.length, 1);
  assert.match(feedback.textContent, /^\[요청 거부\].*sheet engine offline/);

  mock.failSmartStats = true;
  userClick(statsBtn);
  await flushAsyncTurns(10);
  assert.equal(mock.executeRibbonCalls, 1);
  assert.equal(mock.backendRedoStack.length, 1);
  assert.match(feedback.textContent, /^\[요청 거부\].*smart assistant offline/);
  mock.failSmartStats = false;

  mock.failSecurityAudit = true;
  userClick(verifyBtn);
  await flushAsyncTurns(10);
  assert.equal(mock.executeRibbonCalls, 1);
  assert.equal(mock.backendRedoStack.length, 1);
  assert.match(feedback.textContent, /^\[요청 거부\].*SecurityViolation/);
  assert.match(secStatus.textContent, /^\[요청 거부\].*SecurityViolation/);

  mock.failSecurityAudit = false;

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(mock.redoRibbonCalls, 1);
  assert.equal(f2.style.fontWeight, "600");

  mock.failSheetEval = false;
  formulaInput.value = "=B2+100";
  userClick(document.getElementById("btn-eval-formula"));
  await flushAsyncTurns(10);
  assert.equal(f2.dataset.formula, "=B2+100");
  assert.equal(f2.textContent, "1250");

  const a8 = document.querySelector('#pane-sheet td[data-cell="A8"]');
  const a9 = document.querySelector('#pane-sheet td[data-cell="A9"]');
  a8.focus();
  a8.textContent = "직통 010-1234-5678";
  a8.dispatchEvent({ type: "input", target: a8 });
  a8.blur();
  await flushAsyncTurns();

  a9.focus();
  a9.textContent = '="연락처 010-1234-5678"';
  a9.dispatchEvent({ type: "input", target: a9 });
  a9.blur();
  await flushAsyncTurns();
  assert.equal(a9.textContent, "연락처 010-1234-5678");

  mock.failSheetEval = true;
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(10);
  assert.equal(a8.textContent, "직통 010-1234-5678");
  assert.equal(a9.dataset.formula, '="연락처 010-1234-5678"');
  assert.match(feedback.textContent, /^\[요청 거부\].*sheet engine offline/);

  mock.failSheetEval = false;
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(10);
  assert.equal(a8.textContent, "직통 010-****-****");
  assert.equal(a9.dataset.formula, '="연락처 010-****-****"');
  assert.equal(a9.textContent, "연락처 010-****-****");
});

test("workspace snapshot SSOT restores per-sheet row counts, doc layout styles, slide title styles, and canvas-stage view state on Undo and Redo", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness({ transformHtml: demoHtml });

  const docEditor = document.getElementById("doc-editor");
  await flushAsyncTurns();
  switchModule(document, "SHEET");
  await flushAsyncTurns();
  await typeDemoSheet2(document, userClick, flushAsyncTurns);
  switchModule(document, "DOC");
  await flushAsyncTurns();

  userClick(commandItem(document, "document-layout-columns"));
  await flushAsyncTurns();
  userClick(commandItem(document, "document-layout-orientation"));
  await flushAsyncTurns();
  userClick(commandItem(document, "document-layout-margins"));
  await flushAsyncTurns();
  userClick(commandItem(document, "document-layout-paper"));
  await flushAsyncTurns();

  assert.equal(docEditor.style.columnCount, "2");
  assertTurnedNarrowPage(document, docEditor);

  for (let i = 0; i < 4; i += 1) {
    sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
    await flushAsyncTurns();
  }
  assert.equal(docEditor.style.columnCount, "");
  assert.equal(docEditor.style.maxWidth, "");
  assert.equal(docEditor.style.padding, "");
  assert.equal(docEditor.dataset.eoPage, undefined);

  for (let i = 0; i < 4; i += 1) {
    sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
    await flushAsyncTurns();
  }
  assert.equal(docEditor.style.columnCount, "2");
  assertTurnedNarrowPage(document, docEditor);

  switchModule(document, "SHEET");
  await flushAsyncTurns();
  await flushAsyncTurns();

  assert.equal(document.querySelectorAll("#pane-sheet tbody tr").length, 12);
  userClick(commandItem(document, "sheet-insert-table"));
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#pane-sheet tbody tr").length, 13);

  userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]'));
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#pane-sheet tbody tr").length, 12);

  userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#pane-sheet tbody tr").length, 13);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#pane-sheet tbody tr").length, 12);
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A13"]'), null);

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#pane-sheet tbody tr").length, 13);
  assert.notEqual(document.querySelector('#pane-sheet td[data-cell="A13"]'), null);

  switchModule(document, "SLIDE");
  await flushAsyncTurns();
  await flushAsyncTurns();

  const titleZone = document.getElementById("slide-title-zone");
  userClick(commandItem(document, "slide-home-bold"));
  await flushAsyncTurns();
  userClick(commandItem(document, "slide-home-italic"));
  await flushAsyncTurns();
  userClick(commandItem(document, "slide-home-underline"));
  await flushAsyncTurns();
  userClick(commandItem(document, "slide-home-style"));
  await flushAsyncTurns();

  assert.equal(titleZone.style.fontWeight, "700");
  assert.equal(titleZone.style.fontStyle, "italic");
  assert.equal(titleZone.style.textDecoration, "underline");
  assert.equal(titleZone.style.fontSize, "22px");

  for (let i = 0; i < 4; i += 1) {
    sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
    await flushAsyncTurns();
  }
  assert.equal(titleZone.style.fontWeight, "");
  assert.equal(titleZone.style.fontStyle, "");
  assert.equal(titleZone.style.textDecoration, "");
  assert.equal(titleZone.style.fontSize, "");

  for (let i = 0; i < 4; i += 1) {
    sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
    await flushAsyncTurns();
  }
  assert.equal(titleZone.style.fontWeight, "700");
  assert.equal(titleZone.style.fontStyle, "italic");
  assert.equal(titleZone.style.textDecoration, "underline");
  assert.equal(titleZone.style.fontSize, "22px");

  await flushAsyncTurns();
  const stage = document.querySelector(".canvas-stage");

  userClick(document.getElementById("btn-zoom-in"));
  await flushAsyncTurns();
  userClick(commandItem(document, "slide-view-split"));
  await flushAsyncTurns();

  assert.equal(stage.style.zoom, "125%");
  assert.equal(stage.dataset.splitView, "true");

  for (let i = 0; i < 2; i += 1) {
    sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
    await flushAsyncTurns();
  }
  assert.equal(stage.style.zoom, "");
  assert.equal(stage.dataset.splitView, undefined);

  for (let i = 0; i < 2; i += 1) {
    sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
    await flushAsyncTurns();
  }
  assert.equal(stage.style.zoom, "125%");
  assert.equal(stage.dataset.splitView, "true");
});

test("JSON-only real-backend IPC syncs state, toggles per-cell inline styles independently, and awaits async cell flush before switchSheetTab and activatePane", async () => {
  const rustState = {
    activeModule: "DOC",
    ribbonStyle: "standard",
    isBold: false,
    isItalic: false,
    isUnderline: false,
    columnCount: 1,
    isLandscape: false,
    marginPresetIndex: 0,
    paperSize: "A4",
    zoomPercent: 100,
    ribbonCollapsed: false,
    splitViewEnabled: false,
    executedCount: 0,
  };
  const rustUndo = [];
  const rustRedo = [];
  let slowEvalGate = null;
  const evalLog = [];

  const { document, userClick } = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") {
        return JSON.stringify({
          platform: "macOS",
          shortcutModifier: "⌘",
          recentFormatFilters: ["전체", "XLSX"],
          openTabs: [],
        });
      }
      if (command === "get_ribbon_category_commands") {
        return JSON.stringify({
          commands: [
            { id: "sheet-home-bold", title: "굵게", shortcut: "⌘B" },
            { id: "sheet-view-zoom", title: "100% 실제 배율", shortcut: "⌘0" },
          ],
        });
      }
      if (command === "execute_ribbon_command") {
        const cmdId = String(JSON.parse(args.payload).commandId || "");
        rustUndo.push(structuredClone(rustState));
        rustRedo.length = 0;
        rustState.executedCount += 1;
        if (cmdId.endsWith("-home-bold")) {
          rustState.isBold = !rustState.isBold;
        } else if (cmdId.endsWith("-view-zoom")) {
          rustState.zoomPercent = 100;
        }
        return JSON.stringify({
          commandId: cmdId,
          summary: "명령 실행",
          ...rustState,
          canUndo: true,
          canRedo: false,
        });
      }
      if (command === "undo_ribbon_command") {
        const prev = rustUndo.pop();
        if (prev) {
          rustRedo.push(structuredClone(rustState));
          Object.assign(rustState, prev);
        }
        return JSON.stringify({
          commandId: "undo",
          summary: "실행 취소 완료",
          ...rustState,
          canUndo: rustUndo.length > 0,
          canRedo: rustRedo.length > 0,
        });
      }
      if (command === "evaluate_sheet") {
        const req = JSON.parse(args.payload);
        evalLog.push(req);
        if (slowEvalGate) {
          await slowEvalGate.promise;
        }
        const targetValues = (req.targets || []).map((t) => {
          if (t.sheet === "Sheet1" && t.cell === "F2") {
            const b2 = (req.cells || []).find((c) => c.sheet === "Sheet1" && c.cell === "B2");
            if (b2?.value === "2000") return "5800";
          }
          if (t.sheet === "Sheet1" && t.cell === "D2") {
            return "777";
          }
          return "4950";
        });
        return JSON.stringify({
          query: req.targets?.[0]?.cell || "F2",
          value: targetValues[0] || "4950",
          values: {},
          targetValues,
        });
      }
      return JSON.stringify({ status: "ok" });
    },
  });

  switchModule(document, "SHEET");
  await flushAsyncTurns();

  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const a1 = document.querySelector('#pane-sheet td[data-cell="A1"]');
  const boldBtn = commandItem(document, "sheet-home-bold");
  const zoomBtn = commandItem(document, "sheet-view-zoom");

  userClick(f2);
  userClick(boldBtn);
  await flushAsyncTurns();
  assert.equal(f2.style.fontWeight, "600");
  assert.equal(rustState.isBold, true);

  userClick(a1);
  userClick(boldBtn);
  await flushAsyncTurns();
  assert.equal(rustState.isBold, false);
  assert.equal(f2.style.fontWeight, "600");
  assert.equal(a1.style.fontWeight, "600");

  userClick(zoomBtn);
  await flushAsyncTurns();
  assert.equal(document.querySelector(".canvas-stage").style.zoom, "100%");

  let releaseSlowEval = null;
  slowEvalGate = {
    promise: new Promise((resolve) => {
      releaseSlowEval = resolve;
    }),
  };

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "2000";
  b2.dispatchEvent({ type: "input", target: b2 });

  const addBtn = document.getElementById("btn-add-sheet-tab");
  const paddedTab = document.createElement("button");
  paddedTab.className = "sheet-tab-btn";
  paddedTab.dataset.sheetTab = " 요약 ";
  paddedTab.textContent = " 요약 ";
  addBtn.before(paddedTab);

  paddedTab.click();
  await flushAsyncTurns(4);
  assert.equal(
    document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]').classList.contains("active"),
    true
  );

  slowEvalGate = null;
  releaseSlowEval();
  await flushAsyncTurns(12);

  assert.equal(paddedTab.classList.contains("active"), true);
  assert.equal(
    document.getElementById("status-command-feedback").textContent,
    "시트 탭 전환:  요약 "
  );

  userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns(8);
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B2"]').textContent, "2000");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="F2"]').textContent, "5800");

  let releasePaneEval = null;
  slowEvalGate = {
    promise: new Promise((resolve) => {
      releasePaneEval = resolve;
    }),
  };
  const d2 = document.querySelector('#pane-sheet td[data-cell="D2"]');
  d2.focus();
  d2.textContent = "=500+277";
  d2.dispatchEvent({ type: "input", target: d2 });

  switchModule(document, "DOC");
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("pane-sheet").hidden, false);

  slowEvalGate = null;
  releasePaneEval();
  await flushAsyncTurns(12);
  assert.equal(document.getElementById("pane-doc").hidden, false);

  switchModule(document, "SHEET");
  await flushAsyncTurns(8);
  assert.equal(document.querySelector('#pane-sheet td[data-cell="D2"]').textContent, "777");
  assert.ok(evalLog.length >= 2);
});

test("issue #107: offline evaluator mirrors #NUM! circular flag, no demo seed, and sparse wide-range aggregates", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const offlineRpc = makeStandaloneOfflineRpc(sandbox);

  const cyc = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "A1" }, { cell: "C1" }],
      cells: [
        { cell: "A1", value: "=B1+1" },
        { cell: "B1", value: "=A1+1" },
        { cell: "C1", value: "=5" },
      ],
    })
  );
  assert.deepEqual(cyc.targetValues, ["#NUM!", "5"]);
  assert.equal(cyc.circular, true);
  assert.ok(!JSON.stringify(cyc).includes("#CYCLE!"));

  const noCycle = offlineRpc.evaluate_sheet(
    JSON.stringify({ sheet: "Sheet1", targets: [{ cell: "=1+1" }], cells: [] })
  );
  assert.equal(noCycle.circular, false);

  for (const payload of [
    { sheet: "Data", expr: "=Sheet2!A1", cells: [{ sheet: "Data", cell: "A1", value: 5 }] },
    { sheet: "Data", sheets: ["Data"], expr: "=Sheet2!A1" },
    { sheet: "Data", expr: "=Sheet1!A1+Sheet1!B1" },
  ]) {
    assert.equal(offlineRpc.evaluate_workbook_formula(JSON.stringify(payload)).value, "#REF!");
  }

  const wide = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [
        { cell: "=SUM(A1:Z10000)" },
        { cell: "=COUNT(A1:Z10000)" },
        { cell: "=COUNTA(A1:Z10000)" },
        { cell: "=MAX(A1:Z10000)" },
        { cell: "=COUNTBLANK(A1:Z10000)" },
      ],
      cells: [
        { cell: "A1", value: 10 },
        { cell: "Z10000", value: 20 },
        { cell: "M5000", value: "가나다" },
        { cell: "C3", value: "=A1*3" },
      ],
    })
  );
  assert.deepEqual(wide.targetValues, ["60", "3", "4", "30", String(260000 - 4)]);
});

test("issue #107: circular reference shows #NUM! in the grid and a status-bar warning", async () => {
  const { document } = await createWorkspaceHarness({ transformHtml: demoHtml });
  switchModule(document, "SHEET");
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  b2.focus();
  b2.textContent = "=F2";
  b2.dispatchEvent({ type: "input", target: b2 });
  b2.dispatchEvent({ type: "keydown", key: "Enter", target: b2 });
  await flushAsyncTurns(10);

  assert.equal(f2.textContent, "#NUM!");
  assert.match(document.getElementById("status-command-feedback").textContent, /순환 참조/);
});
