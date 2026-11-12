import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { addDemoHeading, demoHtml, openDemoDeck, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

test("design tokens, ARIA tab roles, zoom controls, split compare pane, and format filters stay synchronized", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();

  for (const id of ["ribbon-tabs", "ribbon-tool-strip", "contextual-tool-badge", "btn-ribbon-style", "btn-chrome-toggle"]) {
    assert.equal(document.getElementById(id), null, `${id}: the ribbon UI is gone`);
  }
  assert.equal(document.querySelector(".qat-btn"), null, "no quick-access toolbar");
  assert.ok(document.querySelector('[role="menubar"]'), "the menu bar is the command surface");

  assert.equal(document.getElementById("tab-strip"), null, "a window has no document tab strip");

  for (const mod of ["HWP", "SLIDE", "PDF", "DOC"]) switchModule(document, mod);
  await flushAsyncTurns(4);

  const zoomLabel = document.getElementById("status-zoom-label");
  const stage = document.querySelector(".canvas-stage");
  assert.equal(zoomLabel.textContent, "100%");
  userClick(document.getElementById("btn-zoom-in"));
  assert.equal(zoomLabel.textContent, "125%");
  assert.equal(stage.style.zoom, "125%");
  userClick(document.getElementById("btn-zoom-out"));
  assert.equal(zoomLabel.textContent, "100%");
  assert.equal(stage.style.zoom, "100%");

  const splitBtn = commandItem(document, "document-view-split");
  userClick(splitBtn);
  await flushAsyncTurns();
  assert.equal(stage.dataset.splitView, "true");
  assert.ok(stage.querySelector(".split-compare-pane"));

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.ok(!stage.querySelector(".split-compare-pane"));

  assert.equal(document.getElementById("format-filters"), null, "format chips that switched modules are gone");
});

