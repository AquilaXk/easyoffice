(function (host) {
  "use strict";

  // Row/column-oriented sheet commands (sort, pivot/chart summary, conditional formatting)
  // operating on the sheet model so they work for rows that are not currently materialized.
  const { normalizeFormattedNumericCell } = host.__EASYOFFICE_SHEET_UTILS__;
  const SORT_COL_LETTERS = ["A", "B", "C", "D", "E", "F", "G", "H"];

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

  function setCondfmtFlag(entry, on) {
    if (on) {
      entry.style = {
        fontWeight: "",
        fontStyle: "",
        textDecoration: "",
        fontSize: "",
        ...entry.style,
        condfmtHigh: true,
      };
      return;
    }
    if (!entry.style?.condfmtHigh) return;
    delete entry.style.condfmtHigh;
    const s = entry.style;
    if (!(s.fontWeight || s.fontStyle || s.textDecoration || s.fontSize || s.frozenPanes)) {
      delete entry.style;
    }
  }

  function createSheetOps(api) {
    const {
      document,
      sheetDataStore,
      getActiveSheetName,
      getActiveSheetCellRef,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      refreshWindowFromModel,
      refreshFocusedSheetCellPendingSnapshot,
      updateSheetSelectionAndAggregates,
      hasCellContent,
    } = api;

    function sortActiveSheetByColumnB() {
      const curSheet = getActiveSheetName();
      const map = snapshotDomSheetCells();
      sheetDataStore[curSheet] = map;
      const maxRow = scanDataRange(map, null);
      const populatedRows = [];
      const blankRows = [];
      for (let r = 2; r <= maxRow; r += 1) {
        const cells = {};
        let isBlankRow = true;
        SORT_COL_LETTERS.forEach((col) => {
          const entry = map[col + r];
          if (hasCellContent(entry)) isBlankRow = false;
          if (entry) cells[col] = entry;
        });
        if (isBlankRow) {
          blankRows.push({ origRow: r, bNum: 0, cells });
        } else {
          populatedRows.push({ origRow: r, bNum: numberOf(map["B" + r]) ?? 0, cells });
        }
      }
      if (populatedRows.length < 2) return;
      let isAscending = true;
      for (let i = 1; i < populatedRows.length; i += 1) {
        if (populatedRows[i].bNum < populatedRows[i - 1].bNum) {
          isAscending = false;
          break;
        }
      }
      populatedRows.sort((a, b) => (isAscending ? b.bNum - a.bNum : a.bNum - b.bNum));
      const rowData = [...populatedRows, ...blankRows];
      rowData.forEach((item, idx) => {
        const targetRow = idx + 2;
        SORT_COL_LETTERS.forEach((col) => {
          const src = item.cells[col];
          if (!src) {
            delete map[col + targetRow];
            return;
          }
          const moved = { value: src.value, formula: "" };
          if (src.formula) {
            moved.formula = src.formula.replace(
              new RegExp("([A-Z]+)" + item.origRow + String.raw`\b`, "g"),
              "$1" + targetRow
            );
          }
          const style = cloneStyleWithoutFreeze(src.style);
          if (style) moved.style = style;
          map[col + targetRow] = moved;
        });
      });
      refreshWindowFromModel();
      saveActiveSheetCellsToWorkbook();
      refreshFocusedSheetCellPendingSnapshot();
      updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "F2");
    }

    function applySheetPivotOrChartSummary(mode, execRes) {
      const map = snapshotDomSheetCells();
      const nums = [];
      scanDataRange(map, nums);
      const sum = nums.reduce((acc, v) => acc + v, 0);
      const avg = nums.length > 0 ? Math.round((sum / nums.length) * 10) / 10 : 0;
      const maxVal = nums.length > 0 ? Math.max(...nums) : 0;
      const prefix = mode === "pivot" ? "피벗 집계" : "차트 데이터 요약";
      const summaryToken = execRes?.summary || mode;
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

    function toggleSheetConditionalFormatting(enabled) {
      const curSheet = getActiveSheetName();
      const map = snapshotDomSheetCells();
      sheetDataStore[curSheet] = map;
      const refNums = [];
      scanDataRange(map, refNums);
      const avg = refNums.length > 0 ? refNums.reduce((a, b) => a + b, 0) / refNums.length : 800;
      Object.keys(map).forEach((ref) => {
        const entry = map[ref];
        if (!enabled) {
          setCondfmtFlag(entry, false);
        } else {
          const n = numberOf(entry);
          setCondfmtFlag(entry, n !== null && n >= avg);
        }
        if (!hasCellContent(entry) && !entry.style) delete map[ref];
      });
      refreshWindowFromModel();
      saveActiveSheetCellsToWorkbook();
    }


    return {
      sortActiveSheetByColumnB,
      applySheetPivotOrChartSummary,
      toggleSheetConditionalFormatting,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_OPS__ = createSheetOps;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
