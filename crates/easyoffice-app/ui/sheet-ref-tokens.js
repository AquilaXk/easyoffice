(function (host) {
  "use strict";

  // The one pattern that finds the references of a formula text (PD-225). The range clipboard shifts them on a paste and
  // a fill, and the point mode of the formula editor (`sheet-formula-point.js`) colours and moves them, so both read the
  // same tokens.
  //
  // One formula token is either a string literal (copied as is) or a reference: an optional sheet
  // prefix (`Name!` or a quoted name with '' escapes) followed by a cell, a cell range, a column
  // range or a row range, so a prefixed range moves as a unit. Names that merely look like a cell
  // (`A1B`, `LOG10(`, `my_name1`) are rejected by the boundary checks. Group 1 is the sheet prefix with its `!`, group 2
  // the reference; a literal and a structured reference leave group 2 undefined.
  const CELL = String.raw`\$?[A-Za-z]{1,3}\$?[1-9]\d{0,6}`;
  const COL_RANGE = String.raw`\$?[A-Za-z]{1,3}:\$?[A-Za-z]{1,3}`;
  const ROW_RANGE = String.raw`\$?[1-9]\d{0,6}:\$?[1-9]\d{0,6}`;
  // A bracket group (`Table1[[#This Row],[Q1]]`, `[@Q1]`) is a structured reference: it names columns, so a fill or a
  // paste leaves it as written (PD-225). The group nests up to the depth the engine reads (`'` escapes the next character).
  const GROUP_RE = String.raw`\[(?:[^\[\]']|'.|\[(?:[^\[\]']|'.|\[(?:[^\[\]']|'.|\[(?:[^\[\]']|'.)*\])*\])*\])*\]`;
  const TOKEN_RE = new RegExp(
    String.raw`"(?:[^"]|"")*"|` + GROUP_RE + String.raw`|(?<![\p{L}\p{N}_.$!])((?:'(?:[^']|'')+'|[\p{L}\p{N}_.]+)!)?` +
      `(${CELL}(?::${CELL})?|${COL_RANGE}|${ROW_RANGE})` +
      String.raw`(?![\p{L}\p{N}_(!])`,
    "gu"
  );

  host.__EASYOFFICE_SHEET_REF_TOKENS__ = { TOKEN_RE };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
