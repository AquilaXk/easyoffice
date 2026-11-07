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

  function docEffectPaper({ docEditor }) {
    docEditor.dataset.paperSize = docEditor.dataset.paperSize === "Letter" ? "A4" : "Letter";
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
      getActiveModule,
      getHwpHost,
      setActiveModule,
      getActiveRibbonCategory,
      setActiveRibbonCategory,
      getActiveSheetName,
      setActiveSheetName,
      getActiveSheetCellRef,
      getActiveSlideIndex,
      getPdfRotationDeg,
      setPdfRotationDeg,
      getShortcutModifier,
      enqueueWorkspaceHistoryOp,
      onModuleApplied,
      getWindowBridge,
    } = deps;

    const {
      MODULE_LABELS,
      MODULE_SPECS,
      CONTEXTUAL_TOOL_BADGES,
      RIBBON_STYLE_LABELS,
      RIBBON_CATEGORY_ORDER,
      syncLocalRibbonStateFromBackend,
      formatPlatformShortcut,
      resolveRibbonShortcutLabel,
    } = ribbonCatalog;

    const {
      sheetMapHasAnyFormula,
      sheetCellDataChanged,
      computeEvaluatedSheetStore,
    } = sheetStoreHelpers;

    const ribbonSessionId = createRibbonSessionId();
    const ribbonIpcPayload = (fields) => JSON.stringify({ ...fields, sessionId: ribbonSessionId });
    const workspaceUndoStack = [];
    const workspaceRedoStack = [];
    let ribbonRenderSeq = 0;
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

    function captureWorkspaceSnapshot(isRibbon = false) {
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      workspacePanels.saveActiveDocTabState();
      workspacePanels.syncActiveSlideDomStylesToDeck();
      const curSheet = getActiveSheetName();
      sheetDataStore[curSheet] = sheetController.snapshotDomSheetCells();
      if (!sheetRowCounts[curSheet]) {
        sheetRowCounts[curSheet] = sheetController.getSheetRowCount(curSheet);
      }
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
        sheets: cloneSheets(sheetDataStore),
        sheetRowCounts: structuredClone(sheetRowCounts),
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

      const statusMod = document.getElementById("status-module");
      const statusSpec = document.getElementById("status-format-spec");
      if (statusMod) statusMod.textContent = "모듈: " + (MODULE_LABELS[mod] || mod);
      if (statusSpec) statusSpec.textContent = "포맷: " + (MODULE_SPECS[mod] || "");

      const ctxBadge = document.getElementById("contextual-tool-badge");
      if (ctxBadge) ctxBadge.textContent = CONTEXTUAL_TOOL_BADGES[mod] || "표·개체 도구";

      const hwpNotice = document.getElementById("hwp-manuscript-preview");
      const docEditorEl = document.getElementById("doc-editor");
      if (hwpNotice) {
        hwpNotice.hidden =
          mod !== "HWP" || !docEditorEl?.classList.contains("manuscript-grid");
      }
      if (typeof onModuleApplied === "function") onModuleApplied(mod);
    }

    function applyRibbonStyleDom(rawStyle, categoriesOverride) {
      const styleKey = normalizeRibbonStyleKey(rawStyle);
      localRibbonState.ribbonStyle = styleKey;
      const btnRibbonStyle = document.getElementById("btn-ribbon-style");
      if (btnRibbonStyle) {
        btnRibbonStyle.textContent =
          styleKey === "hangul" ? "리본: 한글(HWP) 7탭" : "리본: 표준 오피스";
      }
      const labels =
        categoriesOverride || RIBBON_STYLE_LABELS[styleKey] || RIBBON_STYLE_LABELS.standard;
      let nextActiveCat = getActiveRibbonCategory();
      document.querySelectorAll("#ribbon-tabs .ribbon-btn").forEach((b, idx) => {
        if (labels[idx]) b.textContent = labels[idx];
        if (RIBBON_CATEGORY_ORDER[idx]) {
          b.dataset.category = RIBBON_CATEGORY_ORDER[idx];
          if (b.classList.contains("active")) {
            nextActiveCat = RIBBON_CATEGORY_ORDER[idx];
          }
        }
      });
      return nextActiveCat;
    }

    function switchRibbonStyleTracked() {
      return enqueueWorkspaceHistoryOp(async () => {
        const sheetController = getSheetController();
        await sheetController.commitPendingFocusedSheetCell(true);
        const preSnapshot = captureWorkspaceSnapshot(false);
        const curStyle = normalizeRibbonStyleKey(localRibbonState.ribbonStyle);
        const nextStyle = curStyle === "standard" ? "hangul" : "standard";
        const res = await invokeIpc("switch_ribbon_style", ribbonIpcPayload({ style: nextStyle }));
        const nextActiveCat = applyRibbonStyleDom(nextStyle, res?.categories);
        await renderRibbonCategoryTools(nextActiveCat);
        commitDeferredUndoSnapshot(preSnapshot);
        sheetController.refreshFocusedSheetCellPendingSnapshot();
      });
    }

    function createRibbonToolButton(cmd) {
      const btn = document.createElement("button");
      btn.className = "ribbon-tool-btn";
      btn.dataset.commandId = cmd.id;
      const span = document.createElement("span");
      span.textContent = cmd.title || "";
      btn.appendChild(span);
      const boundShortcut = resolveRibbonShortcutLabel(cmd.id, cmd.shortcut);
      if (boundShortcut) {
        const small = document.createElement("small");
        const activeModKey = typeof getShortcutModifier === "function" ? getShortcutModifier() : "⌘";
        small.textContent = "(" + formatPlatformShortcut(boundShortcut, activeModKey) + ")";
        btn.appendChild(document.createTextNode(" "));
        btn.appendChild(small);
      }
      btn.addEventListener("click", () => {
        void executeTrackedRibbonCommand(cmd.id, { fallbackTitle: cmd.title }).catch(() => {});
      });
      return btn;
    }

    async function renderRibbonCategoryTools(categoryKey) {
      const seq = ribbonRenderSeq + 1;
      ribbonRenderSeq = seq;
      setActiveRibbonCategory(categoryKey || "home");
      const catKey = getActiveRibbonCategory();
      localRibbonState.activeCategory = catKey;
      document.querySelectorAll("#ribbon-tabs .ribbon-btn").forEach((b) => {
        const isAct = b.dataset.category === catKey;
        b.classList.toggle("active", isAct);
        b.setAttribute("role", "tab");
        b.setAttribute("aria-selected", isAct ? "true" : "false");
      });
      const strip = document.getElementById("ribbon-tool-strip");
      if (!strip) return;
      const curMod = getActiveModule();
      const modKey = curMod === "CONV" ? "DOC" : curMod;
      const res = await invokeIpc(
        "get_ribbon_category_commands",
        JSON.stringify({ module: modKey, category: catKey })
      );
      if (seq !== ribbonRenderSeq) return;
      const cmds = (res && Array.isArray(res.commands) && res.commands) || [];
      strip.innerHTML = "";
      cmds.forEach((cmd) => strip.appendChild(createRibbonToolButton(cmd)));
      getSheetController()?.syncSheetFormatToolbar?.();
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
      await renderRibbonCategoryTools(getActiveRibbonCategory()).catch(() => {});
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

    function createSheetTabButton(shKey) {
      const newTab = document.createElement("button");
      newTab.className = "sheet-tab-btn";
      newTab.setAttribute("role", "tab");
      newTab.dataset.sheetTab = shKey;
      const numMatch = /^Sheet(\d+)$/.exec(shKey);
      newTab.textContent = numMatch ? "시트 " + numMatch[1] : shKey;
      return newTab;
    }

    // Drops tabs for sheets that no longer exist, adds missing ones and marks the active tab.
    function syncSheetTabBar(activeSheetName) {
      const sheetTabBar = document.getElementById("sheet-tab-bar");
      if (!sheetTabBar) return;
      const addSheetBtn = document.getElementById("btn-add-sheet-tab");
      const tabSelector = ".sheet-tab-btn[data-sheet-tab]";
      const knownKeys = new Set(["Sheet1", "Sheet2", ...Object.keys(sheetDataStore)]);
      for (const btn of sheetTabBar.querySelectorAll(tabSelector)) {
        if (!knownKeys.has(btn.dataset.sheetTab)) btn.remove();
      }
      const renderedKeys = new Set(Array.from(sheetTabBar.querySelectorAll(tabSelector)).map((btn) => btn.dataset.sheetTab));
      for (const shKey of Object.keys(sheetDataStore)) {
        if (!renderedKeys.has(shKey) && addSheetBtn) addSheetBtn.before(createSheetTabButton(shKey));
      }
      for (const btn of sheetTabBar.querySelectorAll(tabSelector)) {
        const isAct = btn.dataset.sheetTab === activeSheetName;
        btn.classList.toggle("active", isAct);
        btn.setAttribute("role", "tab");
        btn.setAttribute("aria-selected", isAct ? "true" : "false");
      }
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
      const targetStyle = normalizeRibbonStyleKey(
        snap.ribbonState?.ribbonStyle || localRibbonState.ribbonStyle
      );
      if (snap.ribbonState) {
        Object.assign(localRibbonState, structuredClone(snap.ribbonState));
      }
      if (backendSyncRes) {
        syncLocalRibbonStateFromBackend(localRibbonState, backendSyncRes);
      }
      await invokeIpc("switch_ribbon_style", ribbonIpcPayload({ style: targetStyle })).catch(
        () => null
      );
      applyRibbonStyleDom(targetStyle);
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
        sheetController.updateSheetSelectionAndAggregates(snap.activeSheetCellRef || "F2");
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
        const donePrefix = isUndo ? "실행 취소 완료 (" : "다시 실행 완료 (";
        if (sourceStack.length === 0) {
          if (fb) fb.textContent = emptyMsg;
          return;
        }
        historyMutationEpoch += 1;
        const targetSnap = sourceStack.at(-1);
        const currentSnap = captureWorkspaceSnapshot(Boolean(targetSnap.isRibbon));
        currentSnap.ribbonModule = targetSnap.ribbonModule;
        let evaluatedSheets = null;
        if (
          sheetCellDataChanged(currentSnap.sheets, targetSnap.sheets) &&
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
        if (fb) fb.textContent = donePrefix + getActiveModule() + ")";
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
    const PAGE_PADDINGS = ["28px 36px", "16px 20px", "36px 52px"];
    const MAX_DOC_COLUMNS = 3;
    const LANDSCAPE_MAX_WIDTH = "980px";
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

    function docEffectOrientation({ docEditor }) {
      const landscape = docEditor.style.maxWidth !== LANDSCAPE_MAX_WIDTH;
      docEditor.style.maxWidth = landscape ? LANDSCAPE_MAX_WIDTH : "";
    }

    function docEffectMargins({ docEditor }) {
      docEditor.style.padding = cyclePreset(PAGE_PADDINGS, docEditor.style.padding);
    }

    function docEffectBorder({ docEditor }) {
      const tableEl = docEditor.querySelector("#doc-table") || document.getElementById("doc-table");
      if (!tableEl) return;
      tableEl.style.borderStyle = tableEl.style.borderStyle !== "double" ? "double" : "";
    }

    function docEffectModuleSpecific({ docEditor, cmdId, execRes }) {
      const workspacePanels = getWorkspacePanels();
      workspacePanels.applyDocModuleSpecificDomEffect(cmdId, docEditor, execRes);
      workspacePanels.computeSmartStats();
      workspacePanels.renderModuleOutline(getActiveModule());
    }

    function docEffectInsertNote({ docEditor, cmdId, execRes }) {
      const workspacePanels = getWorkspacePanels();
      const p = document.createElement("p");
      p.textContent = "[" + (execRes?.summary || cmdId) + "]";
      docEditor.appendChild(p);
      workspacePanels.computeSmartStats();
      workspacePanels.renderModuleOutline(getActiveModule());
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

    const MODULE_SPECIFIC_DOC_COMMANDS = new Set(["doc-mod-toc", "doc-mod-footnote", "doc-mod-mailmerge", "hwp-mod-header", "hwp-mod-field"]);
    const INSERT_NOTE_SUFFIXES = ["-insert-chart", "-insert-shape", "-insert-link", "-data-fn-browser", "-review-comment"];

    // First matching entry wins; the order mirrors the original command precedence.
    const DOC_EDITOR_EFFECTS = [
      [(id) => id.endsWith("-home-style"), docEffectHomeStyle],
      [(id) => id.endsWith("-review-track"), docEffectTrack],
      [(id) => id.endsWith("-layout-columns"), docEffectColumns],
      [(id) => id.endsWith("-layout-orientation"), docEffectOrientation],
      [(id) => id.endsWith("-layout-margins"), docEffectMargins],
      [(id) => id.endsWith("-layout-paper"), docEffectPaper],
      [(id) => id === "hwp-mod-border", docEffectBorder],
      [(id) => MODULE_SPECIFIC_DOC_COMMANDS.has(id), docEffectModuleSpecific],
      [(id) => endsWithAny(id, INSERT_NOTE_SUFFIXES), docEffectInsertNote],
      [(id) => id.endsWith("-insert-table"), () => getWorkspacePanels().appendDocTableRow()],
      [(id) => id.endsWith("-data-autosum") || id === "doc-mod-tableformula", () => getWorkspacePanels().computeDocTableSummary()],
      [(id) => id.endsWith("-data-sort"), docEffectSort],
    ];

    function docEditorEffectFor(cmdId) {
      for (const [matches, run] of DOC_EDITOR_EFFECTS) {
        if (matches(cmdId)) return run;
      }
      return null;
    }

    function applyDocEditorRibbonEffect(docEditor, cmdId, execRes) {
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
      runEffect({ docEditor, targetEl, cmdId, execRes });
      return true;
    }

    async function prepareRibbonAsyncEffect(cmdId, effectiveModule) {
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      if (cmdId.endsWith("-review-verify") || cmdId.endsWith("-data-validation")) {
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

    const ZOOM_CYCLE = { 100: 125, 125: 150, 150: 90 };
    const DEFAULT_ZOOM_PERCENT = 100;

    function viewEffectTheme() {
      host.__EASYOFFICE_THEME__.toggle();
    }

    function viewEffectZoom(_cmdId, execRes) {
      let zoom = Number(execRes?.zoomPercent || 0);
      if (!zoom) {
        const curZoom = Number(localRibbonState.zoomPercent || DEFAULT_ZOOM_PERCENT);
        zoom = ZOOM_CYCLE[curZoom] ?? DEFAULT_ZOOM_PERCENT;
        localRibbonState.zoomPercent = zoom;
      }
      const stage = document.querySelector(".canvas-stage");
      if (stage) stage.style.zoom = zoom + "%";
      const zoomLabel = document.getElementById("status-zoom-label");
      if (zoomLabel) zoomLabel.textContent = zoom + "%";
      const slider = document.getElementById("status-zoom-slider");
      if (slider) slider.value = String(zoom);
    }

    function viewEffectRibbon(_cmdId, execRes) {
      const strip = document.getElementById("ribbon-tool-strip");
      if (!strip) return;
      if (execRes?.ribbonCollapsed !== undefined) {
        strip.hidden = Boolean(execRes.ribbonCollapsed);
        return;
      }
      strip.hidden = !strip.hidden;
      localRibbonState.ribbonCollapsed = strip.hidden;
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

    // First matching entry wins and ends the dispatch (these commands have no module-specific follow-up).
    const VIEW_RIBBON_EFFECTS = [
      [(id) => id.endsWith("-view-theme"), viewEffectTheme],
      [(id) => id.endsWith("-view-zoom"), viewEffectZoom],
      [(id) => id.endsWith("-view-ribbon"), viewEffectRibbon],
      [(id) => id.endsWith("-view-split"), viewEffectSplit],
      [(id) => endsWithAny(id, ["-review-verify", "-data-validation"]), (_id, _res, prepared) => getWorkspacePanels().applySecurityAuditDom(prepared?.auditOutcome)],
      [(id) => id.endsWith("-review-stats"), (_id, _res, prepared) => getWorkspacePanels().applySmartDiagnoseDom(prepared?.smartRes, false)],
      [(id) => id === "hwp-mod-manuscript" || id.endsWith("-manuscript"), viewEffectManuscript],
    ];

    // Document pane commands; true when the doc editor handled the command.
    function applyDocumentPaneEffect(cmdId, execRes) {
      const curMod = getActiveModule();
      const docEditor = document.getElementById("doc-editor");
      const workspacePanels = getWorkspacePanels();
      const isConvDocCommand = cmdId.startsWith("document-") || cmdId.startsWith("doc-mod-");
      if (curMod === "CONV" && isConvDocCommand) {
        workspacePanels.restoreDocTabState("DOC");
        if (applyDocEditorRibbonEffect(docEditor, cmdId, execRes)) {
          workspacePanels.saveDocTabStateForModule("DOC");
          return true;
        }
      }
      if ((curMod === "DOC" || curMod === "HWP") && applyDocEditorRibbonEffect(docEditor, cmdId, execRes)) {
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
          sheetController.sortActiveSheetByColumnB();
          setSheetSummary(execRes);
          break;
        case "sheet-mod-pivot":
          sheetController.applySheetPivotOrChartSummary("pivot", execRes);
          break;
        case "sheet-mod-chart":
        case "sheet-insert-chart":
          sheetController.applySheetPivotOrChartSummary("chart", execRes);
          break;
        case "sheet-mod-condfmt":
          sheetController.toggleSheetConditionalFormatting(!document.querySelector("#pane-sheet td.condfmt-high"));
          setSheetSummary(execRes);
          break;
        case "sheet-mod-freeze":
          sheetController.toggleSheetFreezePanes(!document.querySelector("#pane-sheet thead.frozen-header"));
          setSheetSummary(execRes);
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
    const SLIDE_INSERT_OBJECT_COMMANDS = new Set(["slide-insert-table", "slide-insert-chart", "slide-insert-shape"]);
    const SLIDE_NOTE_COMMANDS = new Set(["slide-insert-link", "slide-data-autosum", "slide-data-fn-browser", "slide-review-comment"]);

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

    function slideContentEffect(cmdId, execRes, ctx) {
      if (SLIDE_INSERT_OBJECT_COMMANDS.has(cmdId)) {
        getWorkspacePanels().insertSlideStageObject(cmdId, execRes);
      } else if (SLIDE_NOTE_COMMANDS.has(cmdId)) {
        slideAppendNote(cmdId, execRes, ctx);
      } else if (cmdId === "slide-data-sort" && ctx.curSlide?.lower) {
        slideSortLower(execRes, ctx);
      } else if (cmdId === "slide-mod-presenter") {
        slidePresenterMode(ctx);
      } else if (cmdId === "slide-mod-transition") {
        slideCycleTransition(ctx);
      } else {
        return false;
      }
      return true;
    }

    function slideStageEffect(cmdId, execRes, ctx) {
      const { stageBox, lowerZone } = ctx;
      if (cmdId === "slide-layout-columns" && lowerZone) {
        const curCols = Number(lowerZone.style.columnCount || "1");
        const cols = curCols >= MAX_DOC_COLUMNS ? 1 : curCols + 1;
        lowerZone.style.columnCount = cols > 1 ? String(cols) : "";
      } else if (cmdId === "slide-layout-margins") {
        stageBox.style.padding = cyclePreset(SLIDE_PADDINGS, stageBox.style.padding);
      } else if (cmdId === "slide-mod-snap" || cmdId === "slide-review-track") {
        stageBox.style.outline = stageBox.style.outline ? "" : "2px dashed var(--accent-strong)";
      } else {
        stageBox.dataset.layoutState = execRes?.summary || cmdId;
      }
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
      "pdf-mod-highlight": { cls: "pdf-highlight-badge", dataKey: "pdfHighlight", fallback: "형광펜 주석 강조 적용" },
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

    async function applyRibbonDomSideEffects(cmdId, execRes, preparedEffect) {
      const viewEffect = VIEW_RIBBON_EFFECTS.find(([matches]) => matches(cmdId));
      if (viewEffect) {
        viewEffect[1](cmdId, execRes, preparedEffect);
        return;
      }
      if (applyDocumentPaneEffect(cmdId, execRes)) return;
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
      if (PDF_BADGE_EFFECTS[cmdId]) applyPdfBadge(PDF_BADGE_EFFECTS[cmdId], execRes);
      if (cmdId.startsWith("pdf-")) getWorkspacePanels().applyPdfStudioFormState();
    }

    // HWP ribbon commands edit the text-only fallback editor; the embedded editor has its own toolbar.
    function isHwpEmbedCommandBlocked(cmdId, mod) {
      const hwpHost = getHwpHost?.();
      if (mod !== "HWP" || !hwpHost?.isEmbedActive() || cmdId.endsWith("-view-theme")) return false;
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = hwpHost.toolHint;
      return true;
    }

    function executeTrackedRibbonCommand(cmdId, options = {}) {
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
        await applyRibbonDomSideEffects(cmdId, execRes, preparedEffect);
        commitDeferredUndoSnapshot(preSnapshot);
        sheetController.refreshFocusedSheetCellPendingSnapshot();
        const fb = document.getElementById("status-command-feedback");
        if (fb && execRes && !options.skipFeedback) {
          const fallbackMsg = options.fallbackTitle ? "명령 실행: " + options.fallbackTitle : "";
          fb.textContent = execRes.summary || fallbackMsg;
        }
        return execRes;
      });
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
      switchRibbonStyleTracked,
      renderRibbonCategoryTools,
      activatePaneInternal,
      activatePane,
      executeTrackedRibbonCommand,
    };
  }

  host.__EASYOFFICE_CREATE_RIBBON_HISTORY__ = createRibbonHistory;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
