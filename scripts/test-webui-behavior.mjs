import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

test("focused cell edit supports undo, redo formula recalculation, and subsequent re-edit", async () => {
  const { document, sandbox } = await createWorkspaceHarness();
  document.querySelector('.mod-btn[data-module="SHEET"]').click();
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const f7 = document.querySelector('#pane-sheet td[data-cell="F7"]');
  assert.equal(b2.textContent, "1150");
  assert.equal(f2.textContent, "4950");
  assert.equal(f7.textContent, "17890");

  b2.focus();
  b2.textContent = "2000";
  b2.dispatchEvent({ type: "input", target: b2 });
  b2.dispatchEvent({ type: "keydown", key: "Enter", target: b2 });
  await flushAsyncTurns();
  assert.equal(b2.textContent, "2000");
  assert.equal(f2.textContent, "5800");
  assert.equal(f7.textContent, "18740");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(b2.textContent, "1150");
  assert.equal(f2.textContent, "4950");
  assert.equal(f7.textContent, "17890");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(b2.textContent, "2000");
  assert.equal(f2.textContent, "5800");
  assert.equal(f7.textContent, "18740");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  b2.focus();
  b2.textContent = "3150";
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

test("real userClick on save waits for blur-triggered async evaluate_sheet before exporting", async () => {
  const pendingEvaluations = [];
  const convertPayloads = [];

  const { document, userClick } = await createWorkspaceHarness({
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
        return { commands: [] };
      }
      if (command === "evaluate_sheet") {
        return new Promise((resolve) => {
          pendingEvaluations.push({ payload: args.payload, resolve });
        });
      }
      if (command === "convert_document_format") {
        convertPayloads.push(args.payload);
        return {
          status: "CONVERTED",
          sourceFormat: "xlsx",
          targetFormat: "xlsx",
          sourceBytes: 128,
          outputBytes: 256,
          specification: "ECMA-376",
          convertedBase64: "UEs=",
        };
      }
      return { status: "ok" };
    },
  });

  document.querySelector('.mod-btn[data-module="SHEET"]').click();
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "3150";
  b2.dispatchEvent({ type: "input", target: b2 });

  userClick(document.getElementById("qat-save"));
  await flushAsyncTurns();

  assert.equal(pendingEvaluations.length, 1);
  assert.equal(convertPayloads.length, 0);

  pendingEvaluations[0].resolve({
    query: "F2",
    value: "6950",
    values: { "Sheet1!F2": "6950", "Sheet1!F7": "19890" },
  });
  await flushAsyncTurns();

  assert.equal(convertPayloads.length, 1);
  assert.match(convertPayloads[0], /\[\[TABLE_ROW\]\][^\n]*\t6950/);
});

test("switching sheet tabs with structured JSON IPC and special sheet names avoids contamination", async () => {
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
  const parsedRpc = JSON.parse(pendingEvaluations[0].payload);
  assert.ok(
    parsedRpc.cells.some(
      (c) => c.sheet === "Sheet1" && c.cell === "B2" && c.value === "2150"
    )
  );
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

test("trailing backslashes, special-character sheet names, and unblurred save work cleanly offline", async () => {
  const { document, downloads } = await createWorkspaceHarness();
  document.querySelector('.mod-btn[data-module="SHEET"]').click();
  await flushAsyncTurns();

  const sheetTabBar = document.getElementById("sheet-tab-bar");
  const addSheetBtn = document.getElementById("btn-add-sheet-tab");
  const customTab = document.createElement("button");
  customTab.className = "sheet-tab-btn";
  customTab.dataset.sheetTab = "예산(안), Q1";
  customTab.textContent = "예산(안), Q1";
  addSheetBtn.before(customTab);
  customTab.click();
  await flushAsyncTurns();

  const customB1 = document.querySelector('#pane-sheet td[data-cell="B1"]');
  assert.equal(customB1.textContent, "500");

  sheetTabBar.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]').click();
  await flushAsyncTurns();

  const a9 = document.querySelector('#pane-sheet td[data-cell="A9"]');
  a9.focus();
  a9.textContent = "C:\\";
  a9.dispatchEvent({ type: "input", target: a9 });
  a9.blur();
  await flushAsyncTurns();

  const b9 = document.querySelector('#pane-sheet td[data-cell="B9"]');
  b9.focus();
  b9.textContent = "='예산(안), Q1'!B1+50";
  b9.dispatchEvent({ type: "input", target: b9 });
  b9.blur();
  await flushAsyncTurns();
  assert.equal(b9.textContent, "550");

  customTab.click();
  await flushAsyncTurns();
  const sCustomB1 = document.querySelector('#pane-sheet td[data-cell="B1"]');
  sCustomB1.focus();
  sCustomB1.textContent = "700";
  sCustomB1.dispatchEvent({ type: "input", target: sCustomB1 });

  document.getElementById("qat-save").click();
  await flushAsyncTurns();
  assert.equal(downloads.length, 1);

  sheetTabBar.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]').click();
  await flushAsyncTurns();
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A9"]').textContent, "C:\\");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B9"]').textContent, "750");
});

