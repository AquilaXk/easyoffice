(function (host) {
  "use strict";

  const MAX_UNDO_SNAPSHOTS = 64;

  // Per-page-load client id so the backend keeps ribbon state/undo stacks per browser window.
  function createRibbonSessionId() {
    const cryptoApi = host.crypto || globalThis.crypto;
    if (cryptoApi && typeof cryptoApi.getRandomValues === "function") {
      const bytes = cryptoApi.getRandomValues(new Uint8Array(16));
      return "eo-" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
    }
    return "eo-" + Date.now().toString(36);
  }

  const DOC_RUN_COMMANDS = [
    ["-home-bold", "bold"],
    ["-home-italic", "italic"],
    ["-home-underline", "underline"],
  ];

  function cloneSheets(sheets) {
    const fast = host.__EASYOFFICE_SHEET_GRID_UTILS__?.cloneSheetStore;
    return typeof fast === "function" ? fast(sheets) : structuredClone(sheets);
  }

  function normalizeRibbonStyleKey(rawStyle) {
    return String(rawStyle || "").toLowerCase().includes("hangul") ? "hangul" : "standard";
  }

  // Next preset after the current one (or the second preset when the current value is not a preset).
  function cyclePreset(presets, current) {
    const curIdx = presets.indexOf(current);
    return presets[curIdx >= 0 ? (curIdx + 1) % presets.length : 1];
  }

  function docEffectTrack({ docEditor }) {
    const firstP = docEditor.querySelector(":scope > p:not(#hwp-manuscript-preview)");
    if (!firstP) return;
    const nextOn = firstP.dataset.highlighted !== "true";
    firstP.dataset.highlighted = nextOn ? "true" : "false";
    firstP.style.backgroundColor = nextOn ? "rgba(92, 107, 192, 0.14)" : "";
  }

  function viewEffectTheme() {
    host.__EASYOFFICE_THEME__.toggle();
  }

  function createRibbonHistory(deps) {
    const {
      document,
      invokeIpc,
      docTabStore,
      sheetDataStore,
      sheetRowCounts,
      slideDeckState,
      localRibbonState,
      ribbonCatalog,
      sheetStoreHelpers,
      extractEditableDocumentText,
      getSheetController,
      getWorkspacePanels,
      getDocRuns,
      getDocLink,
      getDocToc,
      getDocPageSetup,
      getDocTable,
      getDocPages,
      getActiveModule,
      getHwpHost,
      setActiveModule,
      getActiveSheetName,
      setActiveSheetName,
      getActiveSheetCellRef,
      getActiveSlideIndex,
      getPdfRotationDeg,
      setPdfRotationDeg,
      getPdfEdit,
      enqueueWorkspaceHistoryOp,
      onModuleApplied,
      getWindowBridge,
    } = deps;

    const { syncLocalRibbonStateFromBackend } = ribbonCatalog;

    const {
      sheetMapHasAnyFormula,
      sheetCellDataChanged,
      computeEvaluatedSheetStore,
    } = sheetStoreHelpers;

    const ribbonSessionId = createRibbonSessionId();
    const ribbonIpcPayload = (fields) => JSON.stringify({ ...fields, sessionId: ribbonSessionId });
    const workspaceUndoStack = [];
    const workspaceRedoStack = [];
    let historyMutationEpoch = 0;

    function getHistoryMutationEpoch() {
      return historyMutationEpoch;
    }

    function pushRawUndoSnapshot(snap) {
      if (!snap) return;
      historyMutationEpoch += 1;
      workspaceUndoStack.push(snap);
      if (workspaceUndoStack.length > MAX_UNDO_SNAPSHOTS) {
        workspaceUndoStack.shift();
      }
      workspaceRedoStack.length = 0;
    }

    function commitDeferredUndoSnapshot(preSnapshot) {
      pushRawUndoSnapshot(preSnapshot);
    }

    // Document names (opened or saved file names) are part of the undoable workspace state; the
    // window bridge owns them because they also feed the window title.
    function readTabTitles() {
      return getWindowBridge ? getWindowBridge().getTitles() : {};
    }
    function applyTabTitles(titles) {
      if (getWindowBridge) getWindowBridge().applyTitles(titles);
    }

    // `layoutOnly` leaves the cells out: a change that touches no cell (the auto filter's) does not
    // pay for copying them, and its undo keeps the cells as they are then.
    function captureWorkspaceSnapshot(isRibbon = false, layoutOnly = false) {
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      workspacePanels.saveActiveDocTabState();
      workspacePanels.syncActiveSlideDomStylesToDeck();
      const curSheet = getActiveSheetName();
      sheetDataStore[curSheet] = sheetController.snapshotDomSheetCells();
      if (!sheetRowCounts[curSheet]) {
        sheetRowCounts[curSheet] = sheetController.getSheetRowCount(curSheet);
      }
      const sheetColumnWidths =
        typeof sheetController.getColumnWidths === "function" ? sheetController.getColumnWidths() : {};
      return {
        isRibbon: Boolean(isRibbon),
        activeModule: getActiveModule(),
        activeSheetName: curSheet,
        activeSheetCellRef: getActiveSheetCellRef(),
        activeSlideIndex: getActiveSlideIndex(),
        pdfRotationDeg: getPdfRotationDeg(),
        pdfStudio: workspacePanels.capturePdfStudioState(),
        viewState: workspacePanels.captureWorkspaceViewState(),
        theme: host.__EASYOFFICE_THEME__.getMode(),
        docTabs: structuredClone(docTabStore),
        layoutOnly,
        sheets: layoutOnly ? undefined : cloneSheets(sheetDataStore),
        sheetRowCounts: structuredClone(sheetRowCounts),
        sheetColumnWidths,
        sheetRules: typeof sheetController.getSheetRules === "function" ? sheetController.getSheetRules() : {},
        sheetLayout: host.__EASYOFFICE_SHEET_LAYOUT__.getAll(),
        sheetOrder: host.__EASYOFFICE_SHEET_TAB_STRIP__.reconcile(Object.keys(sheetDataStore)),
        slides: structuredClone(slideDeckState),
        ribbonState: structuredClone(localRibbonState),
        tabTitles: readTabTitles(),
      };
    }

    function pushUndoSnapshot(isRibbon = false) {
      pushRawUndoSnapshot(captureWorkspaceSnapshot(isRibbon));
    }

    function applyModuleAccent(mod) {
      setActiveModule(mod);
      localRibbonState.activeModule = mod;
      const shell = document.getElementById("app") || document.querySelector(".app-shell");
      if (shell) shell.dataset.module = mod;

      const hwpNotice = document.getElementById("hwp-manuscript-preview");
      const docEditorEl = document.getElementById("doc-editor");
      if (hwpNotice) {
        hwpNotice.hidden =
          mod !== "HWP" || !docEditorEl?.classList.contains("manuscript-grid");
      }
      if (typeof onModuleApplied === "function") onModuleApplied(mod);
    }

    async function activatePaneInternal(mod, skipTabBufferSync) {
      historyMutationEpoch += 1;
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      const curMod = getActiveModule();
      if (!skipTabBufferSync && curMod === "SHEET" && mod !== "SHEET") {
        await sheetController.flushFocusedSheetCellBeforeSwitch();
        sheetController.saveActiveSheetCellsToWorkbook();
      }
      if (!skipTabBufferSync && curMod === "SLIDE" && mod !== "SLIDE") {
        workspacePanels.syncActiveSlideDomStylesToDeck();
      }
      if (!skipTabBufferSync && (curMod === "DOC" || curMod === "HWP") && mod !== curMod) {
        workspacePanels.saveActiveDocTabState();
      }
      applyModuleAccent(mod);
      const panes = {
        DOC: document.getElementById("pane-doc"),
        HWP: getHwpHost?.()?.paneForModule() || document.getElementById("pane-doc"),
        SHEET: document.getElementById("pane-sheet"),
        SLIDE: document.getElementById("pane-slide"),
        PDF: document.getElementById("pane-pdf"),
        CONV: document.getElementById("pane-converter"),
      };
      Object.values(panes).forEach((el) => {
        if (el) el.hidden = true;
      });
      const target = panes[mod] || panes.DOC;
      if (target) target.hidden = false;

      getWindowBridge?.()?.refresh();
      if (!skipTabBufferSync && (mod === "DOC" || mod === "HWP") && mod !== curMod) {
        workspacePanels.restoreDocTabState(mod);
      }
      workspacePanels.renderModuleOutline(mod);
      getSheetController()?.syncSheetFormatToolbar?.();
      void getHwpHost?.()?.onModuleActivated(mod);
    }

    function activatePane(mod, skipTabBufferSync) {
      return enqueueWorkspaceHistoryOp(() => activatePaneInternal(mod, skipTabBufferSync));
    }

    function applyPdfRotationDom(nextRot) {
      setPdfRotationDeg(nextRot);
      const badge = document.getElementById("pdf-rotation-badge");
      const card = document.getElementById("pdf-page-card");
      if (badge) badge.textContent = "회전: " + nextRot + "°";
      if (card) {
        card.style.transform = "rotate(" + nextRot + "deg)";
        card.dataset.rotatedQuarter = nextRot % 180 !== 0 ? "true" : "false";
      }
    }

    // Draws the tabs again: sheets that are gone lose theirs, new ones get one, hidden ones none.
    function syncSheetTabBar(activeSheetName) {
      host.__EASYOFFICE_SHEET_TAB_STRIP__.render({ document, keys: Object.keys(sheetDataStore), active: activeSheetName });
    }

    function restoreSnapshotStores(snap, sheetController) {
      if (snap.docTabs) {
        docTabStore.DOC = snap.docTabs.DOC ? structuredClone(snap.docTabs.DOC) : null;
        docTabStore.HWP = snap.docTabs.HWP ? structuredClone(snap.docTabs.HWP) : null;
      }
      if (snap.sheets) {
        const clonedRowCounts = snap.sheetRowCounts ? structuredClone(snap.sheetRowCounts) : null;
        sheetController.replaceLiveSheetDataStore(cloneSheets(snap.sheets), clonedRowCounts);
      }
      if (snap.sheetColumnWidths && typeof sheetController.replaceColumnWidths === "function") {
        sheetController.replaceColumnWidths(structuredClone(snap.sheetColumnWidths));
      }
      if (snap.sheetRules) host.__EASYOFFICE_SHEET_RULES__.replaceAll(structuredClone(snap.sheetRules), true);
      if (snap.sheetLayout) host.__EASYOFFICE_SHEET_LAYOUT__.replaceAll(structuredClone(snap.sheetLayout));
      if (Array.isArray(snap.sheetOrder)) host.__EASYOFFICE_SHEET_TAB_STRIP__.replaceOrder(snap.sheetOrder);
      if (Array.isArray(snap.slides)) {
        slideDeckState.splice(0, slideDeckState.length, ...structuredClone(snap.slides));
      }
    }

    async function restoreWorkspaceSnapshot(snap, backendSyncRes) {
      if (!snap) return;
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      sheetController.bumpSnapshotEpoch();
      restoreSnapshotStores(snap, sheetController);
      if (snap.ribbonState) {
        Object.assign(localRibbonState, structuredClone(snap.ribbonState));
      }
      if (backendSyncRes) {
        syncLocalRibbonStateFromBackend(localRibbonState, backendSyncRes);
      }
      applyPdfRotationDom(typeof snap.pdfRotationDeg === "number" ? snap.pdfRotationDeg : 0);
      workspacePanels.restorePdfStudioState(snap.pdfStudio);
      workspacePanels.restoreWorkspaceViewState(snap.viewState);

      if (snap.theme) host.__EASYOFFICE_THEME__.setMode(snap.theme);
      localRibbonState.darkThemeEnabled = host.__EASYOFFICE_THEME__.getEffective() === "dark";
      applyTabTitles(snap.tabTitles);

      const targetMod = snap.activeModule || getActiveModule();
      if (targetMod === "DOC" || targetMod === "HWP") {
        workspacePanels.restoreDocTabState(targetMod);
      } else if (docTabStore.DOC) {
        workspacePanels.restoreDocTabState("DOC");
      }
      if (snap.activeSheetName) {
        setActiveSheetName(snap.activeSheetName);
        syncSheetTabBar(snap.activeSheetName);
        sheetController.restoreDomSheetCells(snap.activeSheetName);
        sheetController.updateSheetSelectionAndAggregates(snap.activeSheetCellRef || "A1");
        // The rules are evaluated on the restored cells, not on the page's cells of before.
        if (snap.sheetRules) host.__EASYOFFICE_SHEET_RULES__.notify();
      }
      if (typeof snap.activeSlideIndex === "number" && slideDeckState.length > 0) {
        workspacePanels.renderSlideDeck(Math.min(snap.activeSlideIndex, slideDeckState.length - 1));
      }
      await activatePaneInternal(targetMod, true);
      sheetController.refreshFocusedSheetCellPendingSnapshot();
    }

    function performWorkspaceHistoryStep(isUndo) {
      return enqueueWorkspaceHistoryOp(async () => {
        const sheetController = getSheetController();
        await sheetController.drainInFlightSheetWork();
        const fb = document.getElementById("status-command-feedback");
        const sourceStack = isUndo ? workspaceUndoStack : workspaceRedoStack;
        const destStack = isUndo ? workspaceRedoStack : workspaceUndoStack;
        const emptyMsg = isUndo ? "실행 취소할 작업이 없습니다" : "다시 실행할 작업이 없습니다";
        const ipcCmd = isUndo ? "undo_ribbon_command" : "redo_ribbon_command";
        const doneMsg = isUndo ? "실행 취소" : "다시 실행";
        if (sourceStack.length === 0) {
          if (fb) fb.textContent = emptyMsg;
          return;
        }
        historyMutationEpoch += 1;
        const targetSnap = sourceStack.at(-1);
        const currentSnap = captureWorkspaceSnapshot(Boolean(targetSnap.isRibbon), targetSnap.layoutOnly === true);
        currentSnap.ribbonModule = targetSnap.ribbonModule;
        let evaluatedSheets = null;
        if (
          sheetCellDataChanged(currentSnap.sheets, targetSnap.sheets, false) &&
          sheetMapHasAnyFormula(targetSnap.sheets)
        ) {
          const evalRes = await sheetController.trackSheetAsyncWork(
            computeEvaluatedSheetStore(
              targetSnap.sheets,
              targetSnap.activeSheetName || getActiveSheetName(),
              null
            )
          );
          evaluatedSheets = evalRes.sheets;
        }
        let backendSyncRes = null;
        if (targetSnap.isRibbon === true) {
          const ribbonModule = targetSnap.ribbonModule || targetSnap.activeModule;
          backendSyncRes = await invokeIpc(ipcCmd, ribbonIpcPayload({ module: ribbonModule }));
        }
        sourceStack.pop();
        destStack.push(currentSnap);
        const snapToRestore = evaluatedSheets
          ? { ...targetSnap, sheets: evaluatedSheets }
          : targetSnap;
        await restoreWorkspaceSnapshot(snapToRestore, backendSyncRes);
        if (fb) fb.textContent = doneMsg;
      });
    }

    function performWorkspaceUndo() {
      return performWorkspaceHistoryStep(true);
    }

    function performWorkspaceRedo() {
      return performWorkspaceHistoryStep(false);
    }

    function syncDocStyleBarFromTarget(targetEl) {
      getWorkspacePanels().syncDocStyleBarFromTarget(targetEl);
    }

    const STYLE_FONT_SIZES = ["15px", "18px", "22px", "13.5px"];
    const MAX_DOC_COLUMNS = 3;
    const endsWithAny = (cmdId, suffixes) => suffixes.some((suffix) => cmdId.endsWith(suffix));

    function docEffectHomeStyle({ targetEl }) {
      targetEl.style.fontSize = cyclePreset(STYLE_FONT_SIZES, targetEl.style.fontSize);
      syncDocStyleBarFromTarget(targetEl);
    }

    function docEffectColumns({ docEditor }) {
      const curCols = Number(docEditor.style.columnCount || "1");
      const cols = curCols >= MAX_DOC_COLUMNS ? 1 : curCols + 1;
      docEditor.style.columnCount = cols > 1 ? String(cols) : "";
    }

    // 편집 용지 opens the page setup dialog, 용지 방향 전환 turns the page and 여백 전환 goes to the next margin preset
    // (`doc-page-setup.js`).
    function docEffectPaper() {
      getDocPageSetup().open();
    }

    function docEffectOrientation() {
      getDocPageSetup().turn();
    }

    function docEffectMargins() {
      getDocPageSetup().nextMargins();
    }

    // 이중선 테두리: the table the caret is in.
    function docEffectBorder({ docEditor }) {
      const range = getDocRuns().currentRange();
      const tableEl = range ? host.__EASYOFFICE_DOC_TABLE_GRID__.tableAt(docEditor, range.startContainer) : null;
      if (!tableEl) return;
      tableEl.style.borderStyle = tableEl.style.borderStyle !== "double" ? "double" : "";
    }

    function docEffectModuleSpecific({ docEditor, cmdId, execRes }) {
      const workspacePanels = getWorkspacePanels();
      workspacePanels.applyDocModuleSpecificDomEffect(cmdId, docEditor, execRes);
      workspacePanels.computeSmartStats();
      workspacePanels.renderModuleOutline(getActiveModule());
    }

    // The dialog's levels travel with the command (`options.toc`); the block is made in the page like the other editor effects.
    function docEffectToc({ options }) {
      const workspacePanels = getWorkspacePanels();
      getDocToc().applyOptions(options.toc);
      workspacePanels.computeSmartStats();
      workspacePanels.renderModuleOutline(getActiveModule());
    }

    // 표 만들기: a new table at the caret (else the end of the document), also in a blank document.
    function docEffectInsertTable({ docEditor }) {
      const { firstCell } = host.__EASYOFFICE_DOC_TABLE__.insertTable(docEditor, getDocRuns().currentRange());
      const range = document.createRange?.();
      const sel = host.getSelection?.();
      if (range && sel) {
        range.setStart(firstCell, 0);
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
      }
      getWorkspacePanels().computeSmartStats();
    }

    function docEffectInsertNote({ docEditor, cmdId, execRes }) {
      const workspacePanels = getWorkspacePanels();
      const p = document.createElement("p");
      p.textContent = "[" + (execRes?.summary || cmdId) + "]";
      docEditor.appendChild(p);
      workspacePanels.computeSmartStats();
      workspacePanels.renderModuleOutline(getActiveModule());
    }

    // A document edits links in the link dialog; the embedded HWP surface keeps the note.
    function docEffectInsertLink(args) {
      if (!args.cmdId.startsWith("document-")) docEffectInsertNote(args);
    }

    function docEffectSort({ execRes }) {
      const tbody = document.getElementById("doc-table-body");
      if (!tbody) return;
      const rows = Array.from(tbody.querySelectorAll("tr:not(#doc-table-sum-row)"));
      const asc = Boolean(execRes?.sortAscending);
      rows.sort((a, b) => {
        const aText = (a.cells[0]?.textContent || "").trim();
        const bText = (b.cells[0]?.textContent || "").trim();
        return asc ? aText.localeCompare(bText, "ko") : bText.localeCompare(aText, "ko");
      });
      const sumRow = document.getElementById("doc-table-sum-row");
      for (const r of rows) {
        if (sumRow) sumRow.before(r);
        else tbody.appendChild(r);
      }
    }

    const DOC_EXACT_EFFECTS = {
      "hwp-mod-border": docEffectBorder,
      "doc-mod-toc": docEffectToc,
      "doc-mod-footnote": docEffectModuleSpecific,
      "doc-mod-mailmerge": docEffectModuleSpecific,
      "hwp-mod-header": docEffectModuleSpecific,
      "hwp-mod-field": docEffectModuleSpecific,
      "doc-mod-tableformula": () => getDocTable().recalculate(),
    };
    const DOC_SUFFIX_EFFECTS = {
      "-home-style": docEffectHomeStyle,
      "-review-track": docEffectTrack,
      "-layout-columns": docEffectColumns,
      "-layout-orientation": docEffectOrientation,
      "-layout-margins": docEffectMargins,
      "-layout-paper": docEffectPaper,
      "-insert-chart": docEffectInsertNote,
      "-insert-shape": docEffectInsertNote,
      "-insert-link": docEffectInsertLink,
      "-data-fn-browser": docEffectInsertNote,
      "-review-comment": docEffectInsertNote,
      "-insert-table": docEffectInsertTable,
      "-data-autosum": () => getDocTable().autoSum(),
      "-data-sort": docEffectSort,
    };

    function commandSuffixOf(cmdId) {
      const idx = cmdId.indexOf("-");
      return idx >= 0 ? cmdId.slice(idx) : "";
    }

    function docEditorEffectFor(cmdId) {
      return DOC_EXACT_EFFECTS[cmdId] || DOC_SUFFIX_EFFECTS[commandSuffixOf(cmdId)] || null;
    }

    function applyDocEditorRibbonEffect(docEditor, cmdId, execRes, options) {
      if (!docEditor) return false;
      const targetEl = getWorkspacePanels().getFocusedDocBlock(docEditor);
      const runProp = DOC_RUN_COMMANDS.find(([suffix]) => cmdId.endsWith(suffix));
      if (runProp) {
        // Character formatting applies to the selection only (run model, PD-077).
        getDocRuns().toggle(runProp[1], { undo: false });
        return true;
      }
      const runEffect = docEditorEffectFor(cmdId);
      if (!runEffect) return false;
      runEffect({ docEditor, targetEl, cmdId, execRes, options });
      return true;
    }

    async function prepareRibbonAsyncEffect(cmdId, effectiveModule) {
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      if (cmdId.endsWith("-review-verify") || (cmdId.endsWith("-data-validation") && !cmdId.startsWith("sheet-"))) {
        await workspacePanels.runModuleSecurityAudit(effectiveModule);
        return { kind: "securityAudit", auditOutcome: { ok: true, errorMessage: "" } };
      }
      if (cmdId.endsWith("-review-stats")) {
        const smartRes = await invokeIpc("run_smart_assistant", extractEditableDocumentText());
        return { kind: "smartStats", smartRes };
      }
      if (
        (cmdId === "sheet-data-autosum" || cmdId === "sheet-data-fn-browser") &&
        effectiveModule !== "DOC" &&
        effectiveModule !== "HWP"
      ) {
        const formulaPrep = await sheetController.prepareFormulaBarEvaluation({
          autoSum: cmdId === "sheet-data-autosum",
        });
        return { kind: "sheetFormula", formulaPrep };
      }
      return null;
    }

    const DEFAULT_ZOOM_PERCENT = 100;

    function viewEffectZoom(_cmdId, execRes) {
      // "100% 실제 배율" (Cmd+0) always resets to 100%; the engine reports the same value.
      const zoom = Number(execRes?.zoomPercent || DEFAULT_ZOOM_PERCENT);
      localRibbonState.zoomPercent = zoom;
      const stage = document.querySelector(".canvas-stage");
      if (stage) stage.style.zoom = zoom + "%";
      const zoomLabel = document.getElementById("status-zoom-label");
      if (zoomLabel) zoomLabel.textContent = zoom + "%";
      const slider = document.getElementById("status-zoom-slider");
      if (slider) slider.value = String(zoom);
    }

    // The ribbon bar is gone (PD-098); the command stays in the catalog and keeps the state flag.
    function viewEffectRibbon(_cmdId, execRes) {
      localRibbonState.ribbonCollapsed =
        execRes?.ribbonCollapsed !== undefined ? Boolean(execRes.ribbonCollapsed) : !localRibbonState.ribbonCollapsed;
    }

    function viewEffectSplit(_cmdId, execRes) {
      const stage = document.querySelector(".canvas-stage");
      if (!stage) return;
      const nextSplit =
        execRes?.splitViewEnabled !== undefined ? Boolean(execRes.splitViewEnabled) : stage.dataset.splitView !== "true";
      stage.dataset.splitView = nextSplit ? "true" : "false";
      getWorkspacePanels().syncSplitComparePane(stage);
    }

    function viewEffectManuscript() {
      const docEditorEl = document.getElementById("doc-editor");
      const gridOn = docEditorEl ? !docEditorEl.classList.contains("manuscript-grid") : false;
      localRibbonState.manuscriptGridEnabled = gridOn;
      if (docEditorEl) docEditorEl.classList.toggle("manuscript-grid", gridOn);
      const gridBtn = document.getElementById("btn-toggle-doc-grid");
      if (gridBtn) {
        gridBtn.classList.toggle("active", gridOn);
        gridBtn.setAttribute("aria-pressed", gridOn ? "true" : "false");
      }
      const hwpNotice = document.getElementById("hwp-manuscript-preview");
      if (hwpNotice) {
        hwpNotice.classList.toggle("manuscript-grid", gridOn);
        hwpNotice.hidden = getActiveModule() !== "HWP" || !gridOn;
      }
      getWorkspacePanels().saveActiveDocTabState();
    }

    const applyAuditFromPrepared = (_id, _res, prepared) =>
      getWorkspacePanels().applySecurityAuditDom(prepared?.auditOutcome);
    const applyStatsFromPrepared = (_id, _res, prepared) =>
      getWorkspacePanels().applySmartDiagnoseDom(prepared?.smartRes, false);

    const VIEW_SUFFIX_EFFECTS = {
      "-view-theme": viewEffectTheme,
      "-view-zoom": viewEffectZoom,
      "-view-ribbon": viewEffectRibbon,
      "-view-split": viewEffectSplit,
      "-review-verify": applyAuditFromPrepared,
      "-data-validation": (id, res, prepared) =>
        id.startsWith("sheet-") ? getSheetController().openDataValidationDialog() : applyAuditFromPrepared(id, res, prepared),
      "-review-stats": applyStatsFromPrepared,
      "-manuscript": viewEffectManuscript,
      "-mod-manuscript": viewEffectManuscript,
    };

    function viewRibbonEffectFor(cmdId) {
      return VIEW_SUFFIX_EFFECTS[commandSuffixOf(cmdId)] || (cmdId.endsWith("-manuscript") ? viewEffectManuscript : null);
    }

    // Document pane commands; true when the doc editor handled the command.
    function applyDocumentPaneEffect(cmdId, execRes, options) {
      const curMod = getActiveModule();
      const docEditor = document.getElementById("doc-editor");
      const workspacePanels = getWorkspacePanels();
      const isConvDocCommand = cmdId.startsWith("document-") || cmdId.startsWith("doc-mod-");
      if (curMod === "CONV" && isConvDocCommand) {
        workspacePanels.restoreDocTabState("DOC");
        if (applyDocEditorRibbonEffect(docEditor, cmdId, execRes, options)) {
          workspacePanels.saveDocTabStateForModule("DOC");
          return true;
        }
      }
      if ((curMod === "DOC" || curMod === "HWP") && applyDocEditorRibbonEffect(docEditor, cmdId, execRes, options)) {
        workspacePanels.saveActiveDocTabState();
        return true;
      }
      return false;
    }

    function setSheetSummary(execRes) {
      const out = document.getElementById("sheet-eval-result");
      if (out && execRes?.summary) out.textContent = execRes.summary;
    }

    function applySheetRibbonEffect(cmdId, execRes, preparedEffect) {
      const sheetController = getSheetController();
      // Cell formatting (bold/italic/underline/strike/size) applies to the whole selected range.
      if (sheetController.applySheetFormatCommand(cmdId)) return;
      switch (cmdId) {
        case "sheet-data-autosum":
        case "sheet-data-fn-browser":
          sheetController.commitPreparedFormulaBarEvaluation(preparedEffect?.formulaPrep, null);
          break;
        case "sheet-insert-table":
          sheetController.appendSheetRow();
          break;
        case "sheet-data-sort":
          void sheetController.sortActiveSheetRange();
          setSheetSummary(execRes);
          break;
        case "sheet-mod-pivot":
          sheetController.openPivotDialog();
          break;
        case "sheet-mod-chart":
        case "sheet-insert-chart":
          sheetController.applySheetChartSummary(execRes);
          break;
        case "sheet-mod-condfmt":
          sheetController.openConditionalFormatDialog();
          break;
        case "sheet-mod-freeze":
          void sheetController.runSheetStructure("freeze-selection");
          break;
        default:
          setSheetSummary(execRes);
      }
    }

    const SLIDE_TITLE_TOGGLES = {
      "slide-home-bold": ["fontWeight", "700"],
      "slide-home-italic": ["fontStyle", "italic"],
      "slide-home-underline": ["textDecoration", "underline"],
    };
    const SLIDE_TITLE_SIZES = ["20px", "22px", "24px", "18px"];
    const SLIDE_TITLE_COLORS = ["", "var(--accent-strong)", "var(--accent-light)", ""];
    const SLIDE_PADDINGS = ["20px", "12px", "28px"];
    const SLIDE_TRANSITIONS = ["fade", "push", "zoom", "none"];

    function slideTitleEffect(cmdId, ctx) {
      const { titleZone } = ctx;
      if (!titleZone) return false;
      const toggle = SLIDE_TITLE_TOGGLES[cmdId];
      if (toggle) {
        const [prop, value] = toggle;
        titleZone.style[prop] = titleZone.style[prop] !== value ? value : "";
      } else if (cmdId === "slide-home-style") {
        const presetIdx = SLIDE_TITLE_SIZES.indexOf(cyclePreset(SLIDE_TITLE_SIZES, titleZone.style.fontSize));
        titleZone.style.fontSize = SLIDE_TITLE_SIZES[presetIdx];
        titleZone.style.color = SLIDE_TITLE_COLORS[presetIdx];
      } else {
        return false;
      }
      getWorkspacePanels().syncActiveSlideDomStylesToDeck();
      return true;
    }

    function slideAppendNote(cmdId, execRes, ctx) {
      const { curSlide, curIdx } = ctx;
      const tag = "[" + (execRes?.summary || cmdId) + "]";
      if (curSlide) {
        const prefix = curSlide.lower ? curSlide.lower + " · " : "";
        curSlide.lower = prefix + tag;
      }
      getWorkspacePanels().renderSlideDeck(curIdx);
    }

    function slideSortLower(execRes, ctx) {
      const { curSlide, curIdx } = ctx;
      const asc = Boolean(execRes?.sortAscending);
      const parts = curSlide.lower
        .split("·")
        .map((s) => s.trim())
        .filter(Boolean);
      parts.sort((a, b) => (asc ? a.localeCompare(b, "ko") : b.localeCompare(a, "ko")));
      curSlide.lower = parts.join(" · ");
      getWorkspacePanels().renderSlideDeck(curIdx);
    }

    function slidePresenterMode(ctx) {
      const { stageBox } = ctx;
      if (stageBox) {
        stageBox.dataset.presenterMode = stageBox.dataset.presenterMode !== "true" ? "true" : "false";
        getWorkspacePanels().syncActiveSlideDomStylesToDeck();
      }
      document.getElementById("btn-slide-laser")?.click();
    }

    function slideCycleTransition(ctx) {
      const sel = document.getElementById("slide-transition-select");
      const { curSlide, curIdx } = ctx;
      if (!sel || !curSlide) return;
      sel.value = SLIDE_TRANSITIONS[(SLIDE_TRANSITIONS.indexOf(sel.value) + 1) % SLIDE_TRANSITIONS.length];
      curSlide.transition = sel.value;
      getWorkspacePanels().renderSlideDeck(curIdx);
    }

    const insertSlideObjectHandler = (cmdId, execRes) => {
      getWorkspacePanels().insertSlideStageObject(cmdId, execRes);
      return true;
    };
    const appendSlideNoteHandler = (cmdId, execRes, ctx) => {
      slideAppendNote(cmdId, execRes, ctx);
      return true;
    };

    const SLIDE_CONTENT_HANDLERS = {
      "slide-insert-table": insertSlideObjectHandler,
      "slide-insert-chart": insertSlideObjectHandler,
      "slide-insert-shape": insertSlideObjectHandler,
      "slide-insert-link": appendSlideNoteHandler,
      "slide-data-autosum": appendSlideNoteHandler,
      "slide-data-fn-browser": appendSlideNoteHandler,
      "slide-review-comment": appendSlideNoteHandler,
      "slide-data-sort": (_cmdId, execRes, ctx) => {
        if (!ctx.curSlide?.lower) return false;
        slideSortLower(execRes, ctx);
        return true;
      },
      "slide-mod-presenter": (_cmdId, _execRes, ctx) => {
        slidePresenterMode(ctx);
        return true;
      },
      "slide-mod-transition": (_cmdId, _execRes, ctx) => {
        slideCycleTransition(ctx);
        return true;
      },
    };

    function slideContentEffect(cmdId, execRes, ctx) {
      const handler = SLIDE_CONTENT_HANDLERS[cmdId];
      return typeof handler === "function" ? handler(cmdId, execRes, ctx) : false;
    }

    const toggleStageOutline = ({ stageBox }) => {
      stageBox.style.outline = stageBox.style.outline ? "" : "2px dashed var(--accent-strong)";
    };

    const SLIDE_STAGE_HANDLERS = {
      "slide-layout-columns": ({ stageBox, lowerZone }, cmdId, execRes) => {
        if (!lowerZone) {
          stageBox.dataset.layoutState = execRes?.summary || cmdId;
          return;
        }
        const curCols = Number(lowerZone.style.columnCount || "1");
        const cols = curCols >= MAX_DOC_COLUMNS ? 1 : curCols + 1;
        lowerZone.style.columnCount = cols > 1 ? String(cols) : "";
      },
      "slide-layout-margins": ({ stageBox }) => {
        stageBox.style.padding = cyclePreset(SLIDE_PADDINGS, stageBox.style.padding);
      },
      "slide-mod-snap": toggleStageOutline,
      "slide-review-track": toggleStageOutline,
    };

    function slideStageEffect(cmdId, execRes, ctx) {
      const handler = SLIDE_STAGE_HANDLERS[cmdId];
      if (typeof handler === "function") handler(ctx, cmdId, execRes);
      else ctx.stageBox.dataset.layoutState = execRes?.summary || cmdId;
      getWorkspacePanels().syncActiveSlideDomStylesToDeck();
    }

    function applySlideRibbonEffect(cmdId, execRes) {
      const curIdx = getActiveSlideIndex();
      const ctx = {
        lowerZone: document.getElementById("slide-lower-zone"),
        stageBox: document.getElementById("slide-stage-box"),
        titleZone: document.getElementById("slide-title-zone"),
        curIdx,
        curSlide: slideDeckState[curIdx],
      };
      const handled = slideTitleEffect(cmdId, ctx) || slideContentEffect(cmdId, execRes, ctx);
      if (!handled && ctx.stageBox) slideStageEffect(cmdId, execRes, ctx);
      const metaStatus = document.getElementById("slide-meta-status");
      if (metaStatus && execRes?.summary && cmdId !== "slide-mod-transition") {
        metaStatus.textContent =
          "슬라이드 " + (getActiveSlideIndex() + 1) + " / " + slideDeckState.length + " · " + execRes.summary;
      }
    }

    const PDF_BADGE_EFFECTS = {
      "pdf-mod-splitmerge": { cls: "pdf-splitmerge-badge", dataKey: "pdfSplitmerge", fallback: "페이지 분할·병합 준비 완료" },
    };

    function applyPdfBadge(spec, execRes) {
      const card = document.getElementById("pdf-page-card");
      if (!card) return;
      const existing = card.querySelector("." + spec.cls);
      const badge = existing || document.createElement("span");
      badge.className = spec.cls;
      badge.dataset[spec.dataKey] = "true";
      badge.textContent = execRes?.summary || spec.fallback;
      if (!existing) card.appendChild(badge);
    }

    async function applyRibbonDomSideEffects(cmdId, execRes, preparedEffect, options = {}) {
      const viewEffect = viewRibbonEffectFor(cmdId);
      if (viewEffect) {
        viewEffect(cmdId, execRes, preparedEffect);
        return;
      }
      if (applyDocumentPaneEffect(cmdId, execRes, options)) return;
      if (cmdId.startsWith("sheet-")) {
        applySheetRibbonEffect(cmdId, execRes, preparedEffect);
        return;
      }
      if (cmdId.startsWith("slide-")) {
        applySlideRibbonEffect(cmdId, execRes);
        return;
      }
      if (cmdId === "pdf-mod-rotate") {
        const rotated = typeof execRes?.pdfRotationDegrees === "number" ? execRes.pdfRotationDegrees : (getPdfRotationDeg() + 90) % 360;
        applyPdfRotationDom(rotated);
        getWorkspacePanels().applyPdfStudioFormState();
        return;
      }
      if (cmdId === "pdf-mod-tooffice") {
        await activatePaneInternal("CONV", false);
        return;
      }
      if (cmdId === "pdf-mod-highlight") getWorkspacePanels().applyPdfHighlight(options.highlight);
      if (cmdId === "pdf-mod-splitmerge" && getPdfEdit?.()?.isActive()) {
        // On the opened file the shell asks and does it; the badge tells what happened.
        applyPdfBadge(PDF_BADGE_EFFECTS[cmdId], { summary: await getPdfEdit().splitMerge() });
        return;
      }
      if (PDF_BADGE_EFFECTS[cmdId]) applyPdfBadge(PDF_BADGE_EFFECTS[cmdId], execRes);
      if (cmdId.startsWith("pdf-")) getWorkspacePanels().applyPdfStudioFormState();
    }

    // HWP ribbon commands edit the text-only fallback editor; the embedded editor has its own toolbar.
    function isHwpEmbedCommandBlocked(cmdId, mod) {
      const hwpHost = getHwpHost?.();
      if (mod !== "HWP" || !hwpHost?.isEmbedActive() || cmdId.endsWith("-view-theme")) return false;
      return true;
    }

    // 대화상자를 여는 명령은 상태 줄을 바꾸지 않는다.
    const SHEET_PAGE_COMMANDS = new Set(["sheet-layout-paper", "sheet-layout-orientation", "sheet-layout-margins"]);
    const DIALOG_COMMANDS = new Set(["sheet-mod-pivot", "sheet-mod-condfmt", "sheet-mod-freeze", "pdf-mod-highlight", "doc-mod-toc"]);

    function executeTrackedRibbonCommand(cmdId, options = {}) {
      // The highlight command asks for its phrase, kind and color first; the dialog's 확인 runs it again with them.
      if (cmdId === "pdf-mod-highlight" && !options.highlight) {
        getWorkspacePanels().openPdfHighlightDialog();
        return Promise.resolve(null);
      }
      // The table of contents asks for its levels first and the dialog's 확인 runs the command with them; the update only rebuilds the page's block.
      if (cmdId === "doc-mod-toc" && !options.toc) return Promise.resolve(getDocToc().open());
      if (cmdId === "doc-mod-toc-update") return Promise.resolve(getDocToc().update());
      // The sheet's paper, orientation and margin commands are the page setup dialog (`sheet-page.js`).
      if (SHEET_PAGE_COMMANDS.has(cmdId)) return Promise.resolve(getSheetController().runSheetStructure("page-setup"));
      // The page break splits the block at the caret in the page, like Ctrl+Enter in the editor.
      if (cmdId === "doc-mod-page-break") return Promise.resolve(getDocPages().insertPageBreak());
      // The link dialog opens at once, while the selection is still in the editor; the command itself still runs.
      if (cmdId === "document-insert-link") getDocLink().open();
      return enqueueWorkspaceHistoryOp(async () => {
        const sheetController = getSheetController();
        await sheetController.commitPendingFocusedSheetCell(true);
        const preSnapshot = captureWorkspaceSnapshot(true);
        const effectiveModule = options.targetModule || getActiveModule();
        if (isHwpEmbedCommandBlocked(cmdId, effectiveModule)) return null;
        const preparedEffect = await prepareRibbonAsyncEffect(cmdId, effectiveModule);
        const currentStyle = normalizeRibbonStyleKey(localRibbonState.ribbonStyle);
        const execRes = await invokeIpc(
          "execute_ribbon_command",
          ribbonIpcPayload({ commandId: cmdId })
        );
        preSnapshot.ribbonModule = execRes?.activeModule || effectiveModule;
        syncLocalRibbonStateFromBackend(localRibbonState, execRes);
        localRibbonState.ribbonStyle = currentStyle;
        localRibbonState.darkThemeEnabled = host.__EASYOFFICE_THEME__.getEffective() === "dark";
        if (options.targetModule && options.targetModule !== getActiveModule()) {
          await activatePaneInternal(options.targetModule, false);
        } else {
          localRibbonState.activeModule = getActiveModule();
        }
        await applyRibbonDomSideEffects(cmdId, execRes, preparedEffect, options);
        commitDeferredUndoSnapshot(preSnapshot);
        sheetController.refreshFocusedSheetCellPendingSnapshot();
        const fb = document.getElementById("status-command-feedback");
        if (fb && execRes && !options.skipFeedback && !DIALOG_COMMANDS.has(cmdId)) {
          const fallbackMsg = options.fallbackTitle ? "명령 실행: " + options.fallbackTitle : "";
          fb.textContent = execRes.summary || fallbackMsg;
        }
        return execRes;
      });
    }

    // 형광펜·밑줄 주석 대화상자: 확인은 입력을 명령에 실어 실행하고(실행 취소 한 단계), 빈 문구는 주석을 지웁니다.
    function bindPdfHighlightDialog() {
      const dialog = document.getElementById("pdf-highlight-dialog");
      const apply = () => {
        const value = (id) => document.getElementById(id)?.value ?? "";
        const highlight = {
          text: value("pdf-highlight-text").trim(),
          kind: value("pdf-highlight-kind-select"),
          color: value("pdf-highlight-color-select"),
        };
        dialog?.close?.();
        void executeTrackedRibbonCommand("pdf-mod-highlight", { highlight }).catch(() => {});
      };
      document.getElementById("btn-pdf-highlight-apply")?.addEventListener("click", apply);
    }

    return {
      enqueueWorkspaceHistoryOp,
      getHistoryMutationEpoch,
      pushRawUndoSnapshot,
      commitDeferredUndoSnapshot,
      captureWorkspaceSnapshot,
      pushUndoSnapshot,
      pushRibbonUndoSnapshot: pushUndoSnapshot,
      restoreWorkspaceSnapshot,
      performWorkspaceUndo,
      performWorkspaceRedo,
      applyModuleAccent,
      activatePaneInternal,
      activatePane,
      executeTrackedRibbonCommand,
      bindPdfHighlightDialog,
    };
  }

  host.__EASYOFFICE_CREATE_RIBBON_HISTORY__ = createRibbonHistory;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
