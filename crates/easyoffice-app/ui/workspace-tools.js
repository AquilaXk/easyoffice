(function (host) {
  "use strict";

  const { serializeDomChildren, restoreDomChildren } = host.__EASYOFFICE_SHEET_UTILS__;

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
      getFocusedDocBlock,
      computeDocTableSummary,
      syncActiveSlideDomStylesToDeck,
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
          restoreDomChildren(tempDiv, saved.editorNodes || [], document);
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
          const rawVal = (entry?.formula || entry?.value || "").trim().replaceAll("\t", " ").replaceAll("\n", " ");
          const dispVal = (entry?.value || entry?.formula || "").trim().replaceAll("\t", " ").replaceAll("\n", " ");
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
              const dense = Array.from({ length: rowArr.length }, (_, i) => rowArr[i] || "");
              if (dense.length > 0) lines.push("[[TABLE_ROW]]\t" + dense.join("\t"));
            });
        }
      } else if (scope === "SLIDE") {
        if (getActiveModule() === "SLIDE" && typeof syncActiveSlideDomStylesToDeck === "function") {
          syncActiveSlideDomStylesToDeck();
        }
        slideDeckState.forEach((s) => {
          const title = (s.title || "").trim().replaceAll("\t", " ").replaceAll("\n", " ");
          let extraText = "";
          if (Array.isArray(s.insertedNodes) && s.insertedNodes.length > 0) {
            const extraDiv = document.createElement("div");
            extraDiv.className = "sheet-table";
            restoreDomChildren(extraDiv, s.insertedNodes, document);
            extraText = extractEditableDocumentText(extraDiv);
          }
          const combinedLower = [s.lower || "", extraText].filter(Boolean).join("\n");
          const bullets = combinedLower
            .split(/\n+/)
            .map((item) => item.trim().replaceAll("\t", " "))
            .filter(Boolean)
            .join(";;");
          const notes = (s.notes || "").trim().replaceAll("\t", " ").replaceAll("\n", " ");
          lines.push("[[SLIDE]]\t" + title + "\t" + bullets + "\t" + notes);
        });
      } else if (scope === "PDF") {
        document.querySelectorAll("#pdf-page-card h3, #pdf-page-card p:not(#pdf-form-status)").forEach((el) => {
          const t = (el.textContent || "").trim();
          if (t) lines.push(t);
        });
        const wmInputVal = (document.getElementById("pdf-watermark-input")?.value || "").trim();
        const wmLayerVal = (document.getElementById("pdf-watermark-layer")?.textContent || "").trim();
        const wmVal = wmInputVal || wmLayerVal;
        if (wmVal) lines.push("워터마크: " + wmVal);
        if (wmLayerVal && wmLayerVal !== wmVal) lines.push("워터마크 레이어: " + wmLayerVal);
        const stampVal = (
          document.getElementById("pdf-stamp-select")?.value ||
          document.getElementById("pdf-stamp-badge")?.textContent ||
          ""
        ).trim();
        if (stampVal) lines.push("결재 스탬프: " + stampVal);
        const applicantVal = (document.getElementById("pdf-form-applicant")?.value || "").trim();
        if (applicantVal) lines.push("신청인: " + applicantVal);
        const deptVal = (document.getElementById("pdf-form-dept")?.value || "").trim();
        if (deptVal) lines.push("부서: " + deptVal);
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
          JSON.stringify({
            sourceFormat: srcExt,
            targetFormat: dstExt,
            title,
            body,
          })
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

    let insertModeLabel = "삽입";

    function computeSmartStats() {
      const statsEl = document.getElementById("smart-stats");
      if (!statsEl) return;
      const text = extractEditableDocumentText();
      const chars = Array.from(text).length;
      const words = text ? text.split(/\s+/).length : 0;
      const secs = Math.max(1, Math.ceil((words / 220) * 60));
      statsEl.textContent = "문자: " + chars + " · 단어: " + words + " · 읽기 시간: " + secs + "초";
      const estPages = Math.max(1, Math.ceil(chars / 1200));
      const zoneEl = document.getElementById("doc-page-zone-indicator");
      if (zoneEl) {
        zoneEl.textContent = "1 / " + estPages + " 쪽 · 구역: 1 / 1 · " + insertModeLabel;
      }
      const posEl = document.getElementById("status-studio-pos");
      if (posEl) {
        const editor = document.getElementById("doc-editor");
        const cols = Number.parseInt(editor?.style?.columnCount || "1", 10) || 1;
        const lineCount = Math.max(1, text ? text.split(/\n+/).length : 1);
        const colChars = Math.max(1, chars % 60 || Math.min(60, chars));
        posEl.textContent =
          "1/" + estPages + " 쪽   " + cols + "단   " + lineCount + "줄   " + colChars + "칸 | 구역: 1 / 1 |";
      }
      const fileEl = document.getElementById("status-doc-filename");
      if (fileEl) {
        const activeTabTitle = document
          .querySelector(".tab-strip .tab-pill.active")
          ?.textContent?.replace(/×$/, "")
          ?.trim();
        const curMod = getActiveModule();
        const fallbackName =
          curMod === "HWP"
            ? "[붙임2] 행정안전부 전자결재 표준 기안문.hwpx"
            : "[붙임2] 2026년 디지털행정혁신 종합 추진계획(안).docx";
        fileEl.textContent = (activeTabTitle || fallbackName) + " — " + estPages + "페이지";
      }
    }

    function applySmartDiagnoseDom(res, updateFeedback) {
      if (!res) return;
      const statsEl = document.getElementById("smart-stats");
      const summaryEl = document.getElementById("smart-summary-output");
      const fb = document.getElementById("status-command-feedback");
      if (statsEl) {
        statsEl.textContent = `문자: ${res.charCount} · 단어: ${res.wordCount} · 문장: ${res.sentenceCount} · 읽기: ${res.readingTimeSeconds}초 · 요약: ${res.summaryCount}건`;
      }
      if (summaryEl) {
        const lines = Array.isArray(res.summaryLines) ? res.summaryLines : [];
        const actions = Array.isArray(res.actionItems) ? res.actionItems : [];
        const suggestions = Array.isArray(res.styleSuggestions) ? res.styleSuggestions : [];
        const summaryText = lines.length > 0 ? lines.join(" / ") : "핵심 문장 요약 완료";
        const actionSuffix = actions.length > 0 ? " · 조치: " + actions.join(", ") : "";
        const styleSuffix = suggestions.length > 0 ? " · 교정: " + suggestions.join(" / ") : "";
        summaryEl.textContent = "요약: " + summaryText + actionSuffix + styleSuffix;
      }
      if (fb && updateFeedback) {
        fb.textContent = `문장 진단 완료 · 요약 ${res.summaryCount}건 · 실행 항목 ${res.actionItemCount}건 · 교정 제안 ${res.suggestionCount}건`;
      }
    }

    async function runSmartDiagnoseAction() {
      const res = await invokeIpc("run_smart_assistant", extractEditableDocumentText());
      applySmartDiagnoseDom(res, true);
    }

    function resolveModuleContainerExt(targetMod) {
      const effectiveMod = targetMod || getActiveModule();
      const extMap = { DOC: "docx", HWP: "hwpx", SHEET: "xlsx", SLIDE: "pptx", PDF: "pdf", CONV: document.getElementById("conv-src-fmt")?.value || "docx" };
      return extMap[effectiveMod] || "docx";
    }

    async function runModuleSecurityAudit(targetMod) {
      const effectiveMod = targetMod || getActiveModule();
      const ext = resolveModuleContainerExt(effectiveMod);
      const body = buildStructuredConversionBody(effectiveMod === "CONV" ? ext : effectiveMod);
      const title = "SecurityAudit_" + effectiveMod;
      const convRes = await invokeIpc(
        "convert_document_format",
        JSON.stringify({ sourceFormat: ext, targetFormat: ext, title, body })
      );
      const containerBase64 = String(convRes?.convertedBase64 || convRes?.converted_base64 || "");
      return await invokeIpc("audit_security", JSON.stringify({ fileName: title + "." + ext, containerBase64 }));
    }

    async function fetchSecurityAuditOutcome(targetMod) {
      try {
        await runModuleSecurityAudit(targetMod);
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
      } else if (
        outcome.errorMessage.startsWith("엔진 미연결:") ||
        outcome.errorMessage.startsWith("IPC 네트워크 오류:")
      ) {
        txt.textContent = "[요청 거부] " + outcome.errorMessage;
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

      let pendingTotal = countPendingGroupMasks(nodeGroups, groupResults);
      if (wmInput && wmInputRes !== null) {
        assertValidScrubResult(wmInput.value, wmInputRes);
        pendingTotal += Number(wmInputRes.totalMasked || 0);
      }
      if (scrubSeparateLayer && wmLayer && wmLayerRes !== null) {
        assertValidScrubResult(wmLayer.textContent, wmLayerRes);
        pendingTotal += Number(wmLayerRes.totalMasked || 0);
      }
      if (appInput && appRes !== null) {
        assertValidScrubResult(appInput.value, appRes);
        pendingTotal += Number(appRes.totalMasked || 0);
      }
      if (deptInput && deptRes !== null) {
        assertValidScrubResult(deptInput.value, deptRes);
        pendingTotal += Number(deptRes.totalMasked || 0);
      }

      if (pendingTotal === 0) {
        return 0;
      }

      commitDeferredUndoSnapshot(preSnapshot);
      let totalMasked = applyScrubResultsToGroups(nodeGroups, groupResults);
      if (wmInput && wmInputRes) {
        const count = Number(wmInputRes.totalMasked || 0);
        if (count > 0) wmInput.value = wmInputRes.scrubbedText;
        totalMasked += count;
      }
      if (wmLayer) {
        if (wmInput && wmLayer.textContent === prevWmInputVal) {
          wmLayer.textContent = wmInput.value;
        } else if (wmLayerRes) {
          const count = Number(wmLayerRes.totalMasked || 0);
          if (count > 0) wmLayer.textContent = wmLayerRes.scrubbedText;
          totalMasked += count;
        }
      }
      if (appInput && appRes) {
        const count = Number(appRes.totalMasked || 0);
        if (count > 0) appInput.value = appRes.scrubbedText;
        totalMasked += count;
      }
      if (deptInput && deptRes) {
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
          let scrubbed;
          try {
            scrubbed = await scrubSheetStoreMap(
              sheetDataStore,
              curSheet,
              sheetController.trackSheetAsyncWork
            );
          } catch (sheetScrubErr) {
            const fb = document.getElementById("status-command-feedback");
            if (fb && !String(fb.textContent || "").startsWith("[요청 거부]")) {
              fb.textContent = "[요청 거부] " + (sheetScrubErr?.message || String(sheetScrubErr));
            }
            throw sheetScrubErr;
          }
          totalMasked = scrubbed.totalMasked;
          if (totalMasked > 0) {
            commitDeferredUndoSnapshot(preSnapshot);
            sheetController.replaceLiveSheetDataStore(scrubbed.nextSheets);
            sheetController.restoreDomSheetCells(curSheet);
            sheetController.refreshFocusedSheetCellPendingSnapshot();
            sheetController.updateSheetSelectionAndAggregates(getActiveSheetCellRef() || "F2");
          }
        } else if (targetScope === "SLIDE") {
          if (curMod === "SLIDE" && typeof syncActiveSlideDomStylesToDeck === "function") {
            syncActiveSlideDomStylesToDeck();
          }
          const fields = [];
          const slideNodeJobs = [];
          slideDeckState.forEach((slide) => {
            ["title", "lower", "notes"].forEach((key) => {
              if ((slide[key] || "").trim()) {
                fields.push({ slide, key, text: slide[key] });
              }
            });
            if (Array.isArray(slide.insertedNodes) && slide.insertedNodes.length > 0) {
              const tempDiv = document.createElement("div");
              restoreDomChildren(tempDiv, slide.insertedNodes, document);
              const groups = collectActiveScrubGroups(tempDiv);
              if (groups.length > 0) {
                slideNodeJobs.push({ slide, tempDiv, groups });
              }
            }
          });
          const [results, nodeJobResults] = await Promise.all([
            Promise.all(fields.map((item) => invokeIpc("scrub_pii", item.text))),
            Promise.all(
              slideNodeJobs.map((job) =>
                Promise.all(job.groups.map((g) => invokeIpc("scrub_pii", g.origText)))
              )
            ),
          ]);
          results.forEach((res, idx) => {
            assertValidScrubResult(fields[idx].text, res);
            totalMasked += Number(res.totalMasked || 0);
          });
          nodeJobResults.forEach((groupRes, jIdx) => {
            totalMasked += countPendingGroupMasks(slideNodeJobs[jIdx].groups, groupRes);
          });
          if (totalMasked > 0) {
            commitDeferredUndoSnapshot(preSnapshot);
            results.forEach((res, idx) => {
              const count = Number(res.totalMasked || 0);
              if (count > 0) fields[idx].slide[fields[idx].key] = res.scrubbedText;
            });
            nodeJobResults.forEach((groupRes, jIdx) => {
              const job = slideNodeJobs[jIdx];
              const modCount = applyScrubResultsToGroups(job.groups, groupRes);
              if (modCount > 0) {
                job.slide.insertedNodes = serializeDomChildren(job.tempDiv);
              }
            });
            renderSlideDeck(getActiveSlideIndex());
          }
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
            const pendingCount = countPendingGroupMasks(activeGroups, groupResults);
            if (pendingCount > 0) {
              commitDeferredUndoSnapshot(preSnapshot);
              totalMasked += applyScrubResultsToGroups(activeGroups, groupResults);
              computeDocTableSummary();
              saveActiveDocTabState();
            }
          } else if (Array.isArray(docTabStore[targetScope]?.editorNodes)) {
            const tempDiv = document.createElement("div");
            restoreDomChildren(tempDiv, docTabStore[targetScope].editorNodes, document);
            const activeGroups = collectActiveScrubGroups(tempDiv);
            const groupResults = await Promise.all(
              activeGroups.map((g) => invokeIpc("scrub_pii", g.origText))
            );
            const pendingCount = countPendingGroupMasks(activeGroups, groupResults);
            if (pendingCount > 0) {
              commitDeferredUndoSnapshot(preSnapshot);
              const modMasked = applyScrubResultsToGroups(activeGroups, groupResults);
              if (modMasked > 0) {
                docTabStore[targetScope] = {
                  ...docTabStore[targetScope],
                  editorNodes: serializeDomChildren(tempDiv),
                };
              }
              totalMasked += modMasked;
            }
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

    function setupStudioChromeControls(applyZoomPercent, onStudioCmd) {
      const editor = document.getElementById("doc-editor");
      const chromeToggleBtn = document.getElementById("btn-chrome-toggle");
      if (chromeToggleBtn) {
        chromeToggleBtn.addEventListener("click", () => {
          const shell = document.getElementById("app") || document.querySelector(".app-shell");
          if (!shell) return;
          const next = shell.dataset.chromeExpanded !== "true";
          shell.dataset.chromeExpanded = next ? "true" : "false";
          shell.dataset.panelsExpanded = next ? "true" : "false";
          chromeToggleBtn.setAttribute("aria-expanded", next ? "true" : "false");
          chromeToggleBtn.textContent = next ? "▴" : "▾";
        });
      }
      document.querySelectorAll(".studio-menu-item[data-menu-cat]").forEach((btn) => {
        btn.addEventListener("click", () => {
          if (btn.dataset.menuCat === "file") {
            document.getElementById("qat-save")?.click();
            return;
          }
          const shell = document.getElementById("app") || document.querySelector(".app-shell");
          if (shell && shell.dataset.chromeExpanded !== "true") {
            shell.dataset.chromeExpanded = "true";
            shell.dataset.panelsExpanded = "true";
            if (chromeToggleBtn) {
              chromeToggleBtn.setAttribute("aria-expanded", "true");
              chromeToggleBtn.textContent = "▴";
            }
          }
          const rBtn = document.querySelector('.ribbon-btn[data-category="' + btn.dataset.menuCat + '"]');
          if (rBtn) rBtn.click();
        });
      });
      let studioClipboardText = "";
      function withDocStyleUndo(mutateFn) {
        const pre = typeof captureWorkspaceSnapshot === "function" ? captureWorkspaceSnapshot(false) : null;
        mutateFn();
        saveActiveDocTabState();
        if (pre && typeof commitDeferredUndoSnapshot === "function") commitDeferredUndoSnapshot(pre);
      }
      function resolveActiveDocBlock() {
        return (typeof getFocusedDocBlock === "function" && getFocusedDocBlock(editor)) || editor?.querySelector("p") || null;
      }
      document.querySelectorAll("#icon-toolbar [data-studio-cmd]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const cmd = btn.dataset.studioCmd;
          if (!cmd) return;
          if (cmd === "palette") return void document.getElementById("btn-command-palette")?.click();
          if (cmd === "studio-save") return void document.getElementById("qat-save")?.click();
          if (cmd === "studio-undo") return void document.getElementById("qat-undo")?.click();
          if (cmd === "studio-redo") return void document.getElementById("qat-redo")?.click();
          const fb = document.getElementById("status-command-feedback");
          const blk = resolveActiveDocBlock();
          const textTarget = blk?.querySelector?.(".eo-h-title") || blk;
          if (cmd === "clip-copy") {
            studioClipboardText = (textTarget?.textContent || "").trim();
            if (fb) fb.textContent = "선택 문단 클립보드 복사 완료 (" + studioClipboardText.length + "자)";
            return;
          }
          if (cmd === "clip-cut") {
            withDocStyleUndo(() => {
              studioClipboardText = (textTarget?.textContent || "").trim();
              if (textTarget) textTarget.textContent = "";
              computeSmartStats();
            });
            if (fb) fb.textContent = "선택 문단 오려두기 완료";
            return;
          }
          if (cmd === "clip-paste") {
            withDocStyleUndo(() => {
              const p = document.createElement("p");
              p.textContent = studioClipboardText || "붙여넣기 문단";
              if (blk?.parentNode === editor) blk.insertAdjacentElement("afterend", p);
              else editor?.appendChild(p);
              computeSmartStats();
            });
            if (fb) fb.textContent = "클립보드 문단 붙이기 완료";
            return;
          }
          if (cmd === "para-numbering" || cmd === "para-bullet") {
            const prefix = cmd === "para-numbering" ? "1. " : "○ ";
            withDocStyleUndo(() => {
              if (textTarget) {
                const cur = textTarget.textContent || "";
                textTarget.textContent = cur.startsWith(prefix) ? cur.slice(prefix.length) : prefix + cur;
              }
              computeSmartStats();
            });
            if (fb) fb.textContent = cmd === "para-numbering" ? "문단 번호 서식 적용 완료" : "글머리표 서식 적용 완료";
            return;
          }
          if (cmd === "indent-in" || cmd === "indent-out") {
            withDocStyleUndo(() => {
              if (blk) {
                const curPx = Number.parseInt(blk.style.paddingLeft || "0", 10) || 0;
                const nextPx = Math.max(0, Math.min(96, curPx + (cmd === "indent-in" ? 16 : -16)));
                blk.style.paddingLeft = nextPx ? nextPx + "px" : "";
              }
            });
            if (fb) fb.textContent = cmd === "indent-in" ? "한 수준 증가 적용 완료" : "한 수준 감소 적용 완료";
            return;
          }
          if (cmd === "para-shape") {
            document.getElementById("btn-align-justify")?.click();
            if (fb) fb.textContent = "문단 모양 정렬 적용 완료";
            return;
          }
          const isHwp = getActiveModule() === "HWP";
          if (cmd === "mod-symbol") {
            if (isHwp && typeof onStudioCmd === "function") return void onStudioCmd("hwp-mod-field");
            withDocStyleUndo(() => {
              if (textTarget) textTarget.textContent = (textTarget.textContent || "") + " ※";
              computeSmartStats();
            });
            if (fb) fb.textContent = "문자표 기호 삽입 완료";
            return;
          }
          if (cmd === "mod-header") {
            if (isHwp && typeof onStudioCmd === "function") return void onStudioCmd("hwp-mod-header");
            withDocStyleUndo(() => {
              const hdr = editor?.querySelector(".eo-hwp-header") || document.createElement("header");
              hdr.className = "eo-hwp-header";
              hdr.dataset.hwpHeader = "true";
              const p = document.createElement("p");
              p.textContent = "문서 머리말 · 꼬리말 (표준 양식)";
              hdr.replaceChildren(p);
              if (!hdr.parentElement && editor) editor.prepend(hdr);
              computeSmartStats();
            });
            if (fb) fb.textContent = "머리말 · 꼬리말 적용 완료";
            return;
          }
          if (cmd === "mod-footnote") {
            if (!isHwp && typeof onStudioCmd === "function") return void onStudioCmd("doc-mod-footnote");
            withDocStyleUndo(() => {
              const idx = (editor?.querySelectorAll(".eo-doc-footnote").length || 0) + 1;
              const aside = document.createElement("aside");
              aside.className = "eo-doc-footnote";
              aside.dataset.docFootnote = "true";
              aside.textContent = "각주 " + idx + ") 표준 행정 문서 관리 지침 및 근거 조항 참조";
              editor?.appendChild(aside);
              computeSmartStats();
            });
            if (fb) fb.textContent = "각주 · 미주 삽입 완료";
            return;
          }
          if (typeof onStudioCmd === "function") onStudioCmd(cmd);
        });
      });
      const styleSelect = document.getElementById("doc-style-select");
      const roleBadge = document.getElementById("doc-style-role-badge");
      if (styleSelect && editor) {
        styleSelect.addEventListener("change", () =>
          withDocStyleUndo(() => {
            if (roleBadge) roleBadge.textContent = styleSelect.value;
            editor.dataset.styleRole = styleSelect.value;
          })
        );
      }
      const fontLang = document.getElementById("doc-font-lang");
      if (fontLang && editor) {
        fontLang.addEventListener("change", () => withDocStyleUndo(() => { editor.dataset.fontLang = fontLang.value; }));
      }
      const fontSelect = document.getElementById("doc-font-select");
      const fontBadge = document.getElementById("doc-style-font-badge");
      if (fontSelect && editor) {
        fontSelect.addEventListener("change", () =>
          withDocStyleUndo(() => {
            if (fontBadge) fontBadge.textContent = fontSelect.value;
            editor.style.fontFamily = fontSelect.value;
          })
        );
      }
      const sizeInput = document.getElementById("doc-font-size-input");
      const sizeBadge = document.getElementById("doc-style-size-badge");
      function applyFontSize(pt) {
        withDocStyleUndo(() => {
          const formatted = Math.max(6, Math.min(72, Number.isFinite(pt) ? pt : 11)).toFixed(1);
          if (sizeInput) sizeInput.value = formatted;
          if (sizeBadge) sizeBadge.textContent = formatted + " pt";
          if (editor) editor.style.fontSize = formatted + "pt";
        });
      }
      sizeInput?.addEventListener("change", () => applyFontSize(parseFloat(sizeInput.value)));
      document.getElementById("btn-size-up")?.addEventListener("click", () => applyFontSize(parseFloat(sizeInput?.value || "11") + 1));
      document.getElementById("btn-size-down")?.addEventListener("click", () => applyFontSize(parseFloat(sizeInput?.value || "11") - 1));
      const lsSelect = document.getElementById("doc-linespacing-select");
      const lsSteps = [100, 130, 160, 180, 200, 300];
      function applyLineSpacing(pct) {
        withDocStyleUndo(() => {
          const clamped = Math.max(100, Math.min(300, Number.isFinite(pct) ? pct : 160));
          if (lsSelect) lsSelect.value = String(clamped);
          if (editor) {
            editor.dataset.lineSpacing = String(clamped);
            editor.style.lineHeight = (clamped / 100).toFixed(2);
          }
        });
      }
      lsSelect?.addEventListener("change", () => applyLineSpacing(parseInt(lsSelect.value, 10)));
      document.getElementById("btn-ls-up")?.addEventListener("click", () => {
        const cur = parseInt(lsSelect?.value || "160", 10) || 160;
        applyLineSpacing(lsSteps.find((s) => s > cur) || 300);
      });
      document.getElementById("btn-ls-down")?.addEventListener("click", () => {
        const cur = parseInt(lsSelect?.value || "160", 10) || 160;
        applyLineSpacing([...lsSteps].reverse().find((s) => s < cur) || 100);
      });
      [
        ["btn-stylebar-strike", "strike"],
        ["btn-stylebar-charfx", "charfx"],
        ["btn-stylebar-textcolor", "textcolor"],
        ["btn-stylebar-highlight", "highlight"],
      ].forEach(([id, key]) => {
        const btn = document.getElementById(id);
        btn?.addEventListener("click", () =>
          withDocStyleUndo(() => {
            const next = !btn.classList.contains("active");
            btn.classList.toggle("active", next);
            btn.setAttribute("aria-pressed", next ? "true" : "false");
            if (editor) editor.dataset[key] = next ? "true" : "false";
          })
        );
      });
      const alignMap = [
        ["btn-align-left", "left", "left"],
        ["btn-align-center", "center", "center"],
        ["btn-align-right", "right", "right"],
        ["btn-align-justify", "justify", "justify"],
        ["btn-align-distribute", "distribute", "justify"],
        ["btn-align-divide", "divide", "justify"],
      ];
      alignMap.forEach(([id, mode, cssAlign]) => {
        document.getElementById(id)?.addEventListener("click", () =>
          withDocStyleUndo(() => {
            alignMap.forEach(([otherId]) => {
              const other = document.getElementById(otherId);
              if (other) {
                other.classList.toggle("active", otherId === id);
                other.setAttribute("aria-pressed", otherId === id ? "true" : "false");
              }
            });
            if (editor) {
              editor.dataset.textAlign = mode;
              editor.style.textAlign = cssAlign;
            }
          })
        );
      });
      const bindToggleMark = (id, key) => {
        const btn = document.getElementById(id);
        if (btn && editor) {
          btn.addEventListener("click", () =>
            withDocStyleUndo(() => {
              const next = !btn.classList.contains("active");
              btn.classList.toggle("active", next);
              btn.setAttribute("aria-pressed", next ? "true" : "false");
              editor.dataset[key] = next ? "true" : "false";
            })
          );
        }
      };
      bindToggleMark("btn-toggle-ctrl-marks", "showCtrlMarks");
      bindToggleMark("btn-toggle-para-marks", "showParaMarks");
      const docGridBtn = document.getElementById("btn-toggle-doc-grid");
      if (docGridBtn && editor) {
        docGridBtn.addEventListener("click", () =>
          withDocStyleUndo(() => {
            const next = !docGridBtn.classList.contains("active");
            docGridBtn.classList.toggle("active", next);
            docGridBtn.setAttribute("aria-pressed", next ? "true" : "false");
            editor.classList.toggle("manuscript-grid", next);
            const hwpPreview = document.getElementById("hwp-manuscript-preview");
            if (hwpPreview) {
              hwpPreview.classList.toggle("manuscript-grid", next);
              hwpPreview.hidden = getActiveModule() !== "HWP" || !next;
            }
          })
        );
      }
      const insertBtn = document.getElementById("status-insert-mode");
      function toggleInsertMode() {
        insertModeLabel = insertModeLabel === "삽입" ? "수정" : "삽입";
        if (insertBtn) insertBtn.textContent = insertModeLabel;
        computeSmartStats();
      }
      insertBtn?.addEventListener("click", toggleInsertMode);
      host.addEventListener("keydown", (ev) => {
        if (ev.key === "Insert") toggleInsertMode();
      });
      const zoomSlider = document.getElementById("status-zoom-slider");
      if (zoomSlider && typeof applyZoomPercent === "function") {
        const onSlide = () => applyZoomPercent(Number(zoomSlider.value) || 100);
        zoomSlider.addEventListener("input", onSlide);
        zoomSlider.addEventListener("change", onSlide);
      }
      document.getElementById("btn-zoom-fit-width")?.addEventListener("click", () => applyZoomPercent?.(120, "width"));
      document.getElementById("btn-zoom-fit-page")?.addEventListener("click", () => applyZoomPercent?.(100, "page"));
    }

    return {
      resolveSourceScopeKind,
      buildStructuredConversionBody,
      triggerBase64Download,
      saveActiveModuleDocument,
      computeSmartStats,
      applySmartDiagnoseDom,
      runSmartDiagnoseAction,
      runModuleSecurityAudit,
      fetchSecurityAuditOutcome,
      applySecurityAuditDom,
      runSecurityAuditAction,
      runBenchmarkAction,
      runPiiScrubAction,
      setupStudioChromeControls,
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_TOOLS__ = createWorkspaceTools;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
