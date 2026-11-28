(function (host) {
  "use strict";

  // Cross-module search adapters for Find (`⌘F` / `Ctrl+F`) in SLIDE, PDF, and BOARD,
  // conforming to find-replace.js's source interface (`kind`, `sync`, `anchor`, `show`,
  // `clearHighlight`, `watch`, `dispose`).

  function textOfNodeSpecs(specs) {
    if (!Array.isArray(specs)) return "";
    const parts = [];
    const walk = (list) => {
      for (const s of list) {
        if (!s || typeof s !== "object") continue;
        if (s.t === "text" && s.v) parts.push(String(s.v));
        else if (Array.isArray(s.children)) walk(s.children);
      }
    };
    walk(specs);
    return parts.join(" ");
  }

  function clearSearchClass(document, cls) {
    document.querySelectorAll("." + cls).forEach((el) => el.classList.remove(cls));
  }

  function cssEsc(val) {
    return typeof host.CSS?.escape === "function" ? host.CSS.escape(String(val)) : String(val).replace(/["\\]/g, "\\$&");
  }

  // 슬라이드의 찾기·바꾸기 원본(PD-398). 글상자·도형·표 칸·그림 대체 텍스트·제목·본문·노트가 블록이고, 바꾸기는 문서 창과 같은
  // `find-core.js`의 `applyEdits`로 블록 문자열만 고쳐 쓴다. 차트·삽입 개체의 글자는 찾기만 된다.
  const REPLACE_NOTICE = "차트와 삽입한 개체의 글자는 바꿀 수 없습니다";
  const SKIPPED_LABEL = "차트·삽입 개체";

  function createSlideSource(deps) {
    const { document, getSlideDeck, getSlideIndex, switchSlide, getSlideCanvas, withUndo, renderSlideDeck } = deps;
    const { applyEdits } = host.__EASYOFFICE_FIND_CORE__;
    let watchCb = null;

    function chartText(el) {
      const cats = Array.isArray(el.categories) ? el.categories.join(" ") : "";
      const ser = Array.isArray(el.series) ? el.series.map((s) => s.name || "").join(" ") : "";
      return [el.title || "", cats, ser].filter(Boolean).join(" ");
    }

    // One block per table cell, so a replacement never has to map an offset back across the joined row text.
    function elementBlocks(i, el, blocks) {
      if (!el || typeof el !== "object") return;
      if (el.kind === "table" && Array.isArray(el.rows)) {
        el.rows.forEach((row, r) => (Array.isArray(row) ? row : []).forEach((cell, c) => {
          if (cell) blocks.push({ id: `s:${i}:cell:${el.id}:${r}:${c}`, text: String(cell) });
        }));
        return;
      }
      const text = el.kind === "chart" ? chartText(el) : el.kind === "image" ? String(el.alt || "") : String(el.text || "");
      if (text) blocks.push({ id: `s:${i}:${el.kind === "chart" ? "chart" : "el"}:${el.id}`, text });
    }

    function sync(index) {
      const deck = getSlideDeck?.() || [];
      const blocks = [];
      for (let i = 0; i < deck.length; i += 1) {
        const s = deck[i];
        if (!s) continue;
        if (s.title) blocks.push({ id: `s:${i}:title`, text: String(s.title) });
        if (s.lower) blocks.push({ id: `s:${i}:body`, text: String(s.lower) });
        const nodes = textOfNodeSpecs(s.insertedNodes);
        if (nodes) blocks.push({ id: `s:${i}:nodes`, text: nodes });
        for (const el of s.elements || []) elementBlocks(i, el, blocks);
        if (s.notes) blocks.push({ id: `s:${i}:notes`, text: String(s.notes) });
      }
      index.setBlocks(blocks);
    }

    // Writes the edited text of block `id` back to the deck; false when the block is not editable text.
    function writeBlock(deck, id, edits) {
      const parts = String(id).split(":");
      const s = deck[Number(parts[1])];
      const zone = parts[2];
      if (!s) return false;
      const edit = (text) => applyEdits([String(text ?? "")], edits)[0];
      if (zone === "title" || zone === "body" || zone === "notes") {
        s[zone === "body" ? "lower" : zone] = edit(s[zone === "body" ? "lower" : zone]);
        return true;
      }
      if (zone === "el") {
        const el = (s.elements || []).find((e) => e.id === parts.slice(3).join(":"));
        if (!el) return false;
        const field = el.kind === "image" ? "alt" : "text";
        el[field] = edit(el[field]);
        return true;
      }
      if (zone === "cell") {
        const el = (s.elements || []).find((e) => e.id === parts.slice(3, -2).join(":"));
        const row = el?.rows?.[Number(parts.at(-2))];
        if (!row) return false;
        row[Number(parts.at(-1))] = edit(row[Number(parts.at(-1))]);
        return true;
      }
      return false;
    }

    // `plan` is `[{ id, edits }]`: one undo step for the whole replacement, then the active slide is drawn again.
    function replace(plan) {
      const deck = getSlideDeck?.() || [];
      const outcome = withUndo(() => {
        let replaced = 0;
        let skipped = 0;
        for (const { id, edits } of plan) {
          if (writeBlock(deck, id, edits)) replaced += edits.length;
          else skipped += edits.length;
        }
        return { replaced, skipped, skippedLabel: SKIPPED_LABEL, notice: REPLACE_NOTICE };
      });
      if (outcome.replaced > 0) renderSlideDeck?.(Number(getSlideIndex?.() || 0));
      return outcome;
    }

    function anchor() {
      const idx = Math.max(0, Number(getSlideIndex?.() || 0));
      return { id: `s:${idx}:title`, offset: 0 };
    }

    function clearHighlight() {
      clearSearchClass(document, "eo-search-hit");
    }

    function show(_near, current) {
      clearHighlight();
      if (!current) return;
      const parts = String(current.id).split(":");
      const slideIdx = Number(parts[1]);
      const zone = parts[2];
      if (!Number.isInteger(slideIdx) || slideIdx < 0) return;
      if (Number(getSlideIndex?.()) !== slideIdx) switchSlide?.(slideIdx);
      const canvas = getSlideCanvas?.() || host.__EASYOFFICE_SLIDE_CANVAS__;
      if (zone === "el" || zone === "cell" || zone === "chart") {
        const elId = parts.slice(3, zone === "cell" ? -2 : undefined).join(":");
        canvas?.selectElement?.(elId);
        const card = document.querySelector(`#slide-canvas-layer .slide-el[data-element-id="${cssEsc(elId)}"]`);
        card?.classList?.add("eo-search-hit");
        card?.scrollIntoView?.({ block: "nearest" });
      } else if (zone === "notes") {
        const notes = document.getElementById("slide-presenter-notes");
        notes?.classList?.add("eo-search-hit");
        notes?.scrollIntoView?.({ block: "nearest" });
      } else if (zone === "body" || zone === "nodes") {
        const bodyEl = document.getElementById("slide-lower-zone");
        bodyEl?.classList?.add("eo-search-hit");
        bodyEl?.scrollIntoView?.({ block: "nearest" });
      } else {
        const titleEl = document.getElementById("slide-title-zone");
        titleEl?.classList?.add("eo-search-hit");
        titleEl?.scrollIntoView?.({ block: "nearest" });
      }
    }

    return {
      kind: "slide",
      sync,
      anchor,
      show,
      replace,
      clearHighlight,
      watch: (cb) => { watchCb = cb; },
      notifyChange: () => watchCb?.(),
      dispose: () => { clearHighlight(); watchCb = null; },
    };
  }

  function createPdfSource(deps) {
    const { document, getPdfPages, getPdfCurrentPage, goToPdfPage } = deps;
    let watchCb = null;

    function pageParagraphBounds(pages, paras, pageNum) {
      let cursor = 0;
      for (let p = 0; p < Math.min(pageNum - 1, pages.length); p += 1) {
        const pLines = String(pages[p]?.text || "").split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
        let matched = 0;
        for (const line of pLines) {
          const hitIdx = paras.findIndex((el, idx) => idx >= cursor && String(el.textContent || "").trim() === line);
          if (hitIdx >= 0) { cursor = hitIdx + 1; matched += 1; }
        }
        if (matched === 0 && pLines.length > 0) cursor = Math.min(paras.length, cursor + pLines.length);
      }
      const curLines = String(pages[pageNum - 1]?.text || "").split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
      const end = curLines.length > 0 ? Math.min(paras.length, cursor + curLines.length) : paras.length;
      return { start: Math.min(cursor, paras.length), end: Math.max(Math.min(cursor, paras.length), end) };
    }

    function sync(index) {
      const blocks = [];
      const heading = document.querySelector("#pdf-page-card h3")?.textContent || "";
      if (heading) blocks.push({ id: "pdf:heading:0", text: heading });
      const pages = typeof getPdfPages === "function" ? getPdfPages() : [];
      if (Array.isArray(pages) && pages.length > 0) {
        pages.forEach((p, i) => {
          const num = Number(p?.pageNumber) || i + 1;
          const text = String(p?.text || "");
          if (text) blocks.push({ id: `pdf:page:${num}`, text });
        });
      } else {
        document.querySelectorAll("#pdf-page-card p:not(#pdf-form-status)").forEach((p, idx) => {
          const t = String(p.textContent || "");
          if (t) blocks.push({ id: `pdf:para:${idx}`, text: t });
        });
      }
      const formGrid = document.getElementById("pdf-acroform-grid");
      const formShown = Boolean(formGrid && !formGrid.hidden);
      const appVal = String(document.getElementById("pdf-form-applicant")?.value || "");
      if (appVal || formShown) blocks.push({ id: "pdf:input:pdf-form-applicant", text: appVal ? "신청인 성명 " + appVal : "신청인 성명" });
      const deptVal = String(document.getElementById("pdf-form-dept")?.value || "");
      if (deptVal || formShown) blocks.push({ id: "pdf:input:pdf-form-dept", text: deptVal ? "소속 부서명 " + deptVal : "소속 부서명" });
      document.querySelectorAll("#pdf-form-extracted .pdf-form-extracted-chip").forEach((chip, idx) => {
        const raw = String(chip.textContent || "");
        const t = raw.endsWith(": (입력 없음)") ? raw.slice(0, -": (입력 없음)".length) : raw;
        if (t) blocks.push({ id: `pdf:chip:${idx}`, text: t });
      });
      index.setBlocks(blocks);
    }

    function anchor() {
      const pages = typeof getPdfPages === "function" ? getPdfPages() : [];
      if (Array.isArray(pages) && pages.length > 0) {
        const curFn = getPdfCurrentPage || host.__EASYOFFICE_MODULE_FIND_HOOKS__?.getPdfCurrentPage;
        const cur = Math.max(1, Math.min(pages.length, Number(curFn?.() || 1)));
        const p = pages[cur - 1];
        return { id: `pdf:page:${Number(p?.pageNumber) || cur}`, offset: 0 };
      }
      if (document.querySelector("#pdf-page-card h3")?.textContent) return { id: "pdf:heading:0", offset: 0 };
      return { id: "pdf:para:0", offset: 0 };
    }

    function clearHighlight() {
      clearSearchClass(document, "eo-search-hit");
    }

    function show(_near, current) {
      clearHighlight();
      if (!current) return;
      const parts = String(current.id).split(":");
      const kind = parts[1];
      if (kind === "page") {
        const pageNum = Number(parts[2]) || 1;
        goToPdfPage?.(pageNum);
        const queryVal = String(document.getElementById("find-query")?.value || "").toLowerCase();
        const paras = Array.from(document.querySelectorAll("#pdf-page-card p:not(#pdf-form-status)"));
        const pages = typeof getPdfPages === "function" ? getPdfPages() : [];
        const { start, end } = pageParagraphBounds(pages, paras, pageNum);
        const inPage = paras.slice(start, end);
        const hit =
          inPage.find((p) => queryVal && String(p.textContent || "").toLowerCase().includes(queryVal)) ||
          paras.slice(start).find((p) => queryVal && String(p.textContent || "").toLowerCase().includes(queryVal)) ||
          inPage[0] ||
          paras.find((p) => queryVal && String(p.textContent || "").toLowerCase().includes(queryVal)) ||
          paras[0];
        hit?.classList?.add("eo-search-hit");
        hit?.scrollIntoView?.({ block: "nearest" });
      } else if (kind === "input") {
        const grid = document.getElementById("pdf-acroform-grid");
        if (grid) grid.hidden = false;
        const inp = document.getElementById(parts[2]);
        inp?.classList?.add("eo-search-hit");
        inp?.scrollIntoView?.({ block: "nearest" });
      } else if (kind === "chip") {
        const grid = document.getElementById("pdf-acroform-grid");
        if (grid) grid.hidden = false;
        const chip = document.querySelectorAll("#pdf-form-extracted .pdf-form-extracted-chip")[Number(parts[2])];
        chip?.classList?.add("eo-search-hit");
        chip?.scrollIntoView?.({ block: "nearest" });
      } else if (kind === "para") {
        const p = document.querySelectorAll("#pdf-page-card p:not(#pdf-form-status)")[Number(parts[2])];
        p?.classList?.add("eo-search-hit");
        p?.scrollIntoView?.({ block: "nearest" });
      } else if (kind === "heading") {
        const h = document.querySelector("#pdf-page-card h3");
        h?.classList?.add("eo-search-hit");
        h?.scrollIntoView?.({ block: "nearest" });
      }
    }

    return {
      kind: "pdf",
      sync,
      anchor,
      show,
      clearHighlight,
      watch: (cb) => { watchCb = cb; },
      notifyChange: () => watchCb?.(),
      dispose: () => { clearHighlight(); watchCb = null; },
    };
  }

  function createBoardSource(deps) {
    const { document, getBoardStudio } = deps;
    let watchCb = null;

    function sync(index) {
      const board = getBoardStudio?.();
      const st = board?.getState?.();
      if (!st) { index.setBlocks([]); return; }
      const blocks = [];
      for (const n of st.nodes || []) {
        const txt = [n.text || "", n.title || "", n.alt || ""].filter(Boolean).join(" ");
        if (txt) blocks.push({ id: `board:node:${n.id}`, text: txt });
      }
      for (const c of st.connectors || []) {
        if (c.label) blocks.push({ id: `board:conn:${c.id}`, text: String(c.label) });
      }
      index.setBlocks(blocks);
    }

    function anchor() {
      const st = getBoardStudio?.()?.getState?.();
      const firstId = st?.nodes?.[0]?.id;
      return firstId ? { id: `board:node:${firstId}`, offset: 0 } : null;
    }

    function clearHighlight() {
      clearSearchClass(document, "eo-search-hit");
    }

    function show(_near, current) {
      clearHighlight();
      if (!current) return;
      const board = getBoardStudio?.();
      if (!board) return;
      const [, kind, ...rest] = String(current.id).split(":");
      const itemId = rest.join(":");
      if (kind === "node") {
        board.selectNode?.(itemId);
        board.centerOnItem?.("node", itemId);
        document.querySelector(`#board-node-layer .board-node[data-node-id="${cssEsc(itemId)}"]`)?.classList?.add("eo-search-hit");
      } else if (kind === "conn") {
        board.selectConnector?.(itemId);
        board.centerOnItem?.("conn", itemId);
      }
    }

    return {
      kind: "board",
      sync,
      anchor,
      show,
      clearHighlight,
      watch: (cb) => { watchCb = cb; },
      notifyChange: () => watchCb?.(),
      dispose: () => { clearHighlight(); watchCb = null; },
    };
  }

  function extraPaletteItems(activeModule, fmt, runNativeMenu) {
    const items = [];
    if (activeModule === "DOC") {
      for (const [cmd, title, en, menuId] of [
        ["doc-strike", "취소선", "strikethrough", "format:strike"],
        ["doc-sup", "위 첨자", "superscript", "format:superscript"],
        ["doc-sub", "아래 첨자", "subscript", "format:subscript"],
      ]) {
        items.push({ id: "studio-cmd:" + cmd, module: "DOC", title, shortcut: cmd === "doc-sup" ? fmt("⇧⌘=") : cmd === "doc-sub" ? fmt("⌘=") : "", searchKey: "서식 " + title + " " + en, run: () => runNativeMenu(menuId) });
      }
    } else if (activeModule === "SLIDE") {
      items.push({ id: "studio-cmd:find", module: "SLIDE", title: "찾기", shortcut: fmt("⌘F"), searchKey: "편집 찾기 find " + fmt("⌘F"), run: () => runNativeMenu("edit:find") });
      for (const [cmd, title, en, menuId] of [
        ["slide-bring-forward", "앞으로 가져오기", "bring forward", "slide:bring-forward"],
        ["slide-send-backward", "뒤로 보내기", "send backward", "slide:send-backward"],
        ["slide-align-left", "왼쪽 맞춤", "align left", "slide:align-left"],
        ["slide-align-center", "가운데 맞춤", "align center", "slide:align-center"],
        ["slide-align-right", "오른쪽 맞춤", "align right", "slide:align-right"],
        ["slide-align-top", "위쪽 맞춤", "align top", "slide:align-top"],
        ["slide-align-middle", "중간 맞춤", "align middle", "slide:align-middle"],
        ["slide-align-bottom", "아래쪽 맞춤", "align bottom", "slide:align-bottom"],
        ["slide-distribute-h", "가로 간격을 동일하게", "distribute horizontally", "slide:distribute-h"],
        ["slide-distribute-v", "세로 간격을 동일하게", "distribute vertically", "slide:distribute-v"],
      ]) {
        items.push({ id: "studio-cmd:" + cmd, module: "SLIDE", title, shortcut: "", searchKey: "슬라이드 정돈 맞춤 " + title + " " + en, run: () => runNativeMenu(menuId) });
      }
    } else if (activeModule === "PDF") {
      items.push({ id: "studio-cmd:find", module: "PDF", title: "찾기", shortcut: fmt("⌘F"), searchKey: "편집 찾기 find " + fmt("⌘F"), run: () => runNativeMenu("edit:find") });
    }
    return items;
  }

  host.__EASYOFFICE_CREATE_MODULE_FIND_SOURCE__ = function createModuleFindSource(mod, deps) {
    if (mod === "SLIDE") return createSlideSource(deps);
    if (mod === "PDF") return createPdfSource(deps);
    if (mod === "BOARD") return createBoardSource(deps);
    return null;
  };
  host.__EASYOFFICE_EXTRA_PALETTE_ITEMS__ = extraPaletteItems;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
