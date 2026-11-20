// 참조 구현 (reference implementation): rhwp-studio `src/i18n/locales/ko.ts` lines 132, 135, 188, 202, 335, 1274, 1300
// (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`) for Korean object, shape, chart, and alignment labels.

(function (host) {
  "use strict";

  const SVG_NS = "http://www.w3.org/2000/svg";
  const STAGE_SIZES = Object.freeze({
    "16:9": Object.freeze({ w: 960, h: 540 }),
    "4:3": Object.freeze({ w: 720, h: 540 }),
    A4: Object.freeze({ w: 841.89, h: 595.28 }),
  });
  const LAYOUT_LABELS = Object.freeze({
    titleSlide: "제목 슬라이드", titleAndContent: "제목 및 내용",
    sectionHeader: "구역 머리글", twoColumn: "두 개의 내용", blank: "빈 화면",
  });
  const SHAPE_LABELS = Object.freeze({
    rect: "직사각형", roundRect: "모서리가 둥근 직사각형", ellipse: "타원",
    rightArrow: "오른쪽 화살표", callout: "설명선", diamond: "마름모",
  });
  const CHART_LABELS = Object.freeze({
    column: "묶은 세로 막대형", bar: "묶은 가로 막대형", line: "꺾은선형", pie: "원형", scatter: "분산형",
  });
  const TRANSITIONS = new Set(["none", "fade", "push", "zoom"]);
  const ELEMENT_KINDS = new Set(["textbox", "shape", "image", "table", "chart"]);
  const ALIGNMENTS = new Set(["left", "center", "right", "justify"]);
  const CHART_LEGENDS = new Set(["right", "bottom", "top", "left", "none"]);
  const RESIZE_DIRS = Object.freeze(["nw", "n", "ne", "e", "se", "s", "sw", "w"]);

  const SNAP_TOLERANCE_PT = 6;
  const MIN_EL_W = 24;
  const MIN_EL_H = 20;
  const MAX_SLIDES = 512;
  const MAX_ELEMENTS_PER_SLIDE = 256;
  const MAX_SINGLE_IMAGE_BYTES = 32 * 1024 * 1024;
  const MAX_TOTAL_IMAGE_BYTES = 128 * 1024 * 1024;
  const MAX_DIMENSION_PX = 65535;
  const MAX_TABLE_ROWS = 64;
  const MAX_TABLE_COLS = 32;
  const MAX_CHART_POINTS = 128;
  const MAX_CHART_SERIES = 16;
  const MAX_TEXT_CHARS = 16384;
  const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  const IHDR_SIG = [0x49, 0x48, 0x44, 0x52];

  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const round2 = (v) => Math.round(v * 100) / 100;
  const readU32BE = (u8, at) => (((u8[at] << 24) >>> 0) + (u8[at + 1] << 16) + (u8[at + 2] << 8) + u8[at + 3]) >>> 0;
  const stageSizeOf = (aspect) => STAGE_SIZES[aspect] || STAGE_SIZES["16:9"];
  const normalizeLayout = (raw) => (Object.hasOwn(LAYOUT_LABELS, raw) ? raw : "titleAndContent");
  const normalizeTransition = (raw) => (TRANSITIONS.has(raw) ? raw : "fade");

  function normalizeHex6(raw, fallback) {
    if (raw === null || raw === undefined || raw === "") return fallback;
    const clean = String(raw).trim().replace(/^#/, "");
    if (/^[0-9A-Fa-f]{6}$/.test(clean)) return clean.toUpperCase();
    if (/^[0-9A-Fa-f]{3}$/.test(clean)) return (clean[0] + clean[0] + clean[1] + clean[1] + clean[2] + clean[2]).toUpperCase();
    return fallback;
  }

  function parseImageHeaderFallback(u8) {
    if (!(u8 instanceof Uint8Array) || u8.length < 4) return null;
    const extParser = host.__EASYOFFICE_SHEET_LAYOUT__?.parseImageHeader || host.__EASYOFFICE_DOC_IMAGE_API__?.parseImageHeader;
    if (typeof extParser === "function") return extParser(u8);
    if (u8.length >= 24 && readU32BE(u8, 8) === 13) {
      if (PNG_MAGIC.every((b, i) => u8[i] === b) && IHDR_SIG.every((b, i) => u8[12 + i] === b)) {
        const widthPx = readU32BE(u8, 16), heightPx = readU32BE(u8, 20);
        if (widthPx >= 1 && heightPx >= 1 && widthPx <= MAX_DIMENSION_PX && heightPx <= MAX_DIMENSION_PX) {
          return { format: "png", widthPx, heightPx };
        }
      }
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
        if (marker === 0xc0 || marker === 0xc1 || marker === 0xc2) {
          if (segLen < 7) return null;
          const heightPx = (u8[pos + 3] << 8) | u8[pos + 4], widthPx = (u8[pos + 5] << 8) | u8[pos + 6];
          return widthPx >= 1 && heightPx >= 1 ? { format: "jpg", widthPx, heightPx } : null;
        }
        pos += segLen;
      }
    }
    return null;
  }

  function altFromHint(nameOrMime) {
    const raw = String(nameOrMime || "").trim();
    if (!raw || raw.includes("/")) return "";
    return raw.split(/[\\/]/).pop().trim().replace(/\.(?:png|jpe?g)$/i, "").slice(0, 512);
  }

  function sanitizeRows(rawRows) {
    if (!Array.isArray(rawRows) || rawRows.length === 0) return [["구분", "목표", "실적"], ["1분기", "100", "115"]];
    const rCount = Math.min(MAX_TABLE_ROWS, rawRows.length);
    let cCount = 1;
    for (let r = 0; r < rCount; r += 1) {
      if (Array.isArray(rawRows[r])) cCount = Math.max(cCount, Math.min(MAX_TABLE_COLS, rawRows[r].length));
    }
    const out = [];
    for (let r = 0; r < rCount; r += 1) {
      const srcRow = Array.isArray(rawRows[r]) ? rawRows[r] : [];
      const row = [];
      for (let c = 0; c < cCount; c += 1) row.push(String(srcRow[c] ?? "").slice(0, 4096));
      out.push(row);
    }
    return out;
  }

  function sanitizeChartSeries(rawCategories, rawSeries) {
    const categories = Array.isArray(rawCategories) && rawCategories.length > 0
      ? rawCategories.slice(0, MAX_CHART_POINTS).map((c) => String(c ?? "").slice(0, 256))
      : ["1분기", "2분기", "3분기"];
    const srcSeries = Array.isArray(rawSeries) && rawSeries.length > 0
      ? rawSeries.slice(0, MAX_CHART_SERIES)
      : [{ name: "매출", values: [120, 165, 210] }];
    const series = srcSeries.map((s, idx) => {
      const vals = Array.isArray(s?.values) ? s.values.slice(0, MAX_CHART_POINTS) : [];
      return {
        name: String(s?.name || "계열 " + (idx + 1)).slice(0, 128),
        values: categories.map((_, i) => (Number.isFinite(Number(vals[i])) ? Number(vals[i]) : 0)),
      };
    });
    return { categories, series };
  }

  function normalizeSlideElement(raw, aspect = "16:9", fallbackId = "el-1") {
    if (!raw || typeof raw !== "object") return null;
    const rawKind = String(raw.kind || "").trim();
    const kind = rawKind === "textBox" ? "textbox" : rawKind;
    if (!ELEMENT_KINDS.has(kind)) return null;
    const stage = stageSizeOf(aspect);
    const id = String(raw.id || fallbackId).trim().slice(0, 64) || fallbackId;
    const defW = kind === "chart" ? 380 : kind === "table" ? 360 : kind === "image" ? 240 : kind === "shape" ? 200 : 320;
    const defH = kind === "chart" ? 200 : kind === "table" ? 150 : kind === "image" ? 160 : kind === "shape" ? 100 : 80;
    const w = round2(clamp(Number.isFinite(Number(raw.w)) ? Number(raw.w) : defW, MIN_EL_W, stage.w));
    const h = round2(clamp(Number.isFinite(Number(raw.h)) ? Number(raw.h) : defH, MIN_EL_H, stage.h));
    const x = round2(clamp(Number.isFinite(Number(raw.x)) ? Number(raw.x) : (stage.w - w) / 2, 0, Math.max(0, stage.w - MIN_EL_W)));
    const y = round2(clamp(Number.isFinite(Number(raw.y)) ? Number(raw.y) : (stage.h - h) / 2, 0, Math.max(0, stage.h - MIN_EL_H)));

    if (kind === "textbox") {
      return {
        id, kind, x, y, w, h,
        text: String(raw.text ?? "글상자").slice(0, MAX_TEXT_CHARS),
        fontSizePt: clamp(Number(raw.fontSizePt) || 18, 8, 96),
        bold: Boolean(raw.bold), italic: Boolean(raw.italic), underline: Boolean(raw.underline),
        align: ALIGNMENTS.has(raw.align) ? raw.align : "left",
        color: normalizeHex6(raw.color, "1F2937"),
        fill: raw.fill === null || raw.fill === "" ? "" : normalizeHex6(raw.fill, ""),
        role: raw.role ? String(raw.role).slice(0, 32) : null,
      };
    }
    if (kind === "shape") {
      return {
        id, kind, x, y, w, h,
        shapeType: Object.hasOwn(SHAPE_LABELS, raw.shapeType) ? raw.shapeType : "rect",
        text: String(raw.text ?? "").slice(0, MAX_TEXT_CHARS),
        fill: normalizeHex6(raw.fill, "2563EB"), stroke: normalizeHex6(raw.stroke, "1D4ED8"),
        strokeWidthPt: clamp(Number(raw.strokeWidthPt) >= 0 ? Number(raw.strokeWidthPt) : 2, 0, 24),
        color: normalizeHex6(raw.color, "FFFFFF"), fontSizePt: clamp(Number(raw.fontSizePt) || 16, 8, 96),
        bold: raw.bold !== undefined ? Boolean(raw.bold) : true,
        italic: Boolean(raw.italic), underline: Boolean(raw.underline),
        align: ALIGNMENTS.has(raw.align) ? raw.align : "center",
      };
    }
    if (kind === "image") {
      const utils = host.__EASYOFFICE_SHEET_UTILS__;
      let imageId = raw.imageId ? String(raw.imageId) : "";
      let mime = raw.mime === "image/jpeg" ? "image/jpeg" : "image/png";
      let naturalWidthPx = Number(raw.naturalWidthPx) || 0, naturalHeightPx = Number(raw.naturalHeightPx) || 0;
      const rawB64 = typeof raw.data === "string" && raw.data.length > 0
        ? raw.data
        : (typeof raw.dataBase64 === "string" ? raw.dataBase64 : "");
      if (!imageId && rawB64.length > 0 && utils) {
        let bytes = null;
        try { bytes = utils.base64ToBytes(rawB64); } catch { bytes = null; }
        if (!bytes || bytes.length === 0 || bytes.length > MAX_SINGLE_IMAGE_BYTES) return null;
        const header = parseImageHeaderFallback(bytes);
        if (!header || utils.imageAreaProblem(header)) return null;
        imageId = utils.registerDocImage(rawB64, header.format);
        mime = header.format === "jpg" ? "image/jpeg" : "image/png";
        naturalWidthPx = header.widthPx;
        naturalHeightPx = header.heightPx;
      }
      if (!imageId) return null;
      return {
        id, kind, x, y, w, h, mime, imageId,
        alt: String(raw.alt ?? "").slice(0, 1024),
        naturalWidthPx: naturalWidthPx || Math.round(w), naturalHeightPx: naturalHeightPx || Math.round(h),
      };
    }
    if (kind === "table") {
      return {
        id, kind, x, y, w, h,
        headerRow: raw.headerRow !== undefined ? Boolean(raw.headerRow) : true,
        fill: normalizeHex6(raw.fill, "DBEAFE"), rows: sanitizeRows(raw.rows),
      };
    }
    const { categories, series } = sanitizeChartSeries(raw.categories, raw.series);
    return {
      id, kind: "chart", x, y, w, h,
      chartType: Object.hasOwn(CHART_LABELS, raw.chartType) ? raw.chartType : "column",
      title: String(raw.title ?? "").slice(0, 160),
      legend: CHART_LEGENDS.has(raw.legend) ? raw.legend : "right",
      categories, series,
    };
  }

  function serializeElementForWire(el) {
    if (!el || !ELEMENT_KINDS.has(el.kind)) return null;
    const base = { id: el.id, kind: el.kind, x: round2(el.x), y: round2(el.y), w: round2(el.w), h: round2(el.h) };
    if (el.kind === "textbox") {
      return {
        ...base, text: String(el.text ?? ""), fontSizePt: el.fontSizePt || 18,
        bold: Boolean(el.bold), italic: Boolean(el.italic), underline: Boolean(el.underline),
        align: el.align || "left", color: normalizeHex6(el.color, "1F2937"),
        fill: el.fill ? normalizeHex6(el.fill, "") : "", role: el.role ?? null,
      };
    }
    if (el.kind === "shape") {
      return {
        ...base, shapeType: Object.hasOwn(SHAPE_LABELS, el.shapeType) ? el.shapeType : "rect",
        text: String(el.text ?? ""), fill: normalizeHex6(el.fill, "2563EB"),
        stroke: normalizeHex6(el.stroke, "1D4ED8"), strokeWidthPt: Number(el.strokeWidthPt ?? 2),
        color: normalizeHex6(el.color, "FFFFFF"), fontSizePt: el.fontSizePt || 16,
        bold: Boolean(el.bold), align: el.align || "center",
      };
    }
    if (el.kind === "image") {
      const utils = host.__EASYOFFICE_SHEET_UTILS__;
      const data = (el.imageId && utils?.getDocImage?.(el.imageId)?.base64) || el.data || "";
      if (!data) return null;
      return { ...base, mime: el.mime === "image/jpeg" ? "image/jpeg" : "image/png", data, alt: String(el.alt ?? "") };
    }
    if (el.kind === "table") {
      return { ...base, headerRow: Boolean(el.headerRow), fill: normalizeHex6(el.fill, "DBEAFE"), rows: sanitizeRows(el.rows) };
    }
    const { categories, series } = sanitizeChartSeries(el.categories, el.series);
    return {
      ...base, chartType: Object.hasOwn(CHART_LABELS, el.chartType) ? el.chartType : "column",
      title: String(el.title ?? ""), legend: CHART_LEGENDS.has(el.legend) ? el.legend : "right",
      categories, series,
    };
  }

  function serializeDeckPayload(slideDeckState, document, extractEditableDocumentText) {
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const rawAspect = slideDeckState[0]?.aspect;
    const aspect = Object.hasOwn(STAGE_SIZES, rawAspect) ? rawAspect : "16:9";
    const slides = slideDeckState.map((s) => {
      let extraText = "";
      if (Array.isArray(s.insertedNodes) && s.insertedNodes.length > 0 && utils?.restoreDomChildren) {
        const extraDiv = document.createElement("div");
        extraDiv.className = "sheet-table";
        utils.restoreDomChildren(extraDiv, s.insertedNodes, document);
        extraText = typeof extractEditableDocumentText === "function" ? extractEditableDocumentText(extraDiv) : (extraDiv.textContent || "");
      }
      const bullets = [s.lower || "", extraText].join("\n").split(/\n+/).map((t) => t.trim()).filter(Boolean);
      const elements = (Array.isArray(s.elements) ? s.elements : []).map(serializeElementForWire).filter(Boolean);
      return {
        title: (s.title || "").trim(), bullets, notes: (s.notes || "").trim(),
        layout: normalizeLayout(s.layout), transition: normalizeTransition(s.transition), elements,
      };
    });
    return { aspect, slides };
  }

  function hydrateDecodedDeck(res, createEmptySlideStyleFields) {
    const aspect = Object.hasOwn(STAGE_SIZES, res?.aspect) ? res.aspect : "16:9";
    const srcSlides = Array.isArray(res?.slides) && res.slides.length > 0
      ? res.slides.slice(0, MAX_SLIDES)
      : [{ title: "", bullets: [], notes: "" }];
    return srcSlides.map((s) => {
      const rawEls = Array.isArray(s?.elements) ? s.elements.slice(0, MAX_ELEMENTS_PER_SLIDE) : [];
      const elements = [];
      for (let i = 0; i < rawEls.length; i += 1) {
        const norm = normalizeSlideElement(rawEls[i], aspect, "el-" + (i + 1));
        if (norm) elements.push(norm);
      }
      return {
        title: String(s?.title || ""), lower: Array.isArray(s?.bullets) ? s.bullets.join("\n") : String(s?.lower || ""),
        notes: String(s?.notes || ""), transition: normalizeTransition(s?.transition),
        ...createEmptySlideStyleFields(), aspect, layout: normalizeLayout(s?.layout), elements,
      };
    });
  }

  function buildShapeSvg(document, el) {
    const vw = Math.max(24, Math.round(el.w || 200)), vh = Math.max(20, Math.round(el.h || 100));
    const sw = clamp(Number(el.strokeWidthPt ?? 2), 0, 24), pad = Math.max(1, sw / 2);
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("class", "slide-el-shape-svg");
    svg.setAttribute("viewBox", "0 0 " + vw + " " + vh);
    svg.setAttribute("preserveAspectRatio", "none");
    svg.setAttribute("aria-hidden", "true");

    let geom;
    const st = el.shapeType || "rect";
    if (st === "ellipse") {
      geom = document.createElementNS(SVG_NS, "ellipse");
      geom.setAttribute("cx", String(round2(vw / 2)));
      geom.setAttribute("cy", String(round2(vh / 2)));
      geom.setAttribute("rx", String(round2(Math.max(2, vw / 2 - pad))));
      geom.setAttribute("ry", String(round2(Math.max(2, vh / 2 - pad))));
    } else if (st === "diamond" || st === "rightArrow" || st === "callout") {
      geom = document.createElementNS(SVG_NS, "polygon");
      let pts;
      if (st === "diamond") {
        pts = [round2(vw / 2) + "," + round2(pad), round2(vw - pad) + "," + round2(vh / 2), round2(vw / 2) + "," + round2(vh - pad), round2(pad) + "," + round2(vh / 2)];
      } else if (st === "rightArrow") {
        const headX = round2(Math.max(pad + 6, vw * 0.66)), yTop = round2(vh * 0.26), yBot = round2(vh * 0.74);
        pts = [round2(pad) + "," + yTop, headX + "," + yTop, headX + "," + round2(pad), round2(vw - pad) + "," + round2(vh / 2), headX + "," + round2(vh - pad), headX + "," + yBot, round2(pad) + "," + yBot];
      } else {
        const bodyBot = round2(Math.max(pad + 8, vh * 0.78));
        pts = [round2(pad) + "," + round2(pad), round2(vw - pad) + "," + round2(pad), round2(vw - pad) + "," + bodyBot, round2(vw * 0.36) + "," + bodyBot, round2(vw * 0.16) + "," + round2(vh - pad), round2(vw * 0.22) + "," + bodyBot, round2(pad) + "," + bodyBot];
      }
      geom.setAttribute("points", pts.join(" "));
    } else {
      geom = document.createElementNS(SVG_NS, "rect");
      geom.setAttribute("x", String(round2(pad)));
      geom.setAttribute("y", String(round2(pad)));
      geom.setAttribute("width", String(round2(Math.max(2, vw - pad * 2))));
      geom.setAttribute("height", String(round2(Math.max(2, vh - pad * 2))));
      if (st === "roundRect") {
        const r = String(round2(Math.min(18, Math.min(vw, vh) * 0.2)));
        geom.setAttribute("rx", r);
        geom.setAttribute("ry", r);
      }
    }
    geom.setAttribute("fill", "#" + normalizeHex6(el.fill, "2563EB"));
    geom.setAttribute("stroke", sw > 0 ? "#" + normalizeHex6(el.stroke, "1D4ED8") : "none");
    geom.setAttribute("stroke-width", String(sw));
    svg.appendChild(geom);
    return svg;
  }

  function buildSlideChartSvg(document, el, forPrint) {
    const svgApi = host.__EASYOFFICE_SHEET_CHART_SVG__, utils = host.__EASYOFFICE_SHEET_UTILS__;
    if (!svgApi?.renderChartSvg || !utils?.colIndexToLetters) return null;
    const { categories, series } = sanitizeChartSeries(el.categories, el.series);
    const map = Object.create(null);
    map.A1 = { value: "", t: "s" };
    const isScatter = el.chartType === "scatter";
    for (let r = 0; r < categories.length; r += 1) {
      const cat = String(categories[r] ?? ""), asNum = Number(cat);
      map["A" + (r + 2)] = isScatter && cat !== "" && Number.isFinite(asNum) ? { value: String(asNum) } : { value: cat, t: "s" };
    }
    for (let s = 0; s < series.length; s += 1) {
      const col = utils.colIndexToLetters(s + 1);
      map[col + "1"] = { value: String(series[s].name || "계열 " + (s + 1)), t: "s" };
      for (let r = 0; r < categories.length; r += 1) {
        const v = series[s].values[r];
        map[col + (r + 2)] = { value: Number.isFinite(v) ? String(v) : "0" };
      }
    }
    const dataRange = "A1:" + utils.colIndexToLetters(series.length) + (categories.length + 1);
    const svg = svgApi.renderChartSvg({
      document,
      chart: { chartType: el.chartType || "column", title: el.title || "", legend: el.legend || "right", dataRange },
      sheetName: "SlideChart", sheetDataStore: { SlideChart: map },
      width: Math.max(160, Math.round(el.w || 380)), height: Math.max(110, Math.round(el.h || 200)),
      forPrint: Boolean(forPrint),
    });
    if (svg) { svg.style.width = "100%"; svg.style.height = "100%"; }
    return svg;
  }

  function applyTextStyles(node, el) {
    node.style.fontSize = (el.fontSizePt || 18) + "pt";
    node.style.fontWeight = el.bold ? "700" : "";
    node.style.fontStyle = el.italic ? "italic" : "";
    node.style.textDecoration = el.underline ? "underline" : "";
    node.style.textAlign = el.align || "left";
    if (el.color) node.style.color = "#" + normalizeHex6(el.color, "1F2937");
  }

  function positionElementDom(card, el, stage) {
    card.style.left = round2((el.x / stage.w) * 100) + "%";
    card.style.top = round2((el.y / stage.h) * 100) + "%";
    card.style.width = round2((el.w / stage.w) * 100) + "%";
    card.style.height = round2((el.h / stage.h) * 100) + "%";
  }

  function buildElementDom(document, el, stage, options = {}) {
    const { interactive = false, selected = false, forPrint = false } = options;
    const card = document.createElement("div");
    card.className = "slide-el slide-el--" + el.kind + (selected ? " is-selected" : "");
    card.dataset.elementId = el.id;
    card.dataset.kind = el.kind;
    if (interactive) card.tabIndex = 0;
    positionElementDom(card, el, stage);

    if (el.kind === "textbox") {
      const box = document.createElement("div");
      box.className = "slide-el-textbox";
      box.textContent = el.text || "";
      applyTextStyles(box, el);
      if (el.fill) card.style.backgroundColor = "#" + normalizeHex6(el.fill, "F3F4F6");
      card.appendChild(box);
    } else if (el.kind === "shape") {
      card.dataset.shapeType = el.shapeType || "rect";
      card.appendChild(buildShapeSvg(document, el));
      const label = document.createElement("div");
      label.className = "slide-el-shape-text";
      label.textContent = el.text || "";
      applyTextStyles(label, { ...el, align: el.align || "center", color: el.color || "FFFFFF" });
      card.appendChild(label);
    } else if (el.kind === "image") {
      const utils = host.__EASYOFFICE_SHEET_UTILS__;
      const img = document.createElement("img");
      img.className = "slide-el-image";
      const url = el.imageId ? utils?.docImageUrl?.(el.imageId) : "";
      if (url) img.setAttribute("src", url);
      else {
        const entry = el.imageId ? utils?.getDocImage?.(el.imageId) : null;
        if (entry?.base64) img.setAttribute("src", "data:" + (el.mime || "image/png") + ";base64," + entry.base64);
      }
      img.setAttribute("alt", el.alt || "");
      img.setAttribute("draggable", "false");
      if (el.imageId) img.dataset.eoImageId = el.imageId;
      card.appendChild(img);
    } else if (el.kind === "table") {
      const tbl = document.createElement("table");
      tbl.className = "slide-el-table";
      const rows = sanitizeRows(el.rows);
      const headFill = el.fill ? "#" + normalizeHex6(el.fill, "DBEAFE") : "";
      let bodyStart = 0;
      if (el.headerRow && rows.length > 0) {
        const thead = document.createElement("thead"), tr = document.createElement("tr");
        rows[0].forEach((cellText, cIdx) => {
          const th = document.createElement("th");
          th.textContent = cellText;
          th.dataset.rowIdx = "0";
          th.dataset.colIdx = String(cIdx);
          if (headFill) th.style.backgroundColor = headFill;
          tr.appendChild(th);
        });
        thead.appendChild(tr);
        tbl.appendChild(thead);
        bodyStart = 1;
      }
      const tbody = document.createElement("tbody");
      for (let r = bodyStart; r < rows.length; r += 1) {
        const tr = document.createElement("tr");
        rows[r].forEach((cellText, cIdx) => {
          const td = document.createElement("td");
          td.textContent = cellText;
          td.dataset.rowIdx = String(r);
          td.dataset.colIdx = String(cIdx);
          tr.appendChild(td);
        });
        tbody.appendChild(tr);
      }
      tbl.appendChild(tbody);
      card.appendChild(tbl);
    } else if (el.kind === "chart") {
      const svg = buildSlideChartSvg(document, el, forPrint);
      if (svg) card.appendChild(svg);
    }
    if (interactive) {
      for (const dir of RESIZE_DIRS) {
        const handle = document.createElement("span");
        handle.className = "slide-el-handle";
        handle.dataset.dir = dir;
        handle.setAttribute("aria-hidden", "true");
        card.appendChild(handle);
      }
    }
    return card;
  }

  function renderStaticElementsLayer(document, slide, aspect = "16:9", options = {}) {
    const els = Array.isArray(slide?.elements) ? slide.elements : [];
    const layer = document.createElement("div");
    layer.className = "slide-canvas-layer slide-canvas-layer--static";
    const stage = stageSizeOf(aspect);
    for (let i = 0; i < els.length; i += 1) {
      const norm = normalizeSlideElement(els[i], aspect, "el-" + (i + 1));
      if (norm) layer.appendChild(buildElementDom(document, norm, stage, { interactive: false, forPrint: Boolean(options.forPrint) }));
    }
    return layer;
  }

  function computeSmartSnap(box, siblings, stage, snapEnabled, mode, dir) {
    if (!snapEnabled) return { x: box.x, y: box.y, w: box.w, h: box.h, vLine: null, hLine: null };
    let { x, y, w, h } = box;
    let bestDx = SNAP_TOLERANCE_PT + 0.001, bestDy = SNAP_TOLERANCE_PT + 0.001;
    let vLine = null, hLine = null;
    const vTargets = [stage.w / 2], hTargets = [stage.h / 2];
    for (let i = 0; i < siblings.length; i += 1) {
      const s = siblings[i];
      vTargets.push(s.x, s.x + s.w / 2, s.x + s.w);
      hTargets.push(s.y, s.y + s.h / 2, s.y + s.h);
    }
    if (mode === "move") {
      const myX = [0, w / 2, w], myY = [0, h / 2, h];
      for (let t = 0; t < vTargets.length; t += 1) {
        for (let m = 0; m < 3; m += 1) {
          const ad = Math.abs(vTargets[t] - (x + myX[m]));
          if (ad <= SNAP_TOLERANCE_PT && ad < bestDx) { bestDx = ad; vLine = vTargets[t]; x = round2(vTargets[t] - myX[m]); }
        }
      }
      for (let t = 0; t < hTargets.length; t += 1) {
        for (let m = 0; m < 3; m += 1) {
          const ad = Math.abs(hTargets[t] - (y + myY[m]));
          if (ad <= SNAP_TOLERANCE_PT && ad < bestDy) { bestDy = ad; hLine = hTargets[t]; y = round2(hTargets[t] - myY[m]); }
        }
      }
      x = clamp(x, 0, Math.max(0, stage.w - w));
      y = clamp(y, 0, Math.max(0, stage.h - h));
    } else {
      if (dir.includes("e")) {
        const right = x + w;
        for (let t = 0; t < vTargets.length; t += 1) {
          const ad = Math.abs(vTargets[t] - right);
          if (ad <= SNAP_TOLERANCE_PT && ad < bestDx) { bestDx = ad; vLine = vTargets[t]; w = Math.max(MIN_EL_W, round2(vTargets[t] - x)); }
        }
      } else if (dir.includes("w")) {
        const right = x + w;
        for (let t = 0; t < vTargets.length; t += 1) {
          const ad = Math.abs(vTargets[t] - x);
          if (ad <= SNAP_TOLERANCE_PT && ad < bestDx && right - vTargets[t] >= MIN_EL_W) { bestDx = ad; vLine = vTargets[t]; x = round2(vTargets[t]); w = round2(right - x); }
        }
      }
      if (dir.includes("s")) {
        const bottom = y + h;
        for (let t = 0; t < hTargets.length; t += 1) {
          const ad = Math.abs(hTargets[t] - bottom);
          if (ad <= SNAP_TOLERANCE_PT && ad < bestDy) { bestDy = ad; hLine = hTargets[t]; h = Math.max(MIN_EL_H, round2(hTargets[t] - y)); }
        }
      } else if (dir.includes("n")) {
        const bottom = y + h;
        for (let t = 0; t < hTargets.length; t += 1) {
          const ad = Math.abs(hTargets[t] - y);
          if (ad <= SNAP_TOLERANCE_PT && ad < bestDy && bottom - hTargets[t] >= MIN_EL_H) { bestDy = ad; hLine = hTargets[t]; y = round2(hTargets[t]); h = round2(bottom - y); }
        }
      }
    }
    return { x: round2(x), y: round2(y), w: round2(w), h: round2(h), vLine, hLine };
  }

  function createSlideCanvas(deps, hooks = {}) {
    const {
      document, slideDeckState: deck, getActiveSlideIndex, getActiveModule,
      getPanels, pushUndoSnapshot, captureWorkspaceSnapshot, pushRawUndoSnapshot, noteActivity,
    } = deps;
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const $ = (id) => document.getElementById(id);

    let selectedId = null, snapEnabled = true, activeDrag = null, editingSessionKey = "", onElementDblClick = null;

    const say = (msg) => { const fb = $("status-command-feedback"); if (fb) fb.textContent = msg; };
    const currentAspect = () => (Object.hasOwn(STAGE_SIZES, deck[0]?.aspect) ? deck[0].aspect : "16:9");
    const currentSlide = () => deck[getActiveSlideIndex()] || deck[0] || null;
    function ensureElements(slide = currentSlide()) {
      if (!slide) return [];
      if (!Array.isArray(slide.elements)) slide.elements = [];
      return slide.elements;
    }
    function nextElementId(elements) {
      const used = new Set(elements.map((e) => String(e.id || "")));
      let n = elements.length + 1;
      while (used.has("el-" + n)) n += 1;
      return "el-" + n;
    }
    function hideSnapGuides() {
      const snapLayer = $("slide-snap-layer");
      if (!snapLayer) return;
      snapLayer.replaceChildren();
      snapLayer.hidden = true;
    }
    function renderSnapGuides(vLine, hLine, stage) {
      const snapLayer = $("slide-snap-layer");
      if (!snapLayer) return;
      snapLayer.replaceChildren();
      if (vLine === null && hLine === null) { snapLayer.hidden = true; return; }
      if (vLine !== null) {
        const line = document.createElement("div");
        line.className = "slide-snap-line slide-snap-line-v";
        line.style.left = round2((vLine / stage.w) * 100) + "%";
        snapLayer.appendChild(line);
      }
      if (hLine !== null) {
        const line = document.createElement("div");
        line.className = "slide-snap-line slide-snap-line-h";
        line.style.top = round2((hLine / stage.h) * 100) + "%";
        snapLayer.appendChild(line);
      }
      snapLayer.hidden = false;
    }
    function beginInlineUndoOnce(key) {
      if (editingSessionKey === key) return;
      const pre = captureWorkspaceSnapshot(false);
      pre.slides = structuredClone(deck);
      pushRawUndoSnapshot(pre);
      editingSessionKey = key;
    }

    function bindEditableNode(node, key, onCommit) {
      editingSessionKey = "";
      node.setAttribute("contenteditable", "plaintext-only");
      node.focus?.();
      const onInput = () => { beginInlineUndoOnce(key); onCommit(String(node.textContent || "")); noteActivity?.(); };
      const onBlur = () => {
        node.removeAttribute("contenteditable");
        node.removeEventListener("input", onInput);
        node.removeEventListener("blur", onBlur);
        editingSessionKey = "";
      };
      node.addEventListener("input", onInput);
      node.addEventListener("blur", onBlur);
    }

    function attachInteractiveHandlers(card, el) {
      card.addEventListener("mousedown", (ev) => {
        if (ev.button !== 0) return;
        const handle = ev.target?.closest?.(".slide-el-handle");
        if (handle) {
          ev.preventDefault?.();
          ev.stopPropagation?.();
          startPointerDrag("resize", handle.dataset.dir || "se", ev, el, card);
          return;
        }
        if (ev.target?.isContentEditable || ev.target?.getAttribute?.("contenteditable") === "plaintext-only") {
          selectElement(el.id);
          return;
        }
        ev.stopPropagation?.();
        selectElement(el.id);
        card.focus?.();
        startPointerDrag("move", "", ev, el, card);
      });
      card.addEventListener("dblclick", (ev) => {
        ev.stopPropagation?.();
        selectElement(el.id);
        if (el.kind === "textbox") {
          const box = card.querySelector(".slide-el-textbox");
          if (box) bindEditableNode(box, "tb:" + el.id, (txt) => { el.text = txt.slice(0, MAX_TEXT_CHARS); });
          return;
        }
        if (el.kind === "table") {
          const cell = ev.target?.closest?.("td, th");
          if (cell && card.contains(cell)) {
            const r = Number(cell.dataset.rowIdx), c = Number(cell.dataset.colIdx);
            bindEditableNode(cell, "tbl:" + el.id + ":" + r + ":" + c, (txt) => {
              if (Array.isArray(el.rows?.[r])) el.rows[r][c] = txt.slice(0, 4096);
            });
            return;
          }
        }
        onElementDblClick?.(el);
      });
    }

    function render() {
      const layer = $("slide-canvas-layer");
      if (!layer) return;
      const els = ensureElements(currentSlide());
      if (selectedId && !els.some((e) => e.id === selectedId)) selectedId = null;
      const stage = stageSizeOf(currentAspect());
      layer.replaceChildren(...els.map((el) => {
        const card = buildElementDom(document, el, stage, { interactive: true, selected: el.id === selectedId });
        attachInteractiveHandlers(card, el);
        return card;
      }));
      hideSnapGuides();
    }

    function selectElement(id) {
      const els = ensureElements();
      selectedId = id && els.some((e) => e.id === id) ? id : null;
      $("slide-canvas-layer")?.querySelectorAll(".slide-el").forEach((node) => {
        node.classList.toggle("is-selected", node.dataset.elementId === selectedId);
      });
      return selectedId;
    }
    const clearSelection = () => selectElement(null);
    const getSelectedElement = () => (selectedId ? ensureElements().find((e) => e.id === selectedId) || null : null);

    function startPointerDrag(mode, dir, ev, el, card) {
      const stageBox = $("slide-stage-box");
      if (!stageBox) return;
      const rect = stageBox.getBoundingClientRect(), stage = stageSizeOf(currentAspect());
      const preSnap = captureWorkspaceSnapshot(false);
      preSnap.slides = structuredClone(deck);
      activeDrag = {
        mode, dir, el, card, stage,
        scaleX: stage.w / Math.max(1, rect.width || 640),
        scaleY: stage.h / Math.max(1, rect.height || 360),
        startClientX: ev.clientX || 0, startClientY: ev.clientY || 0,
        orig: { x: el.x, y: el.y, w: el.w, h: el.h },
        siblings: ensureElements().filter((item) => item.id !== el.id),
        preSnap, moved: false,
      };
    }

    function onDocMouseMove(ev) {
      if (!activeDrag) return;
      const d = activeDrag;
      const dxPt = ((ev.clientX || 0) - d.startClientX) * d.scaleX;
      const dyPt = ((ev.clientY || 0) - d.startClientY) * d.scaleY;
      const next = { ...d.orig };
      if (d.mode === "move") {
        next.x = clamp(d.orig.x + dxPt, 0, Math.max(0, d.stage.w - d.orig.w));
        next.y = clamp(d.orig.y + dyPt, 0, Math.max(0, d.stage.h - d.orig.h));
      } else {
        if (d.dir.includes("e")) next.w = clamp(d.orig.w + dxPt, MIN_EL_W, d.stage.w - d.orig.x);
        if (d.dir.includes("s")) next.h = clamp(d.orig.h + dyPt, MIN_EL_H, d.stage.h - d.orig.y);
        if (d.dir.includes("w")) { const r = d.orig.x + d.orig.w; next.x = clamp(d.orig.x + dxPt, 0, r - MIN_EL_W); next.w = r - next.x; }
        if (d.dir.includes("n")) { const b = d.orig.y + d.orig.h; next.y = clamp(d.orig.y + dyPt, 0, b - MIN_EL_H); next.h = b - next.y; }
      }
      const snapped = computeSmartSnap(next, d.siblings, d.stage, snapEnabled, d.mode, d.dir);
      d.el.x = snapped.x; d.el.y = snapped.y; d.el.w = snapped.w; d.el.h = snapped.h;
      d.moved = d.moved || snapped.x !== d.orig.x || snapped.y !== d.orig.y || snapped.w !== d.orig.w || snapped.h !== d.orig.h;
      positionElementDom(d.card, d.el, d.stage);
      renderSnapGuides(snapped.vLine, snapped.hLine, d.stage);
    }

    function onDocMouseUp() {
      if (!activeDrag) return;
      const d = activeDrag;
      activeDrag = null;
      hideSnapGuides();
      if (d.moved) {
        pushRawUndoSnapshot(d.preSnap);
        if (d.el.kind === "shape" || d.el.kind === "chart") render();
        noteActivity?.();
      }
    }

    function addElement(spec, options = {}) {
      const slide = currentSlide();
      if (!slide) return null;
      const els = ensureElements(slide);
      if (els.length >= MAX_ELEMENTS_PER_SLIDE) { say("슬라이드당 개체 수 한도(256개)를 초과했습니다."); return null; }
      const norm = normalizeSlideElement(spec, currentAspect(), nextElementId(els));
      if (!norm) return null;
      if (options.recordUndo !== false) pushUndoSnapshot(false);
      if (options.clearStubKind) {
        const stub = $("slide-lower-zone")?.querySelector?.(`[data-slide-object="${options.clearStubKind}"]`);
        if (stub) { stub.remove(); getPanels()?.syncActiveSlideDomStylesToDeck?.(); }
      }
      els.push(norm);
      selectedId = norm.id;
      render();
      noteActivity?.();
      return norm;
    }

    function updateElement(id, patch, options = {}) {
      const els = ensureElements();
      const idx = els.findIndex((e) => e.id === id);
      if (idx < 0) return null;
      const merged = normalizeSlideElement({ ...els[idx], ...patch, id: els[idx].id, kind: els[idx].kind }, currentAspect(), els[idx].id);
      if (!merged) return null;
      if (options.recordUndo !== false) pushUndoSnapshot(false);
      Object.assign(els[idx], merged);
      selectedId = merged.id;
      render();
      noteActivity?.();
      return els[idx];
    }

    function deleteElement(id = selectedId, options = {}) {
      if (!id) return false;
      const els = ensureElements(), idx = els.findIndex((e) => e.id === id);
      if (idx < 0) return false;
      if (options.recordUndo !== false) pushUndoSnapshot(false);
      els.splice(idx, 1);
      if (selectedId === id) selectedId = null;
      render();
      noteActivity?.();
      say("개체 지우기 완료");
      return true;
    }

    function reorderElement(id, toFront) {
      const els = ensureElements(), idx = els.findIndex((e) => e.id === id);
      if (idx < 0 || (toFront ? idx === els.length - 1 : idx === 0)) return false;
      pushUndoSnapshot(false);
      const [picked] = els.splice(idx, 1);
      if (toFront) els.push(picked); else els.unshift(picked);
      selectedId = picked.id;
      render();
      noteActivity?.();
      say(toFront ? "맨 앞으로 가져오기" : "맨 뒤로 보내기");
      return true;
    }

    function totalDeckImageBytes() {
      let bytes = 0;
      for (const s of deck) {
        for (const el of s.elements || []) {
          if (el.kind === "image" && el.imageId) bytes += utils?.getDocImage?.(el.imageId)?.bytesLen || 0;
        }
      }
      return bytes;
    }

    async function insertPictureFromBytes(bytes, nameOrMime = "", options = {}) {
      const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      const fail = (error) => { say(error); return { ok: false, error }; };
      if (u8.length === 0) return fail("그림을 넣지 못했습니다: 빈 파일입니다.");
      if (u8.length > MAX_SINGLE_IMAGE_BYTES) return fail("그림을 넣지 못했습니다: 최대 32 MiB까지 넣을 수 있습니다.");
      const header = parseImageHeaderFallback(u8);
      if (!header) return fail("그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다.");
      if (utils?.imageAreaProblem(header)) return fail("그림을 넣지 못했습니다: " + utils.imageAreaProblem(header) + ".");
      if (totalDeckImageBytes() + u8.length > MAX_TOTAL_IMAGE_BYTES) return fail("전체 그림 용량 한도(128 MiB)를 초과했습니다.");
      if (ensureElements().length >= MAX_ELEMENTS_PER_SLIDE) return fail("슬라이드당 개체 수 한도(256개)를 초과했습니다.");

      const stage = stageSizeOf(currentAspect());
      let w = Math.max(MIN_EL_W, header.widthPx), h = Math.max(MIN_EL_H, header.heightPx);
      if (header.widthPx < MIN_EL_W || header.heightPx < MIN_EL_H) {
        const up = Math.max(120 / header.widthPx, 90 / header.heightPx);
        w = round2(header.widthPx * up); h = round2(header.heightPx * up);
      }
      const maxW = stage.w * 0.45, maxH = stage.h * 0.5;
      if (w > maxW || h > maxH) {
        const scale = Math.min(maxW / w, maxH / h);
        w = round2(Math.max(MIN_EL_W, w * scale)); h = round2(Math.max(MIN_EL_H, h * scale));
      }
      const imageId = utils.registerDocImage(utils.bytesToBase64(u8), header.format);
      const element = addElement({
        kind: "image", x: round2((stage.w - w) / 2), y: round2((stage.h - h) / 2), w, h,
        mime: header.format === "jpg" ? "image/jpeg" : "image/png", imageId,
        alt: altFromHint(nameOrMime), naturalWidthPx: header.widthPx, naturalHeightPx: header.heightPx,
      }, options);
      say("그림 삽입");
      return { ok: true, element };
    }

    function applyFormatting(cmdId) {
      const el = getSelectedElement();
      if (!el || (el.kind !== "textbox" && el.kind !== "shape")) return false;
      if (cmdId === "slide-home-bold") el.bold = !el.bold;
      else if (cmdId === "slide-home-italic") el.italic = !el.italic;
      else if (cmdId === "slide-home-underline") el.underline = !el.underline;
      else if (cmdId === "slide-home-style") {
        const sizes = [16, 20, 24, 14];
        el.fontSizePt = sizes[(sizes.indexOf(el.fontSizePt) + 1) % sizes.length];
      } else return false;
      render();
      noteActivity?.();
      return true;
    }

    function toggleSnap() {
      snapEnabled = !snapEnabled;
      hideSnapGuides();
      say(snapEnabled ? "스마트 정렬 가이드: 켬" : "스마트 정렬 가이드: 끔");
      return snapEnabled;
    }

    function setSlideLayout(layoutKey, options = {}) {
      const slide = currentSlide(), next = normalizeLayout(layoutKey);
      if (!slide || (slide.layout === next && $("slide-stage-box")?.dataset.layout === next)) return false;
      if (options.recordUndo !== false) pushUndoSnapshot(false);
      slide.layout = next;
      const stageBox = $("slide-stage-box");
      if (stageBox) stageBox.dataset.layout = next;
      const sel = $("slide-layout-select");
      if (sel && sel.value !== next) sel.value = next;
      noteActivity?.();
      say("레이아웃: " + LAYOUT_LABELS[next]);
      return true;
    }

    function isEditableTarget(target) {
      if (!target || target.nodeType !== 1) return false;
      const tag = String(target.tagName || "").toUpperCase();
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable) return true;
      const ce = target.getAttribute?.("contenteditable");
      if (ce === "true" || ce === "plaintext-only") return true;
      return Boolean(target.closest?.('#slide-title-zone, #slide-lower-zone, [contenteditable="plaintext-only"]'));
    }

    function handleKeydown(e) {
      if (getActiveModule() !== "SLIDE" || !selectedId || document.querySelector("dialog[open]")) return false;
      const target = e.target || document.activeElement;
      if (isEditableTarget(target) || $("slide-thumb-list")?.contains?.(target) || e.ctrlKey || e.metaKey || e.altKey) return false;
      if (e.key === "Escape") { e.preventDefault(); clearSelection(); return true; }
      if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); return deleteElement(selectedId); }
      const step = e.shiftKey ? 10 : 2;
      let dx = 0, dy = 0;
      if (e.key === "ArrowLeft") dx = -step;
      else if (e.key === "ArrowRight") dx = step;
      else if (e.key === "ArrowUp") dy = -step;
      else if (e.key === "ArrowDown") dy = step;
      else return false;
      const el = getSelectedElement();
      if (!el) return false;
      e.preventDefault();
      const stage = stageSizeOf(currentAspect());
      updateElement(el.id, { x: clamp(el.x + dx, 0, Math.max(0, stage.w - el.w)), y: clamp(el.y + dy, 0, Math.max(0, stage.h - el.h)) });
      $("slide-canvas-layer")?.querySelector?.(`.slide-el[data-element-id="${el.id}"]`)?.focus?.();
      return true;
    }

    function bind() {
      $("slide-stage-box")?.addEventListener("mousedown", (ev) => { if (!ev.target?.closest?.(".slide-el")) clearSelection(); });
      document.addEventListener("mousemove", onDocMouseMove);
      document.addEventListener("mouseup", onDocMouseUp);
      const layoutSel = $("slide-layout-select");
      layoutSel?.addEventListener("change", () => setSlideLayout(layoutSel.value));
      const transSel = $("slide-transition-select");
      transSel?.addEventListener("change", () => {
        const slide = currentSlide(), next = normalizeTransition(transSel.value);
        if (!slide || slide.transition === next) return;
        pushUndoSnapshot(false);
        slide.transition = next;
        getPanels()?.renderSlideDeck?.(getActiveSlideIndex());
        noteActivity?.();
      });
    }

    return {
      bind, render, getElements: () => ensureElements(), getSelectedElement, selectElement, clearSelection,
      addElement, updateElement, deleteElement,
      bringToFront: (id = selectedId) => reorderElement(id, true),
      sendToBack: (id = selectedId) => reorderElement(id, false),
      insertPictureFromBytes, applyFormatting, toggleSnap, isSnapEnabled: () => snapEnabled,
      setSlideLayout, currentAspect, stageSize: () => stageSizeOf(currentAspect()), handleKeydown,
      setDoubleClickHandler: (fn) => { onElementDblClick = fn; },
      startPresenterShow: () => hooks.startShow?.(true, { presenter: true }),
      onLeaveModule: () => { activeDrag = null; hideSnapGuides(); clearSelection(); hooks.onLeave?.(); },
    };
  }

  host.__EASYOFFICE_SLIDE_CANVAS_CODEC__ = {
    STAGE_SIZES, LAYOUT_LABELS, SHAPE_LABELS, CHART_LABELS,
    stageSizeOf, normalizeLayout, normalizeTransition, normalizeHex6,
    normalizeSlideElement, serializeDeckPayload, hydrateDecodedDeck, renderStaticElementsLayer,
  };
  host.__EASYOFFICE_CREATE_SLIDE_CANVAS__ = createSlideCanvas;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
