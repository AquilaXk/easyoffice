import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

test("focused cell edit supports undo, redo formula recalculation, and subsequent re-edit", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
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

  const { document, userClick } = await createWorkspaceHarness({
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

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
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

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
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
  const { document, userClick } = await createWorkspaceHarness({
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

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "2150";
  b2.dispatchEvent({ type: "input", target: b2 });

  const sheet2Btn = document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]');
  userClick(sheet2Btn);
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
  userClick(sheet1Btn);
  await flushAsyncTurns();

  assert.equal(document.querySelector('#pane-sheet td[data-cell="B2"]').textContent, "2150");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="F2"]').textContent, "5950");
});

test("trailing backslashes, special-character sheet names, and unblurred save work cleanly offline", async () => {
  const { document, sandbox, downloads, revokedUrls, userClick, advanceTimers } =
    await createWorkspaceHarness();
  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();

  const sheetTabBar = document.getElementById("sheet-tab-bar");
  const addSheetBtn = document.getElementById("btn-add-sheet-tab");
  const customTab = document.createElement("button");
  customTab.className = "sheet-tab-btn";
  customTab.dataset.sheetTab = "예산(안), Q1";
  const labelSpan = document.createElement("span");
  labelSpan.textContent = "예산(안), Q1";
  customTab.appendChild(labelSpan);
  addSheetBtn.before(customTab);

  assert.equal(customTab.matches(":scope"), true);
  assert.equal(
    sheetTabBar.querySelector('[data-sheet-tab="예산(안), Q1"] > span'),
    labelSpan
  );

  const apostropheTab = document.createElement("button");
  apostropheTab.className = "sheet-tab-btn";
  apostropheTab.dataset.sheetTab = "'특수시트'";
  apostropheTab.textContent = "'특수시트'";
  addSheetBtn.before(apostropheTab);

  userClick(apostropheTab);
  await flushAsyncTurns();
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B1"]').textContent, "500");

  userClick(customTab);
  await flushAsyncTurns();

  const customB1 = document.querySelector('#pane-sheet td[data-cell="B1"]');
  assert.equal(customB1.textContent, "500");

  userClick(sheetTabBar.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
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

  const c9 = document.querySelector('#pane-sheet td[data-cell="C9"]');
  c9.focus();
  c9.textContent = "='''특수시트'''!B1+25";
  c9.dispatchEvent({ type: "input", target: c9 });
  c9.blur();
  await flushAsyncTurns();
  assert.equal(c9.textContent, "525");

  const d9 = document.querySelector('#pane-sheet td[data-cell="D9"]');
  d9.focus();
  d9.textContent = "=100+200";
  d9.dispatchEvent({ type: "input", target: d9 });
  sandbox.dispatchWindowKeydown({ key: "s", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.activeElement, d9);
  assert.equal(d9.textContent, "=100+200");
  assert.equal(downloads.length, 1);
  assert.equal(revokedUrls.length, 0);
  advanceTimers(1500);
  assert.equal(revokedUrls.length, 1);

  d9.blur();
  await flushAsyncTurns();
  assert.equal(d9.textContent, "300");

  userClick(customTab);
  await flushAsyncTurns();
  const sCustomB1 = document.querySelector('#pane-sheet td[data-cell="B1"]');
  sCustomB1.focus();
  sCustomB1.textContent = "700";
  sCustomB1.dispatchEvent({ type: "input", target: sCustomB1 });

  userClick(document.getElementById("qat-save"));
  await flushAsyncTurns();
  assert.equal(downloads.length, 2);

  userClick(sheetTabBar.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns();
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A9"]').textContent, "C:\\");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B9"]').textContent, "750");
});

test("security audit blocks >512-char and whitespace XXE plus PDF inputs while PII scrub handles split inline tags, active-tab isolation, and PDF undo", async () => {
  const { document, sandbox, downloads, userClick } = await createWorkspaceHarness();
  const docEditor = document.getElementById("doc-editor");

  userClick(document.querySelector('.mod-btn[data-module="HWP"]'));
  await flushAsyncTurns();
  docEditor.innerHTML = "<p>한글 기안문 전화 010-7777-8888</p>";
  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns();

  const filler = "가나다라마바사아자차카타파하 ".repeat(50);
  docEditor.innerHTML =
    '<div class="doc-wrapper" data-expr="A1>10"><p><strong>담당자</strong> 직통 010-1234-5678</p>' +
    "<p><strong>contact</strong>@example.org 문의</p>" +
    '<p id="xxe-target"></p>' +
    '<div class="doc-table-toolbar" contenteditable="false"><button id="btn-doc-add-row">+행</button></div>' +
    '<table id="doc-table"><tbody id="doc-table-body"><tr><td>기획팀</td><td>010-9876-5432</td><td data-doc-num="900101-1234567">900101-1234567</td></tr></tbody></table></div>' +
    '<p id="hwp-manuscript-preview" hidden>[원고지]</p>' +
    '<footer class="doc-page-footer">- 1 -</footer>';
  document.getElementById("xxe-target").textContent =
    filler + '검토본 <! DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>';

  assert.equal(docEditor.querySelectorAll(":scope > .doc-wrapper > p").length, 3);

  userClick(document.getElementById("btn-run-security-audit"));
  await flushAsyncTurns();
  assert.match(document.getElementById("security-status-text").textContent, /REJECTED/);

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns();

  assert.match(document.getElementById("pii-result-text").textContent, /4건 마스킹 완료/);
  assert.ok(docEditor.querySelector("strong"), "inline <strong> must survive PII scrub");
  assert.equal(docEditor.querySelector("strong").textContent, "담당자");
  assert.ok(docEditor.querySelector("#doc-table"), "inline #doc-table must survive PII scrub");
  assert.ok(!docEditor.textContent.includes("010-1234-5678"));
  assert.ok(!docEditor.textContent.includes("contact@example.org"));
  assert.ok(!docEditor.textContent.includes("010-9876-5432"));
  assert.ok(!docEditor.textContent.includes("900101-1234567"));
  assert.equal(docEditor.querySelector("[data-doc-num]").dataset.docNum, "0");
  assert.ok(docEditor.textContent.includes("010-****-****"));
  assert.ok(docEditor.textContent.includes("***@***"));
  assert.ok(docEditor.textContent.includes("******-*******"));

  userClick(document.querySelector('.mod-btn[data-module="HWP"]'));
  await flushAsyncTurns();
  assert.ok(
    docEditor.textContent.includes("010-7777-8888"),
    "inactive HWP buffer must not be mutated when scrubbing DOC"
  );

  userClick(document.querySelector('.mod-btn[data-module="PDF"]'));
  await flushAsyncTurns();
  const wmInput = document.getElementById("pdf-watermark-input");
  const applicantInput = document.getElementById("pdf-form-applicant");
  wmInput.value = '대외비 audit@example.org <!DOCTYPE x [<!ENTITY e "x">]>';
  applicantInput.value = "김연구 010-5555-6666";
  userClick(document.getElementById("btn-pdf-apply"));
  await flushAsyncTurns();

  userClick(document.getElementById("btn-run-security-audit"));
  await flushAsyncTurns();
  assert.match(document.getElementById("security-status-text").textContent, /REJECTED/);

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns();
  assert.match(document.getElementById("pii-result-text").textContent, /2건 마스킹 완료/);
  assert.ok(!wmInput.value.includes("audit@example.org"));
  assert.ok(!document.getElementById("pdf-watermark-layer").textContent.includes("audit@example.org"));
  assert.ok(!applicantInput.value.includes("010-5555-6666"));

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.ok(wmInput.value.includes("audit@example.org"));
  assert.equal(applicantInput.value, "김연구 010-5555-6666");

  const offlineRpc = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    getActiveSheetCellRef: () => "A1",
    ribbonCategoryTools: { DOC: { home: [] } },
    ribbonStyleLabels: { standard: [], hangul: [] },
    contextualToolBadges: {},
    moduleSpecs: {},
    localRibbonState: {},
  });
  assert.equal(
    offlineRpc.audit_security(
      JSON.stringify({
        entries: [{ name: "word/document.xml", compressed: 0, uncompressed: 4096, encrypted: false }],
        xml: "<doc/>",
      })
    ).status,
    "REJECTED"
  );
  assert.equal(
    offlineRpc.audit_security(JSON.stringify({ entries: [], xml: "<doc/>" })).checkedEntries,
    0
  );
  assert.throws(
    () => offlineRpc.audit_security("word/document.xml,0,4096,false|<doc/>"),
    /InvalidArgument/
  );

  userClick(document.querySelector('.mod-btn[data-module="SLIDE"]'));
  await flushAsyncTurns();
  userClick(document.getElementById("btn-add-slide"));
  await flushAsyncTurns();
  const slideNotes = document.getElementById("slide-presenter-notes");
  slideNotes.value = "발표 문의 lead@example.com / 010-2222-3333";
  slideNotes.dispatchEvent({ type: "input", target: slideNotes });
  userClick(document.querySelector('#slide-thumb-list [data-slide-idx="0"]'));
  await flushAsyncTurns();

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns();
  assert.match(document.getElementById("pii-result-text").textContent, /2건 마스킹 완료/);
  userClick(document.getElementById("qat-save"));
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
  tauriHarness.userClick(tauriHarness.document.getElementById("btn-run-security-audit"));
  await flushAsyncTurns();
  assert.match(
    tauriHarness.document.getElementById("security-status-text").textContent,
    /REJECTED.*XXE DOCTYPE blocked/
  );
});

test("rapid async ribbon undo and overlapping sheet evaluations serialize cleanly", async () => {
  const pendingUndoResolvers = [];
  const pendingSheetEvals = [];
  const exportedBodies = [];
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
          commands: [{ id: "document-home-bold", title: "굵게", shortcut: "Ctrl+B" }],
        };
      }
      if (command === "execute_ribbon_command") {
        return { commandId: "document-home-bold", summary: "굵게", isBold: true };
      }
      if (command === "undo_ribbon_command") {
        return new Promise((resolve) => {
          pendingUndoResolvers.push(resolve);
        });
      }
      if (command === "evaluate_sheet") {
        return new Promise((resolve) => {
          pendingSheetEvals.push({ payload: args.payload, resolve });
        });
      }
      if (command === "convert_document_format") {
        exportedBodies.push(args.payload);
        return {
          status: "OK",
          sourceFormat: "xlsx",
          targetFormat: "xlsx",
          sourceBytes: 64,
          outputBytes: 64,
          specification: "ECMA-376",
          convertedBase64: "UEs=",
        };
      }
      return { status: "ok" };
    },
  });

  const themeBtn = document.getElementById("btn-theme-toggle");
  userClick(themeBtn);
  assert.equal(document.documentElement.dataset.theme, "dark");

  const boldBtn = document.querySelector('[data-command-id="document-home-bold"]');
  userClick(boldBtn);
  await flushAsyncTurns();

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();

  assert.equal(pendingUndoResolvers.length, 1);
  assert.equal(document.documentElement.dataset.theme, "dark");

  pendingUndoResolvers[0]({ commandId: "undo", summary: "실행 취소 완료" });
  await flushAsyncTurns(10);

  assert.equal(document.documentElement.dataset.theme, "light");

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();
  const d9 = document.querySelector('#pane-sheet td[data-cell="D9"]');
  const e9 = document.querySelector('#pane-sheet td[data-cell="E9"]');
  d9.focus();
  d9.textContent = "=100+200";
  d9.dispatchEvent({ type: "input", target: d9 });
  e9.focus();
  e9.textContent = "=400+100";
  e9.dispatchEvent({ type: "input", target: e9 });
  sandbox.dispatchWindowKeydown({ key: "s", ctrlKey: true });
  await flushAsyncTurns();

  assert.equal(pendingSheetEvals.length, 2);
  assert.equal(exportedBodies.length, 0);

  const req2 = JSON.parse(pendingSheetEvals[1].payload);
  const targetValues2 = req2.targets.map((t) => {
    if (t.cell === "D9") return "300";
    if (t.cell === "E9") return "500";
    return "0";
  });
  pendingSheetEvals[1].resolve({ query: "D9", value: "300", values: {}, targetValues: targetValues2 });
  await flushAsyncTurns();
  assert.equal(exportedBodies.length, 0, "save must wait until slower earlier opA also settles");

  pendingSheetEvals[0].resolve({ query: "D9", value: "300", values: {}, targetValues: ["300"] });
  await flushAsyncTurns(10);
  assert.equal(exportedBodies.length, 1);
  assert.equal(document.activeElement, e9);
  assert.equal(e9.textContent, "=400+100");
  assert.match(exportedBodies[0], /\[\[SHEET_A1\]\]\tE9\t=400\+100/);
  assert.match(exportedBodies[0], /\[\[TABLE_ROW\]\][^\n]*300\t500/);
});

