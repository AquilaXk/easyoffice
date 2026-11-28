(function (host) {
  "use strict";

  function isEditing(st) {
    return Boolean(st && (st.mode || st.userEdited));
  }

  function latinKeyOf(ev) {
    const fn = host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__?.latinKeyOf;
    if (typeof fn === "function") return fn(ev);
    if (!ev || ev.isComposing) return "";
    const key = String(ev.key || "");
    const code = String(ev.code || "");
    if ((ev.ctrlKey || ev.metaKey || ev.altKey) && !(ev.getModifierState?.("AltGraph") && !ev.metaKey) && /^Key[A-Z]$/.test(code) && (ev.altKey || key === "Process" || key === "Unidentified" || (key.length === 1 && key.charCodeAt(0) > 0x7f))) {
      return code.slice(3);
    }
    return key.length === 1 ? key.toUpperCase() : "";
  }

  function isPrintable(ev) {
    return ev.key?.length === 1 && !ev.ctrlKey && !ev.metaKey && !ev.altKey;
  }

  function markEditing(td, st, mode) {
    st.mode = mode;
    td.dataset.editMode = mode;
    td.classList.add("editing");
  }

  // Sheet editing interaction: selection model, ready/enter/edit modes, keyboard, mouse, IME, clipboard.
  function createSheetInput(api) {
    const { document, grid, sheetCellFocusState, isFormulaStr } = api;
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    const nav = host.__EASYOFFICE_SHEET_NAV__;
    const multiApi = host.__EASYOFFICE_SHEET_MULTI_RANGE__;
    const sel = { ar: 1, ac: 1, cr: 1, cc: 1, axis: null, whole: false, span: null, tabFrom: null };
    let extraRanges = [];
    const resetNav = (axis, keepExtras = false) => {
      sel.axis = axis;
      sel.whole = false;
      sel.span = null;
      sel.tabFrom = null;
      if (!keepExtras) extraRanges = [];
    };
    const bound = new WeakSet();
    let delegatesBound = false;
    let uiHoldBlur = false;
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
      selectTarget: (target) => {
        if (!target.axis) return select(target.r, target.c, false);
        selectAxis(target.axis, target.index, false);
        focusAnchor();
      },
      tablesIn: (sheet, r1, c1, r2, c2) => host.__EASYOFFICE_SHEET_LAYOUT__.tablesIn(sheet, r1, c1, r2, c2),
      tablesOf: (sheet) => host.__EASYOFFICE_SHEET_LAYOUT__.tablesOf(sheet),
      tableBox: (table) => host.__EASYOFFICE_SHEET_LAYOUT__.tableBox(table),
      getNames: () => host.__EASYOFFICE_SHEET_NAMES__?.getAll?.() ?? [],
      findName: (name, sheet) => host.__EASYOFFICE_SHEET_NAMES__?.find?.(name, sheet) ?? null,
      focusAnchor: () => focusAnchor(),
      focusCell: (row, col, text, from, to, mode) => focusHeldCell(row, col, text, from, to, mode),
      startFormulaWithName: (nameText) => startFormulaWithName(nameText),
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
      return { r1: Math.min(sel.ar, sel.cr), r2: Math.max(sel.ar, sel.cr), c1: Math.min(sel.ac, sel.cc), c2: Math.max(sel.ac, sel.cc) };
    }
    const getRanges = () => (multiApi ? multiApi.rangesOf(getRange(), extraRanges) : [getRange()]);
    function pushRange() {
      if (!multiApi) return;
      const cur = getRange();
      if (extraRanges.some((r) => multiApi.sameRect(r, cur))) return;
      if (extraRanges.length >= multiApi.MAX_MULTI_RANGES) {
        feedback("다중 선택 범위는 최대 " + multiApi.MAX_MULTI_RANGES + "개까지");
        return;
      }
      extraRanges.push(cur);
    }
    function clearExtras() {
      if (extraRanges.length > 0) {
        extraRanges = [];
        refreshChrome();
      }
    }

    function gridTdOf(el) {
      const td = el && typeof el.closest === "function" ? el.closest("td[data-cell]") : null;
      return td && document.getElementById("pane-sheet")?.contains(td) ? td : null;
    }

    const { paintSelection, refreshChrome } = host.__EASYOFFICE_CREATE_SHEET_SELECTION_CHROME__({
      document, api, grid, sel, protectApi, point, fill, getRange, getRanges, getExtraRanges: () => extraRanges, stateOf, tdAt,
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

    function commitIfEditing(keepFocus = false) {
      const ae = document.activeElement;
      const td = gridTdOf(ae);
      if (td && isEditing(stateOf(td))) api.commitSheetCellEdit(td, keepFocus, keepFocus).catch(() => {});
    }

    // Whether a click or a move may land on the cell: a protected sheet can keep locked or unlocked cells from
    // being selected (PD-216).
    function selectable(r, c) {
      return !protectApi || protectApi.canSelect(api.getSheetName(), r, c, api.getEntry(grid.refOf(r, c))?.style);
    }

    function select(row, col, extend, keepExtras = false) {
      const targetRow = clamp(row, 1, grid.MAX_ROWS);
      const targetCol = clamp(col, 1, grid.MAX_COLS);
      const merged = extend ? null : grid.mergeAt?.(targetRow, targetCol);
      const r = merged ? merged.r1 : targetRow;
      const c = merged ? merged.c1 : targetCol;
      if (!extend && !selectable(r, c)) return;
      commitIfEditing();
      resetNav(null, keepExtras || extend);
      if (extend) {
        sel.cr = r;
        sel.cc = c;
      } else {
        sel.ar = sel.cr = r;
        sel.ac = sel.cc = c;
      }
      const frozen = merged && grid.frozenCounts ? grid.frozenCounts() : null;
      const vr = frozen && r <= frozen.rows && targetRow > frozen.rows ? targetRow : r;
      const vc = frozen && c <= frozen.cols && targetCol > frozen.cols ? targetCol : c;
      grid.ensureVisible(vr, vc);
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

    // Selects several ranges at once (이동 옵션): the first is the main range, the others its companions (at most `MAX_MULTI_RANGES`).
    function selectRanges(list) {
      selectRange(list[0]);
      if (!multiApi || list.length < 2) return;
      extraRanges = list.slice(1, multiApi.MAX_MULTI_RANGES).map((r) => ({ ...r }));
      refreshChrome();
    }

    // Selects whole rows or columns (a click on a header; `extend` grows the selection to `index`).
    function selectAxis(axis, index, extend, keepExtras = false) {
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
      resetNav(axis, keepExtras || extend);
      refreshChrome();
    }

    function selectAxisSpan(axis, first, last) {
      commitIfEditing();
      if (axis === "col") {
        sel.ac = clamp(first, 1, grid.MAX_COLS);
        sel.cc = clamp(last, 1, grid.MAX_COLS);
        sel.ar = 1;
        sel.cr = grid.MAX_ROWS;
      } else {
        sel.ar = clamp(first, 1, grid.MAX_ROWS);
        sel.cr = clamp(last, 1, grid.MAX_ROWS);
        sel.ac = 1;
        sel.cc = grid.MAX_COLS;
      }
      resetNav(axis, false);
      focusAnchor();
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
      const frozen = grid.frozenCounts ? grid.frozenCounts() : { rows: 0, cols: 0 };
      return {
        maxRows: grid.MAX_ROWS,
        maxCols: grid.MAX_COLS,
        dataRows: ext.dataRows,
        dataCols: ext.dataCols,
        freezeRows: frozen.rows,
        freezeCols: frozen.cols,
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
    const boxApi = host.__EASYOFFICE_SHEET_EDIT_BOX__;
    const setCaretRange = (td, from, to = from) => boxApi.setCaretRange(document, host, td, from, to);
    const setCaretAtOffset = (td, offset) => boxApi.setCaretRange(document, host, td, offset, offset);
    const placeCaret = (td) => boxApi.placeCaret(document, host, td);
    const textOffsetIn = (root, target, offset) => boxApi.textOffsetIn(root, target, offset);
    const spliceTextAt = (raw, start, end, ch) => boxApi.spliceTextAt(raw, start, end, ch);

    function insertNewlineAtCaret(td) {
      const raw = td.textContent || "";
      let startPos = raw.length;
      let endPos = raw.length;
      const selObj = typeof host.getSelection === "function" ? host.getSelection() : null;
      if (selObj && selObj.rangeCount > 0) {
        const range = selObj.getRangeAt(0);
        if (range?.startContainer && range?.endContainer && td.contains(range.startContainer) && td.contains(range.endContainer)) {
          const s = textOffsetIn(td, range.startContainer, range.startOffset);
          const e = textOffsetIn(td, range.endContainer, range.endOffset);
          if (s !== null && e !== null) {
            startPos = Math.min(s, e);
            endPos = Math.max(s, e);
          }
        }
      }
      td.textContent = raw.slice(0, startPos) + "\n" + raw.slice(endPos);
      setCaretAtOffset(td, startPos + 1);
      onInput(td);
      editBox.fit(td);
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
      host.__EASYOFFICE_SHEET_LOOK__?.syncRotSpan?.(td);
      api.syncSheetCellFocusBaseline(td, true);
      refreshChrome();
    }

    function onFocus(td) {
      const existing = stateOf(td);
      if ((point.holdsBlur() || uiHoldBlur) && existing && isEditing(existing)) {
        updateSelection(td.dataset.cell);
        return;
      }
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

    function focusHeldCell(row, col, text, from, to, mode) {
      grid.ensureVisible(row, col);
      const td = tdAt(row, col);
      if (!td) return null;
      td.focus({ preventScroll: true });
      let st = stateOf(td);
      if (!st || !isEditing(st)) {
        startEdit(td, mode || "edit", false);
        st = stateOf(td);
      } else {
        markEditing(td, st, mode || st.mode);
      }
      if (td.textContent !== text) td.textContent = text;
      const trimmed = text.trim();
      if (!trimmed.startsWith("'") && td._nf !== "@" && isFormulaStr(trimmed)) td.dataset.formula = trimmed;
      else delete td.dataset.formula;
      setCaretRange(td, from, to);
      editBox.fit(td);
      refreshChrome();
      return td;
    }

    function startFormulaWithName(nameText) {
      grid.ensureVisible(sel.ar, sel.ac);
      const td = tdAt(sel.ar, sel.ac);
      if (!td || !nameText || editBlocked(td)) return false;
      if (gridTdOf(document.activeElement) !== td) td.focus({ preventScroll: true });
      startEdit(td, "edit", true);
      if (!isEditing(stateOf(td))) return false;
      const text = "=" + nameText;
      td.textContent = text;
      setCaretRange(td, text.length, text.length);
      onInput(td);
      return true;
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

    const shortcuts = host.__EASYOFFICE_CREATE_SHEET_SHORTCUTS__?.({
      document,
      api,
      grid,
      sel,
      getRange,
      getRanges,
      selectAxisSpan,
      focusAnchor,
      discardInlineEdit: (td) => cancelEdit(td),
      clip,
      visibleEntryOf,
      openFormatDialog: () => document.querySelector('#menu-bar .md-item[data-sheet-numfmt="dialog"]')?.click(),
      runStructureAct: (act) => api.runStructure?.(act) ?? document.querySelector('#menu-bar .md-item[data-sheet-act="' + act + '"]')?.click(),
      triggerAutoSum: () => document.querySelector('#icon-toolbar [data-studio-cmd="data-autosum"], #menu-bar .md-item[data-studio-menu-cmd="data-autosum"]')?.click(),
      feedback,
    });

    function handleEnterTab(ev, editing, targetTd) {
      if (editing && ev.key === "Enter") {
        if (ev.altKey && !ev.ctrlKey && !ev.metaKey) {
          ev.preventDefault();
          const td = targetTd || gridTdOf(document.activeElement) || tdAt(sel.ar, sel.ac);
          if (td) insertNewlineAtCaret(td);
          return;
        }
        if ((ev.ctrlKey || ev.metaKey) && !ev.altKey) {
          if (shortcuts?.handleKey(ev, true, targetTd)) return;
          ev.preventDefault();
          commitIfEditing(true);
          return;
        }
      }
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
      if (ev.key === "Home" && !ev.altKey) to = nav.home(view, { r: sel.ar, c: sel.ac }, mod);
      else if (ev.key === "End" && mod && !ev.altKey) to = nav.end(view);
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
      const mac = isMacClient();
      // Ctrl+PageDown and Ctrl+PageUp (on a Mac also Option+Right and Option+Left) go to the next and previous sheet (PD-291); a cell
      // being edited keeps the keys for its text.
      const sheetStep = !editing && api.stepSheet ? nav.sheetStep(ev, mac) : 0;
      if (sheetStep !== 0) {
        ev.preventDefault();
        api.stepSheet(sheetStep);
        return;
      }
      // Ctrl+Shift+PageDown and Ctrl+Shift+PageUp (on a Mac also Option+Shift+Right and Option+Left) extend or shrink sheet group (PD-346).
      const sheetGroupStep = !editing && api.stepSheetGroup ? nav.sheetGroupStep(ev, mac) : 0;
      if (sheetGroupStep !== 0) {
        ev.preventDefault();
        api.stepSheetGroup(sheetGroupStep);
        return;
      }
      if (ARROWS[ev.key]) return handleArrow(ev, st, editing);
      if (ev.key === "Enter" || ev.key === "Tab") return handleEnterTab(ev, editing, td);
      if (ev.key === "Escape") return handleEscapeKey(ev, editing, td);
      if (ev.key === "F2" && td) {
        ev.preventDefault();
        startEdit(td, "edit", false);
        return;
      }
      if (handleNavKey(ev, editing)) return;
      if (shortcuts?.handleKey(ev, editing, td)) return;
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
      const latin = latinKeyOf(ev);
      if (mod && ev.altKey && latin === "V") {
        ev.preventDefault();
        pasteUi.open();
        return;
      }
      if (mod && !ev.altKey && latin === "A") {
        ev.preventDefault();
        selectAll();
        return;
      }
      if (ev.key === "Delete" || ev.key === "Backspace") {
        ev.preventDefault();
        void (clip.clearRanges ? clip.clearRanges(getRanges()) : clip.clearRange(getRange())).catch(() => {});
        return;
      }
      if (isPrintable(ev)) startTyping(ev, td);
    }

    function handleEscapeKey(ev, editing, td) {
      if (editing) {
        ev.preventDefault();
        cancelEdit(td);
      } else if (sel.ar !== sel.cr || sel.ac !== sel.cc || extraRanges.length > 0) {
        select(sel.ar, sel.ac, false);
      }
    }

    // IME composition: a bare "Process" key starts an edit; every composition key is consumed.
    function handleCompositionKey(ev, td) {
      if ((ev.ctrlKey || ev.metaKey || ev.altKey) && !ev.isComposing) return false;
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

    const mouse = host.__EASYOFFICE_CREATE_SHEET_MOUSE__({
      document, api, grid, sel, protectApi, point, fill, viewport, gridTdOf, stateOf, select, selectAxis, commitIfEditing, startEdit, refreshChrome, pushRange,
    });

    function onClipboardEvent(ev) {
      const td = gridTdOf(ev.target);
      if (td && isEditing(stateOf(td))) {
        if (ev.type === "paste") insertPlainText(ev);
        return;
      }
      if (ev.type === "paste") {
        void clip.onPaste(ev, getRange()).catch(() => {});
        return;
      }
      if (getRanges().length > 1) {
        ev.preventDefault();
        feedback("이 명령은 여러 개의 선택 영역에서 실행할 수 없습니다.");
        return;
      }
      clip.onCopy(ev, getRange(), ev.type === "cut");
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
      if ((cmd === "clip-copy" || cmd === "clip-cut") && getRanges().length > 1) {
        feedback("이 명령은 여러 개의 선택 영역에서 실행할 수 없습니다.");
        return Promise.resolve(false);
      }
      return clip.runCommand(cmd, getRange());
    }

    // ---- wiring ----------------------------------------------------------------------------
    function bindTd(td) {
      if (bound.has(td)) return;
      bound.add(td);
      td.addEventListener("focus", () => onFocus(td));
      td.addEventListener("blur", () => {
        if (point.holdsBlur() || uiHoldBlur) return;
        void api.commitSheetCellEdit(td, false).catch(() => {});
      });
    }

    function captureSymbolTarget() {
      const ae = document.activeElement, bar = document.getElementById("formula-input");
      if (bar && ae === bar) return { kind: "formulaBar", start: bar.selectionStart ?? bar.value.length, end: bar.selectionEnd ?? bar.value.length };
      const td = gridTdOf(ae) || tdAt(sel.ar, sel.ac), st = td ? stateOf(td) : null;
      if (!td || !isEditing(st)) return { kind: "cell", ref: grid.refOf(sel.ar, sel.ac), row: sel.ar, col: sel.ac };
      let startOffset = (td.textContent || "").length, endOffset = startOffset;
      const selObj = host.getSelection?.(), r = selObj?.rangeCount > 0 ? selObj.getRangeAt(0) : null;
      if (r && td.contains(r.startContainer) && td.contains(r.endContainer)) {
        const s = textOffsetIn(td, r.startContainer, r.startOffset), e = textOffsetIn(td, r.endContainer, r.endOffset);
        if (s !== null && e !== null) { startOffset = Math.min(s, e); endOffset = Math.max(s, e); }
      }
      return { kind: "editing", ref: td.dataset.cell, row: td._row, col: td._col, startOffset, endOffset };
    }
    function insertSymbolAtEditingTarget(ch, target) {
      uiHoldBlur = false;
      if (target?.kind === "formulaBar") {
        const bar = document.getElementById("formula-input");
        if (!bar) return { handled: false };
        const next = spliceTextAt(bar.value, target.start, target.end, ch);
        bar.value = next.text;
        bar.focus?.();
        bar.setSelectionRange?.(next.caret, next.caret);
        bar.dispatchEvent?.(new (host.Event || Event)("input", { bubbles: true }));
        return { handled: true, inserted: true };
      }
      const td = target?.kind === "editing" ? tdAt(target.row, target.col) : null;
      if (!td || !isEditing(stateOf(td))) return { handled: false };
      const next = spliceTextAt(td.textContent, target.startOffset, target.endOffset, ch);
      td.textContent = next.text;
      td.focus?.({ preventScroll: true });
      setCaretAtOffset(td, next.caret);
      onInput(td);
      return { handled: true, inserted: true };
    }
    function restoreSymbolFocus(target) {
      const td = target?.kind === "editing" ? tdAt(target.row, target.col) : null;
      if (td && isEditing(stateOf(td))) { td.focus?.({ preventScroll: true }); setCaretRange(td, target.startOffset, target.endOffset); }
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
      const tabBar = document.getElementById("sheet-tab-bar");
      tabBar?.addEventListener("keydown", (ev) => {
        if (ev.target?.closest?.("input, textarea")) return;
        const mac = isMacClient();
        const sStep = api.stepSheet ? nav.sheetStep(ev, mac) : 0;
        if (sStep !== 0) {
          ev.preventDefault();
          api.stepSheet(sStep);
          return;
        }
        const gStep = api.stepSheetGroup ? nav.sheetGroupStep(ev, mac) : 0;
        if (gStep !== 0) {
          ev.preventDefault();
          api.stepSheetGroup(gStep);
        }
      });
    }

    return {
      bindTd, bindSheetCellEvents, updateSelection, refreshChrome, paintSelection, focusAnchor, commitIfEditing, endEdit,
      getRange, getRanges, clearExtras, clearSelectionContents: () => (clip.clearRanges ? clip.clearRanges(getRanges()) : clip.clearRange(getRange())),
      openPasteSpecial: () => pasteUi.open(),
      copyFormats: (range) => clip.copyFormats(range),
      pasteFormats: (snap, range) => clip.pasteFormats(snap, range),
      selectAxis, selectRange, selectRanges, restoreSelection, runClipboardCommand, shiftFormula: clip.shiftFormula, getSelection: () => ({ ...sel }),
      handleBarKey: (ev) => { if (!shortcuts?.handleBarKey(ev)) point.barKey(ev); },
      holdsTabPress: point.holdsTabPress, detachForSwitch: point.detach, afterSheetSwitch: point.afterSwitch, insertFormulaName: point.insertName,
      hasFormulaEditSurface: point.hasInsertSurface, holdsFormulaMenu: point.holdsFormulaMenu, isEditingFormula: point.isEditingFormula,
      originSheet: point.originSheet, beginFormulaInsertHold: point.beginInsertHold, endFormulaInsertHold: point.endInsertHold,
      setUiHold: (on) => { uiHoldBlur = Boolean(on); }, captureSymbolTarget, insertSymbolAtEditingTarget, restoreSymbolFocus,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_INPUT__ = createSheetInput;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
