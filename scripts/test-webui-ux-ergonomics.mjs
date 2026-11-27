import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem, commandItem, putCaretInFirstTable } from "./test-support/modules.mjs";
import { addDemoHeading, demoHtml, openDemoDeck, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { createSelection } from "./test-support/doc-run-harness.mjs";

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

  const pressEsc = (dlg) => document.dispatchEvent({ type: "keydown", key: "Escape", target: dlg, preventDefault() {}, stopPropagation() {} });
  userClick(commandItem(document, "doc-mod-mailmerge"));
  await flushAsyncTurns();
  const mmDlg = document.getElementById("doc-mailmerge-dialog");
  assert.equal(mmDlg.open, true, "mail merge opens field input dialog instead of hardcoding values");
  pressEsc(mmDlg);
  assert.equal(mmDlg.open, false);
  assert.equal(document.getElementById("mailmerge-target-line").textContent, "수신: {{기관명}} {{성명}} ({{일자}})");
  assert.equal(docEditor.querySelector(".eo-doc-mailmerge"), null);

  userClick(commandItem(document, "doc-mod-mailmerge"));
  await flushAsyncTurns();
  const mmRows = Array.from(mmDlg.querySelectorAll(".doc-mailmerge-row"));
  const mmInputs = Object.fromEntries(mmRows.map((r) => [r.querySelector(".doc-mm-key").value, r.querySelector(".doc-mm-val")]));
  mmInputs["기관명"].value = "기획예산처";
  mmInputs["성명"].value = "홍길동";
  mmInputs["일자"].value = "2026-11-25";
  userClick(document.getElementById("btn-doc-mailmerge-apply"));
  await flushAsyncTurns(6);
  assert.equal(mmDlg.open, false);
  assert.equal(document.getElementById("mailmerge-target-line").textContent, "수신: 기획예산처 홍길동 (2026-11-25)");
  assert.equal(docEditor.querySelector(".eo-doc-mailmerge"), null, "no dummy mail merge banner inserted");
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(6);
  assert.equal(document.getElementById("mailmerge-target-line").textContent, "수신: {{기관명}} {{성명}} ({{일자}})");

  userClick(commandItem(document, "document-insert-shape"));
  await flushAsyncTurns();
  const shapeDlg = document.getElementById("doc-shape-dialog");
  assert.equal(shapeDlg.open, true);
  const fillToggle = document.querySelector("#doc-shape-fill-host [data-color-toggle]");
  const fillPalette = document.getElementById("doc-shape-fill-palette");
  userClick(fillToggle);
  assert.equal(fillPalette.hidden, false);
  const redSwatch = Array.from(fillPalette.querySelectorAll(".sb-hl-swatch")).find((s) => s.getAttribute("aria-label") === "#ff0000");
  userClick(redSwatch);
  assert.equal(fillPalette.hidden, true);
  assert.equal(document.getElementById("doc-shape-fill").value, "#ff0000");
  assert.equal(document.querySelector("#doc-shape-fill-host .dialog-swatch-chip").style.getPropertyValue("--dialog-swatch"), "#ff0000");
  document.getElementById("doc-shape-text").value = "검토 도형";
  userClick(document.getElementById("btn-doc-shape-ok"));
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector(".eo-doc-shape svg") !== null, true);
  assert.equal(docEditor.querySelector(".eo-doc-shape svg")?.firstChild?.getAttribute?.("fill"), "#ff0000");

  userClick(commandItem(document, "document-insert-chart"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("doc-chart-dialog").open, true);
  document.getElementById("doc-chart-title-input").value = "분기별 추이";
  userClick(document.getElementById("btn-doc-chart-ok"));
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector(".eo-doc-chart") !== null, true);
  assert.equal(document.getElementById("status-command-feedback").textContent, "차트 삽입 완료");

  userClick(commandItem(document, "document-review-stats"));
  await flushAsyncTurns(6);
  const statsDlg = document.getElementById("doc-stats-dialog");
  assert.equal(statsDlg.open, true, "review-stats opens document statistics dialog");
  assert.match(document.getElementById("doc-stats-status").textContent, /공백 포함 \d+자/);
  pressEsc(statsDlg);
  assert.equal(statsDlg.open, false);

  userClick(commandItem(document, "document-home-style"));
  await flushAsyncTurns();
  assert.notEqual(document.getElementById("doc-style-select").value, "");

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
  const { document, sandbox, userClick, host } = await createWorkspaceHarness();
  const frozen = (name) => host.__EASYOFFICE_SHEET_LAYOUT__.freezeOf(name);
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

  userClick(commandItem(document, "sheet-insert-chart"));
  await flushAsyncTurns();
  assert.match(document.getElementById("sheet-eval-result").textContent, /차트 데이터 요약 · 합계/);

  userClick(commandItem(document, "sheet-mod-condfmt"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("sheet-cf-dialog").open, true);

  // The freeze command pins the rows above and the columns left of the selected cell (B2 here).
  document.querySelector('#pane-sheet td[data-cell="B2"]').focus();
  userClick(commandItem(document, "sheet-mod-freeze"));
  await flushAsyncTurns();
  assert.deepEqual(frozen("Sheet1"), { rows: 1, cols: 1 });

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(frozen("Sheet1"), null);
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.deepEqual(frozen("Sheet1"), { rows: 1, cols: 1 });

  userClick(document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]'));
  await flushAsyncTurns();
  assert.equal(frozen("Sheet2"), null);

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
  assert.deepEqual(frozen("Sheet1"), { rows: 1, cols: 1 }, "each sheet keeps its own panes");
});

