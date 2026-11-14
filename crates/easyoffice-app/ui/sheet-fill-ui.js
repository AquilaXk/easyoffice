(function (host) {
  "use strict";

  // The fill handle of the sheet window (PD-206): a small square at the lower right corner of the
  // selection. Dragging it fills the cells the pointer passes over, up, down, left or right, from
  // the selected cells; a double click fills down as far as the data next to the selection goes.
  // The content of a fill comes from sheet-fill-core.js. Ctrl or Cmd while releasing swaps copy and
  // series, Alt (Option) copies.
  const HANDLE_CLASS = "sheet-fill-handle";
  const PREVIEW_CLASS = "sheet-fill-preview";
  // Side of the handle box in px: the 8px square of the reference editor's object handles
  // (`table-object-renderer.ts:80`), its 1px paper edge inside the box.
  const HANDLE_SIZE = 8;

  function createSheetFillUi(deps) {
    const { document, grid, getRange, getAxis, getEntry, getSheetName, applyCellEdits, setRange, commitIfEditing, shiftFormula, feedback } = deps;
    const core = host.__EASYOFFICE_SHEET_FILL_CORE__;
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    let handle = null;
    let preview = null;
    let drag = null;

    const viewport = () => document.getElementById("sheet-viewport");

    function ensureElements() {
      const vp = viewport();
      if (!vp) return false;
      if (!handle) {
        handle = document.createElement("div");
        handle.className = HANDLE_CLASS;
        handle.hidden = true;
        handle.setAttribute("aria-hidden", "true");
        handle.addEventListener("mousedown", onHandleDown);
        handle.addEventListener("dblclick", onHandleDoubleClick);
        preview = document.createElement("div");
        preview.className = PREVIEW_CLASS;
        preview.hidden = true;
        vp.addEventListener("scroll", sync);
        // A release inside the viewport and one outside it (the document) end the same drag once.
        vp.addEventListener("mouseup", onUp);
      }
      if (handle.parentNode !== vp) {
        vp.appendChild(preview);
        vp.appendChild(handle);
      }
      return true;
    }

    // Pixel box of a range inside the scrolled content of the viewport.
    function boxOf(rg) {
      const left = grid.ROW_HEAD_W + grid.colLeft(rg.c1);
      const right = grid.ROW_HEAD_W + grid.colLeft(rg.c2 + 1);
      const top = grid.HEAD_H + grid.rowTop(rg.r1);
      const bottom = grid.HEAD_H + grid.rowTop(rg.r2 + 1);
      return { left, top, width: right - left, height: bottom - top, right, bottom };
    }

    function isEditing() {
      return Boolean(document.querySelector("#pane-sheet td.editing"));
    }

    // Shows the handle at the corner of the selection unless the corner is behind a header or the
    // selection is a whole row or column.
    function sync() {
      if (!ensureElements()) return;
      const rg = getRange();
      const vp = viewport();
      const box = boxOf(rg);
      const half = HANDLE_SIZE / 2;
      const shown = !getAxis() && !isEditing() && !drag && box.right > grid.ROW_HEAD_W && box.bottom > grid.HEAD_H;
      const behindHeader =
        vp.clientWidth > 0 && (box.right - vp.scrollLeft < grid.ROW_HEAD_W + half || box.bottom - vp.scrollTop < grid.HEAD_H + half);
      handle.hidden = !shown || behindHeader;
      handle.style.left = box.right - half + "px";
      handle.style.top = box.bottom - half + "px";
    }

    function showPreview(rg) {
      if (!ensureElements()) return;
      if (!rg) {
        preview.hidden = true;
        return;
      }
      const box = boxOf(rg);
      preview.style.left = box.left + "px";
      preview.style.top = box.top + "px";
      preview.style.width = box.width + "px";
      preview.style.height = box.height + "px";
      preview.hidden = false;
    }

    // The part of the fill past the source, from the cell under the pointer.
    function targetOf(src, row, col) {
      const dr = row > src.r2 ? row - src.r2 : row < src.r1 ? row - src.r1 : 0;
      const dc = col > src.c2 ? col - src.c2 : col < src.c1 ? col - src.c1 : 0;
      if (dr === 0 && dc === 0) return null;
      if (Math.abs(dr) >= Math.abs(dc)) {
        return dr > 0
          ? { direction: "down", count: dr, range: { ...src, r1: src.r2 + 1, r2: row } }
          : { direction: "up", count: -dr, range: { ...src, r1: row, r2: src.r1 - 1 } };
      }
      return dc > 0
        ? { direction: "right", count: dc, range: { ...src, c1: src.c2 + 1, c2: col } }
        : { direction: "left", count: -dc, range: { ...src, c1: col, c2: src.c1 - 1 } };
    }

    function unionOf(a, b) {
      return { r1: Math.min(a.r1, b.r1), c1: Math.min(a.c1, b.c1), r2: Math.max(a.r2, b.r2), c2: Math.max(a.c2, b.c2) };
    }

    function stop() {
      drag = null;
      showPreview(null);
      document.removeEventListener("mouseup", onUp);
      document.removeEventListener("keydown", onKey, true);
      sync();
    }

    function onHandleDown(ev) {
      if (ev.button) return;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      commitIfEditing();
      drag = { src: getRange(), plan: null };
      handle.hidden = true;
      document.addEventListener("mouseup", onUp);
      document.addEventListener("keydown", onKey, true);
    }

    function onKey(ev) {
      if (ev.key !== "Escape") return;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      stop();
    }

    // The pointer is over a cell: the fill that would follow, drawn as an outline.
    function over(row, col) {
      if (!drag) return;
      drag.plan = targetOf(drag.src, row, col);
      showPreview(drag.plan ? unionOf(drag.src, drag.plan.range) : null);
    }

    function modeOf(ev) {
      if (ev.altKey) return "copy";
      return ev.ctrlKey || ev.metaKey ? "toggle" : "auto";
    }

    function mergeInTheWay(rg) {
      return layout?.mergesIn(getSheetName(), rg.r1, rg.c1, rg.r2, rg.c2).length > 0;
    }

    // Writes the fill and selects the filled cells together with the source: one undo step.
    function run(src, plan, mode) {
      const whole = unionOf(src, plan.range);
      if (mergeInTheWay(whole)) {
        feedback("병합 셀과 겹침");
        return Promise.resolve(null);
      }
      const result = core.planFill({
        source: src,
        direction: plan.direction,
        count: plan.count,
        mode,
        cellAt: (r, c) => getEntry(grid.refOf(r, c)),
        shiftFormula,
        refOf: grid.refOf,
        tableFor: (name, row, col) => (name ? layout?.tableNamed(name) : layout?.tableAt(getSheetName(), row, col)),
      });
      if (result.refused) {
        feedback("채우기 범위 초과");
        return Promise.resolve(null);
      }
      setRange(whole);
      return applyCellEdits(result.edits, "채우기");
    }

    function onUp(ev) {
      const state = drag;
      if (!state) return;
      stop();
      if (state.plan) void run(state.src, state.plan, modeOf(ev)).catch(() => {});
    }

    // Double click: down as far as the data beside the selection goes.
    function onHandleDoubleClick(ev) {
      ev.preventDefault?.();
      const src = getRange();
      const found = core.detectFillEnd(src, (r, c) => hasContent(getEntry(grid.refOf(r, c))), grid.MAX_ROWS, grid.MAX_COLS);
      if (found?.refused) {
        feedback("채우기 범위 초과");
        return;
      }
      if (!found) return;
      const range = { ...src, r1: src.r2 + 1, r2: found.end };
      void run(src, { direction: "down", count: found.end - src.r2, range }, modeOf(ev)).catch(() => {});
    }

    const hasContent = (entry) => Boolean(entry && (entry.value || entry.formula));

    return { sync, over, isDragging: () => drag !== null };
  }

  host.__EASYOFFICE_CREATE_SHEET_FILL_UI__ = createSheetFillUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
