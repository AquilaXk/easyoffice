import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

test("offline RPC matches Rust sheet quoting, explicit sheet whitelist, error propagation, text refs, and expression evaluation", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const offlineRpc = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    getActiveSheetCellRef: () => "A1",
    ribbonCategoryTools: { DOC: { home: [] } },
    ribbonStyleLabels: { standard: [], hangul: [] },
    contextualToolBadges: {},
    moduleSpecs: {},
    localRibbonState: {},
  });

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
    "기획조정실",
    "direct single-cell text reference must preserve text value"
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "요약",
        sheets: ["요약", "R&D", "Sheet2"],
        expr: "=(Sheet2!A1 + 50) * 2 + SUM('R&D'!A1 + 10, 5)",
        cells: [
          { sheet: "Sheet2", cell: "A1", value: 100 },
          { sheet: "R&D", cell: "A1", value: 20 },
        ],
      })
    ).value,
    "335",
    "parenthesized arithmetic and scalar expressions inside SUM must evaluate cleanly"
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "Sheet1",
        expr: "=1+*",
        cells: [],
      })
    ).value,
    "#VALUE!",
    "malformed trailing operator must fail closed with #VALUE!"
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "Sheet1",
        expr: '="#1: " & A1 & " / 색상: " & B1',
        cells: [
          { sheet: "Sheet1", cell: "A1", value: 42 },
          { sheet: "Sheet1", cell: "B1", value: "#5C6BC0" },
        ],
      })
    ).value,
    "#1: 42 / 색상: #5C6BC0"
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "Sheet1",
        expr: '="값: " & A1',
        cells: [{ sheet: "Sheet1", cell: "A1", value: "=1/0" }],
      })
    ).value,
    "#DIV/0!"
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "Sheet1",
        sheets: ["Sheet1", "Sheet2"],
        expr: "=Sheet2!A1 + 1",
        cells: [{ sheet: "Sheet2", cell: "A1", value: "=1/0" }],
      })
    ).value,
    "#DIV/0!",
    "cross-sheet arithmetic must preserve #DIV/0! error code"
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "Sheet1",
        expr: "=SUM(A1:A2)",
        cells: [
          { sheet: "Sheet1", cell: "A1", value: 10 },
          { sheet: "Sheet1", cell: "A2", value: "=1/0" },
        ],
      })
    ).value,
    "#DIV/0!",
    "range aggregation must propagate #DIV/0! from referenced formula cell"
  );

  const plainErrorTextRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [
        { cell: "C1" },
        { cell: "C2" },
        { cell: "C3" },
        { cell: "C4" },
      ],
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
  assert.deepEqual(
    plainErrorTextRes.targetValues,
    ["#REF!", "Label: #REF!", "#VALUE!", "25"],
    "plain text '#REF!' must behave as text rather than a formula error"
  );

  const quoteIdentityRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Main",
      sheets: ["Main", "A'B", "'A'B'"],
      targets: [
        { cell: "A1" },
        { cell: "A2" },
      ],
      cells: [
        { sheet: "'A''B'", cell: "A1", value: 11 },
        { sheet: "'''A''B'''", cell: "A1", value: 77 },
        { sheet: "Main", cell: "A1", value: "='A''B'!A1" },
        { sheet: "Main", cell: "A2", value: "='''A''B'''!A1" },
      ],
    })
  );
  assert.deepEqual(
    quoteIdentityRes.targetValues,
    ["11", "77"],
    "single-layer unquoting must keep A'B distinct from 'A'B'"
  );

  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheets: [],
        expr: "=1+1",
        cells: [{ sheet: "Sheet1", cell: "A1", value: 10 }],
      })
    ).value,
    "#REF!",
    "explicit empty sheets array must reject evaluation with #REF!"
  );

  const explicitRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheets: ["예산2026"],
      targets: [
        { cell: "A2" },
        { sheet: "Sheet1", cell: "A1" },
        { cell: "A3" },
      ],
      cells: [
        { sheet: "예산2026", cell: "A1", value: 400 },
        { sheet: "예산2026", cell: "A2", value: "=A1+50" },
        { sheet: "예산2026", cell: "A3", value: "=Sheet2!A1+1" },
        { sheet: "Sheet2", cell: "A1", value: 999 },
      ],
    })
  );
  assert.deepEqual(
    explicitRes.targetValues,
    ["450", "#REF!", "#REF!"],
    "cells on unlisted sheets must not resurrect those sheets when sheets whitelist is explicit"
  );

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

