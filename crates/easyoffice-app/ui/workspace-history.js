(function (host) {
  "use strict";

  // The undo and redo stacks of the workspace and the step that moves a record from one to the other. A record is a whole
  // workspace snapshot (`captureWorkspaceSnapshot`, any module) or a sheet change (`sheet-history.js`, only what the
  // change replaced). Both stacks are bounded in steps and in bytes (PD-221): the oldest step leaves first; a single
  // step that alone is over the byte bound is not kept, and the record of steps ends there.
  const MAX_HISTORY_STEPS = 100;
  const MAX_HISTORY_BYTES = 192 * 1024 * 1024;
  // What a snapshot costs besides its sheets: the documents, slides and settings it copies.
  const SNAPSHOT_BASE_BYTES = 16 * 1024;

  function createWorkspaceHistory(deps) {
    const {
      document,
      invokeIpc,
      sheetDataStore,
      sheetStoreHelpers,
      ribbonIpcPayload,
      sheetHistory,
      getSheetController,
      getActiveSheetName,
      captureWorkspaceSnapshot,
      restoreWorkspaceSnapshot,
      enqueueWorkspaceHistoryOp,
    } = deps;
    const { sheetMapHasAnyFormula, sheetCellDataChanged, computeEvaluatedSheetStore } = sheetStoreHelpers;
    const D = host.__EASYOFFICE_SHEET_DELTA__;

    const undoStack = [];
    const redoStack = [];
    let mutationEpoch = 0;

    const bumpEpoch = () => {
      mutationEpoch += 1;
    };
    const getEpoch = () => mutationEpoch;
    const say = (text) => {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    };

    function snapshotBytes(snap) {
      let entries = 0;
      for (const map of Object.values(snap.sheets || {})) entries += Object.keys(map || {}).length;
      return SNAPSHOT_BASE_BYTES + D.bytesOfEntries(entries);
    }

    const totalBytes = () => undoStack.concat(redoStack).reduce((sum, rec) => sum + rec.bytes, 0);

    function enforceLimits() {
      while (undoStack.length > MAX_HISTORY_STEPS) undoStack.shift();
      while (undoStack.length + redoStack.length > 0 && totalBytes() > MAX_HISTORY_BYTES) {
        if (undoStack.length > 0) undoStack.shift();
        else redoStack.shift();
      }
    }

    function pushRecord(record) {
      if (!record) return;
      bumpEpoch();
      record.bytes ??= snapshotBytes(record);
      redoStack.length = 0;
      if (record.bytes > MAX_HISTORY_BYTES) {
        undoStack.length = 0;
        say("실행 취소 기록 한도 초과: 기록을 비움");
        return;
      }
      undoStack.push(record);
      enforceLimits();
    }

    // Protecting or lifting a protection cannot be undone: the record of steps ends there (PD-216).
    function clear() {
      bumpEpoch();
      undoStack.length = 0;
      redoStack.length = 0;
    }
    host.__EASYOFFICE_WORKSPACE_HISTORY__ = { clear, sizes: () => ({ undo: undoStack.length, redo: redoStack.length, bytes: totalBytes() }) };

    // The step of a sheet change: the cells and settings of the record go back, the record that leads the other way
    // goes on the opposite stack, and what the cells feed (formulas, rules, spills) is calculated again.
    async function stepSheetRecord(record, isUndo, source, dest) {
      const controller = getSheetController();
      if (sheetHistory.wouldChange(record)) {
        host.__EASYOFFICE_SHEET_PROTECT__.alertBlocked();
        return false;
      }
      bumpEpoch();
      const backendSyncRes = record.isRibbon
        ? await invokeIpc(isUndo ? "undo_ribbon_command" : "redo_ribbon_command", ribbonIpcPayload({ module: record.ribbonModule || record.view.activeModule }))
        : null;
      controller.bumpSnapshotEpoch();
      const { inverse, notes } = sheetHistory.swap(record);
      source.pop();
      dest.push(inverse);
      enforceLimits();
      sheetHistory.restoreRowCounts(record);
      for (const [name, change] of notes.extent) controller.noteCellsRestored(name, change);
      await restoreWorkspaceSnapshot(sheetHistory.snapshotOf(record, notes), backendSyncRes, { keepProtections: true });
      if (record.sheets.some((entry) => entry.delta.n > 0 || entry.absent)) await controller.recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
      return true;
    }

    // The step of a whole snapshot.
    async function stepSnapshot(target, isUndo, source, dest) {
      const controller = getSheetController();
      if (host.__EASYOFFICE_SHEET_PROTECT__.restoreWouldChange(target, sheetDataStore)) {
        host.__EASYOFFICE_SHEET_PROTECT__.alertBlocked();
        return false;
      }
      bumpEpoch();
      const current = captureWorkspaceSnapshot(Boolean(target.isRibbon), target.layoutOnly === true);
      current.ribbonModule = target.ribbonModule;
      let evaluatedSheets = null;
      if (sheetCellDataChanged(current.sheets, target.sheets, false) && sheetMapHasAnyFormula(target.sheets)) {
        const evalRes = await controller.trackSheetAsyncWork(
          computeEvaluatedSheetStore(target.sheets, target.activeSheetName || getActiveSheetName(), null)
        );
        evaluatedSheets = evalRes.sheets;
      }
      let backendSyncRes = null;
      if (target.isRibbon === true) {
        const ribbonModule = target.ribbonModule || target.activeModule;
        backendSyncRes = await invokeIpc(isUndo ? "undo_ribbon_command" : "redo_ribbon_command", ribbonIpcPayload({ module: ribbonModule }));
      }
      source.pop();
      current.bytes = snapshotBytes(current);
      dest.push(current);
      enforceLimits();
      await restoreWorkspaceSnapshot(evaluatedSheets ? { ...target, sheets: evaluatedSheets } : target, backendSyncRes, { keepProtections: true });
      return true;
    }

    function performStep(isUndo) {
      return enqueueWorkspaceHistoryOp(async () => {
        await getSheetController().drainInFlightSheetWork();
        const source = isUndo ? undoStack : redoStack;
        const dest = isUndo ? redoStack : undoStack;
        if (source.length === 0) {
          say(isUndo ? "실행 취소할 작업이 없습니다" : "다시 실행할 작업이 없습니다");
          return;
        }
        sheetDataStore[getActiveSheetName()] = getSheetController().snapshotDomSheetCells();
        const target = source.at(-1);
        const done = target.kind === "delta" ? await stepSheetRecord(target, isUndo, source, dest) : await stepSnapshot(target, isUndo, source, dest);
        if (done) say(isUndo ? "실행 취소" : "다시 실행");
      });
    }

    return {
      pushRecord,
      clear,
      getEpoch,
      bumpEpoch,
      undo: () => performStep(true),
      redo: () => performStep(false),
      sizes: () => ({ undo: undoStack.length, redo: redoStack.length, bytes: totalBytes() }),
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_HISTORY__ = createWorkspaceHistory;
  host.__EASYOFFICE_HISTORY_LIMITS__ = { MAX_HISTORY_STEPS, MAX_HISTORY_BYTES };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
