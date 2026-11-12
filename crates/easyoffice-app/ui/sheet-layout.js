(function (host) {
  "use strict";

  // Position-bound layout of every sheet (PD-194): merged ranges and hidden rows and columns.
  // Rows and columns are 1-based. A run is `[first, last]`; the runs of one axis are sorted and
  // disjoint. A merge is `{ r1, c1, r2, c2 }` (the anchor is its top-left cell). The engine moves
  // them with the cells (`structure` session action), so this module only stores and looks up.

  const MAX_MERGES = 100000;
  const BLOCK = 64; // merge lookup buckets, in rows
  const TALL_BLOCKS = 4096; // a merge taller than this many buckets is kept in a plain list

  const bySheet = Object.create(null);
  let version = 0;
  const listeners = [];

  function blank() {
    return { merges: [], hiddenRows: [], hiddenCols: [], index: null };
  }

  function entry(name) {
    return bySheet[name] || (bySheet[name] = blank());
  }

  function isEmpty(layout) {
    return !layout || (layout.merges.length === 0 && layout.hiddenRows.length === 0 && layout.hiddenCols.length === 0);
  }

  function touch() {
    version += 1;
    listeners.forEach((fn) => fn());
  }

  // ---- runs -----------------------------------------------------------------------------------
  function runContains(runs, i) {
    let lo = 0;
    let hi = runs.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (runs[mid][1] < i) lo = mid + 1;
      else if (runs[mid][0] > i) hi = mid - 1;
      else return true;
    }
    return false;
  }

  // Number of hidden indexes below `i`.
  function hiddenBefore(runs, i) {
    let n = 0;
    for (let k = 0; k < runs.length && runs[k][0] < i; k += 1) n += Math.min(runs[k][1], i - 1) - runs[k][0] + 1;
    return n;
  }

  function hiddenTotal(runs) {
    let n = 0;
    for (const [a, b] of runs) n += b - a + 1;
    return n;
  }

  // The index of the `v`-th (0-based) visible position, counting from 1.
  function visibleToIndex(runs, v) {
    let i = v + 1;
    for (const [a, b] of runs) {
      if (a > i) break;
      i += b - a + 1;
    }
    return i;
  }

  function addRun(runs, a, b) {
    const next = runs.concat([[a, b]]).sort((x, y) => x[0] - y[0]);
    const out = [];
    for (const run of next) {
      const last = out[out.length - 1];
      if (last && run[0] <= last[1] + 1) last[1] = Math.max(last[1], run[1]);
      else out.push([run[0], run[1]]);
    }
    return out;
  }

  function removeRun(runs, a, b) {
    const out = [];
    for (const [s, e] of runs) {
      if (e < a || s > b) out.push([s, e]);
      else {
        if (s < a) out.push([s, a - 1]);
        if (e > b) out.push([b + 1, e]);
      }
    }
    return out;
  }

  // ---- merges ---------------------------------------------------------------------------------
  function buildIndex(layout) {
    const buckets = new Map();
    const tall = [];
    for (const m of layout.merges) {
      const first = Math.floor(m.r1 / BLOCK);
      const last = Math.floor(m.r2 / BLOCK);
      if (last - first > TALL_BLOCKS) {
        tall.push(m);
        continue;
      }
      for (let b = first; b <= last; b += 1) {
        const list = buckets.get(b);
        if (list) list.push(m);
        else buckets.set(b, [m]);
      }
    }
    layout.index = { buckets, tall };
  }

  function contains(m, r, c) {
    return r >= m.r1 && r <= m.r2 && c >= m.c1 && c <= m.c2;
  }

  function mergeAt(name, r, c) {
    const layout = bySheet[name];
    if (!layout || layout.merges.length === 0) return null;
    if (!layout.index) buildIndex(layout);
    const list = layout.index.buckets.get(Math.floor(r / BLOCK));
    if (list) for (const m of list) if (contains(m, r, c)) return m;
    for (const m of layout.index.tall) if (contains(m, r, c)) return m;
    return null;
  }

  function mergesIn(name, r1, c1, r2, c2) {
    const layout = bySheet[name];
    if (!layout) return [];
    return layout.merges.filter((m) => m.r1 <= r2 && m.r2 >= r1 && m.c1 <= c2 && m.c2 >= c1);
  }

  // Merges the range; merges it overlaps are replaced (the reference's add-merge rule).
  function addMerge(name, r1, c1, r2, c2) {
    const layout = entry(name);
    layout.merges = layout.merges.filter((m) => !(m.r1 <= r2 && m.r2 >= r1 && m.c1 <= c2 && m.c2 >= c1));
    if ((r2 > r1 || c2 > c1) && layout.merges.length < MAX_MERGES) layout.merges.push({ r1, c1, r2, c2 });
    layout.index = null;
    touch();
  }

  // Unmerges every merge that touches the range; returns how many it removed.
  function removeMerges(name, r1, c1, r2, c2) {
    const layout = entry(name);
    const before = layout.merges.length;
    layout.merges = layout.merges.filter((m) => !(m.r1 <= r2 && m.r2 >= r1 && m.c1 <= c2 && m.c2 >= c1));
    layout.index = null;
    if (layout.merges.length !== before) touch();
    return before - layout.merges.length;
  }

  // ---- hidden rows and columns ------------------------------------------------------------------
  function setHidden(name, axis, a, b, hidden) {
    const layout = entry(name);
    const key = axis === "row" ? "hiddenRows" : "hiddenCols";
    layout[key] = hidden ? addRun(layout[key], a, b) : removeRun(layout[key], a, b);
    touch();
  }

  const rowHidden = (name, r) => Boolean(bySheet[name]) && runContains(bySheet[name].hiddenRows, r);
  const colHidden = (name, c) => Boolean(bySheet[name]) && runContains(bySheet[name].hiddenCols, c);

  // ---- JSON form (the page's save payload and the engine's `structure` answer) ---------------------
  function toJson(layout) {
    const out = {};
    if (layout.merges.length) {
      out.merges = layout.merges.map((m) => refOf(m.r1, m.c1) + ":" + refOf(m.r2, m.c2));
    }
    if (layout.hiddenRows.length) out.hiddenRows = layout.hiddenRows.map((r) => [r[0], r[1]]);
    if (layout.hiddenCols.length) out.hiddenCols = layout.hiddenCols.map((r) => [r[0], r[1]]);
    return out;
  }

  function refOf(r, c) {
    return host.__EASYOFFICE_SHEET_GRID_UTILS__.refOf(r, c);
  }

  function fromJson(json) {
    const layout = blank();
    const parse = host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef;
    for (const text of Array.isArray(json?.merges) ? json.merges : []) {
      const [a, b] = String(text).split(":");
      const p = parse(a);
      const q = parse(b ?? "");
      if (p && q && layout.merges.length < MAX_MERGES) layout.merges.push({ r1: p.row, c1: p.col, r2: q.row, c2: q.col });
    }
    const runs = (list) =>
      (Array.isArray(list) ? list : []).filter((p) => Array.isArray(p) && p[0] >= 1 && p[1] >= p[0]).map((p) => [p[0], p[1]]);
    layout.hiddenRows = runs(json?.hiddenRows);
    layout.hiddenCols = runs(json?.hiddenCols);
    return layout;
  }

  function getAll() {
    const out = {};
    for (const name of Object.keys(bySheet)) {
      const json = toJson(bySheet[name]);
      if (Object.keys(json).length > 0) out[name] = json;
    }
    return out;
  }

  function replaceAll(all) {
    for (const name of Object.keys(bySheet)) delete bySheet[name];
    for (const name of Object.keys(all || {})) {
      const layout = fromJson(all[name]);
      if (!isEmpty(layout)) bySheet[name] = layout;
    }
    touch();
  }

  // Replaces one sheet's layout with the JSON the engine answered.
  function setSheet(name, json) {
    const layout = fromJson(json);
    if (isEmpty(layout)) delete bySheet[name];
    else bySheet[name] = layout;
    touch();
  }

  // ---- how a row or column edit moves a position (the engine moves the formulas) -----------------
  // `edit` is `{ axis: "row"|"col", op: "insert"|"delete", at, count }`, `at` 1-based.
  function mapIndex(edit, i) {
    if (i < edit.at) return i;
    if (edit.op === "insert") return i + edit.count;
    return i >= edit.at + edit.count ? i - edit.count : null;
  }

  // `ref` after the edit; null when a deletion removes it.
  function remapRef(edit, ref) {
    const p = host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef(ref);
    if (!p) return ref;
    const row = edit.axis === "row" ? mapIndex(edit, p.row) : p.row;
    const col = edit.axis === "col" ? mapIndex(edit, p.col) : p.col;
    return row === null || col === null ? null : refOf(row, col);
  }

  host.__EASYOFFICE_SHEET_LAYOUT__ = {
    mapIndex,
    remapRef,
    get: (name) => bySheet[name] || null,
    version: () => version,
    onChange: (fn) => listeners.push(fn),
    isEmpty,
    runContains,
    hiddenBefore,
    hiddenTotal,
    visibleToIndex,
    mergeAt,
    mergesIn,
    addMerge,
    removeMerges,
    setHidden,
    rowHidden,
    colHidden,
    getAll,
    replaceAll,
    setSheet,
    toJson,
    MAX_MERGES,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
