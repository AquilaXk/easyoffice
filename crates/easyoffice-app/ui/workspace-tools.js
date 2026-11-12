(function (host) {
  "use strict";

  const { restoreDomChildren } = host.__EASYOFFICE_SHEET_UTILS__;
  const savePayload = host.__EASYOFFICE_SAVE_PAYLOAD__;
  const desktopBridge = host.__EASYOFFICE_DESKTOP_BRIDGE__;
  const LS_STEPS = [100, 130, 160, 180, 200, 300];
  const LS_STEPS_REV = [300, 200, 180, 160, 130, 100];
  const STUDIO_QUICK_REDIRECT = {
    palette: "btn-command-palette",
    "zoom-in": "btn-zoom-in", "zoom-out": "btn-zoom-out", "zoom-fit-page": "btn-zoom-fit-page", "zoom-fit-width": "btn-zoom-fit-width",
    "ctrl-marks": "btn-toggle-ctrl-marks", "para-marks": "btn-toggle-para-marks", "doc-grid": "btn-toggle-doc-grid",
    "table-add-row": "btn-doc-add-row", "table-merge": "btn-doc-merge-toggle", "table-sum": "btn-doc-eval-table",
    "smart-diagnose": "btn-smart-diagnose", "scrub-pii": "btn-scrub-pii", "security-audit": "btn-run-security-audit", "view-theme": "btn-theme-toggle",
  };
  // Menu command -> tool dialog (PD-098 step 3b); the action buttons keep their ids.
  const TOOL_DIALOGS = { "smart-diagnose": "smart-dialog", "scrub-pii": "pii-dialog", "security-audit": "security-dialog", "bench-run": "bench-dialog", "view-outline": "outline-dialog" };
  // Menu command -> app-provided action.
  const STUDIO_DIRECT_ACTIONS = { "studio-save": "save", "studio-save-as": "saveAs", "studio-undo": "undo", "studio-redo": "redo", "bench-run": "benchmark", find: "find", replace: "replace", print: "print" };
  const EMBED_BLIND_TOOLS = new Set(["scrub-pii", "security-audit"]);
  const SYMBOL_TABLE_CHAR = "※";
  // `target`: the ribbon command a module runs for it (`hwp` tells which module); none means the page inserts it itself.
  const INSERT_MOD_DISPATCH = {
    "mod-symbol": { label: "문자표 기호 삽입 완료" },
    "mod-header": { hwp: true, target: "hwp-mod-header", label: "머리말 · 꼬리말 적용 완료" },
    "mod-footnote": { hwp: false, target: "doc-mod-footnote", label: "각주 · 미주 삽입 완료" },
  };

  function describeConversionSpec(res) {
    const limited = res?.compatibility === "limited" ? " · 호환성 제한: " + (res.compatibilityWarning || "") : "";
    const notes = Array.isArray(res?.warnings) && res.warnings.length > 0 ? " · 참고: " + res.warnings.join(" ") : "";
    return (res?.specification || "") + limited + notes;
  }

  // Distributed / divided paragraphs have no counterpart in the document model and are written as justified.
  function describeApproximatedAlign(doc, mod) {
    if (mod !== "DOC" && mod !== "HWP") return "";
    const count = doc.querySelectorAll("#doc-editor [data-eo-align]").length;
    return count > 0 ? " · 배분·나눔 정렬 " + count + "개 문단은 양쪽 정렬로 저장" : "";
  }

  // The PDF Studio's highlight annotation as the card badge shows it: "형광펜: '문구'".
  function describePdfHighlight(text, kind) {
    return (kind === "underline" ? "밑줄" : "형광펜") + ": '" + text + "'";
  }

  function describeDeliveryFailure(delivery, prefix) {
    if (delivery?.ok) return "";
    if (delivery?.cancelled) return prefix + "저장 취소: 저장 위치를 선택하지 않았습니다";
    if (delivery?.error) return prefix + "파일 저장 실패: " + delivery.error;
    return prefix + "다운로드 실패: 변환 결과 파일을 받지 못했습니다";
  }

  function buildSmartSummaryText(res) {
    const lines = Array.isArray(res.summaryLines) ? res.summaryLines : [];
    const actions = Array.isArray(res.actionItems) ? res.actionItems : [];
    const suggestions = Array.isArray(res.styleSuggestions) ? res.styleSuggestions : [];
    const summaryText = lines.length > 0 ? lines.join(" / ") : "핵심 문장 요약 완료";
    const actionSuffix = actions.length > 0 ? " · 조치: " + actions.join(", ") : "";
    const styleSuffix = suggestions.length > 0 ? " · 교정: " + suggestions.join(" / ") : "";
    return "요약: " + summaryText + actionSuffix + styleSuffix;
  }

  function createWorkspaceTools(deps, panelHooks) {
    const {
      document,
      URL,
      invokeIpc,
      docTabStore,
      sheetDataStore,
      slideDeckState,
      extractEditableDocumentText,
      sheetController,
      getActiveModule,
      getHwpHost,
      getHwpSession,
      getActiveSheetName,
      enqueueWorkspaceHistoryOp,
      captureWorkspaceSnapshot,
      commitDeferredUndoSnapshot,
      runStudioClipboardCommand,
      insertTextAtSelection,
      getDocRuns,
      getWindowBridge,
      getFileOpen,
      getPdfRotationDeg,
    } = deps;

    const {
      saveActiveDocTabState,
      ensureDefaultDocTabStore,
      getFocusedDocBlock,
      syncActiveSlideDomStylesToDeck,
    } = panelHooks;

    const SCOPE_BY_FORMAT = Object.freeze({
      xlsx: "SHEET", xls: "SHEET", ods: "SHEET", csv: "SHEET", tsv: "SHEET", sheet: "SHEET",
      pptx: "SLIDE", ppsx: "SLIDE", ppt: "SLIDE", odp: "SLIDE", slide: "SLIDE",
      pdf: "PDF",
      hwp: "HWP", hwt: "HWP", hwpx: "HWP", hwtx: "HWP",
    });

    function resolveSourceScopeKind(sourceFormatOrModule) {
      const token = String(sourceFormatOrModule || getActiveModule() || "DOC").trim().toLowerCase();
      return SCOPE_BY_FORMAT[token] || "DOC";
    }

    // Structured save content (PD-073): `blocks` for DOC/HWP/PDF, every sheet for SHEET, `slides`
    // for SLIDE. Merged into the convert_document_format JSON payload; no line markup.
    function buildConversionContent(sourceFormatOrModule) {
      saveActiveDocTabState();
      ensureDefaultDocTabStore();
      const scope = resolveSourceScopeKind(sourceFormatOrModule);
      if (scope === "DOC" || scope === "HWP") {
        const saved = docTabStore[scope];
        const tempDiv = document.createElement("div");
        restoreDomChildren(tempDiv, saved?.editorNodes || [], document);
        return {
          blocks: savePayload.collectDocBlocks(tempDiv, {
            baseAlign: saved?.layoutStyle?.textAlign,
            baseLineHeight: saved?.layoutStyle?.lineHeight,
          }),
        };
      }
      if (scope === "SHEET") {
        sheetDataStore[getActiveSheetName()] = sheetController.snapshotDomSheetCells();
        const keys = Array.from(document.querySelectorAll(".sheet-tab-btn[data-sheet-tab]")).map(
          (b) => b.dataset.sheetTab
        );
        return { sheets: savePayload.buildSheetsPayload(keys, sheetDataStore, sheetController.getColumnWidths(), sheetController.getSheetRules(), host.__EASYOFFICE_SHEET_LAYOUT__.getAll()) };
      }
      if (scope === "SLIDE") {
        if (getActiveModule() === "SLIDE" && typeof syncActiveSlideDomStylesToDeck === "function") {
          syncActiveSlideDomStylesToDeck();
        }
        const slides = slideDeckState.map((s) => {
          let extraText = "";
          if (Array.isArray(s.insertedNodes) && s.insertedNodes.length > 0) {
            const extraDiv = document.createElement("div");
            extraDiv.className = "sheet-table";
            restoreDomChildren(extraDiv, s.insertedNodes, document);
            extraText = extractEditableDocumentText(extraDiv);
          }
          const bullets = [s.lower || "", extraText].join("\n").split(/\n+/).map((t) => t.trim()).filter(Boolean);
          return { title: (s.title || "").trim(), bullets, notes: (s.notes || "").trim() };
        });
        return { slides };
      }
      const lines = [];
      document.querySelectorAll("#pdf-page-card h3, #pdf-page-card p:not(#pdf-form-status)").forEach((el) => {
        lines.push(el.textContent || "");
      });
      const valOf = (id) => (document.getElementById(id)?.value || "").trim();
      const wmLayerVal = (document.getElementById("pdf-watermark-layer")?.textContent || "").trim();
      const stampVal = (valOf("pdf-stamp-select") || document.getElementById("pdf-stamp-badge")?.textContent || "").trim();
      if (valOf("pdf-form-applicant")) lines.push("신청인: " + valOf("pdf-form-applicant"));
      if (valOf("pdf-form-dept")) lines.push("부서: " + valOf("pdf-form-dept"));
      // Page settings travel as fields (a PDF target writes them as page rotation, watermark and stamp).
      const pdf = { rotationDegrees: getPdfRotationDeg?.() ?? 0 };
      const watermark = valOf("pdf-watermark-input") || wmLayerVal;
      if (watermark) pdf.watermark = watermark;
      if (stampVal) pdf.stamp = stampVal;
      const highlightText = valOf("pdf-highlight-input");
      if (highlightText) {
        pdf.highlight = {
          text: highlightText,
          kind: valOf("pdf-highlight-kind") || "highlight",
          color: valOf("pdf-highlight-color") || "#FFFF00",
        };
      }
      return { blocks: savePayload.plainParagraphBlocks(lines), pdf };
    }

    // The one place the converter decides its input (conversion, Ctrl+S in the converter, security audit):
    // the chosen source file, else the open HWP document (exported by the editor), else the editor content.
    // `srcFormat` is the converter's source selection; `domFormat` the format the editor content is read as.
    // Resolves { srcFormat, title, content } (`title` is null for editor content) or { problem }.
    async function resolveConverterInput(srcFormat, domFormat = srcFormat) {
      const fileOpen = getFileOpen?.();
      const source = fileOpen?.getConverterSource() || (await getHwpSession?.()?.getConverterSource(srcFormat));
      if (!source) return { srcFormat: domFormat, title: null, content: buildConversionContent(domFormat) };
      const problem = fileOpen?.converterSourceProblem(source.size) || "";
      if (problem) return { problem };
      return { srcFormat: source.format || srcFormat, title: source.baseName, content: source.fields };
    }

    // Download file name from the module's document tab title (PD-073).
    function resolveSaveFileBase(mod) {
      const name = getWindowBridge?.()?.getDocumentName(mod);
      return savePayload.sanitizeFileBaseName(name, "EasyOffice_" + mod + "_문서");
    }

    // A saved document takes the name of the file it was saved as (converter output does not).
    function noteSavedName(name) {
      if (getActiveModule() !== "CONV" && name) getWindowBridge?.()?.setDocumentName(name);
    }

    function triggerBase64Download(base64Data, baseFilename, ext) {
      if (!base64Data) return false;
      try {
        const bytes = host.__EASYOFFICE_SHEET_UTILS__.base64ToBytes(base64Data);
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

    // "Save" (not "save as" or an export) writes back to the opened file when the shell knows it.
    const deliverInPlace = (base64Data, baseFilename, ext, extra) =>
      deliverSavedFile(base64Data, baseFilename, ext, { ...extra, inPlace: true });

    // Hands saved bytes to the user: the native save dialog inside the desktop shell (WebView2
    // downloads can be silent), the Blob download in the `--serve` browser preview. Never throws;
    // resolves { ok, cancelled, path, error }.
    async function deliverSavedFile(base64Data, baseFilename, ext, options) {
      const fileName = baseFilename + "." + String(ext).toLowerCase();
      if (!base64Data) return { ok: false, cancelled: false, name: "", error: "" };
      if (desktopBridge?.isAvailable()) {
        try {
          const res = await desktopBridge.saveFile(base64Data, fileName, options);
          if (res.status === "cancelled") return { ok: false, cancelled: true, name: "", error: "" };
          return { ok: true, cancelled: false, name: String(res.name || fileName), error: "" };
        } catch (err) {
          return { ok: false, cancelled: false, name: "", error: err?.message || String(err) };
        }
      }
      const ok = triggerBase64Download(base64Data, baseFilename, ext);
      return { ok, cancelled: false, name: ok ? fileName : "", error: "" };
    }

    // Embedded HWP editor: HWP 5.0 bytes exported by the editor itself, same delivery as every save.
    // `closing`: the save runs for a window close or quit, where a password document goes to a save-as.
    async function saveEmbeddedHwp(session, closing, saveAs) {
      const fb = document.getElementById("status-command-feedback");
      const inPlace = !saveAs;
      const result = await session.save({
        deliver: inPlace ? deliverInPlace : deliverSavedFile,
        inPlace,
        closing,
        baseName: resolveSaveFileBase("HWP"),
      });
      if (fb) fb.textContent = result.message;
      if (result.ok) noteSavedName(result.name);
      return result.ok;
    }

    // The PDF studio on the opened file (PD-170): the shell asks where, and writes the edited file.
    async function savePdfEdit(pdfEdit) {
      try {
        const res = await pdfEdit.save();
        if (res.cancelled) {
          setStatusFeedback("문서 저장 취소 (PDF): 저장 위치를 선택하지 않았습니다");
          return false;
        }
        setStatusFeedback("문서 저장 완료 (" + res.name + " · " + res.bytes + "B · " + res.pages + "쪽 · 원본 PDF의 그림·글꼴·배치 보존)");
        noteSavedName(res.name);
        return true;
      } catch (err) {
        setStatusFeedback("문서 저장 실패 (PDF): " + (err?.message || String(err)));
        return false;
      }
    }

    function setStatusFeedback(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    async function convertForSave(srcExt, dstExt, title, content) {
      try {
        const res = await invokeIpc(
          "convert_document_format",
          JSON.stringify({ sourceFormat: srcExt, targetFormat: dstExt, title, ...content })
        );
        const b64 = String(res?.convertedBase64 || "");
        return { res, b64, failure: b64 ? "" : "변환 결과가 비어 있습니다" };
      } catch (err) {
        return { res: null, b64: "", failure: err?.message || String(err) };
      }
    }

    const SAVE_EXT_BY_MODULE = { DOC: "docx", HWP: "hwpx", SHEET: "xlsx", SLIDE: "pptx", PDF: "pdf" };
    const BIDIRECTIONAL_SAVE_EXTS = {
      DOC: new Set(["docx", "odt", "rtf", "md", "txt", "hwpx"]),
      HWP: new Set(["hwpx"]),
      SHEET: new Set(["xlsx", "xls", "ods", "csv", "tsv"]),
      SLIDE: new Set(["pptx", "ppsx", "odp"]),
    };

    function openedExtensionOf(mod) {
      const name = String(getWindowBridge?.()?.getDocumentName(mod) || "").trim();
      const dot = name.lastIndexOf(".");
      if (dot < 0 || dot === name.length - 1) return "";
      const ext = name.slice(dot + 1).toLowerCase();
      return ext === "markdown" ? "md" : ext;
    }

    function defaultSaveSpecForModule(mod, saveAs) {
      const fallbackExt = SAVE_EXT_BY_MODULE[mod] || "docx";
      if (saveAs || mod === "PDF") return { ext: fallbackExt, inPlace: false };
      const openedExt = openedExtensionOf(mod);
      // No file name: the document came from no file (or its open failed). Save asks where to write;
      // a window that saved once carries the file's name, so its next Save goes back to that file.
      if (!openedExt) return { ext: fallbackExt, inPlace: false };
      if (BIDIRECTIONAL_SAVE_EXTS[mod]?.has(openedExt)) return { ext: openedExt, inPlace: true };
      return { ext: fallbackExt, inPlace: false };
    }

    // `saveAs` and lossy decode formats always ask where to save (the shell's save dialog), never overwriting the opened file.
    async function runSaveActiveModuleDocument(closing, saveAs) {
      await sheetController.commitPendingFocusedSheetCell(true);
      const curMod = getActiveModule();
      const hwpHost = getHwpHost?.();
      const hwpSession = getHwpSession?.();
      if (curMod === "HWP" && hwpHost?.isEmbedActive() && hwpSession) return saveEmbeddedHwp(hwpSession, closing === true, saveAs);
      if (curMod === "PDF" && deps.getPdfEdit?.()?.isActive()) return savePdfEdit(deps.getPdfEdit());
      const isConv = curMod === "CONV";
      const spec = isConv
        ? { ext: document.getElementById("conv-dst-fmt")?.value || "hwpx", inPlace: false }
        : defaultSaveSpecForModule(curMod, saveAs);
      const dstExt = spec.ext || "docx";
      let srcExt = dstExt;
      let title = resolveSaveFileBase(curMod);
      let content;
      if (isConv) {
        const convSrc = document.getElementById("conv-src-fmt")?.value;
        let input;
        try {
          input = await resolveConverterInput(convSrc, convSrc && convSrc !== "pages" ? convSrc : dstExt);
        } catch (err) {
          input = { problem: err?.message || String(err) };
        }
        if (input.problem) {
          setStatusFeedback("문서 저장 실패 (" + dstExt.toUpperCase() + "): 원본 파일을 사용할 수 없습니다: " + input.problem);
          return false;
        }
        ({ srcFormat: srcExt, content } = input);
        title = input.title ?? title;
      } else {
        content = buildConversionContent(curMod);
      }
      const converted = await convertForSave(srcExt, dstExt, title, content);
      let failure = converted.failure;
      const deliver = spec.inPlace ? deliverInPlace : deliverSavedFile;
      const delivery = failure ? null : await deliver(converted.b64, title, dstExt);
      if (delivery?.cancelled) {
        setStatusFeedback("문서 저장 취소 (" + dstExt.toUpperCase() + "): 저장 위치를 선택하지 않았습니다");
        return false;
      }
      if (!failure && !delivery.ok) failure = delivery.error || "파일 다운로드를 시작하지 못했습니다";
      if (failure) {
        setStatusFeedback("문서 저장 실패 (" + dstExt.toUpperCase() + "): " + failure);
        return false;
      }
      const res = converted.res;
      setStatusFeedback(
        "문서 저장 완료 (" + delivery.name + " · " + (res.outputBytes || 0) + "B · " + describeConversionSpec(res) + ")" + describeApproximatedAlign(document, curMod)
      );
      noteSavedName(delivery.name);
      return true;
    }

    function saveActiveModuleDocument(options) {
      return enqueueWorkspaceHistoryOp(() => runSaveActiveModuleDocument(options?.closing, options?.saveAs === true));
    }

    let insertModeLabel = "삽입";

    function isWhitespaceCode(c) {
      if (c <= 0x20) return c === 0x20 || ((c - 9) >>> 0) <= 4;
      return (
        c === 0x00a0 ||
        c === 0x1680 ||
        (c >= 0x2000 && c <= 0x200a) ||
        c === 0x2028 ||
        c === 0x2029 ||
        c === 0x202f ||
        c === 0x205f ||
        c === 0x3000 ||
        c === 0xfeff
      );
    }

    function countTextMetrics(text) {
      const s = String(text || "");
      if (s.length === 0) return { chars: 0, words: 0, lineCount: 1 };
      let chars = 0;
      let words = 1;
      let lineCount = 1;
      let inSpace = false;
      let inNewline = false;
      for (let i = 0; i < s.length; ) {
        const cp = s.codePointAt(i);
        chars += 1;
        i += cp > 0xffff ? 2 : 1;
        const space = isWhitespaceCode(cp);
        if (space && !inSpace) words += 1;
        inSpace = space;
        const nl = cp === 10;
        if (nl && !inNewline) lineCount += 1;
        inNewline = nl;
      }
      return { chars, words, lineCount };
    }

    function updateStudioPosition(text, chars, estPages, precomputedLines) {
      const posEl = document.getElementById("status-studio-pos");
      if (!posEl) return;
      const editor = document.getElementById("doc-editor");
      const cols = Number.parseInt(editor?.style?.columnCount || "1", 10) || 1;
      const lineCount = precomputedLines ?? countTextMetrics(text).lineCount;
      const colChars = Math.max(1, chars % 60 || Math.min(60, chars));
      posEl.textContent =
        "1/" + estPages + " 쪽   " + cols + "단   " + lineCount + "줄   " + colChars + "칸 | 구역: 1 / 1 |";
    }

    function updateDocFilename(estPages) {
      const fileEl = document.getElementById("status-doc-filename");
      if (!fileEl) return;
      const activeTabTitle = getWindowBridge?.()?.getDocumentName(getActiveModule());
      fileEl.textContent = (activeTabTitle || "제목 없음") + " — " + estPages + "페이지";
    }

    function computeSmartStats() {
      const statsEl = document.getElementById("smart-stats");
      if (!statsEl) return;
      const text = extractEditableDocumentText();
      const { chars, words, lineCount } = countTextMetrics(text);
      const secs = Math.max(1, Math.ceil((words / 220) * 60));
      statsEl.textContent = "문자: " + chars + " · 단어: " + words + " · 읽기 시간: " + secs + "초";
      const estPages = Math.max(1, Math.ceil(chars / 1200));
      const zoneEl = document.getElementById("doc-page-zone-indicator");
      if (zoneEl) {
        zoneEl.textContent = "1 / " + estPages + " 쪽 · 구역: 1 / 1 · " + insertModeLabel;
      }
      updateStudioPosition(text, chars, estPages, lineCount);
      updateDocFilename(estPages);
    }

    function applySmartDiagnoseDom(res, updateFeedback) {
      if (!res) return;
      const statsEl = document.getElementById("smart-stats");
      const summaryEl = document.getElementById("smart-summary-output");
      const fb = document.getElementById("status-command-feedback");
      if (statsEl) {
        statsEl.textContent = `문자: ${res.charCount} · 단어: ${res.wordCount} · 문장: ${res.sentenceCount} · 읽기: ${res.readingTimeSeconds}초 · 요약: ${res.summaryCount}건`;
      }
      if (summaryEl) summaryEl.textContent = buildSmartSummaryText(res);
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
      const convSrc = document.getElementById("conv-src-fmt")?.value;
      const convExt = convSrc && convSrc !== "pages" ? convSrc : document.getElementById("conv-dst-fmt")?.value || "docx";
      const extMap = { DOC: "docx", HWP: "hwpx", SHEET: "xlsx", SLIDE: "pptx", PDF: "pdf", CONV: convExt };
      return extMap[effectiveMod] || "docx";
    }

    async function runModuleSecurityAudit(targetMod) {
      const effectiveMod = targetMod || getActiveModule();
      const ext = resolveModuleContainerExt(effectiveMod);
      let srcExt = ext;
      let content;
      if (effectiveMod === "CONV") {
        const input = await resolveConverterInput(document.getElementById("conv-src-fmt")?.value, ext);
        if (input.problem) throw new Error("원본 파일을 사용할 수 없습니다: " + input.problem);
        ({ srcFormat: srcExt, content } = input);
      } else {
        content = buildConversionContent(effectiveMod);
      }
      const title = "SecurityAudit_" + effectiveMod;
      const convRes = await invokeIpc(
        "convert_document_format",
        JSON.stringify({ sourceFormat: srcExt, targetFormat: ext, title, ...content })
      );
      const containerBase64 = String(convRes?.convertedBase64 || "");
      return await invokeIpc("audit_security", JSON.stringify({ fileName: title + "." + ext, containerBase64 }));
    }

    async function fetchSecurityAuditOutcome(targetMod) {
      try {
        await runModuleSecurityAudit(targetMod);
        return { ok: true, errorMessage: "" };
      } catch (err) {
        return { ok: false, errorMessage: err?.message || String(err), connection: Boolean(err?.connection) };
      }
    }

    function applySecurityAuditDom(outcome) {
      if (!outcome) return;
      const txt = document.getElementById("security-status-text");
      if (!txt) return;
      const curMod = getActiveModule();
      if (outcome.ok) {
        txt.textContent = "문서 보안 검사 완료 (" + curMod + "): SAFE";
      } else if (outcome.connection) {
        txt.textContent = "[요청 거부] " + outcome.errorMessage;
      } else {
        txt.textContent = "문서 보안 검사 완료 (" + curMod + "): REJECTED (" + outcome.errorMessage + ")";
      }
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = txt.textContent;
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
        const real = (Array.isArray(res.realPaths) ? res.realPaths : [])
          .map((r) => r.caseId + " " + r.elapsedUs + "µs/" + r.outputBytes + "B")
          .join(" · ");
        if (box) {
          const eqLabel = res.allEquivalent ? "결과 일치" : "불일치";
          box.textContent =
            "성능 진단 완료 (" + eqLabel + ") · " + summary + (real ? " · 실제 경로: " + real : "");
        }
        if (fb) {
          fb.textContent = "성능 진단 완료 · " + summary;
        }
      }
    }

    function withDocStyleUndo(mutateFn) {
      const curMod = getActiveModule();
      if (curMod !== "DOC" && curMod !== "HWP") return;
      const pre = typeof captureWorkspaceSnapshot === "function" ? captureWorkspaceSnapshot(false) : null;
      mutateFn();
      saveActiveDocTabState();
      if (pre && typeof commitDeferredUndoSnapshot === "function") commitDeferredUndoSnapshot(pre);
    }

    function resolveActiveDocBlock(editor) {
      return (typeof getFocusedDocBlock === "function" && getFocusedDocBlock(editor)) || editor?.querySelector("p") || null;
    }

    function toggleEditorDatasetBtn(btn, editor, key) {
      if (!btn) return;
      withDocStyleUndo(() => {
        const next = !btn.classList.contains("active");
        btn.classList.toggle("active", next);
        btn.setAttribute("aria-pressed", next ? "true" : "false");
        if (editor) editor.dataset[key] = next ? "true" : "false";
      });
    }

    // Cut/copy/paste act on the live editor selection and the system clipboard
    // (workspace-session.js); a missing selection never erases the whole paragraph.
    function execStudioClipboardCmd(cmd) {
      if (!cmd.startsWith("clip-")) return false;
      if (typeof runStudioClipboardCommand === "function") {
        void Promise.resolve(runStudioClipboardCommand(cmd, withDocStyleUndo)).catch(() => {});
      }
      return true;
    }

    const PARA_LIST_CMDS = { "para-numbering": { kind: "ordered", label: "문단 번호 서식 적용 완료" }, "para-bullet": { kind: "bullet", label: "글머리표 서식 적용 완료" } };
    const PARA_INDENT_CMDS = { "indent-in": { delta: 16, label: "한 수준 증가 적용 완료" }, "indent-out": { delta: -16, label: "한 수준 감소 적용 완료" } };
    function execStudioParaCmd(cmd, fb, isDocMod) {
      const list = PARA_LIST_CMDS[cmd];
      const ind = PARA_INDENT_CMDS[cmd];
      if (!list && !ind && cmd !== "para-shape") return false;
      if (!isDocMod) return true;
      if (list) {
        if (getDocRuns().toggleList(list.kind).changed) {
          computeSmartStats();
          if (fb) fb.textContent = list.label;
        }
      } else if (ind) {
        getDocRuns().shiftIndent(ind.delta);
        if (fb) fb.textContent = ind.label;
      } else {
        document.getElementById("btn-align-justify")?.click();
        if (fb) fb.textContent = "문단 모양 정렬 적용 완료";
      }
      return true;
    }

    function upsertDocHeaderBlock(editor) {
      const hdr = editor?.querySelector(".eo-hwp-header") || document.createElement("header");
      hdr.className = "eo-hwp-header";
      hdr.dataset.hwpHeader = "true";
      const p = document.createElement("p");
      p.textContent = "문서 머리말 · 꼬리말 (표준 양식)";
      hdr.replaceChildren(p);
      if (!hdr.parentElement && editor) editor.prepend(hdr);
      computeSmartStats();
    }

    function appendDocFootnoteBlock(editor) {
      const idx = (editor?.querySelectorAll(".eo-doc-footnote").length || 0) + 1;
      const aside = document.createElement("aside");
      aside.className = "eo-doc-footnote";
      aside.dataset.docFootnote = "true";
      aside.textContent = "각주 " + idx + ") 표준 행정 문서 관리 지침 및 근거 조항 참조";
      editor?.appendChild(aside);
      computeSmartStats();
    }

    function applyFallbackInsertMod(cmd, editor, textTarget) {
      if (cmd === "mod-symbol") {
        // A text node at the caret: the paragraph's inline runs keep their markup.
        if (!insertTextAtSelection(SYMBOL_TABLE_CHAR) && textTarget) {
          const kids = textTarget.childNodes || [];
          const last = kids[kids.length - 1];
          const container = last?.classList?.contains?.("eo-run") ? last : textTarget;
          container.appendChild(document.createTextNode(SYMBOL_TABLE_CHAR));
        }
        getDocRuns?.()?.refreshToolbarFromSelection?.();
        computeSmartStats();
      } else if (cmd === "mod-header") {
        upsertDocHeaderBlock(editor);
      } else if (cmd === "mod-footnote") {
        appendDocFootnoteBlock(editor);
      }
    }

    function insertSymbolInNonDocModule(curMod) {
      if (insertTextAtSelection(SYMBOL_TABLE_CHAR)) {
        if (curMod === "SHEET") sheetController.saveActiveSheetCellsToWorkbook();
        if (curMod === "SLIDE") syncActiveSlideDomStylesToDeck();
        return true;
      }
      if (curMod === "SHEET") {
        const td = document.querySelector("#pane-sheet td.selected-cell") || document.querySelector('#pane-sheet td[data-cell="A1"]');
        if (!td) return false;
        td.textContent = (td.textContent || "") + SYMBOL_TABLE_CHAR;
        sheetController.saveActiveSheetCellsToWorkbook();
        return true;
      }
      if (curMod === "SLIDE") {
        const zone = document.getElementById("slide-lower-zone") || document.getElementById("slide-title-zone");
        if (!zone) return false;
        const target = zone.querySelector("p") || zone;
        target.appendChild(document.createTextNode(SYMBOL_TABLE_CHAR));
        syncActiveSlideDomStylesToDeck();
        return true;
      }
      return false;
    }

    function execStudioInsertModCmd(cmd, editor, textTarget, fb, onStudioCmd, curMod, isDocMod) {
      const spec = INSERT_MOD_DISPATCH[cmd];
      if (!spec) return false;
      if (!isDocMod) {
        if (cmd === "mod-symbol" && insertSymbolInNonDocModule(curMod) && fb) fb.textContent = spec.label;
        return true;
      }
      if (spec.target && (curMod === "HWP") === spec.hwp && typeof onStudioCmd === "function") {
        onStudioCmd(spec.target);
        return true;
      }
      withDocStyleUndo(() => applyFallbackInsertMod(cmd, editor, textTarget));
      if (fb) fb.textContent = spec.label;
      return true;
    }

    let studioActions = {};

    function openToolDialog(dialogId) {
      const dialog = document.getElementById(dialogId);
      if (typeof dialog?.showModal === "function" && !dialog.open) dialog.showModal();
    }

    function dispatchStudioIconCmd(cmd, editor, onStudioCmd) {
      if (!cmd) return;
      // These two inspect the page's own fallback DOM: with the embedded editor active that DOM is hidden and a scan would report a false "clean".
      if (EMBED_BLIND_TOOLS.has(cmd) && getActiveModule() === "HWP" && getHwpHost?.()?.isEmbedActive()) return;
      if (TOOL_DIALOGS[cmd]) openToolDialog(TOOL_DIALOGS[cmd]);
      const direct = STUDIO_DIRECT_ACTIONS[cmd];
      if (direct) return void studioActions[direct]?.();
      const redirectId = STUDIO_QUICK_REDIRECT[cmd];
      if (redirectId) return void document.getElementById(redirectId)?.click();
      if (TOOL_DIALOGS[cmd]) return;
      const curMod = getActiveModule();
      const isDocMod = curMod === "DOC" || curMod === "HWP";
      const fb = document.getElementById("status-command-feedback");
      const blk = resolveActiveDocBlock(editor);
      const textTarget = blk?.querySelector?.(".eo-h-title") || blk;
      if (execStudioClipboardCmd(cmd)) return;
      if (execStudioParaCmd(cmd, fb, isDocMod)) return;
      if (execStudioInsertModCmd(cmd, editor, textTarget, fb, onStudioCmd, curMod, isDocMod)) return;
      if (typeof onStudioCmd === "function") onStudioCmd(cmd);
    }

    const applyStudioLineSpacing = (pct) => getDocRuns().setLineSpacing(pct);

    // Font, size, color, highlight and strikethrough act on the selection only (run model).
    function bindCharacterRunControls() {
      const runs = () => getDocRuns();
      const sizeInput = document.getElementById("doc-font-size-input");
      document.getElementById("doc-font-select")?.addEventListener("change", (ev) => runs().setFont(ev.target.value));
      sizeInput?.addEventListener("change", () => runs().setSize(Number.parseFloat(sizeInput.value)));
      sizeInput?.addEventListener("keydown", (ev) => {
        if (ev.key !== "Enter") return;
        ev.preventDefault?.();
        runs().setSize(Number.parseFloat(sizeInput.value));
      });
      document.getElementById("btn-size-up")?.addEventListener("click", () => runs().bumpSize(1));
      document.getElementById("btn-size-down")?.addEventListener("click", () => runs().bumpSize(-1));
      document.getElementById("btn-stylebar-strike")?.addEventListener("click", () => runs().toggle("strike"));
      const colorPicker = host.__EASYOFFICE_COLOR_PICKER__;
      [["btn-stylebar-textcolor", "doc-textcolor-host", "color", { remember: true }], ["btn-stylebar-highlight", "doc-highlight-host", "highlight", { clear: true }]].forEach(([btnId, hostId, prop, options]) => {
        const btn = document.getElementById(btnId);
        btn?.addEventListener("click", () => runs().toggleColor(prop));
        const hostEl = document.getElementById(hostId);
        if (!hostEl) return;
        colorPicker.setupColorPicker(hostEl, (color) => {
          if (btn) btn.style.borderBottomColor = color || "";
          runs().setColor(prop, color);
        }, options);
      });
    }

    function bindLineSpacingDoubleClick(editor, lsSelect) {
      if (!lsSelect) return;
      lsSelect.addEventListener("dblclick", (ev) => {
        ev.preventDefault?.();
        const hostEl = lsSelect.parentElement?.classList?.contains?.("custom-select") ? lsSelect.parentElement : lsSelect;
        if (!hostEl?.parentElement) return;
        const input = document.createElement("input");
        input.type = "text";
        input.className = "sb-select sb-ls-select";
        input.setAttribute("aria-label", "줄 간격 직접 입력");
        input.value = lsSelect.value || "160";
        hostEl.hidden = true;
        hostEl.parentElement.insertBefore(input, hostEl);
        input.focus?.();
        input.select?.();
        let done = false;
        const finish = (apply) => {
          if (done) return;
          done = true;
          const num = Number.parseInt(input.value, 10);
          if (apply && Number.isFinite(num) && num > 0) applyStudioLineSpacing(num);
          input.remove?.();
          hostEl.hidden = false;
        };
        input.addEventListener("keydown", (ke) => {
          if (ke.key === "Enter") { ke.preventDefault?.(); finish(true); }
          else if (ke.key === "Escape") { ke.preventDefault?.(); finish(false); }
        });
        input.addEventListener("blur", () => finish(true));
      });
    }

    function bindStudioTypographyControls(editor) {
      document.getElementById("doc-style-select")?.addEventListener("change", (ev) => getDocRuns().setStyle(ev.target.value));
      bindCharacterRunControls();
      const lsSelect = document.getElementById("doc-linespacing-select");
      lsSelect?.addEventListener("change", () => applyStudioLineSpacing(Number.parseInt(lsSelect.value, 10)));
      bindLineSpacingDoubleClick(editor, lsSelect);
      document.getElementById("btn-ls-up")?.addEventListener("click", () => {
        const cur = Number.parseInt(lsSelect?.value || "160", 10) || 160;
        applyStudioLineSpacing(LS_STEPS.find((s) => s > cur) || 300);
      });
      document.getElementById("btn-ls-down")?.addEventListener("click", () => {
        const cur = Number.parseInt(lsSelect?.value || "160", 10) || 160;
        applyStudioLineSpacing(LS_STEPS_REV.find((s) => s < cur) || 100);
      });
    }

    function toggleDocManuscriptGrid(docGridBtn, editor) {
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
      });
    }

    const menuKeyboard = host.__EASYOFFICE_CREATE_MENU_KEYBOARD__({ document });

    function bindStudioMenuBar(editor, onStudioCmd) {
      menuKeyboard.bind();
      for (const item of document.querySelectorAll(".md-item[data-studio-menu-cmd]")) {
        item.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          menuKeyboard.closeAll();
          if (item.getAttribute("aria-disabled") === "true") return;
          dispatchStudioIconCmd(item.dataset.studioMenuCmd, editor, onStudioCmd);
        });
      }
      // rhwp-studio: with a menu open, a single unmodified key runs the item whose shortcut label is that key.
      host.addEventListener?.("keydown", (ev) => {
        if (ev.ctrlKey || ev.metaKey || ev.altKey || ev.shiftKey || !ev.key || ev.key.length !== 1) return;
        const open = document.querySelector(".menu-item.open");
        if (!open) return;
        const key = ev.key.toUpperCase();
        const hit = Array.from(open.querySelectorAll(".md-item[data-studio-menu-cmd]")).find(
          (item) => item.getAttribute("aria-disabled") !== "true" && item.querySelector(".md-shortcut")?.textContent.toUpperCase() === key
        );
        if (!hit) return;
        ev.preventDefault?.();
        hit.click();
      });
    }

    function toggleStudioInsertMode(insertBtn) {
      insertModeLabel = insertModeLabel === "삽입" ? "수정" : "삽입";
      if (insertBtn) insertBtn.textContent = insertModeLabel;
      computeSmartStats();
    }

    function computeFitZoom(editor, mode, fallback) {
      const vp = [".doc-canvas-scroll", ".doc-scroll-container", "#sheet-grid", ".slide-canvas-stage", "#slide-stage-box", "#pane-pdf", ".canvas-stage"]
        .map((sel) => document.querySelector(sel))
        .find((el) => el && !el.hidden && !el.closest?.("[hidden]") && Number(el.clientWidth) > 0);
      const cw = Number(vp?.clientWidth) || 0;
      const ch = Number(vp?.clientHeight) || 0;
      if (cw <= 0 || (mode === "page" && ch <= 0)) return fallback;
      const pw = Number(editor?.offsetWidth) || 794;
      const ph = Number(editor?.offsetHeight) || 1123;
      const wPct = ((cw - 40) / pw) * 100;
      const raw = mode === "page" ? Math.min(wPct, ((ch - 20) / ph) * 100) : wPct;
      return Math.max(50, Math.min(200, Math.round(raw)));
    }

    function setupStudioChromeControls(applyZoomPercent, onStudioCmd, actions) {
      studioActions = actions || {};
      const editor = document.getElementById("doc-editor");
      for (const closer of document.querySelectorAll("[data-dialog-close]")) closer.addEventListener("click", () => closer.closest("dialog")?.close?.());
      bindStudioMenuBar(editor, onStudioCmd);
      for (const btn of document.querySelectorAll("#icon-toolbar [data-studio-cmd]")) btn.addEventListener("click", () => { if (!btn.disabled) dispatchStudioIconCmd(btn.dataset.studioCmd, editor, onStudioCmd); });
      bindStudioTypographyControls(editor);
      for (const [id, key] of [["btn-toggle-ctrl-marks", "showCtrlMarks"], ["btn-toggle-para-marks", "showParaMarks"]]) {
        const btn = document.getElementById(id);
        btn?.addEventListener("click", () => toggleEditorDatasetBtn(btn, editor, key));
      }
      for (const mode of ["left", "center", "right", "justify", "distribute", "divide"]) {
        document.getElementById("btn-align-" + mode)?.addEventListener("click", () => getDocRuns().setAlign(mode));
      }
      const docGridBtn = document.getElementById("btn-toggle-doc-grid");
      if (docGridBtn && editor) docGridBtn.addEventListener("click", () => toggleDocManuscriptGrid(docGridBtn, editor));
      const insertBtn = document.getElementById("status-insert-mode");
      insertBtn?.addEventListener("click", () => toggleStudioInsertMode(insertBtn));
      host.addEventListener("keydown", (ev) => { if (ev.key === "Insert") toggleStudioInsertMode(insertBtn); });
      document.getElementById("status-studio-pos")?.addEventListener("click", () => {
        document.getElementById("btn-doc-outline")?.click?.();
        openToolDialog("outline-dialog");
      });
      const applyZoom = (pct, mode) => { applyZoomPercent?.(pct, mode); getDocRuns()?.syncRuler?.((Number(pct) || 100) / 100); };
      document.getElementById("status-zoom-label")?.addEventListener("click", () => applyZoom(100));
      const zoomSlider = document.getElementById("status-zoom-slider");
      if (zoomSlider && typeof applyZoomPercent === "function") {
        const onSlide = () => applyZoom(Number(zoomSlider.value) || 100);
        for (const evt of ["input", "change"]) zoomSlider.addEventListener(evt, onSlide);
        zoomSlider.addEventListener("keydown", (ev) => {
          const cur = Number(zoomSlider.value) || 100;
          const next = { PageUp: cur + 10, PageDown: cur - 10, Home: 50, End: 200, ArrowUp: cur + 5, ArrowRight: cur + 5, ArrowDown: cur - 5, ArrowLeft: cur - 5 }[ev.key];
          if (next === undefined) return;
          ev.preventDefault?.();
          applyZoom(Math.max(50, Math.min(200, next)));
        });
      }
      for (const [id, pct, mode] of [["btn-zoom-fit-width", 120, "width"], ["btn-zoom-fit-page", 100, "page"]]) {
        document.getElementById(id)?.addEventListener("click", () => applyZoom(computeFitZoom(editor, mode, pct), mode));
      }
    }

    const { runPiiScrubAction } = host.__EASYOFFICE_CREATE_WORKSPACE_SCRUB__(deps, panelHooks, { resolveSourceScopeKind, computeSmartStats, describePdfHighlight });

    return {
      runPiiScrubAction, resolveSourceScopeKind, buildConversionContent, resolveConverterInput, resolveSaveFileBase, triggerBase64Download, deliverSavedFile,
      describeDeliveryFailure, describeConversionSpec, describePdfHighlight, openToolDialog, saveActiveModuleDocument,
      computeSmartStats, applySmartDiagnoseDom, runSmartDiagnoseAction, runModuleSecurityAudit,
      fetchSecurityAuditOutcome, applySecurityAuditDom, runSecurityAuditAction, runBenchmarkAction,
      setupStudioChromeControls, runStudioCommand: (cmd) => dispatchStudioIconCmd(cmd, document.getElementById("doc-editor"), null),
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_TOOLS__ = createWorkspaceTools;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
