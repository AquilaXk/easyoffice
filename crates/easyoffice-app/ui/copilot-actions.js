// 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., see THIRD_PARTY_NOTICES.md)

(function (host) {
  "use strict";

  const MAX_CONTEXT_CELLS = 512;
  // The engine puts this many paragraphs in the prompt (`MAX_PROMPT_DOC_PARAGRAPHS`) and answers for the same limit.
  const MAX_CONTEXT_PARAGRAPHS = 200;
  // A selection command on a later block is sent with empty entries up to it (the engine reads 2,048 at most).
  const MAX_COMMAND_BLOCK_INDEX = 2047;
  const SURROUNDING_PARAGRAPH_WINDOW = 16;
  const MAX_CONTEXT_SLIDES = 64;
  const MAX_SLIDES = 256;
  const MAX_CONTEXT_TEXT_CHARS = 24000;
  // A paragraph a selection command works on is sent whole up to one code point past the engine's 24 KiB context
  // (`MAX_PROMPT_CONTEXT_BYTES`): a longer one is cut here, stays over the limit, and the engine refuses the command.
  const MAX_COMMAND_SPAN_CHARS = 24 * 1024 + 1;
  const MAX_OPERATIONS = 64;
  const MAX_WRITE_CELLS = 1024;
  const MAX_RANGE_CELLS = 4096;

  const compareCoordEntries = (a, b) => a.row - b.row || a.col - b.col;
  const formatSelectionA1 = (rg, colName) => (
    !rg || !Number.isInteger(rg.r1) || !Number.isInteger(rg.c1)
      ? "A1"
      : colName(rg.c1) + rg.r1 + (rg.r1 === rg.r2 && rg.c1 === rg.c2 ? "" : ":" + colName(rg.c2) + rg.r2)
  );
  const docBlocksOf = (editor) => host.__EASYOFFICE_COPILOT_DOC_HTML__?.docBlocksOf
    ? host.__EASYOFFICE_COPILOT_DOC_HTML__.docBlocksOf(editor)
    : Array.from(editor?.querySelectorAll("p, h1, h2, h3, h4, ul, ol, blockquote, table") || [])
      .filter((el) => el.parentElement === editor && !el.hidden && !el.classList.contains("manuscript-notice"));
  const buildDocBlockElement = (document, item) => {
    if (host.__EASYOFFICE_COPILOT_DOC_HTML__?.buildDocBlockElement) return host.__EASYOFFICE_COPILOT_DOC_HTML__.buildDocBlockElement(document, item);
    const p = document.createElement("p");
    p.textContent = String(item?.text ?? item ?? "");
    return p;
  };
  const createFallbackExtOps = () => ({
    getEmbedHwpHost: () => null, refreshHwpSnapshot: async () => null, canUndoHwpSync: () => false,
    getHwpSelectionLabel: () => "", getHwpCommandTarget: () => null,
    extractHwpContextAsync: async () => ({ selection: {}, context: {} }), extractPdfContext: () => ({ pageCount: 0, text: "" }),
  });

  function createCopilotActions(deps) {
    const {
      document,
      sheetDataStore,
      sheetRowCounts,
      slideDeckState,
      getActiveModule,
      getActiveSheetName,
      getActiveSlideIndex,
      getSheetController,
      getWorkspacePanels,
      getPdfEdit,
      activatePane,
      captureWorkspaceSnapshot,
      restoreWorkspaceSnapshot,
      commitDeferredUndoSnapshot,
      getHistoryMutationEpoch,
      noteActivity,
      extractEditableDocumentText,
      localScrubPii,
    } = deps;

    const { colName, parseRef, refOf } = host.__EASYOFFICE_SHEET_REFS__;
    const { clipCodePoints } = host.__EASYOFFICE_COPILOT_TEXT__;
    const docHtml = host.__EASYOFFICE_COPILOT_DOC_HTML__ || null;
    const sheetOps = host.__EASYOFFICE_COPILOT_SHEET_OPS__ || null;
    const extOps = typeof host.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__ === "function"
      ? host.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__(deps)
      : createFallbackExtOps();
    let lastUndoEntry = null;
    let activeRun = null;
    const idleWaiters = [];
    // 연출이 끝날 때까지 기다리는 약속(자동 저장과 창 닫기가 임시로 쓴 글을 읽지 않게 한다).
    const whenIdle = () => (activeRun === null ? Promise.resolve() : new Promise((resolve) => idleWaiters.push(resolve)));
    function endRun() {
      activeRun = null;
      for (const resolve of idleWaiters.splice(0)) resolve();
    }

    // 접근성: 움직임 줄이기를 켠 사용자에게는 포인터·타이핑 연출 없이 바로 적용한다.
    const isReducedMotion = () => host.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches === true;
    const preview = host.__EASYOFFICE_CREATE_COPILOT_PREVIEW__({
      document,
      win: host,
      docBlocksOf,
      slideDeckState,
      sheetDataStore,
      getActiveSheetName,
      getSheetController,
      extOps,
    });
    const cursorApi = host.__EASYOFFICE_COPILOT_CURSOR__;
    const performer = host.__EASYOFFICE_CREATE_COPILOT_PERFORMER__({
      document,
      win: host,
      docBlocksOf,
      getWorkspacePanels,
      slideDeckState,
      getActiveSlideIndex,
      getActiveSheetName,
      getSheetController,
      blockText: preview.blockText,
      isReducedMotion,
      createCursor: () => cursorApi.createCopilotCursor({ document, win: host }),
      createTypist: host.__EASYOFFICE_COPILOT_TYPING__.createTypist,
    });

    function getSelectionLabel(mod = getActiveModule()) {
      if (mod === "SHEET") {
        const sc = getSheetController();
        return sc ? formatSelectionA1(sc.getSelectionRange?.(), colName) : "A1";
      }
      if (mod === "SLIDE") {
        return "슬라이드 " + (getActiveSlideIndex() + 1);
      }
      if (mod === "BASE" || mod === "BOARD") {
        return host.__EASYOFFICE_BASE_BOARD_HOST__?.extractCopilotContext?.(mod)?.selection?.range || "";
      }
      if (mod === "HWP" && extOps.getEmbedHwpHost()) {
        return extOps.getHwpSelectionLabel();
      }
      if (mod === "DOC" || mod === "HWP") {
        const sel = host.getSelection?.();
        const editor = document.getElementById("doc-editor");
        if (sel && sel.rangeCount > 0 && !sel.isCollapsed) {
          const range = sel.getRangeAt(0);
          if (editor && editor.contains(range.commonAncestorContainer)) {
            const txt = String(range.toString?.() || "").trim();
            if (txt) return "선택 텍스트 " + Math.min(Array.from(txt).length, 999) + "자";
          }
        }
      }
      return "";
    }

    function freezeSheetSelection() {
      if (sheetOps?.freezeSelection) {
        return sheetOps.freezeSelection(getActiveSheetName(), getSheetController());
      }
      return { sheet: getActiveSheetName(), range: getSelectionLabel("SHEET") || "A1" };
    }

    function parseSelectionBounds(rangeStr) {
      const clean = String(rangeStr || "").trim();
      if (!clean) return null;
      const parts = clean.split(":");
      if (parts.length > 2) return null;
      const p1 = parseRef(parts[0]);
      const p2 = parts.length === 2 ? parseRef(parts[1]) : p1;
      if (!p1 || !p2) return null;
      return {
        r1: Math.min(p1.row, p2.row),
        r2: Math.max(p1.row, p2.row),
        c1: Math.min(p1.col, p2.col),
        c2: Math.max(p1.col, p2.col),
      };
    }

    function extractSheetContext(selectionRange) {
      const sc = getSheetController();
      const sheetName = getActiveSheetName();
      if (sc) sheetDataStore[sheetName] = sc.snapshotDomSheetCells();
      const map = sheetDataStore[sheetName] || {};
      const selBounds = parseSelectionBounds(selectionRange);
      const parsed = [];
      let minR = Infinity, maxR = 0, minC = Infinity, maxC = 0;
      for (const coord of Object.keys(map)) {
        const rec = map[coord];
        if (!rec) continue;
        const raw = rec.formula ? String(rec.formula) : String(rec.value ?? "");
        const value = String(rec.value ?? "");
        if (!raw && !value) continue;
        const parts = parseRef(coord);
        if (!parts) continue;
        if (parts.row < minR) minR = parts.row;
        if (parts.row > maxR) maxR = parts.row;
        if (parts.col < minC) minC = parts.col;
        if (parts.col > maxC) maxC = parts.col;
        const inSel = Boolean(
          selBounds
          && parts.row >= selBounds.r1 && parts.row <= selBounds.r2
          && parts.col >= selBounds.c1 && parts.col <= selBounds.c2,
        );
        parsed.push({ coord: refOf(parts.row, parts.col), row: parts.row, col: parts.col, inSel, raw, value });
      }
      parsed.sort((a, b) => {
        if (a.inSel !== b.inSel) return a.inSel ? -1 : 1;
        return compareCoordEntries(a, b);
      });
      const cells = parsed.slice(0, MAX_CONTEXT_CELLS).map(({ coord, raw, value }) => ({ coord, raw, value }));
      const tabs = Object.keys(sheetDataStore).slice(0, 64);
      const usedRange = maxR >= minR && maxC >= minC ? refOf(minR, minC) + ":" + refOf(maxR, maxC) : "";
      const out = { name: sheetName, cells };
      if (tabs.length > 0) out.tabs = tabs;
      if (usedRange) out.usedRange = usedRange;
      if (parsed.length > cells.length) out.totalCells = parsed.length;
      return out;
    }

    // One entry per block, empty ones included, so that an index in the context is the block index an operation
    // names. Out-of-window sparse slots emit `null` so the engine distinguishes unloaded blocks from genuine blanks.
    function extractDocContext(span) {
      const editor = document.getElementById("doc-editor");
      const blocks = docBlocksOf(editor);
      const winStart = span ? Math.max(0, span.start - SURROUNDING_PARAGRAPH_WINDOW) : 0;
      const winEnd = span
        ? Math.min(blocks.length - 1, MAX_COMMAND_BLOCK_INDEX, span.end + SURROUNDING_PARAGRAPH_WINDOW)
        : -1;
      const lastSent = span ? Math.max(MAX_CONTEXT_PARAGRAPHS - 1, winEnd) : MAX_CONTEXT_PARAGRAPHS - 1;
      const count = Math.min(blocks.length, lastSent + 1);
      const paragraphs = [];
      for (let i = 0; i < count; i += 1) {
        const inSpan = Boolean(span) && i >= span.start && i <= span.end;
        const inWindow = Boolean(span) && i >= winStart && i <= winEnd;
        const limit = inSpan ? MAX_COMMAND_SPAN_CHARS : MAX_CONTEXT_TEXT_CHARS;
        if (inSpan || inWindow || i < MAX_CONTEXT_PARAGRAPHS) {
          const serialized = docHtml ? docHtml.serializeBlockForContext(blocks[i]) : String(blocks[i].textContent || "").trim();
          paragraphs.push(clipCodePoints(serialized, limit));
        } else {
          paragraphs.push(null);
        }
      }
      let selectedText = "";
      const sel = host.getSelection?.();
      if (sel && sel.rangeCount > 0 && !sel.isCollapsed && editor?.contains(sel.getRangeAt(0).commonAncestorContainer)) {
        selectedText = clipCodePoints(sel.getRangeAt(0).toString?.() || "", MAX_CONTEXT_TEXT_CHARS);
      }
      return { paragraphs, selectedText, omittedParagraphs: Math.max(0, blocks.length - count) };
    }

    function blockIndexOf(node, editor) {
      let cur = node && node.nodeType === 1 ? node : node?.parentElement;
      while (cur && cur.parentElement !== editor) cur = cur.parentElement;
      return cur ? docBlocksOf(editor).indexOf(cur) : -1;
    }

    // Where a selection command works: the blocks a text selection in the document touches, or the slide title or
    // body a selection sits in. `null` when there is nothing selected there.
    function getCommandTarget(mod = getActiveModule()) {
      if (mod === "HWP" && extOps.getEmbedHwpHost()) {
        return extOps.getHwpCommandTarget();
      }
      const sel = host.getSelection?.();
      if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return null;
      const range = sel.getRangeAt(0);
      if (mod === "DOC" || mod === "HWP") {
        const editor = document.getElementById("doc-editor");
        if (!editor || !editor.contains(range.commonAncestorContainer)) return null;
        const first = blockIndexOf(range.startContainer, editor);
        const last = blockIndexOf(range.endContainer, editor);
        if (first < 0 || last < first || last > MAX_COMMAND_BLOCK_INDEX) return null;
        const chars = Array.from(String(range.toString?.() || "").trim()).length;
        return { selection: { range: "선택 텍스트 " + Math.min(chars, 999) + "자", paragraphIndex: first, paragraphEnd: last } };
      }
      if (mod === "SLIDE") {
        for (const [id, slideField] of [["slide-title-zone", "title"], ["slide-lower-zone", "body"]]) {
          if (document.getElementById(id)?.contains(range.commonAncestorContainer)) {
            return { selection: { range: "슬라이드 " + (getActiveSlideIndex() + 1), slideIndex: getActiveSlideIndex(), slideField } };
          }
        }
      }
      return null;
    }

    // What a selection command's target holds right now, as one string: the text of the selected blocks, or the title or
    // body of the selected slide. `null` when the target is gone. Regenerate sends nothing when this differs from the
    // snapshot taken with the first request (plate `useAIChat`/`AIChatPlugin` `reload`: restore the target, and stop
    // when it cannot be restored).
    function snapshotTarget(mod, target) {
      const sel = target?.selection;
      if (!sel) return null;
      if (mod === "HWP" && extOps.getEmbedHwpHost()) {
        return String(sel.selectedText || "");
      }
      if (mod === "DOC" || mod === "HWP") {
        const blocks = docBlocksOf(document.getElementById("doc-editor"));
        const texts = [];
        for (let i = sel.paragraphIndex; i <= sel.paragraphEnd; i += 1) {
          if (!blocks[i]) return null;
          texts.push(docHtml ? docHtml.serializeBlockForContext(blocks[i]) : String(blocks[i].textContent || "").trim());
        }
        return JSON.stringify(texts);
      }
      if (mod === "SLIDE") {
        getWorkspacePanels()?.syncActiveSlideDomStylesToDeck?.();
        const slide = slideDeckState[sel.slideIndex];
        return slide ? JSON.stringify([sel.slideField, String(sel.slideField === "title" ? slide.title : slide.lower || "")]) : null;
      }
      return null;
    }

    function extractSlideContext() {
      const panels = getWorkspacePanels();
      panels?.syncActiveSlideDomStylesToDeck?.();
      const slides = slideDeckState.slice(0, MAX_CONTEXT_SLIDES).map((s, index) => {
        const rawBody = String(s?.lower || "");
        const bullets = rawBody.split(/\n+/).map((t) => t.trim()).filter(Boolean);
        const elements = (Array.isArray(s?.elements) ? s.elements : []).slice(0, 32).map((el) => ({
          id: String(el.id || ""),
          kind: String(el.kind || ""),
          x: Number(el.x || 0),
          y: Number(el.y || 0),
          w: Number(el.w || 0),
          h: Number(el.h || 0),
          text: clipCodePoints(el.text || el.title || el.alt || "", 160),
        }));
        const item = {
          index,
          title: String(s?.title || ""),
          bullets,
        };
        const notes = String(s?.notes || "");
        if (notes.trim()) item.notes = notes;
        const layout = String(s?.layout || "titleAndContent");
        if (layout && layout !== "titleAndContent") item.layout = layout;
        if (elements.length > 0) item.elements = elements;
        return item;
      });
      return { activeIndex: getActiveSlideIndex(), aspect: String(slideDeckState[0]?.aspect || "16:9"), slides };
    }

    function extractContext(mod = getActiveModule(), { target, frozenSelection } = {}) {
      const label = getSelectionLabel(mod);
      if (mod === "SHEET") {
        const sheet = frozenSelection?.sheet || getActiveSheetName();
        const range = frozenSelection?.range || label || "A1";
        return { selection: { sheet, range }, context: { sheet: extractSheetContext(range) } };
      }
      if (mod === "DOC" || (mod === "HWP" && !extOps.getEmbedHwpHost())) {
        const sel = target?.selection;
        const span = sel ? { start: sel.paragraphIndex, end: sel.paragraphEnd } : null;
        return { selection: sel ? { ...sel } : label ? { range: label } : {}, context: { doc: extractDocContext(span) } };
      }
      if (mod === "SLIDE") {
        return { selection: target?.selection ? { ...target.selection } : { slideIndex: getActiveSlideIndex() }, context: { slide: extractSlideContext() } };
      }
      if (mod === "BASE" || mod === "BOARD") return host.__EASYOFFICE_BASE_BOARD_HOST__?.extractCopilotContext?.(mod) || { selection: {}, context: {} };
      if (mod === "PDF") return { selection: {}, context: { pdf: extOps.extractPdfContext() } };
      return { selection: {}, context: {} };
    }

    async function extractContextAsync(mod = getActiveModule(), options = {}) {
      if (mod === "HWP" && extOps.getEmbedHwpHost()) {
        return extOps.extractHwpContextAsync(options.target || null);
      }
      return extractContext(mod, options);
    }

    function applyCellStyleFields(map, ref, spec) {
      const styleCodec = host.__EASYOFFICE_SHEET_STYLE_CODEC__;
      const look = host.__EASYOFFICE_SHEET_LOOK__;
      if (!styleCodec?.patchCell) return;
      const hasBold = typeof spec.bold === "boolean";
      const fillHex = spec.fill !== undefined ? look?.normalizeColor(spec.fill) : undefined;
      const textHex = spec.color !== undefined ? look?.normalizeColor(spec.color) : undefined;
      const hasNumFmt = typeof spec.numFmt === "string";
      if (!hasBold && fillHex === undefined && textHex === undefined && !hasNumFmt) return;
      styleCodec.patchCell(map, ref, (st) => {
        if (hasBold) st.fontWeight = spec.bold ? "600" : "";
        if (fillHex !== undefined) st.fill = fillHex || "";
        if (textHex !== undefined) st.fontColor = textHex || "";
        if (hasNumFmt) st.numFmt = spec.numFmt.trim();
      });
    }

    // `formulaRefs` collects the cells that got a formula: in manual calculation mode the formulas that are new are
    // calculated at once, as for a formula typed, pasted or filled.
    function executeSheetWrite(op, map, sheetName, locations, formulaRefs) {
      const cells = Array.isArray(op.cells) ? op.cells : [];
      if (cells.length === 0 || cells.length > MAX_WRITE_CELLS) {
        throw new Error("시트 쓰기 셀 개수가 허용 범위를 벗어났습니다.");
      }
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      let minR = Infinity, maxR = 0, minC = Infinity, maxC = 0;
      for (const item of cells) {
        const parts = parseRef(String(item?.coord || "").trim().toUpperCase());
        if (!parts || parts.row < 1 || parts.col < 1) {
          throw new Error("유효하지 않은 셀 좌표입니다: " + String(item?.coord || ""));
        }
        const ref = refOf(parts.row, parts.col);
        if (guard?.isOn(sheetName) && !guard.cellEditable(sheetName, parts.row, parts.col, map[ref]?.style)) {
          guard.alertBlocked?.();
          throw new Error("보호된 시트 셀은 편집할 수 없습니다: " + ref);
        }
        const rawStr = String(item.raw ?? item.formula ?? item.value ?? "");
        const isFormula = rawStr.startsWith("=");
        const prev = map[ref];
        const nextEntry = {
          value: isFormula ? "" : rawStr,
          formula: isFormula ? rawStr : "",
        };
        if (prev?.style) nextEntry.style = prev.style;
        if (prev?.link) nextEntry.link = prev.link;
        if (!isFormula) {
          const t = host.__EASYOFFICE_SHEET_CELL_TYPE__?.explicitTag(rawStr, item.t);
          if (t) nextEntry.t = t;
        }
        map[ref] = nextEntry;
        if (isFormula) formulaRefs?.add(ref);
        applyCellStyleFields(map, ref, item);
        if (parts.row > (sheetRowCounts[sheetName] || 12)) sheetRowCounts[sheetName] = parts.row;
        minR = Math.min(minR, parts.row);
        maxR = Math.max(maxR, parts.row);
        minC = Math.min(minC, parts.col);
        maxC = Math.max(maxC, parts.col);
      }
      if (maxR >= minR && maxC >= minC) {
        const range = formatSelectionA1({ r1: minR, r2: maxR, c1: minC, c2: maxC }, colName);
        locations.push({ module: "SHEET", label: range, sheet: sheetName, range });
      }
    }

    function executeSheetFormat(op, map, sheetName, locations) {
      const names = host.__EASYOFFICE_SHEET_NAMES__;
      const rawRange = String(op.range || "").trim().toUpperCase();
      const rg = names?.parseAddress(rawRange);
      if (!rg) throw new Error("유효하지 않은 셀 범위입니다: " + rawRange);
      const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
      if (area <= 0 || area > MAX_RANGE_CELLS) {
        throw new Error("서식 적용 범위가 허용 한도를 초과했습니다.");
      }
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      if (guard?.isOn(sheetName) && !guard.guardAction(sheetName, "formatCells")) {
        throw new Error("보호된 시트에서는 셀 서식을 변경할 수 없습니다.");
      }
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        for (let c = rg.c1; c <= rg.c2; c += 1) {
          const ref = refOf(r, c);
          if (!map[ref]) map[ref] = { value: "", formula: "" };
          applyCellStyleFields(map, ref, op);
        }
      }
      const label = formatSelectionA1(rg, colName);
      locations.push({ module: "SHEET", label, sheet: sheetName, range: label });
    }

    function executeSheetChart(op, sheetName, locations) {
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      if (guard?.isOn(sheetName)) {
        throw new Error("보호된 시트에서는 차트를 추가할 수 없습니다.");
      }
      const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
      const svgApi = host.__EASYOFFICE_SHEET_CHART_SVG__;
      if (!layoutApi || !svgApi) {
        throw new Error("차트 모듈을 사용할 수 없습니다.");
      }
      const existing = layoutApi.chartsOf(sheetName);
      const created = svgApi.createChartModel({
        sheetName,
        sheetDataStore,
        existingCharts: existing,
        spec: op,
      });
      layoutApi.setCharts(sheetName, [...existing, created]);
      const label = created.title || created.dataRange;
      locations.push({ module: "SHEET", label, sheet: sheetName, range: created.dataRange });
    }

    async function flushSheetEvaluationAndFormats(sheetName, formulaRefs = []) {
      const sc = getSheetController();
      if (!sc) return;
      if (sheetName && sheetName !== getActiveSheetName() && sheetDataStore[sheetName]) {
        await sc.switchSheetTab?.(sheetName);
      }
      sc.restoreDomSheetCells(sheetName);
      await sc.recalculateActiveSheetFormulas({ updateFocusedDisplay: true, editedRefs: formulaRefs });
      const map = sheetDataStore[sheetName] || {};
      const byCode = new Map();
      for (const ref of Object.keys(map)) {
        const entry = map[ref];
        const code = entry?.style?.numFmt;
        const val = String(entry?.value ?? "");
        if (!code || val === "") continue;
        if (!byCode.has(code)) byCode.set(code, new Set());
        byCode.get(code).add(val);
      }
      const numFmtDisplay = host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__;
      if (numFmtDisplay?.preview && byCode.size > 0) {
        for (const [code, set] of byCode.entries()) {
          await numFmtDisplay.preview(code, Array.from(set));
        }
        sc.restoreDomSheetCells(sheetName);
      }
      sc.syncChartLayer?.();
      sc.syncSheetFormatToolbar?.();
    }

    function executeDocOperation(kind, op, locations) {
      const editor = document.getElementById("doc-editor");
      if (!editor) throw new Error("문서 편집 영역을 찾을 수 없습니다.");
      if (kind === "doc.scrub_pii") {
        if (docHtml?.scrubDocPiiInEditor) return docHtml.scrubDocPiiInEditor(editor, localScrubPii, locations);
        return false;
      }

      const existing = docBlocksOf(editor);
      const tracking = Boolean(docHtml?.isTrackChangesActive?.(editor));
      if (kind === "doc.replace_span") {
        const idx = Number(op?.index);
        if (!Number.isInteger(idx) || idx < 0 || idx >= existing.length) {
          throw new Error("문서 부분 교체 대상 문단 번호가 범위를 벗어났습니다.");
        }
        const search = String(op?.search ?? "");
        const replace = String(op?.replace ?? "");
        if (docHtml?.replaceSpanWithTrackChanges) {
          docHtml.replaceSpanWithTrackChanges(document, editor, existing[idx], search, replace);
        } else {
          if (!search.trim()) throw new Error("검색 대상 문자열은 비어 있을 수 없습니다.");
          const cur = String(existing[idx].textContent || "");
          const at = cur.indexOf(search);
          if (at < 0 || cur.indexOf(search, at + 1) >= 0) {
            throw new Error("변경할 대상 구간을 문단에서 유일하게 찾을 수 없습니다.");
          }
          existing[idx].textContent = cur.slice(0, at) + replace + cur.slice(at + search.length);
        }
        locations.push({ module: "DOC", label: "문단 " + (idx + 1), paragraphIndex: idx });
        return true;
      }
      if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
        if (op.replacements.length === 0 || op.replacements.length > MAX_CONTEXT_PARAGRAPHS) {
          throw new Error("문서 편집 문단 개수가 허용 범위를 벗어났습니다.");
        }
        for (const rep of op.replacements) {
          const idx = Number(rep?.index);
          if (!Number.isInteger(idx) || idx < 0) continue;
          const nextText = String(rep?.text ?? "");
          if (idx < existing.length) {
            if (docHtml?.replaceParagraphContentWithTrackChanges) {
              docHtml.replaceParagraphContentWithTrackChanges(document, editor, existing[idx], nextText);
            } else {
              existing[idx].textContent = nextText;
            }
          } else {
            const added = buildDocBlockElement(document, nextText);
            if (added) {
              if (tracking) docHtml.wrapBlockInserted(document, added, docHtml.nextRevisionId(editor), new Date().toISOString());
              editor.appendChild(added);
            }
          }
          locations.push({ module: "DOC", label: "문단 " + (idx + 1), paragraphIndex: idx });
        }
        return true;
      }

      const items = Array.isArray(op.blocks)
        ? op.blocks
        : Array.isArray(op.paragraphs)
          ? op.paragraphs
          : op.text !== undefined
            ? [String(op.text)]
            : [];
      if (items.length === 0 || items.length > MAX_CONTEXT_PARAGRAPHS) {
        throw new Error("문서 편집 문단 개수가 허용 범위를 벗어났습니다.");
      }

      if (kind === "doc.replace_paragraphs") {
        const start = Number.isInteger(op.startIndex) ? Math.max(0, op.startIndex) : 0;
        const end = Number.isInteger(op.endIndex) ? Math.min(existing.length, op.endIndex + 1) : existing.length;
        const anchor = existing[end] || null;
        if (tracking) {
          const revId = docHtml.nextRevisionId(editor);
          const dateIso = new Date().toISOString();
          for (let i = start; i < end; i += 1) {
            if (existing[i]) docHtml.markBlockDeletedInPlace(document, existing[i], revId, dateIso);
          }
          items.forEach((item, offset) => {
            const el = buildDocBlockElement(document, item);
            if (el) {
              docHtml.wrapBlockInserted(document, el, revId + 1, dateIso);
              editor.insertBefore(el, anchor);
            }
            const idx = start + offset;
            locations.push({ module: "DOC", label: "문단 " + (idx + 1), paragraphIndex: idx });
          });
        } else {
          for (let i = start; i < end; i += 1) existing[i]?.remove();
          items.forEach((item, offset) => {
            const el = buildDocBlockElement(document, item);
            if (el) editor.insertBefore(el, anchor);
            const idx = start + offset;
            locations.push({ module: "DOC", label: "문단 " + (idx + 1), paragraphIndex: idx });
          });
        }
        return true;
      }

      if (!tracking && existing.length === 1 && String(existing[0].textContent || "").trim() === "") {
        existing[0].remove();
      }
      const baseIdx = docBlocksOf(editor).length;
      const appendRevId = tracking ? docHtml.nextRevisionId(editor) : 0;
      const appendDate = tracking ? new Date().toISOString() : "";
      items.forEach((item, offset) => {
        const el = buildDocBlockElement(document, item);
        if (el) {
          if (tracking) docHtml.wrapBlockInserted(document, el, appendRevId, appendDate);
          editor.appendChild(el);
        }
        const idx = baseIdx + offset;
        locations.push({ module: "DOC", label: "문단 " + (idx + 1), paragraphIndex: idx });
      });
      return true;
    }

    function executeSlideOperation(kind, op, locations) {
      const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
      const aspect = String(slideDeckState[0]?.aspect || "16:9");
      const normEls = (list) =>
        Array.isArray(list) && codec
          ? list.slice(0, 64).map((e, i) => codec.normalizeSlideElement(e, aspect, "el-" + (i + 1))).filter(Boolean)
          : undefined;
      const bulletBody = Array.isArray(op.bullets) ? op.bullets.map((b) => String(b ?? "")).join("\n") : undefined;
      if (kind === "slide.add") {
        if (slideDeckState.length >= MAX_SLIDES) {
          throw new Error("슬라이드 최대 개수를 초과했습니다.");
        }
        const empty = host.__EASYOFFICE_RIBBON_CATALOG__.createEmptySlideStyleFields();
        const base = slideDeckState[getActiveSlideIndex()] || slideDeckState[0];
        const nextLayout = op.layout
          ? (codec ? codec.normalizeLayout(op.layout) : String(op.layout))
          : (base?.layout && base.layout !== "titleSlide" ? base.layout : "titleAndContent");
        slideDeckState.push({
          title: String(op.title || "새 슬라이드"),
          lower: bulletBody !== undefined ? bulletBody : String(op.body ?? op.lower ?? ""),
          notes: String(op.notes || ""),
          transition: op.transition ? (codec ? codec.normalizeTransition(op.transition) : String(op.transition)) : "none",
          ...empty,
          aspect,
          layout: nextLayout,
          elements: normEls(op.elements) || [],
        });
        const idx = slideDeckState.length - 1;
        locations.push({ module: "SLIDE", label: "슬라이드 " + (idx + 1), slideIndex: idx });
        return idx;
      }
      const rawIdx = Number.isInteger(op.index)
        ? op.index
        : Number.isInteger(op.slideIndex)
          ? op.slideIndex
          : getActiveSlideIndex();
      if (rawIdx < 0 || rawIdx >= slideDeckState.length) {
        throw new Error("유효하지 않은 슬라이드 번호입니다.");
      }
      const target = slideDeckState[rawIdx];
      if (!Array.isArray(target.elements)) target.elements = [];
      if (kind === "slide.add_element") {
        const elSpec = op.element && typeof op.element === "object" ? op.element : op;
        const created = codec?.normalizeSlideElement?.(elSpec, aspect, "el-" + (target.elements.length + 1));
        if (!created) throw new Error("유효하지 않은 슬라이드 개체 항목입니다.");
        if (target.elements.length >= 256) throw new Error("슬라이드당 개체 수 한도를 초과했습니다.");
        target.elements.push(created);
        locations.push({ module: "SLIDE", label: "슬라이드 " + (rawIdx + 1), slideIndex: rawIdx });
        return rawIdx;
      }
      if (op.title !== undefined) target.title = String(op.title);
      if (bulletBody !== undefined) target.lower = bulletBody;
      else if (op.body !== undefined || op.lower !== undefined) target.lower = String(op.body ?? op.lower ?? "");
      if (op.notes !== undefined) target.notes = String(op.notes);
      if (op.layout !== undefined && codec) target.layout = codec.normalizeLayout(op.layout);
      if (op.transition !== undefined && codec) target.transition = codec.normalizeTransition(op.transition);
      const replacedEls = normEls(op.elements);
      if (replacedEls !== undefined) target.elements = replacedEls;
      locations.push({ module: "SLIDE", label: "슬라이드 " + (rawIdx + 1), slideIndex: rawIdx });
      return rawIdx;
    }

    function normalizeOperations(mod, operations, frozenSelection) {
      if (!Array.isArray(operations)) return operations;
      if (mod === "SHEET" && frozenSelection?.sheet) {
        return operations.map((op) => {
          if (!op || typeof op !== "object") return op;
          const kind = String(op.op || op.type || op.kind || "").trim();
          const next = op.sheet ? op : { ...op, sheet: frozenSelection.sheet };
          const needsRange = kind === "sheet.format" || kind === "sheet.cond_format" || kind === "sheet.sort_filter"
            || (kind === "sheet.table" && (!next.action || next.action === "create") && !next.tableName);
          if (needsRange && !next.range && frozenSelection.range) {
            return { ...next, range: frozenSelection.range };
          }
          return next;
        });
      }
      if (mod !== "SLIDE") return operations;
      const active = getActiveSlideIndex();
      return operations.map((op) => {
        const kind = String(op?.op || op?.type || op?.kind || "").trim();
        const named = Number.isInteger(op?.index) || Number.isInteger(op?.slideIndex);
        return op && typeof op === "object" && !named && (kind === "slide.update" || kind === "slide.add_element") ? { ...op, index: active } : op;
      });
    }

    const canApply = (mod, mode, operations) =>
      mode === "edit" && mod !== "PDF" && Array.isArray(operations) && operations.length > 0;

    // `options.beforeSnap`: 연출이 시작되기 전에 뜬 스냅숏. 연출이 임시로 쓴 것은 이미 되돌린 뒤라 그 스냅숏이 되돌리기 한 단계의 앞 상태다.
    async function applyOperations(mod, mode, operations, turnId, options = {}) {
      if (!canApply(mod, mode, operations)) {
        return { applied: false, locations: [] };
      }
      if (operations.length > MAX_OPERATIONS) {
        throw new Error("코파일럿 작업 개수가 허용 상한(64개)을 초과했습니다.");
      }

      if (mod === "HWP" && extOps.getEmbedHwpHost()) {
        const hwpRes = await extOps.applyHwpOperations(operations);
        noteActivity?.();
        lastUndoEntry = { turnId, hwpEmbed: true, afterGeneration: hwpRes.afterDocumentGeneration };
        return { applied: hwpRes.applied, locations: hwpRes.locations };
      }

      if (mod === "SLIDE") getWorkspacePanels()?.syncActiveSlideDomStylesToDeck?.();
      const beforeSnap = options.beforeSnap || captureWorkspaceSnapshot(false);
      const startEpoch = getHistoryMutationEpoch?.();
      const derivedLocations = [];
      let touchedSheet = null;
      const writtenFormulas = new Map();
      let touchedDoc = false;
      let focusSlideIdx = null;
      let touchedBaseOrBoard = false;

      // The operations apply as a whole: one that fails (an unknown kind, a protected cell, a range over the limit) puts back what
      // the ones before it changed, so a reply never leaves the document half edited.
      try {
        if (mod === "BASE") {
          touchedBaseOrBoard = extOps.applyBaseOperationsBatch(operations, derivedLocations);
        } else if (mod === "BOARD") {
          touchedBaseOrBoard = extOps.applyBoardOperationsBatch(operations, derivedLocations);
        } else {
          for (const op of operations) {
            if (!op || typeof op !== "object") throw new Error("유효하지 않은 코파일럿 작업 항목입니다.");
            const kind = String(op.op || op.type || op.kind || "").trim();
            if (kind === "sheet.write" || kind === "sheet.format" || kind === "sheet.chart" || sheetOps?.EXTENDED_SHEET_KINDS?.has(kind)) {
              const sheetName = String(op.sheet || getActiveSheetName());
              if (!Object.hasOwn(sheetDataStore, sheetName)) {
                if (sheetName !== getActiveSheetName()) throw new Error("없는 시트입니다: " + sheetName);
                sheetDataStore[sheetName] = Object.create(null);
              }
              touchedSheet = op.destSheet ? String(op.destSheet) : sheetName;
              if (kind === "sheet.write") {
                if (!writtenFormulas.has(sheetName)) writtenFormulas.set(sheetName, new Set());
                executeSheetWrite(op, sheetDataStore[sheetName], sheetName, derivedLocations, writtenFormulas.get(sheetName));
              }
              else if (kind === "sheet.format") executeSheetFormat(op, sheetDataStore[sheetName], sheetName, derivedLocations);
              else if (kind === "sheet.chart") executeSheetChart(op, sheetName, derivedLocations);
              else sheetOps.executeExtendedSheetOp(kind, op, { sheetName, sheetDataStore, sheetRowCounts, locations: derivedLocations });
            } else if (kind === "doc.append" || kind === "doc.replace_paragraphs" || kind === "doc.replace_span" || kind === "doc.scrub_pii") {
              touchedDoc = executeDocOperation(kind, op, derivedLocations) || touchedDoc;
            } else if (kind === "slide.add" || kind === "slide.update" || kind === "slide.add_element") {
              focusSlideIdx = executeSlideOperation(kind, op, derivedLocations);
            } else {
              throw new Error("지원하지 않는 코파일럿 작업입니다: " + kind);
            }
          }
        }

        if (touchedSheet) await flushSheetEvaluationAndFormats(touchedSheet, [...(writtenFormulas.get(touchedSheet) || [])]);
        if (touchedDoc) {
          host.__EASYOFFICE_DOC_NOTES_API__?.syncAll?.();
          const panels = getWorkspacePanels();
          panels?.saveActiveDocTabState?.();
          panels?.computeSmartStats?.();
          panels?.renderModuleOutline?.(mod);
        }
        if (focusSlideIdx !== null) {
          const panels = getWorkspacePanels();
          panels?.renderSlideDeck?.(focusSlideIdx);
          panels?.renderModuleOutline?.("SLIDE");
        }
        if (touchedBaseOrBoard) {
          getWorkspacePanels()?.renderModuleOutline?.(mod);
        }
      } catch (err) {
        const reason = String(err?.message || err);
        if (getHistoryMutationEpoch?.() !== startEpoch) {
          throw new Error(reason + " (그 사이 다른 편집이 있어 앞서 적용한 변경을 되돌리지 않았습니다)");
        }
        try {
          await restoreWorkspaceSnapshot(beforeSnap, undefined, { keepProtections: true });
        } catch (restoreErr) {
          throw new Error(reason + " (앞서 적용한 변경을 되돌리지 못했습니다: " + String(restoreErr?.message || restoreErr) + ")");
        }
        throw err;
      }

      if (!options.dryRun) {
        commitDeferredUndoSnapshot(beforeSnap);
        noteActivity?.();
        lastUndoEntry = { turnId, snapshot: beforeSnap, epoch: getHistoryMutationEpoch?.() };
      }
      return { applied: true, locations: derivedLocations };
    }

    async function dryRunVerifyOperations(mod, operations, invokeIpc) {
      if (typeof host.__EASYOFFICE_COPILOT_VERIFY_OPS__ !== "function") return { ok: true, issues: [] };
      return host.__EASYOFFICE_COPILOT_VERIFY_OPS__(mod, operations, {
        document,
        invokeIpc,
        slideDeckState,
        getActiveSlideIndex,
        sheetDataStore,
        sheetRowCounts,
        sheetOps,
        getActiveSheetName,
        getSheetController,
        docBlocksOf,
        buildDocBlockElement,
        executeSlideOperation,
        getHwpSnapshot: extOps.getHwpSnapshot,
      });
    }

    async function executeReadTool(mod, toolName, args) {
      if (toolName === "read_doc_blocks") return extOps.readDocBlocksTool(mod, args, docBlocksOf);
      if (toolName === "read_base_records") return extOps.readBaseRecordsTool(args);
      if (toolName === "read_sheet_range") return sheetOps.readSheetRangeTool(args, { sheetDataStore, getActiveSheetName, getSheetController });
      if (toolName === "find_sheet_cells") return sheetOps.findSheetCellsTool(args, { sheetDataStore, getActiveSheetName, getSheetController });
      if (toolName === "search_pdf_pages") return extOps.searchPdfPagesTool(args);
      throw new Error("알 수 없는 조회 도구입니다: " + toolName);
    }

    /**
     * 포인터가 편집할 곳으로 가서 선택하고 글을 한 자씩 쳐 넣는 연출을 한 뒤 연산을 적용한다. 적용은 연출이 끝난 다음 한 번이고 되돌리기도 한 단계다.
     * 연출을 멈추면(중지, 손을 댐) 쓰던 부분까지만 적용한다. 움직임 줄이기가 켜져 있으면 연출 없이 바로 적용한다.
     */
    async function applyOperationsAnimated(mod, mode, operations, turnId) {
      if (!canApply(mod, mode, operations)) return { applied: false, locations: [] };
      if (mod === "HWP" && extOps.getEmbedHwpHost()) {
        const applied = await applyOperations(mod, mode, operations, turnId);
        return { ...applied, interrupted: false };
      }
      const beforeSnap = captureWorkspaceSnapshot(false);
      const run = performer.perform(mod, operations);
      activeRun = run;
      let result;
      try {
        result = await run.done;
      } finally {
        endRun();
      }
      if (result.discarded) return { applied: false, locations: [], interrupted: true, discarded: true };
      if (result.operations.length === 0) return { applied: false, locations: [], interrupted: result.interrupted };
      const applied = await applyOperations(mod, mode, result.operations, turnId, { beforeSnap });
      return { ...applied, interrupted: result.interrupted };
    }

    // A restore would drop whatever the user did after the turn, so the undo is gone once the history moved.
    function canUndoTurn(turnId) {
      if (!lastUndoEntry || lastUndoEntry.turnId !== turnId) return false;
      if (lastUndoEntry.hwpEmbed) {
        return extOps.canUndoHwpSync
          ? extOps.canUndoHwpSync(lastUndoEntry.afterGeneration)
          : Boolean(extOps.getEmbedHwpHost());
      }
      return getHistoryMutationEpoch?.() === lastUndoEntry.epoch;
    }

    // Whether this turn still has an undo entry, usable or not (it stops being usable once the user edited).
    function hasUndoEntry(turnId) {
      return Boolean(lastUndoEntry && lastUndoEntry.turnId === turnId);
    }

    async function undoLastTurn(turnId) {
      if (!lastUndoEntry || (turnId !== undefined && lastUndoEntry.turnId !== turnId)) return false;
      if (lastUndoEntry.hwpEmbed) {
        const gen = lastUndoEntry.afterGeneration;
        const ok = await extOps.undoHwpTurn(gen);
        if (ok) {
          lastUndoEntry = null;
          noteActivity?.();
        }
        return ok;
      }
      if (getHistoryMutationEpoch?.() !== lastUndoEntry.epoch) return false;
      const snap = lastUndoEntry.snapshot;
      lastUndoEntry = null;
      await restoreWorkspaceSnapshot(snap, undefined, { keepProtections: true });
      host.__EASYOFFICE_DOC_NOTES_API__?.syncAll?.();
      noteActivity?.();
      return true;
    }

    async function verifyFingerprintAsync(mod, operations, expectedFp) {
      if (mod === "HWP" && extOps.getEmbedHwpHost()) {
        return extOps.verifyHwpFingerprintAsync(expectedFp);
      }
      return preview.fingerprint(mod, operations) === expectedFp;
    }

    const locateTarget = (loc) => extOps.locateTarget?.(loc, { docBlocksOf }) ?? Promise.resolve(false);

    return {
      getSelectionLabel,
      freezeSheetSelection,
      extractContext,
      extractContextAsync,
      refreshHwpSnapshot: extOps.refreshHwpSnapshot,
      getCommandTarget,
      snapshotTarget,
      applyOperations,
      applyOperationsAnimated,
      dryRunVerifyOperations,
      executeReadTool,
      verifyFingerprintAsync,
      stopPerformance: () => activeRun?.stop(),
      // 쓰던 부분도 적용하지 않고 버린다(새 대화).
      abortPerformance: () => activeRun?.discard(),
      whenIdle,
      // 제안 때 활성 슬라이드를 연산에 고정한다(수락 전에 다른 슬라이드로 옮겨도 제안한 슬라이드에 적용한다).
      normalizeOperations,
      isPerforming: () => activeRun !== null,
      buildChanges: preview.buildChanges,
      fingerprint: preview.fingerprint,
      renderChangeList: preview.renderList,
      showDocPreview: preview.showDocOverlays,
      clearPreview: preview.clear,
      canUndoTurn,
      hasUndoEntry,
      undoLastTurn,
      locateTarget,
      clearUndo: () => {
        lastUndoEntry = null;
      },
    };
  }

  host.__EASYOFFICE_CREATE_COPILOT_ACTIONS__ = createCopilotActions;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
