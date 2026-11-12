// Reference implementation (cell hit test, find-by and find-scope): Univer (Apache-2.0, Copyright 2023-present
// DreamNum Co., Ltd.), commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, `packages/sheets-find-replace/src/
// controllers/sheet-find-replace.controller.ts` 1088-1145 (`hitCell`) and `packages/find-replace/src/services/
// find-replace.service.ts` 507-518 (`FindBy`, `FindScope`). See THIRD_PARTY_NOTICES.md.
(function (host) {
  "use strict";

  // Find source for the sheets. A block is a cell, in row-major order. By default the active sheet is
  // searched and the text a cell shows is matched (a formula cell shows its result, so it cannot be
  // replaced). The options follow the reference's find-by and find-scope: `formula` matches the formula
  // text of a formula cell (and replaces it), `workbook` searches every sheet, with cell ids `sheet!ref`.
  // A replacement goes through the same evaluate-and-commit path as a formula-bar edit, as one undo step.

  const { applyEdits } = host.__EASYOFFICE_FIND_CORE__;
  const MSG_STALE = "시트가 바뀌어 바꾸기를 취소했습니다. 다시 시도하세요.";
  const SHEET_SEPARATOR = "!";

  function createSheetFindSource(deps) {
    const {
      sheetDataStore, getActiveSheetName, sheetController, gridUtils,
      captureWorkspaceSnapshot, commitDeferredUndoSnapshot, computeEvaluatedSheetStore, isFormulaStr,
    } = deps;

    // The text one entry offers to the search: its formula when searching by formula, else what it shows.
    function cellText(entry, formula) {
      if (formula && isFormulaStr(entry.formula || "")) return entry.formula;
      return entry.value == null ? "" : String(entry.value);
    }

    const cellId = (sheet, ref, workbook) => (workbook ? sheet + SHEET_SEPARATOR + ref : ref);

    function splitId(id, workbook, activeSheet) {
      if (!workbook) return { sheet: activeSheet, ref: id };
      const at = id.lastIndexOf(SHEET_SEPARATOR);
      return { sheet: id.slice(0, at), ref: id.slice(at + 1) };
    }

    function sheetCells(sheet, map, options) {
      const cells = [];
      for (const ref of Object.keys(map)) {
        const entry = map[ref];
        const packed = entry ? gridUtils.packRef(ref) : 0;
        const text = packed > 0 ? cellText(entry, options.formula) : "";
        if (text !== "") cells.push({ packed, id: cellId(sheet, ref, options.workbook), text });
      }
      return cells.sort((a, b) => a.packed - b.packed);
    }

    // `options` is `{ formula, workbook }`.
    function sync(index, options = {}) {
      const active = getActiveSheetName();
      const liveMap = sheetController.snapshotDomSheetCells();
      const sheets = options.workbook ? Object.keys(sheetDataStore) : [active];
      const blocks = [];
      for (const sheet of sheets) {
        const map = sheet === active ? liveMap : sheetDataStore[sheet] || {};
        for (const { id, text } of sheetCells(sheet, map, options)) blocks.push({ id, text });
      }
      index.setBlocks(blocks);
    }

    // Starts from the selected cell.
    function anchor(options = {}) {
      const ref = deps.getActiveCellRef?.();
      return ref ? { id: cellId(getActiveSheetName(), ref, options.workbook), offset: 0 } : null;
    }

    // Shows the match's cell; a match on another sheet switches to that sheet first.
    function show(_near, current, options = {}) {
      if (!current) return;
      const { sheet, ref } = splitId(current.id, options.workbook, getActiveSheetName());
      const switched = sheet === getActiveSheetName() ? Promise.resolve() : sheetController.switchSheetTab(sheet);
      void switched.then(() => sheetController.updateSheetSelectionAndAggregates(ref)).catch(() => {});
    }

    // Applies the edits of one cell to the candidate entry. A formula cell is edited only when searching
    // by formula (otherwise its text is a result); false when the cell cannot take the edit.
    function editEntry(entry, edits, formula) {
      const isFormulaCell = isFormulaStr(entry.formula || "");
      if (isFormulaCell && !formula) return false;
      const next = applyEdits([cellText(entry, formula)], edits)[0];
      if (isFormulaStr(next)) {
        entry.formula = next;
        entry.value = "";
      } else {
        entry.formula = "";
        entry.value = next;
      }
      return true;
    }

    async function replace(plan, options = {}) {
      await sheetController.drainInFlightSheetWork();
      const active = getActiveSheetName();
      const epoch = sheetController.getSnapshotEpoch();
      const pre = captureWorkspaceSnapshot(false);
      sheetController.snapshotDomSheetCells();
      const candidate = gridUtils.cloneSheetStore(sheetDataStore);
      let replaced = 0;
      let skipped = 0;
      for (const { id, edits } of plan) {
        const { sheet, ref } = splitId(id, options.workbook, active);
        const entry = candidate[sheet]?.[ref];
        if (entry && editEntry(entry, edits, options.formula)) replaced += edits.length;
        else skipped += edits.length;
      }
      if (replaced === 0) return { replaced, skipped };
      const evaluated = await sheetController.trackSheetAsyncWork(computeEvaluatedSheetStore(candidate, active, null));
      if (epoch !== sheetController.getSnapshotEpoch()) throw new Error(MSG_STALE);
      commitDeferredUndoSnapshot(pre);
      sheetController.replaceLiveSheetDataStore(evaluated.sheets);
      sheetController.restoreDomSheetCells(active);
      sheetController.refreshFocusedSheetCellPendingSnapshot();
      return { replaced, skipped };
    }

    return {
      kind: "sheet", sync, watch: () => {}, anchor, show, clearHighlight: () => {}, replace, dispose: () => {},
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_FIND_SOURCE__ = createSheetFindSource;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
