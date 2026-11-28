(function (host) {
  "use strict";

  const SYMBOL_CATEGORIES = [
    { id: "general", label: "기호", glyphs: Array.from("※○●◎◇◆□■△▲▽▼☆★♠♣♥♦♪♫☎☏☑☐☒☞☜☝☟⌘⌥⇧⌃✓✔✕✗") },
    { id: "punct", label: "문장부호", glyphs: Array.from("…·‥―—–「」『』〈〉《》【】〔〕〃々ㆍ˚′″§¶†‡‰") },
    { id: "arrow", label: "화살표", glyphs: Array.from("→←↑↓↔↕↗↘↙↖⇒⇐⇑⇓⇔➔➜➤➥➦") },
    { id: "math", label: "수학", glyphs: Array.from("±×÷≠≈≡≤≥∞√∑∏∫∂∆∇∈∉⊂⊃∪∩∀∃∴∵∠⊥∥") },
    { id: "unit", label: "단위", glyphs: Array.from("℃℉₩￥€£¢㎡㎥㎞㎝㎜㎏㎎㎖ℓ㏊㎐㎑㎒㎓％ÅΩμ") },
    { id: "enclosed", label: "원문자", glyphs: Array.from("①②③④⑤⑥⑦⑧⑨⑩ⓐⓑⓒⓓⓔⓕⓖⓗⓘⓙ㉠㉡㉢㉣㉤㉥㉦㉧㉨㉩") },
  ];

  // The symbol dialog of the Word program has two tabs: 기호 and 특수 문자.
  const glyphsOf = (...ids) => SYMBOL_CATEGORIES.filter((c) => ids.includes(c.id)).flatMap((c) => c.glyphs);
  const WORD_CATEGORIES = [
    { id: "symbols", label: "기호", glyphs: glyphsOf("general", "arrow", "math", "unit", "enclosed") },
    { id: "special", label: "특수 문자", glyphs: glyphsOf("punct") },
  ];

  const DEFAULT_CHAR = "※";
  const GRID_COLS = 10;

  function createDocSymbolDialog(deps) {
    const {
      document,
      getActiveModule,
      getSheetController,
      getDocRuns,
      getDocPages,
      getSlideDeckCount,
      extractEditableDocumentText,
      resolveActiveDocBlock,
      insertTextAtSelection,
      withDocStyleUndo,
      computeSmartStats,
      syncActiveSlideDomStylesToDeck,
      openToolDialog,
      closeDialog,
      setFeedback,
    } = deps;

    let selectedChar = DEFAULT_CHAR;
    let activeCatId = "general";
    let savedTarget = null;
    let bound = false;

    const $ = (id) => document.getElementById(id);
    const categories = () => (isWordDoc(getActiveModule()) ? WORD_CATEGORIES : SYMBOL_CATEGORIES);

    function formatCodePoint(ch) {
      const cp = String(ch || DEFAULT_CHAR).codePointAt(0) || 0;
      return "U+" + cp.toString(16).toUpperCase().padStart(4, "0");
    }

    function updatePreview() {
      const charEl = $("symbol-picker-preview-char");
      const codeEl = $("symbol-picker-preview-code");
      if (charEl) charEl.textContent = selectedChar;
      if (codeEl) codeEl.textContent = formatCodePoint(selectedChar);
    }

    function selectGlyph(ch) {
      selectedChar = ch || DEFAULT_CHAR;
      const grid = $("symbol-picker-grid");
      if (grid) {
        grid.querySelectorAll(".symbol-cell").forEach((btn) => {
          const hit = btn.dataset.symbolChar === selectedChar;
          btn.classList.toggle("is-selected", hit);
          btn.setAttribute("aria-selected", hit ? "true" : "false");
        });
      }
      updatePreview();
    }

    function renderGrid() {
      const grid = $("symbol-picker-grid");
      if (!grid) return;
      const cats = categories();
      const cat = cats.find((c) => c.id === activeCatId) || cats[0];
      const buttons = cat.glyphs.map((ch, idx) => {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "symbol-cell" + (ch === selectedChar ? " is-selected" : "");
        btn.dataset.symbolChar = ch;
        btn.dataset.symbolIdx = String(idx);
        btn.setAttribute("role", "option");
        btn.setAttribute("aria-selected", ch === selectedChar ? "true" : "false");
        btn.textContent = ch;
        btn.addEventListener("click", () => selectGlyph(ch));
        btn.addEventListener("dblclick", () => {
          selectGlyph(ch);
          commitInsert();
        });
        btn.addEventListener("keydown", (ev) => {
          if (ev.key === "Enter") {
            ev.preventDefault?.();
            ev.stopPropagation?.();
            selectGlyph(ch);
            commitInsert();
            return;
          }
          const delta = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: GRID_COLS, ArrowUp: -GRID_COLS }[ev.key];
          if (!delta) return;
          ev.preventDefault?.();
          const cells = Array.from(grid.querySelectorAll(".symbol-cell"));
          const next = cells[Math.max(0, Math.min(cells.length - 1, idx + delta))];
          if (next) {
            selectGlyph(next.dataset.symbolChar);
            next.focus?.();
          }
        });
        return btn;
      });
      grid.replaceChildren(...buttons);
    }

    function renderTabs() {
      const tabs = $("symbol-picker-tabs");
      if (!tabs) return;
      const els = categories().map((cat) => {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "symbol-cat-tab" + (cat.id === activeCatId ? " active" : "");
        btn.dataset.symCat = cat.id;
        btn.setAttribute("role", "tab");
        btn.setAttribute("aria-selected", cat.id === activeCatId ? "true" : "false");
        btn.textContent = cat.label;
        btn.addEventListener("click", () => {
          activeCatId = cat.id;
          const first = cat.glyphs[0] || DEFAULT_CHAR;
          if (!cat.glyphs.includes(selectedChar)) selectedChar = first;
          renderTabs();
          renderGrid();
          updatePreview();
        });
        return btn;
      });
      tabs.replaceChildren(...els);
    }

    function captureTargetContext() {
      const mod = getActiveModule();
      const docRuns = getDocRuns?.();
      if (mod === "DOC" || mod === "HWP") {
        const editor = $("doc-editor");
        const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
        const live = sel?.rangeCount > 0 ? sel.getRangeAt(0) : null;
        const r = live && editor?.contains?.(live.startContainer) ? live : docRuns?.currentRange?.();
        const cloned = r && editor?.contains?.(r.startContainer)
          ? (typeof r.cloneRange === "function" ? r.cloneRange() : r)
          : null;
        return { mod, range: cloned, block: resolveActiveDocBlock(editor) };
      }
      if (mod === "SHEET") {
        const sc = getSheetController();
        const sheet = sc?.captureSymbolTarget?.() || null;
        sc?.setSymbolDialogHold?.(true);
        return { mod: "SHEET", sheet };
      }
      if (mod === "SLIDE") {
        const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
        const rawRange = sel && sel.rangeCount > 0 ? sel.getRangeAt(0) : null;
        const pane = $("pane-slide");
        const range = rawRange && pane?.contains?.(rawRange.startContainer)
          ? (typeof rawRange.cloneRange === "function" ? rawRange.cloneRange() : { ...rawRange })
          : null;
        const slideCanvas = host.__EASYOFFICE_SLIDE_CANVAS__;
        return { mod: "SLIDE", range, slideElId: slideCanvas?.getSelectedElement?.()?.id || null };
      }
      return { mod };
    }

    function releaseSheetHold(restoreFocus) {
      if (savedTarget?.mod !== "SHEET") return;
      const sc = getSheetController();
      sc?.setSymbolDialogHold?.(false);
      if (restoreFocus) sc?.restoreSymbolHoldFocus?.(savedTarget.sheet);
    }

    function restoreSelectionRange(range) {
      const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
      if (!sel || !range) return false;
      let target = range;
      if (typeof range.insertNode !== "function" && typeof document.createRange === "function") {
        try {
          const domRange = document.createRange();
          domRange.setStart(range.startContainer, range.startOffset);
          domRange.setEnd(range.endContainer ?? range.startContainer, range.endOffset ?? range.startOffset);
          target = domRange;
        } catch {
          target = range;
        }
      }
      sel.removeAllRanges?.();
      sel.addRange?.(target);
      return true;
    }

    function spliceTextRangeFallback(range, ch) {
      const node = range?.startContainer;
      if (!node || node.nodeType !== 3) return false;
      const val = String(node.data ?? node.nodeValue ?? "");
      const start = Math.max(0, Math.min(val.length, Number(range.startOffset) || 0));
      const end = node === range.endContainer ? Math.max(start, Math.min(val.length, Number(range.endOffset) || start)) : start;
      node.data = val.slice(0, start) + ch + val.slice(end);
      if (typeof range.setStart === "function") range.setStart(node, start + ch.length);
      if (typeof range.setEnd === "function") range.setEnd(node, start + ch.length);
      return true;
    }

    function insertIntoDoc(ch, ctx) {
      const editor = $("doc-editor");
      const target = ctx.block || resolveActiveDocBlock(editor);
      withDocStyleUndo(() => {
        if (ctx.range && editor?.contains?.(ctx.range.startContainer)) restoreSelectionRange(ctx.range);
        if (!insertTextAtSelection(ch)) {
          if (!spliceTextRangeFallback(ctx.range, ch) && target) {
            const kids = target.childNodes || [];
            const last = kids[kids.length - 1];
            const container = last?.classList?.contains?.("eo-run") ? last : target;
            container.appendChild(document.createTextNode(ch));
          }
        }
        getDocRuns?.()?.refreshToolbarFromSelection?.();
        computeSmartStats();
      });
      setFeedback?.((ctx.mod === "HWP" ? "문자표 기호 삽입 완료 (" : "기호 삽입 완료 (") + ch + ")");
    }

    function insertIntoSlide(ch, ctx) {
      const slideCanvas = host.__EASYOFFICE_SLIDE_CANVAS__;
      const pane = $("pane-slide");
      if (ctx.range && pane?.contains?.(ctx.range.startContainer)) {
        restoreSelectionRange(ctx.range);
        if (insertTextAtSelection(ch)) {
          syncActiveSlideDomStylesToDeck();
          setFeedback?.("기호 삽입 완료 (" + ch + ")");
          return;
        }
      }
      if (ctx.slideElId && slideCanvas) {
        const el = slideCanvas.getElements?.()?.find((item) => item.id === ctx.slideElId);
        if (el && (el.kind === "textbox" || el.kind === "shape")) {
          slideCanvas.updateElement(el.id, { text: String(el.text ?? "") + ch });
          setFeedback?.("기호 삽입 완료 (" + ch + ")");
          return;
        }
      }
      const zone = $("slide-lower-zone") || $("slide-title-zone");
      if (!zone) return;
      const target = zone.querySelector("p") || zone;
      target.appendChild(document.createTextNode(ch));
      syncActiveSlideDomStylesToDeck();
      setFeedback?.("기호 삽입 완료 (" + ch + ")");
    }

    function commitInsert() {
      const ch = selectedChar || DEFAULT_CHAR;
      const ctx = savedTarget || { mod: getActiveModule() };
      savedTarget = null;
      closeDialog?.("symbol-picker-dialog");
      if (ctx.mod === "DOC" || ctx.mod === "HWP") {
        insertIntoDoc(ch, ctx);
        return;
      }
      if (ctx.mod === "SHEET") {
        const sc = getSheetController();
        void Promise.resolve(sc?.insertSymbol?.(ch, ctx.sheet)).then((res) => {
          if (res?.inserted) setFeedback?.("기호 삽입 완료 (" + ch + ")");
        }).catch(() => {});
        return;
      }
      if (ctx.mod === "SLIDE") insertIntoSlide(ch, ctx);
    }

    function cancelInsert() {
      releaseSheetHold(true);
      savedTarget = null;
      closeDialog?.("symbol-picker-dialog");
    }

    function bind() {
      if (bound) return;
      bound = true;
      const dlg = $("symbol-picker-dialog");
      $("btn-symbol-insert")?.addEventListener("click", commitInsert);
      dlg?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => {
        btn.addEventListener("click", cancelInsert);
      });
      dlg?.addEventListener("close", () => {
        if (!savedTarget) return;
        releaseSheetHold(true);
        savedTarget = null;
      });
    }

    function openSymbolDialog() {
      bind();
      savedTarget = captureTargetContext();
      const cats = categories();
      if (!cats.some((c) => c.id === activeCatId)) activeCatId = cats[0].id;
      if (!cats.some((c) => c.glyphs.includes(selectedChar))) {
        activeCatId = cats[0].id;
        selectedChar = DEFAULT_CHAR;
      }
      renderTabs();
      renderGrid();
      updatePreview();
      openToolDialog("symbol-picker-dialog");
      $("btn-symbol-insert")?.focus?.();
    }

    // The rows of the Word program's 단어 개수 dialog, in its order.
    function wordCountRows(includeNotes) {
      const stats = host.__EASYOFFICE_DOC_WORD_COUNT__.count(document.getElementById("doc-editor"), { includeNotes });
      return [
        ["페이지", String(Math.max(1, getDocPages?.()?.pageCount?.() ?? 1))],
        ["단어", String(stats.words)],
        ["문자(공백 제외)", String(stats.charsWithoutSpaces)],
        ["문자(공백 포함)", String(stats.charsWithSpaces)],
        ["단락", String(stats.paragraphs)],
        ["줄", String(stats.lines)],
      ];
    }

    function countDocStatsMetrics(smartRes) {
      const text = String(extractEditableDocumentText() || "");
      const charsWithSpaces = Array.from(text).length;
      const charsWithoutSpaces = Array.from(text.replaceAll(/\s+/g, "")).length;
      const trimmed = text.trim();
      const words = trimmed ? trimmed.split(/\s+/).length : 0;
      const mod = getActiveModule();
      let unitLabel = "줄 수";
      let unitValue = text ? text.split("\n").length : 0;
      let pageLabel = "쪽 수";
      let pageValue = Math.max(1, getDocPages?.()?.pageCount?.() ?? 1);
      if (mod === "DOC" || mod === "HWP") {
        unitLabel = "문단 수";
        unitValue = Array.from(
          document.querySelectorAll("#doc-editor p, #doc-editor h1, #doc-editor h2, #doc-editor h3, #doc-editor h4, #doc-editor h5, #doc-editor h6, #doc-editor li, #doc-editor blockquote, #doc-editor pre")
        ).filter((el) => !el.closest?.("#hwp-manuscript-preview, footer, .eo-doc-notes, [data-eo-comments], [data-eo-hf]")).length;
      } else if (mod === "SHEET") {
        pageLabel = "시트 수";
        pageValue = Math.max(1, document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]").length);
      } else if (mod === "SLIDE") {
        pageLabel = "슬라이드 수";
        pageValue = Math.max(1, getSlideDeckCount?.() ?? 1);
      } else if (mod === "PDF") {
        pageValue = Math.max(1, Number(document.getElementById("pdf-page-count")?.textContent) || 1);
      }
      const counts = [
        ["글자 수 (공백 포함)", String(charsWithSpaces)],
        ["글자 수 (공백 제외)", String(charsWithoutSpaces)],
        ["단어 수", String(words)],
        [unitLabel, String(unitValue)],
      ];
      const sentences = Number(smartRes?.sentenceCount ?? (trimmed ? trimmed.split(/[.!?。\n]+/).filter((s) => s.trim()).length : 0));
      const readSeconds = words > 0 ? Math.max(1, Math.ceil((words / 220) * 60)) : 0;
      return [
        ...counts,
        ["문장 수", String(sentences)],
        [pageLabel, String(pageValue)],
        ["예상 읽기 시간", readSeconds > 0 ? readSeconds + "초" : "0초"],
      ];
    }

    const isWordDoc = (mod) => mod === "DOC" && document.getElementById("doc-editor")?.dataset?.eoDocFlavor !== "pages";

    let statsBound = false;

    function renderStats(smartRes) {
      const grid = $("doc-stats-grid");
      if (!grid) return false;
      const word = isWordDoc(getActiveModule());
      const notesRow = $("doc-stats-notes-row");
      if (notesRow) notesRow.hidden = !word;
      const rows = word ? wordCountRows($("doc-stats-notes")?.checked !== false) : countDocStatsMetrics(smartRes);
      const children = [];
      for (const [k, v] of rows) {
        const dt = document.createElement("dt");
        dt.textContent = k;
        const dd = document.createElement("dd");
        dd.textContent = v;
        children.push(dt, dd);
      }
      grid.replaceChildren(...children);
      const title = $("doc-stats-title");
      if (title) title.textContent = word ? "단어 개수" : "문서 통계";
      const statusEl = $("doc-stats-status");
      if (statusEl) {
        statusEl.textContent = word ? "" : "공백 포함 " + rows[0][1] + "자 · 공백 제외 " + rows[1][1] + "자 · 단어 " + rows[2][1] + "개";
      }
      return true;
    }

    function openStatsDialog(smartRes) {
      if (!renderStats(smartRes)) return;
      const notes = $("doc-stats-notes");
      if (!statsBound && notes) {
        statsBound = true;
        notes.checked = true;
        notes.addEventListener("change", () => renderStats(smartRes));
      }
      openToolDialog("doc-stats-dialog");
      $("btn-doc-stats-close")?.focus?.();
    }

    function isDocMetaNode(el) {
      if (!el || el.nodeType !== 1) return false;
      const tag = String(el.tagName || "").toUpperCase(), ds = el.dataset;
      return tag === "FOOTER" || el.id === "hwp-manuscript-preview" || Boolean(ds && (ds.eoHf !== undefined || ds.eoNotes !== undefined || ds.eoComments !== undefined));
    }

    function caretLineCol(editor) {
      const sel = host.getSelection?.(), range = sel && sel.rangeCount > 0 ? sel.getRangeAt(0) : null;
      if (!editor || !range || !editor.contains(range.startContainer)) {
        const text = String(extractEditableDocumentText() || "");
        if (!text) return { line: 1, col: 1 };
        const lines = text.split("\n");
        return { line: lines.length, col: Array.from(lines[lines.length - 1] || "").length + 1 };
      }
      const target = range.startContainer, offset = range.startOffset;
      const BLOCKS = new Set(["P", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "TR", "DIV", "BLOCKQUOTE", "PRE", "FIGCAPTION"]);
      let line = 1, col = 0, blockSeen = false, stopped = false;
      const feedText = (str) => {
        for (let i = 0; i < str.length; ) {
          const cp = str.codePointAt(i);
          i += cp > 0xffff ? 2 : 1;
          if (cp === 10) { line += 1; col = 0; } else col += 1;
        }
      };
      const walk = (node) => {
        if (stopped) return;
        if (node.nodeType === 3) {
          const v = String(node.nodeValue || "");
          if (node === target) { feedText(v.slice(0, offset)); stopped = true; return; }
          feedText(v);
          return;
        }
        if (node.nodeType !== 1 || isDocMetaNode(node)) return;
        const tag = String(node.tagName || "").toUpperCase();
        if (tag === "BR") { if (node === target) { stopped = true; return; } line += 1; col = 0; return; }
        if (BLOCKS.has(tag)) { if (blockSeen) { line += 1; col = 0; } blockSeen = true; }
        const kids = node.childNodes || [], limit = node === target ? Math.min(offset, kids.length) : kids.length;
        for (let i = 0; i < limit && !stopped; i += 1) walk(kids[i]);
        if (node === target) stopped = true;
      };
      walk(editor);
      return { line, col: col + 1 };
    }

    function nextTextInBlock(block, startNode) {
      let seen = false;
      const walk = (n) => {
        if (n === startNode) { seen = true; return null; }
        if (n.nodeType === 3) return seen && (n.nodeValue || "").length > 0 ? n : null;
        if (n.nodeType !== 1 || n.tagName === "BR") return null;
        for (const c of n.childNodes || []) { const hit = walk(c); if (hit) return hit; }
        return null;
      };
      return walk(block);
    }

    function handleOverwriteBeforeInput(ev, editor, insertModeLabel) {
      if (insertModeLabel !== "수정" || !editor || ev.isComposing || ev.inputType !== "insertText" || !ev.data) return;
      const mod = getActiveModule();
      if (mod !== "DOC" && mod !== "HWP") return;
      const sel = host.getSelection?.(), range = sel && sel.rangeCount > 0 ? sel.getRangeAt(0) : null;
      if (!range || !range.collapsed || !editor.contains(range.startContainer)) return;
      let node = range.startContainer, off = range.startOffset;
      let block = node.nodeType === 1 ? node : node.parentElement;
      const BLOCKS = new Set(["P", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "TD", "TH", "DIV", "FIGCAPTION"]);
      while (block && block !== editor && !BLOCKS.has(String(block.tagName || "").toUpperCase())) block = block.parentElement;
      block = block || editor;
      if (node.nodeType !== 3) {
        const kid = (node.childNodes || [])[off];
        if (kid?.nodeType === 3) { node = kid; off = 0; } else return;
      }
      if (off >= (node.nodeValue || "").length) {
        const next = nextTextInBlock(block, node);
        if (!next) return;
        node = next; off = 0;
      }
      const val = String(node.nodeValue || "");
      let end = off, count = Array.from(ev.data).length;
      while (count > 0 && end < val.length) {
        const cp = val.codePointAt(end);
        end += cp > 0xffff ? 2 : 1;
        count -= 1;
      }
      if (end === off) return;
      ev.preventDefault?.();
      withDocStyleUndo(() => {
        node.nodeValue = val.slice(0, off) + ev.data + val.slice(end);
        const nextRange = document.createRange?.();
        if (nextRange) {
          nextRange.setStart(node, off + ev.data.length);
          nextRange.collapse?.(true);
          sel.removeAllRanges?.();
          sel.addRange?.(nextRange);
        }
        computeSmartStats();
      });
    }

    function computeFitZoom(editor, mode, fallback) {
      const mod = getActiveModule();
      const vpSel = {
        DOC: [".doc-canvas-scroll", ".doc-scroll-container"], HWP: [".doc-canvas-scroll", ".doc-scroll-container"],
        SLIDE: [".slide-canvas-stage", "#pane-slide"], PDF: ["#pane-pdf"], SHEET: ["#sheet-viewport", "#pane-sheet"], BASE: ["#pane-base"],
      }[mod] || [".canvas-stage"];
      const vp = [...vpSel, ".canvas-stage"].map((s) => document.querySelector(s)).find((el) => el && !el.hidden && !el.closest?.("[hidden]") && Number(el.clientWidth) > 0);
      const cw = Number(vp?.clientWidth) || 0, ch = Number(vp?.clientHeight) || 0;
      if (cw <= 0 || (mode === "page" && ch <= 0)) return fallback;
      const surfaceSel = { DOC: "#doc-editor", HWP: "#doc-editor", SLIDE: "#slide-stage-box", PDF: "#pdf-page-card", SHEET: "#sheet-grid" }[mod];
      const defaults = { DOC: [794, 1123], HWP: [794, 1123], SLIDE: [960, 540], PDF: [595, 842], SHEET: [960, 600] }[mod] || [794, 1123];
      const surface = surfaceSel ? document.querySelector(surfaceSel) : editor;
      const pw = Number(surface?.offsetWidth || surface?.clientWidth) || defaults[0];
      const ph = Number(surface?.offsetHeight || surface?.clientHeight) || defaults[1];
      const wPct = ((cw - 40) / pw) * 100;
      const raw = mode === "page" ? Math.min(wPct, ((ch - 20) / ph) * 100) : wPct;
      return Math.max(50, Math.min(200, Math.round(raw)));
    }

    return { openSymbolDialog, openStatsDialog, caretLineCol, handleOverwriteBeforeInput, computeFitZoom };
  }

  host.__EASYOFFICE_CREATE_DOC_SYMBOL_DIALOG__ = createDocSymbolDialog;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
