// Point mode of the formula editor, the pure part: where in a formula text a cell may be pointed at, what reference a
// click or an arrow puts there, which colour each reference of a formula wears, and the F4 cycle of `$`.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 4e2f00ca4de29f7c65c43cefc469acc06cdc5ffa):
//   packages/engine-formula/src/formula-editor-helpers.ts :158-172 (`isFormulaReferenceAddingContext`,
//       `isFormulaReferenceAddingTextContext`), :174-205 (`resolveFormulaReferenceEditingContext`), :218-247 (the colour
//       of a reference in `buildFormulaTextRuns`)
//   packages/engine-formula/src/basics/match-token.ts :52-61 (`matchRefDrawToken`)
//   packages/sheets-formula-ui/src/commands/utils/reference-absolute.ts :37-240 (`toggleReferenceAbsoluteAtCursor` and its
//       helpers: the F4 cycle, the side of a range under the caret, the caret after the change)
//   packages/sheets-formula-ui/src/views/formula-editor/hooks/use-sheet-selection-change.ts :80-82
//       (`insertFormulaReferenceText`)
// Modifications: the reference reads a formula through its lexer (`sequenceNodesBuilder`) and an editor document; here the
// references come from the pattern the range clipboard already uses (`sheet-ref-tokens.js`), so the functions work on the
// plain text and a caret offset of the text, `=` included. A reference only follows `=`, an operator, `(` and `,`
// (spaces between are skipped), never the `%`, `#` and `@` the reference's `matchRefDrawToken` accepts, and never inside a
// text literal or a quoted sheet name. A selected text is replaced by the reference. The cycle covers the cell, range, column
// and row forms of the pattern and writes letters in upper case.
//
// Structured references (PD-296): neither the reference above nor IronCalc has a point that writes a table's column (IronCalc
// turns a structured reference into a range while it parses and never writes one; the reference only rewrites the
// `[@Column]` of a calculated column, Univer `sheets-table/src/commands/utils/calculated-column.ts` lines 89-140, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit ac29fd8aaa3ce43cd1f672e75a596da22c132616; IronCalc `base/src/expressions/
// parser/stringify.rs`, MIT OR Apache-2.0, commit f93662006b8fb1c401dc870bbda26c5d60ac6037; both read, no code taken), so the text a
// pointed range of a table becomes follows the spreadsheet program's help page for tables ("Use table names in formulas",
// on by default): the whole data of a column is `Table[Column]`, several columns `Table[[A]:[B]]`, a cell in the formula's
// own row `[@Column]` in the table and `Table[[#This Row],[Column]]` outside it, and anything else (part of a column, the
// header, the totals row) stays an A1 range. The spelling of a column name is that of `sheet-table-totals.js` (`columnRef`),
// which the formula bar and the totals row use; the engine reads the text back through `structured_ref.rs`.
(function (host) {
  "use strict";

  const { TOKEN_RE } = host.__EASYOFFICE_SHEET_REF_TOKENS__;
  const refsApi = () => host.__EASYOFFICE_SHEET_REFS__;
  const totalsApi = () => host.__EASYOFFICE_SHEET_TABLE_TOTALS__;
  // The characters of a table's name in front of its bracket group.
  const TABLE_NAME_CHAR = /[\p{L}\p{N}_.]/u;

  // A reference is pointed after `=`, an operator, `(` or `,`.
  const POINT_AFTER = "=+-*/^&<>(,";
  // It may stand before an operator, a closing bracket, a separator or a space, not before a name, a number or `(`.
  const POINT_BEFORE = "=<>+-*/&^),:%{}[]#@ ";
  const NONE = Object.freeze({ mode: "none", start: 0, end: 0, span: null });

  // The `"` that opens a text literal the text never closes, else -1.
  function openQuoteAt(text) {
    let open = -1;
    for (let i = 0; i < text.length; i += 1) if (text[i] === '"') open = open < 0 ? i : -1;
    return open;
  }

  // The references of a formula text as `{ start, end, token, sheet, ref }` (`end` is exclusive, `sheet` the prefix with its
  // `!`). A text that is not a formula has none, nor has a literal or a structured reference.
  function refSpans(text) {
    const spans = [];
    if (!text.startsWith("=")) return spans;
    const open = openQuoteAt(text);
    for (const m of text.matchAll(TOKEN_RE)) {
      if (m[2] === undefined || (open >= 0 && m.index > open)) continue;
      spans.push({ start: m.index, end: m.index + m[0].length, token: m[0], sheet: m[1] || "", ref: m[2] });
    }
    return spans;
  }

  // The structured references of a formula text as `{ start, end, token, sheet: "", ref: null }`: a bracket group and the
  // table's name in front of it (`Table1[Q1]`, `[@Q1]`). A text that is not a formula has none, nor has a literal.
  function structuredSpans(text) {
    const spans = [];
    if (!text.startsWith("=")) return spans;
    const open = openQuoteAt(text);
    for (const m of text.matchAll(TOKEN_RE)) {
      if (m[2] !== undefined || m[0][0] !== "[" || (open >= 0 && m.index > open)) continue;
      let start = m.index;
      while (start > 0 && TABLE_NAME_CHAR.test(text[start - 1])) start -= 1;
      const end = m.index + m[0].length;
      spans.push({ start, end, token: text.slice(start, end), sheet: "", ref: null });
    }
    return spans;
  }

  // Every reference that gets a colour and a box, in text order: the cells and the structured references.
  const colorSpans = (text) => [...refSpans(text), ...structuredSpans(text)].sort((a, b) => a.start - b.start);

  // Whether `caret` stands inside a text literal or a quoted sheet name.
  function insideQuotes(text, caret) {
    let double = false;
    let single = false;
    for (let i = 0; i < caret; i += 1) {
      if (text[i] === '"' && !single) double = !double;
      else if (text[i] === "'" && !double) single = !single;
    }
    return double || single;
  }

  // Ref `isFormulaReferenceAddingTextContext`: the character before the caret asks for an operand and the one after it is
  // not the start of one.
  function canPointAt(text, from, to) {
    let i = from - 1;
    while (i > 0 && text[i] === " ") i -= 1;
    if (i < 0 || !POINT_AFTER.includes(text[i]) || insideQuotes(text, from)) return false;
    return to >= text.length || POINT_BEFORE.includes(text[to]);
  }

  // Ref `resolveFormulaReferenceEditingContext`: what a pointed reference does at `from`..`to` (a caret, or a selection):
  // `replace` the reference the caret is in or at the end of (or the selection touches), `add` at the caret, or `none`.
  // `start`..`end` is the text the reference takes the place of.
  function pointContext(text, from, to = from) {
    if (!text.startsWith("=")) return NONE;
    const spans = colorSpans(text);
    const hit = to > from ? spans.find((s) => s.start < to && s.end > from) : spans.find((s) => from > s.start && from <= s.end);
    if (hit) return { mode: "replace", start: hit.start, end: hit.end, span: hit };
    return canPointAt(text, from, to) ? { mode: "add", start: from, end: to, span: null } : NONE;
  }

  // Ref `insertFormulaReferenceText`, also for a replaced text: `ctx.start..ctx.end` becomes `refText`, the caret goes after it.
  function applyRef(text, ctx, refText) {
    const end = ctx.start + refText.length;
    return { text: text.slice(0, ctx.start) + refText + text.slice(ctx.end), start: ctx.start, end, caret: end };
  }

  // The reference a pointed range is written as: `B2`, `B2:D4`, and `B:D` or `2:4` for whole columns or rows.
  function rangeText(rg, axis) {
    const { colName } = refsApi();
    if (axis === "col") return colName(rg.c1) + ":" + colName(rg.c2);
    if (axis === "row") return rg.r1 + ":" + rg.r2;
    const first = colName(rg.c1) + rg.r1;
    return rg.r1 === rg.r2 && rg.c1 === rg.c2 ? first : first + ":" + colName(rg.c2) + rg.r2;
  }

  // The cells a reference text covers, `{ r1, c1, r2, c2, axis }` (axis: "col" or "row" for a whole column or row), or null.
  function rangeOf(ref) {
    const { parseRef, colIndex, MAX_ROWS, MAX_COLS } = refsApi();
    const parts = ref.replaceAll("$", "").toUpperCase().split(":");
    const order = (a, b) => [Math.min(a, b), Math.max(a, b)];
    if (parts.length === 2 && /^[A-Z]+$/.test(parts[0]) && /^[A-Z]+$/.test(parts[1])) {
      const [c1, c2] = order(colIndex(parts[0]), colIndex(parts[1]));
      return { r1: 1, c1, r2: MAX_ROWS, c2, axis: "col" };
    }
    if (parts.length === 2 && /^\d+$/.test(parts[0]) && /^\d+$/.test(parts[1])) {
      const [r1, r2] = order(Number(parts[0]), Number(parts[1]));
      return { r1, c1: 1, r2, c2: MAX_COLS, axis: "row" };
    }
    const a = parseRef(parts[0]);
    const b = parts.length === 2 ? parseRef(parts[1]) : a;
    if (!a || !b) return null;
    const [r1, r2] = order(a.row, b.row);
    const [c1, c2] = order(a.col, b.col);
    return { r1, c1, r2, c2, axis: null };
  }

  // The sheet a prefix (`Sheet2!`, `'My Sheet'!`) names.
  function sheetOf(prefix) {
    const name = prefix.slice(0, -1);
    return name.startsWith("'") ? name.slice(1, -1).replaceAll("''", "'") : name;
  }

  // The structured reference a pointed range `rg` (`{ r1, c1, r2, c2, axis }`) of the table `table` (`{ name, columns, header,
  // totals }`, its cells `box`) is written as, or null for an A1 range. `at` is the cell of the formula `{ r, c }` when it is on
  // the table's sheet, else null; a cell in its row is the table's `#This Row`.
  function structuredRef(rg, table, box, at) {
    const first = box.r1 + (table.header === false ? 0 : 1);
    const last = box.r2 - (table.totals === true ? 1 : 0);
    if (rg.axis || rg.r1 < first || rg.r2 > last || rg.c1 < box.c1 || rg.c2 > box.c2) return null;
    const from = table.columns[rg.c1 - box.c1];
    const to = table.columns[rg.c2 - box.c1];
    if (!from || !to) return null;
    const { escapeColumn, columnRef } = totalsApi();
    const one = rg.c1 === rg.c2;
    const columns = "[" + escapeColumn(from) + "]" + (one ? "" : ":[" + escapeColumn(to) + "]");
    if (rg.r1 === rg.r2 && at && at.r === rg.r1) {
      if (at.c < box.c1 || at.c > box.c2) return table.name + "[[#This Row]," + columns + "]";
      // The spreadsheet program writes a name with a blank in double brackets here (`[@[Sales Amount]]`).
      const plain = one && !/\s/.test(from) && !columnRef("", from).startsWith("[[");
      return "[@" + (plain ? escapeColumn(from) : columns) + "]";
    }
    if (rg.r1 !== first || rg.r2 !== last) return null;
    return one ? columnRef(table.name, from) : table.name + "[" + columns + "]";
  }

  // Ref `buildFormulaTextRuns`: references get the colours of a palette of `count` in the order they first appear, and the
  // same text keeps its colour. Gives the colour index of each span.
  function colorIndexes(spans, count) {
    const seen = new Map();
    return spans.map((s) => {
      if (!seen.has(s.token)) seen.set(s.token, seen.size % count);
      return seen.get(s.token);
    });
  }

  // ---- F4 ------------------------------------------------------------------------------------------------------------
  // Ref `_getNextAbsoluteRefType`: A1 -> $A$1 -> A$1 -> $A1 -> A1, keyed by (column fixed, row fixed).
  const NEXT_FIXING = { "00": "11", "11": "01", "01": "10", "10": "00" };
  const CELL_END_RE = /^(\$?)([A-Za-z]{1,3})(\$?)([1-9]\d{0,6})$/;
  const AXIS_END_RE = /^\$?(?:[A-Za-z]{1,3}|[1-9]\d{0,6})$/;

  function cycleEnd(end) {
    const [, col, letters, row, digits] = CELL_END_RE.exec(end);
    const next = NEXT_FIXING[(col ? "1" : "0") + (row ? "1" : "0")];
    return (next[0] === "1" ? "$" : "") + letters.toUpperCase() + (next[1] === "1" ? "$" : "") + digits;
  }

  const upperEnd = (end) => end.toUpperCase();

  // Ref `_getRangeReferenceTarget`: which end of a range the caret is on; next to the colon it is both.
  function rangeTarget(prefixLength, body, offsetInToken) {
    const colon = prefixLength + body.indexOf(":");
    if (offsetInToken === colon || offsetInToken === colon + 1) return "both";
    return offsetInToken < colon ? "start" : "end";
  }

  // Ref `_toggleReferenceToken`: the reference with the next fixing, and where its colon is before and after.
  function toggleToken(span, offsetInToken) {
    const parts = span.ref.split(":");
    if (parts.length === 2 && parts.every((p) => AXIS_END_RE.test(p))) {
      // A column or a row range is either free or fixed on both ends.
      const fixed = parts[0].startsWith("$");
      const ends = parts.map((p) => (fixed ? "" : "$") + upperEnd(p.replace("$", "")));
      return { token: span.sheet + ends.join(":"), target: "both", before: null, after: null };
    }
    if (!parts.every((p) => CELL_END_RE.test(p))) return null;
    const target = parts.length === 1 ? "start" : rangeTarget(span.sheet.length, span.ref, offsetInToken);
    const ends = parts.map((p, i) => {
      const turn = target === "both" || (target === "start" && i === 0) || (target === "end" && i === 1);
      return turn ? cycleEnd(p) : upperEnd(p);
    });
    const token = span.sheet + ends.join(":");
    const colon = parts.length === 2;
    return { token, target, before: colon ? span.token.indexOf(":") : null, after: colon ? token.indexOf(":") : null };
  }

  // Ref `_translateCursorOffset`: the caret follows the text it was in.
  function carriedCaret(caret, span, changed) {
    if (changed.target === "both" && changed.before !== null && changed.after !== null) {
      if (caret === span.start + changed.before) return span.start + changed.after;
      if (caret === span.start + changed.before + 1) return span.start + changed.after + 1;
    }
    if (caret <= span.start) return caret;
    const shift = changed.token.length - (span.end - span.start);
    return caret >= span.end ? caret + shift : span.start + Math.min(caret - span.start, changed.token.length);
  }

  // Ref `toggleReferenceAbsoluteAtCursor`: the formula with the next `$` form of the reference the caret is in or at the end
  // of, as `{ text, start, end, caret }`, or null when the caret is on none.
  function cycleRef(text, caret) {
    if (!text.startsWith("=") || text.length <= 1) return null;
    const at = Math.min(Math.max(caret, 0), text.length);
    const span = refSpans(text).find((s) => at >= s.start && at <= s.end);
    const changed = span ? toggleToken(span, at - span.start) : null;
    if (!changed || changed.token === span.token) return null;
    return {
      text: text.slice(0, span.start) + changed.token + text.slice(span.end),
      start: span.start,
      end: span.start + changed.token.length,
      caret: carriedCaret(at, span, changed),
    };
  }

  host.__EASYOFFICE_SHEET_FORMULA_POINT__ = { refSpans, colorSpans, pointContext, applyRef, rangeText, rangeOf, sheetOf, structuredRef, colorIndexes, cycleRef };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
