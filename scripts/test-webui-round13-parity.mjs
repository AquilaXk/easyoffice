import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

function makeF2AnchoredOfflineRpc(sandbox) {
  return sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    getActiveSheetCellRef: () => "F2",
    ribbonCategoryTools: { DOC: { home: [] } },
    ribbonStyleLabels: { standard: [], hangul: [] },
    contextualToolBadges: {},
    moduleSpecs: {},
    localRibbonState: {},
  });
}

test("left-skewed binary chains evaluate iteratively without RangeError across 15 chained 1000-term cells", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const offlineRpc = makeF2AnchoredOfflineRpc(sandbox);

  const cells = [{ sheet: "Sheet1", cell: "A1", value: 1 }];
  const plusThousand = "+1".repeat(1000);
  for (let idx = 2; idx <= 16; idx += 1) {
    const prevRef = "A" + String(idx - 1);
    cells.push({
      sheet: "Sheet1",
      cell: "A" + String(idx),
      value: "=" + prevRef + plusThousand,
    });
  }

  const res = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "A16" }],
      cells,
    })
  );
  assert.deepEqual(res.targetValues, ["15001"]);

  const concatChain = '="seed"' + '&"x"'.repeat(500);
  const concatRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      expr: concatChain,
    })
  );
  assert.equal(concatRes.value.length, 504);
});

test("depth > 64 cutoff does not poison workbook memo for shallower targets in a 66-cell chain", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const offlineRpc = makeF2AnchoredOfflineRpc(sandbox);

  const cells = [];
  for (let i = 1; i <= 65; i += 1) {
    cells.push({
      sheet: "Sheet1",
      cell: "A" + String(i),
      value: "=A" + String(i + 1),
    });
  }
  cells.push({ sheet: "Sheet1", cell: "A66", value: "100" });
  cells.push({ sheet: "Sheet1", cell: "B1", value: "=IFERROR(A1, 999)" });

  const res = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "A1" }, { cell: "A65" }, { cell: "A2" }],
      cells,
    })
  );
  assert.deepEqual(res.targetValues, ["#REF!", "100", "100"]);

  const ifErrorRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      targets: [{ cell: "B1" }, { cell: "A65" }],
      cells,
    })
  );
  assert.deepEqual(ifErrorRes.targetValues, ["999", "100"]);
});

test("digit-leading unquoted sheets fail with #VALUE!, INDIRECT resolves scalar and range refs, and standalone ROW() anchors at A1", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const offlineRpc = makeF2AnchoredOfflineRpc(sandbox);

  const sheetSyntaxRes = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Main",
      sheets: ["Main", "2026", "123", "1.2.3", "2026Q1", "Summary", "'A''B'", "A'B"],
      targets: [
        { cell: "=2026!A1" },
        { cell: "=123!A1" },
        { cell: "=1.2.3!A1" },
        { cell: "='2026'!A1" },
        { cell: "of:=[2026Q1.A1]" },
        { cell: '=SUM(INDIRECT("\'Summary\'!A1:\'Summary\'!A2"))' },
        { cell: '=INDIRECT("\'\'\'A\'\'\'\'B\'\'\'!A1")' },
        { cell: '=INDIRECT("2026!A1")' },
        { cell: '=INDIRECT("A\'B!A1")' },
        { cell: '=INDIRECT("Missing!A1")' },
        { cell: '=INDIRECT("A1", 1/0)' },
        { cell: '=INDIRECT("A1", FALSE)' },
        { cell: '=ROW(INDIRECT("Summary!B7:C9"))' },
      ],
      cells: [
        { sheet: "Main", cell: "A1", value: 77 },
        { sheet: "2026", cell: "A1", value: 420 },
        { sheet: "123", cell: "A1", value: 111 },
        { sheet: "1.2.3", cell: "A1", value: 222 },
        { sheet: "2026Q1", cell: "A1", value: 840 },
        { sheet: "Summary", cell: "A1", value: 30 },
        { sheet: "Summary", cell: "A2", value: 45 },
        { sheet: "'A''B'", cell: "A1", value: 29 },
        { sheet: "A'B", cell: "A1", value: 11 },
      ],
    })
  );
  assert.deepEqual(sheetSyntaxRes.targetValues, [
    "#VALUE!",
    "#VALUE!",
    "#VALUE!",
    "420",
    "840",
    "75",
    "29",
    "#REF!",
    "#REF!",
    "#REF!",
    "#DIV/0!",
    "#VALUE!",
    "7",
  ]);

  assert.equal(
    offlineRpc.evaluate_sheet(JSON.stringify({ sheet: "Sheet1", expr: "=ROW()" })).value,
    "1"
  );
  assert.equal(
    offlineRpc.evaluate_workbook_formula(JSON.stringify({ sheet: "Sheet1", expr: "=ROW()" })).value,
    "1"
  );
  assert.equal(
    offlineRpc.evaluate_sheet(
      JSON.stringify({ sheet: "Sheet1", cell: "C5", expr: "=ROW()" })
    ).value,
    "5"
  );
  assert.throws(
    () =>
      offlineRpc.evaluate_sheet(
        JSON.stringify({ sheet: "Sheet1", cell: "NOT_A_CELL", expr: "=ROW()" })
      ),
    /InvalidArgument/
  );
});

