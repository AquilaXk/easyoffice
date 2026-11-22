// 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., see THIRD_PARTY_NOTICES.md)

(function (host) {
  "use strict";

  const MAX_CONTEXT_CELLS = 512;
  // The engine puts this many paragraphs in the prompt (`MAX_PROMPT_DOC_PARAGRAPHS`) and answers for the same limit.
  const MAX_CONTEXT_PARAGRAPHS = 200;
  // A selection command on a later block is sent with empty entries up to it (the engine reads 2,048 at most).
  const MAX_COMMAND_BLOCK_INDEX = 2047;
  const MAX_CONTEXT_SLIDES = 64;
  const MAX_CONTEXT_TEXT_CHARS = 24000;
  // A paragraph a selection command works on is sent whole up to one code point past the engine's 24 KiB context
  // (`MAX_PROMPT_CONTEXT_BYTES`): a longer one is cut here, stays over the limit, and the engine refuses the command.
  const MAX_COMMAND_SPAN_CHARS = 24 * 1024 + 1;
  const MAX_OPERATIONS = 64;
  const MAX_WRITE_CELLS = 1024;
  const MAX_RANGE_CELLS = 4096;
  const ALLOWED_BLOCK_TAGS = new Set(["p", "h1", "h2", "h3", "h4", "blockquote"]);

  function compareCoordEntries(a, b) {
    return a.row - b.row || a.col - b.col;
  }

  function formatSelectionA1(rg, colName) {
    if (!rg || !Number.isInteger(rg.r1) || !Number.isInteger(rg.c1)) return "A1";
    const start = colName(rg.c1) + rg.r1;
    if (rg.r1 === rg.r2 && rg.c1 === rg.c2) return start;
    return start + ":" + colName(rg.c2) + rg.r2;
  }

  function docBlocksOf(editor) {
    if (!editor) return [];
    return Array.from(editor.querySelectorAll("p, h1, h2, h3, h4, ul, ol, blockquote"))
      .filter((el) => el.parentElement === editor && !el.hidden && !el.classList.contains("manuscript-notice"));
  }

  function buildDocBlockElement(document, item) {
    if (typeof item === "string") {
      const p = document.createElement("p");
      p.textContent = item;
      return p;
    }
    if (!item || typeof item !== "object") return null;
    if (Number.isInteger(item.heading) && item.heading >= 1) {
      const h = document.createElement("h" + Math.min(4, item.heading));
      h.textContent = String(item.text ?? item.content ?? "");
      return h;
    }
    const listKind = String(item.list || "").toLowerCase();
    const rawKind = String(item.tag || item.kind || "p").toLowerCase();
    if (listKind || rawKind === "ul" || rawKind === "ol" || rawKind === "task-list" || Array.isArray(item.items)) {
      const isOl = listKind === "ordered" || rawKind === "ol";
      const isTask = listKind === "task" || rawKind === "task-list" || item.task === true;
      const list = document.createElement(isOl ? "ol" : "ul");
      if (isTask && !isOl) {
        list.className = "eo-task-list";
        list.dataset.eoList = "task";
      }
      const entries = Array.isArray(item.items) ? item.items.slice(0, MAX_CONTEXT_PARAGRAPHS) : [String(item.text || "")];
      for (const entry of entries) {
        const li = document.createElement("li");
        if (isTask && !isOl) {
          const checked = typeof entry === "object" && entry !== null ? Boolean(entry.checked) : false;
          li.dataset.eoChecked = checked ? "true" : "false";
          li.textContent = typeof entry === "object" && entry !== null ? String(entry.text || "") : String(entry ?? "");
        } else {
          li.textContent = typeof entry === "object" && entry !== null ? String(entry.text || "") : String(entry ?? "");
        }
        list.appendChild(li);
      }
      return list;
    }
    const tag = ALLOWED_BLOCK_TAGS.has(rawKind) ? rawKind : "p";
    const el = document.createElement(tag);
    el.textContent = String(item.text ?? item.content ?? "");
    return el;
  }

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
    const preview = host.__EASYOFFICE_CREATE_COPILOT_PREVIEW__({ document, win: host, docBlocksOf, slideDeckState, sheetDataStore, getActiveSheetName });
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
      if (mod === "DOC") {
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

    function extractSheetContext() {
      const sc = getSheetController();
      const sheetName = getActiveSheetName();
      if (sc) sheetDataStore[sheetName] = sc.snapshotDomSheetCells();
      const map = sheetDataStore[sheetName] || {};
      const parsed = [];
      for (const coord of Object.keys(map)) {
        const rec = map[coord];
        if (!rec) continue;
        const raw = rec.formula ? String(rec.formula) : String(rec.value ?? "");
        const value = String(rec.value ?? "");
        if (!raw && !value) continue;
        const parts = parseRef(coord);
        if (!parts) continue;
        parsed.push({ coord: refOf(parts.row, parts.col), row: parts.row, col: parts.col, raw, value });
      }
      parsed.sort(compareCoordEntries);
      const cells = parsed.slice(0, MAX_CONTEXT_CELLS).map(({ coord, raw, value }) => ({ coord, raw, value }));
      return { name: sheetName, cells };
    }

    // One entry per block, empty ones included, so that an index in the context is the block index an operation
    // names. A command target past the window is sent too (its neighbours stay empty); `omittedParagraphs` counts
    // the blocks that were left out.
    function extractDocContext(span) {
      const editor = document.getElementById("doc-editor");
      const blocks = docBlocksOf(editor);
      const lastSent = span ? Math.max(MAX_CONTEXT_PARAGRAPHS - 1, span.end) : MAX_CONTEXT_PARAGRAPHS - 1;
      const count = Math.min(blocks.length, lastSent + 1);
      const paragraphs = [];
      for (let i = 0; i < count; i += 1) {
        const inSpan = Boolean(span) && i >= span.start && i <= span.end;
        const limit = inSpan ? MAX_COMMAND_SPAN_CHARS : MAX_CONTEXT_TEXT_CHARS;
        paragraphs.push(inSpan || i < MAX_CONTEXT_PARAGRAPHS ? clipCodePoints(String(blocks[i].textContent || "").trim(), limit) : "");
      }
      let selectedText = "";
      const sel = host.getSelection?.();
      if (sel && sel.rangeCount > 0 && !sel.isCollapsed && editor?.contains(sel.getRangeAt(0).commonAncestorContainer)) {
        selectedText = clipCodePoints(sel.getRangeAt(0).toString?.() || "", MAX_CONTEXT_TEXT_CHARS);
      }
      return { paragraphs, selectedText, omittedParagraphs: blocks.length - count };
    }

    function blockIndexOf(node, editor) {
      let cur = node && node.nodeType === 1 ? node : node?.parentElement;
      while (cur && cur.parentElement !== editor) cur = cur.parentElement;
      return cur ? docBlocksOf(editor).indexOf(cur) : -1;
    }

    // Where a selection command works: the blocks a text selection in the document touches, or the slide title or
    // body a selection sits in. `null` when there is nothing selected there.
    function getCommandTarget(mod = getActiveModule()) {
      const sel = host.getSelection?.();
      if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return null;
      const range = sel.getRangeAt(0);
      if (mod === "DOC") {
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
      if (mod === "DOC") {
        const blocks = docBlocksOf(document.getElementById("doc-editor"));
        const texts = [];
        for (let i = sel.paragraphIndex; i <= sel.paragraphEnd; i += 1) {
          if (!blocks[i]) return null;
          texts.push(String(blocks[i].textContent || "").trim());
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
        const body = String(s?.lower || "");
        const bullets = body.split(/\n+/).map((t) => t.trim()).filter(Boolean);
        const elements = (Array.isArray(s?.elements) ? s.elements : []).slice(0, 32).map((el) => ({
          id: String(el.id || ""),
          kind: String(el.kind || ""),
          x: Number(el.x || 0),
          y: Number(el.y || 0),
          w: Number(el.w || 0),
          h: Number(el.h || 0),
          text: clipCodePoints(el.text || el.title || el.alt || "", 160),
        }));
        return {
          index,
          title: String(s?.title || ""),
          bullets,
          body,
          notes: String(s?.notes || ""),
          layout: String(s?.layout || "titleAndContent"),
          elements,
        };
      });
      return { activeIndex: getActiveSlideIndex(), aspect: String(slideDeckState[0]?.aspect || "16:9"), slides };
    }

    function extractPdfContext() {
      const rawText = clipCodePoints(
        String(extractEditableDocumentText?.("PDF") || document.getElementById("pdf-page-card")?.textContent || "").trim(),
        MAX_CONTEXT_TEXT_CHARS,
      );
      const pageCount = Number(getPdfEdit?.()?.pageCount?.() || 1) || 1;
      return { text: rawText, pageCount };
    }

    function extractContext(mod = getActiveModule(), { target } = {}) {
      const label = getSelectionLabel(mod);
      if (mod === "SHEET") {
        const sheet = getActiveSheetName();
        return { selection: { sheet, range: label || "A1" }, context: { sheet: extractSheetContext() } };
      }
      if (mod === "DOC") {
        const sel = target?.selection;
        const span = sel ? { start: sel.paragraphIndex, end: sel.paragraphEnd } : null;
        return { selection: sel ? { ...sel } : label ? { range: label } : {}, context: { doc: extractDocContext(span) } };
      }
      if (mod === "SLIDE") {
        return { selection: target?.selection ? { ...target.selection } : { slideIndex: getActiveSlideIndex() }, context: { slide: extractSlideContext() } };
      }
      if (mod === "BASE" || mod === "BOARD") return host.__EASYOFFICE_BASE_BOARD_HOST__?.extractCopilotContext?.(mod) || { selection: {}, context: {} };
      if (mod === "PDF") return { selection: {}, context: { pdf: extractPdfContext() } };
      return { selection: {}, context: {} };
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

    function executeSheetWrite(op, map, sheetName, locations) {
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

    async function flushSheetEvaluationAndFormats(sheetName) {
      const sc = getSheetController();
      if (!sc) return;
      sc.restoreDomSheetCells(sheetName);
      await sc.recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
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
      sc.syncSheetFormatToolbar?.();
    }

    function executeDocOperation(kind, op, locations) {
      const editor = document.getElementById("doc-editor");
      if (!editor) throw new Error("문서 편집 영역을 찾을 수 없습니다.");
      if (kind === "doc.scrub_pii") {
        const blocks = docBlocksOf(editor);
        let changedCount = 0;
        blocks.forEach((blk, idx) => {
          const walk = (node) => {
            for (const child of node.childNodes) {
              if (child.nodeType === 3) {
                const res = localScrubPii(child.data);
                if (res && res.totalMasked > 0 && typeof res.scrubbedText === "string") {
                  child.data = res.scrubbedText;
                  changedCount += res.totalMasked;
                }
              } else if (child.nodeType === 1) {
                walk(child);
              }
            }
          };
          const before = blk.textContent;
          walk(blk);
          if (blk.textContent !== before) {
            locations.push({ module: "DOC", label: "문단 " + (idx + 1), paragraphIndex: idx });
          }
        });
        return changedCount > 0 || blocks.length > 0;
      }

      const existing = docBlocksOf(editor);
      if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
        if (op.replacements.length === 0 || op.replacements.length > MAX_CONTEXT_PARAGRAPHS) {
          throw new Error("문서 편집 문단 개수가 허용 범위를 벗어났습니다.");
        }
        for (const rep of op.replacements) {
          const idx = Number(rep?.index);
          if (!Number.isInteger(idx) || idx < 0) continue;
          const nextText = String(rep?.text ?? "");
          if (idx < existing.length) {
            existing[idx].textContent = nextText;
          } else {
            const p = document.createElement("p");
            p.textContent = nextText;
            editor.appendChild(p);
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
        for (let i = start; i < end; i += 1) existing[i]?.remove();
        items.forEach((item, offset) => {
          const el = buildDocBlockElement(document, item);
          if (el) editor.insertBefore(el, anchor);
          const idx = start + offset;
          locations.push({ module: "DOC", label: "문단 " + (idx + 1), paragraphIndex: idx });
        });
        return true;
      }

      if (existing.length === 1 && String(existing[0].textContent || "").trim() === "") {
        existing[0].remove();
      }
      const baseIdx = docBlocksOf(editor).length;
      items.forEach((item, offset) => {
        const el = buildDocBlockElement(document, item);
        if (el) editor.appendChild(el);
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
        if (slideDeckState.length >= MAX_CONTEXT_SLIDES) {
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

    function normalizeOperations(mod, operations) {
      if (mod !== "SLIDE" || !Array.isArray(operations)) return operations;
      const active = getActiveSlideIndex();
      return operations.map((op) => {
        const kind = String(op?.op || op?.type || op?.kind || "").trim();
        const named = Number.isInteger(op?.index) || Number.isInteger(op?.slideIndex);
        return op && typeof op === "object" && !named && (kind === "slide.update" || kind === "slide.add_element") ? { ...op, index: active } : op;
      });
    }

    const canApply = (mod, mode, operations) =>
      mode === "edit" && mod !== "PDF" && mod !== "BASE" && mod !== "BOARD" && Array.isArray(operations) && operations.length > 0;

    // `options.beforeSnap`: 연출이 시작되기 전에 뜬 스냅숏. 연출이 임시로 쓴 것은 이미 되돌린 뒤라 그 스냅숏이 되돌리기 한 단계의 앞 상태다.
    async function applyOperations(mod, mode, operations, turnId, options = {}) {
      if (!canApply(mod, mode, operations)) {
        return { applied: false, locations: [] };
      }
      if (operations.length > MAX_OPERATIONS) {
        throw new Error("코파일럿 작업 개수가 허용 상한(64개)을 초과했습니다.");
      }
      if (mod === "SLIDE") getWorkspacePanels()?.syncActiveSlideDomStylesToDeck?.();
      const beforeSnap = options.beforeSnap || captureWorkspaceSnapshot(false);
      const startEpoch = getHistoryMutationEpoch?.();
      const derivedLocations = [];
      let touchedSheet = null;
      let touchedDoc = false;
      let focusSlideIdx = null;

      // The operations apply as a whole: one that fails (an unknown kind, a protected cell, a range over the limit) puts back what
      // the ones before it changed, so a reply never leaves the document half edited.
      try {
        for (const op of operations) {
          if (!op || typeof op !== "object") throw new Error("유효하지 않은 코파일럿 작업 항목입니다.");
          const kind = String(op.op || op.type || op.kind || "").trim();
          if (kind === "sheet.write" || kind === "sheet.format" || kind === "sheet.chart") {
            const sheetName = String(op.sheet || getActiveSheetName());
            if (!Object.hasOwn(sheetDataStore, sheetName)) {
              // The active sheet may not have a store entry yet; a name the model made up must not create one.
              if (sheetName !== getActiveSheetName()) throw new Error("없는 시트입니다: " + sheetName);
              sheetDataStore[sheetName] = Object.create(null);
            }
            touchedSheet = sheetName;
            if (kind === "sheet.write") executeSheetWrite(op, sheetDataStore[sheetName], sheetName, derivedLocations);
            else if (kind === "sheet.format") executeSheetFormat(op, sheetDataStore[sheetName], sheetName, derivedLocations);
            else executeSheetChart(op, sheetName, derivedLocations);
          } else if (kind === "doc.append" || kind === "doc.replace_paragraphs" || kind === "doc.scrub_pii") {
            touchedDoc = executeDocOperation(kind, op, derivedLocations) || touchedDoc;
          } else if (kind === "slide.add" || kind === "slide.update" || kind === "slide.add_element") {
            focusSlideIdx = executeSlideOperation(kind, op, derivedLocations);
          } else {
            throw new Error("지원하지 않는 코파일럿 작업입니다: " + kind);
          }
        }

        if (touchedSheet) await flushSheetEvaluationAndFormats(touchedSheet);
        if (touchedDoc) {
          const panels = getWorkspacePanels();
          panels?.saveActiveDocTabState?.();
          panels?.computeSmartStats?.();
          panels?.renderModuleOutline?.("DOC");
        }
        if (focusSlideIdx !== null) {
          const panels = getWorkspacePanels();
          panels?.renderSlideDeck?.(focusSlideIdx);
          panels?.renderModuleOutline?.("SLIDE");
        }
      } catch (err) {
        const reason = String(err?.message || err);
        // The history moved while the operations ran (the user edited): a restore would drop those edits too.
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

      commitDeferredUndoSnapshot(beforeSnap);
      noteActivity?.();
      lastUndoEntry = { turnId, snapshot: beforeSnap, epoch: getHistoryMutationEpoch?.() };
      return { applied: true, locations: derivedLocations };
    }

    /**
     * 포인터가 편집할 곳으로 가서 선택하고 글을 한 자씩 쳐 넣는 연출을 한 뒤 연산을 적용한다. 적용은 연출이 끝난 다음 한 번이고 되돌리기도 한 단계다.
     * 연출을 멈추면(중지, 손을 댐) 쓰던 부분까지만 적용한다. 움직임 줄이기가 켜져 있으면 연출 없이 바로 적용한다.
     */
    async function applyOperationsAnimated(mod, mode, operations, turnId) {
      if (!canApply(mod, mode, operations)) return { applied: false, locations: [] };
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
      return Boolean(lastUndoEntry && lastUndoEntry.turnId === turnId && getHistoryMutationEpoch?.() === lastUndoEntry.epoch);
    }

    // Whether this turn still has an undo entry, usable or not (it stops being usable once the user edited).
    function hasUndoEntry(turnId) {
      return Boolean(lastUndoEntry && lastUndoEntry.turnId === turnId);
    }

    async function undoLastTurn(turnId) {
      if (!lastUndoEntry || (turnId !== undefined && lastUndoEntry.turnId !== turnId)) return false;
      if (getHistoryMutationEpoch?.() !== lastUndoEntry.epoch) return false;
      const snap = lastUndoEntry.snapshot;
      lastUndoEntry = null;
      await restoreWorkspaceSnapshot(snap, undefined, { keepProtections: true });
      noteActivity?.();
      return true;
    }

    async function locateTarget(loc) {
      if (!loc || typeof loc !== "object") return false;
      const mod = String(loc.module || getActiveModule()).toUpperCase();
      if (mod !== getActiveModule() && typeof activatePane === "function") {
        await activatePane(mod);
      }
      if (mod === "SHEET") {
        const sc = getSheetController();
        const sheetName = String(loc.sheet || getActiveSheetName());
        if (sheetName && sheetName !== getActiveSheetName() && sheetDataStore[sheetName]) {
          await sc?.switchSheetTab?.(sheetName);
        }
        const rangeText = String(loc.range || loc.label || "").trim().toUpperCase();
        const names = host.__EASYOFFICE_SHEET_NAMES__;
        const nameBox = document.getElementById("active-cell-ref");
        if (rangeText && names?.parseAddress(rangeText) && nameBox) {
          nameBox.textContent = rangeText;
          const ev = typeof host.KeyboardEvent === "function"
            ? new host.KeyboardEvent("keydown", { key: "Enter", bubbles: true })
            : { type: "keydown", key: "Enter", target: nameBox };
          nameBox.dispatchEvent(ev);
          return true;
        }
        return false;
      }
      if (mod === "DOC") {
        const editor = document.getElementById("doc-editor");
        const blocks = docBlocksOf(editor);
        const idx = Number.isInteger(loc.paragraphIndex) ? loc.paragraphIndex : 0;
        const target = blocks[Math.max(0, Math.min(blocks.length - 1, idx))];
        if (!target) return false;
        target.scrollIntoView?.({ block: "center" });
        editor.focus?.();
        const sel = host.getSelection?.();
        if (sel && document.createRange) {
          const range = document.createRange();
          range.setStart(target, 0);
          range.setEnd(target, 0);
          sel.removeAllRanges?.();
          sel.addRange?.(range);
        }
        return true;
      }
      if (mod === "SLIDE") {
        const idx = Number.isInteger(loc.slideIndex)
          ? loc.slideIndex
          : Math.max(0, (Number.parseInt(String(loc.label || "").replace(/\D+/g, ""), 10) || 1) - 1);
        if (idx < 0 || idx >= slideDeckState.length) return false;
        getWorkspacePanels()?.renderSlideDeck?.(idx);
        document.getElementById("slide-thumb-list")?.querySelector(`[data-slide-idx="${idx}"]`)?.focus?.();
        return true;
      }
      return false;
    }

    return {
      getSelectionLabel,
      extractContext,
      getCommandTarget,
      snapshotTarget,
      applyOperations,
      applyOperationsAnimated,
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
