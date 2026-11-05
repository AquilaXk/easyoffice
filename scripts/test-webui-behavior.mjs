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
  const parsedConvert = JSON.parse(convertPayloads[0]);
  assert.match(parsedConvert.body, /\[\[TABLE_ROW\]\][^\n]*\t6950/);
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
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A1"]').textContent, "부문");

  pendingEvaluations[0].resolve({
    query: "F2",
    value: "5950",
    values: {
      "Sheet1!F2": "5950",
      "Sheet2!C1": "350",
    },
  });
  await flushAsyncTurns(10);

  assert.equal(document.querySelector('#pane-sheet td[data-cell="A1"]').textContent, "100");
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
  assert.throws(
    () =>
      offlineRpc.audit_security(
        JSON.stringify({
          entries: [{ name: "word/document.xml", compressed: 0, uncompressed: 4096, encrypted: false }],
          xml: "<doc/>",
        })
      ),
    /SecurityViolation/
  );
  assert.throws(
    () =>
      offlineRpc.audit_security(
        JSON.stringify({
          entries: [],
          xml: '<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><doc/>',
        })
      ),
    /SecurityViolation/
  );
  assert.throws(
    () => offlineRpc.audit_security(JSON.stringify({ entries: [], xml: "<doc/>" })),
    /InvalidArgument/
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
  await flushAsyncTurns();
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
  const exportedConvert = JSON.parse(exportedBodies[0]);
  assert.match(exportedConvert.body, /\[\[SHEET_A1\]\]\tE9\t=400\+100/);
  assert.match(exportedConvert.body, /\[\[TABLE_ROW\]\][^\n]*300\t500/);
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
  assert.equal(a9.textContent, '담당자 "김연구원" 010-****-****');

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(a9.dataset.formula, '="담당자 ""김연구원"" 010-1234-5678"');
  assert.equal(a9.textContent, '담당자 "김연구원" 010-1234-5678');

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

test("failed undo-redo and formula-bar IPC keep stacks intact, offline sheet identity preserves literal quotes, and concurrent harnesses isolate globals", async () => {
  const baseHarness = await createWorkspaceHarness();
  const offlineRpc = baseHarness.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
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
        sheet: "특수시트",
        expr: "=SUM(A1:A2)",
        cells: [
          { sheet: "특수시트", cell: "A1", value: 200 },
          { sheet: "'특수시트'", cell: "A2", value: 300 },
        ],
      })
    ).value,
    "200"
  );
  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "특수시트",
        expr: "='특수시트'!A1+'''특수시트'''!A2",
        cells: [
          { sheet: "특수시트", cell: "A1", value: 200 },
          { sheet: "'특수시트'", cell: "A2", value: 300 },
        ],
      })
    ).value,
    "500"
  );
  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "예산표",
        sheets: ["예산표"],
        expr: "=Sheet2!A1+10",
        cells: [{ sheet: "예산표", cell: "A1", value: 300 }],
      })
    ).value,
    "#REF!"
  );
  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "Sheet1",
        expr: '="Room B2: " & B2',
        cells: [{ sheet: "Sheet1", cell: "B2", value: 1150 }],
      })
    ).value,
    "Room B2: 1150"
  );

  let failUndoOnce = true;
  let failRedoOnce = true;
  let failSheetEval = false;
  const retryHarness = await createWorkspaceHarness({
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
        return { commandId: "document-home-bold", summary: "굵게", isBold: true };
      }
      if (command === "undo_ribbon_command") {
        if (failUndoOnce) {
          failUndoOnce = false;
          return Promise.reject(new Error("transient undo failure"));
        }
        return { commandId: "undo", summary: "실행 취소 완료" };
      }
      if (command === "redo_ribbon_command") {
        if (failRedoOnce) {
          failRedoOnce = false;
          return Promise.reject(new Error("transient redo failure"));
        }
        return { commandId: "redo", summary: "다시 실행 완료" };
      }
      if (command === "evaluate_sheet") {
        if (failSheetEval) {
          return Promise.reject(new Error("sheet eval rejected"));
        }
        return { query: "F2", value: "4950", values: {}, targetValues: ["4950"] };
      }
      return { status: "ok" };
    },
  });

  const boldBtn = retryHarness.document.querySelector('[data-command-id="document-home-bold"]');
  retryHarness.userClick(boldBtn);
  await flushAsyncTurns();
  assert.equal(retryHarness.document.querySelector("#doc-editor h1").style.fontWeight, "600");

  retryHarness.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(retryHarness.document.querySelector("#doc-editor h1").style.fontWeight, "600");

  retryHarness.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(retryHarness.document.querySelector("#doc-editor h1").style.fontWeight, "");

  retryHarness.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(retryHarness.document.querySelector("#doc-editor h1").style.fontWeight, "");

  retryHarness.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(retryHarness.document.querySelector("#doc-editor h1").style.fontWeight, "600");

  retryHarness.userClick(retryHarness.document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();
  const f2 = retryHarness.document.querySelector('#pane-sheet td[data-cell="F2"]');
  const formulaInput = retryHarness.document.getElementById("formula-input");
  failSheetEval = true;
  formulaInput.value = "=B2+999";
  retryHarness.userClick(retryHarness.document.getElementById("btn-eval-formula"));
  await flushAsyncTurns();
  assert.equal(f2.dataset.formula, "=SUM(B2:E2)");
  assert.equal(f2.textContent, "4950");

  let resolveA = null;
  let resolveB = null;
  const hA = await createWorkspaceHarness({
    tauriInvoke: (cmd) => {
      if (cmd === "get_workspace_bootstrap") {
        return { platform: "Windows", shortcutModifier: "Ctrl", recentFormatFilters: ["전체"], openTabs: [] };
      }
      if (cmd === "get_ribbon_category_commands") {
        return { commands: [{ id: "document-home-bold", title: "굵게", shortcut: "Ctrl+B" }] };
      }
      if (cmd === "execute_ribbon_command") {
        return new Promise((r) => {
          resolveA = r;
        });
      }
      return { status: "ok" };
    },
  });
  const hB = await createWorkspaceHarness({
    tauriInvoke: (cmd) => {
      if (cmd === "get_workspace_bootstrap") {
        return { platform: "macOS", shortcutModifier: "⌘", recentFormatFilters: ["전체"], openTabs: [] };
      }
      if (cmd === "get_ribbon_category_commands") {
        return { commands: [{ id: "document-home-italic", title: "기울임", shortcut: "⌘I" }] };
      }
      if (cmd === "execute_ribbon_command") {
        return new Promise((r) => {
          resolveB = r;
        });
      }
      return { status: "ok" };
    },
  });

  hA.userClick(hA.document.querySelector('[data-command-id="document-home-bold"]'));
  hB.userClick(hB.document.querySelector('[data-command-id="document-home-italic"]'));
  await flushAsyncTurns();
  resolveA({ commandId: "document-home-bold", summary: "굵게A", isBold: true });
  resolveB({ commandId: "document-home-italic", summary: "기울임B", isItalic: true });
  await flushAsyncTurns(10);

  assert.equal(hA.document.querySelector("#doc-editor h1").style.fontWeight, "600");
  assert.equal(hA.document.querySelector("#doc-editor h1").style.fontStyle, "");
  assert.equal(hB.document.querySelector("#doc-editor h1").style.fontWeight, "");
  assert.equal(hB.document.querySelector("#doc-editor h1").style.fontStyle, "italic");
});

