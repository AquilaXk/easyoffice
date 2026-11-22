(function (host) {
  "use strict";

  // Virtualized spreadsheet grid. The model lives in `sheetDataStore`; this module only
  // materializes the cells around the viewport (plus a small overscan) as real table rows.
  // Logical addressing covers the full 1,048,576 x 16,384 sheet while the DOM stays small.
  const { MAX_ROWS, MAX_COLS } = host.__EASYOFFICE_SHEET_REFS__;
  // `DEFAULT_COL_W`: the width of a column a sheet says nothing about (PD-305, PD-306: 8 characters of the 8 px digit of Malgun Gothic 11 pt, up to 8 px); a sheet that does has its own
  // (`defaultColW()`), which the engine works out from the file's `defaultColWidth` and `baseColWidth`.
  const ROW_H = 22, HEAD_H = 26, ROW_HEAD_W = 56, DEFAULT_COL_W = 72, MIN_COL_W = 36, MAX_COL_W = 720;
  const OVERSCAN_ROWS = 8, OVERSCAN_COLS = 2, FALLBACK_ROWS = 60, FALLBACK_COLS = 40, DEFAULT_COLS = 8, MIN_ROWS = 12;

  const { PACK_STRIDE, colName, colIndex, packCoord, packRef, parseRef, refOf } = host.__EASYOFFICE_SHEET_REFS__;
  // The cell text helpers (`sheet-grid-text.js`), the selection classes (`sheet-grid-selection.js`) and the layout classes
  // (`sheet-grid-layout-paint.js`) are loaded before this file.

  const { isNumericText, formatGeneral, cloneSheetStore, measureWrapped, measureLine, setColumnWidthLookup, setDisplayValue, readTdValue } = host.__EASYOFFICE_SHEET_GRID_TEXT__;
  // Rows beyond each edge of the window whose formatted text is fetched ahead of a scroll.
  const NUMFMT_PREFETCH_ROWS = 60;

  function hasLayout(vp) {
    return Boolean(vp) && vp.clientHeight > 0 && vp.clientWidth > 0;
  }

  function rowOf(tr) {
    if (tr?.tagName !== "TR") return 0;
    const raw = tr.dataset.row || tr.querySelector("th[data-row]")?.dataset.row;
    return Number(raw) || 0;
  }

  function createSheetGrid(deps) {
    const { document, getSheetName, getEntry, getExtent, getCursor, hooks, getMap } = deps;
    const widthsBySheet = Object.create(null);
    const fallback = { firstRow: 1, cols: DEFAULT_COLS };
    const tdByPacked = new Map(), colHeadByCol = new Map(), rowHeadByRow = new Map();
    let widthCacheSheet = null, view = null, spacerTop = null, spacerBottom = null;
    let widthCacheCols = [], widthCachePrefix = [];
    // Hidden rows and columns and merged ranges of the active sheet (PD-194).
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    let widthCacheVersion = -1;
    const layoutOf = () => layoutApi?.get(getSheetName()) || null;
    const defaultColW = () => layoutApi?.defaultColPxOf?.(getSheetName()) ?? DEFAULT_COL_W;
    // The height of a row with none of its own: the grid's `ROW_H` (the stylesheet's), or the default row height of the sheet when a user
    // set one (PD-286).
    const rowBase = () => host.__EASYOFFICE_SHEET_ROW_HEIGHT__.baseOf(layoutApi?.defaultRowHeightOf(getSheetName()), ROW_H);
    // Rows follow the height of their wrapped cells (PD-207); `null` where the page cannot measure text.
    const rowFit = host.__EASYOFFICE_CREATE_ROW_FIT__?.({
      rowH: rowBase,
      maxRows: MAX_ROWS,
      getSheetName,
      getMap: getMap || (() => null),
      columnWidth: (c) => colWidth(c),
      isRowHidden: (r) => rowIsHidden(r),
      isMerged: (r, c) => Boolean(layoutApi?.mergeAt(getSheetName(), r, c)),
      measure: (text, width, font) => measureWrapped(sampleTd(), text, width, font),
      packRef,
      // The heights a user set, as `[[row, px], ...]` (PD-278).
      customHeights: () => (rowHeightsOf() ? host.__EASYOFFICE_SHEET_ROW_HEIGHT__.pxEntries(rowHeightsOf()) : []),
    });
    function rowHeightsOf() {
      return layoutApi?.rowHeightsOf(getSheetName()) || null;
    }
    // Frozen rows and columns stay in the window whatever the scroll (PD-208).
    const freeze = host.__EASYOFFICE_CREATE_SHEET_FREEZE__?.({
      layoutApi,
      getSheetName,
      rowTop: (r) => rowTop(r),
      colLeft: (c) => colLeft(c),
      headH: HEAD_H,
      rowHeadW: ROW_HEAD_W,
      maxRows: MAX_ROWS,
      maxCols: MAX_COLS,
    });
    const frozenCounts = () => (freeze ? freeze.counts() : { rows: 0, cols: 0 });
    const sampleTd = () => {
      seedDomMaps();
      return tdByPacked.values().next().value || null;
    };
    // Row heights are looked up only once a wrapped or large cell exists, a row has a height of a user, or one has been fitted before (it may be gone now).
    // A default row height other than the grid's moves every row, and a large font needs more than a short default row has.
    const wrapRowsPossible = () => Boolean(rowFit) && (lookApi.wrapSeen() || rowFit.any() || Boolean(rowHeightsOf()) || rowBase() !== ROW_H);
    // Works the row heights out again; a sync that ran out of its budget leaves the rest of the rows plain and says so.
    function syncRows() {
      const changed = rowFit.sync();
      const say = rowFit.overBudget() && document.getElementById("status-command-feedback");
      if (say) say.textContent = "행 높이를 맞추기에 시트가 너무 커서 일부 행은 기본 높이로 둠";
      return changed;
    }
    // The look of a cell as the page paints it: neighbours decide the shared edges, and the edges the rules draw come over the cell's
    // own (PD-284). A rule edge changes the look of the cells beside it too: the rules ask for a "relook" when their edges moved.
    const lookApi = host.__EASYOFFICE_SHEET_LOOK__;
    const borderAt = (r, c) => (r < 1 || c < 1 ? undefined : host.__EASYOFFICE_SHEET_RULES__.withEdges(getSheetName(), r, c, getEntry(refOf(r, c))?.style?.border));
    const paintLook = (td, m = layoutApi?.mergeAt(getSheetName(), td._row, td._col)) =>
      lookApi?.paintTd(td, td._st, td._row, td._col, borderAt, m && m.r1 === td._row && m.c1 === td._col ? m : null);
    host.__EASYOFFICE_SHEET_RULES__?.on("relook", (td) => paintLook(td));

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
      for (const [a, b] of layoutApi ? layoutApi.hiddenColRuns(name) : []) {
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
      const plain = defaultColW();
      let acc = 0;
      for (let i = 0; i < cols.length; i += 1) {
        acc += w[cols[i]] - plain;
        prefix[i] = acc;
      }
      widthCacheCols = cols;
      widthCachePrefix = prefix;
    }

    function colWidth(c) {
      if (layoutApi?.colHidden(getSheetName(), c)) return 0;
      return widths()[c] || defaultColW();
    }

    // Top edge of `row` and the height of the rows between `first` and `last`, hidden rows counting 0.
    function rowTop(row) {
      const hidden = layoutApi ? layoutApi.rowsHiddenBefore(getSheetName(), row) : 0;
      return (row - 1 - hidden) * rowBase() + (rowFit ? rowFit.extraBefore(row) : 0);
    }

    // Height of the visible rows from `row` to the last one, with the taller rows counted at their height.
    function rowsHeightFrom(row) {
      const name = getSheetName();
      const hiddenAfter = layoutApi ? layoutApi.rowsHiddenTotal(name) - layoutApi.rowsHiddenBefore(name, row) : 0;
      const extra = rowFit ? rowFit.extraTotal() - rowFit.extraBefore(row) : 0;
      return Math.max(0, MAX_ROWS - row + 1 - hiddenAfter) * rowBase() + extra;
    }

    // The row whose band holds the vertical offset `y` of the scrolled content.
    function rowAtY(y) {
      if (!rowFit || !rowFit.any()) return rowAtVisible(Math.max(0, Math.floor(y / rowBase())));
      // Rows under the grid's height put the row below the guess too, so the search covers the whole sheet.
      let lo = 1;
      let hi = MAX_ROWS;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (rowTop(mid) <= y) lo = mid;
        else hi = mid - 1;
      }
      return lo;
    }

    const visibleIndexOf = (row) => row - 1 - (layoutApi ? layoutApi.rowsHiddenBefore(getSheetName(), row) : 0);

    // Sets the height of every materialized row to its height (a plain row keeps the stylesheet's). A row under the stylesheet's
    // takes its height from `--rh` (`row-short`), and a row whose height cuts its cells off keeps them on one line (`row-clip`).
    function applyRowHeights(tbody) {
      if (!rowFit) return;
      for (const tr of tbody.children) {
        const r = rowOf(tr);
        if (r === 0) continue;
        const th = tr.children[0];
        if (!th._rz) (th._rz = th.appendChild(document.createElement("span"))).className = "row-resizer"; // the edge to drag (the first rows come with the page)
        const h = rowFit.heightOf(r);
        const want = h === ROW_H ? "" : h + "px";
        if (tr.style.height !== want) {
          tr.style.height = want;
          if (h < ROW_H) tr.style.setProperty("--rh", want);
          else tr.style.removeProperty("--rh");
        }
        tr.classList.toggle("row-short", h < ROW_H);
        tr.classList.toggle("row-clip", rowFit.isClipped(r));
      }
    }

    function rowAtVisible(v) {
      return layoutApi ? layoutApi.rowAtVisible(getSheetName(), v) : v + 1;
    }

    function colLeft(c) {
      let x = (c - 1) * defaultColW();
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
      let c = Math.min(MAX_COLS, Math.max(1, Math.floor(x / defaultColW()) + 1));
      while (c > 1 && colLeft(c) > x) c -= 1;
      while (c < MAX_COLS && colLeft(c + 1) <= x) c += 1;
      return c;
    }

    function setColWidth(c, px) {
      const w = Math.max(MIN_COL_W, Math.min(MAX_COL_W, Math.round(px)));
      if (w === defaultColW()) delete widths()[c];
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
          // A width equal to the sheet's default stays: the file wrote it down, and the default of the sheet may arrive after it.
          if (Number.isFinite(w)) next[Number(c)] = w;
        });
        widthsBySheet[name] = next;
      });
      widthCacheSheet = null;
    }

    const frozenHeight = (n) => (n > 0 ? freeze.height(n) : 0);
    const frozenWidth = (n) => (n > 0 ? freeze.width(n) : 0);

    function computeView() {
      const vp = viewportEl();
      if (hasLayout(vp)) {
        const { rows: fr, cols: fc } = frozenCounts();
        const first = rowAtY(vp.scrollTop + frozenHeight(fr));
        const top = visibleIndexOf(first);
        // Rows under the grid's height put more of them in the viewport: the last one is found by its position.
        const visible = rowFit?.any() ? visibleIndexOf(rowAtY(vp.scrollTop + vp.clientHeight)) - top + 1 : Math.ceil((vp.clientHeight - HEAD_H) / rowBase()) + 1;
        return {
          layout: true,
          fr,
          fc,
          r0: Math.max(fr + 1, rowAtVisible(Math.max(0, top - OVERSCAN_ROWS))),
          r1: Math.min(MAX_ROWS, rowAtVisible(top + visible + OVERSCAN_ROWS)),
          c0: Math.max(fc + 1, colAtX(vp.scrollLeft + frozenWidth(fc)) - OVERSCAN_COLS),
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

    // `keepText`: the cell is being typed in, so its text and its type stay what they are (the model has not got them yet).
    function paintTd(td, entry, keepText = false) {
      const value = entry?.value ?? "";
      const formula = entry?.formula || "";
      td._nf = entry?.style?.numFmt || "";
      if (!keepText) {
        td._ct = formula ? "" : (entry?.t || "");
        setDisplayValue(td, value);
      }
      if (formula) td.dataset.formula = formula;
      else delete td.dataset.formula;
      // A cell an array filled (PD-217) keeps the address of the formula that made it.
      if (entry?.spillOf) td.dataset.spillOf = entry.spillOf; else delete td.dataset.spillOf;
      host.__EASYOFFICE_SHEET_LINK__?.paint(td, entry?.link, entry);
      if (layoutApi?.noteAtRef(getSheetName(), td.dataset.cell)) td.dataset.note = "true";
      else if (td.dataset.note !== undefined) delete td.dataset.note;
      const st = entry?.style;
      td.style.fontWeight = st?.fontWeight || "";
      td.style.fontStyle = st?.fontStyle || "";
      td.style.textDecoration = st?.textDecoration || "";
      td.style.fontSize = st?.fontSize || "";
      td.style.fontFamily = st?.fontFamily || "";
      td._st = st;
      // A locked cell of a protected sheet cannot be typed into (PD-216).
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      const editable = !guard || guard.cellEditable(getSheetName(), td._row, td._col, st);
      // A cell that cannot be typed into still takes the keyboard focus, so the arrows and the copy keys keep working on it.
      if ((td.getAttribute("contenteditable") === "true") !== editable) {
        td.setAttribute("contenteditable", editable ? "true" : "false");
        if (editable) td.removeAttribute("tabindex");
        else td.setAttribute("tabindex", "-1");
      }
      paintLook(td);
      host.__EASYOFFICE_SHEET_TABLE_STYLE__?.paintTd(td, getSheetName());
      host.__EASYOFFICE_SHEET_RULES__?.paintTd(td, getSheetName());
    }

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

    function frozenHead(c) {
      const th = createHeaderCell(c);
      colHeadByCol.set(c, th);
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
      const fc = v.layout ? v.fc : 0;
      for (let c = 1; c <= fc; c += 1) list.push(frozenHead(c));
      const left = colLeft(v.c0) - colLeft(fc + 1);
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
      const fc = v.layout ? v.fc : 0;
      const place = (c) => {
        const ref = refOf(r, c);
        const td = old.get(ref) || createTd(ref, r, c);
        td._row = r;
        td._col = c;
        tdByPacked.set(rowBase + c, td);
        list.push(td);
      };
      for (let c = 1; c <= fc; c += 1) place(c);
      if (v.layout && v.c0 > fc + 1) list.push(spacerCell());
      for (let c = v.c0; c <= v.c1; c += 1) place(c);
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
      // The frozen rows come before the top spacer.
      const kids = [...tbody.children];
      const others = kids.filter((tr) => tr !== spacerTop);
      const first = others.find((tr) => rowOf(tr) > v.fr);
      if (kids.indexOf(spacerTop) !== (first ? others.indexOf(first) : others.length)) {
        spacerTop.remove();
        if (first) first.before(spacerTop);
        else tbody.appendChild(spacerTop);
      }
      if (tbody.children[tbody.children.length - 1] !== spacerBottom) {
        spacerBottom.remove();
        tbody.appendChild(spacerBottom);
      }
      spacerTop.children[0].style.height = rowTop(v.r0) - frozenHeight(v.fr) + "px";
      spacerBottom.children[0].style.height = rowsHeightFrom(v.r1 + 1) + "px";
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

    // A hidden row has no `tr`: an auto filter can hide nearly every row, so a window of visible rows spans thousands.
    const rowIsHidden = (r) => Boolean(layoutApi) && layoutApi.rowHidden(getSheetName(), r);

    // Drops rows outside the window and refreshes the survivors; returns them keyed by row number.
    function reconcileExistingRows(tbody, next, sameCols) {
      const existing = new Map();
      for (const tr of Array.from(tbody.children)) {
        if (tr.className && String(tr.className).includes("sheet-spacer")) continue;
        const r = rowOf(tr);
        if ((r < next.r0 && !(next.layout && r <= next.fr)) || r > next.r1 || r === 0 || rowIsHidden(r)) {
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
      // Each new row goes before the next row that is already there, else at the end.
      const later = [...existing.keys()].sort((a, b) => a - b);
      let at = 0;
      const wanted = [];
      for (let r = 1; next.layout && r <= next.fr; r += 1) wanted.push(r);
      for (let r = next.r0; r <= next.r1; r += 1) wanted.push(r);
      for (const r of wanted) {
        while (at < later.length && later[at] < r) at += 1;
        if (existing.has(r) || rowIsHidden(r)) continue;
        const tr = buildRow(r, next);
        // A frozen row with no frozen row after it goes before the top spacer.
        if (next.layout && r <= next.fr && (at >= later.length || later[at] > next.fr) && spacerTop?.parentElement === tbody) spacerTop.before(tr);
        else if (at < later.length) existing.get(later[at]).before(tr);
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
          if (rowIsHidden(r)) continue;
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
        Boolean(prev) && prev.c0 === next.c0 && prev.c1 === next.c1 && prev.layout === next.layout && prev.fc === next.fc;
      const sameRows = Boolean(prev) && prev.r0 === next.r0 && prev.r1 === next.r1 && prev.fr === next.fr;
      if (!force && sameCols && sameRows) return false;
      if (willDropFocus(next, sameCols) && hooks.beforeStructuralChange) hooks.beforeStructuralChange();
      view = next;
      // A forced render also rebuilds the header: column widths are per sheet (tab switch, file open).
      if (!sameCols || force) rebuildHead(headRow, next);

      const existing = reconcileExistingRows(tbody, next, sameCols);
      syncSpacers(tbody, next);
      insertMissingRows(tbody, existing, next);
      applyRowHeights(tbody);
      syncTableWidth(table, next);
      applyLayoutClasses();
      freeze?.paint(headRow, Array.from(tbody.children, (tr) => [rowOf(tr), tr]), next);
      if (hooks.afterRender) hooks.afterRender();
      prefetchNumFmt(next);
      return true;
    }

    const applyLayoutClasses = host.__EASYOFFICE_CREATE_SHEET_GRID_LAYOUT_PAINT__({
      layoutApi, layoutOf, getSheetName, tdByPacked, colHeadByCol, rowHeadByRow, seedDomMaps, colLeft, rowTop,
    });

    // The layout changed (hide, unhide, merge, edit): redraw with the new row and column geometry.
    function layoutChanged() {
      widthCacheSheet = null;
      if (wrapRowsPossible()) syncRows();
      const rendered = renderWindow(true);
      hooks.afterLayout?.();
      return rendered;
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

    const paintSelectionRange = host.__EASYOFFICE_CREATE_SHEET_GRID_SELECTION__({ tdByPacked, colHeadByCol, rowHeadByRow, seedDomMaps, getView: () => view });

    // Repaints every materialized cell from the model (keeps the elements, so focus survives).
    // `isTyping(td)` marks a cell that keeps the text it is typed in.
    function refreshCells(isTyping = null) {
      const tbody = tbodyEl();
      if (!tbody) return [];
      seedDomMaps();
      const tds = [];
      for (const td of tdByPacked.values()) {
        paintTd(td, getEntry(td.dataset.cell), isTyping ? isTyping(td) : false);
        tds.push(td);
      }
      applyLayoutClasses();
      if (wrapRowsPossible() && syncRows()) renderWindow(true);
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
        // A frozen row or column is always in view; the others scroll clear of the frozen ones.
        const { rows: fr, cols: fc } = frozenCounts();
        const viewH = vp.clientHeight - HEAD_H;
        const top = rowTop(row);
        const bottom = top + (rowFit ? rowFit.heightOf(row) : rowBase());
        if (row > fr && top < vp.scrollTop + frozenHeight(fr)) vp.scrollTop = top - frozenHeight(fr);
        else if (row > fr && bottom > vp.scrollTop + viewH) vp.scrollTop = bottom - viewH;
        const left = colLeft(col);
        const viewW = vp.clientWidth - ROW_HEAD_W;
        if (col > fc && left < vp.scrollLeft + frozenWidth(fc)) vp.scrollLeft = left - frozenWidth(fc);
        else if (col > fc && left + colWidth(col) > vp.scrollLeft + viewW) vp.scrollLeft = left + colWidth(col) - viewW;
      }
      return renderWindow(false);
    }

    // Applies a new column width to the live header cell and the table width.
    setColumnWidthLookup(colWidth);

    // Sets the widths of several columns (`[[column, px], ...]`) and brings the page up to date once: the row heights of wrapped
    // cells are worked out one time for the lot, not once per column. Returns the widths as set.
    function applyColWidths(list) {
      const set = list.map(([c, px]) => {
        const w = setColWidth(c, px);
        repaintNumFmtColumn(c);
        const th = colHeadByCol.get(c) || document.querySelector('#pane-sheet th[data-col="' + colName(c) + '"]');
        if (th) th.style.width = w + "px";
        return w;
      });
      if (wrapRowsPossible() && syncRows()) renderWindow(true);
      const table = tableEl();
      if (table && view) syncTableWidth(table, view);
      return set;
    }

    const applyColWidth = (c, px) => applyColWidths([[c, px]])[0];

    function visibleRowSpan() {
      const vp = viewportEl();
      if (hasLayout(vp)) return Math.max(1, Math.floor((vp.clientHeight - HEAD_H) / rowBase()) - 1);
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

    // Pictures and charts on the content of the grid (PD-299). `anchorAt` gives the cells a rectangle of the content (`gx`, `gy`
    // from the top left of the first cell, in px) starts and ends on, with the EMU offsets in them, as a two-cell anchor says it.
    const emuPerPx = () => layoutApi?.EMU_PER_PX || 9525;
    const emuToPx = (emu) => Math.round((Number(emu) || 0) / emuPerPx());
    const rowAtContentY = (gy) => Math.min(MAX_ROWS, rowAtY(Math.max(0, gy)));

    function anchorAt(gxRaw, gyRaw, w, h) {
      const emu = emuPerPx();
      const gx = Math.max(0, gxRaw);
      const gy = Math.max(0, gyRaw);
      const c1 = colAtX(gx);
      const r1 = rowAtContentY(gy);
      const fromCol = Math.max(0, Math.min(MAX_COLS - 1, c1 - 1));
      const fromRow = Math.max(0, Math.min(MAX_ROWS - 1, r1 - 1));
      const gx1 = gx + w;
      const gy1 = gy + h;
      const toCol = Math.max(fromCol, Math.min(MAX_COLS - 1, colAtX(gx1) - 1));
      const toRow = Math.max(fromRow, Math.min(MAX_ROWS - 1, rowAtContentY(gy1) - 1));
      const off = (v) => Math.max(0, Math.round(Math.max(0, v) * emu));
      return {
        fromCol,
        fromRow,
        colOffEmu: off(gx - colLeft(c1)),
        rowOffEmu: off(gy - rowTop(r1)),
        toCol,
        toRow,
        toColOffEmu: off(gx1 - colLeft(toCol + 1)),
        toRowOffEmu: off(gy1 - rowTop(toRow + 1)),
      };
    }

    // The rectangle of the content an anchor lies on, from `anchor` and the size it keeps (`stored`: `left`, `top`, `width`,
    // `height` in px). A two-cell anchor (`editAs` missing or `twoCell`) is as large as its cells, and so grows and shrinks
    // with the rows and columns; a one-cell anchor starts on its cell and keeps its size; an absolute anchor keeps its place.
    function contentRectOf(anchor, stored) {
      if (anchor.editAs === "absolute") return { gx: stored.left, gy: stored.top, w: stored.width, h: stored.height };
      const gx = colLeft(anchor.fromCol + 1) + emuToPx(anchor.colOffEmu);
      const gy = rowTop(anchor.fromRow + 1) + emuToPx(anchor.rowOffEmu);
      if (anchor.editAs === "oneCell") return { gx, gy, w: stored.width, h: stored.height };
      const w = colLeft(anchor.toCol + 1) + emuToPx(anchor.toColOffEmu) - gx;
      const h = rowTop(anchor.toRow + 1) + emuToPx(anchor.toRowOffEmu) - gy;
      // An end that is not past the start (an anchor that has no end offsets) has no size of its cells: it keeps its own.
      return { gx, gy, w: w > 0 ? w : stored.width, h: h > 0 ? h : stored.height };
    }

    return {
      MAX_ROWS, MAX_COLS, get ROW_H() { return rowBase(); }, ROW_HEAD_W, HEAD_H, rowTop, rowAtY, anchorAt, contentRectOf, get DEFAULT_COL_W() { return defaultColW(); },
      colName, colIndex, packCoord, packRef, parseRef, refOf, colWidth, colLeft, colAtX,
      applyColWidth, applyColWidths, layoutChanged, rowAtVisible, MIN_COL_W, MAX_COL_W, rowHeight: (r) => rowFit?.heightOf(r) ?? rowBase(), rowFitOverBudget: () => Boolean(rowFit?.overBudget()),
      measureText: (font, family, text) => measureLine(sampleTd(), font, family, text),
      mergeAt: (r, c) => layoutApi?.mergeAt(getSheetName(), r, c) || null,
      mergesIn: (r1, c1, r2, c2) => layoutApi?.mergesIn(getSheetName(), r1, c1, r2, c2) ?? [],
      rowHidden: (r) => Boolean(layoutApi?.rowHidden(getSheetName(), r)),
      colHidden: (c) => Boolean(layoutApi?.colHidden(getSheetName(), c)),
      getAllWidths, replaceAllWidths, repaintFx, repaintNumFmt, renderWindow, refreshCells,
      ensureVisible, visibleRowSpan, getTdAt, getTdByRef, forEachMaterializedTd,
      paintSelectionRange, paintTd, setDisplayValue, readTdValue, install, getView: () => view,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_GRID__ = createSheetGrid;
  host.__EASYOFFICE_SHEET_GRID_UTILS__ = {
    MAX_ROWS, MAX_COLS, colName, colIndex, packCoord, packRef, parseRef, refOf, isNumericText, formatGeneral, cloneSheetStore,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
