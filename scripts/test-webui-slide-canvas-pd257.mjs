import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem, studioItem } from "./test-support/modules.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

// Minimal valid 2x3 PNG (RGBA, valid PNG signature + IHDR + IDAT + IEND).
const VALID_PNG_BYTES = Uint8Array.from(
  Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAIAAAADCAYAAAC56t6BAAAAFElEQVR4nGP8z8Dwn4GBgYGBgQEAAP//AwX/p7n6AAAAAElFTkSuQmCC",
    "base64"
  )
);

async function openSlideHarness(options = {}) {
  const h = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-slide-pd257", ...options });
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  return h;
}

const $ = (doc, id) => doc.getElementById(id);

function slideMenuAction(doc, act) {
  const el = doc.querySelector(`#menu-bar [data-slide-act="${act}"]`);
  assert.ok(el, `missing slide menu action: ${act}`);
  return el;
}

test("PD-257: 글상자·6종 도형·PNG 그림·표·5종 벡터 차트 삽입 및 드래그·리사이즈·스마트 스냅 가이드 동작", async () => {
  const { document, sandbox, userClick, host } = await openSlideHarness();
  const canvasApi = host.__EASYOFFICE_SLIDE_CANVAS__;
  assert.ok(canvasApi, "slide canvas API is exposed");

  const stage = $(document, "slide-stage-box");
  const layer = $(document, "slide-canvas-layer");
  const snapLayer = $(document, "slide-snap-layer");
  assert.ok(stage && layer && snapLayer, "stage, canvas layer and snap layer exist");
  assert.equal(stage.dataset.layout, "titleAndContent");

  // 1. Textbox insertion, inline editing, and selection-scoped bold/italic/underline vs title fallback
  userClick(slideMenuAction(document, "insert-textbox"));
  await flushAsyncTurns(4);
  let els = layer.querySelectorAll(".slide-el");
  assert.equal(els.length, 1);
  const tbEl = els[0];
  assert.equal(tbEl.dataset.kind, "textbox");
  assert.equal(tbEl.classList.contains("is-selected"), true);

  const tbInner = tbEl.querySelector(".slide-el-textbox");
  tbEl.dispatchEvent({ type: "dblclick", target: tbInner });
  tbInner.textContent = "보조 설명 글상자";
  tbInner.dispatchEvent({ type: "input", target: tbInner });
  tbInner.blur();
  await flushAsyncTurns(2);

  userClick(commandItem(document, "slide-home-bold"));
  await flushAsyncTurns(4);
  userClick(commandItem(document, "slide-home-italic"));
  await flushAsyncTurns(4);
  userClick(commandItem(document, "slide-home-underline"));
  await flushAsyncTurns(4);

  const renderedTb = layer.querySelector('.slide-el[data-kind="textbox"] .slide-el-textbox');
  assert.equal(renderedTb.textContent, "보조 설명 글상자");
  assert.equal(renderedTb.style.fontWeight, "700");
  assert.equal(renderedTb.style.fontStyle, "italic");
  assert.equal(renderedTb.style.textDecoration, "underline");
  assert.equal($(document, "slide-title-zone").style.fontWeight, "", "selected element absorbs bold instead of title");

  // Deselecting routes slide-home-bold back to #slide-title-zone
  canvasApi.clearSelection();
  userClick(commandItem(document, "slide-home-bold"));
  await flushAsyncTurns(4);
  assert.equal($(document, "slide-title-zone").style.fontWeight, "700");

  // 2. 6 shape types via #slide-shape-dialog + SVG geometry + double-click edit
  const shapeTypes = ["rect", "roundRect", "ellipse", "rightArrow", "callout", "diamond"];
  userClick(commandItem(document, "slide-insert-shape"));
  await flushAsyncTurns(4);
  const shapeDialog = $(document, "slide-shape-dialog");
  assert.equal(shapeDialog.open, true);
  $(document, "slide-shape-type").value = "roundRect";
  $(document, "slide-shape-text").value = "승인 완료";
  $(document, "slide-shape-fill").value = "#2563eb";
  $(document, "slide-shape-stroke").value = "#1d4ed8";
  $(document, "slide-shape-stroke-width").value = "3";
  $(document, "slide-shape-x").value = "480";
  $(document, "slide-shape-y").value = "140";
  $(document, "slide-shape-w").value = "220";
  $(document, "slide-shape-h").value = "100";
  userClick($(document, "btn-slide-shape-ok"));
  await flushAsyncTurns(4);
  assert.equal(shapeDialog.open, false);

  for (const st of shapeTypes) {
    const shapeCard = layer.querySelector('.slide-el[data-kind="shape"]');
    assert.ok(shapeCard, "shape element rendered");
    shapeCard.dispatchEvent({ type: "dblclick", target: shapeCard });
    assert.equal(shapeDialog.open, true);
    $(document, "slide-shape-type").value = st;
    userClick($(document, "btn-slide-shape-ok"));
    await flushAsyncTurns(2);
    const svg = layer.querySelector('.slide-el[data-kind="shape"] svg.slide-el-shape-svg');
    assert.ok(svg, `SVG rendered for ${st}`);
    assert.ok(svg.children.length >= 1, `shape geometry child exists for ${st}`);
  }
  const finalShapeLabel = layer.querySelector('.slide-el[data-kind="shape"] .slide-el-shape-text');
  assert.equal(finalShapeLabel.textContent, "승인 완료");

  // 3. PNG picture insertion (insertPictureFromBytes) + handle-based storage (no inline base64 bloat)
  const picRes = await canvasApi.insertPictureFromBytes(VALID_PNG_BYTES, "아키텍처 다이어그램.png");
  assert.equal(picRes.ok, true);
  assert.ok(picRes.element.imageId, "image uses registered imageId handle");
  assert.equal(picRes.element.data, undefined, "in-memory slide element does not duplicate raw base64");
  const imgCard = layer.querySelector('.slide-el[data-kind="image"]');
  assert.ok(imgCard);
  const imgNode = imgCard.querySelector("img.slide-el-image");
  assert.ok(imgNode);
  assert.equal(imgNode.getAttribute("alt"), "아키텍처 다이어그램");

  // Double-click image opens #slide-image-dialog to edit x/y/w/h and alt text
  imgCard.dispatchEvent({ type: "dblclick", target: imgCard });
  const imgDialog = $(document, "slide-image-dialog");
  assert.equal(imgDialog.open, true);
  $(document, "slide-image-lock-ratio").checked = false;
  $(document, "slide-image-x").value = "80";
  $(document, "slide-image-y").value = "260";
  $(document, "slide-image-w").value = "240";
  $(document, "slide-image-h").value = "160";
  $(document, "slide-image-alt").value = "시스템 구성도";
  userClick($(document, "btn-slide-image-ok"));
  await flushAsyncTurns(2);
  assert.equal(layer.querySelector('.slide-el[data-kind="image"] img').getAttribute("alt"), "시스템 구성도");

  // 4. Table insertion via #slide-table-dialog + inline cell double-click editing
  userClick(commandItem(document, "slide-insert-table"));
  await flushAsyncTurns(4);
  const tableDialog = $(document, "slide-table-dialog");
  assert.equal(tableDialog.open, true);
  $(document, "slide-table-rows").value = "2";
  $(document, "slide-table-cols").value = "3";
  $(document, "slide-table-header").checked = true;
  $(document, "slide-table-x").value = "360";
  $(document, "slide-table-y").value = "260";
  $(document, "slide-table-w").value = "320";
  $(document, "slide-table-h").value = "140";
  userClick($(document, "btn-slide-table-ok"));
  await flushAsyncTurns(4);

  const tblCard = layer.querySelector('.slide-el[data-kind="table"]');
  assert.ok(tblCard);
  assert.equal(tblCard.querySelectorAll("th").length, 3, "header row cells rendered as <th>");
  const firstTd = tblCard.querySelector("tbody td");
  firstTd.dispatchEvent({ type: "dblclick", target: firstTd });
  firstTd.textContent = "118%";
  firstTd.dispatchEvent({ type: "input", target: firstTd });
  firstTd.blur();
  await flushAsyncTurns(2);
  assert.equal(canvasApi.getElements()[3].rows[1][0], "118%");

  // 5. 5 vector chart types via #slide-chart-dialog reusing sheet-chart-svg
  userClick(commandItem(document, "slide-insert-chart"));
  await flushAsyncTurns(4);
  const chartDialog = $(document, "slide-chart-dialog");
  assert.equal(chartDialog.open, true);
  $(document, "slide-chart-type").value = "column";
  $(document, "slide-chart-heading").value = "분기 매출";
  $(document, "slide-chart-legend").value = "right";
  $(document, "slide-chart-categories").value = "1분기, 2분기, 3분기";
  $(document, "slide-chart-series").value = "매출: 120, 165, 210\n목표: 100, 150, 190";
  userClick($(document, "btn-slide-chart-ok"));
  await flushAsyncTurns(4);

  const chartSelectorByType = {
    column: ".eo-chart-bar",
    bar: ".eo-chart-bar",
    line: ".eo-chart-line",
    pie: ".eo-chart-slice",
    scatter: ".eo-chart-point",
  };
  for (const [ct, subSel] of Object.entries(chartSelectorByType)) {
    const cCard = layer.querySelector('.slide-el[data-kind="chart"]');
    cCard.dispatchEvent({ type: "dblclick", target: cCard });
    assert.equal(chartDialog.open, true);
    $(document, "slide-chart-type").value = ct;
    userClick($(document, "btn-slide-chart-ok"));
    await flushAsyncTurns(2);
    const renderedChart = layer.querySelector('.slide-el[data-kind="chart"] svg.eo-chart-svg');
    assert.ok(renderedChart, `chart SVG rendered for ${ct}`);
    assert.ok(renderedChart.querySelectorAll(subSel).length >= 2, `chart vector marks (${subSel}) rendered for ${ct}`);
  }

  // 6. Drag move + SE resize + Smart snap guides (stage center & sibling edge within 6pt)
  // Stage bounding box in harness is 640x360; for 16:9 logical stage is 960x540 (1 px = 1.5 pt).
  const tbId = layer.querySelector('.slide-el[data-kind="textbox"]').dataset.elementId;
  canvasApi.updateElement(tbId, { x: 100, y: 50, w: 200, h: 80 });
  const dragTarget = layer.querySelector('.slide-el[data-kind="textbox"]');
  const currentTb = () => canvasApi.getElements().find((e) => e.id === tbId);

  // Move textbox so its horizontal center (x + 100) approaches stage center 480pt -> x = 377pt (within 6pt of 380pt!)
  // dxPt = 277pt => dxPx = 277 / 1.5 = 184.67px -> use 185px (277.5pt -> x = 377.5pt, center = 477.5pt, |477.5 - 480| = 2.5pt <= 6pt)
  dragTarget.dispatchEvent({ type: "mousedown", button: 0, clientX: 10, clientY: 10, target: dragTarget });
  document.dispatchEvent({ type: "mousemove", clientX: 195, clientY: 132 });
  assert.equal(snapLayer.hidden, false, "snap guide layer becomes visible during aligned drag");
  assert.ok(snapLayer.querySelector(".slide-snap-line-v"), "vertical center snap line rendered");
  document.dispatchEvent({ type: "mouseup", clientX: 195, clientY: 132 });
  await flushAsyncTurns(4);
  assert.equal(snapLayer.hidden, true, "snap guide layer hides when drag finishes");
  assert.equal(currentTb().x, 380, "snapped to stage horizontal center (380 + 200/2 = 480)");

  // Sibling edge snap: image is at x = 80; drag textbox near x = 83 (dxPt = -297pt -> dxPx = -198px) -> snaps to x = 80!
  const refreshedDragTarget = layer.querySelector('.slide-el[data-kind="textbox"]');
  refreshedDragTarget.dispatchEvent({ type: "mousedown", button: 0, clientX: 300, clientY: 50, target: refreshedDragTarget });
  document.dispatchEvent({ type: "mousemove", clientX: 102, clientY: 50 });
  assert.equal(snapLayer.hidden, false, "sibling alignment guide visible");
  document.dispatchEvent({ type: "mouseup", clientX: 102, clientY: 50 });
  await flushAsyncTurns(4);
  assert.equal(currentTb().x, 80, "snapped to sibling left edge x=80");

  // Toggle smart snap off via slide-mod-snap -> no snapping
  userClick(commandItem(document, "slide-mod-snap"));
  await flushAsyncTurns(4);
  assert.equal(canvasApi.isSnapEnabled(), false);

  // SE resize handle updates logical w/h
  const targetAfter = layer.querySelector('.slide-el[data-kind="textbox"]');
  const seHandle = targetAfter.querySelector('.slide-el-handle[data-dir="se"]');
  assert.ok(seHandle, "SE resize handle exists");
  seHandle.dispatchEvent({ type: "mousedown", button: 0, clientX: 100, clientY: 100, target: seHandle });
  document.dispatchEvent({ type: "mousemove", clientX: 140, clientY: 120 }); // +60pt width, +30pt height
  document.dispatchEvent({ type: "mouseup", clientX: 140, clientY: 120 });
  await flushAsyncTurns(4);
  assert.equal(currentTb().w, 260);
  assert.equal(currentTb().h, 110);

  // Turn snap back on
  userClick(commandItem(document, "slide-mod-snap"));
  await flushAsyncTurns(4);
  assert.equal(canvasApi.isSnapEnabled(), true);
});

