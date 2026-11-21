// Reference implementation (ported, safe JS): Univer `sheets-ui` selection movement, Apache License 2.0,
// Copyright 2023-present DreamNum Co., Ltd. (https://github.com/dream-num/univer, commit
// eaf4cfa04fabdbcfc922c692f3450459f8a29546):
//   packages/sheets-ui/src/commands/commands/utils/selection-utils.ts lines 36-135 (`findNextRange`: the next visible
//   row or column, a boundary range and the wrap to the next line), 137-326 (`findNextGapRange`: the walk of
//   Ctrl+Arrow over a run of values and across a gap), 423-465 (`getEdgeOfRange`), 470-490 (`rangeHasValue`);
//   packages/sheets-ui/src/commands/commands/set-selection.command.ts lines 184-357 (`MoveSelectionEnterAndTabCommand`:
//   Enter and Tab inside a range, the first Tab of a row remembered for the Enter after it).
// x-spreadsheet `src/component/sheet.js` lines 92-127 (`selectorMove`) was read and compared: it moves one cell or to the
// sheet edge and has none of these rules. Nothing of it is taken.
// Modifications: the reference reads a worksheet object and moves ranges with merged-cell alignment; these are pure
// functions of a small sheet view (`hasValue`, `mergeAt`, `rowHidden`, `colHidden`, sheet and data extent) that move
// one cell, a merged range counts as one cell of its top-left value and is left past its far edge. A walk that starts
// past the last used row or column does not scan the empty cells (the reference visits each one). The reference wraps
// the arrow keys at the sheet edge; here they stop there. Home, Ctrl+Home, Ctrl+End, the page moves and the keys to the next
// and previous sheet (PD-290) are not in the reference and are written for the spreadsheet application's rules. Univer's command, selection service and shortcut
// tables are not used.
(function (host) {
  "use strict";

  const inBounds = (view, r, c) => r >= 1 && c >= 1 && r <= view.maxRows && c <= view.maxCols;
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

  // The cell of `at` that is the far edge of the merged range it lies in, in the direction of the move
  // (`getEdgeOfRange`); a cell outside every merge is its own edge.
  function edgeOf(view, at, dr, dc) {
    const m = view.mergeAt(at.r, at.c);
    if (!m) return at;
    return { r: dr > 0 ? m.r2 : dr < 0 ? m.r1 : at.r, c: dc > 0 ? m.c2 : dc < 0 ? m.c1 : at.c };
  }

  // The cell a merged range is selected by: its top-left cell.
  function anchorOf(view, at) {
    const m = view.mergeAt(at.r, at.c);
    return m ? { r: m.r1, c: m.c1 } : at;
  }

  // The next cell past `at` along `(dr, dc)` whose row and column are shown, or null past the sheet edge.
  function nextShown(view, at, dr, dc) {
    let r = at.r + dr;
    let c = at.c + dc;
    while (inBounds(view, r, c) && ((dr && view.rowHidden(r)) || (dc && view.colHidden(c)))) {
      r += dr;
      c += dc;
    }
    return inBounds(view, r, c) ? { r, c } : null;
  }

  // One arrow step from `from`: past the far edge of a merged range and over hidden rows and columns; the edge of
  // the sheet keeps the cell where it is.
  function step(view, from, dr, dc) {
    const next = nextShown(view, edgeOf(view, from, dr, dc), dr, dc);
    return next ? anchorOf(view, next) : from;
  }

  // Whether the cell, or the merged range it lies in, holds a value.
  function holds(view, at) {
    const a = anchorOf(view, at);
    return view.hasValue(a.r, a.c);
  }

  // Ctrl+Arrow (`findNextGapRange`). From a cell with a value, a run of values is walked to its last cell; from the
  // last cell of a run, or from an empty cell, the walk goes to the next cell with a value; with none left it stops at
  // the edge of the sheet. Nothing past the used rows and columns has a value, so a walk that starts there runs to the
  // edge (down, right) or comes back to the used part at once (up, left).
  function jump(view, from, dr, dc) {
    let at = edgeOf(view, from, dr, dc);
    let has = holds(view, from);
    let first = true;
    for (;;) {
      if (!has) {
        if ((dr > 0 && at.r >= view.dataRows) || (dc > 0 && at.c >= view.dataCols)) {
          return { r: dr > 0 ? view.maxRows : at.r, c: dc > 0 ? view.maxCols : at.c };
        }
        if (dr < 0 && at.r > view.dataRows + 1) at = { r: view.dataRows + 1, c: at.c };
        if (dc < 0 && at.c > view.dataCols + 1) at = { r: at.r, c: view.dataCols + 1 };
      }
      const next = nextShown(view, at, dr, dc);
      if (!next) return at;
      const nextHas = holds(view, next);
      if (has && !nextHas && !first) return at;
      at = edgeOf(view, next, dr, dc);
      if (!has && nextHas) return at;
      has = nextHas;
      first = false;
    }
  }

  // The next index past `base` along `sign` inside `[lo, hi]` that `hidden` does not hide, or null.
  function nextInside(base, sign, lo, hi, hidden) {
    let n = base + sign;
    while (n >= lo && n <= hi && hidden(n)) n += sign;
    return n >= lo && n <= hi ? n : null;
  }

  // The first shown index of `[lo, hi]` from the side `sign` comes from.
  function firstInside(sign, lo, hi, hidden) {
    return nextInside(sign > 0 ? lo - 1 : hi + 1, sign, lo, hi, hidden) ?? (sign > 0 ? lo : hi);
  }

  // Enter and Tab in a range of several cells (`findNextRange` with a boundary): the active cell moves down (Enter) or
  // right (Tab) inside the range, at its end to the next column or row, and from the last cell to the first. A negative
  // direction is the same walk backwards.
  function cycle(view, range, at, dr, dc) {
    const rows = dr !== 0;
    const sign = rows ? dr : dc;
    const m = view.mergeAt(at.r, at.c);
    const far = (lo, hi) => (m ? (sign > 0 ? m[hi] : m[lo]) : null);
    const major = rows ? { lo: range.r1, hi: range.r2, hid: view.rowHidden, at: far("r1", "r2") ?? at.r } : { lo: range.c1, hi: range.c2, hid: view.colHidden, at: far("c1", "c2") ?? at.c };
    const minor = rows ? { lo: range.c1, hi: range.c2, hid: view.colHidden, at: far("c1", "c2") ?? at.c } : { lo: range.r1, hi: range.r2, hid: view.rowHidden, at: far("r1", "r2") ?? at.r };
    let along = nextInside(major.at, sign, major.lo, major.hi, major.hid);
    let across = minor.at;
    if (along === null) {
      along = firstInside(sign, major.lo, major.hi, major.hid);
      across = nextInside(minor.at, sign, minor.lo, minor.hi, minor.hid) ?? firstInside(sign, minor.lo, minor.hi, minor.hid);
    }
    return anchorOf(view, rows ? { r: along, c: across } : { r: across, c: along });
  }

  // The cell the Enter after a row of Tabs goes to: below (or above) the cell the Tabs started from, in its column.
  function afterTabs(view, origin, dr) {
    return step(view, origin, dr, 0);
  }

  // PageDown, PageUp (rows) and Alt+PageDown, Alt+PageUp (columns): `span` shown rows or columns along the direction,
  // stopping at the edge of the sheet.
  function page(view, from, dr, dc, span) {
    let at = edgeOf(view, from, dr, dc);
    for (let i = 0; i < span; i += 1) {
      const next = nextShown(view, at, dr, dc);
      if (!next) break;
      at = next;
    }
    return anchorOf(view, at);
  }

  // The first shown row or column (Home, Ctrl+Home).
  function firstShown(count, hidden) {
    for (let i = 1; i <= count; i += 1) if (!hidden(i)) return i;
    return 1;
  }

  // Home: the first shown column of the row; Ctrl+Home: the first shown cell of the sheet.
  function home(view, from, whole) {
    return { r: whole ? firstShown(view.maxRows, view.rowHidden) : from.r, c: firstShown(view.maxCols, view.colHidden) };
  }

  // Ctrl+End: the cell where the last used row meets the last used column.
  function end(view) {
    return { r: clamp(view.dataRows, 1, view.maxRows), c: clamp(view.dataCols, 1, view.maxCols) };
  }

  // The keys that go to the next or previous sheet (PD-290): Ctrl+PageDown and Ctrl+PageUp, and on a Mac Option+Right and Option+Left
  // (the Option+Arrow of the text editing is the same key, so the caller leaves it to the text while a cell is edited). Shift
  // with them selects across sheets in the spreadsheet program, which this page has no state for: it is no step. Returns 1, -1 or 0.
  // The page keys of the cell move (PageDown without Ctrl, Alt+PageDown) stay with `page`.
  function sheetStep(ev, mac) {
    if (ev.shiftKey) return 0;
    const { key } = ev;
    if ((key === "PageDown" || key === "PageUp") && ev.ctrlKey && !ev.metaKey && !ev.altKey) return key === "PageDown" ? 1 : -1;
    if (mac && (key === "ArrowRight" || key === "ArrowLeft") && ev.altKey && !ev.ctrlKey && !ev.metaKey) return key === "ArrowRight" ? 1 : -1;
    return 0;
  }

  // The sheet `step` places from `active` among the shown sheets in tab order, null at the first or last sheet (the spreadsheet
  // program does not wrap) and when `active` is not a shown sheet.
  function sheetTarget(shown, active, step) {
    const at = shown.indexOf(active);
    const to = at + step;
    return at >= 0 && step !== 0 && to >= 0 && to < shown.length ? shown[to] : null;
  }

  host.__EASYOFFICE_SHEET_NAV__ = { step, jump, cycle, afterTabs, page, home, end, sheetStep, sheetTarget };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