test("failed ribbon async prep preserves backend redo stack without calling execute_ribbon_command, while undo-redo and PII scrub stay atomic", async () => {
  let failSheetEval = false;
  let failSmartStats = false;
  let executeRibbonCalls = 0;
  let undoRibbonCalls = 0;
  let redoRibbonCalls = 0;
  const backendUndoStack = [];
  const backendRedoStack = [];
  let backendIsBold = false;

  const { document, sandbox, userClick } = await createWorkspaceHarness({
    tauriInvoke: (command, args) => {
      if (command === "get_workspace_bootstrap") {
        return {
          platform: "Windows",
          shortcutModifier: "Ctrl",
          recentFormatFilters: ["전체"],
          openTabs: [],
        };
      }
      if (command === "get_ribbon_category_commands") {
        return {
          commands: [
            { id: "sheet-home-bold", title: "굵게", shortcut: "Ctrl+B" },
            { id: "sheet-data-autosum", title: "자동 합계 (SUM)", shortcut: "Alt+=" },
            { id: "sheet-review-stats", title: "문서 통계·글자 수", shortcut: "Ctrl+Shift+I" },
          ],
        };
      }
      if (command === "execute_ribbon_command") {
        executeRibbonCalls += 1;
        const cmdId = String(args.payload || "");
        const prevBold = backendIsBold;
        if (cmdId.endsWith("-home-bold")) {
          backendIsBold = !backendIsBold;
        }
        backendUndoStack.push({ cmdId, prevBold, nextBold: backendIsBold });
        backendRedoStack.length = 0;
        return { commandId: cmdId, summary: cmdId + " 실행됨", isBold: backendIsBold };
      }
      if (command === "undo_ribbon_command") {
        undoRibbonCalls += 1;
        const entry = backendUndoStack.pop();
        if (!entry) {
          return Promise.reject(new Error("backend undo stack empty"));
        }
        backendIsBold = entry.prevBold;
        backendRedoStack.push(entry);
        return { commandId: "undo", summary: "실행 취소 완료", isBold: backendIsBold };
      }
      if (command === "redo_ribbon_command") {
        redoRibbonCalls += 1;
        const entry = backendRedoStack.pop();
        if (!entry) {
          return Promise.reject(new Error("backend redo stack empty"));
        }
        backendIsBold = entry.nextBold;
        backendUndoStack.push(entry);
        return { commandId: "redo", summary: "다시 실행 완료", isBold: backendIsBold };
      }
      if (command === "run_smart_assistant") {
        if (failSmartStats) {
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
      }
      if (command === "scrub_pii") {
        const text = String(args.payload || "");
        if (text.includes("010-1234-5678")) {
          return {
            scrubbedText: text.replaceAll("010-1234-5678", "010-****-****"),
            totalMasked: 1,
          };
        }
        return { scrubbedText: text, totalMasked: 0 };
      }
      if (command === "evaluate_sheet") {
        if (failSheetEval) {
          return Promise.reject(new Error("sheet engine offline"));
        }
        const req = JSON.parse(args.payload);
        const targetValues = (req.targets || []).map((t) => {
          if (t.cell === "F2") {
            const f2Cell = (req.cells || []).find(
              (c) => c.sheet === "Sheet1" && c.cell === "F2"
            );
            if (f2Cell && f2Cell.value === "=B2+100") return "1250";
            if (f2Cell && f2Cell.value === "=B2+999") return "2149";
            return "4950";
          }
          if (t.cell === "A9") {
            const a9Cell = (req.cells || []).find(
              (c) => c.sheet === "Sheet1" && c.cell === "A9"
            );
            if (a9Cell && a9Cell.value.includes("010-****-****")) {
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
      }
      return { status: "ok" };
    },
  });

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();

  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const formulaInput = document.getElementById("formula-input");
  const feedback = document.getElementById("status-command-feedback");
  const boldBtn = document.querySelector('[data-command-id="sheet-home-bold"]');
  const autosumBtn = document.querySelector('[data-command-id="sheet-data-autosum"]');
  const statsBtn = document.querySelector('[data-command-id="sheet-review-stats"]');

  userClick(boldBtn);
  await flushAsyncTurns(10);
  assert.equal(f2.style.fontWeight, "600");
  assert.equal(executeRibbonCalls, 1);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.style.fontWeight, "");
  assert.equal(undoRibbonCalls, 1);
  assert.equal(backendRedoStack.length, 1, "undo must populate backend redo stack");

  failSheetEval = true;
  formulaInput.value = "=B2+999";
  userClick(autosumBtn);
  await flushAsyncTurns(10);

  assert.equal(f2.dataset.formula, "=SUM(B2:E2)", "failed autosum must not mutate cell formula");
  assert.equal(f2.textContent, "4950", "failed autosum must not mutate cell display");
  assert.equal(executeRibbonCalls, 1, "failed autosum prep must not invoke execute_ribbon_command");
  assert.equal(undoRibbonCalls, 1, "failed autosum prep must not need compensating undo");
  assert.equal(backendRedoStack.length, 1, "failed autosum must preserve backend redo stack");
  assert.match(feedback.textContent, /^\[요청 거부\].*sheet engine offline/);

  failSmartStats = true;
  userClick(statsBtn);
  await flushAsyncTurns(10);
  assert.equal(executeRibbonCalls, 1, "failed stats prep must not invoke execute_ribbon_command");
  assert.equal(backendRedoStack.length, 1, "failed stats prep must preserve backend redo stack");
  assert.match(feedback.textContent, /^\[요청 거부\].*smart assistant offline/);
  failSmartStats = false;

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(redoRibbonCalls, 1, "redo must succeed from preserved backend redo stack");
  assert.equal(f2.style.fontWeight, "600", "redo must restore bold formatting on F2");

  failSheetEval = false;
  formulaInput.value = "=B2+100";
  userClick(document.getElementById("btn-eval-formula"));
  await flushAsyncTurns(10);
  assert.equal(f2.dataset.formula, "=B2+100");
  assert.equal(f2.textContent, "1250");

  failSheetEval = true;
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.dataset.formula, "=B2+100", "failed undo must keep current formula");
  assert.equal(f2.textContent, "1250", "failed undo must keep current value");
  assert.match(feedback.textContent, /^\[요청 거부\].*sheet engine offline/);

  failSheetEval = false;
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.dataset.formula, "=SUM(B2:E2)");
  assert.equal(f2.textContent, "4950");

  failSheetEval = true;
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.dataset.formula, "=SUM(B2:E2)", "failed redo must keep undone formula");
  assert.equal(f2.textContent, "4950", "failed redo must keep undone value");
  assert.match(feedback.textContent, /^\[요청 거부\].*sheet engine offline/);

  failSheetEval = false;
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
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

  failSheetEval = true;
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(10);
  assert.equal(a8.textContent, "직통 010-1234-5678", "failed scrub recalc must not mutate plain cell");
  assert.equal(a9.dataset.formula, '="연락처 010-1234-5678"', "failed scrub recalc must not mutate formula cell");
  assert.equal(a9.textContent, "연락처 010-1234-5678");
  assert.match(feedback.textContent, /^\[요청 거부\].*sheet engine offline/);

  failSheetEval = false;
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(10);
  assert.equal(a8.textContent, "직통 010-****-****");
  assert.equal(a9.dataset.formula, '="연락처 010-****-****"');
  assert.equal(a9.textContent, "연락처 010-****-****");
});
