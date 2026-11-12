(function (host) {
  "use strict";

  // Row and column groups (PD-208): the rules of grouping, folding and the level buttons. An outline
  // is `{ rows, cols, foldedRows, foldedCols, summaryAbove, summaryLeft }` (the save payload's form,
  // 1-based): `rows` and `cols` are runs `[first, last, level]` (level 1 to 7) that are sorted and
  // disjoint; `foldedRows` and `foldedCols` are the runs a fold hid, each inside a group. Every
  // function returns a new outline and leaves its argument alone. The engine moves the runs when
  // rows or columns are inserted or deleted (`structure` session action).
  //
  // A group is a stretch of consecutive rows whose level is at least `k`; the row (column) that
  // sums it up is the one below it (to its right), or above it (to its left) when `summaryAbove`
  // (`summaryLeft`) is set. Folding a group hides every row of the stretch and unfolding shows them
  // all again; the level buttons (`showLevel`) are the way to fold nested groups one level at a time.

  const MAX_LEVEL = 7;
  const MAX_GROUPED = 100000;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

  const keys = (axis) => (axis === "row" ? { levels: "rows", folded: "foldedRows" } : { levels: "cols", folded: "foldedCols" });
  const levelsOf = (outline, axis) => outline?.[keys(axis).levels] || [];
  const foldedOf = (outline, axis) => outline?.[keys(axis).folded] || [];
  const summaryAbove = (outline, axis) => Boolean(axis === "row" ? outline?.summaryAbove : outline?.summaryLeft);

  function levelAt(runs, i) {
    let lo = 0;
    let hi = runs.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (runs[mid][1] < i) lo = mid + 1;
      else if (runs[mid][0] > i) hi = mid - 1;
      else return runs[mid][2];
    }
    return 0;
  }

  const maxLevel = (outline, axis) => levelsOf(outline, axis).reduce((m, r) => Math.max(m, r[2]), 0);
  const groupedTotal = (runs) => runs.reduce((n, r) => n + r[1] - r[0] + 1, 0);

  // `runs` with the levels of `first..last` replaced by `fn(level)` (0 outside every group).
  function mapLevels(runs, first, last, fn) {
    const out = [];
    const push = (a, b, level) => {
      if (level <= 0 || a > b) return;
      const prev = out[out.length - 1];
      if (prev && prev[1] + 1 === a && prev[2] === level) prev[1] = b;
      else out.push([a, b, level]);
    };
    let i = 0;
    for (; i < runs.length && runs[i][1] < first; i += 1) push(...runs[i]);
    if (i < runs.length && runs[i][0] < first) push(runs[i][0], first - 1, runs[i][2]);
    let at = first;
    for (; i < runs.length && runs[i][0] <= last; i += 1) {
      const [a, b, level] = runs[i];
      const s = Math.max(a, first);
      const e = Math.min(b, last);
      if (s > at) push(at, s - 1, fn(0));
      push(s, e, fn(level));
      at = e + 1;
      if (b > last) push(last + 1, b, level);
    }
    if (at <= last) push(at, last, fn(0));
    for (; i < runs.length; i += 1) push(...runs[i]);
    return out;
  }

  // Stretches of consecutive indexes whose level is at least `k`.
  function segments(runs, k) {
    const out = [];
    for (const [a, b, level] of runs) {
      if (level < k) continue;
      const last = out[out.length - 1];
      if (last && last[1] + 1 === a) last[1] = b;
      else out.push([a, b]);
    }
    return out;
  }

  // Every group of one axis, outermost first: `{ level, first, last, summary }`; `summary` is 0 when
  // the group starts at the first row and its summary would lie above the sheet.
  function groups(outline, axis) {
    const runs = levelsOf(outline, axis);
    const above = summaryAbove(outline, axis);
    const out = [];
    for (let k = 1; k <= maxLevel(outline, axis); k += 1) {
      for (const [first, last] of segments(runs, k)) out.push({ level: k, first, last, summary: above ? first - 1 : last + 1 });
    }
    return out;
  }

  // The groups of one level, kept per outline object: an edit makes a new one.
  const byLevelCache = new WeakMap();
  function byLevel(outline, axis) {
    let entry = byLevelCache.get(outline);
    if (!entry) byLevelCache.set(outline, (entry = {}));
    if (!entry[axis]) {
      entry[axis] = [];
      for (const g of groups(outline, axis)) (entry[axis][g.level] ||= []).push(g);
    }
    return entry[axis];
  }

  // The groups with a row (column) in `lo..hi` or a summary there, outermost first. A level's groups
  // are sorted and disjoint, so each level is cut by binary search.
  function visibleGroups(outline, axis, lo, hi) {
    const out = [];
    for (const list of byLevel(outline, axis)) {
      if (!list) continue;
      let a = 0;
      let b = list.length;
      while (a < b) {
        const mid = (a + b) >> 1;
        if (list[mid].last + 1 < lo) a = mid + 1;
        else b = mid;
      }
      for (; a < list.length && list[a].first - 1 <= hi; a += 1) out.push(list[a]);
    }
    return out;
  }

  const covers = (runs, first, last) => {
    const run = layoutApi.runAt(runs, first);
    return Boolean(run) && run[1] >= last;
  };

  const isFolded = (outline, axis, group) => covers(foldedOf(outline, axis), group.first, group.last);

  // The folded runs may only hold indexes of a group.
  function trimFolded(outline) {
    const next = { ...outline };
    for (const axis of ["row", "col"]) {
      const k = keys(axis);
      const plain = levelsOf(next, axis).map((r) => [r[0], r[1]]);
      const kept = [];
      for (const [a, b] of foldedOf(next, axis)) {
        for (const [s, e] of plain) {
          if (e < a) continue;
          if (s > b) break;
          kept.push([Math.max(a, s), Math.min(b, e)]);
        }
      }
      const merged = kept.reduce((acc, run) => layoutApi.addRun(acc, run[0], run[1]), []);
      if (merged.length) next[k.folded] = merged;
      else delete next[k.folded];
      if (!levelsOf(next, axis).length) delete next[k.levels];
    }
    return next;
  }

  const clean = (outline) => {
    const next = trimFolded(outline);
    if (!next.summaryAbove) delete next.summaryAbove;
    if (!next.summaryLeft) delete next.summaryLeft;
    return next;
  };

  // Groups the rows (columns) `first..last` one level deeper. `{ outline }` or `{ error }`:
  // `level` when one of them already is at the deepest level, `limit` when the sheet holds too many.
  function group(outline, axis, first, last) {
    const runs = levelsOf(outline, axis);
    for (const [a, b, level] of runs) if (b >= first && a <= last && level >= MAX_LEVEL) return { error: "level" };
    const mapped = mapLevels(runs, first, last, (level) => level + 1);
    if (groupedTotal(mapped) > MAX_GROUPED) return { error: "limit" };
    return { outline: clean({ ...outline, [keys(axis).levels]: mapped }) };
  }

  // Takes the rows (columns) `first..last` one level out; `{ error: "none" }` when none is in a group.
  function ungroup(outline, axis, first, last) {
    const runs = levelsOf(outline, axis);
    if (!runs.some((r) => r[1] >= first && r[0] <= last)) return { error: "none" };
    const mapped = mapLevels(runs, first, last, (level) => level - 1);
    return { outline: clean({ ...outline, [keys(axis).levels]: mapped }) };
  }

  // Folds or unfolds `group` (a stretch from `groups`).
  function setFolded(outline, axis, group, fold) {
    const k = keys(axis).folded;
    const runs = foldedOf(outline, axis);
    const next = fold ? layoutApi.addRun(runs, group.first, group.last) : layoutApi.removeRun(runs, group.first, group.last);
    return clean({ ...outline, [k]: next });
  }

  // What the level button `n` shows: the rows of a level below `n`, so every deeper group is folded.
  function showLevel(outline, axis, n) {
    const hidden = segments(levelsOf(outline, axis), n);
    return clean({ ...outline, [keys(axis).folded]: hidden });
  }

  // The groups a fold command acts on for the selection `first..last`: those whose summary lies in
  // the selection, else the innermost group that holds the whole selection.
  function groupsFor(outline, axis, first, last) {
    const all = groups(outline, axis);
    const summed = all.filter((g) => g.summary >= first && g.summary <= last && g.summary >= 1);
    if (summed.length) return summed;
    const holding = all.filter((g) => g.first <= first && g.last >= last);
    return holding.length ? [holding[holding.length - 1]] : [];
  }

  function fold(outline, axis, first, last, hide) {
    const targets = groupsFor(outline, axis, first, last).filter((g) => isFolded(outline, axis, g) !== hide);
    if (!targets.length) return { error: "none" };
    return { outline: targets.reduce((acc, g) => setFolded(acc, axis, g, hide), outline) };
  }

  // 윤곽 지우기: no groups on either axis, the summary sides stay.
  function clear(outline) {
    return clean({ summaryAbove: outline?.summaryAbove, summaryLeft: outline?.summaryLeft });
  }

  const hasGroups = (outline) => levelsOf(outline, "row").length > 0 || levelsOf(outline, "col").length > 0;

  host.__EASYOFFICE_SHEET_OUTLINE__ = {
    MAX_LEVEL,
    MAX_GROUPED,
    levelAt,
    levelsOf,
    foldedOf,
    maxLevel,
    groups,
    visibleGroups,
    isFolded,
    group,
    ungroup,
    setFolded,
    showLevel,
    fold,
    clear,
    hasGroups,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