test("multi-PII inline formatting, br boundaries, sheet formula literal PII, and multi-paragraph PDF undo preserve DOM structure", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  const docEditor = document.getElementById("doc-editor");
  docEditor.innerHTML =
    '<p id="multi-inline">1차 <strong>010-1234-5678</strong> / <em>a@b.com</em> 확인</p>' +
    '<p id="br-phone">사번 2026<br>010-9999-8888</p>' +
    '<p id="br-email">ext9<br>user@example.com</p>';

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns();

  assert.match(document.getElementById("pii-result-text").textContent, /4건 마스킹 완료/);
  const multiInline = document.getElementById("multi-inline");
  assert.ok(multiInline.querySelector("strong"));
  assert.ok(multiInline.querySelector("em"));
  assert.equal(multiInline.querySelector("strong").textContent, "010-****-****");
  assert.equal(multiInline.querySelector("em").textContent, "***@***");
  assert.equal(multiInline.textContent, "1차 010-****-**** / ***@*** 확인");

  const brPhone = document.getElementById("br-phone");
  assert.ok(brPhone.querySelector("br"));
  assert.equal(brPhone.innerHTML, "사번 2026<br>010-****-****");

  const brEmail = document.getElementById("br-email");
  assert.ok(brEmail.querySelector("br"));
  assert.equal(brEmail.innerHTML, "ext9<br>***@***");

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();
  const a9 = document.querySelector('#pane-sheet td[data-cell="A9"]');
  a9.focus();
  a9.textContent = '="담당자 ""김연구원"" 010-1234-5678"';
  a9.dispatchEvent({ type: "input", target: a9 });
  a9.blur();
  await flushAsyncTurns();

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns();
  assert.match(document.getElementById("pii-result-text").textContent, /1건 마스킹 완료/);
  assert.equal(a9.dataset.formula, '="담당자 ""김연구원"" 010-****-****"');
  assert.ok(!a9.textContent.includes("010-1234-5678"));

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(a9.dataset.formula, '="담당자 ""김연구원"" 010-1234-5678"');

  userClick(document.querySelector('.mod-btn[data-module="PDF"]'));
  await flushAsyncTurns();
  const pdfCard = document.getElementById("pdf-page-card");
  const formStatus = document.getElementById("pdf-form-status");
  pdfCard.querySelector("h3").innerHTML = '결재문서 <span>010-1111-2222</span>';
  const firstBodyP = pdfCard.querySelector("p:not(#pdf-form-status)");
  firstBodyP.innerHTML = '1문단 <strong>010-3333-4444</strong>';
  const secondBodyP = document.createElement("p");
  secondBodyP.innerHTML = '2문단 <em>sec@example.org</em>';
  formStatus.before(secondBodyP);

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns();
  assert.match(document.getElementById("pii-result-text").textContent, /3건 마스킹 완료/);
  const bodyPsAfterScrub = pdfCard.querySelectorAll("p:not(#pdf-form-status)");
  assert.equal(bodyPsAfterScrub.length, 2);
  assert.equal(bodyPsAfterScrub[0].innerHTML, "1문단 <strong>010-****-****</strong>");
  assert.equal(bodyPsAfterScrub[1].innerHTML, "2문단 <em>***@***</em>");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  const bodyPsAfterUndo = pdfCard.querySelectorAll("p:not(#pdf-form-status)");
  assert.equal(bodyPsAfterUndo.length, 2);
  assert.equal(pdfCard.querySelector("h3").innerHTML, "결재문서 <span>010-1111-2222</span>");
  assert.equal(bodyPsAfterUndo[0].innerHTML, "1문단 <strong>010-3333-4444</strong>");
  assert.equal(bodyPsAfterUndo[1].innerHTML, "2문단 <em>sec@example.org</em>");
});

