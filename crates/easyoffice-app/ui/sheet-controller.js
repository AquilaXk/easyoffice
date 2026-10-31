(function (host) {
  "use strict";

  const DEFAULT_SHEET_ROW_COUNT = 12;
  const GRID_COL_LETTERS = ["A", "B", "C", "D", "E", "F", "G", "H"];

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
      for (let i = 0; i < keys.length; i += 1) {
        const m = /^[A-Za-z]+([1-9][0-9]*)$/.exec(keys[i]);
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

    function buildGridRowElement(rowIdx, defaultCellText) {
      const tr = document.createElement("tr");
      const cellsHtml = GRID_COL_LETTERS.map(
        (c) => '<td contenteditable="true" data-cell="' + c + rowIdx + '">' + defaultCellText + "</td>"
      ).join("");
      tr.innerHTML = '<th data-row="' + rowIdx + '">' + rowIdx + "</th>" + cellsHtml;
      return tr;
    }

    function ensureSheetGridRowCount(targetRowCount) {
      const tbody = document.querySelector("#pane-sheet tbody");
      if (!tbody) return;
      const desired = Math.max(1, Number(targetRowCount) || DEFAULT_SHEET_ROW_COUNT);
      const existingRows = Array.from(tbody.querySelectorAll("tr"));
      while (existingRows.length > desired) {
        const extra = existingRows.pop();
        extra.remove();
      }
      for (let rowIdx = existingRows.length + 1; rowIdx <= desired; rowIdx += 1) {
        const tr = buildGridRowElement(rowIdx, "");
        tbody.appendChild(tr);
        bindSheetCellEvents(tr);
      }
    }

    function appendSheetRow() {
      const curSheet = getActiveSheetName();
      const nextCount = getSheetRowCount(curSheet) + 1;
      sheetRowCounts[curSheet] = nextCount;
      const tbody = document.querySelector("#pane-sheet tbody");
      if (tbody) {
        ensureSheetGridRowCount(nextCount - 1);
        const tr = buildGridRowElement(nextCount, "0");
        tbody.appendChild(tr);
        bindSheetCellEvents(tr);
      }
      sheetDataStore[curSheet] = snapshotDomSheetCells();
    }

    function snapshotDomSheetCells() {
      const curSheet = getActiveSheetName();
      const prevSnap = sheetDataStore[curSheet] || {};
      const snap = {};
      document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
        const ref = td.dataset.cell;
        const textVal = (td.textContent || "").trim();
        const formula = td.dataset.formula || "";
        const prevEntry = prevSnap[ref];
        const keepComputedValue =
          isFormulaStr(formula) &&
          textVal === formula &&
          Boolean(prevEntry) &&
          prevEntry.formula === formula &&
          prevEntry.value !== "" &&
          !isFormulaStr(prevEntry.value);
        const cellEntry = {
          value: keepComputedValue ? prevEntry.value : textVal,
          formula,
        };
        const fontWeight = td.style.fontWeight || "";
        const fontStyle = td.style.fontStyle || "";
        const textDecoration = td.style.textDecoration || "";
        const fontSize = td.style.fontSize || "";
        if (fontWeight || fontStyle || textDecoration || fontSize) {
          cellEntry.style = { fontWeight, fontStyle, textDecoration, fontSize };
        }
        snap[ref] = cellEntry;
      });
      return snap;
    }

    function saveActiveSheetCellsToWorkbook() {
      const curSheet = getActiveSheetName();
      sheetDataStore[curSheet] = snapshotDomSheetCells();
      if (!sheetRowCounts[curSheet]) {
        const domRows = document.querySelectorAll("#pane-sheet tbody tr").length;
        sheetRowCounts[curSheet] = domRows > 0 ? domRows : DEFAULT_SHEET_ROW_COUNT;
      }
    }

    function ensureSheetCellFocusEntry(td, sheetName) {
      const initialText = (td.textContent || "").trim();
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
      const text = (td.textContent || "").trim();
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
        st.pendingSnapshot = captureWorkspaceSnapshot(false);
      }
    }

    function restoreDomSheetCells(sheetName) {
      if (!sheetDataStore.Sheet1) {
        sheetDataStore.Sheet1 = snapshotDomSheetCells();
      }
      const targetRowCount = getSheetRowCount(sheetName);
      ensureSheetGridRowCount(targetRowCount);
      const targetSnap = sheetDataStore[sheetName] || {};
      document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
        const ref = td.dataset.cell;
        const entry = targetSnap[ref];
        if (entry) {
          td.textContent = entry.value || "";
          if (entry.formula) {
            td.dataset.formula = entry.formula;
          } else {
            delete td.dataset.formula;
          }
        } else {
          td.textContent = "";
          delete td.dataset.formula;
        }
        const cellStyle = entry?.style;
        td.style.fontWeight = cellStyle?.fontWeight || "";
        td.style.fontStyle = cellStyle?.fontStyle || "";
        td.style.textDecoration = cellStyle?.textDecoration || "";
        td.style.fontSize = cellStyle?.fontSize || "";
        if (sheetCellFocusState.has(td)) {
          syncSheetCellFocusBaseline(td, true);
        } else if (document.activeElement === td) {
          ensureSheetCellFocusEntry(td, sheetName);
        }
      });
    }

    function replaceLiveSheetDataStore(nextSheets, nextRowCounts) {
      Object.keys(sheetDataStore).forEach((k) => delete sheetDataStore[k]);
      Object.assign(sheetDataStore, nextSheets);
      if (nextRowCounts && typeof nextRowCounts === "object") {
        Object.keys(sheetRowCounts).forEach((k) => delete sheetRowCounts[k]);
        Object.assign(sheetRowCounts, nextRowCounts);
      } else {
        Object.keys(sheetRowCounts).forEach((k) => {
          if (!Object.prototype.hasOwnProperty.call(sheetDataStore, k)) {
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

    function updateSheetSelectionAndAggregates(cellRef) {
      setActiveSheetCellRef(cellRef);
      const parsedRef = /^([A-Z]+)(\d+)$/.exec(String(cellRef || "").trim().toUpperCase());
      const colLetter = parsedRef ? parsedRef[1] : cellRef.charAt(0);
      const rowNumber = parsedRef ? parsedRef[2] : cellRef.slice(1);

      document.querySelectorAll("#pane-sheet th[data-col]").forEach((th) => {
        th.classList.toggle("active-axis", th.dataset.col === colLetter);
      });
      document.querySelectorAll("#pane-sheet th[data-row]").forEach((th) => {
        th.classList.toggle("active-axis", th.dataset.row === rowNumber);
      });

      let activeTd = null;
      const numericVals = [];
      const curSheet = getActiveSheetName();
      const activeSheetSnap = sheetDataStore[curSheet] || {};
      document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
        const ref = td.dataset.cell;
        const isSel = ref === cellRef;
        td.classList.toggle("selected-cell", isSel);
        if (isSel) activeTd = td;
        const focusSt = sheetCellFocusState.get(td);
        let displayCandidate = (td.textContent || "").trim();
        if (
          isFormulaStr(td.dataset.formula || "") &&
          !focusSt?.userEdited &&
          isFormulaStr(displayCandidate)
        ) {
          const storedVal = (activeSheetSnap[ref]?.value || "").trim();
          if (storedVal && !isFormulaStr(storedVal)) {
            displayCandidate = storedVal;
          }
        }
        const num = Number(displayCandidate.replaceAll(",", "").trim());
        if (Number.isFinite(num) && displayCandidate !== "") {
          numericVals.push(num);
        }
      });

      const refBadge = document.getElementById("active-cell-ref");
      if (refBadge) refBadge.textContent = cellRef;
      const formulaInput = document.getElementById("formula-input");
      if (formulaInput && activeTd && document.activeElement !== formulaInput) {
        const activeFocusSt = sheetCellFocusState.get(activeTd);
        const liveCellText = (activeTd.textContent || "").trim();
        if (activeFocusSt?.userEdited) {
          formulaInput.value = liveCellText;
        } else {
          formulaInput.value = activeTd.dataset.formula || liveCellText;
        }
      }

      const aggEl = document.getElementById("status-sheet-agg");
      if (aggEl) {
        if (numericVals.length > 0) {
          let sum = 0;
          let max = numericVals[0];
          for (let i = 0; i < numericVals.length; i += 1) {
            sum += numericVals[i];
            if (numericVals[i] > max) max = numericVals[i];
          }
          const avg = Math.round((sum / numericVals.length) * 10) / 10;
          aggEl.textContent =
            "선택 집계 · 합계 " +
            sum.toLocaleString() +
            " · 평균 " +
            avg.toLocaleString() +
            " · 개수 " +
            numericVals.length +
            " · 최대 " +
            max.toLocaleString();
        } else {
          aggEl.textContent = "선택 집계 · 숫자 데이터 없음";
        }
      }
    }

    function recalculateActiveSheetFormulas(options = {}) {
      const forceFocusedDisplay = Boolean(options.updateFocusedDisplay);
      const run = async () => {
        const epoch = sheetSnapshotEpoch;
        const seq = latestSheetRecalcSeq + 1;
        latestSheetRecalcSeq = seq;
        const curSheet = getActiveSheetName();
        sheetDataStore[curSheet] = snapshotDomSheetCells();
        const liveEval = await evaluateLiveFormulaTargets(sheetDataStore, curSheet);
        if (!liveEval || epoch !== sheetSnapshotEpoch || seq !== latestSheetRecalcSeq) {
          return;
        }
        const { rpcRes, formulaTargets, targetValues, batchValues } = liveEval;
        let updatedActiveSheet = false;
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
            const td = document.querySelector('#pane-sheet td[data-cell="' + target.cell + '"]');
            const focusSt = td ? sheetCellFocusState.get(td) : null;
            if (!td || !isFormulaStr(td.dataset.formula || "")) return;
            if (!forceFocusedDisplay && (focusSt?.userEdited || focusSt?.snapshotPushed)) return;
            const isEditingFormulaInPlace =
              !forceFocusedDisplay &&
              document.activeElement === td &&
              isFormulaStr((td.textContent || "").trim());
            if (!isEditingFormulaInPlace) {
              td.textContent = value;
              syncSheetCellFocusBaseline(td, forceFocusedDisplay);
            }
          }
        });
        if (updatedActiveSheet && getActiveModule() === "SHEET") {
          updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "F2");
        }
        refreshFocusedSheetCellPendingSnapshot();
      };
      return trackSheetAsyncWork(run());
    }

    async function prepareFormulaBarEvaluation() {
      const input = document.getElementById("formula-input");
      const out = document.getElementById("sheet-eval-result");
      if (!input || !out) return null;
      const epoch = sheetSnapshotEpoch;
      const targetSheet = getActiveSheetName();
      const targetCellRef = getActiveSheetCellRef();
      sheetDataStore[targetSheet] = snapshotDomSheetCells();
      const candidateSheets = JSON.parse(JSON.stringify(sheetDataStore));
      if (!candidateSheets[targetSheet]) {
        candidateSheets[targetSheet] = {};
      }
      const rawFormula = input.value.trim();
      const isFormula = isFormulaStr(rawFormula);
      const prevCell = candidateSheets[targetSheet][targetCellRef];
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
      if (!prep || prep.epoch !== sheetSnapshotEpoch) return;
      const out = document.getElementById("sheet-eval-result");
      commitDeferredUndoSnapshot(preSnapshot);
      replaceLiveSheetDataStore(prep.evaluated.sheets);
      restoreDomSheetCells(getActiveSheetName());
      refreshFocusedSheetCellPendingSnapshot();
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
      if (fallbackSnap.sheets && fallbackSnap.sheets[shName] && td.dataset?.cell) {
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

    function commitSheetCellEdit(td, keepFocus) {
      const st = sheetCellFocusState.get(td);
      const curSheet = getActiveSheetName();
      if (!st || st.sheetName !== curSheet) {
        if (!keepFocus) sheetCellFocusState.delete(td);
        return Promise.resolve(false);
      }
      const editSheet = st.sheetName;
      const rawText = (td.textContent || "").trim();
      const textChanged = Boolean(st.userEdited) || rawText !== st.initialText;
      if (textChanged) {
        if (isFormulaStr(rawText)) {
          td.dataset.formula = rawText;
        } else {
          delete td.dataset.formula;
        }
      }
      const effectiveRaw = td.dataset.formula || rawText;
      pushCellUndoSnapshotIfNeeded(td, st, effectiveRaw);
      const shouldRecalc = st.snapshotPushed || textChanged;
      if (keepFocus) {
        syncSheetCellFocusBaseline(td, true);
      } else {
        sheetCellFocusState.delete(td);
        if (document.activeElement === td && typeof td.blur === "function") {
          td.blur();
        }
      }
      if (!shouldRecalc) {
        if (
          !keepFocus &&
          isFormulaStr(td.dataset.formula || "") &&
          isFormulaStr((td.textContent || "").trim())
        ) {
          const cached = sheetDataStore[editSheet]?.[td.dataset.cell]?.value;
          if (cached !== undefined && cached !== "" && !isFormulaStr(cached)) {
            td.textContent = cached;
            if (getActiveSheetName() === editSheet) {
              updateSheetSelectionAndAggregates(getActiveSheetCellRef() || td.dataset.cell);
            }
          }
        }
        if (keepFocus) {
          refreshFocusedSheetCellPendingSnapshot();
        }
        return Promise.resolve(false);
      }
      sheetDataStore[editSheet] = snapshotDomSheetCells();
      const commitRun = async () => {
        await recalculateActiveSheetFormulas();
        if (
          keepFocus &&
          document.activeElement === td &&
          !sheetCellFocusState.get(td)?.userEdited
        ) {
          syncSheetCellFocusBaseline(td, true);
          refreshFocusedSheetCellPendingSnapshot();
        }
        if (getActiveSheetName() === editSheet) {
          updateSheetSelectionAndAggregates(getActiveSheetCellRef() || td.dataset.cell);
        }
        return true;
      };
      return trackSheetAsyncWork(commitRun());
    }

    async function flushFocusedSheetCellBeforeSwitch() {
      const activeEl = document.activeElement;
      if (activeEl && sheetCellFocusState.has(activeEl)) {
        await commitSheetCellEdit(activeEl, false);
      } else if (
        activeEl &&
        activeEl.nodeType === 1 &&
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
        activeEl &&
        activeEl.nodeType === 1 &&
        activeEl.dataset?.cell &&
        typeof activeEl.blur === "function"
      ) {
        activeEl.blur();
      }
      await drainInFlightSheetWork();
      return committed;
    }

    function bindSheetCellEvents(rootEl) {
      rootEl.querySelectorAll("td[data-cell]").forEach((td) => {
        td.addEventListener("click", () => updateSheetSelectionAndAggregates(td.dataset.cell));
        td.addEventListener("focus", () => {
          updateSheetSelectionAndAggregates(td.dataset.cell);
          const initialText = (td.textContent || "").trim();
          sheetCellFocusState.set(td, {
            initialText,
            initialRaw: td.dataset.formula || initialText,
            pendingSnapshot: captureWorkspaceSnapshot(false),
            snapshotPushed: false,
            userEdited: false,
            sheetName: getActiveSheetName(),
          });
        });
        td.addEventListener("keydown", (ev) => {
          if (ev.key === "Enter") {
            ev.preventDefault();
            td.blur();
          }
        });
        td.addEventListener("blur", () => {
          void commitSheetCellEdit(td, false).catch(() => {});
        });
        td.addEventListener("input", () => {
          const st = sheetCellFocusState.get(td);
          const rawText = (td.textContent || "").trim();
          if (isFormulaStr(rawText)) {
            td.dataset.formula = rawText;
          } else {
            delete td.dataset.formula;
          }
          if (st) {
            st.userEdited = true;
            pushCellUndoSnapshotIfNeeded(td, st, td.dataset.formula || rawText);
          }
          const formulaInput = document.getElementById("formula-input");
          if (formulaInput && getActiveSheetCellRef() === td.dataset.cell) {
            formulaInput.value = rawText;
          }
          updateSheetSelectionAndAggregates(td.dataset.cell);
        });
      });
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
          b.classList.toggle("active", b.dataset.sheetTab === targetSheetName);
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
      ensureSheetGridRowCount,
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
      performSwitchSheetTabAsync,
      switchSheetTab,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__ = createSheetController;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
