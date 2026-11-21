// 데이터시트 보기의 행 창: 보이는 행과 위아래 여유분만 DOM에 두고 나머지는 위아래 빈 행의 높이로 대신한다 (PD-290).
// 범위 계산은 `base-virtual.js` (TanStack Virtual `virtual-core`, MIT; see `THIRD_PARTY_NOTICES.md`), 이 파일은 EasyOffice 자체 코드다.

(function (host) {
  "use strict";

  // Up to this many rows and group rows the grid is drawn whole: the table looks and behaves as it always did.
  const WINDOW_MIN_ITEMS = 120;
  // Rows kept above and below the visible ones, so a scroll of a few rows moves no node.
  const OVERSCAN = 12;
  // Until the real heights are measured (and where there is no layout at all): the height of a row (`base.css`), of the header, of the view.
  const ROW_PX = 24, HEAD_PX = 26, VIEW_PX = 800;
  // Browsers lay out in 1/64 px: a height read from a rect is rounded to that step, and a smaller difference is no difference.
  const LAYOUT_UNITS = 64;
  // aria-rowindex: the header row is 1, the items follow, then the new record row and the footer.
  const ROWS_BEFORE_ITEMS = 2;
  // A row that is not the height of its neighbours (a relation cell with chips is taller) is measured when it is built, so the
  // list never rests on one height for all rows. Before a row has been built, its height is the estimate.

  // The items of the grid are the group rows (when it is grouped) and the records of the groups that are not collapsed.
  function planSegments(groups, grouped) {
    const segments = [], starts = [];
    let count = 0, records = 0;
    for (const group of groups) {
      segments.push({ group, start: count, headed: grouped, recordBase: records });
      starts.push(count);
      const shown = group.collapsed ? 0 : group.records.length;
      count += (grouped ? 1 : 0) + shown; records += shown;
    }
    return { segments, starts, count };
  }

  // opts: groups, grouped, colSpan, buildGroupRow(group), buildDataRow(rowData, recordIndex), scrollKey, widthKey,
  // memo: what outlives a redraw of the table ({ key, top, left, sizes, rowPx, groupPx, margin }, and widths: a Map).
  function createGridWindow(doc, opts) {
    const V = host.__EASYOFFICE_BASE_VIRTUAL__, memo = opts.memo;
    const plan = planSegments(opts.groups, opts.grouped), count = plan.count, windowed = count > WINDOW_MIN_ITEMS;
    const tbody = doc.createElement("tbody"), rowsByIndex = new Map(), indexByRow = new WeakMap();
    const makeSpacer = () => {
      const tr = doc.createElement("tr"), td = doc.createElement("td");
      tr.className = "base-grid-spacer"; tr.hidden = true; tr.setAttribute("aria-hidden", "true");
      td.colSpan = opts.colSpan; tr.appendChild(td);
      return tr;
    };
    const top = makeSpacer(), bottom = makeSpacer();
    tbody.appendChild(top); tbody.appendChild(bottom);

    // A new key (another table, a new document) starts at the top with nothing measured; a redraw of the same table keeps what it knew.
    // The measured heights are by item: a list of another length starts them again.
    if (memo.key !== opts.scrollKey) Object.assign(memo, { key: opts.scrollKey, top: 0, left: 0, sizes: null, rowPx: 0, groupPx: 0, margin: HEAD_PX });
    if (memo.sizes?.length !== count) memo.sizes = new Float64Array(count);
    const sizes = memo.sizes;
    let wrap = null, tblEl = null, flat = null, painted = null, disposed = false, measured = false, observer = null, topPx = 0, trailing = 0;
    let rowPx = memo.rowPx || ROW_PX, groupPx = memo.groupPx || rowPx;

    const segmentAt = (i) => plan.segments[V.findNearestBinarySearch(0, plan.segments.length - 1, (k) => plan.starts[k], i)];
    const isGroupRow = (i) => opts.grouped && segmentAt(i).start === i;
    const sizeAt = (i) => sizes[i] || (opts.grouped && isGroupRow(i) ? groupPx : rowPx);
    const setSpacer = (tr, px) => { tr.hidden = !(px > 0); tr.firstChild.style.height = px + "px"; };
    const hasLayout = () => wrap.clientHeight > 0;
    const relayout = () => { flat = V.layoutItems(count, sizeAt, memo.margin, flat); };
    const heightOf = (tr) => Math.round(tr.getBoundingClientRect().height * LAYOUT_UNITS) / LAYOUT_UNITS;

    function buildItem(i) {
      const seg = segmentAt(i), k = i - seg.start;
      if (seg.headed && k === 0) return opts.buildGroupRow(seg.group);
      const j = k - (seg.headed ? 1 : 0);
      return opts.buildDataRow(seg.group.records[j], seg.recordBase + j);
    }

    function dropRows(from, to) {
      for (let i = from; i <= to; i += 1) {
        const tr = rowsByIndex.get(i);
        if (tr) { tr.remove(); rowsByIndex.delete(i); }
      }
    }

    function makeRows(from, to, before) {
      for (let i = from; i <= to; i += 1) {
        const tr = buildItem(i);
        tr.setAttribute("aria-rowindex", String(i + ROWS_BEFORE_ITEMS));
        rowsByIndex.set(i, tr); indexByRow.set(tr, i);
        tbody.insertBefore(tr, before);
      }
    }

    // The real heights of the rows from..to. When one differs from what the list assumed, the list is laid out again; the rows above the
    // first visible one (`anchor`) that grew or shrank would move what is on screen, so the scroll offset follows by the same amount.
    function measureRows(from, to, anchor) {
      if (!hasLayout()) return 0;
      let dirty = false;
      for (let i = from; i <= to; i += 1) {
        const tr = rowsByIndex.get(i), h = tr ? heightOf(tr) : 0;
        if (h > 0 && Math.abs(h - sizeAt(i)) >= 1 / LAYOUT_UNITS) dirty = true;
        if (h > 0) sizes[i] = h;
      }
      if (!dirty) return 0;
      const before = flat[anchor * 2];
      relayout();
      return flat[anchor * 2] - before;
    }

    // A control that is being edited in a row that leaves the DOM would lose what was typed: it leaves the control first, which commits it.
    function releaseFocus(from, to) {
      const active = doc.activeElement, tr = active?.closest?.("tr"), index = tr ? indexByRow.get(tr) : undefined;
      if (index !== undefined && (index < from || index > to)) active.blur?.();
    }

    function paint(from, to, anchor) {
      if (painted) {
        releaseFocus(from, to);
        if (disposed) return;
        if (from > painted.to || to < painted.from) { dropRows(painted.from, painted.to); painted = null; }
        else { dropRows(painted.from, from - 1); dropRows(to + 1, painted.to); }
      }
      const keepFrom = painted ? Math.max(painted.from, from) : to + 1, keepTo = painted ? Math.min(painted.to, to) : to;
      makeRows(from, Math.min(keepFrom - 1, to), rowsByIndex.get(keepFrom) || bottom);
      makeRows(Math.max(keepTo + 1, from), to, bottom);
      const shift = measureRows(from, to, anchor);
      painted = { from, to };
      topPx = flat[from * 2] - memo.margin;
      setSpacer(top, topPx);
      setSpacer(bottom, V.listEnd(flat, count) - (flat[to * 2] + flat[to * 2 + 1]));
      if (shift) wrap.scrollTop += shift;
    }

    // Paints until the rows in the DOM cover the view (a row that turned out taller or shorter can leave a gap the first time).
    function sync() {
      for (let pass = 0; pass < 3 && !disposed && count > 0; pass += 1) {
        const range = V.calculateRange(flat, count, wrap.clientHeight || VIEW_PX, wrap.scrollTop || 0, trailing);
        if (!range || (painted && range.startIndex >= painted.from && range.endIndex <= painted.to)) return;
        const { from, to } = V.extendRange(range, OVERSCAN, count);
        paint(from, to, range.startIndex);
      }
    }

    // Column widths are fixed from the first painted window: the columns keep their widths while the rows that set them scroll away.
    function fixColumns() {
      let widths = opts.widths.get(opts.widthKey);
      if (!widths) {
        widths = Array.from(tblEl.querySelectorAll("thead th"), (th) => th.getBoundingClientRect().width);
        if (widths.length === 0 || widths.some((w) => !(w > 0))) return;
        opts.widths.set(opts.widthKey, widths);
      }
      const colgroup = doc.createElement("colgroup");
      for (const w of widths) {
        const col = doc.createElement("col");
        col.style.width = w + "px";
        colgroup.appendChild(col);
      }
      tblEl.insertBefore(colgroup, tblEl.firstChild);
      tblEl.style.width = widths.reduce((a, b) => a + b, 0) + "px";
      tblEl.classList.add("is-fixed");
    }

    // Everything that needs a layout, once, as soon as the pane has one (a pane that is hidden has none): the height of the header above the
    // list, the estimate for the rows that are not built yet (the mean of the first window), the column widths and what follows the list.
    function measureLayout() {
      if (measured || disposed || !windowed || !painted || !hasLayout()) return;
      measured = true;
      const cached = opts.widths.has(opts.widthKey), fresh = !memo.rowPx;
      memo.margin = tblEl.querySelector("thead").getBoundingClientRect().height || memo.margin;
      const means = { row: [], group: [] };
      for (let i = painted.from; i <= painted.to; i += 1) {
        const h = rowsByIndex.has(i) ? heightOf(rowsByIndex.get(i)) : 0;
        if (h > 0) (isGroupRow(i) ? means.group : means.row).push(h);
      }
      const mean = (list, fallback) => (list.length ? list.reduce((a, b) => a + b, 0) / list.length : fallback);
      if (fresh) { rowPx = mean(means.row, rowPx); groupPx = mean(means.group, rowPx); Object.assign(memo, { rowPx, groupPx }); }
      relayout();
      const shift = measureRows(painted.from, painted.to, painted.from);
      topPx = flat[painted.from * 2] - memo.margin;
      setSpacer(top, topPx);
      setSpacer(bottom, V.listEnd(flat, count) - (flat[painted.to * 2] + flat[painted.to * 2 + 1]));
      if (shift) wrap.scrollTop += shift;
      if (!cached) fixColumns();
      trailing = Math.max(wrap.scrollHeight - V.listEnd(flat, count), 0);
      sync();
    }

    function onScroll() {
      memo.top = wrap.scrollTop; memo.left = wrap.scrollLeft;
      if (windowed) sync();
    }

    function attach(wrapEl, tableEl) {
      wrap = wrapEl; tblEl = tableEl;
      relayout();
      if (windowed) {
        if (opts.widths.has(opts.widthKey)) fixColumns();
        setSpacer(bottom, V.listEnd(flat, count) - memo.margin);
      } else if (count > 0) paint(0, count - 1, 0);
      wrap.scrollTop = memo.top; wrap.scrollLeft = memo.left;
      wrap.addEventListener("scroll", onScroll, { passive: true });
      if (!windowed) return;
      sync();
      measureLayout();
      const Observer = doc.defaultView?.ResizeObserver || host.ResizeObserver;
      if (typeof Observer === "function") { observer = new Observer(() => { sync(); measureLayout(); }); observer.observe(wrap); }
    }

    function dispose() {
      disposed = true;
      observer?.disconnect();
      wrap?.removeEventListener("scroll", onScroll);
    }

    return { tbody, count, attach, dispose };
  }

  host.__EASYOFFICE_BASE_GRID_WINDOW__ = Object.freeze({ createGridWindow, WINDOW_MIN_ITEMS, OVERSCAN });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