test("SLIDE object insertion, non-duplicating lower-zone typing, inserted-node PII scrub, presenter mode, and PDF badges work end-to-end", async () => {
  const docSelection = { _range: null, get rangeCount() { return this._range ? 1 : 0; }, getRangeAt() { return this._range; }, removeAllRanges() { this._range = null; }, addRange(r) { this._range = r; } };
  const { document, sandbox, userClick } = await createWorkspaceHarness({ selection: docSelection, transformHtml: demoHtml });

  await openDemoDeck(document);
  switchModule(document, "SLIDE");
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 3);

  const pressEsc = (dlg) => document.dispatchEvent({ type: "keydown", key: "Escape", target: dlg, preventDefault() {}, stopPropagation() {} });
  const canvasLayer = document.getElementById("slide-canvas-layer");
  const lowerZone = document.getElementById("slide-lower-zone");

  await flushAsyncTurns();
  userClick(commandItem(document, "slide-insert-table"));
  await flushAsyncTurns();
  const tblDlg = document.getElementById("slide-table-dialog");
  assert.equal(tblDlg.open, true);
  pressEsc(tblDlg);
  await flushAsyncTurns();
  assert.equal(tblDlg.open, false);
  assert.equal(canvasLayer.querySelectorAll(".slide-el").length, 0, "Esc cancels without dummy node");
  assert.equal(lowerZone.querySelector("[data-slide-object]"), null);

  userClick(commandItem(document, "slide-insert-table"));
  await flushAsyncTurns();
  userClick(document.getElementById("btn-slide-table-ok"));
  await flushAsyncTurns();
  userClick(commandItem(document, "slide-insert-chart"));
  await flushAsyncTurns();
  userClick(document.getElementById("btn-slide-chart-ok"));
  await flushAsyncTurns();
  userClick(commandItem(document, "slide-insert-shape"));
  await flushAsyncTurns();
  document.getElementById("slide-shape-text").value = "담당자 연락처 010-5555-7777";
  userClick(document.getElementById("btn-slide-shape-ok"));
  await flushAsyncTurns();

  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 3);
  assert.equal(lowerZone.querySelector("[data-slide-object]"), null, "no legacy lower-zone dummy stubs created");
  assert.ok(canvasLayer.querySelector('.slide-el[data-kind="table"]'));
  assert.ok(canvasLayer.querySelector('.slide-el[data-kind="chart"]'));
  assert.ok(canvasLayer.querySelector('.slide-el[data-kind="shape"]'));

  lowerZone.focus();
  const lowerP = lowerZone.querySelector("p");
  lowerZone.dispatchEvent({ type: "beforeinput", target: lowerP });
  lowerP.textContent = "수정된 슬라이드 본문 요약";
  lowerZone.dispatchEvent({ type: "input", target: lowerP });

  userClick(document.querySelector('.slide-thumb[data-slide-idx="1"]'));
  await flushAsyncTurns();
  userClick(document.querySelector('.slide-thumb[data-slide-idx="0"]'));
  await flushAsyncTurns();
  assert.equal(lowerZone.querySelector("p").textContent, "수정된 슬라이드 본문 요약");
  assert.ok(canvasLayer.querySelector('.slide-el[data-kind="table"]'));

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns();
  assert.doesNotMatch(
    canvasLayer.querySelector('.slide-el[data-kind="shape"] .slide-el-shape-text').textContent,
    /010-5555-7777/
  );

  userClick(commandItem(document, "slide-insert-link"));
  await flushAsyncTurns();
  const linkDlg = document.getElementById("doc-link-dialog");
  assert.equal(linkDlg.open, true);
  pressEsc(linkDlg);
  assert.equal(linkDlg.open, false);
  userClick(commandItem(document, "slide-review-comment"));
  await flushAsyncTurns();
  const commentDlg = document.getElementById("doc-comment-dialog");
  assert.equal(commentDlg.open, true);
  document.getElementById("doc-comment-input").value = "발표 순서 확인";
  userClick(document.getElementById("btn-doc-comment-ok"));
  await flushAsyncTurns();
  assert.doesNotMatch(lowerZone.textContent, /\[.*\]/, "slide link and comment never append command bracket stubs");
  assert.match(document.getElementById("slide-presenter-notes").value, /발표 순서 확인/);

  userClick(commandItem(document, "slide-layout-orientation"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("slide-stage-box").dataset.orientation, "portrait");
  assert.equal(document.getElementById("slide-stage-box").style.outline, "");

  await flushAsyncTurns();
  userClick(commandItem(document, "slide-mod-presenter"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("slide-stage-box").dataset.presenterMode, "true");
  assert.equal(document.getElementById("slide-laser-dot").hidden, false);

  userClick(document.getElementById("btn-smart-diagnose"));
  await flushAsyncTurns();
  assert.match(document.getElementById("smart-summary-output").textContent, /분기 실적|발표/);

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
  assert.equal(document.getElementById("shortcut-hint").textContent, "Ctrl+/");

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
  styleSelect.value = "제목 1";
  styleSelect.dispatchEvent({ type: "change" });
  assert.equal(document.getElementById("doc-style-role-badge").textContent, "제목 1");
  assert.equal(docEditor.querySelector(".eo-h-title").closest("h1").tagName, "H1", "the selected paragraph is an outline-1 heading");

  const fontSelect = document.getElementById("doc-font-select");
  fontSelect.value = "함초롬바탕";
  fontSelect.dispatchEvent({ type: "change" });
  assert.equal(document.getElementById("doc-style-font-badge").textContent, "함초롬바탕");
  assert.equal(docEditor.style.fontFamily, "", "font applies to the selection only, not the whole document");
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoFont, "함초롬바탕");

  userClick(document.getElementById("btn-size-up"));
  assert.equal(document.getElementById("doc-font-size-input").value, "11.0");
  assert.equal(document.getElementById("doc-style-size-badge").textContent, "11.0 pt");
  assert.equal(docEditor.style.fontSize, "", "size applies to the selection only, not the whole document");
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoSize, "22");
  userClick(document.getElementById("btn-size-down"));
  assert.equal(document.getElementById("doc-font-size-input").value, "10.0");

  userClick(document.getElementById("btn-ls-up"));
  assert.equal(document.getElementById("doc-linespacing-select").value, "115");
  const selectedBlock = docEditor.querySelector(".eo-h-title").closest("h1");
  assert.equal(selectedBlock.style.lineHeight, "1.15", "line spacing is set on the selected paragraph");
  assert.equal(docEditor.style.lineHeight, "");
  userClick(document.getElementById("btn-ls-down"));
  assert.equal(document.getElementById("doc-linespacing-select").value, "100", "the steps follow the official list 1.0, 1.15, 1.5, 2.0, 2.5, 3.0");

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
  assert.equal(styleSelect.value, "바탕글");
  assert.ok(
    Array.from(styleSelect.querySelectorAll("option")).some((o) => o.value === "개요 6")
  );
  assert.equal(
    Array.from(styleSelect.querySelectorAll("option")).some((o) => o.value === "공문서 바탕글"),
    false
  );
  assert.equal(document.getElementById("btn-toggle-ctrl-marks").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-toggle-para-marks").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-toggle-doc-grid").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-align-justify").classList.contains("active"), true);
  assert.equal(document.getElementById("ruler-corner").textContent, "mm");

  switchModule(document, "DOC");
  await flushAsyncTurns();
  assert.equal(styleSelect.value, "표준", "without a selection the box shows the document default");
  assert.equal(document.getElementById("doc-style-font-badge").textContent, "맑은 고딕", "the font box shows the document default until a selection carries a run font");
  assert.equal(document.getElementById("btn-toggle-ctrl-marks").classList.contains("active"), true);
  assert.equal(document.getElementById("btn-toggle-para-marks").classList.contains("active"), false);
  assert.equal(document.getElementById("btn-toggle-doc-grid").classList.contains("active"), true);
  assert.equal(docEditor.querySelector('[data-eo-align="distribute"]').tagName, "H1", "the paragraph's alignment survives a module switch");

  assert.equal(document.getElementById("ruler-corner").textContent, "cm");
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
  putCaretInFirstTable(document, docSelection);
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
  userClick(document.querySelector('[data-studio-menu-cmd="review-verify"]'));
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
  sandbox.dispatchWindowKeydown({ key: "/", ctrlKey: true });
  assert.equal(paletteModal.open, false);
});

