// Reference implementation: Univer (Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.), commit
// 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, `packages/docs-toc/src/commands/table-of-contents.command.ts`:
// `buildTableOfContentsInstruction` 290-305 (the shown levels are an outline range, 1 to at most 9),
// `collectHeadings` 592-626 and `getOutlineLevel` 628-637 (headings in document order, blank titles and
// headings outside the block and the level range skipped), `buildTableOfContentsBody` 441-561 (an optional
// title paragraph, one entry per heading at the heading's own level, indent growing with the level, a new
// block built from the old one's title) and `updateTableOfContents` 120-190 (the block is replaced in
// place, so updating twice gives the same block). The reference works on an index-based document model with
// page-number fields; here the same steps run on the editor's DOM, entries are links to the headings and
// the page numbers are left out (the engine has no pagination). See THIRD_PARTY_NOTICES.md.
(function (host) {
  "use strict";

  // Table of contents of the document editor: a `<nav class="eo-doc-toc">` of `<p data-eo-toc>` paragraphs
  // (0 is the title, 1-6 an entry at that heading level) whose entries are `<a href="#id">` links to the
  // headings. The save payload sends the paragraphs with a `toc` role and the engine writes them as a
  // table-of-contents block (DOCX `w:sdt`, ODT `text:table-of-content`).

  const TOC_CLASS = "eo-doc-toc";
  const TOC_SELECTOR = "nav." + TOC_CLASS;
  const HEADING_SELECTOR = "h1, h2, h3, h4, h5, h6";
  const HEADING_ID_PREFIX = "eo-h-";
  const HEADING_ID_RE = /^eo-h-\d+$/;
  const DISPLAY_LEVELS = [1, 2, 3];
  const DEFAULT_LEVELS = 3;
  const TITLE_ROLE = 0;
  // Limits of one block (fail-closed: a document over them keeps its block as it was).
  const MAX_HEADINGS = 5000;
  const MAX_TITLE_CHARS = 500;
  const DEFAULT_TITLE = "목차";
  const MSG_NO_HEADINGS = "제목 없음";
  const MSG_TOO_MANY = "제목 수 초과";
  const MSG_TITLE_LONG = "제목 길이 초과";
  const MSG_NO_TOC = "목차 없음";
  const MSG_NO_EDITOR = "삽입 위치 없음";

  const normalizeLevels = (value) => {
    const n = Number(value);
    return DISPLAY_LEVELS.includes(n) ? n : DEFAULT_LEVELS;
  };

  const levelOf = (heading) => Number(String(heading.tagName).slice(1));
  const insideToc = (el) => Boolean(el.closest?.(TOC_SELECTOR));
  const entryTitle = (el) => String(el.textContent || "").replaceAll(/\s+/g, " ").trim();

  // The headings of levels 1..maxLevel in document order, as `{ el, level, text }`, or `{ error }`.
  function collectHeadings(root, maxLevel) {
    const found = [];
    for (const el of root.querySelectorAll(HEADING_SELECTOR)) {
      const level = levelOf(el);
      const text = entryTitle(el);
      if (level > maxLevel || text === "" || insideToc(el)) continue;
      if (found.length >= MAX_HEADINGS) return { error: MSG_TOO_MANY };
      if (text.length > MAX_TITLE_CHARS) return { error: MSG_TITLE_LONG };
      found.push({ el, level, text });
    }
    return found.length === 0 ? { error: MSG_NO_HEADINGS } : { headings: found };
  }

  // Gives the shown headings their link targets; ids of headings that dropped out of the range go.
  function assignHeadingIds(root, headings) {
    root.querySelectorAll(HEADING_SELECTOR).forEach((el) => {
      if (HEADING_ID_RE.test(el.id)) el.removeAttribute("id");
    });
    headings.forEach((h, index) => {
      h.id = HEADING_ID_PREFIX + (index + 1);
      h.el.setAttribute("id", h.id);
    });
  }

  function tocParagraph(document, role) {
    const p = document.createElement("p");
    p.dataset.eoToc = String(role);
    return p;
  }

  const MAX_PAGE_PASSES = 3;

  // The page of every heading from the page layout of the editor (`doc-pages.js`), measured now; `null` when the
  // layout is off (the manuscript grid) or has no pages, so an entry then shows no number.
  function pageMap() {
    const api = host.__EASYOFFICE_DOC_PAGES__;
    if (!api?.isActive?.()) return null;
    api.layoutNow();
    return api.pageCount() < 1 ? null : new Map(api.headingPages().map((h) => [h.el, h.page]));
  }

  function setEntryPage(p, page, dots) {
    if (page === undefined) {
      delete p.dataset.eoTocPage;
      delete p.dataset.eoTocDots;
      return;
    }
    p.dataset.eoTocPage = String(page);
    p.dataset.eoTocDots = String(dots);
  }

  // The block itself can push headings to a later page, so the numbers are measured again with the block in
  // place until they stop changing (at most MAX_PAGE_PASSES times).
  function settlePages(toc, headings, dots) {
    const entries = Array.from(toc.querySelectorAll("p[data-eo-toc]")).slice(1);
    for (let pass = 0; pass < MAX_PAGE_PASSES; pass += 1) {
      const pages = pageMap();
      if (!pages) return;
      let changed = false;
      headings.forEach((h, i) => {
        const page = pages.get(h.el);
        if (page === (entries[i].dataset.eoTocPage === undefined ? undefined : Number(entries[i].dataset.eoTocPage))) return;
        setEntryPage(entries[i], page, dots);
        changed = true;
      });
      if (!changed) return;
    }
  }

  function buildToc(document, headings, title, levels, options) {
    const nav = document.createElement("nav");
    nav.className = TOC_CLASS;
    nav.dataset.docToc = "true";
    nav.dataset.tocLevels = String(levels);
    nav.dataset.tocPages = String(options.pages);
    nav.dataset.tocDots = String(options.dots);
    nav.setAttribute("contenteditable", "false");
    const titleP = tocParagraph(document, TITLE_ROLE);
    titleP.textContent = title;
    nav.appendChild(titleP);
    for (const h of headings) {
      const p = tocParagraph(document, h.level);
      const a = document.createElement("a");
      a.setAttribute("href", "#" + h.id);
      a.textContent = h.text;
      p.appendChild(a);
      if (options.pages) setEntryPage(p, options.pageMap?.get(h.el), options.dots);
      nav.appendChild(p);
    }
    return nav;
  }

  const existingToc = (root) => root.querySelector(TOC_SELECTOR);
  const titleOf = (nav) => {
    const titleP = nav.querySelector('p[data-eo-toc="0"]');
    return (titleP && entryTitle(titleP)) || DEFAULT_TITLE;
  };

  // The levels of a block: the number it was made with, else the deepest entry it holds (a file opened
  // from disk keeps no setting).
  function levelsOf(nav) {
    if (nav.dataset.tocLevels) return normalizeLevels(nav.dataset.tocLevels);
    const deepest = Math.max(0, ...Array.from(nav.querySelectorAll("p[data-eo-toc]"), (p) => Number(p.dataset.eoToc)));
    return normalizeLevels(deepest);
  }

  // The page options of a block: the ones it was made with, else what its entries show (a file opened from
  // disk keeps no setting).
  function optionsOf(nav) {
    const shown = nav.querySelector("p[data-eo-toc-page]");
    const pages = nav.dataset.tocPages === undefined ? Boolean(shown) : nav.dataset.tocPages === "true";
    const dots = nav.dataset.tocDots === undefined ? shown?.dataset.eoTocDots !== "false" : nav.dataset.tocDots === "true";
    return { pages, dots };
  }

  // The block child of `root` that holds the range, or the first child.
  function insertionPoint(root, range) {
    let node = range?.startContainer ?? null;
    while (node && node.parentNode !== root) node = node.parentNode;
    return node ?? root.firstChild;
  }

  // Builds the block for headings 1..`levels` and puts it in place of the existing block, else before the
  // block the caret is in. `{ changed, count, toc }`, or `{ changed: false, error }`.
  function applyToc(root, levels, range = null, pageOptions = {}) {
    const options = { pages: pageOptions.pages === true, dots: pageOptions.dots !== false };
    const shown = normalizeLevels(levels);
    const collected = collectHeadings(root, shown);
    if (collected.error) return { changed: false, error: collected.error };
    const old = existingToc(root);
    assignHeadingIds(root, collected.headings);
    const toc = buildToc(root.ownerDocument, collected.headings, old ? titleOf(old) : DEFAULT_TITLE, shown, {
      ...options,
      pageMap: options.pages ? pageMap() : null,
    });
    if (old) {
      old.parentNode.insertBefore(toc, old);
      old.remove();
    } else {
      root.insertBefore(toc, insertionPoint(root, range));
    }
    if (options.pages) settlePages(toc, collected.headings, options.dots);
    return { changed: true, count: collected.headings.length, toc };
  }

  // Rebuilds the block that is in the document with its own levels.
  function updateToc(root) {
    const old = existingToc(root);
    return old ? applyToc(root, levelsOf(old), null, optionsOf(old)) : { changed: false, error: MSG_NO_TOC };
  }

  // The heading an entry points at: by the link's id, else (an entry read from a file) the heading of the
  // same level and text that comes as often before it as the entry's twins do.
  function headingFor(root, entry) {
    const id = String(entry.querySelector("a")?.getAttribute("href") || "").slice(1);
    const all = Array.from(root.querySelectorAll(HEADING_SELECTOR));
    const byId = id ? all.find((h) => h.id === id) : null;
    if (byId) return byId;
    const twin = (el, level, text) => levelOf(el) === level && entryTitle(el) === text;
    const level = Number(entry.dataset.eoToc);
    const text = entryTitle(entry);
    const nav = entry.parentNode;
    const before = Array.from(nav.querySelectorAll("p[data-eo-toc]")).filter((p) => Number(p.dataset.eoToc) === level && entryTitle(p) === text);
    return all.filter((h) => twin(h, level, text))[before.indexOf(entry)] ?? null;
  }

  // ---- dialog and navigation ------------------------------------------------------------------

  function createDocToc(deps) {
    const { document, getRange, withUndo, setFeedback, runCommand } = deps;
    const byId = (id) => document.getElementById(id);
    const ui = () => ({
      dialog: byId("doc-toc-dialog"), levels: byId("doc-toc-levels"), pages: byId("doc-toc-pages"), fill: byId("doc-toc-fill"),
      status: byId("doc-toc-status"), apply: byId("btn-doc-toc-apply"),
    });

    // 채울 모양 only means something while the page numbers are on.
    const syncFill = () => {
      const els = ui();
      els.fill.disabled = !els.pages.checked;
    };

    function open() {
      const root = byId("doc-editor");
      if (!root) {
        setFeedback?.(MSG_NO_EDITOR);
        return false;
      }
      const els = ui();
      const old = existingToc(root);
      const shown = old ? optionsOf(old) : { pages: true, dots: true };
      els.levels.value = String(old ? levelsOf(old) : DEFAULT_LEVELS);
      els.pages.checked = shown.pages;
      els.fill.value = shown.dots ? "dot" : "none";
      syncFill();
      els.status.textContent = "";
      els.dialog.showModal();
      els.levels.focus();
      return true;
    }

    // 확인: a document the block cannot be made for keeps the dialog open with the reason; else the command runs with the levels.
    function apply() {
      const root = byId("doc-editor");
      const els = ui();
      const levels = normalizeLevels(els.levels.value);
      const refused = collectHeadings(root, levels).error;
      if (refused) {
        els.status.textContent = refused;
        return;
      }
      els.dialog.close();
      void runCommand({ levels, pages: els.pages.checked, dots: els.fill.value === "dot" }).catch(() => {});
    }

    // The command's effect (inside its undo step): the block is made at the caret, the result is on the status line.
    function applyOptions({ levels, pages, dots }) {
      const root = byId("doc-editor");
      const result = applyToc(root, levels, getRange(), { pages, dots });
      setFeedback?.(result.error || DEFAULT_TITLE + " " + result.count + "항목");
      root.focus?.();
      return result;
    }

    // The 목차 업데이트 command: rebuilds the block in the document, feedback on the status line.
    function update() {
      const root = byId("doc-editor");
      const result = withUndo(() => (root ? updateToc(root) : { changed: false, error: MSG_NO_EDITOR }));
      setFeedback?.(result.error || DEFAULT_TITLE + " " + result.count + "항목");
      return result;
    }

    function goTo(entry) {
      const root = byId("doc-editor");
      const target = headingFor(root, entry);
      if (!target) return false;
      target.scrollIntoView?.({ block: "start" });
      const sel = deps.env.getSelection?.();
      const range = document.createRange?.();
      if (sel && range) {
        range.setStart(target, 0);
        range.setEnd(target, 0);
        sel.removeAllRanges();
        sel.addRange(range);
      }
      return true;
    }

    function bind() {
      const els = ui();
      if (!els.dialog) return;
      els.apply.addEventListener("click", apply);
      els.pages.addEventListener("change", syncFill);
      byId("doc-editor")?.addEventListener("click", (ev) => {
        const entry = ev.target.closest?.(TOC_SELECTOR) ? ev.target.closest("p[data-eo-toc]") : null;
        if (!entry || Number(entry.dataset.eoToc) === TITLE_ROLE) return;
        ev.preventDefault?.();
        goTo(entry);
      });
    }

    return { bind, open, update, applyOptions, goTo };
  }

  host.__EASYOFFICE_DOC_TOC__ = { DISPLAY_LEVELS, DEFAULT_LEVELS, MAX_HEADINGS, MAX_TITLE_CHARS, collectHeadings, applyToc, updateToc, headingFor };
  host.__EASYOFFICE_CREATE_DOC_TOC__ = createDocToc;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