test("design tokens, ARIA tab roles, zoom controls, split compare pane, and format filters stay synchronized", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();

  const ribbonTabs = document.getElementById("ribbon-tabs");
  assert.equal(ribbonTabs.getAttribute("role"), "tablist");
  const homeTabBtn = ribbonTabs.querySelector('[data-category="home"]');
  const viewTabBtn = ribbonTabs.querySelector('[data-category="view"]');
  assert.equal(homeTabBtn.getAttribute("role"), "tab");
  assert.equal(homeTabBtn.getAttribute("aria-selected"), "true");
  assert.equal(viewTabBtn.getAttribute("aria-selected"), "false");

  const firstOpenTab = document.querySelector("#tab-strip .tab-pill");
  assert.equal(firstOpenTab.getAttribute("role"), "tab");
  assert.equal(firstOpenTab.getAttribute("aria-selected"), "true");

  const ctxBadge = document.getElementById("contextual-tool-badge");
  assert.equal(ctxBadge.textContent, "표·개체 도구");
  userClick(document.querySelector('.mod-btn[data-module="HWP"]'));
  await flushAsyncTurns();
  assert.equal(ctxBadge.textContent, "쪽·결재·원고지 도구");
  userClick(document.querySelector('.mod-btn[data-module="SLIDE"]'));
  await flushAsyncTurns();
  assert.equal(ctxBadge.textContent, "발표자·레이저 도구");
  userClick(document.querySelector('.mod-btn[data-module="PDF"]'));
  await flushAsyncTurns();
  assert.equal(ctxBadge.textContent, "전자결재·주석 도구");
  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns();

  const zoomLabel = document.getElementById("status-zoom-label");
  const stage = document.querySelector(".canvas-stage");
  assert.equal(zoomLabel.textContent, "100%");
  userClick(document.getElementById("btn-zoom-in"));
  assert.equal(zoomLabel.textContent, "125%");
  assert.equal(stage.style.zoom, "125%");
  userClick(document.getElementById("btn-zoom-out"));
  assert.equal(zoomLabel.textContent, "100%");
  assert.equal(stage.style.zoom, "100%");

  userClick(viewTabBtn);
  await flushAsyncTurns();
  assert.equal(viewTabBtn.getAttribute("aria-selected"), "true");
  assert.equal(homeTabBtn.getAttribute("aria-selected"), "false");

  const splitBtn = document.querySelector('[data-command-id="document-view-split"]');
  userClick(splitBtn);
  await flushAsyncTurns();
  assert.equal(stage.dataset.splitView, "true");
  assert.ok(stage.querySelector(".split-compare-pane"));

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.ok(!stage.querySelector(".split-compare-pane"));

  const filterBox = document.getElementById("format-filters");
  const odfChip = Array.from(filterBox.querySelectorAll(".filter-chip")).find(
    (c) => c.textContent === "ODF"
  );
  const allChip = Array.from(filterBox.querySelectorAll(".filter-chip")).find(
    (c) => c.textContent === "전체"
  );
  userClick(document.querySelector('.mod-btn[data-module="CONV"]'));
  await flushAsyncTurns();
  userClick(odfChip);
  await flushAsyncTurns();
  assert.equal(document.getElementById("pane-doc").hidden, false);
  assert.equal(filterBox.dataset.activeFilter, "ODF");
  userClick(allChip);
  await flushAsyncTurns();
  assert.equal(filterBox.dataset.activeFilter, "ALL");
});

