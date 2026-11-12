(function (host) {
  "use strict";

  // Row heights that follow wrapped text (PD-207). A row with a wrapped cell is as tall as its tallest
  // wrapped cell; every other row keeps the grid's row height. Heights are derived from the sheet
  // model (never stored), so undo, file open and structure edits only have to call `sync()`.
  //
  // One pass over the sheet's cells finds the wrapped ones (a cell without the key is a property
  // read); a wrapped cell costs one canvas measurement per distinct (font, width, text), remembered
  // in `lineCache`. The rows that are taller than `rowH` are kept as a sorted list with running sums,
  // so the top of a row is one binary search and a scroll position finds its row the same way.
  const MAX_ROW_PX = 545; // 409 pt, the tallest row a spreadsheet file holds
  const BASE_FONT_PX = 13;
  const CACHE_LIMIT = 50000;
  const MIN_CELL_PX = 12; // a column narrower than this holds no text

  function createRowFit(deps) {
    const { rowH, getSheetName, getMap, columnWidth, isRowHidden, isMerged, measure, packRef } = deps;
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const lineCache = new Map();
    const bySheet = Object.create(null);

    const emptyState = () => ({ rows: [], extra: [], total: 0, sig: "" });
    const stateOf = () => bySheet[getSheetName()] || (bySheet[getSheetName()] = emptyState());

    // Lines `text` takes in `width` px of `font` (an explicit line break starts a new line); `measure` answers null while the page cannot measure.
    function linesOf(text, width, font) {
      const key = font + "|" + width + "|" + text;
      let n = lineCache.get(key);
      if (n === undefined) {
        n = measure(text, width, font);
        // An unmeasurable cell (no canvas yet) is one line and is asked again at the next sync.
        if (n === null) return 1;
        if (lineCache.size >= CACHE_LIMIT) lineCache.clear();
        lineCache.set(key, n);
      }
      return n;
    }

    function fontOf(st) {
      return (st.fontStyle || "normal") + " " + (st.fontWeight || "400") + " " + (st.fontSize || BASE_FONT_PX + "px");
    }

    function cellHeight(st, text, width) {
      const px = Number.parseFloat(st.fontSize) || BASE_FONT_PX;
      const lineH = Math.round(px * look.LINE_HEIGHT_RATIO);
      const lines = linesOf(text, width, fontOf(st));
      // One line fits the plain row, as any unwrapped cell does whatever its font size.
      if (lines <= 1) return 0;
      return Math.min(MAX_ROW_PX, lines * lineH + 2 * look.WRAP_PAD_V + 1);
    }

    // Recomputes the heights of the active sheet; true when any row got another height.
    function sync() {
      const map = getMap();
      const state = stateOf();
      const heights = new Map();
      if (map && measure) {
        for (const ref in map) {
          const entry = map[ref];
          const st = entry.style;
          if (!st || !st.wrap) continue;
          const text = String(entry.value ?? "");
          if (text === "") continue;
          const packed = packRef(ref);
          if (packed <= 0) continue;
          const col = packed & 32767;
          const row = (packed - col) / 32768;
          if (isRowHidden(row) || isMerged(row, col)) continue;
          const width = columnWidth(col) - 13;
          if (width < MIN_CELL_PX) continue;
          const h = cellHeight(st, text, width);
          if (h > rowH && h > (heights.get(row) || 0)) heights.set(row, h);
        }
      }
      const rows = [...heights.keys()].sort((a, b) => a - b);
      const extra = new Array(rows.length);
      let acc = 0;
      for (let i = 0; i < rows.length; i += 1) {
        acc += heights.get(rows[i]) - rowH;
        extra[i] = acc;
      }
      const sig = rows.map((r) => r + ":" + heights.get(r)).join(",");
      const changed = sig !== state.sig;
      Object.assign(state, { rows, extra, total: acc, sig, heights });
      return changed;
    }

    // Sum of the extra heights of the rows above `row`.
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
      heightOf: (row) => stateOf().heights?.get(row) || rowH,
      any: () => stateOf().rows.length > 0,
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
