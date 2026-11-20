import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ROUTER_JS = path.join(ROOT, "crates/easyoffice-app/ui/board-router.js");
const STUDIO_JS = path.join(ROOT, "crates/easyoffice-app/ui/board-studio.js");
const BOARD_CSS = path.join(ROOT, "crates/easyoffice-app/ui/board.css");

// Minimal valid 2x3 PNG (signature + IHDR + IDAT + IEND).
const VALID_PNG_BYTES = Uint8Array.from(
  Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAIAAAADCAYAAAC56t6BAAAAFElEQVR4nGP8z8Dwn4GBgYGBgQEAAP//AwX/p7n6AAAAAElFTkSuQmCC",
    "base64"
  )
);

function segmentIntersectsRectStrict(a, b, rect) {
  const rx0 = rect.x + 0.01;
  const ry0 = rect.y + 0.01;
  const rx1 = rect.x + rect.w - 0.01;
  const ry1 = rect.y + rect.h - 0.01;
  const minX = Math.min(a.x, b.x);
  const maxX = Math.max(a.x, b.x);
  const minY = Math.min(a.y, b.y);
  const maxY = Math.max(a.y, b.y);
  if (Math.abs(a.x - b.x) < 0.05) {
    return a.x > rx0 && a.x < rx1 && maxY > ry0 && minY < ry1;
  }
  if (Math.abs(a.y - b.y) < 0.05) {
    return a.y > ry0 && a.y < ry1 && maxX > rx0 && minX < rx1;
  }
  return true;
}

function assertOrthogonalAvoids(points, obstacles) {
  assert.ok(Array.isArray(points) && points.length >= 2, "orthogonal route has at least 2 points");
  for (let i = 0; i < points.length - 1; i += 1) {
    const p = points[i];
    const q = points[i + 1];
    const isH = Math.abs(p.y - q.y) < 0.05;
    const isV = Math.abs(p.x - q.x) < 0.05;
    assert.ok(isH || isV, `segment ${i} must be axis-aligned 90deg: (${p.x},${p.y})->(${q.x},${q.y})`);
    for (const obs of obstacles) {
      assert.equal(
        segmentIntersectsRectStrict(p, q, obs),
        false,
        `segment ${i} (${p.x},${p.y})->(${q.x},${q.y}) must not cross obstacle ${obs.id}`
      );
    }
  }
}

async function openBoardHarness() {
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-board-pd260" });
  await flushAsyncTurns(6);
  assert.equal(fs.existsSync(ROUTER_JS), true, "board-router.js exists");
  assert.equal(fs.existsSync(STUDIO_JS), true, "board-studio.js exists");
  assert.equal(fs.existsSync(BOARD_CSS), true, "board.css exists");
  loadUiModule(ROUTER_JS, h.host);
  loadUiModule(STUDIO_JS, h.host);
  const pane = h.document.createElement("div");
  pane.id = "pane-board";
  pane.className = "editor-pane";
  const main = h.document.querySelector(".workspace") || h.document.body;
  main.appendChild(pane);
  const studio = h.host.__EASYOFFICE_CREATE_BOARD_STUDIO__({
    document: h.document,
    env: h.host,
    rootEl: pane,
    getActiveModule: () => "BOARD",
    notify: (msg) => {
      const fb = h.document.getElementById("status-command-feedback");
      if (fb) fb.textContent = msg;
    },
  });
  return { ...h, pane, studio, router: h.host.__EASYOFFICE_BOARD_ROUTER__ };
}