test("focused formula undo-redo updates displayed result, missing sheet returns #REF!, and in-flight or failed ribbon IPC keeps undo stack consistent", async () => {
  const { document, sandbox, userClick, advanceTimers } = await createWorkspaceHarness();
  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();

  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const formulaInput = document.getElementById("formula-input");
  assert.equal(f2.textContent, "4950");

  f2.focus();
  f2.textContent = "=B2+100";
  f2.dispatchEvent({ type: "input", target: f2 });
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.activeElement, f2);
  assert.equal(f2.textContent, "4950");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.activeElement, f2);
  assert.equal(f2.textContent, "1250");
  assert.equal(f2.dataset.formula, "=B2+100");
  assert.equal(formulaInput.value, "=B2+100");

  const g9 = document.querySelector('#pane-sheet td[data-cell="G9"]');
  g9.focus();
  g9.textContent = "=NoSuchSheet!A1+10";
  g9.dispatchEvent({ type: "input", target: g9 });
  g9.blur();
  await flushAsyncTurns();
  assert.equal(g9.textContent, "#REF!");

  assert.equal(
    document.querySelectorAll('#pane-sheet td[data-cell^="B"][data-cell$="2"]').length,
    2
  );
  assert.equal(
    document.querySelectorAll(
      '#pane-sheet td[data-cell^="B"][data-cell$="2"]:not([data-cell="B12"], .none)'
    ).length,
    1
  );

  const timerOrder = [];
  sandbox.setTimeout(() => {
    timerOrder.push("t100");
    sandbox.setTimeout(() => {
      timerOrder.push("nested200");
    }, 100);
  }, 100);
  sandbox.setTimeout(() => {
    timerOrder.push("t250");
  }, 250);
  advanceTimers(300);
  assert.deepEqual(timerOrder, ["t100", "nested200", "t250"]);

  let shouldFailRibbon = false;
  let pendingRibbonResolve = null;
  let undoRpcCalls = 0;
  const asyncHarness = await createWorkspaceHarness({
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
        return {
          commands: [{ id: "document-home-bold", title: "굵게", shortcut: "Ctrl+B" }],
        };
      }
      if (command === "execute_ribbon_command") {
        if (shouldFailRibbon) {
          return Promise.reject(new Error("IPC rejected"));
        }
        return new Promise((resolve) => {
          pendingRibbonResolve = resolve;
        });
      }
      if (command === "undo_ribbon_command") {
        undoRpcCalls += 1;
        return { commandId: "undo", summary: "실행 취소 완료" };
      }
      return { status: "ok" };
    },
  });

  const boldBtn = asyncHarness.document.querySelector('[data-command-id="document-home-bold"]');
  asyncHarness.userClick(boldBtn);
  asyncHarness.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(undoRpcCalls, 0);

  pendingRibbonResolve({ commandId: "document-home-bold", summary: "굵게", isBold: true });
  await flushAsyncTurns(10);
  assert.equal(undoRpcCalls, 1);
  assert.equal(
    asyncHarness.document.querySelector("#doc-editor h1").style.fontWeight || "",
    ""
  );

  shouldFailRibbon = true;
  asyncHarness.userClick(boldBtn);
  await flushAsyncTurns(10);
  asyncHarness.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(undoRpcCalls, 1);
});
