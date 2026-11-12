(function (host) {
  "use strict";

  // Sheet sort (PD-200). The page picks the rows that may move and sends the key columns to the
  // engine (`sessionAction: "sort"`), which answers the stable row order and the rewritten
  // formulas; the page then moves the cells. The rules follow the reference sheet sort package
  // (`packages/sheets-sort`, Apache-2.0, see THIRD_PARTY_NOTICES.md): rows hidden by the user or by
  // a filter stay where they are, a range with merged cells sorts only when equal merges tile it
  // (`isRangeDividedEqually`), and a header row is left out on request.
  //
  // Rows and columns are 1-based, as everywhere on the page.

  const MAX_KEYS = 64;
  const MAX_REQUEST_CELLS = 2000000; // the engine's budget for rows times keys
  const MAX_FORMULA_CELLS = 2000000;

  const FAILURES = {
    merge: "병합된 셀이 있어 정렬할 수 없음",
    mergeSize: "크기가 다른 병합 셀이 있어 정렬할 수 없음",
    large: "정렬 범위가 너무 큼",
    engine: "정렬하지 못함",
  };

  // The merges of `range` as one unit size, or the reason they cannot be sorted. `merges` are
  // the ones that touch the range. Every merge must lie inside it, all must have one size, and
  // together they must cover the whole range (so rows move in blocks of `h` and the layout stays).
  function unitOf(merges, range) {
    if (merges.length === 0) return { h: 1, w: 1 };
    let h = 0;
    let w = 0;
    let area = 0;
    for (const m of merges) {
      if (m.r1 < range.r1 || m.r2 > range.r2 || m.c1 < range.c1 || m.c2 > range.c2) return { error: "merge" };
      const mh = m.r2 - m.r1 + 1;
      const mw = m.c2 - m.c1 + 1;
      if (h === 0) {
        h = mh;
        w = mw;
      } else if (mh !== h || mw !== w) {
        return { error: "mergeSize" };
      }
      area += mh * mw;
    }
    const total = (range.r2 - range.r1 + 1) * (range.c2 - range.c1 + 1);
    return area === total ? { h, w } : { error: "mergeSize" };
  }

  // The first row of every unit that may move: not hidden by the user or by a filter. A unit is
  // hidden when its first row is (the reference checks the merge's anchor row).
  function unitRows(range, h, hidden) {
    const rows = [];
    for (let r = range.r1; r + h - 1 <= range.r2; r += h) if (!hidden(r)) rows.push(r);
    return rows;
  }

  // The number a cell holds (formatted numbers such as `1,234` or `50%` count), or null.
  function numberOf(entry) {
    const text = host.__EASYOFFICE_SHEET_UTILS__.normalizeFormattedNumericCell(entry?.value);
    const n = Number(text);
    return text !== "" && Number.isFinite(n) ? n : null;
  }

  // A sort key as the engine takes it: null for a blank cell, a number, or the text.
  function keyValue(entry, numberOf) {
    const text = String(entry?.value ?? "").trim();
    if (text === "") return null;
    const n = numberOf(entry);
    return n === null ? text : n;
  }

  const isBold = (style) => style?.fontWeight === "bold" || Number(style?.fontWeight) >= 600;

  // Whether the first row of `range` looks like a header: its cells are text where the row below
  // holds numbers, or they are bold where the row below is not. A range of one row has none.
  function detectHeader(map, range, refOf, numberOf) {
    if (range.r2 <= range.r1) return false;
    let text = 0;
    let boldHead = 0;
    let boldBody = 0;
    for (let c = range.c1; c <= range.c2; c += 1) {
      const head = map[refOf(range.r1, c)];
      const body = map[refOf(range.r1 + 1, c)];
      const headText = String(head?.value ?? "").trim();
      if (headText !== "") {
        if (numberOf(head) !== null) return false;
        text += 1;
        if (numberOf(body) !== null) return true;
      }
      if (isBold(head?.style)) boldHead += 1;
      if (isBold(body?.style)) boldBody += 1;
    }
    return text > 0 && boldHead > 0 && boldBody === 0;
  }

  // The moves of the units: `order[p]` is the unit index whose rows go to the place of unit `p`.
  // Returns `[{ from, to }]` row offsets (first rows) for the units that change place.
  function movesOf(order, rows) {
    const moves = [];
    for (let p = 0; p < order.length; p += 1) if (order[p] !== p) moves.push({ from: rows[order[p]], to: rows[p] });
    return moves;
  }

  host.__EASYOFFICE_SHEET_SORT_CORE__ = { MAX_KEYS, FAILURES, numberOf, unitOf, unitRows, keyValue, detectHeader, movesOf };

  function createSheetSort(deps) {
    const { gridUtils, call, layoutApi, cloneStyle } = deps;
    const { refOf } = gridUtils;

    // The key columns of the units: the value of the anchor cell of each unit (a key column inside
    // a merge reads the merge's own cell).
    function keyColumns(map, spec, rows, w) {
      return spec.keys.map((key) => {
        const anchor = w > 1 ? spec.range.c1 + Math.floor((key.col - spec.range.c1) / w) * w : key.col;
        return { asc: key.asc, values: rows.map((r) => keyValue(map[refOf(r, anchor)], numberOf)) };
      });
    }

    // The formula cells of the units and the request lists that go with them.
    function formulaCells(map, spec, rows, h) {
      const cells = [];
      const texts = [];
      for (let u = 0; u < rows.length; u += 1) {
        for (let k = 0; k < h; k += 1) {
          for (let c = spec.range.c1; c <= spec.range.c2; c += 1) {
            const entry = map[refOf(rows[u] + k, c)];
            if (!entry?.formula || cells.length >= MAX_FORMULA_CELLS) continue;
            cells.push([u, k, c]);
            texts.push([u, entry.formula]);
          }
        }
      }
      return { cells, texts };
    }

    // Moves the cells of the units to their new places in `map`; `formulas` are the rewritten
    // formula texts of `cells` after the move.
    function apply(map, spec, rows, h, order, cells, formulas) {
      const before = new Map();
      for (let u = 0; u < rows.length; u += 1) {
        if (order[u] === u) continue;
        for (let k = 0; k < h; k += 1) {
          for (let c = spec.range.c1; c <= spec.range.c2; c += 1) {
            const ref = refOf(rows[u] + k, c);
            before.set(ref, map[ref]);
          }
        }
      }
      const rewritten = new Map();
      cells.forEach(([u, k, c], i) => rewritten.set(refOf(rows[u] + k, c), formulas[i]));
      for (let p = 0; p < order.length; p += 1) {
        if (order[p] === p) continue;
        for (let k = 0; k < h; k += 1) {
          for (let c = spec.range.c1; c <= spec.range.c2; c += 1) {
            const source = refOf(rows[order[p]] + k, c);
            const target = refOf(rows[p] + k, c);
            const src = before.get(source);
            if (!src) {
              delete map[target];
              continue;
            }
            const moved = { value: src.value, formula: "" };
            if (src.formula) moved.formula = rewritten.get(source) ?? src.formula;
            const style = cloneStyle(src.style);
            if (style) moved.style = style;
            map[target] = moved;
          }
        }
      }
    }

    // Sorts `spec.range` of the sheet `name` by `spec.keys` (`{ col, asc }`, 1-based columns inside
    // the range). `spec.hasHeader` leaves the first row out; `spec.caseSensitive` tells `a` from
    // `A`. Resolves `{ ok: true, moved }` (`moved`: rows that changed place) or `{ ok: false, why }`
    // with a key of `FAILURES`.
    async function run(name, map, spec) {
      const range = { ...spec.range, r1: spec.range.r1 + (spec.hasHeader ? 1 : 0) };
      if (range.r2 <= range.r1 || spec.keys.length === 0 || spec.keys.length > MAX_KEYS) return { ok: true, moved: 0 };
      const unit = unitOf(layoutApi.mergesIn(name, range.r1, range.c1, range.r2, range.c2), range);
      if (unit.error) return { ok: false, why: unit.error };
      const rows = unitRows(range, unit.h, (r) => layoutApi.rowHidden(name, r));
      if (rows.length < 2) return { ok: true, moved: 0 };
      if (rows.length * spec.keys.length > MAX_REQUEST_CELLS) return { ok: false, why: "large" };
      const framed = { ...spec, range };
      const { cells, texts } = formulaCells(map, framed, rows, unit.h);
      let answer;
      try {
        answer = await call({
          sessionAction: "sort",
          caseSensitive: Boolean(spec.caseSensitive),
          keys: keyColumns(map, framed, rows, unit.w),
          sheet: name,
          range: { r1: range.r1, r2: range.r2 },
          rows,
          height: unit.h,
          formulas: texts,
        });
      } catch {
        return { ok: false, why: "engine" };
      }
      const order = answer?.order;
      const formulas = answer?.formulas;
      if (!Array.isArray(order) || order.length !== rows.length || !Array.isArray(formulas) || formulas.length !== cells.length) {
        return { ok: false, why: "engine" };
      }
      apply(map, framed, rows, unit.h, order, cells, formulas);
      return { ok: true, moved: movesOf(order, rows).length };
    }

    return { run };
  }

  host.__EASYOFFICE_CREATE_SHEET_SORT__ = createSheetSort;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
