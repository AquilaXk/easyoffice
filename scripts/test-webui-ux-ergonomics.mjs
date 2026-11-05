import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

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

test("DOC and HWP module-specific ribbon tools build real DOM structures and direct typing supports Undo/Redo across repeated focus-retained edits", async () => {
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
  docEditor.dispatchEvent({ type: "beforeinput", target: h1 });
  h1.textContent = "직접 타이핑한 한글 제목";
  docEditor.dispatchEvent({ type: "input", target: h1 });

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector("h1").textContent, prevTitle);
  assert.equal(docEditor.classList.contains("manuscript-grid"), true);

  const h1AfterUndo = docEditor.querySelector("h1");
  docEditor.dispatchEvent({ type: "beforeinput", target: h1AfterUndo });
  h1AfterUndo.textContent = "포커스 유지 재타이핑 제목";
  docEditor.dispatchEvent({ type: "input", target: h1AfterUndo });

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(
    docEditor.querySelector("h1").textContent,
    "포커스 유지 재타이핑 제목",
    "re-typing while keeping focus after Undo must record a fresh snapshot and clear Redo stack"
  );

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector("h1").textContent, prevTitle);

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector("h1").textContent, "포커스 유지 재타이핑 제목");
});

test("SHEET supports keyboard arrow/Tab/Delete cell navigation, inline editing key pass-through, sort baseline/blank-row preservation, condfmt, freeze, and autosum actions", async () => {
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

  b2.textContent = "1250";
  b2.dispatchEvent({ type: "input", target: b2 });
  const leftAllowed = b2.dispatchEvent({ type: "keydown", key: "ArrowLeft", target: b2 });
  const rightAllowed = b2.dispatchEvent({ type: "keydown", key: "ArrowRight", target: b2 });
  const delAllowed = b2.dispatchEvent({ type: "keydown", key: "Delete", target: b2 });
  assert.equal(leftAllowed, true);
  assert.equal(rightAllowed, true);
  assert.equal(delAllowed, true);
  assert.equal(document.getElementById("active-cell-ref").textContent, "B2");
  assert.equal(b2.textContent, "1250");
  b2.dispatchEvent({ type: "keydown", key: "Enter", target: b2 });
  await flushAsyncTurns();
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(b2.textContent, "1150");

  b2.focus();
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
  f2.focus();
  userClick(document.querySelector('[data-command-id="sheet-data-sort"]'));
  await flushAsyncTurns();
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B2"]').textContent, "510");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B6"]').textContent, "1150");
  assert.equal(f2.dataset.formula, "=SUM(B2:E2)");
  f2.blur();
  await flushAsyncTurns();
  assert.equal(
    f2.dataset.formula,
    "=SUM(B2:E2)",
    "blurring a focused formula cell after sort must preserve its formula"
  );

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
  const sheetThead = document.querySelector("#pane-sheet table thead");
  assert.equal(sheetThead.classList.contains("frozen-header"), true);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(sheetThead.classList.contains("frozen-header"), false);
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#pane-sheet td.condfmt-high").length, 0);

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.ok(document.querySelectorAll("#pane-sheet td.condfmt-high").length > 0);
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(sheetThead.classList.contains("frozen-header"), true);

  userClick(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]'));
  await flushAsyncTurns();
  assert.equal(sheetThead.classList.contains("frozen-header"), false);
  assert.equal(document.querySelectorAll("#pane-sheet td.condfmt-high").length, 0);

  userClick(document.querySelector('#ribbon-tabs [data-category="formulasAndData"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-command-id="sheet-data-sort"]'));
  await flushAsyncTurns();
  assert.equal(
    document.querySelector('#pane-sheet td[data-cell="A2"]').textContent,
    "기획조정실",
    "blank rows 3..6 on Sheet2 must remain below populated row 2 during sort"
  );

  userClick(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns();
  assert.equal(sheetThead.classList.contains("frozen-header"), true);
  assert.ok(document.querySelectorAll("#pane-sheet td.condfmt-high").length > 0);
});

