(function (host) {
  "use strict";

  // Row/column-oriented sheet commands (sort range choice, chart summary)
  // operating on the sheet model so they work for rows that are not currently materialized.

  const { numberOf } = host.__EASYOFFICE_SHEET_SORT_CORE__;

  // Packs (row << 8) | colCharCode for single-letter columns A..Z (0 when invalid).
  function packSingleColRef(ref) {
    const len = ref.length;
    if (len < 2) return 0;
    const c0 = ref.charCodeAt(0);
    const colCode = c0 >= 97 && c0 <= 122 ? c0 - 32 : c0;
    if (colCode < 65 || colCode > 90) return 0;
    let row = 0;
    for (let i = 1; i < len; i += 1) {
      const d = ref.charCodeAt(i) - 48;
      if (d < 0 || d > 9) return 0;
      row = row * 10 + d;
    }
    return row >= 1 ? (row << 8) | colCode : 0;
  }

  // Single-pass O(N) scan of sheetDataStore keys: finds max data row (at least 6) and
  // optionally collects non-formula numeric values in columns B..E (colCode 66..69, row >= 2).
  function scanDataRange(map, outNums) {
    let maxRow = 6;
    const keys = Object.keys(map);
    for (let i = 0; i < keys.length; i += 1) {
      const ref = keys[i];
      const entry = map[ref];
      if (!entry || entry.formula) continue;
      const packed = packSingleColRef(ref);
      if (packed === 0) continue;
      const colCode = packed & 0xff;
      const row = packed >>> 8;
      if (row < 2 || colCode < 66 || colCode > 69) continue;
      const n = numberOf(entry);
      if (n === null) continue;
      if (row > maxRow) maxRow = row;
      if (outNums) outNums.push(n);
    }
    return maxRow;
  }

  function cloneStyleWithoutFreeze(style) {
    if (!style) return undefined;
    const copy = structuredClone(style);
    delete copy.frozenPanes;
    return copy;
  }

  function createSheetOps(api) {
    const {
      document,
      gridUtils,
      sheetDataStore,
      getActiveSheetName,
      getActiveSheetCellRef,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      refreshWindowFromModel,
      refreshFocusedSheetCellPendingSnapshot,
      updateSheetSelectionAndAggregates,
      recalculateActiveSheetFormulas,
      getRange,
      call,
      hasCellContent,
    } = api;
    const sorter = host.__EASYOFFICE_CREATE_SHEET_SORT__({
      gridUtils,
      call,
      layoutApi: host.__EASYOFFICE_SHEET_LAYOUT__,
      cloneStyle: cloneStyleWithoutFreeze,
    });

    // Highest row/column that holds content (0 when the sheet is empty).
    function usedExtent(map) {
      let maxRow = 0;
      let maxCol = 0;
      for (const ref of Object.keys(map)) {
        if (!hasCellContent(map[ref])) continue;
        const pos = gridUtils.parseRef(ref);
        if (!pos) continue;
        if (pos.row > maxRow) maxRow = pos.row;
        if (pos.col > maxCol) maxCol = pos.col;
      }
      return { maxRow, maxCol };
    }

    // A multi-cell selection sorts exactly that block (first column is the key). Without one the
    // data range is every used column below the header row, keyed on column B (or A when alone),
    // ending at the last row with a typed key so formula total rows below it stay in place.
    function sortRange(map, used) {
      const sel = getRange();
      const hasSelection = sel.r1 !== sel.r2 || sel.c1 !== sel.c2;
      if (hasSelection) {
        return {
          r1: sel.r1,
          r2: Math.min(sel.r2, used.maxRow),
          c1: sel.c1,
          c2: Math.min(sel.c2, used.maxCol),
          keyCol: sel.c1,
        };
      }
      const keyCol = Math.min(2, used.maxCol);
      let r2 = 1;
      for (let r = used.maxRow; r >= 2; r -= 1) {
        const entry = map[gridUtils.refOf(r, keyCol)];
        if (hasCellContent(entry) && !entry.formula) {
          r2 = r;
          break;
        }
      }
      return { r1: 2, r2, c1: 1, c2: used.maxCol, keyCol };
    }

    // `target` is `{ range: { r1, r2, c1, c2, keyCol }, direction: 1 | -1 }` (the auto filter's
    // sort of one column), or `{ range, keys: [{ col, asc }], hasHeader, caseSensitive }` (the sort
    // dialog). Without a target the selection or data range is sorted by its key column, and one
    // that is already ascending is reversed. Resolves to whether rows changed place.
    async function sortActiveSheetRange(target) {
      const curSheet = getActiveSheetName();
      const map = snapshotDomSheetCells();
      sheetDataStore[curSheet] = map;
      const range = target?.range ?? sortRange(map, usedExtent(map));
      if (range.r2 <= range.r1) return false;
      const spec = {
        range,
        keys: target?.keys ?? [{ col: range.keyCol, asc: (target?.direction ?? 1) === 1 }],
        hasHeader: Boolean(target?.hasHeader),
        caseSensitive: Boolean(target?.caseSensitive),
      };
      let result = await sorter.run(curSheet, map, spec);
      if (result.ok && result.moved === 0 && !target) {
        spec.keys = [{ col: range.keyCol, asc: false }];
        result = await sorter.run(curSheet, map, spec);
      }
      if (!result.ok) {
        const fb = document.getElementById("status-command-feedback");
        if (result.why === "protected") host.__EASYOFFICE_SHEET_PROTECT__.alertBlocked();
        else if (fb) fb.textContent = host.__EASYOFFICE_SHEET_SORT_CORE__.FAILURES[result.why];
        return false;
      }
      if (result.moved === 0) return false;
      refreshWindowFromModel();
      saveActiveSheetCellsToWorkbook();
      refreshFocusedSheetCellPendingSnapshot();
      updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "A1");
      await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
      return true;
    }

    function applySheetChartSummary(execRes) {
      const { aggregateNumbers } = host.__EASYOFFICE_SHEET_UTILS__;
      const map = snapshotDomSheetCells();
      const nums = [];
      scanDataRange(map, nums);
      const { count, sum, max: maxVal, avg: rawAvg } = aggregateNumbers(nums);
      const avg = count > 0 ? Math.round(rawAvg * 10) / 10 : 0;
      const prefix = "차트 데이터 요약";
      const summaryToken = execRes?.summary || "chart";
      const summaryLine =
        prefix +
        " · 합계 " +
        sum +
        " · 평균 " +
        avg +
        " · 최대 " +
        maxVal +
        " (" +
        summaryToken +
        ")";
      const out = document.getElementById("sheet-eval-result");
      if (out) {
        out.textContent = summaryLine;
        out.setAttribute("title", summaryLine);
      }
      const smartOut = document.getElementById("smart-summary-output");
      if (smartOut) smartOut.textContent = summaryLine;
    }

    return {
      sortActiveSheetRange,
      applySheetChartSummary,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_OPS__ = createSheetOps;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
