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
      commitDeferredUndoSnapshot,
      onSheetSwitchedOutline,
    } = deps;

    const {
      isFormulaStr,
      extractEvaluatedTargetValue,
      computeEvaluatedSheetStore,
      evaluateLiveFormulaTargets,
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

    function inferMaxRowFromSheetMap(sheetObj) {
      let maxRow = DEFAULT_SHEET_ROW_COUNT;
      const keys = Object.keys(sheetObj || {});
      for (const key of keys) {
        const m = /^[A-Za-z]+([1-9]\d*)$/.exec(key);
        if (m) {
          const rowNum = Number(m[1]);
          if (Number.isSafeInteger(rowNum) && rowNum > maxRow) {
            maxRow = rowNum;
          }
        }
      }
      return maxRow;
    }

    function getSheetRowCount(sheetName) {
      const stored = sheetRowCounts[sheetName];
      if (typeof stored === "number" && Number.isSafeInteger(stored) && stored >= 1) {
        return stored;
      }
      const inferred = inferMaxRowFromSheetMap(sheetDataStore[sheetName]);
      sheetRowCounts[sheetName] = inferred;
      return inferred;
    }

    // ---- used extent (rows/cols that hold data) ---------------------------------------------
    let extentCache = {};

    function scanExtent(sheetName) {
      const map = sheetDataStore[sheetName] || {};
      let dataRows = 0;
      let dataCols = 0;
      Object.keys(map).forEach((ref) => {
        if (!hasCellContent(map[ref])) return;
        const p = gridUtils.parseRef(ref);
        if (!p) return;
        if (p.row > dataRows) dataRows = p.row;
        if (p.col > dataCols) dataCols = p.col;
      });
      return { dataRows, dataCols };
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
    // `input` is created right below; the grid only calls these hooks after both exist.
    const grid = host.__EASYOFFICE_CREATE_SHEET_GRID__({
      document,
      getSheetName: () => getActiveSheetName(),
      getEntry: entryAt,
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
    const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;

    function appendSheetRow() {
      const curSheet = getActiveSheetName();
      const map = snapshotDomSheetCells();
      const nextCount = getSheetRowCount(curSheet) + 1;
      sheetRowCounts[curSheet] = nextCount;
      for (let c = 1; c <= 8; c += 1) {
        map[gridUtils.refOf(nextCount, c)] = { value: "0", formula: "" };
      }
      sheetDataStore[curSheet] = map;
      bumpExtent(curSheet, nextCount, 8);
      grid.renderWindow(true);
      grid.refreshCells();
    }

    function frozenPanesActive() {
      return Boolean(
        document.querySelector("#pane-sheet thead.frozen-header") ||
          document.querySelector("#pane-sheet table")?.dataset?.frozenPanes === "true"
      );
    }

    function syncTdIntoMap(map, td) {
      const ref = td.dataset.cell;
      const textVal = grid.readTdValue(td);
      const formula = td.dataset.formula || "";
      const prevEntry = map[ref];
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
      const condfmtHigh = td.classList.contains("condfmt-high");
      const hasStyle = Boolean(fontWeight || fontStyle || textDecoration || fontSize || condfmtHigh);
      if (value === "" && formula === "" && !hasStyle) {
        delete map[ref];
        return;
      }
      const cellEntry = { value, formula };
      if (hasStyle) {
        cellEntry.style = { fontWeight, fontStyle, textDecoration, fontSize };
        if (condfmtHigh) cellEntry.style.condfmtHigh = true;
      }
      map[ref] = cellEntry;
      if (value !== "" || formula !== "") {
        const p = gridUtils.parseRef(ref);
        if (p) bumpExtent(getActiveSheetName(), p.row, p.col);
      }
    }

    // The frozen-panes flag lives on the A1 entry (it is a per-sheet setting, A1 may be unrendered).
    function applyFrozenFlag(map, frozen) {
      const a1 = map.A1;
      if (frozen) {
        const entry = a1 || { value: "", formula: "" };
        entry.style = {
          fontWeight: "",
          fontStyle: "",
          textDecoration: "",
          fontSize: "",
          ...entry.style,
          frozenPanes: true,
        };
        map.A1 = entry;
      } else if (a1?.style?.frozenPanes) {
        delete a1.style.frozenPanes;
        const s = a1.style;
        if (!(s.fontWeight || s.fontStyle || s.textDecoration || s.fontSize || s.condfmtHigh)) {
          delete a1.style;
        }
        if (!hasCellContent(a1) && !a1.style) delete map.A1;
      }
    }

    // Merges the materialized window into the sheet model (cells outside the window are
    // already authoritative in the model) and returns the model map.
    function snapshotDomSheetCells() {
      const curSheet = getActiveSheetName();
      const map = sheetDataStore[curSheet] || {};
      document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => syncTdIntoMap(map, td));
      applyFrozenFlag(map, frozenPanesActive());
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
      Object.keys(activeSheetMap()).forEach((ref) => {
        const p = gridUtils.parseRef(ref);
        if (p && p.row >= range.r1 && p.row <= range.r2 && p.col >= range.c1 && p.col <= range.c2) {
          cb(ref);
        }
      });
    }

    function clearExisting(range) {
      snapshotDomSheetCells();
      const edits = [];
      cellRefOfRange(range, (ref) => edits.push({ ref, value: "", formula: "" }));
      return edits;
    }

    // Applies a batch of cell writes (paste, clear, cut) as ONE undoable operation.
    function applyCellEdits(edits, label) {
      if (edits.length === 0) return Promise.resolve(false);
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        const curSheet = getActiveSheetName();
        const map = snapshotDomSheetCells();
        sheetDataStore[curSheet] = map;
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        edits.forEach((e) => {
          const style = map[e.ref]?.style;
          if (e.value === "" && e.formula === "" && !style) {
            delete map[e.ref];
            return;
          }
          map[e.ref] = style
            ? { value: e.value, formula: e.formula, style }
            : { value: e.value, formula: e.formula };
          const p = gridUtils.parseRef(e.ref);
          if (p) bumpExtent(curSheet, p.row, p.col);
        });
        grid.renderWindow(false);
        refreshWindowFromModel();
        await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
        input.refreshChrome();
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = (label || "셀 편집") + ": " + edits.length + "칸";
        return true;
      });
    }

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
      delete extentCache[sheetName];
      const targetSnap = sheetDataStore[sheetName] || {};
      const targetFrozen = Boolean(targetSnap.A1?.style?.frozenPanes);
      toggleSheetFreezePanes(targetFrozen, true);
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
            sheetRowCounts[k] = inferMaxRowFromSheetMap(sheetDataStore[k]);
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
        const tdByRef = new Map();
        document.querySelectorAll("#pane-sheet td[data-cell]").forEach((el) => {
          tdByRef.set(el.dataset.cell, el);
        });
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
            const td = tdByRef.get(target.cell);
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
          updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "F2");
        }
        announceCircularReference(rpcRes?.circular);
        refreshFocusedSheetCellPendingSnapshot();
      };
      return trackSheetAsyncWork(run());
    }

    function inferAutoSumFormulaForCell(cellRef, sheetMap) {
      const m = /^([A-Z]+)(\d+)$/.exec(String(cellRef || "F2").toUpperCase());
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

    function commitSheetCellEdit(td, keepFocus) {
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
        Object.keys(activeSheetMap()).forEach((ref) => {
          const p = gridUtils.parseRef(ref);
          if (p) cb(ref, p.row, p.col);
        });
      },
      getExtent: () => getSheetExtent(getActiveSheetName()),
      hasValue: (r, c) => hasCellContent(entryAt(gridUtils.refOf(r, c))),
      setActiveSheetCellRef,
      getActiveSheetCellRef,
      onSelectionChanged: () => format.scheduleToolbarSync(),
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
      grid,
      sheetDataStore,
      getActiveSheetName,
      getActiveSheetCellRef,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      refreshWindowFromModel,
      refreshFocusedSheetCellPendingSnapshot,
      updateSheetSelectionAndAggregates,
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

    function toggleSheetFreezePanes(enabled, skipSave = false) {
      const thead = document.querySelector("#pane-sheet table thead");
      const table = document.querySelector("#pane-sheet table");
      if (thead) {
        thead.classList.toggle("frozen-header", Boolean(enabled));
      }
      if (table) {
        if (enabled) {
          table.dataset.frozenPanes = "true";
        } else {
          delete table.dataset.frozenPanes;
        }
      }
      if (!skipSave) {
        saveActiveSheetCellsToWorkbook();
      }
    }

    async function performSwitchSheetTabAsync(targetSheetName) {
      await flushFocusedSheetCellBeforeSwitch();
      if (!sheetDataStore.Sheet1 && getActiveSheetName() === "Sheet1") {
        sheetDataStore.Sheet1 = snapshotDomSheetCells();
      }
      saveActiveSheetCellsToWorkbook();
      setActiveSheetName(targetSheetName);
      if (!sheetDataStore[targetSheetName]) {
        sheetDataStore[targetSheetName] = {
          A1: { value: targetSheetName + " 항목", formula: "" },
          B1: { value: "500", formula: "" },
        };
      }
      if (!sheetRowCounts[targetSheetName]) {
        sheetRowCounts[targetSheetName] = inferMaxRowFromSheetMap(sheetDataStore[targetSheetName]);
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
      const defaultRef = targetSheetName === "Sheet1" ? "F2" : "C1";
      updateSheetSelectionAndAggregates(defaultRef);
      if (getActiveModule() === "SHEET" && typeof onSheetSwitchedOutline === "function") {
        onSheetSwitchedOutline();
      }
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = "시트 탭 전환: " + targetSheetName;
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
      sortActiveSheetByColumnB: ops.sortActiveSheetByColumnB,
      applySheetPivotOrChartSummary: ops.applySheetPivotOrChartSummary,
      toggleSheetConditionalFormatting: ops.toggleSheetConditionalFormatting,
      applySheetFormatCommand: format.applyCommand,
      getColumnWidths: grid.getAllWidths,
      replaceColumnWidths: grid.replaceAllWidths,
      formatSheetRange: format.formatRange,
      syncSheetFormatToolbar: format.syncToolbar,
      toggleSheetFreezePanes,
      performSwitchSheetTabAsync,
      switchSheetTab,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__ = createSheetController;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);

