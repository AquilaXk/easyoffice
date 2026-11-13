(function (host) {
  "use strict";

  const DEFAULT_SHEET_ROW_COUNT = 12;
  // Cells hold their pre-edit snapshot eagerly up to this many entries; beyond it the snapshot
  // is captured when the cell has been idle (or at the first edit) to keep navigation instant.
  const EAGER_SNAPSHOT_MAX_ENTRIES = 4000;
  const IDLE_SNAPSHOT_DELAY_MS = 250;

  function hasCellContent(entry) {
    return Boolean(entry && (entry.value || entry.formula));
  }

  function createSheetController(deps) {
    const {
      document,
      sheetDataStore,
      sheetRowCounts,
      sheetStoreHelpers,
      getActiveModule,
      getActiveSheetName,
      setActiveSheetName,
      getActiveSheetCellRef,
      setActiveSheetCellRef,
      enqueueWorkspaceHistoryOp,
      captureWorkspaceSnapshot,
      pushRawUndoSnapshot,
      restoreWorkspaceSnapshot,
      commitDeferredUndoSnapshot,
      onSheetSwitchedOutline,
    } = deps;

    const {
      isFormulaStr,
      extractEvaluatedTargetValue,
      computeEvaluatedSheetStore,
      evaluateLiveFormulaTargets,
      followCalculation,
    } = sheetStoreHelpers;

    const sheetCellFocusState = new WeakMap();
    let latestSheetRecalcSeq = 0;
    let sheetSnapshotEpoch = 0;
    let inFlightSheetWorkPromise = Promise.resolve();

    function bumpSnapshotEpoch() {
      sheetSnapshotEpoch += 1;
      return sheetSnapshotEpoch;
    }

    function getSnapshotEpoch() {
      return sheetSnapshotEpoch;
    }

    function trackSheetAsyncWork(promise) {
      const current = Promise.resolve(promise);
      inFlightSheetWorkPromise = Promise.allSettled([inFlightSheetWorkPromise, current]).then(
        () => {}
      );
      return promise;
    }

    async function drainInFlightSheetWork() {
      let prev;
      do {
        prev = inFlightSheetWorkPromise;
        await prev;
      } while (inFlightSheetWorkPromise !== prev);
    }

    const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
    const lookApi = host.__EASYOFFICE_SHEET_LOOK__;

    function inferMaxRowFromSheetMap(sheetObj) {
      let maxRow = DEFAULT_SHEET_ROW_COUNT;
      for (const key of Object.keys(sheetObj || {})) {
        const packed = gridUtils.packRef(key);
        if (packed > 0) {
          const col = packed & 32767;
          const rowNum = (packed - col) / 32768;
          if (rowNum > maxRow) maxRow = rowNum;
        }
      }
      return maxRow;
    }

    // ---- used extent (rows/cols that hold data) ---------------------------------------------
    let extentCache = {};

    function scanExtent(sheetName) {
      const map = sheetDataStore[sheetName] || {};
      let dataRows = 0;
      let dataCols = 0;
      let maxRow = DEFAULT_SHEET_ROW_COUNT;
      for (const ref of Object.keys(map)) {
        const packed = gridUtils.packRef(ref);
        if (packed > 0) {
          const col = packed & 32767;
          const row = (packed - col) / 32768;
          if (row > maxRow) maxRow = row;
          if (hasCellContent(map[ref])) {
            if (row > dataRows) dataRows = row;
            if (col > dataCols) dataCols = col;
          }
        }
      }
      const stored = sheetRowCounts[sheetName];
      if (!(typeof stored === "number" && Number.isSafeInteger(stored) && stored >= 1)) {
        sheetRowCounts[sheetName] = maxRow;
      }
      return { dataRows, dataCols };
    }

    function getSheetRowCount(sheetName) {
      const stored = sheetRowCounts[sheetName];
      if (typeof stored === "number" && Number.isSafeInteger(stored) && stored >= 1) {
        return stored;
      }
      if (!extentCache[sheetName]) {
        extentCache[sheetName] = scanExtent(sheetName);
      }
      return sheetRowCounts[sheetName];
    }

    function getSheetExtent(sheetName) {
      if (!extentCache[sheetName]) extentCache[sheetName] = scanExtent(sheetName);
      const e = extentCache[sheetName];
      return {
        rows: Math.max(getSheetRowCount(sheetName), e.dataRows),
        cols: Math.max(8, e.dataCols),
        dataRows: e.dataRows,
        dataCols: e.dataCols,
      };
    }

    function bumpExtent(sheetName, row, col) {
      const e = extentCache[sheetName];
      if (e) {
        e.dataRows = Math.max(e.dataRows, row);
        e.dataCols = Math.max(e.dataCols, col);
      }
      if (row > getSheetRowCount(sheetName)) sheetRowCounts[sheetName] = row;
    }

    function activeSheetMap() {
      return sheetDataStore[getActiveSheetName()] || {};
    }

    function entryAt(ref) {
      return activeSheetMap()[ref];
    }

    function isGridFocused() {
      const ae = document.activeElement;
      return Boolean(ae?.tagName === "TD" && ae.dataset?.cell);
    }

    let refocusAfterRender = false;
    let rulesUi = null;
    let lookOps = null;
    // `input` is created right below; the grid only calls these hooks after both exist.
    const grid = host.__EASYOFFICE_CREATE_SHEET_GRID__({
      document,
      getSheetName: () => getActiveSheetName(),
      getEntry: entryAt,
      getMap: () => sheetDataStore[getActiveSheetName()],
      getExtent: () => getSheetExtent(getActiveSheetName()),
      getCursor: () => {
        const s = input.getSelection();
        return { row: s.cr, col: s.cc };
      },
      hooks: {
        bindTd: (td) => input.bindTd(td),
        beforeStructuralChange: () => {
          refocusAfterRender = isGridFocused();
          input.commitIfEditing();
        },
        afterRender: () => {
          input.paintSelection();
          if (refocusAfterRender) {
            refocusAfterRender = false;
            input.focusAnchor();
          }
        },
      },
    });

    function appendSheetRow() {
      const curSheet = getActiveSheetName();
      if (host.__EASYOFFICE_SHEET_PROTECT__?.guardAction(curSheet, "insertRows") === false) return;
      const map = snapshotDomSheetCells();
      const nextCount = getSheetRowCount(curSheet) + 1;
      sheetRowCounts[curSheet] = nextCount;
      sheetDataStore[curSheet] = map;
      grid.renderWindow(true);
      grid.refreshCells();
    }

    function syncTdIntoMap(map, td) {
      const ref = td.dataset.cell;
      const textVal = grid.readTdValue(td);
      const formula = td.dataset.formula || "";
      const prevEntry = map[ref];
      const spill = host.__EASYOFFICE_SHEET_SPILL__;
      // A cell an array filled that the user did not change stays a result (PD-217).
      if (!formula && spill?.stillResult(prevEntry, textVal)) return;
      const keepComputedValue =
        isFormulaStr(formula) &&
        textVal === formula &&
        Boolean(prevEntry) &&
        prevEntry.formula === formula &&
        prevEntry.value !== "" &&
        !isFormulaStr(prevEntry.value);
      const value = keepComputedValue ? prevEntry.value : textVal;
      const fontWeight = td.style.fontWeight || "";
      const fontStyle = td.style.fontStyle || "";
      const textDecoration = td.style.textDecoration || "";
      const fontSize = td.style.fontSize || "";
      const numFmt = td._nf || "";
      const look = td._st;
      const hasStyle = Boolean(fontWeight || fontStyle || textDecoration || fontSize || numFmt || lookApi?.hasLook(look));
      if (value === "" && formula === "" && !hasStyle) {
        delete map[ref];
        return;
      }
      const cellEntry = { value, formula };
      if (hasStyle) {
        cellEntry.style = { fontWeight, fontStyle, textDecoration, fontSize };
        if (numFmt) cellEntry.style.numFmt = numFmt;
        for (const key of lookApi?.hasLook(look) ? lookApi.LOOK_KEYS : []) if (look[key]) cellEntry.style[key] = look[key];
      }
      if (spill?.isAnchor(prevEntry) && prevEntry.formula === formula) {
        cellEntry.spillRows = prevEntry.spillRows;
        cellEntry.spillCols = prevEntry.spillCols;
      }
      // A hyperlink (PD-218) stays while the cell's text is the one it was made for; a write removes it.
      if (prevEntry?.link && (prevEntry.formula || "") === formula && (formula !== "" || prevEntry.value === value)) {
        cellEntry.link = prevEntry.link;
      }
      map[ref] = cellEntry;
      if (value !== "" || formula !== "") {
        const packed = gridUtils.packRef(ref);
        if (packed > 0) {
          const col = packed & 32767;
          const row = (packed - col) / 32768;
          bumpExtent(getActiveSheetName(), row, col);
        }
      }
    }

    // Merges the materialized window into the sheet model (cells outside the window are
    // already authoritative in the model) and returns the model map.
    function snapshotDomSheetCells() {
      const curSheet = getActiveSheetName();
      const map = sheetDataStore[curSheet] || {};
      document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => syncTdIntoMap(map, td));
      return map;
    }

    function saveActiveSheetCellsToWorkbook() {
      const curSheet = getActiveSheetName();
      sheetDataStore[curSheet] = snapshotDomSheetCells();
      if (!sheetRowCounts[curSheet]) {
        const domRows = document.querySelectorAll("#pane-sheet tbody tr").length;
        sheetRowCounts[curSheet] = domRows > 0 ? domRows : DEFAULT_SHEET_ROW_COUNT;
      }
    }

    function cellRefOfRange(range, cb) {
      for (const ref of Object.keys(activeSheetMap())) {
        const packed = gridUtils.packRef(ref);
        if (packed > 0) {
          const col = packed & 32767;
          const row = (packed - col) / 32768;
          if (row >= range.r1 && row <= range.r2 && col >= range.c1 && col <= range.c2) {
            cb(ref);
          }
        }
      }
    }

    function clearExisting(range) {
      snapshotDomSheetCells();
      const edits = [];
      cellRefOfRange(range, (ref) => edits.push({ ref, value: "", formula: "" }));
      return edits;
    }

    // Batches of cell writes (paste, fill, clear, cut) are ONE undoable operation each.
    const { applyCellEdits } = host.__EASYOFFICE_CREATE_SHEET_CELL_WRITER__({
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
      refreshChrome: () => input.refreshChrome(),
    });

    function refreshWindowFromModel() {
      grid.refreshCells().forEach((td) => {
        if (sheetCellFocusState.has(td)) syncSheetCellFocusBaseline(td, true);
        else if (document.activeElement === td) ensureSheetCellFocusEntry(td, getActiveSheetName());
      });
    }

    function ensureSheetCellFocusEntry(td, sheetName) {
      const initialText = grid.readTdValue(td);
      const entry = {
        initialText,
        initialRaw: td.dataset.formula || initialText,
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
      st.initialRaw = td.dataset.formula || text;
      if (clearPending) {
        st.pendingSnapshot = null;
        st.snapshotPushed = false;
        st.userEdited = false;
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

    function restoreDomSheetCells(sheetName) {
      // Seed the default sheet only for an empty workbook (an opened file defines its own sheets).
      if (Object.keys(sheetDataStore).length === 0) {
        sheetDataStore.Sheet1 = snapshotDomSheetCells();
      }
      extentCache[sheetName] = scanExtent(sheetName);
      grid.renderWindow(true);
      refreshWindowFromModel();
    }

    function replaceLiveSheetDataStore(nextSheets, nextRowCounts) {
      extentCache = {};
      Object.keys(sheetDataStore).forEach((k) => delete sheetDataStore[k]);
      Object.assign(sheetDataStore, nextSheets);
      if (nextRowCounts && typeof nextRowCounts === "object") {
        Object.keys(sheetRowCounts).forEach((k) => delete sheetRowCounts[k]);
        Object.assign(sheetRowCounts, nextRowCounts);
      } else {
        Object.keys(sheetRowCounts).forEach((k) => {
          if (!Object.hasOwn(sheetDataStore, k)) {
            delete sheetRowCounts[k];
          }
        });
        Object.keys(sheetDataStore).forEach((k) => {
          if (!sheetRowCounts[k]) {
            extentCache[k] = scanExtent(k);
          }
        });
      }
    }

    // selection + aggregates now live in sheet-input.js

    // Circular references evaluate to the standard #NUM! literal; the engine reports them via a
    // separate `circular` flag so the status bar can warn the way spreadsheet apps do.
    function announceCircularReference(isCircular) {
      if (isCircular !== true) return;
      const feedback = document.getElementById("status-command-feedback");
      if (feedback) {
        feedback.textContent = "순환 참조 경고: 해당 수식은 #NUM!으로 표시됩니다.";
      }
    }

    function recalculateActiveSheetFormulas(options = {}) {
      const forceFocusedDisplay = Boolean(options.updateFocusedDisplay);
      const run = async () => {
        const epoch = sheetSnapshotEpoch;
        const seq = latestSheetRecalcSeq + 1;
        latestSheetRecalcSeq = seq;
        const curSheet = getActiveSheetName();
        // Large workbooks: let the keystroke finish and paint before the payload is built.
        if (isHeavySheet() && typeof host.setTimeout === "function") {
          await new Promise((resolve) => host.setTimeout(resolve, 0));
        }
        sheetDataStore[curSheet] = snapshotDomSheetCells();
        const liveEval = await evaluateLiveFormulaTargets(sheetDataStore, curSheet);
        if (!liveEval || epoch !== sheetSnapshotEpoch || seq !== latestSheetRecalcSeq) {
          return;
        }
        const { rpcRes, formulaTargets, targetValues, batchValues } = liveEval;
        let updatedActiveSheet = false;
        // The cells an array fills changed: the window shows them again (PD-217).
        if (liveEval.spillsChanged && getActiveModule() === "SHEET") {
          refreshWindowFromModel();
          updatedActiveSheet = true;
        }
        formulaTargets.forEach((target, idx) => {
          const rawVal = extractEvaluatedTargetValue(
            rpcRes,
            targetValues,
            batchValues,
            target,
            idx,
            formulaTargets.length
          );
          if (rawVal === undefined || rawVal === null) return;
          const value = String(rawVal);
          const storedEntry = sheetDataStore[target.sheet]?.[target.cell];
          if (storedEntry && isFormulaStr(storedEntry.formula || "")) {
            storedEntry.value = value;
          }
          if (getActiveSheetName() === target.sheet) {
            updatedActiveSheet = true;
            const td = grid.getTdByRef(target.cell);
            const focusSt = td ? sheetCellFocusState.get(td) : null;
            if (!td || !isFormulaStr(td.dataset.formula || "")) return;
            if (!forceFocusedDisplay && (focusSt?.userEdited || focusSt?.snapshotPushed)) return;
            const isEditingFormulaInPlace =
              !forceFocusedDisplay &&
              document.activeElement === td &&
              isFormulaStr((td.textContent || "").trim());
            if (!isEditingFormulaInPlace) {
              grid.setDisplayValue(td, value);
              syncSheetCellFocusBaseline(td, forceFocusedDisplay);
            }
          }
        });
        if (updatedActiveSheet && getActiveModule() === "SHEET") {
          updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "A1");
        }
        announceCircularReference(rpcRes?.circular);
        // Unreached cells keep their shown value; ask again until the engine has them all.
        if (
          followCalculation(rpcRes, document.getElementById("status-command-feedback")) &&
          typeof host.setTimeout === "function"
        ) {
          host.setTimeout(() => void recalculateActiveSheetFormulas(), 0);
        }
        refreshFocusedSheetCellPendingSnapshot();
      };
      return trackSheetAsyncWork(run());
    }

    function inferAutoSumFormulaForCell(cellRef, sheetMap) {
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

    async function prepareFormulaBarEvaluation(options = {}) {
      if (formulaBarBlocked()) return null;
      const input = document.getElementById("formula-input");
      const out = document.getElementById("sheet-eval-result");
      if (!input || !out) return null;
      const epoch = sheetSnapshotEpoch;
      const targetSheet = getActiveSheetName();
      const targetCellRef = getActiveSheetCellRef();
      sheetDataStore[targetSheet] = snapshotDomSheetCells();
      const candidateSheets = gridUtils.cloneSheetStore(sheetDataStore);
      if (!candidateSheets[targetSheet]) {
        candidateSheets[targetSheet] = {};
      }
      const prevCell = candidateSheets[targetSheet][targetCellRef];
      let rawFormula = input.value.trim();
      if (options.autoSum) {
        const cellBaseline = (prevCell?.formula || prevCell?.value || "").trim();
        if (rawFormula === cellBaseline && !isFormulaStr(rawFormula)) {
          rawFormula = inferAutoSumFormulaForCell(targetCellRef, candidateSheets[targetSheet]);
          input.value = rawFormula;
        }
      }
      const isFormula = isFormulaStr(rawFormula);
      const nextCell = {
        value: isFormula ? "" : rawFormula,
        formula: isFormula ? rawFormula : "",
      };
      if (prevCell?.style) {
        nextCell.style = prevCell.style;
      }
      candidateSheets[targetSheet][targetCellRef] = nextCell;
      const evaluated = await trackSheetAsyncWork(
        computeEvaluatedSheetStore(candidateSheets, targetSheet, {
          sheet: targetSheet,
          cell: targetCellRef,
        })
      );
      return { epoch, targetSheet, targetCellRef, evaluated };
    }

    function commitPreparedFormulaBarEvaluation(prep, preSnapshot = null) {
      if (prep?.epoch !== sheetSnapshotEpoch) return;
      const out = document.getElementById("sheet-eval-result");
      commitDeferredUndoSnapshot(preSnapshot);
      replaceLiveSheetDataStore(prep.evaluated.sheets);
      restoreDomSheetCells(getActiveSheetName());
      refreshFocusedSheetCellPendingSnapshot();
      announceCircularReference(prep.evaluated.circular);
      if (out && getActiveSheetName() === prep.targetSheet) {
        out.textContent = "결과 (" + prep.targetCellRef + "): " + prep.evaluated.extraValue;
        updateSheetSelectionAndAggregates(getActiveSheetCellRef() || prep.targetCellRef);
      }
    }

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

    function evaluateActiveSheetFormulaBar(recordUndo) {
      const drainThenRun = async () => {
        await drainInFlightSheetWork();
        const preSnapshot = recordUndo ? captureWorkspaceSnapshot(false) : null;
        const prep = await prepareFormulaBarEvaluation();
        commitPreparedFormulaBarEvaluation(prep, preSnapshot);
      };
      if (recordUndo) {
        return enqueueWorkspaceHistoryOp(drainThenRun);
      }
      return drainThenRun();
    }

    function buildPreEditSnapshotForCell(td, st) {
      if (st.pendingSnapshot) {
        return st.pendingSnapshot;
      }
      const fallbackSnap = captureWorkspaceSnapshot(false);
      const shName = st.sheetName || getActiveSheetName();
      if (fallbackSnap.sheets?.[shName] && td.dataset?.cell) {
        const prevStyle = fallbackSnap.sheets[shName][td.dataset.cell]?.style;
        const cellSnap = {
          value: st.initialText,
          formula: isFormulaStr(st.initialRaw) ? st.initialRaw : "",
        };
        if (prevStyle) {
          cellSnap.style = prevStyle;
        }
        fallbackSnap.sheets[shName][td.dataset.cell] = cellSnap;
      }
      return fallbackSnap;
    }

    function pushCellUndoSnapshotIfNeeded(td, st, effectiveRaw) {
      if (!st || st.snapshotPushed || effectiveRaw === st.initialRaw) return;
      const snap = buildPreEditSnapshotForCell(td, st);
      pushRawUndoSnapshot(snap);
      st.snapshotPushed = true;
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
      const gate = rulesUi?.gateEntry(td, st) ?? null;
      if (!gate) return commitSheetCellEditNow(td, keepFocus);
      return trackSheetAsyncWork(
        gate.then((accepted) => {
          if (!accepted) {
            st.userEdited = false;
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
      const editSheet = st.sheetName;
      const rawText = grid.readTdValue(td);
      const textChanged = Boolean(st.userEdited) || rawText !== st.initialText;
      const wasEditing = Boolean(st.mode) || textChanged;
      input.endEdit(td);
      if (textChanged) {
        if (isFormulaStr(rawText)) td.dataset.formula = rawText;
        else delete td.dataset.formula;
      }
      pushCellUndoSnapshotIfNeeded(td, st, td.dataset.formula || rawText);
      const shouldRecalc = st.snapshotPushed || textChanged;
      releaseCellFocus(td, keepFocus);
      if (!shouldRecalc) {
        settleUnchangedCell(td, { editSheet, rawText, keepFocus, wasEditing });
        return Promise.resolve(false);
      }
      sheetDataStore[editSheet] = snapshotDomSheetCells();
      if (!isFormulaStr(td.dataset.formula || "")) grid.setDisplayValue(td, rawText);
      return trackSheetAsyncWork(recalcAfterCellEdit(td, editSheet, keepFocus));
    }

    async function flushFocusedSheetCellBeforeSwitch() {
      const activeEl = document.activeElement;
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

    let idleSnapshotTimer = null;

    function isHeavySheet() {
      const ext = getSheetExtent(getActiveSheetName());
      return ext.dataRows * ext.dataCols > EAGER_SNAPSHOT_MAX_ENTRIES;
    }

    function ensurePendingSnapshot(st) {
      if (!st.pendingSnapshot && !st.userEdited && !st.snapshotPushed) {
        st.pendingSnapshot = captureWorkspaceSnapshot(false);
      }
    }

    // Small sheets capture the pre-edit snapshot on focus; large sheets capture it once the
    // cell has been idle (or at the first edit) so arrow-key navigation never pays for a deep copy.
    function schedulePendingSnapshot(st) {
      if (!isHeavySheet()) {
        st.pendingSnapshot = captureWorkspaceSnapshot(false);
        return;
      }
      if (idleSnapshotTimer !== null) host.clearTimeout?.(idleSnapshotTimer);
      idleSnapshotTimer =
        host.setTimeout?.(() => {
          idleSnapshotTimer = null;
          if (sheetCellFocusState.get(document.activeElement) === st) ensurePendingSnapshot(st);
        }, IDLE_SNAPSHOT_DELAY_MS) ?? null;
    }

    const input = host.__EASYOFFICE_CREATE_SHEET_INPUT__({
      document,
      grid,
      sheetCellFocusState,
      isFormulaStr,
      getSheetName: () => getActiveSheetName(),
      getEntry: entryAt,
      getEntryCount: () => Object.keys(activeSheetMap()).length,
      forEachEntryRef: (cb) => {
        for (const ref of Object.keys(activeSheetMap())) {
          const packed = gridUtils.packRef(ref);
          if (packed > 0) {
            const col = packed & 32767;
            const row = (packed - col) / 32768;
            cb(ref, row, col);
          }
        }
      },
      getExtent: () => getSheetExtent(getActiveSheetName()),
      hasValue: (r, c) => hasCellContent(entryAt(gridUtils.refOf(r, c))),
      setActiveSheetCellRef,
      getActiveSheetCellRef,
      onSelectionChanged: () => {
        format.scheduleToolbarSync();
        lookOps?.sync();
        rulesUi?.syncArrow();
      },
      onFormatKey: (kind) => void format.formatRange(kind).catch(() => {}),
      commitSheetCellEdit,
      ensureSheetCellFocusEntry,
      syncSheetCellFocusBaseline,
      pushCellUndoSnapshotIfNeeded,
      ensurePendingSnapshot,
      schedulePendingSnapshot,
      applyCellEdits,
      clearExisting,
    });

    function updateSheetSelectionAndAggregates(cellRef) {
      input.updateSelection(cellRef);
    }

    let gridInstalled = false;
    function bindSheetCellEvents(rootEl) {
      input.bindSheetCellEvents(rootEl);
      if (gridInstalled) return;
      gridInstalled = true;
      grid.install();
      grid.renderWindow(true);
      if (sheetDataStore[getActiveSheetName()]) refreshWindowFromModel();
    }

    function runSheetClipboardCommand(cmd) {
      return input.runClipboardCommand(cmd);
    }

    const ops = host.__EASYOFFICE_CREATE_SHEET_OPS__({
      document,
      gridUtils,
      sheetDataStore,
      getActiveSheetName,
      getActiveSheetCellRef,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      refreshWindowFromModel,
      refreshFocusedSheetCellPendingSnapshot,
      updateSheetSelectionAndAggregates,
      recalculateActiveSheetFormulas,
      getRange: () => input.getRange(),
      call: sheetStoreHelpers.callSheetAction,
      hasCellContent,
    });

    const format = host.__EASYOFFICE_CREATE_SHEET_FORMAT__({
      document,
      gridUtils,
      sheetDataStore,
      getActiveSheetName,
      getActiveModule,
      getRange: () => input.getRange(),
      getSelection: () => input.getSelection(),
      getExtent: () => getSheetExtent(getActiveSheetName()),
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      refreshWindowFromModel,
      refreshFocusedSheetCellPendingSnapshot,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
    });

    lookOps = host.__EASYOFFICE_CREATE_SHEET_LOOK__({
      document,
      format,
      gridUtils,
      sheetDataStore,
      getActiveSheetName,
      getActiveModule,
      getRange: () => input.getRange(),
      getSelection: () => input.getSelection(),
    });
    host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__?.attach({
      call: sheetStoreHelpers.callSheetAction,
      onFilled: () => grid.repaintNumFmt(),
    });
    const structure = host.__EASYOFFICE_CREATE_SHEET_STRUCTURE__({
      gridUtils,
      applyCellEdits,
      document,
      sheetDataStore,
      sheetRowCounts,
      sheetStoreHelpers,
      grid,
      getActiveSheetName,
      getSheetRowCount,
      getRange: () => input.getRange(),
      getSelection: () => input.getSelection(),
      selectAxis: (axis, index, extend) => input.selectAxis(axis, index, extend),
      selectRange: (rg) => input.selectRange(rg),
      switchSheet: (name) => performSwitchSheetTabAsync(name),
      onNamesChanged: () => void recalculateActiveSheetFormulas().catch(() => {}),
      commitIfEditing: () => input.commitIfEditing(),
      focusAnchor: () => input.focusAnchor(),
      refreshChrome: () => input.refreshChrome(),
      snapshotDomSheetCells,
      replaceLiveSheetDataStore,
      refreshWindowFromModel,
      recalculateActiveSheetFormulas,
      getExtent: () => getSheetExtent(getActiveSheetName()),
      sortRange: (target) => ops.sortActiveSheetRange(target),
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
      restoreWorkspaceSnapshot,
    });
    structure.bind();

    const sheetRules = host.__EASYOFFICE_SHEET_RULES__;
    rulesUi = host.__EASYOFFICE_CREATE_SHEET_RULES_UI__({
      document,
      sheetDataStore,
      sheetRowCounts,
      getActiveSheetName,
      getRange: () => input.getRange(),
      isFormulaStr,
      sheetStoreHelpers,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
      flushFocusedSheetCellBeforeSwitch,
      performSwitchSheetTabAsync,
      applyCellEdits,
      readTdValue: (td) => grid.readTdValue(td),
    });
    // Rule changes (dialogs, undo, open) re-evaluate; new appearances repaint the cells.
    sheetRules.on("changed", () => void recalculateActiveSheetFormulas().catch(() => {}));
    sheetRules.on("repaint", () => grid.repaintFx());
    sheetRules.on("readCells", (sheet, ref) => String(sheetDataStore[sheet]?.[ref]?.value ?? ""));

    async function performSwitchSheetTabAsync(targetSheetName) {
      await flushFocusedSheetCellBeforeSwitch();
      if (!sheetDataStore.Sheet1 && getActiveSheetName() === "Sheet1") {
        sheetDataStore.Sheet1 = snapshotDomSheetCells();
      }
      saveActiveSheetCellsToWorkbook();
      setActiveSheetName(targetSheetName);
      if (!sheetDataStore[targetSheetName]) {
        sheetDataStore[targetSheetName] = {};
      }
      restoreDomSheetCells(targetSheetName);
      refreshFocusedSheetCellPendingSnapshot();
      const sheetTabBar = document.getElementById("sheet-tab-bar");
      if (sheetTabBar) {
        sheetTabBar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]").forEach((b) => {
          const isAct = b.dataset.sheetTab === targetSheetName;
          b.classList.toggle("active", isAct);
          b.setAttribute("role", "tab");
          b.setAttribute("aria-selected", isAct ? "true" : "false");
        });
      }
      updateSheetSelectionAndAggregates("A1");
      if (getActiveModule() === "SHEET" && typeof onSheetSwitchedOutline === "function") {
        onSheetSwitchedOutline();
      }
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = "시트 탭 전환: " + targetSheetName;
      structure.refreshSheetChrome();
    }

    function switchSheetTab(targetSheetName) {
      return enqueueWorkspaceHistoryOp(() => performSwitchSheetTabAsync(targetSheetName));
    }

    return {
      DEFAULT_SHEET_ROW_COUNT,
      bumpSnapshotEpoch,
      getSnapshotEpoch,
      trackSheetAsyncWork,
      drainInFlightSheetWork,
      getSheetRowCount,
      appendSheetRow,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      refreshFocusedSheetCellPendingSnapshot,
      restoreDomSheetCells,
      replaceLiveSheetDataStore,
      updateSheetSelectionAndAggregates,
      recalculateActiveSheetFormulas,
      prepareFormulaBarEvaluation,
      commitPreparedFormulaBarEvaluation,
      evaluateActiveSheetFormulaBar,
      flushFocusedSheetCellBeforeSwitch,
      commitPendingFocusedSheetCell,
      bindSheetCellEvents,
      runSheetClipboardCommand,
      runSheetStructure: structure.run,
      sortActiveSheetRange: ops.sortActiveSheetRange,
      applySheetChartSummary: ops.applySheetChartSummary,
      applySheetFormatCommand: format.applyCommand,
      openConditionalFormatDialog: () => rulesUi.openConditionalFormatDialog(),
      openDataValidationDialog: () => rulesUi.openDataValidationDialog(),
      openHyperlinkDialog: structure.openHyperlink,
      openPivotDialog: () => rulesUi.openPivotDialog(),
      getSheetRules: () => sheetRules.getAll(),
      getColumnWidths: grid.getAllWidths,
      // The sizes the grid shows and the page setup of the active sheet, for printing (`sheet-page-print.js`).
      printGeometry: () => ({
        page: host.__EASYOFFICE_SHEET_LAYOUT__.pageOf(getActiveSheetName()),
        colWidth: grid.colWidth, colHidden: grid.colHidden, rowHidden: grid.rowHidden, rowTop: grid.rowTop,
      }),
      replaceColumnWidths: grid.replaceAllWidths,
      formatSheetRange: format.formatRange,
      syncSheetFormatToolbar: format.syncToolbar,
      performSwitchSheetTabAsync,
      switchSheetTab,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__ = createSheetController;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);

