// Row heights a user set (PD-277), the `customHeight` rows of a spreadsheet file. A sheet's `rowHeights` is a `Map` of the
// 1-based row to its height in points; every other row follows its content (`sheet-row-fit.js`). Setting a height turns the
// automatic height of the row off, and fitting a row (a double click on the edge of its header) gives the row back to its
// content, which is what taking it out of the map does.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 4767b3b309467c283bbd52b725536ca73861e0fc):
//   packages/sheets/src/commands/commands/set-worksheet-row-height.command.ts :50-201
//       (`DeltaRowHeightCommand`: a drag moves the height of the pressed row by the distance; the selected rows when the
//       pressed one is among them, else only the pressed one; the height is one the sheet may hold, else nothing happens)
//   same file :210-315 (`SetRowHeightCommand`: the rows get the height and stop following their content)
//   same file :323-389 (`SetWorksheetRowIsAutoHeightCommand`: the rows follow their content again)
//   packages/sheets/src/commands/mutations/set-worksheet-row-height.mutation.ts :118-180 (the height and the flag of a row, kept apart)
// Modifications: the height is kept in points (a file's `ht` as it came, so a file written again says the same), and the
// flag is the row's presence in the map instead of a second field; a range of rows is stored per row up to a cap, and a
// request over the cap is refused whole (the reference stores every row of a "select all" range).
(function (host) {
  "use strict";

  const PX_PER_PT = 4 / 3;
  const MAX_PT = 409; // the tallest row a spreadsheet file holds
  const MAX_ROW = 1048576;
  const MAX_ROW_HEIGHTS = 100000; // rows with a height one sheet may carry (the engine's limit)
  const MIN_PX = 4; // a drag stops here: a row of height 0 is a hidden row, which has its own command

  const ptToPx = (pt) => Math.round(pt * PX_PER_PT);
  const pxToPt = (px) => Math.round(px * 75) / 100; // hundredths of a point, as the files keep it
  const clampPx = (px) => Math.max(MIN_PX, Math.min(ptToPx(MAX_PT), Math.round(px)));
  const validPt = (pt) => typeof pt === "number" && Number.isFinite(pt) && pt > 0 && pt <= MAX_PT;

  // The map from the engine's `rowHeights` (`[[row, pt], ...]`, a third item 0 marks a height that follows the content, which
  // the page works out itself and so leaves out). A bad entry is skipped, and the map stops at the cap.
  function fromJson(list) {
    const map = new Map();
    if (!Array.isArray(list)) return map;
    for (const item of list) {
      if (map.size >= MAX_ROW_HEIGHTS) break;
      if (!Array.isArray(item) || item[2] === 0) continue;
      const [row, pt] = item;
      if (Number.isInteger(row) && row >= 1 && row <= MAX_ROW && validPt(pt)) map.set(row, pt);
    }
    return map;
  }

  const toJson = (map) => [...map].sort((a, b) => a[0] - b[0]).map(([row, pt]) => [row, pt]);

  // `[[row, px], ...]` in row order: what the grid lays out.
  const pxEntries = (map) => [...map].sort((a, b) => a[0] - b[0]).map(([row, pt]) => [row, ptToPx(pt)]);

  // A copy of `map` with the rows `first..last` at `px` (clamped to what a row may be), null when that would be more rows than a sheet
  // may carry. A range of a million rows is counted, never walked.
  function withHeight(map, first, last, px) {
    const next = new Map(map);
    const clamped = clampPx(px);
    const pt = clamped >= ptToPx(MAX_PT) ? MAX_PT : pxToPt(clamped); // the tallest row is 409 pt exactly
    const fresh = last - first + 1 - [...map.keys()].filter((r) => r >= first && r <= last).length;
    if (map.size + fresh > MAX_ROW_HEIGHTS) return null;
    for (let r = first; r <= last; r += 1) next.set(r, pt);
    return next;
  }

  // A copy of `map` without the rows `first..last`: they follow their content again.
  function withoutRows(map, first, last) {
    const next = new Map(map);
    for (const r of map.keys()) if (r >= first && r <= last) next.delete(r);
    return next;
  }

  // The rows a drag on the edge of row `pressed` sets: the selected rows when it is one of them, else only it
  // (`selection` is `{ axis, r1, r2 }` as the grid's range is).
  function rowsToSet(pressed, selection) {
    return selection?.axis === "row" && pressed >= selection.r1 && pressed <= selection.r2 ? [selection.r1, selection.r2] : [pressed, pressed];
  }

  host.__EASYOFFICE_SHEET_ROW_HEIGHT__ = Object.freeze({
    fromJson, toJson, pxEntries, withHeight, withoutRows, rowsToSet, ptToPx, pxToPt, clampPx, MIN_PX, MAX_ROW_HEIGHTS,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
