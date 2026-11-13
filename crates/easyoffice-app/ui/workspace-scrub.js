(function (host) {
  "use strict";

  // PII scan/mask across modules (smart-assistant dialog "스캔·마스킹 실행"). Split out of
  // workspace-tools.js without behaviour change; `shared` carries the two tool helpers it needs.
  const { serializeDomChildren, restoreDomChildren } = host.__EASYOFFICE_SHEET_UTILS__;


  function inputScrubField(input) {
    if (!input || !(input.value || "").trim()) return null;
    return { read: () => input.value, write: (v) => { input.value = v; } };
  }

  // The watermark layer is scrubbed on its own only when it does not simply mirror the watermark input.
  function layerScrubField(wmLayer, wmInput, prevWmInputVal) {
    if (!wmLayer) return null;
    const mirrorsInput = Boolean(wmInput) && wmLayer.textContent === prevWmInputVal;
    if (mirrorsInput || !(wmLayer.textContent || "").trim()) return null;
    return { read: () => wmLayer.textContent, write: (v) => { wmLayer.textContent = v; } };
  }

  function applyFieldScrub(field, res) {
    if (!field || !res) return 0;
    const count = Number(res.totalMasked || 0);
    if (count > 0) field.write(res.scrubbedText);
    return count;
  }

  function createWorkspaceScrub(deps, panelHooks, shared) {
    const {
      document,
      invokeIpc,
      docTabStore,
      sheetDataStore,
      slideDeckState,
      collectActiveScrubGroups,
      applyScrubResultsToGroups,
      scrubSheetStoreMap,
      sheetController,
      getActiveModule,
      getActiveSheetName,
      getActiveSheetCellRef,
      getActiveSlideIndex,
      enqueueWorkspaceHistoryOp,
      captureWorkspaceSnapshot,
      commitDeferredUndoSnapshot,
    } = deps;

    const {
      saveActiveDocTabState,
      ensureDefaultDocTabStore,
      computeDocTableSummary,
      syncActiveSlideDomStylesToDeck,
      renderSlideDeck,
      applyPdfStudioFormState,
    } = panelHooks;
    const { resolveSourceScopeKind, computeSmartStats, describePdfHighlight } = shared;


    function assertValidScrubResult(origText, res) {
      if (
        !res ||
        typeof res.scrubbedText !== "string" ||
        (origText.length > 0 && res.scrubbedText.length === 0)
      ) {
        const msg = "개인정보 마스킹 응답 검증에 실패했습니다.";
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = "[요청 거부] " + msg;
        throw new Error(msg);
      }
    }

    function countPendingGroupMasks(activeGroups, results) {
      let total = 0;
      results.forEach((res, idx) => {
        assertValidScrubResult(activeGroups[idx].origText, res);
        total += Number(res.totalMasked || 0);
      });
      return total;
    }

    function requestFieldScrub(field) {
      return field ? invokeIpc("scrub_pii", field.read()) : Promise.resolve(null);
    }

    function sumFieldMasks(fields, results) {
      let total = 0;
      fields.forEach((field, idx) => {
        if (!field || results[idx] === null) return;
        assertValidScrubResult(field.read(), results[idx]);
        total += Number(results[idx].totalMasked || 0);
      });
      return total;
    }

    async function scrubPdfDom(preSnapshot) {
      const pdfNodes = Array.from(
        document.querySelectorAll("#pdf-page-card h3, #pdf-page-card p:not(#pdf-form-status)")
      );
      const nodeGroups = pdfNodes.flatMap((el) => collectActiveScrubGroups(el));
      const wmInput = document.getElementById("pdf-watermark-input");
      const wmLayer = document.getElementById("pdf-watermark-layer");
      const prevWmInputVal = wmInput ? wmInput.value : "";
      const fields = [
        inputScrubField(wmInput),
        layerScrubField(wmLayer, wmInput, prevWmInputVal),
        inputScrubField(document.getElementById("pdf-form-applicant")),
        inputScrubField(document.getElementById("pdf-form-dept")),
        inputScrubField(document.getElementById("pdf-highlight-input")),
      ];
      const fieldPromises = fields.map(requestFieldScrub);
      const [groupResults, ...fieldResults] = await Promise.all([
        Promise.all(nodeGroups.map((g) => invokeIpc("scrub_pii", g.origText))),
        ...fieldPromises,
      ]);

      const pendingTotal = countPendingGroupMasks(nodeGroups, groupResults) + sumFieldMasks(fields, fieldResults);
      if (pendingTotal === 0) return 0;

      commitDeferredUndoSnapshot(preSnapshot);
      let totalMasked = applyScrubResultsToGroups(nodeGroups, groupResults);
      totalMasked += applyFieldScrub(fields[0], fieldResults[0]);
      if (wmLayer && wmInput && wmLayer.textContent === prevWmInputVal) wmLayer.textContent = wmInput.value;
      totalMasked += applyFieldScrub(fields[1], fieldResults[1]);
      for (let i = 2; i < fields.length; i += 1) totalMasked += applyFieldScrub(fields[i], fieldResults[i]);
      // The card badge shows the highlight phrase, so it follows the masked state.
      const badge = document.querySelector("#pdf-page-card .pdf-highlight-badge");
      const phrase = document.getElementById("pdf-highlight-input")?.value || "";
      if (badge && phrase) badge.textContent = describePdfHighlight(phrase, document.getElementById("pdf-highlight-kind")?.value);
      const formStatus = document.getElementById("pdf-form-status");
      if (formStatus && (formStatus.textContent || "").startsWith("양식 반영 완료")) {
        applyPdfStudioFormState();
      }
      return totalMasked;
    }

    async function scrubSheetScope(preSnapshot) {
      const curSheet = getActiveSheetName();
      // The masking rewrites cells of every sheet, so a protected sheet has to be unprotected first (PD-216).
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      if (guard && Object.keys(sheetDataStore).some((name) => guard.isOn(name))) {
        guard.say(guard.BLOCKED);
        return 0;
      }
      sheetDataStore[curSheet] = sheetController.snapshotDomSheetCells();
      let scrubbed;
      try {
        scrubbed = await scrubSheetStoreMap(sheetDataStore, curSheet, sheetController.trackSheetAsyncWork);
      } catch (sheetScrubErr) {
        const fb = document.getElementById("status-command-feedback");
        if (fb && !String(fb.textContent || "").startsWith("[요청 거부]")) {
          fb.textContent = "[요청 거부] " + (sheetScrubErr?.message || String(sheetScrubErr));
        }
        throw sheetScrubErr;
      }
      if (scrubbed.totalMasked > 0) {
        commitDeferredUndoSnapshot(preSnapshot);
        sheetController.replaceLiveSheetDataStore(scrubbed.nextSheets);
        sheetController.restoreDomSheetCells(curSheet);
        sheetController.refreshFocusedSheetCellPendingSnapshot();
        sheetController.updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "A1");
      }
      return scrubbed.totalMasked;
    }

    const SLIDE_TEXT_KEYS = ["title", "lower", "notes"];

    function collectSlideScrubJobs() {
      const fields = [];
      const slideNodeJobs = [];
      slideDeckState.forEach((slide) => {
        for (const key of SLIDE_TEXT_KEYS) {
          if ((slide[key] || "").trim()) fields.push({ slide, key, text: slide[key] });
        }
        if (!Array.isArray(slide.insertedNodes) || slide.insertedNodes.length === 0) return;
        const tempDiv = document.createElement("div");
        restoreDomChildren(tempDiv, slide.insertedNodes, document);
        const groups = collectActiveScrubGroups(tempDiv);
        if (groups.length > 0) slideNodeJobs.push({ slide, tempDiv, groups });
      });
      return { fields, slideNodeJobs };
    }

    function applySlideScrubResults(fields, results, slideNodeJobs, nodeJobResults) {
      results.forEach((res, idx) => {
        if (Number(res.totalMasked || 0) > 0) fields[idx].slide[fields[idx].key] = res.scrubbedText;
      });
      nodeJobResults.forEach((groupRes, jIdx) => {
        const job = slideNodeJobs[jIdx];
        if (applyScrubResultsToGroups(job.groups, groupRes) > 0) {
          job.slide.insertedNodes = serializeDomChildren(job.tempDiv);
        }
      });
    }

    async function scrubSlideScope(curMod, preSnapshot) {
      if (curMod === "SLIDE" && typeof syncActiveSlideDomStylesToDeck === "function") {
        syncActiveSlideDomStylesToDeck();
      }
      const { fields, slideNodeJobs } = collectSlideScrubJobs();
      const [results, nodeJobResults] = await Promise.all([
        Promise.all(fields.map((item) => invokeIpc("scrub_pii", item.text))),
        Promise.all(
          slideNodeJobs.map((job) => Promise.all(job.groups.map((g) => invokeIpc("scrub_pii", g.origText))))
        ),
      ]);
      let totalMasked = 0;
      results.forEach((res, idx) => {
        assertValidScrubResult(fields[idx].text, res);
        totalMasked += Number(res.totalMasked || 0);
      });
      nodeJobResults.forEach((groupRes, jIdx) => {
        totalMasked += countPendingGroupMasks(slideNodeJobs[jIdx].groups, groupRes);
      });
      if (totalMasked > 0) {
        commitDeferredUndoSnapshot(preSnapshot);
        applySlideScrubResults(fields, results, slideNodeJobs, nodeJobResults);
        renderSlideDeck(getActiveSlideIndex());
      }
      return totalMasked;
    }

    function scrubGroupResults(activeGroups) {
      return Promise.all(activeGroups.map((g) => invokeIpc("scrub_pii", g.origText)));
    }

    async function scrubLiveEditor(editorEl, preSnapshot) {
      const activeGroups = collectActiveScrubGroups(editorEl);
      const groupResults = await scrubGroupResults(activeGroups);
      if (countPendingGroupMasks(activeGroups, groupResults) === 0) return 0;
      commitDeferredUndoSnapshot(preSnapshot);
      const masked = applyScrubResultsToGroups(activeGroups, groupResults);
      computeDocTableSummary();
      saveActiveDocTabState();
      return masked;
    }

    async function scrubStoredEditorNodes(targetScope, preSnapshot) {
      const tempDiv = document.createElement("div");
      restoreDomChildren(tempDiv, docTabStore[targetScope].editorNodes, document);
      const activeGroups = collectActiveScrubGroups(tempDiv);
      const groupResults = await scrubGroupResults(activeGroups);
      if (countPendingGroupMasks(activeGroups, groupResults) === 0) return 0;
      commitDeferredUndoSnapshot(preSnapshot);
      const modMasked = applyScrubResultsToGroups(activeGroups, groupResults);
      if (modMasked > 0) {
        docTabStore[targetScope] = { ...docTabStore[targetScope], editorNodes: serializeDomChildren(tempDiv) };
      }
      return modMasked;
    }

    async function scrubDocScope(curMod, targetScope, preSnapshot) {
      ensureDefaultDocTabStore();
      const editorEl = document.getElementById("doc-editor");
      if (curMod === targetScope && editorEl) return scrubLiveEditor(editorEl, preSnapshot);
      if (Array.isArray(docTabStore[targetScope]?.editorNodes)) return scrubStoredEditorNodes(targetScope, preSnapshot);
      return 0;
    }

    const PII_RESULT_LABEL = "개인정보(주민번호·연락처·이메일·카드·계좌·여권·면허·사업자번호) ";

    function scrubTargetScope(curMod) {
      if (curMod !== "CONV") return curMod;
      return resolveSourceScopeKind(document.getElementById("conv-src-fmt")?.value || "docx");
    }

    async function scrubByScope(curMod, targetScope, preSnapshot) {
      if (targetScope === "SHEET") return scrubSheetScope(preSnapshot);
      if (targetScope === "SLIDE") return scrubSlideScope(curMod, preSnapshot);
      if (targetScope === "PDF") return scrubPdfDom(preSnapshot);
      return scrubDocScope(curMod, targetScope, preSnapshot);
    }

    async function runPiiScrub(recordUndo) {
      const piiOut = document.getElementById("pii-result-text");
      if (!piiOut) return;
      await sheetController.commitPendingFocusedSheetCell(true);
      const preSnapshot = recordUndo ? captureWorkspaceSnapshot(false) : null;
      const curMod = getActiveModule();
      const totalMasked = await scrubByScope(curMod, scrubTargetScope(curMod), preSnapshot);
      piiOut.textContent =
        totalMasked > 0
          ? PII_RESULT_LABEL + totalMasked + "건 마스킹 완료 (" + curMod + ")"
          : PII_RESULT_LABEL + "스캔 완료 (감지 0건 · " + curMod + ")";
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = piiOut.textContent;
      computeSmartStats();
    }

    function runPiiScrubAction(recordUndo) {
      return enqueueWorkspaceHistoryOp(() => runPiiScrub(recordUndo));
    }

    return { runPiiScrubAction };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_SCRUB__ = createWorkspaceScrub;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
