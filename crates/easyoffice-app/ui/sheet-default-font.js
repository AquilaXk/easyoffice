// The default font of the open workbook (PD-306), the font of the Normal cell style. Its widest digit is the unit of the column
// widths in characters, which only the engine knows (`sheet_digit_width.rs`), so the page keeps the font as the engine told it
// and sends it back with every save and every row or column edit; it works out no width itself.
//
// A new workbook, a CSV, a TSV and an ODS without a font of its own have the standard font of the Korean edition of the
// spreadsheet program (Malgun Gothic 11 pt, 72 px columns), which is no value here (`null`): the engine says the standard font
// without being told. An XLSX with another Normal font (Calibri 11, Arial 12, ...) gives `{name, size}` and `colPx`, the width of
// a column in a sheet that says nothing, which the page needs for the sheets it adds itself.
(function (host) {
  "use strict";

  const STANDARD_COL_PX = 72; // `DEFAULT_COL_PX` of the engine: 8 characters of the 8 px digit of Malgun Gothic 11 pt
  const MIN_COL_PX = 8, MAX_COL_PX = 2000; // `MIN_COLUMN_PX`, `MAX_COLUMN_PX`
  const MAX_NAME_CHARS = 64, MIN_SIZE_PT = 1, MAX_SIZE_PT = 409; // the bounds of every font name and size (`sheet_fonts.rs`)
  const STANDARD_FONT_NAME = "맑은 고딕";
  const STANDARD_FONT_SIZE_PT = 11;
  const PT_TO_PX = 96 / 72;
  const STANDARD_FONT_STACK = "'맑은 고딕', 'Malgun Gothic', 'Apple SD Gothic Neo', 'Noto Sans KR', sans-serif";
  const GENERIC_FAMILIES = new Set(["serif", "sans-serif", "monospace", "cursive", "fantasy", "system-ui"]);

  let font = null; // {name, size} or null (the standard font)
  let colPx = STANDARD_COL_PX;
  let ver = 0;

  // The bounds of `clean_font_name`: 1 to 64 characters without control characters or the CSS and markup delimiters.
  const validName = (name) => typeof name === "string" && name.length > 0 && [...name].length <= MAX_NAME_CHARS
    && !/[\u0000-\u001f\u007f-\u009f"'\\;{}<>`]/.test(name) && name.trim() === name;
  const validSize = (size) => typeof size === "number" && Number.isFinite(size) && size >= MIN_SIZE_PT && size <= MAX_SIZE_PT;
  const ptToPx = (pt) => Math.round(pt * PT_TO_PX * 100) / 100;
  const STANDARD_FONT_PX = ptToPx(STANDARD_FONT_SIZE_PT);

  function quoteCssFont(name) {
    if (!validName(name)) return "";
    return GENERIC_FAMILIES.has(name.toLowerCase()) ? name : "'" + name + "'";
  }

  function baseFontPt() {
    return font ? font.size : STANDARD_FONT_SIZE_PT;
  }

  function baseFontPx() {
    return ptToPx(baseFontPt());
  }

  function baseFontFamily() {
    if (!font || font.name === STANDARD_FONT_NAME) return STANDARD_FONT_STACK;
    const quoted = quoteCssFont(font.name);
    return quoted ? quoted + ", " + STANDARD_FONT_STACK : STANDARD_FONT_STACK;
  }

  function cssFontSize() {
    return baseFontPt() + "pt";
  }

  function applyCss(doc = host.document) {
    const root = doc?.documentElement;
    if (!root?.style) return;
    if (font) {
      root.style.setProperty("--sheet-cell-font-family", baseFontFamily());
      root.style.setProperty("--sheet-cell-font-size", cssFontSize());
    } else {
      root.style.removeProperty("--sheet-cell-font-family");
      root.style.removeProperty("--sheet-cell-font-size");
    }
  }

  // Takes the engine's `defaultFont` and `defaultColPx` of an opened file; anything that is not a bounded font is the standard font.
  function set(next, px, doc = host.document) {
    const prevKey = font ? font.name + "\u0000" + font.size : "";
    const ok = next && typeof next === "object" && validName(next.name) && validSize(next.size);
    font = ok ? { name: next.name, size: next.size } : null;
    colPx = ok && Number.isInteger(px) && px >= MIN_COL_PX && px <= MAX_COL_PX ? px : STANDARD_COL_PX;
    const nextKey = font ? font.name + "\u0000" + font.size : "";
    if (nextKey !== prevKey) ver += 1;
    applyCss(doc);
  }

  host.__EASYOFFICE_SHEET_DEFAULT_FONT__ = {
    set,
    // What a snapshot keeps (`set` takes the font and the width back): null for the standard font.
    get: () => (font ? { ...font, colPx } : null),
    restore: (snap, doc = host.document) => set(snap, snap?.colPx, doc),
    // The width of a column in a sheet that says nothing.
    colPx: () => colPx,
    // The request field: nothing for the standard font.
    field: () => (font ? { defaultFont: { name: font.name, size: font.size } } : {}),
    baseFontPt,
    baseFontPx,
    baseFontFamily,
    cssFontSize,
    quoteCssFont,
    applyCss,
    version: () => ver,
    STANDARD_COL_PX,
    STANDARD_FONT_NAME,
    STANDARD_FONT_SIZE_PT,
    STANDARD_FONT_PX,
    STANDARD_FONT_STACK,
    MIN_SIZE_PT,
    MAX_SIZE_PT,
    PT_TO_PX,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