test("PD-260: 빈 초기 상태·4종 노드(도형 6종/텍스트/PNG 그림 핸들/차트 5종)·Fail-Closed 검증", async () => {
  const { document, host, pane, studio, userClick } = await openBoardHarness();
  const initial = studio.getState();
  assert.deepEqual(initial.nodes, [], "PD-176 empty start: zero nodes");
  assert.deepEqual(initial.connectors, [], "PD-176 empty start: zero connectors");
  assert.deepEqual(initial.viewport, { panX: 0, panY: 0, zoom: 100 });

  // Verify official Korean toolbar labels
  const toolbar = pane.querySelector('#board-toolbar[role="toolbar"]');
  assert.ok(toolbar, "board toolbar present");
  assert.equal(toolbar.getAttribute("aria-label"), "보드 도구 모음");
  const expectedBtnLabels = [
    ["btn-board-select", "선택"],
    ["btn-board-add-shape", "도형"],
    ["btn-board-add-text", "텍스트"],
    ["btn-board-add-image", "그림"],
    ["btn-board-add-chart", "차트"],
    ["btn-board-connect-ortho", "직각 연결선"],
    ["btn-board-connect-curved", "곡선 연결선"],
    ["btn-board-front", "맨 앞으로 가져오기"],
    ["btn-board-back", "맨 뒤로 보내기"],
    ["btn-board-delete", "개체 지우기"],
    ["btn-board-analyze", "요소 분석"],
    ["btn-board-print", "인쇄"],
    ["btn-board-export-pdf", "PDF로 내보내기"],
    ["btn-board-zoom-reset", "100%"],
    ["btn-board-fit-all", "전체 맞춤"],
  ];
  for (const [id, text] of expectedBtnLabels) {
    const el = document.getElementById(id);
    assert.ok(el, `button #${id} exists`);
    assert.equal(el.textContent.trim(), text);
  }
  assert.equal(document.getElementById("btn-board-zoom-out").getAttribute("aria-label"), "축소");
  assert.equal(document.getElementById("btn-board-zoom-in").getAttribute("aria-label"), "확대");
  assert.ok(pane.querySelector("#board-viewport .board-controls #btn-board-zoom-reset"), "floating bottom-left zoom island mounted");

  // 6 shape types
  const shapeSelect = document.getElementById("board-shape-kind");
  const shapeTypes = ["rect", "roundRect", "ellipse", "diamond", "rightArrow", "callout"];
  for (const st of shapeTypes) {
    shapeSelect.value = st;
    userClick(document.getElementById("btn-board-add-shape"));
  }
  assert.equal(studio.getState().nodes.length, 6);
  assert.equal(pane.querySelectorAll('.board-node[data-kind="shape"] svg.board-shape-svg').length, 6);

  // Text node + inline edit
  userClick(document.getElementById("btn-board-add-text"));
  const textNodeEl = pane.querySelector('.board-node[data-kind="text"]');
  assert.ok(textNodeEl);
  const textBody = textNodeEl.querySelector(".board-node-text");
  textNodeEl.dispatchEvent({ type: "dblclick", target: textBody });
  textBody.textContent = "시스템 구성 메모";
  textBody.dispatchEvent({ type: "input", target: textBody });
  textBody.blur();
  assert.equal(studio.getState().nodes.at(-1).text, "시스템 구성 메모");

  // Image handle storage + Fail-Closed invalid/oversize/coord checks
  const emptyPic = await studio.insertPictureFromBytes(new Uint8Array(0), "empty.png");
  assert.equal(emptyPic.ok, false);
  const badPic = await studio.insertPictureFromBytes(new Uint8Array([1, 2, 3, 4, 5]), "bad.png");
  assert.equal(badPic.ok, false);
  const overPic = await studio.insertPictureFromBytes(new Uint8Array(32 * 1024 * 1024 + 1), "big.png");
  assert.equal(overPic.ok, false);

  const okPic = await studio.insertPictureFromBytes(VALID_PNG_BYTES, "다이어그램.png");
  assert.equal(okPic.ok, true);
  assert.ok(okPic.node.imageId, "image stored via handle id");
  assert.equal(okPic.node.data, undefined, "raw base64 not duplicated on board node");
  assert.equal(host.__EASYOFFICE_SHEET_UTILS__.getDocImage(okPic.node.imageId) !== null, true);

  // Out-of-bounds coordinates fail-closed clamp or reject
  assert.throws(() => studio.addNode({ kind: "shape", x: 500000, y: 0, w: 120, h: 80 }), /좌표/);
  assert.throws(() => studio.addNode({ kind: "unknown", x: 0, y: 0, w: 100, h: 60 }), /지원하지/);

  // 5 native vector chart types + chart dialog edit
  userClick(document.getElementById("btn-board-add-chart"));
  const chartNodeEl = pane.querySelector('.board-node[data-kind="chart"]');
  assert.ok(chartNodeEl);
  const chartDlg = document.getElementById("board-chart-dialog");
  const chartMarks = {
    column: ".eo-chart-bar",
    bar: ".eo-chart-bar",
    line: ".eo-chart-line",
    pie: ".eo-chart-slice",
    scatter: ".eo-chart-point",
  };
  for (const [ct, sel] of Object.entries(chartMarks)) {
    chartNodeEl.dispatchEvent({ type: "dblclick", target: chartNodeEl });
    assert.equal(chartDlg.open, true);
    document.getElementById("board-chart-type").value = ct;
    document.getElementById("board-chart-title").value = "차트-" + ct;
    document.getElementById("board-chart-categories").value = "1분기, 2분기, 3분기";
    document.getElementById("board-chart-series").value = "매출: 120, 165, 210\n목표: 100, 150, 190";
    userClick(document.getElementById("btn-board-chart-ok"));
    assert.equal(chartDlg.open, false);
    const svg = pane.querySelector('.board-node[data-kind="chart"] svg.eo-chart-svg');
    assert.ok(svg, `chart SVG exists for ${ct}`);
    assert.ok(svg.querySelectorAll(sel).length >= 2, `chart marks (${sel}) rendered for ${ct}`);
  }

  // Z-Order & delete
  const firstId = studio.getState().nodes[0].id;
  studio.selectNode(firstId);
  userClick(document.getElementById("btn-board-front"));
  assert.equal(studio.getState().nodes.at(-1).id, firstId);
  userClick(document.getElementById("btn-board-back"));
  assert.equal(studio.getState().nodes[0].id, firstId);
  userClick(document.getElementById("btn-board-delete"));
  assert.equal(studio.getState().nodes.some((n) => n.id === firstId), false);
});