test("PD-257: Z-Order(맨 앞으로/맨 뒤로), 방향키 미세 이동, Delete 개체 삭제, 단일 Undo/Redo 복원", async () => {
  const { document, sandbox, userClick, host } = await openSlideHarness();
  const canvasApi = host.__EASYOFFICE_SLIDE_CANVAS__;

  userClick(slideMenuAction(document, "insert-textbox"));
  await flushAsyncTurns(4);
  const firstId = canvasApi.getElements()[0].id;

  userClick(slideMenuAction(document, "insert-textbox"));
  await flushAsyncTurns(4);
  const secondId = canvasApi.getElements()[1].id;
  assert.deepEqual(canvasApi.getElements().map((e) => e.id), [firstId, secondId]);

  // Select first element and bring to front
  canvasApi.selectElement(firstId);
  userClick(slideMenuAction(document, "bring-front"));
  await flushAsyncTurns(4);
  assert.deepEqual(canvasApi.getElements().map((e) => e.id), [secondId, firstId]);

  // Send back
  userClick(slideMenuAction(document, "send-back"));
  await flushAsyncTurns(4);
  assert.deepEqual(canvasApi.getElements().map((e) => e.id), [firstId, secondId]);

  // Arrow key nudge (2pt) and Shift+Arrow (10pt)
  const startX = canvasApi.getElements()[0].x;
  const startY = canvasApi.getElements()[0].y;
  const card = document.querySelector(`.slide-el[data-element-id="${firstId}"]`);
  card.focus();
  sandbox.dispatchWindowKeydown({ key: "ArrowRight", target: card });
  assert.equal(canvasApi.getElements()[0].x, startX + 2);
  sandbox.dispatchWindowKeydown({ key: "ArrowDown", shiftKey: true, target: card });
  assert.equal(canvasApi.getElements()[0].y, startY + 10);

  // Delete removes only the selected canvas element, keeping the slide in the deck
  sandbox.dispatchWindowKeydown({ key: "Delete", target: card });
  await flushAsyncTurns(4);
  assert.equal(canvasApi.getElements().length, 1);
  assert.equal(canvasApi.getElements()[0].id, secondId);
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 1);

  // Undo restores deleted element; Redo deletes it again
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(canvasApi.getElements().length, 2);
  assert.equal(canvasApi.getElements()[0].id, firstId);

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(canvasApi.getElements().length, 1);
  assert.equal(canvasApi.getElements()[0].id, secondId);
});

