// 참조 구현 (reference implementation): Apache ECharts (Apache-2.0, Copyright 2017-2025 The Apache Software Foundation,
// https://github.com/apache/echarts, commit 34f4d927eafe) `src/chart/pie/pieLayout.ts` lines 35-150 (the sector angle of a value is
// `value * 2π / sum`, the sectors follow one another clockwise from the start angle) and `src/util/layout.ts` `getCircleLayout`
// lines 225-251 (the inner and outer radius are percentages of half the smaller side; the doughnut default of the option is
// `['40%', '70%']`). 3단계 판단: (1) 허용 라이선스 라이브러리를 번들하지 않는다(렌더러는 SVG 직접 생성, ECharts 전체는 크기·캔버스 의존),
// (2) 이미 쓰는 `sheet-chart-svg.js`의 원형 렌더러가 같은 데이터 모양(첫 계열의 값)과 범례를 쓰므로 그 규약을 따른다, (3) 각도와 반지름 계산만
// 위 함수에서 옮겨 SVG 경로로 그린다. See `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  const INNER = 0.4;
  const OUTER = 0.7;
  const TAU = Math.PI * 2;
  const START = -Math.PI / 2;

  // The slices of the first series, as `{ index, label, value }`: only positive finite values are drawn (the pie's rule).
  function sliceRows(dataset) {
    const first = dataset.series[0];
    if (!first) return [];
    const rows = [];
    for (let i = 0; i < first.values.length; i += 1) {
      const v = first.values[i];
      if (typeof v === "number" && Number.isFinite(v) && v > 0) rows.push({ index: i, label: dataset.categories[i] || String(i + 1), value: v });
    }
    return rows;
  }

  // The path of one ring sector from angle `a0` to `a1` (SVG angles, clockwise), outer radius `r`, inner radius `r0`.
  function ringPath(cx, cy, r0, r, a0, a1, round2) {
    const point = (rad, a) => round2(cx + rad * Math.cos(a)) + " " + round2(cy + rad * Math.sin(a));
    const large = a1 - a0 > Math.PI ? 1 : 0;
    return `M ${point(r, a0)} A ${r} ${r} 0 ${large} 1 ${point(r, a1)} L ${point(r0, a1)} A ${r0} ${r0} 0 ${large} 0 ${point(r0, a0)} Z`;
  }

  function render(kit, document, svg, dataset, box, colors) {
    const rows = sliceRows(dataset);
    const total = rows.reduce((sum, r) => sum + r.value, 0);
    if (rows.length === 0 || !(total > 0)) return { ok: false, legendItems: [] };
    const { makeSvgEl, seriesColor, round2 } = kit;
    const half = Math.max(16, Math.min(box.width, box.height) / 2);
    const cx = round2(box.left + box.width / 2);
    const cy = round2(box.top + box.height / 2);
    // The ring keeps the proportion of the reference (40% to 70% of half the smaller side), scaled to the size the pie plot fills.
    const r = round2(half * 0.86);
    const r0 = round2((r * INNER) / OUTER);
    const unit = TAU / total;
    let angle = START;
    for (const row of rows) {
      const sweep = rows.length === 1 ? TAU - 0.0001 : row.value * unit;
      svg.appendChild(makeSvgEl(document, "path", {
        class: "eo-chart-slice eo-chart-doughnut", d: ringPath(cx, cy, r0, r, angle, angle + sweep, round2), fill: seriesColor(row.index),
        stroke: colors.bg, "stroke-width": "1.2", "data-index": row.index, "data-value": row.value,
      }));
      angle += sweep;
    }
    return {
      ok: true,
      legendItems: rows.map((row) => ({ color: seriesColor(row.index), label: row.label + " (" + Math.round((row.value / total) * 1000) / 10 + "%)" })),
    };
  }

  host.__EASYOFFICE_CHART_KINDS__ = Object.assign(host.__EASYOFFICE_CHART_KINDS__ || {}, { doughnut: { render, sliceRows, ringPath } });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
