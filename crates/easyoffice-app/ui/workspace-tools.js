(function (host) {
  "use strict";

  function createWorkspaceTools(deps, panelHooks) {
    const {
      document,
      URL,
      invokeIpc,
      docTabStore,
      sheetDataStore,
      slideDeckState,
      parseCellRefParts,
      extractEditableDocumentText,
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
      renderSlideDeck,
      applyPdfStudioFormState,
    } = panelHooks;

    function resolveSourceScopeKind(sourceFormatOrModule) {
      const token = String(sourceFormatOrModule || getActiveModule() || "DOC")
        .trim()
        .toLowerCase();
      if (["xlsx", "xls", "ods", "csv", "tsv", "sheet"].includes(token)) return "SHEET";
      if (["pptx", "ppsx", "ppt", "odp", "slide"].includes(token)) return "SLIDE";
      if (["pdf"].includes(token)) return "PDF";
      if (["hwp", "hwt", "hwpx", "hwtx"].includes(token)) return "HWP";
      return "DOC";
    }

    function appendTableRowsFromTrList(rows, lines) {
      if (rows.length === 0) return;
      lines.push("[[TABLE_NEW]]");
      rows.forEach((tr) => {
        const cells = Array.from(tr.querySelectorAll("th, td")).map((cell) =>
          (cell.textContent || "").trim().replaceAll("\t", " ").replaceAll("\n", " ")
        );
        if (cells.length > 0) {
          lines.push("[[TABLE_ROW]]\t" + cells.join("\t"));
        }
      });
    }

    function buildStructuredConversionBody(sourceFormatOrModule) {
      saveActiveDocTabState();
      ensureDefaultDocTabStore();
      const scope = resolveSourceScopeKind(sourceFormatOrModule);
      const lines = [];

      if (scope === "HWP") {
        lines.push("[[AGENCY]]\t행정안전부");
      }

      if (scope === "DOC" || scope === "HWP") {
        const saved = docTabStore[scope];
        if (saved) {
          const tempDiv = document.createElement("div");
          tempDiv.innerHTML = saved.editorHtml || "";
          const docText = extractEditableDocumentText(tempDiv);
          if (docText) {
            lines.push(docText);
          }
          tempDiv.querySelectorAll("table").forEach((inlineTable) => {
            appendTableRowsFromTrList(Array.from(inlineTable.querySelectorAll("tr")), lines);
          });
        }
      } else if (scope === "SHEET") {
        const curSheet = getActiveSheetName();
        sheetDataStore[curSheet] = sheetController.snapshotDomSheetCells();
        const sheetObj = sheetDataStore[curSheet] || {};
        const rowMap = new Map();
        Object.keys(sheetObj).forEach((ref) => {
          const entry = sheetObj[ref];
          const rawVal = (entry?.formula || entry?.value || "")
            .trim()
            .replaceAll("\t", " ")
            .replaceAll("\n", " ");
          const dispVal = (entry?.value || entry?.formula || "")
            .trim()
            .replaceAll("\t", " ")
            .replaceAll("\n", " ");
          if (!rawVal && !dispVal) return;
          lines.push("[[SHEET_A1]]\t" + ref + "\t" + (rawVal || dispVal));
          const p = parseCellRefParts(ref);
          const rIdx = Math.max(0, p.row - 1);
          const cIdx = Math.max(0, p.col);
          if (!rowMap.has(rIdx)) rowMap.set(rIdx, []);
          rowMap.get(rIdx)[cIdx] = dispVal || rawVal;
        });
        if (rowMap.size > 0) {
          lines.push("[[TABLE_NEW]]");
          Array.from(rowMap.keys())
            .sort((a, b) => a - b)
            .forEach((rIdx) => {
              const rowArr = rowMap.get(rIdx) || [];
              const dense = [];
              for (let i = 0; i < rowArr.length; i += 1) {
                dense.push(rowArr[i] || "");
              }
              if (dense.length > 0) {
                lines.push("[[TABLE_ROW]]\t" + dense.join("\t"));
              }
            });
        }
      } else if (scope === "SLIDE") {
        slideDeckState.forEach((s) => {
          const title = (s.title || "").trim().replaceAll("\t", " ").replaceAll("\n", " ");
          const bullets = (s.lower || "")
            .split(/\n+|·/)
            .map((item) => item.trim().replaceAll("\t", " "))
            .filter(Boolean)
            .join(";;");
          const notes = (s.notes || "").trim().replaceAll("\t", " ").replaceAll("\n", " ");
          lines.push("[[SLIDE]]\t" + title + "\t" + bullets + "\t" + notes);
        });
      } else if (scope === "PDF") {
        document
          .querySelectorAll("#pdf-page-card h3, #pdf-page-card p:not(#pdf-form-status)")
          .forEach((el) => {
            const t = (el.textContent || "").trim();
            if (t) lines.push(t);
          });
        const wmInputVal = (document.getElementById("pdf-watermark-input")?.value || "").trim();
        const wmLayerVal = (
          document.getElementById("pdf-watermark-layer")?.textContent || ""
        ).trim();
        const wmVal = wmInputVal || wmLayerVal;
        if (wmVal) {
          lines.push("워터마크: " + wmVal);
        }
        if (wmLayerVal && wmLayerVal !== wmVal) {
          lines.push("워터마크 레이어: " + wmLayerVal);
        }
        const stampVal = (
          document.getElementById("pdf-stamp-select")?.value ||
          document.getElementById("pdf-stamp-badge")?.textContent ||
          ""
        ).trim();
        if (stampVal) {
          lines.push("결재 스탬프: " + stampVal);
        }
        const applicantVal = (document.getElementById("pdf-form-applicant")?.value || "").trim();
        if (applicantVal) {
          lines.push("신청인: " + applicantVal);
        }
        const deptVal = (document.getElementById("pdf-form-dept")?.value || "").trim();
        if (deptVal) {
          lines.push("부서: " + deptVal);
        }
      }

      return lines.join("\n") || "본문 텍스트";
    }

    function triggerBase64Download(base64Data, baseFilename, ext) {
      if (!base64Data) return false;
      try {
        const binStr = host.atob(base64Data);
        const bytes = new Uint8Array(binStr.length);
        for (let i = 0; i < binStr.length; i += 1) {
          bytes[i] = binStr.charCodeAt(i);
        }
        const blob = new Blob([bytes], { type: "application/octet-stream" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = baseFilename + "." + ext.toLowerCase();
        document.body.appendChild(a);
        a.click();
        a.remove();
        host.setTimeout(() => URL.revokeObjectURL(url), 1500);
        return true;
      } catch {
        return false;
      }
    }

    function saveActiveModuleDocument() {
      return enqueueWorkspaceHistoryOp(async () => {
        await sheetController.commitPendingFocusedSheetCell(true);
        const curMod = getActiveModule();
        const extMap = {
          DOC: "docx",
          HWP: "hwpx",
          SHEET: "xlsx",
          SLIDE: "pptx",
          PDF: "pdf",
          CONV: document.getElementById("conv-dst-fmt")?.value || "hwpx",
        };
        const dstExt = extMap[curMod] || "docx";
        let srcExt = dstExt;
        if (curMod === "CONV") {
          srcExt = document.getElementById("conv-src-fmt")?.value || "docx";
        }
        const sourceTarget = curMod === "CONV" ? srcExt : curMod;
        const body = buildStructuredConversionBody(sourceTarget);
        const title = "EasyOffice_" + curMod + "_문서";
        const res = await invokeIpc(
          "convert_document_format",
          srcExt + "->" + dstExt + "|" + title + "|" + body
        );
        const fb = document.getElementById("status-command-feedback");
        if (res && res.convertedBase64) {
          triggerBase64Download(res.convertedBase64, title, dstExt);
        }
        if (fb && res) {
          fb.textContent =
            "문서 저장 완료 (" +
            dstExt.toUpperCase() +
            " · " +
            (res.outputBytes || 0) +
            "B · " +
            (res.specification || "") +
            ")";
        }
      });
    }

    function computeSmartStats() {
      const statsEl = document.getElementById("smart-stats");
      if (!statsEl) return;
      const text = extractEditableDocumentText();
      const chars = Array.from(text).length;
      const words = text ? text.split(/\s+/).length : 0;
      const secs = Math.max(1, Math.ceil((words / 220) * 60));
      statsEl.textContent = "문자: " + chars + " · 단어: " + words + " · 읽기 시간: " + secs + "초";
    }

    function applySmartDiagnoseDom(res, updateFeedback) {
      if (!res) return;
      const statsEl = document.getElementById("smart-stats");
      const summaryEl = document.getElementById("smart-summary-output");
      const fb = document.getElementById("status-command-feedback");
      if (statsEl) {
        statsEl.textContent =
          "문자: " +
          res.charCount +
          " · 단어: " +
          res.wordCount +
          " · 문장: " +
          res.sentenceCount +
          " · 읽기: " +
          res.readingTimeSeconds +
          "초 · 요약: " +
          res.summaryCount +
          "건";
      }
      if (summaryEl) {
        const lines = Array.isArray(res.summaryLines) ? res.summaryLines : [];
        const actions = Array.isArray(res.actionItems) ? res.actionItems : [];
        const summaryText = lines.length > 0 ? lines.join(" / ") : "핵심 문장 요약 완료";
        const actionSuffix = actions.length > 0 ? " · 조치: " + actions.join(", ") : "";
        summaryEl.textContent = "요약: " + summaryText + actionSuffix;
      }
      if (fb && updateFeedback) {
        fb.textContent =
          "문장 진단 완료 · 요약 " +
          res.summaryCount +
          "건 · 실행 항목 " +
          res.actionItemCount +
          "건 · 교정 제안 " +
          res.suggestionCount +
          "건";
      }
    }

    async function runSmartDiagnoseAction() {
      const bodyText = extractEditableDocumentText();
      const res = await invokeIpc("run_smart_assistant", bodyText);
      applySmartDiagnoseDom(res, true);
    }

    function buildSecurityAuditPayload(targetMod) {
      const effectiveMod = targetMod || getActiveModule();
      let auditSource = effectiveMod;
      if (effectiveMod === "CONV") {
        auditSource = document.getElementById("conv-src-fmt")?.value || "docx";
      }
      const bodyText = buildStructuredConversionBody(auditSource);
      const byteLen = new TextEncoder().encode(bodyText).length;
      const entryNameMap = {
        DOC: "word/document.xml",
        HWP: "Contents/section0.xml",
        SHEET: "xl/worksheets/sheet1.xml",
        SLIDE: "ppt/slides/slide1.xml",
        PDF: "Catalog/Pages.pdf",
        CONV: "package/content.xml",
      };
      const entryName = entryNameMap[effectiveMod] || "word/document.xml";
      const xmlSnippet = '<doc module="' + effectiveMod + '">' + bodyText + "</doc>";
      return JSON.stringify({
        entries: [
          {
            name: entryName,
            compressed: Math.max(64, Math.floor(byteLen / 2)),
            uncompressed: Math.max(128, byteLen),
            encrypted: false,
          },
        ],
        xml: xmlSnippet,
      });
    }

    async function fetchSecurityAuditOutcome(targetMod) {
      const payload = buildSecurityAuditPayload(targetMod);
      try {
        await invokeIpc("audit_security", payload);
        return { ok: true, errorMessage: "" };
      } catch (err) {
        return { ok: false, errorMessage: err?.message || String(err) };
      }
    }

    function applySecurityAuditDom(outcome) {
      if (!outcome) return;
      const txt = document.getElementById("security-status-text");
      if (!txt) return;
      const curMod = getActiveModule();
      if (outcome.ok) {
        txt.textContent = "문서 보안 검사 완료 (" + curMod + "): SAFE";
      } else {
        txt.textContent =
          "문서 보안 검사 완료 (" + curMod + "): REJECTED (" + outcome.errorMessage + ")";
      }
    }

    function runSecurityAuditAction() {
      return enqueueWorkspaceHistoryOp(async () => {
        await sheetController.commitPendingFocusedSheetCell(true);
        const outcome = await fetchSecurityAuditOutcome(getActiveModule());
        applySecurityAuditDom(outcome);
      });
    }

    async function runBenchmarkAction() {
      const res = await invokeIpc("run_engine_benchmark", "5");
      const box =
        document.getElementById("bench-result-box") || document.getElementById("bench-Summary");
      const fb = document.getElementById("status-command-feedback");
      if (res && Array.isArray(res.suites)) {
        const summary = res.suites
          .map(
            (s) =>
              s.suiteId +
              " " +
              s.baselineP50Us +
              "µs→" +
              s.optimizedP50Us +
              "µs (" +
              s.speedupFactor +
              "x)"
          )
          .join(" · ");
        if (box) {
          const eqLabel = res.allEquivalent ? "결과 일치" : "불일치";
          box.textContent = "성능 진단 완료 (" + eqLabel + ") · " + summary;
        }
        if (fb) {
          fb.textContent = "성능 진단 완료 · " + summary;
        }
      }
    }

    async function scrubPdfDom(preSnapshot) {
      const pdfNodes = Array.from(
        document.querySelectorAll("#pdf-page-card h3, #pdf-page-card p:not(#pdf-form-status)")
      );
      const nodeGroups = [];
      pdfNodes.forEach((el) => {
        nodeGroups.push(...collectActiveScrubGroups(el));
      });
      const wmInput = document.getElementById("pdf-watermark-input");
      const wmLayer = document.getElementById("pdf-watermark-layer");
      const prevWmInputVal = wmInput ? wmInput.value : "";
      const scrubSeparateLayer =
        Boolean(wmLayer) &&
        !(wmInput && wmLayer.textContent === prevWmInputVal) &&
        Boolean((wmLayer.textContent || "").trim());
      const appInput = document.getElementById("pdf-form-applicant");
      const deptInput = document.getElementById("pdf-form-dept");

      const wmInputPromise =
        wmInput && (wmInput.value || "").trim()
          ? invokeIpc("scrub_pii", wmInput.value)
          : Promise.resolve(null);
      const wmLayerPromise = scrubSeparateLayer
        ? invokeIpc("scrub_pii", wmLayer.textContent)
        : Promise.resolve(null);
      const appPromise =
        appInput && (appInput.value || "").trim()
          ? invokeIpc("scrub_pii", appInput.value)
          : Promise.resolve(null);
      const deptPromise =
        deptInput && (deptInput.value || "").trim()
          ? invokeIpc("scrub_pii", deptInput.value)
          : Promise.resolve(null);

      const [groupResults, wmInputRes, wmLayerRes, appRes, deptRes] = await Promise.all([
        Promise.all(nodeGroups.map((g) => invokeIpc("scrub_pii", g.origText))),
        wmInputPromise,
        wmLayerPromise,
        appPromise,
        deptPromise,
      ]);
      commitDeferredUndoSnapshot(preSnapshot);
      let totalMasked = applyScrubResultsToGroups(nodeGroups, groupResults);
      if (wmInput && wmInputRes && typeof wmInputRes.scrubbedText === "string") {
        const count = Number(wmInputRes.totalMasked || 0);
        if (count > 0) wmInput.value = wmInputRes.scrubbedText;
        totalMasked += count;
      }
      if (wmLayer) {
        if (wmInput && wmLayer.textContent === prevWmInputVal) {
          wmLayer.textContent = wmInput.value;
        } else if (wmLayerRes && typeof wmLayerRes.scrubbedText === "string") {
          const count = Number(wmLayerRes.totalMasked || 0);
          if (count > 0) wmLayer.textContent = wmLayerRes.scrubbedText;
          totalMasked += count;
        }
      }
      if (appInput && appRes && typeof appRes.scrubbedText === "string") {
        const count = Number(appRes.totalMasked || 0);
        if (count > 0) appInput.value = appRes.scrubbedText;
        totalMasked += count;
      }
      if (deptInput && deptRes && typeof deptRes.scrubbedText === "string") {
        const count = Number(deptRes.totalMasked || 0);
        if (count > 0) deptInput.value = deptRes.scrubbedText;
        totalMasked += count;
      }
      const formStatus = document.getElementById("pdf-form-status");
      if (formStatus && (formStatus.textContent || "").startsWith("양식 반영 완료")) {
        applyPdfStudioFormState();
      }
      return totalMasked;
    }

    function runPiiScrubAction(recordUndo) {
      return enqueueWorkspaceHistoryOp(async () => {
        const piiOut = document.getElementById("pii-result-text");
        if (!piiOut) return;
        await sheetController.commitPendingFocusedSheetCell(true);
        const preSnapshot = recordUndo ? captureWorkspaceSnapshot(false) : null;
        const curMod = getActiveModule();
        let targetScope = curMod;
        if (curMod === "CONV") {
          targetScope = resolveSourceScopeKind(
            document.getElementById("conv-src-fmt")?.value || "docx"
          );
        }
        let totalMasked = 0;

        if (targetScope === "SHEET") {
          const curSheet = getActiveSheetName();
          sheetDataStore[curSheet] = sheetController.snapshotDomSheetCells();
          const scrubbed = await scrubSheetStoreMap(
            sheetDataStore,
            curSheet,
            sheetController.trackSheetAsyncWork
          );
          totalMasked = scrubbed.totalMasked;
          commitDeferredUndoSnapshot(preSnapshot);
          sheetController.replaceLiveSheetDataStore(scrubbed.nextSheets);
          sheetController.restoreDomSheetCells(curSheet);
          sheetController.refreshFocusedSheetCellPendingSnapshot();
          sheetController.updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "F2");
        } else if (targetScope === "SLIDE") {
          const fields = [];
          slideDeckState.forEach((slide) => {
            ["title", "lower", "notes"].forEach((key) => {
              if ((slide[key] || "").trim()) {
                fields.push({ slide, key, text: slide[key] });
              }
            });
          });
          const results = await Promise.all(
            fields.map((item) => invokeIpc("scrub_pii", item.text))
          );
          commitDeferredUndoSnapshot(preSnapshot);
          results.forEach((res, idx) => {
            if (res && typeof res.scrubbedText === "string") {
              const count = Number(res.totalMasked || 0);
              if (count > 0) fields[idx].slide[fields[idx].key] = res.scrubbedText;
              totalMasked += count;
            }
          });
          renderSlideDeck(getActiveSlideIndex());
        } else if (targetScope === "PDF") {
          totalMasked = await scrubPdfDom(preSnapshot);
        } else {
          ensureDefaultDocTabStore();
          const editorEl = document.getElementById("doc-editor");
          if (curMod === targetScope && editorEl) {
            const activeGroups = collectActiveScrubGroups(editorEl);
            const groupResults = await Promise.all(
              activeGroups.map((g) => invokeIpc("scrub_pii", g.origText))
            );
            commitDeferredUndoSnapshot(preSnapshot);
            totalMasked += applyScrubResultsToGroups(activeGroups, groupResults);
            computeDocTableSummary();
            saveActiveDocTabState();
          } else if (docTabStore[targetScope]?.editorHtml) {
            const tempDiv = document.createElement("div");
            tempDiv.innerHTML = docTabStore[targetScope].editorHtml;
            const activeGroups = collectActiveScrubGroups(tempDiv);
            const groupResults = await Promise.all(
              activeGroups.map((g) => invokeIpc("scrub_pii", g.origText))
            );
            commitDeferredUndoSnapshot(preSnapshot);
            const modMasked = applyScrubResultsToGroups(activeGroups, groupResults);
            if (modMasked > 0) {
              docTabStore[targetScope] = {
                ...docTabStore[targetScope],
                editorHtml: tempDiv.innerHTML,
              };
            }
            totalMasked += modMasked;
          }
        }

        if (totalMasked > 0) {
          piiOut.textContent =
            "주민번호·전화번호·이메일 " + totalMasked + "건 마스킹 완료 (" + curMod + ")";
        } else {
          piiOut.textContent =
            "주민번호·전화번호·이메일 스캔 완료 (감지 0건 · " + curMod + ")";
        }
        computeSmartStats();
      });
    }

    return {
      resolveSourceScopeKind,
      buildStructuredConversionBody,
      triggerBase64Download,
      saveActiveModuleDocument,
      computeSmartStats,
      applySmartDiagnoseDom,
      runSmartDiagnoseAction,
      buildSecurityAuditPayload,
      fetchSecurityAuditOutcome,
      applySecurityAuditDom,
      runSecurityAuditAction,
      runBenchmarkAction,
      runPiiScrubAction,
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_TOOLS__ = createWorkspaceTools;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
