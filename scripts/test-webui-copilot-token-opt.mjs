import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { acceptProposal, reviewButton } from "./test-support/copilot-accept.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { open, answer } from "./test-support/copilot-open.mjs";
import {
  applyCopilotEdits,
  parseApplyCopilotEditsParams,
  readParagraphRange,
} from "../apps/hwp-studio/src/embed/copilot-bridge.ts";

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

test("context compactor computes exact SHA-256, evicts history in cache-aligned chunks, caches read tools, and shrinks failure payloads", async () => {
  const env = {};
  loadUiModule(UI("copilot-context-compact.js"), env);
  const compact = env.__EASYOFFICE_COPILOT_CONTEXT_COMPACT__;

  for (const sample of ["", "hello", "한글 테스트 😀", "x".repeat(300)]) {
    assert.equal(
      compact.sha256Hex(sample),
      createHash("sha256").update(sample, "utf8").digest("hex"),
      `sha256 match for ${sample.slice(0, 12)}`,
    );
  }

  // Chunk eviction keeps prefix intact by dropping whole 4-item chunks and strips duplicate `content`
  const hist = Array.from({ length: 14 }, (_, i) => ({
    role: i % 2 === 0 ? "user" : "assistant",
    text: `메시지 ${i}`,
    content: `메시지 ${i}`,
  }));
  const trimmed = compact.compactHistory(hist, 12, 4);
  assert.equal(trimmed.length, 10, "dropped one aligned 4-item chunk (14 -> 10)");
  assert.equal(trimmed[0].text, "메시지 4");
  assert.equal("content" in trimmed[0], false, "duplicate content field stripped");

  // Loop compactor deduplicates normalized read tools with SHA-256 receipt and compacts validation retries
  const lc = compact.createLoopCompactor({ maxLoopBytes: 1200 });
  let calls = 0;
  const exec = async () => {
    calls += 1;
    return { cells: [{ coord: "A1", value: "값" }] };
  };
  const first = await lc.runCachedReadTool("SHEET", { name: "read_sheet_range", range: " a1:b2 " }, exec);
  const second = await lc.runCachedReadTool("SHEET", { name: "read_sheet_range", range: "A1:B2" }, exec);
  assert.equal(calls, 1, "normalized identical read tool invoked once");
  assert.equal(first.cached, false);
  assert.equal(second.cached, true);
  assert.match(second.receipt, /도구 캐시 영수증/);

  const hugeOp = { kind: "sheet.write", sheet: "S1", range: "A1:Z20", cells: Array.from({ length: 20 }, (_, i) => ({ coord: `A${i + 1}`, raw: "길다".repeat(20) })) };
  const retry = lc.summarizeValidationFailure({ reply: "작성" }, [hugeOp], ["#DIV/0!"]);
  assert.ok(retry.assistantText.length < 200, `retry summary compacted: ${retry.assistantText.length} bytes`);
  assert.doesNotMatch(retry.assistantText, /길다/);

  // Oversized older loop steps become SHA-256 step receipts while latest steps stay intact
  const steps = [];
  lc.pushStepPair(steps, "a1", "r1 ".repeat(500));
  lc.pushStepPair(steps, "a2", "r2");
  assert.match(steps[1].text, /^\[단계 영수증\] sha256=/);
  assert.equal(steps[3].text, "r2");
});

