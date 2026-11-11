(function (host) {
  "use strict";

  // Row/column-oriented sheet commands (sort, chart summary)
  // operating on the sheet model so they work for rows that are not currently materialized.
  const { normalizeFormattedNumericCell } = host.__EASYOFFICE_SHEET_UTILS__;

  function numberOf(entry) {
    const text = normalizeFormattedNumericCell(entry?.value);
    const n = Number(text);
    return text !== "" && Number.isFinite(n) ? n : null;
  }

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
      shiftFormula,
      hasCellContent,
    } = api;

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

    function sortKeyOf(entry) {
      const text = String(entry?.value ?? "").trim();
      return { empty: text === "", num: numberOf(entry), text };
    }

    // Numbers before text, text by locale order; blank keys always last.
    function compareSortKeys(a, b, dir) {
      if (a.empty || b.empty) return Number(a.empty) - Number(b.empty);
      let cmp;
      if (a.num !== null && b.num !== null) cmp = a.num - b.num;
      else if (a.num !== null) cmp = -1;
      else if (b.num !== null) cmp = 1;
      else cmp = a.text.localeCompare(b.text, "ko");
      return dir * cmp;
    }

    function sortActiveSheetRange() {
      const curSheet = getActiveSheetName();
      const map = snapshotDomSheetCells();
      sheetDataStore[curSheet] = map;
      const range = sortRange(map, usedExtent(map));
      if (range.r2 <= range.r1) return Promise.resolve();
      const populatedRows = [];
      const blankRows = [];
      for (let r = range.r1; r <= range.r2; r += 1) {
        const cells = [];
        let isBlankRow = true;
        for (let c = range.c1; c <= range.c2; c += 1) {
          const entry = map[gridUtils.refOf(r, c)];
          if (hasCellContent(entry)) isBlankRow = false;
          cells.push(entry);
        }
        const row = { origRow: r, key: sortKeyOf(map[gridUtils.refOf(r, range.keyCol)]), cells };
        (isBlankRow ? blankRows : populatedRows).push(row);
      }
      if (populatedRows.length < 2) return Promise.resolve();
      const isAscending = populatedRows.every(
        (row, i) => i === 0 || compareSortKeys(populatedRows[i - 1].key, row.key, 1) <= 0
      );
      const dir = isAscending ? -1 : 1;
      populatedRows.sort((a, b) => compareSortKeys(a.key, b.key, dir));
      [...populatedRows, ...blankRows].forEach((item, idx) => {
        const targetRow = range.r1 + idx;
        const dRow = targetRow - item.origRow;
        item.cells.forEach((src, offset) => {
          const ref = gridUtils.refOf(targetRow, range.c1 + offset);
          if (!src) {
            delete map[ref];
            return;
          }
          const moved = { value: src.value, formula: "" };
          if (src.formula) moved.formula = dRow === 0 ? src.formula : shiftFormula(src.formula, dRow, 0);
          const style = cloneStyleWithoutFreeze(src.style);
          if (style) moved.style = style;
          map[ref] = moved;
        });
      });
      refreshWindowFromModel();
      saveActiveSheetCellsToWorkbook();
      refreshFocusedSheetCellPendingSnapshot();
      updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "A1");
      return recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
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
