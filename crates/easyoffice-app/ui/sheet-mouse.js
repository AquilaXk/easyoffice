(function (host) {
  "use strict";

  // The mouse of the sheet grid: a press selects a cell or a whole row or column, a drag extends the range (or the point of a
  // formula, or a fill), the edge of a column header resizes it and a double click edits the cell. `sheet-input.js` owns the selection
  // and the edit modes; this module only reads the events and calls them.

  // `sel` is the selection of the input module (`cr`, `cc` the corner a drag moves). `select`, `selectAxis`, `commitIfEditing`
  // and `startEdit` are the input module's. Returns the handlers the input module binds to the viewport and the document.
  function createMouse({ document, api, grid, sel, protectApi, point, fill, viewport, gridTdOf, stateOf, select, selectAxis, commitIfEditing, startEdit, refreshChrome }) {
    let drag = null;
    let resize = null;

    function onMouseDown(ev) {
      if (ev.button || ev.target?.closest?.(".row-resizer")) return;
      const handle = ev.target?.closest?.(".col-resizer");
      if (handle) {
        ev.preventDefault();
        if (protectApi && !protectApi.guardAction(api.getSheetName(), "formatColumns")) return;
        const col = grid.colIndex(handle.dataset.colResizer || "A");
        resize = { col, startX: ev.clientX, startW: grid.colWidth(col) };
        return;
      }
      const head = ev.target?.closest?.("#pane-sheet th[data-col], #pane-sheet th[data-row]");
      if (head) {
        const axis = head.dataset.col ? "col" : "row";
        if (point.press(ev, { axis, index: axis === "col" ? grid.colIndex(head.dataset.col) : Number(head.dataset.row) })) return;
        ev.preventDefault();
        if (head.dataset.col) selectAxis("col", grid.colIndex(head.dataset.col), Boolean(ev.shiftKey));
        else selectAxis("row", Number(head.dataset.row), Boolean(ev.shiftKey));
        return;
      }
      const td = gridTdOf(ev.target);
      if (!td) return;
      const st = stateOf(td);
      if (st?.mode && document.activeElement === td) {
        point.clearPoint();
        return;
      }
      const p = grid.parseRef(td.dataset.cell);
      if (!p) return;
      if (point.press(ev, { r: p.row, c: p.col })) {
        drag = { active: true, point: true };
        return;
      }
      ev.preventDefault();
      commitIfEditing();
      drag = { active: true };
      select(p.row, p.col, Boolean(ev.shiftKey));
    }

    function autoScroll(ev) {
      const vp = viewport();
      if (!vp || typeof vp.getBoundingClientRect !== "function") return;
      const viewportHeight = vp.clientHeight;
      if (typeof viewportHeight !== "number" || Number.isNaN(viewportHeight) || viewportHeight <= 0) return;
      const rect = vp.getBoundingClientRect();
      if (ev.clientY > rect.bottom - 10) vp.scrollTop += 48;
      else if (ev.clientY < rect.top + 36) vp.scrollTop -= 48;
      if (ev.clientX > rect.right - 10) vp.scrollLeft += 96;
      else if (ev.clientX < rect.left + 66) vp.scrollLeft -= 96;
    }

    function onMouseMove(ev) {
      if (resize) {
        grid.applyColWidth(resize.col, resize.startW + (ev.clientX - resize.startX));
        return;
      }
      if (drag || fill.isDragging()) autoScroll(ev);
    }

    function onMouseOver(ev) {
      if ((!drag && !fill.isDragging()) || resize) return;
      const td = gridTdOf(ev.target);
      const p = td ? grid.parseRef(td.dataset.cell) : null;
      if (fill.isDragging()) {
        if (p) fill.over(p.row, p.col);
        return;
      }
      if (drag?.point) {
        if (p) point.over({ r: p.row, c: p.col });
        return;
      }
      if (p && (p.row !== sel.cr || p.col !== sel.cc)) {
        sel.cr = p.row;
        sel.cc = p.col;
        refreshChrome();
      }
    }

    function onMouseUp() {
      const wasResize = Boolean(resize);
      drag = null;
      point.release();
      resize = null;
      if (wasResize) document.documentElement?.dispatchEvent?.({ type: "eo-doc-mutated" });
    }

    function onDblClick(ev) {
      const td = gridTdOf(ev.target);
      if (td) startEdit(td, "edit", false);
    }

    return { onMouseDown, onMouseMove, onMouseOver, onMouseUp, onDblClick };
  }

  host.__EASYOFFICE_CREATE_SHEET_MOUSE__ = createMouse;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
