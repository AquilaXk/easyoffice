(function (host) {
  "use strict";

  // Position-bound layout of every sheet (PD-194): merged ranges and hidden rows and columns.
  // Rows and columns are 1-based. A run is `[first, last]`; the runs of one axis are sorted and
  // disjoint. A merge is `{ r1, c1, r2, c2 }` (the anchor is its top-left cell). The engine moves
  // them with the cells (`structure` session action), so this module only stores and looks up.
  // The auto filter (PD-198) is `{ range: { r1, c1, r2, c2 }, columns, hidden }`: `columns` are the
  // engine's column conditions as sent and received, `hidden` the rows the filter hides, kept apart
  // from `hiddenRows` (the user's). The grid asks for the union through a per-sheet row index.
  // The tables (PD-224) are the engine's JSON as sent and received: `[{ name, range: "A1:C5", header, totals, columns,
  // style, firstColumn, lastColumn, rowStripes, columnStripes, filter? }]`. The engine moves their ranges with the cells
  // (`structure` session action); this module stores them and gives a copied sheet's tables names of their own.
  // The sheet is hidden (PD-209): `hidden` is true for a sheet without a tab; `veryHidden` marks one the sheet menu
  // does not list either (a file's `veryHidden` sheet), which is hidden as well.
  // The protection (PD-216): `protect` is the engine's JSON `{ on, deny: [flag], pwd, ranges }` (the password only as the
  // digest the engine made), `unlockedRows` and `unlockedCols` are runs of rows and columns whose own format is
  // unlocked (`sheet-protect.js` reads them).
  // The view (PD-208): `freeze` `{ rows, cols }` (counts), `tabColor` `"rrggbb"` and the outline
  // (`sheet-outline.js` owns its rules): level runs `[first, last, level]` and the folded runs
  // `foldedRows`, `foldedCols`, which are hidden like `hiddenRows` but belong to the groups.

  const MAX_MERGES = 100000, MAX_CHARTS_PER_SHEET = 64, MAX_CHART_SERIES = 32;
  const MAX_IMAGES_PER_SHEET = 64, MAX_IMAGES_WORKBOOK = 256, MAX_SINGLE_IMAGE_BYTES = 32 * 1024 * 1024, MAX_TOTAL_IMAGE_BYTES = 128 * 1024 * 1024;
  const MAX_NOTES_PER_SHEET = 4096, MAX_NOTES_WORKBOOK = 16384, MAX_NOTE_AUTHOR_BYTES = 256, MAX_NOTE_TEXT_BYTES = 32768, EMU_PER_PX = 9525;
  const CHART_TYPES = Object.freeze(["column", "bar", "line", "pie", "scatter"]);
  const CHART_TYPE_SET = new Set(CHART_TYPES);
  const CHART_LEGENDS = Object.freeze(["right", "bottom", "top", "none", "left"]);
  const CHART_LEGEND_SET = new Set(CHART_LEGENDS);
  const BLOCK = 64, TALL_BLOCKS = 4096, DEFAULT_COL_PX = 72, MIN_COL_PX = 8, MAX_DEFAULT_COL_PX = 2000; // the last three: PD-305, PD-306, `default_col_px`

  const bySheet = Object.create(null);
  let version = 0;
  const listeners = [];

  function blank() {
    return { merges: [], hiddenRows: [], hiddenCols: [], rowHeights: new Map(), defaultRowHeight: null, defaultColWidth: null, defaultColPx: null, filter: null, freeze: null, tabColor: null, hidden: false, veryHidden: false, outline: null, page: null, protect: null, unlockedRows: [], unlockedCols: [], tables: [], charts: [], pivots: [], images: [], notes: [], noteMap: null, index: null, rows: null };
  }

  const entry = (name) => bySheet[name] || (bySheet[name] = blank());

  function isEmpty(l) {
    return !l || (!l.merges.length && !l.hiddenRows.length && !l.hiddenCols.length && !l.rowHeights.size && l.defaultRowHeight === null && !l.defaultColWidth && !l.filter && !l.freeze && !l.tabColor && !l.hidden && !l.veryHidden && !l.outline && !l.page && !l.protect && !l.unlockedRows.length && !l.unlockedCols.length && !l.tables.length && !l.charts.length && !l.pivots.length && !l.images.length && !l.notes.length);
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

  // The run that holds `i`, or null.
  function runAt(runs, i) {
    for (const run of runs) {
      if (run[0] > i) return null;
      if (run[1] >= i) return run;
    }
    return null;
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

  // Adds the merges of a pasted block in one pass and one version step. The rectangles do not overlap each other and lie where
  // the caller has just cleared (`removeMerges`), so none replaces another; a single cell or an upside-down rectangle is no
  // merge, and the sheet's bound ends the list.
  function addMerges(name, list) {
    const layout = entry(name);
    for (const m of list) {
      if (layout.merges.length >= MAX_MERGES) break;
      if (m.r2 >= m.r1 && m.c2 >= m.c1 && (m.r2 > m.r1 || m.c2 > m.c1)) layout.merges.push({ r1: m.r1, c1: m.c1, r2: m.r2, c2: m.c2 });
    }
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
    layout.rows = null;
    touch();
  }

  // ---- the rows the user or the filter hides ------------------------------------------------------
  // Sorted union of two sorted run lists.
  function unionRuns(a, b) {
    if (b.length === 0) return a;
    if (a.length === 0) return b;
    const out = [];
    let i = 0;
    let j = 0;
    while (i < a.length || j < b.length) {
      const next = j >= b.length || (i < a.length && a[i][0] <= b[j][0]) ? a[i++] : b[j++];
      const last = out[out.length - 1];
      if (last && next[0] <= last[1] + 1) last[1] = Math.max(last[1], next[1]);
      else out.push([next[0], next[1]]);
    }
    return out;
  }

  // Typed arrays over the union of the two kinds of hidden rows: every lookup is a binary search
  // (the filter can hide tens of thousands of separate runs, so a scan per row is not an option).
  function rowIndex(layout) {
    if (layout.rows) return layout.rows;
    const folded = layout.outline?.foldedRows || [];
    const tableRuns = (layout.tables || []).reduce((all, t) => unionRuns(all, t.filterHidden || []), []);
    const runs = unionRuns(unionRuns(unionRuns(layout.hiddenRows, layout.filter ? layout.filter.hidden : []), tableRuns), folded);
    const n = runs.length;
    const starts = new Int32Array(n);
    const ends = new Int32Array(n);
    const before = new Int32Array(n + 1);
    for (let k = 0; k < n; k += 1) {
      starts[k] = runs[k][0];
      ends[k] = runs[k][1];
      before[k + 1] = before[k] + (runs[k][1] - runs[k][0] + 1);
    }
    layout.rows = { n, starts, ends, before };
    return layout.rows;
  }

  // Number of runs whose first row is below `i` (`strict`) or at most `i`.
  function runsStartingBefore(ix, i, strict) {
    let lo = 0;
    let hi = ix.n;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (strict ? ix.starts[mid] < i : ix.starts[mid] <= i) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  const NO_ROWS = { n: 0, starts: new Int32Array(0), ends: new Int32Array(0), before: new Int32Array(1) };
  const NO_RUNS = { hidden: [], filtered: [] };
  const rowsOf = (name) => (bySheet[name] ? rowIndex(bySheet[name]) : NO_ROWS);

  // What the engine is told of the hidden rows of a sheet (PD-232): the rows a user hid or a folded group hid, and the rows the sheet's
  // filter and the tables' filters hid (1-based runs). `SUBTOTAL` and `AGGREGATE` leave them out. Held in the row index, which a
  // change of the layout may drop, so two answers are compared by `sameHiddenRuns`.
  function hiddenRunsOf(name) {
    const layout = bySheet[name];
    if (!layout) return NO_RUNS;
    const ix = rowIndex(layout);
    if (!ix.engine) {
      const tableRuns = (layout.tables || []).reduce((all, t) => unionRuns(all, t.filterHidden || []), []);
      ix.engine = { hidden: unionRuns(layout.hiddenRows, layout.outline?.foldedRows || []), filtered: unionRuns(layout.filter ? layout.filter.hidden : [], tableRuns) };
    }
    return ix.engine;
  }
  const sameRuns = (a, b) => a.length === b.length && a.every((run, i) => run[0] === b[i][0] && run[1] === b[i][1]);
  const sameHiddenRuns = (a, b) => a === b || (sameRuns(a.hidden, b.hidden) && sameRuns(a.filtered, b.filtered));

  // Rows hidden above row `i`.
  function rowsHiddenBefore(name, i) {
    const ix = rowsOf(name);
    const k = runsStartingBefore(ix, i, true);
    if (k === 0) return 0;
    const cut = ix.ends[k - 1] >= i ? ix.ends[k - 1] - i + 1 : 0;
    return ix.before[k] - cut;
  }

  const rowsHiddenTotal = (name) => rowsOf(name).before[rowsOf(name).n];

  // The row at 0-based visible position `v`.
  function rowAtVisible(name, v) {
    const ix = rowsOf(name);
    let lo = 0;
    let hi = ix.n;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (ix.starts[mid] - 1 - ix.before[mid] <= v) lo = mid + 1;
      else hi = mid;
    }
    return v + 1 + ix.before[lo];
  }

  function rowHidden(name, r) {
    const ix = rowsOf(name);
    const k = runsStartingBefore(ix, r, false);
    return k > 0 && ix.ends[k - 1] >= r;
  }
  // Columns the user hid and the ones a folded group hid.
  const hiddenColRuns = (name) => unionRuns(bySheet[name]?.hiddenCols || [], bySheet[name]?.outline?.foldedCols || []);
  const colHidden = (name, c) =>
    Boolean(bySheet[name]) && (runContains(bySheet[name].hiddenCols, c) || runContains(bySheet[name].outline?.foldedCols || [], c));

  // ---- JSON form (the page's save payload and the engine's `structure` answer) ---------------------
  function toJson(layout) {
    const out = {};
    if (layout.merges.length) {
      out.merges = layout.merges.map((m) => refOf(m.r1, m.c1) + ":" + refOf(m.r2, m.c2));
    }
    if (layout.hiddenRows.length) out.hiddenRows = layout.hiddenRows.map((r) => [r[0], r[1]]);
    if (layout.hiddenCols.length) out.hiddenCols = layout.hiddenCols.map((r) => [r[0], r[1]]);
    if (layout.rowHeights?.size) out.rowHeights = host.__EASYOFFICE_SHEET_ROW_HEIGHT__.toJson(layout.rowHeights);
    const defaultHeight = host.__EASYOFFICE_SHEET_ROW_HEIGHT__.defaultToJson(layout.defaultRowHeight);
    if (defaultHeight) out.defaultRowHeight = defaultHeight;
    if (layout.defaultColWidth) Object.assign(out, { defaultColWidth: { ...layout.defaultColWidth } }, layout.defaultColPx ? { defaultColPx: layout.defaultColPx } : null);
    if (layout.filter) out.filter = filterToJson(layout.filter);
    if (layout.freeze) out.freeze = { rows: layout.freeze.rows, cols: layout.freeze.cols };
    if (layout.tabColor) out.tabColor = layout.tabColor;
    if (layout.hidden) out.hidden = true;
    if (layout.veryHidden) out.veryHidden = true;
    if (layout.outline) out.outline = structuredClone(layout.outline);
    if (layout.page) out.page = structuredClone(layout.page);
    if (layout.protect) out.protect = structuredClone(layout.protect);
    if (layout.unlockedRows?.length) out.unlockedRows = layout.unlockedRows.map((r) => [r[0], r[1]]);
    if (layout.unlockedCols?.length) out.unlockedCols = layout.unlockedCols.map((r) => [r[0], r[1]]);
    if (layout.tables?.length) out.tables = structuredClone(layout.tables);
    if (layout.charts?.length) out.charts = structuredClone(layout.charts);
    if (layout.pivots?.length) out.pivots = structuredClone(layout.pivots);
    if (layout.images?.length) out.images = structuredClone(layout.images);
    if (layout.notes?.length) out.notes = structuredClone(layout.notes);
    return out;
  }

  function filterToJson(f) {
    const out = {
      range: refOf(f.range.r1, f.range.c1) + ":" + refOf(f.range.r2, f.range.c2),
      columns: structuredClone(f.columns),
    };
    if (f.hidden.length) out.hidden = f.hidden.map((r) => [r[0], r[1]]);
    if (f.hiddenButtons?.length) out.hiddenButtons = [...f.hiddenButtons];
    return out;
  }

  function filterFromJson(json) {
    const parse = host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef;
    const [a, b] = String(json?.range ?? "").split(":");
    const p = parse(a);
    const q = parse(b ?? "");
    if (!p || !q) return null;
    const out = {
      range: { r1: p.row, c1: p.col, r2: q.row, c2: q.col },
      columns: Array.isArray(json.columns) ? structuredClone(json.columns) : [],
      hidden: Array.isArray(json.hidden) ? json.hidden.map((r) => [r[0], r[1]]) : [],
    };
    if (Array.isArray(json.hiddenButtons) && json.hiddenButtons.length) out.hiddenButtons = [...json.hiddenButtons];
    return out;
  }

  function refOf(r, c) {
    return host.__EASYOFFICE_SHEET_GRID_UTILS__.refOf(r, c);
  }

  // The 22 px row and 72 px column fallbacks of the anchor readers below only stand in for a value a malformed anchor lacks (PD-299).
  function normalizeChartAnchor(raw) {
    const a = raw && typeof raw === "object" ? raw : {};
    const fcNum = Number(a.fromCol);
    const frNum = Number(a.fromRow);
    const fromCol = Math.max(0, Math.min(16383, Number.isFinite(fcNum) ? Math.trunc(fcNum) : 4));
    const fromRow = Math.max(0, Math.min(1048575, Number.isFinite(frNum) ? Math.trunc(frNum) : 1));
    const tcNum = Number(a.toCol);
    const trNum = Number(a.toRow);
    const toCol = Math.max(fromCol, Math.min(16383, Number.isFinite(tcNum) ? Math.trunc(tcNum) : fromCol + 6));
    const toRow = Math.max(fromRow, Math.min(1048575, Number.isFinite(trNum) ? Math.trunc(trNum) : fromRow + 11));
    const leftNum = Number(a.left);
    const topNum = Number(a.top);
    const left = Math.max(0, Math.min(50000000, Math.round(Number.isFinite(leftNum) ? leftNum : fromCol * DEFAULT_COL_PX)));
    const top = Math.max(0, Math.min(50000000, Math.round(Number.isFinite(topNum) ? topNum : fromRow * 22)));
    // A size from the engine is kept as it is: the lower limits of a chart a user drags apply to the drag only.
    const width = Math.max(1, Math.min(8192, Math.round(Number(a.width) || 420)));
    const height = Math.max(1, Math.min(8192, Math.round(Number(a.height) || 260)));
    return { fromCol, fromRow, ...emuOffsets(a), toCol, toRow, left, top, width, height, editAs: editAsOf(a.editAs) };
  }

  // What an anchor does when the rows and columns under it change: `twoCell` (moves and sizes with them, the default),
  // `oneCell` (moves, keeps its size) or `absolute` (stays). Any other value is `twoCell`.
  const editAsOf = (v) => (v === "oneCell" || v === "absolute" ? v : "twoCell");

  // The offsets of the two corners inside their cells, in EMU.
  function emuOffsets(a) {
    const emu = (v) => Math.max(0, Math.min(0x7fffffff, Math.round(Number(v) || 0)));
    return { colOffEmu: emu(a.colOffEmu), rowOffEmu: emu(a.rowOffEmu), toColOffEmu: emu(a.toColOffEmu), toRowOffEmu: emu(a.toRowOffEmu) };
  }

  function trimUtf8(str, maxBytes) {
    const s = String(str ?? "");
    if (s.length * 3 <= maxBytes) return s;
    const enc = new TextEncoder();
    if (enc.encode(s).length <= maxBytes) return s;
    let out = "";
    let used = 0;
    for (const ch of s) {
      const b = enc.encode(ch).length;
      if (used + b > maxBytes) break;
      out += ch;
      used += b;
    }
    return out;
  }

  function chartsFromJson(rawList) {
    if (rawList === undefined || rawList === null) return [];
    if (!Array.isArray(rawList)) throw new Error("차트 목록 형식이 올바르지 않습니다.");
    if (rawList.length > MAX_CHARTS_PER_SHEET) {
      throw new Error("시트당 차트 수 한도(" + MAX_CHARTS_PER_SHEET + "개)를 초과했습니다.");
    }
    return rawList.map((item, idx) => {
      if (!item || typeof item !== "object") throw new Error("차트 항목이 올바르지 않습니다.");
      const chartType = String(item.chartType || "");
      if (!CHART_TYPE_SET.has(chartType)) {
        throw new Error("지원하지 않는 차트 종류입니다: " + chartType);
      }
      let series;
      if (item.series !== undefined && item.series !== null) {
        if (!Array.isArray(item.series)) throw new Error("차트 계열 형식이 올바르지 않습니다.");
        if (item.series.length > MAX_CHART_SERIES) {
          throw new Error("차트 계열 수 한도(" + MAX_CHART_SERIES + "개)를 초과했습니다.");
        }
        series = item.series.map((s) => {
          if (!s || typeof s !== "object") throw new Error("차트 계열 항목이 올바르지 않습니다.");
          const out = {};
          if (typeof s.name === "string") out.name = trimUtf8(s.name, 512);
          if (typeof s.nameRef === "string" && s.nameRef.trim()) out.nameRef = trimUtf8(s.nameRef.trim(), 512);
          if (typeof s.categoriesRef === "string" && s.categoriesRef.trim()) out.categoriesRef = trimUtf8(s.categoriesRef.trim(), 512);
          if (typeof s.valuesRef === "string" && s.valuesRef.trim()) out.valuesRef = trimUtf8(s.valuesRef.trim(), 512);
          return out;
        });
      }
      const out = {
        id: trimUtf8(item.id || "chart-" + (idx + 1), 128),
        chartType,
        title: trimUtf8(item.title ?? "", 512),
        dataRange: trimUtf8(String(item.dataRange ?? "").trim(), 512),
        switchRowCol: item.switchRowCol === true,
        legend: CHART_LEGEND_SET.has(item.legend) ? item.legend : "right",
        anchor: normalizeChartAnchor(item.anchor),
      };
      if (series && series.length > 0) out.series = series;
      return out;
    });
  }

  const PNG_SIG = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52];
  const u32be = (u8, at) => (((u8[at] << 24) >>> 0) + (u8[at + 1] << 16) + (u8[at + 2] << 8) + u8[at + 3]) >>> 0;

  function parseImageHeader(u8) {
    if (u8.length >= 24 && PNG_SIG.every((b, i) => u8[i] === b)) {
      const widthPx = u32be(u8, 16), heightPx = u32be(u8, 20);
      return widthPx >= 1 && widthPx <= 65535 && heightPx >= 1 && heightPx <= 65535 ? { format: "png", widthPx, heightPx } : null;
    }
    if (u8.length < 4 || u8[0] !== 0xff || u8[1] !== 0xd8 || u8[2] !== 0xff) return null;
    for (let pos = 2; pos < u8.length;) {
      while (pos < u8.length && u8[pos] === 0xff) pos += 1;
      if (pos >= u8.length) return null;
      const m = u8[pos++];
      if (m === 0xd9 || m === 0xda) return null;
      if (m === 0x01 || (m >= 0xd0 && m <= 0xd8)) continue;
      if (pos + 2 > u8.length) return null;
      const len = (u8[pos] << 8) | u8[pos + 1];
      if (len < 2 || pos + len > u8.length) return null;
      if (m === 0xc0 || m === 0xc1 || m === 0xc2) {
        if (len < 7) return null;
        const heightPx = (u8[pos + 3] << 8) | u8[pos + 4], widthPx = (u8[pos + 5] << 8) | u8[pos + 6];
        return widthPx >= 1 && heightPx >= 1 ? { format: "jpg", widthPx, heightPx } : null;
      }
      pos += len;
    }
    return null;
  }

  function normalizeImageAnchor(raw, fallbackW, fallbackH) {
    const a = raw && typeof raw === "object" ? raw : {};
    const fromCol = Math.max(0, Math.min(16383, Number.isFinite(Number(a.fromCol)) ? Math.trunc(Number(a.fromCol)) : 0));
    const fromRow = Math.max(0, Math.min(1048575, Number.isFinite(Number(a.fromRow)) ? Math.trunc(Number(a.fromRow)) : 0));
    const colOffEmu = Math.max(0, Math.min(0x7fffffff, Math.round(Number(a.colOffEmu) || 0)));
    const rowOffEmu = Math.max(0, Math.min(0x7fffffff, Math.round(Number(a.rowOffEmu) || 0)));
    const widthPx = Math.max(1, Math.min(4096, Math.round(Number(a.widthPx ?? a.width) || fallbackW || 240)));
    const heightPx = Math.max(1, Math.min(4096, Math.round(Number(a.heightPx ?? a.height) || fallbackH || 160)));
    const leftRaw = a.leftPx !== undefined ? Number(a.leftPx) : Number(a.left);
    const topRaw = a.topPx !== undefined ? Number(a.topPx) : Number(a.top);
    const leftPx = Math.max(0, Math.min(50000000, Math.round(Number.isFinite(leftRaw) ? leftRaw : fromCol * DEFAULT_COL_PX + Math.round(colOffEmu / EMU_PER_PX))));
    const topPx = Math.max(0, Math.min(50000000, Math.round(Number.isFinite(topRaw) ? topRaw : fromRow * 22 + Math.round(rowOffEmu / EMU_PER_PX))));
    const toCol = Math.max(fromCol, Math.min(16383, Number.isFinite(Number(a.toCol)) ? Math.trunc(Number(a.toCol)) : Math.min(16383, Math.floor((leftPx + widthPx) / DEFAULT_COL_PX))));
    const toRow = Math.max(fromRow, Math.min(1048575, Number.isFinite(Number(a.toRow)) ? Math.trunc(Number(a.toRow)) : Math.min(1048575, Math.floor((topPx + heightPx) / 22))));
    const toColOffEmu = Math.max(0, Math.min(0x7fffffff, Math.round(Number(a.toColOffEmu) || 0)));
    const toRowOffEmu = Math.max(0, Math.min(0x7fffffff, Math.round(Number(a.toRowOffEmu) || 0)));
    return { fromCol, fromRow, colOffEmu, rowOffEmu, toCol, toRow, toColOffEmu, toRowOffEmu, leftPx, topPx, widthPx, heightPx, editAs: editAsOf(a.editAs) };
  }

  function imagesFromJson(rawList) {
    if (rawList === undefined || rawList === null) return [];
    if (!Array.isArray(rawList)) throw new Error("그림 목록 형식이 올바르지 않습니다.");
    if (rawList.length > MAX_IMAGES_PER_SHEET) throw new Error("시트당 그림 수 한도(" + MAX_IMAGES_PER_SHEET + "개)를 초과했습니다.");
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    return rawList.map((item, idx) => {
      if (!item || typeof item !== "object") throw new Error("그림 항목이 올바르지 않습니다.");
      let mime = item.mime === "image/jpeg" || item.mime === "image/jpg" ? "image/jpeg" : item.mime === "image/png" || (!item.mime && item.imageId) ? "image/png" : null;
      if (!mime) throw new Error("지원하지 않는 그림 형식입니다: " + String(item.mime || ""));
      let imageId = typeof item.imageId === "string" && item.imageId ? item.imageId : "";
      let naturalWidthPx = Math.max(1, Math.round(Number(item.naturalWidthPx) || 0));
      let naturalHeightPx = Math.max(1, Math.round(Number(item.naturalHeightPx) || 0));
      if (typeof item.data === "string" && item.data.length > 0) {
        const u8 = utils.base64ToBytes(item.data);
        if (u8.length === 0 || u8.length > MAX_SINGLE_IMAGE_BYTES) throw new Error("그림 크기가 허용 범위를 벗어났습니다.");
        const hdr = parseImageHeader(u8);
        if (!hdr) throw new Error("손상되었거나 지원하지 않는 그림 데이터입니다.");
        mime = hdr.format === "jpg" ? "image/jpeg" : "image/png";
        imageId = utils.registerDocImage(item.data, hdr.format);
        naturalWidthPx = hdr.widthPx;
        naturalHeightPx = hdr.heightPx;
      } else if (!imageId) {
        throw new Error("그림 데이터가 비어 있습니다.");
      } else {
        if (!Number(item.naturalWidthPx)) naturalWidthPx = Math.max(1, Math.round(Number(item.anchor?.widthPx) || 240));
        if (!Number(item.naturalHeightPx)) naturalHeightPx = Math.max(1, Math.round(Number(item.anchor?.heightPx) || 160));
      }
      return {
        id: trimUtf8(item.id || "pic-" + (idx + 1), 128),
        name: trimUtf8(item.name || "그림 " + (idx + 1), 256),
        alt: trimUtf8(item.alt ?? "", 1024),
        mime,
        imageId,
        naturalWidthPx,
        naturalHeightPx,
        anchor: normalizeImageAnchor(item.anchor, naturalWidthPx, naturalHeightPx),
      };
    });
  }

  function notesFromJson(rawList) {
    if (rawList === undefined || rawList === null) return [];
    if (!Array.isArray(rawList)) throw new Error("셀 메모 목록 형식이 올바르지 않습니다.");
    if (rawList.length > MAX_NOTES_PER_SHEET) throw new Error("시트당 셀 메모 수 한도(" + MAX_NOTES_PER_SHEET + "개)를 초과했습니다.");
    const enc = new TextEncoder();
    const byRef = new Map();
    for (const item of rawList) {
      if (!item || typeof item !== "object") throw new Error("셀 메모 항목이 올바르지 않습니다.");
      const p = parseRef1(String(item.ref ?? "").trim());
      if (!p) throw new Error("유효하지 않은 셀 메모 참조입니다: " + String(item.ref ?? ""));
      const author = String(item.author ?? "");
      const text = String(item.text ?? "");
      if (enc.encode(author).length > MAX_NOTE_AUTHOR_BYTES) throw new Error("메모 작성자 길이가 256바이트 한도를 초과했습니다.");
      if (enc.encode(text).length > MAX_NOTE_TEXT_BYTES) throw new Error("메모 본문 길이가 32768바이트 한도를 초과했습니다.");
      const normRef = refOf(p.row, p.col);
      byRef.set(normRef, { ref: normRef, author, text, visible: item.visible === true });
    }
    return [...byRef.values()];
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
    layout.rowHeights = host.__EASYOFFICE_SHEET_ROW_HEIGHT__.fromJson(json?.rowHeights);
    layout.defaultRowHeight = host.__EASYOFFICE_SHEET_ROW_HEIGHT__.defaultFromJson(json?.defaultRowHeight);
    layout.defaultColWidth = json?.defaultColWidth && typeof json.defaultColWidth === "object" ? { ...json.defaultColWidth } : null; // XLSX `defaultColWidth`, `baseColWidth`: kept so a save writes them back (PD-303)
    layout.defaultColPx = layout.defaultColWidth && Number.isInteger(json.defaultColPx) && json.defaultColPx >= MIN_COL_PX && json.defaultColPx <= MAX_DEFAULT_COL_PX ? json.defaultColPx : null; // the width the engine worked out for them (PD-305)
    layout.filter = json?.filter ? filterFromJson(json.filter) : null;
    layout.freeze = normalizeFreeze(json?.freeze);
    layout.tabColor = /^[0-9a-f]{6}$/i.test(json?.tabColor || "") ? json.tabColor.toLowerCase() : null;
    layout.veryHidden = json?.veryHidden === true;
    layout.hidden = json?.hidden === true || layout.veryHidden;
    layout.outline = json?.outline && Object.keys(json.outline).length > 0 ? structuredClone(json.outline) : null;
    layout.page = json?.page && Object.keys(json.page).length > 0 ? structuredClone(json.page) : null;
    layout.protect = json?.protect && typeof json.protect === "object" ? structuredClone(json.protect) : null;
    layout.unlockedRows = runs(json?.unlockedRows);
    layout.unlockedCols = runs(json?.unlockedCols);
    layout.tables = Array.isArray(json?.tables) ? structuredClone(json.tables) : [];
    layout.charts = chartsFromJson(json?.charts);
    layout.pivots = host.__EASYOFFICE_SHEET_PIVOT_MODEL__
      ? host.__EASYOFFICE_SHEET_PIVOT_MODEL__.pivotsFromJson(json?.pivots)
      : Array.isArray(json?.pivots)
        ? structuredClone(json.pivots)
        : [];
    layout.images = imagesFromJson(json?.images);
    layout.notes = notesFromJson(json?.notes);
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
    const nextMap = Object.create(null);
    for (const name of Object.keys(all || {})) {
      const layout = fromJson(all[name]);
      if (!isEmpty(layout)) nextMap[name] = layout;
    }
    for (const name of Object.keys(bySheet)) delete bySheet[name];
    Object.assign(bySheet, nextMap);
    touch();
  }

  // Replaces one sheet's layout with the JSON the engine answered.
  function setSheet(name, json) {
    const prev = bySheet[name];
    const prevCharts = json && json.charts === undefined ? prev?.charts : undefined;
    const prevPivots = json && json.pivots === undefined ? prev?.pivots : undefined;
    const prevImages = json && json.images === undefined ? prev?.images : undefined;
    const prevNotes = json && json.notes === undefined ? prev?.notes : undefined;
    // The engine's answer to a row or column edit does not carry the default row height: the sheet keeps its own.
    const prevDefault = json && json.defaultRowHeight === undefined ? prev?.defaultRowHeight : null;
    let hydrated = json;
    if (Array.isArray(json?.images) && prev?.images?.length) {
      const byId = new Map(prev.images.map((im) => [im.id, im]));
      hydrated = {
        ...json,
        images: json.images.map((im) => {
          if (!im || typeof im !== "object" || im.imageId || im.data) return im;
          const old = byId.get(im.id);
          return old ? { imageId: old.imageId, naturalWidthPx: old.naturalWidthPx, naturalHeightPx: old.naturalHeightPx, ...im } : im;
        }),
      };
    }
    const layout = fromJson(hydrated);
    if (prevDefault) layout.defaultRowHeight = prevDefault;
    if (prevCharts?.length && layout.charts.length === 0) layout.charts = structuredClone(prevCharts);
    if (prevPivots?.length && layout.pivots.length === 0) layout.pivots = structuredClone(prevPivots);
    if (prevImages?.length && layout.images.length === 0) layout.images = structuredClone(prevImages);
    // A fresh layout has no note index (`noteMap` is null), so the kept notes need none either.
    if (prevNotes?.length && layout.notes.length === 0) layout.notes = structuredClone(prevNotes);
    if (isEmpty(layout)) delete bySheet[name];
    else bySheet[name] = layout;
    touch();
  }

  const chartsOf = (name) => bySheet[name]?.charts || [];
  function setCharts(name, charts) {
    const next = chartsFromJson(charts);
    const layout = entry(name);
    layout.charts = next;
    if (isEmpty(layout)) delete bySheet[name];
    touch();
  }

  const imagesOf = (name) => bySheet[name]?.images || [];
  function setImages(name, images) {
    const next = imagesFromJson(images);
    const layout = entry(name);
    layout.images = next;
    if (isEmpty(layout)) delete bySheet[name];
    touch();
  }

  const notesOf = (name) => bySheet[name]?.notes || [];
  function setNotes(name, notes) {
    const next = notesFromJson(notes);
    const layout = entry(name);
    layout.notes = next;
    layout.noteMap = null;
    if (isEmpty(layout)) delete bySheet[name];
    touch();
  }

  function noteAtRef(name, ref) {
    const layout = bySheet[name];
    if (!layout || layout.notes.length === 0 || !ref) return null;
    let map = layout.noteMap;
    if (!map) {
      map = new Map();
      for (let i = 0; i < layout.notes.length; i += 1) map.set(layout.notes[i].ref, layout.notes[i]);
      layout.noteMap = map;
    }
    const key = host.__EASYOFFICE_SHEET_UTILS__ ? host.__EASYOFFICE_SHEET_UTILS__.asciiUpperJs(ref) : String(ref).toUpperCase();
    return map.get(key) || null;
  }

  const pivotBoxes = new WeakMap();
  const pivotsOf = (name) => bySheet[name]?.pivots || [];
  function setPivots(name, pivots) {
    const model = host.__EASYOFFICE_SHEET_PIVOT_MODEL__;
    const next = model ? model.pivotsFromJson(pivots) : structuredClone(pivots || []);
    const layout = entry(name);
    layout.pivots = next;
    if (isEmpty(layout)) delete bySheet[name];
    touch();
  }

  function parseRef1(refText) {
    const gridParse = host.__EASYOFFICE_SHEET_GRID_UTILS__?.parseRef;
    if (gridParse) return gridParse(refText);
    const p = host.__EASYOFFICE_SHEET_RULES__?.parseCell(refText);
    return p ? { row: p.row + 1, col: p.col + 1 } : null;
  }

  function pivotBox(pivot) {
    const loc = String(pivot?.location || "");
    const held = pivot && typeof pivot === "object" ? pivotBoxes.get(pivot) : null;
    if (held && held.location === loc) return held;
    const [rawA, rawB] = loc.split(":");
    const a = parseRef1(rawA);
    const b = parseRef1(rawB ?? rawA);
    const box = a && b
      ? { location: loc, r1: Math.min(a.row, b.row), c1: Math.min(a.col, b.col), r2: Math.max(a.row, b.row), c2: Math.max(a.col, b.col) }
      : { location: loc, r1: 1, c1: 1, r2: 0, c2: 0 };
    if (pivot && typeof pivot === "object") pivotBoxes.set(pivot, box);
    return box;
  }

  function pivotsIn(name, r1, c1, r2, c2) {
    return pivotsOf(name).filter((p) => {
      const b = pivotBox(p);
      return b.r2 >= b.r1 && b.r1 <= r2 && b.r2 >= r1 && b.c1 <= c2 && b.c2 >= c1;
    });
  }

  const pivotAt = (name, row1, col1) => pivotsIn(name, row1, col1, row1, col1)[0] || null;

  // ---- the auto filter ---------------------------------------------------------------------------
  const filterOf = (name) => bySheet[name]?.filter || null;

  // Sets (or with null removes) the sheet's filter.
  function setFilter(name, filter) {
    const layout = entry(name);
    layout.filter = filter;
    layout.rows = null;
    if (isEmpty(layout)) delete bySheet[name];
    touch();
  }

  // Sets the rows (1-based runs) the conditions of a table's own filter hide (PD-226).
  function setTableFilterHidden(name, tableName, runs) {
    const layout = bySheet[name];
    const table = layout?.tables?.find((t) => t.name === tableName);
    if (!table) return false;
    if (runs.length) table.filterHidden = runs.map((r) => [r[0], r[1]]);
    else delete table.filterHidden;
    layout.rows = null;
    touch();
    return true;
  }

  // Sets the conditions of a table's own filter (a list, empty for buttons without conditions; `null` for a table
  // without filter buttons) and the rows they hide.
  function setTableFilter(name, tableName, columns, runs) {
    const layout = bySheet[name];
    const table = layout?.tables?.find((t) => t.name === tableName);
    if (!table) return false;
    if (columns === null) {
      delete table.filter;
      delete table.hiddenButtons;
    } else {
      table.filter = columns;
    }
    if (runs?.length) table.filterHidden = runs.map((r) => [r[0], r[1]]);
    else delete table.filterHidden;
    layout.rows = null;
    touch();
    return true;
  }

  // Who owns the filter button of the header cell `row`, `col` of a sheet (PD-232, PD-237): a table's name for a table with filter buttons whose
  // header row has the cell (unless `hiddenButtons` hides that column's button), `null` for the sheet's filter when its header row has it
  // (unless `hiddenButtons` hides it), `undefined` for a cell with no button. The grid draws the button and the panel opens by this answer.
  function filterOwnerAt(name, row, col) {
    const table = host.__EASYOFFICE_SHEET_TABLE_STYLE__?.tableAt(name, row, col);
    if (table && table.header !== false && Array.isArray(table.filter) && tableBox(table).r1 === row) {
      return table.hiddenButtons?.includes(col) ? undefined : table.name;
    }
    const f = bySheet[name]?.filter;
    if (f && row === f.range.r1 && col >= f.range.c1 && col <= f.range.c2) {
      return f.hiddenButtons?.includes(col) ? undefined : null;
    }
    return undefined;
  }

  // Clears the conditions of a table's own filter and the rows they hide (the table keeps its filter buttons).
  // True when there was anything to clear.
  function clearTableFilter(name, tableName) {
    const layout = bySheet[name];
    const table = layout?.tables?.find((t) => t.name === tableName);
    if (!table || (!table.filter?.length && !table.filterHidden?.length)) return false;
    if (table.filter) table.filter = [];
    delete table.filterHidden;
    layout.rows = null;
    touch();
    return true;
  }

  // ---- the tables (PD-224) ----------------------------------------------------------------------------------------
  const tablesOf = (name) => bySheet[name]?.tables || [];

  // A table's rectangle `{ range, r1, c1, r2, c2 }` (1-based), parsed once per range text; `r2 < r1` for a range that is none.
  const boxes = new WeakMap();
  function tableBox(table) {
    const held = boxes.get(table);
    if (held && held.range === table.range) return held;
    const parse = host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef;
    const [a, b] = String(table.range).split(":").map((ref) => parse(ref));
    const box = a && b ? { range: table.range, r1: a.row, c1: a.col, r2: b.row, c2: b.col } : { range: table.range, r1: 1, c1: 1, r2: 0, c2: 0 };
    boxes.set(table, box);
    return box;
  }

  // The tables of the sheet that share a cell with the rectangle `r1..r2` by `c1..c2`.
  function tablesIn(name, r1, c1, r2, c2) {
    return tablesOf(name).filter((t) => {
      const b = tableBox(t);
      return b.r2 >= b.r1 && b.r1 <= r2 && b.r2 >= r1 && b.c1 <= c2 && b.c2 >= c1;
    });
  }

  // Replaces the tables of a sheet (the page's own changes to them: a new table, a new style, a new size); one version step.
  function setTables(name, tables) {
    const layout = entry(name);
    layout.tables = tables;
    layout.rows = null;
    if (isEmpty(layout)) delete bySheet[name];
    touch();
  }

  // A header cell holds the text of its name: a formula typed there is fixed as the text of its result, as the
  // spreadsheet program does. `map` is the sheet's cell model, whose formula cells already hold their values.
  // True when a cell changed.
  function settleTableHeaders(map, name) {
    let changed = false;
    for (const t of tablesIn(name, 1, 1, 1048576, 16384)) {
      if (!t.header || !map) continue;
      const [from, to] = String(t.range).split(":").map((ref) => host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef(ref));
      for (let c = from.col; c <= to.col; c += 1) {
        const entry = map[refOf(from.row, c)];
        if (entry && String(entry.formula || "").startsWith("=")) {
          entry.formula = "";
          changed = true;
        }
      }
    }
    return changed;
  }

  // ---- the column names of a table follow its header cells (PD-225) ----------------------------------------------
  // `columns` of a table is what structured references are resolved against. A header cell the user changes names its
  // column anew: the list says which columns got a new name, the page rewrites the formulas that name them with the engine
  // (`tableColumn`) and then sets the name here. A header seen for the first time (a file just opened) only becomes the
  // baseline, and a header that is empty, a formula, over 255 characters or the name of another column of the table is left
  // to the engine, which repairs such names when the file is written.
  const seenHeaders = new Map();

  function tableColumnRenames(stores) {
    const renames = [];
    const parse = host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef;
    for (const name of Object.keys(bySheet)) {
      const map = stores?.[name];
      for (const t of tablesOf(name)) {
        if (!t.header || !map) continue;
        const from = parse(String(t.range).split(":")[0]);
        if (!from) continue;
        t.columns.forEach((column, i) => {
          const cell = map[refOf(from.row, from.col + i)];
          const text = String(cell?.value ?? "");
          const key = name + "|" + t.name + "|" + i;
          const before = seenHeaders.get(key);
          seenHeaders.set(key, text);
          const same = (other) => other.toLowerCase() === text.toLowerCase();
          if (before === undefined || text === before || text === column || text === "" || String(cell?.formula || "").startsWith("=")) return;
          if (Array.from(text).length > MAX_TABLE_NAME || t.columns.some((other, j) => j !== i && same(other))) return;
          renames.push({ sheet: name, table: t.name, index: i, from: column, to: text });
        });
      }
    }
    return renames;
  }

  // Forgets the header texts seen: a document opened or closed starts its tables afresh (the next look is a baseline).
  const resetTableHeaders = () => seenHeaders.clear();

  // Gives column `index` of the table the name `columnName`; true when it changed.
  function setTableColumn(sheet, table, index, columnName) {
    const t = tablesOf(sheet).find((x) => x.name === table);
    if (!t || index < 0 || index >= t.columns.length || t.columns[index] === columnName) return false;
    t.columns[index] = columnName;
    touch();
    return true;
  }

  // The table of that name on any sheet, or the table of `sheet` that has the cell (1-based row and column).
  function tableNamed(name) {
    const folded = String(name).toLowerCase();
    for (const sheet of Object.keys(bySheet)) {
      const found = tablesOf(sheet).find((t) => String(t.name).toLowerCase() === folded);
      if (found) return found;
    }
    return null;
  }

  const tableAt = (sheet, row, col) => tablesIn(sheet, row, col, row, col)[0] || null;

  const MAX_TABLE_NAME = 255;

  // `tables` of a copied sheet with names that no table of `layouts` (sheet name -> layout JSON) has: `Name_2`, `Name_3`, ...
  // cut so that the name keeps within the 255 characters a file allows.
  function copiedTables(tables, layouts) {
    const taken = new Set();
    for (const json of Object.values(layouts || {})) for (const t of json?.tables || []) taken.add(String(t.name).toLowerCase());
    return structuredClone(tables).map((t) => {
      let name = t.name;
      for (let n = 2; taken.has(name.toLowerCase()); n += 1) {
        const suffix = "_" + n;
        name = Array.from(t.name).slice(0, MAX_TABLE_NAME - suffix.length).join("") + suffix;
      }
      taken.add(name.toLowerCase());
      return { ...t, name };
    });
  }

  // ---- the view: frozen panes, tab colour and groups (PD-208) -------------------------------------
  const freezeOf = (name) => bySheet[name]?.freeze || null;
  const tabColorOf = (name) => bySheet[name]?.tabColor || null;
  const outlineOf = (name) => bySheet[name]?.outline || null;
  const hiddenOf = (name) => bySheet[name]?.hidden === true;
  const veryHiddenOf = (name) => bySheet[name]?.veryHidden === true;

  function setView(name, key, value) {
    const layout = entry(name);
    layout[key] = value;
    layout.rows = null;
    if (isEmpty(layout)) delete bySheet[name];
    touch();
  }

  const freezeAxis = (v) => (Number.isFinite(Number(v)) && Number(v) > 0 ? Math.trunc(Number(v)) : 0);
  const normalizeFreeze = (f) => { const rows = freezeAxis(f?.rows), cols = freezeAxis(f?.cols); return rows > 0 || cols > 0 ? { rows, cols } : null; };
  const setFreeze = (name, freeze) => setView(name, "freeze", normalizeFreeze(freeze));
  // The colour of the sheet's tab as "rrggbb", null for none (PD-229).
  const setTabColor = (name, rgb) => setView(name, "tabColor", /^[0-9a-f]{6}$/i.test(rgb || "") ? rgb.toLowerCase() : null);
  const setHiddenSheet = (name, hidden) => setView(name, "hidden", hidden === true);
  // The page setup of the sheet (`sheet-page.js`): the engine's `page` JSON, null for the default.
  const pageOf = (name) => bySheet[name]?.page || null;
  const setPage = (name, page) => setView(name, "page", page && Object.keys(page).length > 0 ? structuredClone(page) : null);
  // The protection of the sheet (`sheet-protect.js`): the engine's `protect` JSON, null for none.
  const protectOf = (name) => bySheet[name]?.protect || null;
  const setProtect = (name, protect) => setView(name, "protect", protect ? structuredClone(protect) : null);
  // Rows and columns whose own format is unlocked, as runs.
  const unlockedOf = (name, axis) => bySheet[name]?.[axis === "row" ? "unlockedRows" : "unlockedCols"] || [];
  const setUnlocked = (name, axis, runs) => setView(name, axis === "row" ? "unlockedRows" : "unlockedCols", runs.map((r) => [r[0], r[1]]));
  // The heights a user set (`sheet-row-height.js`): a `Map` of the row to points, replaced whole.
  const rowHeightsOf = (name) => bySheet[name]?.rowHeights || null;
  const setRowHeights = (name, map) => setView(name, "rowHeights", map);
  // The default row height in points (`sheet-row-height.js`), null for the grid's own: the height of every row with none of its own.
  const defaultRowHeightOf = (name) => bySheet[name]?.defaultRowHeight ?? null;
  const setDefaultRowHeight = (name, pt) => setView(name, "defaultRowHeight", host.__EASYOFFICE_SHEET_ROW_HEIGHT__.defaultFromJson([pt]));
  const setOutline = (name, outline) => setView(name, "outline", outline && Object.keys(outline).length > 0 ? outline : null);

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
    if (edit.axis === "row" && edit.cols && (p.col < edit.cols[0] || p.col > edit.cols[1])) return ref;
    const row = edit.axis === "row" ? mapIndex(edit, p.row) : p.row;
    const col = edit.axis === "col" ? mapIndex(edit, p.col) : p.col;
    return row === null || col === null ? null : refOf(row, col);
  }

  host.__EASYOFFICE_SHEET_LAYOUT__ = {
    mapIndex, remapRef, get: (name) => bySheet[name] || null, version: () => version, onChange: (fn) => listeners.push(fn),
    isEmpty, runContains, runAt, addRun, removeRun, hiddenBefore, hiddenTotal, visibleToIndex,
    mergeAt, mergesIn, addMerge, addMerges, removeMerges, setHidden, rowHidden, rowsHiddenBefore, rowsHiddenTotal, rowAtVisible,
    tablesOf, tablesIn, tableBox, setTables, settleTableHeaders, tableColumnRenames, resetTableHeaders, tableNamed, tableAt, setTableColumn, copiedTables,
    filterOf, setFilter, setTableFilterHidden, setTableFilter, filterOwnerAt, hiddenRunsOf, sameHiddenRuns, NO_RUNS, clearTableFilter, filterToJson,
    colHidden, hiddenColRuns, freezeOf, setFreeze, tabColorOf, setTabColor, hiddenOf, veryHiddenOf, setHiddenSheet,
    outlineOf, setOutline, rowHeightsOf, setRowHeights, defaultRowHeightOf, setDefaultRowHeight, defaultColPxOf: (name) => bySheet[name]?.defaultColPx || host.__EASYOFFICE_SHEET_DEFAULT_FONT__?.colPx() || DEFAULT_COL_PX, DEFAULT_COL_PX, pageOf, setPage, protectOf, setProtect, unlockedOf, setUnlocked,
    getAll, replaceAll, setSheet, toJson, chartsOf, setCharts, pivotsOf, setPivots, pivotBox, pivotsIn, pivotAt,
    imagesOf, setImages, notesOf, setNotes, noteAtRef, parseImageHeader, normalizeImageAnchor, trimUtf8,
    MAX_MERGES, MAX_CHARTS_PER_SHEET, MAX_CHART_SERIES, CHART_TYPES, CHART_LEGENDS,
    MAX_IMAGES_PER_SHEET, MAX_IMAGES_WORKBOOK, MAX_SINGLE_IMAGE_BYTES, MAX_TOTAL_IMAGE_BYTES,
    MAX_NOTES_PER_SHEET, MAX_NOTES_WORKBOOK, MAX_NOTE_AUTHOR_BYTES, MAX_NOTE_TEXT_BYTES, EMU_PER_PX,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