test("Slice 1: table sort/fn wizard, validation non-audit, rightIndentTwips roundtrip, line/col & overwrite, slide shape italic/underline, PDF/SHEET outlines, and active fit zoom", async () => {
  const audited = [];
  const selection = createSelection();
  const h1 = await createWorkspaceHarness({ selection, transformHtml: demoHtml });
  const origIpc = h1.host.__EASYOFFICE_TEST_IPC__;
  h1.host.__EASYOFFICE_TEST_IPC__ = async (cmd, payload) => {
    if (cmd === "audit_security") audited.push(payload);
    return origIpc(cmd, payload);
  };
  await flushAsyncTurns(8);

  // 1. DOC data-validation never triggers audit_security
  h1.userClick(commandItem(h1.document, "document-data-validation"));
  await flushAsyncTurns(6);
  assert.equal(audited.length, 0, "DOC data-validation must not miswire to securityAudit");

  // 2. DOC caret table sort + Undo + function wizard
  const docEditor1 = h1.document.getElementById("doc-editor");
  const getDocTableRows = () =>
    Array.from(h1.document.getElementById("doc-table-body").querySelectorAll("tr:not(#doc-table-sum-row)")).map(
      (r) => r.cells[0].textContent
    );
  const rowsBefore = getDocTableRows();
  assert.ok(rowsBefore.length >= 2);
  putCaretInFirstTable(h1.document, selection);
  h1.userClick(commandItem(h1.document, "document-data-sort"));
  await flushAsyncTurns(6);
  const rowsAfter = getDocTableRows();
  assert.notDeepEqual(rowsAfter, rowsBefore, "caret table rows actually reordered");
  h1.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(6);
  const rowsUndo = getDocTableRows();
  assert.deepEqual(rowsUndo, rowsBefore, "Ctrl+Z restores table sort order");

  // Function wizard inserts and computes =SUM(ABOVE) at caret cell
  const sumCell = h1.document.getElementById("doc-table-sum-cell");
  const rFn = h1.document.createRange();
  rFn.setStart(sumCell, 0);
  rFn.setEnd(sumCell, 0);
  selection.removeAllRanges();
  selection.addRange(rFn);
  h1.userClick(commandItem(h1.document, "document-data-fn-browser"));
  await flushAsyncTurns(4);
  assert.equal(h1.document.getElementById("doc-fn-dialog").open, true);
  h1.userClick(h1.document.getElementById("btn-doc-fn-ok"));
  await flushAsyncTurns(4);
  assert.match(sumCell.textContent, /^1000/);

  // 3. rightIndentTwips save payload preservation
  const pInd = h1.document.createElement("p");
  pInd.textContent = "오른쪽 여백 테스트";
  pInd.style.paddingRight = "32px";
  pInd.dataset.eoRightTwips = "480";
  docEditor1.appendChild(pInd);
  const blocksOut = h1.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(docEditor1);
  assert.ok(blocksOut.some((b) => b.rightIndentTwips === 480), "save payload preserves rightIndentTwips");

  // Clean harness for file-open roundtrips, line/col, overwrite, SLIDE, PDF, SHEET outlines & active fit zoom
  const sel2 = createSelection();
  const { document, host, userClick } = await createWorkspaceHarness({ selection: sel2 });
  await flushAsyncTurns(8);
  const docEditor = document.getElementById("doc-editor");
  await pickFile(document, makeFixtureFile("우여백.docx", { blocks: [{ type: "paragraph", rightIndentTwips: 480, runs: [{ text: "복원 여백" }] }] }));
  await flushAsyncTurns(8);
  const restoredP = docEditor.querySelector("p");
  assert.equal(restoredP.dataset.eoRightTwips, "480");
  assert.equal(restoredP.style.paddingRight, "32px");

  // 4. Real caret line/col and Overwrite mode ('수정')
  restoredP.replaceChildren(document.createTextNode("첫째줄\n둘째가나다"));
  const textNode = restoredP.firstChild;
  const rPos = document.createRange();
  rPos.setStart(textNode, 6);
  rPos.setEnd(textNode, 6);
  sel2.removeAllRanges();
  sel2.addRange(rPos);
  document.dispatchEvent({ type: "selectionchange" });
  assert.match(document.getElementById("status-studio-pos").textContent, /2줄\s+3칸/);

  userClick(document.getElementById("status-insert-mode"));
  assert.equal(document.getElementById("status-insert-mode").textContent, "수정");
  rPos.setStart(textNode, 4);
  rPos.setEnd(textNode, 4);
  sel2.removeAllRanges();
  sel2.addRange(rPos);
  docEditor.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: "★", preventDefault() {} });
  assert.equal(textNode.data, "첫째줄\n★째가나다", "overwrite mode replaces character at caret");

  // Split compare pane renders live document content, never dummy static text
  userClick(commandItem(document, "document-view-split"));
  await flushAsyncTurns(4);
  const splitPane = document.querySelector(".canvas-stage .split-compare-pane");
  assert.ok(splitPane);
  assert.doesNotMatch(splitPane.textContent, /화면 분할 참조 보기/);
  assert.match(splitPane.textContent, /복원 여백|첫째줄/);

  // 5. SLIDE shape italic/underline wire persistence & slide-layout-paper cycling
  switchModule(document, "SLIDE");
  await flushAsyncTurns(4);
  userClick(commandItem(document, "slide-insert-shape"));
  await flushAsyncTurns(4);
  document.getElementById("slide-shape-text").value = "서식 도형";
  document.getElementById("slide-shape-italic").checked = true;
  document.getElementById("slide-shape-underline").checked = true;
  userClick(document.getElementById("btn-slide-shape-ok"));
  await flushAsyncTurns(4);
  const shapeTextEl = document.querySelector('#slide-canvas-layer .slide-el[data-kind="shape"] .slide-el-shape-text');
  assert.equal(shapeTextEl.style.fontStyle, "italic");
  assert.equal(shapeTextEl.style.textDecoration, "underline");
  const shapeEl = host.__EASYOFFICE_SLIDE_CANVAS__.getElements().at(-1);
  assert.equal(shapeEl.italic, true);
  assert.equal(shapeEl.underline, true);
  userClick(commandItem(document, "slide-layout-paper"));
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("slide-stage-box").dataset.aspect, "4:3");

  // 6. PDF decoded pdfOutlines + empty outline when absent + SHEET hidden sheet excluded
  await pickFile(document, makeFixtureFile("북마크.pdf", {
    title: "규정집", pages: 2, paragraphs: ["제1조 목적"],
    pdfOutlines: [{ title: "제1장 총칙", level: 1 }, { title: "제1조 목적", level: 2 }],
  }));
  await flushAsyncTurns(8);
  userClick(studioItem(document, "view-outline"));
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("outline-dialog").open, true);
  const pdfOutItems = Array.from(document.querySelectorAll("#outline-list .outline-item")).map((li) => li.textContent);
  assert.deepEqual(pdfOutItems, ["제1장 총칙", "  제1조 목적"]);
  document.getElementById("outline-dialog").close();

  await pickFile(document, makeFixtureFile("무북마크.pdf", { title: "단순문서", pages: 1, paragraphs: ["본문"] }));
  await flushAsyncTurns(8);
  userClick(studioItem(document, "view-outline"));
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("outline-dialog").open, true);
  assert.equal(document.querySelectorAll("#outline-list .outline-item").length, 0);
  document.getElementById("outline-dialog").close();

  await openDemoWorkbook(document);
  switchModule(document, "SHEET");
  await flushAsyncTurns(6);
  host.__EASYOFFICE_SHEET_LAYOUT__.setHiddenSheet("Sheet2", true);
  userClick(studioItem(document, "view-outline"));
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("outline-dialog").open, true);
  const sheetOutItems = Array.from(document.querySelectorAll("#outline-list .outline-item")).map((li) => li.textContent);
  assert.ok(sheetOutItems.length >= 1, "visible sheets stay in outline");
  assert.ok(!sheetOutItems.some((txt) => txt.includes("시트 2") || txt.includes("Sheet2")), "hidden sheet excluded from outline");
  document.getElementById("outline-dialog").close();

  // 7. computeFitZoom uses active module container width instead of hidden #doc-editor
  switchModule(document, "PDF");
  await flushAsyncTurns(4);
  const panePdf = document.getElementById("pane-pdf");
  const pdfCard = document.getElementById("pdf-page-card");
  Object.defineProperty(panePdf, "clientWidth", { configurable: true, value: 900 });
  Object.defineProperty(pdfCard, "offsetWidth", { configurable: true, value: 595 });
  userClick(document.getElementById("btn-zoom-fit-width"));
  assert.equal(document.getElementById("status-zoom-label").textContent, "145%");
});

