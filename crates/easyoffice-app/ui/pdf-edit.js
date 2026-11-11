(function (host) {
  "use strict";

  // PDF studio edits on the opened file's objects (PD-164). In the desktop shell the PDF that a
  // window opens is also handed to the shell once (`pdf_edit_open`); the existing controls then
  // work on that file instead of on its text: the rotation control turns the pages, the stamp and
  // watermark controls lay text over them, 페이지 분할·병합 merges or splits, and saving writes the
  // edited PDF with the original's pictures, fonts and layout. Without the shell, or for a file the
  // shell refuses (encrypted, over a limit), nothing changes: the text model keeps working as it did.
  // The page never sees a path; it sends bytes once and gets names, counts and sentences back.

  const STAMP_BADGE_ID = "pdf-stamp-badge";
  const WATERMARK_LAYER_ID = "pdf-watermark-layer";
  const BASE64_CHUNK = 0x8000;

  function bytesToBase64(bytes) {
    const shared = host.__EASYOFFICE_SHEET_UTILS__?.bytesToBase64;
    if (typeof shared === "function") return shared(bytes, (bin) => host.btoa(bin));
    const chunks = [];
    for (let i = 0; i < bytes.length; i += BASE64_CHUNK) {
      chunks.push(String.fromCharCode.apply(null, bytes.subarray(i, i + BASE64_CHUNK)));
    }
    return host.btoa(chunks.join(""));
  }

  function createPdfEdit(deps) {
    const { document, desktopBridge, getPdfRotationDeg } = deps;
    let active = false;

    const shell = () => (desktopBridge?.isAvailable?.() ? desktopBridge.pdfEdit : null);
    const reason = (err) => err?.message || String(err);

    function isActive() {
      return active;
    }

    // Called after a PDF was decoded into the studio. Returns the text to add to the status line:
    // empty outside the shell, otherwise whether the original's objects are being edited.
    async function open(fileName, bytes) {
      active = false;
      const bridge = shell();
      if (!bridge) return "";
      try {
        const state = await bridge.open(fileName, bytesToBase64(bytes));
        active = true;
        return " · 원본 PDF 편집 사용 (" + state.pages + "쪽 · 그림·글꼴·배치 보존)";
      } catch (err) {
        return " · 원본 PDF 편집 불가: " + reason(err) + " (읽어 온 텍스트만 편집합니다)";
      }
    }

    function textOf(id) {
      return (document.getElementById(id)?.textContent || "").trim();
    }

    // What the page shows now, as the shell needs it: one rotation for the file, the stamp and the
    // watermark as applied (the badge and the layer, which the apply button fills). Sent before a
    // save, so undo and redo of those controls reach the file too.
    async function syncFromUi() {
      const bridge = shell();
      if (!active || !bridge) return null;
      const turn = ((Math.round(Number(getPdfRotationDeg()) || 0) % 360) + 360) % 360;
      await bridge.apply({ kind: "turn", degrees: turn });
      return bridge.apply({ kind: "overlay", stamp: textOf(STAMP_BADGE_ID), watermark: textOf(WATERMARK_LAYER_ID) });
    }

    // Status text for the apply button; empty when the edit is not on the file.
    function noteApplied() {
      return active ? "스탬프·워터마크는 저장할 때 원본 PDF 위에 겹쳐 씁니다" : "";
    }

    // Saves the edited PDF (the shell asks where). Resolves { cancelled } or { name, bytes, pages }.
    async function save() {
      await syncFromUi();
      const res = await shell().save();
      return res.status === "cancelled" ? { cancelled: true } : { cancelled: false, name: String(res.name || ""), bytes: res.bytes, pages: res.pages };
    }

    // ---- 페이지 분할·병합 dialog ---------------------------------------------------------------

    const MODE_HINTS = {
      merge: "고른 PDF의 쪽을 현재 쪽 뒤에 붙입니다. 실행 취소로 되돌릴 수 없습니다.",
      range: "쪽 범위마다 파일 하나로 저장합니다. 예: 1-3, 5, 7-",
      each: "쪽마다 파일 하나로 저장합니다.",
    };
    const RANGE_MISSING = "쪽 범위를 입력해 주세요. 예: 1-3, 5, 7-";
    let pendingChoice = null;
    let dialogBound = false;

    const field = (id) => document.getElementById(id);

    function finishChoice(choice) {
      const resolve = pendingChoice;
      pendingChoice = null;
      if (resolve) resolve(choice);
    }

    function syncMode() {
      const mode = field("pdf-splitmerge-mode").value;
      const range = field("pdf-splitmerge-range");
      range.disabled = mode !== "range";
      field("pdf-splitmerge-hint").textContent = MODE_HINTS[mode] || "";
    }

    function confirmChoice() {
      const mode = field("pdf-splitmerge-mode").value;
      const ranges = field("pdf-splitmerge-range").value.trim();
      if (mode === "range" && !ranges) {
        field("pdf-splitmerge-hint").textContent = RANGE_MISSING;
        return;
      }
      finishChoice({ mode, ranges: mode === "range" ? ranges : "" });
      field("pdf-splitmerge-dialog").close?.();
    }

    function bindDialog() {
      if (dialogBound) return;
      dialogBound = true;
      const dialog = field("pdf-splitmerge-dialog");
      field("pdf-splitmerge-mode").addEventListener("change", syncMode);
      field("btn-pdf-splitmerge-run").addEventListener("click", confirmChoice);
      field("pdf-splitmerge-range").addEventListener("keydown", (ev) => {
        if (ev.key !== "Enter") return;
        ev.preventDefault?.();
        confirmChoice();
      });
      // Close button, 취소, Esc and a click on the backdrop all end the dialog without a choice.
      dialog.addEventListener("close", () => finishChoice(null));
      for (const closer of dialog.querySelectorAll("[data-dialog-close]")) closer.addEventListener("click", () => finishChoice(null));
    }

    // Shows the dialog and resolves what was chosen ({ mode, ranges }) or null.
    function chooseSplitMerge() {
      const dialog = field("pdf-splitmerge-dialog");
      if (!dialog || typeof dialog.showModal !== "function") return Promise.resolve(null);
      bindDialog();
      finishChoice(null);
      const mode = field("pdf-splitmerge-mode");
      mode.value = "merge";
      host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(mode);
      field("pdf-splitmerge-range").value = "";
      syncMode();
      return new Promise((resolve) => {
        pendingChoice = resolve;
        dialog.showModal();
      });
    }

    // 페이지 분할·병합: the dialog asks what to do, the shell asks which files or which folder and does
    // it. Resolves the sentence for the status line and the badge.
    async function splitMerge() {
      const choice = await chooseSplitMerge();
      if (!choice) return "페이지 분할·병합을 취소했습니다";
      try {
        const res = await shell().splitMerge(choice);
        if (res.action === "merged") return "PDF " + res.count + "개를 이어 붙였습니다 (총 " + res.pages + "쪽 · 실행 취소로 되돌릴 수 없습니다)";
        if (res.action === "split") {
          return (choice.mode === "range" ? "쪽 범위 " + res.count + "개를 파일로 나누어 저장했습니다" : "쪽마다 " + res.count + "개 파일로 나누어 저장했습니다");
        }
        return "페이지 분할·병합을 취소했습니다";
      } catch (err) {
        return "페이지 분할·병합 실패: " + reason(err);
      }
    }

    return Object.freeze({ isActive, open, syncFromUi, noteApplied, save, splitMerge });
  }

  host.__EASYOFFICE_CREATE_PDF_EDIT__ = createPdfEdit;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
