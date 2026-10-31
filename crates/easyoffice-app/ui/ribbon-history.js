(function (host) {
  "use strict";

  const MAX_UNDO_SNAPSHOTS = 64;

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
      getActiveModule,
      setActiveModule,
      getActiveRibbonCategory,
      setActiveRibbonCategory,
      getActiveSheetName,
      setActiveSheetName,
      getActiveSheetCellRef,
      getActiveSlideIndex,
      getPdfRotationDeg,
      setPdfRotationDeg,
      enqueueWorkspaceHistoryOp,
    } = deps;

    const {
      MODULE_ACCENTS,
      MODULE_LABELS,
      MODULE_SPECS,
      CONTEXTUAL_TOOL_BADGES,
      syncLocalRibbonStateFromBackend,
    } = ribbonCatalog;

    const {
      sheetMapHasAnyFormula,
      sheetCellDataChanged,
      computeEvaluatedSheetStore,
    } = sheetStoreHelpers;

    const workspaceUndoStack = [];
    const workspaceRedoStack = [];
    let ribbonRenderSeq = 0;

    function pushRawUndoSnapshot(snap) {
      if (!snap) return;
      workspaceUndoStack.push(snap);
      if (workspaceUndoStack.length > MAX_UNDO_SNAPSHOTS) {
        workspaceUndoStack.shift();
      }
      workspaceRedoStack.length = 0;
    }

    function commitDeferredUndoSnapshot(preSnapshot) {
      pushRawUndoSnapshot(preSnapshot);
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
        theme: document.documentElement.dataset.theme || "light",
        docTabs: JSON.parse(JSON.stringify(docTabStore)),
        sheets: JSON.parse(JSON.stringify(sheetDataStore)),
        sheetRowCounts: JSON.parse(JSON.stringify(sheetRowCounts)),
        slides: JSON.parse(JSON.stringify(slideDeckState)),
        ribbonState: JSON.parse(JSON.stringify(localRibbonState)),
      };
    }

    function pushUndoSnapshot(isRibbon = false) {
      pushRawUndoSnapshot(captureWorkspaceSnapshot(isRibbon));
    }

    function applyModuleAccent(mod) {
      setActiveModule(mod);
      localRibbonState.activeModule = mod;
      const accent = MODULE_ACCENTS[mod] || "var(--eo-primary)";
      document.documentElement.style.setProperty("--eo-module-active", accent);
      const swatch = document.getElementById("module-swatch");
      const label = document.getElementById("module-accent-label");
      if (swatch) swatch.style.background = accent;
      if (label) label.textContent = "모듈 색상: " + (MODULE_LABELS[mod] || mod);

      const statusMod = document.getElementById("status-module");
      const statusSpec = document.getElementById("status-format-spec");
      if (statusMod) statusMod.textContent = "모듈: " + (MODULE_LABELS[mod] || mod);
      if (statusSpec) statusSpec.textContent = "포맷: " + (MODULE_SPECS[mod] || "");

      const ctxBadge = document.getElementById("contextual-tool-badge");
      if (ctxBadge) ctxBadge.textContent = CONTEXTUAL_TOOL_BADGES[mod] || "표·개체 도구";

      const hwpNotice = document.getElementById("hwp-manuscript-preview");
      if (hwpNotice) {
        hwpNotice.hidden = mod !== "HWP" && !localRibbonState.manuscriptGridEnabled;
      }
    }

    async function renderRibbonCategoryTools(categoryKey) {
      const seq = ribbonRenderSeq + 1;
      ribbonRenderSeq = seq;
      setActiveRibbonCategory(categoryKey || "home");
      const catKey = getActiveRibbonCategory();
      localRibbonState.activeCategory = catKey;
      const strip = document.getElementById("ribbon-tool-strip");
      if (!strip) return;
      const curMod = getActiveModule();
      const modKey = curMod === "CONV" ? "DOC" : curMod;
      const res = await invokeIpc("get_ribbon_category_commands", modKey + "|" + catKey);
      if (seq !== ribbonRenderSeq) return;
      const cmds = (res && Array.isArray(res.commands) && res.commands) || [];
      strip.innerHTML = "";
      cmds.forEach((cmd) => {
        const btn = document.createElement("button");
        btn.className = "ribbon-tool-btn";
        btn.dataset.commandId = cmd.id;
        btn.innerHTML =
          "<span>" + cmd.title + "</span> <small>(" + (cmd.shortcut || "") + ")</small>";
        btn.addEventListener("click", () => {
          void executeTrackedRibbonCommand(cmd.id, { fallbackTitle: cmd.title }).catch(() => {});
        });
        strip.appendChild(btn);
      });
    }

    async function activatePaneInternal(mod, skipTabBufferSync) {
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
      if (!skipTabBufferSync && (mod === "DOC" || mod === "HWP") && mod !== curMod) {
        workspacePanels.restoreDocTabState(mod);
      }
      applyModuleAccent(mod);
      const panes = {
        DOC: document.getElementById("pane-doc"),
        HWP: document.getElementById("pane-doc"),
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

      document.querySelectorAll(".mod-btn").forEach((btn) => {
        btn.classList.toggle("active", btn.dataset.module === mod);
      });
      document.querySelectorAll(".tab-pill").forEach((tab) => {
        tab.classList.toggle("active", tab.dataset.module === mod);
      });
      workspacePanels.renderModuleOutline(mod);
      await renderRibbonCategoryTools(getActiveRibbonCategory()).catch(() => {});
    }

    function activatePane(mod, skipTabBufferSync) {
      return enqueueWorkspaceHistoryOp(() => activatePaneInternal(mod, skipTabBufferSync));
    }

    async function restoreWorkspaceSnapshot(snap, backendSyncRes) {
      if (!snap) return;
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      sheetController.bumpSnapshotEpoch();
      if (snap.docTabs) {
        docTabStore.DOC = snap.docTabs.DOC ? JSON.parse(JSON.stringify(snap.docTabs.DOC)) : null;
        docTabStore.HWP = snap.docTabs.HWP ? JSON.parse(JSON.stringify(snap.docTabs.HWP)) : null;
      }
      if (snap.sheets) {
        const clonedSheets = JSON.parse(JSON.stringify(snap.sheets));
        const clonedRowCounts = snap.sheetRowCounts
          ? JSON.parse(JSON.stringify(snap.sheetRowCounts))
          : null;
        sheetController.replaceLiveSheetDataStore(clonedSheets, clonedRowCounts);
      }
      if (Array.isArray(snap.slides)) {
        slideDeckState.splice(
          0,
          slideDeckState.length,
          ...JSON.parse(JSON.stringify(snap.slides))
        );
      }
      if (snap.ribbonState) {
        Object.assign(localRibbonState, JSON.parse(JSON.stringify(snap.ribbonState)));
      }
      if (backendSyncRes) {
        syncLocalRibbonStateFromBackend(localRibbonState, backendSyncRes);
      }
      const nextRot = typeof snap.pdfRotationDeg === "number" ? snap.pdfRotationDeg : 0;
      setPdfRotationDeg(nextRot);
      const badge = document.getElementById("pdf-rotation-badge");
      const card = document.getElementById("pdf-page-card");
      if (badge) badge.textContent = "회전: " + nextRot + "°";
      if (card) card.style.transform = "rotate(" + nextRot + "deg)";
      workspacePanels.restorePdfStudioState(snap.pdfStudio);
      workspacePanels.restoreWorkspaceViewState(snap.viewState);

      if (snap.theme) {
        document.documentElement.dataset.theme = snap.theme;
        const themeBtn = document.getElementById("btn-theme-toggle");
        if (themeBtn) {
          themeBtn.textContent = snap.theme === "dark" ? "라이트 모드" : "다크 모드";
        }
      }

      const targetMod = snap.activeModule || getActiveModule();
      if (targetMod === "DOC" || targetMod === "HWP") {
        workspacePanels.restoreDocTabState(targetMod);
      } else if (docTabStore.DOC) {
        workspacePanels.restoreDocTabState("DOC");
      }
      if (snap.activeSheetName) {
        setActiveSheetName(snap.activeSheetName);
        const sheetTabBar = document.getElementById("sheet-tab-bar");
        const addSheetBtn = document.getElementById("btn-add-sheet-tab");
        if (sheetTabBar) {
          const knownKeys = new Set(["Sheet1", "Sheet2", ...Object.keys(sheetDataStore)]);
          sheetTabBar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]").forEach((btn) => {
            if (!knownKeys.has(btn.dataset.sheetTab)) {
              btn.remove();
            }
          });
          const renderedKeys = new Set(
            Array.from(sheetTabBar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]")).map(
              (btn) => btn.dataset.sheetTab
            )
          );
          Object.keys(sheetDataStore).forEach((shKey) => {
            if (!renderedKeys.has(shKey) && addSheetBtn) {
              const newTab = document.createElement("button");
              newTab.className = "sheet-tab-btn";
              newTab.dataset.sheetTab = shKey;
              const numMatch = /^Sheet(\d+)$/.exec(shKey);
              newTab.textContent = numMatch ? "시트 " + numMatch[1] : shKey;
              addSheetBtn.before(newTab);
            }
          });
          sheetTabBar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]").forEach((btn) => {
            btn.classList.toggle("active", btn.dataset.sheetTab === snap.activeSheetName);
          });
        }
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
        const targetSnap = sourceStack[sourceStack.length - 1];
        const currentSnap = captureWorkspaceSnapshot(Boolean(targetSnap.isRibbon));
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
          backendSyncRes = await invokeIpc(ipcCmd, "");
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

    function applyDocEditorRibbonEffect(docEditor, cmdId, execRes) {
      if (!docEditor) return false;
      const workspacePanels = getWorkspacePanels();
      const targetEl = workspacePanels.getFocusedDocBlock(docEditor);
      if (cmdId.endsWith("-home-bold")) {
        const boldOn = targetEl.style.fontWeight !== "600";
        targetEl.style.fontWeight = boldOn ? "600" : "";
        return true;
      }
      if (cmdId.endsWith("-home-italic")) {
        const italicOn = targetEl.style.fontStyle !== "italic";
        targetEl.style.fontStyle = italicOn ? "italic" : "";
        return true;
      }
      if (cmdId.endsWith("-home-underline")) {
        const underOn = targetEl.style.textDecoration !== "underline";
        targetEl.style.textDecoration = underOn ? "underline" : "";
        return true;
      }
      if (cmdId.endsWith("-home-style")) {
        const sizes = ["15px", "18px", "22px", "13.5px"];
        const curIdx = sizes.indexOf(targetEl.style.fontSize);
        const presetIdx = curIdx >= 0 ? (curIdx + 1) % sizes.length : 1;
        targetEl.style.fontSize = sizes[presetIdx];
        return true;
      }
      if (cmdId.endsWith("-review-track")) {
        const firstP = docEditor.querySelector(":scope > p:not(#hwp-manuscript-preview)");
        if (firstP) {
          const nextOn = firstP.dataset.highlighted !== "true";
          firstP.dataset.highlighted = nextOn ? "true" : "false";
          firstP.style.backgroundColor = nextOn ? "rgba(92, 107, 192, 0.14)" : "";
        }
        return true;
      }
      if (cmdId.endsWith("-layout-columns")) {
        let cols = Number(execRes?.columnCount || 0);
        if (!cols) {
          const curCols = Number(docEditor.style.columnCount || "1");
          cols = curCols >= 3 ? 1 : curCols + 1;
        }
        docEditor.style.columnCount = cols > 1 ? String(cols) : "";
        return true;
      }
      if (cmdId.endsWith("-layout-orientation")) {
        const landscape =
          execRes?.isLandscape !== undefined
            ? Boolean(execRes.isLandscape)
            : docEditor.style.maxWidth !== "980px";
        docEditor.style.maxWidth = landscape ? "980px" : "";
        return true;
      }
      if (cmdId.endsWith("-layout-margins")) {
        const pads = ["28px 36px", "16px 20px", "36px 52px"];
        let idx = Number(execRes?.marginPresetIndex ?? -1);
        if (idx < 0) {
          const curPadIdx = pads.indexOf(docEditor.style.padding);
          idx = curPadIdx >= 0 ? (curPadIdx + 1) % pads.length : 1;
        } else {
          idx %= pads.length;
        }
        docEditor.style.padding = pads[idx];
        return true;
      }
      if (cmdId.endsWith("-layout-paper")) {
        const paper = execRes?.paperSize || (docEditor.dataset.paperSize === "Letter" ? "A4" : "Letter");
        docEditor.dataset.paperSize = paper;
        return true;
      }
      if (cmdId === "hwp-mod-border") {
        const tableEl = document.getElementById("doc-table");
        if (tableEl) {
          const borderOn =
            execRes?.doubleDiagonalBorderEnabled ?? tableEl.style.borderStyle !== "double";
          tableEl.style.borderStyle = borderOn ? "double" : "";
        }
        return true;
      }
      if (
        cmdId.endsWith("-insert-chart") ||
        cmdId.endsWith("-insert-shape") ||
        cmdId.endsWith("-insert-link") ||
        cmdId.endsWith("-data-fn-browser") ||
        cmdId.endsWith("-review-comment") ||
        cmdId === "doc-mod-toc" ||
        cmdId === "doc-mod-footnote" ||
        cmdId === "doc-mod-mailmerge" ||
        cmdId === "hwp-mod-header" ||
        cmdId === "hwp-mod-field"
      ) {
        const p = document.createElement("p");
        p.textContent = "[" + (execRes?.summary || cmdId) + "]";
        docEditor.appendChild(p);
        workspacePanels.computeSmartStats();
        workspacePanels.renderModuleOutline(getActiveModule());
        return true;
      }
      if (cmdId.endsWith("-insert-table")) {
        workspacePanels.appendDocTableRow();
        return true;
      }
      if (cmdId.endsWith("-data-autosum") || cmdId === "doc-mod-tableformula") {
        workspacePanels.computeDocTableSummary();
        return true;
      }
      if (cmdId.endsWith("-data-sort")) {
        const tbody = document.getElementById("doc-table-body");
        if (tbody) {
          const rows = Array.from(tbody.querySelectorAll("tr:not(#doc-table-sum-row)"));
          const asc = Boolean(execRes?.sortAscending);
          rows.sort((a, b) => {
            const aText = (a.cells[0]?.textContent || "").trim();
            const bText = (b.cells[0]?.textContent || "").trim();
            return asc ? aText.localeCompare(bText, "ko") : bText.localeCompare(aText, "ko");
          });
          const sumRow = document.getElementById("doc-table-sum-row");
          rows.forEach((r) => {
            if (sumRow) {
              sumRow.before(r);
            } else {
              tbody.appendChild(r);
            }
          });
        }
        return true;
      }
      return false;
    }

    async function prepareRibbonAsyncEffect(cmdId, effectiveModule) {
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      if (cmdId.endsWith("-review-verify") || cmdId.endsWith("-data-validation")) {
        const payload = workspacePanels.buildSecurityAuditPayload(effectiveModule);
        await invokeIpc("audit_security", payload);
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
        const formulaPrep = await sheetController.prepareFormulaBarEvaluation();
        return { kind: "sheetFormula", formulaPrep };
      }
      return null;
    }

    async function applyRibbonDomSideEffects(cmdId, execRes, preparedEffect) {
      const sheetController = getSheetController();
      const workspacePanels = getWorkspacePanels();
      if (cmdId.endsWith("-view-theme")) {
        const html = document.documentElement;
        const next = html.dataset.theme === "dark" ? "light" : "dark";
        html.dataset.theme = next;
        const themeBtn = document.getElementById("btn-theme-toggle");
        if (themeBtn) themeBtn.textContent = next === "dark" ? "라이트 모드" : "다크 모드";
        return;
      }
      if (cmdId.endsWith("-view-zoom")) {
        let zoom = Number(execRes?.zoomPercent || 0);
        if (!zoom) {
          const curZoom = Number(localRibbonState.zoomPercent || 100);
          if (curZoom === 100) zoom = 125;
          else if (curZoom === 125) zoom = 150;
          else if (curZoom === 150) zoom = 90;
          else zoom = 100;
          localRibbonState.zoomPercent = zoom;
        }
        const stage = document.querySelector(".canvas-stage");
        if (stage) stage.style.zoom = zoom + "%";
        return;
      }
      if (cmdId.endsWith("-view-ribbon")) {
        const strip = document.getElementById("ribbon-tool-strip");
        if (strip) {
          if (execRes?.ribbonCollapsed !== undefined) {
            strip.hidden = Boolean(execRes.ribbonCollapsed);
          } else {
            strip.hidden = !strip.hidden;
            localRibbonState.ribbonCollapsed = strip.hidden;
          }
        }
        return;
      }
      if (cmdId.endsWith("-view-split")) {
        const stage = document.querySelector(".canvas-stage");
        if (stage) {
          const nextSplit =
            execRes?.splitViewEnabled !== undefined
              ? Boolean(execRes.splitViewEnabled)
              : stage.dataset.splitView !== "true";
          stage.dataset.splitView = nextSplit ? "true" : "false";
        }
        return;
      }
      if (cmdId.endsWith("-review-verify") || cmdId.endsWith("-data-validation")) {
        workspacePanels.applySecurityAuditDom(preparedEffect?.auditOutcome);
        return;
      }
      if (cmdId.endsWith("-review-stats")) {
        workspacePanels.applySmartDiagnoseDom(preparedEffect?.smartRes, false);
        return;
      }
      if (cmdId === "hwp-mod-manuscript" || cmdId.endsWith("-manuscript")) {
        let gridOn;
        if (execRes?.manuscriptGridEnabled === undefined) {
          gridOn = !localRibbonState.manuscriptGridEnabled;
          localRibbonState.manuscriptGridEnabled = gridOn;
        } else {
          gridOn = Boolean(execRes.manuscriptGridEnabled);
        }
        const docEditorEl = document.getElementById("doc-editor");
        if (docEditorEl) {
          docEditorEl.classList.toggle("manuscript-grid", gridOn);
        }
        const hwpNotice = document.getElementById("hwp-manuscript-preview");
        if (hwpNotice) {
          hwpNotice.hidden = !gridOn;
        }
        workspacePanels.saveActiveDocTabState();
        return;
      }

      const curMod = getActiveModule();
      const docEditor = document.getElementById("doc-editor");
      if (
        curMod === "CONV" &&
        (cmdId.startsWith("document-") || cmdId.startsWith("doc-mod-"))
      ) {
        workspacePanels.restoreDocTabState("DOC");
        if (applyDocEditorRibbonEffect(docEditor, cmdId, execRes)) {
          workspacePanels.saveDocTabStateForModule("DOC");
          return;
        }
      }
      if (
        (curMod === "DOC" || curMod === "HWP") &&
        applyDocEditorRibbonEffect(docEditor, cmdId, execRes)
      ) {
        workspacePanels.saveActiveDocTabState();
        return;
      }
      if (cmdId.startsWith("sheet-")) {
        const cellRef = getActiveSheetCellRef();
        const activeTd = document.querySelector('#pane-sheet td[data-cell="' + cellRef + '"]');
        if (cmdId === "sheet-home-bold" && activeTd) {
          const boldOn = activeTd.style.fontWeight !== "600";
          activeTd.style.fontWeight = boldOn ? "600" : "";
          sheetController.saveActiveSheetCellsToWorkbook();
        } else if (cmdId === "sheet-home-italic" && activeTd) {
          const italicOn = activeTd.style.fontStyle !== "italic";
          activeTd.style.fontStyle = italicOn ? "italic" : "";
          sheetController.saveActiveSheetCellsToWorkbook();
        } else if (cmdId === "sheet-home-underline" && activeTd) {
          const underOn = activeTd.style.textDecoration !== "underline";
          activeTd.style.textDecoration = underOn ? "underline" : "";
          sheetController.saveActiveSheetCellsToWorkbook();
        } else if (cmdId === "sheet-home-style" && activeTd) {
          const sizes = ["13px", "15px", "17px", "12px"];
          const curIdx = sizes.indexOf(activeTd.style.fontSize);
          const presetIdx = curIdx >= 0 ? (curIdx + 1) % sizes.length : 1;
          activeTd.style.fontSize = sizes[presetIdx];
          sheetController.saveActiveSheetCellsToWorkbook();
        } else if (cmdId === "sheet-data-autosum" || cmdId === "sheet-data-fn-browser") {
          sheetController.commitPreparedFormulaBarEvaluation(preparedEffect?.formulaPrep, null);
        } else if (cmdId === "sheet-insert-table") {
          sheetController.appendSheetRow();
        } else {
          const out = document.getElementById("sheet-eval-result");
          if (out && execRes?.summary) out.textContent = execRes.summary;
        }
        return;
      }
      if (cmdId.startsWith("slide-")) {
        const titleZone = document.getElementById("slide-title-zone");
        const lowerZone = document.getElementById("slide-lower-zone");
        const stageBox = document.getElementById("slide-stage-box");
        const curIdx = getActiveSlideIndex();
        const curSlide = slideDeckState[curIdx];
        if (cmdId === "slide-home-bold" && titleZone) {
          const boldOn = titleZone.style.fontWeight !== "700";
          titleZone.style.fontWeight = boldOn ? "700" : "";
          workspacePanels.syncActiveSlideDomStylesToDeck();
        } else if (cmdId === "slide-home-italic" && titleZone) {
          const italicOn = titleZone.style.fontStyle !== "italic";
          titleZone.style.fontStyle = italicOn ? "italic" : "";
          workspacePanels.syncActiveSlideDomStylesToDeck();
        } else if (cmdId === "slide-home-underline" && titleZone) {
          const underOn = titleZone.style.textDecoration !== "underline";
          titleZone.style.textDecoration = underOn ? "underline" : "";
          workspacePanels.syncActiveSlideDomStylesToDeck();
        } else if (cmdId === "slide-home-style" && titleZone) {
          const sizes = ["20px", "22px", "24px", "18px"];
          const colors = ["", "var(--eo-primary)", "var(--eo-accent-slide)", ""];
          const curIdxSize = sizes.indexOf(titleZone.style.fontSize);
          const presetIdx = curIdxSize >= 0 ? (curIdxSize + 1) % sizes.length : 1;
          titleZone.style.fontSize = sizes[presetIdx];
          titleZone.style.color = colors[presetIdx];
          workspacePanels.syncActiveSlideDomStylesToDeck();
        } else if (
          cmdId === "slide-insert-table" ||
          cmdId === "slide-insert-chart" ||
          cmdId === "slide-insert-shape"
        ) {
          workspacePanels.appendNewSlideDeckItem();
        } else if (
          cmdId === "slide-insert-link" ||
          cmdId === "slide-data-autosum" ||
          cmdId === "slide-data-fn-browser" ||
          cmdId === "slide-review-comment"
        ) {
          const tag = "[" + (execRes?.summary || cmdId) + "]";
          if (curSlide) {
            const prefix = curSlide.lower ? curSlide.lower + " · " : "";
            curSlide.lower = prefix + tag;
          }
          if (lowerZone && curSlide) {
            const p = document.createElement("p");
            p.textContent = curSlide.lower;
            lowerZone.replaceChildren(p);
          }
        } else if (cmdId === "slide-data-sort" && curSlide && curSlide.lower) {
          const asc = Boolean(execRes?.sortAscending);
          const parts = curSlide.lower
            .split("·")
            .map((s) => s.trim())
            .filter(Boolean);
          parts.sort((a, b) => (asc ? a.localeCompare(b, "ko") : b.localeCompare(a, "ko")));
          curSlide.lower = parts.join(" · ");
          workspacePanels.renderSlideDeck(curIdx);
        } else if (cmdId === "slide-mod-presenter") {
          document.getElementById("btn-slide-laser")?.click();
        } else if (cmdId === "slide-mod-transition") {
          const sel = document.getElementById("slide-transition-select");
          if (sel && curSlide) {
            const opts = ["fade", "push", "zoom", "none"];
            const nextIdx = (opts.indexOf(sel.value) + 1) % opts.length;
            sel.value = opts[nextIdx];
            curSlide.transition = sel.value;
            workspacePanels.renderSlideDeck(curIdx);
          }
        } else if (stageBox) {
          if (cmdId === "slide-layout-columns" && lowerZone) {
            const cols = execRes?.columnCount || 1;
            lowerZone.style.columnCount = cols > 1 ? String(cols) : "";
          } else if (cmdId === "slide-layout-margins") {
            const idx = Number(execRes?.marginPresetIndex ?? 0) % 3;
            const pads = ["20px", "12px", "28px"];
            stageBox.style.padding = pads[idx];
          } else if (cmdId === "slide-mod-snap" || cmdId === "slide-review-track") {
            stageBox.style.outline = stageBox.style.outline ? "" : "2px dashed var(--eo-primary)";
          } else {
            stageBox.dataset.layoutState = execRes?.summary || cmdId;
          }
          workspacePanels.syncActiveSlideDomStylesToDeck();
        }
        const metaStatus = document.getElementById("slide-meta-status");
        if (metaStatus && execRes?.summary && cmdId !== "slide-mod-transition") {
          metaStatus.textContent =
            "슬라이드 " +
            (getActiveSlideIndex() + 1) +
            " / " +
            slideDeckState.length +
            " · " +
            execRes.summary;
        }
        return;
      }
      if (cmdId === "pdf-mod-rotate") {
        let nextRot = (getPdfRotationDeg() + 90) % 360;
        if (typeof execRes?.pdfRotationDegrees === "number") {
          nextRot = execRes.pdfRotationDegrees;
        }
        setPdfRotationDeg(nextRot);
        const badge = document.getElementById("pdf-rotation-badge");
        const card = document.getElementById("pdf-page-card");
        if (badge) badge.textContent = "회전: " + nextRot + "°";
        if (card) card.style.transform = "rotate(" + nextRot + "deg)";
        return;
      }
      if (cmdId === "pdf-mod-tooffice") {
        await activatePaneInternal("CONV", false);
        return;
      }
      if (cmdId.startsWith("pdf-")) {
        workspacePanels.applyPdfStudioFormState();
      }
    }

    function executeTrackedRibbonCommand(cmdId, options = {}) {
      return enqueueWorkspaceHistoryOp(async () => {
        const sheetController = getSheetController();
        await sheetController.commitPendingFocusedSheetCell(true);
        const preSnapshot = captureWorkspaceSnapshot(true);
        const effectiveModule = options.targetModule || getActiveModule();
        const preparedEffect = await prepareRibbonAsyncEffect(cmdId, effectiveModule);
        const execRes = await invokeIpc("execute_ribbon_command", cmdId);
        syncLocalRibbonStateFromBackend(localRibbonState, execRes);
        if (options.targetModule && options.targetModule !== getActiveModule()) {
          await activatePaneInternal(options.targetModule, false);
        } else {
          localRibbonState.activeModule = getActiveModule();
        }
        await applyRibbonDomSideEffects(cmdId, execRes, preparedEffect);
        commitDeferredUndoSnapshot(preSnapshot);
        const fb = document.getElementById("status-command-feedback");
        if (fb && execRes && !options.skipFeedback) {
          const fallbackMsg = options.fallbackTitle ? "명령 실행: " + options.fallbackTitle : "";
          fb.textContent = execRes.summary || fallbackMsg;
        }
        return execRes;
      });
    }

    return {
      pushRawUndoSnapshot,
      commitDeferredUndoSnapshot,
      captureWorkspaceSnapshot,
      pushUndoSnapshot,
      restoreWorkspaceSnapshot,
      performWorkspaceUndo,
      performWorkspaceRedo,
      applyModuleAccent,
      renderRibbonCategoryTools,
      activatePaneInternal,
      activatePane,
      executeTrackedRibbonCommand,
    };
  }

  host.__EASYOFFICE_CREATE_RIBBON_HISTORY__ = createRibbonHistory;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
