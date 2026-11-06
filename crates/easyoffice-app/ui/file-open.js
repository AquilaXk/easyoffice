(function (host) {
  "use strict";

  // Opens a user's own file (PD-075): file picker (Ctrl+O), drag and drop, and the single entry
  // point `openDocumentFromBytes(fileName, bytes)` that a desktop shell can call as well. The
  // backend decodes the real bytes into the structured save shape (`blocks` / `sheets` / `slides`);
  // this module loads that model into the matching workspace module as one undoable state.
  // Also lets the converter panel convert a chosen source file instead of the editor text.

  const MAX_FILE_BYTES = 10 * 1024 * 1024; // base64 inflates 4/3; the HTTP IPC body limit is 16 MiB
  const MAX_FILE_LABEL = "10 MiB";
  const GRID_LIMITS = { maxRows: 1000, maxCols: 8 }; // the sheet editor grid (A-H)
  const MODULE_LABELS = { DOC: "문서", HWP: "한글 문서", SHEET: "스프레드시트", SLIDE: "프레젠테이션", PDF: "PDF 스튜디오" };
  const SUPPORTED_EXTENSIONS = [
    "docx", "doc", "odt", "rtf", "md", "markdown", "txt",
    "hwp", "hwpx", "hwt", "hwtx",
    "xlsx", "xls", "ods", "csv", "tsv",
    "pptx", "ppt", "ppsx", "odp",
    "pdf",
  ];
  const BASE64_CHUNK = 0x8000;

  function baseName(raw) {
    return String(raw || "").split(/[\\/]/).pop().trim();
  }

  function extensionOf(name) {
    const m = /\.([A-Za-z0-9]+)$/.exec(name);
    return m ? m[1].toLowerCase() : "";
  }

  function bytesToBase64(bytes, btoaFn) {
    let bin = "";
    for (let i = 0; i < bytes.length; i += BASE64_CHUNK) {
      bin += String.fromCharCode.apply(null, bytes.subarray(i, i + BASE64_CHUNK));
    }
    return btoaFn(bin);
  }

  function validateFile(name, size) {
    if (!name) return "파일 이름을 확인할 수 없습니다";
    if (size === 0) return "빈 파일은 열 수 없습니다";
    if (size > MAX_FILE_BYTES) {
      return "파일이 너무 큽니다 (" + (size / 1048576).toFixed(1) + " MiB). 최대 " + MAX_FILE_LABEL + "까지 열 수 있습니다";
    }
    return "";
  }

  // ---- model -> editor state -------------------------------------------------------------

  const textSpec = (v) => ({ t: "text", v: String(v) });
  const elSpec = (tag, children, style, attrs) => ({
    t: "el", tag, attrs: attrs || {}, data: {}, style: style || {}, children: children || [],
  });

  function runSpecs(runs) {
    const out = [];
    (runs || []).forEach((run) => {
      const lines = String(run.text ?? "").split("\n");
      lines.forEach((line, idx) => {
        if (idx > 0) out.push(elSpec("br"));
        if (!line) return;
        let spec = textSpec(line);
        const runs = host.__EASYOFFICE_DOC_RUNS__;
        const fmt = runs ? runs.fromModelRun(run) : {};
        if (fmt.size || fmt.font || fmt.color || fmt.highlight) {
          out.push(runs.buildRunSpec(fmt, [spec]));
          return;
        }
        [["strike", "s"], ["underline", "u"], ["italic", "em"], ["bold", "strong"]].forEach(([flag, tag]) => {
          if (run[flag] === true) spec = elSpec(tag, [spec]);
        });
        out.push(spec);
      });
    });
    return out;
  }

  function buildEditorNodes(blocks) {
    const nodes = [];
    let openList = null;
    (blocks || []).forEach((block) => {
      if (block.type === "table") {
        openList = null;
        const rows = (block.rows || []).map((row) =>
          elSpec("tr", row.map((cell) => elSpec("td", [textSpec(typeof cell === "string" ? cell : cell?.text ?? "")])))
        );
        nodes.push(elSpec("table", [elSpec("tbody", rows)], {}, { class: "doc-inline-table" }));
        return;
      }
      const style = block.align ? { textAlign: block.align } : {};
      const inline = runSpecs(block.runs);
      const heading = Number(block.heading) || 0;
      if (heading >= 1 && heading <= 6) {
        openList = null;
        nodes.push(elSpec("h" + heading, inline, style));
      } else if (block.list === "bullet" || block.list === "ordered") {
        const tag = block.list === "ordered" ? "ol" : "ul";
        if (!openList || openList.tag !== tag) {
          openList = elSpec(tag);
          nodes.push(openList);
        }
        openList.children.push(elSpec("li", inline, style));
      } else {
        openList = null;
        nodes.push(elSpec("p", inline, style));
      }
    });
    return nodes;
  }

  function buildSheetStore(sheets) {
    const store = {};
    const rowCounts = {};
    (sheets || []).forEach((sheet) => {
      const map = {};
      let maxRow = 0;
      (sheet.cells || []).forEach((cell) => {
        const raw = String(cell.raw ?? "");
        if (!raw) return;
        map[cell.ref] = raw.startsWith("=") ? { value: "", formula: raw } : { value: raw, formula: "" };
        const m = /(\d+)$/.exec(cell.ref);
        if (m) maxRow = Math.max(maxRow, Number(m[1]));
      });
      store[sheet.name] = map;
      rowCounts[sheet.name] = Math.max(12, maxRow);
    });
    return { store, rowCounts };
  }

  function createFileOpen(deps) {
    const {
      document, invokeIpc, enqueueOp, runExclusive, captureWorkspaceSnapshot, pushRawUndoSnapshot,
      restoreWorkspaceSnapshot, commitPendingSheetCell, computeEvaluatedSheetStore,
      createEmptySlideStyleFields, getActiveModule,
    } = deps;
    const confirmFn = deps.confirm || ((msg) => (typeof host.confirm === "function" ? host.confirm(msg) : true));
    const btoaFn = (bin) => host.btoa(bin);
    const baselines = {};
    let converterSource = null;

    const feedbackEl = () => document.getElementById("status-command-feedback");
    function setFeedback(text) {
      const el = feedbackEl();
      if (el) el.textContent = text;
    }
    function fingerprint(snap, mod) {
      return host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__.moduleFingerprint(snap, mod);
    }

    // Per-module baselines: what each module held when last opened/saved/initialised.
    function captureBaselines() {
      return runExclusive(async () => {
        const snap = captureWorkspaceSnapshot(false);
        Object.keys(MODULE_LABELS).forEach((mod) => { baselines[mod] = fingerprint(snap, mod); });
      });
    }

    function markModuleSaved() {
      return runExclusive(async () => {
        const mod = getActiveModule();
        if (!MODULE_LABELS[mod]) return;
        baselines[mod] = fingerprint(captureWorkspaceSnapshot(false), mod);
      });
    }

    function isModuleEdited(snap, mod) {
      return Object.prototype.hasOwnProperty.call(baselines, mod) && baselines[mod] !== fingerprint(snap, mod);
    }

    function applyPdf(next, res) {
      const lines = [];
      (res.blocks || []).forEach((b) => {
        if (b.type === "table") (b.rows || []).forEach((r) => lines.push(r.map((c) => c.text).join(" | ")));
        else lines.push((b.runs || []).map((r) => r.text).join(""));
      });
      const title = res.title || "";
      next.pdfStudio = {
        ...(next.pdfStudio || {}),
        stampValue: "", stampBadge: "", watermarkInput: "", watermarkLayer: "",
        applicant: "", dept: "", formStatus: "양식 준비 완료",
        cardHeadingNodes: [textSpec(title)],
        cardBodyParagraphsNodes: lines.filter((l) => l.trim()).map((l) => [textSpec(l)]),
      };
      next.pdfRotationDeg = 0;
    }

    async function applyDecoded(next, res, name) {
      const mod = res.module;
      if (mod === "DOC" || mod === "HWP") {
        next.docTabs = next.docTabs || {};
        next.docTabs[mod] = {
          editorNodes: buildEditorNodes(res.blocks),
          layoutStyle: next.docTabs[mod]?.layoutStyle || { showParaMarks: "true" },
        };
      } else if (mod === "SHEET") {
        const sheets = Array.isArray(res.sheets) && res.sheets.length > 0 ? res.sheets : [{ name: "Sheet1", cells: [] }];
        const { store, rowCounts } = buildSheetStore(sheets);
        const first = sheets[0].name;
        const evaluated = await computeEvaluatedSheetStore(store, first, null);
        next.sheets = evaluated.sheets;
        next.sheetRowCounts = rowCounts;
        next.activeSheetName = first;
        next.activeSheetCellRef = "A1";
      } else if (mod === "SLIDE") {
        const slides = Array.isArray(res.slides) ? res.slides : [];
        next.slides = (slides.length > 0 ? slides : [{ title: "", bullets: [], notes: "" }]).map((s) => ({
          title: s.title || "", lower: (s.bullets || []).join("\n"), notes: s.notes || "",
          transition: "fade", ...createEmptySlideStyleFields(),
        }));
        next.activeSlideIndex = 0;
      } else if (mod === "PDF") {
        applyPdf(next, res);
      } else {
        throw new Error("지원하지 않는 모듈입니다: " + mod);
      }
      next.activeModule = mod;
      next.tabTitles = { ...(next.tabTitles || {}), [mod]: name };
    }

    function pruneSheetTabs(names) {
      const keep = new Set(names);
      document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]").forEach((btn) => {
        if (!keep.has(btn.dataset.sheetTab)) btn.remove();
      });
    }

    function describe(res, name) {
      const label = MODULE_LABELS[res.module] || res.module;
      const notes = Array.isArray(res.warnings) && res.warnings.length > 0 ? " · 참고: " + res.warnings.join(" ") : "";
      return "열기 완료: " + name + " (" + String(res.format).toUpperCase() + " → " + label + " 모듈 · " + res.itemCount +
        (res.kind === "workbook" ? "개 시트" : res.kind === "slides" ? "개 슬라이드" : "개 블록") + ")" + notes;
    }

    function fail(message) {
      setFeedback("파일 열기 실패: " + message);
      return { ok: false, error: message };
    }

    // The one entry point: file picker, drag and drop, and the desktop shell all call this.
    async function openDocumentFromBytes(fileName, bytes) {
      const name = baseName(fileName);
      const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      const invalid = validateFile(name, data.length);
      if (invalid) return fail(invalid + (name ? " (" + name + ")" : ""));
      const convBox = document.getElementById("conv-result-box");
      const convText = convBox ? convBox.textContent : "";
      let res;
      try {
        res = await invokeIpc("convert_document_format", JSON.stringify({
          action: "decode", fileName: name, sourceBase64: bytesToBase64(data, btoaFn), ...GRID_LIMITS,
        }));
      } catch (err) {
        if (convBox) convBox.textContent = convText;
        return fail(name + " — " + (err?.message || String(err)));
      }
      if (!res || res.status !== "OK" || !MODULE_LABELS[res.module]) {
        return fail(name + " — 백엔드 응답을 해석할 수 없습니다");
      }
      setFeedback("파일 여는 중: " + name);
      const outcome = await enqueueOp(async () => {
        await commitPendingSheetCell(true);
        const pre = captureWorkspaceSnapshot(false);
        if (isModuleEdited(pre, res.module)) {
          const ok = confirmFn(
            MODULE_LABELS[res.module] + " 모듈에 저장하지 않은 편집 내용이 있습니다.\n'" + name + "'을(를) 열면 현재 내용이 바뀝니다 (실행 취소로 되돌릴 수 있습니다). 계속할까요?"
          );
          if (!ok) return { cancelled: true };
        }
        const next = JSON.parse(JSON.stringify(pre));
        await applyDecoded(next, res, name);
        pushRawUndoSnapshot(pre);
        await restoreWorkspaceSnapshot(next, null);
        if (res.module === "SHEET") pruneSheetTabs(Object.keys(next.sheets));
        baselines[res.module] = fingerprint(captureWorkspaceSnapshot(false), res.module);
        return { ok: true };
      }).catch((err) => ({ error: err?.message || String(err) }));
      if (outcome.cancelled) {
        setFeedback("파일 열기를 취소했습니다 (" + name + ")");
        return { ok: false, cancelled: true };
      }
      if (outcome.error) return fail(name + " — " + outcome.error);
      setFeedback(describe(res, name));
      return { ok: true, module: res.module, format: res.format, warnings: res.warnings || [], title: name };
    }

    async function readFileBytes(file) {
      const invalid = validateFile(baseName(file?.name), Number(file?.size ?? 0));
      if (invalid) return { error: invalid };
      try {
        return { bytes: new Uint8Array(await file.arrayBuffer()) };
      } catch (err) {
        return { error: "파일을 읽지 못했습니다: " + (err?.message || String(err)) };
      }
    }

    async function openFile(file) {
      const name = baseName(file?.name);
      const read = await readFileBytes(file);
      if (read.error) return fail(read.error + (name ? " (" + name + ")" : ""));
      return openDocumentFromBytes(name, read.bytes);
    }

    // ---- converter source file ----------------------------------------------------------

    function renderConverterSource() {
      const label = document.getElementById("conv-source-name");
      const clearBtn = document.getElementById("btn-conv-clear-source");
      if (label) label.textContent = converterSource ? converterSource.name + " (" + converterSource.bytes.length + "B)" : "선택 안 함 — 편집기 내용을 변환합니다";
      if (clearBtn) clearBtn.hidden = !converterSource;
    }

    async function setConverterSource(file) {
      const box = document.getElementById("conv-result-box");
      const name = baseName(file?.name);
      const read = await readFileBytes(file);
      if (read.error) {
        if (box) box.textContent = "원본 파일을 사용할 수 없습니다: " + read.error + (name ? " (" + name + ")" : "");
        return false;
      }
      converterSource = { name, bytes: read.bytes };
      const ext = extensionOf(name);
      const srcSel = document.getElementById("conv-src-fmt");
      if (srcSel && ext && srcSel.querySelector('option[value="' + (ext === "markdown" ? "md" : ext) + '"]')) {
        srcSel.value = ext === "markdown" ? "md" : ext;
      }
      renderConverterSource();
      if (box) box.textContent = "원본 파일 선택됨: " + name + " — [즉시 변환 검증]을 누르면 이 파일의 내용을 변환합니다.";
      return true;
    }

    function clearConverterSource() {
      converterSource = null;
      renderConverterSource();
    }

    // Fields merged into the convert request, or null when the editor content is converted.
    function getConverterSource() {
      if (!converterSource) return null;
      const stem = converterSource.name.replace(/\.[A-Za-z0-9]+$/, "") || "변환_문서";
      return {
        baseName: stem,
        fields: { fileName: converterSource.name, sourceBase64: bytesToBase64(converterSource.bytes, btoaFn) },
      };
    }

    // ---- DOM wiring ---------------------------------------------------------------------

    function hasFiles(ev) {
      const dt = ev?.dataTransfer;
      if (!dt) return false;
      return (dt.files && dt.files.length > 0) || Array.from(dt.types || []).includes("Files");
    }

    function closeMenus() {
      document.querySelectorAll(".menu-item.open").forEach((el) => el.classList.remove("open"));
    }

    function openFilePicker() {
      const input = document.getElementById("file-open-input");
      if (!input) return setFeedback("파일 열기 입력을 찾을 수 없습니다");
      input.click();
    }

    function bindUi() {
      const input = document.getElementById("file-open-input");
      input?.addEventListener("change", () => {
        const file = input.files && input.files[0];
        input.value = "";
        if (file) void openFile(file);
      });
      document.querySelectorAll("[data-file-open]").forEach((el) => {
        el.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          closeMenus();
          openFilePicker();
        });
      });
      const convInput = document.getElementById("conv-source-file");
      convInput?.addEventListener("change", () => {
        const file = convInput.files && convInput.files[0];
        convInput.value = "";
        if (file) void setConverterSource(file);
      });
      document.getElementById("btn-conv-clear-source")?.addEventListener("click", clearConverterSource);
      renderConverterSource();

      const shell = document.getElementById("app");
      shell?.addEventListener("dragover", (ev) => {
        if (!hasFiles(ev)) return;
        ev.preventDefault();
        shell.classList.add("eo-drop-active");
      });
      shell?.addEventListener("dragleave", () => shell.classList.remove("eo-drop-active"));
      shell?.addEventListener("drop", (ev) => {
        if (!hasFiles(ev)) return;
        ev.preventDefault();
        shell.classList.remove("eo-drop-active");
        const files = Array.from(ev.dataTransfer.files || []);
        if (files.length === 0) return;
        const pending = getActiveModule() === "CONV" ? setConverterSource(files[0]) : openFile(files[0]);
        void pending.then(() => {
          if (files.length > 1) {
            setFeedback(feedbackEl()?.textContent + " · 여러 파일 중 첫 번째 파일만 열었습니다 (" + files.length + "개 선택됨)");
          }
        });
      });
    }

    // Window namespace entry point for the desktop shell (OS file associations).
    host.__EASYOFFICE_WORKSPACE_API__ = Object.assign(host.__EASYOFFICE_WORKSPACE_API__ || {}, {
      openDocumentFromBytes,
    });

    return {
      openDocumentFromBytes, openFile, openFilePicker, bindUi, captureBaselines, markModuleSaved,
      getConverterSource, setConverterSource, clearConverterSource,
    };
  }

  host.__EASYOFFICE_CREATE_FILE_OPEN__ = createFileOpen;
  host.__EASYOFFICE_FILE_OPEN_UTILS__ = {
    MAX_FILE_BYTES, SUPPORTED_EXTENSIONS, GRID_LIMITS, buildEditorNodes, buildSheetStore, bytesToBase64,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
