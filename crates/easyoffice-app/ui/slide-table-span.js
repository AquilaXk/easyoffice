(function (host) {
  "use strict";

  // Merged cells of a slide table (PD-402). A table element keeps `merges`: one `{ row, col, rowSpan, colSpan }` per merge, the
  // top-left cell and how many rows and columns it spans, as `TableSpan` of the file engine (`rich_slide_table.rs`) does. A merge
  // that leaves the grid, passes the table limits (64 rows, 32 columns) or lies over another merge is dropped, so a hostile list
  // can neither index outside the table nor cover a cell twice. The file form is `gridSpan`, `rowSpan` on the origin cell and
  // `hMerge`, `vMerge` on the covered cells (PptxGenJS `src/gen-xml.ts` 213-282, MIT, see THIRD_PARTY_NOTICES.md).
  const MAX_ROWS = 64;
  const MAX_COLS = 32;

  const asInt = (v) => (Number.isInteger(v) && v >= 0 && v <= 65535 ? v : null);

  // The merges that fit a `rows` x `cols` grid, in reading order, with no cell covered twice.
  function normalizeMerges(raw, rows, cols) {
    if (!Array.isArray(raw) || rows < 1 || cols < 1) return [];
    const taken = new Set();
    const out = [];
    for (const m of raw.slice(0, MAX_ROWS * MAX_COLS)) {
      const row = asInt(m?.row), col = asInt(m?.col), rowSpan = asInt(m?.rowSpan), colSpan = asInt(m?.colSpan);
      if (row === null || col === null || rowSpan === null || colSpan === null) continue;
      if (rowSpan < 1 || colSpan < 1 || (rowSpan === 1 && colSpan === 1)) continue;
      if (row + rowSpan > Math.min(rows, MAX_ROWS) || col + colSpan > Math.min(cols, MAX_COLS)) continue;
      const cells = [];
      for (let r = row; r < row + rowSpan; r += 1) for (let c = col; c < col + colSpan; c += 1) cells.push(r * MAX_COLS + c);
      if (cells.some((k) => taken.has(k))) continue;
      cells.forEach((k) => taken.add(k));
      out.push({ row, col, rowSpan, colSpan });
    }
    return out.sort((a, b) => a.row - b.row || a.col - b.col);
  }

  // `map[r][c]` is the merge a cell is the origin of (`origin`), or `covered` when a merge covers it.
  function coverMap(merges, rows, cols) {
    const map = Array.from({ length: rows }, () => Array.from({ length: cols }, () => null));
    for (const m of normalizeMerges(merges, rows, cols)) {
      for (let r = m.row; r < m.row + m.rowSpan; r += 1) {
        for (let c = m.col; c < m.col + m.colSpan; c += 1) map[r][c] = r === m.row && c === m.col ? { origin: m } : { covered: m };
      }
    }
    return map;
  }

  // A range of cells `{ r0, c0, r1, c1 }` grown until no merge is cut by its edge, as the presentation program selects it.
  function growRange(range, merges) {
    let { r0, c0, r1, c1 } = range;
    for (let again = true; again;) {
      again = false;
      for (const m of merges) {
        const hit = m.row <= r1 && m.row + m.rowSpan - 1 >= r0 && m.col <= c1 && m.col + m.colSpan - 1 >= c0;
        if (!hit) continue;
        const next = { r0: Math.min(r0, m.row), c0: Math.min(c0, m.col), r1: Math.max(r1, m.row + m.rowSpan - 1), c1: Math.max(c1, m.col + m.colSpan - 1) };
        if (next.r0 !== r0 || next.c0 !== c0 || next.r1 !== r1 || next.c1 !== c1) {
          ({ r0, c0, r1, c1 } = next);
          again = true;
        }
      }
    }
    return { r0, c0, r1, c1 };
  }

  // 셀 병합: the cells of `range` become one. The texts of the cells (not empty) join in reading order into the origin, one per
  // line, and the others are emptied. `null` when the range is one cell.
  function mergeRange(rows, merges, range) {
    const full = growRange(range, normalizeMerges(merges, rows.length, rows[0]?.length ?? 0));
    if (full.r0 === full.r1 && full.c0 === full.c1) return null;
    const texts = [];
    const nextRows = rows.map((r) => [...r]);
    for (let r = full.r0; r <= full.r1; r += 1) {
      for (let c = full.c0; c <= full.c1; c += 1) {
        if (nextRows[r][c]) texts.push(nextRows[r][c]);
        nextRows[r][c] = "";
      }
    }
    nextRows[full.r0][full.c0] = texts.join("\n").slice(0, 4096);
    const inside = (m) => m.row >= full.r0 && m.row <= full.r1 && m.col >= full.c0 && m.col <= full.c1;
    const kept = normalizeMerges(merges, rows.length, rows[0]?.length ?? 0).filter((m) => !inside(m));
    kept.push({ row: full.r0, col: full.c0, rowSpan: full.r1 - full.r0 + 1, colSpan: full.c1 - full.c0 + 1 });
    return { rows: nextRows, merges: normalizeMerges(kept, rows.length, rows[0]?.length ?? 0), range: full };
  }

  // 셀 분할 of a merged cell: the merge goes away and its cells are separate again (the text stays in the first one). `null` when the
  // cell is no merge.
  function splitAt(merges, rows, cols, at) {
    const list = normalizeMerges(merges, rows, cols);
    const hit = list.find((m) => at.row >= m.row && at.row < m.row + m.rowSpan && at.col >= m.col && at.col < m.col + m.colSpan);
    return hit ? list.filter((m) => m !== hit) : null;
  }

  // The merges after a row or column is inserted before `index` (`delta` 1) or the one at `index` is deleted (`delta` -1): a merge
  // after it moves, a merge across it grows or shrinks, as the presentation program keeps merged cells through row and column edits.
  function shiftMerges(merges, axis, index, delta) {
    const [start, span] = axis === "row" ? ["row", "rowSpan"] : ["col", "colSpan"];
    return merges.map((m) => {
      const next = { ...m };
      if (m[start] >= index + (delta < 0 ? 1 : 0)) next[start] = m[start] + delta;
      else if (m[start] + m[span] > index) next[span] = m[span] + delta;
      return next;
    });
  }

  host.__EASYOFFICE_SLIDE_TABLE_SPAN__ = Object.freeze({ MAX_ROWS, MAX_COLS, normalizeMerges, coverMap, growRange, mergeRange, splitAt, shiftMerges });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