test("PD-260: 직각(orthogonal) 장애물 회피 자동 라우팅·드래그 시 실시간 재계산·곡선(curved) 3차 베지어", async () => {
  const { document, pane, studio, router } = await openBoardHarness();

  const nA = studio.addNode({ id: "nA", kind: "shape", shapeType: "rect", x: 40, y: 200, w: 120, h: 80, text: "시작" });
  const nB = studio.addNode({ id: "nB", kind: "shape", shapeType: "roundRect", x: 440, y: 200, w: 120, h: 80, text: "도착" });
  const nObs = studio.addNode({ id: "nObs", kind: "shape", shapeType: "diamond", x: 230, y: 140, w: 140, h: 200, text: "장애물" });

  const routed = router.routeOrthogonal(nA, nB, { obstacles: [nA, nB, nObs] });
  assert.equal(routed.avoidedObstacles, true, "detects obstacle avoidance");
  assert.equal(routed.intersectsObstacle, false, "never crosses obstacle interior");
  assertOrthogonalAvoids(routed.points, [nObs]);
  assert.match(routed.smoothPathD, /\bQ\s+[\d.-]+\s+[\d.-]+/, "smoothPathD emits quadratic rounded 90-degree bends");

  // Reverse-facing curved Bezier control offset (`curvature * 25 * sqrt(-distance)`)
  const revCurved = router.routeCurved(nB, nA, { fromPort: "right", toPort: "left", curvature: 0.25 });
  assert.ok(revCurved.controlPoints[0].x > revCurved.from.x + 20, "outward right tangent loops smoothly when target is behind");
  assert.ok(revCurved.controlPoints[1].x < revCurved.to.x - 20, "outward left tangent loops smoothly when source is ahead");

  const conn = studio.addConnector({ fromId: "nA", toId: "nB", routing: "orthogonal", label: "승인 흐름" });
  assert.ok(conn);
  const pathEl = pane.querySelector(`#board-connector-layer .board-connector[data-connector-id="${conn.id}"] path.board-connector-path`);
  assert.ok(pathEl);
  assert.match(pathEl.getAttribute("d"), /^M\s+[\d.-]+\s+[\d.-]+(?:\s+L\s+[\d.-]+\s+[\d.-]+)+$/);

  // Drag-move obstacle out of the corridor -> live reroute immediately shortens path
  const obsDom = pane.querySelector('.board-node[data-node-id="nObs"]');
  obsDom.dispatchEvent({ type: "mousedown", button: 0, clientX: 250, clientY: 200, target: obsDom });
  document.dispatchEvent({ type: "mousemove", clientX: 250, clientY: 620 });
  document.dispatchEvent({ type: "mouseup", clientX: 250, clientY: 620 });
  await flushAsyncTurns(2);

  const movedObs = studio.getState().nodes.find((n) => n.id === "nObs");
  assert.ok(movedObs.y > 500, "obstacle dragged downward out of corridor");
  const rerouted = studio.getConnectorRoute(conn.id);
  assert.equal(rerouted.avoidedObstacles, false, "no longer detours once obstacle cleared");
  assert.ok(rerouted.points.length <= 2, "shortest straight orthogonal path restored");
  assert.notEqual(pathEl.getAttribute("d"), routed.pathD, "DOM path updated live during drag");

  // Multi-obstacle maze test on router directly
  const mStart = { id: "ms", x: 0, y: 120, w: 80, h: 60 };
  const mEnd = { id: "me", x: 480, y: 120, w: 80, h: 60 };
  const mObs = [
    { id: "o1", x: 140, y: 20, w: 70, h: 240 },
    { id: "o2", x: 260, y: 100, w: 70, h: 260 },
    { id: "o3", x: 370, y: 0, w: 60, h: 220 },
  ];
  const mazeRoute = router.routeOrthogonal(mStart, mEnd, { obstacles: [mStart, mEnd, ...mObs] });
  assert.equal(mazeRoute.avoidedObstacles, true);
  assertOrthogonalAvoids(mazeRoute.points, mObs);

  // Curved Bezier connector via toolbar mode click
  document.getElementById("btn-board-connect-curved").click();
  const nADom = pane.querySelector('.board-node[data-node-id="nA"]');
  nADom.dispatchEvent({ type: "mousedown", button: 0, clientX: 60, clientY: 220, target: nADom });
  obsDom.dispatchEvent({ type: "mousedown", button: 0, clientX: 250, clientY: 620, target: obsDom });
  const cConn = studio.getState().connectors.find((c) => c.routing === "curved");
  assert.ok(cConn, "curved connector created via toolbar mode");
  const cRoute = studio.getConnectorRoute(cConn.id);
  assert.equal(cRoute.routing, "curved");
  assert.equal(cRoute.controlPoints.length, 2);
  assert.match(cRoute.pathD, /^M\s+[\d.-]+\s+[\d.-]+\s+C\s+/);

  // 8-direction resize handle updates node w/h and reroutes connectors live
  studio.selectNode("nB");
  const nBDom = pane.querySelector('.board-node[data-node-id="nB"]');
  const seHandle = nBDom.querySelector('.board-resize-handle[data-dir="se"]');
  assert.ok(seHandle);
  seHandle.dispatchEvent({ type: "mousedown", button: 0, clientX: 560, clientY: 280, target: seHandle });
  document.dispatchEvent({ type: "mousemove", clientX: 600, clientY: 320 });
  document.dispatchEvent({ type: "mouseup", clientX: 600, clientY: 320 });
  const resizedB = studio.getState().nodes.find((n) => n.id === "nB");
  assert.equal(resizedB.w, 160);
  assert.equal(resizedB.h, 120);
});

