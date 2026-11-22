// 데이터시트 보기의 행 창: 보이는 행과 위아래 여유분만 DOM에 두고 나머지는 위아래 빈 행의 높이로 대신한다 (PD-290).
// 범위 계산은 `base-virtual.js` (TanStack Virtual `virtual-core`, MIT; see `THIRD_PARTY_NOTICES.md`), 이 파일은 EasyOffice 자체 코드다.

(function (host) {
  "use strict";

  // Up to this many rows and group rows the grid is drawn whole: the table looks and behaves as it always did.
  const WINDOW_MIN_ITEMS = 120;
  // Rows kept above and below the visible ones, so a scroll of a few rows moves no node.
  const OVERSCAN = 12;
  // Until the real heights are measured (and where there is no layout at all): the height of a row (`base.css`), of the header, of the view.
  const ROW_PX = 24, HEAD_PX = 26, FOOT_PX = 24, VIEW_PX = 800, CORNER_PX = 56, DEFAULT_COL_PX = 144;
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

  // opts: groups, grouped, colSpan, buildGroupRow(group), buildDataRow(rowData, recordIndex), scrollKey, widthKey, onRowBuilt(tr, item) (optional),
  // memo: what outlives a redraw of the table ({ key, top, left, sizes, rowPx, groupPx, margin, foot }, and widths: a Map).
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
    const baseRowPx = opts.rowHeight || ROW_PX;
    const hasExplicitWidths = Boolean(opts.fields?.some((f) => f.width !== undefined));
    if (memo.key !== opts.scrollKey) Object.assign(memo, { key: opts.scrollKey, top: 0, left: 0, sizes: null, rowPx: 0, groupPx: 0, cfgRowPx: baseRowPx, margin: HEAD_PX, foot: FOOT_PX });
    else if (memo.cfgRowPx !== baseRowPx) Object.assign(memo, { sizes: null, rowPx: 0, groupPx: 0, cfgRowPx: baseRowPx });
    if (!(memo.margin > 0)) memo.margin = HEAD_PX;
    if (!(memo.foot > 0)) memo.foot = FOOT_PX;
    if (memo.sizes?.length !== count) memo.sizes = new Float64Array(count);
    const sizes = memo.sizes;
    let wrap = null, tblEl = null, flat = null, painted = null, disposed = false, measured = false, observer = null, topPx = 0, trailing = 0;
    let rowPx = memo.rowPx || baseRowPx, groupPx = memo.groupPx || ROW_PX;

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
        opts.onRowBuilt?.(tr, i);
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

    // Column widths combine measured auto widths (`opts.widths`) with user-set `field.width`.
    function fixColumns(forceMeasure = false) {
      let autoWidths = opts.widths.get(opts.widthKey);
      if (!autoWidths || forceMeasure) {
        if (tblEl.firstChild?.tagName === "COLGROUP") tblEl.firstChild.remove();
        tblEl.classList.remove("is-fixed");
        tblEl.style.width = "";
        const domWidths = Array.from(tblEl.querySelectorAll("thead th"), (th) => Math.round(th.getBoundingClientRect().width * LAYOUT_UNITS) / LAYOUT_UNITS);
        if (domWidths.length === opts.colSpan && domWidths.every((w) => w > 0)) {
          autoWidths = autoWidths && opts.growOnly ? autoWidths.map((pw, idx) => Math.max(pw, domWidths[idx])) : domWidths;
          opts.widths.set(opts.widthKey, autoWidths);
        } else if (!autoWidths) {
          if (!hasExplicitWidths) return;
          autoWidths = [CORNER_PX, ...(opts.fields || []).map(() => DEFAULT_COL_PX)];
        }
      }
      if (tblEl.firstChild?.tagName === "COLGROUP") tblEl.firstChild.remove();
      const widths = autoWidths.map((aw, idx) => (idx > 0 && opts.fields?.[idx - 1]?.width !== undefined ? opts.fields[idx - 1].width : aw));
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

    // Measure sticky chrome, painted row heights (for both short and windowed tables), and column widths once layout exists.
    function measureLayout() {
      if (measured || disposed || !hasLayout()) return;
      measured = true;
      const cached = opts.widths.has(opts.widthKey), fresh = !memo.rowPx;
      memo.margin = tblEl.querySelector("thead")?.getBoundingClientRect().height || memo.margin;
      memo.foot = tblEl.querySelector("tfoot")?.getBoundingClientRect().height || memo.foot;
      if (painted) {
        const means = { row: [], group: [] };
        for (let i = painted.from; i <= painted.to; i += 1) {
          const h = rowsByIndex.has(i) ? heightOf(rowsByIndex.get(i)) : 0;
          if (h > 0) (isGroupRow(i) ? means.group : means.row).push(h);
        }
        const mean = (list, fallback) => (list.length ? list.reduce((a, b) => a + b, 0) / list.length : fallback);
        if (fresh) { rowPx = mean(means.row, rowPx); groupPx = mean(means.group, groupPx); Object.assign(memo, { rowPx, groupPx }); }
        relayout();
        const shift = measureRows(painted.from, painted.to, painted.from);
        if (windowed) {
          topPx = flat[painted.from * 2] - memo.margin;
          setSpacer(top, topPx);
          setSpacer(bottom, V.listEnd(flat, count) - (flat[painted.to * 2] + flat[painted.to * 2 + 1]));
          if (shift) wrap.scrollTop += shift;
        }
      }
      if ((windowed && (!cached || opts.growOnly)) || hasExplicitWidths) {
        fixColumns(windowed && (!cached || Boolean(opts.growOnly)));
        memo.margin = tblEl.querySelector("thead")?.getBoundingClientRect().height || memo.margin;
        memo.foot = tblEl.querySelector("tfoot")?.getBoundingClientRect().height || memo.foot;
      }
      trailing = count > 0 ? Math.max((wrap.scrollHeight || 0) - V.listEnd(flat, count), 0) : 0;
      if (windowed) sync();
    }

    function onScroll() {
      memo.top = wrap.scrollTop; memo.left = wrap.scrollLeft;
      if (windowed) sync();
    }

    function previewRowHeight(px) {
      if (disposed || !tblEl) return;
      tblEl.style.setProperty("--base-row-height", px + "px");
      tblEl.dataset.rowHeight = String(px);
      rowPx = px; memo.rowPx = px; memo.cfgRowPx = px; sizes.fill(0);
      relayout();
      if (windowed && painted) {
        topPx = flat[painted.from * 2] - memo.margin;
        setSpacer(top, topPx);
        setSpacer(bottom, V.listEnd(flat, count) - (flat[painted.to * 2] + flat[painted.to * 2 + 1]));
        sync();
      }
      if (wrap) trailing = count > 0 ? Math.max((wrap.scrollHeight || 0) - V.listEnd(flat, count), 0) : 0;
    }

    function attach(wrapEl, tableEl) {
      wrap = wrapEl; tblEl = tableEl;
      relayout();
      if (windowed) {
        if (opts.widths.has(opts.widthKey) || hasExplicitWidths) fixColumns(false);
        setSpacer(bottom, V.listEnd(flat, count) - memo.margin);
      } else {
        if (count > 0) paint(0, count - 1, 0);
        if (hasExplicitWidths) fixColumns(false);
      }
      wrap.scrollTop = memo.top; wrap.scrollLeft = memo.left;
      wrap.addEventListener("scroll", onScroll, { passive: true });
      if (windowed) sync();
      measureLayout();
      const Observer = doc.defaultView?.ResizeObserver || host.ResizeObserver;
      if (typeof Observer === "function") { observer = new Observer(() => { if (!measured) measureLayout(); if (windowed) sync(); }); observer.observe(wrap); }
    }

    function dispose() {
      disposed = true;
      observer?.disconnect();
      wrap?.removeEventListener("scroll", onScroll);
    }

    // ---- what the cell selection asks of the list (PD-294) ----
    const rowAt = (i) => rowsByIndex.get(i);
    const itemOf = (tr) => indexByRow.get(tr);
    // The records of the item `i` (null for a group row).
    const recordAt = (i) => {
      const seg = segmentAt(i), k = i - seg.start - (seg.headed ? 1 : 0);
      return k < 0 ? null : seg.group.records[k];
    };
    const builtItems = () => painted;
    const scrollOffset = () => wrap.scrollTop || 0;
    // What the sticky footer and the new record row below the list add to the scroll height: the measured value, or what the element holds beyond the list.
    const tailPx = () => trailing || Math.max((wrap.scrollHeight || 0) - V.listEnd(flat, count), 0);

    // Brings item `i` into view the way `scrollToIndex` of the original does: the offset is worked out from the heights known so far, the
    // rows that scroll brings in are measured, and the offset is worked out again (up to three times) because a row that is taller than
    // estimated moves the item. The rows are built at once (not on the scroll event), so the item is in the DOM when this returns.
    function scrollToItem(i, align = "auto") {
      if (disposed || !wrap || count === 0) return;
      for (let pass = 0; pass < 3; pass += 1) {
        const size = wrap.clientHeight || VIEW_PX, offset = scrollOffset();
        const view = { offset, size, padStart: memo.margin, padEnd: memo.foot, max: V.listEnd(flat, count) + tailPx() - size };
        const to = V.offsetForIndex(flat, count, i, align, view);
        if (to === null || Math.abs(to - offset) < 1 / LAYOUT_UNITS) break;
        wrap.scrollTop = to;
        sync();
      }
      sync();
    }

    // The rows a page holds: as many of the usual row height as fit between the sticky header and the sticky footer, at least one.
    const visibleSpan = () => Math.max(Math.floor(((wrap.clientHeight || VIEW_PX) - memo.margin - memo.foot) / rowPx), 1);

    return { tbody, count, attach, dispose, rowAt, itemOf, recordAt, isGroup: isGroupRow, builtItems, scrollToItem, visibleSpan, previewRowHeight };
  }

  host.__EASYOFFICE_BASE_GRID_WINDOW__ = Object.freeze({ createGridWindow, WINDOW_MIN_ITEMS, OVERSCAN });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