test("PD-257: 마스터 레이아웃·화면비·전환 효과·발표자 도구 시작·F5 슬라이드 쇼·인쇄 미리보기 5종 개체 렌더링", async () => {
  const printedRoots = [];
  const { document, sandbox, userClick, host } = await openSlideHarness();
  host.CSSStyleSheet = class {
    constructor() { this.cssRules = []; }
    insertRule(text, at) { this.cssRules.splice(at, 0, { cssText: text, style: { setProperty() {} } }); return at; }
  };
  host.requestAnimationFrame = (cb) => { cb(); return 1; };
  host.print = () => {
    const root = document.getElementById("eo-print-root");
    if (root) printedRoots.push(root.cloneNode(true));
  };
  document.adoptedStyleSheets = [];
  const canvasApi = host.__EASYOFFICE_SLIDE_CANVAS__;

  // Populate all 5 element kinds on slide 1
  canvasApi.addElement({ kind: "textbox", x: 60, y: 120, w: 240, h: 70, text: "발표 핵심 요약" });
  canvasApi.addElement({ kind: "shape", shapeType: "diamond", x: 340, y: 120, w: 180, h: 90, text: "검증 단계", fill: "2563EB", stroke: "1D4ED8", strokeWidthPt: 2 });
  await canvasApi.insertPictureFromBytes(VALID_PNG_BYTES, "로고.png");
  canvasApi.addElement({ kind: "table", x: 60, y: 260, w: 320, h: 140, headerRow: true, fill: "DBEAFE", rows: [["구분", "실적"], ["1분기", "125"]] });
  canvasApi.addElement({
    kind: "chart", chartType: "line", x: 420, y: 250, w: 360, h: 180,
    title: "성장 추이", legend: "bottom", categories: ["1분기", "2분기"], series: [{ name: "매출", values: [100, 160] }],
  });

  // Master layout select (twoColumn -> blank -> titleSlide) & slide-mod-master dialog
  const layoutSel = $(document, "slide-layout-select");
  assert.ok(layoutSel);
  layoutSel.value = "twoColumn";
  layoutSel.dispatchEvent({ type: "change", target: layoutSel });
  await flushAsyncTurns(4);
  assert.equal($(document, "slide-stage-box").dataset.layout, "twoColumn");

  userClick(commandItem(document, "slide-mod-master"));
  await flushAsyncTurns(4);
  const layoutDialog = $(document, "slide-layout-dialog");
  assert.equal(layoutDialog.open, true);
  $(document, "slide-layout-dialog-select").value = "titleSlide";
  userClick($(document, "btn-slide-layout-ok"));
  await flushAsyncTurns(4);
  assert.equal($(document, "slide-stage-box").dataset.layout, "titleSlide");
  assert.equal(layoutSel.parentElement.querySelector(".custom-select-value").textContent, "제목 슬라이드");

  // Aspect 4:3 and A4 + transition cycle
  const aspectSel = $(document, "slide-aspect-select");
  aspectSel.value = "4:3";
  aspectSel.dispatchEvent({ type: "change", target: aspectSel });
  await flushAsyncTurns(4);
  assert.equal($(document, "slide-stage-box").dataset.aspect, "4:3");

  userClick(commandItem(document, "slide-mod-transition"));
  await flushAsyncTurns(4);
  const transSel = $(document, "slide-transition-select");
  assert.equal(transSel.value, "push");
  assert.equal(transSel.parentElement.querySelector(".custom-select-value").textContent, "밀어내기");

  // Presenter tool command starts slideshow with presenter panel immediately visible
  userClick(commandItem(document, "slide-mod-presenter"));
  await flushAsyncTurns(6);
  assert.equal($(document, "slide-show-overlay").hidden, false);
  assert.equal($(document, "slide-show-presenter").hidden, false, "presenter view opens immediately");
  const showFrame = $(document, "slide-show-frame");
  assert.equal(showFrame.dataset.layout, "titleSlide");
  assert.equal(showFrame.dataset.transition, "push");
  assert.equal(showFrame.querySelectorAll(".slide-canvas-layer .slide-el").length, 5, "all 5 elements rendered in slideshow");
  assert.ok(showFrame.querySelector('.slide-el[data-kind="chart"] svg.eo-chart-svg'));
  assert.ok(showFrame.querySelector('.slide-el[data-kind="shape"] svg.slide-el-shape-svg'));
  assert.ok(showFrame.querySelector('.slide-el[data-kind="table"] table'));
  assert.ok(showFrame.querySelector('.slide-el[data-kind="image"] img'));
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  await flushAsyncTurns(4);
  assert.equal($(document, "slide-show-overlay").hidden, true);

  // Print preview renders all 5 elements inside #eo-print-root
  userClick(studioItem(document, "print"));
  await flushAsyncTurns(10);
  assert.equal(printedRoots.length, 1);
  const printPage = printedRoots[0].querySelector(".eo-print-page .slide-stage");
  assert.ok(printPage);
  assert.equal(printPage.dataset.aspect, "4:3");
  assert.equal(printPage.dataset.layout, "titleSlide");
  assert.equal(printPage.querySelectorAll(".slide-canvas-layer .slide-el").length, 5, "all 5 elements rendered in print output");
  assert.ok(printPage.querySelector('.slide-el[data-kind="chart"] svg.eo-chart-svg'));
});

