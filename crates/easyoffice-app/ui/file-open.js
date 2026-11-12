(function (host) {
  "use strict";

  // Opens a user's own file (PD-075): file picker (Ctrl+O), drag and drop, and the single entry
  // point `openDocumentFromBytes(fileName, bytes)` that a desktop shell can call as well. The
  // backend decodes the real bytes into the structured save shape (`blocks` / `sheets` / `slides`);
  // this module loads that model into the matching workspace module as one undoable state.
  // Also lets the converter panel convert a chosen source file instead of the editor text.

  // Files that travel through IPC (decoded by the engine, or sent as a converter source). The same number as
  // `MAX_SOURCE_BYTES` in convert_open.rs (scripts/test-file-size-limits.mjs keeps them equal); base64 inflates
  // 4/3, so 10 MiB stays under the 16 MiB HTTP IPC body limit.
  const MAX_FILE_BYTES = 10 * 1024 * 1024;
  const MAX_FILE_LABEL = "10 MiB";
  const MAX_HWP_BYTES = 64 * 1024 * 1024; // HWP bytes go straight to the editor frame, not through the IPC body
  const MAX_HWP_LABEL = "64 MiB";
  const HWP_EXTENSIONS = new Set(["hwp", "hwpx", "hwt", "hwtx"]);
  const GRID_LIMITS = { maxRows: 1048576, maxCols: 16384 }; // the sheet editor addresses the full XLSX grid (A..XFD)
  const MODULE_LABELS = { DOC: "문서", HWP: "한글 문서", SHEET: "스프레드시트", SLIDE: "프레젠테이션", PDF: "PDF 스튜디오" };
  const SUPPORTED_EXTENSIONS = [
    "docx", "doc", "odt", "rtf", "md", "markdown", "txt",
    "hwp", "hwpx", "hwt", "hwtx",
    "xlsx", "xls", "ods", "csv", "tsv",
    "pptx", "ppt", "ppsx", "odp",
    "pdf", "pages",
  ];

  function baseName(raw) {
    return String(raw || "").split(/[\\/]/).pop().trim();
  }

  function extensionOf(name) {
    const s = String(name || "");
    const dot = s.lastIndexOf(".");
    return dot >= 0 && dot < s.length - 1 ? s.slice(dot + 1).toLowerCase() : "";
  }

  // The converter's format selector value for a file name, or "" when it has none.
  function converterFormatOption(name) {
    const ext = extensionOf(name);
    return ext === "markdown" ? "md" : ext;
  }

  const tooLarge = (size) => "파일이 너무 큽니다 (" + (size / 1048576).toFixed(1) + " MiB). ";

  // The converter takes its source through IPC whatever the file type: an HWP file is not routed to the editor.
  function converterSourceProblem(size) {
    return size > MAX_FILE_BYTES ? tooLarge(size) + "변환기 원본은 최대 " + MAX_FILE_LABEL + "까지 선택할 수 있습니다" : "";
  }

  // `hwpRoute`: the file goes to the embedded editor, which takes the larger HWP limit. `"converter"`: the
  // file is the converter's source. Otherwise an HWP file is read as text through IPC like any other, and the
  // message says why the limit is lower.
  function validateFile(name, size, hwpRoute) {
    if (!name) return "파일 이름을 확인할 수 없습니다";
    if (size === 0) return "빈 파일은 열 수 없습니다";
    if (hwpRoute === "converter") return converterSourceProblem(size);
    const hwpFamily = HWP_EXTENSIONS.has(extensionOf(name));
    const hwp = hwpRoute === true && hwpFamily;
    if (size > (hwp ? MAX_HWP_BYTES : MAX_FILE_BYTES)) {
      if (hwp) return tooLarge(size) + "최대 " + MAX_HWP_LABEL + "까지 열 수 있습니다";
      if (hwpFamily) return tooLarge(size) + "한글 편집기를 쓰지 못하는 경우 텍스트로 불러올 수 있는 한도는 " + MAX_FILE_LABEL + "입니다";
      return tooLarge(size) + "최대 " + MAX_FILE_LABEL + "까지 열 수 있습니다";
    }
    return "";
  }

  // ---- model -> editor state -------------------------------------------------------------

  const textSpec = (v) => ({ t: "text", v: String(v) });
  const elSpec = (tag, children, style, attrs) => ({
    t: "el", tag, attrs: attrs || {}, data: {}, style: style || {}, children: children || [],
  });

  const SCRIPT_TAGS = { superscript: "sup", subscript: "sub" };
  const PX_PER_TWIP = 1 / 15; // 1 twip = 1/20 pt, 1 pt = 4/3 px
  const IMAGE_FORMATS = new Set(["png", "jpg"]);
  const IMAGE_ID_RE = /^[A-Za-z0-9+/=]+$/;

  function twipsToPx(twips) {
    return String(Math.round(twips * PX_PER_TWIP * 1000) / 1000) + "px";
  }

  // Paragraph indents / line spacing of a decoded block as inline style (what the indent buttons
  // and ruler write) plus the exact twip values, so saving gives back the same numbers.
  function applyParagraphLayout(spec, block) {
    const left = Number(block.leftIndentTwips);
    const first = Number(block.firstLineIndentTwips);
    const spacing = Number(block.lineSpacingPercent);
    if (Number.isFinite(left) && left > 0) {
      spec.style.paddingLeft = twipsToPx(left);
      spec.data.eoLeftTwips = String(left);
    }
    if (Number.isFinite(first) && first !== 0) {
      spec.style.textIndent = twipsToPx(first);
      spec.data.eoFirstLineTwips = String(first);
    }
    if (Number.isFinite(spacing) && spacing > 0 && spacing !== 100) {
      spec.style.lineHeight = String(spacing / 100);
      spec.data.eoLineSpacing = String(spacing);
    }
    return spec;
  }

  // A picture block: bytes live in the image store, the node carries only the id, size and alt.
  function imageSpec(block) {
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const format = String(block.format || "");
    const base64 = String(block.base64 || "");
    if (!utils || !IMAGE_FORMATS.has(format) || !IMAGE_ID_RE.test(base64)) {
      return elSpec("p", [textSpec(String(block.alt || "[그림]"))]);
    }
    const alt = String(block.alt ?? "");
    const spec = elSpec("img", [], {}, { class: "doc-inline-image", contenteditable: "false", title: alt });
    spec.data = {
      eoImageId: utils.registerDocImage(base64, format),
      eoImageFormat: format,
      eoImageWidthEmu: String(Number(block.widthEmu) || 0),
      eoImageHeightEmu: String(Number(block.heightEmu) || 0),
      eoImageAlt: alt,
    };
    return spec;
  }

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
          spec = runs.buildRunSpec(fmt, [spec]);
        } else {
          [["strike", "s"], ["underline", "u"], ["italic", "em"], ["bold", "strong"]].forEach(([flag, tag]) => {
            if (run[flag] === true) spec = elSpec(tag, [spec]);
          });
        }
        const scriptTag = SCRIPT_TAGS[run.script];
        const styled = scriptTag ? elSpec(scriptTag, [spec]) : spec;
        const href = host.__EASYOFFICE_DOC_LINK__?.normalizeUrl(run.link);
        out.push(href ? { ...elSpec("a", [styled], {}, { href }), data: { eoLink: "1" } } : styled);
      });
    });
    return out;
  }

  // A safe integer: `(row << 16) | col` wraps at 32 bits, so rows 65536 apart shared a key.
  const cellKey = (row, col) => row * 2 ** 32 + col;

  // Anchors (with their spans) and covered cells of a decoded table's `merges` (PD-130). A region is
  // used only when it is inside the grid, every cell of it exists and no other region has it.
  function mergeLayout(rows, merges) {
    const anchors = new Map();
    const covered = new Set();
    (Array.isArray(merges) ? merges : []).forEach((m) => {
      const { row, col, rowSpan, colSpan } = m && typeof m === "object" ? m : {};
      const ints = [row, col, rowSpan, colSpan];
      if (!ints.every(Number.isInteger) || row < 0 || col < 0 || rowSpan < 1 || colSpan < 1) return;
      if (rowSpan * colSpan < 2 || row + rowSpan > rows.length) return;
      for (let r = row; r < row + rowSpan; r += 1) {
        for (let c = col; c < col + colSpan; c += 1) {
          if (c >= rows[r].length || anchors.has(cellKey(r, c)) || covered.has(cellKey(r, c))) return;
        }
      }
      for (let r = row; r < row + rowSpan; r += 1) {
        for (let c = col; c < col + colSpan; c += 1) covered.add(cellKey(r, c));
      }
      covered.delete(cellKey(row, col));
      anchors.set(cellKey(row, col), { rowSpan, colSpan });
    });
    return { anchors, covered };
  }

  // A table block: merged regions as colspan/rowspan on their top-left cell, covered cells not drawn,
  // and each row's original index so a save can tell which rows of a region were deleted.
  function tableSpec(block) {
    const rows = Array.isArray(block.rows) ? block.rows.map((r) => (Array.isArray(r) ? r : [])) : [];
    const { anchors, covered } = mergeLayout(rows, block.merges);
    const trs = rows.map((row, r) => {
      const cells = [];
      row.forEach((cell, c) => {
        if (covered.has(cellKey(r, c))) return;
        const span = anchors.get(cellKey(r, c));
        const attrs = {};
        if (span?.colSpan > 1) attrs.colspan = String(span.colSpan);
        if (span?.rowSpan > 1) attrs.rowspan = String(span.rowSpan);
        cells.push(elSpec("td", [textSpec(typeof cell === "string" ? cell : cell?.text ?? "")], {}, attrs));
      });
      const tr = elSpec("tr", cells);
      tr.data = { eoRow: String(r) };
      return tr;
    });
    return elSpec("table", [elSpec("tbody", trs)], {}, { class: "doc-inline-table" });
  }

  // One entry or title paragraph of a table-of-contents block; entries are links to their headings (`doc-toc.js`).
  function tocParagraphSpec(block) {
    const inline = runSpecs(block.runs);
    const p = elSpec("p", block.toc === 0 ? inline : [elSpec("a", inline, {}, { href: "#" })]);
    p.data.eoToc = String(block.toc);
    return p;
  }

  function buildEditorNodes(blocks) {
    const nodes = [];
    let openList = null;
    let openToc = null;
    (blocks || []).forEach((block) => {
      if (block.type === "paragraph" && Number.isInteger(block.toc)) {
        if (!openToc) {
          openToc = elSpec("nav", [], {}, { class: "eo-doc-toc", "data-doc-toc": "true", contenteditable: "false" });
          nodes.push(openToc);
        }
        openList = null;
        openToc.children.push(tocParagraphSpec(block));
        return;
      }
      openToc = null;
      if (block.type === "table") {
        openList = null;
        nodes.push(tableSpec(block));
        return;
      }
      if (block.type === "image") {
        openList = null;
        nodes.push(imageSpec(block));
        return;
      }
      const style = block.align ? { textAlign: block.align } : {};
      const inline = runSpecs(block.runs);
      const heading = Number(block.heading) || 0;
      if (heading >= 1 && heading <= 6) {
        openList = null;
        nodes.push(applyParagraphLayout(elSpec("h" + heading, inline, style), block));
      } else if (block.list === "bullet" || block.list === "ordered") {
        const tag = block.list === "ordered" ? "ol" : "ul";
        if (openList?.tag !== tag) {
          openList = elSpec(tag);
          nodes.push(openList);
        }
        openList.children.push(applyParagraphLayout(elSpec("li", inline, style), block));
      } else {
        openList = null;
        nodes.push(applyParagraphLayout(elSpec("p", inline, style), block));
      }
    });
    return nodes;
  }

  // `{ [1-based column]: px }` from a decoded sheet's `columns` (PD-089); invalid entries are skipped.
  function columnWidthMap(columns) {
    const widths = {};
    (Array.isArray(columns) ? columns : []).forEach((col) => {
      const index = Number(col?.index);
      const px = Number(col?.widthPx);
      if (Number.isInteger(index) && index >= 1 && Number.isFinite(px) && px > 0) widths[index] = px;
    });
    return widths;
  }

  // Cell entries keep the decoded `style` (bold/italic/underline/strike/size) so a file's formatting
  // shows after opening and is written again by the next save; styled blank cells are kept too.
  function buildSheetStore(sheets) {
    const codec = host.__EASYOFFICE_SHEET_STYLE_CODEC__;
    const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
    const store = {};
    const rowCounts = {};
    const columnWidths = {};
    const rulesBySheet = {};
    const layoutBySheet = {};
    (sheets || []).forEach((sheet) => {
      const map = {};
      let maxRow = 0;
      (sheet.cells || []).forEach((cell) => {
        const raw = String(cell.raw ?? "");
        const style = codec ? codec.styleFromPayload(cell.style, sheet.numFmts) : null;
        if (!raw && !style) return;
        const entry = raw.startsWith("=") ? { value: "", formula: raw } : { value: raw, formula: "" };
        if (style) entry.style = style;
        map[cell.ref] = entry;
        const packed = gridUtils ? gridUtils.packRef(cell.ref) : 0;
        if (packed > 0) {
          const r = Math.floor(packed / 32768);
          if (r > maxRow) maxRow = r;
        } else {
          const m = /(?<!\d)(\d+)$/.exec(cell.ref);
          if (m) maxRow = Math.max(maxRow, Number(m[1]));
        }
      });
      store[sheet.name] = map;
      rowCounts[sheet.name] = Math.max(12, maxRow);
      const widths = columnWidthMap(sheet.columns);
      if (Object.keys(widths).length > 0) columnWidths[sheet.name] = widths;
      if ((sheet.rules?.cf || []).length > 0 || (sheet.rules?.dv || []).length > 0) rulesBySheet[sheet.name] = sheet.rules;
      if (sheet.merges?.length || sheet.hiddenRows?.length || sheet.hiddenCols?.length) {
        layoutBySheet[sheet.name] = { merges: sheet.merges, hiddenRows: sheet.hiddenRows, hiddenCols: sheet.hiddenCols };
      }
    });
    return { store, rowCounts, columnWidths, rulesBySheet, layoutBySheet };
  }

  function hasFiles(ev) {
    const dt = ev?.dataTransfer;
    if (!dt) return false;
    return (dt.files && dt.files.length > 0) || Array.from(dt.types || []).includes("Files");
  }

  function applyPdf(next, res, name) {
    const lines = [];
    (res.blocks || []).forEach((b) => {
      if (b.type === "table") (b.rows || []).forEach((r) => lines.push(r.map((c) => c.text).join(" | ")));
      else if (b.type === "image") lines.push(String(b.alt || "[그림]"));
      else lines.push((b.runs || []).map((r) => r.text).join(""));
    });
    const title = res.title || name;
    next.pdfStudio = {
      ...next.pdfStudio,
      stampValue: "", stampBadge: "", watermarkInput: "", watermarkLayer: "",
      applicant: "", dept: "", formStatus: "양식 준비 완료",
      cardHeadingNodes: [textSpec(title)],
      cardBodyParagraphsNodes: lines.filter((l) => l.trim()).map((l) => [textSpec(l)]),
    };
    next.pdfRotationDeg = 0;
  }

  function createFileOpen(deps) {
    const {
      document, invokeIpc, enqueueOp, runExclusive, captureWorkspaceSnapshot, pushRawUndoSnapshot,
      restoreWorkspaceSnapshot, commitPendingSheetCell, computeEvaluatedSheetStore,
      createEmptySlideStyleFields, getActiveModule, replaceColumnWidths, replaceSheetRules,
    } = deps;
    const hasHwpRoute = () => Boolean(deps.getHwpSession?.());
    // A named kind answered by the native dialog (`confirm_dialog` in the shell); without a way to ask, no.
    const confirmFn = deps.confirm || (async () => false);
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
      return Object.hasOwn(baselines, mod) && baselines[mod] !== fingerprint(snap, mod);
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
        const { store, rowCounts, columnWidths, rulesBySheet, layoutBySheet } = buildSheetStore(sheets);
        host.__EASYOFFICE_SHEET_LAYOUT__.replaceAll(layoutBySheet);
        next.sheetLayout = layoutBySheet;
        const first = sheets[0].name;
        // The rules go in first, so the evaluation below learns them with the cells.
        if (typeof replaceSheetRules === "function") replaceSheetRules(rulesBySheet);
        next.sheetRules = rulesBySheet;
        const evaluated = await computeEvaluatedSheetStore(store, first, null);
        next.sheets = evaluated.sheets;
        next.sheetRowCounts = rowCounts;
        next.sheetColumnWidths = columnWidths;
        next.activeSheetName = first;
        next.activeSheetCellRef = "A1";
        if (typeof replaceColumnWidths === "function") replaceColumnWidths(columnWidths);
      } else if (mod === "SLIDE") {
        const slides = Array.isArray(res.slides) ? res.slides : [];
        next.slides = (slides.length > 0 ? slides : [{ title: "", bullets: [], notes: "" }]).map((s) => ({
          title: s.title || "", lower: (s.bullets || []).join("\n"), notes: s.notes || "",
          transition: "fade", ...createEmptySlideStyleFields(),
        }));
        next.activeSlideIndex = 0;
      } else if (mod === "PDF") {
        applyPdf(next, res, name);
      } else {
        throw new Error("지원하지 않는 모듈입니다: " + mod);
      }
      next.activeModule = mod;
      next.tabTitles = { ...next.tabTitles, [mod]: name };
    }

    // A file replaces the whole workbook: its tabs stay, and the sample formula result goes away.
    function pruneSheetTabs(names) {
      const result = document.getElementById("sheet-eval-result");
      if (result) result.textContent = "";
      const keep = new Set(names);
      document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]").forEach((btn) => {
        if (!keep.has(btn.dataset.sheetTab)) btn.remove();
      });
    }

    const ITEM_UNIT_BY_KIND = new Map([
      ["workbook", "개 시트"],
      ["slides", "개 슬라이드"],
    ]);
    const DEFAULT_ITEM_UNIT = "개 블록";

    function describe(res, name) {
      const label = MODULE_LABELS[res.module] || res.module;
      const notes = Array.isArray(res.warnings) && res.warnings.length > 0 ? " · 참고: " + res.warnings.join(" ") : "";
      return "열기 완료: " + name + " (" + String(res.format).toUpperCase() + " → " + label + " 모듈 · " + res.itemCount +
        (ITEM_UNIT_BY_KIND.get(res.kind) ?? DEFAULT_ITEM_UNIT) + ")" + notes;
    }

    function fail(message) {
      setFeedback("파일 열기 실패: " + message);
      return { ok: false, error: message };
    }

    // The one entry point: file picker, drag and drop, and the desktop shell all call this.
    // HWP family files go to the embedded editor (hwp-session.js) unless `textOnly` forces the decode path.
    async function openDocumentFromBytes(fileName, bytes, options) {
      const name = baseName(fileName);
      const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      const hwpSession = options?.textOnly ? null : deps.getHwpSession?.();
      if (hwpSession && HWP_EXTENSIONS.has(extensionOf(name))) {
        const opened = await hwpSession.openFromBytes(name, data, {
          textFallback: () => openDocumentFromBytes(name, data, { textOnly: true }),
        });
        // The embedded editor now holds this document, which the converter offers as its source.
        if (opened?.ok && opened.module === "HWP") selectConverterFormat(name);
        return opened;
      }
      const invalid = validateFile(name, data.length);
      if (invalid) return fail(invalid + (name ? " (" + name + ")" : ""));
      const convBox = document.getElementById("conv-result-box");
      const convText = convBox ? convBox.textContent : "";
      let res;
      try {
        res = await invokeIpc("convert_document_format", JSON.stringify({
          action: "decode", fileName: name, sourceBase64: host.__EASYOFFICE_SHEET_UTILS__.bytesToBase64(data, btoaFn), ...GRID_LIMITS,
        }));
      } catch (err) {
        if (convBox) convBox.textContent = convText;
        return fail(name + " — " + (err?.message || String(err)));
      }
      if (res?.status !== "OK" || !MODULE_LABELS[res.module]) {
        return fail(name + " — 응답을 처리하지 못했습니다. 다시 시도해 주세요.");
      }
      setFeedback("파일 여는 중: " + name);
      const outcome = await enqueueOp(async () => {
        await commitPendingSheetCell(true);
        const pre = captureWorkspaceSnapshot(false);
        if (isModuleEdited(pre, res.module)) {
          if ((await confirmFn("replace-unsaved-module")) !== true) return { cancelled: true };
        }
        const next = structuredClone(pre);
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
      // A PDF is also handed to the shell, so that the studio edits its objects (PD-170).
      const pdfNote = res.module === "PDF" && deps.pdfEdit ? await deps.pdfEdit.open(name, data) : "";
      setFeedback(describe(res, name) + pdfNote);
      return { ok: true, module: res.module, format: res.format, warnings: res.warnings || [], title: name };
    }

    async function readFileBytes(file, hwpRoute) {
      const invalid = validateFile(baseName(file?.name), Number(file?.size ?? 0), hwpRoute);
      if (invalid) return { error: invalid };
      try {
        return { bytes: new Uint8Array(await file.arrayBuffer()) };
      } catch (err) {
        return { error: "파일을 읽지 못했습니다: " + (err?.message || String(err)) };
      }
    }

    async function openFile(file) {
      const name = baseName(file?.name);
      const read = await readFileBytes(file, hasHwpRoute());
      if (read.error) return fail(read.error + (name ? " (" + name + ")" : ""));
      return openDocumentFromBytes(name, read.bytes);
    }

    // ---- converter source file ----------------------------------------------------------

    function renderConverterSource() {
      const label = document.getElementById("conv-source-name");
      const clearBtn = document.getElementById("btn-conv-clear-source");
      if (label) label.textContent = converterSource ? converterSource.name + " (" + converterSource.bytes.length + "B)" : "선택 안 함";
      if (clearBtn) clearBtn.hidden = !converterSource;
    }

    function selectConverterFormat(name) {
      const srcSel = document.getElementById("conv-src-fmt");
      const option = converterFormatOption(name);
      if (srcSel && option && srcSel.querySelector('option[value="' + option + '"]')) srcSel.value = option;
    }

    async function setConverterSource(file) {
      const box = document.getElementById("conv-result-box");
      const name = baseName(file?.name);
      const read = await readFileBytes(file, "converter");
      if (read.error) {
        if (box) box.textContent = "원본 파일을 사용할 수 없습니다: " + read.error + (name ? " (" + name + ")" : "");
        return false;
      }
      converterSource = { name, bytes: read.bytes };
      selectConverterFormat(name);
      renderConverterSource();
      if (box) box.textContent = "원본 파일 선택됨: " + name;
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
        size: converterSource.bytes.length,
        fields: { fileName: converterSource.name, sourceBase64: host.__EASYOFFICE_SHEET_UTILS__.bytesToBase64(converterSource.bytes, btoaFn) },
      };
    }

    // ---- DOM wiring ---------------------------------------------------------------------

    function closeMenus() {
      host.__EASYOFFICE_CLOSE_MENUS__(document);
    }

    // In a managed window (PD-088) opening a file means a new window: `onOpenRequest` handles the
    // menu/shortcut entry and `routeFile` the picker/drop entry; both fall through to in-place open.
    function openFilePicker() {
      if (deps.onOpenRequest?.()) return;
      const input = document.getElementById("file-open-input");
      if (!input) return setFeedback("파일 열기 입력을 찾을 수 없습니다");
      input.click();
    }

    function bindUi() {
      const input = document.getElementById("file-open-input");
      input?.addEventListener("change", () => {
        const file = input.files?.[0];
        input.value = "";
        if (file && !deps.routeFile?.(file)) void openFile(file);
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
        const file = convInput.files?.[0];
        convInput.value = "";
        if (file) void setConverterSource(file);
      });
      document.getElementById("btn-conv-clear-source")?.addEventListener("click", clearConverterSource);
      renderConverterSource();

      // The browser's own drop behaviour (opening or downloading the file) is off for the whole
      // document; only the editing area accepts a file and shows the drop outline. Reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md),
      // `apps/studio-host/src/main.ts`: the same document-wide guard and editing-area drop target.
      document.addEventListener("dragover", (ev) => ev.preventDefault());
      document.addEventListener("drop", (ev) => ev.preventDefault());
      const dropArea = document.querySelector(".canvas-stage");
      dropArea?.addEventListener("dragover", (ev) => {
        if (!hasFiles(ev)) return;
        ev.preventDefault();
        dropArea.classList.add("eo-drop-active");
      });
      dropArea?.addEventListener("dragleave", () => dropArea.classList.remove("eo-drop-active"));
      dropArea?.addEventListener("drop", (ev) => {
        if (!hasFiles(ev)) return;
        ev.preventDefault();
        dropArea.classList.remove("eo-drop-active");
        const files = Array.from(ev.dataTransfer.files || []);
        if (files.length === 0) return;
        if (getActiveModule() !== "CONV" && deps.routeFile?.(files[0])) return;
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
      getConverterSource, setConverterSource, clearConverterSource, converterSourceProblem,
    };
  }

  const EXT_TO_FORMAT = Object.freeze({
    docx: "docx", doc: "docx", odt: "odt", rtf: "rtf", md: "md", markdown: "md", txt: "txt",
    hwpx: "hwpx", hwp: "hwp", hwt: "hwp", hwtx: "hwpx",
    xlsx: "xlsx", xls: "xls", ods: "ods", csv: "csv", tsv: "tsv",
    pptx: "pptx", ppsx: "ppsx", ppt: "pptx", odp: "odp", pdf: "pdf",
  });

  function inferFormatFromFileName(fileName) {
    const raw = String(fileName || "").trim();
    const dot = raw.lastIndexOf(".");
    return dot >= 0 && dot < raw.length - 1 ? EXT_TO_FORMAT[raw.slice(dot + 1).toLowerCase()] || "" : "";
  }
  host.__EASYOFFICE_INFER_FORMAT__ = inferFormatFromFileName;

  host.__EASYOFFICE_CREATE_FILE_OPEN__ = createFileOpen;
  host.__EASYOFFICE_FILE_OPEN_UTILS__ = {
    MAX_FILE_BYTES, MAX_HWP_BYTES, SUPPORTED_EXTENSIONS, GRID_LIMITS, buildEditorNodes, buildSheetStore,
    converterSourceProblem, converterFormatOption,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