test("DOC and HWP module-specific ribbon tools build real DOM structures and direct typing supports Undo/Redo across repeated focus-retained edits", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness({ transformHtml: demoHtml });
  const docEditor = document.getElementById("doc-editor");

  const sumCell = document.getElementById("doc-table-sum-cell");
  userClick(document.getElementById("btn-doc-eval-table"));
  assert.equal(sumCell.textContent, "1000");
  assert.equal(sumCell.dataset.docAvg, "500");
  assert.equal(sumCell.dataset.docCount, "2");
  assert.match(document.getElementById("status-command-feedback").textContent, /평균\(AVERAGE\): 500/);

  const mergePara = document.createElement("p");
  mergePara.id = "mailmerge-target-line";
  mergePara.textContent = "수신: {{기관명}} {{성명}} ({{일자}})";
  docEditor.appendChild(mergePara);
  docEditor.dispatchEvent({ type: "input", target: mergePara });

  userClick(commandItem(document, "doc-mod-toc"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("doc-toc-dialog").open, true, "the command asks for the levels first");
  document.getElementById("btn-doc-toc-apply").click();
  await flushAsyncTurns();
  const tocNav = docEditor.querySelector(".eo-doc-toc");
  assert.ok(tocNav);
  assert.ok(tocNav.querySelectorAll("p[data-eo-toc]").length >= 3, "the title and the entries of the demo headings");

  userClick(commandItem(document, "doc-mod-footnote"));
  await flushAsyncTurns();
  assert.ok(docEditor.querySelector(".eo-doc-footnote"));

  userClick(commandItem(document, "doc-mod-mailmerge"));
  await flushAsyncTurns();
  assert.equal(
    document.getElementById("mailmerge-target-line").textContent,
    "수신: 행정안전부 이수현 (2026-10-25)"
  );
  assert.ok(docEditor.querySelector(".eo-doc-mailmerge"));

  switchModule(document, "HWP");
  await flushAsyncTurns();
  addDemoHeading(document);
  userClick(commandItem(document, "hwp-mod-header"));
  await flushAsyncTurns();
  assert.ok(docEditor.querySelector(".eo-hwp-header"));

  userClick(commandItem(document, "hwp-mod-field"));
  await flushAsyncTurns();
  assert.ok(docEditor.querySelector(".eo-hwp-field"));

  userClick(commandItem(document, "hwp-mod-manuscript"));
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

test("SHEET supports keyboard arrow/Tab/Delete cell navigation, inline editing key pass-through, sort baseline/blank-row preservation, rule dialogs, freeze, and autosum actions", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  await openDemoWorkbook(document);
  switchModule(document, "SHEET");
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

  await flushAsyncTurns();
  f2.focus();
  userClick(commandItem(document, "sheet-data-sort"));
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
  userClick(commandItem(document, "sheet-data-autosum"));
  await flushAsyncTurns();
  assert.equal(b9.dataset.formula, "=SUM(B2:B6)");
  assert.equal(b9.textContent, "4100");

  await flushAsyncTurns();
  userClick(commandItem(document, "sheet-mod-pivot"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("sheet-pivot-dialog").open, true);

  userClick(commandItem(document, "sheet-mod-chart"));
  await flushAsyncTurns();
  assert.match(document.getElementById("sheet-eval-result").textContent, /차트 데이터 요약 · 합계/);

  userClick(commandItem(document, "sheet-mod-condfmt"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("sheet-cf-dialog").open, true);

  userClick(commandItem(document, "sheet-mod-freeze"));
  await flushAsyncTurns();
  const sheetThead = document.querySelector("#pane-sheet table thead");
  assert.equal(sheetThead.classList.contains("frozen-header"), true);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(sheetThead.classList.contains("frozen-header"), false);
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(sheetThead.classList.contains("frozen-header"), true);

  userClick(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]'));
  await flushAsyncTurns();
  assert.equal(sheetThead.classList.contains("frozen-header"), false);

  await flushAsyncTurns();
  userClick(commandItem(document, "sheet-data-sort"));
  await flushAsyncTurns();
  assert.equal(
    document.querySelector('#pane-sheet td[data-cell="A2"]').textContent,
    "기획조정실",
    "blank rows 3..6 on Sheet2 must remain below populated row 2 during sort"
  );

  userClick(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns();
  assert.equal(sheetThead.classList.contains("frozen-header"), true);
});

test("SLIDE object insertion, non-duplicating lower-zone typing, inserted-node PII scrub, presenter mode, and PDF badges work end-to-end", async () => {
  const docSelection = { _range: null, get rangeCount() { return this._range ? 1 : 0; }, getRangeAt() { return this._range; }, removeAllRanges() { this._range = null; }, addRange(r) { this._range = r; } };
  const { document, sandbox, userClick } = await createWorkspaceHarness({ selection: docSelection, transformHtml: demoHtml });

  await openDemoDeck(document);
  switchModule(document, "SLIDE");
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 3);

  await flushAsyncTurns();
  userClick(commandItem(document, "slide-insert-table"));
  await flushAsyncTurns();
  userClick(commandItem(document, "slide-insert-chart"));
  await flushAsyncTurns();
  userClick(commandItem(document, "slide-insert-shape"));
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

  await flushAsyncTurns();
  userClick(commandItem(document, "slide-mod-presenter"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("slide-stage-box").dataset.presenterMode, "true");
  assert.equal(document.getElementById("slide-laser-dot").hidden, false);

  userClick(document.getElementById("btn-smart-diagnose"));
  await flushAsyncTurns();
  assert.match(document.getElementById("smart-summary-output").textContent, /슬라이드 차트 개체|발표 자료/);

  switchModule(document, "PDF");
  await flushAsyncTurns();
  await flushAsyncTurns();
  userClick(commandItem(document, "pdf-mod-highlight"));
  await flushAsyncTurns();
  assert.ok(!document.getElementById("pdf-page-card").querySelector(".pdf-highlight-badge"), "the command asks first");
  document.getElementById("pdf-highlight-text").value = "핵심";
  userClick(document.getElementById("btn-pdf-highlight-apply"));
  await flushAsyncTurns();
  userClick(commandItem(document, "pdf-mod-splitmerge"));
  await flushAsyncTurns();

  const pdfCard = document.getElementById("pdf-page-card");
  assert.equal(pdfCard.querySelector(".pdf-highlight-badge").textContent, "형광펜: '핵심'");
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

  switchModule(document, "SLIDE");
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

  switchModule(document, "DOC");
  await flushAsyncTurns();
  const boldText = document.querySelector("#doc-editor .eo-h-title").childNodes[0];
  docSelection.addRange({ startContainer: boldText, startOffset: 1, endContainer: boldText, endOffset: 4, collapsed: false });
  userClick(document.getElementById("btn-stylebar-bold"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("btn-stylebar-bold").getAttribute("aria-pressed"), "true");
  assert.match(document.getElementById("doc-page-zone-indicator").textContent, /1 \/ \d+ 쪽 · 구역: 1 \/ \d+ · 삽입/);

  const docEditor = document.getElementById("doc-editor");
  const styleSelect = document.getElementById("doc-style-select");
  styleSelect.value = "개요 1";
  styleSelect.dispatchEvent({ type: "change" });
  assert.equal(document.getElementById("doc-style-role-badge").textContent, "개요 1");
  assert.equal(docEditor.querySelector(".eo-h-title").closest("h1").tagName, "H1", "the selected paragraph is an outline-1 heading");

  const fontSelect = document.getElementById("doc-font-select");
  fontSelect.value = "함초롬바탕";
  fontSelect.dispatchEvent({ type: "change" });
  assert.equal(document.getElementById("doc-style-font-badge").textContent, "함초롬바탕");
  assert.equal(docEditor.style.fontFamily, "", "font applies to the selection only, not the whole document");
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoFont, "함초롬바탕");

  userClick(document.getElementById("btn-size-up"));
  assert.equal(document.getElementById("doc-font-size-input").value, "12.0");
  assert.equal(document.getElementById("doc-style-size-badge").textContent, "12.0 pt");
  assert.equal(docEditor.style.fontSize, "", "size applies to the selection only, not the whole document");
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoSize, "24");
  userClick(document.getElementById("btn-size-down"));
  assert.equal(document.getElementById("doc-font-size-input").value, "11.0");

  userClick(document.getElementById("btn-ls-up"));
  assert.equal(document.getElementById("doc-linespacing-select").value, "180");
  const selectedBlock = docEditor.querySelector(".eo-h-title").closest("h1");
  assert.equal(selectedBlock.style.lineHeight, "1.80", "line spacing is set on the selected paragraph");
  assert.equal(docEditor.style.lineHeight, "");
  userClick(document.getElementById("btn-ls-down"));
  assert.equal(document.getElementById("doc-linespacing-select").value, "160");

  userClick(document.getElementById("btn-align-distribute"));
  assert.equal(document.getElementById("btn-align-distribute").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-align-justify").classList.contains("active"), false);
  assert.equal(selectedBlock.dataset.eoAlign, "distribute");
  assert.equal(selectedBlock.style.textAlign, "justify", "distributed text is saved as justified");

  userClick(document.getElementById("btn-stylebar-strike"));
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoStrike, "true");
  userClick(document.getElementById("btn-stylebar-textcolor"));
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoColor, "FF0000");
  userClick(document.getElementById("btn-stylebar-highlight"));
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoHighlight, "FFFF00");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector(".eo-run")?.dataset.eoHighlight, undefined, "undo removes the highlight run property");
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoHighlight, "FFFF00", "redo restores it");

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

  switchModule(document, "HWP");
  await flushAsyncTurns();
  assert.equal(styleSelect.value, "공문서 바탕글");
  assert.ok(
    Array.from(styleSelect.querySelectorAll("option")).some((o) => o.value === "공문서 바탕글")
  );
  assert.equal(document.getElementById("btn-toggle-ctrl-marks").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-toggle-para-marks").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-toggle-doc-grid").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-align-justify").classList.contains("active"), true);

  switchModule(document, "DOC");
  await flushAsyncTurns();
  assert.equal(styleSelect.value, "바탕글", "without a selection the box shows the document default");
  assert.equal(document.getElementById("doc-style-font-badge").textContent, "휴먼명조", "the font box shows the document default until a selection carries a run font");
  assert.equal(document.getElementById("btn-toggle-ctrl-marks").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-toggle-para-marks").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-toggle-doc-grid").classList.contains("active"), true);
  assert.equal(docEditor.querySelector('[data-eo-align="distribute"]').tagName, "H1", "the paragraph's alignment survives a module switch");

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

  switchModule(document, "PDF");
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

  switchModule(document, "DOC");
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

  switchModule(document, "PDF");
  await flushAsyncTurns();
  const rowCountInPdf = document.getElementById("doc-table-body").querySelectorAll("tr").length;
  userClick(document.querySelector('[data-studio-menu-cmd="table-add-row"]'));
  userClick(document.querySelector('[data-studio-menu-cmd="mod-footnote"]'));
  await flushAsyncTurns();
  assert.equal(document.getElementById("doc-table-body").querySelectorAll("tr").length, rowCountInPdf);

  switchModule(document, "DOC");
  await flushAsyncTurns();
  userClick(document.querySelector('[data-studio-menu-cmd="security-audit"]'));
  await flushAsyncTurns();
  assert.match(document.getElementById("status-command-feedback").textContent, /보안|감사/);

  userClick(document.getElementById("btn-command-palette"));
  const paletteModal = document.getElementById("cmd-palette-modal");
  assert.equal(paletteModal.open, true);
  assert.equal(paletteModal.querySelector(".cp-slash")?.dataset.icon, "search", "the search icon replaces the slash glyph");
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

