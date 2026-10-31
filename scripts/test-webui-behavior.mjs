import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
test("focused cell undo then re-edit triggers recalculation and preserves undo history", async () => {
  const { document, sandbox } = await createWorkspaceHarness();
  document.querySelector('.mod-btn[data-module="SHEET"]').click();
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const f7 = document.querySelector('#pane-sheet td[data-cell="F7"]');
  assert.equal(b2.textContent, "1150");
  assert.equal(f2.textContent, "4950");

  b2.focus();
  b2.textContent = "2000";
  b2.dispatchEvent({ type: "input", target: b2 });

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.activeElement, b2);
  assert.equal(b2.textContent, "1150");

  b2.textContent = "3150";
  b2.dispatchEvent({ type: "input", target: b2 });
  b2.dispatchEvent({ type: "keydown", key: "Enter", target: b2 });
  await flushAsyncTurns();

  assert.equal(b2.textContent, "3150");
  assert.equal(f2.textContent, "6950");
  assert.equal(f7.textContent, "19890");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(b2.textContent, "1150");
  assert.equal(f2.textContent, "4950");
  assert.equal(f7.textContent, "17890");
});

test("clicking formula cell while async recalculation is in flight preserves data-formula on blur", async () => {
  const pendingEvaluations = [];
  let evaluateSheetCallCount = 0;

  const { document } = await createWorkspaceHarness({
    tauriInvoke: (command, args) => {
      if (command === "get_workspace_bootstrap") {
        return {
          platform: "Windows",
          shortcutModifier: "Ctrl",
          recentFormatFilters: ["전체", "XLSX"],
          openTabs: [{ id: "tab-sheet", title: "예산.xlsx", module: "SHEET", isPinned: false }],
        };
      }
      if (command === "get_ribbon_category_commands") {
        return { commands: [] };
      }
      if (command === "evaluate_sheet") {
        evaluateSheetCallCount += 1;
        return new Promise((resolve) => {
          pendingEvaluations.push({ payload: args.payload, resolve });
        });
      }
      return { status: "ok" };
    },
  });

  document.querySelector('.mod-btn[data-module="SHEET"]').click();
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const formulaInput = document.getElementById("formula-input");

  b2.focus();
  b2.textContent = "3150";
  b2.dispatchEvent({ type: "input", target: b2 });

  f2.focus();
  assert.equal(evaluateSheetCallCount, 1);
  assert.equal(pendingEvaluations.length, 1);
  assert.equal(f2.dataset.formula, "=SUM(B2:E2)");

  formulaInput.focus();
  formulaInput.value = "=MAX(B2:E2)";

  pendingEvaluations[0].resolve({
    query: "F2",
    value: "6950",
    values: { "Sheet1!F2": "6950", "Sheet1!F7": "19890" },
  });
  await flushAsyncTurns();

  assert.equal(f2.textContent, "6950");
  assert.equal(f2.dataset.formula, "=SUM(B2:E2)");
  assert.equal(formulaInput.value, "=MAX(B2:E2)");

  f2.focus();
  f2.blur();
  await flushAsyncTurns();
  assert.equal(f2.dataset.formula, "=SUM(B2:E2)");
  assert.equal(evaluateSheetCallCount, 1);
});

test("switching sheet tabs with unblurred edit and in-flight IPC avoids cross-sheet contamination", async () => {
  const pendingEvaluations = [];
  const { document } = await createWorkspaceHarness({
    tauriInvoke: (command, args) => {
      if (command === "get_workspace_bootstrap") {
        return {
          platform: "macOS",
          shortcutModifier: "⌘",
          recentFormatFilters: ["전체"],
          openTabs: [],
        };
      }
      if (command === "get_ribbon_category_commands") {
        return { commands: [] };
      }
      if (command === "evaluate_sheet") {
        return new Promise((resolve) => {
          pendingEvaluations.push({ payload: args.payload, resolve });
        });
      }
      return { status: "ok" };
    },
  });

  document.querySelector('.mod-btn[data-module="SHEET"]').click();
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "2150";
  b2.dispatchEvent({ type: "input", target: b2 });

  const sheet2Btn = document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]');
  sheet2Btn.click();
  await flushAsyncTurns();

  assert.equal(pendingEvaluations.length, 1);
  assert.match(pendingEvaluations[0].payload, /Sheet1!B2=2150/);
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A1"]').textContent, "100");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="F2"]').textContent, "");

  pendingEvaluations[0].resolve({
    query: "F2",
    value: "5950",
    values: {
      "Sheet1!F2": "5950",
      "Sheet2!C1": "350",
    },
  });
  await flushAsyncTurns();

  assert.equal(document.querySelector('#pane-sheet td[data-cell="F2"]').textContent, "");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="C1"]').textContent, "350");

  const sheet1Btn = document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]');
  sheet1Btn.click();
  await flushAsyncTurns();

  assert.equal(document.querySelector('#pane-sheet td[data-cell="B2"]').textContent, "2150");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="F2"]').textContent, "5950");
});

