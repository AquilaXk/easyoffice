(function (host) {
  "use strict";

  // 이동 옵션 (Go To Special) of the sheet window (PD-391): which cells of a selection, or of the used range, are notes, constants,
  // formulas, blanks, the current region or array, the last cell, or in shown rows and columns. Pure functions of a small view of
  // the sheet. The cells found are returned as rectangles that the multi-range selection can take: runs of a row joined
  // under each other, at most `MAX_RANGES` of them (a result that needs more is refused, never cut).
  // Not covered: objects, row and column differences, precedents and dependents, conditional formats and validation, which
  // need the engine's dependency graph or the layers of the sheet.

  const MAX_RANGES = 64;
  // The blank search walks every cell of the scope, and every kind collects the cells it finds: past this many cells the
  // search is refused. The shown-cell search crosses row runs with column runs and walks rows and columns only.
  const MAX_SCAN_CELLS = 1000000;

  const KINDS = Object.freeze(["notes", "constants", "formulas", "blanks", "region", "array", "last", "visible"]);
  const VALUE_TYPES = Object.freeze(["number", "text", "logical", "error"]);

  const isFilled = (entry) => Boolean(entry) && (Boolean(entry.formula) || (entry.value !== undefined && entry.value !== null && entry.value !== ""));

  // The type of what a cell shows: a number, text, a logical value or an error.
  function valueTypeOf(entry, inferTag) {
    const tag = entry.t && entry.t !== "f" ? entry.t : inferTag(String(entry.value ?? ""));
    return tag === "n" ? "number" : tag === "b" ? "logical" : tag === "e" ? "error" : "text";
  }

  const inRect = (r, row, col) => row >= r.r1 && row <= r.r2 && col >= r.c1 && col <= r.c2;
  const inAny = (rects, row, col) => rects.some((r) => inRect(r, row, col));

  // Joins cells (`[row, col]` pairs) into rectangles: the runs of each row, then runs with the same columns on the next row.
  function rectsOf(cells) {
    const sorted = [...cells].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    const open = new Map();
    const done = [];
    let rowStart = 0;
    while (rowStart < sorted.length) {
      const row = sorted[rowStart][0];
      let rowEnd = rowStart;
      while (rowEnd < sorted.length && sorted[rowEnd][0] === row) rowEnd += 1;
      const runs = [];
      for (let i = rowStart; i < rowEnd; i += 1) {
        const col = sorted[i][1];
        const last = runs[runs.length - 1];
        if (last && last.c2 === col - 1) last.c2 = col;
        else if (!last || last.c2 !== col) runs.push({ c1: col, c2: col });
      }
      const next = new Map();
      for (const run of runs) {
        const key = run.c1 + ":" + run.c2;
        const above = open.get(key);
        if (above && above.r2 === row - 1) {
          above.r2 = row;
          next.set(key, above);
          open.delete(key);
        } else {
          next.set(key, { r1: row, c1: run.c1, r2: row, c2: run.c2 });
        }
      }
      for (const rect of open.values()) done.push(rect);
      open.clear();
      for (const [key, rect] of next) open.set(key, rect);
      rowStart = rowEnd;
    }
    for (const rect of open.values()) done.push(rect);
    return done.sort((a, b) => a.r1 - b.r1 || a.c1 - b.c1);
  }

  // The runs `[first, last]` of `from..to` whose rows or columns `hidden` does not hide; `null` as soon as there are more than
  // `max` of them (the caller would refuse the result anyway).
  function shownRuns(from, to, hidden, max) {
    const runs = [];
    for (let i = from; i <= to; i += 1) {
      if (hidden(i)) continue;
      const last = runs[runs.length - 1];
      if (last && last[1] === i - 1) last[1] = i;
      else if (runs.length >= max) return null;
      else runs.push([i, i]);
    }
    return runs;
  }

  // The region around a cell: the bounding box of the filled cells that touch it, each other or the box (the Ctrl+* region).
  function regionAround(view, row, col) {
    let box = { r1: row, c1: col, r2: row, c2: col };
    const filledAt = (r, c) => r >= 1 && c >= 1 && r <= view.maxRows && c <= view.maxCols && view.isFilled(r, c);
    for (let grew = true; grew; ) {
      grew = false;
      const edges = [
        [box.r1 - 1, box.c1 - 1, box.c2 + 1, "r1", -1, true],
        [box.r2 + 1, box.c1 - 1, box.c2 + 1, "r2", 1, true],
        [box.c1 - 1, box.r1 - 1, box.r2 + 1, "c1", -1, false],
        [box.c2 + 1, box.r1 - 1, box.r2 + 1, "c2", 1, false],
      ];
      for (const [at, from, to, side, dir, isRow] of edges) {
        for (let i = Math.max(1, from); i <= to; i += 1) {
          if (isRow ? filledAt(at, i) : filledAt(i, at)) {
            box = { ...box, [side]: box[side] + dir };
            grew = true;
            break;
          }
        }
      }
    }
    return box;
  }

  // `view`: `{ maxRows, maxCols, usedRows, usedCols, forEachEntry(visit(row, col, entry)), isFilled(row, col), hasNote(row, col),
  // rowHidden(row), colHidden(col), arrayAt(row, col), inferTag }`. `options`: `{ kind, types, scope, active }` where `scope` is the
  // selected rectangles (null for the whole used range), `active` the `{ row, col }` of the active cell and `types` the value
  // types a constants or formulas search keeps. Answers `{ ranges }` (empty when nothing matches) or `{ error }`.
  function findSpecial(view, { kind, types = VALUE_TYPES, scope = null, active = { row: 1, col: 1 } }) {
    if (!KINDS.includes(kind)) return { error: "kind" };
    const used = { r1: 1, c1: 1, r2: Math.max(1, view.usedRows), c2: Math.max(1, view.usedCols) };
    const area = scope && scope.length > 0 ? scope : [used];
    const keepTypes = new Set(types);
    const cells = [];
    const take = (row, col) => {
      cells.push([row, col]);
      return cells.length <= MAX_SCAN_CELLS;
    };

    if (kind === "region") {
      const box = regionAround(view, active.row, active.col);
      return { ranges: [box] };
    }
    if (kind === "array") {
      const area2 = view.arrayAt(active.row, active.col);
      return { ranges: area2 ? [{ r1: area2.r0, c1: area2.c0, r2: area2.r1, c2: area2.c1 }] : [] };
    }
    if (kind === "last") {
      return { ranges: view.usedRows > 0 && view.usedCols > 0 ? [{ r1: view.usedRows, c1: view.usedCols, r2: view.usedRows, c2: view.usedCols }] : [] };
    }
    if (kind === "visible") {
      // The shown rows and columns of each rectangle: runs of rows and runs of columns cross into rectangles.
      // The walk stops at the used range (a selection of whole rows or columns would walk a million lines), a count of runs
      // past the bound ends it, and the product is checked before it is made.
      const ranges = [];
      for (const r of area) {
        const rows = shownRuns(r.r1, Math.min(r.r2, Math.max(r.r1, used.r2)), view.rowHidden, MAX_RANGES);
        const cols = rows && shownRuns(r.c1, Math.min(r.c2, Math.max(r.c1, used.c2)), view.colHidden, MAX_RANGES);
        if (!rows || !cols || ranges.length + rows.length * cols.length > MAX_RANGES) return { error: "ranges" };
        for (const [r1, r2] of rows) for (const [c1, c2] of cols) ranges.push({ r1, c1, r2, c2 });
      }
      return { ranges };
    }
    if (kind === "blanks") {
      // Each empty cell of the scope inside the used range is looked at; a scope with too many cells is refused.
      const bounds = area.map((r) => ({ r1: r.r1, c1: r.c1, r2: Math.min(r.r2, used.r2), c2: Math.min(r.c2, used.c2) }));
      const scanned = bounds.reduce((sum, r) => sum + Math.max(0, r.r2 - r.r1 + 1) * Math.max(0, r.c2 - r.c1 + 1), 0);
      if (scanned > MAX_SCAN_CELLS) return { error: "scope" };
      const seen = new Set();
      for (const r of bounds) {
        for (let row = r.r1; row <= r.r2; row += 1) {
          for (let col = r.c1; col <= r.c2; col += 1) {
            const key = row * (view.maxCols + 1) + col;
            if (seen.has(key) || view.isFilled(row, col)) continue;
            seen.add(key);
            take(row, col);
          }
        }
      }
      return rectsResult(cells);
    }

    // Notes, constants and formulas are looked for in the whole sheet when nothing but one cell is selected.
    const searched = scope && scope.length > 0 ? scope : [{ r1: 1, c1: 1, r2: view.maxRows, c2: view.maxCols }];
    const seen = new Set();
    const visit = (row, col, entry) => {
      if (!inAny(searched, row, col)) return;
      const key = row * (view.maxCols + 1) + col;
      if (seen.has(key)) return;
      seen.add(key);
      if (kind === "notes") {
        if (view.hasNote(row, col)) take(row, col);
        return;
      }
      if (!isFilled(entry)) return;
      const isFormula = Boolean(entry.formula);
      if (isFormula !== (kind === "formulas")) return;
      if (keepTypes.has(valueTypeOf(entry, view.inferTag))) take(row, col);
    };
    view.forEachEntry(visit);
    if (kind === "notes") view.forEachNote?.((row, col) => visit(row, col, null));
    return rectsResult(cells);
  }

  function rectsResult(cells) {
    if (cells.length > MAX_SCAN_CELLS) return { error: "scope" };
    const ranges = rectsOf(cells);
    return ranges.length > MAX_RANGES ? { error: "ranges", count: ranges.length } : { ranges };
  }

  host.__EASYOFFICE_SHEET_SPECIAL_CELLS__ = Object.freeze({ KINDS, VALUE_TYPES, MAX_RANGES, MAX_SCAN_CELLS, findSpecial, rectsOf, regionAround });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
