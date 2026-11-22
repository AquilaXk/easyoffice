// 참조 구현 (reference implementation): rhwp `src/ole_chart/svg_renderer.rs` lines 39-296 (MIT, Copyright (c) 2025-2026
// Edward Kim; see `THIRD_PARTY_NOTICES.md`) for plot frame, category/legend geometry and axis tick formatting, combined
// with Paul Heckbert's Nice Numbers scale algorithm (Graphics Gems, 1990). Rewritten with `document.createElementNS` SVG
// DOM nodes for 5 spreadsheet chart types (`column`, `bar`, `line`, `pie`, `scatter`), negative baselines, single-pass
// cross-sheet series extraction and theme/print palettes.

(function (host) {
  "use strict";

  const SVG_NS = "http://www.w3.org/2000/svg";
  const CHART_PALETTE = Object.freeze(["#4472c4", "#ed7d31", "#a5a5a5", "#ffc000", "#5b9bd5", "#70ad47"]);
  const CHART_TYPE_LABELS = Object.freeze({
    column: "묶은 세로 막대형",
    bar: "묶은 가로 막대형",
    line: "꺾은선형",
    pie: "원형",
    scatter: "분산형",
  });
  const MAX_CHART_SERIES = 32;
  const MAX_SERIES_POINTS = 2048;

  const PLAIN_NUM_RE = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/;
  const PCT_NUM_RE = /^([+-]?(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?|[+-]?\.\d+)\s*%$/;
  const GROUPED_NUM_RE = /^[+-]?\d{1,3}(?:,\d{3})+(?:\.\d+)?$/;
  const CURRENCY_NUM_RE = /^([+-]?)\s*(?:NT\$|HK\$|R\$|[₩$€£¥₽₫])\s*([+-]?(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?)$/;
  const PAREN_NEG_RE = /^\(\s*(?:NT\$|HK\$|R\$|[₩$€£¥₽₫])?\s*((?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?)\s*\)$/;

  const round2 = (n) => Math.round((Number(n) || 0) * 100) / 100;

  function parseCellNumber(entry) {
    if (!entry || entry.t === "s" || entry.style?.numFmt === "@") return null;
    const raw = String(entry.value ?? "").trim();
    if (!raw || raw.startsWith("#")) return null;
    const fmtPct = Boolean(entry.style?.numFmt && String(entry.style.numFmt).includes("%"));
    if (PLAIN_NUM_RE.test(raw)) {
      const n = Number(raw);
      return Number.isFinite(n) ? { value: n, percent: fmtPct } : null;
    }
    const pct = PCT_NUM_RE.exec(raw);
    if (pct) {
      const n = Number(pct[1].replaceAll(",", "")) / 100;
      return Number.isFinite(n) ? { value: n, percent: true } : null;
    }
    if (GROUPED_NUM_RE.test(raw)) {
      const n = Number(raw.replaceAll(",", ""));
      return Number.isFinite(n) ? { value: n, percent: fmtPct } : null;
    }
    const cur = CURRENCY_NUM_RE.exec(raw);
    if (cur) {
      const neg = cur[1] === "-" || cur[2].startsWith("-");
      const mag = Number(cur[2].replace(/^[+-]/, "").replaceAll(",", ""));
      return Number.isFinite(mag) ? { value: neg ? -mag : mag, percent: false } : null;
    }
    const paren = PAREN_NEG_RE.exec(raw);
    if (paren) {
      const mag = Number(paren[1].replaceAll(",", ""));
      return Number.isFinite(mag) ? { value: -mag, percent: false } : null;
    }
    return null;
  }

  function formatCellLabel(entry) {
    if (!entry) return "";
    const raw = String(entry.value ?? "").trim();
    if (!raw) return "";
    const nf = entry.style?.numFmt;
    if (nf && entry.t !== "s" && nf !== "@") {
      const shown = host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__?.shown?.(nf, raw, entry.t);
      if (shown?.t) return shown.t;
    }
    return raw;
  }

  function resolveSheetMap(sheetDataStore, sheetName) {
    if (!sheetDataStore || !sheetName) return {};
    if (Object.prototype.hasOwnProperty.call(sheetDataStore, sheetName)) return sheetDataStore[sheetName] || {};
    const target = String(sheetName).toLowerCase();
    for (const key of Object.keys(sheetDataStore)) {
      if (key.toLowerCase() === target) return sheetDataStore[key] || {};
    }
    return {};
  }

  function parseSheetRangeRef(refText, defaultSheet) {
    const refs = host.__EASYOFFICE_SHEET_REFS__;
    const text = String(refText ?? "").trim().replace(/^=/, "");
    if (!text || !refs) return null;
    let sheet = defaultSheet || "Sheet1";
    let rest = text;
    if (text.startsWith("'")) {
      let i = 1;
      let name = "";
      for (;;) {
        const q = text.indexOf("'", i);
        if (q < 0) return null;
        name += text.slice(i, q);
        if (text[q + 1] === "'") {
          name += "'";
          i = q + 2;
        } else {
          if (text[q + 1] !== "!") return null;
          sheet = name;
          rest = text.slice(q + 2);
          break;
        }
      }
    } else {
      const bang = text.indexOf("!");
      if (bang > 0) {
        sheet = text.slice(0, bang).trim();
        rest = text.slice(bang + 1);
      }
    }
    const parts = rest.replaceAll("$", "").split(":");
    if (parts.length > 2) return null;
    const p = refs.parseRef(parts[0]);
    const q = refs.parseRef(parts[1] ?? parts[0]);
    if (p && q) {
      return {
        sheet,
        r1: Math.min(p.row, q.row),
        c1: Math.min(p.col, q.col),
        r2: Math.max(p.row, q.row),
        c2: Math.max(p.col, q.col),
      };
    }
    const def = host.__EASYOFFICE_SHEET_NAMES__?.find?.(text, defaultSheet);
    if (def?.refersTo && def.refersTo !== refText) return parseSheetRangeRef(def.refersTo, defaultSheet);
    return null;
  }

  function formatAbsRange(sheet, r1, c1, r2, c2) {
    const refs = host.__EASYOFFICE_SHEET_REFS__;
    const quote = host.__EASYOFFICE_SHEET_UTILS__.quoteFormulaSheetName;
    const a = "$" + refs.colName(c1) + "$" + r1;
    const b = r1 === r2 && c1 === c2 ? "" : ":$" + refs.colName(c2) + "$" + r2;
    return quote(sheet) + "!" + a + b;
  }

  function inspectMatrixLayout(box, swapped, map, isScatter) {
    const refs = host.__EASYOFFICE_SHEET_REFS__;
    const nMajor = swapped ? box.c2 - box.c1 + 1 : box.r2 - box.r1 + 1;
    const nMinor = swapped ? box.r2 - box.r1 + 1 : box.c2 - box.c1 + 1;
    const at = (maj, min) => {
      const r = swapped ? box.r1 + min : box.r1 + maj;
      const c = swapped ? box.c1 + maj : box.c1 + min;
      return map[refs.refOf(r, c)];
    };
    const cornerText = formatCellLabel(at(0, 0)).trim();
    const cornerBlank = cornerText === "";
    if (isScatter) {
      const hasCatAxis = nMinor >= 2;
      let firstMajorText = false;
      const scanMin = Math.min(nMinor - 1, MAX_CHART_SERIES);
      for (let m = 0; m <= scanMin; m += 1) {
        const e = at(0, m);
        if (formatCellLabel(e) !== "" && parseCellNumber(e) === null) {
          firstMajorText = true;
          break;
        }
      }
      const hasSeriesHeader = nMajor >= 2 && (firstMajorText || (hasCatAxis && cornerBlank));
      return { nMajor, nMinor, at, hasCatAxis, hasSeriesHeader };
    }
    let firstMinorText = false;
    const scanMaj = Math.min(nMajor - 1, 64);
    for (let j = nMajor >= 2 ? 1 : 0; j <= scanMaj; j += 1) {
      const e = at(j, 0);
      if (formatCellLabel(e) !== "" && parseCellNumber(e) === null) {
        firstMinorText = true;
        break;
      }
    }
    const hasCatAxis = nMinor >= 2 && (firstMinorText || (cornerBlank && nMajor >= 2));
    let firstMajorText = false;
    const startMin = hasCatAxis ? 1 : 0;
    const scanMin = Math.min(nMinor - 1, startMin + MAX_CHART_SERIES - 1);
    for (let m = startMin; m <= scanMin; m += 1) {
      const e = at(0, m);
      if (formatCellLabel(e) !== "" && parseCellNumber(e) === null) {
        firstMajorText = true;
        break;
      }
    }
    const hasSeriesHeader = nMajor >= 2 && (firstMajorText || (hasCatAxis && cornerBlank));
    return { nMajor, nMinor, at, hasCatAxis, hasSeriesHeader };
  }

  function deriveChartSeries(chart, defaultSheet, sheetDataStore) {
    const box = parseSheetRangeRef(chart.dataRange, defaultSheet);
    if (!box) return [];
    const map = resolveSheetMap(sheetDataStore, box.sheet);
    const swapped = chart.switchRowCol === true;
    const isScatter = chart.chartType === "scatter";
    const { nMajor, nMinor, at, hasCatAxis, hasSeriesHeader } = inspectMatrixLayout(box, swapped, map, isScatter);
    const startMaj = hasSeriesHeader ? 1 : 0;
    const endMaj = Math.min(nMajor - 1, startMaj + MAX_SERIES_POINTS - 1);
    if (endMaj < startMaj) return [];
    const startMin = hasCatAxis ? 1 : 0;
    const endMin = Math.min(nMinor - 1, startMin + MAX_CHART_SERIES - 1);
    if (endMin < startMin) return [];

    const rangeForTrack = (minIdx, maj0, maj1) =>
      swapped
        ? formatAbsRange(box.sheet, box.r1 + minIdx, box.c1 + maj0, box.r1 + minIdx, box.c1 + maj1)
        : formatAbsRange(box.sheet, box.r1 + maj0, box.c1 + minIdx, box.r1 + maj1, box.c1 + minIdx);

    const categoriesRef = hasCatAxis ? rangeForTrack(0, startMaj, endMaj) : "";
    const out = [];
    for (let m = startMin; m <= endMin; m += 1) {
      const idx = m - startMin;
      const headLabel = hasSeriesHeader ? formatCellLabel(at(0, m)) : "";
      const item = {
        name: headLabel || "계열 " + (idx + 1),
        valuesRef: rangeForTrack(m, startMaj, endMaj),
      };
      if (hasSeriesHeader) item.nameRef = rangeForTrack(m, 0, 0);
      if (categoriesRef) item.categoriesRef = categoriesRef;
      out.push(item);
    }
    return out;
  }

  function readCellsInBox(box, sheetDataStore) {
    const refs = host.__EASYOFFICE_SHEET_REFS__;
    const map = resolveSheetMap(sheetDataStore, box.sheet);
    const cells = [];
    for (let r = box.r1; r <= box.r2 && cells.length < MAX_SERIES_POINTS; r += 1) {
      for (let c = box.c1; c <= box.c2 && cells.length < MAX_SERIES_POINTS; c += 1) {
        cells.push(map[refs.refOf(r, c)]);
      }
    }
    return cells;
  }

  function extractFromExplicitSeries(chart, defaultSheet, sheetDataStore) {
    const isScatter = chart.chartType === "scatter";
    const seriesList = chart.series.slice(0, MAX_CHART_SERIES);
    const parsedSeries = [];
    let maxPoints = 0;
    let totalFinite = 0;
    let pctCount = 0;
    for (let i = 0; i < seriesList.length; i += 1) {
      const s = seriesList[i];
      let name = String(s.name || "").trim();
      if (s.nameRef) {
        const nb = parseSheetRangeRef(s.nameRef, defaultSheet);
        if (nb) {
          const liveName = formatCellLabel(readCellsInBox(nb, sheetDataStore)[0]);
          if (liveName) name = liveName;
        }
      }
      if (!name) name = "계열 " + (i + 1);
      const vb = s.valuesRef ? parseSheetRangeRef(s.valuesRef, defaultSheet) : null;
      const entries = vb ? readCellsInBox(vb, sheetDataStore) : [];
      const values = entries.map((entry) => {
        const num = parseCellNumber(entry);
        if (!num) return null;
        totalFinite += 1;
        if (num.percent) pctCount += 1;
        return num.value;
      });
      if (values.length > maxPoints) maxPoints = values.length;
      parsedSeries.push({ name, values });
    }
    const catRef = seriesList.find((s) => s.categoriesRef)?.categoriesRef || "";
    const cb = catRef ? parseSheetRangeRef(catRef, defaultSheet) : null;
    const catEntries = cb ? readCellsInBox(cb, sheetDataStore) : [];
    const count = Math.max(maxPoints, catEntries.length);
    const categories = [];
    const xValues = [];
    for (let k = 0; k < count; k += 1) {
      const entry = catEntries[k];
      categories.push(formatCellLabel(entry) || String(k + 1));
      if (isScatter) {
        const nx = parseCellNumber(entry);
        xValues.push(nx ? nx.value : k + 1);
      }
    }
    return {
      categories,
      xValues,
      series: parsedSeries,
      allPercent: totalFinite > 0 && pctCount === totalFinite,
    };
  }

  function extractChartDataset(chart, defaultSheet, sheetDataStore) {
    if (Array.isArray(chart?.series) && chart.series.length > 0) {
      return extractFromExplicitSeries(chart, defaultSheet, sheetDataStore);
    }
    const box = parseSheetRangeRef(chart?.dataRange, defaultSheet);
    if (!box) return { categories: [], xValues: [], series: [], allPercent: false };
    const map = resolveSheetMap(sheetDataStore, box.sheet);
    const swapped = chart?.switchRowCol === true;
    const isScatter = chart?.chartType === "scatter";
    const { nMajor, nMinor, at, hasCatAxis, hasSeriesHeader } = inspectMatrixLayout(box, swapped, map, isScatter);
    const startMaj = hasSeriesHeader ? 1 : 0;
    const endMaj = Math.min(nMajor - 1, startMaj + MAX_SERIES_POINTS - 1);
    const startMin = hasCatAxis ? 1 : 0;
    const endMin = Math.min(nMinor - 1, startMin + MAX_CHART_SERIES - 1);
    if (endMaj < startMaj || endMin < startMin) {
      return { categories: [], xValues: [], series: [], allPercent: false };
    }
    const categories = [];
    const xValues = [];
    for (let j = startMaj; j <= endMaj; j += 1) {
      const idx = j - startMaj;
      if (hasCatAxis) {
        const e = at(j, 0);
        categories.push(formatCellLabel(e) || String(idx + 1));
        if (isScatter) {
          const nx = parseCellNumber(e);
          xValues.push(nx ? nx.value : idx + 1);
        }
      } else {
        categories.push(String(idx + 1));
        if (isScatter) xValues.push(idx + 1);
      }
    }
    const series = [];
    let totalFinite = 0;
    let pctCount = 0;
    for (let m = startMin; m <= endMin; m += 1) {
      const sIdx = m - startMin;
      const name = (hasSeriesHeader ? formatCellLabel(at(0, m)) : "") || "계열 " + (sIdx + 1);
      const values = [];
      for (let j = startMaj; j <= endMaj; j += 1) {
        const num = parseCellNumber(at(j, m));
        if (num) {
          totalFinite += 1;
          if (num.percent) pctCount += 1;
          values.push(num.value);
        } else {
          values.push(null);
        }
      }
      series.push({ name, values });
    }
    return {
      categories,
      xValues,
      series,
      allPercent: totalFinite > 0 && pctCount === totalFinite,
    };
  }

  function niceNum(range, round) {
    if (!(range > 0) || !Number.isFinite(range)) return 1;
    const exp = Math.floor(Math.log10(range));
    const f = range / 10 ** exp;
    let nf;
    if (round) {
      if (f < 1.5) nf = 1;
      else if (f < 3) nf = 2;
      else if (f < 7) nf = 5;
      else nf = 10;
    } else if (f <= 1) nf = 1;
    else if (f <= 2) nf = 2;
    else if (f <= 5) nf = 5;
    else nf = 10;
    return nf * 10 ** exp;
  }

  function buildNiceScale(rawMin, rawMax, includeZero, targetTicks = 5) {
    let lo = Number.isFinite(rawMin) ? rawMin : 0;
    let hi = Number.isFinite(rawMax) ? rawMax : 1;
    if (includeZero) {
      if (lo > 0) lo = 0;
      if (hi < 0) hi = 0;
    }
    if (lo === hi) {
      if (lo === 0) hi = 1;
      else {
        const pad = Math.abs(lo) * 0.2 || 1;
        lo = includeZero && lo > 0 ? 0 : lo - pad;
        hi = includeZero && hi < 0 ? 0 : hi + pad;
      }
    }
    const span = niceNum(hi - lo, false);
    const step = niceNum(span / Math.max(2, targetTicks - 1), true);
    const niceMin = Math.floor(lo / step + 1e-9) * step;
    const niceMax = Math.ceil(hi / step - 1e-9) * step;
    const count = Math.min(16, Math.max(1, Math.round((niceMax - niceMin) / step)));
    const ticks = [];
    for (let i = 0; i <= count; i += 1) {
      const v = niceMin + i * step;
      ticks.push(Math.abs(v) < step * 1e-9 ? 0 : Number(v.toPrecision(12)));
    }
    return { min: ticks[0], max: ticks[ticks.length - 1], step, ticks };
  }

  function formatTickValue(val, asPercent) {
    if (asPercent) {
      const p = Number((val * 100).toPrecision(10));
      return p + "%";
    }
    if (Math.abs(val) >= 1000 && Math.abs(val - Math.round(val)) < 1e-9) {
      return Math.round(val).toLocaleString("en-US");
    }
    return String(Number(val.toPrecision(10)));
  }

  function truncateLabel(text, maxChars) {
    const chars = Array.from(String(text ?? ""));
    return chars.length > maxChars ? chars.slice(0, Math.max(1, maxChars - 1)).join("") + "…" : chars.join("");
  }

  function makeSvgEl(document, tag, attrs, text) {
    const el = typeof document.createElementNS === "function"
      ? document.createElementNS(SVG_NS, tag)
      : document.createElement(tag);
    if (attrs) {
      for (const [k, v] of Object.entries(attrs)) {
        if (v !== undefined && v !== null) el.setAttribute(k, String(v));
      }
    }
    if (text !== undefined) el.textContent = String(text);
    return el;
  }

  function themePalette(forPrint) {
    if (forPrint) {
      return {
        bg: "#ffffff",
        border: "#b4b4b4",
        text: "#111827",
        muted: "#333333",
        grid: "#d8d8d8",
        axis: "#666666",
      };
    }
    return {
      bg: "var(--ui-surface)",
      border: "var(--ui-border-light)",
      text: "var(--ui-text)",
      muted: "var(--ui-text-secondary)",
      grid: "var(--ui-border-light)",
      axis: "var(--ui-border-strong)",
    };
  }

  function seriesColor(idx) {
    return CHART_PALETTE[idx % CHART_PALETTE.length];
  }

  function seriesExtent(seriesList) {
    let min = Infinity;
    let max = -Infinity;
    let count = 0;
    for (const s of seriesList) {
      for (const v of s.values) {
        if (typeof v === "number" && Number.isFinite(v)) {
          if (v < min) min = v;
          if (v > max) max = v;
          count += 1;
        }
      }
    }
    return count > 0 ? { min, max, count } : null;
  }

  function renderLegendGroup(document, svg, items, pos, bounds, colors) {
    if (!items.length || pos === "none") return;
    const g = makeSvgEl(document, "g", { class: "eo-chart-legend", "data-legend-pos": pos });
    if (pos === "right" || pos === "left") {
      const startX = round2(bounds.legendX);
      let y = round2(bounds.plotTop + 10);
      const maxY = bounds.plotTop + bounds.plotH - 6;
      for (let i = 0; i < items.length && y <= maxY; i += 1) {
        g.appendChild(makeSvgEl(document, "rect", { x: startX, y: round2(y - 7), width: 9, height: 9, rx: 1.5, fill: items[i].color }));
        g.appendChild(makeSvgEl(document, "text", { x: round2(startX + 13), y: round2(y + 1), fill: colors.text, "font-size": "10", "font-family": "sans-serif" }, truncateLabel(items[i].label, 10)));
        y += 16;
      }
    } else {
      const y = round2(bounds.legendY);
      let x = round2(bounds.plotLeft);
      const maxX = bounds.width - 10;
      for (let i = 0; i < items.length; i += 1) {
        const text = truncateLabel(items[i].label, 12);
        const itemW = Math.max(44, Array.from(text).length * 7 + 20);
        if (x + itemW > maxX && i > 0) break;
        g.appendChild(makeSvgEl(document, "rect", { x, y: round2(y - 7), width: 9, height: 9, rx: 1.5, fill: items[i].color }));
        g.appendChild(makeSvgEl(document, "text", { x: round2(x + 13), y: round2(y + 1), fill: colors.text, "font-size": "10", "font-family": "sans-serif" }, text));
        x = round2(x + itemW);
      }
    }
    svg.appendChild(g);
  }

  function renderColumnPlot(document, svg, dataset, box, colors) {
    const ext = seriesExtent(dataset.series);
    if (!ext) return false;
    const scale = buildNiceScale(ext.min, ext.max, true, 5);
    const span = scale.max - scale.min || 1;
    const yOf = (v) => box.top + box.height - ((v - scale.min) / span) * box.height;
    const zeroY = yOf(0);

    for (const tick of scale.ticks) {
      const ty = round2(yOf(tick));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: box.left, y1: ty, x2: round2(box.left + box.width), y2: ty, stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(
        makeSvgEl(
          document,
          "text",
          { class: "eo-chart-tick", x: round2(box.left - 6), y: round2(ty + 3), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "end" },
          formatTickValue(tick, dataset.allPercent)
        )
      );
    }

    const catCount = Math.max(1, dataset.categories.length);
    const sCount = Math.max(1, dataset.series.length);
    const groupW = box.width / catCount;
    const innerW = groupW * 0.72;
    const barW = Math.max(2, innerW / sCount);
    const padX = (groupW - barW * sCount) / 2;

    for (let c = 0; c < catCount; c += 1) {
      for (let s = 0; s < sCount; s += 1) {
        const val = dataset.series[s].values[c];
        if (typeof val !== "number" || !Number.isFinite(val)) continue;
        const vy = yOf(val);
        const topY = val >= 0 ? vy : zeroY;
        const barH = Math.max(val === 0 ? 1 : 0.5, Math.abs(zeroY - vy));
        const bx = box.left + c * groupW + padX + s * barW;
        svg.appendChild(
          makeSvgEl(document, "rect", {
            class: "eo-chart-bar",
            x: round2(bx),
            y: round2(topY),
            width: round2(Math.max(1.5, barW - (sCount > 1 ? 1 : 0))),
            height: round2(barH),
            fill: seriesColor(s),
            "data-series": s,
            "data-index": c,
            "data-value": val,
          })
        );
      }
      const stride = Math.max(1, Math.ceil(catCount / Math.max(2, Math.floor(box.width / 44))));
      if (c % stride === 0) {
        svg.appendChild(
          makeSvgEl(
            document,
            "text",
            { class: "eo-chart-cat", x: round2(box.left + (c + 0.5) * groupW), y: round2(box.top + box.height + 14), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "middle" },
            truncateLabel(dataset.categories[c] ?? String(c + 1), 9)
          )
        );
      }
    }
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: box.left, y1: round2(zeroY), x2: round2(box.left + box.width), y2: round2(zeroY), stroke: colors.axis, "stroke-width": "1" }));
    return true;
  }

  function renderBarPlot(document, svg, dataset, box, colors) {
    const ext = seriesExtent(dataset.series);
    if (!ext) return false;
    const scale = buildNiceScale(ext.min, ext.max, true, 5);
    const span = scale.max - scale.min || 1;
    const xOf = (v) => box.left + ((v - scale.min) / span) * box.width;
    const zeroX = xOf(0);

    for (const tick of scale.ticks) {
      const tx = round2(xOf(tick));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: tx, y1: box.top, x2: tx, y2: round2(box.top + box.height), stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(
        makeSvgEl(
          document,
          "text",
          { class: "eo-chart-tick", x: tx, y: round2(box.top + box.height + 14), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "middle" },
          formatTickValue(tick, dataset.allPercent)
        )
      );
    }

    const catCount = Math.max(1, dataset.categories.length);
    const sCount = Math.max(1, dataset.series.length);
    const groupH = box.height / catCount;
    const innerH = groupH * 0.72;
    const barH = Math.max(2, innerH / sCount);
    const padY = (groupH - barH * sCount) / 2;

    for (let c = 0; c < catCount; c += 1) {
      for (let s = 0; s < sCount; s += 1) {
        const val = dataset.series[s].values[c];
        if (typeof val !== "number" || !Number.isFinite(val)) continue;
        const vx = xOf(val);
        const leftX = val >= 0 ? zeroX : vx;
        const barW = Math.max(val === 0 ? 1 : 0.5, Math.abs(vx - zeroX));
        const by = box.top + c * groupH + padY + s * barH;
        svg.appendChild(
          makeSvgEl(document, "rect", {
            class: "eo-chart-bar",
            x: round2(leftX),
            y: round2(by),
            width: round2(barW),
            height: round2(Math.max(1.5, barH - (sCount > 1 ? 1 : 0))),
            fill: seriesColor(s),
            "data-series": s,
            "data-index": c,
            "data-value": val,
          })
        );
      }
      const stride = Math.max(1, Math.ceil(catCount / Math.max(2, Math.floor(box.height / 16))));
      if (c % stride === 0) {
        svg.appendChild(
          makeSvgEl(
            document,
            "text",
            { class: "eo-chart-cat", x: round2(box.left - 6), y: round2(box.top + (c + 0.5) * groupH + 3), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "end" },
            truncateLabel(dataset.categories[c] ?? String(c + 1), 8)
          )
        );
      }
    }
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: round2(zeroX), y1: box.top, x2: round2(zeroX), y2: round2(box.top + box.height), stroke: colors.axis, "stroke-width": "1" }));
    return true;
  }

  function renderLinePlot(document, svg, dataset, box, colors) {
    const ext = seriesExtent(dataset.series);
    if (!ext) return false;
    const scale = buildNiceScale(ext.min, ext.max, true, 5);
    const span = scale.max - scale.min || 1;
    const yOf = (v) => box.top + box.height - ((v - scale.min) / span) * box.height;

    for (const tick of scale.ticks) {
      const ty = round2(yOf(tick));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: box.left, y1: ty, x2: round2(box.left + box.width), y2: ty, stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(
        makeSvgEl(
          document,
          "text",
          { class: "eo-chart-tick", x: round2(box.left - 6), y: round2(ty + 3), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "end" },
          formatTickValue(tick, dataset.allPercent)
        )
      );
    }

    const catCount = Math.max(1, dataset.categories.length);
    const xAt = (c) => (catCount === 1 ? box.left + box.width / 2 : box.left + (c / (catCount - 1)) * box.width);
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: box.left, y1: round2(yOf(0)), x2: round2(box.left + box.width), y2: round2(yOf(0)), stroke: colors.axis, "stroke-width": "1" }));

    for (let s = 0; s < dataset.series.length; s += 1) {
      const color = seriesColor(s);
      const segments = [];
      let penDown = false;
      const markers = [];
      for (let c = 0; c < catCount; c += 1) {
        const val = dataset.series[s].values[c];
        if (typeof val !== "number" || !Number.isFinite(val)) {
          penDown = false;
          continue;
        }
        const px = round2(xAt(c));
        const py = round2(yOf(val));
        segments.push((penDown ? "L " : "M ") + px + " " + py);
        penDown = true;
        markers.push({ px, py, c, val });
      }
      if (segments.length > 0) {
        svg.appendChild(makeSvgEl(document, "path", { class: "eo-chart-line", d: segments.join(" "), fill: "none", stroke: color, "stroke-width": "2", "stroke-linecap": "round", "stroke-linejoin": "round", "data-series": s }));
      }
      for (const m of markers) {
        svg.appendChild(makeSvgEl(document, "circle", { class: "eo-chart-marker", cx: m.px, cy: m.py, r: "3.2", fill: color, "data-series": s, "data-index": m.c, "data-value": m.val }));
      }
    }

    const stride = Math.max(1, Math.ceil(catCount / Math.max(2, Math.floor(box.width / 44))));
    for (let c = 0; c < catCount; c += stride) {
      svg.appendChild(
        makeSvgEl(
          document,
          "text",
          { class: "eo-chart-cat", x: round2(xAt(c)), y: round2(box.top + box.height + 14), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "middle" },
          truncateLabel(dataset.categories[c] ?? String(c + 1), 9)
        )
      );
    }
    return true;
  }

  function renderPiePlot(document, svg, dataset, box, colors) {
    const first = dataset.series[0];
    if (!first) return { ok: false, legendItems: [] };
    const slices = [];
    let total = 0;
    for (let i = 0; i < first.values.length; i += 1) {
      const v = first.values[i];
      if (typeof v === "number" && Number.isFinite(v) && v > 0) {
        slices.push({ index: i, label: dataset.categories[i] || String(i + 1), value: v, color: seriesColor(i) });
        total += v;
      }
    }
    if (slices.length === 0 || !(total > 0)) return { ok: false, legendItems: [] };

    const cx = round2(box.left + box.width / 2);
    const cy = round2(box.top + box.height / 2);
    const radius = round2(Math.max(16, Math.min(box.width, box.height) * 0.43));
    const legendItems = slices.map((sl) => ({
      color: sl.color,
      label: sl.label + " (" + Math.round((sl.value / total) * 1000) / 10 + "%)",
    }));

    if (slices.length === 1) {
      svg.appendChild(
        makeSvgEl(document, "circle", {
          class: "eo-chart-slice",
          cx,
          cy,
          r: radius,
          fill: slices[0].color,
          stroke: colors.bg,
          "stroke-width": "1",
          "data-index": slices[0].index,
          "data-value": slices[0].value,
        })
      );
      return { ok: true, legendItems };
    }

    let angle = -Math.PI / 2;
    for (const sl of slices) {
      const sweep = (sl.value / total) * Math.PI * 2;
      const nextAngle = angle + sweep;
      const x1 = round2(cx + radius * Math.cos(angle));
      const y1 = round2(cy + radius * Math.sin(angle));
      const x2 = round2(cx + radius * Math.cos(nextAngle));
      const y2 = round2(cy + radius * Math.sin(nextAngle));
      const largeArc = sweep > Math.PI ? 1 : 0;
      const d = `M ${cx} ${cy} L ${x1} ${y1} A ${radius} ${radius} 0 ${largeArc} 1 ${x2} ${y2} Z`;
      svg.appendChild(
        makeSvgEl(document, "path", {
          class: "eo-chart-slice",
          d,
          fill: sl.color,
          stroke: colors.bg,
          "stroke-width": "1.2",
          "data-index": sl.index,
          "data-value": sl.value,
        })
      );
      angle = nextAngle;
    }
    return { ok: true, legendItems };
  }

  function renderScatterPlot(document, svg, dataset, box, colors) {
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    let totalPts = 0;
    const seriesPoints = dataset.series.map((s) => {
      const pts = [];
      for (let i = 0; i < s.values.length; i += 1) {
        const y = s.values[i];
        const x = dataset.xValues[i] ?? i + 1;
        if (typeof x === "number" && Number.isFinite(x) && typeof y === "number" && Number.isFinite(y)) {
          if (x < minX) minX = x;
          if (x > maxX) maxX = x;
          if (y < minY) minY = y;
          if (y > maxY) maxY = y;
          pts.push({ x, y, index: i });
          totalPts += 1;
        }
      }
      return pts;
    });
    if (totalPts === 0) return false;

    const xScale = buildNiceScale(minX, maxX, minX >= 0 && minX <= maxX * 0.25, 5);
    const yScale = buildNiceScale(minY, maxY, minY >= 0 && minY <= maxY * 0.25, 5);
    const xSpan = xScale.max - xScale.min || 1;
    const ySpan = yScale.max - yScale.min || 1;
    const xOf = (v) => box.left + ((v - xScale.min) / xSpan) * box.width;
    const yOf = (v) => box.top + box.height - ((v - yScale.min) / ySpan) * box.height;

    for (const txVal of xScale.ticks) {
      const tx = round2(xOf(txVal));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: tx, y1: box.top, x2: tx, y2: round2(box.top + box.height), stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-tick", x: tx, y: round2(box.top + box.height + 14), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "middle" }, formatTickValue(txVal, false)));
    }
    for (const tyVal of yScale.ticks) {
      const ty = round2(yOf(tyVal));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: box.left, y1: ty, x2: round2(box.left + box.width), y2: ty, stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-tick", x: round2(box.left - 6), y: round2(ty + 3), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "end" }, formatTickValue(tyVal, dataset.allPercent)));
    }
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: box.left, y1: round2(box.top + box.height), x2: round2(box.left + box.width), y2: round2(box.top + box.height), stroke: colors.axis, "stroke-width": "1" }));
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: box.left, y1: box.top, x2: box.left, y2: round2(box.top + box.height), stroke: colors.axis, "stroke-width": "1" }));

    for (let s = 0; s < seriesPoints.length; s += 1) {
      const color = seriesColor(s);
      const pts = seriesPoints[s];
      if (pts.length > 1) {
        const sorted = [...pts].sort((a, b) => a.x - b.x);
        const d = sorted.map((p, idx) => (idx === 0 ? "M " : "L ") + round2(xOf(p.x)) + " " + round2(yOf(p.y))).join(" ");
        svg.appendChild(makeSvgEl(document, "path", { class: "eo-chart-scatter-line", d, fill: "none", stroke: color, "stroke-width": "1.2", "stroke-opacity": "0.55", "data-series": s }));
      }
      for (const p of pts) {
        svg.appendChild(
          makeSvgEl(document, "circle", {
            class: "eo-chart-point",
            cx: round2(xOf(p.x)),
            cy: round2(yOf(p.y)),
            r: "3.6",
            fill: color,
            "data-series": s,
            "data-index": p.index,
            "data-x": p.x,
            "data-y": p.y,
          })
        );
      }
    }
    return true;
  }

  function renderChartSvg(opts) {
    const { document, chart, sheetName, sheetDataStore, forPrint = false } = opts;
    const width = Math.max(120, Math.round(Number(opts.width || chart?.anchor?.width) || 420));
    const height = Math.max(90, Math.round(Number(opts.height || chart?.anchor?.height) || 260));
    const colors = themePalette(forPrint);
    const svg = makeSvgEl(document, "svg", {
      class: "eo-chart-svg",
      viewBox: `0 0 ${width} ${height}`,
      width,
      height,
      "aria-hidden": "true",
      focusable: "false",
    });

    svg.appendChild(makeSvgEl(document, "rect", { class: "eo-chart-bg", x: 0, y: 0, width, height, fill: colors.bg }));
    if (forPrint) {
      svg.appendChild(makeSvgEl(document, "rect", { x: 0.5, y: 0.5, width: width - 1, height: height - 1, fill: "none", stroke: colors.border, "stroke-width": "1" }));
    }

    const titleText = String(chart?.title ?? "").trim();
    let plotTop = 14;
    if (titleText) {
      svg.appendChild(
        makeSvgEl(
          document,
          "text",
          { class: "eo-chart-title", x: round2(width / 2), y: 20, fill: colors.text, "font-size": "12.5", "font-weight": "600", "font-family": "sans-serif", "text-anchor": "middle" },
          truncateLabel(titleText, 36)
        )
      );
      plotTop = 32;
    }

    const legendPos = chart?.legend || "right";
    const isPie = chart?.chartType === "pie";
    const isBar = chart?.chartType === "bar";
    const sideLegendW = Math.min(104, Math.max(76, Math.round(width * 0.24)));
    const leftReserve = legendPos === "left" ? sideLegendW : 0;
    const rightReserve = legendPos === "right" ? sideLegendW : 14;
    const topLegendH = legendPos === "top" ? 20 : 0;
    const bottomLegendH = legendPos === "bottom" ? 20 : 0;
    const plotLeft = leftReserve + (isPie ? 14 : isBar ? 58 : 48);
    const bottomAxisH = isPie ? 10 : 22;
    const actualTop = plotTop + topLegendH;
    const plotW = Math.max(40, width - plotLeft - rightReserve);
    const plotH = Math.max(36, height - actualTop - bottomAxisH - bottomLegendH);

    const dataset = extractChartDataset(chart, sheetName, sheetDataStore);
    const box = { left: plotLeft, top: actualTop, width: plotW, height: plotH };
    let rendered = false;
    let legendItems = dataset.series.map((s, idx) => ({ color: seriesColor(idx), label: s.name }));

    if (chart?.chartType === "bar") rendered = renderBarPlot(document, svg, dataset, box, colors);
    else if (chart?.chartType === "line") rendered = renderLinePlot(document, svg, dataset, box, colors);
    else if (chart?.chartType === "pie") {
      const pieRes = renderPiePlot(document, svg, dataset, box, colors);
      rendered = pieRes.ok;
      legendItems = pieRes.legendItems;
    } else if (chart?.chartType === "scatter") rendered = renderScatterPlot(document, svg, dataset, box, colors);
    else rendered = renderColumnPlot(document, svg, dataset, box, colors);

    if (!rendered) {
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-empty", x: round2(width / 2), y: round2(height / 2), fill: colors.muted, "font-size": "11", "font-family": "sans-serif", "text-anchor": "middle" }, "데이터 없음"));
    } else if (legendPos !== "none") {
      renderLegendGroup(
        document,
        svg,
        legendItems,
        legendPos,
        {
          width,
          plotLeft,
          plotTop: actualTop,
          plotH,
          legendX: legendPos === "left" ? 8 : plotLeft + plotW + 10,
          legendY: legendPos === "top" ? plotTop + 12 : actualTop + plotH + bottomAxisH + 12,
        },
        colors
      );
    }

    return svg;
  }

  function nextChartId(existingCharts) {
    const used = new Set((existingCharts || []).map((c) => String(c?.id || "")));
    let n = (existingCharts?.length || 0) + 1;
    while (used.has("chart-" + n)) n += 1;
    return "chart-" + n;
  }

  function createChartModel({ sheetName, sheetDataStore, existingCharts = [], spec = {}, anchorFromRect }) {
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    if (existingCharts.length >= (layoutApi?.MAX_CHARTS_PER_SHEET ?? 64)) {
      throw new Error("시트당 차트 수 한도(64개)를 초과했습니다.");
    }
    const chartType = String(spec.chartType || "column");
    if (!(layoutApi?.CHART_TYPES || ["column", "bar", "line", "pie", "scatter"]).includes(chartType)) {
      throw new Error("지원하지 않는 차트 종류입니다: " + chartType);
    }
    const dataRange = String(spec.dataRange ?? spec.range ?? "").trim();
    const box = parseSheetRangeRef(dataRange, sheetName);
    if (!box) throw new Error("유효한 데이터 범위를 입력하세요: " + dataRange);
    const switchRowCol = spec.switchRowCol === true;
    const legend = (layoutApi?.CHART_LEGENDS || ["right", "bottom", "top", "none", "left"]).includes(spec.legend)
      ? spec.legend
      : "right";
    const title = String(spec.title ?? "").trim().slice(0, 160);
    const series = deriveChartSeries({ chartType, dataRange, switchRowCol }, sheetName, sheetDataStore);
    if (series.length > MAX_CHART_SERIES) {
      throw new Error("차트 계열 수 한도(32개)를 초과했습니다.");
    }
    const idx = existingCharts.length;
    const colPx = layoutApi?.defaultColPxOf?.(sheetName) ?? 64; // the sheet's default column (PD-305)
    const baseLeft = 56 + box.c2 * colPx + 16 + (idx % 4) * 24;
    const baseTop = 26 + (box.r1 - 1) * 24 + 4 + (idx % 4) * 20;
    const width = Math.max(180, Math.min(1600, Math.round(Number(spec.width) || 420)));
    const height = Math.max(140, Math.min(1200, Math.round(Number(spec.height) || 260)));
    const anchor = typeof anchorFromRect === "function"
      ? anchorFromRect(baseLeft, baseTop, width, height)
      : {
          fromCol: Math.min(16383, box.c2),
          fromRow: Math.max(0, box.r1 - 1),
          toCol: Math.min(16383, box.c2 + Math.max(1, Math.ceil(width / colPx))),
          toRow: Math.min(1048575, Math.max(0, box.r1 - 1) + Math.max(1, Math.ceil(height / 24))),
          left: baseLeft,
          top: baseTop,
          width,
          height,
        };
    const out = { id: nextChartId(existingCharts), chartType, title, dataRange, switchRowCol, legend, anchor };
    if (series.length > 0) out.series = series;
    return out;
  }

  host.__EASYOFFICE_SHEET_CHART_SVG__ = {
    CHART_PALETTE,
    CHART_TYPE_LABELS,
    parseCellNumber,
    parseSheetRangeRef,
    formatAbsRange,
    deriveChartSeries,
    extractChartDataset,
    buildNiceScale,
    renderChartSvg,
    nextChartId,
    createChartModel,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
