(function (host) {
  "use strict";

  // Writes a batch of cell changes (paste, paste special, fill, clear, cut) as ONE undoable
  // operation. An edit is `{ ref, value, formula }` plus an optional `style`: absent keeps the
  // cell's style, null removes it, an object replaces it. `widths` ({ column: px }) are applied in
  // the same undo step. The frozen-panes flag lives on the A1 style and survives a style write.
  const EMPTY_STYLE = { fontWeight: "", fontStyle: "", textDecoration: "", fontSize: "" };

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

    function styleOf(edit, prev) {
      let style = "style" in edit ? edit.style : prev?.style;
      if (edit.ref === "A1" && prev?.style?.frozenPanes && !style?.frozenPanes) {
        style = { ...EMPTY_STYLE, ...style, frozenPanes: true };
      }
      return style;
    }

    function writeEdit(map, edit, sheet) {
      const style = styleOf(edit, map[edit.ref]);
      if (edit.value === "" && edit.formula === "" && !style) {
        delete map[edit.ref];
        return;
      }
      map[edit.ref] = style
        ? { value: edit.value, formula: edit.formula, style }
        : { value: edit.value, formula: edit.formula };
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
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        for (const edit of edits) writeEdit(map, edit, curSheet);
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
