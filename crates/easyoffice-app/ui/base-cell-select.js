// 참조 구현 (reference implementation): 키 규칙과 글 처리는 `base-cell-nav.js`(glide-data-grid, MIT, Copyright (c) 2021 typeguard, Inc.),
// 스크롤 계산은 `base-virtual.js`(TanStack Virtual `virtual-core`, MIT, Copyright (c) 2021-present Tanner Linsley; see `THIRD_PARTY_NOTICES.md`).
// 이 파일은 EasyOffice 자체 코드다: 칸 선택, 키보드 이동, 편집 시작, 복사·붙여넣기, 지우기를 표 DOM에 잇는다 (PD-294).
// 선택은 표 요소 하나가 받는다(`role="grid"`, `aria-activedescendant`): 행이 창 밖으로 나가도 초점이 남고, 칸 안의 컨트롤은 탭 순서에서 빠진다.

(function (host) {
  "use strict";

  const CELL = "td.base-grid-cell";
  // The controls in a row that the keys of the grid replace: only the table is a tab stop.
  const ROW_CONTROLS = "td.base-grid-cell input, td.base-grid-cell select, td.base-grid-cell button, td.base-grid-cell a, button.base-row-delete-btn";
  const ACTIVE_ID = "base-active-cell";
  // What takes a click as it is: a link, a button (a pill, the delete button), a select and a checkbox.
  const NATIVE_CONTROL = "a, button, select, input.base-cell-checkbox";
  // data-sel of a cell in the range: s, then the sides of the range it lies on (t, b, l, r) and a for the active cell. One lookup, no string work per cell.
  const IN_RANGE = 1, EDGE_T = 2, EDGE_B = 4, EDGE_L = 8, EDGE_R = 16, ACTIVE = 32;
  const SEL_TOKENS = Array.from({ length: 64 }, (_, f) => ["s", f & EDGE_T && "t", f & EDGE_B && "b", f & EDGE_L && "l", f & EDGE_R && "r", f & ACTIVE && "a"].filter(Boolean).join(" "));
  // The text kinds of a cell are an input that is read-only until the cell is edited; a select and the pills of a multi select take the focus as they are.
  const isTextInput = (el) => el.classList?.contains("base-cell-input");

  // deps: updateCells(edits) -> result of the model, setFeedback(text), relationIds(field, text) -> the record ids a text of labels stands for.
  function createCellSelect(doc, deps) {
    const N = host.__EASYOFFICE_BASE_CELL_NAV__, nav = host.__EASYOFFICE_SHEET_NAV__;
    // sel: { ar, ac: the active cell, cr, cc: the far corner, span, tabFrom } with 1-based item and column numbers (what `sheet-nav.js` reads).
    let sel = null, viewKey = "", ctx = null, editing = null, refocus = false, drag = false;

    const cellAt = (r, c) => ctx.win.rowAt(r - 1)?.childNodes[c];
    const coordsOf = (td) => {
      const item = td.parentElement ? ctx.win.itemOf(td.parentElement) : undefined, c = ctx.colOf.get(td.dataset.fieldId);
      return item === undefined || c === undefined ? null : { r: item + 1, c };
    };
    const cellOfTarget = (ev) => ev.target?.closest?.(CELL) || null;

    // The table as `sheet-nav.js` reads it: group rows are hidden rows, a cell has a value when its text is not empty.
    function viewOf({ win, fields }) {
      let last = win.count;
      while (last > 0 && win.isGroup(last - 1)) last -= 1;
      return {
        maxRows: win.count, maxCols: fields.length, dataRows: last, dataCols: fields.length,
        hasValue: (r, c) => (win.recordAt(r - 1)?.displays[fields[c - 1].id] ?? "") !== "",
        mergeAt: () => null, rowHidden: (r) => win.isGroup(r - 1), colHidden: () => false,
      };
    }

    const inTable = (r, c) => r >= 1 && c >= 1 && r <= ctx.win.count && c <= ctx.fields.length;
    const selectionFits = () => [[sel.ar, sel.ac], [sel.cr, sel.cc], ...(sel.span ? [[sel.span.r2, sel.span.c2]] : [])].every(([r, c]) => inTable(r, c)) && !ctx.win.isGroup(sel.ar - 1);

    // ---- painting ----

    function flagsAt(range, r, c, active) {
      if (r < range.r1 || r > range.r2 || c < range.c1 || c > range.c2) return 0;
      return IN_RANGE | (r === range.r1 ? EDGE_T : 0) | (r === range.r2 ? EDGE_B : 0) | (c === range.c1 ? EDGE_L : 0) | (c === range.c2 ? EDGE_R : 0) | (active ? ACTIVE : 0);
    }

    // The link of the active cell and the delete button of its row are the controls that stay in the tab order (Tab leaves the grid for them).
    function reachable(td, on) {
      td.querySelector("a.base-url-link")?.setAttribute("tabindex", on ? "0" : "-1");
      td.parentElement.querySelector("button.base-row-delete-btn")?.setAttribute("tabindex", on ? "0" : "-1");
    }

    function paintCell(td, flags) {
      if (!td) return;
      td.setAttribute("aria-selected", flags ? "true" : "false");
      if (flags) td.setAttribute("data-sel", SEL_TOKENS[flags]); else td.removeAttribute("data-sel");
      if (flags & ACTIVE) {
        td.setAttribute("id", ACTIVE_ID);
        ctx.tbl.setAttribute("aria-activedescendant", ACTIVE_ID);
        reachable(td, true);
      } else if (td.getAttribute("id") === ACTIVE_ID) {
        td.removeAttribute("id");
        reachable(td, false);
      }
    }

    // The cells of `range` that are in the DOM: the rows the window holds, nothing else is visited.
    function eachBuiltCell(range, fn) {
      const built = ctx.win.builtItems();
      if (!built) return;
      for (let i = Math.max(range.r1 - 1, built.from); i <= Math.min(range.r2 - 1, built.to); i += 1) {
        const tr = ctx.win.rowAt(i);
        if (tr && !ctx.win.isGroup(i)) for (let c = range.c1; c <= range.c2; c += 1) fn(tr.childNodes[c], i + 1, c);
      }
    }

    function repaint(old, next) {
      if (old) eachBuiltCell(N.rangeOf(old), (td) => paintCell(td, 0));
      if (!next) { ctx.tbl.removeAttribute("aria-activedescendant"); return; }
      const range = N.rangeOf(next);
      eachBuiltCell(range, (td, r, c) => paintCell(td, flagsAt(range, r, c, r === next.ar && c === next.ac)));
      if (!cellAt(next.ar, next.ac)) ctx.tbl.removeAttribute("aria-activedescendant");
    }

    // Called for every row the window builds: the controls leave the tab order, a text input is read-only until it is edited, and the row is painted.
    function rowBuilt(tr, item) {
      for (const el of tr.querySelectorAll(ROW_CONTROLS)) {
        el.setAttribute("tabindex", "-1");
        if (isTextInput(el)) el.readOnly = true;
      }
      if (!ctx || ctx.win.isGroup(item)) return;
      const range = sel ? N.rangeOf(sel) : null;
      for (let c = 1; c <= ctx.fields.length; c += 1) paintCell(tr.childNodes[c], range ? flagsAt(range, item + 1, c, item + 1 === sel.ar && c === sel.ac) : 0);
    }

    // ---- moving the selection ----

    function revealColumn(td) {
      const wrap = ctx.wrap;
      if (!td || !(wrap.clientWidth > 0)) return;
      const view = wrap.getBoundingClientRect(), box = td.getBoundingClientRect(), head = td.parentElement.firstChild.getBoundingClientRect().width;
      if (box.left < view.left + head) wrap.scrollLeft -= view.left + head - box.left;
      else if (box.right > view.left + wrap.clientWidth) wrap.scrollLeft += box.right - (view.left + wrap.clientWidth);
    }

    // The far corner is the cell that moved last (the active cell when the range is one cell).
    function revealCorner() {
      ctx.win.scrollToItem(sel.cr - 1);
      revealColumn(cellAt(sel.cr, sel.cc));
    }

    // A cell that is being edited is left first, which commits it; the selection is the new one when the table is drawn again.
    function setSel(next, reveal) {
      const old = sel;
      sel = next;
      if (editing) leaveEdit(false);
      if (reveal) revealCorner();
      repaint(old, sel);
    }

    function selectFirst() {
      const first = nav.home(ctx.view, { r: 1, c: 1 }, true);
      if (inTable(first.r, first.c) && !ctx.view.rowHidden(first.r)) setSel(N.single(first.r, first.c), false);
    }

    // ---- editing ----

    function onEditBlur() {
      if (!editing) return;
      if (editing.text) editing.ctl.readOnly = true;
      editing = null;
    }

    function beginEdit(td, ctl) {
      editing = { td, ctl, text: isTextInput(ctl), orig: ctl.value };
      ctl.addEventListener("blur", onEditBlur, { once: true });
    }

    // Leaves the control (`restore`: with the text it had). The change event of the browser commits what was typed, and the table is drawn again.
    function leaveEdit(restore) {
      const edit = editing;
      if (!edit) return;
      if (restore && edit.text) edit.ctl.value = edit.orig;
      editing = null;
      if (edit.text) edit.ctl.readOnly = true;
      refocus = true;
      edit.ctl.blur();
      if (refocus) { refocus = false; ctx?.tbl.focus({ preventScroll: true }); }
    }

    // Starts the edit of the active cell. `seed` is the text a typed key puts in (an empty one clears the cell), none keeps the text.
    // Returns whether the seed went in: the key that gave it is then taken (a date input and a select take the key as it is).
    function startEdit(seed) {
      const field = ctx.fields[sel.ac - 1];
      let td = cellAt(sel.ar, sel.ac);
      if (!td) { ctx.win.scrollToItem(sel.ar - 1); td = cellAt(sel.ar, sel.ac); }
      if (!td || field.type === "formula") return false;
      if (field.type === "checkbox") { if (seed === undefined) td.querySelector("input.base-cell-checkbox")?.click(); return false; }
      if (field.type === "multiSelect") { const pill = td.querySelector("button.base-multi-opt"); if (pill && seed === undefined) { beginEdit(td, pill); pill.focus({ preventScroll: true }); } return false; }
      if (field.type === "select" || field.type === "relation") {
        const select = td.querySelector("select");
        if (!select) return false;
        beginEdit(td, select);
        select.focus({ preventScroll: true });
        if (seed === undefined) { try { select.showPicker?.(); } catch { /* a picker needs a user gesture */ } }
        return false;
      }
      const input = td.querySelector("input.base-cell-input");
      if (!input) return false;
      beginEdit(td, input);
      input.readOnly = false;
      if (seed !== undefined) input.value = field.type === "date" ? "" : seed;
      input.focus({ preventScroll: true });
      if (seed === undefined) { try { input.setSelectionRange(input.value.length, input.value.length); } catch { /* a date input has no caret */ } }
      return seed !== undefined && field.type !== "date";
    }

    // ---- clipboard and Delete ----

    function onCopy(ev) {
      if (editing || !sel || !ev.clipboardData) return;
      const range = N.rangeOf(sel), rows = [];
      for (let r = range.r1; r <= range.r2; r += 1) {
        const rec = ctx.win.recordAt(r - 1);
        if (rec) rows.push(ctx.fields.slice(range.c1 - 1, range.c2).map((f) => rec.displays[f.id]));
      }
      ev.clipboardData.setData("text/plain", N.toTsv(rows));
      ev.preventDefault();
    }

    const editOf = (r, c, value) => {
      const rec = ctx.win.recordAt(r - 1), field = ctx.fields[c - 1];
      return rec && field.type !== "formula" ? { recordId: rec.record.id, fieldId: field.id, value: field.type === "relation" && typeof value === "string" ? deps.relationIds(field, value) : value } : null;
    };

    function clearRange() {
      const range = N.rangeOf(sel), edits = [];
      for (let r = range.r1; r <= range.r2; r += 1) {
        for (let c = range.c1; c <= range.c2; c += 1) {
          const edit = editOf(r, c, N.emptyValueOf(ctx.fields[c - 1]));
          if (edit) edits.push(edit);
        }
      }
      if (edits.length > 0) deps.updateCells(edits);
    }

    // One text in a range fills the range; a block goes to the cells from the top left of the range on, over the records (group rows are passed)
    // and the fields that are there; what falls outside the table is left out.
    function onPaste(ev) {
      if (editing || !sel || !ev.clipboardData) return;
      const text = ev.clipboardData.getData("text/plain");
      if (!text) return;
      ev.preventDefault();
      const rows = N.parseTsv(text);
      if (!rows) { deps.setFeedback("붙여 넣을 수 없습니다: 글이 너무 길거나 칸이 너무 많습니다."); return; }
      const range = N.rangeOf(sel), edits = [], count = ctx.win.count, cols = ctx.fields.length;
      let endR = range.r1, endC = range.c1;
      if (rows.length === 1 && rows[0].length === 1 && !N.isSingle(range)) {
        for (let r = range.r1; r <= range.r2; r += 1) for (let c = range.c1; c <= range.c2; c += 1) edits.push(editOf(r, c, rows[0][0]));
      } else {
        let r = range.r1;
        for (const line of rows) {
          while (r <= count && ctx.win.isGroup(r - 1)) r += 1;
          if (r > count) break;
          for (let j = 0; j < line.length && range.c1 + j <= cols; j += 1) edits.push(editOf(r, range.c1 + j, line[j]));
          endR = r; endC = Math.max(endC, Math.min(range.c1 + line.length - 1, cols));
          r += 1;
        }
        const next = { ...sel, ar: range.r1, ac: range.c1, cr: endR, cc: endC, span: null };
        if (endR !== range.r1 || endC !== range.c1) setSel(next, false);
      }
      const live = edits.filter(Boolean);
      if (live.length > 0) deps.updateCells(live);
    }

    // ---- keys ----

    function run(intent, ev) {
      if (intent.t === "walk") {
        const next = N.walkAfter(ctx.view, sel, intent.dr, intent.dc);
        // Tab at the edge of the table moves nothing: it is left to the browser, so the focus goes on (the grid does not trap it).
        if (!editing && intent.dr === 0 && next.ar === sel.ar && next.ac === sel.ac) return;
        ev.preventDefault();
        setSel(next, true);
      } else if (intent.t === "cancel") {
        ev.preventDefault();
        leaveEdit(true);
      } else if (intent.t === "edit" || intent.t === "type") {
        if (startEdit(intent.t === "type" ? intent.text : undefined) || intent.t === "edit") ev.preventDefault();
      } else if (intent.t === "clear") {
        ev.preventDefault();
        clearRange();
      } else if (intent.t === "escape") {
        if (!N.isSingle(N.rangeOf(sel))) setSel(N.single(sel.ar, sel.ac), false);
      } else if (intent.t === "all") {
        ev.preventDefault();
        setSel({ ...N.single(nav.home(ctx.view, { r: 1, c: 1 }, true).r, 1), cr: ctx.view.dataRows, cc: ctx.fields.length }, false);
      } else {
        ev.preventDefault();
        setSel(N.selectionAfter(ctx.view, sel, intent, ctx.win.visibleSpan()), true);
      }
    }

    function onKeyDown(ev) {
      if (!ctx || !sel) return;
      // A key of an input method (the composition of Hangul): the first one starts an edit, the control takes the rest.
      if (ev.isComposing || ev.key === "Process" || ev.keyCode === 229) {
        if (!editing && ev.key === "Process") startEdit("");
        return;
      }
      const intent = N.intentOf(ev, Boolean(editing));
      // Enter belongs to the control when it is a pill or a select (it toggles, it picks); Tab and Esc still leave.
      if (!intent || (editing && !editing.text && ev.key === "Enter")) return;
      if (!editing && doc.activeElement !== ctx.tbl) ctx.tbl.focus({ preventScroll: true });
      run(intent, ev);
    }

    // ---- pointer and focus ----

    // A click on a link, a button or a checkbox goes to it; a select opens on the click that finds its cell selected already.
    const nativeTarget = (ev, at) => {
      const hit = ev.target?.closest?.(NATIVE_CONTROL);
      if (!hit) return false;
      return hit.tagName !== "SELECT" || (sel?.ar === at.r && sel?.ac === at.c && N.isSingle(N.rangeOf(sel)));
    };

    function onMouseDown(ev) {
      const td = cellOfTarget(ev), at = td && ctx && coordsOf(td);
      if (!at || ev.button !== 0 || (editing && editing.td === td)) return;
      const extend = ev.shiftKey && sel, native = nativeTarget(ev, at);
      setSel(extend ? { ...sel, span: null, cr: at.r, cc: at.c } : N.single(at.r, at.c), false);
      if (native) return;
      ev.preventDefault();
      ctx.tbl.focus({ preventScroll: true });
      drag = true;
      doc.addEventListener("mousemove", onDragMove);
      doc.addEventListener("mouseup", endDrag);
    }

    // The range follows the pointer; above or below the list the corner steps one row per move and the list scrolls with it.
    function onDragMove(ev) {
      if (!ctx || !sel) return;
      const td = cellOfTarget(ev), at = td && coordsOf(td);
      if (at) {
        if (at.r !== sel.cr || at.c !== sel.cc) setSel({ ...sel, span: null, cr: at.r, cc: at.c }, false);
        return;
      }
      const box = ctx.wrap.getBoundingClientRect(), dr = ev.clientY < box.top ? -1 : ev.clientY > box.bottom ? 1 : 0;
      if (dr !== 0) setSel(N.selectionAfter(ctx.view, sel, { t: "move", dr, dc: 0, extend: true }, 1), true);
    }

    function endDrag() {
      drag = false;
      doc.removeEventListener("mousemove", onDragMove);
      doc.removeEventListener("mouseup", endDrag);
    }

    function onDblClick(ev) {
      const td = cellOfTarget(ev);
      if (!td || !sel || editing || ev.target?.closest?.(NATIVE_CONTROL)) return;
      startEdit();
    }

    // A control that takes the focus without the keys (a screen reader, a script): its cell is selected, and a text input or a select is edited.
    function onFocusIn(ev) {
      if (!ctx || editing) return;
      if (ev.target === ctx.tbl) { if (!sel) selectFirst(); return; }
      const td = cellOfTarget(ev), at = td && coordsOf(td);
      if (!at) return;
      if (!sel || sel.ar !== at.r || sel.ac !== at.c) setSel(N.single(at.r, at.c), false);
      if (isTextInput(ev.target)) { ev.target.readOnly = false; beginEdit(td, ev.target); } else if (ev.target.tagName === "SELECT") beginEdit(td, ev.target);
    }

    // ---- life of one drawn table ----

    // Called with the table, its scroll element, the rows of its window and the fields, before the window is attached. A new `key` (the table or
    // the filter, sort and group of it changed) forgets the selection; a table drawn again for an edit keeps it.
    function bind(grid, key) {
      endDrag();
      if (key !== viewKey) { viewKey = key; sel = null; }
      ctx = { ...grid, colOf: new Map(grid.fields.map((f, i) => [f.id, i + 1])), view: viewOf(grid) };
      editing = null;
      if (sel && !selectionFits()) sel = null;
      const { tbl } = ctx;
      tbl.setAttribute("role", "grid");
      tbl.setAttribute("aria-multiselectable", "true");
      tbl.setAttribute("tabindex", "0");
      tbl.addEventListener("keydown", onKeyDown);
      tbl.addEventListener("mousedown", onMouseDown);
      tbl.addEventListener("dblclick", onDblClick);
      tbl.addEventListener("focusin", onFocusIn);
      tbl.addEventListener("copy", onCopy);
      tbl.addEventListener("paste", onPaste);
    }

    // The focus of the table that is about to be replaced goes to the new one, so the keys go on working after an edit.
    function beforeRender() {
      refocus = refocus || Boolean(ctx && ctx.tbl.contains(doc.activeElement));
    }

    function afterAttach() {
      if (!refocus || !ctx) return;
      refocus = false;
      ctx.tbl.focus({ preventScroll: true });
    }

    function release() {
      endDrag();
      ctx = null;
      editing = null;
    }

    return { bind, rowBuilt, beforeRender, afterAttach, release, selection: () => sel };
  }

  host.__EASYOFFICE_CREATE_BASE_CELL_SELECT__ = createCellSelect;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