test("markdown pipe table serialization, doc.replace_span inline preservation & Track Changes, lightweight ghost, and multi-module context trim", async () => {
  const seenGhostReqs = [];
  const docH = await open({
    module: "DOC",
    run: (req) => {
      seenGhostReqs.push(req);
      if (req.lightweight) {
        return { status: "ok", mode: "ask", reply: "추천 문구입니다.", operations: [] };
      }
      if (req.prompt.includes("부분 고침")) {
        return answer([{ kind: "doc.replace_span", index: 0, search: "매출 100원", replace: "매출 120원" }]);
      }
      return { status: "ok", mode: "ask", reply: "확인", operations: [] };
    },
  });

  // Plain unformatted table -> Markdown pipe table; formatted table -> <table> fallback
  const docHtml = docH.host.__EASYOFFICE_COPILOT_DOC_HTML__;
  const plainTable = docHtml.buildDocBlockElement(docH.document, {
    table: { headerRow: true, rows: [["구분", "매출"], ["서울", "100"]] },
  });
  const md = docHtml.serializeBlockForContext(plainTable);
  assert.equal(md, "| 구분 | 매출 |\n| --- | --- |\n| 서울 | 100 |", "plain table serialized as compact Markdown pipe table");

  const richTable = docHtml.buildDocBlockElement(docH.document, {
    table: { headerRow: true, rows: [["구분", "매출"], ["서울", "<strong>100</strong>"]] },
  });
  assert.match(docHtml.serializeBlockForContext(richTable), /^<table>/, "formatted table keeps inline HTML");
  assert.ok(md.length < docHtml.serializeBlockForContext(richTable).length);

  // doc.replace_span preserves sibling bold span.eo-run and reverts in 1 Undo
  const p0 = docH.editor.querySelector("p");
  p0.replaceChildren();
  const strong = docH.document.createElement("span");
  strong.className = "eo-run";
  strong.dataset.eoBold = "true";
  strong.textContent = "핵심 ";
  p0.appendChild(strong);
  p0.appendChild(docH.document.createTextNode("매출 100원 달성"));
  await docH.send("부분 고침", 18);
  assert.ok(await acceptProposal(docH.document, docH.userClick));
  assert.equal(p0.querySelector('span.eo-run[data-eo-bold="true"]')?.textContent, "핵심 ");
  assert.equal(p0.textContent, "핵심 매출 120원 달성");

  docH.userClick(docH.assistants().at(-1).querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(8);
  const restoredP0 = docH.editor.querySelector("p");
  assert.equal(restoredP0.textContent, "핵심 매출 100원 달성");
  assert.equal(restoredP0.querySelector('span.eo-run[data-eo-bold="true"]')?.textContent, "핵심 ");

  // Track Changes doc.replace_span records del.eo-rev-del + ins.eo-rev-ins
  docH.editor.dataset.eoTrackChanges = "true";
  restoredP0.textContent = "기존 매출 100원 기록";
  await docH.send("부분 고침", 18);
  assert.ok(await acceptProposal(docH.document, docH.userClick));
  const tcP0 = docH.editor.querySelector("p");
  assert.equal(tcP0.querySelector("del.eo-rev-del")?.textContent, "기존 매출 100원 기록");
  assert.equal(tcP0.querySelector("ins.eo-rev-ins")?.textContent, "기존 매출 120원 기록");
  docH.editor.dataset.eoTrackChanges = "";

  // Ghost prediction sends lightweight: true and omits duplicate selectedText
  docH.userClick(docH.document.getElementById("btn-copilot-ghost"));
  const gp = docH.document.createElement("p");
  const txtNode = docH.document.createTextNode("초안 계속 ");
  gp.appendChild(txtNode);
  docH.editor.replaceChildren(gp);
  const range = docH.document.createRange();
  range.setStart(txtNode, txtNode.data.length);
  range.setEnd(txtNode, txtNode.data.length);
  docH.host.getSelection = () => ({ rangeCount: 1, isCollapsed: true, getRangeAt: () => range });
  docH.editor.dispatchEvent({ type: "input", target: gp });
  docH.advanceTimers(1000);
  await flushAsyncTurns(10);
  const lwReq = seenGhostReqs.find((r) => r.lightweight === true);
  assert.ok(lwReq, "ghost prediction sends lightweight: true");
  assert.equal(lwReq.context.doc.selectedText, undefined, "ghost omits duplicate selectedText");

  // Stable promptCacheKey & history without duplicate content across user turns
  const chatRuns = seenGhostReqs.filter((r) => !r.lightweight);
  assert.ok(chatRuns.length >= 2);
  assert.ok(chatRuns[0].promptCacheKey);
  assert.equal(chatRuns[0].promptCacheKey, chatRuns[1].promptCacheKey);
  assert.equal("content" in (chatRuns[1].history[0] || {}), false);
});

test("SLIDE omits duplicate body/notes/default layout, PDF sends activePage, SHEET includes tabs/usedRange/prioritized selection", async () => {
  // SLIDE
  let slideCtx = null;
  const slideH = await open({
    module: "SLIDE",
    run: (req) => {
      slideCtx = req.context.slide;
      return { status: "ok", mode: "ask", reply: "확인", operations: [] };
    },
  });
  slideH.host.__EASYOFFICE_OFFICE_PRINT__.slideDeckState[0].layout = "titleAndContent";
  slideH.document.getElementById("slide-stage-box").dataset.layout = "titleAndContent";
  await slideH.send("슬라이드 점검");
  assert.equal("body" in slideCtx.slides[0], false, "duplicate body omitted from SLIDE context");
  assert.equal("notes" in slideCtx.slides[0], false, "empty notes omitted");
  assert.equal("layout" in slideCtx.slides[0], false, "default titleAndContent layout omitted");

  slideH.host.__EASYOFFICE_OFFICE_PRINT__.slideDeckState[0].layout = "twoColumn";
  slideH.document.getElementById("slide-stage-box").dataset.layout = "twoColumn";
  slideH.host.__EASYOFFICE_OFFICE_PRINT__.slideDeckState[0].notes = "발표 메모";
  await slideH.send("두 번째 슬라이드 점검");
  assert.equal(slideCtx.slides[0].layout, "twoColumn", "non-default layout preserved");
  assert.equal(slideCtx.slides[0].notes, "발표 메모", "non-empty notes preserved");

  // PDF activePage
  let pdfCtx = null;
  const pdfH = await open({
    module: "PDF",
    run: (req) => {
      pdfCtx = req.context.pdf;
      return { status: "ok", mode: "ask", reply: "[3쪽] 확인", operations: [] };
    },
  });
  pdfH.host.__EASYOFFICE_MODULE_FIND_HOOKS__ = {
    ...(pdfH.host.__EASYOFFICE_MODULE_FIND_HOOKS__ || {}),
    getPdfPages: () => [{ page: 1, text: "첫째" }, { page: 2, text: "둘째" }, { page: 3, text: "셋째" }],
  };
  pdfH.document.getElementById("pdf-page-card").dataset.activePage = "3";
  await pdfH.send("3쪽 요약");
  assert.equal(pdfCtx.activePage, 3);

  // SHEET tabs, usedRange & prioritized selection.range when >512 cells exist
  let sheetCtx = null;
  const sheetH = await open({
    module: "SHEET",
    run: (req) => {
      sheetCtx = req.context.sheet;
      return { status: "ok", mode: "ask", reply: "확인", operations: [] };
    },
  });
  const sc = sheetH.host.__EASYOFFICE_SHEET_CONTROLLER__ || null;
  const store = sheetH.host.__EASYOFFICE_OFFICE_PRINT__ ? null : null;
  // Seed 540 cells via Sheet write API or DOM if sheet controller is present
  const sheetNameBox = sheetH.document.getElementById("active-cell-ref");
  sheetNameBox.textContent = "Z540";
  sheetNameBox.dispatchEvent({ type: "keydown", key: "Enter", target: sheetNameBox });
  await flushAsyncTurns(4);
  const actionsHost = sheetH.host;
  const env = {};
  loadUiModule(UI("sheet-refs.js"), env);
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-diff.js"), env);
  loadUiModule(UI("copilot-typing.js"), env);
  loadUiModule(UI("copilot-cursor.js"), env);
  loadUiModule(UI("copilot-timeline.js"), env);
  loadUiModule(UI("copilot-preview.js"), env);
  loadUiModule(UI("copilot-perform-slide.js"), env);
  loadUiModule(UI("copilot-perform.js"), env);
  loadUiModule(UI("copilot-actions.js"), env);
  const bigMap = {};
  for (let r = 1; r <= 540; r += 1) bigMap["A" + r] = { value: "R" + r, formula: "" };
  const actions = env.__EASYOFFICE_CREATE_COPILOT_ACTIONS__({
    document: sheetH.document,
    sheetDataStore: { S1: bigMap, 요약: {} },
    sheetRowCounts: {},
    slideDeckState: [],
    getActiveModule: () => "SHEET",
    getActiveSheetName: () => "S1",
    getActiveSlideIndex: () => 0,
    getSheetController: () => ({ getSelectionRange: () => ({ r1: 535, r2: 540, c1: 1, c2: 1 }), snapshotDomSheetCells: () => bigMap }),
    getWorkspacePanels: () => ({}),
    getPdfEdit: () => null,
    activatePane: async () => {},
    captureWorkspaceSnapshot: () => ({}),
    restoreWorkspaceSnapshot: async () => {},
    commitDeferredUndoSnapshot: () => {},
    getHistoryMutationEpoch: () => 1,
    noteActivity: () => {},
    extractEditableDocumentText: () => "",
    localScrubPii: () => ({ totalMasked: 0 }),
  });
  const ext = actions.extractContext("SHEET");
  assert.deepEqual(ext.context.sheet.tabs, ["S1", "요약"]);
  assert.equal(ext.context.sheet.usedRange, "A1:A540");
  assert.equal(ext.context.sheet.totalCells, 540);
  assert.ok(ext.context.sheet.cells.slice(0, 6).some((c) => c.coord === "A540"), "selection.range prioritized at head of cells");
});

test("HWP 64 KiB Korean trailing truncation emits omittedParagraphs/null slots without losing blanks, and bridge replace_span enforces unique match", async () => {
  // 1. Over-64 KiB Korean doc via readParagraphRange -> buildHwpContextFromSnapshot preserves blanks and counts trimmed tail
  const paraText = "한글가나다라마바사".repeat(120); // ~3,240 UTF-8 bytes -> ~20 paras exceed 64 KiB
  const docParas = Array.from({ length: 80 }, (_, i) => (i === 1 ? "" : `${i}:${paraText}`));
  const fakeWasm = {
    documentGeneration: 7,
    pageCount: 3,
    getSectionCount: () => 1,
    getParagraphCount: () => docParas.length,
    getParagraphLength: (_s, p) => docParas[p].length,
    getTextRange: (_s, p, start, end) => docParas[p].slice(start, end),
    getControlTextPositions: () => [],
    replaceText: (_s, p, _a, _b, next) => {
      docParas[p] = next;
      fakeWasm.documentGeneration += 1;
      return { ok: true };
    },
    splitParagraph() {},
    deleteParagraph() {},
    insertText() {},
  };
  const fakeInput = {
    getCursorPosition: () => ({ sectionIndex: 0, paragraphIndex: 0, charOffset: 0 }),
    getSelection: () => null,
    executeOperation: ({ operation }) => operation(fakeWasm),
  };

  const range1 = readParagraphRange(fakeWasm, fakeInput, { startIndex: 0, count: 200 });
  assert.equal(range1.truncatedByBytes, true);
  assert.ok(range1.paragraphs.length < docParas.length);

  const env = {};
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-ext-ops.js"), env);
  const extOps = env.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__({
    document: { getElementById: () => null },
    getHwpHost: () => ({
      usesLegacy: () => false,
      getStatus: () => "ready",
      call: async (method, params) => {
        if (method === "readParagraphRange") return readParagraphRange(fakeWasm, fakeInput, params);
        return {};
      },
    }),
  });
  const ctxNoSel = await extOps.extractHwpContextAsync(null);
  assert.equal(ctxNoSel.context.doc.paragraphs[1], "", "genuine blank paragraph stays empty string");
  assert.ok(ctxNoSel.context.doc.omittedParagraphs > 0, `trailing truncated paragraphs counted: ${ctxNoSel.context.doc.omittedParagraphs}`);

  // When target selection is past 64 KiB byte cutoff (index 60), extra window fetches around 60 and emits `null` for unloaded middle
  const ctxSel = await extOps.extractHwpContextAsync({
    selection: { range: "선택", paragraphIndex: 60, paragraphEnd: 60, selectedText: "60" },
  });
  assert.equal(ctxSel.context.doc.paragraphs[1], "");
  assert.ok(ctxSel.context.doc.paragraphs[60].startsWith("60:"), "target beyond 64 KiB cutoff loaded");
  assert.ok(ctxSel.context.doc.paragraphs.includes(null), "unloaded middle slots marked null");

  // 2. HWP bridge replace_span step works uniquely and rejects empty / duplicate / missing search Fail-Closed
  docParas[0] = "첫 문단 고유 구절입니다.";
  docParas[2] = "중복과 중복이 있습니다.";
  const parsed = parseApplyCopilotEditsParams({
    steps: [{ kind: "replace_span", index: 0, search: "고유 구절", replace: "바뀐 구절" }],
  });
  const applied = applyCopilotEdits(fakeWasm, fakeInput, parsed);
  assert.equal(applied.ok, true);
  assert.equal(docParas[0], "첫 문단 바뀐 구절입니다.");

  assert.throws(() => parseApplyCopilotEditsParams({ steps: [{ kind: "replace_span", index: 0, search: "   ", replace: "x" }] }));
  assert.throws(() => applyCopilotEdits(fakeWasm, fakeInput, parseApplyCopilotEditsParams({ steps: [{ kind: "replace_span", index: 2, search: "중복", replace: "x" }] })), /여러 번/);
  assert.throws(() => applyCopilotEdits(fakeWasm, fakeInput, parseApplyCopilotEditsParams({ steps: [{ kind: "replace_span", index: 0, search: "없는구절", replace: "x" }] })), /찾을 수 없/);
});
