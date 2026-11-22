// 참조 구현 (reference implementation): TanStack Virtual `virtual-core` (MIT, Copyright (c) 2021-present Tanner Linsley,
// commit eb1cb7c9d1709b6fd2ecc773988ffbb095cd4f61, `packages/virtual-core/src/index.ts`; see `THIRD_PARTY_NOTICES.md`) 행 범위 계산.
// 옮긴 함수: `findNearestBinarySearch` (2778-2805), `findNearestBinarySearchFlat` (2807-2826), 한 줄 목록 쪽 `calculateRangeImpl`
// (2828-2866)과 `calculateRange`의 끝 보정 (1883-1921), `defaultRangeExtractor` (98-113), 한 줄 목록 쪽 `getMeasurements` (1731-1767).
// PD-294에서 더한 함수: `getOffsetForIndex` (2408-2449)와 `getOffsetForAlignment` (2381-2402)의 start·end·auto 정렬, `scrollToIndex`의 인덱스 보정 (2463-2475).
// 원본의 `Virtualizer` 클래스(스크롤 요소 관찰, 크기 측정 캐시, 여러 줄, 부드러운 스크롤)는 가져오지 않았다. DOM은 `base-grid-window.js`가 다룬다.

(function (host) {
  "use strict";

  // `flat` holds a pair per item: its start and its size (a Float64Array, as in the original: no object per item).
  const START = 0, SIZE = 1;

  function findNearestBinarySearch(low, high, getCurrentValue, value) {
    while (low <= high) {
      const middle = ((low + high) / 2) | 0, currentValue = getCurrentValue(middle);
      if (currentValue < value) low = middle + 1;
      else if (currentValue > value) high = middle - 1;
      else return middle;
    }
    return low > 0 ? low - 1 : 0;
  }

  function findNearestBinarySearchFlat(flat, high, value) {
    let low = 0;
    while (low <= high) {
      const middle = ((low + high) / 2) | 0, currentValue = flat[middle * 2 + START];
      if (currentValue < value) low = middle + 1;
      else if (currentValue > value) high = middle - 1;
      else return middle;
    }
    return low > 0 ? low - 1 : 0;
  }

  // The starts and sizes of `count` items laid one after the other from `scrollMargin` (the list does not begin at the top of the
  // scroll element: the sticky header is above it). `sizeAt(i)` is the size of item i. The array of an earlier layout is reused when it is big enough.
  function layoutItems(count, sizeAt, scrollMargin, previous) {
    const flat = previous && previous.length >= count * 2 ? previous : new Float64Array(count * 2);
    let runningStart = scrollMargin;
    for (let i = 0; i < count; i += 1) {
      const size = sizeAt(i);
      flat[i * 2 + START] = runningStart;
      flat[i * 2 + SIZE] = size;
      runningStart += size;
    }
    return flat;
  }

  const listEnd = (flat, count) => (count === 0 ? 0 : flat[(count - 1) * 2 + START] + flat[(count - 1) * 2 + SIZE]);

  // The first and the last item that show in `outerSize` px from `scrollOffset`. `trailing` is what follows the list in the scroll element
  // (the new record row and the footer): an offset up to there is where the element can sit, so it is not read as past the last item.
  function calculateRange(flat, count, outerSize, scrollOffset, trailing) {
    if (count === 0 || outerSize === 0) return null;
    const lastIndex = count - 1;
    if (count <= 1) return { startIndex: 0, endIndex: lastIndex };
    const maxScrollOffset = Math.max(listEnd(flat, count) + trailing - outerSize, 0), offset = Math.min(scrollOffset, maxScrollOffset);
    const startIndex = findNearestBinarySearchFlat(flat, lastIndex, offset);
    let endIndex = startIndex;
    const limit = offset + outerSize;
    while (endIndex < lastIndex && flat[endIndex * 2 + START] + flat[endIndex * 2 + SIZE] < limit) endIndex += 1;
    return { startIndex, endIndex };
  }

  // The range grown by `overscan` items on both sides (the original returns the index of each item as an array: the two ends are enough here).
  function extendRange(range, overscan, count) {
    return { from: Math.max(range.startIndex - overscan, 0), to: Math.min(range.endIndex + overscan, count - 1) };
  }

  // The scroll offset that brings item `index` into view (`getOffsetForIndex`, `scrollToIndex`), or null for an empty list. `align` is
  // "start", "end" or "auto" ("center" is left out: nothing asks for it). `view` is { offset: where the element is scrolled to, size: its
  // height, padStart / padEnd: the sticky header and footer that cover the ends of it (the original's `scrollPaddingStart` and `scrollPaddingEnd`),
  // max: the largest offset the element can scroll to }. "auto" leaves the offset alone when the item is fully inside.
  function offsetForIndex(flat, count, index, align, view) {
    if (count === 0) return null;
    const i = Math.max(0, Math.min(index, count - 1)), start = flat[i * 2 + START], end = start + flat[i * 2 + SIZE];
    const inner = Math.max(view.size - view.padStart - view.padEnd, 0);
    let side = align;
    if (side === "auto") {
      if (inner > 0 && end - start > inner) {
        if (start <= view.offset + view.padStart && end >= view.offset + view.size - view.padEnd) return view.offset;
        side = "start";
      } else if (end >= view.offset + view.size - view.padEnd) side = "end";
      else if (start <= view.offset + view.padStart) side = "start";
      else return view.offset;
    }
    // The last item aligned to the end goes to the real end of the element: what follows the list (the new record row) shows as well.
    if (side === "end" && i === count - 1) return Math.max(view.max, 0);
    const target = side === "end" ? end + view.padEnd - view.size : start - view.padStart;
    return Math.max(Math.min(view.max, target), 0);
  }

  host.__EASYOFFICE_BASE_VIRTUAL__ = Object.freeze({ findNearestBinarySearch, findNearestBinarySearchFlat, layoutItems, listEnd, calculateRange, extendRange, offsetForIndex });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
