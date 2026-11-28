import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// PD-402: the chart kinds 도넛형, 방사형, 거품형, 주식형 and 혼합 in the slide and sheet windows.

async function openSlide() {
  const h = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-chart-kinds-pd402" });
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  return h;
}

const layer = (h) => h.document.getElementById("slide-canvas-layer");
const CATS = ["1", "2", "3", "4"];
const add = (h, chartType, series, categories = CATS) => {
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "chart", x: 40, y: 40, w: 480, h: 300, chartType, title: "차트", categories, series, legend: "bottom" });
  // The chart just added is the last one of the slide.
  const all = layer(h).querySelectorAll("svg.eo-chart-svg");
  return all[all.length - 1];
};
const s = (name, values) => ({ name, values });

test("the insert dialogs list the five new kinds with the Korean names", async () => {
  const h = await openSlide();
  for (const id of ["slide-chart-type", "sheet-chart-type"]) {
    const options = h.document.getElementById(id).querySelectorAll("option").map((o) => [o.getAttribute("value"), o.textContent]);
    assert.deepEqual(options.slice(5), [["doughnut", "도넛형"], ["radar", "방사형"], ["bubble", "거품형"], ["stock", "주식형"], ["combo", "혼합"]], id);
  }
});

test("도넛형 draws one ring sector per positive value with angles of value * 2π / sum", async () => {
  const h = await openSlide();
  const svg = add(h, "doughnut", [s("매출", [10, 30, 0, 60])]);
  const slices = svg.querySelectorAll(".eo-chart-doughnut");
  assert.equal(slices.length, 3, "the zero is no slice");
  assert.deepEqual(slices.map((p) => p.getAttribute("data-value")), ["10", "30", "60"]);
  const { sliceRows, ringPath } = h.host.__EASYOFFICE_CHART_KINDS__.doughnut;
  assert.equal(sliceRows({ series: [s("a", [1, -2, NaN, 3])], categories: ["x", "y", "z", "w"] }).length, 2);
  assert.match(ringPath(100, 100, 40, 70, 0, Math.PI, (v) => Math.round(v * 100) / 100), /^M 170 100 A 70 70 0 0 1 30 100 L 60 100 A 40 40 0 0 0 140 100 Z$/);
});

test("방사형 puts the axes counterclockwise from the top and one polygon per series", async () => {
  const h = await openSlide();
  const svg = add(h, "radar", [s("가", [5, 8, 3, 6]), s("나", [2, 4, 9, 1])]);
  assert.equal(svg.querySelectorAll(".eo-chart-radar-area").length, 2);
  assert.equal(svg.querySelectorAll(".eo-chart-axis").length, 4);
  const { axisPoint } = h.host.__EASYOFFICE_CHART_KINDS__.radar;
  const top = axisPoint(100, 100, 50, 0, 4);
  const left = axisPoint(100, 100, 50, 1, 4);
  assert.deepEqual([Math.round(top.x), Math.round(top.y)], [100, 50], "the first axis is on top");
  assert.deepEqual([Math.round(left.x), Math.round(left.y)], [50, 100], "the next one is to the left (counterclockwise)");
  const two = add(h, "radar", [s("x", [1, 2])], ["a", "b"]);
  assert.equal(two.querySelectorAll(".eo-chart-radar-area").length, 0, "two axes make no polygon");
});

test("거품형 sizes the bubbles with the square root of the size and keeps pairs of series", async () => {
  const h = await openSlide();
  const svg = add(h, "bubble", [s("Y", [3, 5, 2, 8]), s("Y (크기)", [100, 25, 4, 0])]);
  const bubbles = svg.querySelectorAll(".eo-chart-bubble");
  assert.equal(bubbles.length, 4);
  const r = (size) => Number(bubbles.find((b) => b.getAttribute("data-size") === String(size)).getAttribute("r"));
  assert.ok(Math.abs(r(100) / r(25) - 2) < 0.05, "four times the size is twice the radius");
  assert.equal(r(0), 3, "no size is the smallest bubble");
  const { radiusOf } = h.host.__EASYOFFICE_CHART_KINDS__.bubble;
  assert.equal(radiusOf(50, 100, 40), 40 * Math.sqrt(0.5));
  assert.equal(radiusOf(NaN, 100, 40), 3);
});

test("주식형 draws a body between open and close and a whisker from low to high, with three or four series", async () => {
  const h = await openSlide();
  const four = [s("시가", [10, 12, 11, 15]), s("고가", [14, 15, 13, 18]), s("저가", [8, 10, 9, 12]), s("종가", [12, 11, 12, 14])];
  const svg = add(h, "stock", four);
  assert.equal(svg.querySelectorAll(".eo-chart-candle").length, 4);
  assert.equal(svg.querySelectorAll(".eo-chart-whisker").length, 4);
  const fills = svg.querySelectorAll(".eo-chart-candle").map((c) => c.getAttribute("fill"));
  const { RISE, FALL } = h.host.__EASYOFFICE_CHART_KINDS__.stock;
  assert.deepEqual(fills, [RISE, FALL, RISE, FALL], "close above open rises");
  const three = add(h, "stock", four.slice(1));
  assert.equal(three.querySelectorAll(".eo-chart-candle").length, 0);
  assert.equal(three.querySelectorAll(".eo-chart-close-tick").length, 4);
  const { candleAt } = h.host.__EASYOFFICE_CHART_KINDS__.stock;
  assert.equal(candleAt(four.map((x) => ({ ...x, values: [null] })), 0), null, "a missing value leaves no candle");
});

test("혼합 draws the first series as columns and the others as lines on one scale", async () => {
  const h = await openSlide();
  const svg = add(h, "combo", [s("막대", [10, 20, 30, 40]), s("선", [5, 15, 25, 35]), s("선 2", [40, 30, 20, 10])]);
  assert.equal(svg.querySelectorAll(".eo-chart-bar").length, 4);
  assert.equal(svg.querySelectorAll(".eo-chart-line").length, 2);
  assert.equal(svg.querySelectorAll(".eo-chart-point").length, 8);
});

test("a kind with too little data says so and an unknown kind is not drawn by the new renderers", async () => {
  const h = await openSlide();
  const empty = add(h, "doughnut", [s("a", [0, 0, 0, 0])]);
  assert.ok(empty.querySelector(".eo-chart-empty"), "no positive value, no ring");
  const registry = h.host.__EASYOFFICE_CHART_KINDS__;
  assert.deepEqual(Object.keys(registry).sort(), ["bubble", "combo", "doughnut", "radar", "stock"]);
  assert.equal(Object.hasOwn(registry, "constructor"), false);
});