test("SLIDE object insertion, non-duplicating lower-zone typing, inserted-node PII scrub, presenter mode, and PDF badges work end-to-end", async () => {
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

  lowerZone.focus();
  const lowerP = lowerZone.querySelector("p");
  lowerZone.dispatchEvent({ type: "beforeinput", target: lowerP });
  lowerP.textContent = "수정된 슬라이드 본문 요약";
  lowerZone.dispatchEvent({ type: "input", target: lowerP });

  userClick(document.querySelector('.slide-thumb[data-slide-idx="1"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('.slide-thumb[data-slide-idx="0"]'));
  await flushAsyncTurns();
  assert.equal(
    lowerZone.querySelector("p").textContent,
    "수정된 슬라이드 본문 요약",
    "typing in #slide-lower-zone must not duplicate inserted object text into slide.lower"
  );
  assert.ok(lowerZone.querySelector('[data-slide-object="table"]'));

  const shapeObj = lowerZone.querySelector('[data-slide-object="shape"]');
  shapeObj.textContent = "담당자 연락처 010-5555-7777";
  lowerZone.dispatchEvent({ type: "input", target: shapeObj });
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns();
  assert.doesNotMatch(
    lowerZone.querySelector('[data-slide-object="shape"]').textContent,
    /010-5555-7777/
  );

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

  const utils = sandbox.__EASYOFFICE_SHEET_UTILS__;
  const holder = document.createElement("div");
  const td = document.createElement("td");
  td.rowSpan = 2;
  td.colSpan = 3;
  td.textContent = "병합 셀";
  holder.appendChild(td);
  const specs = utils.serializeDomChildren(holder);
  assert.equal(specs[0].attrs.rowspan, "2");
  assert.equal(specs[0].rowSpan, 2);
  const restoredHolder = document.createElement("div");
  utils.restoreDomChildren(restoredHolder, specs, document);
  assert.equal(restoredHolder.childNodes[0].rowSpan, 2);
  assert.equal(restoredHolder.childNodes[0].getAttribute("rowspan"), "2");

  userClick(document.querySelector('.mod-btn[data-module="SLIDE"]'));
  await flushAsyncTurns();
  lowerZone.querySelector("p").textContent = "추진 배경·목적\n세부 실행 계획";
  lowerZone.dispatchEvent({ type: "input", target: lowerZone.querySelector("p") });
  userClick(document.querySelector('.slide-thumb[data-slide-idx="1"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('.slide-thumb[data-slide-idx="0"]'));
  await flushAsyncTurns();
  assert.match(lowerZone.querySelector("p").textContent, /추진 배경·목적/);

  const catalog = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  assert.equal(catalog.formatPlatformShortcut("⌘B", "Ctrl"), "Ctrl+B");
  assert.equal(catalog.formatPlatformShortcut("⌃⌘C", "Ctrl"), "Ctrl+Alt+C");
  assert.equal(catalog.formatPlatformShortcut("⌃⌘S", "Ctrl"), "Ctrl+Alt+S");
  assert.equal(catalog.formatPlatformShortcut("⌃⌥S", "Ctrl"), "Ctrl+Alt+S");
  assert.equal(catalog.formatPlatformShortcut("⇧⌘P", "Ctrl"), "Shift+Ctrl+P");
  assert.equal(catalog.formatPlatformShortcut("⌘B", "⌘"), "⌘B");
  assert.equal(document.getElementById("shortcut-hint").textContent, "Ctrl+K");

  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns();
  userClick(document.getElementById("btn-stylebar-bold"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("btn-stylebar-bold").getAttribute("aria-pressed"), "true");
  assert.match(document.getElementById("doc-page-zone-indicator").textContent, /1 \/ \d+ 쪽 · 구역: 1 \/ \d+ · 삽입/);

  const docEditor = document.getElementById("doc-editor");
  const styleSelect = document.getElementById("doc-style-select");
  styleSelect.value = "개요 1";
  styleSelect.dispatchEvent({ type: "change" });
  assert.equal(document.getElementById("doc-style-role-badge").textContent, "개요 1");
  assert.equal(docEditor.dataset.styleRole, "개요 1");

  const fontSelect = document.getElementById("doc-font-select");
  fontSelect.value = "함초롬바탕";
  fontSelect.dispatchEvent({ type: "change" });
  assert.equal(document.getElementById("doc-style-font-badge").textContent, "함초롬바탕");
  assert.equal(docEditor.style.fontFamily, "함초롬바탕");

  userClick(document.getElementById("btn-size-up"));
  assert.equal(document.getElementById("doc-font-size-input").value, "12.0");
  assert.equal(document.getElementById("doc-style-size-badge").textContent, "12.0 pt");
  assert.equal(docEditor.style.fontSize, "12.0pt");
  userClick(document.getElementById("btn-size-down"));
  assert.equal(document.getElementById("doc-font-size-input").value, "11.0");

  userClick(document.getElementById("btn-ls-up"));
  assert.equal(document.getElementById("doc-linespacing-select").value, "180");
  assert.equal(docEditor.dataset.lineSpacing, "180");
  userClick(document.getElementById("btn-ls-down"));
  assert.equal(document.getElementById("doc-linespacing-select").value, "160");

  userClick(document.getElementById("btn-align-distribute"));
  assert.equal(document.getElementById("btn-align-distribute").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-align-justify").classList.contains("active"), false);
  assert.equal(docEditor.dataset.textAlign, "distribute");

  userClick(document.getElementById("btn-stylebar-strike"));
  assert.equal(docEditor.dataset.strike, "true");
  userClick(document.getElementById("btn-stylebar-charfx"));
  assert.equal(docEditor.dataset.charfx, "true");
  userClick(document.getElementById("btn-stylebar-textcolor"));
  assert.equal(docEditor.dataset.textcolor, "true");
  userClick(document.getElementById("btn-stylebar-highlight"));
  assert.equal(docEditor.dataset.highlight, "true");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.notEqual(docEditor.dataset.highlight, "true");
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.dataset.highlight, "true");

  userClick(document.getElementById("btn-toggle-doc-grid"));
  assert.equal(docEditor.classList.contains("manuscript-grid"), true);
  assert.equal(
    document.getElementById("hwp-manuscript-preview").classList.contains("manuscript-grid"),
    true
  );

  userClick(document.getElementById("btn-toggle-ctrl-marks"));
  assert.equal(docEditor.dataset.showCtrlMarks, "true");
  assert.equal(docEditor.dataset.showParaMarks, "true");
  userClick(document.getElementById("btn-toggle-para-marks"));
  assert.equal(docEditor.dataset.showParaMarks, "false");

  const fontLang = document.getElementById("doc-font-lang");
  fontLang.value = "한글";
  fontLang.dispatchEvent({ type: "change" });
  assert.equal(docEditor.dataset.fontLang, "한글");

  userClick(document.querySelector('.mod-btn[data-module="HWP"]'));
  await flushAsyncTurns();
  assert.equal(styleSelect.value, "공문서 바탕글");
  assert.ok(
    Array.from(styleSelect.querySelectorAll("option")).some((o) => o.value === "공문서 바탕글")
  );
  assert.equal(fontLang.value, "대표");
  assert.equal(document.getElementById("btn-toggle-ctrl-marks").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-toggle-para-marks").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-toggle-doc-grid").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-align-justify").classList.contains("active"), true);

  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns();
  assert.equal(styleSelect.value, "개요 1");
  assert.equal(fontLang.value, "한글");
  assert.equal(document.getElementById("doc-style-font-badge").textContent, "함초롬바탕");
  assert.equal(document.getElementById("btn-toggle-ctrl-marks").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-toggle-para-marks").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-toggle-doc-grid").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-align-distribute").classList.contains("active"), true);

  assert.equal(document.getElementById("ruler-corner").textContent, "mm");
  assert.ok(document.getElementById("h-ruler").querySelector("#ruler-pin-first-indent"));
  assert.ok(document.getElementById("h-ruler").querySelector("#ruler-pin-left-margin"));
  assert.ok(document.getElementById("h-ruler").querySelector("#ruler-pin-right-margin"));
  assert.equal(document.getElementById("v-ruler").querySelectorAll("span").length, 13);

  const insertBtn = document.getElementById("status-insert-mode");
  assert.equal(insertBtn.textContent, "삽입");
  userClick(insertBtn);
  assert.equal(insertBtn.textContent, "수정");
  assert.match(document.getElementById("doc-page-zone-indicator").textContent, /1 \/ \d+ 쪽 · 구역: 1 \/ \d+ · 수정/);
  sandbox.dispatchWindowKeydown({ key: "Insert" });
  assert.equal(insertBtn.textContent, "삽입");
  assert.match(document.getElementById("doc-page-zone-indicator").textContent, /1 \/ \d+ 쪽 · 구역: 1 \/ \d+ · 삽입/);

  const zoomSlider = document.getElementById("status-zoom-slider");
  zoomSlider.value = "135";
  zoomSlider.dispatchEvent({ type: "input" });
  assert.equal(document.getElementById("status-zoom-label").textContent, "135%");
  userClick(document.getElementById("btn-zoom-fit-width"));
  assert.equal(document.getElementById("status-zoom-label").textContent, "120%");
  assert.equal(zoomSlider.value, "120");
  userClick(document.getElementById("btn-zoom-fit-page"));
  assert.equal(document.getElementById("status-zoom-label").textContent, "100%");
  assert.equal(zoomSlider.value, "100");

  userClick(document.querySelector('.mod-btn[data-module="PDF"]'));
  await flushAsyncTurns();
  userClick(document.getElementById("btn-pdf-rotate"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("pdf-page-card").dataset.rotatedQuarter, "true");
  assert.match(document.getElementById("pdf-form-status").textContent, /회전:\s*90°/);

  const appShell = document.getElementById("app");
  assert.equal(document.getElementById("menu-bar").parentElement, appShell);
  assert.equal(document.getElementById("icon-toolbar").parentElement, appShell);
  assert.equal(document.getElementById("doc-style-bar").parentElement, appShell);

  const editMenuBtn = document.querySelector('.studio-menu-item[data-menu-cat="home"]');
  const editMenuWrap = editMenuBtn.parentElement;
  userClick(editMenuBtn);
  assert.equal(editMenuWrap.classList.contains("open"), true);
  assert.notEqual(appShell.dataset.chromeExpanded, "true");
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  assert.equal(editMenuWrap.classList.contains("open"), false);

  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns();
  const docTableBody = document.getElementById("doc-table-body");
  const initialRowCount = docTableBody.querySelectorAll("tr").length;
  userClick(document.querySelector('[data-studio-menu-cmd="table-add-row"]'));
  await flushAsyncTurns();
  assert.equal(docTableBody.querySelectorAll("tr").length, initialRowCount + 1);

  docEditor.focus();
  const p0 = docEditor.querySelector("p");
  const prevP0Text = p0.textContent;
  docEditor.dispatchEvent({ type: "beforeinput", target: p0 });
  p0.textContent = "표 행 추가 후 본문 타이핑";
  docEditor.dispatchEvent({ type: "input", target: p0 });
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector("p").textContent, prevP0Text);
  assert.equal(
    document.getElementById("doc-table-body").querySelectorAll("tr").length,
    initialRowCount + 1
  );

  userClick(document.querySelector('.mod-btn[data-module="PDF"]'));
  await flushAsyncTurns();
  const rowCountInPdf = document.getElementById("doc-table-body").querySelectorAll("tr").length;
  userClick(document.querySelector('[data-studio-menu-cmd="table-add-row"]'));
  userClick(document.querySelector('[data-studio-menu-cmd="mod-footnote"]'));
  await flushAsyncTurns();
  assert.equal(document.getElementById("doc-table-body").querySelectorAll("tr").length, rowCountInPdf);

  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('[data-studio-menu-cmd="security-audit"]'));
  await flushAsyncTurns();
  assert.match(document.getElementById("status-command-feedback").textContent, /보안|감사/);

  userClick(document.getElementById("btn-command-palette"));
  const paletteModal = document.getElementById("cmd-palette-modal");
  assert.equal(paletteModal.open, true);
  assert.equal(paletteModal.querySelector(".cp-slash")?.textContent, "/");
  const cpItems = paletteModal.querySelectorAll("#cmd-palette-list .cp-item");
  assert.ok(cpItems.length > 0);
  assert.equal(cpItems[0].classList.contains("cp-item--selected"), true);
  assert.equal(cpItems[0].querySelector(".cp-item-label")?.textContent, "저장");
  assert.equal(cpItems[0].querySelector(".cp-item-shortcut")?.textContent, "Ctrl+S");

  const feedbackBeforeModalBold = document.getElementById("status-command-feedback").textContent;
  sandbox.dispatchWindowKeydown({ key: "b", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.getElementById("status-command-feedback").textContent, feedbackBeforeModalBold);

  sandbox.dispatchWindowKeydown({ key: "ArrowDown" });
  const cpItemsAfterDown = paletteModal.querySelectorAll("#cmd-palette-list .cp-item");
  assert.equal(cpItemsAfterDown[0].classList.contains("cp-item--selected"), false);
  assert.equal(cpItemsAfterDown[1].classList.contains("cp-item--selected"), true);

  const cmdInput = document.getElementById("cmd-search-input");
  cmdInput.value = "__non_existent_command_query__";
  cmdInput.dispatchEvent({ type: "input" });
  assert.equal(paletteModal.querySelector("#cmd-palette-list .cp-empty")?.textContent, "검색 결과 없음");
  cmdInput.value = "";
  cmdInput.dispatchEvent({ type: "input" });
  sandbox.dispatchWindowKeydown({ key: "k", ctrlKey: true });
  assert.equal(paletteModal.open, false);
});