test("regression guards: single-pass mailmerge before footer, h4 stats, multi-table sort feedback, portrait slide serialization/print", async () => {
  const selection = createSelection();
  const { document, host, sandbox, userClick } = await createWorkspaceHarness({ selection });
  await flushAsyncTurns(6);
  const docEditor = document.getElementById("doc-editor");

  // 1. Insert merge token before footer & verify single-pass replacement when replacement has {{...}}
  const footer = document.createElement("footer");
  footer.className = "doc-page-layer";
  footer.textContent = "바닥글 {{성명}}";
  docEditor.replaceChildren();
  const h4 = document.createElement("h4");
  h4.textContent = "소제목 H4";
  const bodyP = document.createElement("p");
  bodyP.textContent = "{{성명}}";
  docEditor.appendChild(h4);
  docEditor.appendChild(bodyP);
  docEditor.appendChild(footer);
  selection.removeAllRanges();

  userClick(commandItem(document, "doc-mod-mailmerge"));
  await flushAsyncTurns(4);
  userClick(document.getElementById("btn-doc-mailmerge-token"));
  await flushAsyncTurns(4);
  assert.ok(Array.from(docEditor.children).indexOf(footer) === docEditor.children.length - 1, "token inserted before footer");

  // Reopen mailmerge and submit via Enter key inside input; ensure {{기관명}} in replacement is not double-replaced
  bodyP.textContent = "{{성명}} / {{기관명}}";
  userClick(commandItem(document, "doc-mod-mailmerge"));
  await flushAsyncTurns(4);
  const valInputs = Array.from(document.querySelectorAll("#doc-mailmerge-rows .doc-mm-val"));
  const keyInputs = Array.from(document.querySelectorAll("#doc-mailmerge-rows .doc-mm-key"));
  keyInputs[0].value = "성명";
  valInputs[0].value = "{{기관명}}";
  keyInputs[1].value = "기관명";
  valInputs[1].value = "안전부";
  valInputs[0].dispatchEvent({ type: "keydown", key: "Enter", target: valInputs[0], preventDefault() {} });
  await flushAsyncTurns(6);
  assert.equal(bodyP.textContent, "{{기관명}} / 안전부", "single-pass mailmerge must not re-expand inserted tokens");
  assert.equal(footer.textContent, "바닥글 {{성명}}", "footer chrome must not be overwritten by mailmerge");

  // 2. Document statistics includes h4 and excludes footer & hwp-manuscript-preview
  const preview = document.createElement("div");
  preview.id = "hwp-manuscript-preview";
  const hiddenP = document.createElement("p");
  hiddenP.textContent = "원고지 안내";
  preview.appendChild(hiddenP);
  docEditor.appendChild(preview);
  userClick(studioItem(document, "review-stats"));
  await flushAsyncTurns(6);
  const statsPairs = {};
  const dts = Array.from(document.querySelectorAll("#doc-stats-grid dt"));
  const dds = Array.from(document.querySelectorAll("#doc-stats-grid dd"));
  dts.forEach((dt, i) => { statsPairs[dt.textContent] = dds[i]?.textContent; });
  assert.equal(Number(statsPairs["문단 수"]), 3, "h4 + bodyP + tokenP = 3 (excludes footer and hwp-manuscript-preview)");
  document.getElementById("doc-stats-dialog")?.close?.();

  // 3. Multi-table sort acts on focused table (not #doc-table-body) & keeps error feedback outside tables
  docEditor.replaceChildren();
  const mkTable = (rows) => {
    const t = document.createElement("table");
    const tb = document.createElement("tbody");
    for (const val of rows) {
      const tr = document.createElement("tr");
      const td = document.createElement("td");
      td.textContent = val;
      tr.appendChild(td);
      tb.appendChild(tr);
    }
    t.appendChild(tb);
    return t;
  };
  const t1 = mkTable(["다", "가"]);
  const t2 = mkTable(["가", "나"]);
  const outP = document.createElement("p");
  outP.textContent = "표 밖 문단";
  docEditor.appendChild(t1);
  docEditor.appendChild(t2);
  docEditor.appendChild(outP);
  const r2 = document.createRange();
  r2.setStart(t2.querySelector("td").firstChild, 0);
  r2.setEnd(t2.querySelector("td").firstChild, 0);
  selection.removeAllRanges();
  selection.addRange(r2);
  userClick(commandItem(document, "document-data-sort"));
  await flushAsyncTurns(4);
  assert.deepEqual(Array.from(t2.querySelectorAll("td")).map((td) => td.textContent), ["나", "가"], "second table sorted descending on first click");
  assert.deepEqual(Array.from(t1.querySelectorAll("td")).map((td) => td.textContent), ["다", "가"], "first table untouched");
  userClick(commandItem(document, "document-data-sort"));
  await flushAsyncTurns(4);
  assert.deepEqual(Array.from(t2.querySelectorAll("td")).map((td) => td.textContent), ["가", "나"], "second table sorted ascending on second click");

  // Caret outside tables (in outP) refuses to sort either table and shows MSG_OUTSIDE
  const rOut = document.createRange();
  rOut.setStart(outP.firstChild, 0);
  rOut.setEnd(outP.firstChild, 0);
  selection.removeAllRanges();
  selection.addRange(rOut);
  userClick(commandItem(document, "document-data-sort"));
  await flushAsyncTurns(4);
  assert.deepEqual(Array.from(t1.querySelectorAll("td")).map((td) => td.textContent), ["다", "가"], "first table untouched when caret is outside tables");
  assert.deepEqual(Array.from(t2.querySelectorAll("td")).map((td) => td.textContent), ["가", "나"], "second table untouched when caret is outside tables");
  assert.equal(document.getElementById("status-command-feedback").textContent, "표 안에서 사용");

  // Single-table document also refuses sort when caret is outside the table
  t2.remove();
  selection.removeAllRanges();
  selection.addRange(rOut);
  userClick(commandItem(document, "document-data-sort"));
  await flushAsyncTurns(4);
  assert.deepEqual(Array.from(t1.querySelectorAll("td")).map((td) => td.textContent), ["다", "가"], "single table untouched when caret is outside");
  assert.equal(document.getElementById("status-command-feedback").textContent, "표 안에서 사용");

  // Moving caret from t1 into non-paragraph .eo-doc-notes refuses sort instead of using stale activeDocBlockEl
  const rT1 = document.createRange();
  rT1.setStart(t1.querySelector("td").firstChild, 0);
  rT1.setEnd(t1.querySelector("td").firstChild, 0);
  selection.removeAllRanges();
  selection.addRange(rT1);
  document.dispatchEvent({ type: "selectionchange" });
  const notesBox = document.createElement("aside");
  notesBox.className = "eo-doc-notes";
  notesBox.dataset.eoNotes = "";
  const noteP = document.createElement("p");
  noteP.textContent = "각주 문단";
  notesBox.appendChild(noteP);
  docEditor.appendChild(notesBox);
  const rNote = document.createRange();
  rNote.setStart(noteP.firstChild, 0);
  rNote.setEnd(noteP.firstChild, 0);
  selection.removeAllRanges();
  selection.addRange(rNote);
  document.dispatchEvent({ type: "selectionchange" });
  userClick(commandItem(document, "document-data-sort"));
  await flushAsyncTurns(4);
  assert.deepEqual(Array.from(t1.querySelectorAll("td")).map((td) => td.textContent), ["다", "가"], "t1 untouched when caret moves to .eo-doc-notes");
  assert.equal(document.getElementById("status-command-feedback").textContent, "표 안에서 사용");

  // Empty selection when first block in docEditor is a table refuses sort (no firstBlock fallback)
  const freshSel = createSelection();
  const hNoSel = await createWorkspaceHarness({ selection: freshSel });
  await flushAsyncTurns(6);
  const edNoSel = hNoSel.document.getElementById("doc-editor");
  const tFirst = hNoSel.document.createElement("table");
  const tbFirst = hNoSel.document.createElement("tbody");
  for (const val of ["다", "가"]) {
    const tr = hNoSel.document.createElement("tr");
    const td = hNoSel.document.createElement("td");
    td.textContent = val;
    tr.appendChild(td);
    tbFirst.appendChild(tr);
  }
  tFirst.appendChild(tbFirst);
  edNoSel.replaceChildren(tFirst);
  freshSel.removeAllRanges();
  hNoSel.userClick(commandItem(hNoSel.document, "document-data-sort"));
  await flushAsyncTurns(4);
  assert.deepEqual(Array.from(tFirst.querySelectorAll("td")).map((td) => td.textContent), ["다", "가"], "first-child table untouched without caret");
  assert.equal(hNoSel.document.getElementById("status-command-feedback").textContent, "표 안에서 사용");

  // Single-row table preserves error message in status feedback
  const tSingle = mkTable(["단일행"]);
  docEditor.appendChild(tSingle);
  const rSingle = document.createRange();
  rSingle.setStart(tSingle.querySelector("td").firstChild, 0);
  rSingle.setEnd(tSingle.querySelector("td").firstChild, 0);
  selection.removeAllRanges();
  selection.addRange(rSingle);
  userClick(commandItem(document, "document-data-sort"));
  await flushAsyncTurns(4);
  assert.match(document.getElementById("status-command-feedback").textContent, /부족/);

  // 4. SLIDE portrait orientation propagates to addSlide, print pages, and serializeDeckPayload; review-track has no fake state
  switchModule(document, "SLIDE");
  await flushAsyncTurns(4);
  userClick(commandItem(document, "slide-layout-orientation"));
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("slide-stage-box").dataset.orientation, "portrait");
  userClick(document.getElementById("btn-slide-add"));
  await flushAsyncTurns(4);
  userClick(document.getElementById("btn-slide-move-up"));
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("slide-stage-box").dataset.orientation, "portrait", "added slide preserves portrait orientation");
  const serialized = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__.serializeDeckPayload(
    [{ aspect: "16:9", orientation: "portrait", title: "", lower: "", notes: "", elements: [] }],
    document,
  );
  assert.equal(serialized.orientation, "portrait");

  const ppApi = host.__EASYOFFICE_PRINT_PREVIEW__;
  await ppApi.open();
  await flushAsyncTurns(6);
  const printSlidePage = document.querySelector("#print-preview-pane .eo-print-page[data-orientation='portrait']");
  assert.ok(printSlidePage, "print preview builds portrait slide page");
  ppApi.close();

  delete document.getElementById("slide-stage-box").dataset.layoutState;
  const trackCmdEl = commandItem(document, "slide-review-track");
  if (trackCmdEl) userClick(trackCmdEl);
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("slide-stage-box").dataset.layoutState, undefined, "unsupported slide-review-track must not write fake layoutState");
});


