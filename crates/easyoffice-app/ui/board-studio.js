// 참조 구현 (reference implementation): rhwp-studio `src/i18n/locales/ko.ts` (MIT, Copyright (c) 2025-2026 Edward Kim),
// Excalidraw (`excalidraw/excalidraw`, MIT, Copyright (c) 2020 Excalidraw, commit 091bc0e9d86fef5727c3f4d148a73e441b064ed1,
// `packages/excalidraw/components/Island.scss`), xyflow (`xyflow/xyflow`, MIT, Copyright (c) 2019-2025 webkid GmbH,
// commit 29ab9e001c2ba320c9d84b00c729c745132d9145, `packages/system/src/styles/{style,node-resizer}.css`);
// see `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  const SVG_NS = "http://www.w3.org/2000/svg";
  const MAX_NODES = 512, MAX_CONNECTORS = 1024, MIN_COORD = -100000, MAX_COORD = 100000;
  const MIN_NODE_W = 36, MIN_NODE_H = 28, MAX_NODE_SIZE = 10000, MAX_TEXT_CHARS = 16384;
  const MAX_SINGLE_IMAGE_BYTES = 32 * 1024 * 1024, MAX_TOTAL_IMAGE_BYTES = 128 * 1024 * 1024;
  const MAX_DIMENSION_PX = 65535, MAX_SLIDE_ELEMENTS = 256, MIN_ZOOM = 25, MAX_ZOOM = 400;
  const ZOOM_STEPS = [25, 33, 50, 67, 75, 90, 100, 125, 150, 200, 300, 400];
  const RESIZE_DIRS = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
  const PORT_DIRS = [["top", "위쪽 연결점"], ["right", "오른쪽 연결점"], ["bottom", "아래쪽 연결점"], ["left", "왼쪽 연결점"]];
  const SHAPE_LABELS = Object.freeze({
    rect: "직사각형", roundRect: "모서리가 둥근 직사각형", ellipse: "타원",
    diamond: "마름모", rightArrow: "오른쪽 화살표", callout: "설명선",
  });
  const CHART_LABELS = Object.freeze({
    column: "묶은 세로 막대형", bar: "묶은 가로 막대형", line: "꺾은선형", pie: "원형", scatter: "분산형",
  });
  const NODE_KINDS = new Set(["shape", "text", "image", "chart"]);
  const PNG_SIG = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  const IHDR_SIG = [0x49, 0x48, 0x44, 0x52];
  const CHART_PALETTE = ["#4472c4", "#ed7d31", "#a5a5a5", "#ffc000", "#5b9bd5", "#70ad47"];

  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const round2 = (v) => Math.round((Number(v) || 0) * 100) / 100;
  const u32be = (u8, at) => (((u8[at] << 24) >>> 0) + (u8[at + 1] << 16) + (u8[at + 2] << 8) + u8[at + 3]) >>> 0;

  function domEl(doc, tag, attrs, children) {
    const node = doc.createElement(tag);
    if (attrs) {
      for (const [k, v] of Object.entries(attrs)) {
        if (v === undefined || v === null) continue;
        if (k === "text") node.textContent = String(v);
        else if (k === "className") node.className = String(v);
        else if (k === "hidden") node.hidden = Boolean(v);
        else node.setAttribute(k, String(v));
      }
    }
    if (children) for (const c of children) if (c) node.appendChild(c);
    return node;
  }

  function svgEl(doc, tag, attrs, text) {
    const el = doc.createElementNS(SVG_NS, tag);
    if (attrs) for (const [k, v] of Object.entries(attrs)) if (v !== undefined && v !== null) el.setAttribute(k, String(v));
    if (text !== undefined) el.textContent = String(text);
    return el;
  }

  function normalizeHex6(raw, fallback) {
    if (raw === null || raw === undefined || raw === "") return fallback;
    const c = String(raw).trim().replace(/^#/, "");
    if (/^[0-9A-Fa-f]{6}$/.test(c)) return c.toUpperCase();
    if (/^[0-9A-Fa-f]{3}$/.test(c)) return (c[0] + c[0] + c[1] + c[1] + c[2] + c[2]).toUpperCase();
    return fallback;
  }

  function parseImageHeader(u8) {
    if (!(u8 instanceof Uint8Array) || u8.length < 4) return null;
    const ext = host.__EASYOFFICE_SHEET_LAYOUT__?.parseImageHeader || host.__EASYOFFICE_DOC_IMAGE_API__?.parseImageHeader;
    if (typeof ext === "function") return ext(u8);
    if (u8.length >= 24 && u32be(u8, 8) === 13 && PNG_SIG.every((b, i) => u8[i] === b) && IHDR_SIG.every((b, i) => u8[12 + i] === b)) {
      const widthPx = u32be(u8, 16), heightPx = u32be(u8, 20);
      if (widthPx >= 1 && heightPx >= 1 && widthPx <= MAX_DIMENSION_PX && heightPx <= MAX_DIMENSION_PX) return { format: "png", widthPx, heightPx };
    }
    if (u8[0] === 0xff && u8[1] === 0xd8 && u8[2] === 0xff) {
      let pos = 2;
      while (pos < u8.length) {
        while (pos < u8.length && u8[pos] === 0xff) pos += 1;
        if (pos >= u8.length) return null;
        const marker = u8[pos++];
        if (marker === 0xd9 || marker === 0xda) return null;
        if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) continue;
        if (pos + 2 > u8.length) return null;
        const segLen = (u8[pos] << 8) | u8[pos + 1];
        if (segLen < 2 || pos + segLen > u8.length) return null;
        if (marker >= 0xc0 && marker <= 0xc2) {
          if (segLen < 7) return null;
          const heightPx = (u8[pos + 3] << 8) | u8[pos + 4], widthPx = (u8[pos + 5] << 8) | u8[pos + 6];
          return widthPx >= 1 && heightPx >= 1 ? { format: "jpg", widthPx, heightPx } : null;
        }
        pos += segLen;
      }
    }
    return null;
  }

  function sanitizeChart(rawCategories, rawSeries) {
    const categories = Array.isArray(rawCategories) && rawCategories.length > 0
      ? rawCategories.slice(0, 128).map((c) => String(c ?? "").slice(0, 128))
      : ["1분기", "2분기", "3분기"];
    const srcSeries = Array.isArray(rawSeries) && rawSeries.length > 0
      ? rawSeries.slice(0, 16)
      : [{ name: "실적", values: [120, 165, 210] }];
    const series = srcSeries.map((s, idx) => {
      const vals = Array.isArray(s?.values) ? s.values.slice(0, 128) : [];
      return { name: String(s?.name || "계열 " + (idx + 1)).slice(0, 128), values: categories.map((_, i) => (Number.isFinite(Number(vals[i])) ? Number(vals[i]) : 0)) };
    });
    return { categories, series };
  }

  const createInitialBoardState = () => ({ nodes: [], connectors: [], viewport: { panX: 0, panY: 0, zoom: 100 } });

  function validateCoord(val, label) {
    const n = Number(val);
    if (!Number.isFinite(n) || n < MIN_COORD || n > MAX_COORD) throw new Error(label + " 좌표가 허용 범위(-100,000..100,000)를 벗어났습니다.");
    return round2(n);
  }

  function normalizeBoardNode(raw, fallbackId) {
    if (!raw || typeof raw !== "object") throw new Error("잘못된 보드 요소 입력입니다.");
    const rawKind = String(raw.kind || "").trim(), kind = rawKind === "textbox" ? "text" : rawKind;
    if (!NODE_KINDS.has(kind)) throw new Error("지원하지 않는 보드 요소 종류입니다: " + rawKind);
    const defW = kind === "chart" ? 340 : kind === "image" ? 220 : kind === "text" ? 200 : 150;
    const defH = kind === "chart" ? 210 : kind === "image" ? 160 : kind === "text" ? 72 : 84;
    const x = raw.x === undefined ? 120 : validateCoord(raw.x, "X"), y = raw.y === undefined ? 120 : validateCoord(raw.y, "Y");
    const w = round2(clamp(Number.isFinite(Number(raw.w)) ? Number(raw.w) : defW, MIN_NODE_W, MAX_NODE_SIZE));
    const h = round2(clamp(Number.isFinite(Number(raw.h)) ? Number(raw.h) : defH, MIN_NODE_H, MAX_NODE_SIZE));
    const id = String(raw.id || fallbackId).trim().slice(0, 64) || fallbackId;
    if (kind === "shape") {
      const shapeType = Object.hasOwn(SHAPE_LABELS, raw.shapeType) ? raw.shapeType : "rect";
      return {
        id, kind, x, y, w, h, shapeType, text: String(raw.text ?? SHAPE_LABELS[shapeType]).slice(0, MAX_TEXT_CHARS),
        fill: normalizeHex6(raw.fill, "EFF6FF"), stroke: normalizeHex6(raw.stroke, "2563EB"),
        strokeWidthPt: clamp(Number(raw.strokeWidthPt ?? 1.5), 0, 24), color: normalizeHex6(raw.color, "1E293B"), fontSizePt: clamp(Number(raw.fontSizePt) || 10, 8, 96),
      };
    }
    if (kind === "text") {
      return { id, kind, x, y, w, h, text: String(raw.text ?? "텍스트").slice(0, MAX_TEXT_CHARS), fontSizePt: clamp(Number(raw.fontSizePt) || 9.5, 8, 96), color: raw.color ? normalizeHex6(raw.color, "") : "", fill: raw.fill ? normalizeHex6(raw.fill, "") : "" };
    }
    if (kind === "image") {
      const utils = host.__EASYOFFICE_SHEET_UTILS__;
      let imageId = raw.imageId ? String(raw.imageId) : "", mime = raw.mime === "image/jpeg" ? "image/jpeg" : "image/png";
      let naturalWidthPx = Number(raw.naturalWidthPx) || 0, naturalHeightPx = Number(raw.naturalHeightPx) || 0;
      const rawB64 = typeof raw.data === "string" ? raw.data : (typeof raw.dataBase64 === "string" ? raw.dataBase64 : "");
      if (!imageId && rawB64.length > 0 && utils) {
        let bytes = null;
        try { bytes = utils.base64ToBytes(rawB64); } catch { bytes = null; }
        if (!bytes || bytes.length === 0 || bytes.length > MAX_SINGLE_IMAGE_BYTES) throw new Error("유효하지 않거나 용량 한도를 초과한 그림입니다.");
        const header = parseImageHeader(bytes);
        if (!header) throw new Error("지원하지 않거나 손상된 그림 파일입니다.");
        imageId = utils.registerDocImage(rawB64, header.format);
        mime = header.format === "jpg" ? "image/jpeg" : "image/png";
        naturalWidthPx = header.widthPx;
        naturalHeightPx = header.heightPx;
      }
      if (!imageId) throw new Error("그림 핸들(imageId)이 필요합니다.");
      return { id, kind, x, y, w, h, mime, imageId, alt: String(raw.alt ?? "").slice(0, 512), naturalWidthPx: naturalWidthPx || Math.round(w), naturalHeightPx: naturalHeightPx || Math.round(h) };
    }
    return { id, kind: "chart", x, y, w, h, chartType: Object.hasOwn(CHART_LABELS, raw.chartType) ? raw.chartType : "column", title: String(raw.title ?? "차트").slice(0, 160), legend: ["right", "bottom", "top", "left", "none"].includes(raw.legend) ? raw.legend : "right", ...sanitizeChart(raw.categories, raw.series) };
  }

  // The nodes and connectors of a board model, checked one by one. `strict` (a file being opened) refuses the whole
  // model at the first node or connector that is not valid, so nothing is silently lost; otherwise (a restored
  // snapshot) such an entry is left out and counted, and every other node still comes back.
  function normalizeBoardState(next, strict) {
    const rawNodes = Array.isArray(next?.nodes) ? next.nodes : [], rawConns = Array.isArray(next?.connectors) ? next.connectors : [];
    if (strict && (rawNodes.length > MAX_NODES || rawConns.length > MAX_CONNECTORS)) throw new Error("보드 요소 또는 연결선 수가 한도를 넘었습니다.");
    const nodes = [], ids = new Set(), conns = [];
    let dropped = Math.max(0, rawNodes.length - MAX_NODES) + Math.max(0, rawConns.length - MAX_CONNECTORS);
    const keep = (make) => { try { return make(); } catch (err) { if (strict) throw err; dropped += 1; return null; } };
    rawNodes.slice(0, MAX_NODES).forEach((raw, i) => {
      const node = keep(() => {
        const n = normalizeBoardNode(raw, "bn-" + (i + 1));
        if (ids.has(n.id)) throw new Error("요소 ID가 중복되었습니다: " + n.id);
        return n;
      });
      if (node) { ids.add(node.id); nodes.push(node); }
    });
    rawConns.slice(0, MAX_CONNECTORS).forEach((c, i) => {
      const conn = keep(() => {
        if (!c || !ids.has(String(c.fromId)) || !ids.has(String(c.toId)) || c.fromId === c.toId) throw new Error("연결선이 없는 요소를 가리킵니다.");
        return { id: String(c.id || "bc-" + (i + 1)).slice(0, 64), fromId: String(c.fromId), toId: String(c.toId), fromPort: c.fromPort || "auto", toPort: c.toPort || "auto", routing: c.routing === "curved" ? "curved" : "orthogonal", label: String(c.label ?? "").slice(0, 128) };
      });
      if (conn) conns.push(conn);
    });
    const vp = next?.viewport || {};
    const viewport = { panX: round2(clamp(Number(vp.panX) || 0, MIN_COORD, MAX_COORD)), panY: round2(clamp(Number(vp.panY) || 0, MIN_COORD, MAX_COORD)), zoom: clamp(Math.round(Number(vp.zoom) || 100), MIN_ZOOM, MAX_ZOOM) };
    return { nodes, connectors: conns, viewport, dropped };
  }

  function buildShapeSvg(doc, node) {
    const vw = Math.max(24, Math.round(node.w)), vh = Math.max(20, Math.round(node.h));
    const sw = clamp(Number(node.strokeWidthPt ?? 2), 0, 24), pad = Math.max(1, sw / 2), st = node.shapeType || "rect";
    const svg = svgEl(doc, "svg", { class: "board-shape-svg", viewBox: `0 0 ${vw} ${vh}`, preserveAspectRatio: "none", "aria-hidden": "true" });
    let geom;
    if (st === "ellipse") {
      geom = svgEl(doc, "ellipse", { cx: round2(vw / 2), cy: round2(vh / 2), rx: round2(Math.max(2, vw / 2 - pad)), ry: round2(Math.max(2, vh / 2 - pad)) });
    } else if (st === "diamond" || st === "rightArrow" || st === "callout") {
      const hx = round2(Math.max(pad + 6, vw * 0.66)), by = round2(Math.max(pad + 8, vh * 0.78));
      const pts = st === "diamond"
        ? [`${round2(vw / 2)},${round2(pad)}`, `${round2(vw - pad)},${round2(vh / 2)}`, `${round2(vw / 2)},${round2(vh - pad)}`, `${round2(pad)},${round2(vh / 2)}`]
        : st === "rightArrow"
          ? [`${round2(pad)},${round2(vh * 0.26)}`, `${hx},${round2(vh * 0.26)}`, `${hx},${round2(pad)}`, `${round2(vw - pad)},${round2(vh / 2)}`, `${hx},${round2(vh - pad)}`, `${hx},${round2(vh * 0.74)}`, `${round2(pad)},${round2(vh * 0.74)}`]
          : [`${round2(pad)},${round2(pad)}`, `${round2(vw - pad)},${round2(pad)}`, `${round2(vw - pad)},${by}`, `${round2(vw * 0.36)},${by}`, `${round2(vw * 0.16)},${round2(vh - pad)}`, `${round2(vw * 0.22)},${by}`, `${round2(pad)},${by}`];
      geom = svgEl(doc, "polygon", { points: pts.join(" ") });
    } else {
      const attrs = { x: round2(pad), y: round2(pad), width: round2(Math.max(2, vw - pad * 2)), height: round2(Math.max(2, vh - pad * 2)) };
      if (st === "roundRect") { const r = round2(Math.min(18, Math.min(vw, vh) * 0.2)); attrs.rx = r; attrs.ry = r; }
      geom = svgEl(doc, "rect", attrs);
    }
    geom.setAttribute("fill", "#" + normalizeHex6(node.fill, "EFF6FF"));
    geom.setAttribute("stroke", sw > 0 ? "#" + normalizeHex6(node.stroke, "2563EB") : "none");
    geom.setAttribute("stroke-width", String(sw));
    svg.appendChild(geom);
    return svg;
  }

  function buildFallbackChartSvg(doc, node) {
    const w = Math.max(160, Math.round(node.w)), h = Math.max(110, Math.round(node.h)), ct = node.chartType || "column";
    const svg = svgEl(doc, "svg", { class: "eo-chart-svg", viewBox: `0 0 ${w} ${h}`, width: w, height: h, "aria-hidden": "true" });
    const seriesList = Array.isArray(node.series) && node.series.length > 0 ? node.series : [{ values: [120, 165, 210] }];
    const vals = seriesList[0].values || [120, 165, 210];
    const maxV = Math.max(1, ...seriesList.flatMap((s) => (s.values || []).map((v) => Math.abs(Number(v) || 0))));
    if (ct === "pie") {
      const cx = w * 0.5, cy = h * 0.54, r = Math.min(w, h) * 0.32;
      vals.forEach((_, idx) => {
        const a0 = (idx / vals.length) * Math.PI * 2, a1 = ((idx + 1) / vals.length) * Math.PI * 2;
        svg.appendChild(svgEl(doc, "path", { class: "eo-chart-slice", d: `M ${cx} ${cy} L ${round2(cx + r * Math.cos(a0))} ${round2(cy + r * Math.sin(a0))} A ${r} ${r} 0 0 1 ${round2(cx + r * Math.cos(a1))} ${round2(cy + r * Math.sin(a1))} Z`, fill: CHART_PALETTE[idx % CHART_PALETTE.length] }));
      });
    } else if (ct === "line" || ct === "scatter") {
      seriesList.forEach((ser, sIdx) => {
        const sv = ser.values || vals, col = CHART_PALETTE[sIdx % CHART_PALETTE.length];
        const pts = sv.map((v, i) => [round2(32 + (i + 0.5) * ((w - 52) / Math.max(1, sv.length))), round2(h - 24 - (Math.max(0, v) / maxV) * (h - 54))]);
        if (ct === "line" && pts.length > 1) svg.appendChild(svgEl(doc, "path", { class: "eo-chart-line", d: pts.map((p, i) => (i ? "L " : "M ") + p[0] + " " + p[1]).join(" "), fill: "none", stroke: col, "stroke-width": 2 }));
        pts.forEach((p) => svg.appendChild(svgEl(doc, "circle", { class: "eo-chart-point", cx: p[0], cy: p[1], r: 3.5, fill: col })));
      });
    } else {
      vals.forEach((v, i) => {
        const ratio = Math.max(0.05, Math.abs(Number(v) || 0) / maxV);
        const bx = ct === "bar" ? 36 : round2(32 + i * ((w - 52) / Math.max(1, vals.length)));
        const by = ct === "bar" ? round2(28 + i * ((h - 48) / Math.max(1, vals.length))) : round2(h - 24 - ratio * (h - 54));
        const bw = ct === "bar" ? round2(ratio * (w - 60)) : Math.max(8, round2(((w - 52) / Math.max(1, vals.length)) * 0.6));
        const bh = ct === "bar" ? Math.max(8, round2(((h - 48) / Math.max(1, vals.length)) * 0.6)) : round2(ratio * (h - 54));
        svg.appendChild(svgEl(doc, "rect", { class: "eo-chart-bar", x: bx, y: by, width: bw, height: bh, fill: CHART_PALETTE[i % CHART_PALETTE.length] }));
      });
    }
    return svg;
  }

  function buildBoardChartSvg(doc, node, forPrint) {
    const svgApi = host.__EASYOFFICE_SHEET_CHART_SVG__, utils = host.__EASYOFFICE_SHEET_UTILS__;
    if (svgApi?.renderChartSvg && utils?.colIndexToLetters) {
      const { categories, series } = sanitizeChart(node.categories, node.series);
      const map = Object.create(null);
      map.A1 = { value: "", t: "s" };
      for (let r = 0; r < categories.length; r += 1) {
        const cat = String(categories[r] ?? ""), num = Number(cat);
        map["A" + (r + 2)] = node.chartType === "scatter" && cat !== "" && Number.isFinite(num) ? { value: String(num) } : { value: cat, t: "s" };
      }
      for (let s = 0; s < series.length; s += 1) {
        const col = utils.colIndexToLetters(s + 1);
        map[col + "1"] = { value: String(series[s].name || "계열 " + (s + 1)), t: "s" };
        for (let r = 0; r < categories.length; r += 1) map[col + (r + 2)] = { value: Number.isFinite(series[s].values[r]) ? String(series[s].values[r]) : "0" };
      }
      const svg = svgApi.renderChartSvg({
        document: doc, sheetName: "BoardChart", sheetDataStore: { BoardChart: map },
        chart: { chartType: node.chartType || "column", title: node.title || "", legend: node.legend || "right", dataRange: "A1:" + utils.colIndexToLetters(series.length) + (categories.length + 1) },
        width: Math.max(160, Math.round(node.w)), height: Math.max(110, Math.round(node.h)), forPrint: Boolean(forPrint),
      });
      if (svg) { svg.style.width = "100%"; svg.style.height = "100%"; return svg; }
    }
    return buildFallbackChartSvg(doc, node);
  }

  function createBoardStudio(deps = {}) {
    const doc = deps.document || host.document, router = host.__EASYOFFICE_BOARD_ROUTER__;
    const utils = host.__EASYOFFICE_SHEET_UTILS__, kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__?.(doc);
    const notify = typeof deps.notify === "function" ? deps.notify : (msg) => { const fb = doc.getElementById("status-command-feedback"); if (fb) fb.textContent = msg; };

    const state = createInitialBoardState(), routeCache = new Map();
    let mode = "select", selectedNodeId = null, selectedConnectorId = null, pendingConnect = null;
    let editingChartId = null, activeDrag = null, spacePressed = false;
    let root = deps.rootEl || doc.getElementById("pane-board") || null;
    let viewportEl = null, worldEl = null, connectorLayerEl = null, nodeLayerEl = null, zoomLabelEl = null, shapeSelectEl = null, imageInputEl = null;
    const $ = (id) => doc.getElementById(id);

    const nextId = (prefix, items) => {
      const used = new Set(items.map((item) => String(item.id)));
      let n = items.length + 1;
      while (used.has(prefix + "-" + n)) n += 1;
      return prefix + "-" + n;
    };
    const nodeMap = () => new Map(state.nodes.map((n) => [n.id, n]));
    const totalBoardImageBytes = () => state.nodes.reduce((sum, n) => sum + (n.kind === "image" && n.imageId ? utils?.getDocImage?.(n.imageId)?.bytesLen || 0 : 0), 0);

    function applyViewportTransform() {
      if (worldEl) worldEl.style.transform = `translate(${round2(state.viewport.panX)}px, ${round2(state.viewport.panY)}px) scale(${state.viewport.zoom / 100})`;
      if (zoomLabelEl) zoomLabelEl.textContent = state.viewport.zoom + "%";
    }

    function setMode(nextMode) {
      mode = nextMode;
      pendingConnect = null;
      for (const [id, m] of [["btn-board-select", "select"], ["btn-board-connect-ortho", "connect-ortho"], ["btn-board-connect-curved", "connect-curved"]]) {
        const btn = $(id);
        if (btn) { btn.setAttribute("aria-pressed", mode === m ? "true" : "false"); btn.classList.toggle("active", mode === m); }
      }
      renderNodes();
    }

    function renderConnectors() {
      if (!connectorLayerEl || !router) return;
      const byId = nodeMap();
      routeCache.clear();
      if (!connectorLayerEl.querySelector("defs")) {
        const defs = svgEl(doc, "defs");
        for (const [mid, cls] of [["board-arrow-end", "board-arrow-head"], ["board-arrow-end-selected", "board-arrow-head board-arrow-head--selected"]]) {
          const mk = svgEl(doc, "marker", { id: mid, markerWidth: 8, markerHeight: 8, refX: 7, refY: 4, orient: "auto", markerUnits: "strokeWidth" });
          mk.appendChild(svgEl(doc, "path", { d: "M 0 0 L 8 4 L 0 8 z", class: cls }));
          defs.appendChild(mk);
        }
        connectorLayerEl.appendChild(defs);
      }
      const existing = new Map(Array.from(connectorLayerEl.querySelectorAll("g.board-connector")).map((el) => [el.dataset.connectorId, el]));
      const activeIds = new Set();
      for (let i = 0; i < state.connectors.length; i += 1) {
        const conn = state.connectors[i], route = router.routeConnector(conn, byId, state.nodes);
        if (!route) continue;
        routeCache.set(conn.id, route);
        activeIds.add(conn.id);
        const isSel = conn.id === selectedConnectorId;
        let g = existing.get(conn.id), hit, path, lblBg, lblText;
        if (!g) {
          g = svgEl(doc, "g", { class: "board-connector", "data-connector-id": conn.id, "data-routing": conn.routing });
          hit = svgEl(doc, "path", { class: "board-connector-hit", fill: "none" });
          path = svgEl(doc, "path", { class: "board-connector-path", fill: "none" });
          g.appendChild(hit);
          g.appendChild(path);
          g.addEventListener("mousedown", (ev) => { ev.stopPropagation?.(); selectConnector(conn.id); });
          connectorLayerEl.appendChild(g);
        } else {
          hit = g.querySelector("path.board-connector-hit");
          path = g.querySelector("path.board-connector-path");
          lblBg = g.querySelector("rect.board-connector-label-bg");
          lblText = g.querySelector("text.board-connector-label");
        }
        g.setAttribute("class", "board-connector" + (isSel ? " is-selected" : ""));
        g.dataset.routing = conn.routing;
        hit.setAttribute("d", route.pathD);
        path.setAttribute("d", route.pathD);
        path.setAttribute("marker-end", isSel ? "url(#board-arrow-end-selected)" : "url(#board-arrow-end)");
        if (conn.label) {
          const textW = Math.max(28, conn.label.length * 10 + 10);
          if (!lblBg) {
            lblBg = svgEl(doc, "rect", { class: "board-connector-label-bg", height: 20, rx: 3 });
            lblText = svgEl(doc, "text", { class: "board-connector-label", "text-anchor": "middle" });
            g.appendChild(lblBg);
            g.appendChild(lblText);
          }
          lblBg.setAttribute("x", String(round2(route.labelPos.x - textW * 0.5)));
          lblBg.setAttribute("y", String(round2(route.labelPos.y - 10)));
          lblBg.setAttribute("width", String(textW));
          lblText.setAttribute("x", String(route.labelPos.x));
          lblText.setAttribute("y", String(round2(route.labelPos.y + 4)));
          lblText.textContent = conn.label;
        } else { lblBg?.remove(); lblText?.remove(); }
      }
      for (const [id, el] of existing.entries()) if (!activeIds.has(id)) el.remove();
    }

    function positionNodeDom(card, node) {
      card.style.left = round2(node.x) + "px";
      card.style.top = round2(node.y) + "px";
      card.style.width = round2(node.w) + "px";
      card.style.height = round2(node.h) + "px";
    }

    function buildNodeContent(docTarget, node, forPrint) {
      if (node.kind === "shape") {
        const lbl = domEl(docTarget, "div", { className: "board-node-label", text: node.text || "" });
        lbl.style.color = "#" + normalizeHex6(node.color, "1E293B");
        lbl.style.fontSize = (node.fontSizePt || 10) + "pt";
        return [buildShapeSvg(docTarget, node), lbl];
      }
      if (node.kind === "text") {
        const body = domEl(docTarget, "div", { className: "board-node-text", text: node.text || "" });
        body.style.fontSize = (node.fontSizePt || 9.5) + "pt";
        if (node.color) body.style.color = "#" + normalizeHex6(node.color, "1F2937");
        return [body];
      }
      if (node.kind === "image") {
        const url = node.imageId ? utils?.docImageUrl?.(node.imageId) : "";
        const b64 = !url && node.imageId ? utils?.getDocImage?.(node.imageId)?.base64 : "";
        return [domEl(docTarget, "img", { className: "board-node-image", src: url || (b64 ? `data:${node.mime || "image/png"};base64,${b64}` : null), alt: node.alt || "", draggable: "false" })];
      }
      return [buildBoardChartSvg(docTarget, node, forPrint)];
    }

    function bindNodeEvents(card, node) {
      card.addEventListener("mousedown", (ev) => {
        if (ev.button !== 0) return;
        const portEl = ev.target?.closest?.(".board-port");
        if (portEl) { ev.preventDefault?.(); ev.stopPropagation?.(); handleConnectClick(node.id, portEl.dataset.port || "auto"); return; }
        if (mode === "connect-ortho" || mode === "connect-curved") { ev.preventDefault?.(); ev.stopPropagation?.(); handleConnectClick(node.id, "auto"); return; }
        const handle = ev.target?.closest?.(".board-resize-handle");
        if (handle) { ev.preventDefault?.(); ev.stopPropagation?.(); activeDrag = { kind: "resize", dir: handle.dataset.dir || "se", node, card, startX: ev.clientX || 0, startY: ev.clientY || 0, orig: { ...node } }; return; }
        if (ev.target?.getAttribute?.("contenteditable") === "plaintext-only") { selectNode(node.id); return; }
        ev.stopPropagation?.();
        selectNode(node.id);
        card.focus?.();
        activeDrag = { kind: "move", dir: "", node, card, startX: ev.clientX || 0, startY: ev.clientY || 0, orig: { ...node } };
      });
      card.addEventListener("dblclick", (ev) => {
        ev.stopPropagation?.();
        selectNode(node.id);
        if (node.kind === "chart") { openChartDialog(node); return; }
        const t = card.querySelector(node.kind === "shape" ? ".board-node-label" : ".board-node-text");
        if (!t) return;
        t.setAttribute("contenteditable", "plaintext-only");
        t.focus?.();
        // One undo step per edit session: the state before the first change is taken when it comes, not for every key.
        let undoTaken = false;
        const onInput = () => {
          if (!undoTaken) { deps.recordUndo?.(); undoTaken = true; }
          node.text = String(t.textContent || "").slice(0, MAX_TEXT_CHARS);
          deps.noteActivity?.();
        };
        const onBlur = () => { t.removeAttribute("contenteditable"); t.removeEventListener("input", onInput); t.removeEventListener("blur", onBlur); };
        t.addEventListener("input", onInput);
        t.addEventListener("blur", onBlur);
      });
    }

    function renderNodes() {
      if (!nodeLayerEl) return;
      const cards = state.nodes.map((node) => {
        const cls = `board-node board-node--${node.kind}` + (node.id === selectedNodeId ? " is-selected" : "") + (pendingConnect?.fromId === node.id ? " is-connect-source" : "");
        const card = domEl(doc, "div", { className: cls, tabindex: 0 }, buildNodeContent(doc, node, false));
        card.dataset.nodeId = node.id;
        card.dataset.kind = node.kind;
        if (node.shapeType) card.dataset.shapeType = node.shapeType;
        positionNodeDom(card, node);
        for (const dir of RESIZE_DIRS) {
          const h = domEl(doc, "span", { className: "board-resize-handle", "aria-hidden": "true" });
          h.dataset.dir = dir;
          card.appendChild(h);
        }
        for (const [port, label] of PORT_DIRS) {
          const p = domEl(doc, "button", { type: "button", className: "board-port", "aria-label": label });
          p.dataset.port = port;
          card.appendChild(p);
        }
        bindNodeEvents(card, node);
        return card;
      });
      nodeLayerEl.replaceChildren(...cards);
    }

    const renderAll = () => { applyViewportTransform(); renderNodes(); renderConnectors(); deps.onStateChange?.(getState()); };

    function selectNode(id) {
      selectedNodeId = id && state.nodes.some((n) => n.id === id) ? id : null;
      selectedConnectorId = null;
      nodeLayerEl?.querySelectorAll(".board-node").forEach((el) => el.classList.toggle("is-selected", el.dataset.nodeId === selectedNodeId));
      connectorLayerEl?.querySelectorAll(".board-connector").forEach((el) => el.classList.remove("is-selected"));
      return selectedNodeId;
    }

    function selectConnector(id) {
      selectedConnectorId = id && state.connectors.some((c) => c.id === id) ? id : null;
      selectedNodeId = null;
      nodeLayerEl?.querySelectorAll(".board-node").forEach((el) => el.classList.remove("is-selected"));
      renderConnectors();
      return selectedConnectorId;
    }

    function handleConnectClick(nodeId, port) {
      const routing = mode === "connect-curved" ? "curved" : "orthogonal";
      if (!pendingConnect) { pendingConnect = { fromId: nodeId, fromPort: port || "auto", routing }; renderNodes(); notify("도착 요소를 클릭해 연결선을 완성하세요."); return; }
      if (pendingConnect.fromId === nodeId) { pendingConnect = null; renderNodes(); return; }
      const conn = addConnector({ fromId: pendingConnect.fromId, toId: nodeId, fromPort: pendingConnect.fromPort, toPort: port || "auto", routing: pendingConnect.routing });
      pendingConnect = null;
      setMode("select");
      if (conn) selectConnector(conn.id);
    }

    function onDocMouseMove(ev) {
      if (!activeDrag) return;
      const d = activeDrag;
      if (d.kind === "pan") {
        state.viewport.panX = round2(clamp(d.origPanX + ((ev.clientX || 0) - d.startX), MIN_COORD, MAX_COORD));
        state.viewport.panY = round2(clamp(d.origPanY + ((ev.clientY || 0) - d.startY), MIN_COORD, MAX_COORD));
        applyViewportTransform();
        return;
      }
      const scale = Math.max(0.25, state.viewport.zoom / 100), dx = ((ev.clientX || 0) - d.startX) / scale, dy = ((ev.clientY || 0) - d.startY) / scale, n = d.node;
      if (d.kind === "move") { n.x = round2(clamp(d.orig.x + dx, MIN_COORD, MAX_COORD)); n.y = round2(clamp(d.orig.y + dy, MIN_COORD, MAX_COORD)); }
      else {
        if (d.dir.includes("e")) n.w = round2(clamp(d.orig.w + dx, MIN_NODE_W, MAX_NODE_SIZE));
        if (d.dir.includes("s")) n.h = round2(clamp(d.orig.h + dy, MIN_NODE_H, MAX_NODE_SIZE));
        if (d.dir.includes("w")) { const r = d.orig.x + d.orig.w; n.x = round2(clamp(d.orig.x + dx, MIN_COORD, r - MIN_NODE_W)); n.w = round2(clamp(r - n.x, MIN_NODE_W, MAX_NODE_SIZE)); }
        if (d.dir.includes("n")) { const b = d.orig.y + d.orig.h; n.y = round2(clamp(d.orig.y + dy, MIN_COORD, b - MIN_NODE_H)); n.h = round2(clamp(b - n.y, MIN_NODE_H, MAX_NODE_SIZE)); }
      }
      positionNodeDom(d.card, n);
      renderConnectors();
    }

    function onDocMouseUp() {
      if (!activeDrag) return;
      const d = activeDrag;
      activeDrag = null;
      if (d.kind === "pan") return;
      const n = d.node, o = d.orig;
      const changed = n && o && (n.x !== o.x || n.y !== o.y || n.w !== o.w || n.h !== o.h);
      if (changed && typeof deps.recordUndo === "function") {
        const cur = { x: n.x, y: n.y, w: n.w, h: n.h };
        Object.assign(n, { x: o.x, y: o.y, w: o.w, h: o.h });
        deps.recordUndo();
        Object.assign(n, cur);
      }
      if (d.kind === "resize" && (d.node.kind === "shape" || d.node.kind === "chart")) renderNodes();
      if (changed) deps.noteActivity?.();
      deps.onStateChange?.(getState());
    }

    function addNode(spec) {
      if (state.nodes.length >= MAX_NODES) throw new Error("보드 요소 수 한도(512개)를 초과했습니다.");
      const norm = normalizeBoardNode(spec, nextId("bn", state.nodes));
      deps.recordUndo?.();
      state.nodes.push(norm);
      selectedNodeId = norm.id;
      selectedConnectorId = null;
      renderAll();
      deps.noteActivity?.();
      return norm;
    }

    function addConnector(spec) {
      if (!spec || typeof spec !== "object") return null;
      if (state.connectors.length >= MAX_CONNECTORS) throw new Error("연결선 수 한도(1,024개)를 초과했습니다.");
      const fromId = String(spec.fromId || ""), toId = String(spec.toId || "");
      if (!fromId || !toId || fromId === toId || !state.nodes.some((n) => n.id === fromId) || !state.nodes.some((n) => n.id === toId)) return null;
      const conn = {
        id: String(spec.id || nextId("bc", state.connectors)).slice(0, 64), fromId, toId,
        fromPort: spec.fromPort || "auto", toPort: spec.toPort || "auto",
        routing: spec.routing === "curved" ? "curved" : "orthogonal", label: String(spec.label ?? "").slice(0, 128),
      };
      deps.recordUndo?.();
      state.connectors.push(conn);
      renderConnectors();
      deps.noteActivity?.();
      deps.onStateChange?.(getState());
      return conn;
    }

    function reorderSelected(toFront) {
      if (!selectedNodeId) return false;
      const idx = state.nodes.findIndex((n) => n.id === selectedNodeId);
      if (idx < 0 || (toFront ? idx === state.nodes.length - 1 : idx === 0)) return false;
      deps.recordUndo?.();
      const [picked] = state.nodes.splice(idx, 1);
      if (toFront) state.nodes.push(picked); else state.nodes.unshift(picked);
      renderAll();
      deps.noteActivity?.();
      return true;
    }

    function deleteSelected() {
      if (selectedConnectorId) {
        const idx = state.connectors.findIndex((c) => c.id === selectedConnectorId);
        if (idx < 0) return false;
        deps.recordUndo?.();
        state.connectors.splice(idx, 1);
        selectedConnectorId = null;
      } else if (selectedNodeId) {
        const idx = state.nodes.findIndex((n) => n.id === selectedNodeId);
        if (idx < 0) return false;
        deps.recordUndo?.();
        const removed = state.nodes[idx].id;
        state.nodes.splice(idx, 1);
        state.connectors = state.connectors.filter((c) => c.fromId !== removed && c.toId !== removed);
        selectedNodeId = null;
      } else return false;
      renderAll();
      deps.noteActivity?.();
      return true;
    }

    async function insertPictureFromBytes(bytes, nameOrMime = "") {
      const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      const fail = (error) => { notify(error); return { ok: false, error }; };
      if (u8.length === 0) return fail("그림을 넣지 못했습니다: 빈 파일입니다.");
      if (u8.length > MAX_SINGLE_IMAGE_BYTES) return fail("그림을 넣지 못했습니다: 최대 32 MiB까지 넣을 수 있습니다.");
      const header = parseImageHeader(u8);
      if (!header) return fail("그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다.");
      if (totalBoardImageBytes() + u8.length > MAX_TOTAL_IMAGE_BYTES) return fail("전체 그림 용량 한도(128 MiB)를 초과했습니다.");
      if (state.nodes.length >= MAX_NODES) return fail("보드 요소 수 한도(512개)를 초과했습니다.");
      if (!utils?.registerDocImage) return fail("이미지 저장소가 초기화되지 않았습니다.");
      const scale = Math.min(1, 320 / Math.max(MIN_NODE_W, header.widthPx), 240 / Math.max(MIN_NODE_H, header.heightPx));
      const offset = (state.nodes.length % 5) * 24;
      const imageId = utils.registerDocImage(utils.bytesToBase64(u8), header.format);
      const alt = String(nameOrMime || "").split(/[\\/]/).pop().replace(/\.(?:png|jpe?g)$/i, "").slice(0, 256);
      const node = addNode({
        kind: "image", x: 140 + offset, y: 120 + offset,
        w: round2(Math.max(120, header.widthPx * scale)), h: round2(Math.max(90, header.heightPx * scale)),
        mime: header.format === "jpg" ? "image/jpeg" : "image/png", imageId, alt, naturalWidthPx: header.widthPx, naturalHeightPx: header.heightPx,
      });
      return { ok: true, node };
    }

    const setZoom = (z) => { state.viewport.zoom = clamp(Math.round(Number(z) || 100), MIN_ZOOM, MAX_ZOOM); applyViewportTransform(); };
    const stepZoom = (delta) => setZoom(delta > 0 ? (ZOOM_STEPS.find((z) => z > state.viewport.zoom) ?? MAX_ZOOM) : ([...ZOOM_STEPS].sort((a, b) => b - a).find((z) => z < state.viewport.zoom) ?? MIN_ZOOM));

    function fitAll() {
      if (state.nodes.length === 0) { state.viewport = { panX: 0, panY: 0, zoom: 100 }; applyViewportTransform(); return; }
      const b = router.analyzeBoard(state).bounds, rect = viewportEl?.getBoundingClientRect?.() || { width: 960, height: 540 };
      const vw = Math.max(240, Number(rect.width) || 960), vh = Math.max(180, Number(rect.height) || 540);
      const z = clamp(Math.round(Math.min(vw / Math.max(1, b.width + 80), vh / Math.max(1, b.height + 80)) * 100), MIN_ZOOM, MAX_ZOOM), s = z / 100;
      state.viewport = { zoom: z, panX: round2((vw - b.width * s) * 0.5 - b.minX * s), panY: round2((vh - b.height * s) * 0.5 - b.minY * s) };
      applyViewportTransform();
    }

    function renderAnalysisDialog() {
      const body = $("board-analysis-body");
      if (!body || !router) return;
      const rep = router.analyzeBoard(state), fmt = (arr) => (arr.length === 0 ? "없음" : arr.slice(0, 6).map((n) => n.label).join(", "));
      const rows = [
        ["전체 요소", String(rep.totalNodes)], ["도형", String(rep.shapeCount)], ["텍스트", String(rep.textCount)],
        ["그림", String(rep.imageCount)], ["차트", String(rep.chartCount)],
        ["연결선", `${rep.connectorCount} (직각 ${rep.orthogonalConnectorCount} · 곡선 ${rep.curvedConnectorCount})`],
        ["연결 그룹", String(rep.connectedComponents)], ["시작 요소", fmt(rep.startNodes)], ["종점 요소", fmt(rep.endNodes)],
        ["고립 요소", fmt(rep.isolatedNodes)], ["순환 연결", rep.hasCycle ? "있음" : "없음"],
        ["최다 연결 요소", rep.topHub ? `${rep.topHub.label} (${rep.topHub.degree}개 연결)` : "없음"],
        ["캔버스 범위", rep.totalNodes ? `${Math.round(rep.bounds.width)} × ${Math.round(rep.bounds.height)} pt` : "0 × 0 pt"],
      ];
      const grid = domEl(doc, "dl", { className: "board-analysis-grid" });
      for (const [k, v] of rows) { grid.appendChild(domEl(doc, "dt", { text: k })); grid.appendChild(domEl(doc, "dd", { text: v })); }
      body.replaceChildren(grid);
    }

    const openAnalysisDialog = () => { renderAnalysisDialog(); kit ? kit.openDialog("board-analysis-dialog") : $("board-analysis-dialog")?.showModal?.(); };

    function openChartDialog(node) {
      if (!node || node.kind !== "chart") return;
      editingChartId = node.id;
      if ($("board-chart-type")) $("board-chart-type").value = node.chartType || "column";
      if ($("board-chart-title")) $("board-chart-title").value = node.title || "";
      if ($("board-chart-categories")) $("board-chart-categories").value = (node.categories || []).join(", ");
      if ($("board-chart-series")) $("board-chart-series").value = (node.series || []).map((s) => `${s.name}: ${(s.values || []).join(", ")}`).join("\n");
      if ($("board-chart-status")) $("board-chart-status").textContent = "";
      kit ? kit.openDialog("board-chart-dialog") : $("board-chart-dialog")?.showModal?.();
    }

    function applyChartDialog() {
      const node = state.nodes.find((n) => n.id === editingChartId);
      if (!node) { kit?.closeDialog("board-chart-dialog"); return; }
      const categories = String($("board-chart-categories")?.value || "").split(",").map((s) => s.trim()).filter(Boolean);
      const series = String($("board-chart-series")?.value || "").split(/\r?\n/).map((line, idx) => {
        const t = line.trim();
        if (!t) return null;
        const c = t.indexOf(":");
        const name = c >= 0 ? t.slice(0, c).trim() || "계열 " + (idx + 1) : "계열 " + (idx + 1);
        const vals = (c >= 0 ? t.slice(c + 1) : t).split(",").map((v) => Number(v.trim())).filter(Number.isFinite);
        return vals.length > 0 ? { name, values: vals } : null;
      }).filter(Boolean);
      if (categories.length === 0 || series.length === 0) {
        if ($("board-chart-status")) $("board-chart-status").textContent = "항목과 최소 1개 이상의 숫자 계열을 입력하세요.";
        return;
      }
      const rawType = $("board-chart-type")?.value || "column";
      deps.recordUndo?.();
      node.chartType = Object.hasOwn(CHART_LABELS, rawType) ? rawType : "column";
      node.title = String($("board-chart-title")?.value || "").trim().slice(0, 160);
      Object.assign(node, sanitizeChart(categories, series));
      editingChartId = null;
      kit ? kit.closeDialog("board-chart-dialog") : $("board-chart-dialog")?.close?.();
      renderNodes();
      deps.noteActivity?.();
    }

    function computeContentBounds() {
      const byId = nodeMap(), routes = [];
      let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      const expand = (x, y) => { if (x < minX) minX = x; if (y < minY) minY = y; if (x > maxX) maxX = x; if (y > maxY) maxY = y; };
      for (const n of state.nodes) { expand(n.x, n.y); expand(n.x + n.w, n.y + n.h); }
      for (const conn of state.connectors) {
        const route = router?.routeConnector(conn, byId, state.nodes);
        if (!route) continue;
        routes.push({ conn, route });
        for (const p of route.points) expand(p.x, p.y);
        if (route.controlPoints) for (const cp of route.controlPoints) expand(cp.x, cp.y);
      }
      if (!Number.isFinite(minX)) return { minX: 0, minY: 0, width: 960, height: 540, routes };
      return { minX, minY, width: Math.max(1, maxX - minX), height: Math.max(1, maxY - minY), routes };
    }

    function buildPrintPage(docArg, rules) {
      const d = docArg || doc;
      if (state.nodes.length === 0) throw new Error("인쇄할 내용이 없습니다.");
      const b = computeContentBounds(), pageW = 1046, pageH = 718, pad = 28;
      const scale = Math.min((pageW - pad * 2) / b.width, (pageH - pad * 2) / b.height, 1.5);
      const offX = round2((pageW - b.width * scale) * 0.5 - b.minX * scale), offY = round2((pageH - b.height * scale) * 0.5 - b.minY * scale);
      const page = domEl(d, "div", { className: ("eo-print-page board-print-page " + (rules?.classFor ? rules.classFor(297, 210) : "")).trim() });
      const stage = domEl(d, "div", { className: "board-print-stage" });
      const svg = svgEl(d, "svg", { class: "board-print-connectors", viewBox: `0 0 ${pageW} ${pageH}`, "aria-hidden": "true" });
      const defs = svgEl(d, "defs"), marker = svgEl(d, "marker", { id: "board-print-arrow", markerWidth: 8, markerHeight: 8, refX: 7, refY: 4, orient: "auto" });
      marker.appendChild(svgEl(d, "path", { d: "M 0 0 L 8 4 L 0 8 z", fill: "#335095" }));
      defs.appendChild(marker);
      svg.appendChild(defs);
      const mapPt = (p) => `${round2(offX + p.x * scale)} ${round2(offY + p.y * scale)}`;
      for (const { route } of b.routes) {
        const dStr = route.routing === "curved" && route.controlPoints?.length === 2
          ? `M ${mapPt(route.points[0])} C ${mapPt(route.controlPoints[0])}, ${mapPt(route.controlPoints[1])}, ${mapPt(route.points[1])}`
          : route.points.map((p, idx) => (idx === 0 ? "M " : "L ") + mapPt(p)).join(" ");
        svg.appendChild(svgEl(d, "path", { d: dStr, fill: "none", stroke: "#335095", "stroke-width": 2, "marker-end": "url(#board-print-arrow)" }));
      }
      stage.appendChild(svg);
      for (const n of state.nodes) {
        const card = domEl(d, "div", { className: `board-node board-node--${n.kind} board-print-node` }, buildNodeContent(d, n, true));
        card.style.left = round2(offX + n.x * scale) + "px";
        card.style.top = round2(offY + n.y * scale) + "px";
        card.style.width = Math.max(12, round2(n.w * scale)) + "px";
        card.style.height = Math.max(12, round2(n.h * scale)) + "px";
        stage.appendChild(card);
      }
      page.appendChild(stage);
      return page;
    }

    function toSlideDeckPdfPayload(title = "보드") {
      const SW = 960, SH = 540, pad = 24, b = computeContentBounds();
      const scale = Math.min((SW - pad * 2) / b.width, (SH - pad * 2) / b.height);
      const offX = (SW - b.width * scale) * 0.5 - b.minX * scale, offY = (SH - b.height * scale) * 0.5 - b.minY * scale;
      const mapX = (x) => round2(clamp(offX + x * scale, 0, SW - 1)), mapY = (y) => round2(clamp(offY + y * scale, 0, SH - 1));
      const elements = [];
      for (const n of state.nodes) {
        if (elements.length >= MAX_SLIDE_ELEMENTS) break;
        const x = mapX(n.x), y = mapY(n.y), w = round2(clamp(n.w * scale, 1, SW - x)), h = round2(clamp(n.h * scale, 1, SH - y));
        if (n.kind === "shape") elements.push({ id: n.id, kind: "shape", x, y, w, h, shapeType: n.shapeType || "rect", text: n.text || "", fill: n.fill || "EFF6FF", stroke: n.stroke || "2563EB", strokeWidthPt: n.strokeWidthPt ?? 1.5, color: n.color || "1E293B", fontSizePt: n.fontSizePt || 10, bold: false, align: "center" });
        else if (n.kind === "text") elements.push({ id: n.id, kind: "textbox", x, y, w, h, text: n.text || "", fontSizePt: n.fontSizePt || 9.5, bold: false, italic: false, underline: false, align: "left", color: n.color || "1F2937", fill: n.fill || "", role: null });
        else if (n.kind === "image") { const data = n.imageId ? utils?.getDocImage?.(n.imageId)?.base64 || "" : ""; if (data) elements.push({ id: n.id, kind: "image", x, y, w, h, mime: n.mime || "image/png", data, alt: n.alt || "" }); }
        else if (n.kind === "chart") elements.push({ id: n.id, kind: "chart", x, y, w, h, chartType: n.chartType || "column", title: n.title || "", legend: n.legend || "right", ...sanitizeChart(n.categories, n.series) });
      }
      const pushSeg = (id, x0, y0, x1, y1) => {
        if (elements.length >= MAX_SLIDE_ELEMENTS) return;
        const sx = round2(clamp(Math.min(x0, x1), 0, SW - 1.5)), sy = round2(clamp(Math.min(y0, y1), 0, SH - 1.5));
        elements.push({ id, kind: "shape", x: sx, y: sy, w: round2(clamp(Math.max(1.5, Math.abs(x1 - x0)), 1, SW - sx)), h: round2(clamp(Math.max(1.5, Math.abs(y1 - y0)), 1, SH - sy)), shapeType: "rect", text: "", fill: "335095", stroke: "335095", strokeWidthPt: 0, color: "FFFFFF", fontSizePt: 8, bold: false, align: "center" });
      };
      for (const { conn, route } of b.routes) {
        if (elements.length >= MAX_SLIDE_ELEMENTS) break;
        if (route.routing === "curved" && route.controlPoints?.length === 2) {
          const p0 = route.points[0], p3 = route.points[1], [c1, c2] = route.controlPoints;
          let prev = { x: mapX(p0.x), y: mapY(p0.y) };
          for (let s = 1; s <= 6; s += 1) {
            const t = s / 6, u = 1 - t;
            const cur = { x: mapX(u * u * u * p0.x + 3 * u * u * t * c1.x + 3 * u * t * t * c2.x + t * t * t * p3.x), y: mapY(u * u * u * p0.y + 3 * u * u * t * c1.y + 3 * u * t * t * c2.y + t * t * t * p3.y) };
            pushSeg(`${conn.id}-c${s}`, prev.x, prev.y, cur.x, cur.y);
            prev = cur;
          }
        } else {
          for (let i = 0; i < route.points.length - 1; i += 1) pushSeg(`${conn.id}-s${i}`, mapX(route.points[i].x), mapY(route.points[i].y), mapX(route.points[i + 1].x), mapY(route.points[i + 1].y));
        }
      }
      return { sourceFormat: "pptx", targetFormat: "pdf", title: String(title || deps.getTitle?.() || "보드"), aspect: "16:9", slides: [{ title: "", bullets: [], notes: "", layout: "blank", transition: "none", elements }] };
    }

    async function exportPdf(title) {
      if (state.nodes.length === 0) { notify("PDF로 내보낼 보드 요소가 없습니다."); return null; }
      const invokeIpc = deps.invokeIpc || host.__EASYOFFICE_INVOKE_BACKEND__;
      if (typeof invokeIpc !== "function") throw new Error("PDF 변환 백엔드를 사용할 수 없습니다.");
      const payload = toSlideDeckPdfPayload(title), res = await invokeIpc("convert_document_format", JSON.stringify(payload));
      const b64 = String(res?.convertedBase64 || "");
      if (!b64) throw new Error("PDF 변환 결과가 비어 있습니다.");
      const baseTitle = String(payload.title || "보드"), fileName = baseTitle + ".pdf";
      if (typeof deps.deliverExport === "function") {
        const delivery = await deps.deliverExport(b64, baseTitle, "pdf");
        if (delivery?.cancelled) { notify("PDF 저장 취소: 저장 위치를 선택하지 않았습니다."); return null; }
      } else {
        const bridge = deps.desktopBridge || host.__EASYOFFICE_DESKTOP_BRIDGE__;
        if (bridge?.isAvailable?.()) await bridge.saveFile(b64, fileName, { inPlace: false });
      }
      notify("PDF 내보내기 완료 (" + fileName + ")");
      return res;
    }

    function createDialogShell(id, titleId, titleText, xId, bodyEl, footSpecs) {
      const dlg = domEl(doc, "dialog", { id, className: "tool-dialog board-dialog", "aria-labelledby": titleId });
      const xBtn = domEl(doc, "button", { id: xId, type: "button", className: "dialog-close", text: "×", "aria-label": "닫기" });
      const head = domEl(doc, "div", { className: "dialog-title" }, [domEl(doc, "span", { id: titleId, text: titleText }), xBtn]);
      const foot = domEl(doc, "div", { className: "dialog-footer" }, footSpecs.map(([fid, ftext, p]) => domEl(doc, "button", { id: fid, type: "button", className: "dialog-btn" + (p ? " dialog-btn-primary" : ""), text: ftext })));
      dlg.appendChild(domEl(doc, "div", { className: "dialog-wrap" }, [head, bodyEl, foot]));
      doc.body.appendChild(dlg);
      return { dlg, xBtn };
    }

    function ensureDialogs() {
      $("board-analysis-dialog")?.remove?.();
      $("board-chart-dialog")?.remove?.();
      {
        const body = domEl(doc, "div", { id: "board-analysis-body", className: "dialog-body" });
        const { dlg, xBtn } = createDialogShell("board-analysis-dialog", "board-analysis-title", "요소 분석", "btn-board-analysis-x", body, [["btn-board-analysis-refresh", "새로 고침", false], ["btn-board-analysis-close", "닫기", true]]);
        $("btn-board-analysis-refresh").addEventListener("click", renderAnalysisDialog);
        const closeFn = () => (kit ? kit.closeDialog("board-analysis-dialog") : dlg.close?.());
        $("btn-board-analysis-close").addEventListener("click", closeFn);
        xBtn.addEventListener("click", closeFn);
      }
      {
        const body = domEl(doc, "div", { className: "dialog-body" });
        const typeSel = domEl(doc, "select", { id: "board-chart-type", className: "dialog-select" }, Object.entries(CHART_LABELS).map(([value, text]) => domEl(doc, "option", { value, text })));
        const titleIn = domEl(doc, "input", { id: "board-chart-title", type: "text", className: "dialog-text-input" });
        const catsIn = domEl(doc, "input", { id: "board-chart-categories", type: "text", className: "dialog-text-input" });
        const serIn = domEl(doc, "textarea", { id: "board-chart-series", className: "dialog-text-input board-chart-series-input" });
        for (const [labelText, field] of [["차트 종류", typeSel], ["제목", titleIn], ["항목", catsIn], ["계열", serIn]]) {
          body.appendChild(domEl(doc, "div", { className: "dialog-row" }, [domEl(doc, "label", { className: "dialog-label", for: field.id, text: labelText }), field]));
        }
        body.appendChild(domEl(doc, "p", { id: "board-chart-status", className: "dialog-pdf-note", role: "status" }));
        const { dlg, xBtn } = createDialogShell("board-chart-dialog", "board-chart-dialog-title", "차트 데이터 편집", "btn-board-chart-x", body, [["btn-board-chart-ok", "확인", true], ["btn-board-chart-cancel", "취소", false]]);
        $("btn-board-chart-ok").addEventListener("click", applyChartDialog);
        const closeFn = () => { editingChartId = null; kit ? kit.closeDialog("board-chart-dialog") : dlg.close?.(); };
        $("btn-board-chart-cancel").addEventListener("click", closeFn);
        xBtn.addEventListener("click", closeFn);
      }
    }

    function mount(targetEl) {
      root = targetEl || root || $("pane-board");
      if (!root) return false;
      const tbBtn = (id, text, aria, pressed) => domEl(doc, "button", { id, type: "button", className: "tb-btn board-tb-btn" + (pressed === "true" ? " active" : ""), text, "aria-label": aria, "aria-pressed": pressed });
      const ctrlBtn = (id, text, aria, extraCls = "") => domEl(doc, "button", { id, type: "button", className: ("board-ctrl-btn " + extraCls).trim(), text, "aria-label": aria });
      const sep = () => domEl(doc, "span", { className: "tb-sep", "aria-hidden": "true" });
      const grp = (cls, items) => domEl(doc, "div", { className: ("tb-group " + cls).trim() }, items);

      shapeSelectEl = domEl(doc, "select", { id: "board-shape-kind", className: "sb-select", "aria-label": "도형 종류" }, Object.entries(SHAPE_LABELS).map(([value, text]) => domEl(doc, "option", { value, text })));
      imageInputEl = domEl(doc, "input", { id: "board-image-file-input", type: "file", accept: ".png,.jpg,.jpeg,image/png,image/jpeg", hidden: true, "aria-label": "그림 파일 선택" });

      const bar = domEl(doc, "div", { id: "board-toolbar", className: "eo-doc-style-bar board-toolbar", role: "toolbar", "aria-label": "보드 도구 모음" }, [
        grp("", [tbBtn("btn-board-select", "선택", null, "true")]),
        sep(),
        grp("", [tbBtn("btn-board-add-shape", "도형"), shapeSelectEl, tbBtn("btn-board-add-text", "텍스트"), tbBtn("btn-board-add-image", "그림"), imageInputEl, tbBtn("btn-board-add-chart", "차트")]),
        sep(),
        grp("", [tbBtn("btn-board-connect-ortho", "직각 연결선", null, "false"), tbBtn("btn-board-connect-curved", "곡선 연결선", null, "false")]),
        sep(),
        grp("", [tbBtn("btn-board-front", "맨 앞으로 가져오기"), tbBtn("btn-board-back", "맨 뒤로 보내기"), tbBtn("btn-board-delete", "개체 지우기")]),
        sep(),
        grp("board-tools-group", [tbBtn("btn-board-analyze", "요소 분석"), tbBtn("btn-board-print", "인쇄"), tbBtn("btn-board-export-pdf", "PDF로 내보내기")]),
      ]);

      zoomLabelEl = ctrlBtn("btn-board-zoom-reset", "100%", "배율 100%로 초기화", "board-zoom-pct");
      const zoomIsland = domEl(doc, "div", { className: "board-controls", role: "group", "aria-label": "확대/축소 제어" }, [
        ctrlBtn("btn-board-zoom-out", "−", "축소"),
        zoomLabelEl,
        ctrlBtn("btn-board-zoom-in", "+", "확대"),
        ctrlBtn("btn-board-fit-all", "전체 맞춤", null, "board-ctrl-fit"),
      ]);

      connectorLayerEl = svgEl(doc, "svg", { id: "board-connector-layer", class: "board-connector-layer", "aria-label": "연결선 영역" });
      nodeLayerEl = domEl(doc, "div", { id: "board-node-layer", className: "board-node-layer" });
      worldEl = domEl(doc, "div", { id: "board-world", className: "board-world" }, [connectorLayerEl, nodeLayerEl]);
      viewportEl = domEl(doc, "div", { id: "board-viewport", className: "board-viewport", role: "region", "aria-label": "보드 캔버스" }, [worldEl, zoomIsland]);
      root.replaceChildren(bar, viewportEl);
      ensureDialogs();

      const on = (id, fn) => $(id).addEventListener("click", fn);
      on("btn-board-select", () => setMode("select"));
      on("btn-board-add-shape", () => { const st = (state.nodes.length % 6) * 28; addNode({ kind: "shape", shapeType: shapeSelectEl.value || "rect", x: 100 + st, y: 100 + st, w: 150, h: 84 }); });
      on("btn-board-add-text", () => { const st = (state.nodes.length % 6) * 28; addNode({ kind: "text", x: 120 + st, y: 120 + st, w: 200, h: 72, text: "텍스트" }); });
      on("btn-board-add-image", () => imageInputEl.click?.());
      imageInputEl.addEventListener("change", async () => { const f = imageInputEl.files?.[0]; if (f) { const buf = await f.arrayBuffer?.(); if (buf) await insertPictureFromBytes(new Uint8Array(buf), f.name); imageInputEl.value = ""; } });
      on("btn-board-add-chart", () => { const st = (state.nodes.length % 5) * 28; addNode({ kind: "chart", chartType: "column", x: 140 + st, y: 140 + st, w: 340, h: 210, title: "분기 실적" }); });
      on("btn-board-connect-ortho", () => setMode(mode === "connect-ortho" ? "select" : "connect-ortho"));
      on("btn-board-connect-curved", () => setMode(mode === "connect-curved" ? "select" : "connect-curved"));
      on("btn-board-front", () => reorderSelected(true));
      on("btn-board-back", () => reorderSelected(false));
      on("btn-board-delete", deleteSelected);
      on("btn-board-analyze", openAnalysisDialog);
      on("btn-board-print", () => host.__EASYOFFICE_OFFICE_PRINT__?.print?.());
      on("btn-board-export-pdf", () => void exportPdf());
      on("btn-board-zoom-out", () => stepZoom(-1));
      on("btn-board-zoom-in", () => stepZoom(1));
      on("btn-board-zoom-reset", () => { state.viewport = { panX: 0, panY: 0, zoom: 100 }; applyViewportTransform(); });
      on("btn-board-fit-all", fitAll);

      viewportEl.addEventListener("mousedown", (ev) => {
        if (ev.button !== 0 || ev.target?.closest?.(".board-node") || ev.target?.closest?.(".board-connector") || ev.target?.closest?.(".board-controls")) return;
        selectNode(null);
        pendingConnect = null;
        if (spacePressed || ev.target === viewportEl || ev.target === worldEl) activeDrag = { kind: "pan", startX: ev.clientX || 0, startY: ev.clientY || 0, origPanX: state.viewport.panX, origPanY: state.viewport.panY };
      });
      viewportEl.addEventListener("wheel", (ev) => {
        ev.preventDefault?.();
        if (ev.ctrlKey || ev.metaKey) stepZoom(ev.deltaY < 0 ? 1 : -1);
        else { state.viewport.panX = round2(clamp(state.viewport.panX - (ev.deltaX || 0), MIN_COORD, MAX_COORD)); state.viewport.panY = round2(clamp(state.viewport.panY - (ev.deltaY || 0), MIN_COORD, MAX_COORD)); applyViewportTransform(); }
      });

      doc.addEventListener("mousemove", onDocMouseMove);
      doc.addEventListener("mouseup", onDocMouseUp);
      doc.addEventListener("keyup", (ev) => { if (ev.key === " ") spacePressed = false; });
      renderAll();
      return true;
    }

    function handleKeydown(ev) {
      if (deps.getActiveModule && deps.getActiveModule() !== "BOARD") return false;
      if (ev.key === " ") spacePressed = ev.type !== "keyup";
      if (doc.querySelector("dialog[open]")) return false;
      const t = ev.target || doc.activeElement, tag = String(t?.tagName || "").toUpperCase();
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || t?.getAttribute?.("contenteditable") === "plaintext-only") return false;
      if (ev.key === "Escape") { pendingConnect = null; setMode("select"); selectNode(null); return true; }
      if ((ev.key === "Delete" || ev.key === "Backspace") && deleteSelected()) { ev.preventDefault?.(); return true; }
      return false;
    }

    const getState = () => ({ nodes: state.nodes.map((n) => ({ ...n })), connectors: state.connectors.map((c) => ({ ...c })), viewport: { ...state.viewport } });

    function replaceState(nextState) {
      const next = normalizeBoardState(nextState, false);
      state.nodes = next.nodes; state.connectors = next.connectors; state.viewport = next.viewport;
      selectedNodeId = null; selectedConnectorId = null; pendingConnect = null;
      renderAll();
      if (next.dropped > 0) notify("복원하지 못한 항목 " + next.dropped + "개를 건너뛰었습니다.");
      return { dropped: next.dropped };
    }

    const getConnectorRoute = (id) => routeCache.get(id) || (state.connectors.find((c) => c.id === id) ? router?.routeConnector(state.connectors.find((c) => c.id === id), nodeMap(), state.nodes) : null);

    if (root) mount(root);
    const api = {
      mount, getState, replaceState, addNode, addConnector, selectNode, selectConnector, deleteSelected,
      bringToFront: () => reorderSelected(true), sendToBack: () => reorderSelected(false),
      insertPictureFromBytes, setZoom, stepZoom, fitAll, setMode,
      analyze: () => router?.analyzeBoard(state), openAnalysisDialog, openChartDialog, getConnectorRoute,
      buildPrintPage, toSlideDeckPdfPayload, exportPdf, handleKeydown, render: renderAll,
    };
    host.__EASYOFFICE_BOARD_STUDIO__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_BOARD_STUDIO__ = createBoardStudio;
  host.__EASYOFFICE_BOARD_CODEC__ = { SHAPE_LABELS, CHART_LABELS, createInitialBoardState, normalizeBoardNode, normalizeBoardState };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