test("DOC and HWP module-specific ribbon tools build real DOM structures and direct typing supports Undo/Redo", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  const docEditor = document.getElementById("doc-editor");

  const sumCell = document.getElementById("doc-table-sum-cell");
  userClick(document.getElementById("btn-doc-eval-table"));
  assert.equal(sumCell.textContent, "1000");
  assert.equal(sumCell.dataset.docAvg, "500");
  assert.equal(sumCell.dataset.docCount, "2");
  assert.match(document.getElementById("status-command-feedback").textContent, /평균\(AVERAGE\): 500/);

  const modTab = document.querySelector('#ribbon-tabs [data-category="moduleSpecific"]');
  userClick(modTab);
  await flushAsyncTurns();

  const mergePara = document.createElement("p");
  mergePara.id = "mailmerge-target-line";
  mergePara.textContent = "수신: {{기관명}} {{성명}} ({{일자}})";
  docEditor.appendChild(mergePara);
  docEditor.dispatchEvent({ type: "input", target: mergePara });

  userClick(document.querySelector('[data-command-id="doc-mod-toc"]'));
  await flushAsyncTurns();
  const tocNav = docEditor.querySelector(".eo-doc-toc");
  assert.ok(tocNav);
  assert.ok(tocNav.querySelectorAll("li").length >= 3);

  userClick(document.querySelector('[data-command-id="doc-mod-footnote"]'));
  await flushAsyncTurns();
  assert.ok(docEditor.querySelector(".eo-doc-footnote"));

  userClick(document.querySelector('[data-command-id="doc-mod-mailmerge"]'));
  await flushAsyncTurns();
  assert.equal(
    document.getElementById("mailmerge-target-line").textContent,
    "수신: 행정안전부 이수현 (2026-10-25)"
  );
  assert.ok(docEditor.querySelector(".eo-doc-mailmerge"));

  userClick(document.querySelector('.mod-btn[data-module="HWP"]'));
  await flushAsyncTurns();
  userClick(modTab);
  await flushAsyncTurns();

  userClick(document.querySelector('[data-command-id="hwp-mod-header"]'));
  await flushAsyncTurns();
  assert.ok(docEditor.querySelector(".eo-hwp-header"));

  userClick(document.querySelector('[data-command-id="hwp-mod-field"]'));
  await flushAsyncTurns();
  assert.ok(docEditor.querySelector(".eo-hwp-field"));

  userClick(document.querySelector('[data-command-id="hwp-mod-manuscript"]'));
  await flushAsyncTurns();
  assert.equal(docEditor.classList.contains("manuscript-grid"), true);
  assert.equal(document.getElementById("hwp-manuscript-preview").hidden, false);

  docEditor.focus();
  const h1 = docEditor.querySelector("h1");
  const prevTitle = h1.textContent;
  h1.textContent = "직접 타이핑한 한글 제목";
  docEditor.dispatchEvent({ type: "input", target: h1 });

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector("h1").textContent, prevTitle);
  assert.equal(docEditor.classList.contains("manuscript-grid"), true);

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector("h1").textContent, "직접 타이핑한 한글 제목");
});

