(function (host) {
  "use strict";

  // 혼합 chart (PD-402): the first series is drawn as columns and the others as lines over the same category axis and one value
  // scale, the layout of the default combo of the spreadsheet program (묶은 세로 막대형 - 꺾은선형). The geometry is that of the column and line plots
  // of `sheet-chart-svg.js` (a band per category, the column centered in 60% of it, the line through the band centers); the value
  // scale takes every series, so the lines and the columns read against the same axis. The file form is a `c:barChart` with the
  // first series and a `c:lineChart` with the rest (`chart_kinds.rs`).
  const BAND_FILL = 0.6;

  function render(kit, document, svg, dataset, box, colors) {
    const n = dataset.categories.length;
    const extent = kit.seriesExtent(dataset.series);
    if (n === 0 || !extent) return { ok: false, legendItems: [] };
    const { makeSvgEl, seriesColor, round2, buildNiceScale, formatTickValue, truncateLabel } = kit;
    const scale = buildNiceScale(Math.min(0, extent.min), Math.max(0, extent.max), true, 5);
    const span = scale.max - scale.min || 1;
    const yOf = (v) => box.top + box.height - ((v - scale.min) / span) * box.height;
    const base = round2(yOf(Math.max(scale.min, Math.min(0, scale.max))));
    for (const t of scale.ticks) {
      const ty = round2(yOf(t));
      svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-gridline", x1: box.left, y1: ty, x2: round2(box.left + box.width), y2: ty, stroke: colors.grid, "stroke-width": "1" }));
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-tick", x: round2(box.left - 6), y: round2(ty + 3), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "end" }, formatTickValue(t, dataset.allPercent)));
    }
    svg.appendChild(makeSvgEl(document, "line", { class: "eo-chart-axis", x1: box.left, y1: base, x2: round2(box.left + box.width), y2: base, stroke: colors.axis, "stroke-width": "1" }));
    const band = box.width / n;
    const xOf = (i) => box.left + (i + 0.5) * band;
    const labelEvery = Math.max(1, Math.ceil(n / Math.max(1, Math.floor(box.width / 48))));
    for (let i = 0; i < n; i += labelEvery) {
      svg.appendChild(makeSvgEl(document, "text", { class: "eo-chart-cat", x: round2(xOf(i)), y: round2(box.top + box.height + 14), fill: colors.muted, "font-size": "9.5", "font-family": "sans-serif", "text-anchor": "middle" }, truncateLabel(dataset.categories[i] || String(i + 1), 8)));
    }
    const [columns, ...lines] = dataset.series;
    columns.values.forEach((v, i) => {
      if (typeof v !== "number" || !Number.isFinite(v)) return;
      const y = yOf(v);
      svg.appendChild(makeSvgEl(document, "rect", {
        class: "eo-chart-bar", x: round2(xOf(i) - (band * BAND_FILL) / 2), y: round2(Math.min(y, Number(base))), width: round2(band * BAND_FILL),
        height: round2(Math.max(1, Math.abs(Number(base) - y))), fill: seriesColor(0), "data-series": 0, "data-index": i, "data-value": v,
      }));
    });
    lines.forEach((s, k) => {
      const color = seriesColor(k + 1);
      const pts = s.values.map((v, i) => (typeof v === "number" && Number.isFinite(v) ? { x: xOf(i), y: yOf(v), i, v } : null)).filter(Boolean);
      if (pts.length > 1) {
        svg.appendChild(makeSvgEl(document, "path", { class: "eo-chart-line", d: pts.map((p, j) => (j === 0 ? "M " : "L ") + round2(p.x) + " " + round2(p.y)).join(" "), fill: "none", stroke: color, "stroke-width": "2", "data-series": k + 1 }));
      }
      pts.forEach((p) => svg.appendChild(makeSvgEl(document, "circle", { class: "eo-chart-point", cx: round2(p.x), cy: round2(p.y), r: "3", fill: color, "data-series": k + 1, "data-index": p.i, "data-value": p.v })));
    });
    return { ok: true, legendItems: dataset.series.map((s, idx) => ({ color: seriesColor(idx), label: s.name })) };
  }

  host.__EASYOFFICE_CHART_KINDS__ = Object.assign(host.__EASYOFFICE_CHART_KINDS__ || {}, { combo: { render } });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