test("focus-retaining ribbon and theme clicks refresh focused cell pendingSnapshot so Undo preserves formatting and backend alignment", async () => {
  let executeRibbonCalls = 0;
  let undoRibbonCalls = 0;
  const backendUndoStack = [];
  let backendIsBold = false;

  const { document, sandbox, userClick } = await createWorkspaceHarness({
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
        return {
          commands: [{ id: "sheet-home-bold", title: "굵게", shortcut: "⌘B" }],
        };
      }
      if (command === "execute_ribbon_command") {
        executeRibbonCalls += 1;
        const cmdId = String(args.payload || "");
        const prevBold = backendIsBold;
        if (cmdId === "sheet-home-bold") {
          backendIsBold = !backendIsBold;
        }
        backendUndoStack.push({ cmdId, prevBold });
        return { commandId: cmdId, summary: cmdId, isBold: backendIsBold };
      }
      if (command === "undo_ribbon_command") {
        undoRibbonCalls += 1;
        const entry = backendUndoStack.pop();
        if (!entry) {
          return Promise.reject(new Error("backend undo stack underflow"));
        }
        backendIsBold = entry.prevBold;
        return { commandId: "undo", summary: "실행 취소 완료", isBold: backendIsBold };
      }
      if (command === "evaluate_sheet") {
        const req = JSON.parse(args.payload);
        const targetValues = (req.targets || []).map(() => "4950");
        return { query: "F2", value: "4950", values: {}, targetValues };
      }
      return { status: "ok" };
    },
  });

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();

  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const boldBtn = document.querySelector('[data-command-id="sheet-home-bold"]');
  assert.ok(f2);
  assert.ok(boldBtn);

  f2.focus();
  assert.equal(document.activeElement, f2);
  const baselineText = f2.textContent;

  boldBtn.click();
  await flushAsyncTurns(10);
  assert.equal(document.activeElement, f2);
  assert.equal(f2.style.fontWeight, "600");
  assert.equal(executeRibbonCalls, 1);

  f2.textContent = "999";
  f2.dispatchEvent({ type: "input", target: f2 });
  f2.blur();
  await flushAsyncTurns(10);
  assert.equal(f2.textContent, "999");
  assert.equal(f2.style.fontWeight, "600");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.textContent, baselineText);
  assert.equal(f2.style.fontWeight, "600");
  assert.equal(undoRibbonCalls, 0);
  assert.equal(backendUndoStack.length, 1);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.style.fontWeight, "");
  assert.equal(undoRibbonCalls, 1);
  assert.equal(backendUndoStack.length, 0);

  const themeBtn = document.getElementById("btn-theme-toggle");
  f2.focus();
  assert.equal(document.activeElement, f2);
  f2.textContent = "123";
  f2.dispatchEvent({ type: "input", target: f2 });
  themeBtn.click();
  await flushAsyncTurns(10);
  assert.equal(document.activeElement, f2);
  assert.equal(document.documentElement.dataset.theme, "dark");

  f2.textContent = "888";
  f2.dispatchEvent({ type: "input", target: f2 });
  f2.blur();
  await flushAsyncTurns(10);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.textContent, "123");
  assert.equal(document.documentElement.dataset.theme, "dark");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(document.documentElement.dataset.theme, "light");
  assert.equal(f2.textContent, "123");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.textContent, baselineText);
});

