(function (host) {
  "use strict";

  // Writes a batch of cell changes (paste, paste special, fill, clear, cut) as ONE undoable
  // operation. An edit is `{ ref, value, formula }` plus an optional `style`: absent keeps the
  // cell's style, null removes it, an object replaces it; an optional `link` (PD-218) is the cell's hyperlink,
  // and an edit without one leaves the cell without. `widths` ({ column: px }) are applied in
  // the same undo step.

  function createSheetCellWriter(deps) {
    const {
      document,
      grid,
      gridUtils,
      sheetDataStore,
      getActiveSheetName,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      snapshotDomSheetCells,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
      bumpExtent,
      refreshWindowFromModel,
      recalculateActiveSheetFormulas,
      refreshChrome,
    } = deps;

    // On a protected sheet a write keeps the lock state of the cell it lands on: a pasted format does not unlock it.
    function keepLocks(style, prev) {
      if (!style) return prev?.unlocked || prev?.locked || prev?.hideFormula ? { unlocked: prev.unlocked, locked: prev.locked, hideFormula: prev.hideFormula } : style;
      const next = { ...style };
      for (const key of ["unlocked", "locked", "hideFormula"]) {
        if (prev?.[key]) next[key] = prev[key];
        else delete next[key];
      }
      return Object.freeze(next);
    }

    // Whether the sheet lets every edit land (PD-216): a locked cell refuses the whole batch, as a paste does in
    // the spreadsheet programs, and a width change needs 열 서식.
    function editsAllowed(edits, sheet, map, widthCount) {
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      if (!guard || !guard.isOn(sheet)) return true;
      if (widthCount > 0 && !guard.guardAction(sheet, "formatColumns")) return false;
      for (const edit of edits) {
        const at = gridUtils.parseRef(edit.ref);
        if (at && !guard.cellEditable(sheet, at.row, at.col, map[edit.ref]?.style)) {
          guard.alertBlocked();
          return false;
        }
      }
      return true;
    }

    function styleOf(edit, prev, protectedSheet) {
      const style = "style" in edit ? edit.style : prev?.style;
      return protectedSheet && "style" in edit ? keepLocks(style, prev?.style) : style;
    }

    function writeEdit(map, edit, sheet, protectedSheet) {
      const style = styleOf(edit, map[edit.ref], protectedSheet);
      if (edit.value === "" && edit.formula === "" && !style) {
        delete map[edit.ref];
        return;
      }
      const entry = style
        ? { value: edit.value, formula: edit.formula, style }
        : { value: edit.value, formula: edit.formula };
      // A write removes the cell's hyperlink (PD-218) unless the edit brings one: a paste, a fill or the link dialog.
      if (edit.link) entry.link = edit.link;
      map[edit.ref] = entry;
      const packed = gridUtils.packRef(edit.ref);
      if (packed > 0) {
        const col = packed & 32767;
        bumpExtent(sheet, (packed - col) / 32768, col);
      }
    }

    function applyCellEdits(edits, label, widths) {
      const widthCols = widths ? Object.keys(widths) : [];
      if (edits.length === 0 && widthCols.length === 0) return Promise.resolve(false);
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        const curSheet = getActiveSheetName();
        const map = snapshotDomSheetCells();
        sheetDataStore[curSheet] = map;
        if (!editsAllowed(edits, curSheet, map, widthCols.length)) return false;
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        const protectedSheet = Boolean(host.__EASYOFFICE_SHEET_PROTECT__?.isOn(curSheet));
        for (const edit of edits) writeEdit(map, edit, curSheet, protectedSheet);
        for (const col of widthCols) grid.applyColWidth(Number(col), widths[col]);
        grid.renderWindow(false);
        refreshWindowFromModel();
        await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
        refreshChrome();
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = (label || "셀 편집") + ": " + (edits.length > 0 ? edits.length + "칸" : widthCols.length + "열");
        return true;
      });
    }

    return { applyCellEdits };
  }

  host.__EASYOFFICE_CREATE_SHEET_CELL_WRITER__ = createSheetCellWriter;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