test("trailing backslashes, quoted cross-sheet refs, and unblurred save work cleanly offline", async () => {
  const { document, downloads } = await createWorkspaceHarness();
  document.querySelector('.mod-btn[data-module="SHEET"]').click();
  await flushAsyncTurns();

  const a9 = document.querySelector('#pane-sheet td[data-cell="A9"]');
  a9.focus();
  a9.textContent = "C:\\";
  a9.dispatchEvent({ type: "input", target: a9 });
  a9.blur();
  await flushAsyncTurns();

  const b9 = document.querySelector('#pane-sheet td[data-cell="B9"]');
  b9.focus();
  b9.textContent = "='Sheet2'!A1+50";
  b9.dispatchEvent({ type: "input", target: b9 });
  b9.blur();
  await flushAsyncTurns();
  assert.equal(b9.textContent, "150");

  const sheet2Btn = document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]');
  sheet2Btn.click();
  await flushAsyncTurns();

  const s2c3 = document.querySelector('#pane-sheet td[data-cell="C3"]');
  s2c3.focus();
  s2c3.textContent = "=B3+10";
  s2c3.dispatchEvent({ type: "input", target: s2c3 });
  s2c3.blur();
  await flushAsyncTurns();
  assert.equal(s2c3.textContent, "10");

  const s2a1 = document.querySelector('#pane-sheet td[data-cell="A1"]');
  s2a1.focus();
  s2a1.textContent = "400";
  s2a1.dispatchEvent({ type: "input", target: s2a1 });

  document.getElementById("qat-save").click();
  await flushAsyncTurns();
  assert.equal(downloads.length, 1);

  const sheet1Btn = document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]');
  sheet1Btn.click();
  await flushAsyncTurns();

  assert.equal(document.querySelector('#pane-sheet td[data-cell="A9"]').textContent, "C:\\");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B9"]').textContent, "450");
});

test("security audit blocks pipe-containing XXE doctype and PII scrub preserves inline table", async () => {
  const { document } = await createWorkspaceHarness();
  const docEditor = document.getElementById("doc-editor");

  docEditor.innerHTML =
    '<div class="doc-table-toolbar" contenteditable="false"><button id="btn-doc-add-row">+행</button></div>' +
    '<table id="doc-table"><tbody id="doc-table-body"><tr><td>기획팀</td><td>010-9876-5432</td><td data-doc-num="420">420</td></tr></tbody></table>' +
    '<p id="hwp-manuscript-preview" hidden>[원고지]</p>' +
    '<footer class="doc-page-footer">- 1 -</footer>';
  const bareText = document.createTextNode("담당자 직통 010-1234-5678");
  const divLine = document.createElement("div");
  divLine.textContent =
    '문의 메일 contact@example.org | A | B <!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>';
  const toolbar = docEditor.querySelector(".doc-table-toolbar");
  toolbar.before(bareText);
  toolbar.before(divLine);

  document.getElementById("btn-run-security-audit").click();
  await flushAsyncTurns();
  assert.match(document.getElementById("security-status-text").textContent, /REJECTED/);

  document.getElementById("btn-smart-diagnose").click();
  await flushAsyncTurns();
  const summaryText = document.getElementById("smart-summary-output").textContent;
  assert.match(summaryText, /010-1234-5678|contact@example\.org/);

  document.getElementById("btn-scrub-pii").click();
  await flushAsyncTurns();

  assert.match(document.getElementById("pii-result-text").textContent, /3건 마스킹 완료/);
  assert.ok(docEditor.querySelector("#doc-table"), "inline #doc-table must survive PII scrub");
  assert.ok(
    docEditor.querySelector(".doc-table-toolbar"),
    "inline toolbar must survive PII scrub"
  );
  assert.ok(!docEditor.textContent.includes("010-1234-5678"));
  assert.ok(!docEditor.textContent.includes("contact@example.org"));
  assert.ok(!docEditor.textContent.includes("010-9876-5432"));
  assert.ok(docEditor.textContent.includes("010-****-****"));
  assert.ok(docEditor.textContent.includes("***@***.***"));
});
