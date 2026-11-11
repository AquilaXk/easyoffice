(function (host) {
  "use strict";

  // Find source for the active sheet. A block is a cell, in row-major order. By default the text a
  // cell shows is searched (a formula cell shows its result, so it cannot be replaced); with
  // `includeFormulas` a formula cell is searched and replaced by its formula text. A replacement goes
  // through the same evaluate-and-commit path as a formula-bar edit, as one undo step.

  const { applyEdits } = host.__EASYOFFICE_FIND_CORE__;
  const MSG_STALE = "시트가 바뀌어 바꾸기를 취소했습니다. 다시 시도하세요.";

  function createSheetFindSource(deps) {
    const {
      sheetDataStore, getActiveSheetName, sheetController, gridUtils,
      captureWorkspaceSnapshot, commitDeferredUndoSnapshot, computeEvaluatedSheetStore, isFormulaStr,
    } = deps;
    let includeFormulas = false;

    function cellText(entry) {
      if (includeFormulas && entry.formula) return entry.formula;
      return entry.value == null ? "" : String(entry.value);
    }

    function sync(index) {
      const map = sheetController.snapshotDomSheetCells();
      const cells = [];
      for (const ref of Object.keys(map)) {
        const entry = map[ref];
        const packed = entry ? gridUtils.packRef(ref) : 0;
        if (packed > 0) cells.push({ packed, id: ref, text: cellText(entry) });
      }
      cells.sort((a, b) => a.packed - b.packed);
      index.setBlocks(cells.filter((cell) => cell.text !== ""));
    }

    // Starts from the selected cell.
    function anchor() {
      const ref = deps.getActiveCellRef?.();
      return ref ? { id: ref, offset: 0 } : null;
    }

    function show(_near, current) {
      if (current) sheetController.updateSheetSelectionAndAggregates(current.id);
    }

    // Applies the edits of one cell to the candidate entry; false when the cell cannot take them.
    function editEntry(entry, edits) {
      const onFormula = includeFormulas && Boolean(entry.formula);
      if (!onFormula && entry.formula) return false;
      const next = applyEdits([cellText(entry)], edits)[0];
      if (!onFormula && !isFormulaStr(next)) {
        entry.value = next;
      } else {
        entry.formula = next;
        entry.value = "";
      }
      return true;
    }

    async function replace(plan) {
      await sheetController.drainInFlightSheetWork();
      const sheet = getActiveSheetName();
      const epoch = sheetController.getSnapshotEpoch();
      const pre = captureWorkspaceSnapshot(false);
      sheetController.snapshotDomSheetCells();
      const candidate = gridUtils.cloneSheetStore(sheetDataStore);
      let replaced = 0;
      let skipped = 0;
      for (const { id: ref, edits } of plan) {
        const entry = candidate[sheet]?.[ref];
        if (entry && editEntry(entry, edits)) replaced += edits.length;
        else skipped += edits.length;
      }
      if (replaced === 0) return { replaced, skipped };
      const evaluated = await sheetController.trackSheetAsyncWork(computeEvaluatedSheetStore(candidate, sheet, null));
      if (epoch !== sheetController.getSnapshotEpoch()) throw new Error(MSG_STALE);
      commitDeferredUndoSnapshot(pre);
      sheetController.replaceLiveSheetDataStore(evaluated.sheets);
      sheetController.restoreDomSheetCells(sheet);
      sheetController.refreshFocusedSheetCellPendingSnapshot();
      return { replaced, skipped };
    }

    return {
      kind: "sheet", sync, watch: () => {}, anchor, show, clearHighlight: () => {}, replace, dispose: () => {},
      setIncludeFormulas: (on) => { includeFormulas = Boolean(on); },
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_FIND_SOURCE__ = createSheetFindSource;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
