(function (host) {
  "use strict";

  // Numeric row height (0..409 pt) and column width (0..255 chars) dialogs and autofit (PD-356).
  // Exact digit-width conversion mirrors `crates/easyoffice-formats/src/sheet_digit_width.rs` and
  // `sheet_default_col_width.rs` (ECMA-376 18.3.1.13).
  const MAX_ROW_PT = 409;
  const MAX_COL_CHARS = 255;
  const DEFAULT_SIZE_PT = 11;
  const MAX_DIGIT_PX = 64;
  const UNITS_PER_EM = 2048;
  const PX_PER_PT = 96 / 72;
  const CALIBRI_ADVANCE = 1038;
  const MALGUN_ADVANCE = 1116;

  const DIGIT_ADVANCES = {
    calibri: CALIBRI_ADVANCE,
    carlito: CALIBRI_ADVANCE,
    arial: 1139,
    helvetica: 1139,
    "liberation sans": 1139,
    "times new roman": 1024,
    "liberation serif": 1024,
    tinos: 1024,
    "courier new": 1229,
    "liberation mono": 1229,
    cousine: 1229,
    verdana: 1302,
    tahoma: 1118,
    georgia: 1257,
    "trebuchet ms": 1074,
    cambria: 1135,
    caladea: 1135,
    "malgun gothic": MALGUN_ADVANCE,
    "맑은 고딕": MALGUN_ADVANCE,
    aptos: 1094,
    "aptos narrow": CALIBRI_ADVANCE,
    "aptos display": 1071,
    "aptos mono": 1229,
    "aptos serif": 1120,
  };

  function workbookDigitPx() {
    const df = host.__EASYOFFICE_SHEET_DEFAULT_FONT__?.get?.();
    const key = String(df?.name || "맑은 고딕").trim().toLowerCase();
    const adv = DIGIT_ADVANCES[key] ?? CALIBRI_ADVANCE;
    const pt = Number.isFinite(df?.size) && df.size > 0 && df.size <= MAX_ROW_PT ? df.size : DEFAULT_SIZE_PT;
    return Math.max(1, Math.min(MAX_DIGIT_PX, Math.round((adv / UNITS_PER_EM) * pt * PX_PER_PT)));
  }

  function pxToXlsxWidth(px, digitPx = workbookDigitPx()) {
    const d = Math.max(1, Math.min(MAX_DIGIT_PX, digitPx));
    return Math.round((Math.floor((px / d) * 256) / 256) * 100) / 100;
  }

  function charsToPx(chars, digitPx = workbookDigitPx()) {
    if (!Number.isFinite(chars) || chars <= 0) return 0;
    const d = Math.max(1, Math.min(MAX_DIGIT_PX, digitPx));
    const rounding = Math.floor(128 / d);
    return Math.floor(((256 * chars + rounding) / 256) * d);
  }

  function createSizeDialogs(deps) {
    const {
      document,
      kit,
      grid,
      getActiveSheetName,
      getRange,
      getRanges,
      getSelection,
      getExtent,
      commitIfEditing,
      focusAnchor,
      refreshChrome,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
      fitColumns,
      fitRows,
      say,
    } = deps;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    const RH = host.__EASYOFFICE_SHEET_ROW_HEIGHT__;
    const { $, val, setVal, openDialog, closeDialog, note } = kit;
    let mode = "row";
    let bound = false;

    function rowSpans() {
      const sel = getSelection();
      if (RH.coversAllRows({ ...sel, ...getRange() })) return [[1, RH.MAX_ROW]];
      const ranges = typeof getRanges === "function" ? getRanges() : [getRange()];
      return ranges.map((r) => [r.r1, r.r2]);
    }

    function colSpans() {
      const sel = getSelection();
      const ranges = typeof getRanges === "function" ? getRanges() : [getRange()];
      if (sel.whole || (sel.axis === "col" && ranges[0].c1 <= 1 && ranges[0].c2 >= grid.MAX_COLS)) {
        const ext = getExtent?.() || { cols: 8 };
        return [[1, Math.max(8, ext.cols || 8)]];
      }
      return ranges.map((r) => [r.c1, r.c2]);
    }

    function currentRowHeightText(spans) {
      let firstPt = null;
      const limit = Math.min(spans.reduce((s, [a, b]) => s + (b - a + 1), 0), 2000);
      if (spans.length === 1 && spans[0][0] === 1 && spans[0][1] >= RH.MAX_ROW) {
        const defPt = layoutApi.defaultRowHeightOf(getActiveSheetName());
        return String(defPt ?? Math.round(grid.ROW_H * 75) / 100);
      }
      let checked = 0;
      for (const [r1, r2] of spans) {
        for (let r = r1; r <= r2 && checked < limit; r += 1, checked += 1) {
          const pt = layoutApi.rowHidden(getActiveSheetName(), r) ? 0 : Math.round(grid.rowHeight(r) * 75) / 100;
          if (firstPt === null) firstPt = pt;
          else if (Math.abs(firstPt - pt) > 0.01) return "";
        }
      }
      return firstPt === null ? "" : String(firstPt);
    }

    function currentColWidthText(spans) {
      let firstW = null;
      const sheet = getActiveSheetName();
      const digit = workbookDigitPx();
      let checked = 0;
      for (const [c1, c2] of spans) {
        for (let c = c1; c <= c2 && checked < 512; c += 1, checked += 1) {
          const w = layoutApi.colHidden(sheet, c) ? 0 : pxToXlsxWidth(grid.colWidth(c), digit);
          if (firstW === null) firstW = w;
          else if (Math.abs(firstW - w) > 0.01) return "";
        }
      }
      return firstW === null ? "" : String(firstW);
    }

    function open(nextMode) {
      ensureBound();
      commitIfEditing();
      const sheet = getActiveSheetName();
      if (protectApi && !protectApi.guardAction(sheet, nextMode === "row" ? "formatRows" : "formatColumns")) {
        return false;
      }
      mode = nextMode;
      const titleEl = $("sheet-size-dialog-title");
      const labelEl = $("sheet-size-label");
      const input = $("sheet-size-input");
      if (titleEl) titleEl.textContent = mode === "row" ? "행 높이" : "열 너비";
      if (labelEl) labelEl.textContent = mode === "row" ? "행 높이(pt)" : "열 너비(문자)";
      if (input) {
        input.min = "0";
        input.max = mode === "row" ? String(MAX_ROW_PT) : String(MAX_COL_CHARS);
        input.step = "0.25";
      }
      const initText = mode === "row" ? currentRowHeightText(rowSpans()) : currentColWidthText(colSpans());
      setVal("sheet-size-input", initText);
      note("sheet-size-status", "");
      openDialog("sheet-size-dialog");
      input?.focus?.();
      input?.select?.();
      return true;
    }

    function applyRowHeight(value) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        const name = getActiveSheetName();
        if (protectApi && !protectApi.guardAction(name, "formatRows")) return false;
        const spans = rowSpans();
        const all = spans.length === 1 && spans[0][0] === 1 && spans[0][1] >= RH.MAX_ROW;
        const step = sheetHistory.begin({ parts: ["sheetLayout"] });
        if (value === 0) {
          for (const [r1, r2] of spans) {
            const layout = layoutApi.get(name);
            if ((layout ? layoutApi.hiddenTotal(layout.hiddenRows) : 0) + (r2 - r1 + 1) > 100000) {
              say("숨길 수 있는 행은 100000개까지");
              return false;
            }
            layoutApi.setHidden(name, "row", r1, r2, true);
          }
          sheetHistory.commit(step);
          grid.layoutChanged();
          refreshChrome();
          say("행 숨김");
          return true;
        }
        const pt = Math.round(value * 100) / 100;
        if (all) {
          layoutApi.setHidden(name, "row", 1, RH.MAX_ROW, false);
          layoutApi.setDefaultRowHeight(name, pt);
          layoutApi.setRowHeights(name, new Map());
        } else {
          const before = layoutApi.rowHeightsOf(name) || new Map();
          const next = new Map(before);
          let fresh = 0;
          for (const [r1, r2] of spans) {
            for (let r = r1; r <= r2; r += 1) {
              if (!next.has(r)) fresh += 1;
              if (before.size + fresh > RH.MAX_ROW_HEIGHTS) {
                say("행 높이를 지정한 행이 너무 많음");
                return false;
              }
              next.set(r, pt);
            }
            layoutApi.setHidden(name, "row", r1, r2, false);
          }
          layoutApi.setRowHeights(name, next);
        }
        sheetHistory.commit(step);
        grid.layoutChanged();
        refreshChrome();
        say("행 높이: " + pt + "pt");
        return true;
      });
    }

    function applyColWidth(value) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        const name = getActiveSheetName();
        if (protectApi && !protectApi.guardAction(name, "formatColumns")) return false;
        const spans = colSpans();
        const step = sheetHistory.begin({ parts: ["sheetColumnWidths", "sheetLayout"] });
        if (value === 0) {
          for (const [c1, c2] of spans) layoutApi.setHidden(name, "col", c1, c2, true);
          sheetHistory.commit(step);
          grid.layoutChanged();
          refreshChrome();
          say("열 숨김");
          return true;
        }
        const px = Math.max(grid.MIN_COL_W, Math.min(grid.MAX_COL_W, Math.round(charsToPx(value))));
        const updates = [];
        for (const [c1, c2] of spans) {
          layoutApi.setHidden(name, "col", c1, c2, false);
          for (let c = c1; c <= c2; c += 1) updates.push([c, px]);
        }
        grid.applyColWidths(updates);
        sheetHistory.commit(step);
        grid.layoutChanged();
        refreshChrome();
        say("열 너비: " + value);
        return true;
      });
    }

    function submit() {
      const raw = String(val("sheet-size-input")).trim();
      const num = raw === "" ? NaN : Number(raw);
      const maxVal = mode === "row" ? MAX_ROW_PT : MAX_COL_CHARS;
      if (!Number.isFinite(num) || num < 0 || num > maxVal) {
        note("sheet-size-status", "0~" + maxVal + " 사이의 숫자");
        return;
      }
      const targetMode = mode;
      closeDialog("sheet-size-dialog");
      focusAnchor();
      if (targetMode === "row") void applyRowHeight(num);
      else void applyColWidth(num);
    }

    function autofitRows() {
      const spans = rowSpans();
      if (spans.length === 1 && spans[0][0] === 1 && spans[0][1] >= RH.MAX_ROW) {
        return fitRows(1, RH.MAX_ROW, true);
      }
      const [first, last] = spans[0] || [1, 1];
      return fitRows(first, last, false);
    }

    function autofitCols() {
      const cols = new Set();
      for (const [c1, c2] of colSpans()) {
        for (let c = c1; c <= c2 && cols.size < 256; c += 1) cols.add(c);
      }
      return fitColumns(Array.from(cols).sort((a, b) => a - b));
    }

    function ensureBound() {
      if (bound) return;
      bound = true;
      $("btn-sheet-size-ok")?.addEventListener("click", submit);
      $("sheet-size-input")?.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          submit();
        }
      });
    }

    return {
      openRowHeight: () => open("row"),
      openColWidth: () => open("col"),
      autofitRows,
      autofitCols,
      ensureBound,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_SIZE_DIALOGS__ = createSizeDialogs;
  host.__EASYOFFICE_SHEET_COL_UNITS__ = { workbookDigitPx, pxToXlsxWidth, charsToPx, MAX_ROW_PT, MAX_COL_CHARS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
