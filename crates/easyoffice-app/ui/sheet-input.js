(function (host) {
  "use strict";

  const { normalizeFormattedNumericCell } = host.__EASYOFFICE_SHEET_UTILS__;

  // Ranges up to this many cells are read by address; larger ones pick the cheaper of address and entry walks.
  const DIRECT_AREA_CELLS = 4096;

  function isEditing(st) {
    return Boolean(st && (st.mode || st.userEdited));
  }

  function numericOf(raw) {
    const t = normalizeFormattedNumericCell(raw);
    if (t === "") return null;
    const n = Number(t);
    return Number.isFinite(n) ? n : null;
  }

  function formatAggregate(nums) {
    if (nums.length === 0) return "선택 집계 · 숫자 데이터 없음";
    const { count, sum, min, max, avg: rawAvg } = host.__EASYOFFICE_SHEET_UTILS__.aggregateNumbers(nums);
    const avg = Math.round(rawAvg * 10) / 10;
    const fmt = (n) => Number(n.toPrecision(12)).toLocaleString();
    return (
      "선택 집계 · 합계 " + fmt(sum) + " · 평균 " + avg.toLocaleString() + " · 개수 " + count +
      " · 최소 " + fmt(min) + " · 최대 " + fmt(max)
    );
  }

  function isPrintable(ev) {
    return ev.key?.length === 1 && !ev.ctrlKey && !ev.metaKey && !ev.altKey;
  }

  function markEditing(td, st, mode) {
    st.mode = mode;
    td.dataset.editMode = mode;
    td.classList.add("editing");
  }

  // Sheet editing interaction: selection model, ready/edit modes, keyboard, mouse, IME,
  // clipboard events and the selection aggregate.
  //
  // Modes of the focused cell:
  //   ready  - the cell is selected; printable keys start an "enter" edit that REPLACES the
  //            content, F2 / double-click start an "edit" edit that keeps the content.
  //   enter  - started by typing. Arrow keys commit and move (spreadsheet "Enter mode").
  //   edit   - started by F2 / double-click. Left/Right move the caret; Up/Down commit and move.
  // Enter commits and moves down (Shift+Enter up), Tab commits and moves right (Shift+Tab left),
  // Esc cancels and restores. Keys pressed while an IME composes are never interpreted.
  // Ctrl+Arrow: jump to the edge of the contiguous data block (or the next block).
  function jumpLimit(dr, dc, ext) {
    if (dr > 0) return ext.dataRows;
    return dc > 0 ? ext.dataCols : 0;
  }

  function isPastData(dr, dc, nr, nc, limit) {
    if (dr > 0) return nr > limit;
    return dc > 0 && nc > limit;
  }

  function createSheetInput(api) {
    const { document, grid, sheetCellFocusState, isFormulaStr } = api;
    // `axis`: "row" or "col" while a whole row or column header is selected, else null.
    const sel = { ar: 1, ac: 1, cr: 1, cc: 1, axis: null };
    const bound = new WeakSet();
    let delegatesBound = false;
    let drag = null;
    let resize = null;
    const clip = host.__EASYOFFICE_CREATE_SHEET_CLIPBOARD__({
      grid,
      isFormulaStr,
      getEntry: (ref) => api.getEntry(ref),
      applyCellEdits: (edits, label) => api.applyCellEdits(edits, label),
      clearExisting: (range) => api.clearExisting(range),
    });

    const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
    const viewport = () => document.getElementById("sheet-viewport");
    const stateOf = (td) => sheetCellFocusState.get(td);

    function tdAt(row, col) {
      if (typeof grid.getTdAt === "function") return grid.getTdAt(row, col);
      return document.querySelector('#pane-sheet td[data-cell="' + grid.refOf(row, col) + '"]');
    }

    function getRange() {
      return {
        r1: Math.min(sel.ar, sel.cr),
        r2: Math.max(sel.ar, sel.cr),
        c1: Math.min(sel.ac, sel.cc),
        c2: Math.max(sel.ac, sel.cc),
      };
    }

    function gridTdOf(el) {
      const td = el && typeof el.closest === "function" ? el.closest("td[data-cell]") : null;
      return td && document.getElementById("pane-sheet")?.contains(td) ? td : null;
    }

    // ---- aggregate -------------------------------------------------------------------------

    function collectNumbers(range, liveTd) {
      const nums = [];
      const liveRef = liveTd ? liveTd.dataset.cell : "";
      const push = (ref) => {
        const raw = ref === liveRef ? grid.readTdValue(liveTd) : api.getEntry(ref)?.value;
        const n = numericOf(raw);
        if (n !== null) nums.push(n);
      };
      const area = (range.r2 - range.r1 + 1) * (range.c2 - range.c1 + 1);
      // Counting the entries copies every key of the sheet: a small range is read cell by cell without it.
      const mapSize =
        area <= DIRECT_AREA_CELLS
          ? area
          : typeof api.getEntryCount === "function"
            ? api.getEntryCount()
            : (api.activeSheetMap?.size ?? 20000);
      if (area <= mapSize) {
        for (let r = range.r1; r <= range.r2; r += 1) {
          for (let c = range.c1; c <= range.c2; c += 1) push(grid.refOf(r, c));
        }
        return nums;
      }
      let visitedLive = false;
      api.forEachEntryRef((ref, row, col) => {
        if (row >= range.r1 && row <= range.r2 && col >= range.c1 && col <= range.c2) {
          if (ref === liveRef) visitedLive = true;
          push(ref);
        }
      });
      if (liveRef && !visitedLive) {
        const p = grid.parseRef(liveRef);
        if (p && p.row >= range.r1 && p.row <= range.r2 && p.col >= range.c1 && p.col <= range.c2) {
          push(liveRef);
        }
      }
      return nums;
    }

    // ---- selection painting ----------------------------------------------------------------
    function paintSelection() {
      const rg = getRange();
      if (typeof grid.paintSelectionRange === "function") {
        grid.paintSelectionRange({ r1: rg.r1, r2: rg.r2, c1: rg.c1, c2: rg.c2, ar: sel.ar, ac: sel.ac });
        return;
      }
      const multi = rg.r1 !== rg.r2 || rg.c1 !== rg.c2;
      const tbody = document.querySelector("#pane-sheet tbody");
      if (!tbody) return;
      tbody.querySelectorAll("td[data-cell]").forEach((td) => {
        let r = td._row;
        let c = td._col;
        if (!(r > 0 && c > 0)) {
          const p = grid.parseRef(td.dataset.cell);
          if (!p) return;
          r = p.row;
          c = p.col;
          td._row = r;
          td._col = c;
        }
        const active = r === sel.ar && c === sel.ac;
        const inside = multi && r >= rg.r1 && r <= rg.r2 && c >= rg.c1 && c <= rg.c2;
        td.classList.toggle("selected-cell", active);
        td.classList.toggle("in-range", inside);
      });
      document.querySelectorAll("#pane-sheet th[data-col]").forEach((th) => {
        const idx = grid.colIndex(th.dataset.col);
        th.classList.toggle("active-axis", idx >= rg.c1 && idx <= rg.c2);
      });
      document.querySelectorAll("#pane-sheet th[data-row]").forEach((th) => {
        const row = Number(th.dataset.row);
        th.classList.toggle("active-axis", row >= rg.r1 && row <= rg.r2);
      });
    }

    function refreshChrome() {
      const ref = grid.refOf(sel.ar, sel.ac);
      api.setActiveSheetCellRef(ref);
      paintSelection();
      const activeTd = tdAt(sel.ar, sel.ac);
      const badge = document.getElementById("active-cell-ref");
      if (badge) badge.textContent = ref;
      const formulaInput = document.getElementById("formula-input");
      if (formulaInput && document.activeElement !== formulaInput) {
        if (activeTd) {
          const live = grid.readTdValue(activeTd);
          formulaInput.value = stateOf(activeTd)?.userEdited ? live : activeTd.dataset.formula || live;
        } else {
          const entry = api.getEntry(ref);
          formulaInput.value = entry?.formula || entry?.value || "";
        }
      }
      const aggEl = document.getElementById("status-sheet-agg");
      if (aggEl) {
        const editingTd = activeTd && stateOf(activeTd)?.userEdited ? activeTd : null;
        aggEl.textContent = formatAggregate(collectNumbers(getRange(), editingTd));
      }
      const out = document.getElementById("sheet-eval-result");
      if (out) out.setAttribute("title", out.textContent || "");
      api.onSelectionChanged?.();
    }

    // Public selection refresh (focus, outline links, restore, recalculation).
    function updateSelection(cellRef) {
      const p = grid.parseRef(cellRef) || { row: 1, col: 1 };
      if (p.row !== sel.ar || p.col !== sel.ac) {
        sel.axis = null;
        sel.ar = sel.cr = p.row;
        sel.ac = sel.cc = p.col;
        grid.ensureVisible(p.row, p.col);
      }
      refreshChrome();
    }

    function focusAnchor() {
      const td = tdAt(sel.ar, sel.ac);
      if (td) {
        if (document.activeElement !== td) td.focus({ preventScroll: true });
        return;
      }
      const vp = viewport();
      if (vp && typeof vp.focus === "function") vp.focus({ preventScroll: true });
    }

    function commitIfEditing() {
      const ae = document.activeElement;
      const td = gridTdOf(ae);
      if (td && isEditing(stateOf(td))) api.commitSheetCellEdit(td, false).catch(() => {});
    }

    function select(row, col, extend) {
      let r = clamp(row, 1, grid.MAX_ROWS);
      let c = clamp(col, 1, grid.MAX_COLS);
      commitIfEditing();
      // A click or a move that lands inside a merged range selects its anchor cell.
      const merged = extend ? null : grid.mergeAt?.(r, c);
      if (merged) {
        r = merged.r1;
        c = merged.c1;
      }
      sel.axis = null;
      if (extend) {
        sel.cr = r;
        sel.cc = c;
      } else {
        sel.ar = sel.cr = r;
        sel.ac = sel.cc = c;
      }
      grid.ensureVisible(r, c);
      focusAnchor();
      refreshChrome();
    }

    // Selects whole rows or columns (a click on a header; `extend` grows the selection to `index`).
    function selectAxis(axis, index, extend) {
      commitIfEditing();
      const last = axis === "col" ? grid.MAX_COLS : grid.MAX_ROWS;
      const at = clamp(index, 1, last);
      if (axis === "col") {
        if (!(extend && sel.axis === "col")) sel.ac = at;
        sel.cc = at;
        sel.ar = 1;
        sel.cr = grid.MAX_ROWS;
      } else {
        if (!(extend && sel.axis === "row")) sel.ar = at;
        sel.cr = at;
        sel.ac = 1;
        sel.cc = grid.MAX_COLS;
      }
      sel.axis = axis;
      refreshChrome();
    }

    function selectAll() {
      sel.axis = null;
      const ext = api.getExtent();
      sel.ar = 1;
      sel.ac = 1;
      sel.cr = Math.max(1, ext.dataRows);
      sel.cc = Math.max(1, ext.dataCols);
      refreshChrome();
    }

    function inGridBounds(rr, cc) {
      return rr >= 1 && cc >= 1 && rr <= grid.MAX_ROWS && cc <= grid.MAX_COLS;
    }

    // Ctrl+Arrow from a populated run: walk to the last populated cell of that run.
    function jumpAlongData(nr, nc, dr, dc) {
      let r = nr;
      let c = nc;
      while (inGridBounds(r + dr, c + dc) && api.hasValue(r + dr, c + dc)) {
        r += dr;
        c += dc;
      }
      return { r, c };
    }

    // Ctrl+Arrow across a gap: stop at the next populated cell or at the sheet edge.
    function jumpAcrossGap(from, dr, dc, limit) {
      let nr = from.r + dr;
      let nc = from.c + dc;
      while (inGridBounds(nr, nc) && !api.hasValue(nr, nc)) {
        if (isPastData(dr, dc, nr, nc, limit)) {
          return { r: dr > 0 ? grid.MAX_ROWS : from.r, c: dc > 0 ? grid.MAX_COLS : from.c };
        }
        nr += dr;
        nc += dc;
      }
      if (!inGridBounds(nr, nc)) return { r: dr < 0 ? 1 : from.r, c: dc < 0 ? 1 : from.c };
      return { r: nr, c: nc };
    }

    function jumpTarget(dr, dc, extend) {
      const from = { r: extend ? sel.cr : sel.ar, c: extend ? sel.cc : sel.ac };
      if (!inGridBounds(from.r + dr, from.c + dc)) return from;
      const nr = from.r + dr;
      const nc = from.c + dc;
      if (api.hasValue(from.r, from.c) && api.hasValue(nr, nc)) return jumpAlongData(nr, nc, dr, dc);
      return jumpAcrossGap(from, dr, dc, jumpLimit(dr, dc, api.getExtent()));
    }

    function moveBy(dr, dc, opts) {
      const { extend, jump } = opts;
      if (jump) {
        const t = jumpTarget(dr, dc, extend);
        select(t.r, t.c, extend);
        return;
      }
      let baseR = extend ? sel.cr : sel.ar;
      let baseC = extend ? sel.cc : sel.ac;
      // Leave a merged range past its far edge, and step over hidden rows and columns.
      const merged = grid.mergeAt?.(baseR, baseC);
      if (merged) {
        if (dr > 0) baseR = merged.r2;
        if (dc > 0) baseC = merged.c2;
      }
      let nr = baseR + dr;
      let nc = baseC + dc;
      while (dr && inGridBounds(nr, nc) && grid.rowHidden?.(nr)) nr += dr;
      while (dc && inGridBounds(nr, nc) && grid.colHidden?.(nc)) nc += dc;
      select(nr, nc, extend);
    }

    // ---- edit modes ------------------------------------------------------------------------
    function placeCaret(td) {
      const selObj = typeof host.getSelection === "function" ? host.getSelection() : null;
      if (!selObj || typeof document.createRange !== "function") return;
      const range = document.createRange();
      range.selectNodeContents(td);
      range.collapse(false);
      selObj.removeAllRanges();
      selObj.addRange(range);
    }

    function startEdit(td, mode, replace) {
      const st = stateOf(td) || api.ensureSheetCellFocusEntry(td, api.getSheetName());
      if (st.mode) {
        markEditing(td, st, mode);
        return;
      }
      api.ensurePendingSnapshot(st);
      st.restore = {
        text: grid.readTdValue(td),
        formula: td.dataset.formula || "",
      };
      markEditing(td, st, mode);
      const raw = td.dataset.formula || grid.readTdValue(td);
      delete td.dataset.raw;
      delete td.dataset.display;
      if (replace) {
        td.textContent = "";
        delete td.dataset.formula;
      } else {
        td.textContent = raw;
        api.syncSheetCellFocusBaseline(td, false);
      }
      placeCaret(td);
    }

    function endEdit(td) {
      const st = stateOf(td);
      if (st) {
        st.mode = null;
        st.restore = null;
      }
      delete td.dataset.editMode;
      td.classList.remove("editing");
    }

    function cancelEdit(td) {
      const st = stateOf(td);
      if (!st) return;
      const r = st.restore || {
        text: st.initialText,
        formula: isFormulaStr(st.initialRaw) ? st.initialRaw : "",
      };
      grid.setDisplayValue(td, r.text);
      if (r.formula) td.dataset.formula = r.formula;
      else delete td.dataset.formula;
      st.userEdited = false;
      endEdit(td);
      api.syncSheetCellFocusBaseline(td, true);
      refreshChrome();
    }

    function onFocus(td) {
      updateSelection(td.dataset.cell);
      const text = grid.readTdValue(td);
      const entry = {
        initialText: text,
        initialRaw: td.dataset.formula || text,
        pendingSnapshot: null,
        snapshotPushed: false,
        userEdited: false,
        sheetName: api.getSheetName(),
        mode: null,
        restore: null,
      };
      sheetCellFocusState.set(td, entry);
      api.schedulePendingSnapshot(entry);
    }

    function onInput(td) {
      const st = stateOf(td);
      const rawText = (td.textContent || "").trim();
      if (isFormulaStr(rawText)) td.dataset.formula = rawText;
      else delete td.dataset.formula;
      if (st) {
        if (!st.mode) markEditing(td, st, "edit");
        st.userEdited = true;
        api.pushCellUndoSnapshotIfNeeded(td, st, td.dataset.formula || rawText);
      }
      const formulaInput = document.getElementById("formula-input");
      if (formulaInput && api.getActiveSheetCellRef() === td.dataset.cell) formulaInput.value = rawText;
      refreshChrome();
    }

    // ---- keyboard --------------------------------------------------------------------------
    const ARROWS = {
      ArrowUp: [-1, 0],
      ArrowDown: [1, 0],
      ArrowLeft: [0, -1],
      ArrowRight: [0, 1],
    };

    function handleArrow(ev, st, editing) {
      const [dr, dc] = ARROWS[ev.key];
      if (editing && st.mode === "edit" && dc !== 0) return;
      ev.preventDefault();
      const mod = ev.ctrlKey || ev.metaKey;
      if (editing) {
        select((sel.ar + dr), sel.ac + dc, false);
        return;
      }
      moveBy(dr, dc, { extend: ev.shiftKey, jump: mod });
    }

    function handleEnterTab(ev, editing) {
      ev.preventDefault();
      if (editing) commitIfEditing();
      if (ev.key === "Enter") select(sel.ar + (ev.shiftKey ? -1 : 1), sel.ac, false);
      else select(sel.ar, sel.ac + (ev.shiftKey ? -1 : 1), false);
    }

    function handleNavKey(ev, editing) {
      const mod = ev.ctrlKey || ev.metaKey;
      if (editing) return false;
      const base = ev.shiftKey;
      if (ev.key === "Home") {
        ev.preventDefault();
        select(mod ? 1 : sel.ar, 1, base);
        return true;
      }
      if (ev.key === "End" && mod) {
        ev.preventDefault();
        const ext = api.getExtent();
        select(Math.max(1, ext.dataRows), Math.max(1, ext.dataCols), base);
        return true;
      }
      if (ev.key === "PageDown" || ev.key === "PageUp") {
        ev.preventDefault();
        const span = grid.visibleRowSpan() * (ev.key === "PageDown" ? 1 : -1);
        select((base ? sel.cr : sel.ar) + span, base ? sel.cc : sel.ac, base);
        return true;
      }
      return false;
    }

    function onKeydown(ev) {
      // Alt+Down opens the drop-down of an auto filter header (sheet-filter-ui.js): it moves nothing.
      if (ev.altKey && ev.key === "ArrowDown" && !ev.ctrlKey && !ev.metaKey) return;
      const td = gridTdOf(ev.target) || tdAt(sel.ar, sel.ac);
      if (handleCompositionKey(ev, td)) return;
      const st = td ? stateOf(td) : null;
      const editing = isEditing(st);
      const mod = ev.ctrlKey || ev.metaKey;
      if (ARROWS[ev.key]) return handleArrow(ev, st, editing);
      if (ev.key === "Enter" || ev.key === "Tab") return handleEnterTab(ev, editing);
      if (ev.key === "Escape") return handleEscapeKey(ev, editing, td);
      if (ev.key === "F2" && td) {
        ev.preventDefault();
        startEdit(td, "edit", false);
        return;
      }
      if (handleNavKey(ev, editing)) return;
      if (mod && !ev.shiftKey && !ev.altKey && ev.key === "5") {
        ev.preventDefault();
        api.onFormatKey?.("strike");
        return;
      }
      if (editing) return;
      handleIdleKey(ev, mod, td);
    }

    // Keys that act on the selection (select all, clear, type-to-edit) while no cell is being edited.
    function handleIdleKey(ev, mod, td) {
      if (mod && ev.key.toLowerCase() === "a") {
        ev.preventDefault();
        selectAll();
        return;
      }
      if (ev.key === "Delete" || ev.key === "Backspace") {
        ev.preventDefault();
        void clip.clearRange(getRange()).catch(() => {});
        return;
      }
      if (isPrintable(ev)) startTyping(ev, td);
    }

    function handleEscapeKey(ev, editing, td) {
      if (editing) {
        ev.preventDefault();
        cancelEdit(td);
      } else if (sel.ar !== sel.cr || sel.ac !== sel.cc) {
        select(sel.ar, sel.ac, false);
      }
    }

    // IME composition: a bare "Process" key starts an edit; every composition key is consumed.
    function handleCompositionKey(ev, td) {
      if (!(ev.isComposing || ev.key === "Process" || ev.keyCode === 229)) return false;
      const cst = td ? stateOf(td) : null;
      if (td && !isEditing(cst) && ev.key === "Process") startEdit(td, "enter", true);
      return true;
    }

    function startTyping(ev, td) {
      if (td && document.activeElement === td) {
        startEdit(td, "enter", true);
        return;
      }
      // Focus is on the viewport (the anchor cell scrolled out): bring it back and type for the user.
      grid.ensureVisible(sel.ar, sel.ac);
      const target = tdAt(sel.ar, sel.ac);
      if (!target) return;
      ev.preventDefault();
      target.focus({ preventScroll: true });
      startEdit(target, "enter", true);
      target.textContent = ev.key;
      onInput(target);
      placeCaret(target);
    }

    function onBeforeInput(ev) {
      const td = gridTdOf(ev.target);
      if (!td) return;
      const st = stateOf(td);
      if (isEditing(st)) return;
      if (ev.inputType === "insertText" || ev.inputType === "insertCompositionText") {
        startEdit(td, "enter", true);
      }
    }

    function onCompositionStart(ev) {
      const td = gridTdOf(ev.target);
      if (td && !isEditing(stateOf(td))) startEdit(td, "enter", true);
    }

    // ---- mouse -----------------------------------------------------------------------------
    function onMouseDown(ev) {
      if (ev.button) return;
      const handle = ev.target?.closest?.(".col-resizer");
      if (handle) {
        ev.preventDefault();
        const col = grid.colIndex(handle.dataset.colResizer || "A");
        resize = { col, startX: ev.clientX, startW: grid.colWidth(col) };
        return;
      }
      const head = ev.target?.closest?.("#pane-sheet th[data-col], #pane-sheet th[data-row]");
      if (head) {
        ev.preventDefault();
        if (head.dataset.col) selectAxis("col", grid.colIndex(head.dataset.col), Boolean(ev.shiftKey));
        else selectAxis("row", Number(head.dataset.row), Boolean(ev.shiftKey));
        return;
      }
      const td = gridTdOf(ev.target);
      if (!td) return;
      const st = stateOf(td);
      if (st?.mode && document.activeElement === td) return;
      const p = grid.parseRef(td.dataset.cell);
      if (!p) return;
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
      if (drag) autoScroll(ev);
    }

    function onMouseOver(ev) {
      if (!drag || resize) return;
      const td = gridTdOf(ev.target);
      const p = td ? grid.parseRef(td.dataset.cell) : null;
      if (p && (p.row !== sel.cr || p.col !== sel.cc)) {
        sel.cr = p.row;
        sel.cc = p.col;
        refreshChrome();
      }
    }

    function onMouseUp() {
      const wasResize = Boolean(resize);
      drag = null;
      resize = null;
      if (wasResize) document.documentElement?.dispatchEvent?.({ type: "eo-doc-mutated" });
    }

    function onDblClick(ev) {
      const td = gridTdOf(ev.target);
      if (td) startEdit(td, "edit", false);
    }

    // ---- clipboard events ------------------------------------------------------------------
    function onClipboardEvent(ev) {
      const td = gridTdOf(ev.target);
      if (td && isEditing(stateOf(td))) {
        if (ev.type === "paste") insertPlainText(ev);
        return;
      }
      if (ev.type === "paste") void clip.onPaste(ev, getRange()).catch(() => {});
      else clip.onCopy(ev, getRange(), ev.type === "cut");
    }

    function insertPlainText(ev) {
      const data = ev.clipboardData || ev.dataTransfer;
      const text = data && typeof data.getData === "function" ? data.getData("text/plain") : "";
      if (!text) return;
      ev.preventDefault();
      const flat = text.replace(/[\r\n\t]+/g, " ");
      if (typeof document.execCommand === "function") document.execCommand("insertText", false, flat);
    }

    function runClipboardCommand(cmd) {
      return clip.runCommand(cmd, getRange());
    }

    // ---- wiring ----------------------------------------------------------------------------
    function bindTd(td) {
      if (bound.has(td)) return;
      bound.add(td);
      td.addEventListener("focus", () => onFocus(td));
      td.addEventListener("blur", () => {
        void api.commitSheetCellEdit(td, false).catch(() => {});
      });
    }

    function bindSheetCellEvents(rootEl) {
      rootEl.querySelectorAll("td[data-cell]").forEach(bindTd);
      const vp = viewport();
      if (!vp || delegatesBound) return;
      delegatesBound = true;
      vp.addEventListener("keydown", onKeydown);
      vp.addEventListener("beforeinput", onBeforeInput);
      vp.addEventListener("compositionstart", onCompositionStart);
      vp.addEventListener("input", (ev) => {
        const td = gridTdOf(ev.target);
        if (td) onInput(td);
      });
      vp.addEventListener("mousedown", onMouseDown);
      vp.addEventListener("mouseover", onMouseOver);
      vp.addEventListener("mousemove", onMouseMove);
      vp.addEventListener("mouseup", onMouseUp);
      vp.addEventListener("dblclick", onDblClick);
      ["copy", "cut", "paste"].forEach((type) => vp.addEventListener(type, onClipboardEvent));
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      host.addEventListener?.("mouseup", onMouseUp);
    }

    return {
      bindTd,
      bindSheetCellEvents,
      updateSelection,
      refreshChrome,
      paintSelection,
      focusAnchor,
      commitIfEditing,
      endEdit,
      getRange,
      selectAxis,
      runClipboardCommand,
      shiftFormula: clip.shiftFormula,
      getSelection: () => ({ ...sel }),
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_INPUT__ = createSheetInput;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
