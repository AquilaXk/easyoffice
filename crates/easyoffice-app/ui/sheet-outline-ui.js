(function (host) {
  "use strict";

  // The outline of the sheet window (PD-208): a bar and a button for every group beside the row
  // numbers (rows) and above the column letters (columns), and the level buttons 1 to n+1 in the
  // corner of each. The bars follow the scroll, so only the groups in view are drawn.

  const UNIT = 14; // one level lane, and one level button
  const BUTTON = 11; // the box of a group's button, in the lane
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const outlineApi = host.__EASYOFFICE_SHEET_OUTLINE__;

  const LABELS = { row: "행", col: "열" };

  function createSheetOutlineUi({ document, grid, view, getActiveSheetName }) {
    const $ = (id) => document.getElementById(id);
    const frame = () => $("sheet-frame");
    const viewport = () => $("sheet-viewport");

    function el(tag, className, parent) {
      const node = document.createElement(tag);
      node.className = className;
      parent?.appendChild(node);
      return node;
    }

    // Size of the first `n` rows (columns), which stay in place while the others scroll.
    const pinned = (axis, outlineFrozen) => (outlineFrozen > 0 ? (axis === "row" ? grid.rowTop(outlineFrozen + 1) : grid.colLeft(outlineFrozen + 1)) : 0);

    function button(parent, axis, group, folded, at) {
      const b = el("button", "outline-toggle" + (folded ? " folded" : ""), parent);
      b.type = "button";
      b.tabIndex = 0;
      b.dataset.axis = axis;
      b.dataset.level = String(group.level);
      b.dataset.first = String(group.first);
      b.dataset.last = String(group.last);
      b.dataset.folded = String(folded);
      const text = folded ? "하위 수준 표시" : "하위 수준 숨기기";
      b.title = text;
      b.setAttribute("aria-label", LABELS[axis] + " " + group.first + "~" + group.last + " " + text);
      b.setAttribute("aria-expanded", String(!folded));
      const lane = (group.level - 1) * UNIT + (UNIT - BUTTON) / 2;
      if (axis === "row") {
        b.style.left = lane + "px";
        b.style.top = at - BUTTON / 2 + "px";
      } else {
        b.style.top = lane + "px";
        b.style.left = at - BUTTON / 2 + "px";
      }
    }

    function bar(parent, axis, group, from, to) {
      if (to - from < 1) return;
      const line = el("div", "outline-bar", parent);
      const lane = (group.level - 1) * UNIT + UNIT / 2;
      if (axis === "row") {
        line.style.left = lane + "px";
        line.style.top = from + "px";
        line.style.height = to - from + "px";
      } else {
        line.style.top = lane + "px";
        line.style.left = from + "px";
        line.style.width = to - from + "px";
      }
    }

    // The row (column) at offset `x` of the scrolled content, by binary search on its position.
    function indexAt(axis, x) {
      if (axis === "col") return grid.colAtX(x);
      let lo = 1;
      let hi = grid.MAX_ROWS;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (grid.rowTop(mid) <= x) lo = mid;
        else hi = mid - 1;
      }
      return lo;
    }

    function levelButtons(axis, count) {
      const box = el("div", "outline-levels outline-levels-" + axis, null);
      for (let n = 1; n <= count + 1; n += 1) {
        const b = el("button", "outline-level", box);
        b.type = "button";
        b.textContent = String(n);
        b.dataset.axis = axis;
        b.dataset.level = String(n);
        b.title = LABELS[axis] + " 수준 " + n;
        b.setAttribute("aria-label", LABELS[axis] + " 수준 " + n);
      }
      return box;
    }

    function render(axis) {
      const side = $(axis === "row" ? "sheet-outline-rows" : "sheet-outline-cols");
      const vp = viewport();
      if (!side || !vp) return;
      const name = getActiveSheetName();
      const outline = layoutApi.outlineOf(name);
      const count = outline ? outlineApi.maxLevel(outline, axis) : 0;
      const size = count > 0 ? (count + 1) * UNIT : 0;
      frame()?.style.setProperty(axis === "row" ? "--outline-w" : "--outline-h", size + "px");
      side.hidden = size === 0;
      if (size === 0) {
        side.replaceChildren();
        return;
      }
      const focused = document.activeElement;
      const keep = side.contains(focused) ? { ...focused.dataset } : null;
      const frozen = layoutApi.freezeOf(name);
      const fixed = axis === "row" ? frozen?.rows || 0 : frozen?.cols || 0;
      const edge = pinned(axis, fixed);
      const scroll = axis === "row" ? vp.scrollTop : vp.scrollLeft;
      const extent = axis === "row" ? vp.clientHeight : vp.clientWidth;
      const place = (i) => {
        const at = axis === "row" ? grid.rowTop(i) : grid.colLeft(i);
        return i <= fixed ? at : Math.max(at - scroll, edge);
      };
      const plane = el("div", "outline-plane", null);
      const lo = Math.max(1, indexAt(axis, scroll + edge) - 1);
      const hi = indexAt(axis, scroll + extent) + 1;
      // The groups of the pinned rows (columns) stay drawn whatever the scroll.
      const shown = new Set(fixed > 0 ? outlineApi.visibleGroups(outline, axis, 1, fixed) : []);
      for (const g of outlineApi.visibleGroups(outline, axis, lo, hi)) shown.add(g);
      for (const g of shown) {
        const folded = outlineApi.isFolded(outline, axis, g);
        if (!folded) bar(plane, axis, g, place(g.first) + 2, place(g.last + 1) - 2);
        const size_ = place(g.summary + 1) - place(g.summary);
        if (g.summary >= 1 && size_ > 0) button(plane, axis, g, folded, place(g.summary) + size_ / 2);
      }
      side.replaceChildren(levelButtons(axis, count), plane);
      if (keep?.level && keep.first) {
        const same = [...side.querySelectorAll(".outline-toggle")].find((b) => b.dataset.level === keep.level && b.dataset.first === keep.first);
        same?.focus?.();
      }
    }

    function renderAll() {
      render("row");
      render("col");
    }

    function onClick(ev) {
      const target = ev.target?.closest?.(".outline-toggle, .outline-level");
      if (!target) return;
      const { axis, level } = target.dataset;
      if (target.classList.contains("outline-level")) {
        void view.showLevel(axis, Number(level));
        return;
      }
      const outline = layoutApi.outlineOf(getActiveSheetName()) || {};
      const g = { level: Number(level), first: Number(target.dataset.first), last: Number(target.dataset.last) };
      void view.toggleGroup(axis, g, !outlineApi.isFolded(outline, axis, g));
    }

    function bind() {
      for (const id of ["sheet-outline-rows", "sheet-outline-cols"]) $(id)?.addEventListener("click", onClick);
      viewport()?.addEventListener("scroll", renderAll);
      layoutApi.onChange(renderAll);
      host.addEventListener?.("resize", renderAll);
      const RO = host.ResizeObserver;
      if (typeof RO === "function" && viewport()) new RO(renderAll).observe(viewport());
    }

    return { bind, render: renderAll };
  }

  host.__EASYOFFICE_CREATE_SHEET_OUTLINE_UI__ = createSheetOutlineUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
