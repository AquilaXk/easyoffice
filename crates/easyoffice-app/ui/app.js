(function (host) {
  "use strict";

  const document = host.document;
  const URL = host.URL;

  const ribbonCatalog = host.__EASYOFFICE_RIBBON_CATALOG__;
  const desktopBridge = host.__EASYOFFICE_DESKTOP_BRIDGE__;
  const {
    RIBBON_CATEGORY_TOOLS, MODULE_SHORT_BADGES, DEFAULT_HWP_EDITOR_NODES,
    createEmptySlideStyleFields, createInitialSlideDeckState, createDefaultRibbonState, formatPlatformShortcut,
  } = ribbonCatalog;

  const slideDeckState = createInitialSlideDeckState();
  const localRibbonState = createDefaultRibbonState();

  let activeModule = "DOC";
  let activeSheetName = "Sheet1";
  let activeSheetCellRef = "A1";
  let activeSlideIndex = 0;
  let slideLaserActive = false;
  let pdfRotationDeg = 0;
  let activeShortcutModifier = "⌘";
  let workspaceHistoryQueue = Promise.resolve();

  const docTabStore = { DOC: null, HWP: null };

  const sheetDataStore = Object.assign(Object.create(null), { Sheet1: null });
  const sheetRowCounts = Object.create(null);
  let workspaceSession = null, copilotUi = null;
  let docIme = null, docPaste = null, docRunsUi = null, docLink = null, docToc = null, docTable = null, docPages = null, docPageFrame = null, docPageSetup = null, hwpHost = null, hwpSession = null, windowBridge = null;
  const icons = host.__EASYOFFICE_ICONS__;

  function chainWorkspaceQueue(opFn) {
    const run = workspaceHistoryQueue.then(opFn, opFn);
    workspaceHistoryQueue = run.then(() => {}, () => {});
    return run;
  }

  function enqueueWorkspaceHistoryOp(opFn) {
    const run = chainWorkspaceQueue(opFn);
    workspaceSession?.noteActivity();
    return run;
  }

  const { parseCellRefParts, normalizeFormattedNumericCell, localScrubPii } = host.__EASYOFFICE_SHEET_UTILS__;
  const {
    isExcludedDocChromeElement, extractEditableDocumentText, collectActiveScrubGroups, applyScrubResultsToGroups, tokenizeFormulaStringLiterals,
  } = host.__EASYOFFICE_CREATE_PII_SCRUBBER__({ document, localScrubPii });

  const { invokeIpc, canStream: canStreamIpc } = host.__EASYOFFICE_CREATE_IPC_TRANSPORT__({ document, host, desktopBridge });
  const pdfEdit = host.__EASYOFFICE_CREATE_PDF_EDIT__({ document, desktopBridge, getPdfRotationDeg: () => pdfRotationDeg });
  host.__EASYOFFICE_INVOKE_BACKEND__ = invokeIpc;

  let sheetController = null, workspacePanels = null, slideStudio = null, shellChrome = null;
  const snapshotDomSheetCells = () => sheetController.snapshotDomSheetCells();

  const sheetStoreHelpers = host.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc, normalizeFormattedNumericCell, tokenizeFormulaStringLiterals,
    getColumnWidths: () => sheetController?.getColumnWidths?.(),
    getFileName: () => windowBridge?.getDocumentName("SHEET"),
  });

  windowBridge = host.__EASYOFFICE_WINDOW_BRIDGE__.createWindowBridge({
    document, env: host, desktopBridge, getActiveModule: () => activeModule,
    setFeedback: (text) => setStatusFeedback(text),
    onRecentsChanged: (entries) => shellChrome.pushRecents(entries),
    settleDirty: () => workspaceSession.settle(),
    isDirty: () => workspaceSession.isDirty(),
    save: () => saveWorkspaceDocument({ closing: true }),
    discardStoredState: () => { workspaceSession.discardStoredState(); hwpSession.discardAutosave(); host.__EASYOFFICE_SHEET_TAB_STRIP__?.clearGroup?.(); },
  });

  const ribbonHistory = host.__EASYOFFICE_CREATE_RIBBON_HISTORY__({
    document, invokeIpc, docTabStore, sheetDataStore, sheetRowCounts, slideDeckState,
    localRibbonState, ribbonCatalog, sheetStoreHelpers, extractEditableDocumentText,
    getSheetController: () => sheetController,
    getWorkspacePanels: () => workspacePanels, getDocRuns: () => docRunsUi, getDocLink: () => docLink, getDocToc: () => docToc, getDocTable: () => docTable, getDocPages: () => docPages, getDocPageSetup: () => docPageSetup, getSlideStudio: () => slideStudio,
    getActiveModule: () => activeModule, getHwpHost: () => hwpHost,
    getWindowBridge: () => windowBridge,
    setActiveModule: (next) => { activeModule = next; },
    getActiveSheetName: () => activeSheetName,
    setActiveSheetName: (next) => { activeSheetName = next; },
    getActiveSheetCellRef: () => activeSheetCellRef,
    getActiveSlideIndex: () => activeSlideIndex,
    getPdfRotationDeg: () => pdfRotationDeg,
    setPdfRotationDeg: (next) => { pdfRotationDeg = next; },
    getPdfEdit: () => pdfEdit,
    enqueueWorkspaceHistoryOp,
    onModuleApplied: (mod) => { docIme?.abandon(); workspaceSession?.applyModuleContext(mod); copilotUi?.onModuleChanged(); },
  });

  docIme = host.__EASYOFFICE_CREATE_DOC_IME__({
    getActiveModule: () => activeModule, getDocTab: (mod) => docTabStore[mod],
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot, pushRawUndoSnapshot: ribbonHistory.pushRawUndoSnapshot,
    getEpoch: ribbonHistory.getHistoryMutationEpoch, markDocStale: () => workspacePanels.markActiveDocTabStale(),
    computeStats: () => workspacePanels.computeSmartStats(),
    selectionIsRange: () => { const r = host.getSelection?.()?.rangeCount ? host.getSelection().getRangeAt(0) : null; return Boolean(r && !r.collapsed && document.getElementById("doc-editor")?.contains(r.startContainer)); },
  });
  docRunsUi = host.__EASYOFFICE_CREATE_DOC_RUNS_UI__({
    document, getActiveModule: () => activeModule, saveDocState: () => workspacePanels.saveActiveDocTabState(), isComposing: docIme.isComposing,
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot,
  });
  docPaste = host.__EASYOFFICE_CREATE_DOC_PASTE__({ document, env: host, withUndo: docRunsUi.withUndo, setFeedback: (text) => setStatusFeedback(text), getActiveModule: () => activeModule });
  docLink = host.__EASYOFFICE_CREATE_DOC_LINK__({
    document, env: host, getRange: () => docRunsUi.currentRange(), withUndo: docRunsUi.withUndo, setFeedback: (text) => setStatusFeedback(text),
    getActiveModule: () => activeModule, onSlideLink: (url, text, elId) => slideStudio?.appendLink?.(url, text, elId),
  });

  docToc = host.__EASYOFFICE_CREATE_DOC_TOC__({
    document, env: host, getRange: () => docRunsUi.currentRange(), withUndo: docRunsUi.withUndo, setFeedback: (text) => setStatusFeedback(text),
    runCommand: (toc) => ribbonHistory.executeTrackedRibbonCommand("doc-mod-toc", { toc }),
  });
  docTable = host.__EASYOFFICE_CREATE_DOC_TABLE__({ document, env: host, getRange: () => docRunsUi.currentRange(), withUndo: docRunsUi.withUndo, setFeedback: (text) => setStatusFeedback(text), captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot, commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot, saveDocState: () => workspacePanels.saveActiveDocTabState(), getActiveModule: () => activeModule, isHwpEmbedActive: () => hwpHost?.isEmbedActive?.() === true, onSlideComment: (text) => slideStudio?.appendNote?.(text) });
  docPages = host.__EASYOFFICE_CREATE_DOC_PAGES__({ document, env: host, withUndo: docRunsUi.withUndo, onChange: () => workspacePanels.renderPageStatus() });
  docPageFrame = host.__EASYOFFICE_CREATE_DOC_PAGE_FRAME__({ document, env: host, withUndo: docRunsUi.withUndo, setFeedback: (text) => setStatusFeedback(text), getFileName: () => windowBridge?.getDocumentName(activeModule), getActiveModule: () => activeModule });

  docPageSetup = host.__EASYOFFICE_CREATE_DOC_PAGE_SETUP__({ document, env: host, withUndo: docRunsUi.withUndo, setFeedback: (text) => setStatusFeedback(text), getFrameMargins: () => docPageFrame.margins(), getActiveModule: () => activeModule });

  sheetController = host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__({
    document, sheetDataStore, sheetRowCounts, sheetStoreHelpers,
    getActiveModule: () => activeModule,
    getActiveSheetName: () => activeSheetName,
    setActiveSheetName: (next) => { activeSheetName = next; },
    getActiveSheetCellRef: () => activeSheetCellRef,
    setActiveSheetCellRef: (next) => { activeSheetCellRef = next; },
    enqueueWorkspaceHistoryOp,
    restoreWorkspaceSnapshot: ribbonHistory.restoreWorkspaceSnapshot,
    sheetHistory: ribbonHistory.sheetHistory,
    onSheetSwitchedOutline: () => workspacePanels.renderModuleOutline("SHEET"),
    onClearMenuStateChanged: () => shellChrome?.pushMenuState?.(),
  });

  workspacePanels = host.__EASYOFFICE_CREATE_WORKSPACE_PANELS__({
    document, URL, invokeIpc, docTabStore, sheetDataStore, slideDeckState, localRibbonState,
    getDocRuns: () => docRunsUi, getDocTable: () => docTable, getDocPages: () => docPages, getDocPageFrame: () => docPageFrame, defaultHwpEditorNodes: DEFAULT_HWP_EDITOR_NODES, createEmptySlideStyleFields,
    parseCellRefParts, isExcludedDocChromeElement, extractEditableDocumentText,
    collectActiveScrubGroups, applyScrubResultsToGroups,
    scrubSheetStoreMap: sheetStoreHelpers.scrubSheetStoreMap, sheetController,
    getActiveModule: () => activeModule, getHwpHost: () => hwpHost, getHwpSession: () => hwpSession,
    getWindowBridge: () => windowBridge, getFileOpen: () => fileOpen,
    getActiveSheetName: () => activeSheetName,
    getActiveSheetCellRef: () => activeSheetCellRef,
    getActiveSlideIndex: () => activeSlideIndex,
    setActiveSlideIndex: (next) => { activeSlideIndex = next; },
    getPdfRotationDeg: () => pdfRotationDeg,
    getPdfEdit: () => pdfEdit,
    onSlideRendered: (idx) => slideStudio?.afterRender(idx),
    enqueueWorkspaceHistoryOp,
    runExclusive: chainWorkspaceQueue,
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot,
    runStudioClipboardCommand: (cmd, withUndo) => workspaceSession.runClipboardCommand(cmd, withUndo),
    insertTextAtSelection: (text) => workspaceSession.insertTextAtSelection(text),
  });

  workspaceSession = host.__EASYOFFICE_CREATE_WORKSPACE_SESSION__({
    document, shortcutBindings: ribbonCatalog.WORKSPACE_SHORTCUT_BINDINGS,
    getActiveModule: () => activeModule, getHwpHost: () => hwpHost,
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    restoreWorkspaceSnapshot: ribbonHistory.restoreWorkspaceSnapshot,
    runExclusive: chainWorkspaceQueue,
    getWorkspacePanels: () => workspacePanels, getDocPaste: () => docPaste,
    getSheetController: () => sheetController, getSheetStore: () => sheetDataStore,
    getDocumentId: () => windowBridge.launch.windowId,
    onDirtyChange: (unsaved) => windowBridge.setDirty(unsaved),
    isCapturePaused: () => copilotUi?.isPerforming() === true, whenCaptureAllowed: () => copilotUi?.whenIdle() ?? Promise.resolve(),
    isDesktopShell: () => desktopBridge.isAvailable(),
    onMenuContext: (commandModule) => shellChrome.pushMenuState(commandModule), onMenuItemsApplied: () => { docTable.syncMenu(); sheetController?.syncCfClearMenu?.(); },
  });

  slideStudio = host.__EASYOFFICE_CREATE_SLIDE_STUDIO__({
    document, slideDeckState, getActiveModule: () => activeModule,
    getActiveSlideIndex: () => activeSlideIndex, getPanels: () => workspacePanels,
    pushUndoSnapshot: ribbonHistory.pushUndoSnapshot,
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    pushRawUndoSnapshot: ribbonHistory.pushRawUndoSnapshot,
    getHistoryMutationEpoch: ribbonHistory.getHistoryMutationEpoch,
    noteActivity: () => workspaceSession?.noteActivity(),
  });

  const fileOpen = host.__EASYOFFICE_CREATE_FILE_OPEN__({
    document, invokeIpc, pdfEdit, createEmptySlideStyleFields, getActiveModule: () => activeModule,
    getHwpSession: () => hwpSession,
    confirm: (kind) => desktopBridge.confirm(kind),
    onOpenRequest: () => windowBridge.requestOpenFile(),
    routeFile: (file) => windowBridge.routeFile(file),
    enqueueOp: enqueueWorkspaceHistoryOp, runExclusive: chainWorkspaceQueue,
    onOpenedClean: () => { copilotUi?.onDocumentOpened?.(); return windowBridge.isManaged() ? workspaceSession.markSaved() : undefined; },
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    pushRawUndoSnapshot: ribbonHistory.pushRawUndoSnapshot,
    restoreWorkspaceSnapshot: ribbonHistory.restoreWorkspaceSnapshot,
    commitPendingSheetCell: (flush) => sheetController.commitPendingFocusedSheetCell(flush),
    restoreSheetView: () => sheetController.restoreView(),
    computeEvaluatedSheetStore: sheetStoreHelpers.computeEvaluatedSheetStore,
    replaceColumnWidths: (widths) => sheetController.replaceColumnWidths(widths), replaceSheetRules: (rules) => host.__EASYOFFICE_SHEET_RULES__.replaceAll(rules),
  });

  hwpHost = host.__EASYOFFICE_CREATE_HWP_HOST__({
    document, invokeIpc, getActiveModule: () => activeModule,
    shellShortcuts: workspaceSession.forwardedKeys(),
    onStateChange: () => workspaceSession.applyModuleContext(activeModule),
    canExportPdf: desktopBridge.isAvailable(),
    // The macOS desktop app draws the one menu bar natively, so the editor hides its own row (PD-396).
    hasNativeMenu: () => desktopBridge.isAvailable() && activeShortcutModifier === "⌘",
    systemFonts: desktopBridge.isAvailable() ? desktopBridge.systemFonts : null,
  });

  const hwpPrint = host.__EASYOFFICE_CREATE_HWP_PRINT__({ document, hwpHost, env: host, notify: (text) => setStatusFeedback(text), desktopBridge });

  const officePrint = host.__EASYOFFICE_CREATE_OFFICE_PRINT__({
    document, env: host, desktopBridge, getActiveModule: () => activeModule, slideDeckState, sheetController, notify: (text) => setStatusFeedback(text),
    restoreDomChildren: host.__EASYOFFICE_SHEET_UTILS__.restoreDomChildren,
  });
  const findReplace = host.__EASYOFFICE_CREATE_FIND_REPLACE__({
    document, env: host, getActiveModule: () => activeModule, setFeedback: (text) => setStatusFeedback(text),
    getBaseStudio: () => host.__EASYOFFICE_BASE_BOARD_HOST__?.getBaseStudio(),
    sheetDataStore, sheetController, sheetStoreHelpers, getActiveSheetName: () => activeSheetName, getActiveSheetCellRef: () => activeSheetCellRef,
    sheetHistory: ribbonHistory.sheetHistory, captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot, commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot,
    saveDocState: () => workspacePanels.saveActiveDocTabState(), noteActivity: () => workspaceSession.noteActivity(),
  });

  hwpSession = host.__EASYOFFICE_CREATE_HWP_SESSION__({
    document, hwpHost, invokeIpc, getActiveModule: () => activeModule,
    confirm: (kind) => desktopBridge.confirm(kind),
    activateHwp: () => ribbonHistory.activatePane("HWP"),
    deliverSavedFile: (base64, name, ext, options) => workspacePanels.deliverSavedFile(base64, name, ext, options),
    resolveBaseName: () => workspacePanels.resolveSaveFileBase("HWP"),
    pdfExport: desktopBridge.isAvailable() ? desktopBridge.pdfExport : null,
    getDocumentId: () => windowBridge.launch.windowId,
    setDocumentName: (name) => windowBridge.setDocumentName(name, "HWP"),
    onDirtyChange: (dirty) => workspaceSession.setExternalDirty(dirty),
    defaultNotice: host.__EASYOFFICE_HWP_HOST_CONSTANTS__.MSG_UNAVAILABLE,
  });

  host.__EASYOFFICE_WINDOW_BRIDGE__.instance = windowBridge;

  // The editor's 파일 menu forwards shell commands (whitelisted in hwp-host.js); these run them.
  shellChrome = host.__EASYOFFICE_CREATE_SHELL_CHROME__({
    document, hwpHost, icons, ribbonState: localRibbonState,
    // Only macOS has a native menu bar that needs the window's state (PD-121).
    hasNativeMenu: () => activeShortcutModifier === "⌘",
    actions: {
      newDocument: () => windowBridge.newDocument("HWP"), openFile: () => fileOpen.openFilePicker(),
      openRecent: (id) => windowBridge.openRecent(id), recentDialog: () => windowBridge.chooseRecent(), save: () => saveWorkspaceDocument(),
      saveAs: (format) => hwpSession.saveAs(format), exportPdf: () => hwpSession.exportPdf(), print: () => hwpPrint.print(),
      close: () => windowBridge.requestClose(),
    },
  });

  copilotUi = host.__EASYOFFICE_CREATE_COPILOT_UI__({
    document, invokeIpc, canStreamIpc, sheetDataStore, sheetRowCounts, slideDeckState,
    getActiveModule: () => activeModule, getActiveSheetName: () => activeSheetName, getActiveSlideIndex: () => activeSlideIndex,
    getSheetController: () => sheetController, getWorkspacePanels: () => workspacePanels, getPdfEdit: () => pdfEdit, getHwpHost: () => hwpHost,
    activatePane: (mod) => ribbonHistory.activatePane(mod), captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    restoreWorkspaceSnapshot: ribbonHistory.restoreWorkspaceSnapshot, commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot,
    getHistoryMutationEpoch: ribbonHistory.getHistoryMutationEpoch, beginHistoryGroup: ribbonHistory.beginHistoryGroup, endHistoryGroup: ribbonHistory.endHistoryGroup,
    noteActivity: () => workspaceSession?.noteActivity(), extractEditableDocumentText, localScrubPii, isImeComposing: docIme.isComposing, setStatusFeedback,
  });

  function setStatusFeedback(text) {
    const fb = document.getElementById("status-command-feedback");
    if (fb) fb.textContent = text;
  }

  function saveWorkspaceDocument(options) {
    return workspacePanels.saveActiveModuleDocument(options).then((saved) => {
      if (saved !== false) { workspaceSession.markSaved(); fileOpen.markModuleSaved(); }
      return saved;
    });
  }

  let paletteSelectedIdx = 0;
  let paletteFilteredItems = [];

  const DROP_HINT = "파일을 놓으면 문서를 엽니다";
  const NEW_DOCUMENT_LABELS = {
    DOC: "문서 (DOCX)", HWP: "한글 문서 (HWP)", SHEET: "스프레드시트 (XLSX)",
    SLIDE: "프레젠테이션 (PPTX)", BASE: "데이터베이스 (BASE)", BOARD: "보드 (BOARD)", PDF: "PDF 스튜디오", CONV: "포맷 변환기",
  };

  const EXCLUDED_PALETTE_SUFFIX = {
    DOC: new Set(["data-validation"]),
    HWP: new Set(["layout-orientation", "layout-margins", "data-validation"]),
    SHEET: new Set(["layout-paper", "layout-orientation", "layout-columns", "layout-margins", "mod-chart", "review-stats"]),
    SLIDE: new Set(["layout-columns", "layout-margins", "data-autosum", "data-fn-browser", "data-sort", "data-validation", "review-track"]),
    PDF: new Set(["home-bold", "home-italic", "home-underline", "home-style", "insert-table", "insert-chart", "insert-shape", "insert-link", "layout-paper", "layout-orientation", "layout-columns", "layout-margins", "data-autosum", "data-fn-browser", "data-sort", "data-validation", "review-track"]),
  };
  const WORKSPACE_BUTTON_PALETTE = {
    BASE: [["btn-base-add-record", "새 레코드"], ["btn-base-add-field", "필드 추가"], ["btn-base-filter", "필터"], ["btn-base-sort", "정렬"], ["btn-base-add-table", "새 테이블"], ["btn-base-import", "가져오기…"], ["btn-base-export-xlsx", "XLSX로 내보내기"], ["btn-base-export-csv", "CSV로 내보내기"], ["btn-base-export-tsv", "TSV로 내보내기"]],
    BOARD: [["btn-board-select", "선택 도구"], ["btn-board-add-shape", "도형 추가"], ["btn-board-add-text", "텍스트 추가"], ["btn-board-add-image", "그림 추가"], ["btn-board-add-chart", "차트 추가"], ["btn-board-connect-ortho", "직각 연결선"], ["btn-board-connect-curved", "곡선 연결선"], ["btn-board-front", "맨 앞으로 가져오기"], ["btn-board-back", "맨 뒤로 보내기"], ["btn-board-delete", "개체 지우기"], ["btn-board-analyze", "요소 분석"], ["btn-board-export-pdf", "PDF로 내보내기"], ["btn-board-fit-all", "전체 맞춤"]],
  };

  function executePaletteItem(item) {
    if (!item) return;
    const modal = document.getElementById("cmd-palette-modal");
    if (modal && typeof modal.close === "function") modal.close();
    if (item.run) { item.run(); return; }
    const done = item.id === "studio-save" ? saveWorkspaceDocument()
      : item.id === "studio-save-as" ? saveWorkspaceDocument({ saveAs: true })
      : item.id === "studio-undo" ? ribbonHistory.performWorkspaceUndo()
      : item.id === "studio-redo" ? ribbonHistory.performWorkspaceRedo()
      : item.id === "file-open" ? (fileOpen.openFilePicker(), Promise.resolve())
      : item.id.startsWith("window-new-") ? windowBridge.newDocument(item.id.slice("window-new-".length)).catch((err) => setStatusFeedback(err?.message || String(err)))
      : ribbonHistory.executeTrackedRibbonCommand(item.id, { targetModule: item.module });
    if (item.id.endsWith("-review-verify")) void Promise.resolve(done).then(() => host.__EASYOFFICE_LAYOUT_LINT__?.appendLintFeedbackAfterVerify?.()).catch(() => {});
    else void Promise.resolve(done).catch(() => {});
  }

  function collectCommandPaletteItems() {
    const fmt = (sc) => (typeof formatPlatformShortcut === "function" ? formatPlatformShortcut(sc, activeShortcutModifier) : sc);
    const isHwp = activeModule === "HWP", isPages = activeModule === "DOC" && document.getElementById("doc-editor")?.dataset?.eoDocFlavor === "pages";
    const undoLabel = isHwp ? "되돌리기" : "실행 취소";
    const items = [
      { id: "studio-save", module: activeModule, title: "저장", shortcut: fmt("⌘S"), searchKey: "파일 저장 save " + fmt("⌘S") },
      { id: "studio-save-as", module: activeModule, title: "다른 이름으로 저장…", shortcut: fmt("⇧⌘S"), searchKey: "파일 다른 이름으로 저장 save as " + fmt("⇧⌘S") },
      { id: "studio-undo", module: activeModule, title: undoLabel, shortcut: fmt("⌘Z"), searchKey: "편집 " + undoLabel + " 실행 취소 되돌리기 undo " + fmt("⌘Z") },
      { id: "studio-redo", module: activeModule, title: "다시 실행", shortcut: fmt("⇧⌘Z"), searchKey: "편집 다시 실행 redo " + fmt("⇧⌘Z") },
    ];
    for (const [kind, label] of Object.entries(NEW_DOCUMENT_LABELS)) items.push({ id: "window-new-" + kind, module: activeModule, title: "새 문서 · " + label, shortcut: "", searchKey: "새 문서 창 열기 new " + label.toLowerCase() });
    items.push({ id: "file-open", module: activeModule, title: "열기…", shortcut: fmt("⌘O"), searchKey: "파일 열기 open " + fmt("⌘O") });
    const cats = RIBBON_CATEGORY_TOOLS[activeModule];
    if (cats) {
      const excluded = EXCLUDED_PALETTE_SUFFIX[activeModule], koBadge = MODULE_SHORT_BADGES[activeModule] || activeModule;
      for (const cat of Object.keys(cats)) {
        for (const c of cats[cat]) {
          const suffix = c.id.replace(/^[a-z]+-/, "");
          if (c.id.endsWith("-view-ribbon") || excluded?.has(suffix)) continue;
          if (isPages && ["layout-columns", "mod-symbol", "para-numbering", "para-bullet", "doc-mod-mailmerge"].includes(suffix)) continue;
          const pages = isPages ? host.__EASYOFFICE_PAGES_COMMANDS__ : null, label = pages?.PALETTE_TITLES[suffix] ?? c.title;
          const dispShortcut = fmt(pages?.PALETTE_KEYS[suffix] ?? ribbonCatalog.resolveRibbonShortcutLabel(c.id, c.shortcut));
          const extraKeys = c.id.endsWith("-insert-link") ? " 책갈피 링크 hyperlink" : "";
          items.push({ id: c.id, module: activeModule, title: label, shortcut: dispShortcut, searchKey: (koBadge + " " + label + extraKeys + " " + dispShortcut + " " + c.id + " " + activeModule).toLowerCase() });
        }
      }
    }
    if (activeModule === "DOC" && !isPages) items.push({ id: "doc-mod-toc-update", module: "DOC", title: "목차 업데이트", shortcut: "", searchKey: "입력 문서 목차 업데이트 toc" }, { id: "doc-mod-page-break", module: "DOC", title: "페이지 나누기", shortcut: fmt("⌘Enter"), searchKey: "레이아웃 문서 페이지 나누기 page break " + fmt("⌘Enter") });
    if (isPages) items.push({ id: "doc-mod-toc-update", module: "DOC", title: "목차 업데이트", shortcut: "", searchKey: "삽입 문서 목차 업데이트 toc" }, ...host.__EASYOFFICE_PAGES_COMMANDS__.paletteItems(fmt, runNativeMenu));
    if (activeModule === "DOC" || isHwp || activeModule === "SHEET") items.push({ id: "studio-cmd:find", module: activeModule, title: "찾기", shortcut: fmt("⌘F"), searchKey: "편집 찾기 find " + fmt("⌘F"), run: () => runNativeMenu("edit:find") }, { id: "studio-cmd:replace", module: activeModule, title: isHwp ? "찾아 바꾸기" : isPages ? "찾기 및 대치" : "찾기 및 바꾸기", shortcut: fmt(isHwp ? "⌘F2" : "⌃H"), searchKey: "편집 " + (isPages ? "찾기 및 대치" : "찾기 및 바꾸기 찾아 바꾸기") + " replace " + fmt(isHwp ? "⌘F2" : "⌃H"), run: () => runNativeMenu("edit:replace") });
    if (activeModule === "DOC" || isHwp) {
      const hfItems = isHwp || isPages ? [["mod-header", "머리말", "header", isHwp ? "page:header" : "page:doc-header"], ["mod-footer", "꼬리말", "footer", isHwp ? "page:footer" : "page:doc-footer"]] : [["mod-header", "머리글", "header", "page:doc-header"], ["mod-footer", "바닥글", "footer", "page:doc-footer"]];
      for (const [cmd, title, english, menuId] of [...hfItems, ["para-checklist", "체크리스트", "checklist", "format:para-checklist"], ...(activeModule === "DOC" ? [["insert-image", isPages ? "이미지" : "그림", "image picture", "insert:image"], ["mod-endnote", "미주", "endnote", "insert:endnote"], ["review-accept-all", "모든 변경 내용 수락", "accept all changes", "tool:review-accept-all"], ["review-reject-all", "모든 변경 내용 거부", "reject all changes", "tool:review-reject-all"], ["table-border-bg", "테두리 및 음영", "cell border background", "table:border-bg"], ["table-cell-props", "표 속성", "table cell properties", "table:cell-props"], ["table-col-equal", isPages ? "같은 너비로 열 조절" : "열 너비를 같게", "equal column width", "table:col-equal"]] : [])]) {
        items.push({ id: "studio-cmd:" + cmd, module: activeModule, title, shortcut: "", searchKey: "쪽 " + title + " " + english, run: () => runNativeMenu(menuId) });
      }
    }
    if (activeModule === "SHEET") items.push(...host.__EASYOFFICE_NATIVE_MENU_TABLE__.sheetMenuPaletteItems(document));
    if (activeModule === "BASE" || activeModule === "BOARD") {
      const ws = [["zoom-in", "확대", "⌘=", "view:zoom-in"], ["zoom-out", "축소", "⌘-", "view:zoom-out"], ["view-zoom", "100%", "⌘0", "view:zoom-100"], ["view-theme", "밝게/어둡게 전환", "⌃⌥D", "view:theme-toggle"], ["copilot-toggle", "코파일럿", "⇧⌘K", "tool:copilot"], ["print", "인쇄…", "⌘P", "file:print"], ["find", "찾기", "⌘F", "edit:find"], ...(activeModule === "BASE" ? [["replace", "찾기 및 바꾸기", "⌘F2", "edit:replace"]] : [])];
      for (const [id, title, sc, menuId] of ws) items.push({ id: "ws-" + id, module: activeModule, title, shortcut: fmt(sc), searchKey: title + " " + id + " " + fmt(sc), run: () => runNativeMenu(menuId) });
      for (const [btnId, title] of WORKSPACE_BUTTON_PALETTE[activeModule]) items.push({ id: "btn:" + btnId, module: activeModule, title, shortcut: "", searchKey: activeModule.toLowerCase() + " " + title, run: () => document.getElementById(btnId)?.click?.() });
    }
    if (host.__EASYOFFICE_EXTRA_PALETTE_ITEMS__) items.push(...host.__EASYOFFICE_EXTRA_PALETTE_ITEMS__(activeModule, fmt, runNativeMenu));
    items.push({ id: "view:toolbar-labels", module: activeModule, title: "도구 상자 라벨 표시", shortcut: "", searchKey: "보기 도구 상자 라벨 표시 toolbar labels", run: () => runNativeMenu("view:toolbar-labels") });
    return items;
  }

  function renderCommandPaletteList(filterText) {
    const listEl = document.getElementById("cmd-palette-list");
    if (!listEl) return;
    const q = String(filterText || "").trim().toLowerCase();
    const all = collectCommandPaletteItems();
    paletteFilteredItems = !q ? all.slice(0, 80) : all
      .map((item, idx) => ({ item, idx, score: item.title.toLowerCase() === q ? 0 : item.title.toLowerCase().startsWith(q) ? 1 : item.title.toLowerCase().includes(q) ? 2 : item.searchKey.toLowerCase().includes(q) ? 3 : 9 }))
      .filter((x) => x.score < 9).sort((a, b) => a.score - b.score || a.idx - b.idx).slice(0, 80).map((x) => x.item);
    if (paletteSelectedIdx >= paletteFilteredItems.length) paletteSelectedIdx = 0;
    listEl.innerHTML = "";
    const searchInput = document.getElementById("cmd-search-input");
    searchInput?.removeAttribute("aria-activedescendant");
    if (paletteFilteredItems.length === 0) {
      const emptyLi = document.createElement("li");
      emptyLi.className = "cp-empty";
      emptyLi.setAttribute("role", "option");
      emptyLi.setAttribute("aria-disabled", "true");
      emptyLi.textContent = "검색 결과 없음";
      listEl.appendChild(emptyLi);
      return;
    }
    paletteFilteredItems.forEach((item, idx) => {
      const li = document.createElement("li");
      const selected = idx === paletteSelectedIdx;
      li.className = selected ? "cp-item cp-item--selected" : "cp-item";
      li.id = "cp-option-" + idx;
      li.setAttribute("role", "option");
      li.setAttribute("aria-selected", selected ? "true" : "false");
      if (selected) searchInput?.setAttribute("aria-activedescendant", li.id);
      const labelSpan = document.createElement("span");
      labelSpan.className = "cp-item-label";
      labelSpan.textContent = item.title;
      li.appendChild(labelSpan);
      if (item.shortcut) {
        const scSpan = document.createElement("span");
        scSpan.className = "cp-item-shortcut";
        scSpan.textContent = item.shortcut;
        li.appendChild(scSpan);
      }
      li.addEventListener("click", () => executePaletteItem(item));
      listEl.appendChild(li);
      if (idx === paletteSelectedIdx) li.scrollIntoView?.({ block: "nearest" });
    });
  }

  function setupRibbonChrome(boot) {
    activeShortcutModifier = workspaceSession.applyPlatformChrome(boot);
    shellChrome.setAppVersion(boot.appVersion);
    // Update notice: desktop app only (the shell does the checking).
    if (desktopBridge.isAvailable()) host.__EASYOFFICE_CREATE_UPDATE_NOTICE__({ document, bridge: desktopBridge, env: host }).bind();

    ribbonHistory.applyModuleAccent(activeModule);

    // Legacy single-page hosts seed the document name from the bootstrap payload; windows name themselves.
    if (!windowBridge.isManaged() && Array.isArray(boot.openTabs)) {
      const known = windowBridge.getTitles();
      for (const tab of boot.openTabs) {
        if (tab && typeof tab.title === "string" && !known[tab.module]) windowBridge.setDocumentName(tab.title, tab.module);
      }
    }
  }

  function syncExternalZoom(pct) {
    const zoomRange = host.__EASYOFFICE_ZOOM_RANGE__;
    const { min, max } = zoomRange.rangeOf(activeModule);
    const nextZoom = Math.max(min, Math.min(max, Math.round(Number(pct) || 100)));
    localRibbonState.zoomPercent = nextZoom;
    const zoomLabel = document.getElementById("status-zoom-label");
    if (zoomLabel) zoomLabel.textContent = nextZoom + "%";
    const zoomSlider = document.getElementById("status-zoom-slider");
    zoomRange.syncSlider(zoomSlider, activeModule);
    if (zoomSlider) zoomSlider.value = String(nextZoom);
    return nextZoom;
  }

  function applyZoomPercent(pct, fitMode) {
    const nextZoom = syncExternalZoom(pct);
    const stage = document.querySelector(".canvas-stage");
    if (activeModule === "BOARD") {
      if (stage) { stage.style.zoom = ""; delete stage.dataset.zoomFit; }
      host.__EASYOFFICE_BASE_BOARD_HOST__?.getBoardStudio?.()?.setZoom?.(nextZoom, { syncHost: false });
      return;
    }
    if (stage) { stage.style.zoom = nextZoom + "%"; if (fitMode) stage.dataset.zoomFit = fitMode; }
    docRunsUi?.syncRuler?.(nextZoom / 100);
  }

  function applyZoomStepDelta(delta) {
    applyZoomPercent(host.__EASYOFFICE_ZOOM_RANGE__.step(activeModule, Number(localRibbonState.zoomPercent || 100), delta));
  }
  // The sheet's view restores a sheet's own zoom through this (`sheet-view-live.js`) and BOARD shares it too.
  host.__EASYOFFICE_ZOOM__ = { apply: applyZoomPercent, syncExternal: syncExternalZoom, current: () => Number(localRibbonState.zoomPercent || 100) };

  function setupStaticNavigationHandlers() {
    const zoomOutBtn = document.getElementById("btn-zoom-out");
    const zoomInBtn = document.getElementById("btn-zoom-in");
    if (zoomOutBtn) {
      zoomOutBtn.addEventListener("click", () => applyZoomStepDelta(-1));
    }
    if (zoomInBtn) {
      zoomInBtn.addEventListener("click", () => applyZoomStepDelta(1));
    }
    workspacePanels.setupStudioChromeControls(
      applyZoomPercent,
      (cmd) => {
        if (cmd === "view-zoom" && !RIBBON_CATEGORY_TOOLS[activeModule]) {
          applyZoomPercent(100);
          return;
        }
        const modPrefix = getRibbonModulePrefix(activeModule);
        const fullCmd =
          host.__EASYOFFICE_RIBBON_CATALOG__.resolveMenuCommand(cmd, activeModule) ||
          (cmd.startsWith("doc-") || cmd.startsWith("hwp-") ? cmd : modPrefix + "-" + cmd);
        void ribbonHistory.executeTrackedRibbonCommand(fullCmd).catch(() => {});
      },
      {
        save: () => void saveWorkspaceDocument().catch(() => {}),
        saveAs: () => void saveWorkspaceDocument({ saveAs: true }).catch(() => {}),
        undo: () => void ribbonHistory.performWorkspaceUndo().catch(() => {}),
        redo: () => void ribbonHistory.performWorkspaceRedo().catch(() => {}),
        benchmark: () => void workspacePanels.runBenchmarkAction().catch(() => {}),
        ...(({ find, replace, print }) => ({ find, replace, print }))(shortcutActions()),
      }
    );
  }

  function setupPanelAndThemeActions() {
    const btnBench = document.getElementById("btn-run-benchmark");
    if (btnBench) {
      btnBench.addEventListener("click", () => {
        void workspacePanels.runBenchmarkAction().catch(() => {});
      });
    }

    document.querySelectorAll("[data-theme-mode-choice]").forEach((item) => {
      item.addEventListener("click", () => {
        void ribbonHistory
          .enqueueWorkspaceHistoryOp(async () => {
            ribbonHistory.pushUndoSnapshot(false);
            host.__EASYOFFICE_THEME__.setMode(item.dataset.themeModeChoice);
          })
          .catch(() => {});
      });
    });

    const themeBtn = document.getElementById("btn-theme-toggle");
    if (themeBtn) {
      themeBtn.addEventListener("click", () => {
        void ribbonHistory
          .enqueueWorkspaceHistoryOp(async () => {
            await sheetController.commitPendingFocusedSheetCell(true);
            ribbonHistory.pushUndoSnapshot(false);
            host.__EASYOFFICE_THEME__.toggle();
            sheetController.refreshFocusedSheetCellPendingSnapshot();
          })
          .catch(() => {});
      });
    }
  }

  function getRibbonModulePrefix(mod) {
    return mod === "DOC" || mod === "CONV" ? "document" : String(mod || "DOC").toLowerCase();
  }

  function toggleDocTableCellMerge(mergeBtn) {
    const tbody = document.getElementById("doc-table-body");
    if (!tbody) return;
    const sumRow = document.getElementById("doc-table-sum-row");
    const cells = sumRow ? sumRow.querySelectorAll("td") : tbody.querySelectorAll("tr td");
    if (cells.length < 2) return;
    ribbonHistory.pushUndoSnapshot(false);
    const isMerged = cells[0].colSpan > 1;
    cells[0].colSpan = isMerged ? 1 : 2;
    cells[1].hidden = !isMerged;
    mergeBtn.textContent = isMerged ? "셀 병합 토글" : "셀 병합 해제";
    workspacePanels.saveActiveDocTabState();
    const fb = document.getElementById("status-command-feedback");
    if (fb) fb.textContent = isMerged ? "문서 표 셀 분할 완료" : "문서 표 셀 합치기 완료";
  }

  function handleDocEditorTableClick(e) {
    if (activeModule !== "DOC" && activeModule !== "HWP") return;
    if (e.target.closest("#btn-doc-add-row")) {
      ribbonHistory.pushUndoSnapshot(false);
      workspacePanels.appendDocTableRow();
      workspacePanels.saveActiveDocTabState();
      return;
    }
    const mergeBtn = e.target.closest("#btn-doc-merge-toggle");
    if (mergeBtn) {
      toggleDocTableCellMerge(mergeBtn);
      return;
    }
    if (e.target.closest("#btn-doc-eval-table")) {
      workspacePanels.computeDocTableSummary();
      workspacePanels.saveActiveDocTabState();
    }
  }

  function setupDocTableAndSheetTabHandlers() {
    [
      ["btn-stylebar-bold", "bold"],
      ["btn-stylebar-italic", "italic"],
      ["btn-stylebar-underline", "underline"],
    ].forEach(([btnId, cmdSuffix]) => {
      const btn = document.getElementById(btnId);
      if (btn) {
        btn.addEventListener("click", () => {
          const modPrefix = getRibbonModulePrefix(activeModule);
          void ribbonHistory
            .executeTrackedRibbonCommand(modPrefix + "-home-" + cmdSuffix)
            .catch(() => {});
        });
      }
    });
    const docEditor = document.getElementById("doc-editor");
    if (docEditor) {
      docEditor.addEventListener("click", handleDocEditorTableClick);
    }

    const sheetTabBar = document.getElementById("sheet-tab-bar");
    const addSheetBtn = document.getElementById("btn-add-sheet-tab");
    if (sheetTabBar) {
      sheetTabBar.addEventListener("click", (e) => {
        const btn = e.target.closest(".sheet-tab-btn");
        if (!btn || btn.id === "btn-add-sheet-tab") return;
        const rawAttr = btn.dataset.sheetTab;
        const targetSheet =
          rawAttr !== undefined && rawAttr.trim().length > 0 ? rawAttr : "Sheet1";
        void sheetController.clickSheetTab(targetSheet, { shiftKey: Boolean(e.shiftKey), ctrlKey: Boolean(e.ctrlKey || e.metaKey) }).catch(() => {});
      });
    }
    // The add button puts a new sheet after the last one (the tab menu's insert puts it before a tab).
    addSheetBtn?.addEventListener("click", () => void sheetController.runSheetStructure("tab-add").catch(() => {}));
  }

  // A failure the transport has not shown yet (everything but the engine request itself).
  function reportConvertFailure(err) {
    if (err?.reported) return;
    const text = "변환 실패: " + (err?.message || String(err));
    const box = document.getElementById("conv-result-box");
    if (box) box.textContent = text;
    setStatusFeedback(text);
  }

  function setupSheetAndConverterHandlers() {
    const btnConvert = document.getElementById("btn-run-convert");
    if (btnConvert) {
      btnConvert.addEventListener("click", () => {
        const runConvert = () =>
          ribbonHistory.enqueueWorkspaceHistoryOp(async () => {
            await sheetController.commitPendingFocusedSheetCell(true);
            const src = document.getElementById("conv-src-fmt").value;
            const dst = document.getElementById("conv-dst-fmt").value;
            const input = await workspacePanels.resolveConverterInput(src);
            const box = document.getElementById("conv-result-box");
            if (input.problem) {
              if (box) box.textContent = "원본 파일을 사용할 수 없습니다: " + input.problem;
              return;
            }
            const convTitle = input.title ?? "통합_변환_문서";
            const res = await workspacePanels.convertAskingPasswordLoss({ sourceFormat: input.srcFormat, targetFormat: dst, title: convTitle, ...input.content });
            if (res?.cancelled) {
              if (box) box.textContent = "변환 취소: 암호 없이 저장하지 않았습니다";
              return;
            }
            const delivery = await workspacePanels.deliverSavedFile(res?.convertedBase64, convTitle, dst, { inPlace: false, exportOnly: true });
            if (box && res) {
              const srcTxt = `${res.sourceFormat.toUpperCase()} (${res.sourceBytes}B)`;
              const dstTxt = `${res.targetFormat.toUpperCase()} (${res.outputBytes}B)`;
              box.textContent = `[${res.status}] ${srcTxt} → ${dstTxt} · ${workspacePanels.describeConversionSpec(res)}${workspacePanels.describeDeliveryFailure(delivery, " · ")}`;
            }
          });
        void runConvert().catch(reportConvertFailure);
      });
    }

    const sheetPane = document.getElementById("pane-sheet");
    if (sheetPane) sheetController.bindSheetCellEvents(sheetPane);
    const evalBtn = document.getElementById("btn-eval-formula");
    const formulaInputEl = document.getElementById("formula-input");
    if (formulaInputEl && evalBtn) {
      formulaInputEl.addEventListener("keydown", (ev) => {
        if (sheetController.handleFormulaBarKey(ev)) return;
        if (ev.key === "Enter") { ev.preventDefault(); evalBtn.click(); }
      });
    }
    if (evalBtn) evalBtn.addEventListener("click", () => { void sheetController.evaluateActiveSheetFormulaBar(true).catch(() => {}); });
  }

  function setupSlideAndPdfHandlers() {
    ribbonHistory.bindPdfHighlightDialog();
    document.getElementById("btn-add-slide")?.addEventListener("click", () => slideStudio.addSlide());
    const slideTransSelect = document.getElementById("slide-transition-select");
    if (slideTransSelect) {
      slideTransSelect.addEventListener("change", () => {
        ribbonHistory.pushUndoSnapshot(false);
        if (slideDeckState[activeSlideIndex]) {
          slideDeckState[activeSlideIndex].transition = slideTransSelect.value;
        }
        workspacePanels.renderSlideDeck(activeSlideIndex);
      });
    }
    const slideStageBox = document.getElementById("slide-stage-box");
    if (slideStageBox) {
      slideStageBox.addEventListener("pointermove", (ev) => {
        if (!slideLaserActive) return;
        const dot = document.getElementById("slide-laser-dot");
        if (!dot) return;
        const rect = slideStageBox.getBoundingClientRect();
        const x = Math.max(8, Math.min(rect.width - 8, ev.clientX - rect.left));
        const y = Math.max(8, Math.min(rect.height - 8, ev.clientY - rect.top));
        dot.style.left = x + "px";
        dot.style.top = y + "px";
      });
    }
    const btnSlideLaser = document.getElementById("btn-slide-laser");
    if (btnSlideLaser) {
      btnSlideLaser.addEventListener("click", () => {
        slideLaserActive = !slideLaserActive;
        const dot = document.getElementById("slide-laser-dot");
        if (dot) dot.hidden = !slideLaserActive;
        const label = slideLaserActive ? "레이저 포인터: 켬" : "레이저 포인터: 끔";
        btnSlideLaser.title = label;
        btnSlideLaser.setAttribute("aria-pressed", String(slideLaserActive));
      });
    }

    const btnPdfRotate = document.getElementById("btn-pdf-rotate");
    if (btnPdfRotate) {
      btnPdfRotate.addEventListener("click", () => {
        void ribbonHistory
          .executeTrackedRibbonCommand("pdf-mod-rotate", { skipFeedback: true })
          .catch(() => {});
      });
    }
    document.getElementById("btn-pdf-form-toggle")?.addEventListener("click", () => workspacePanels.togglePdfFormPanel());
    const btnPdfApply = document.getElementById("btn-pdf-apply");
    if (btnPdfApply) {
      btnPdfApply.addEventListener("click", () => {
        void ribbonHistory
          .enqueueWorkspaceHistoryOp(() => {
            ribbonHistory.pushUndoSnapshot(false);
            workspacePanels.applyPdfStudioFormState();
            const note = pdfEdit.noteApplied();
            if (note) setStatusFeedback(note);
          })
          .catch(() => {});
      });
    }
  }

  function setupSmartAndSecurityHandlers() {
    const editor = document.getElementById("doc-editor");
    if (editor) {
      docIme.bind(editor);
      workspacePanels.bindDocOverwriteListener(editor);
      editor.addEventListener("focus", () => workspacePanels.saveActiveDocTabState());
      editor.addEventListener("blur", () => workspacePanels.saveActiveDocTabState());
      editor.addEventListener("click", (ev) => {
        const block = workspacePanels.findEditableDocBlock(ev.target, editor);
        if (block) workspacePanels.setActiveDocBlock(block);
      });
      workspacePanels.computeSmartStats();
    }

    document.getElementById("btn-smart-diagnose")?.addEventListener("click", () => void workspacePanels.runSmartDiagnoseAction().catch(() => {}));
    const auditBtn = document.getElementById("btn-run-security-audit") || document.getElementById("btn-security-audit");
    auditBtn?.addEventListener("click", () => void workspacePanels.runSecurityAuditAction().catch(() => {}));
    document.getElementById("btn-scrub-pii")?.addEventListener("click", () => void workspacePanels.runPiiScrubAction(true).catch(() => {}));
  }

  function togglePaletteModal(modal, cmdSearchInput) {
    if (!modal) return;
    if (modal.open && typeof modal.close === "function") return void modal.close();
    if (typeof modal.showModal === "function") {
      paletteSelectedIdx = 0;
      renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
      modal.showModal();
      if (cmdSearchInput) cmdSearchInput.focus();
    }
  }

  function handleOpenPaletteKeydown(e, modal, cmdSearchInput) {
    if (e.key === "Escape") return void modal.close?.();
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key === "/") {
      e.preventDefault();
      return void modal.close?.();
    }
    const focusKey = host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__?.latinKeyOf?.(e) ?? (e.code === "KeyF" && (e.key === "Process" || e.key === "Unidentified" || (e.key?.length === 1 && e.key.charCodeAt(0) > 0x7f)) ? "F" : e.key?.toUpperCase?.());
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && !e.isComposing && focusKey === "F") {
      e.preventDefault();
      return void cmdSearchInput?.focus();
    }
    const step = { ArrowDown: 1, ArrowUp: -1 }[e.key] || 0;
    if (step !== 0 && paletteFilteredItems.length > 0) {
      e.preventDefault();
      const prevIdx = paletteSelectedIdx;
      paletteSelectedIdx = Math.max(0, Math.min(paletteFilteredItems.length - 1, prevIdx + step));
      const listEl = document.getElementById("cmd-palette-list");
      if (listEl && listEl.children.length === paletteFilteredItems.length) {
        const prevLi = listEl.children[prevIdx];
        const nextLi = listEl.children[paletteSelectedIdx];
        if (prevLi && prevIdx !== paletteSelectedIdx) {
          prevLi.className = "cp-item";
          prevLi.setAttribute("aria-selected", "false");
        }
        if (nextLi) {
          nextLi.className = "cp-item cp-item--selected";
          nextLi.setAttribute("aria-selected", "true");
          cmdSearchInput?.setAttribute("aria-activedescendant", nextLi.id);
          nextLi.scrollIntoView?.({ block: "nearest" });
        }
        return;
      }
      renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
      return;
    }
    if (e.key === "Enter" && paletteFilteredItems[paletteSelectedIdx]) {
      e.preventDefault();
      executePaletteItem(paletteFilteredItems[paletteSelectedIdx]);
    }
  }

  // Print and find belong to the module in front: the Hangul editor has its own, document, sheet and base use the find dialog, other modules keep the palette on Ctrl+F.
  function printActiveModule() {
    if (activeModule === "HWP") return hwpPrint.print();
    return officePrint.isSupported(activeModule) ? officePrint.print() : Promise.reject(new Error("현재 창에서는 인쇄를 지원하지 않습니다."));
  }

  function findActiveModule(mode) {
    if (activeModule === "HWP" && hwpHost.isEmbedActive()) {
      void hwpHost.call("automation.execute", { id: mode === "find" ? "edit:find" : "edit:find-replace", allowDialog: true }).catch(() => {});
    } else if (!findReplace.open(mode) && mode === "find") togglePaletteModal(document.getElementById("cmd-palette-modal"), document.getElementById("cmd-search-input"));
  }

  // Dispatch is driven by ribbonCatalog.WORKSPACE_SHORTCUT_BINDINGS so every advertised label
  // has exactly one handler here; unlisted (browser-reserved) combos are never intercepted.
  function shortcutActions(modal, cmdSearchInput) {
    return {
      "slide-show-start": () => slideStudio.startShow(false),
      "slide-show-current": () => slideStudio.startShow(true),
      "slide-move-up": () => slideStudio.moveUp(),
      "slide-move-down": () => slideStudio.moveDown(),
      "slide-delete": () => slideStudio.removeSlide(),
      "slide-group": () => slideStudio.runAct("group"),
      "slide-ungroup": () => slideStudio.runAct("ungroup"), ...host.__EASYOFFICE_PAGES_COMMANDS__.shortcutActions(runNativeMenu), palette: () => togglePaletteModal(modal, cmdSearchInput),
      "slide-format-copy": () => slideStudio.runAct("format-copy"),
      "slide-format-paste": () => slideStudio.runAct("format-paste"),
      "copilot-toggle": () => copilotUi.toggle(),
      "file-open": () => fileOpen.openFilePicker(),
      "recent-dialog": () => void windowBridge.chooseRecent().catch(() => {}),
      "file-new": () => void windowBridge.newDocument(activeModule).catch((err) => setStatusFeedback(err?.message || String(err))),
      "window-close": () => void windowBridge.requestClose().catch(() => {}),
      "app-quit": () => void windowBridge.requestQuit().catch(() => {}),
      print: () => void printActiveModule().catch((err) => setStatusFeedback(err?.message || String(err))),
      find: () => findActiveModule("find"),
      replace: () => findActiveModule("replace"),
      "sheet-goto": () => document.querySelector('#menu-bar .md-item[data-sheet-act="goto"]')?.click(),
      "hwp-export-pdf": () => void hwpSession.exportPdf(),
      "studio-save": () => void saveWorkspaceDocument().catch(() => {}),
      "file-save-as": () => (activeModule === "HWP" && hwpHost.isEmbedActive() ? void hwpSession.saveAs("hwp") : void saveWorkspaceDocument({ saveAs: true }).catch(() => {})),
      "zoom-in": () => void runNativeMenu("view:zoom-in"),
      "zoom-out": () => void runNativeMenu("view:zoom-out"),
      "studio-undo": () => void ribbonHistory.performWorkspaceUndo().catch(() => {}),
      "studio-redo": () => void ribbonHistory.performWorkspaceRedo().catch(() => {}),
      ribbon: (suffix) => {
        if (suffix === "view-zoom" && !RIBBON_CATEGORY_TOOLS[activeModule]) {
          applyZoomPercent(100);
          return;
        }
        const fullCmd = getRibbonModulePrefix(activeModule) + "-" + suffix;
        void ribbonHistory.executeTrackedRibbonCommand(fullCmd).catch(() => {});
      },
    };
  }

  function handleWorkspaceShortcutKeydown(e, modal, cmdSearchInput) {
    if (slideStudio.handleKeydown(e)) return;
    workspaceSession.handleShortcutKeydown(e, shortcutActions(modal, cmdSearchInput));
  }

  // The native menu bar (PD-102) runs the same actions as the keyboard, the page's own menu items and two menu-only ones.
  function runNativeMenu(id) {
    host.__EASYOFFICE_CLOSE_MENUS__?.(document);
    const modal = document.getElementById("cmd-palette-modal");
    const input = document.getElementById("cmd-search-input");
    const menuTable = host.__EASYOFFICE_NATIVE_MENU_TABLE__;
    const { EMBED_EDITOR_COMMANDS, TEXT_FIELD_EDIT_COMMANDS } = menuTable;
    const field = document.activeElement;
    const inlineCutCopy = (activeModule === "BOARD" || activeModule === "SLIDE") && (id === "edit:cut" || id === "edit:copy");
    const ceAttr = inlineCutCopy ? field?.closest?.("[contenteditable]")?.getAttribute?.("contenteditable") : null;
    const inInline = inlineCutCopy && Boolean(field?.isContentEditable || ceAttr === "" || ceAttr === "true" || ceAttr === "plaintext-only");
    if (Object.hasOwn(TEXT_FIELD_EDIT_COMMANDS, id) && (field?.tagName === "INPUT" || field?.tagName === "TEXTAREA" || inInline)) {
      workspaceSession.execTextFieldEdit(TEXT_FIELD_EDIT_COMMANDS[id]);
      return true;
    }
    if (id === "tool:palette" && modal?.open) { togglePaletteModal(modal, input); return true; }
    if (!menuTable.allowsNativeMenuContext(id, document, modal, activeModule)) return false;
    if (id === "view:toolbar-labels") { shellChrome.toggleToolbarLabels?.(); return true; }
    // Inside the Hangul editor the zoom and cell items are the editor's own commands (a command it
    // cannot run right now, such as merging without selected cells, is refused by the editor and ignored).
    const editorCommand = (menuId) => {
      if (activeModule !== "HWP" || !hwpHost.isEmbedActive()) return false;
      void hwpHost.call("automation.execute", { id: EMBED_EDITOR_COMMANDS[menuId], allowDialog: true }).catch(() => {});
      return true;
    };
    if (Object.hasOwn(EMBED_EDITOR_COMMANDS, id) && editorCommand(id)) {
      return true;
    }
    const actions = {
      ...shortcutActions(modal, input),
      ...host.__EASYOFFICE_NATIVE_MENU_TABLE__.pageMenuActions(document),
      "hwp-editor": (_argument, menuId) => void editorCommand(menuId),
      about: () => shellChrome.openAbout(),
    };
    return host.__EASYOFFICE_NATIVE_MENU_TABLE__.runNativeMenuAction(id, actions);
  }
  host.__EASYOFFICE_NATIVE_MENU__ = (id) => runNativeMenu(String(id));
  // The shell reports open failures and the drag hint here (`window_commands.rs`); they show as the shared toast.
  host.__EASYOFFICE_NOTICE__ = (message) => shellChrome.toast(message);
  // The shell calls these (no argument) while a droppable file is dragged over the window (PD-124): the
  // document area gets the reference app's `drag-over` look, in the editor's own container for the Hangul editor.
  const setDragOver = (active) => {
    if (active) setStatusFeedback(DROP_HINT);
    if (activeModule === "HWP" && hwpHost.isEmbedActive()) return void hwpHost.setDragOver(active);
    for (const pane of document.querySelectorAll(".editor-pane")) pane.classList.toggle("drag-over", active);
  };
  host.__EASYOFFICE_DRAG_ON__ = () => setDragOver(true);
  host.__EASYOFFICE_DRAG_OFF__ = () => setDragOver(false);
  // The shell calls this (no argument) when another window changed the recent list (PD-127).
  host.__EASYOFFICE_RECENTS_CHANGED__ = () => void windowBridge.refreshRecents().then((entries) => shellChrome.pushRecents(entries));

  // Combos forwarded by the embedded HWP editor run the same actions as the keyboard.
  function handleForwardedShortcut(combo) {
    const modal = document.getElementById("cmd-palette-modal");
    const input = document.getElementById("cmd-search-input");
    workspaceSession.runForwardedShortcut(combo, shortcutActions(modal, input));
  }

  function setupCommandPaletteHandlers() {
    const modal = document.getElementById("cmd-palette-modal");
    const openPaletteBtn = document.getElementById("btn-command-palette");
    const cmdSearchInput = document.getElementById("cmd-search-input");
    if (openPaletteBtn && modal) {
      openPaletteBtn.addEventListener("click", () => togglePaletteModal(modal, cmdSearchInput));
    }
    if (cmdSearchInput) {
      cmdSearchInput.addEventListener("input", () => {
        paletteSelectedIdx = 0;
        renderCommandPaletteList(cmdSearchInput.value);
      });
    }
    modal?.addEventListener?.("click", (ev) => {
      if (ev.target === modal && typeof modal.close === "function") modal.close();
    });

    host.addEventListener("keydown", (e) => {
      if (e.defaultPrevented || e.isComposing || docIme.isComposing()) return;
      if (modal?.open) {
        handleOpenPaletteKeydown(e, modal, cmdSearchInput);
        return;
      }
      if (document.querySelector("dialog.tool-dialog[open]:not(.find-dialog)")) return;
      handleWorkspaceShortcutKeydown(e, modal, cmdSearchInput);
    });
  }

  host.__EASYOFFICE_ACTIVATE_MODULE__ = (mod) => ribbonHistory.activatePane(mod);

  async function initWorkspace() {
    icons.applyIcons(document);
    host.__EASYOFFICE_CUSTOM_SELECT__.enhanceCustomSelects(document);
    host.__EASYOFFICE_NUMBER_STEPPER__.enhanceNumberFields(document);
    void shellChrome.applyStoredTheme();
    shellChrome.bind();
    copilotUi.bind();
    findReplace.bind();
    workspacePanels.ensureDefaultDocTabStore();
    sheetDataStore.Sheet1 = snapshotDomSheetCells();
    sheetRowCounts.Sheet1 = sheetController.getSheetRowCount("Sheet1");
    sheetRowCounts.Sheet2 = sheetController.getSheetRowCount("Sheet2");
    setupStaticNavigationHandlers();
    setupPanelAndThemeActions();
    setupDocTableAndSheetTabHandlers();
    docRunsUi.bind(); docLink.bind(); docPaste.bind(); docToc.bind(); docTable.bind(); docPageSetup.bind();
    docPages.attach(document.getElementById("doc-editor")); docPageFrame.attach(document.getElementById("doc-editor"), docPages);
    setupSheetAndConverterHandlers();
    setupSlideAndPdfHandlers();
    setupSmartAndSecurityHandlers();
    setupCommandPaletteHandlers();
    workspacePanels.renderModuleOutline("DOC");
    sheetController.updateSheetSelectionAndAggregates("A1");
    slideStudio.bind();
    host.__EASYOFFICE_PAGES_UI__ = host.__EASYOFFICE_CREATE_PAGES_UI__?.({ document, getDocRuns: () => docRunsUi, systemFonts: desktopBridge.isAvailable() ? desktopBridge.systemFonts : null });
    host.__EASYOFFICE_PAGES_UI__?.bind?.();
    host.__EASYOFFICE_BASE_BOARD_HOST__?.init({ document, env: host, invokeIpc, desktopBridge, shellChrome, getActiveModule: () => activeModule, getWindowBridge: () => windowBridge, setFeedback: (t) => setStatusFeedback(t), recordUndo: () => ribbonHistory.pushUndoSnapshot(false), captureUndo: () => ribbonHistory.captureWorkspaceSnapshot(false), commitUndo: ribbonHistory.commitDeferredUndoSnapshot, noteActivity: () => workspaceSession?.noteActivity() });
    workspacePanels.renderSlideDeck(0);
    activeShortcutModifier = workspaceSession.applyPlatformChrome(
      host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__.guessClientPlatform()
    );
    workspaceSession.applyModuleContext(activeModule);
    await windowBridge.applyLaunch((mod) => ribbonHistory.activatePane(mod));
    // The start window has no document: gray the menu items that need one.
    if (windowBridge.isStart) { workspaceSession.applyModuleContext(activeModule); copilotUi.onModuleChanged(); }
    windowBridge.bindStatusTooltips();
    hwpHost.onShortcut((combo) => handleForwardedShortcut(combo)); hwpHost.onCopilotToggle?.(() => copilotUi.toggle());
    // The start window has no document of its own: nothing to restore there.
    const restoredRecord = windowBridge.isStart ? null : await workspaceSession.restoreFromStorage().catch(() => null);
    workspaceSession.establishBaseline(restoredRecord);
    workspaceSession.installActivityTracking();
    fileOpen.bindUi();
    hwpSession.bindUi();
    await fileOpen.captureBaselines();
    await loadWorkspaceBootstrap();
    void shellChrome.pushRecents(await windowBridge.start());
    await windowBridge.loadPending((name, bytes) => fileOpen.openDocumentFromBytes(name, bytes), () => workspaceSession.markSaved());
    if (!windowBridge.isStart) void (activeModule === "HWP" ? hwpHost.whenSettled() : Promise.resolve()).then(() => hwpSession.restore()).catch(() => {});
    const smokeSteps = { hwp: () => hwpHost.runSmoke(() => ribbonHistory.activatePane("HWP")), pdf: () => hwpSession.smokePdf(), print: () => hwpPrint.smoke(), frame_ipc: () => hwpHost.runFrameProbe() };
    void windowBridge.runSmoke({ invokeIpc, steps: smokeSteps });
  }

  async function loadWorkspaceBootstrap() {
    let boot;
    try {
      boot = await invokeIpc("get_workspace_bootstrap", "");
    } catch (err) {
      workspaceSession.showBootstrapError(err?.message || String(err), () => void loadWorkspaceBootstrap());
      return;
    }
    workspaceSession.hideBootstrapError();
    sheetStoreHelpers.enableSheetSessions(boot?.sheetSessions === true);
    setupRibbonChrome(boot);
  }

  document.addEventListener("DOMContentLoaded", () => {
    void initWorkspace().catch(() => {});
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