test("SHEET supports keyboard arrow/Tab/Delete cell navigation and sort, pivot, chart, condfmt, freeze, and autosum actions", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  b2.focus();
  assert.equal(document.getElementById("active-cell-ref").textContent, "B2");

  b2.dispatchEvent({ type: "keydown", key: "ArrowDown", target: b2 });
  assert.equal(document.getElementById("active-cell-ref").textContent, "B3");
  const b3 = document.querySelector('#pane-sheet td[data-cell="B3"]');
  b3.dispatchEvent({ type: "keydown", key: "ArrowRight", target: b3 });
  assert.equal(document.getElementById("active-cell-ref").textContent, "C3");
  const c3 = document.querySelector('#pane-sheet td[data-cell="C3"]');
  c3.dispatchEvent({ type: "keydown", key: "ArrowUp", target: c3 });
  assert.equal(document.getElementById("active-cell-ref").textContent, "C2");
  const c2 = document.querySelector('#pane-sheet td[data-cell="C2"]');
  c2.dispatchEvent({ type: "keydown", key: "Tab", shiftKey: true, target: c2 });
  assert.equal(document.getElementById("active-cell-ref").textContent, "B2");

  b2.dispatchEvent({ type: "keydown", key: "Delete", target: b2 });
  await flushAsyncTurns();
  assert.equal(b2.textContent, "");
  assert.equal(f2.textContent, "3800");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(b2.textContent, "1150");
  assert.equal(f2.textContent, "4950");

  userClick(document.querySelector('#ribbon-tabs [data-category="formulasAndData"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="sheet-data-sort"]'));
  await flushAsyncTurns();
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B2"]').textContent, "510");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B6"]').textContent, "1150");

  const b9 = document.querySelector('#pane-sheet td[data-cell="B9"]');
  b9.focus();
  userClick(document.querySelector('[data-command-id="sheet-data-autosum"]'));
  await flushAsyncTurns();
  assert.equal(b9.dataset.formula, "=SUM(B2:B6)");
  assert.equal(b9.textContent, "4100");

  userClick(document.querySelector('#ribbon-tabs [data-category="moduleSpecific"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="sheet-mod-pivot"]'));
  await flushAsyncTurns();
  assert.match(document.getElementById("sheet-eval-result").textContent, /피벗 집계 · 합계/);

  userClick(document.querySelector('[data-command-id="sheet-mod-chart"]'));
  await flushAsyncTurns();
  assert.match(document.getElementById("sheet-eval-result").textContent, /차트 데이터 요약 · 합계/);

  userClick(document.querySelector('[data-command-id="sheet-mod-condfmt"]'));
  await flushAsyncTurns();
  assert.ok(document.querySelectorAll("#pane-sheet td.condfmt-high").length > 0);

  userClick(document.querySelector('[data-command-id="sheet-mod-freeze"]'));
  await flushAsyncTurns();
  assert.equal(
    document.querySelector("#pane-sheet table thead").classList.contains("frozen-header"),
    true
  );
});

test("SLIDE object insertion, presenter mode, PDF highlight/splitmerge badges, and module-aware smart diagnose work end-to-end", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();

  userClick(document.querySelector('.mod-btn[data-module="SLIDE"]'));
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 3);

  userClick(document.querySelector('#ribbon-tabs [data-category="insert"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="slide-insert-table"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="slide-insert-chart"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="slide-insert-shape"]'));
  await flushAsyncTurns();

  assert.equal(
    document.querySelectorAll("#slide-thumb-list .slide-thumb").length,
    3,
    "inserting table/chart/shape on a slide must not add new slides to the deck"
  );
  const lowerZone = document.getElementById("slide-lower-zone");
  assert.ok(lowerZone.querySelector('[data-slide-object="table"]'));
  assert.ok(lowerZone.querySelector('[data-slide-object="chart"]'));
  assert.ok(lowerZone.querySelector('[data-slide-object="shape"]'));

  userClick(document.querySelector('#ribbon-tabs [data-category="moduleSpecific"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="slide-mod-presenter"]'));
  await flushAsyncTurns();
  assert.equal(document.getElementById("slide-stage-box").dataset.presenterMode, "true");
  assert.equal(document.getElementById("slide-laser-dot").hidden, false);

  userClick(document.getElementById("btn-smart-diagnose"));
  await flushAsyncTurns();
  assert.match(document.getElementById("smart-summary-output").textContent, /슬라이드 차트 개체|발표 자료/);

  userClick(document.querySelector('.mod-btn[data-module="PDF"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('#ribbon-tabs [data-category="moduleSpecific"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="pdf-mod-highlight"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="pdf-mod-splitmerge"]'));
  await flushAsyncTurns();

  const pdfCard = document.getElementById("pdf-page-card");
  assert.ok(pdfCard.querySelector(".pdf-highlight-badge"));
  assert.ok(pdfCard.querySelector(".pdf-splitmerge-badge"));

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.ok(!pdfCard.querySelector(".pdf-splitmerge-badge"));
  assert.ok(pdfCard.querySelector(".pdf-highlight-badge"));
});

