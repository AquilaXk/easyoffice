// 참조 구현 (reference implementation): Apache ECharts (Apache-2.0, Copyright 2017-2025 The Apache Software Foundation,
// https://github.com/apache/echarts, commit 34f4d927eafe) `src/coord/radar/Radar.ts` `resize` lines 130-160 (the axis `i` of `n` lies
// at `startAngle + i * 2π / n`, the first axis at 90° on top, turning counterclockwise; the radius is 75% of half the smaller side)
// and `update` lines 161-170 (five splits, `RADAR_DEFAULT_SPLIT_NUMBER`). 3단계 판단: (1) ECharts를 번들하지 않는다(캔버스·모델 전체가 딸려 옴),
// (2) 눈금 계산은 이미 쓰는 `buildNiceScale`을 넘겨 받아 쓴다, (3) 축 각도와 값→반지름 변환만 위 함수에서 옮겨 SVG로 그린다.
// See `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  const START_ANGLE = Math.PI / 2;
  const RADIUS = 0.75;
  const LABEL_GAP = 9;

  // The point of axis `i` of `n` at `radius` from the center, as ECharts places it (counterclockwise from the top).
  function axisPoint(cx, cy, radius, i, n) {
    const angle = START_ANGLE + (i * Math.PI * 2) / n;
    return { x: cx + radius * Math.cos(angle), y: cy - radius * Math.sin(angle), angle };
  }

  function render(kit, document, svg, dataset, box, colors) {
    const n = dataset.categories.length;
    const extent = kit.seriesExtent(dataset.series);
    if (n < 3 || !extent) return { ok: false, legendItems: [] };
    const { makeSvgEl, seriesColor, round2, buildNiceScale, formatTickValue, truncateLabel } = kit;
    const scale = buildNiceScale(Math.min(0, extent.min), Math.max(0, extent.max), true, 5);
    const span = scale.max - scale.min || 1;
    const cx = round2(box.left + box.width / 2);
    const cy = round2(box.top + box.height / 2);
    const radius = Math.max(16, (Math.min(box.width, box.height) / 2) * RADIUS);
    const at = (value, i) => axisPoint(cx, cy, ((value - scale.min) / span) * radius, i, n);
    const ring = (value) => Array.from({ length: n }, (_, i) => axisPoint(cx, cy, ((value - scale.min) / span) * radius, i, n));
    const points = (list) => list.map((p) => round2(p.x) + "," + round2(p.y)).join(" ");

    for (const tick of scale.ticks) {
      if (tick <= scale.min) continue;
      svg.appendChild(makeSvgEl(document, "polygon", { class: "eo-chart-gridline", points: points(ring(tick)), fill: "none", stroke: colors.grid, "stroke-width": "1" }));
      const label = ring(tick)[0];
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-tick", x: round2(label.x + 3), y: round2(label.y + 3), fill: colors.muted, "font-size": "8.5", "font-family": "sans-serif" }, formatTickValue(tick, dataset.allPercent)));
    }
    for (let i = 0; i < n; i += 1) {
      const end = axisPoint(cx, cy, radius, i, n);
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: cx, y1: cy, x2: round2(end.x), y2: round2(end.y), stroke: colors.axis, "stroke-width": "1" }));
      const label = axisPoint(cx, cy, radius + LABEL_GAP, i, n);
      const anchor = Math.abs(Math.cos(label.angle)) < 0.2 ? "middle" : Math.cos(label.angle) > 0 ? "start" : "end";
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-cat", x: round2(label.x), y: round2(label.y + 3), fill: colors.text, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": anchor }, truncateLabel(dataset.categories[i] || String(i + 1), 10)));
    }
    dataset.series.forEach((s, idx) => {
      const poly = s.values.map((v, i) => at(typeof v === "number" && Number.isFinite(v) ? v : scale.min, i));
      const color = seriesColor(idx);
      svg.appendChild(makeSvgEl(document, "polygon", { class: "eo-chart-radar-area", points: points(poly), fill: color, "fill-opacity": "0.2", stroke: color, "stroke-width": "1.6", "data-series": idx }));
      poly.forEach((p, i) => svg.appendChild(makeSvgEl(document, "circle", { class: "eo-chart-point", cx: round2(p.x), cy: round2(p.y), r: "2.6", fill: color, "data-series": idx, "data-index": i })));
    });
    return { ok: true, legendItems: dataset.series.map((s, idx) => ({ color: seriesColor(idx), label: s.name })) };
  }

  host.__EASYOFFICE_CHART_KINDS__ = Object.assign(host.__EASYOFFICE_CHART_KINDS__ || {}, { radar: { render, axisPoint } });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
