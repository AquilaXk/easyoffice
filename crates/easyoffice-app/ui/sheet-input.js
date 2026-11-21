(function (host) {
  "use strict";

  function isEditing(st) {
    return Boolean(st && (st.mode || st.userEdited));
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
  // Ctrl+Arrow, Enter and Tab inside a range, Home, End and the page keys are the rules of sheet-nav.js (PD-280).

  function createSheetInput(api) {
    const { document, grid, sheetCellFocusState, isFormulaStr } = api;
    // Sheet protection (PD-216); absent in pages that do not load it.
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    const nav = host.__EASYOFFICE_SHEET_NAV__;
    // `axis`: "row" or "col" while a whole row or column header is selected, else null.
    // `span`: the range Enter and Tab walk the active cell through (it is not the corners ar..cr then), else null.
    // `tabFrom`: the cell the current row of Tabs started from, which the Enter after it returns to the column of.
    // `whole`: the selection is the whole sheet (Ctrl+A), which is as far as the sheet's data goes here; the row height of it is the
    // sheet's default (PD-286).
    const sel = { ar: 1, ac: 1, cr: 1, cc: 1, axis: null, whole: false, span: null, tabFrom: null };
    // A selection that is set (not walked) drops the walk and the Tab row.
    const resetNav = (axis) => {
      sel.axis = axis;
      sel.whole = false;
      sel.span = null;
      sel.tabFrom = null;
    };
    const bound = new WeakSet();
    let delegatesBound = false;
    const feedback = (text) => {
      const el = document.getElementById("status-command-feedback");
      if (el) el.textContent = text;
    };
    const clip = host.__EASYOFFICE_CREATE_SHEET_CLIPBOARD__({
      grid,
      isFormulaStr,
      getEntry: (ref) => api.getEntry(ref),
      getSheetName: () => api.getSheetName(),
      feedback,
      applyCellEdits: (edits, label, widths, merges) => api.applyCellEdits(edits, label, widths, merges),
      clearExisting: (range, dropLinks) => api.clearExisting(range, dropLinks),
    });
    const pasteUi = host.__EASYOFFICE_CREATE_SHEET_PASTE_UI__({
      document,
      clip,
      getRange: () => getRange(),
      commitIfEditing: () => commitIfEditing(),
      kit: host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document),
    });
    const fill = host.__EASYOFFICE_CREATE_SHEET_FILL_UI__({
      document,
      grid,
      getRange: () => getRange(),
      getAxis: () => sel.axis,
      getEntry: (ref) => visibleEntryOf(ref),
      getSheetName: () => api.getSheetName(),
      applyCellEdits: (edits, label) => api.applyCellEdits(edits, label),
      setRange: (rg) => setRange(rg),
      commitIfEditing: () => commitIfEditing(),
      shiftFormula: clip.shiftFormula,
      feedback,
    });

    const point = host.__EASYOFFICE_CREATE_SHEET_POINT_UI__({
      document,
      grid,
      feedback,
      getSheetName: () => api.getSheetName(),
      getActiveCell: () => ({ r: sel.ar, c: sel.ac }),
      refOf: (r, c) => grid.refOf(r, c),
      quoteSheet: (name) => host.__EASYOFFICE_SHEET_UTILS__.quoteFormulaSheetName(name),
      gridTdOf: (el) => gridTdOf(el),
      stateOf: (td) => stateOf(td),
      editable: (td) => !protectApi || protectApi.cellEditable(api.getSheetName(), td._row, td._col, td._st),
      setMode: (td, mode) => markEditing(td, stateOf(td), mode),
      afterWrite: (td) => onInput(td),
      stepFrom: (from, dr, dc, jump) => stepFrom(from, dr, dc, jump),
      cancelEdit: (td) => cancelEdit(td),
      resumeCell: (row, col, mode) => resumeCell(row, col, mode),
      finishCell: (commit) => finishCell(commit),
      restoreBar: () => restoreBar(),
      switchSheet: (name) => api.switchSheet(name),
    });

    // The entry of a cell as a fill may read it: a formula a protected sheet hides is a value (PD-216).
    function visibleEntryOf(ref) {
      const entry = api.getEntry(ref);
      const at = protectApi && entry?.formula ? grid.parseRef(ref) : null;
      return at ? protectApi.visibleEntry(api.getSheetName(), at.row, at.col, entry) : entry;
    }

    // The box of the cell being edited grows with its text (PD-280).
    const editBox = host.__EASYOFFICE_SHEET_EDIT_BOX__.createEditBox({
      document,
      isFixedRow: (td) => Boolean(host.__EASYOFFICE_SHEET_LAYOUT__?.rowHeightsOf(api.getSheetName())?.has(td._row)),
      rowHeightOf: (td) => grid.rowHeight(td._row),
    });

    const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
    const viewport = () => document.getElementById("sheet-viewport");
    const stateOf = (td) => sheetCellFocusState.get(td);

    function tdAt(row, col) {
      if (typeof grid.getTdAt === "function") return grid.getTdAt(row, col);
      return document.querySelector('#pane-sheet td[data-cell="' + grid.refOf(row, col) + '"]');
    }

    function getRange() {
      if (sel.span) return { ...sel.span };
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

    // The cell classes, name box, formula bar text and status aggregate that follow the selection (`sheet-selection-chrome.js`).
    const { paintSelection, refreshChrome } = host.__EASYOFFICE_CREATE_SHEET_SELECTION_CHROME__({
      document, api, grid, sel, protectApi, point, fill, getRange, stateOf, tdAt,
    });

    // Public selection refresh (focus, outline links, restore, recalculation).
    function updateSelection(cellRef) {
      const p = grid.parseRef(cellRef) || { row: 1, col: 1 };
      if (p.row !== sel.ar || p.col !== sel.ac) {
        resetNav(null);
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

    // Whether a click or a move may land on the cell: a protected sheet can keep locked or unlocked cells from
    // being selected (PD-216).
    function selectable(r, c) {
      return !protectApi || protectApi.canSelect(api.getSheetName(), r, c, api.getEntry(grid.refOf(r, c))?.style);
    }

    function select(row, col, extend) {
      let r = clamp(row, 1, grid.MAX_ROWS);
      let c = clamp(col, 1, grid.MAX_COLS);
      if (!extend && !selectable(r, c)) return;
      commitIfEditing();
      // A click or a move that lands inside a merged range selects its anchor cell.
      const merged = extend ? null : grid.mergeAt?.(r, c);
      if (merged) {
        r = merged.r1;
        c = merged.c1;
      }
      resetNav(null);
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

    // Selects a range (the cells of a fill) with its first cell as the anchor.
    function setRange(rg) {
      resetNav(null);
      sel.ar = rg.r1;
      sel.ac = rg.c1;
      sel.cr = rg.r2;
      sel.cc = rg.c2;
    }

    // Gives the selection back as a sheet's view had it, `{ ar, ac, cr, cc }`, without scrolling to it.
    function restoreSelection(s) {
      const last = { row: grid.MAX_ROWS, col: grid.MAX_COLS };
      Object.assign(sel, s);
      const [r1, r2, c1, c2] = [Math.min(s.ar, s.cr), Math.max(s.ar, s.cr), Math.min(s.ac, s.cc), Math.max(s.ac, s.cc)];
      resetNav(c1 === 1 && c2 >= last.col ? "row" : r1 === 1 && r2 >= last.row ? "col" : null);
      refreshChrome();
    }

    // Selects a range from outside the grid (the name box): the first cell is the anchor and the range is shown.
    function selectRange(rg) {
      const r1 = clamp(rg.r1, 1, grid.MAX_ROWS);
      const c1 = clamp(rg.c1, 1, grid.MAX_COLS);
      // The name box does not reach a cell the sheet keeps from selection (PD-216).
      if (!selectable(r1, c1)) return;
      commitIfEditing();
      setRange({ r1, c1, r2: clamp(rg.r2, r1, grid.MAX_ROWS), c2: clamp(rg.c2, c1, grid.MAX_COLS) });
      const whole = rg.c1 === 1 && rg.c2 >= grid.MAX_COLS ? "row" : rg.r1 === 1 && rg.r2 >= grid.MAX_ROWS ? "col" : null;
      resetNav(whole);
      grid.ensureVisible(r1, c1);
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
      resetNav(axis);
      refreshChrome();
    }

    function selectAll() {
      resetNav(null);
      sel.whole = true;
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

    // The sheet as the movement rules of sheet-nav.js read it.
    function navView() {
      const ext = api.getExtent();
      return {
        maxRows: grid.MAX_ROWS,
        maxCols: grid.MAX_COLS,
        dataRows: ext.dataRows,
        dataCols: ext.dataCols,
        hasValue: (r, c) => api.hasValue(r, c),
        mergeAt: (r, c) => grid.mergeAt?.(r, c) || null,
        rowHidden: (r) => Boolean(grid.rowHidden?.(r)),
        colHidden: (c) => Boolean(grid.colHidden?.(c)),
      };
    }

    // The next cell in direction `(dr, dc)` that may be selected: a protected sheet that keeps locked (or unlocked)
    // cells from being selected is walked over, so Tab and the arrows reach the cells that can be.
    function stepSelectable(r, c, dr, dc) {
      if (!protectApi || !protectApi.isOn(api.getSheetName()) || (dr === 0 && dc === 0)) return { r, c };
      let nr = r;
      let nc = c;
      for (let i = 0; i < 4096 && inGridBounds(nr, nc) && !selectable(nr, nc); i += 1) {
        nr += dr;
        nc += dc;
      }
      return { r: nr, c: nc };
    }

    // The cell a pointed reference moves to from `from`: a jump to the edge of the data, or a step over the hidden rows and columns.
    function stepFrom(from, dr, dc, jump) {
      return jump ? nav.jump(navView(), from, dr, dc) : nav.step(navView(), from, dr, dc);
    }

    // Arrow keys, with Ctrl (`jump`) to the edge of the data and with Shift (`extend`) from the moving end of the range.
    // A walk through the range (Enter, Tab) ends here: the range is then reshaped from the active cell.
    function moveBy(dr, dc, opts) {
      const { extend, jump } = opts;
      if (sel.span) {
        sel.span = null;
        sel.cr = sel.ar;
        sel.cc = sel.ac;
      }
      const to = stepFrom({ r: extend ? sel.cr : sel.ar, c: extend ? sel.cc : sel.ac }, dr, dc, jump);
      const next = extend || jump ? to : stepSelectable(to.r, to.c, dr, dc);
      select(next.r, next.c, extend);
    }

    // Enter and Tab (`dr`, `dc` is one of the four steps). With several cells selected the active cell walks through
    // them and the range stays; a row of Tabs sends the Enter after it back to the column it started in.
    function walk(dr, dc) {
      const range = getRange();
      const view = navView();
      const at = { r: sel.ar, c: sel.ac };
      if (range.r1 !== range.r2 || range.c1 !== range.c2) {
        const next = nav.cycle(view, range, at, dr, dc);
        Object.assign(sel, { ar: next.r, ac: next.c, cr: next.r, cc: next.c, span: range, tabFrom: null });
        grid.ensureVisible(next.r, next.c);
        focusAnchor();
        refreshChrome();
        return;
      }
      const tabbing = dc > 0 && !dr;
      const origin = tabbing ? sel.tabFrom || at : null;
      const base = dr > 0 && sel.tabFrom ? sel.tabFrom : at;
      const to = nav.step(view, base, dr, dc);
      const next = stepSelectable(to.r, to.c, dr, dc);
      select(next.r, next.c, false);
      sel.tabFrom = origin;
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

    // A locked cell of a protected sheet is not typed into: the status line says why.
    function editBlocked(td) {
      if (!protectApi || protectApi.cellEditable(api.getSheetName(), td._row, td._col, td._st)) return false;
      // A cell of an allowed range with a password asks it first (범위 잠금 해제).
      const range = protectApi.lockedRangeAt(api.getSheetName(), td._row, td._col);
      if (range && protectApi.askRangeUnlock) {
        protectApi.askRangeUnlock(api.getSheetName(), range.name);
        return true;
      }
      protectApi.alertBlocked();
      return true;
    }

    function startEdit(td, mode, replace) {
      if (editBlocked(td)) return;
      const st = stateOf(td) || api.ensureSheetCellFocusEntry(td, api.getSheetName());
      if (st.mode) {
        markEditing(td, st, mode);
        return;
      }
      api.ensurePendingSnapshot(st);
      st.restore = {
        text: grid.readTdValue(td),
        formula: td.dataset.formula || "",
        ct: td._ct || "",
      };
      markEditing(td, st, mode);
      // A formula a protected sheet hides is not unfolded into the cell: the edit shows the value (PD-216).
      const hidden = protectApi?.formulaHidden(api.getSheetName(), td._row, td._col, td._st);
      const raw = host.__EASYOFFICE_SHEET_CELL_TYPE__.editableRaw(
        grid.readTdValue(td),
        hidden ? "" : td.dataset.formula || "",
        td._ct,
        td._nf
      );
      delete td.dataset.raw;
      delete td.dataset.display;
      if (replace) {
        td.textContent = "";
        delete td.dataset.formula;
        td._ct = "";
        st.editBaseline = null;
      } else {
        td.textContent = raw;
        st.editBaseline = raw;
      }
      placeCaret(td);
      editBox.fit(td);
      point.refresh();
    }

    function endEdit(td) {
      const st = stateOf(td);
      if (st) {
        st.mode = null;
        st.restore = null;
        st.editBaseline = null;
      }
      delete td.dataset.editMode;
      td.classList.remove("editing");
      editBox.release(td);
      // The caret scrolled a long text inside the cell; the finished cell shows its text from the start again.
      td.scrollLeft = 0;
      point.editedEnded();
    }

    function cancelEdit(td) {
      const st = stateOf(td);
      if (!st) return;
      const r = st.restore || {
        text: st.initialText,
        formula: !st.initialCt && isFormulaStr(st.initialRaw) ? st.initialRaw : "",
        ct: st.initialCt || "",
      };
      td._ct = r.ct || "";
      grid.setDisplayValue(td, r.text);
      if (r.formula) td.dataset.formula = r.formula;
      else delete td.dataset.formula;
      st.userEdited = false;
      api.retractCellUndoSnapshot(st);
      endEdit(td);
      api.syncSheetCellFocusBaseline(td, true);
      refreshChrome();
    }

    function onFocus(td) {
      updateSelection(td.dataset.cell);
      const text = grid.readTdValue(td);
      const entry = {
        initialText: text,
        initialCt: td._ct || "",
        initialRaw: host.__EASYOFFICE_SHEET_CELL_TYPE__.editableRaw(text, td.dataset.formula || "", td._ct, td._nf),
        editBaseline: null,
        ref: td.dataset.cell,
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
      if (!rawText.startsWith("'") && td._nf !== "@" && isFormulaStr(rawText)) td.dataset.formula = rawText;
      else delete td.dataset.formula;
      if (st) {
        if (!st.mode) markEditing(td, st, "edit");
        st.userEdited = true;
        api.pushCellUndoSnapshotIfNeeded(td, st, td.dataset.formula || rawText);
      }
      const formulaInput = document.getElementById("formula-input");
      if (formulaInput && api.getActiveSheetCellRef() === td.dataset.cell) formulaInput.value = rawText;
      editBox.fit(td);
      refreshChrome();
      point.afterInput();
    }

    // The cell an entry that went on in the formula bar comes back to: edited again, with the mode it had.
    function resumeCell(row, col, mode) {
      grid.ensureVisible(row, col);
      const td = tdAt(row, col);
      if (!td) return null;
      td.focus({ preventScroll: true });
      startEdit(td, mode, false);
      return isEditing(stateOf(td)) ? td : null;
    }

    // Enter (`commit`) or Esc on the cell being edited; false when no cell is.
    function finishCell(commit) {
      const td = gridTdOf(document.activeElement);
      if (!td || !isEditing(stateOf(td))) return false;
      if (commit) handleEnterTab({ key: "Enter", shiftKey: false, preventDefault() {} }, true);
      else cancelEdit(td);
      return true;
    }

    // Esc in the formula bar: the bar shows the cell again and the grid has the focus.
    function restoreBar() {
      document.getElementById("formula-input")?.blur?.();
      refreshChrome();
      focusAnchor();
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
      moveBy(dr, dc, { extend: !editing && ev.shiftKey, jump: !editing && mod });
    }

    function handleEnterTab(ev, editing) {
      ev.preventDefault();
      if (editing) commitIfEditing();
      const dr = ev.key === "Enter" ? (ev.shiftKey ? -1 : 1) : 0;
      const dc = ev.key === "Enter" ? 0 : ev.shiftKey ? -1 : 1;
      walk(dr, dc);
    }

    // The columns one screen to the side holds (Alt+PageDown).
    function visibleColSpan() {
      const vp = viewport();
      if (!(vp?.clientWidth > 0)) return 8;
      const left = vp.scrollLeft || 0;
      return Math.max(1, grid.colAtX(left + vp.clientWidth - grid.ROW_HEAD_W) - grid.colAtX(left) - 1);
    }

    // Home, Ctrl+End and the page keys; with Shift they move the end of the range the active cell started.
    function handleNavKey(ev, editing) {
      const mod = ev.ctrlKey || ev.metaKey;
      if (editing) return false;
      const extend = ev.shiftKey;
      const view = navView();
      const from = { r: extend ? sel.cr : sel.ar, c: extend ? sel.cc : sel.ac };
      let to = null;
      if (ev.key === "Home") to = nav.home(view, { r: sel.ar, c: sel.ac }, mod);
      else if (ev.key === "End" && mod) to = nav.end(view);
      else if ((ev.key === "PageDown" || ev.key === "PageUp") && !mod) {
        const sign = ev.key === "PageDown" ? 1 : -1;
        to = ev.altKey ? nav.page(view, from, 0, sign, visibleColSpan()) : nav.page(view, from, sign, 0, grid.visibleRowSpan());
      }
      if (!to) return false;
      ev.preventDefault();
      select(to.r, to.c, extend);
      return true;
    }

    const isMacClient = () => host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__?.guessClientPlatform?.().platform === "macOS";

    function onKeydown(ev) {
      // Alt+Down opens the drop-down of an auto filter header (sheet-filter-ui.js): it moves nothing.
      if (ev.altKey && ev.key === "ArrowDown" && !ev.ctrlKey && !ev.metaKey) return;
      const td = gridTdOf(ev.target) || tdAt(sel.ar, sel.ac);
      if (handleCompositionKey(ev, td)) return;
      if (point.key(ev)) return;
      const st = td ? stateOf(td) : null;
      const editing = isEditing(st);
      const mod = ev.ctrlKey || ev.metaKey;
      // Ctrl+PageDown and Ctrl+PageUp (on a Mac also Option+Right and Option+Left) go to the next and previous sheet (PD-291); a cell
      // being edited keeps the keys for its text.
      const sheetStep = !editing && api.stepSheet ? nav.sheetStep(ev, isMacClient()) : 0;
      if (sheetStep !== 0) {
        ev.preventDefault();
        api.stepSheet(sheetStep);
        return;
      }
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
      if (mod && ev.altKey && (ev.code === "KeyV" || ev.key.toLowerCase() === "v")) {
        ev.preventDefault();
        pasteUi.open();
        return;
      }
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
    // Presses, drags, column resizing and the double click (`sheet-mouse.js`).
    const mouse = host.__EASYOFFICE_CREATE_SHEET_MOUSE__({
      document, api, grid, sel, protectApi, point, fill, viewport, gridTdOf, stateOf, select, selectAxis, commitIfEditing, startEdit, refreshChrome,
    });

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
      point.bind();
      vp.addEventListener("keydown", onKeydown);
      vp.addEventListener("beforeinput", onBeforeInput);
      vp.addEventListener("compositionstart", onCompositionStart);
      vp.addEventListener("input", (ev) => {
        const td = gridTdOf(ev.target);
        if (td) onInput(td);
      });
      vp.addEventListener("mousedown", mouse.onMouseDown);
      vp.addEventListener("mouseover", mouse.onMouseOver);
      vp.addEventListener("mousemove", mouse.onMouseMove);
      vp.addEventListener("mouseup", mouse.onMouseUp);
      vp.addEventListener("dblclick", mouse.onDblClick);
      ["copy", "cut", "paste"].forEach((type) => vp.addEventListener(type, onClipboardEvent));
      document.addEventListener("mousemove", mouse.onMouseMove);
      document.addEventListener("mouseup", mouse.onMouseUp);
      host.addEventListener?.("mouseup", mouse.onMouseUp);
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
      selectRange,
      restoreSelection,
      runClipboardCommand,
      shiftFormula: clip.shiftFormula,
      getSelection: () => ({ ...sel }),
      handleBarKey: point.barKey,
      detachForSwitch: point.detach,
      afterSheetSwitch: point.afterSwitch,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_INPUT__ = createSheetInput;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
