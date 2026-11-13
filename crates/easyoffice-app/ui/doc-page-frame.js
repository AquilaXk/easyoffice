(function (host) {
  "use strict";

  // Header and footer of the page (PD-211). Each is one `<section data-eo-hf="header|footer">` child of the
  // document editor that holds its paragraphs. The section is not part of the body flow (the page layout, the
  // save payload, find and the statistics skip it); it is drawn in the top and bottom margin of every page,
  // as a copy inside each paper sheet of the page layer, and a page-number field (`.eo-page-field`) shows the
  // number of the sheet it sits on (a CSS counter of the layer). Editing follows the reference editor: the
  // command enters the header or footer of page 1, the body is dimmed and not editable, a toolbar group
  // offers close, delete and page number, and the copies on the other pages follow every keystroke.

  const KINDS = ["header", "footer"];
  const FIELDS = { page: "쪽 번호", pages: "총 쪽수", filename: "파일 이름" };
  // The page setup that places them, in twips, as the editor's `data-eo-page` (`doc-page-setup.js`: "width,height,left,
  // right,top,bottom,header,footer"): top and bottom are the distances from the page edge to the body, header and
  // footer those to the header and footer (the engine's `RichPageMargins`). Without one the reference editor's new-document setup applies
  // (`rhwp` `src/model/page.rs:36-51`: header area 15 mm below a 20 mm margin, body at 35 mm; footer area 15 mm
  // above a 15 mm margin, body ending 30 mm from the bottom).
  const DEFAULT_MARGINS = { top: 1984, bottom: 1700, header: 1134, footer: 850 };
  const PX_PER_TWIP = 1 / 15;
  // Space kept between a header or footer that grew past its area and the body.
  const GROWN_GAP_PX = 6;
  // Position of each margin in `data-eo-page`.
  const PAGE_MARGIN_AT = { top: 4, bottom: 5, header: 6, footer: 7 };
  const LABELS = { header: "머리말", footer: "꼬리말" };
  const APPLY_LABEL = "양쪽";
  const SECTION_CLASS = "eo-page-frame";
  const FIELD_CLASS = "eo-page-field";
  const COPY_CLASS = "doc-page-hf";
  const LAYER_SELECTOR = ":scope > footer.doc-page-layer";
  const MSG_NOT_PAGED = "쪽 보기에서만 편집";

  const isFrame = (el) => el?.nodeType === 1 && el.tagName === "SECTION" && el.dataset?.eoHf !== undefined;

  const lastParagraph = (section) => Array.from(section.children).at(-1) ?? section;

  // `data-eo-page` as `{ top, bottom, header, footer }` in twips, or null when it is absent or malformed.
  function parseMargins(text) {
    const parts = String(text ?? "").split(",").map((v) => Number(v));
    if (parts.length !== 8 || !parts.every((v) => Number.isInteger(v) && v >= 0)) return null;
    return Object.fromEntries(Object.entries(PAGE_MARGIN_AT).map(([key, at]) => [key, parts[at]]));
  }

  function hasContent(section) {
    return Boolean(section) && (section.textContent.trim() !== "" || section.querySelector("." + FIELD_CLASS) !== null);
  }

  function createPageFrame(deps) {
    const document = deps.document;
    const env = deps.env || host;
    const raf = (fn) => (env.requestAnimationFrame ? env.requestAnimationFrame(fn) : env.setTimeout(fn, 16));
    let editor = null;
    let pages = null;
    let observer = null;
    let editing = null;
    let scheduled = false;
    let paintedKey = "";

    const layerOf = () => editor?.querySelector(LAYER_SELECTOR) ?? null;
    const sectionOf = (kind) => (editor ? Array.from(editor.children).find((el) => isFrame(el) && el.dataset.eoHf === kind) ?? null : null);
    const scrollOf = () => editor?.closest?.(".doc-scroll-container") ?? document.querySelector(".doc-scroll-container");
    const toolbar = () => document.getElementById("icon-toolbar");

    // The page setup of the document: the editor's own, else the default.
    const marginsOf = () => parseMargins(editor?.dataset.eoPage);

    function create(kind) {
      const section = document.createElement("section");
      section.className = SECTION_CLASS;
      section.dataset.eoHf = kind;
      const p = document.createElement("p");
      p.appendChild(document.createElement("br"));
      section.appendChild(p);
      const layer = layerOf();
      if (layer) layer.before(section);
      else editor.appendChild(section);
      return section;
    }

    // ---- the copies on the paper sheets ------------------------------------------------------

    function copyOf(section) {
      const copy = document.createElement("div");
      copy.className = COPY_CLASS;
      copy.dataset.hf = section.dataset.eoHf;
      copy.setAttribute("aria-hidden", "true");
      for (const p of section.children) copy.appendChild(p.cloneNode(true));
      for (const el of copy.querySelectorAll("[id], [contenteditable]")) {
        el.removeAttribute("id");
        el.removeAttribute("contenteditable");
      }
      for (const el of copy.querySelectorAll('[data-eo-field="filename"]')) el.dataset.eoName = fileName();
      return copy;
    }

    const fileName = () => String(deps.getFileName?.() ?? "");

    // The header or footer of page `pageNumber` of `total` for printing: the copy a sheet shows, with its fields as text.
    function printCopy(kind, pageNumber, total) {
      const section = sectionOf(kind);
      if (!hasContent(section)) return null;
      const copy = copyOf(section);
      for (const field of copy.querySelectorAll("." + FIELD_CLASS)) {
        const text = field.dataset.eoField === "pages" ? total : field.dataset.eoField === "filename" ? fileName() : pageNumber;
        field.parentNode.insertBefore(document.createTextNode(String(text)), field);
        field.parentNode.removeChild(field);
      }
      return copy;
    }

    // Draws the header and footer on every sheet of the page layer; the sheet of page 1 shows the live section
    // instead of a copy while that one is edited. Only a changed content, name, setup, sheet count or edit state
    // redraws, and then the areas are measured (a header that grew moves the body, `applyGeometry`).
    function paint() {
      const layer = layerOf();
      if (!layer) return;
      const sections = KINDS.map((kind) => sectionOf(kind)).map((s) => (hasContent(s) || s === sectionOf(editing) ? s : null));
      const sheets = layer.children.length;
      const key = [sections.map((s) => (s ? s.innerHTML : "")).join("\u0001"), editing, sheets, fileName(), editor.dataset.eoPage].join("|");
      [["data-hf-header", sections[0]], ["data-hf-footer", sections[1]]].forEach(([name, section]) => {
        if (!section) layer.removeAttribute(name);
        else if (!layer.hasAttribute(name)) layer.setAttribute(name, "");
      });
      // The page-count field reads the number of sheets from this counter.
      const counters = "eo-page 0 eo-total " + sheets;
      if (layer.style.getPropertyValue("counter-reset") !== counters) layer.style.setProperty("counter-reset", counters);
      if (key === paintedKey && sheets > 0) return;
      paintedKey = key;
      Array.from(layer.children).forEach((sheet, index) => {
        for (const old of Array.from(sheet.querySelectorAll(":scope > ." + COPY_CLASS))) old.remove();
        sections.forEach((section, i) => {
          if (!section || (index === 0 && editing === KINDS[i])) return;
          sheet.appendChild(copyOf(section));
        });
      });
      applyGeometry(sections, layer);
    }

    function setVar(name, value) {
      const style = editor.style;
      if (value === null) {
        if (style.getPropertyValue(name)) style.removeProperty(name);
      } else if (style.getPropertyValue(name) !== value) {
        style.setProperty(name, value);
      }
    }

    // Places the areas from the page setup and tells the page layout how far the body must keep away.
    function applyGeometry(sections, layer) {
      const m = marginsOf() ?? DEFAULT_MARGINS;
      const px = (twips) => Math.round(twips * PX_PER_TWIP * 100) / 100;
      const [header, footer] = sections;
      const band = (kind) => px(kind === "header" ? Math.max(0, m.top - m.header) : Math.max(0, m.bottom - m.footer));
      const grown = (kind) => {
        const live = editing === kind ? sectionOf(kind) : null;
        const copy = layer.children[0]?.querySelector(":scope > ." + COPY_CLASS + '[data-hf="' + kind + '"]');
        return Number((live ?? copy)?.offsetHeight) || 0;
      };
      setVar("--hf-hd", header ? px(m.header) + "px" : null);
      setVar("--hf-band-h", header ? band("header") + "px" : null);
      setVar("--hf-ft", footer ? px(m.footer) + "px" : null);
      setVar("--hf-fband-h", footer ? band("footer") + "px" : null);
      // Measured with the areas at their minimum: the body starts below the header, ends above the footer.
      const headerHeight = header ? grown("header") : 0;
      const footerHeight = footer ? grown("footer") : 0;
      const room = (distance, height, area, margin) => Math.max(margin, distance + height + (height > area + 0.5 ? GROWN_GAP_PX : 0));
      setVar("--pg-hf-top", header ? room(px(m.header), headerHeight, band("header"), px(m.top)) + "px" : null);
      setVar("--pg-hf-bottom", footer ? room(px(m.footer), footerHeight, band("footer"), px(m.bottom)) + "px" : null);
      setVar("--hf-fh", footer && editing === "footer" ? footerHeight + "px" : null);
    }

    function schedule() {
      if (scheduled || !editor) return;
      scheduled = true;
      raf(() => {
        scheduled = false;
        sync();
      });
    }

    // Brings the editing state in line with the DOM after any change (typing, an undo that restored the
    // sections, the page layer being made again).
    function sync() {
      if (!editor) return;
      if (editing && !sectionOf(editing)) endEdit();
      for (const kind of KINDS) {
        const section = sectionOf(kind);
        if (!section) continue;
        const want = editing === kind ? "true" : null;
        if (section.getAttribute("contenteditable") !== want) {
          if (want) section.setAttribute("contenteditable", want);
          else section.removeAttribute("contenteditable");
        }
      }
      for (const field of editor.querySelectorAll("." + FIELD_CLASS)) {
        if (field.getAttribute("contenteditable") !== "false") field.setAttribute("contenteditable", "false");
        if (editing && field.dataset.eoField === "filename" && field.dataset.eoName !== fileName()) field.dataset.eoName = fileName();
      }
      paint();
    }

    function onMutations(records) {
      if (records.every((r) => layerOf()?.contains(r.target))) return;
      schedule();
    }

    // ---- editing ------------------------------------------------------------------------------

    function placeCaret(section) {
      const sel = env.getSelection?.();
      if (!sel || typeof document.createRange !== "function") return;
      const last = lastParagraph(section);
      const range = document.createRange();
      range.selectNodeContents(last);
      range.collapse(false);
      sel.removeAllRanges();
      sel.addRange(range);
    }

    function setEditState(kind) {
      editing = kind;
      if (kind) editor.dataset.eoHfEdit = kind;
      else delete editor.dataset.eoHfEdit;
      editor.setAttribute("contenteditable", kind ? "false" : "true");
      scrollOf()?.classList.toggle("hf-editing", Boolean(kind));
      const bar = toolbar();
      if (bar) bar.dataset.hfEditing = kind || "";
      const group = bar?.querySelector(".tb-headerfooter-group");
      if (group) {
        group.hidden = !kind;
        const label = group.querySelector(".tb-hf-label");
        if (label) label.textContent = kind ? LABELS[kind] + " · " + APPLY_LABEL + " 편집 중" : "";
      }
      deps.onEditingChange?.(kind);
    }

    // Enters the header or footer: made first when the document has none (one undo step), then edited in place.
    function open(kind) {
      if (!editor || !KINDS.includes(kind)) return false;
      // The manuscript grid has no pages to draw them on.
      if (editor.classList.contains("manuscript-grid")) {
        deps.setFeedback?.(MSG_NOT_PAGED);
        return false;
      }
      if (editing && editing !== kind) endEdit();
      if (!sectionOf(kind)) {
        const run = () => {
          create(kind);
          return { changed: true };
        };
        if (deps.withUndo) deps.withUndo(run);
        else run();
      }
      const section = sectionOf(kind);
      setEditState(kind);
      sync();
      section.focus?.();
      placeCaret(section);
      section.scrollIntoView?.({ block: "nearest" });
      deps.setFeedback?.(LABELS[kind] + " 편집 중");
      return true;
    }

    // Leaves the edit state; an empty section is not kept.
    function endEdit() {
      if (!editing) return false;
      const kind = editing;
      const section = sectionOf(kind);
      setEditState(null);
      if (section && !hasContent(section)) section.remove();
      sync();
      editor.focus?.();
      deps.setFeedback?.(LABELS[kind] + " 닫음");
      return true;
    }

    // Deletes the header or footer being edited, as one undo step.
    function removeEditing() {
      const kind = editing;
      if (!kind) return false;
      setEditState(null);
      const run = () => {
        sectionOf(kind)?.remove();
        return { changed: true };
      };
      if (deps.withUndo) deps.withUndo(run);
      else run();
      sync();
      editor.focus?.();
      deps.setFeedback?.(LABELS[kind] + " 지움");
      return true;
    }

    // A field (`page`, `pages`, `filename`) at the caret of the header or footer being edited.
    function insertField(kind = "page") {
      if (!Object.hasOwn(FIELDS, kind)) return false;
      const section = editing ? sectionOf(editing) : null;
      if (!section) return false;
      const run = () => {
        const field = document.createElement("span");
        field.className = FIELD_CLASS;
        field.dataset.eoField = kind;
        field.setAttribute("contenteditable", "false");
        const sel = env.getSelection?.();
        const range = sel?.rangeCount ? sel.getRangeAt(0) : null;
        if (range && section.contains(range.commonAncestorContainer)) {
          range.deleteContents();
          range.insertNode(field);
          range.setStartAfter(field);
          range.collapse(true);
          sel.removeAllRanges();
          sel.addRange(range);
        } else {
          lastParagraph(section).appendChild(field);
        }
        return { changed: true };
      };
      if (deps.withUndo) deps.withUndo(run);
      else run();
      sync();
      return true;
    }

    // ---- wiring -------------------------------------------------------------------------------

    function onKeydown(ev) {
      if (editing && ev.key === "Escape") {
        ev.preventDefault?.();
        endEdit();
      }
    }

    function bindToolbar() {
      const group = toolbar()?.querySelector(".tb-headerfooter-group");
      if (!group) return;
      const actions = {
        close: endEdit,
        delete: removeEditing,
        "page-number": () => insertField("page"),
        "total-pages": () => insertField("pages"),
        "file-name": () => insertField("filename"),
      };
      for (const btn of group.querySelectorAll("[data-hf-action]")) {
        btn.addEventListener("click", () => actions[btn.dataset.hfAction]?.());
      }
    }

    function attach(root, pagesApi) {
      detach();
      editor = root;
      pages = pagesApi;
      const Observer = env.MutationObserver;
      if (typeof Observer === "function") {
        observer = new Observer(onMutations);
        observer.observe(editor, { childList: true, characterData: true, subtree: true, attributes: true, attributeFilter: ["contenteditable"] });
      }
      editor.addEventListener("keydown", onKeydown);
      pages?.onSheets?.(() => paint());
      bindToolbar();
      schedule();
    }

    function detach() {
      observer?.disconnect();
      observer = null;
      editor?.removeEventListener?.("keydown", onKeydown);
      editor = null;
    }

    const api = {
      attach, detach, open, endEdit, removeEditing, insertField, paint, sync, printCopy,
      isEditing: () => editing,
      has: (kind) => hasContent(sectionOf(kind)),
      // The distances that place the header and footer now: the page setup's, else the default while there is one.
      margins: () => marginsOf() ?? (KINDS.some((kind) => hasContent(sectionOf(kind))) ? { ...DEFAULT_MARGINS } : null),
    };
    host.__EASYOFFICE_DOC_PAGE_FRAME__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_DOC_PAGE_FRAME__ = createPageFrame;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