test("PD-260: 요소 분석(연결 그룹·시작/종점/고립·순환·최다 허브)·대화상자·성능 실측", async () => {
  const { document, pane, studio, router, userClick } = await openBoardHarness();

  studio.addNode({ id: "A", kind: "shape", x: 0, y: 0, w: 100, h: 60, text: "A" });
  studio.addNode({ id: "B", kind: "shape", x: 180, y: 0, w: 100, h: 60, text: "B" });
  studio.addNode({ id: "C", kind: "text", x: 360, y: 0, w: 100, h: 60, text: "C" });
  studio.addNode({ id: "D", kind: "shape", x: 180, y: 140, w: 100, h: 60, text: "D" });
  studio.addNode({ id: "E", kind: "chart", x: 0, y: 280, w: 200, h: 120, title: "E" });
  studio.addNode({ id: "F", kind: "shape", x: 260, y: 280, w: 100, h: 60, text: "F" });
  studio.addNode({ id: "G", kind: "text", x: 460, y: 280, w: 100, h: 60, text: "G" });

  studio.addConnector({ fromId: "A", toId: "B", routing: "orthogonal" });
  studio.addConnector({ fromId: "B", toId: "C", routing: "orthogonal" });
  studio.addConnector({ fromId: "B", toId: "D", routing: "curved" });
  studio.addConnector({ fromId: "E", toId: "F", routing: "orthogonal" });

  const rep1 = router.analyzeBoard(studio.getState());
  assert.equal(rep1.totalNodes, 7);
  assert.equal(rep1.shapeCount, 4);
  assert.equal(rep1.textCount, 2);
  assert.equal(rep1.chartCount, 1);
  assert.equal(rep1.connectorCount, 4);
  assert.equal(rep1.orthogonalConnectorCount, 3);
  assert.equal(rep1.curvedConnectorCount, 1);
  assert.equal(rep1.connectedComponents, 3); // {A,B,C,D}, {E,F}, {G}
  assert.deepEqual(rep1.startNodes.map((n) => n.id), ["A", "E"]);
  assert.deepEqual(rep1.endNodes.map((n) => n.id), ["C", "D", "F"]);
  assert.deepEqual(rep1.isolatedNodes.map((n) => n.id), ["G"]);
  assert.equal(rep1.hasCycle, false);
  assert.equal(rep1.topHub.id, "B");
  assert.equal(rep1.topHub.degree, 3);

  // Introduce cycle C -> A
  studio.addConnector({ fromId: "C", toId: "A", routing: "curved" });
  const rep2 = router.analyzeBoard(studio.getState());
  assert.equal(rep2.hasCycle, true, "detects cycle A->B->C->A");

  // Open analysis dialog and verify official labels
  userClick(document.getElementById("btn-board-analyze"));
  const dlg = document.getElementById("board-analysis-dialog");
  assert.equal(dlg.open, true);
  assert.equal(document.getElementById("board-analysis-title").textContent, "요소 분석");
  const dlgText = dlg.textContent;
  for (const metric of [
    "전체 요소", "도형", "텍스트", "그림", "차트", "연결선",
    "연결 그룹", "시작 요소", "종점 요소", "고립 요소", "순환 연결", "최다 연결 요소", "캔버스 범위",
  ]) {
    assert.ok(dlgText.includes(metric), `analysis dialog shows metric: ${metric}`);
  }
  assert.ok(dlgText.includes("있음"), "shows cycle present");
  userClick(document.getElementById("btn-board-analysis-close"));
  assert.equal(dlg.open, false);

  // Performance check: 100 nodes + 120 orthogonal routes + analyzeBoard
  const perfNodes = [];
  for (let i = 0; i < 100; i += 1) {
    perfNodes.push({
      id: "p" + i,
      kind: i % 4 === 0 ? "chart" : "shape",
      x: (i % 10) * 180,
      y: Math.floor(i / 10) * 140,
      w: 110,
      h: 70,
    });
  }
  const perfConns = [];
  for (let i = 0; i < 120; i += 1) {
    perfConns.push({ id: "c" + i, fromId: "p" + (i % 90), toId: "p" + ((i + 13) % 100), routing: "orthogonal" });
  }
  const t0 = performance.now();
  for (let i = 0; i < 120; i += 1) {
    router.routeOrthogonal(perfNodes[i % 90], perfNodes[(i + 13) % 100], { obstacles: perfNodes });
  }
  const perfAnalysis = router.analyzeBoard({ nodes: perfNodes, connectors: perfConns });
  const elapsedMs = performance.now() - t0;
  console.log(`[PD-260 perf] 100 nodes, 120 orthogonal routes + analyzeBoard: ${elapsedMs.toFixed(2)} ms`);
  assert.equal(perfAnalysis.totalNodes, 100);
  assert.ok(elapsedMs < 350, `100-node 120-route + analysis took ${elapsedMs.toFixed(2)}ms (< 350ms)`);
});