test("DOC vs HWP and Slide 1 vs Slide 2 layout commands derive state from target DOM SSOT independently", async () => {
  const { document, userClick } = await createWorkspaceHarness();
  const docEditor = document.getElementById("doc-editor");

  userClick(document.querySelector('.ribbon-btn[data-category="layout"]'));
  await flushAsyncTurns();

  userClick(document.querySelector('[data-command-id="document-layout-orientation"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="document-layout-columns"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="document-layout-margins"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="document-layout-paper"]'));
  await flushAsyncTurns();

  assert.equal(docEditor.style.maxWidth, "980px");
  assert.equal(docEditor.style.columnCount, "2");
  assert.equal(docEditor.style.padding, "16px 20px");
  assert.equal(docEditor.dataset.paperSize, "Letter");

  userClick(document.querySelector('.mod-btn[data-module="HWP"]'));
  await flushAsyncTurns();

  assert.equal(docEditor.style.maxWidth, "");
  assert.equal(docEditor.style.columnCount, "");
  assert.equal(docEditor.style.padding, "");
  assert.equal(docEditor.dataset.paperSize, undefined);
  assert.equal(docEditor.classList.contains("manuscript-grid"), false);
  assert.equal(document.getElementById("hwp-manuscript-preview").hidden, true);

  userClick(document.querySelector('[data-command-id="hwp-layout-orientation"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="hwp-layout-columns"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="hwp-layout-margins"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="hwp-layout-paper"]'));
  await flushAsyncTurns();

  assert.equal(docEditor.style.maxWidth, "980px");
  assert.equal(docEditor.style.columnCount, "2");
  assert.equal(docEditor.style.padding, "16px 20px");
  assert.equal(docEditor.dataset.paperSize, "Letter");

  userClick(document.querySelector('.ribbon-btn[data-category="moduleSpecific"]'));
  await flushAsyncTurns();
  const hwpTable = docEditor.querySelector("#doc-table");
  userClick(document.querySelector('[data-command-id="hwp-mod-border"]'));
  await flushAsyncTurns();
  assert.equal(hwpTable.style.borderStyle, "double");
  hwpTable.style.borderStyle = "";
  userClick(document.querySelector('[data-command-id="hwp-mod-border"]'));
  await flushAsyncTurns();
  assert.equal(hwpTable.style.borderStyle, "double");

  userClick(document.querySelector('[data-command-id="hwp-mod-manuscript"]'));
  await flushAsyncTurns();
  assert.equal(docEditor.classList.contains("manuscript-grid"), true);
  assert.equal(document.getElementById("hwp-manuscript-preview").hidden, false);

  userClick(document.getElementById("qat-undo"));
  await flushAsyncTurns(8);
  assert.equal(docEditor.classList.contains("manuscript-grid"), false);
  assert.equal(document.getElementById("hwp-manuscript-preview").hidden, true);

  userClick(document.querySelector('[data-command-id="hwp-mod-manuscript"]'));
  await flushAsyncTurns();
  assert.equal(docEditor.classList.contains("manuscript-grid"), true);
  assert.equal(document.getElementById("hwp-manuscript-preview").hidden, false);

  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns();
  assert.equal(docEditor.classList.contains("manuscript-grid"), false);
  assert.equal(document.getElementById("hwp-manuscript-preview").hidden, true);

  userClick(document.querySelector('.mod-btn[data-module="SLIDE"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('.ribbon-btn[data-category="layout"]'));
  await flushAsyncTurns();

  const lowerZone = document.getElementById("slide-lower-zone");
  const stageBox = document.getElementById("slide-stage-box");

  userClick(document.querySelector('[data-command-id="slide-layout-columns"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="slide-layout-margins"]'));
  await flushAsyncTurns();
  assert.equal(lowerZone.style.columnCount, "2");
  assert.equal(stageBox.style.padding, "12px");

  userClick(document.querySelector('.slide-thumb[data-slide-idx="1"]'));
  await flushAsyncTurns();
  assert.equal(lowerZone.style.columnCount, "");
  assert.equal(stageBox.style.padding, "");

  userClick(document.querySelector('[data-command-id="slide-layout-columns"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="slide-layout-margins"]'));
  await flushAsyncTurns();
  assert.equal(lowerZone.style.columnCount, "2");
  assert.equal(stageBox.style.padding, "12px");

  userClick(document.querySelector('.slide-thumb[data-slide-idx="0"]'));
  await flushAsyncTurns();
  assert.equal(lowerZone.style.columnCount, "2");
  assert.equal(stageBox.style.padding, "12px");
});

test("concurrent switchSheetTab and performWorkspaceUndo serialize ahead of qat-save, convert, and security audit on workspaceHistoryQueue", async () => {
  let evalGateResolve = null;
  let undoGateResolve = null;
  const eventLog = [];

  const { document, sandbox, userClick } = await createWorkspaceHarness({
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") {
        return {
          platform: "macOS",
          shortcutModifier: "⌘",
          recentFormatFilters: ["전체"],
          openTabs: [],
        };
      }
      if (command === "get_ribbon_category_commands") {
        return {
          commands: [{ id: "sheet-home-bold", title: "굵게", shortcut: "⌘B" }],
        };
      }
      if (command === "evaluate_sheet") {
        if (evalGateResolve) {
          await new Promise((resolve) => {
            evalGateResolve = resolve;
          });
        }
        eventLog.push("evaluate_sheet");
        return { query: "F2", value: "5800", values: {}, targetValues: ["5800"] };
      }
      if (command === "execute_ribbon_command") {
        return { commandId: String(args.payload || ""), summary: "ok", isBold: true };
      }
      if (command === "undo_ribbon_command") {
        if (undoGateResolve) {
          await new Promise((resolve) => {
            undoGateResolve = resolve;
          });
        }
        eventLog.push("undo_ribbon_command");
        return { commandId: "undo", summary: "실행 취소 완료", isBold: false };
      }
      if (command === "convert_document_format") {
        eventLog.push("convert:" + String(args.payload || ""));
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
      if (command === "audit_security") {
        eventLog.push("audit_security");
        return { status: "SAFE" };
      }
      return { status: "ok" };
    },
  });

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();

  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "2000";
  b2.dispatchEvent({ type: "input", target: b2 });

  evalGateResolve = () => {};
  const sheet2Tab = document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]');
  sheet2Tab.click();
  document.getElementById("qat-save").click();
  await flushAsyncTurns(4);

  assert.equal(
    eventLog.some((entry) => entry.startsWith("convert:")),
    false
  );

  const releaseEval = evalGateResolve;
  evalGateResolve = null;
  releaseEval();
  await flushAsyncTurns(12);

  const firstConvert = eventLog.find((entry) => entry.startsWith("convert:")) || "";
  assert.match(firstConvert, /기획조정실/);

  eventLog.length = 0;
  userClick(document.querySelector('[data-command-id="sheet-home-bold"]'));
  await flushAsyncTurns(8);

  undoGateResolve = () => {};
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  document.getElementById("qat-save").click();
  document.getElementById("btn-run-convert").click();
  document.getElementById("btn-run-security-audit").click();
  await flushAsyncTurns(4);

  assert.deepEqual(eventLog, []);

  const releaseUndo = undoGateResolve;
  undoGateResolve = null;
  releaseUndo();
  await flushAsyncTurns(16);

  assert.equal(eventLog[0], "undo_ribbon_command");
  assert.equal(
    eventLog.filter((entry) => entry.startsWith("convert:")).length,
    2
  );
  assert.equal(eventLog[eventLog.length - 1], "audit_security");
});

