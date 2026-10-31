(function (host) {
  "use strict";

  const document = host.document;
  const URL = host.URL;

  const ribbonCatalog = host.__EASYOFFICE_RIBBON_CATALOG__;
  const {
    MODULE_SPECS,
    CONTEXTUAL_TOOL_BADGES,
    MODULE_SHORT_BADGES,
    RIBBON_STYLE_LABELS,
    RIBBON_CATEGORY_ORDER,
    RIBBON_CATEGORY_TOOLS,
    DEFAULT_HWP_EDITOR_HTML,
    createEmptySlideStyleFields,
    createInitialSlideDeckState,
    createDefaultRibbonState,
  } = ribbonCatalog;

  const slideDeckState = createInitialSlideDeckState();
  const localRibbonState = createDefaultRibbonState();

  let isImeComposing = false;
  let activeModule = "DOC";
  let activeRibbonStyle = "standard";
  let activeRibbonCategory = "home";
  let activeSheetName = "Sheet1";
  let activeSheetCellRef = "F2";
  let activeSlideIndex = 0;
  let slideLaserActive = false;
  let pdfRotationDeg = 0;
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

  const MOCK_IPC_HANDLERS = host.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => activeModule,
    getActiveSheetCellRef: () => activeSheetCellRef,
    ribbonCategoryTools: RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: RIBBON_STYLE_LABELS,
    contextualToolBadges: CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: MODULE_SPECS,
    localRibbonState,
  });

  async function invokeIpc(command, payload) {
    if (host.__TAURI_INTERNALS__ && typeof host.__TAURI_INTERNALS__.invoke === "function") {
      try {
        const raw = await host.__TAURI_INTERNALS__.invoke(command, { payload });
        return typeof raw === "string" ? JSON.parse(raw) : raw;
      } catch (tauriErr) {
        const msg = tauriErr?.message || String(tauriErr);
        reportIpcFailure(command, msg);
        throw new Error(msg);
      }
    }
    if (isHttpIpcTransportAvailable()) {
      return await invokeHttpIpc(command, payload);
    }
    const handler = MOCK_IPC_HANDLERS[command];
    if (!handler) return { status: "ok" };
    try {
      return await handler(payload);
    } catch (offlineErr) {
      const msg = offlineErr?.message || String(offlineErr);
      reportIpcFailure(command, msg);
      throw new Error(msg);
    }
  }

  function isHttpIpcTransportAvailable() {
    const proto = host.location?.protocol;
    return (proto === "http:" || proto === "https:") && typeof host.fetch === "function";
  }

  function reportIpcFailure(command, msg) {
    const fb = document.getElementById("status-command-feedback");
    if (fb) fb.textContent = "[요청 거부] " + msg;
    let targetId = null;
    if (command === "convert_document_format") {
      targetId = "conv-result-box";
    } else if (command === "audit_security") {
      targetId = "security-status-text";
    }
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
    invokeIpc,
    normalizeFormattedNumericCell,
    tokenizeFormulaStringLiterals,
  });

  const ribbonHistory = host.__EASYOFFICE_CREATE_RIBBON_HISTORY__({
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
    getSheetController: () => sheetController,
    getWorkspacePanels: () => workspacePanels,
    getActiveModule: () => activeModule,
    setActiveModule: (next) => {
      activeModule = next;
    },
    getActiveRibbonCategory: () => activeRibbonCategory,
    setActiveRibbonCategory: (next) => {
      activeRibbonCategory = next;
    },
    getActiveSheetName: () => activeSheetName,
    setActiveSheetName: (next) => {
      activeSheetName = next;
    },
    getActiveSheetCellRef: () => activeSheetCellRef,
    getActiveSlideIndex: () => activeSlideIndex,
    getPdfRotationDeg: () => pdfRotationDeg,
    setPdfRotationDeg: (next) => {
      pdfRotationDeg = next;
    },
    enqueueWorkspaceHistoryOp,
  });

  sheetController = host.__EASYOFFICE_CREATE_SHEET_CONTROLLER__({
    document,
    sheetDataStore,
    sheetRowCounts,
    sheetStoreHelpers,
    getActiveModule: () => activeModule,
    getActiveSheetName: () => activeSheetName,
    setActiveSheetName: (next) => {
      activeSheetName = next;
    },
    getActiveSheetCellRef: () => activeSheetCellRef,
    setActiveSheetCellRef: (next) => {
      activeSheetCellRef = next;
    },
    enqueueWorkspaceHistoryOp,
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    pushRawUndoSnapshot: ribbonHistory.pushRawUndoSnapshot,
    commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot,
    onSheetSwitchedOutline: () => workspacePanels.renderModuleOutline("SHEET"),
  });

  workspacePanels = host.__EASYOFFICE_CREATE_WORKSPACE_PANELS__({
    document,
    URL,
    invokeIpc,
    docTabStore,
    sheetDataStore,
    slideDeckState,
    localRibbonState,
    defaultHwpEditorHtml: DEFAULT_HWP_EDITOR_HTML,
    createEmptySlideStyleFields,
    parseCellRefParts,
    isExcludedDocChromeElement,
    extractEditableDocumentText,
    collectActiveScrubGroups,
    applyScrubResultsToGroups,
    scrubSheetStoreMap: sheetStoreHelpers.scrubSheetStoreMap,
    sheetController,
    getActiveModule: () => activeModule,
    getActiveSheetName: () => activeSheetName,
    getActiveSheetCellRef: () => activeSheetCellRef,
    getActiveSlideIndex: () => activeSlideIndex,
    setActiveSlideIndex: (next) => {
      activeSlideIndex = next;
    },
    getPdfRotationDeg: () => pdfRotationDeg,
    enqueueWorkspaceHistoryOp,
    captureWorkspaceSnapshot: ribbonHistory.captureWorkspaceSnapshot,
    commitDeferredUndoSnapshot: ribbonHistory.commitDeferredUndoSnapshot,
  });

  function renderCommandPaletteList(filterText) {
    const listEl = document.getElementById("cmd-palette-list");
    if (!listEl) return;
    const q = String(filterText || "").trim().toLowerCase();
    const allItems = [];
    Object.keys(RIBBON_CATEGORY_TOOLS).forEach((mod) => {
      const cats = RIBBON_CATEGORY_TOOLS[mod];
      const koBadge = MODULE_SHORT_BADGES[mod] || mod;
      Object.keys(cats).forEach((cat) => {
        cats[cat].forEach((c) => {
          allItems.push({
            id: c.id,
            module: mod,
            label: "[" + koBadge + "] " + c.title + " (" + c.shortcut + ")",
          });
        });
      });
    });
    allItems.push({
      id: "bench-run",
      module: activeModule,
      label: "[성능] 문서 엔진 성능 진단 실행",
    });

    listEl.innerHTML = "";
    allItems
      .filter(
        (item) =>
          !q ||
          item.label.toLowerCase().includes(q) ||
          item.id.toLowerCase().includes(q) ||
          item.module.toLowerCase().includes(q)
      )
      .slice(0, 18)
      .forEach((item) => {
        const li = document.createElement("li");
        li.textContent = item.label;
        li.addEventListener("click", () => {
          const modal = document.getElementById("cmd-palette-modal");
          if (modal && typeof modal.close === "function") modal.close();
          if (item.id === "bench-run") {
            void workspacePanels.runBenchmarkAction().catch(() => {});
          } else {
            void ribbonHistory
              .executeTrackedRibbonCommand(item.id, { targetModule: item.module })
              .catch(() => {});
          }
        });
        listEl.appendChild(li);
      });
  }

  function setupTabsAndRibbonChrome(boot) {
    const pill = document.getElementById("platform-pill");
    const hint = document.getElementById("shortcut-hint");
    if (pill) pill.textContent = boot.platform + " · " + boot.shortcutModifier;
    if (hint) hint.textContent = boot.shortcutModifier + "K";

    const tabStrip = document.getElementById("tab-strip");
    if (tabStrip && Array.isArray(boot.openTabs)) {
      tabStrip.innerHTML = "";
      boot.openTabs.forEach((t, idx) => {
        const btn = document.createElement("button");
        btn.className = "tab-pill" + (idx === 0 ? " active" : "");
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
      boot.recentFormatFilters.forEach((f, idx) => {
        const chip = document.createElement("button");
        chip.className = "filter-chip" + (idx === 0 ? " active" : "");
        chip.textContent = f;
        chip.addEventListener("click", () => {
          document.querySelectorAll(".filter-chip").forEach((c) => c.classList.remove("active"));
          chip.classList.add("active");
          const map = {
            DOCX: "DOC",
            "HWP/HWPX": "HWP",
            XLSX: "SHEET",
            PPTX: "SLIDE",
            PDF: "PDF",
            ODF: "CONV",
          };
          if (map[f]) {
            void ribbonHistory.activatePane(map[f]).catch(() => {});
          }
        });
        filterBox.appendChild(chip);
      });
    }

    document.querySelectorAll(".mod-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        void ribbonHistory.activatePane(btn.dataset.module).catch(() => {});
      });
    });

    document.querySelectorAll(".ribbon-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        document.querySelectorAll(".ribbon-btn").forEach((b) => b.classList.remove("active"));
        btn.classList.add("active");
        void ribbonHistory.renderRibbonCategoryTools(btn.dataset.category).catch(() => {});
      });
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
        const runSwitch = async () => {
          activeRibbonStyle = activeRibbonStyle === "standard" ? "hangul" : "standard";
          localRibbonState.ribbonStyle = activeRibbonStyle;
          const res = await invokeIpc("switch_ribbon_style", activeRibbonStyle);
          btnRibbonStyle.textContent =
            activeRibbonStyle === "hangul" ? "리본: 한글(HWP) 7탭" : "리본: 표준 오피스";
          const labels = res?.categories || RIBBON_STYLE_LABELS[activeRibbonStyle];
          const keys = RIBBON_CATEGORY_ORDER;
          let nextActiveCat = activeRibbonCategory;
          document.querySelectorAll("#ribbon-tabs .ribbon-btn").forEach((b, idx) => {
            if (labels[idx]) b.textContent = labels[idx];
            if (keys[idx]) {
              b.dataset.category = keys[idx];
              if (b.classList.contains("active")) {
                nextActiveCat = keys[idx];
              }
            }
          });
          await ribbonHistory.renderRibbonCategoryTools(nextActiveCat);
        };
        void runSwitch().catch(() => {});
      });
    }

    const themeBtn = document.getElementById("btn-theme-toggle");
    if (themeBtn) {
      themeBtn.addEventListener("click", () => {
        ribbonHistory.pushUndoSnapshot(false);
        const html = document.documentElement;
        const next = html.dataset.theme === "dark" ? "light" : "dark";
        html.dataset.theme = next;
        localRibbonState.darkThemeEnabled = next === "dark";
        themeBtn.textContent = next === "dark" ? "라이트 모드" : "다크 모드";
      });
    }
  }

  function setupDocTableAndSheetTabHandlers() {
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
        const runConvert = async () => {
          await sheetController.commitPendingFocusedSheetCell(true);
          const src = document.getElementById("conv-src-fmt").value;
          const dst = document.getElementById("conv-dst-fmt").value;
          const body = workspacePanels.buildStructuredConversionBody(src);
          const res = await invokeIpc(
            "convert_document_format",
            src + "->" + dst + "|통합_변환_문서|" + body
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
        };
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
    const slideNotesInput = document.getElementById("slide-presenter-notes");
    if (slideNotesInput) {
      slideNotesInput.addEventListener("input", () => {
        if (slideDeckState[activeSlideIndex]) {
          slideDeckState[activeSlideIndex].notes = slideNotesInput.value;
        }
      });
    }
    const slideTitleZone = document.getElementById("slide-title-zone");
    if (slideTitleZone) {
      slideTitleZone.addEventListener("input", () => {
        if (slideDeckState[activeSlideIndex]) {
          slideDeckState[activeSlideIndex].title = slideTitleZone.textContent || "";
        }
      });
    }
    const slideLowerZone = document.getElementById("slide-lower-zone");
    if (slideLowerZone) {
      slideLowerZone.addEventListener("input", () => {
        if (slideDeckState[activeSlideIndex]) {
          slideDeckState[activeSlideIndex].lower = slideLowerZone.textContent || "";
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
        ribbonHistory.pushUndoSnapshot(false);
        workspacePanels.applyPdfStudioFormState();
      });
    }
  }

  function setupSmartAndSecurityHandlers() {
    const editor = document.getElementById("doc-editor");
    const imeBadge =
      document.getElementById("ime-badge") || document.getElementById("ime-indicator");
    if (editor) {
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
      editor.addEventListener("input", () => {
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

    const auditBtn = document.getElementById("btn-run-security-audit");
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
    if (modal.open && typeof modal.close === "function") {
      modal.close();
      return;
    }
    if (typeof modal.showModal === "function") {
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
        renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
        if (typeof modal.showModal === "function") modal.showModal();
      });
    }
    if (cmdSearchInput) {
      cmdSearchInput.addEventListener("input", () => {
        renderCommandPaletteList(cmdSearchInput.value);
      });
    }
    if (closePaletteBtn && modal) {
      closePaletteBtn.addEventListener("click", () => {
        if (typeof modal.close === "function") modal.close();
      });
    }

    host.addEventListener("keydown", (e) => {
      if (e.isComposing || isImeComposing) return;
      const keyLower = e.key.toLowerCase();
      if ((e.metaKey || e.ctrlKey) && keyLower === "k") {
        e.preventDefault();
        togglePaletteModal(modal, cmdSearchInput);
      } else if ((e.metaKey || e.ctrlKey) && keyLower === "s") {
        e.preventDefault();
        void workspacePanels.saveActiveModuleDocument().catch(() => {});
      } else if ((e.metaKey || e.ctrlKey) && !e.shiftKey && keyLower === "z") {
        e.preventDefault();
        void ribbonHistory.performWorkspaceUndo().catch(() => {});
      } else if (
        (e.metaKey || e.ctrlKey) &&
        (keyLower === "y" || (e.shiftKey && keyLower === "z"))
      ) {
        e.preventDefault();
        void ribbonHistory.performWorkspaceRedo().catch(() => {});
      } else if (e.key === "Escape" && modal?.open && typeof modal.close === "function") {
        modal.close();
      }
    });
  }

  async function initWorkspace() {
    workspacePanels.ensureDefaultDocTabStore();
    sheetDataStore.Sheet1 = snapshotDomSheetCells();
    sheetRowCounts.Sheet1 = sheetController.getSheetRowCount("Sheet1");
    sheetRowCounts.Sheet2 = sheetController.getSheetRowCount("Sheet2");
    const boot = await invokeIpc("get_workspace_bootstrap", "");
    setupTabsAndRibbonChrome(boot);
    workspacePanels.renderModuleOutline("DOC");
    void ribbonHistory.renderRibbonCategoryTools("home").catch(() => {});
    sheetController.updateSheetSelectionAndAggregates("F2");
    workspacePanels.renderSlideDeck(0);
    setupQatAndRibbonActions();
    setupDocTableAndSheetTabHandlers();
    setupSheetAndConverterHandlers();
    setupSlideAndPdfHandlers();
    setupSmartAndSecurityHandlers();
    setupCommandPaletteHandlers();
  }

  document.addEventListener("DOMContentLoaded", () => {
    void initWorkspace().catch(() => {});
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
