// 참조 구현 (reference implementation): Apache ECharts (Apache-2.0, Copyright 2017-2025 The Apache Software Foundation,
// https://github.com/apache/echarts, commit 34f4d927eafe) `src/chart/scatter/ScatterSeries.ts` and its bubble example: a bubble is a
// scatter point whose `symbolSize` follows the third value, with the area (not the diameter) proportional to it, so the radius goes
// with the square root of the value. 3단계 판단: (1) ECharts를 번들하지 않는다, (2) X·Y 눈금과 점 배치는 `sheet-chart-svg.js`의 분산형이 쓰는
// `buildNiceScale`과 `xValues`를 그대로 쓴다, (3) 크기→반지름 규칙(제곱근)만 옮기고, 가장 큰 거품을 작은 변의 25%로 둔 것은 OOXML
// `c:bubbleScale val="100"`의 기본이다. See `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  const LARGEST_DIAMETER = 0.25;
  const MIN_RADIUS = 3;

  // The Y series and size series of a bubble dataset: the series come in pairs (the last one may have no size).
  function pairs(dataset) {
    const out = [];
    for (let i = 0; i < dataset.series.length; i += 2) out.push({ y: dataset.series[i], size: dataset.series[i + 1] || null, index: i / 2 });
    return out;
  }

  // The radius of a bubble of `size` when the largest one is `maxSize` and `maxRadius` is its radius: the area follows the size.
  const radiusOf = (size, maxSize, maxRadius) => (size > 0 && maxSize > 0 ? Math.max(MIN_RADIUS, maxRadius * Math.sqrt(size / maxSize)) : MIN_RADIUS);

  function render(kit, document, svg, dataset, box, colors) {
    const { makeSvgEl, seriesColor, round2, buildNiceScale, formatTickValue } = kit;
    const groups = pairs(dataset);
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity, maxSize = 0, count = 0;
    const bubbles = groups.map((g) => {
      const list = [];
      g.y.values.forEach((y, i) => {
        const x = dataset.xValues[i] ?? i + 1;
        if (typeof x !== "number" || !Number.isFinite(x) || typeof y !== "number" || !Number.isFinite(y)) return;
        const raw = g.size?.values[i];
        const size = typeof raw === "number" && Number.isFinite(raw) && raw > 0 ? raw : 0;
        minX = Math.min(minX, x); maxX = Math.max(maxX, x); minY = Math.min(minY, y); maxY = Math.max(maxY, y);
        maxSize = Math.max(maxSize, size);
        list.push({ x, y, size, index: i });
        count += 1;
      });
      return list;
    });
    if (count === 0) return { ok: false, legendItems: [] };
    const xScale = buildNiceScale(minX, maxX, minX >= 0 && minX <= maxX * 0.25, 5);
    const yScale = buildNiceScale(minY, maxY, minY >= 0 && minY <= maxY * 0.25, 5);
    const xOf = (v) => box.left + ((v - xScale.min) / (xScale.max - xScale.min || 1)) * box.width;
    const yOf = (v) => box.top + box.height - ((v - yScale.min) / (yScale.max - yScale.min || 1)) * box.height;
    for (const t of xScale.ticks) {
      const tx = round2(xOf(t));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: tx, y1: box.top, x2: tx, y2: round2(box.top + box.height), stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-tick", x: tx, y: round2(box.top + box.height + 14), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "middle" }, formatTickValue(t, false)));
    }
    for (const t of yScale.ticks) {
      const ty = round2(yOf(t));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: box.left, y1: ty, x2: round2(box.left + box.width), y2: ty, stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-tick", x: round2(box.left - 6), y: round2(ty + 3), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "end" }, formatTickValue(t, dataset.allPercent)));
    }
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: box.left, y1: round2(box.top + box.height), x2: round2(box.left + box.width), y2: round2(box.top + box.height), stroke: colors.axis, "stroke-width": "1" }));
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: box.left, y1: box.top, x2: box.left, y2: round2(box.top + box.height), stroke: colors.axis, "stroke-width": "1" }));
    const maxRadius = (Math.min(box.width, box.height) * LARGEST_DIAMETER) / 2;
    // The largest bubbles first, so the small ones stay visible on top of them.
    bubbles.forEach((list, g) => {
      [...list].sort((a, b) => b.size - a.size).forEach((b) => {
        svg.appendChild(makeSvgEl(document, "circle", {
          class: "eo-chart-bubble", cx: round2(xOf(b.x)), cy: round2(yOf(b.y)), r: round2(radiusOf(b.size, maxSize, maxRadius)),
          fill: seriesColor(g), "fill-opacity": "0.65", stroke: seriesColor(g), "stroke-width": "1", "data-series": g, "data-index": b.index, "data-x": b.x, "data-y": b.y, "data-size": b.size,
        }));
      });
    });
    return { ok: true, legendItems: groups.map((g) => ({ color: seriesColor(g.index), label: g.y.name })) };
  }

  host.__EASYOFFICE_CHART_KINDS__ = Object.assign(host.__EASYOFFICE_CHART_KINDS__ || {}, { bubble: { render, pairs, radiusOf } });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
