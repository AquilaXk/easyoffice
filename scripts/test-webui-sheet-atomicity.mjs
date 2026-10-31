import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

test("offline RPC handles quoted sheet tokens, non-error # strings, transitive quoting, explicit sheets, and strict audit_security numbers", async () => {
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
        sheet: "'특수시트'",
        expr: "=SUM(A1:A3)",
        cells: [
          { sheet: "'특수시트'", cell: "A1", value: 10 },
          { sheet: "특수시트", cell: "A2", value: 20 },
          { sheet: "'''특수시트'''", cell: "A3", value: 30 },
        ],
      })
    ).value,
    "60"
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

  const explicitRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheets: ["예산2026"],
      targets: [
        { cell: "A2" },
        { sheet: "Sheet1", cell: "A1" },
      ],
      cells: [
        { sheet: "예산2026", cell: "A1", value: 400 },
        { sheet: "예산2026", cell: "A2", value: "=A1+50" },
      ],
    })
  );
  assert.deepEqual(explicitRes.targetValues, ["450", "#REF!"]);

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

test("autosum ribbon, sheet undo-redo, and sheet PII scrub remain atomic when evaluate_sheet IPC fails", async () => {
  let failSheetEval = false;
  let undoRibbonCalls = 0;

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
          commands: [{ id: "sheet-data-autosum", title: "자동 합계 (SUM)", shortcut: "Alt+=" }],
        };
      }
      if (command === "execute_ribbon_command") {
        return { commandId: args.payload, summary: "자동 합계 실행됨" };
      }
      if (command === "undo_ribbon_command") {
        undoRibbonCalls += 1;
        return { commandId: "undo", summary: "실행 취소 완료" };
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
  const autosumBtn = document.querySelector('[data-command-id="sheet-data-autosum"]');

  failSheetEval = true;
  formulaInput.value = "=B2+999";
  userClick(autosumBtn);
  await flushAsyncTurns(10);

  assert.equal(f2.dataset.formula, "=SUM(B2:E2)", "failed autosum must not mutate cell formula");
  assert.equal(f2.textContent, "4950", "failed autosum must not mutate cell display");
  assert.equal(undoRibbonCalls, 1, "failed autosum must compensate backend ribbon stack");
  assert.match(feedback.textContent, /^\[요청 거부\].*sheet engine offline/);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(feedback.textContent, "실행 취소할 작업이 없습니다");

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
