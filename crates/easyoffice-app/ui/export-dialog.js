(function (host) {
  "use strict";

  // Safe explicit export & format selector dialog (PD-355).
  // Preserves PD-162 / PD-336 overwrite protection: exports always request a new target
  // (`inPlace: false, exportOnly: true`) and never replace the opened `.pages` package or editable source.

  const EXPORT_SPECS = Object.freeze({
    DOC: [
      { ext: "docx", label: "DOCX 문서 (.docx)" },
      { ext: "odt", label: "ODF 텍스트 (.odt)" },
      { ext: "rtf", label: "RTF 문서 (.rtf)" },
      { ext: "pages", label: "Pages 문서 (.pages)" },
      { ext: "md", label: "Markdown (.md)" },
      { ext: "txt", label: "TXT 텍스트 (.txt)" },
      { ext: "pdf", label: "PDF 문서 (.pdf)" },
    ],
    SHEET: [
      { ext: "xlsx", label: "XLSX 통합 문서 (.xlsx)" },
      { ext: "ods", label: "ODF 스프레드시트 (.ods)" },
      { ext: "csv", label: "CSV (쉼표로 구분) (.csv)" },
      { ext: "tsv", label: "TSV (탭으로 구분) (.tsv)" },
      { ext: "pdf", label: "PDF 문서 (.pdf)" },
    ],
    SLIDE: [
      { ext: "pptx", label: "PPTX 프레젠테이션 (.pptx)" },
      { ext: "odp", label: "ODF 프레젠테이션 (.odp)" },
      { ext: "pdf", label: "PDF 문서 (.pdf)" },
    ],
    BASE: [
      { ext: "eobase", label: "데이터베이스 패키지 (.eobase)" },
      { ext: "xlsx", label: "XLSX 통합 문서 (.xlsx)" },
      { ext: "csv", label: "CSV (쉼표로 구분) (.csv)" },
      { ext: "tsv", label: "TSV (탭으로 구분) (.tsv)" },
    ],
    BOARD: [
      { ext: "eoboard", label: "보드 패키지 (.eoboard)" },
      { ext: "pdf", label: "PDF 문서 (.pdf)" },
    ],
    PDF: [
      { ext: "pdf", label: "PDF 문서 (.pdf)" },
      { ext: "docx", label: "DOCX 문서 (.docx)" },
      { ext: "txt", label: "TXT 텍스트 (.txt)" },
    ],
    HWP: [
      { ext: "hwpx", label: "한글 표준 문서 (.hwpx)" },
      { ext: "hwp", label: "한글 문서 (.hwp)" },
      { ext: "pdf", label: "PDF 문서 (.pdf)" },
    ],
  });

  const KNOWN_EXT_RE = /\.(?:docx|doc|odt|rtf|pages|md|markdown|txt|xlsx|xls|ods|csv|tsv|pptx|ppsx|ppt|odp|pdf|eobase|eoboard|hwpx|hwp)$/i;
  const WINDOWS_RESERVED_RE = /^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/i;

  function formatsForModule(mod) {
    return EXPORT_SPECS[mod] || EXPORT_SPECS.DOC;
  }

  function sanitizeBaseName(raw, fallback) {
    const safeFallback = String(fallback || "EasyOffice_문서").trim() || "EasyOffice_문서";
    let base = String(raw || "")
      .replaceAll(/[\u0000-\u001f\u007f-\u009f]/g, "")
      .split(/[/\\]/)
      .pop()
      .trim();
    base = base.replace(KNOWN_EXT_RE, "").trim();
    const shared = host.__EASYOFFICE_SAVE_PAYLOAD__?.sanitizeFileBaseName;
    if (typeof shared === "function") {
      if (!base || base === "." || base === "..") return safeFallback;
      return shared(base, safeFallback);
    }
    base = base
      .replaceAll(/[<>:"/\\|?*]/g, "_")
      .replaceAll(/\s+/g, " ")
      .replace(/^[\s.]+/, "")
      .replace(/(?<![\s.])[\s.]+$/, "");
    if (Array.from(base).length > 120) {
      base = Array.from(base).slice(0, 120).join("").replace(/(?<![\s.])[\s.]+$/, "");
    }
    if (!base || base === "." || base === "..") return safeFallback;
    if (WINDOWS_RESERVED_RE.test(base.split(".")[0] || "")) base = "_" + base;
    return base || safeFallback;
  }

  function createExportDialogController(runtime) {
    const {
      document,
      getActiveModule,
      getPdfEdit,
      getHwpHost,
      getHwpSession,
      enqueueWorkspaceHistoryOp,
      resolveSaveFileBase,
      buildConversionContent,
      convertForSave,
      deliverSavedFile,
      describeConversionSpec,
      describeApproximatedAlign,
      setStatusFeedback,
      commitPendingSheetCell,
      closeMenus,
    } = runtime;

    function sourceFormatFor(mod) {
      if (mod === "SHEET" || mod === "BASE") return "xlsx";
      if (mod === "SLIDE" || mod === "BOARD") return "pptx";
      if (mod === "PDF") return "pdf";
      return "docx";
    }

    function hasLoadedPdfContent() {
      const pane = document.getElementById("pane-pdf");
      if (pane?.dataset?.empty === "true") return false;
      const card = document.getElementById("pdf-page-card");
      if (!card || card.hidden) return false;
      return Array.from(card.querySelectorAll("h3, p:not(#pdf-form-status)")).some(
        (el) => Boolean((el.textContent || "").trim())
      );
    }

    async function runExportFormat(targetExt, customBaseName) {
      await commitPendingSheetCell?.();
      const curMod = getActiveModule();
      if (curMod === "START" || curMod === "CONV") {
        const msg = "현재 창에서는 내보내기를 지원하지 않습니다.";
        setStatusFeedback(msg);
        return { ok: false, error: msg };
      }
      const ext = String(targetExt || "").trim().toLowerCase();
      const allowed = formatsForModule(curMod);
      if (!allowed.some((item) => item.ext === ext)) {
        const msg = "현재 창(" + curMod + ")에서는 ." + ext + " 내보내기를 지원하지 않습니다.";
        setStatusFeedback(msg);
        return { ok: false, error: msg };
      }

      if (curMod === "PDF" && !hasLoadedPdfContent()) {
        const msg = "내보낼 PDF 문서가 없습니다.";
        setStatusFeedback(msg);
        return { ok: false, error: msg };
      }

      if (curMod === "BOARD" && ext === "pdf") {
        const nodes = host.__EASYOFFICE_BASE_BOARD_HOST__?.getBoardStudio?.()?.getState?.()?.nodes;
        if (Array.isArray(nodes) && nodes.length === 0) {
          const msg = "PDF로 내보낼 보드 요소가 없습니다.";
          setStatusFeedback(msg);
          return { ok: false, error: msg };
        }
      }

      if (curMod === "HWP" && getHwpHost?.()?.isEmbedActive() && getHwpSession?.()) {
        if (ext === "pdf") {
          const ok = await getHwpSession().exportPdf();
          return { ok: Boolean(ok), cancelled: !ok };
        }
        const ok = await getHwpSession().saveAs(ext);
        return { ok: Boolean(ok), cancelled: !ok };
      }

      const safeBase = sanitizeBaseName(customBaseName, resolveSaveFileBase(curMod));

      if (curMod === "PDF" && ext === "pdf" && getPdfEdit?.()?.isActive()) {
        try {
          const suggested =
            typeof customBaseName === "string" && customBaseName.trim() ? safeBase + ".pdf" : undefined;
          const res = await getPdfEdit().save(suggested);
          if (res.cancelled) {
            setStatusFeedback("내보내기 취소 (PDF): 저장 위치를 선택하지 않았습니다");
            return { ok: false, cancelled: true };
          }
          setStatusFeedback(
            "내보내기 완료 (" + res.name + " · " + res.bytes + "B · " + res.pages + "쪽)" + res.notes
          );
          return { ok: true, name: res.name };
        } catch (err) {
          const msg = err?.message || String(err);
          setStatusFeedback("내보내기 실패 (PDF): " + msg);
          return { ok: false, error: msg };
        }
      }

      if ((curMod === "BASE" && ext === "eobase") || (curMod === "BOARD" && ext === "eoboard")) {
        const deliverAsCopy = (b64, base, outExt) => deliverSavedFile(b64, base, outExt, { inPlace: false, exportOnly: true });
        const saved = await host.__EASYOFFICE_BASE_BOARD_HOST__.savePackage(curMod, deliverAsCopy, safeBase);
        if (saved.cancelled) {
          setStatusFeedback("내보내기 취소 (" + ext.toUpperCase() + "): 저장 위치를 선택하지 않았습니다");
          return { ok: false, cancelled: true };
        }
        if (!saved.ok) {
          const msg = saved.failure || saved.error || "파일 다운로드를 시작하지 못했습니다";
          setStatusFeedback("내보내기 실패 (" + ext.toUpperCase() + "): " + msg);
          return { ok: false, error: msg };
        }
        setStatusFeedback("내보내기 완료 (" + saved.name + " · " + saved.bytes + "B)");
        return { ok: true, name: saved.name };
      }

      let content;
      if (curMod === "BASE" || curMod === "BOARD") {
        content = host.__EASYOFFICE_BASE_BOARD_HOST__.buildConversionContent(curMod, safeBase);
      } else {
        content = buildConversionContent(curMod);
      }

      const converted = await convertForSave(sourceFormatFor(curMod), ext, safeBase, content);
      if (converted.cancelled) {
        setStatusFeedback("내보내기 취소 (" + ext.toUpperCase() + "): 암호 없이 저장하지 않았습니다");
        return { ok: false, cancelled: true };
      }
      if (converted.failure) {
        setStatusFeedback("내보내기 실패 (" + ext.toUpperCase() + "): " + converted.failure);
        return { ok: false, error: converted.failure };
      }

      const delivery = await deliverSavedFile(converted.b64, safeBase, ext, { inPlace: false, exportOnly: true });
      if (delivery?.cancelled) {
        setStatusFeedback("내보내기 취소 (" + ext.toUpperCase() + "): 저장 위치를 선택하지 않았습니다");
        return { ok: false, cancelled: true };
      }
      if (!delivery?.ok) {
        const msg = delivery?.error || "파일 다운로드를 시작하지 못했습니다";
        setStatusFeedback("내보내기 실패 (" + ext.toUpperCase() + "): " + msg);
        return { ok: false, error: msg };
      }
      const res = converted.res || {};
      const detail = describeConversionSpec(res);
      setStatusFeedback(
        "내보내기 완료 (" +
          delivery.name +
          " · " +
          (res.outputBytes || 0) +
          "B" +
          (detail ? " · " + detail : "") +
          ")" +
          describeApproximatedAlign(document, curMod)
      );
      return { ok: true, name: delivery.name };
    }

    function exportAsFormat(targetExt, customBaseName) {
      return enqueueWorkspaceHistoryOp(() => runExportFormat(targetExt, customBaseName));
    }

    function syncDialogFields() {
      const mod = getActiveModule();
      const specs = formatsForModule(mod);
      const select = document.getElementById("export-format-select");
      const nameInput = document.getElementById("export-file-name");
      const status = document.getElementById("export-dialog-status");
      if (select) {
        select.replaceChildren();
        for (const item of specs) {
          const opt = document.createElement("option");
          opt.value = item.ext;
          opt.textContent = item.label;
          select.appendChild(opt);
        }
        select.value = specs[0]?.ext || "docx";
        host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(select);
      }
      if (nameInput) nameInput.value = resolveSaveFileBase(mod);
      if (status) status.textContent = "";
    }

    function openDialog() {
      const dialog = document.getElementById("export-dialog");
      if (!dialog) return false;
      syncDialogFields();
      if (typeof dialog.showModal === "function" && !dialog.open) dialog.showModal();
      document.getElementById("export-format-select")?.focus?.();
      return true;
    }

    function bind() {
      const dialog = document.getElementById("export-dialog");
      const select = document.getElementById("export-format-select");
      const status = document.getElementById("export-dialog-status");
      const nameInput = document.getElementById("export-file-name");
      const runBtn = document.getElementById("btn-export-run");

      select?.addEventListener("change", () => {
        if (status) status.textContent = "";
      });

      runBtn?.addEventListener("click", () => {
        const ext = select?.value || "";
        const baseName = nameInput?.value || "";
        runBtn.disabled = true;
        void exportAsFormat(ext, baseName)
          .then((outcome) => {
            if (outcome?.ok) dialog?.close?.();
            else if (status && outcome?.error) status.textContent = outcome.error;
          })
          .finally(() => {
            runBtn.disabled = false;
          });
      });

      for (const item of document.querySelectorAll(".md-item[data-export-format]")) {
        item.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          closeMenus?.();
          if (item.getAttribute("aria-disabled") === "true") return;
          void exportAsFormat(item.dataset.exportFormat).catch(() => {});
        });
      }

      for (const item of document.querySelectorAll(".md-item[data-export-dialog]")) {
        item.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          closeMenus?.();
          if (item.getAttribute("aria-disabled") === "true") return;
          openDialog();
        });
      }
    }

    return { bind, openDialog, exportAsFormat };
  }

  host.__EASYOFFICE_EXPORT_UI__ = {
    EXPORT_SPECS,
    formatsForModule,
    sanitizeBaseName,
    createController: createExportDialogController,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
