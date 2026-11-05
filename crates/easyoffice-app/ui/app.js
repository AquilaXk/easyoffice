(function (host) {
  "use strict";

  const document = host.document;
  const URL = host.URL;

  const ribbonCatalog = host.__EASYOFFICE_RIBBON_CATALOG__;
  const {
    RIBBON_CATEGORY_TOOLS,
    MODULE_SHORT_BADGES,
    DEFAULT_HWP_EDITOR_NODES,
    createEmptySlideStyleFields,
    createInitialSlideDeckState,
    createDefaultRibbonState,
    formatPlatformShortcut,
  } = ribbonCatalog;

  const slideDeckState = createInitialSlideDeckState();
  const localRibbonState = createDefaultRibbonState();

  let isImeComposing = false;
  let activeModule = "DOC";
  let activeRibbonCategory = "home";
  let activeSheetName = "Sheet1";
  let activeSheetCellRef = "F2";
  let activeSlideIndex = 0;
  let slideLaserActive = false;
  let pdfRotationDeg = 0;
  let activeShortcutModifier = "⌘";
  let workspaceHistoryQueue = Promise.resolve();

  const docTabStore = {
    DOC: null,
    HWP: null,
  };

  const sheetDataStore = {
    Sheet1: null,
    Sheet2: {
      A1: { value: "100", formula: "" },
      B1: { value: "250", formula: "" },
      C1: { value: "350", formula: "=SUM(A1:B1)" },
      A2: { value: "기획조정실", formula: "" },
      B2: { value: "1200", formula: "" },
      C2: { value: "1300", formula: "=B2+A1" },
    },
  };

  const sheetRowCounts = {};

  function enqueueWorkspaceHistoryOp(opFn) {
    const run = workspaceHistoryQueue.then(opFn, opFn);
    workspaceHistoryQueue = run.then(
      () => {},
      () => {}
    );
    return run;
  }

  const { parseCellRefParts, normalizeFormattedNumericCell, localScrubPii } =
    host.__EASYOFFICE_SHEET_UTILS__;
  const {
    isExcludedDocChromeElement,
    extractEditableDocumentText,
    collectActiveScrubGroups,
    applyScrubResultsToGroups,
    tokenizeFormulaStringLiterals,
  } = host.__EASYOFFICE_CREATE_PII_SCRUBBER__({ document, localScrubPii });

  function setEngineStatusText(text) {
    const engineEl = document.getElementById("status-engine");
    if (engineEl) engineEl.textContent = text;
  }

  async function invokeIpc(command, payload) {
    if (typeof host.__EASYOFFICE_TEST_IPC__ === "function") {
      try {
        const raw = await host.__EASYOFFICE_TEST_IPC__(command, payload);
        setEngineStatusText("엔진: 테스트 하네스");
        return typeof raw === "string" ? JSON.parse(raw) : raw;
      } catch (testErr) {
        const msg = testErr?.message || String(testErr);
        reportIpcFailure(command, msg);
        throw new Error(msg);
      }
    }
    if (host.__TAURI_INTERNALS__ && typeof host.__TAURI_INTERNALS__.invoke === "function") {
      try {
        const raw = await host.__TAURI_INTERNALS__.invoke(command, { payload });
        setEngineStatusText("엔진: 데스크톱 연결됨");
        return typeof raw === "string" ? JSON.parse(raw) : raw;
      } catch (tauriErr) {
        const msg = tauriErr?.message || String(tauriErr);
        reportIpcFailure(command, msg);
        throw new Error(msg);
      }
    }
    if (isHttpIpcTransportAvailable()) {
      const res = await invokeHttpIpc(command, payload);
      setEngineStatusText("엔진: 로컬 서버 연결됨");
      return res;
    }
    const disconnectedMsg =
      "엔진 미연결: 데스크톱 호스트 또는 로컬 서버(--serve) 연결이 필요합니다.";
    setEngineStatusText("엔진: 미연결 · " + disconnectedMsg);
    reportIpcFailure(command, disconnectedMsg);
    throw new Error(disconnectedMsg);
  }

  host.__EASYOFFICE_INVOKE_BACKEND__ = invokeIpc;

  function isHttpIpcTransportAvailable() {
    const proto = host.location?.protocol;
    return (proto === "http:" || proto === "https:") && typeof host.fetch === "function";
  }

  function reportIpcFailure(command, msg) {
    const fb = document.getElementById("status-command-feedback");
    if (fb) fb.textContent = "[요청 거부] " + msg;
    const targetMap = {
      convert_document_format: "conv-result-box",
      audit_security: "security-status-text",
      scrub_pii: "pii-result-text",
      run_smart_assistant: "smart-summary-output",
      run_engine_benchmark: "bench-result-box",
    };
    const targetId = targetMap[command];
    const targetEl = targetId ? document.getElementById(targetId) : null;
    if (targetEl) targetEl.textContent = "[요청 거부] " + msg;
  }

  async function invokeHttpIpc(command, payload) {
    let resp;
    try {
      resp = await host.fetch("/api/ipc", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ command, payload: String(payload ?? "") }),
      });
    } catch (netErr) {
      const msg = "백엔드 IPC 네트워크 오류: " + (netErr?.message || String(netErr));
      setEngineStatusText("엔진: 연결 오류");
      reportIpcFailure(command, msg);
      throw new Error(msg);
    }
    if (!resp.ok) {
      const errBody = await resp.json().catch(() => ({}));
      const msg = errBody.error || errBody.message || "IPC request failed (" + resp.status + ")";
      reportIpcFailure(command, msg);
      throw new Error(msg);
    }
    return await resp.json();
  }

  let sheetController = null;
  let workspacePanels = null;

  function snapshotDomSheetCells() {
    return sheetController.snapshotDomSheetCells();
  }

  const sheetStoreHelpers = host.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc, normalizeFormattedNumericCell, tokenizeFormulaStringLiterals,
  });

  const ribbonHistory = host.__EASYOFFICE_CREATE_RIBBON_HISTORY__({
    document, invokeIpc, docTabStore, sheetDataStore, sheetRowCounts, slideDeckState,
    localRibbonState, ribbonCatalog, sheetStoreHelpers, extractEditableDocumentText,
    getSheetController: () => sheetController,
    getWorkspacePanels: () => workspacePanels,
    getActiveModule: () => activeModule,
    setActiveModule: (next) => { activeModule = next; },
    getActiveRibbonCategory: () => activeRibbonCategory,
    setActiveRibbonCategory: (next) => { activeRibbonCategory = next; },
    getActiveSheetName: () => activeSheetName,
    setActiveSheetName: (next) => { activeSheetName = next; },
    getActiveSheetCellRef: () => activeSheetCellRef,
    getActiveSlideIndex: () => activeSlideIndex,
    getPdfRotationDeg: () => pdfRotationDeg,
    setPdfRotationDeg: (next) => { pdfRotationDeg = next; },
    getShortcutModifier: () => activeShortcutModifier,
    enqueueWorkspaceHistoryOp,
  });

  sheetController = host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__({
    document, sheetDataStore, sheetRowCounts, sheetStoreHelpers,
    getActiveModule: () => activeModule,
    getActiveSheetName: () => activeSheetName,
    setActiveSheetName: (next) => { activeSheetName = next; },
    getActiveSheetCellRef: () => activeSheetCellRef,
    setActiveSheetCellRef: (next) => { activeSheetCellRef = next; },
    enqueueWorkspaceHistoryOp,
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    pushRawUndoSnapshot: ribbonHistory.pushRawUndoSnapshot,
    commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot,
    onSheetSwitchedOutline: () => workspacePanels.renderModuleOutline("SHEET"),
  });

  workspacePanels = host.__EASYOFFICE_CREATE_WORKSPACE_PANELS__({
    document, URL, invokeIpc, docTabStore, sheetDataStore, slideDeckState, localRibbonState,
    defaultHwpEditorNodes: DEFAULT_HWP_EDITOR_NODES, createEmptySlideStyleFields,
    parseCellRefParts, isExcludedDocChromeElement, extractEditableDocumentText,
    collectActiveScrubGroups, applyScrubResultsToGroups,
    scrubSheetStoreMap: sheetStoreHelpers.scrubSheetStoreMap, sheetController,
    getActiveModule: () => activeModule,
    getActiveSheetName: () => activeSheetName,
    getActiveSheetCellRef: () => activeSheetCellRef,
    getActiveSlideIndex: () => activeSlideIndex,
    setActiveSlideIndex: (next) => { activeSlideIndex = next; },
    getPdfRotationDeg: () => pdfRotationDeg,
    enqueueWorkspaceHistoryOp,
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot,
  });

  let paletteSelectedIdx = 0;
  let paletteFilteredItems = [];

  function executePaletteItem(item) {
    if (!item) return;
    const modal = document.getElementById("cmd-palette-modal");
    if (modal && typeof modal.close === "function") modal.close();
    if (item.id === "bench-run") {
      void workspacePanels.runBenchmarkAction().catch(() => {});
    } else if (item.id === "studio-save") {
      void workspacePanels.saveActiveModuleDocument().catch(() => {});
    } else if (item.id === "studio-undo") {
      void ribbonHistory.performWorkspaceUndo().catch(() => {});
    } else if (item.id === "studio-redo") {
      void ribbonHistory.performWorkspaceRedo().catch(() => {});
    } else {
      void ribbonHistory.executeTrackedRibbonCommand(item.id, { targetModule: item.module }).catch(() => {});
    }
  }

  function collectCommandPaletteItems() {
    const fmt = (sc) => (typeof formatPlatformShortcut === "function" ? formatPlatformShortcut(sc, activeShortcutModifier) : sc);
    const items = [
      { id: "studio-save", module: activeModule, title: "저장", shortcut: fmt("⌘S"), searchKey: "파일 저장 save " + fmt("⌘S") },
      { id: "studio-undo", module: activeModule, title: "실행 취소", shortcut: fmt("⌘Z"), searchKey: "편집 실행 취소 undo " + fmt("⌘Z") },
      { id: "studio-redo", module: activeModule, title: "다시 실행", shortcut: fmt("⌘Y"), searchKey: "편집 다시 실행 redo " + fmt("⌘Y") },
    ];
    const modOrder = [activeModule, ...Object.keys(RIBBON_CATEGORY_TOOLS).filter((m) => m !== activeModule)];
    for (const mod of modOrder) {
      const cats = RIBBON_CATEGORY_TOOLS[mod];
      if (!cats) continue;
      const koBadge = MODULE_SHORT_BADGES[mod] || mod;
      for (const cat of Object.keys(cats)) {
        for (const c of cats[cat]) {
          const dispShortcut = fmt(c.shortcut);
          const title = mod === activeModule ? c.title : koBadge + " · " + c.title;
          items.push({ id: c.id, module: mod, title, shortcut: dispShortcut, searchKey: (koBadge + " " + c.title + " " + dispShortcut + " " + c.id + " " + mod).toLowerCase() });
        }
      }
    }
    items.push({ id: "bench-run", module: activeModule, title: "문서 엔진 성능 진단 실행", shortcut: "F9", searchKey: "성능 문서 엔진 성능 진단 실행 bench-run f9" });
    return items;
  }

  function renderCommandPaletteList(filterText) {
    const listEl = document.getElementById("cmd-palette-list");
    if (!listEl) return;
    const q = String(filterText || "").trim().toLowerCase();
    paletteFilteredItems = collectCommandPaletteItems()
      .filter((item) => !q || item.searchKey.toLowerCase().includes(q) || item.title.toLowerCase().includes(q))
      .slice(0, 18);
    if (paletteSelectedIdx >= paletteFilteredItems.length) paletteSelectedIdx = 0;
    listEl.innerHTML = "";
    if (paletteFilteredItems.length === 0) {
      const emptyLi = document.createElement("li");
      emptyLi.className = "cp-empty";
      emptyLi.textContent = "검색 결과 없음";
      listEl.appendChild(emptyLi);
      return;
    }
    paletteFilteredItems.forEach((item, idx) => {
      const li = document.createElement("li");
      li.className = idx === paletteSelectedIdx ? "cp-item cp-item--selected" : "cp-item";
      li.setAttribute("role", "option");
      li.setAttribute("aria-selected", idx === paletteSelectedIdx ? "true" : "false");
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
    });
  }

  function syncQatShortcutTitles(modifier) {
    if (modifier !== "Ctrl") return;
    const qatSave = document.getElementById("qat-save");
    const qatUndo = document.getElementById("qat-undo");
    const qatRedo = document.getElementById("qat-redo");
    if (qatSave) qatSave.title = "저장 (Ctrl+S)";
    if (qatUndo) qatUndo.title = "실행 취소 (Ctrl+Z)";
    if (qatRedo) qatRedo.title = "다시 실행 (Ctrl+Y)";
    for (const sc of document.querySelectorAll(".md-shortcut")) {
      if (typeof formatPlatformShortcut === "function" && sc.textContent) {
        sc.textContent = formatPlatformShortcut(sc.textContent, modifier);
      }
    }
  }

  function setupTabsAndRibbonChrome(boot) {
    activeShortcutModifier = boot.shortcutModifier || "⌘";
    const pill = document.getElementById("platform-pill");
    const hint = document.getElementById("shortcut-hint");
    if (pill) pill.textContent = boot.platform + " · " + activeShortcutModifier;
    if (hint) {
      hint.textContent =
        activeShortcutModifier === "⌘" ? "⌘K" : activeShortcutModifier + "+K";
    }
    syncQatShortcutTitles(activeShortcutModifier);

    if (boot.moduleAccents && typeof boot.moduleAccents === "object") {
      const rootStyle = document.documentElement.style;
      const accentKeyMap = {
        DOC: "--eo-accent-doc",
        HWP: "--eo-accent-hwp",
        SHEET: "--eo-accent-sheet",
        SLIDE: "--eo-accent-slide",
        PDF: "--eo-accent-pdf",
      };
      Object.keys(accentKeyMap).forEach((modKey) => {
        const val = boot.moduleAccents[modKey];
        if (typeof val === "string" && val.trim()) {
          rootStyle.setProperty(accentKeyMap[modKey], val.trim());
        }
      });
    }
    ribbonHistory.applyModuleAccent(activeModule);

    const tabStrip = document.getElementById("tab-strip");
    if (tabStrip && Array.isArray(boot.openTabs)) {
      tabStrip.innerHTML = "";
      boot.openTabs.forEach((t, idx) => {
        const btn = document.createElement("button");
        const isAct = idx === 0;
        btn.className = "tab-pill" + (isAct ? " active" : "");
        btn.setAttribute("role", "tab");
        btn.setAttribute("aria-selected", isAct ? "true" : "false");
        btn.dataset.module = t.module;
        btn.textContent = t.title;
        btn.addEventListener("click", () => {
          void ribbonHistory.activatePane(t.module).catch(() => {});
        });
        tabStrip.appendChild(btn);
      });
    }

    const filterBox = document.getElementById("format-filters");
    if (filterBox && Array.isArray(boot.recentFormatFilters)) {
      filterBox.innerHTML = "";
      filterBox.dataset.activeFilter = "ALL";
      boot.recentFormatFilters.forEach((f, idx) => {
        const chip = document.createElement("button");
        chip.className = "filter-chip" + (idx === 0 ? " active" : "");
        chip.dataset.filter = f;
        chip.textContent = f;
        chip.addEventListener("click", () => {
          document.querySelectorAll(".filter-chip").forEach((c) => c.classList.remove("active"));
          chip.classList.add("active");
          filterBox.dataset.activeFilter = f === "전체" ? "ALL" : f;
          const map = {
            전체: "DOC",
            DOCX: "DOC",
            ODF: "DOC",
            "HWP/HWPX": "HWP",
            XLSX: "SHEET",
            PPTX: "SLIDE",
            PDF: "PDF",
          };
          if (map[f]) {
            void ribbonHistory.activatePane(map[f]).catch(() => {});
          }
        });
        filterBox.appendChild(chip);
      });
    }
  }

  function applyZoomPercent(pct, fitMode) {
    const nextZoom = Math.max(50, Math.min(200, Math.round(Number(pct) || 100)));
    localRibbonState.zoomPercent = nextZoom;
    const stage = document.querySelector(".canvas-stage");
    if (stage) {
      stage.style.zoom = nextZoom + "%";
      if (fitMode) stage.dataset.zoomFit = fitMode;
    }
    const zoomLabel = document.getElementById("status-zoom-label");
    if (zoomLabel) zoomLabel.textContent = nextZoom + "%";
    const zoomSlider = document.getElementById("status-zoom-slider");
    if (zoomSlider) zoomSlider.value = String(nextZoom);
  }

  function applyZoomStepDelta(delta) {
    const steps = [75, 90, 100, 125, 150];
    const curZoom = Number(localRibbonState.zoomPercent || 100);
    let idx = steps.indexOf(curZoom);
    if (idx < 0) idx = 2;
    const nextIdx = Math.max(0, Math.min(steps.length - 1, idx + delta));
    applyZoomPercent(steps[nextIdx]);
  }

  function setupStaticNavigationHandlers() {
    document.querySelectorAll(".mod-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        void ribbonHistory.activatePane(btn.dataset.module).catch(() => {});
      });
    });

    document.querySelectorAll(".ribbon-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        document.querySelectorAll(".ribbon-btn").forEach((b) => {
          const isAct = b === btn;
          b.classList.toggle("active", isAct);
          b.setAttribute("role", "tab");
          b.setAttribute("aria-selected", isAct ? "true" : "false");
        });
        void ribbonHistory.renderRibbonCategoryTools(btn.dataset.category).catch(() => {});
      });
    });

    const zoomOutBtn = document.getElementById("btn-zoom-out");
    const zoomInBtn = document.getElementById("btn-zoom-in");
    if (zoomOutBtn) {
      zoomOutBtn.addEventListener("click", () => applyZoomStepDelta(-1));
    }
    if (zoomInBtn) {
      zoomInBtn.addEventListener("click", () => applyZoomStepDelta(1));
    }
    workspacePanels.setupStudioChromeControls(applyZoomPercent, (cmd) => {
      const modPrefix = getRibbonModulePrefix(activeModule === "HWP" ? "HWP" : "DOC");
      const fullCmd =
        cmd.startsWith("doc-") || cmd.startsWith("hwp-") ? cmd : modPrefix + "-" + cmd;
      void ribbonHistory.executeTrackedRibbonCommand(fullCmd).catch(() => {});
    });
  }

  function setupQatAndRibbonActions() {
    const qatSave = document.getElementById("qat-save");
    if (qatSave) {
      qatSave.addEventListener("click", () => {
        void workspacePanels.saveActiveModuleDocument().catch(() => {});
      });
    }
    const qatUndo = document.getElementById("qat-undo");
    if (qatUndo) {
      qatUndo.addEventListener("click", () => {
        void ribbonHistory.performWorkspaceUndo().catch(() => {});
      });
    }
    const qatRedo = document.getElementById("qat-redo");
    if (qatRedo) {
      qatRedo.addEventListener("click", () => {
        void ribbonHistory.performWorkspaceRedo().catch(() => {});
      });
    }
    const btnBench = document.getElementById("btn-run-benchmark");
    if (btnBench) {
      btnBench.addEventListener("click", () => {
        void workspacePanels.runBenchmarkAction().catch(() => {});
      });
    }

    const btnRibbonStyle = document.getElementById("btn-ribbon-style");
    if (btnRibbonStyle) {
      btnRibbonStyle.addEventListener("click", () => {
        void ribbonHistory.switchRibbonStyleTracked().catch(() => {});
      });
    }

    const themeBtn = document.getElementById("btn-theme-toggle");
    if (themeBtn) {
      themeBtn.addEventListener("click", () => {
        void ribbonHistory
          .enqueueWorkspaceHistoryOp(async () => {
            await sheetController.commitPendingFocusedSheetCell(true);
            ribbonHistory.pushUndoSnapshot(false);
            const html = document.documentElement;
            const next = html.dataset.theme === "dark" ? "light" : "dark";
            html.dataset.theme = next;
            localRibbonState.darkThemeEnabled = next === "dark";
            themeBtn.textContent = next === "dark" ? "라이트 모드" : "다크 모드";
            sheetController.refreshFocusedSheetCellPendingSnapshot();
          })
          .catch(() => {});
      });
    }
  }

  function getRibbonModulePrefix(mod) {
    return mod === "DOC" || mod === "CONV" ? "document" : String(mod || "DOC").toLowerCase();
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
          const modPrefix = getRibbonModulePrefix(activeModule === "HWP" ? "HWP" : "DOC");
          void ribbonHistory
            .executeTrackedRibbonCommand(modPrefix + "-home-" + cmdSuffix)
            .catch(() => {});
        });
      }
    });
    const docEditor = document.getElementById("doc-editor");
    if (docEditor) {
      docEditor.addEventListener("click", (e) => {
        const addRowBtn = e.target.closest("#btn-doc-add-row");
        if (addRowBtn) {
          ribbonHistory.pushUndoSnapshot(false);
          workspacePanels.appendDocTableRow();
          return;
        }
        const mergeBtn = e.target.closest("#btn-doc-merge-toggle");
        if (mergeBtn) {
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
          return;
        }
        const evalDocBtn = e.target.closest("#btn-doc-eval-table");
        if (evalDocBtn) {
          workspacePanels.computeDocTableSummary();
        }
      });
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
        void sheetController.switchSheetTab(targetSheet).catch(() => {});
      });
    }
    if (addSheetBtn && sheetTabBar) {
      addSheetBtn.addEventListener("click", () => {
        const runAddSheet = () =>
          enqueueWorkspaceHistoryOp(async () => {
            await sheetController.flushFocusedSheetCellBeforeSwitch();
            ribbonHistory.pushUndoSnapshot(false);
            const count =
              sheetTabBar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]").length + 1;
            const sheetKey = "Sheet" + count;
            const newTab = document.createElement("button");
            newTab.className = "sheet-tab-btn";
            newTab.dataset.sheetTab = sheetKey;
            newTab.textContent = "시트 " + count;
            addSheetBtn.before(newTab);
            await sheetController.performSwitchSheetTabAsync(sheetKey);
          });
        void runAddSheet().catch(() => {});
      });
    }
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
            const body = workspacePanels.buildStructuredConversionBody(src);
            const res = await invokeIpc(
              "convert_document_format",
              JSON.stringify({
                sourceFormat: src,
                targetFormat: dst,
                title: "통합_변환_문서",
                body,
              })
            );
            const box = document.getElementById("conv-result-box");
            if (res && res.convertedBase64) {
              workspacePanels.triggerBase64Download(res.convertedBase64, "통합_변환_문서", dst);
            }
            if (box && res) {
              box.textContent =
                "[" +
                res.status +
                "] " +
                res.sourceFormat.toUpperCase() +
                " (" +
                res.sourceBytes +
                "B) → " +
                res.targetFormat.toUpperCase() +
                " (" +
                res.outputBytes +
                "B) · " +
                res.specification;
            }
          });
        void runConvert().catch(() => {});
      });
    }

    const sheetPane = document.getElementById("pane-sheet");
    if (sheetPane) {
      sheetController.bindSheetCellEvents(sheetPane);
    }

    const evalBtn = document.getElementById("btn-eval-formula");
    const formulaInputEl = document.getElementById("formula-input");
    if (formulaInputEl && evalBtn) {
      formulaInputEl.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          evalBtn.click();
        }
      });
    }
    if (evalBtn) {
      evalBtn.addEventListener("click", () => {
        void sheetController.evaluateActiveSheetFormulaBar(true).catch(() => {});
      });
    }
  }

  function setupSlideAndPdfHandlers() {
    const btnAddSlide = document.getElementById("btn-add-slide");
    if (btnAddSlide) {
      btnAddSlide.addEventListener("click", () => {
        ribbonHistory.pushUndoSnapshot(false);
        workspacePanels.appendNewSlideDeckItem();
      });
    }
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
    let slideTypingSnapshotEpoch = -1;
    let slideTypingTarget = null;
    let slideTypingSlideIndex = -1;
    function resetSlideTypingSnapshotState() {
      slideTypingSnapshotEpoch = -1;
      slideTypingTarget = null;
      slideTypingSlideIndex = -1;
    }
    function ensureSlideTypingSnapshot(targetEl) {
      if (
        slideTypingTarget !== targetEl ||
        slideTypingSlideIndex !== activeSlideIndex ||
        slideTypingSnapshotEpoch !== ribbonHistory.getHistoryMutationEpoch()
      ) {
        const prevSlides = JSON.parse(JSON.stringify(slideDeckState));
        const preSnap = ribbonHistory.captureWorkspaceSnapshot(false);
        preSnap.slides = prevSlides;
        ribbonHistory.pushRawUndoSnapshot(preSnap);
        slideTypingSnapshotEpoch = ribbonHistory.getHistoryMutationEpoch();
        slideTypingTarget = targetEl;
        slideTypingSlideIndex = activeSlideIndex;
      }
    }
    const slideNotesInput = document.getElementById("slide-presenter-notes");
    if (slideNotesInput) {
      slideNotesInput.addEventListener("focus", resetSlideTypingSnapshotState);
      slideNotesInput.addEventListener("blur", resetSlideTypingSnapshotState);
      slideNotesInput.addEventListener("beforeinput", () => {
        ensureSlideTypingSnapshot(slideNotesInput);
      });
      slideNotesInput.addEventListener("input", () => {
        ensureSlideTypingSnapshot(slideNotesInput);
        if (slideDeckState[activeSlideIndex]) {
          slideDeckState[activeSlideIndex].notes = slideNotesInput.value;
        }
      });
    }
    const slideTitleZone = document.getElementById("slide-title-zone");
    if (slideTitleZone) {
      slideTitleZone.addEventListener("focus", resetSlideTypingSnapshotState);
      slideTitleZone.addEventListener("blur", resetSlideTypingSnapshotState);
      slideTitleZone.addEventListener("beforeinput", () => {
        ensureSlideTypingSnapshot(slideTitleZone);
      });
      slideTitleZone.addEventListener("input", () => {
        ensureSlideTypingSnapshot(slideTitleZone);
        if (slideDeckState[activeSlideIndex]) {
          slideDeckState[activeSlideIndex].title = slideTitleZone.textContent || "";
        }
      });
    }
    const slideLowerZone = document.getElementById("slide-lower-zone");
    if (slideLowerZone) {
      slideLowerZone.addEventListener("focus", resetSlideTypingSnapshotState);
      slideLowerZone.addEventListener("blur", resetSlideTypingSnapshotState);
      slideLowerZone.addEventListener("beforeinput", () => {
        ensureSlideTypingSnapshot(slideLowerZone);
      });
      slideLowerZone.addEventListener("input", () => {
        ensureSlideTypingSnapshot(slideLowerZone);
        if (slideDeckState[activeSlideIndex]) {
          workspacePanels.syncActiveSlideDomStylesToDeck();
        }
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
        btnSlideLaser.textContent = slideLaserActive ? "레이저 포인터: 켬" : "레이저 포인터: 끔";
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
    const btnPdfApply = document.getElementById("btn-pdf-apply");
    if (btnPdfApply) {
      btnPdfApply.addEventListener("click", () => {
        void ribbonHistory
          .enqueueWorkspaceHistoryOp(() => {
            ribbonHistory.pushUndoSnapshot(false);
            workspacePanels.applyPdfStudioFormState();
          })
          .catch(() => {});
      });
    }
  }

  function setupSmartAndSecurityHandlers() {
    const editor = document.getElementById("doc-editor");
    const imeBadge =
      document.getElementById("ime-badge") || document.getElementById("ime-indicator");
    let docTypingSnapshotEpoch = -1;
    function ensureDocTypingSnapshot() {
      if (activeModule !== "DOC" && activeModule !== "HWP") return;
      if (
        docTypingSnapshotEpoch !== ribbonHistory.getHistoryMutationEpoch() &&
        docTabStore[activeModule]
      ) {
        const prevDocTab = JSON.parse(JSON.stringify(docTabStore[activeModule]));
        const preSnap = ribbonHistory.captureWorkspaceSnapshot(false);
        preSnap.docTabs[activeModule] = prevDocTab;
        ribbonHistory.pushRawUndoSnapshot(preSnap);
        docTypingSnapshotEpoch = ribbonHistory.getHistoryMutationEpoch();
      }
    }
    if (editor) {
      editor.addEventListener("focus", () => {
        docTypingSnapshotEpoch = -1;
        workspacePanels.saveActiveDocTabState();
      });
      editor.addEventListener("blur", () => {
        docTypingSnapshotEpoch = -1;
        workspacePanels.saveActiveDocTabState();
      });
      editor.addEventListener("click", (ev) => {
        const block = workspacePanels.findEditableDocBlock(ev.target, editor);
        if (block) workspacePanels.setActiveDocBlock(block);
      });
      editor.addEventListener("compositionstart", () => {
        isImeComposing = true;
        if (imeBadge) imeBadge.textContent = "IME: 조합 중(한글)";
      });
      editor.addEventListener("compositionend", () => {
        isImeComposing = false;
        if (imeBadge) imeBadge.textContent = "IME: 완료";
        workspacePanels.computeSmartStats();
      });
      editor.addEventListener("beforeinput", () => {
        ensureDocTypingSnapshot();
      });
      editor.addEventListener("input", () => {
        if (activeModule === "DOC" || activeModule === "HWP") {
          ensureDocTypingSnapshot();
          workspacePanels.saveActiveDocTabState();
        }
        if (!isImeComposing) workspacePanels.computeSmartStats();
      });
      workspacePanels.computeSmartStats();
    }

    const diagnoseBtn = document.getElementById("btn-smart-diagnose");
    if (diagnoseBtn) {
      diagnoseBtn.addEventListener("click", () => {
        void workspacePanels.runSmartDiagnoseAction().catch(() => {});
      });
    }

    const auditBtn =
      document.getElementById("btn-run-security-audit") ||
      document.getElementById("btn-security-audit");
    if (auditBtn) {
      auditBtn.addEventListener("click", () => {
        void workspacePanels.runSecurityAuditAction().catch(() => {});
      });
    }

    const piiBtn = document.getElementById("btn-scrub-pii");
    if (piiBtn) {
      piiBtn.addEventListener("click", () => {
        void workspacePanels.runPiiScrubAction(true).catch(() => {});
      });
    }
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

  function setupCommandPaletteHandlers() {
    const modal = document.getElementById("cmd-palette-modal");
    const openPaletteBtn = document.getElementById("btn-command-palette");
    const closePaletteBtn = document.getElementById("btn-close-palette");
    const cmdSearchInput = document.getElementById("cmd-search-input");
    if (openPaletteBtn && modal) {
      openPaletteBtn.addEventListener("click", () => {
        paletteSelectedIdx = 0;
        renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
        if (typeof modal.showModal === "function") modal.showModal();
        if (cmdSearchInput) cmdSearchInput.focus();
      });
    }
    if (cmdSearchInput) {
      cmdSearchInput.addEventListener("input", () => {
        paletteSelectedIdx = 0;
        renderCommandPaletteList(cmdSearchInput.value);
      });
    }
    if (closePaletteBtn && modal) {
      closePaletteBtn.addEventListener("click", () => {
        if (typeof modal.close === "function") modal.close();
      });
    }
    modal?.addEventListener?.("click", (ev) => {
      if (ev.target === modal && typeof modal.close === "function") modal.close();
    });

    host.addEventListener("keydown", (e) => {
      if (e.isComposing || isImeComposing) return;
      if (modal?.open) {
        if (e.key === "Escape" && typeof modal.close === "function") return void modal.close();
        if (e.key === "ArrowDown" && paletteFilteredItems.length > 0) {
          e.preventDefault();
          paletteSelectedIdx = Math.min(paletteFilteredItems.length - 1, paletteSelectedIdx + 1);
          renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
          return;
        }
        if (e.key === "ArrowUp" && paletteFilteredItems.length > 0) {
          e.preventDefault();
          paletteSelectedIdx = Math.max(0, paletteSelectedIdx - 1);
          renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
          return;
        }
        if (e.key === "Enter" && paletteFilteredItems[paletteSelectedIdx]) {
          e.preventDefault();
          executePaletteItem(paletteFilteredItems[paletteSelectedIdx]);
          return;
        }
      }
      if (!e.metaKey && !e.ctrlKey) return;
      const keyLower = e.key.toLowerCase();
      const modPrefix = getRibbonModulePrefix(activeModule);
      if (e.altKey && !e.shiftKey && keyLower === "d") {
        e.preventDefault();
        void ribbonHistory.executeTrackedRibbonCommand(modPrefix + "-view-theme").catch(() => {});
        return;
      }
      if (e.altKey) return;
      if (e.shiftKey && keyLower === "z") {
        e.preventDefault();
        void ribbonHistory.performWorkspaceRedo().catch(() => {});
        return;
      }
      if (e.shiftKey) return;
      const plainHandlers = {
        k: () => togglePaletteModal(modal, cmdSearchInput),
        s: () => void workspacePanels.saveActiveModuleDocument().catch(() => {}),
        z: () => void ribbonHistory.performWorkspaceUndo().catch(() => {}),
        y: () => void ribbonHistory.performWorkspaceRedo().catch(() => {}),
        0: () => void ribbonHistory.executeTrackedRibbonCommand(modPrefix + "-view-zoom").catch(() => {}),
        b: () => void ribbonHistory.executeTrackedRibbonCommand(modPrefix + "-home-bold").catch(() => {}),
        i: () => void ribbonHistory.executeTrackedRibbonCommand(modPrefix + "-home-italic").catch(() => {}),
        u: () => void ribbonHistory.executeTrackedRibbonCommand(modPrefix + "-home-underline").catch(() => {}),
      };
      if (plainHandlers[keyLower]) {
        e.preventDefault();
        plainHandlers[keyLower]();
      }
    });
  }

  async function initWorkspace() {
    workspacePanels.ensureDefaultDocTabStore();
    sheetDataStore.Sheet1 = snapshotDomSheetCells();
    sheetRowCounts.Sheet1 = sheetController.getSheetRowCount("Sheet1");
    sheetRowCounts.Sheet2 = sheetController.getSheetRowCount("Sheet2");
    setupStaticNavigationHandlers();
    setupQatAndRibbonActions();
    setupDocTableAndSheetTabHandlers();
    setupSheetAndConverterHandlers();
    setupSlideAndPdfHandlers();
    setupSmartAndSecurityHandlers();
    setupCommandPaletteHandlers();
    workspacePanels.renderModuleOutline("DOC");
    sheetController.updateSheetSelectionAndAggregates("F2");
    workspacePanels.renderSlideDeck(0);
    const boot = await invokeIpc("get_workspace_bootstrap", "");
    setupTabsAndRibbonChrome(boot);
    void ribbonHistory.renderRibbonCategoryTools("home").catch(() => {});
  }

  document.addEventListener("DOMContentLoaded", () => {
    void initWorkspace().catch(() => {});
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
