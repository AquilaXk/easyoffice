(function (host) {
  "use strict";

  // Cell addressing of the sheet grid: A1 references, column letters and the packed
  // `row * 32768 + col` coordinate (the sheet is 1,048,576 rows by 16,384 columns).
  const MAX_ROWS = 1048576;
  const MAX_COLS = 16384;

  const COL_NAMES = new Array(257);
  COL_NAMES[0] = "";
  for (let i = 1; i <= 256; i += 1) {
    let n = i;
    let out = "";
    while (n > 0) {
      const rem = (n - 1) % 26;
      out = String.fromCodePoint(65 + rem) + out;
      n = Math.floor((n - 1) / 26);
    }
    COL_NAMES[i] = out;
  }

  function colName(idx) {
    if (idx >= 1 && idx <= 256) return COL_NAMES[idx];
    let n = idx;
    let out = "";
    while (n > 0) {
      const rem = (n - 1) % 26;
      out = String.fromCodePoint(65 + rem) + out;
      n = Math.floor((n - 1) / 26);
    }
    return out;
  }

  function colIndex(letters) {
    let n = 0;
    for (let i = 0; i < letters.length; i += 1) {
      n = n * 26 + (letters.charCodeAt(i) - 64);
    }
    return n;
  }

  const PACK_STRIDE = 32768;

  function packCoord(row, col) {
    return row * PACK_STRIDE + col;
  }

  function packRef(ref) {
    if (typeof ref !== "string") return 0;
    const len = ref.length;
    if (len < 2 || len > 12) return 0;
    let i = 0;
    while (i < len && ref.charCodeAt(i) <= 32) i += 1;
    let end = len;
    while (end > i && ref.charCodeAt(end - 1) <= 32) end -= 1;
    if (i < end && ref.charCodeAt(i) === 36) i += 1;
    let col = 0;
    let colLen = 0;
    while (i < end) {
      const ch = ref.charCodeAt(i);
      const upper = ch >= 97 && ch <= 122 ? ch - 32 : ch;
      if (upper < 65 || upper > 90) break;
      col = col * 26 + (upper - 64);
      colLen += 1;
      if (colLen > 3) return 0;
      i += 1;
    }
    if (colLen === 0 || col < 1 || col > MAX_COLS) return 0;
    if (i < end && ref.charCodeAt(i) === 36) i += 1;
    if (i >= end) return 0;
    const firstDigit = ref.charCodeAt(i);
    if (firstDigit < 49 || firstDigit > 57) return 0;
    let row = 0;
    let rowLen = 0;
    while (i < end) {
      const ch = ref.charCodeAt(i);
      if (ch < 48 || ch > 57) return 0;
      row = row * 10 + (ch - 48);
      rowLen += 1;
      if (rowLen > 7) return 0;
      i += 1;
    }
    if (row < 1 || row > MAX_ROWS) return 0;
    return row * PACK_STRIDE + col;
  }

  function parseRef(ref) {
    if (typeof ref !== "string") return null;
    const len = ref.length;
    if (len < 2 || len > 12) return null;
    let i = 0;
    while (i < len && ref.charCodeAt(i) <= 32) i += 1;
    let end = len;
    while (end > i && ref.charCodeAt(end - 1) <= 32) end -= 1;
    let col = 0;
    let colLen = 0;
    while (i < end) {
      const ch = ref.charCodeAt(i);
      const upper = ch >= 97 && ch <= 122 ? ch - 32 : ch;
      if (upper < 65 || upper > 90) break;
      col = col * 26 + (upper - 64);
      colLen += 1;
      if (colLen > 3) return null;
      i += 1;
    }
    if (colLen === 0 || col < 1 || col > MAX_COLS || i >= end) return null;
    const firstDigit = ref.charCodeAt(i);
    if (firstDigit < 49 || firstDigit > 57) return null;
    let row = 0;
    let rowLen = 0;
    while (i < end) {
      const ch = ref.charCodeAt(i);
      if (ch < 48 || ch > 57) return null;
      row = row * 10 + (ch - 48);
      rowLen += 1;
      if (rowLen > 7) return null;
      i += 1;
    }
    if (row < 1 || row > MAX_ROWS) return null;
    return { row, col };
  }

  function refOf(row, col) {
    return colName(col) + row;
  }

  host.__EASYOFFICE_SHEET_REFS__ = {
    MAX_ROWS,
    MAX_COLS,
    PACK_STRIDE,
    colName,
    colIndex,
    packCoord,
    packRef,
    parseRef,
    refOf,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