test("security audit blocks >512-char XXE and handles Tauri rejection while PII scrub preserves inline markup and slides", async () => {
  const { document, downloads } = await createWorkspaceHarness();
  const docEditor = document.getElementById("doc-editor");

  const filler = "가나다라마바사아자차카타파하 ".repeat(50);
  docEditor.innerHTML =
    '<div class="doc-wrapper" data-expr="A1>10"><p><strong>담당자</strong> 직통 010-1234-5678</p><p id="xxe-target"></p>' +
    '<div class="doc-table-toolbar" contenteditable="false"><button id="btn-doc-add-row">+행</button></div>' +
    '<table id="doc-table"><tbody id="doc-table-body"><tr><td>기획팀</td><td>010-9876-5432</td><td data-doc-num="420">420</td></tr></tbody></table></div>' +
    '<p id="hwp-manuscript-preview" hidden>[원고지]</p>' +
    '<footer class="doc-page-footer">- 1 -</footer>';
  document.getElementById("xxe-target").textContent =
    filler +
    '문의 메일 contact@example.org <!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>';

  assert.equal(docEditor.querySelectorAll(":scope > .doc-wrapper > p").length, 2);

  document.getElementById("btn-run-security-audit").click();
  await flushAsyncTurns();
  assert.match(document.getElementById("security-status-text").textContent, /REJECTED/);

  document.getElementById("btn-scrub-pii").click();
  await flushAsyncTurns();

  assert.match(document.getElementById("pii-result-text").textContent, /3건 마스킹 완료/);
  assert.ok(docEditor.querySelector("strong"), "inline <strong> must survive PII scrub");
  assert.equal(docEditor.querySelector("strong").textContent, "담당자");
  assert.ok(docEditor.querySelector("#doc-table"), "inline #doc-table must survive PII scrub");
  assert.ok(!docEditor.textContent.includes("010-1234-5678"));
  assert.ok(!docEditor.textContent.includes("contact@example.org"));
  assert.ok(!docEditor.textContent.includes("010-9876-5432"));
  assert.ok(docEditor.textContent.includes("010-****-****"));
  assert.ok(docEditor.textContent.includes("***@***.***"));

  document.querySelector('.mod-btn[data-module="SLIDE"]').click();
  await flushAsyncTurns();
  document.getElementById("btn-add-slide").click();
  await flushAsyncTurns();
  const slideNotes = document.getElementById("slide-presenter-notes");
  slideNotes.value = "발표 문의 lead@example.com / 010-2222-3333";
  slideNotes.dispatchEvent({ type: "input", target: slideNotes });
  document.querySelector('#slide-thumb-list [data-slide-idx="0"]').click();
  await flushAsyncTurns();

  document.getElementById("btn-scrub-pii").click();
  await flushAsyncTurns();
  assert.match(document.getElementById("pii-result-text").textContent, /2건 마스킹 완료/);
  document.getElementById("qat-save").click();
  await flushAsyncTurns();
  assert.equal(downloads.length, 1);

  const tauriHarness = await createWorkspaceHarness({
    tauriInvoke: (command) => {
      if (command === "get_workspace_bootstrap") {
        return {
          platform: "Windows",
          shortcutModifier: "Ctrl",
          recentFormatFilters: ["전체"],
          openTabs: [],
        };
      }
      if (command === "get_ribbon_category_commands") {
        return { commands: [] };
      }
      if (command === "audit_security") {
        return Promise.reject(new Error("SecurityViolation: XXE DOCTYPE blocked"));
      }
      return { status: "ok" };
    },
  });
  tauriHarness.document.getElementById("btn-run-security-audit").click();
  await flushAsyncTurns();
  assert.match(
    tauriHarness.document.getElementById("security-status-text").textContent,
    /REJECTED.*XXE DOCTYPE blocked/
  );
});
