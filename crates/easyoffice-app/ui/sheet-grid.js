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

  function colName(idx) {
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
      n = n * 26 + (letters.codePointAt(i) - 64);
    }
    return n;
  }

  function parseRef(ref) {
    const m = /^([A-Za-z]{1,3})([1-9]\d{0,6})$/.exec(String(ref || "").trim());
    if (!m) return null;
    const col = colIndex(m[1].toUpperCase());
    const row = Number(m[2]);
    if (col < 1 || col > MAX_COLS || row < 1 || row > MAX_ROWS) return null;
    return { row, col };
  }

  function refOf(row, col) {
    return colName(col) + row;
  }

  const NUMERIC_RE = /^-?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
  const GROUPED_NUMERIC_RE = /^-?\d{1,3}(?:,\d{3})+(?:\.\d+)?$/;

  function isNumericText(text) {
    const t = String(text ?? "").trim();
    return t !== "" && (NUMERIC_RE.test(t) || GROUPED_NUMERIC_RE.test(t));
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
  // round-trip, which matters for undo snapshots of sheets with tens of thousands of cells.
  function cloneSheetStore(store) {
    const out = {};
    Object.keys(store || {}).forEach((name) => {
      const src = store[name] || {};
      const dst = {};
      Object.keys(src).forEach((ref) => {
        const e = src[ref];
        if (!e) return;
        const copy = { ...e };
        if (e.style) copy.style = { ...e.style };
        dst[ref] = copy;
      });
      out[name] = dst;
    });
    return out;
  }

  function hasLayout(vp) {      return Boolean(vp) && vp.clientHeight > 0 && vp.clientWidth > 0;
  }

  function rowOf(tr) {
    if (tr?.tagName !== "TR") return 0;
    const raw = tr.dataset.row || tr.querySelector("th[data-row]")?.dataset.row;
    return Number(raw) || 0;
  }

  function setDisplayValue(td, value) {
    const raw = String(value ?? "");
    const shown = formatGeneral(raw);
    td.textContent = shown;
    if (shown === raw) {
      delete td.dataset.raw;
      delete td.dataset.display;
    } else {
      td.dataset.raw = raw;
      td.dataset.display = shown;
    }
    td.classList.toggle("num", isNumericText(raw));
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
    let view = null;
    let spacerTop = null;
    let spacerBottom = null;

    const viewportEl = () => document.getElementById("sheet-viewport");
    const tbodyEl = () => document.querySelector("#pane-sheet tbody");
    const theadRowEl = () => document.querySelector("#pane-sheet thead tr");
    const tableEl = () => document.querySelector("#pane-sheet table");

    function widths() {
      const name = getSheetName();
      if (!widthsBySheet[name]) widthsBySheet[name] = {};
      return widthsBySheet[name];
    }

    function colWidth(c) {
      return widths()[c] || DEFAULT_COL_W;
    }

    function colLeft(c) {
      let x = (c - 1) * DEFAULT_COL_W;
      const w = widths();
      Object.keys(w).forEach((k) => {
        if (Number(k) < c) x += w[k] - DEFAULT_COL_W;
      });
      return x;
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
    }

    function computeView() {
      const vp = viewportEl();
      if (hasLayout(vp)) {
        const top = Math.floor(vp.scrollTop / ROW_H) + 1;
        const visible = Math.ceil((vp.clientHeight - HEAD_H) / ROW_H) + 1;
        return {
          layout: true,
          r0: Math.max(1, top - OVERSCAN_ROWS),
          r1: Math.min(MAX_ROWS, top + visible + OVERSCAN_ROWS),
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
      setDisplayValue(td, value);
      if (formula) td.dataset.formula = formula;
      else delete td.dataset.formula;
      const st = entry?.style;
      td.style.fontWeight = st?.fontWeight || "";
      td.style.fontStyle = st?.fontStyle || "";
      td.style.textDecoration = st?.textDecoration || "";
      td.style.fontSize = st?.fontSize || "";
      td.classList.toggle("condfmt-high", Boolean(st?.condfmtHigh));
    }

    // Shows `value` with the General format; the raw value is remembered for editing/saving.

    // Raw value of a cell: the unformatted number when the display was rounded.

    function createTd(ref) {
      const td = document.createElement("td");
      td.setAttribute("contenteditable", "true");
      td.dataset.cell = ref;
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
      const corner = headRow.children[0];
      const list = [];
      if (corner) list.push(corner);
      const left = colLeft(v.c0);
      if (v.layout && left > 0) list.push(spacerHead(left));
      for (let c = v.c0; c <= v.c1; c += 1) list.push(createHeaderCell(c));
      const right = colLeft(MAX_COLS + 1) - colLeft(v.c1 + 1);
      if (v.layout && right > 0) list.push(spacerHead(right));
      headRow.replaceChildren(...list);
      if (corner) corner.style.width = ROW_HEAD_W + "px";
    }

    function buildCells(tr, r, v) {
      const old = new Map();
      Array.from(tr.children).forEach((el) => {
        if (el.tagName === "TD" && el.dataset.cell) old.set(el.dataset.cell, el);
      });
      const list = [tr.children[0]];
      if (v.layout && v.c0 > 1) list.push(spacerCell());
      for (let c = v.c0; c <= v.c1; c += 1) {
        const ref = refOf(r, c);
        list.push(old.get(ref) || createTd(ref));
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
      spacerTop.children[0].style.height = (v.r0 - 1) * ROW_H + "px";
      spacerBottom.children[0].style.height = (MAX_ROWS - v.r1) * ROW_H + "px";
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
          tr.remove();
          continue;
        }
        existing.set(r, tr);
        if (!tr.dataset.row) tr.dataset.row = String(r);
        if (!sameCols) buildCells(tr, r, next);
      }
      return existing;
    }

    function insertMissingRows(tbody, existing, next) {
      const rowsAsc = Array.from(existing.keys()).sort((a, b) => a - b);
      const firstTr = rowsAsc.length > 0 ? existing.get(rowsAsc[0]) : null;
      const minExisting = rowsAsc.length > 0 ? rowsAsc[0] : Number.POSITIVE_INFINITY;
      for (let r = next.r0; r <= next.r1; r += 1) {
        if (existing.has(r)) continue;
        const tr = buildRow(r, next);
        if (firstTr && r < minExisting) firstTr.before(tr);
        else if (spacerBottom && spacerBottom.parentElement === tbody) spacerBottom.before(tr);
        else tbody.appendChild(tr);
      }
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
      if (hooks.afterRender) hooks.afterRender();
      return true;
    }

    // Repaints every materialized cell from the model (keeps the elements, so focus survives).
    function refreshCells() {
      const tbody = tbodyEl();
      if (!tbody) return [];
      const tds = Array.from(tbody.querySelectorAll("td[data-cell]"));
      tds.forEach((td) => paintTd(td, getEntry(td.dataset.cell)));
      return tds;
    }

    function ensureVisible(row, col) {
      const vp = viewportEl();
      if (hasLayout(vp)) {
        const viewH = vp.clientHeight - HEAD_H;
        const top = (row - 1) * ROW_H;
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
    function applyColWidth(c, px) {
      const w = setColWidth(c, px);
      const th = document.querySelector('#pane-sheet th[data-col="' + colName(c) + '"]');
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
      parseRef,
      refOf,
      colWidth,
      colLeft,
      colAtX,
      applyColWidth,
      getAllWidths,
      replaceAllWidths,
      renderWindow,
      refreshCells,
      ensureVisible,
      visibleRowSpan,
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
    parseRef,
    refOf,
    isNumericText,
    formatGeneral,
    cloneSheetStore,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
