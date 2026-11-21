(function (host) {
  "use strict";

  // The focus state of a sheet cell and the commit of an entry into it. A cell that takes the focus gets a state (`sheetCellFocusState`:
  // the text and type it held, whether the user changed it, the undo step pushed for it); leaving the cell commits the entry: the
  // text is parsed, one undo step is kept, the model is written and the formulas are calculated again.
  const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;

  // `getRulesUi`, `getInput` and `autoLink` are asked when they are used: the controller makes those parts after this one.
  function createCellEdit({
    document, sheetDataStore, sheetHistory, isFormulaStr, grid, sheetCellFocusState, getActiveSheetName, getActiveSheetCellRef,
    trackSheetAsyncWork, drainInFlightSheetWork, snapshotDomSheetCells, recalculateActiveSheetFormulas, updateSheetSelectionAndAggregates,
    getRulesUi, getInput, autoLink,
  }) {
    function ensureSheetCellFocusEntry(td, sheetName) {
      const initialText = grid.readTdValue(td);
      const entry = {
        initialText,
        initialCt: td._ct || "",
        initialRaw: cellTypeApi.editableRaw(initialText, td.dataset.formula || "", td._ct, td._nf),
        editBaseline: null,
        ref: td.dataset.cell,
        pendingSnapshot: null,
        snapshotPushed: false,
        userEdited: false,
        sheetName,
      };
      sheetCellFocusState.set(td, entry);
      return entry;
    }

    function syncSheetCellFocusBaseline(td, clearPending) {
      const st = sheetCellFocusState.get(td);
      if (!st) return;
      const text = grid.readTdValue(td);
      st.initialText = text;
      st.initialCt = td._ct || "";
      st.initialRaw = cellTypeApi.editableRaw(text, td.dataset.formula || "", td._ct, td._nf);
      st.editBaseline = null;
      if (clearPending) {
        st.pendingSnapshot = null;
        st.snapshotPushed = false;
        st.userEdited = false;
        st.mode = null;
        st.restore = null;
        st.sheetName = getActiveSheetName();
      }
    }

    function refreshFocusedSheetCellPendingSnapshot() {
      const activeEl = document.activeElement;
      if (!activeEl) return;
      const curSheet = getActiveSheetName();
      let st = sheetCellFocusState.get(activeEl);
      if (
        !st &&
        activeEl.nodeType === 1 &&
        activeEl.dataset?.cell &&
        document.getElementById("pane-sheet")?.contains(activeEl)
      ) {
        st = ensureSheetCellFocusEntry(activeEl, curSheet);
      }
      if (st && !st.userEdited && !st.snapshotPushed) {
        st.sheetName = curSheet;
        schedulePendingSnapshot(st);
      }
    }

    // What a cell held when it took the focus is what an undo gives back. The model is read as it is: the cell's own text
    // is in the page, not yet in the model.
    function capturePendingCell(st) {
      st.pendingSnapshot = sheetHistory.begin({ sheet: st.sheetName, refs: [st.ref], sync: false });
    }

    // The cell as it was before the first edit when no capture was made at the focus: the model's entry (style, link)
    // with the text the cell showed.
    function captureCellBeforeEdit(td, st) {
      const ref = td.dataset.cell;
      const prev = sheetDataStore[st.sheetName]?.[ref];
      const formula = !st.initialCt && isFormulaStr(st.initialRaw) ? st.initialRaw : "";
      const entry = prev || st.initialText !== "" || formula !== "" ? { ...prev, value: st.initialText, formula } : undefined;
      if (entry && !st.initialCt) delete entry.t;
      return sheetHistory.begin({ sheet: st.sheetName, refs: [ref], before: { [ref]: entry }, sync: false });
    }

    function pushCellUndoSnapshotIfNeeded(td, st, effectiveRaw) {
      if (!st || st.snapshotPushed || effectiveRaw === st.initialRaw) return;
      st.undoRecord = sheetHistory.commit(st.pendingSnapshot ?? captureCellBeforeEdit(td, st), { compact: false });
      st.snapshotPushed = true;
    }

    // The step pushed at the first change of a cell goes when the entry is cancelled: nothing was entered.
    function retractCellUndoSnapshot(st) {
      if (st.undoRecord) sheetHistory.retract(st.undoRecord);
      st.undoRecord = null;
    }

    function releaseCellFocus(td, keepFocus) {
      if (keepFocus) {
        syncSheetCellFocusBaseline(td, true);
        return;
      }
      sheetCellFocusState.delete(td);
      if (document.activeElement === td && typeof td.blur === "function") td.blur();
    }

    // A formula cell left showing its source text gets its cached result back until the recalc runs.
    function restoreCachedFormulaDisplay(td, editSheet) {
      const cached = sheetDataStore[editSheet]?.[td.dataset.cell]?.value;
      if (cached === undefined || cached === "" || isFormulaStr(cached)) return;
      grid.setDisplayValue(td, cached);
      if (getActiveSheetName() === editSheet) {
        updateSheetSelectionAndAggregates(getActiveSheetCellRef() || td.dataset.cell);
      }
    }

    function showsFormulaSourceText(td) {
      return isFormulaStr(td.dataset.formula || "") && isFormulaStr((td.textContent || "").trim());
    }

    function settleUnchangedCell(td, edit) {
      const { editSheet, rawText, keepFocus, wasEditing } = edit;
      if (!keepFocus && showsFormulaSourceText(td)) {
        restoreCachedFormulaDisplay(td, editSheet);
      } else if (wasEditing && !isFormulaStr(td.dataset.formula || "")) {
        grid.setDisplayValue(td, rawText);
      }
      if (keepFocus) refreshFocusedSheetCellPendingSnapshot();
    }

    async function recalcAfterCellEdit(td, editSheet, keepFocus) {
      await recalculateActiveSheetFormulas();
      if (keepFocus && document.activeElement === td && !sheetCellFocusState.get(td)?.userEdited) {
        syncSheetCellFocusBaseline(td, true);
        refreshFocusedSheetCellPendingSnapshot();
      }
      if (getActiveSheetName() === editSheet) {
        updateSheetSelectionAndAggregates(getActiveSheetCellRef() || td.dataset.cell);
      }
      return true;
    }

    // An entry under a data validation waits for the engine's verdict; a refused entry takes the
    // old value back before the ordinary commit runs.
    function commitSheetCellEdit(td, keepFocus) {
      const st = sheetCellFocusState.get(td);
      const gate = getRulesUi()?.gateEntry(td, st) ?? null;
      if (!gate) return commitSheetCellEditNow(td, keepFocus);
      return trackSheetAsyncWork(
        gate.then((accepted) => {
          if (!accepted) {
            st.userEdited = false;
            td._ct = st.initialCt || "";
            grid.setDisplayValue(td, st.initialText);
          }
          return commitSheetCellEditNow(td, keepFocus);
        })
      );
    }

    function commitSheetCellEditNow(td, keepFocus) {
      const st = sheetCellFocusState.get(td);
      if (!st || st.sheetName !== getActiveSheetName()) {
        if (!keepFocus) sheetCellFocusState.delete(td);
        return Promise.resolve(false);
      }
      const editSheet = st.sheetName, rawDom = grid.readTdValue(td), editBaseline = st.editBaseline;
      const edited = Boolean(st.userEdited) || (editBaseline !== null ? rawDom !== editBaseline : rawDom !== st.initialText);
      const wasEditing = Boolean(st.mode) || edited;
      getInput().endEdit(td);
      let nextValue = st.initialText;
      if (edited) {
        const parsed = cellTypeApi.parseTypedInput(rawDom, td._nf);
        nextValue = parsed.value;
        td._ct = parsed.t || "";
        if (parsed.formula) td.dataset.formula = parsed.formula;
        else delete td.dataset.formula;
        pushCellUndoSnapshotIfNeeded(td, st, parsed.editable);
      }
      const shouldRecalc = st.snapshotPushed || edited;
      releaseCellFocus(td, keepFocus);
      if (!shouldRecalc) {
        settleUnchangedCell(td, { editSheet, rawText: editBaseline !== null ? st.initialText : rawDom, keepFocus, wasEditing });
        return Promise.resolve(false);
      }
      if (!isFormulaStr(td.dataset.formula || "")) grid.setDisplayValue(td, nextValue);
      sheetDataStore[editSheet] = snapshotDomSheetCells();
      if (edited) autoLink(editSheet, td.dataset.cell, true);
      return trackSheetAsyncWork(recalcAfterCellEdit(td, editSheet, keepFocus));
    }

    // `target` is the sheet the window switches to: a formula that waits for a reference goes on there instead of being entered.
    async function flushFocusedSheetCellBeforeSwitch(target) {
      const activeEl = document.activeElement;
      if (getInput().detachForSwitch(target)) return drainInFlightSheetWork();
      if (activeEl && sheetCellFocusState.has(activeEl)) {
        await commitSheetCellEdit(activeEl, false);
      } else if (
        activeEl?.nodeType === 1 &&
        activeEl.dataset?.cell &&
        typeof activeEl.blur === "function"
      ) {
        activeEl.blur();
      }
      await drainInFlightSheetWork();
    }

    async function commitPendingFocusedSheetCell(keepFocus) {
      const activeEl = document.activeElement;
      let committed = false;
      if (activeEl && sheetCellFocusState.has(activeEl)) {
        committed = await commitSheetCellEdit(activeEl, keepFocus);
      } else if (
        !keepFocus &&
        activeEl?.nodeType === 1 &&
        activeEl.dataset?.cell &&
        typeof activeEl.blur === "function"
      ) {
        activeEl.blur();
      }
      await drainInFlightSheetWork();
      return committed;
    }

    function ensurePendingSnapshot(st) {
      if (!st.pendingSnapshot && !st.userEdited && !st.snapshotPushed) capturePendingCell(st);
    }

    // The capture is one cell, so every focus makes it, on any size of sheet.
    const schedulePendingSnapshot = capturePendingCell;

    return {
      ensureSheetCellFocusEntry,
      syncSheetCellFocusBaseline,
      refreshFocusedSheetCellPendingSnapshot,
      pushCellUndoSnapshotIfNeeded,
      retractCellUndoSnapshot,
      commitSheetCellEdit,
      flushFocusedSheetCellBeforeSwitch,
      commitPendingFocusedSheetCell,
      ensurePendingSnapshot,
      schedulePendingSnapshot,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_CELL_EDIT__ = createCellEdit;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