test("PD-257: 저장 페이로드 직렬화(그림 Base64 물질화 포함) → 새 하니스 열기 100% 복원 및 손상된 그림 Fail-Closed 거부", async () => {
  const converts = [];
  const base = await openSlideHarness();
  const cat = base.sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = base.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SLIDE",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });

  const h1 = await openSlideHarness({
    testIpc: async (command, payload) => {
      if (command === "convert_document_format") converts.push(JSON.parse(String(payload)));
      return offline[command](payload);
    },
  });
  const api1 = h1.host.__EASYOFFICE_SLIDE_CANVAS__;

  // Corrupt / empty image bytes are rejected Fail-Closed
  const emptyRes = await api1.insertPictureFromBytes(new Uint8Array(0), "empty.png");
  assert.equal(emptyRes.ok, false);
  const corruptRes = await api1.insertPictureFromBytes(new Uint8Array([0x00, 0x11, 0x22, 0x33, 0x44]), "bad.png");
  assert.equal(corruptRes.ok, false);
  assert.equal(api1.getElements().length, 0, "corrupted bytes never create an element");

  // Build rich slide and save
  const aspectSel = $(h1.document, "slide-aspect-select");
  aspectSel.value = "A4";
  aspectSel.dispatchEvent({ type: "change", target: aspectSel });
  const layoutSel = $(h1.document, "slide-layout-select");
  layoutSel.value = "sectionHeader";
  layoutSel.dispatchEvent({ type: "change", target: layoutSel });
  const transSel = $(h1.document, "slide-transition-select");
  transSel.value = "zoom";
  transSel.dispatchEvent({ type: "change", target: transSel });
  await flushAsyncTurns(4);

  $(h1.document, "slide-title-zone").querySelector("h2").textContent = "분기 실적 발표";
  $(h1.document, "slide-title-zone").dispatchEvent({ type: "input", target: $(h1.document, "slide-title-zone") });

  api1.addElement({
    id: "el-1", kind: "textbox", x: 80, y: 140, w: 360, h: 90,
    text: "보조 설명 글상자", fontSizePt: 18, bold: true, italic: false, underline: false,
    align: "left", color: "1F2937", fill: "F3F4F6", role: null,
  });
  api1.addElement({
    id: "el-2", kind: "shape", x: 480, y: 140, w: 220, h: 100,
    shapeType: "roundRect", text: "승인 완료", fill: "2563EB", stroke: "1D4ED8",
    strokeWidthPt: 2, color: "FFFFFF", fontSizePt: 16, bold: true, align: "center",
  });
  await api1.insertPictureFromBytes(VALID_PNG_BYTES, "아키텍처 다이어그램.png");
  api1.addElement({
    id: "el-4", kind: "table", x: 360, y: 260, w: 420, h: 160,
    headerRow: true, fill: "DBEAFE", rows: [["구분", "목표", "실적"], ["1분기", "100", "118"]],
  });
  api1.addElement({
    id: "el-5", kind: "chart", x: 80, y: 260, w: 260, h: 180,
    chartType: "pie", title: "비중", legend: "right",
    categories: ["국내", "해외"], series: [{ name: "비중", values: [65, 35] }],
  });

  h1.userClick(studioItem(h1.document, "studio-save"));
  await flushAsyncTurns(16);

  const saved = converts.at(-1);
  assert.equal(saved.aspect, "A4");
  assert.equal(saved.slides.length, 1);
  const s0 = saved.slides[0];
  assert.equal(s0.title, "분기 실적 발표");
  assert.equal(s0.layout, "sectionHeader");
  assert.equal(s0.transition, "zoom");
  assert.equal(s0.elements.length, 5);
  const savedImg = s0.elements.find((e) => e.kind === "image");
  assert.ok(savedImg && savedImg.data.length > 20, "image base64 data materialized on save");
  assert.equal(savedImg.mime, "image/png");

  // Reload into a fresh harness via file-open and verify 100% fidelity
  const h2 = await createWorkspaceHarness({ search: "?module=DOC&win=w-reopen-pd257" });
  await flushAsyncTurns(10);
  await pickFile(h2.document, makeFixtureFile("복원검증.pptx", { aspect: saved.aspect, slides: saved.slides }));
  await flushAsyncTurns(12);

  assert.equal($(h2.document, "app").dataset.module, "SLIDE");
  assert.equal($(h2.document, "slide-stage-box").dataset.aspect, "A4");
  assert.equal($(h2.document, "slide-stage-box").dataset.layout, "sectionHeader");
  assert.equal($(h2.document, "slide-transition-select").value, "zoom");
  assert.equal($(h2.document, "slide-title-zone").textContent, "분기 실적 발표");

  const api2 = h2.host.__EASYOFFICE_SLIDE_CANVAS__;
  const restored = api2.getElements();
  assert.equal(restored.length, 5);
  assert.deepEqual(restored.map((e) => e.kind), ["textbox", "shape", "image", "table", "chart"]);
  assert.equal(restored[0].text, "보조 설명 글상자");
  assert.equal(restored[0].bold, true);
  assert.equal(restored[1].shapeType, "roundRect");
  assert.equal(restored[1].fill, "2563EB");
  assert.ok(restored[2].imageId, "reloaded image registered back into handle store");
  assert.equal(restored[2].data, undefined, "reloaded image strips raw base64 from live deck memory");
  assert.deepEqual(restored[3].rows, [["구분", "목표", "실적"], ["1분기", "100", "118"]]);
  assert.equal(restored[4].chartType, "pie");
  assert.equal($(h2.document, "slide-canvas-layer").querySelectorAll(".slide-el").length, 5);

  // Also verify Rust decode wire shape (`kind: "textBox"` and `dataBase64` on images) hydrates cleanly
  const rustDecodeSlides = [
    {
      ...saved.slides[0],
      elements: saved.slides[0].elements.map((el) => {
        if (el.kind === "textbox") return { ...el, kind: "textBox" };
        if (el.kind === "image") {
          const { data, ...rest } = el;
          return { ...rest, dataBase64: data };
        }
        return el;
      }),
    },
  ];
  const h3 = await createWorkspaceHarness({ search: "?module=DOC&win=w-rust-decode-pd257" });
  await flushAsyncTurns(10);
  await pickFile(h3.document, makeFixtureFile("러스트디코드.pptx", { aspect: "4:3", slides: rustDecodeSlides }));
  await flushAsyncTurns(12);
  const restoredRust = h3.host.__EASYOFFICE_SLIDE_CANVAS__.getElements();
  assert.equal(restoredRust.length, 5);
  assert.deepEqual(restoredRust.map((e) => e.kind), ["textbox", "shape", "image", "table", "chart"]);
  assert.ok(restoredRust[2].imageId, "dataBase64 image from Rust decode registered into handle store");
});

