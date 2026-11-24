// Row heights (PD-207, PD-278). A row a user gave a height keeps it (`sheet-row-height.js`); every other row is as tall as its tallest
// cell needs, never under the grid's row height: a wrapped cell by its lines, any cell by its font. Heights are derived from the sheet
// model and the layout (never stored by this module), so undo, file open and structure edits only have to call `sync()`.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 4767b3b309467c283bbd52b725536ca73861e0fc):
//   packages/engine-render/src/components/sheets/sheet.render-skeleton.ts :718-794 (`calculateAutoHeightInRange`: a row that
//       the user gave a height (`ia` false) is skipped; merged cells are skipped), :796-907 (`calculateAutoHeightForCell`: a
//       wrapped cell is its lines plus the padding, any other cell the height of its font plus the padding),
//       :909-920 (`_calculateRowAutoHeight`: the tallest cell, never under the default row height, never over the
//       tallest a row may be)
//   packages/sheets-ui/src/services/auto-height.service.ts :77-206 (the rows are worked out a slice at a time with a time limit)
// Modifications: the text is measured on the canvas the grid already measures with, and each (font, width, text) once
// (`lineCache`); the cells are visited in one pass over the sheet's cell map, not one lookup per row and column; the
// slices of the reference become one budget (200,000 measurements, 1.5 s) after which the rows not worked out yet keep the
// plain height (fail closed) and the next sync goes on with the answers it already has; the rows are kept as a sorted list with
// running sums so the top of a row is one binary search, and a user's height may be under the grid's row height too.
//
// One pass over the sheet's cells finds the wrapped and the large ones (a cell without the key is a property read); a
// wrapped cell costs one canvas measurement per distinct (font, width, text), remembered in `lineCache`.
(function (host) {
  "use strict";

  const MAX_ROW_PX = 545; // 409 pt, the tallest row a spreadsheet file holds
  const FALLBACK_FONT_PX = 14.67; // Normal font (Malgun Gothic 11 pt) when `sheet-default-font.js` is absent
  const CACHE_LIMIT = 50000;
  const MIN_CELL_PX = 12; // a column narrower than this holds no text
  const SINGLE_LINE_RATIO = 1.275; // a row of one line is about this many times the font size, as the spreadsheet programs size it
  const MAX_MEASURES = 200000; // measurements one sync may make
  const BUDGET_MS = 1500; // and the time it may take
  const CLOCK_EVERY = 256; // measurements between two looks at the clock
  // A scrolled box stops growing near 33 million px: the grid's rows plus the extra height of the taller rows stay
  // under this, and the rows past the budget keep the plain height (fail closed).
  const MAX_TOTAL_PX = 30000000;
  const MIN_EXTRA_BUDGET = 2000000;

  function createRowFit(deps) {
    // `rowH()` is the height of a row with none of its own on the active sheet: the grid's row height, or the sheet's default row height
    // (PD-286). The extra heights of a sheet are relative to the one it had at its last `sync()`.
    const { rowH, maxRows, getSheetName, getMap, columnWidth, isRowHidden, isMerged, measure, packRef, customHeights } = deps;
    const now = deps.now || (() => host.performance?.now?.() ?? Date.now());
    const dfApi = deps.defaultFont || host.__EASYOFFICE_SHEET_DEFAULT_FONT__;
    const baseFontPx = () => (typeof deps.baseFontPx === "function" ? deps.baseFontPx() : dfApi?.baseFontPx?.() ?? FALLBACK_FONT_PX);
    const baseFontFamily = () => (typeof deps.baseFontFamily === "function" ? deps.baseFontFamily() : dfApi?.baseFontFamily?.() ?? "");
    const fontVersion = () => (typeof deps.fontVersion === "function" ? deps.fontVersion() : dfApi?.version?.() ?? 0);
    const gridRowH = () => (typeof deps.gridRowH === "function" ? deps.gridRowH() : 22);
    const isDefaultFontTall = (base) => Math.ceil(baseFontPx() * SINGLE_LINE_RATIO) > Math.max(base, gridRowH());
    // The rows' own height may already fill the scrolled box (a tall default row height): the taller rows still get this much.
    const extraBudgetOf = (base) => Math.max(MAX_TOTAL_PX - maxRows * base, MIN_EXTRA_BUDGET);
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const lineCache = new Map();
    const bySheet = Object.create(null);
    let fontPxOf = new WeakMap();
    let measures = 0;
    let started = 0;
    let overBudget = false;
    let lastFontSig = fontVersion() + "\u0000" + baseFontPx() + "\u0000" + baseFontFamily();
    let fontDirty = false;

    function syncFontCache() {
      const sig = fontVersion() + "\u0000" + baseFontPx() + "\u0000" + baseFontFamily();
      if (sig === lastFontSig) return;
      lastFontSig = sig;
      fontDirty = true;
      lineCache.clear();
      fontPxOf = new WeakMap();
      for (const name of Object.keys(bySheet)) delete bySheet[name];
    }

    const emptyState = () => ({ rows: [], extra: [], total: 0, heights: new Map(), clipped: new Set(), capRows: new Set(), min: rowH(), base: rowH() });
    const stateOf = () => {
      syncFontCache();
      return bySheet[getSheetName()] || (bySheet[getSheetName()] = emptyState());
    };

    // Lines `text` takes in `width` px of `font` (an explicit line break starts a new line); `measure` answers null while the page cannot measure.
    function linesOf(text, width, font, family = "") {
      syncFontCache();
      const key = (family || baseFontFamily()) + "|" + font + "|" + width + "|" + text;
      let n = lineCache.get(key);
      if (n === undefined) {
        measures += 1;
        if (measures > MAX_MEASURES || (measures % CLOCK_EVERY === 0 && now() - started > BUDGET_MS)) {
          overBudget = true;
          return 1;
        }
        n = measure(text, width, font, family);
        // An unmeasurable cell (no canvas yet) is one line and is asked again at the next sync.
        if (n === null) return 1;
        if (lineCache.size >= CACHE_LIMIT) lineCache.clear();
        lineCache.set(key, n);
      }
      return n;
    }

    function fontOf(st) {
      return (st?.fontStyle || "normal") + " " + (st?.fontWeight || "400") + " " + (st?.fontSize || baseFontPx() + "px");
    }

    // The font size of a style in px; a style is shared by every cell that wears it, so it is parsed once.
    function fontPx(st) {
      syncFontCache();
      if (!st) return baseFontPx();
      let px = fontPxOf.get(st);
      if (px === undefined) {
        px = Number.parseFloat(st.fontSize) || baseFontPx();
        fontPxOf.set(st, px);
      }
      return px;
    }

    // The height a cell needs: its font on one line, or its lines when it wraps.
    function cellHeight(st, text, width, px) {
      const oneLine = Math.ceil(px * SINGLE_LINE_RATIO);
      if (!st?.wrap || text === "" || width < MIN_CELL_PX) return oneLine;
      const lines = linesOf(text, width, fontOf(st), st.fontFamily || baseFontFamily());
      if (lines <= 1) return oneLine;
      const lineH = Math.round(px * look.LINE_HEIGHT_RATIO);
      return Math.min(MAX_ROW_PX, lines * lineH + 2 * look.WRAP_PAD_V + 1);
    }

    // The rows whose cells ask for more than the grid's row height, by the tallest need.
    function autoHeights(base, wrapRows, multiWrap) {
      const auto = new Map();
      const map = getMap();
      if (!map || !measure) return auto;
      const defaultPx = baseFontPx();
      const defaultTall = isDefaultFontTall(base);
      for (const ref in map) {
        if (overBudget) break;
        const entry = map[ref];
        const st = entry.style;
        if (!st && (!defaultTall || entry.value === undefined || entry.value === null || entry.value === "")) continue;
        const px = st ? fontPx(st) : defaultPx;
        if (!st?.wrap && !(st?.fontSize ? Math.ceil(px * SINGLE_LINE_RATIO) > base : defaultTall)) continue;
        const packed = packRef(ref);
        if (packed <= 0) continue;
        const col = packed & 32767;
        const row = (packed - col) / 32768;
        if (isRowHidden(row) || isMerged(row, col)) continue;
        const h = Math.min(MAX_ROW_PX, cellHeight(st, String(entry.value ?? ""), columnWidth(col) - 13, px));
        if (h > base && h > (auto.get(row) || 0)) auto.set(row, h);
        if (st?.wrap) {
          if (wrapRows) wrapRows.add(row);
          if (multiWrap && h > Math.max(base, Math.ceil(px * SINGLE_LINE_RATIO))) multiWrap.add(row);
        }
      }
      return auto;
    }

    // Recomputes the heights of the active sheet; true when any row got another height.
    function sync() {
      syncFontCache();
      const state = stateOf();
      const wasFontDirty = fontDirty;
      fontDirty = false;
      measures = 0;
      started = now();
      overBudget = false;
      const base = rowH();
      const fontNeed = Math.ceil(baseFontPx() * SINGLE_LINE_RATIO);
      const extraBudget = extraBudgetOf(base);
      const wrapRows = new Set();
      const multiWrap = new Set();
      const heights = autoHeights(base, wrapRows, multiWrap);
      const clipped = new Set();
      for (const [row, px] of customHeights ? customHeights() : []) {
        if (isRowHidden(row)) continue;
        // A user's height under what the cells need cuts them off (a wrapped cell stops wrapping).
        if ((heights.get(row) || 0) > px) {
          clipped.add(row);
          multiWrap.delete(row);
        } else if (multiWrap.has(row) && px < (heights.get(row) || 0)) {
          multiWrap.delete(row);
        }
        if (px === base) heights.delete(row);
        else heights.set(row, px);
      }
      const all = [...heights.keys()].sort((a, b) => a - b);
      const rows = [];
      const extra = [];
      const capRows = new Set();
      let acc = 0;
      let min = base;
      for (const row of all) {
        const h = heights.get(row);
        const next = acc + h - base;
        if (next > extraBudget) {
          heights.delete(row);
          multiWrap.delete(row);
          continue;
        }
        acc = next;
        min = Math.min(min, h);
        rows.push(row);
        extra.push(acc);
        if (clipped.has(row) || h < fontNeed || (wrapRows.has(row) && !multiWrap.has(row))) {
          capRows.add(row);
        }
      }
      const same = (a, b) => a.size === b.size && [...a].every(([k, v]) => b.get(k) === v);
      const sameSet = (a, b) => a.size === b.size && [...a].every((k) => b.has(k));
      const changed = wasFontDirty || base !== state.base || !same(heights, state.heights) || !sameSet(clipped, state.clipped) || !sameSet(capRows, state.capRows);
      Object.assign(state, { rows, extra, total: acc, heights, clipped, capRows, min, base });
      return changed;
    }

    // Sum of the extra heights (negative for a row under the grid's) of the rows above `row`.
    function extraBefore(row) {
      const { rows, extra } = stateOf();
      let lo = 0;
      let hi = rows.length;
      while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (rows[mid] < row) lo = mid + 1;
        else hi = mid;
      }
      return lo === 0 ? 0 : extra[lo - 1];
    }

    return {
      sync,
      extraBefore,
      extraTotal: () => stateOf().total,
      heightOf: (row) => stateOf().heights.get(row) || rowH(),
      isClipped: (row) => stateOf().clipped.has(row),
      isCapped: (row) => stateOf().capRows.has(row),
      // The shortest row: the window must hold enough rows to fill the viewport.
      minHeight: () => stateOf().min,
      any: () => stateOf().rows.length > 0,
      // Whether the last sync stopped at its budget: the rows it had not reached keep the plain height.
      overBudget: () => overBudget,
      baseTall: () => isDefaultFontTall(rowH()),
    };
  }

  // Number of lines `text` takes in `width` px, measured with `widthOf(text)` of the canvas whose font
  // is already set. Latin text breaks at spaces, a word wider than the cell and every other script
  // break anywhere (the cell's `overflow-wrap: anywhere`).
  function countLines(text, width, widthOf) {
    let lines = 0;
    for (const paragraph of text.split("\n")) {
      lines += paragraphLines(paragraph, width, widthOf);
    }
    return Math.max(1, lines);
  }

  function paragraphLines(text, width, widthOf) {
    if (text === "" || widthOf(text) <= width) return 1;
    let lines = 1;
    let line = "";
    for (const token of text.match(/[A-Za-z0-9_'’.,;:!?%()\-+/]+|\s+|[^\sA-Za-z0-9_'’.,;:!?%()\-+/]/gu) || []) {
      const next = line + token;
      if (line === "" || widthOf(next) <= width) {
        line = next;
        if (widthOf(line) > width) line = breakWide(line, width, widthOf, () => (lines += 1));
        continue;
      }
      lines += 1;
      line = /^\s+$/.test(token) ? "" : token;
      if (widthOf(line) > width) line = breakWide(line, width, widthOf, () => (lines += 1));
    }
    return lines;
  }

  // Splits a word wider than the line by characters; returns the last (partial) line.
  function breakWide(word, width, widthOf, newLine) {
    let line = "";
    for (const ch of word) {
      if (line !== "" && widthOf(line + ch) > width) {
        newLine();
        line = ch;
      } else line += ch;
    }
    return line;
  }

  host.__EASYOFFICE_CREATE_ROW_FIT__ = createRowFit;
  host.__EASYOFFICE_SHEET_LINE_COUNT__ = countLines;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
