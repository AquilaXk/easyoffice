(function (host) {
  "use strict";

  // The undo and redo stacks of the workspace and the step that moves a record from one to the other. A record is a whole
  // workspace snapshot (`captureWorkspaceSnapshot`, any module) or a sheet change (`sheet-history.js`, only what the
  // change replaced). The stacks are `command-history.js` (the rhwp-studio `CommandHistory` rules); this module is the
  // adapter that weighs a record and applies it. Both stacks are bounded in steps and in bytes (PD-221): the oldest
  // step leaves first; a single step that alone is over the byte bound is not kept, and the record of steps ends there.
  // The step bound is the snapshot capacity of the reference editor core (`MAX_SNAPSHOTS`, 100).
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

    const stacks = host.__EASYOFFICE_CREATE_COMMAND_HISTORY__({
      maxSteps: MAX_HISTORY_STEPS,
      maxWeight: MAX_HISTORY_BYTES,
      weightOf: snapshotBytes,
    });

    function pushRecord(record) {
      if (!record) return;
      bumpEpoch();
      stacks.record(record);
    }

    // A step that was recorded for a change that did not happen (a cell entry cancelled with Esc).
    function retractRecord(record) {
      if (record && stacks.retract(record)) bumpEpoch();
    }

    // Protecting or lifting a protection cannot be undone: the record of steps ends there (PD-216).
    function clear() {
      bumpEpoch();
      stacks.clear();
    }
    host.__EASYOFFICE_WORKSPACE_HISTORY__ = { clear, sizes: stacks.sizes };

    // The step of a sheet change: the cells and settings of the record go back, the record that leads the other way
    // goes on the opposite stack, and what the cells feed (formulas, rules, spills) is calculated again.
    async function stepSheetRecord(record, isUndo, commit) {
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
      commit(inverse);
      sheetHistory.restoreRowCounts(record);
      for (const [name, change] of notes.extent) controller.noteCellsRestored(name, change);
      await restoreWorkspaceSnapshot(sheetHistory.snapshotOf(record, notes), backendSyncRes, { keepProtections: true });
      // A merge that comes or goes changes where an array may be placed (PD-222): the calculation after the step places it again.
      const cellsChanged = record.sheets.some((entry) => entry.delta.n > 0 || entry.absent);
      if (cellsChanged || record.parts.sheetLayout !== undefined) await controller.recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
      return true;
    }

    // The step of a whole snapshot.
    async function stepSnapshot(target, isUndo, commit) {
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
      current.bytes = snapshotBytes(current);
      commit(current);
      await restoreWorkspaceSnapshot(evaluatedSheets ? { ...target, sheets: evaluatedSheets } : target, backendSyncRes, { keepProtections: true });
      return true;
    }

    function performStep(isUndo) {
      return enqueueWorkspaceHistoryOp(async () => {
        await getSheetController().drainInFlightSheetWork();
        if (!(isUndo ? stacks.canUndo() : stacks.canRedo())) {
          say(isUndo ? "실행 취소할 작업이 없습니다" : "다시 실행할 작업이 없습니다");
          return;
        }
        sheetDataStore[getActiveSheetName()] = getSheetController().snapshotDomSheetCells();
        const apply = (target, commit) => (target.kind === "delta" ? stepSheetRecord(target, isUndo, commit) : stepSnapshot(target, isUndo, commit));
        const done = await (isUndo ? stacks.undo(apply) : stacks.redo(apply));
        if (done) say(isUndo ? "실행 취소" : "다시 실행");
      });
    }

    return {
      pushRecord,
      retractRecord,
      clear,
      getEpoch,
      bumpEpoch,
      undo: () => performStep(true),
      redo: () => performStep(false),
      sizes: stacks.sizes,
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_HISTORY__ = createWorkspaceHistory;
  host.__EASYOFFICE_HISTORY_LIMITS__ = { MAX_HISTORY_STEPS, MAX_HISTORY_BYTES };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