test("evaluate_sheet batch targets anchor validation, Roman numeral sheet ident, and ribbon-style Undo sync", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  const offlineRpc = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    getActiveSheetCellRef: () => "A1",
    ribbonCategoryTools: sandbox.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: sandbox.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_STYLE_LABELS,
    contextualToolBadges: sandbox.__EASYOFFICE_RIBBON_CATALOG__.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: sandbox.__EASYOFFICE_RIBBON_CATALOG__.MODULE_SPECS,
    localRibbonState: sandbox.__EASYOFFICE_RIBBON_CATALOG__.createDefaultRibbonState(),
  });

  const batchAnchor = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      cell: "C5",
      targets: [{ sheet: "Sheet1", cell: "=ROW()" }, { sheet: "Sheet1", cell: "=COLUMN()" }],
    })
  );
  assert.deepEqual(batchAnchor.targetValues, ["5", "3"]);

  const omittedExprAtCell = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Sheet1",
      cell: "C5",
      cells: [{ sheet: "Sheet1", cell: "C5", value: "77" }],
    })
  );
  assert.equal(omittedExprAtCell.value, "77");

  assert.throws(() =>
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "Sheet1",
        cell: "NOT_A_CELL",
        targets: [{ sheet: "Sheet1", cell: "A1" }],
      })
    )
  );
  assert.throws(() =>
    offlineRpc.evaluate_sheet(
      JSON.stringify({
        sheet: "Sheet1",
        cell: "A$B1",
        expr: "=ROW()",
      })
    )
  );

  const romanSheet = offlineRpc.evaluate_sheet(
    JSON.stringify({
      sheet: "Main",
      sheets: ["Main", "Ⅰ분기", "１분기", "2026Q1"],
      targets: [
        { sheet: "Main", cell: "=Ⅰ분기!A1" },
        { sheet: "Main", cell: "=INDIRECT(\"Ⅰ분기!A1\")" },
        { sheet: "Main", cell: "=INDIRECT(\"１분기!A1\")" },
        { sheet: "Main", cell: "=INDIRECT(\"'１분기'!A1\")" },
        { sheet: "Main", cell: "=ROW(A01)" },
        { sheet: "Main", cell: "=ROW(FOOBAR)" },
        { sheet: "Main", cell: "=IF(TRUE, 42, Main!A1:Ⅰ분기!B2)" },
        { sheet: "Main", cell: "=IFERROR(Main!A1:'2026Q1'!B2, 99)" },
        { sheet: "Main", cell: "=Main!A1:Ⅰ분기!B2" },
        { sheet: "Main", cell: "=Missing!A1:B2" },
        { sheet: "Main", cell: "=Missing!A1:Missing!B2" },
        { sheet: "Main", cell: "=Missing!A1:A1" },
        { sheet: "Main", cell: "=Ⅰ분기!A1:A1" },
        { sheet: "Main", cell: "=2026분기!B2" },
      ],
      cells: [
        { sheet: "Ⅰ분기", cell: "A1", value: "66" },
        { sheet: "１분기", cell: "A1", value: "44" },
      ],
    })
  );
  assert.deepEqual(romanSheet.targetValues, [
    "66",
    "66",
    "#REF!",
    "44",
    "#REF!",
    "#NAME?",
    "42",
    "99",
    "#REF!",
    "#REF!",
    "#REF!",
    "#REF!",
    "66",
    "#VALUE!",
  ]);

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();
  const f2 = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const baselineText = f2.textContent;
  const btnStyle = document.getElementById("btn-ribbon-style");
  const firstRibbonTab = document.querySelector("#ribbon-tabs .ribbon-btn");
  const baselineTabText = firstRibbonTab.textContent;

  f2.focus();
  f2.textContent = "456";
  f2.dispatchEvent({ type: "input", target: f2 });
  btnStyle.click();
  await flushAsyncTurns(10);

  assert.equal(btnStyle.textContent, "리본: 한글(HWP) 7탭");
  assert.notEqual(firstRibbonTab.textContent, baselineTabText);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(btnStyle.textContent, "리본: 표준 오피스");
  assert.equal(firstRibbonTab.textContent, baselineTabText);
  assert.equal(f2.textContent, "456");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(btnStyle.textContent, "리본: 한글(HWP) 7탭");
  assert.notEqual(firstRibbonTab.textContent, baselineTabText);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(btnStyle.textContent, "리본: 표준 오피스");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(f2.textContent, baselineText);

  const btnTheme = document.getElementById("btn-theme-toggle");
  assert.equal(document.documentElement.dataset.theme || "light", "light");
  btnTheme.click();
  await flushAsyncTurns(10);
  assert.equal(document.documentElement.dataset.theme, "dark");

  userClick(document.querySelector('[data-category="view"]'));
  await flushAsyncTurns(10);
  const ribbonThemeBtn = document.querySelector('[data-command-id="sheet-view-theme"]');
  assert.ok(ribbonThemeBtn);
  ribbonThemeBtn.click();
  await flushAsyncTurns(10);
  assert.equal(document.documentElement.dataset.theme, "light");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(document.documentElement.dataset.theme, "dark");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(document.documentElement.dataset.theme, "light");

  userClick(document.querySelector('.mod-btn[data-module="HWP"]'));
  await flushAsyncTurns(10);
  const hwpPreview = document.getElementById("hwp-manuscript-preview");
  assert.ok(hwpPreview);
  assert.equal(hwpPreview.classList.contains("manuscript-notice"), true);
});
