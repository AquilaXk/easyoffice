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
  const LABELS = { header: "머리말", footer: "꼬리말" };
  const APPLY_LABEL = "양쪽";
  const SECTION_CLASS = "eo-page-frame";
  const FIELD_CLASS = "eo-page-field";
  const COPY_CLASS = "doc-page-hf";
  const LAYER_SELECTOR = ":scope > footer.doc-page-layer";
  const MSG_NOT_PAGED = "쪽 보기에서만 편집";

  const isFrame = (el) => el?.nodeType === 1 && el.tagName === "SECTION" && el.dataset?.eoHf !== undefined;

  const lastParagraph = (section) => Array.from(section.children).at(-1) ?? section;

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
      return copy;
    }

    // Draws the header and footer on every sheet of the page layer; the sheet of page 1 shows the live section
    // instead of a copy while that one is edited. Only a changed content, a new sheet or an edit state redraws.
    function paint() {
      const layer = layerOf();
      if (!layer) return;
      const sections = KINDS.map((kind) => sectionOf(kind)).map((s) => (hasContent(s) || s === sectionOf(editing) ? s : null));
      const key = sections.map((s) => (s ? s.innerHTML : "")).join("\u0001") + "|" + editing + "|" + layer.children.length;
      [["data-hf-header", sections[0]], ["data-hf-footer", sections[1]]].forEach(([name, section]) => {
        if (!section) layer.removeAttribute(name);
        else if (!layer.hasAttribute(name)) layer.setAttribute(name, "");
      });
      if (key === paintedKey && layer.children.length > 0) return;
      paintedKey = key;
      Array.from(layer.children).forEach((sheet, index) => {
        for (const old of Array.from(sheet.querySelectorAll(":scope > ." + COPY_CLASS))) old.remove();
        sections.forEach((section, i) => {
          if (!section || (index === 0 && editing === KINDS[i])) return;
          sheet.appendChild(copyOf(section));
        });
      });
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

    // A page-number field at the caret of the header or footer being edited.
    function insertPageNumber() {
      const section = editing ? sectionOf(editing) : null;
      if (!section) return false;
      const run = () => {
        const field = document.createElement("span");
        field.className = FIELD_CLASS;
        field.dataset.eoField = "page";
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
      const actions = { close: endEdit, delete: removeEditing, "page-number": insertPageNumber };
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
      attach, detach, open, endEdit, removeEditing, insertPageNumber, paint, sync,
      isEditing: () => editing,
      has: (kind) => hasContent(sectionOf(kind)),
    };
    host.__EASYOFFICE_DOC_PAGE_FRAME__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_DOC_PAGE_FRAME__ = createPageFrame;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
