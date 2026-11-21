(function (host) {
  "use strict";

  // The entry of the formula bar: its text is parsed into the active cell, the sheets are calculated by the engine with that cell
  // changed (`prepareFormulaBarEvaluation`), and the answer replaces the live model in one undo step (`commitPreparedFormulaBarEvaluation`).
  const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;

  function inferAutoSumFormulaForCell(cellRef, sheetMap, isFormulaStr) {
    const m = /^([A-Z]+)(\d+)$/.exec(String(cellRef || "A1").toUpperCase());
    const col = m ? m[1] : "B";
    const row = m ? Number(m[2]) : 7;
    if (row >= 3) {
      let lastPlainRow = row - 1;
      for (let r = row - 1; r >= 2; r -= 1) {
        const entry = sheetMap?.[col + r];
        if (entry && !isFormulaStr(entry.formula || "") && entry.value !== "") {
          lastPlainRow = r;
          break;
        }
      }
      return "=SUM(" + col + "2:" + col + Math.max(2, lastPlainRow) + ")";
    }
    return "=SUM(B" + row + ":E" + row + ")";
  }

  // `autoLink` is asked when it is used: the controller makes the structure part after this one.
  function createFormulaBar({
    document, sheetDataStore, sheetHistory, gridUtils, isFormulaStr, computeEvaluatedSheetStore, getActiveSheetName, getActiveSheetCellRef,
    snapshotDomSheetCells, trackSheetAsyncWork, getSnapshotEpoch, replaceLiveSheetDataStore, restoreDomSheetCells,
    refreshFocusedSheetCellPendingSnapshot, announceCircularReference, updateSheetSelectionAndAggregates, drainInFlightSheetWork,
    enqueueWorkspaceHistoryOp, autoLink,
  }) {
    // The formula bar writes its text into the active cell, so a locked cell of a protected sheet refuses it
    // (PD-216) and the bar shows the cell again.
    function formulaBarBlocked() {
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      const sheet = getActiveSheetName();
      const at = gridUtils.parseRef(getActiveSheetCellRef() || "A1");
      if (!guard || !at || guard.cellEditable(sheet, at.row, at.col, sheetDataStore[sheet]?.[getActiveSheetCellRef()]?.style)) return false;
      guard.alertBlocked();
      updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "A1");
      return true;
    }

    async function prepareFormulaBarEvaluation(options = {}) {
      if (formulaBarBlocked()) return null;
      const input = document.getElementById("formula-input");
      const out = document.getElementById("sheet-eval-result");
      if (!input || !out) return null;
      const epoch = getSnapshotEpoch();
      const targetSheet = getActiveSheetName();
      const targetCellRef = getActiveSheetCellRef();
      sheetDataStore[targetSheet] = snapshotDomSheetCells();
      const candidateSheets = gridUtils.cloneSheetStore(sheetDataStore);
      if (!candidateSheets[targetSheet]) candidateSheets[targetSheet] = {};
      const prevCell = candidateSheets[targetSheet][targetCellRef];
      const prevRaw = cellTypeApi.editableRaw(prevCell?.value || "", prevCell?.formula || "", prevCell?.t, prevCell?.style?.numFmt).trim();
      let rawFormula = input.value.trim();
      if (options.autoSum && rawFormula === prevRaw && !isFormulaStr(rawFormula) && !rawFormula.startsWith("'")) {
        rawFormula = inferAutoSumFormulaForCell(targetCellRef, candidateSheets[targetSheet], isFormulaStr);
        input.value = rawFormula;
      }
      const parsed = cellTypeApi.parseTypedInput(rawFormula, prevCell?.style?.numFmt);
      const nextCell = { value: parsed.value, formula: parsed.formula };
      if (parsed.t) nextCell.t = parsed.t;
      if (prevCell?.style) nextCell.style = prevCell.style;
      if (prevCell?.link) nextCell.link = prevCell.link;
      candidateSheets[targetSheet][targetCellRef] = nextCell;
      const evaluated = await trackSheetAsyncWork(
        computeEvaluatedSheetStore(candidateSheets, targetSheet, { sheet: targetSheet, cell: targetCellRef })
      );
      const textChanged = parsed.editable.trim() !== prevRaw;
      return { epoch, targetSheet, targetCellRef, evaluated, textChanged };
    }

    function commitPreparedFormulaBarEvaluation(prep, change = null) {
      if (prep?.epoch !== getSnapshotEpoch()) return false;
      const out = document.getElementById("sheet-eval-result");
      if (change) sheetHistory.commit(change, { compact: false });
      replaceLiveSheetDataStore(prep.evaluated.sheets);
      restoreDomSheetCells(getActiveSheetName());
      refreshFocusedSheetCellPendingSnapshot();
      announceCircularReference(prep.evaluated.circular);
      if (out && getActiveSheetName() === prep.targetSheet) {
        out.textContent = "결과 (" + prep.targetCellRef + "): " + prep.evaluated.extraValue;
        updateSheetSelectionAndAggregates(getActiveSheetCellRef() || prep.targetCellRef);
      }
      return true;
    }

    function evaluateActiveSheetFormulaBar(recordUndo) {
      const drainThenRun = async () => {
        await drainInFlightSheetWork();
        const change = recordUndo ? sheetHistory.begin({ refs: [getActiveSheetCellRef()] }) : null;
        const prep = await prepareFormulaBarEvaluation();
        const committed = commitPreparedFormulaBarEvaluation(prep, change);
        if (recordUndo && committed && prep.textChanged) autoLink(prep.targetSheet, prep.targetCellRef, false);
      };
      if (recordUndo) {
        return enqueueWorkspaceHistoryOp(drainThenRun);
      }
      return drainThenRun();
    }

    return { prepareFormulaBarEvaluation, commitPreparedFormulaBarEvaluation, evaluateActiveSheetFormulaBar };
  }

  host.__EASYOFFICE_CREATE_SHEET_FORMULA_BAR__ = createFormulaBar;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
