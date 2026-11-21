(function (host) {
  "use strict";

  // Point mode of the formula editor (PD-279, PD-296): while a formula is typed in a cell or in the formula bar, a click, a drag
  // (over cells, or over the column or row headers), a Shift+click or an arrow key on the grid writes the reference of the cells
  // at the caret (after `=`, an operator, `(` or `,`). A merged range is written whole, and the cells of a table as its structured
  // reference where `sheet-formula-point.js` has one. A press where no reference is awaited, with the entry out on another
  // sheet, enters the formula on its own sheet and selects what it hit. A tab of another sheet puts the sheet in front of it,
  // F2 turns between the entry and the edit of the text, F4 cycles the `$` of the reference at the caret, and every reference
  // of the formula gets a colour that is drawn on the text and as a box on the grid. Which places take a reference, the colours and the F4 cycle are `sheet-formula-point.js`; this file ties them
  // to the two places a formula is edited. The pointed reference is a state of the edit (`point`): typing, the caret keys and a
  // click in the text end it.
  //
  // The colours are the series colours of the charts (`sheet-chart-svg.js`), set as `--sheet-ref-N` on the sheet pane. The text of
  // a cell is coloured with the CSS highlight registry, the text of the bar through a copy of it laid over the input.

  const MODE_LABEL = { enter: "입력", edit: "편집", point: "참조" };
  const ARROW_STEP = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] };
  const HIGHLIGHT_PREFIX = "eo-ref-";
  // Keys that keep a pointed reference alive: the movers, the modifiers and F4.
  const KEEP_POINT_KEYS = new Set(["Shift", "Control", "Meta", "Alt", "F4", ...Object.keys(ARROW_STEP)]);

  function createSheetPointUi(deps) {
    const { document, grid, feedback } = deps;
    const P = host.__EASYOFFICE_SHEET_FORMULA_POINT__;
    const byId = (id) => document.getElementById(id);
    const barEl = () => byId("formula-input");

    // The reference being pointed: its text in the formula (`start`..`end`), the cells it was pointed from and to, and `axis`
    // ("col" or "row" for a whole column or row).
    let point = null;
    let barMode = "edit";
    // The bar is an edit once its text was typed in; before that it only shows the cell.
    let barDirty = false;
    // The entry that went on in the formula bar while another sheet is shown: { sheet, ref, mode, fromBar }.
    let session = null;
    let dragging = null;
    let writing = false;
    let detaching = false;
    // A press that ends the entry out on another sheet is under way (see `leave`).
    let leaving = false;
    let painted = false;
    let layer = null;
    let boxes = [];
    let shown = { spans: [], colors: [] };
    let paletteSet = false;
    // The cells a structured reference of the entry stands for, by its text: `{ sheet, rg }`. The page wrote it from those
    // cells; one that was typed is not known and has no box.
    const known = new Map();

    const palette = () => host.__EASYOFFICE_SHEET_CHART_SVG__?.CHART_PALETTE ?? [];
    const colorCount = () => Math.max(1, palette().length);

    // ---- the two places a formula is edited ------------------------------------------------------------------------
    function caretOf(td) {
      const text = td.textContent || "";
      const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
      if (!sel || !sel.rangeCount || !td.contains(sel.anchorNode) || !td.contains(sel.focusNode)) return { from: text.length, to: text.length };
      const offset = (node, at) => {
        const range = document.createRange();
        range.selectNodeContents(td);
        range.setEnd(node, at);
        return range.toString().length;
      };
      const a = offset(sel.anchorNode, sel.anchorOffset);
      const b = offset(sel.focusNode, sel.focusOffset);
      return { from: Math.min(a, b), to: Math.max(a, b) };
    }

    function placeCaret(td, at) {
      const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
      if (!sel || !td.firstChild || typeof document.createRange !== "function") return;
      const range = document.createRange();
      range.setStart(td.firstChild, Math.min(at, td.firstChild.length));
      range.collapse(true);
      sel.removeAllRanges();
      sel.addRange(range);
    }

    function cellSurface(td) {
      return {
        kind: "cell",
        el: td,
        mode: () => deps.stateOf(td)?.mode || "edit",
        setMode: (mode) => deps.setMode(td, mode),
        text: () => td.textContent || "",
        read: () => ({ text: td.textContent || "", ...caretOf(td) }),
        write(text, caret) {
          td.textContent = text;
          deps.afterWrite(td);
          placeCaret(td, caret);
        },
      };
    }

    function barSurface(el) {
      return {
        kind: "bar",
        el,
        mode: () => barMode,
        setMode: (mode) => {
          barMode = mode;
        },
        text: () => el.value,
        read: () => ({ text: el.value, from: el.selectionStart ?? el.value.length, to: el.selectionEnd ?? el.value.length }),
        write(text, caret) {
          el.value = text;
          if (typeof el.setSelectionRange === "function") el.setSelectionRange(caret, caret);
        },
      };
    }

    // The place being edited: the formula bar when it has the focus (or holds an entry from another sheet), else the cell.
    function surface() {
      const bar = barEl();
      if (bar && (session || (barDirty && document.activeElement === bar))) return barSurface(bar);
      const td = deps.gridTdOf(document.activeElement);
      return td && deps.stateOf(td)?.mode && deps.editable(td) ? cellSurface(td) : null;
    }

    function contextOf(s) {
      const { text, from, to } = s.read();
      return P.pointContext(text, from, to);
    }

    // ---- status, colours, boxes --------------------------------------------------------------------------------------
    function applyPalette() {
      const pane = byId("pane-sheet");
      if (paletteSet || !pane) return;
      palette().forEach((color, i) => pane.style.setProperty("--sheet-ref-" + i, color));
      paletteSet = true;
    }

    function boxOf(rg, vp) {
      let left = grid.ROW_HEAD_W + grid.colLeft(rg.c1);
      let right = grid.ROW_HEAD_W + grid.colLeft(rg.c2 + 1);
      let top = grid.HEAD_H + grid.rowTop(rg.r1);
      let bottom = grid.HEAD_H + grid.rowTop(rg.r2 + 1);
      // A whole column or row is millions of pixels long: its box ends at the window.
      if (rg.axis === "col") {
        top = Math.max(top, vp.scrollTop);
        bottom = Math.min(bottom, vp.scrollTop + vp.clientHeight);
      } else if (rg.axis === "row") {
        left = Math.max(left, vp.scrollLeft);
        right = Math.min(right, vp.scrollLeft + vp.clientWidth);
      }
      return { left, top, width: Math.max(0, right - left), height: Math.max(0, bottom - top) };
    }

    // The sheet and the cells a reference of the formula points at, when it is on the sheet on show (else null).
    function targetOf(span) {
      const here = deps.getSheetName();
      const found = span.ref === null
        ? known.get(span.token)
        : { sheet: span.sheet ? P.sheetOf(span.sheet) : session ? session.sheet : here, rg: P.rangeOf(span.ref) };
      return found?.rg && found.sheet.toLowerCase() === here.toLowerCase() ? found.rg : null;
    }

    // The boxes of the references that point at the sheet on show.
    function paintBoxes() {
      const vp = byId("sheet-viewport");
      if (!vp) return;
      const items = [];
      shown.spans.forEach((span, i) => {
        const rg = targetOf(span);
        if (rg) items.push({ rg, color: shown.colors[i] });
      });
      if (!layer) {
        layer = document.createElement("div");
        layer.className = "sheet-ref-layer";
        layer.setAttribute("aria-hidden", "true");
        vp.addEventListener("scroll", paintBoxes);
      }
      if (layer.parentNode !== vp) vp.appendChild(layer);
      while (boxes.length > items.length) boxes.pop().remove();
      while (boxes.length < items.length) {
        const box = document.createElement("div");
        box.className = "sheet-ref-box";
        layer.appendChild(box);
        boxes.push(box);
      }
      items.forEach(({ rg, color }, i) => {
        const box = boxOf(rg, vp);
        boxes[i].dataset.color = String(color);
        Object.assign(boxes[i].style, {
          left: box.left + "px",
          top: box.top + "px",
          width: box.width + "px",
          height: box.height + "px",
        });
      });
    }

    // The text of a cell, one highlight per colour.
    function paintCellText(s, spans, colors) {
      const registry = host.CSS?.highlights;
      if (!registry || typeof host.Highlight !== "function") return;
      const node = s?.kind === "cell" && s.el.childNodes.length === 1 ? s.el.firstChild : null;
      const groups = new Map();
      if (node && node.nodeType === 3) {
        spans.forEach((span, i) => {
          if (span.end > node.length) return;
          const range = document.createRange();
          range.setStart(node, span.start);
          range.setEnd(node, span.end);
          groups.set(colors[i], [...(groups.get(colors[i]) ?? []), range]);
        });
      }
      for (let i = 0; i < colorCount(); i += 1) {
        if (groups.has(i)) registry.set(HIGHLIGHT_PREFIX + i, new host.Highlight(...groups.get(i)));
        else registry.delete(HIGHLIGHT_PREFIX + i);
      }
    }

    // The text of the bar: a copy laid over the input, whose own text is let go while the copy shows.
    function paintBarText(editing) {
      const bar = barEl();
      const copy = byId("sheet-formula-colors");
      if (!bar || !copy) return;
      const spans = editing ? P.colorSpans(bar.value) : [];
      const colors = P.colorIndexes(spans, colorCount());
      copy.hidden = spans.length === 0;
      bar.classList.toggle("colored", spans.length > 0);
      copy.textContent = "";
      if (spans.length === 0) return;
      // The copy sets its letters the way the input does.
      const look = typeof host.getComputedStyle === "function" ? host.getComputedStyle(bar) : null;
      if (look) {
        copy.style.font = look.font;
        copy.style.paddingLeft = (parseFloat(look.paddingLeft) || 0) + (parseFloat(look.borderLeftWidth) || 0) + "px";
      }
      const inner = document.createElement("span");
      inner.className = "sb-formula-colors-text";
      let at = 0;
      const piece = (text, color) => {
        const part = document.createElement("span");
        if (color !== null) part.dataset.color = String(color);
        part.textContent = text;
        inner.appendChild(part);
      };
      spans.forEach((span, i) => {
        piece(bar.value.slice(at, span.start), null);
        piece(span.token, colors[i]);
        at = span.end;
      });
      piece(bar.value.slice(at), null);
      copy.appendChild(inner);
      inner.style.transform = "translateX(" + -(bar.scrollLeft || 0) + "px)";
    }

    function refresh() {
      const s = surface();
      const status = byId("status-sheet-mode");
      if (status) {
        const label = s ? MODE_LABEL[point ? "point" : s.mode()] : "";
        status.hidden = label === "";
        if (status.textContent !== label) status.textContent = label;
      }
      if (!s && !painted) return;
      painted = Boolean(s);
      applyPalette();
      shown.spans = s ? P.colorSpans(s.text()) : [];
      shown.colors = P.colorIndexes(shown.spans, colorCount());
      paintBoxes();
      paintCellText(s, shown.spans, shown.colors);
      paintBarText(Boolean(s));
    }

    // ---- pointing ----------------------------------------------------------------------------------------------------
    // The sheet in front of a reference: none on the sheet the entry started on.
    function prefixOf() {
      const here = deps.getSheetName();
      return session && session.sheet !== here ? deps.quoteSheet(here) + "!" : "";
    }

    function rangeBetween(a, b, axis) {
      const last = axis === "col" ? { r: grid.MAX_ROWS } : axis === "row" ? { c: grid.MAX_COLS } : {};
      return {
        r1: axis === "col" ? 1 : Math.min(a.r, b.r),
        r2: last.r ?? Math.max(a.r, b.r),
        c1: axis === "row" ? 1 : Math.min(a.c, b.c),
        c2: last.c ?? Math.max(a.c, b.c),
      };
    }

    // The range grown over every merged range it touches, until no more is touched (the whole merged range is one cell).
    function withMerges(rg) {
      let box = rg;
      for (;;) {
        const grown = grid.mergesIn(box.r1, box.c1, box.r2, box.c2).reduce(
          (all, m) => ({ r1: Math.min(all.r1, m.r1), c1: Math.min(all.c1, m.c1), r2: Math.max(all.r2, m.r2), c2: Math.max(all.c2, m.c2) }),
          box
        );
        if (grown.r1 === box.r1 && grown.c1 === box.c1 && grown.r2 === box.r2 && grown.c2 === box.c2) return box;
        box = grown;
      }
    }

    // The cell of the entry, when it is on the sheet on show: a cell of the formula's own row is `#This Row`.
    function entryCell() {
      const here = deps.getSheetName();
      if (!session) return { ...deps.getActiveCell() };
      const at = session.sheet.toLowerCase() === here.toLowerCase() ? grid.parseRef(session.ref) : null;
      return at ? { r: at.row, c: at.col } : null;
    }

    // `Table1[Column]` and its kin for cells of the one table of the sheet on show that hold them, else null.
    function structuredOf(rg) {
      const sheet = deps.getSheetName();
      const tables = deps.tablesIn(sheet, rg.r1, rg.c1, rg.r2, rg.c2);
      return tables.length === 1 ? P.structuredRef(rg, tables[0], deps.tableBox(tables[0]), entryCell()) : null;
    }

    function writeRef(s, ctx, anchor, focus, axis) {
      const rg = axis ? rangeBetween(anchor, focus, axis) : withMerges(rangeBetween(anchor, focus, null));
      const structured = axis ? null : structuredOf(rg);
      if (structured) known.set(structured, { sheet: deps.getSheetName(), rg });
      const out = P.applyRef(s.read().text, ctx, structured ?? prefixOf() + P.rangeText(rg, axis));
      point = { start: out.start, end: out.end, anchor, focus, axis };
      writing = true;
      s.write(out.text, out.caret);
      writing = false;
      refresh();
    }

    function pointedCtx() {
      return { start: point.start, end: point.end };
    }

    // The cell a header stands for: the first of its column or row.
    const headCell = (head) => (head.axis === "col" ? { r: 1, c: head.index } : { r: head.index, c: 1 });

    // A press on a cell (`target` is `{ r, c }`) or on a header (`{ axis, index }`) of the grid while a formula is being edited.
    // Returns true when the press is a pointing and the edit keeps its focus. With the entry out on another sheet and the
    // formula finished, the press ends the entry (see `leave`).
    function press(ev, target) {
      const s = surface();
      if (!s) return false;
      const ctx = point && ev.shiftKey ? pointedCtx() : contextOf(s);
      if (ctx.mode === "none") {
        if (!session) return false;
        ev.preventDefault();
        void leave(target);
        return true;
      }
      ev.preventDefault();
      const axis = target.axis ?? null;
      const cell = axis ? headCell(target) : target;
      const anchor = ev.shiftKey ? (point?.anchor ?? deps.getActiveCell()) : cell;
      writeRef(s, ctx, anchor, cell, axis);
      dragging = { s, axis };
      return true;
    }

    // The mouse over a cell or a header (as `press` takes them) while the button is held: a drag that began on a cell goes on
    // over cells, one that began on a column or row header over the headers of that axis.
    function over(target) {
      if (!dragging || !point || point.axis !== dragging.axis || (target.axis ?? null) !== dragging.axis) return;
      writeRef(dragging.s, pointedCtx(), point.anchor, dragging.axis ? headCell(target) : target, dragging.axis);
    }

    function release() {
      dragging = null;
    }

    // The reference the caret is in as the cells it was pointed from and to, when it is on the sheet on show.
    function seedOf(ctx) {
      const rg = ctx.span ? targetOf(ctx.span) : null;
      return rg && !rg.axis ? { anchor: { r: rg.r1, c: rg.c1 }, focus: { r: rg.r2, c: rg.c2 } } : null;
    }

    // An arrow key on the entry of a formula, or on a reference just pointed, moves or widens the reference.
    function arrow(ev, s) {
      if (!point && s.mode() !== "enter") return false;
      const ctx = point ? pointedCtx() : contextOf(s);
      if (ctx.mode === "none") return false;
      const seed = point ? null : seedOf(ctx);
      if (!point && ctx.mode === "replace" && !seed) return false;
      ev.preventDefault();
      if (point?.axis) return true;
      const base = point ? point.focus : (seed?.focus ?? deps.getActiveCell());
      const [dr, dc] = ARROW_STEP[ev.key];
      const next = deps.stepFrom(base, dr, dc, ev.ctrlKey || ev.metaKey);
      const focus = { r: next.r, c: next.c };
      const anchor = ev.shiftKey ? (point?.anchor ?? seed?.anchor ?? base) : focus;
      grid.ensureVisible(focus.r, focus.c);
      writeRef(s, ctx, anchor, focus, null);
      return true;
    }

    // F4: the next `$` form of the reference at the caret.
    function cycle(ev, s) {
      const { text, from } = s.read();
      if (!text.startsWith("=")) return false;
      ev.preventDefault();
      const out = P.cycleRef(text, from);
      if (!out) return true;
      point = point && point.start === out.start ? { ...point, end: out.end } : null;
      writing = true;
      s.write(out.text, out.caret);
      writing = false;
      refresh();
      return true;
    }

    // F2: the entry of a text and its edit take turns (a pointed reference goes back to the edit).
    function turn(ev, s) {
      ev.preventDefault();
      s.setMode(point || s.mode() === "enter" ? "edit" : "enter");
      point = null;
      refresh();
      return true;
    }

    // A key on a place a formula is edited; true when it was taken. The rest of the keys end a pointed reference.
    function key(ev) {
      const s = surface();
      if (!s) return false;
      if (ARROW_STEP[ev.key] && !ev.altKey && arrow(ev, s)) return true;
      if (ev.key === "F2") return turn(ev, s);
      if ((ev.key === "F4" || (ev.metaKey && !ev.ctrlKey && !ev.shiftKey && ev.key.toLowerCase() === "t")) && cycle(ev, s)) return true;
      if (point && !KEEP_POINT_KEYS.has(ev.key)) {
        point = null;
        refresh();
      }
      return false;
    }

    // ---- the entry that goes on another sheet ------------------------------------------------------------------------
    // A switch to another sheet while a formula waits for a reference leaves the cell and goes on in the formula bar: the text
    // is not entered, the cell shows what it had.
    function detach(target) {
      const s = surface();
      if (!target || !s || target === deps.getSheetName()) return false;
      if (session) return true;
      if (!point && contextOf(s).mode === "none") return false;
      const { text, from } = s.read();
      const active = deps.getActiveCell();
      session = { sheet: deps.getSheetName(), ref: deps.refOf(active.r, active.c), mode: point ? "edit" : s.mode(), fromBar: s.kind === "bar" };
      if (s.kind === "cell") {
        detaching = true;
        deps.cancelEdit(s.el);
        s.el.blur?.();
        detaching = false;
        const bar = barEl();
        bar.value = text;
        bar.focus();
        bar.setSelectionRange?.(from, from);
      }
      barMode = session.mode;
      refresh();
      return true;
    }

    // Back on the sheet the entry started on, it goes on where it started.
    function afterSwitch(name) {
      if (!session || name.toLowerCase() !== session.sheet.toLowerCase()) {
        refresh();
        return;
      }
      const back = session;
      session = null;
      const bar = barEl();
      if (back.fromBar) {
        bar.focus();
        barMode = back.mode;
      } else {
        const text = bar.value;
        const caret = bar.selectionStart ?? text.length;
        const at = grid.parseRef(back.ref);
        const td = at ? deps.resumeCell(at.row, at.col, back.mode) : null;
        if (td) cellSurface(td).write(text, caret);
        else feedback("셀 입력을 이어 가지 못함");
      }
      refresh();
    }

    // Enter or Esc in the bar while the entry is out: back to its sheet, where the cell takes it or drops it.
    async function finish(commit) {
      const origin = session.sheet;
      try {
        await deps.switchSheet(origin);
      } catch {
        feedback("원래 시트로 돌아가지 못함");
        return;
      }
      if (!deps.finishCell(commit)) {
        if (commit) byId("btn-eval-formula")?.click();
        else deps.restoreBar();
      }
    }

    // A press on a cell or a header where no reference is awaited, with the entry out on another sheet: the formula is entered
    // on its own sheet like Enter does, then the sheet the press was made on comes back and the press selects what it hit.
    async function leave(target) {
      if (leaving) return;
      leaving = true;
      try {
        const shownSheet = deps.getSheetName();
        await finish(true);
        if (session) return;
        try {
          await deps.switchSheet(shownSheet);
        } catch {
          feedback("누른 시트로 돌아가지 못함");
          return;
        }
        deps.selectTarget(target);
      } finally {
        leaving = false;
      }
    }

    // A key in the formula bar (called by the bar's key handler); true when it was taken.
    function barKey(ev) {
      if (session && (ev.key === "Enter" || ev.key === "Escape")) {
        ev.preventDefault();
        void finish(ev.key === "Enter");
        return true;
      }
      if (ev.key === "Escape") {
        ev.preventDefault();
        endBarEdit();
        deps.restoreBar();
        return true;
      }
      if (ev.key === "Enter") {
        endBarEdit();
        return false;
      }
      return key(ev);
    }

    // ---- events of the places ------------------------------------------------------------------------------------------
    function endBarEdit() {
      barDirty = false;
      point = null;
      known.clear();
      refresh();
    }

    function afterInput() {
      if (!writing) point = null;
      refresh();
    }

    function editEnded() {
      if (detaching) return;
      point = null;
      known.clear();
      refresh();
    }

    function clearPoint() {
      if (!point) return;
      point = null;
      refresh();
    }

    // The tab of a sheet is pressed while a reference is awaited: the button must not take the focus (and with it the entry).
    function holdsTabPress() {
      const s = surface();
      return Boolean(s && (point || session || contextOf(s).mode !== "none"));
    }

    function bind() {
      const bar = barEl();
      if (bar) {
        bar.addEventListener("focus", () => {
          if (session) return;
          barMode = "edit";
          point = null;
          refresh();
        });
        bar.addEventListener("blur", () => {
          if (!session) endBarEdit();
        });
        bar.addEventListener("input", () => {
          barDirty = true;
          afterInput();
        });
        bar.addEventListener("mousedown", clearPoint);
        ["keyup", "mouseup", "scroll"].forEach((type) => bar.addEventListener(type, () => paintBarText(Boolean(surface()))));
      }
      byId("sheet-tab-bar")?.addEventListener("mousedown", (ev) => {
        if (ev.target?.closest?.(".sheet-tab-btn") && holdsTabPress()) ev.preventDefault();
      });
    }

    return {
      bind,
      refresh,
      press,
      over,
      release,
      key,
      barKey,
      detach,
      afterSwitch,
      afterInput,
      editedEnded: editEnded,
      clearPoint,
      holdsBar: () => Boolean(session),
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_POINT_UI__ = createSheetPointUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