test("PD-260: 줌/전체 맞춤·인쇄 페이지(buildPrintPage)·벡터 PDF 페이로드(toSlideDeckPdfPayload)·CSS/접근성 위생", async () => {
  const { document, host, pane, studio, userClick } = await openBoardHarness();

  studio.addNode({ id: "b1", kind: "shape", shapeType: "rect", x: 100, y: 100, w: 160, h: 80, text: "계획" });
  studio.addNode({ id: "b2", kind: "text", x: 460, y: 120, w: 200, h: 80, text: "실행 메모" });
  await studio.insertPictureFromBytes(VALID_PNG_BYTES, "첨부.png");
  studio.addNode({
    id: "b4", kind: "chart", chartType: "pie", x: 420, y: 280, w: 280, h: 180,
    title: "비중", categories: ["A", "B"], series: [{ name: "비율", values: [70, 30] }],
  });
  studio.addConnector({ fromId: "b1", toId: "b2", routing: "orthogonal", label: "다음" });

  // Viewport zoom & fit-all
  userClick(document.getElementById("btn-board-zoom-in"));
  assert.ok(studio.getState().viewport.zoom > 100);
  userClick(document.getElementById("btn-board-zoom-reset"));
  assert.equal(studio.getState().viewport.zoom, 100);
  userClick(document.getElementById("btn-board-fit-all"));
  assert.ok(studio.getState().viewport.zoom >= 25 && studio.getState().viewport.zoom <= 400);

  // Print page DOM
  const rules = { sheet: { cssRules: [], insertRule() {} }, classFor: () => "eo-paper-board" };
  const printPage = studio.buildPrintPage(document, rules);
  assert.ok(printPage.classList.contains("eo-print-page"));
  assert.ok(printPage.classList.contains("eo-paper-board"));
  assert.ok(printPage.querySelector("svg.board-print-connectors path"), "print page contains connector SVG paths");
  assert.ok(printPage.querySelector("svg.eo-chart-svg"), "print page contains vector chart SVG");
  assert.ok(printPage.querySelector("img"), "print page contains image element");
  assert.equal(printPage.querySelectorAll(".board-resize-handle, .board-port").length, 0, "no edit handles in print page");

  // Vector PDF payload (16:9 960x540 normalized)
  const pdfPayload = studio.toSlideDeckPdfPayload("프로젝트 보드");
  assert.equal(pdfPayload.sourceFormat, "pptx");
  assert.equal(pdfPayload.targetFormat, "pdf");
  assert.equal(pdfPayload.aspect, "16:9");
  assert.equal(pdfPayload.slides.length, 1);
  const els = pdfPayload.slides[0].elements;
  assert.ok(els.length >= 5, "includes all 4 nodes plus connector vector segments");
  const imgEl = els.find((e) => e.kind === "image");
  assert.ok(imgEl && imgEl.data.length > 10, "image materialized to base64 for Rust PDF encoder");
  for (const el of els) {
    assert.ok(el.x >= 0 && el.x <= 960, `element x in 16:9 bounds: ${el.x}`);
    assert.ok(el.y >= 0 && el.y <= 540, `element y in 16:9 bounds: ${el.y}`);
    assert.ok(el.w >= 1 && el.h >= 1, `element w/h >= 1: ${el.w}x${el.h}`);
  }

  // Literal color & typography token check on board.css
  const cssRaw = fs.readFileSync(BOARD_CSS, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.equal(/#[0-9a-fA-F]{3,8}\b/.test(cssRaw), false, "board.css has zero hex literal colors");
  assert.equal(/\b(?:rgba?|hsla?)\s*\(/i.test(cssRaw), false, "board.css has zero rgb/hsl literal colors");
  assert.equal(/font-size\s*:\s*\d/i.test(cssRaw), false, "board.css uses tokens.css --font-size-* hierarchy tokens only");

  // Zero unlabeled controls check across board pane + dialogs
  const controls = document.querySelectorAll(
    '#pane-board button, #pane-board select, #pane-board input, #board-analysis-dialog button, #board-chart-dialog button, #board-chart-dialog select, #board-chart-dialog input, #board-chart-dialog textarea'
  );
  assert.ok(controls.length >= 20);
  for (const ctrl of controls) {
    const aria = (ctrl.getAttribute("aria-label") || "").trim();
    const text = ctrl.textContent.trim();
    const id = ctrl.id;
    const hasFor = id ? document.querySelectorAll("label").some((l) => l.getAttribute("for") === id && l.textContent.trim() !== "") : false;
    assert.ok(aria || text || hasFor, `control must have accessible label: ${ctrl.tagName}#${id}.${ctrl.className}`);
  }

  // Cleanup host state check
  assert.ok(host.__EASYOFFICE_BOARD_ROUTER__);
});
