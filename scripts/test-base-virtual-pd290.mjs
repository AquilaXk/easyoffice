import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-290 the range maths ported from the virtual-core of TanStack Virtual (`base-virtual.js`), against a plain scan of the same layout.

const host = {};
loadUiModule(new URL("../crates/easyoffice-app/ui/base-virtual.js", import.meta.url), host);
const V = host.__EASYOFFICE_BASE_VIRTUAL__;

// A seeded generator, so a failure repeats.
function lcg(seed) {
  let s = seed >>> 0;
  return () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 2 ** 32; };
}

// The items that show in [offset, offset + size): those whose end is past the offset and whose start is before the limit, as a plain scan finds them.
function scanRange(flat, count, size, offset) {
  let startIndex = 0;
  for (let i = 0; i < count; i += 1) if (flat[i * 2] <= offset) startIndex = i;
  let endIndex = startIndex;
  while (endIndex < count - 1 && flat[endIndex * 2] + flat[endIndex * 2 + 1] < offset + size) endIndex += 1;
  return { startIndex, endIndex };
}

test("layoutItems lays the items one after the other from the margin, and reuses an array that is big enough", () => {
  const sizes = [24, 24, 30, 24];
  const flat = V.layoutItems(4, (i) => sizes[i], 26, null);
  assert.deepEqual([...flat], [26, 24, 50, 24, 74, 30, 104, 24]);
  assert.equal(V.listEnd(flat, 4), 128);
  assert.equal(V.layoutItems(2, () => 10, 0, flat), flat, "the earlier array is kept");
  assert.equal(V.listEnd(new Float64Array(0), 0), 0);
});

test("calculateRange finds the same items as a scan, for uniform and for mixed sizes", () => {
  const rand = lcg(290);
  for (let round = 0; round < 40; round += 1) {
    const count = 2 + Math.floor(rand() * 400), mixed = round % 2 === 1;
    const sizes = Array.from({ length: count }, () => (mixed ? 18 + Math.floor(rand() * 20) : 24));
    const margin = Math.floor(rand() * 40), flat = V.layoutItems(count, (i) => sizes[i], margin, null), end = V.listEnd(flat, count);
    for (let probe = 0; probe < 60; probe += 1) {
      const size = 50 + Math.floor(rand() * 600), offset = Math.floor(rand() * (end - size));
      assert.deepEqual(V.calculateRange(flat, count, size, offset, 0), scanRange(flat, count, size, offset), `count ${count} size ${size} offset ${offset}`);
    }
  }
});

test("calculateRange holds the edges: nothing to show, one item, an offset past the end", () => {
  const flat = V.layoutItems(100, () => 24, 26, null);
  assert.equal(V.calculateRange(flat, 0, 500, 0, 0), null);
  assert.equal(V.calculateRange(flat, 100, 0, 0, 0), null, "a view with no height");
  assert.deepEqual(V.calculateRange(V.layoutItems(1, () => 24, 26, null), 1, 500, 0, 0), { startIndex: 0, endIndex: 0 });
  assert.deepEqual(V.calculateRange(flat, 100, 240, 0, 0), { startIndex: 0, endIndex: 8 }, "scrolled to the top: the 26 px above the first item are not an item");
  // The element ends 48 px past the list (the new record row and the footer): an offset up to there is the end of the list.
  const end = V.listEnd(flat, 100);
  const atEnd = V.calculateRange(flat, 100, 240, end + 48 - 240, 48);
  assert.equal(atEnd.endIndex, 99);
  // An offset past what the element can reach (a rubber band, a list that got shorter) is read as the last position, not as the last item alone.
  const past = V.calculateRange(flat, 100, 240, end + 5000, 48);
  assert.deepEqual(past, atEnd);
  assert.ok(past.endIndex - past.startIndex >= 7, "the last 192 px of items, not the last item alone");
});

test("extendRange grows the range on both sides and stays inside the list", () => {
  assert.deepEqual(V.extendRange({ startIndex: 50, endIndex: 60 }, 12, 1000), { from: 38, to: 72 });
  assert.deepEqual(V.extendRange({ startIndex: 3, endIndex: 9 }, 12, 1000), { from: 0, to: 21 });
  assert.deepEqual(V.extendRange({ startIndex: 990, endIndex: 999 }, 12, 1000), { from: 978, to: 999 });
});

test("findNearestBinarySearch gives the exact index, or the one before where the value would go", () => {
  const starts = [0, 10, 20, 30, 40];
  const at = (value) => V.findNearestBinarySearch(0, starts.length - 1, (i) => starts[i], value);
  assert.deepEqual([at(-5), at(0), at(5), at(10), at(39), at(40), at(900)], [0, 0, 0, 1, 3, 4, 4]);
  const flat = V.layoutItems(5, () => 10, 0, null);
  for (const value of [-5, 0, 5, 10, 39, 40, 900]) assert.equal(V.findNearestBinarySearchFlat(flat, 4, value), at(value));
});

test("a 100,000 item layout and range cost a few milliseconds", () => {
  const count = 100000, started = performance.now();
  const flat = V.layoutItems(count, () => 24, 26, null);
  const laid = performance.now() - started;
  const range = V.calculateRange(flat, count, 800, 1200000, 48);
  assert.equal(range.startIndex, Math.floor((1200000 - 26) / 24));
  assert.ok(laid < 100, `layout of 100,000 items: ${laid.toFixed(2)} ms`);
});
