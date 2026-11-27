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

  // Single oversized Korean history/loop entry stays within Rust's 16,384 UTF-8 byte ceiling
  const hugeReply = compact.compactHistory([{ role: "user", text: "질문" }, { role: "assistant", text: "한".repeat(7000) }], 12);
  assert.ok(Buffer.byteLength(hugeReply[1].text, "utf8") <= 16384, "history item capped at 16 KiB UTF-8");
  const cappedSteps = [];
  lc.pushStepPair(cappedSteps, "답".repeat(7000), "결".repeat(7000));
  assert.ok(Buffer.byteLength(cappedSteps[0].text, "utf8") <= 16384);
  assert.ok(Buffer.byteLength(cappedSteps[1].text, "utf8") <= 16384);
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
  let pdfDelta = null;
  const pdfH = await open({
    module: "PDF",
    run: (req) => {
      pdfCtx = req.context.pdf;
      pdfDelta = req.contextDelta || null;
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

  // >24,000-char 50-page PDF preserves activePage=45 window, ascending order, and [1..N쪽 생략] front notice
  const fiftyPages = Array.from({ length: 50 }, (_, i) => ({
    pageNumber: i + 1,
    text: `PAGE_${i + 1}_BODY ` + "가나다라마바사아자차카타파하 ".repeat(55),
  }));
  pdfH.host.__EASYOFFICE_MODULE_FIND_HOOKS__ = {
    ...(pdfH.host.__EASYOFFICE_MODULE_FIND_HOOKS__ || {}),
    getPdfPages: () => fiftyPages,
  };
  pdfH.document.getElementById("pdf-page-card").dataset.activePage = "45";
  await pdfH.send("45쪽 핵심 요약");
  assert.equal(pdfCtx.pageCount, 50);
  assert.equal(pdfCtx.activePage, 45);
  assert.ok(Array.from(pdfCtx.text).length <= 24000, `PDF context stays within 24,000 chars: ${pdfCtx.text.length}`);
  assert.match(pdfCtx.text, /^\[1\.\.\d+쪽 생략\]\n\n/, "front omit header preserved when earlier pages skipped");
  assert.ok(pdfCtx.text.includes("[45쪽] PAGE_45_BODY"), "activePage=45 preserved inside req.context.pdf.text");
  assert.ok(pdfCtx.text.includes("[44쪽] PAGE_44_BODY") && pdfCtx.text.includes("[46쪽] PAGE_46_BODY"), "adjacent pages around activePage included");
  const pageMarkers = [...pdfCtx.text.matchAll(/\[(\d+)쪽\]/g)].map((m) => Number(m[1]));
  assert.ok(pageMarkers.length >= 5, "multiple pages included in window");
  for (let i = 1; i < pageMarkers.length; i += 1) {
    assert.ok(pageMarkers[i] > pageMarkers[i - 1], `page numbers strictly ascending: ${pageMarkers[i - 1]} -> ${pageMarkers[i]}`);
  }

  // PD-369: scrolling 45 -> 1 in same PDF session keeps anchor pinned (anchorPage=45) and sends page 1 excerpt in contextDelta; 새 대화 rotates and re-anchors
  pdfH.document.getElementById("pdf-page-card").dataset.activePage = "1";
  await pdfH.send("1쪽 확인");
  assert.equal(pdfCtx.anchorPage, 45, "scrolling within same session keeps PDF anchor window pinned");
  assert.equal(pdfCtx.activePage, 1);
  assert.match(pdfDelta?.summary || "", /^\[1쪽\] PAGE_1_BODY/, "page 1 excerpt sent in contextDelta");

  pdfH.document.getElementById("btn-copilot-new").click();
  await pdfH.send("1쪽 새 대화 확인");
  assert.match(pdfCtx.text, /^\[1쪽\] PAGE_1_BODY/);
  assert.doesNotMatch(pdfCtx.text, /쪽 생략\]/);

  // Oversized single activePage >24,000 chars keeps front omit header and [45쪽] prefix within 24,000 chars
  const hugeSinglePages = Array.from({ length: 50 }, (_, i) => ({
    page: i + 1,
    text: i === 44 ? "HUGE45_" + "한".repeat(25000) : `짧은 ${i + 1}`,
  }));
  pdfH.host.__EASYOFFICE_MODULE_FIND_HOOKS__ = {
    ...(pdfH.host.__EASYOFFICE_MODULE_FIND_HOOKS__ || {}),
    getPdfPages: () => hugeSinglePages,
  };
  pdfH.document.getElementById("pdf-page-card").dataset.activePage = "45";
  await pdfH.send("긴 45쪽 확인");
  assert.ok(Array.from(pdfCtx.text).length <= 24000);
  assert.ok(pdfCtx.text.startsWith("[1..44쪽 생략]\n\n[45쪽] HUGE45_"));

  // Single-page DOM fallback uses clamped [activePage쪽] instead of hardcoded [1쪽] and respects 24,000 char ceiling
  const extPdfEnv = {};
  loadUiModule(UI("copilot-text.js"), extPdfEnv);
  loadUiModule(UI("copilot-ext-ops.js"), extPdfEnv);
  pdfH.document.getElementById("pdf-page-card").dataset.activePage = "6";
  const pdfFallbackOps = extPdfEnv.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__({
    document: pdfH.document,
    getHwpHost: () => null,
    getPdfEdit: () => ({ pageCount: () => 8 }),
    extractEditableDocumentText: () => "가".repeat(25000),
  });
  const fbCtx = pdfFallbackOps.extractPdfContext();
  assert.equal(fbCtx.pageCount, 8);
  assert.equal(fbCtx.activePage, 6);
  assert.ok(fbCtx.text.startsWith("[6쪽] "), "single-page fallback formats actual activePage");
  assert.ok(Array.from(fbCtx.text).length <= 24000, "single-page fallback fits within 24,000 chars");

  // Negative/invalid dataset.activePage falls back to getPdfCurrentPage hook
  pdfH.document.getElementById("pdf-page-card").dataset.activePage = "-3";
  pdfH.host.__EASYOFFICE_MODULE_FIND_HOOKS__ = {
    ...(pdfH.host.__EASYOFFICE_MODULE_FIND_HOOKS__ || {}),
    getPdfPages: () => fiftyPages,
    getPdfCurrentPage: () => 45,
  };
  await pdfH.send("음수 쪽 fallback 확인");
  assert.equal(pdfCtx.activePage, 45);
  assert.ok(pdfCtx.text.includes("[45쪽] PAGE_45_BODY"));

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
  loadUiModule(UI("copilot-edit-mode.js"), env);
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

test("regression fixes: live UI chunk eviction across 9 turns, <del> active span replace, cutOperations stop, agent-loop clone check, and HWP target fallback", async () => {
  // 1. Live copilot-ui history builds chunk-evicted prefix preserving Turn 8 -> Turn 9 shared prefix
  const histories = [];
  const uiH = await open({
    module: "DOC",
    run: (req) => {
      if (!req.lightweight) histories.push(req.history || []);
      return { status: "ok", mode: "ask", reply: "응답 " + req.prompt, operations: [] };
    },
  });
  for (let t = 1; t <= 11; t += 1) {
    await uiH.send("질문 " + t);
  }
  assert.equal(histories.length, 11);
  assert.equal(histories[8].length, 16, "Turn 9 carries 8 completed turns (16 items)");
  assert.equal(histories[9].length, 10, "Turn 10 evicts one 8-item chunk (18 -> 10)");
  assert.equal(histories[9][0].text, "질문 5");
  assert.equal(histories[10].length, 12, "Turn 11 appends turn 10 onto exact Turn 10 prefix");
  assert.deepEqual(histories[10].slice(0, 10), histories[9], "Turn 10 & Turn 11 share exact 10-item cache prefix");

  // 2. <del data-eo-rev-id> and <sup data-eo-note-kind> excluded in replaceSpanWithTrackChanges & preview
  const docHtml = uiH.host.__EASYOFFICE_COPILOT_DOC_HTML__;
  const preview = uiH.host.__EASYOFFICE_CREATE_COPILOT_PREVIEW__({
    document: uiH.document,
    win: uiH.host,
    docBlocksOf: docHtml.docBlocksOf,
    slideDeckState: [],
    sheetDataStore: {},
    getActiveSheetName: () => "S1",
  });
  const editor = uiH.document.getElementById("doc-editor");
  const p = uiH.document.createElement("p");
  const del = uiH.document.createElement("del");
  del.dataset.eoRevId = "1";
  del.textContent = "매출 100";
  const sup = uiH.document.createElement("sup");
  sup.dataset.eoNoteKind = "fn";
  sup.textContent = "1";
  p.appendChild(del);
  p.appendChild(uiH.document.createTextNode("현행 매출 100"));
  p.appendChild(sup);
  editor.replaceChildren(p);

  assert.equal(docHtml.plainActiveTextOfBlock(p), "현행 매출 100");
  const prevChanges = preview.buildChanges("DOC", [
    { kind: "doc.replace_span", index: 0, search: "매출 100", replace: "<strong>매출 200</strong>" },
  ]);
  assert.equal(prevChanges.entries.length, 1);
  assert.equal(prevChanges.entries[0].segments.filter((s) => s.kind === "equal" || s.kind === "insert").map((s) => s.text).join(""), "현행 매출 200");

  docHtml.replaceSpanWithTrackChanges(uiH.document, editor, p, "매출 100", "매출 200");
  assert.equal(del.textContent, "매출 100", "deleted revision node left untouched");
  assert.equal(docHtml.plainActiveTextOfBlock(p), "현행 매출 200");

  // 3. Performer collectTargets + cutOperations on doc.replace_span preserves only typed prefix of replace
  p.textContent = "안녕 세계 끝";
  const perfEnv = { ...uiH.host };
  loadUiModule(UI("copilot-timeline.js"), perfEnv);
  loadUiModule(UI("copilot-perform-slide.js"), perfEnv);
  loadUiModule(UI("copilot-perform.js"), perfEnv);
  const performer = perfEnv.__EASYOFFICE_CREATE_COPILOT_PERFORMER__({
    document: uiH.document,
    win: perfEnv,
    docBlocksOf: docHtml.docBlocksOf,
    slideDeckState: [],
    getActiveSlideIndex: () => 0,
    getActiveSheetName: () => "S1",
    blockText: (s) => String(s ?? ""),
    isReducedMotion: () => false,
    createCursor: () => ({ timing: {}, show() {}, hide() {}, destroy() {}, place() {}, setSelection() {}, setCaret() {}, setIdle() {}, moveTo: async () => true, press() {} }),
    createTypist: () => ({ type: async () => ({ typed: 0 }), stop() {} }),
  });
  const spanOps = [{ kind: "doc.replace_span", index: 0, search: "세계", replace: "지구별" }];
  const targets = performer.collectTargets("DOC", spanOps);
  assert.equal(targets[0].before, "안녕 ");
  assert.equal(targets[0].after, " 끝");
  assert.equal(targets[0].text, "지구별");

  const midCut = performer.cutOperations(spanOps, targets, { target: targets[0], typed: 2 });
  assert.equal(midCut.length, 1);
  assert.equal(midCut[0].replace, "지구", "mid-typing cut keeps prefix of replace only, not paragraph prefix");

  const zeroCut = performer.cutOperations(spanOps, targets, { target: targets[0], typed: 0 });
  assert.equal(zeroCut.length, 0, "0 typed before drag drops unstarted replace_span");

  const unstartedCut = performer.cutOperations(spanOps, targets, { target: targets[0], typed: 0, unstarted: true });
  assert.equal(unstartedCut.length, 0, "unstarted stop drops replace_span");

  // 4. Agent loop verifyProposedOperations checks doc.replace_span against live clone and triggers self-correction
  let verifyTurn = 0;
  const verifyReqs = [];
  const loopH = await open({
    module: "DOC",
    run: (req) => {
      if (req.lightweight) return { status: "ok", mode: "ask", reply: "", operations: [] };
      verifyReqs.push(req);
      verifyTurn += 1;
      if (verifyTurn === 1) {
        return answer([{ kind: "doc.replace_span", index: 0, search: "존재하지않는구문", replace: "x" }]);
      }
      return answer([{ kind: "doc.replace_span", index: 0, search: "문서 본문", replace: "고친 본문" }]);
    },
  });
  const loopP = loopH.document.createElement("p");
  loopP.textContent = "문서 본문입니다.";
  loopH.editor.replaceChildren(loopP);
  await loopH.send("부분 치환", 24);
  assert.equal(verifyTurn, 2, "invalid doc.replace_span triggered verification retry in agent loop");
  assert.ok(Array.isArray(verifyReqs[1].loopSteps) && verifyReqs[1].loopSteps.length >= 2, "retry placed in loopSteps");

  // 5. HWP refreshHwpSnapshot falls back directly to targetStart when surrounding window stops before targetStart
  const hugePara = "가나다".repeat(12000); // > 64 KiB across two paragraphs
  const hwpParas = Array.from({ length: 25 }, (_, i) => (i === 20 ? "TARGET_AT_20" : `${i}:${hugePara.slice(0, 6000)}`));
  const extEnv = {};
  loadUiModule(UI("copilot-text.js"), extEnv);
  loadUiModule(UI("copilot-ext-ops.js"), extEnv);
  const seenCalls = [];
  const extOps = extEnv.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__({
    document: { getElementById: () => null },
    getHwpHost: () => ({
      usesLegacy: () => false,
      getStatus: () => "ready",
      call: async (method, params) => {
        seenCalls.push(params.startIndex);
        if (method !== "readParagraphRange") return {};
        const paras = [];
        let bytes = 0;
        const end = Math.min(hwpParas.length, params.startIndex + params.count);
        let truncatedByBytes = false;
        for (let idx = params.startIndex; idx < end; idx += 1) {
          const text = hwpParas[idx];
          const len = Buffer.byteLength(text, "utf8");
          if (paras.length > 0 && bytes + len > 64 * 1024) {
            truncatedByBytes = true;
            break;
          }
          bytes += len;
          paras.push({ index: idx, text });
        }
        return {
          documentGeneration: 1,
          pageCount: 2,
          totalParagraphs: hwpParas.length,
          truncatedByBytes,
          selection: { hasSelection: true, text: "TARGET_AT_20", startParagraphIndex: 20, endParagraphIndex: 20 },
          paragraphs: paras,
        };
      },
    }),
  });
  const hwpRes = await extOps.extractHwpContextAsync(null);
  assert.ok(seenCalls.includes(20), `direct fallback fetched targetStart=20: calls=${JSON.stringify(seenCalls)}`);
  assert.equal(hwpRes.context.doc.paragraphs[20], "TARGET_AT_20");

  // 6. Agent loop read_doc_blocks Korean output >16 KiB UTF-8 is clipped to stay within Rust MAX_HISTORY_ITEM_BYTES (16,384 B)
  let readTurn = 0;
  let capturedTurn2 = null;
  const readLoopEnv = {};
  loadUiModule(UI("copilot-text.js"), readLoopEnv);
  loadUiModule(UI("copilot-context-compact.js"), readLoopEnv);
  loadUiModule(UI("copilot-agent-loop.js"), readLoopEnv);
  const readLoop = readLoopEnv.__EASYOFFICE_CREATE_COPILOT_AGENT_LOOP__({
    invokeIpc: async (_cmd, payload) => {
      readTurn += 1;
      if (readTurn === 1) {
        return { status: "ok", reply: "", toolCalls: [{ name: "read_doc_blocks", startIndex: 0, endIndex: 10 }], operations: [] };
      }
      capturedTurn2 = JSON.parse(payload);
      return { status: "ok", reply: "완료", operations: [] };
    },
    canStreamIpc: () => false,
    copilotActions: {
      executeReadTool: async () => ({ totalParagraphs: 10, blocks: [{ index: 0, text: "가".repeat(7000) }] }),
    },
  });
  await readLoop.runAgentTurn({
    baseReq: { action: "copilot_run", requestId: "r-utf8-tool", mode: "ask", module: "DOC", prompt: "요약", history: [], context: {}, provider: { kind: "ollama", model: "qwen3" } },
    newSubRequestId: (i) => `r-utf8-tool-s${i}`,
    isCancelled: () => false,
  });
  assert.ok(capturedTurn2 && Array.isArray(capturedTurn2.loopSteps));
  assert.ok(
    Buffer.byteLength(capturedTurn2.loopSteps[1].text, "utf8") <= 16384,
    `loopSteps[1] stays <= 16384 UTF-8 bytes: ${Buffer.byteLength(capturedTurn2.loopSteps[1].text, "utf8")}`,
  );
});
