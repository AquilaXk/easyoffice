// 참조 구현 (reference implementation): Apache ECharts (Apache-2.0, Copyright 2017-2025 The Apache Software Foundation,
// https://github.com/apache/echarts, commit 34f4d927eafe) `src/chart/candlestick/candlestickLayout.ts` lines 60-118 (the body runs between
// `min(open, close)` and `max(open, close)`, the whisker between the lowest and the highest value) and `calculateCandleWidth` lines
// 256-277 (the body is half a category band wide). Default candle colors of ECharts: a rise is `#eb5454`, a fall `#47b262`.
// 3단계 판단: (1) ECharts를 번들하지 않는다, (2) 값 눈금은 이미 쓰는 `buildNiceScale`, 범주 축은 막대 그래프와 같은 밴드 배치를 쓴다,
// (3) 몸통·꼬리 위치 계산과 폭 규칙만 옮긴다. 계열은 시가·고가·저가·종가 순(셋이면 고가·저가·종가). See `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  const RISE = "#eb5454";
  const FALL = "#47b262";

  // The four values of the candle at category `i`: `{ open, high, low, close }`, `null` when one is missing. Three series are
  // high, low and close.
  function candleAt(series, i) {
    const read = (s) => (typeof s?.values[i] === "number" && Number.isFinite(s.values[i]) ? s.values[i] : null);
    const v = series.map(read);
    const [open, high, low, close] = series.length >= 4 ? v : [null, v[0], v[1], v[2]];
    if (high === null || low === null || close === null || (series.length >= 4 && open === null)) return null;
    return { open, high, low, close };
  }

  function render(kit, document, svg, dataset, box, colors) {
    const n = dataset.categories.length;
    const { makeSvgEl, round2, buildNiceScale, formatTickValue, truncateLabel } = kit;
    if (dataset.series.length < 3 || n === 0) return { ok: false, legendItems: [] };
    const candles = Array.from({ length: n }, (_, i) => candleAt(dataset.series, i));
    const all = candles.filter(Boolean).flatMap((c) => [c.high, c.low]);
    if (all.length === 0) return { ok: false, legendItems: [] };
    const scale = buildNiceScale(Math.min(...all), Math.max(...all), false, 5);
    const span = scale.max - scale.min || 1;
    const yOf = (v) => box.top + box.height - ((v - scale.min) / span) * box.height;
    for (const t of scale.ticks) {
      const ty = round2(yOf(t));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: box.left, y1: ty, x2: round2(box.left + box.width), y2: ty, stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-tick", x: round2(box.left - 6), y: round2(ty + 3), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "end" }, formatTickValue(t, false)));
    }
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: box.left, y1: round2(box.top + box.height), x2: round2(box.left + box.width), y2: round2(box.top + box.height), stroke: colors.axis, "stroke-width": "1" }));
    const band = box.width / n;
    const body = Math.max(1, band / 2);
    const labelEvery = Math.max(1, Math.ceil(n / Math.max(1, Math.floor(box.width / 44))));
    candles.forEach((c, i) => {
      const x = box.left + (i + 0.5) * band;
      if (i % labelEvery === 0) {
        svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-cat", x: round2(x), y: round2(box.top + box.height + 14), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "middle" }, truncateLabel(dataset.categories[i] || String(i + 1), 8)));
      }
      if (!c) return;
      const color = c.close >= (c.open ?? c.close) ? RISE : FALL;
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-whisker", x1: round2(x), y1: round2(yOf(c.high)), x2: round2(x), y2: round2(yOf(c.low)), stroke: color, "stroke-width": "1.2", "data-index": i }));
      if (c.open === null) {
        svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-close-tick", x1: round2(x), y1: round2(yOf(c.close)), x2: round2(x + body / 2), y2: round2(yOf(c.close)), stroke: color, "stroke-width": "1.6", "data-index": i }));
        return;
      }
      const top = yOf(Math.max(c.open, c.close));
      const height = Math.max(1, yOf(Math.min(c.open, c.close)) - top);
      svg.appendChild(makeSvgEl(document, "rect", { class: "eo-chart-candle", x: round2(x - body / 2), y: round2(top), width: round2(body), height: round2(height), fill: color, stroke: color, "stroke-width": "1", "data-index": i }));
    });
    return { ok: true, legendItems: [{ color: RISE, label: "상승" }, { color: FALL, label: "하락" }] };
  }

  host.__EASYOFFICE_CHART_KINDS__ = Object.assign(host.__EASYOFFICE_CHART_KINDS__ || {}, { stock: { render, candleAt, RISE, FALL } });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
