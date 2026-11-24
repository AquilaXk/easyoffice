// Column widths that follow the content: a double click on the edge of a column header fits that column, or every
// selected column when the pressed one is among them.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 4767b3b309467c283bbd52b725536ca73861e0fc):
//   packages/sheets-ui/src/controllers/render-controllers/header-resize.render-controller.ts :497-545
//       (the double click: the selected columns when the pressed column is one of them, else that column)
//   packages/engine-render/src/components/sheets/sheet.render-skeleton.ts :925-1036
//       (`calculateAutoWidthInRange`: a column once, hidden columns skipped; `_calculateColWidth`: merged
//       cells other than the anchor and hidden rows skipped, the widest cell wins, the widest allowed ends
//       the search, an empty column keeps its width)
//   packages/sheets-ui/src/controllers/auto-width.controller.ts :27-92 and
//   packages/sheets-ui/src/commands/commands/set-worksheet-auto-col-width.command.ts :33-86
//       (the new widths and the widths they replace are one step)
// Modifications: the text is measured on the canvas the grid already measures wrapped rows with (`measure`) instead of
// a document skeleton; the cells are visited in one pass over the sheet's cell map instead of one lookup per row and
// column; a cell is as wide as its longest line whether it wraps or not, and an empty column goes back to the default
// width, as the spreadsheet programs do (the reference measures a wrapped cell at the width it has and leaves an empty column);
// a number shows in its format, asked of the engine for the cells that have one; the drop-down button of a
// filter header cell adds its width.
(function (host) {
  "use strict";

  const PADDING_PX = 13; // 6px each side and the border, as the grid's cell
  const BUTTON_PX = 18; // the drop-down button of a filter header cell
  const GLYPH_EM = 1.5; // no glyph is wider than this many em: a text this short cannot beat the widest so far
  const FALLBACK_FONT_PX = 14.67; // Normal font (Malgun Gothic 11 pt) when `sheet-default-font.js` is absent
  const FALLBACK_FONT_FAMILY = "'맑은 고딕', 'Malgun Gothic', 'Apple SD Gothic Neo', 'Noto Sans KR', sans-serif";
  const MEMO_LIMIT = 50000;
  const MAX_MEASURES = 200000; // canvas measurements one fit may make
  const BUDGET_MS = 1500; // and the time it may take
  const CLOCK_EVERY = 256; // measurements between two looks at the clock

  function createColFit(deps) {
    const { getMap, packRef, measure: measureText, isColHidden, isRowHidden, mergeAt, headerCells, formatGeneral, defaultWidth, minWidth, maxWidth } = deps;
    const dfApi = deps.defaultFont || host.__EASYOFFICE_SHEET_DEFAULT_FONT__;
    const baseFontPx = () => (typeof deps.baseFontPx === "function" ? deps.baseFontPx() : dfApi?.baseFontPx?.() ?? FALLBACK_FONT_PX);
    const baseFontFamily = () => (typeof deps.baseFontFamily === "function" ? deps.baseFontFamily() : dfApi?.baseFontFamily?.() ?? FALLBACK_FONT_FAMILY);
    const fontVersion = () => (typeof deps.fontVersion === "function" ? deps.fontVersion() : dfApi?.version?.() ?? 0);
    let measures = 0;
    let started = 0;
    let overBudget = false;
    const now = deps.now || (() => host.performance?.now?.() ?? Date.now());
    // One canvas measurement, counted; null when the page cannot measure, and the budget ends the fit by setting `overBudget`.
    function measure(font, family, text) {
      measures += 1;
      if (measures > MAX_MEASURES || (measures % CLOCK_EVERY === 0 && now() - started > BUDGET_MS)) {
        overBudget = true;
        return null;
      }
      return measureText(font, family, text);
    }

    // Widths by font and then by text: a sheet repeats its values, so a text is measured once per font.
    const memo = new Map();
    let memoSize = 0;
    // A style is shared by every cell that wears it (the grid interns them): its font is worked out once.
    let fonts = new WeakMap();
    let cachedFontSig = fontVersion() + "\u0000" + baseFontPx() + "\u0000" + baseFontFamily();
    function syncFontCache() {
      const sig = fontVersion() + "\u0000" + baseFontPx() + "\u0000" + baseFontFamily();
      if (sig === cachedFontSig) return;
      cachedFontSig = sig;
      memo.clear();
      memoSize = 0;
      fonts = new WeakMap();
    }
    const fontOf = (st) => {
      syncFontCache();
      const dpx = baseFontPx();
      const dfam = baseFontFamily();
      if (!st) return `normal 400 ${dpx}px\u0000${dfam}`;
      let key = fonts.get(st);
      if (key === undefined) {
        key = `${st.fontStyle || "normal"} ${st.fontWeight || "400"} ${st.fontSize || dpx + "px"}\u0000${st.fontFamily || dfam}`;
        fonts.set(st, key);
      }
      return key;
    };

    // Width of the longest line of `text` in the font `key` (`fontOf`); null when the page cannot measure.
    function lineWidth(key, text) {
      let texts = memo.get(key);
      if (!texts) memo.set(key, (texts = new Map()));
      let w = texts.get(text);
      if (w === undefined) {
        const [font, family] = key.split("\u0000");
        w = 0;
        for (const line of text.split("\n")) {
          const lw = measure(font, family, line);
          if (lw === null) return null;
          if (lw > w) w = lw;
        }
        if (memoSize >= MEMO_LIMIT) {
          memo.clear();
          memoSize = 0;
          memo.set(key, (texts = new Map()));
        }
        texts.set(text, w);
        memoSize += 1;
      }
      return w;
    }

    // The text a cell shows: its format's text when the engine answered, else the General text.
    function shownOf(entry, formatted) {
      const raw = String(entry.value ?? "");
      const nf = entry.style?.numFmt;
      const textTyped = entry.t === "s" || nf === "@";
      const hit = nf && !textTyped ? formatted.get(nf)?.get(raw) : null;
      return hit ? hit.t : textTyped ? raw : formatGeneral(raw);
    }

    // The values per format code the cells of `cols` hold, for one question to the engine.
    function wantedFormats(map, colSet) {
      const wanted = new Map();
      for (const ref in map) {
        const entry = map[ref];
        const nf = entry.style?.numFmt;
        if (!nf || nf === "@" || entry.t === "s" || entry.value === "" || entry.value === undefined) continue;
        const packed = packRef(ref);
        if (packed <= 0 || !colSet.has(packed & 32767)) continue;
        let raws = wanted.get(nf);
        if (!raws) wanted.set(nf, (raws = new Set()));
        raws.add(String(entry.value));
      }
      return wanted;
    }

    /**
     * The widths that fit the content of `cols` (1-based columns): `{ widths: [{ col, width }] }`, a column once, hidden columns
     * left out. `resolveFormats(wanted)` answers the shown text of formatted values ({@link Map} code -> Map(value -> { t })).
     * `{ refused: "measure" }` when the page cannot measure, `{ refused: "budget" }` when the fit needs more than 200,000
     * measurements or 1.5 s: no width is changed from a half-done fit.
     */
    async function widthsFor(cols, resolveFormats) {
      syncFontCache();
      const colSet = new Set(cols.filter((c) => !isColHidden(c)));
      if (colSet.size === 0) return { widths: [] };
      measures = 0;
      overBudget = false;
      started = now();
      const map = getMap() || {};
      const wanted = resolveFormats ? wantedFormats(map, colSet) : new Map();
      const formatted = wanted.size > 0 ? await resolveFormats(wanted) : new Map();
      const best = new Map();
      const full = maxWidth - PADDING_PX;
      const defaultPx = baseFontPx();
      for (const ref in map) {
        const entry = map[ref];
        const text = shownOf(entry, formatted);
        if (text === "") continue;
        const packed = packRef(ref);
        if (packed <= 0) continue;
        const col = packed & 32767;
        if (!colSet.has(col)) continue;
        const row = (packed - col) / 32768;
        // The spreadsheet programs leave merged cells out of the fit, and the rows nobody sees.
        if (mergeAt(row, col) || isRowHidden(row)) continue;
        const widest = best.get(col) || 0;
        if (widest >= full) continue;
        const extra = headerCells(row, col) ? BUTTON_PX : 0;
        const px = Number.parseFloat(entry.style?.fontSize) || defaultPx;
        if (text.length * px * GLYPH_EM + extra <= widest) continue;
        const w = lineWidth(fontOf(entry.style), text);
        if (w === null) return { refused: overBudget ? "budget" : "measure" };
        if (w + extra > widest) best.set(col, w + extra);
      }
      return {
        widths: [...colSet].map((col) => {
          const text = best.get(col);
          return { col, width: text === undefined ? defaultWidth : Math.max(minWidth, Math.min(maxWidth, Math.ceil(text + PADDING_PX))) };
        }),
      };
    }

    return { widthsFor };
  }

  /**
   * The columns a double click on the edge of column `pressed` fits: the selected columns when it is one of them,
   * else only it. `selection` is `{ axis, c1, c2 }` as the grid's selection range is.
   */
  function columnsToFit(pressed, selection) {
    if (selection?.axis === "col" && pressed >= selection.c1 && pressed <= selection.c2) {
      const cols = [];
      for (let c = selection.c1; c <= selection.c2; c += 1) cols.push(c);
      return cols;
    }
    return [pressed];
  }

  host.__EASYOFFICE_CREATE_SHEET_COL_FIT__ = createColFit;
  host.__EASYOFFICE_SHEET_COL_FIT__ = Object.freeze({ columnsToFit, PADDING_PX, BUTTON_PX });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
