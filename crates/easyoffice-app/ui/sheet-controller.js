(function (host) {
  "use strict";

  const DEFAULT_SHEET_ROW_COUNT = 12;
  // A sheet with more entries than this is heavy: a calculation lets the keystroke paint before it builds its payload.
  const HEAVY_SHEET_MAX_ENTRIES = 4000;

  function hasCellContent(entry) {
    return Boolean(entry && (entry.value || entry.formula));
  }

  const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;

  function createSheetController(deps) {
    const {
      document, sheetDataStore, sheetRowCounts, sheetStoreHelpers, getActiveModule,
      getActiveSheetName, setActiveSheetName, getActiveSheetCellRef, setActiveSheetCellRef,
      enqueueWorkspaceHistoryOp, restoreWorkspaceSnapshot, sheetHistory, onSheetSwitchedOutline,
      onClearMenuStateChanged,
    } = deps;
    const { isFormulaStr, extractEvaluatedTargetValue, computeEvaluatedSheetStore, evaluateLiveFormulaTargets, followCalculation } = sheetStoreHelpers;

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
    let extentCache = Object.create(null);

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

    let refocusAfterRender = false, rulesUi = null, lookOps = null, chartUi = null, syncObjectLayers = null;
    // `input` is created right below; the grid only calls these hooks after both exist.
    const grid = host.__EASYOFFICE_CREATE_SHEET_GRID__({
      document,
      getSheetName: () => getActiveSheetName(),
      getEntry: entryAt,
      getMap: () => sheetDataStore[getActiveSheetName()],
      getExtent: () => getSheetExtent(getActiveSheetName()),
      getCursor: () => {
        const s = input.getSelection();
        // A whole column or row ends at the edge of the sheet; a grid without a layout keeps to the cell it was picked from.
        return s.axis ? { row: s.ar, col: s.ac } : { row: s.cr, col: s.cc };
      },
      hooks: {
        // The rows changed their height or went away: the pictures and charts stand on the rows as they are now (PD-299).
        afterLayout: () => syncObjectLayers?.(),
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
      const fontWeight = td.style.fontWeight || "", fontStyle = td.style.fontStyle || "";
      const textDecoration = td.style.textDecoration || "", fontSize = td.style.fontSize || "";
      const fontFamily = host.__EASYOFFICE_SHEET_STYLE_CODEC__?.normalizeFontFamily?.(td.style.fontFamily) || "";
      const numFmt = td._nf || "", look = td._st;
      const hasStyle = Boolean(fontWeight || fontStyle || textDecoration || fontSize || fontFamily || numFmt || lookApi?.hasLook(look));
      if (value === "" && formula === "" && !hasStyle && !prevEntry?.link) {
        delete map[ref];
        return;
      }
      const cellEntry = { value, formula };
      const ct = !formula && td._ct ? cellTypeApi.explicitTag(value, td._ct) : undefined;
      if (ct) cellEntry.t = ct;
      if (hasStyle) {
        cellEntry.style = { fontWeight, fontStyle, textDecoration, fontSize };
        if (fontFamily) cellEntry.style.fontFamily = fontFamily;
        if (numFmt) cellEntry.style.numFmt = numFmt;
        for (const key of lookApi?.hasLook(look) ? lookApi.LOOK_KEYS : []) if (look[key]) cellEntry.style[key] = look[key];
      }
      spill?.copyComputedMeta(prevEntry, cellEntry, formula);
      if (!formula && value !== "" && prevEntry?.formula === "" && prevEntry?.ph && prevEntry.ph.b === value) {
        cellEntry.ph = prevEntry.ph;
      }
      // A hyperlink (PD-218) stays through any text the cell is given, even none.
      if (prevEntry?.link) cellEntry.link = prevEntry.link;
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
      if ((sheetRowCounts[curSheet] ?? 0) === 0) {
        const domRows = document.querySelectorAll("#pane-sheet tbody tr").length;
        sheetRowCounts[curSheet] = domRows > 0 ? domRows : DEFAULT_SHEET_ROW_COUNT;
      }
    }

    function cellRefOfRange(range, cb) {
      const map = activeSheetMap();
      const r1 = Math.min(range.r1, range.r2), r2 = Math.max(range.r1, range.r2);
      const c1 = Math.min(range.c1, range.c2), c2 = Math.max(range.c1, range.c2);
      if ((r2 - r1 + 1) * (c2 - c1 + 1) <= 4096) {
        for (let r = r1; r <= r2; r++) {
          for (let c = c1; c <= c2; c++) {
            const ref = gridUtils.refOf(r, c);
            if (map && map[ref] !== undefined) cb(ref);
          }
        }
        return;
      }
      for (const ref of Object.keys(map)) {
        const packed = gridUtils.packRef(ref);
        if (packed > 0) {
          const col = packed & 32767, row = (packed - col) / 32768;
          if (row >= r1 && row <= r2 && col >= c1 && col <= c2) cb(ref);
        }
      }
    }

    function clearExisting(range, dropLinks = false) {
      snapshotDomSheetCells();
      const edits = [];
      cellRefOfRange(range, (ref) => edits.push(dropLinks ? { ref, value: "", formula: "", link: null } : { ref, value: "", formula: "" }));
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
      sheetHistory,
      bumpExtent,
      refreshWindowFromModel,
      recalculateActiveSheetFormulas,
      refreshChrome: () => input.refreshChrome(),
    });

    // A step that runs after a cell entry (the growth of a table, a paste) shows the model when it ends, by which time the next
    // cell may be typed in. `keepTyping` lets that cell keep its text and its edit.
    const isTyped = (td) => {
      const st = sheetCellFocusState.get(td);
      return Boolean(st && (st.mode || st.userEdited));
    };

    function refreshWindowFromModel(keepTyping = false) {
      grid.refreshCells(keepTyping ? isTyped : null).forEach((td) => {
        if (keepTyping && isTyped(td)) return;
        if (sheetCellFocusState.has(td)) syncSheetCellFocusBaseline(td, true);
        else if (document.activeElement === td) ensureSheetCellFocusEntry(td, getActiveSheetName());
      });
      chartUi?.syncLayer();
    }

    // `keepExtent`: the used extent is the one `noteCellsRestored` keeps, not read again from every cell.
    function restoreDomSheetCells(sheetName, { keepExtent = false } = {}) {
      // Seed the default sheet only for an empty workbook (an opened file defines its own sheets).
      if (Object.keys(sheetDataStore).length === 0) {
        sheetDataStore.Sheet1 = snapshotDomSheetCells();
      }
      if (!keepExtent) extentCache[sheetName] = scanExtent(sheetName);
      grid.renderWindow(true);
      refreshWindowFromModel();
    }

    // Cells of a sheet were written back by an undo or a redo: `written` are the packed addresses that now hold
    // content, `removed` those that hold none. The used extent grows with the first; a cell removed at its edge makes it
    // be read again when asked for.
    // `moved`: the cells of the sheet were shifted by a row or column edit, so the extent is read again.
    function noteCellsRestored(sheetName, { written, removed, moved }) {
      const e = extentCache[sheetName];
      if (!e) return;
      if (moved) {
        delete extentCache[sheetName];
        return;
      }
      for (const packed of written) bumpExtent(sheetName, Math.floor(packed / 32768), packed % 32768);
      for (const packed of removed) {
        if (Math.floor(packed / 32768) >= e.dataRows || packed % 32768 >= e.dataCols) {
          delete extentCache[sheetName];
          return;
        }
      }
    }

    function replaceLiveSheetDataStore(nextSheets, nextRowCounts) {
      extentCache = Object.create(null);
      Object.keys(sheetDataStore).forEach((k) => delete sheetDataStore[k]);
      Object.assign(sheetDataStore, nextSheets);
      if (nextRowCounts && typeof nextRowCounts === "object") {
        Object.keys(sheetRowCounts).forEach((k) => delete sheetRowCounts[k]);
        Object.assign(sheetRowCounts, nextRowCounts);
      } else {
        Object.keys(sheetRowCounts).forEach((k) => {
          if (!Object.hasOwn(sheetDataStore, k)) delete sheetRowCounts[k];
        });
        Object.keys(sheetDataStore).forEach((k) => {
          if ((sheetRowCounts[k] ?? 0) === 0) extentCache[k] = scanExtent(k);
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

    function snapshotFormulaValues(store) {
      const out = new Map();
      for (const [sh, map] of Object.entries(store || {})) {
        for (const [ref, entry] of Object.entries(map || {})) {
          if (entry && isFormulaStr(entry.formula || "")) out.set(sh + "!" + ref, entry.value ?? "");
        }
      }
      return out;
    }

    function restoreUnallowedFormulaValues(store, prevValues, allowTarget) {
      for (const [key, prevVal] of prevValues) {
        const bang = key.indexOf("!");
        if (bang < 0) continue;
        const sh = key.slice(0, bang), cell = key.slice(bang + 1);
        if (allowTarget(sh, cell)) continue;
        const entry = store[sh]?.[cell];
        if (entry && isFormulaStr(entry.formula || "")) entry.value = prevVal;
      }
    }

    function recalculateActiveSheetFormulas(options = {}) {
      const forceFocusedDisplay = Boolean(options.updateFocusedDisplay);
      const force = options.force || null;
      const calcApi = host.__EASYOFFICE_SHEET_CALC_OPTIONS__;
      const manualUnforced = calcApi?.isManual?.() === true && !force;
      const edited = new Set([...(options.editedRefs || []), ...(options.editedRef ? [options.editedRef] : [])]);
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
        if (manualUnforced) {
          if (sheetStoreHelpers.sheetMapHasAnyFormula(sheetDataStore)) calcApi.markPending(document);
          if (edited.size === 0) {
            chartUi?.syncLayer();
            refreshFocusedSheetCellPendingSnapshot();
            return;
          }
        }
        const partialScope = force === "sheet" || manualUnforced;
        const allowTarget = (sh, cell) =>
          !partialScope || (sh === curSheet && (force === "sheet" || edited.has(cell)));
        const prevValues = partialScope ? snapshotFormulaValues(sheetDataStore) : null;
        const liveEval = await evaluateLiveFormulaTargets(sheetDataStore, curSheet);
        if (epoch !== sheetSnapshotEpoch || seq !== latestSheetRecalcSeq) return;
        if (!liveEval) {
          if (force === "all" || !calcApi?.isManual?.()) calcApi?.clearPending(document);
          chartUi?.syncLayer();
          return;
        }
        if (prevValues) restoreUnallowedFormulaValues(sheetDataStore, prevValues, allowTarget);
        const { rpcRes, formulaTargets, targetValues, batchValues } = liveEval;
        let updatedActiveSheet = false;
        // The cells an array fills changed: the window shows them again (PD-217).
        if (liveEval.spillsChanged && getActiveModule() === "SHEET") {
          refreshWindowFromModel(true);
          updatedActiveSheet = true;
        }
        formulaTargets.forEach((target, idx) => {
          if (!allowTarget(target.sheet, target.cell)) return;
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
            if ((!forceFocusedDisplay && (focusSt?.userEdited || focusSt?.snapshotPushed)) || focusSt?.mode) return;
            const isEditingFormulaInPlace =
              !forceFocusedDisplay &&
              document.activeElement === td &&
              isFormulaStr((td.textContent || "").trim());
            if (!isEditingFormulaInPlace) {
              grid.setDisplayValue(td, value);
              host.__EASYOFFICE_SHEET_LINK__?.paint(td, storedEntry?.link, storedEntry);
              syncSheetCellFocusBaseline(td, forceFocusedDisplay);
            }
          }
        });
        // A formula in a table's header cell is fixed as the text of its result (PD-224).
        if (host.__EASYOFFICE_SHEET_LAYOUT__.settleTableHeaders(sheetDataStore[curSheet], curSheet)) {
          refreshWindowFromModel(true);
          updatedActiveSheet = true;
        }
        // A header cell that got new text names its column anew: the formulas that name the column follow (PD-225).
        sheetStoreHelpers.followTableHeaders(sheetDataStore, () => {
          refreshWindowFromModel(true);
          void recalculateActiveSheetFormulas();
        });
        if (updatedActiveSheet && getActiveModule() === "SHEET") {
          grid.syncRowFit?.();
          updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "A1");
        }
        chartUi?.syncLayer();
        announceCircularReference(rpcRes?.circular);
        // Unreached cells keep their shown value; ask again until the engine has them all.
        if (
          followCalculation(rpcRes, document.getElementById("status-command-feedback")) &&
          typeof host.setTimeout === "function"
        ) {
          host.setTimeout(() => void recalculateActiveSheetFormulas(options), 0);
        } else if (rpcRes?.timedOut !== true) {
          if (force === "all" || !calcApi?.isManual?.()) {
            calcApi?.clearPending(document);
          } else if (force === "sheet") {
            const otherFormulas = Object.entries(sheetDataStore).some(
              ([sh, m]) => sh !== curSheet && Object.values(m || {}).some((e) => e && isFormulaStr(e.formula || ""))
            );
            if (!otherFormulas) calcApi?.clearPending(document);
          }
        }
        refreshFocusedSheetCellPendingSnapshot();
      };
      return trackSheetAsyncWork(run());
    }

    function isHeavySheet() {
      const ext = getSheetExtent(getActiveSheetName());
      return ext.dataRows * ext.dataCols > HEAVY_SHEET_MAX_ENTRIES;
    }

    // The focus state of the cells and the commit of an entry (`sheet-cell-edit.js`); the rules, the input and the structure parts come after it.
    const {
      ensureSheetCellFocusEntry, syncSheetCellFocusBaseline, refreshFocusedSheetCellPendingSnapshot, pushCellUndoSnapshotIfNeeded,
      retractCellUndoSnapshot, commitSheetCellEdit, flushFocusedSheetCellBeforeSwitch, commitPendingFocusedSheetCell,
      ensurePendingSnapshot, schedulePendingSnapshot,
    } = host.__EASYOFFICE_CREATE_SHEET_CELL_EDIT__({
      document, sheetDataStore, sheetHistory, isFormulaStr, grid, sheetCellFocusState, getActiveSheetName, getActiveSheetCellRef,
      trackSheetAsyncWork, drainInFlightSheetWork, snapshotDomSheetCells, recalculateActiveSheetFormulas, updateSheetSelectionAndAggregates,
      getRulesUi: () => rulesUi,
      getInput: () => input,
      autoLink: (sheet, ref, typed) => structure.autoLink(sheet, ref, typed),
    });

    // The entry of the formula bar (`sheet-formula-bar.js`).
    const { prepareFormulaBarEvaluation, commitPreparedFormulaBarEvaluation, evaluateActiveSheetFormulaBar } = host.__EASYOFFICE_CREATE_SHEET_FORMULA_BAR__({
      document, sheetDataStore, sheetHistory, gridUtils, isFormulaStr, computeEvaluatedSheetStore, getActiveSheetName, getActiveSheetCellRef,
      snapshotDomSheetCells, trackSheetAsyncWork, getSnapshotEpoch, replaceLiveSheetDataStore, restoreDomSheetCells,
      refreshFocusedSheetCellPendingSnapshot, announceCircularReference, updateSheetSelectionAndAggregates, drainInFlightSheetWork,
      enqueueWorkspaceHistoryOp,
      autoLink: (sheet, ref, typed) => structure.autoLink(sheet, ref, typed),
    });

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
        rulesUi?.syncClearMenu();
        host.__EASYOFFICE_SHEET_TABLE_BAR__?.sync();
        host.__EASYOFFICE_SHEET_NOTE__?.syncLayer();
      },
      onFormatKey: (kind) => void format.formatRange(kind).catch(() => {}),
      commitSheetCellEdit,
      pushCellUndoSnapshotIfNeeded,
      retractCellUndoSnapshot,
      switchSheet: (name) => performSwitchSheetTabAsync(name),
      // The next (1) or previous (-1) shown sheet in tab order, as the keys of `sheet-nav.js` ask for (PD-291),
      // clearing any active sheet group (PD-346).
      stepSheet: (step) => {
        void enqueueWorkspaceHistoryOp(async () => {
          const strip = host.__EASYOFFICE_SHEET_TAB_STRIP__;
          const active = getActiveSheetName();
          const hadGroup = strip.isGrouped();
          const keys = [...new Set([...Object.keys(sheetDataStore), active])];
          const shown = strip.visibleOf(keys);
          const target = host.__EASYOFFICE_SHEET_NAV__.sheetTarget(shown, active, step);
          if (target !== null) {
            strip.clearGroup(target);
            await performSwitchSheetTabAsync(target);
            input.focusAnchor();
          } else if (hadGroup) {
            strip.clearGroup(active);
            strip.syncSelection(document, keys, active);
            input.focusAnchor();
          }
        }).catch(() => {});
      },
      // Extends or shrinks grouped sheet selection `[anchor..target]` along tab order (PD-346).
      stepSheetGroup: (step) => {
        void enqueueWorkspaceHistoryOp(async () => {
          const strip = host.__EASYOFFICE_SHEET_TAB_STRIP__;
          const active = getActiveSheetName();
          const keys = [...new Set([...Object.keys(sheetDataStore), active])];
          const next = strip.stepGroup(keys, active, step);
          if (!next) return;
          await performSwitchSheetTabAsync(next.active);
          input.focusAnchor();
        }).catch(() => {});
      },
      ensureSheetCellFocusEntry,
      syncSheetCellFocusBaseline,
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

    const viewLive = host.__EASYOFFICE_CREATE_SHEET_VIEW_LIVE__({
      document,
      grid,
      input,
      sheetDataStore,
      getActiveModule,
      getActiveSheetName,
      setActiveSheetName,
      flushFocusedSheetCellBeforeSwitch,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      restoreDomSheetCells,
      refreshFocusedSheetCellPendingSnapshot,
      afterSwitch: (target) => {
        if (getActiveModule() === "SHEET" && typeof onSheetSwitchedOutline === "function") onSheetSwitchedOutline();
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = "시트 탭 전환: " + target;
        structure.refreshSheetChrome();
        input.afterSheetSwitch(target);
        rulesUi?.syncClearMenu();
      },
    });

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
      sheetHistory,
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
      gridUtils, applyCellEdits, document, sheetDataStore, sheetRowCounts, sheetStoreHelpers, grid,
      getActiveSheetName, getActiveModule, getSheetRowCount,
      getRange: () => input.getRange(),
      getSelection: () => input.getSelection(),
      selectAxis: (axis, index, extend) => input.selectAxis(axis, index, extend),
      selectRange: (rg) => input.selectRange(rg),
      switchSheet: (name) => performSwitchSheetTabAsync(name),
      onNamesChanged: () => void recalculateActiveSheetFormulas().catch(() => {}),
      insertFormulaName: (text) => input.insertFormulaName(text),
      hasFormulaEditSurface: () => input.hasFormulaEditSurface(),
      holdsFormulaMenu: () => input.holdsFormulaMenu(),
      isEditingFormula: () => input.isEditingFormula(),
      getFormulaOriginSheet: () => input.originSheet(),
      beginFormulaInsertHold: () => input.beginFormulaInsertHold(),
      endFormulaInsertHold: (slot, text) => input.endFormulaInsertHold(slot, text),
      commitIfEditing: () => input.commitIfEditing(),
      focusAnchor: () => input.focusAnchor(),
      refreshChrome: () => input.refreshChrome(),
      snapshotDomSheetCells,
      replaceLiveSheetDataStore,
      refreshWindowFromModel,
      recalculateActiveSheetFormulas,
      getExtent: () => getSheetExtent(getActiveSheetName()),
      sortRange: (target) => ops.sortActiveSheetRange(target),
      viewLive,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
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
      getSelection: () => input.getSelection(),
      onClearMenuStateChanged: () => {
        if (getActiveModule() === "SHEET") onClearMenuStateChanged?.();
      },
      isFormulaStr,
      sheetStoreHelpers,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
      flushFocusedSheetCellBeforeSwitch,
      performSwitchSheetTabAsync,
      applyCellEdits,
      readTdValue: (td) => grid.readTdValue(td),
    });
    // Rule changes (dialogs, undo, open) re-evaluate; new appearances repaint the cells.
    sheetRules.on("changed", () => void recalculateActiveSheetFormulas().catch(() => {}));
    // The rows a user or a filter hid changed (a hide, a filter, an undo): the formulas that leave hidden rows out are calculated
    // again, and only those in the engine (PD-232).
    // It runs after the step that changed them (an undo restores the cells after the layout).
    // The same for the pivot tables `GETPIVOTDATA` reads: a create, an edit, a delete and an undo of one (PD-292).
    const layoutStale = () => sheetStoreHelpers.hiddenRowsStale(getActiveSheetName()) || sheetStoreHelpers.pivotsStale() || Boolean(sheetStoreHelpers.cellMetaStale?.(sheetDataStore));
    host.__EASYOFFICE_SHEET_LAYOUT__.onChange(() => {
      if (getActiveModule() === "SHEET") rulesUi?.syncClearMenu();
      if (getActiveModule() !== "SHEET" || !layoutStale()) return;
      void enqueueWorkspaceHistoryOp(async () => {
        if (layoutStale()) await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
      }).catch(() => {});
    });
    sheetRules.on("repaint", () => grid.repaintFx());
    sheetRules.on("readCells", (sheet, ref) => String(sheetDataStore[sheet]?.[ref]?.value ?? ""));
    chartUi = host.__EASYOFFICE_CREATE_SHEET_CHART_UI__?.({
      document, grid, sheetDataStore, getActiveSheetName, getActiveModule,
      getSelectionRange: () => input.getRange(), getExtent: () => getSheetExtent(getActiveSheetName()),
      snapshotDomSheetCells, enqueueWorkspaceHistoryOp, drainInFlightSheetWork, sheetHistory, focusAnchor: () => input.focusAnchor(),
    }) ?? null;
    syncObjectLayers = () => {
      chartUi?.syncLayer();
      structure.syncLayers?.();
    };

    // Switching sheets and the view each sheet keeps (`sheet-view-live.js`).
    function performSwitchSheetTabAsync(target) {
      return viewLive.switchTo(target);
    }

    function switchSheetTab(targetSheetName) {
      return enqueueWorkspaceHistoryOp(() => performSwitchSheetTabAsync(targetSheetName));
    }

    // Handles sheet tab pointer selection (Shift range, Ctrl/Cmd toggle, plain click inside/outside group) while
    // yielding cleanly to formula point mode when active.
    function clickSheetTab(targetSheetName, mods = {}) {
      const shiftKey = Boolean(mods.shiftKey);
      const ctrlKey = Boolean(mods.ctrlKey);
      const pointing = Boolean(input.holdsTabPress?.());
      return enqueueWorkspaceHistoryOp(async () => {
        const strip = host.__EASYOFFICE_SHEET_TAB_STRIP__;
        const active = getActiveSheetName();
        const keys = [...new Set([...Object.keys(sheetDataStore), active])];
        if (pointing) {
          strip.clearGroup();
          return performSwitchSheetTabAsync(targetSheetName);
        }
        if (shiftKey) {
          const next = strip.rangeGroup(keys, active, targetSheetName);
          if (!next) return;
          if (next.active !== active) return performSwitchSheetTabAsync(next.active);
          strip.syncSelection(document, keys, active);
          return;
        }
        if (ctrlKey) {
          const next = strip.toggleGroup(keys, active, targetSheetName);
          if (!next || !next.changed) return;
          if (next.switched) return performSwitchSheetTabAsync(next.active);
          strip.syncSelection(document, keys, next.active);
          return;
        }
        if (strip.isGrouped() && strip.hasInGroup(targetSheetName)) {
          if (targetSheetName !== active) return performSwitchSheetTabAsync(targetSheetName);
          strip.syncSelection(document, keys, active);
          return;
        }
        strip.clearGroup(targetSheetName);
        return performSwitchSheetTabAsync(targetSheetName);
      });
    }

    return {
      DEFAULT_SHEET_ROW_COUNT,
      bumpSnapshotEpoch,
      getSnapshotEpoch,
      trackSheetAsyncWork,
      drainInFlightSheetWork,
      getSheetRowCount,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      refreshFocusedSheetCellPendingSnapshot,
      restoreDomSheetCells,
      replaceLiveSheetDataStore,
      noteCellsRestored,
      getSelectionRange: () => input.getRange(),
      updateSheetSelectionAndAggregates,
      recalculateActiveSheetFormulas,
      prepareFormulaBarEvaluation,
      commitPreparedFormulaBarEvaluation,
      evaluateActiveSheetFormulaBar,
      flushFocusedSheetCellBeforeSwitch,
      commitPendingFocusedSheetCell,
      handleFormulaBarKey: (ev) => input.handleBarKey(ev),
      bindSheetCellEvents,
      runSheetClipboardCommand,
      runSheetStructure: structure.run,
      sortActiveSheetRange: ops.sortActiveSheetRange,
      applySheetChartSummary: ops.applySheetChartSummary,
      applySheetFormatCommand: format.applyCommand,
      openConditionalFormatDialog: () => rulesUi.openConditionalFormatDialog(),
      syncCfClearMenu: () => rulesUi?.syncClearMenu(),
      openDataValidationDialog: () => rulesUi.openDataValidationDialog(),
      openHyperlinkDialog: structure.openHyperlink,
      openPivotDialog: () => rulesUi.openPivotDialog(),
      openChartDialog: (id) => chartUi?.openChartDialog(id),
      syncChartLayer: () => syncObjectLayers(),
      getSheetRules: () => sheetRules.getAll(),
      getColumnWidths: grid.getAllWidths,
      // The sizes the grid shows and the page setup of the active sheet, for printing (`sheet-page-print.js`).
      printGeometry: () => ({
        page: host.__EASYOFFICE_SHEET_LAYOUT__.pageOf(getActiveSheetName()),
        colWidth: grid.colWidth, colLeft: grid.colLeft, colHidden: grid.colHidden, rowHidden: grid.rowHidden, rowTop: grid.rowTop,
        charts: host.__EASYOFFICE_SHEET_LAYOUT__.chartsOf(getActiveSheetName()), images: host.__EASYOFFICE_SHEET_LAYOUT__.imagesOf(getActiveSheetName()), sheetName: getActiveSheetName(), sheetStore: sheetDataStore,
      }),
      replaceColumnWidths: grid.replaceAllWidths,
      formatSheetRange: format.formatRange,
      syncSheetFormatToolbar: format.syncToolbar,
      performSwitchSheetTabAsync,
      switchSheetTab,
      clickSheetTab,
      captureView: viewLive.capture,
      restoreView: viewLive.restore,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__ = createSheetController;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);

