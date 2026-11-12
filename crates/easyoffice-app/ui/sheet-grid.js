(function (host) {
  "use strict";

  // Virtualized spreadsheet grid. The model lives in `sheetDataStore`; this module only
  // materializes the cells around the viewport (plus a small overscan) as real table rows.
  // Logical addressing covers the full 1,048,576 x 16,384 sheet while the DOM stays small.
  const MAX_ROWS = 1048576;
  const MAX_COLS = 16384;
  const ROW_H = 24;
  const HEAD_H = 26;
  const ROW_HEAD_W = 56;
  const DEFAULT_COL_W = 96;
  const MIN_COL_W = 36;
  const MAX_COL_W = 720;
  const OVERSCAN_ROWS = 8;
  const OVERSCAN_COLS = 2;
  // Without layout metrics (hidden pane, headless tests) a bounded window is rendered instead.
  const FALLBACK_ROWS = 60;
  const FALLBACK_COLS = 40;
  const DEFAULT_COLS = 8;
  const MIN_ROWS = 12;

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

  const { normalizeFormattedNumericCell } = host.__EASYOFFICE_SHEET_UTILS__;
  // Rows beyond each edge of the window whose formatted text is fetched ahead of a scroll.
  const NUMFMT_PREFETCH_ROWS = 60;
  const NUMERIC_RE = /^-?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;

  function isNumericText(text) {
    const t = normalizeFormattedNumericCell(text);
    return t !== "" && NUMERIC_RE.test(t);
  }

  // "General" display: long decimals (float noise such as 0.30000000000000004 or
  // 1266.66666666667) show at most 10 significant digits; the raw value is kept for editing.
  function formatGeneral(raw) {
    const t = String(raw ?? "");
    if (!/^-?\d*\.\d+$/.test(t)) return t;
    const digits = t.replace(/[-.]/g, "").replace(/^0+/, "");
    if (digits.length <= 10) return t;
    const n = Number(t);
    return Number.isFinite(n) ? String(Number(n.toPrecision(10))) : t;
  }

  // Copies a workbook model (sheet -> cell -> {value, formula, style}) ~4x faster than a JSON
  // round-trip, which matters for undo snapshots of sheets with tens of thousands of cells. A frozen
  // (shared) style object is kept as it is.
  function cloneSheetStore(store) {
    const out = {};
    Object.keys(store || {}).forEach((name) => {
      const src = store[name] || {};
      const dst = {};
      Object.keys(src).forEach((ref) => {
        const e = src[ref];
        if (!e) return;
        const copy = { ...e };
        if (e.style) copy.style = Object.isFrozen(e.style) ? e.style : { ...e.style };
        dst[ref] = copy;
      });
      out[name] = dst;
    });
    return out;
  }

  function hasLayout(vp) {
    return Boolean(vp) && vp.clientHeight > 0 && vp.clientWidth > 0;
  }

  function rowOf(tr) {
    if (tr?.tagName !== "TR") return 0;
    const raw = tr.dataset.row || tr.querySelector("th[data-row]")?.dataset.row;
    return Number(raw) || 0;
  }

  // Width of `text` in the cell's font, from one shared canvas; -1 when the page cannot measure.
  let measurer;
  function textWidth(td, text) {
    if (measurer === undefined) {
      try {
        measurer = td.ownerDocument.createElement("canvas").getContext("2d") || null;
      } catch {
        measurer = null;
      }
    }
    if (!measurer) return -1;
    if (!measurer.eoFamily) {
      const cs = host.getComputedStyle?.(td);
      if (!cs?.fontFamily) return -1;
      measurer.eoFamily = cs.fontFamily;
      measurer.eoSize = cs.fontSize;
    }
    measurer.font = `${td.style.fontStyle || "normal"} ${td.style.fontWeight || "400"} ${td.style.fontSize || measurer.eoSize} ${measurer.eoFamily}`;
    return measurer.measureText(text).width;
  }

  // Width of a column in px (set by the grid that owns the page's sheet).
  let columnWidthOf = null;

  // A formatted number wider than its cell shows as hash marks (padding 6px each side, 1px border).
  function fitToCell(td, text) {
    const room = td._col > 0 && columnWidthOf ? columnWidthOf(td._col) - 13 : NaN;
    if (!(room > 0)) return text;
    const fit = host.__EASYOFFICE_SHEET_NUMFMT__?.fitNumber;
    return fit ? fit(text, room, (t) => textWidth(td, t)) : text;
  }

  // Shows `value` with the cell's number format (`td._nf`, PD-193) or else the General format; the
  // raw value is remembered for editing and saving. The section color of a format rides on a custom
  // property, so it never lands in the inline font properties the sheet model reads back; the
  // common red is the theme's negative color.
  const SECTION_RED = "#ff0000";

  function setDisplayValue(td, value) {
    const raw = String(value ?? "");
    const nf = td._nf;
    const formatted = nf ? host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__?.shown(nf, raw) : null;
    let shown = formatted ? formatted.t : formatGeneral(raw);
    if (formatted && nf !== "@" && isNumericText(raw)) shown = fitToCell(td, shown);
    if (td.textContent !== shown) td.textContent = shown;
    if (shown === raw) {
      if (td.dataset.raw !== undefined) {
        delete td.dataset.raw;
        delete td.dataset.display;
      }
    } else {
      // An unchanged attribute is not written again: a write restyles the cell.
      if (td.dataset.raw !== raw) td.dataset.raw = raw;
      if (td.dataset.display !== shown) td.dataset.display = shown;
    }
    td.classList.toggle("num", nf !== "@" && isNumericText(raw));
    const color = formatted?.c || "";
    if (color || td._nfColor) {
      td._nfColor = color;
      const red = color === SECTION_RED;
      td.style.setProperty("--nf-color", red ? "" : color);
      td.classList.toggle("nf-colored", color !== "" && !red);
      td.classList.toggle("nf-red", red);
    }
  }

  function readTdValue(td) {
    const text = (td.textContent || "").trim();
    if (td.dataset.raw !== undefined && text === td.dataset.display) return td.dataset.raw;
    return text;
  }

  function createSheetGrid(deps) {
    const { document, getSheetName, getEntry, getExtent, getCursor, hooks } = deps;
    const widthsBySheet = Object.create(null);
    const fallback = { firstRow: 1, cols: DEFAULT_COLS };
    const tdByPacked = new Map();
    const colHeadByCol = new Map();
    const rowHeadByRow = new Map();
    let selectedTd = null;
    const rangeTds = [];
    const activeColHeads = [];
    const activeRowHeads = [];
    let widthCacheSheet = null;
    let widthCacheCols = [];
    let widthCachePrefix = [];
    let view = null;
    let spacerTop = null;
    let spacerBottom = null;
    // Hidden rows and columns and merged ranges of the active sheet (PD-194).
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    let widthCacheVersion = -1;
    let layoutApplied = false;
    const layoutOf = () => layoutApi?.get(getSheetName()) || null;

    const viewportEl = () => document.getElementById("sheet-viewport");
    const tbodyEl = () => document.querySelector("#pane-sheet tbody");
    const theadRowEl = () => document.querySelector("#pane-sheet thead tr");
    const tableEl = () => document.querySelector("#pane-sheet table");

    function widths() {
      const name = getSheetName();
      if (!widthsBySheet[name]) widthsBySheet[name] = {};
      return widthsBySheet[name];
    }

    function ensureWidthCache() {
      const name = getSheetName();
      const stamp = layoutApi ? layoutApi.version() : 0;
      if (widthCacheSheet === name && widthCacheVersion === stamp) return;
      widthCacheSheet = name;
      widthCacheVersion = stamp;
      const w = { ...widths() };
      for (const [a, b] of layoutOf()?.hiddenCols || []) {
        for (let c = a; c <= Math.min(b, MAX_COLS); c += 1) w[c] = 0;
      }
      const keys = Object.keys(w);
      if (keys.length === 0) {
        widthCacheCols = [];
        widthCachePrefix = [];
        return;
      }
      const cols = keys.map(Number).sort((a, b) => a - b);
      const prefix = new Array(cols.length);
      let acc = 0;
      for (let i = 0; i < cols.length; i += 1) {
        acc += w[cols[i]] - DEFAULT_COL_W;
        prefix[i] = acc;
      }
      widthCacheCols = cols;
      widthCachePrefix = prefix;
    }

    function colWidth(c) {
      if (layoutApi?.colHidden(getSheetName(), c)) return 0;
      return widths()[c] || DEFAULT_COL_W;
    }

    // Top edge of `row` and the height of the rows between `first` and `last`, hidden rows counting 0.
    function rowTop(row) {
      return (row - 1 - (layoutApi ? layoutApi.rowsHiddenBefore(getSheetName(), row) : 0)) * ROW_H;
    }

    function visibleRowsFrom(row) {
      const name = getSheetName();
      const hiddenAfter = layoutApi ? layoutApi.rowsHiddenTotal(name) - layoutApi.rowsHiddenBefore(name, row) : 0;
      return Math.max(0, MAX_ROWS - row + 1 - hiddenAfter);
    }

    function rowAtVisible(v) {
      return layoutApi ? layoutApi.rowAtVisible(getSheetName(), v) : v + 1;
    }

    function colLeft(c) {
      let x = (c - 1) * DEFAULT_COL_W;
      ensureWidthCache();
      const cols = widthCacheCols;
      const n = cols.length;
      if (n === 0 || cols[0] >= c) return x;
      if (cols[n - 1] < c) return x + widthCachePrefix[n - 1];
      let lo = 0;
      let hi = n - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (cols[mid] < c) lo = mid;
        else hi = mid - 1;
      }
      return x + widthCachePrefix[lo];
    }

    function colAtX(x) {
      let c = Math.min(MAX_COLS, Math.max(1, Math.floor(x / DEFAULT_COL_W) + 1));
      while (c > 1 && colLeft(c) > x) c -= 1;
      while (c < MAX_COLS && colLeft(c + 1) <= x) c += 1;
      return c;
    }

    function setColWidth(c, px) {
      const w = Math.max(MIN_COL_W, Math.min(MAX_COL_W, Math.round(px)));
      if (w === DEFAULT_COL_W) delete widths()[c];
      else widths()[c] = w;
      widthCacheSheet = null;
      return w;
    }

    // Custom column widths of every sheet as `{ [sheet]: { [1-based column]: px } }` (a copy), and the
    // inverse used when a file is opened (PD-089). Widths are clamped like a manual resize.
    function getAllWidths() {
      const out = {};
      Object.keys(widthsBySheet).forEach((name) => {
        if (Object.keys(widthsBySheet[name]).length > 0) out[name] = { ...widthsBySheet[name] };
      });
      return out;
    }

    function replaceAllWidths(bySheet) {
      Object.keys(widthsBySheet).forEach((name) => delete widthsBySheet[name]);
      Object.keys(bySheet || {}).forEach((name) => {
        const next = {};
        Object.keys(bySheet[name] || {}).forEach((c) => {
          const w = Math.max(MIN_COL_W, Math.min(MAX_COL_W, Math.round(Number(bySheet[name][c]))));
          if (Number.isFinite(w) && w !== DEFAULT_COL_W) next[Number(c)] = w;
        });
        widthsBySheet[name] = next;
      });
      widthCacheSheet = null;
    }

    function computeView() {
      const vp = viewportEl();
      if (hasLayout(vp)) {
        const top = Math.floor(vp.scrollTop / ROW_H);
        const visible = Math.ceil((vp.clientHeight - HEAD_H) / ROW_H) + 1;
        return {
          layout: true,
          r0: Math.max(1, rowAtVisible(Math.max(0, top - OVERSCAN_ROWS))),
          r1: Math.min(MAX_ROWS, rowAtVisible(top + visible + OVERSCAN_ROWS)),
          c0: Math.max(1, colAtX(vp.scrollLeft) - OVERSCAN_COLS),
          c1: Math.min(MAX_COLS, colAtX(vp.scrollLeft + vp.clientWidth) + OVERSCAN_COLS),
        };
      }
      const ext = getExtent();
      const cur = getCursor();
      const rows = Math.min(FALLBACK_ROWS, Math.max(MIN_ROWS, ext.rows));
      if (cur.row < fallback.firstRow || cur.row >= fallback.firstRow + rows) {
        fallback.firstRow = Math.max(1, Math.min(MAX_ROWS - rows + 1, cur.row - 5));
        if (cur.row <= rows) fallback.firstRow = 1;
      }
      fallback.cols = Math.min(FALLBACK_COLS, Math.max(fallback.cols, DEFAULT_COLS, ext.cols, cur.col));
      return {
        layout: false,
        r0: fallback.firstRow,
        r1: fallback.firstRow + rows - 1,
        c0: 1,
        c1: fallback.cols,
      };
    }

    function paintTd(td, entry) {
      const value = entry?.value ?? "";
      const formula = entry?.formula || "";
      td._nf = entry?.style?.numFmt || "";
      setDisplayValue(td, value);
      if (formula) td.dataset.formula = formula;
      else delete td.dataset.formula;
      const st = entry?.style;
      td.style.fontWeight = st?.fontWeight || "";
      td.style.fontStyle = st?.fontStyle || "";
      td.style.textDecoration = st?.textDecoration || "";
      td.style.fontSize = st?.fontSize || "";
      host.__EASYOFFICE_SHEET_RULES__?.paintTd(td, getSheetName());
    }

    // Shows `value` with the General format; the raw value is remembered for editing/saving.

    // Raw value of a cell: the unformatted number when the display was rounded.

    function createTd(ref, r, c) {
      const td = document.createElement("td");
      td.setAttribute("contenteditable", "true");
      td.dataset.cell = ref;
      td._row = r;
      td._col = c;
      paintTd(td, getEntry(ref));
      if (hooks.bindTd) hooks.bindTd(td);
      return td;
    }

    function createHeaderCell(c) {
      const th = document.createElement("th");
      th.dataset.col = colName(c);
      th.textContent = colName(c);
      th.style.width = colWidth(c) + "px";
      const handle = document.createElement("span");
      handle.className = "col-resizer";
      handle.dataset.colResizer = colName(c);
      th.appendChild(handle);
      return th;
    }

    function spacerHead(width) {
      const th = document.createElement("th");
      th.className = "sheet-spacer-head";
      th.style.width = width + "px";
      return th;
    }

    function spacerCell() {
      const td = document.createElement("td");
      td.className = "sheet-spacer-x";
      return td;
    }

    function rebuildHead(headRow, v) {
      colHeadByCol.clear();
      const corner = headRow.children[0];
      const list = [];
      if (corner) list.push(corner);
      const left = colLeft(v.c0);
      if (v.layout && left > 0) list.push(spacerHead(left));
      for (let c = v.c0; c <= v.c1; c += 1) {
        const th = createHeaderCell(c);
        colHeadByCol.set(c, th);
        list.push(th);
      }
      const right = colLeft(MAX_COLS + 1) - colLeft(v.c1 + 1);
      if (v.layout && right > 0) list.push(spacerHead(right));
      headRow.replaceChildren(...list);
      if (corner) corner.style.width = ROW_HEAD_W + "px";
    }

    function buildCells(tr, r, v) {
      const old = new Map();
      const children = tr.children;
      const rowBase = r * PACK_STRIDE;
      for (let i = 0; i < children.length; i += 1) {
        const el = children[i];
        if (el.tagName === "TD" && el.dataset.cell) {
          old.set(el.dataset.cell, el);
          if (el._col > 0) tdByPacked.delete(rowBase + el._col);
        }
      }
      const list = [children[0]];
      if (v.layout && v.c0 > 1) list.push(spacerCell());
      for (let c = v.c0; c <= v.c1; c += 1) {
        const ref = refOf(r, c);
        const td = old.get(ref) || createTd(ref, r, c);
        td._row = r;
        td._col = c;
        tdByPacked.set(rowBase + c, td);
        list.push(td);
      }
      if (v.layout && v.c1 < MAX_COLS) list.push(spacerCell());
      tr.replaceChildren(...list);
    }

    function buildRow(r, v) {
      const tr = document.createElement("tr");
      tr.dataset.row = String(r);
      const th = document.createElement("th");
      th.dataset.row = String(r);
      th.textContent = String(r);
      rowHeadByRow.set(r, th);
      tr.appendChild(th);
      buildCells(tr, r, v);
      return tr;
    }

    function makeSpacerRow(cls) {
      const tr = document.createElement("tr");
      tr.className = "sheet-spacer " + cls;
      const td = document.createElement("td");
      td.className = "sheet-spacer-y";
      tr.appendChild(td);
      return tr;
    }

    function syncSpacers(tbody, v) {
      if (!v.layout) {
        if (spacerTop) spacerTop.remove();
        if (spacerBottom) spacerBottom.remove();
        return;
      }
      if (!spacerTop) spacerTop = makeSpacerRow("sheet-spacer-top");
      if (!spacerBottom) spacerBottom = makeSpacerRow("sheet-spacer-bottom");
      const first = tbody.children[0];
      if (first !== spacerTop) {
        spacerTop.remove();
        if (first) first.before(spacerTop);
        else tbody.appendChild(spacerTop);
      }
      const kids = tbody.children;
      if (kids[kids.length - 1] !== spacerBottom) {
        spacerBottom.remove();
        tbody.appendChild(spacerBottom);
      }
      spacerTop.children[0].style.height = rowTop(v.r0) + "px";
      spacerBottom.children[0].style.height = visibleRowsFrom(v.r1 + 1) * ROW_H + "px";
    }

    function activeGridTd() {
      const ae = document.activeElement;
      const inGrid = ae?.tagName === "TD" && ae.dataset?.cell;
      return inGrid ? ae : null;
    }

    function willDropFocus(next, sameCols) {
      const ae = activeGridTd();
      if (!ae) return false;
      const p = parseRef(ae.dataset.cell);
      if (!p) return false;
      return !sameCols || p.row < next.r0 || p.row > next.r1;
    }

    function syncTableWidth(table, v) {
      const inner = v.layout ? colLeft(MAX_COLS + 1) : colLeft(v.c1 + 1);
      table.style.width = ROW_HEAD_W + inner + "px";
    }

    // Drops rows outside the window and refreshes the survivors; returns them keyed by row number.
    function reconcileExistingRows(tbody, next, sameCols) {
      const existing = new Map();
      for (const tr of Array.from(tbody.children)) {
        if (tr.className && String(tr.className).includes("sheet-spacer")) continue;
        const r = rowOf(tr);
        if (r < next.r0 || r > next.r1 || r === 0) {
          if (r > 0) {
            rowHeadByRow.delete(r);
            const rowBase = r * PACK_STRIDE;
            const ch = tr.children;
            for (let i = 0; i < ch.length; i += 1) {
              const el = ch[i];
              if (el._col > 0) tdByPacked.delete(rowBase + el._col);
              else if (el.dataset?.cell) tdByPacked.delete(packRef(el.dataset.cell));
            }
          }
          tr.remove();
          continue;
        }
        existing.set(r, tr);
        if (tr.children[0]) rowHeadByRow.set(r, tr.children[0]);
        if (!tr.dataset.row) tr.dataset.row = String(r);
        if (!sameCols) buildCells(tr, r, next);
      }
      return existing;
    }

    function insertMissingRows(tbody, existing, next) {
      let minExisting = Number.POSITIVE_INFINITY;
      for (const k of existing.keys()) {
        if (k < minExisting) minExisting = k;
      }
      const firstTr = minExisting < Number.POSITIVE_INFINITY ? existing.get(minExisting) : null;
      for (let r = next.r0; r <= next.r1; r += 1) {
        if (existing.has(r)) continue;
        const tr = buildRow(r, next);
        if (firstTr && r < minExisting) firstTr.before(tr);
        else if (spacerBottom && spacerBottom.parentElement === tbody) spacerBottom.before(tr);
        else tbody.appendChild(tr);
      }
    }

    // Asks for the display text of formatted cells just beyond the window (PD-193), so the rows a
    // scroll brings in are drawn with their format at once instead of being repainted when the
    // answer arrives. Only a sheet that has drawn a formatted cell pays the scan.
    function prefetchNumFmt(v) {
      const display = host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__;
      if (!display?.active() || !v.layout) return;
      const scan = (from, to) => {
        for (let r = Math.max(1, from); r <= Math.min(MAX_ROWS, to); r += 1) {
          for (let c = v.c0; c <= v.c1; c += 1) {
            const entry = getEntry(refOf(r, c));
            const code = entry?.style?.numFmt;
            if (code) display.shown(code, String(entry.value ?? ""));
          }
        }
      };
      scan(v.r0 - NUMFMT_PREFETCH_ROWS, v.r0 - 1);
      scan(v.r1 + 1, v.r1 + NUMFMT_PREFETCH_ROWS);
    }

    // Brings the DOM window in line with the viewport. Returns true when rows/cells changed.
    function renderWindow(force) {
      const tbody = tbodyEl();
      const headRow = theadRowEl();
      const table = tableEl();
      if (!tbody || !headRow || !table) return false;
      const next = computeView();
      const prev = view;
      const sameCols =
        Boolean(prev) && prev.c0 === next.c0 && prev.c1 === next.c1 && prev.layout === next.layout;
      const sameRows = Boolean(prev) && prev.r0 === next.r0 && prev.r1 === next.r1;
      if (!force && sameCols && sameRows) return false;
      if (willDropFocus(next, sameCols) && hooks.beforeStructuralChange) hooks.beforeStructuralChange();
      view = next;
      // A forced render also rebuilds the header: column widths are per sheet (tab switch, file open).
      if (!sameCols || force) rebuildHead(headRow, next);

      const existing = reconcileExistingRows(tbody, next, sameCols);
      syncSpacers(tbody, next);
      insertMissingRows(tbody, existing, next);
      syncTableWidth(table, next);
      applyLayoutClasses();
      if (hooks.afterRender) hooks.afterRender();
      prefetchNumFmt(next);
      return true;
    }

    // The header right after a hidden run carries a thick edge on that side and the run's bounds
    // (`data-hidden-run="first-last"`), which a double click on the edge shows again.
    function markHiddenEdge(th, axis, run) {
      th.classList.toggle("hidden-edge-" + axis, Boolean(run));
      if (run) th.dataset.hiddenRun = run[0] + "-" + run[1];
      else if (th.dataset.hiddenRun) delete th.dataset.hiddenRun;
    }

    // Hidden rows and columns get `sheet-hidden`; a merged range paints from its anchor cell
    // (`merge-anchor` with its size in `--mw`/`--mh`) over the cells it covers (`merge-cover`).
    function applyLayoutClasses() {
      const layout = layoutOf();
      const empty = !layout || layoutApi.isEmpty(layout);
      if (empty && !layoutApplied) return;
      layoutApplied = !empty;
      const name = getSheetName();
      const runs = layout?.hiddenRows || [];
      const tbody = tbodyEl();
      if (tbody) {
        for (const tr of tbody.children) {
          const r = rowOf(tr);
          if (r > 0) tr.classList.toggle("sheet-hidden", !empty && layoutApi.rowHidden(name, r));
        }
      }
      const colRuns = layout?.hiddenCols || [];
      const filter = layout?.filter || null;
      const filterCols = filter ? new Set(filter.columns.map((x) => x.col)) : null;
      for (const [c, th] of colHeadByCol) {
        th.classList.toggle("sheet-hidden", !empty && layoutApi.colHidden(name, c));
        markHiddenEdge(th, "col", empty ? null : layoutApi.runAt(colRuns, c - 1));
      }
      for (const [r, th] of rowHeadByRow) markHiddenEdge(th, "row", empty ? null : layoutApi.runAt(runs, r - 1));
      seedDomMaps();
      for (const td of tdByPacked.values()) {
        const r = td._row;
        const c = td._col;
        td.classList.toggle("sheet-hidden", !empty && layoutApi.colHidden(name, c));
        // The header row of an auto filter carries its drop-down button; a column with a condition shows it on.
        const head = filter !== null && r === filter.range.r1 && c >= filter.range.c1 && c <= filter.range.c2;
        td.classList.toggle("filter-head", head);
        td.classList.toggle("filter-on", head && filterCols.has(c));
        const m = empty ? null : layoutApi.mergeAt(name, r, c);
        const anchor = Boolean(m) && m.r1 === r && m.c1 === c;
        td.classList.toggle("merge-anchor", anchor);
        td.classList.toggle("merge-cover", Boolean(m) && !anchor);
        if (anchor) {
          td.style.setProperty("--mw", colLeft(m.c2 + 1) - colLeft(m.c1) + "px");
          const hidden = layoutApi.rowsHiddenBefore(name, m.r2 + 1) - layoutApi.rowsHiddenBefore(name, m.r1);
          td.style.setProperty("--mh", (m.r2 - m.r1 + 1 - hidden) * ROW_H + "px");
        } else if (td.style.getPropertyValue("--mw")) {
          td.style.removeProperty("--mw");
          td.style.removeProperty("--mh");
        }
      }
    }

    // The layout changed (hide, unhide, merge, edit): redraw with the new row and column geometry.
    function layoutChanged() {
      widthCacheSheet = null;
      return renderWindow(true);
    }

    function seedDomMaps() {
      if (tdByPacked.size > 0) return;
      const headRow = theadRowEl();
      if (headRow) {
        const ch = headRow.children;
        for (let i = 0; i < ch.length; i += 1) {
          const colStr = ch[i].dataset?.col;
          if (colStr) colHeadByCol.set(colIndex(colStr), ch[i]);
        }
      }
      const tbody = tbodyEl();
      if (!tbody) return;
      const rows = tbody.children;
      for (let i = 0; i < rows.length; i += 1) {
        const tr = rows[i];
        const r = rowOf(tr);
        if (r <= 0) continue;
        if (tr.children[0]) rowHeadByRow.set(r, tr.children[0]);
        const cells = tr.children;
        for (let j = 1; j < cells.length; j += 1) {
          const td = cells[j];
          const ref = td.dataset?.cell;
          if (!ref) continue;
          const packed = packRef(ref);
          if (packed !== 0) {
            td._row = r;
            td._col = packed & 0x7fff;
            tdByPacked.set(packed, td);
          }
        }
      }
    }

    function getTdAt(row, col) {
      seedDomMaps();
      const td = tdByPacked.get(packCoord(row, col));
      if (td && td.isConnected !== false) return td;
      const found = document.querySelector('#pane-sheet td[data-cell="' + refOf(row, col) + '"]');
      if (found) {
        found._row = row;
        found._col = col;
        tdByPacked.set(packCoord(row, col), found);
      }
      return found;
    }

    function paintSelectionRange(rg) {
      seedDomMaps();
      if (selectedTd) {
        selectedTd.classList.remove("selected-cell");
        selectedTd = null;
      }
      for (let i = 0; i < rangeTds.length; i += 1) rangeTds[i].classList.remove("in-range");
      rangeTds.length = 0;
      for (let i = 0; i < activeColHeads.length; i += 1) activeColHeads[i].classList.remove("active-axis");
      activeColHeads.length = 0;
      for (let i = 0; i < activeRowHeads.length; i += 1) activeRowHeads[i].classList.remove("active-axis");
      activeRowHeads.length = 0;
      if (!rg) return;
      const ar = rg.ar ?? rg.r1;
      const ac = rg.ac ?? rg.c1;
      const anchor = tdByPacked.get(packCoord(ar, ac));
      if (anchor) {
        anchor.classList.add("selected-cell");
        selectedTd = anchor;
      }
      const v = view;
      const mr1 = v ? Math.max(rg.r1, v.r0) : rg.r1;
      const mr2 = v ? Math.min(rg.r2, v.r1) : rg.r2;
      const mc1 = v ? Math.max(rg.c1, v.c0) : rg.c1;
      const mc2 = v ? Math.min(rg.c2, v.c1) : rg.c2;
      if (rg.r1 !== rg.r2 || rg.c1 !== rg.c2) {
        for (let r = mr1; r <= mr2; r += 1) {
          const rowBase = r * PACK_STRIDE;
          for (let c = mc1; c <= mc2; c += 1) {
            const td = tdByPacked.get(rowBase + c);
            if (td) {
              td.classList.add("in-range");
              rangeTds.push(td);
            }
          }
        }
      }
      for (let c = mc1; c <= mc2; c += 1) {
        const th = colHeadByCol.get(c);
        if (th) {
          th.classList.add("active-axis");
          activeColHeads.push(th);
        }
      }
      for (let r = mr1; r <= mr2; r += 1) {
        const th = rowHeadByRow.get(r);
        if (th) {
          th.classList.add("active-axis");
          activeRowHeads.push(th);
        }
      }
    }

    // Repaints every materialized cell from the model (keeps the elements, so focus survives).
    function refreshCells() {
      const tbody = tbodyEl();
      if (!tbody) return [];
      seedDomMaps();
      const tds = [];
      for (const td of tdByPacked.values()) {
        paintTd(td, getEntry(td.dataset.cell));
        tds.push(td);
      }
      applyLayoutClasses();
      return tds;
    }

    // Shows the answers of the engine for formatted cells that were drawn before they arrived; a
    // cell being edited keeps its text.
    function repaintNumFmt() {
      seedDomMaps();
      for (const td of tdByPacked.values()) {
        if (td._nf && td.dataset.editMode === undefined) setDisplayValue(td, readTdValue(td));
      }
    }

    // A column of another width shows its formatted numbers in full or as hash marks again.
    function repaintNumFmtColumn(c) {
      seedDomMaps();
      for (const td of tdByPacked.values()) {
        if (td._col === c && td._nf && td.dataset.editMode === undefined) setDisplayValue(td, readTdValue(td));
      }
    }

    // Repaints only the rule appearance (conditional formatting) of the materialized cells, so
    // a cell being edited keeps its text.
    function repaintFx() {
      seedDomMaps();
      const sheet = getSheetName();
      for (const td of tdByPacked.values()) host.__EASYOFFICE_SHEET_RULES__?.paintTd(td, sheet);
    }

    function ensureVisible(row, col) {
      const vp = viewportEl();
      if (hasLayout(vp)) {
        const viewH = vp.clientHeight - HEAD_H;
        const top = rowTop(row);
        if (top < vp.scrollTop) vp.scrollTop = top;
        else if (top + ROW_H > vp.scrollTop + viewH) vp.scrollTop = top + ROW_H - viewH;
        const left = colLeft(col);
        const viewW = vp.clientWidth - ROW_HEAD_W;
        if (left < vp.scrollLeft) vp.scrollLeft = left;
        else if (left + colWidth(col) > vp.scrollLeft + viewW) {
          vp.scrollLeft = left + colWidth(col) - viewW;
        }
      }
      return renderWindow(false);
    }

    // Applies a new column width to the live header cell and the table width.
    columnWidthOf = colWidth;

    function applyColWidth(c, px) {
      const w = setColWidth(c, px);
      repaintNumFmtColumn(c);
      const th = colHeadByCol.get(c) || document.querySelector('#pane-sheet th[data-col="' + colName(c) + '"]');
      if (th) th.style.width = w + "px";
      const table = tableEl();
      if (table && view) syncTableWidth(table, view);
      return w;
    }

    function visibleRowSpan() {
      const vp = viewportEl();
      if (hasLayout(vp)) return Math.max(1, Math.floor((vp.clientHeight - HEAD_H) / ROW_H) - 1);
      return 20;
    }

    function getTdByRef(ref) {
      seedDomMaps();
      const packed = packRef(ref);
      const td = packed > 0 ? tdByPacked.get(packed) : null;
      return td && td.isConnected !== false ? td : null;
    }

    function forEachMaterializedTd(cb) {
      seedDomMaps();
      tdByPacked.forEach((td) => {
        if (td && td.isConnected !== false) cb(td);
      });
    }

    function install() {
      const vp = viewportEl();
      if (vp && typeof vp.addEventListener === "function") {
        vp.addEventListener("scroll", () => renderWindow(false));
      }
      const RO = host.ResizeObserver;
      if (vp && typeof RO === "function") {
        new RO(() => renderWindow(false)).observe(vp);
      }
    }

    return {
      MAX_ROWS,
      MAX_COLS,
      ROW_H,
      ROW_HEAD_W,
      DEFAULT_COL_W,
      colName,
      colIndex,
      packCoord,
      packRef,
      parseRef,
      refOf,
      colWidth,
      colLeft,
      colAtX,
      applyColWidth,
      layoutChanged,
      rowAtVisible,
      mergeAt: (r, c) => layoutApi?.mergeAt(getSheetName(), r, c) || null,
      rowHidden: (r) => Boolean(layoutApi?.rowHidden(getSheetName(), r)),
      colHidden: (c) => Boolean(layoutApi?.colHidden(getSheetName(), c)),
      getAllWidths,
      replaceAllWidths,
      repaintFx,
      repaintNumFmt,
      renderWindow,
      refreshCells,
      ensureVisible,
      visibleRowSpan,
      getTdAt,
      getTdByRef,
      forEachMaterializedTd,
      paintSelectionRange,
      paintTd,
      setDisplayValue,
      readTdValue,
      install,
      getView: () => view,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_GRID__ = createSheetGrid;
  host.__EASYOFFICE_SHEET_GRID_UTILS__ = {
    MAX_ROWS,
    MAX_COLS,
    colName,
    colIndex,
    packCoord,
    packRef,
    parseRef,
    refOf,
    isNumericText,
    formatGeneral,
    cloneSheetStore,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
