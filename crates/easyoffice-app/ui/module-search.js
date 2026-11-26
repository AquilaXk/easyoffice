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

  function createSlideSource(deps) {
    const { document, getSlideDeck, getSlideIndex, switchSlide, getSlideCanvas } = deps;
    let watchCb = null;

    function elementText(el) {
      if (!el || typeof el !== "object") return "";
      if (el.kind === "table" && Array.isArray(el.rows)) {
        return el.rows.map((r) => (Array.isArray(r) ? r.join(" ") : "")).join(" ");
      }
      if (el.kind === "chart") {
        const cats = Array.isArray(el.categories) ? el.categories.join(" ") : "";
        const ser = Array.isArray(el.series) ? el.series.map((s) => s.name || "").join(" ") : "";
        return [el.title || "", cats, ser].filter(Boolean).join(" ");
      }
      if (el.kind === "image") return String(el.alt || "");
      return String(el.text || "");
    }

    function sync(index) {
      const deck = getSlideDeck?.() || [];
      const blocks = [];
      for (let i = 0; i < deck.length; i += 1) {
        const s = deck[i];
        if (!s) continue;
        if (s.title) blocks.push({ id: `s:${i}:title`, text: String(s.title) });
        const body = [s.lower || "", textOfNodeSpecs(s.insertedNodes)].filter(Boolean).join("\n");
        if (body) blocks.push({ id: `s:${i}:body`, text: body });
        for (const el of s.elements || []) {
          const txt = elementText(el);
          if (txt) blocks.push({ id: `s:${i}:el:${el.id}`, text: txt });
        }
        if (s.notes) blocks.push({ id: `s:${i}:notes`, text: String(s.notes) });
      }
      index.setBlocks(blocks);
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
      if (zone === "el") {
        const elId = parts.slice(3).join(":");
        canvas?.selectElement?.(elId);
        const card = document.querySelector(`#slide-canvas-layer .slide-el[data-element-id="${elId}"]`);
        card?.classList?.add("eo-search-hit");
        card?.scrollIntoView?.({ block: "nearest" });
      } else if (zone === "notes") {
        const notes = document.getElementById("slide-presenter-notes");
        notes?.classList?.add("eo-search-hit");
        notes?.scrollIntoView?.({ block: "nearest" });
      } else if (zone === "body") {
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
      clearHighlight,
      watch: (cb) => { watchCb = cb; },
      notifyChange: () => watchCb?.(),
      dispose: () => { clearHighlight(); watchCb = null; },
    };
  }

  function createPdfSource(deps) {
    const { document, getPdfPages, goToPdfPage } = deps;
    let watchCb = null;

    function sync(index) {
      const blocks = [];
      const pages = typeof getPdfPages === "function" ? getPdfPages() : [];
      if (Array.isArray(pages) && pages.length > 0) {
        pages.forEach((p, i) => {
          const num = Number(p?.pageNumber) || i + 1;
          const text = String(p?.text || "");
          if (text) blocks.push({ id: `pdf:page:${num}`, text });
        });
      } else {
        const heading = document.querySelector("#pdf-page-card h3")?.textContent || "";
        if (heading) blocks.push({ id: "pdf:heading:0", text: heading });
        document.querySelectorAll("#pdf-page-card p:not(#pdf-form-status)").forEach((p, idx) => {
          const t = String(p.textContent || "");
          if (t) blocks.push({ id: `pdf:para:${idx}`, text: t });
        });
      }
      const appVal = String(document.getElementById("pdf-form-applicant")?.value || "");
      if (appVal) blocks.push({ id: "pdf:input:pdf-form-applicant", text: appVal });
      const deptVal = String(document.getElementById("pdf-form-dept")?.value || "");
      if (deptVal) blocks.push({ id: "pdf:input:pdf-form-dept", text: deptVal });
      document.querySelectorAll("#pdf-form-extracted .pdf-form-extracted-chip").forEach((chip, idx) => {
        const t = String(chip.textContent || "");
        if (t) blocks.push({ id: `pdf:chip:${idx}`, text: t });
      });
      index.setBlocks(blocks);
    }

    function anchor() {
      return { id: "pdf:page:1", offset: 0 };
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
        const hit = paras.find((p) => queryVal && String(p.textContent || "").toLowerCase().includes(queryVal)) || paras[0];
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
        document.querySelector(`#board-node-layer .board-node[data-node-id="${itemId}"]`)?.classList?.add("eo-search-hit");
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

  host.__EASYOFFICE_CREATE_MODULE_FIND_SOURCE__ = function createModuleFindSource(mod, deps) {
    if (mod === "SLIDE") return createSlideSource(deps);
    if (mod === "PDF") return createPdfSource(deps);
    if (mod === "BOARD") return createBoardSource(deps);
    return null;
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