test("PD-257: 코파일럿 SLIDE 컨텍스트(레이아웃·개체 요약) 및 좌표 개체 편집·실행 취소 연동", async () => {
  const seenCopilotPayloads = [];
  const base = await openSlideHarness();
  const cat = base.sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = base.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SLIDE",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const { document, userClick, host } = await openSlideHarness({
    testIpc: async (command, payload) => {
      if (command === "run_smart_assistant") {
        const parsed = JSON.parse(String(payload));
        if (parsed.action === "copilot_run") seenCopilotPayloads.push(parsed);
      }
      return offline[command](payload);
    },
  });

  host.__EASYOFFICE_SLIDE_CANVAS__.addElement({ kind: "textbox", x: 60, y: 120, w: 260, h: 70, text: "기존 요약" });
  userClick($(document, "btn-copilot-toggle"));
  const modelBtn = $(document, "btn-copilot-model");
  const modelMenu = $(document, "copilot-model-menu");
  if (modelMenu.hidden) userClick(modelBtn);
  const localProviderBtn = Array.from(modelMenu.querySelectorAll('button[role="menuitemradio"]')).find(
    (b) => b.dataset.quickKind === "ollama"
  );
  userClick(localProviderBtn);
  await flushAsyncTurns(6);

  const input = $(document, "copilot-input");
  input.value = "현재 슬라이드에 도형 개체를 배치하고 두 개의 내용 레이아웃으로 변경하세요.";
  input.dispatchEvent({ type: "input", target: input });
  userClick($(document, "btn-copilot-send"));
  await flushAsyncTurns(16);

  const req = seenCopilotPayloads.at(-1);
  assert.ok(req, "copilot_run payload captured");
  assert.equal(req.context.slide.slides[0].layout, "titleAndContent");
  assert.equal(req.context.slide.slides[0].elements.length, 1);
  assert.equal(req.context.slide.slides[0].elements[0].kind, "textbox");

  assert.equal($(document, "slide-stage-box").dataset.layout, "twoColumn");
  assert.ok($(document, "slide-canvas-layer").querySelector('.slide-el[data-kind="shape"]'));

  const undoBtn = document.querySelector("#copilot-thread .eo-copilot-msg--assistant .eo-copilot-undo-btn");
  assert.ok(undoBtn);
  userClick(undoBtn);
  await flushAsyncTurns(12);
  assert.equal($(document, "slide-stage-box").dataset.layout, "titleAndContent");
  assert.equal($(document, "slide-canvas-layer").querySelector('.slide-el[data-kind="shape"]'), null);
});

