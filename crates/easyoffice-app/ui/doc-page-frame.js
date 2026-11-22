(function (host) {
  "use strict";

  // Header and footer of the page (PD-211, PD-249). Each slot (`header`, `footer`, `firstHeader`, `firstFooter`,
  // `evenHeader`, `evenFooter`) is a `<section data-eo-hf="...">` child of `#doc-editor` outside the body flow.
  // When `data-eo-hf-zones="true"` is enabled on a section, its three `<p data-eo-zone="left|center|right">`
  // compartments sit side-by-side in a 3-column horizontal grid on screen, on every paper sheet copy, in print,
  // and across DOCX (`w:ptab`), ODT (`style:region-*`), HWPX, and Pages roundtrips.

  const KINDS = ["header", "footer"];
  const ALL_KINDS = ["header", "footer", "firstHeader", "firstFooter", "evenHeader", "evenFooter"];
  const ZONES = ["left", "center", "right"];
  const FIELDS = { page: "쪽 번호", pages: "총 쪽수", filename: "파일 이름" };
  const DEFAULT_MARGINS = { top: 1701, bottom: 1440, header: 851, footer: 992 };
  const PX_PER_TWIP = 1 / 15;
  const GROWN_GAP_PX = 6;
  const PAGE_MARGIN_AT = { top: 4, bottom: 5, header: 6, footer: 7 };
  const LABELS = { header: "머리말", footer: "꼬리말" };
  const SECTION_CLASS = "eo-page-frame";
  const FIELD_CLASS = "eo-page-field";
  const COPY_CLASS = "doc-page-hf";
  const LAYER_SELECTOR = ":scope > footer.doc-page-layer";
  const MSG_NOT_PAGED = "쪽 보기에서만 편집";

  const isFrame = (el) => el?.nodeType === 1 && el.tagName === "SECTION" && ALL_KINDS.includes(el.dataset?.eoHf);
  const bandOf = (kind) => (kind === "header" || kind === "firstHeader" || kind === "evenHeader" ? "header" : "footer");
  const variantOf = (kind) => (String(kind || "").startsWith("first") ? "first" : String(kind || "").startsWith("even") ? "even" : "default");
  const slotOf = (band, variant) => {
    const cap = band === "header" ? "Header" : "Footer";
    if (variant === "first") return "first" + cap;
    if (variant === "even") return "even" + cap;
    return band;
  };

  function replaceKids(parent, kids) {
    while (parent.firstChild) parent.removeChild(parent.firstChild);
    for (const kid of kids) parent.appendChild(kid);
  }

  function hasNodeContent(node) {
    return Boolean(node) && (String(node.textContent || "").trim() !== "" || node.querySelector?.("." + FIELD_CLASS) !== null);
  }

  function parseMargins(text) {
    const parts = String(text ?? "").split(",").map((v) => Number(v));
    if ((parts.length !== 8 && parts.length !== 10) || !parts.every((v) => Number.isInteger(v) && v >= 0)) return null;
    return Object.fromEntries(Object.entries(PAGE_MARGIN_AT).map(([key, at]) => [key, parts[at]]));
  }

  const lastParagraph = (section) => Array.from(section.children).at(-1) ?? section;

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
    let lastZoneIdx = 0;
    let savedRange = null;
    const bound = [];

    const layerOf = () => editor?.querySelector(LAYER_SELECTOR) ?? null;
    const sectionOf = (kind) => (editor ? Array.from(editor.children).find((el) => isFrame(el) && el.dataset.eoHf === kind) ?? null : null);
    const scrollOf = () => editor?.closest?.(".doc-scroll-container") ?? document.querySelector(".doc-scroll-container");
    const toolbar = () => document.getElementById("icon-toolbar");
    const marginsOf = () => parseMargins(editor?.dataset.eoPage);
    const fileName = () => String(deps.getFileName?.() ?? "");
    const isFirstDiff = () => editor?.dataset.eoFirstPage === "true";
    const isEvenOddDiff = () => editor?.dataset.eoEvenOdd === "true";

    function scopeLabel(kind) {
      const v = variantOf(kind);
      if (v === "first") return "첫 쪽";
      if (v === "even") return "짝수 쪽";
      return isEvenOddDiff() ? "홀수 쪽" : "양쪽";
    }

    function statusTitle(kind) {
      const base = LABELS[bandOf(kind)];
      const scope = scopeLabel(kind);
      return scope === "양쪽" ? base : scope + " " + base;
    }

    function makeZoneParagraph(zone, sourceNodes) {
      const p = document.createElement("p");
      p.dataset.eoZone = zone;
      p.style.textAlign = zone;
      const nodes = (sourceNodes || []).filter((n) => n.nodeType !== 1 || n.tagName !== "BR" || sourceNodes.length > 1);
      if (nodes.length > 0) {
        for (const n of nodes) p.appendChild(n);
      } else {
        p.appendChild(document.createElement("br"));
      }
      return p;
    }

    function isValidThreeZones(section) {
      const kids = Array.from(section.children);
      return kids.length === 3 && ZONES.every((z, i) => kids[i].tagName === "P" && kids[i].dataset?.eoZone === z);
    }

    function ensureThreeZones(section) {
      if (isValidThreeZones(section)) return;
      const buckets = [[], [], []];
      for (const child of Array.from(section.children)) {
        const align = String(child.dataset?.eoZone || child.style?.textAlign || "").trim().toLowerCase();
        const idx = align === "center" ? 1 : align === "right" ? 2 : 0;
        if (!hasNodeContent(child)) continue;
        if (buckets[idx].length > 0) buckets[idx].push(document.createElement("br"));
        for (const node of Array.from(child.childNodes)) buckets[idx].push(node.cloneNode(true));
      }
      replaceKids(section, ZONES.map((zone, idx) => makeZoneParagraph(zone, buckets[idx])));
    }

    function create(kind, useZones = false) {
      const section = document.createElement("section");
      section.className = SECTION_CLASS;
      section.dataset.eoHf = kind;
      if (useZones) {
        section.dataset.eoHfZones = "true";
        ensureThreeZones(section);
      } else {
        const p = document.createElement("p");
        p.appendChild(document.createElement("br"));
        section.appendChild(p);
      }
      const layer = layerOf();
      if (layer) layer.before(section);
      else editor.appendChild(section);
      return section;
    }

    // ---- copies on paper sheets & print -------------------------------------------------------

    function copyOf(section, band = bandOf(section.dataset.eoHf)) {
      const copy = document.createElement("div");
      copy.className = COPY_CLASS;
      copy.dataset.hf = band;
      if (section.dataset.eoHfZones === "true") copy.dataset.hfZones = "true";
      copy.setAttribute("aria-hidden", "true");
      for (const p of section.children) copy.appendChild(p.cloneNode(true));
      for (const el of copy.querySelectorAll("[id], [contenteditable]")) {
        el.removeAttribute("id");
        el.removeAttribute("contenteditable");
      }
      for (const el of copy.querySelectorAll('[data-eo-field="filename"]')) el.dataset.eoName = fileName();
      return copy;
    }

    function targetSlotForSheet(band, pageIndex) {
      if (pageIndex === 0 && isFirstDiff()) return slotOf(band, "first");
      if (pageIndex % 2 === 1 && isEvenOddDiff()) return slotOf(band, "even");
      return band;
    }

    function editSheetIndex(slot, pageCount) {
      const band = bandOf(slot);
      for (let i = 0; i < pageCount; i += 1) {
        if (targetSlotForSheet(band, i) === slot) return i;
      }
      return 0;
    }

    function sheetSection(band, pageIndex, editIdx = -1) {
      if (editing && bandOf(editing) === band && pageIndex === editIdx) return sectionOf(editing);
      if (pageIndex === 0 && isFirstDiff()) {
        const first = sectionOf(slotOf(band, "first"));
        return hasNodeContent(first) ? first : null;
      }
      if (pageIndex % 2 === 1 && isEvenOddDiff()) {
        const even = sectionOf(slotOf(band, "even"));
        if (hasNodeContent(even)) return even;
      }
      const base = sectionOf(band);
      return hasNodeContent(base) ? base : null;
    }

    function printCopy(kind, pageNumber, total) {
      const band = bandOf(kind);
      const section = sheetSection(band, Math.max(0, (Number(pageNumber) || 1) - 1), -1);
      if (!section || !hasNodeContent(section)) return null;
      const copy = copyOf(section, band);
      for (const field of copy.querySelectorAll("." + FIELD_CLASS)) {
        const text = field.dataset.eoField === "pages" ? total : field.dataset.eoField === "filename" ? fileName() : pageNumber;
        field.parentNode.insertBefore(document.createTextNode(String(text)), field);
        field.parentNode.removeChild(field);
      }
      return copy;
    }

    function setVar(name, value) {
      const style = editor.style;
      if (value === null) {
        if (style.getPropertyValue(name)) style.removeProperty(name);
      } else if (style.getPropertyValue(name) !== value) {
        style.setProperty(name, value);
      }
    }

    function paint() {
      const layer = layerOf();
      if (!layer) return;
      const allSections = ALL_KINDS.map((kind) => sectionOf(kind));
      const sheets = layer.children.length;
      const pageCount = Math.max(1, sheets);
      const editIdx = editing ? editSheetIndex(editing, pageCount) : -1;
      const anyHeader = Array.from({ length: pageCount }, (_, idx) => sheetSection("header", idx, editIdx)).find(Boolean) ?? null;
      const anyFooter = Array.from({ length: pageCount }, (_, idx) => sheetSection("footer", idx, editIdx)).find(Boolean) ?? null;
      const sections = [anyHeader, anyFooter];
      const pitch = Number(pages?.geometry?.()?.pitch) || 0;
      const key = [
        allSections.map((s) => (s ? (s.dataset.eoHfZones || "") + ":" + s.innerHTML : "")).join("\u0001"),
        editing,
        editIdx,
        pitch,
        sheets,
        fileName(),
        editor.dataset.eoPage,
        editor.dataset.eoFirstPage,
        editor.dataset.eoEvenOdd,
      ].join("|");
      [["data-hf-header", anyHeader], ["data-hf-footer", anyFooter]].forEach(([name, section]) => {
        if (!section) layer.removeAttribute(name);
        else if (!layer.hasAttribute(name)) layer.setAttribute(name, "");
      });
      const counters = "eo-page 0 eo-total " + sheets;
      if (layer.style.getPropertyValue("counter-reset") !== counters) layer.style.setProperty("counter-reset", counters);
      const previewPage = editing ? (variantOf(editing) === "even" ? Math.max(2, editIdx + 1) : editIdx + 1) : 1;
      setVar("counter-reset", editing ? "hf-edit-page " + previewPage + " hf-edit-total " + pageCount : null);
      setVar("--hf-edit-sheet-y", editing && editIdx > 0 && pitch > 0 ? Math.round(editIdx * pitch * 100) / 100 + "px" : null);
      if (key === paintedKey && sheets > 0) return;
      paintedKey = key;
      Array.from(layer.children).forEach((sheet, index) => {
        for (const old of Array.from(sheet.querySelectorAll(":scope > ." + COPY_CLASS))) old.remove();
        KINDS.forEach((band) => {
          const section = sheetSection(band, index, editIdx);
          if (!section || (editing && bandOf(editing) === band && index === editIdx)) return;
          sheet.appendChild(copyOf(section, band));
        });
      });
      applyGeometry(sections, layer);
    }

    function applyGeometry(sections, layer) {
      const m = marginsOf() ?? DEFAULT_MARGINS;
      const px = (twips) => Math.round(twips * PX_PER_TWIP * 100) / 100;
      const [header, footer] = sections;
      const band = (kind) => px(kind === "header" ? Math.max(0, m.top - m.header) : Math.max(0, m.bottom - m.footer));
      const grown = (kind) => {
        let maxH = editing && bandOf(editing) === kind ? Number(sectionOf(editing)?.offsetHeight) || 0 : 0;
        for (const sh of Array.from(layer.children)) {
          const h = Number(sh.querySelector(":scope > ." + COPY_CLASS + '[data-hf="' + kind + '"]')?.offsetHeight) || 0;
          if (h > maxH) maxH = h;
        }
        return maxH;
      };
      setVar("--hf-hd", header ? px(m.header) + "px" : null);
      setVar("--hf-band-h", header ? band("header") + "px" : null);
      setVar("--hf-ft", footer ? px(m.footer) + "px" : null);
      setVar("--hf-fband-h", footer ? band("footer") + "px" : null);
      const headerHeight = header ? grown("header") : 0;
      const footerHeight = footer ? grown("footer") : 0;
      const room = (distance, height, area, margin) => Math.max(margin, distance + height + (height > area + 0.5 ? GROWN_GAP_PX : 0));
      setVar("--pg-hf-top", header ? room(px(m.header), headerHeight, band("header"), px(m.top)) + "px" : null);
      setVar("--pg-hf-bottom", footer ? room(px(m.footer), footerHeight, band("footer"), px(m.bottom)) + "px" : null);
      const liveFooterH = editing && bandOf(editing) === "footer" ? (Number(sectionOf(editing)?.offsetHeight) || footerHeight) : 0;
      setVar("--hf-fh", footer && editing && bandOf(editing) === "footer" ? liveFooterH + "px" : null);
    }

    function schedule() {
      if (scheduled || !editor) return;
      scheduled = true;
      raf(() => {
        scheduled = false;
        sync();
      });
    }

    function updateToolbar() {
      const bar = toolbar();
      const group = bar?.querySelector(".tb-headerfooter-group");
      if (!group) return;
      group.hidden = !editing;
      const section = editing ? sectionOf(editing) : null;
      const isZones = section?.dataset.eoHfZones === "true";
      const label = group.querySelector(".tb-hf-label");
      if (label) {
        label.textContent = editing
          ? LABELS[bandOf(editing)] + " · " + scopeLabel(editing) + " 편집 중"
          : "";
      }
      const switchBtn = group.querySelector('[data-hf-action="switch-band"]');
      if (switchBtn) {
        const isHdr = !editing || bandOf(editing) === "header";
        const moveText = isHdr ? "바닥글로 이동" : "머리글로 이동";
        const moveLead = isHdr ? "바닥글로" : "머리글로";
        switchBtn.setAttribute("title", moveText);
        switchBtn.setAttribute("aria-label", moveText);
        const switchLbl = switchBtn.querySelector(".tb-label");
        if (switchLbl) {
          replaceKids(switchLbl, [document.createTextNode(moveLead), document.createElement("br"), document.createTextNode("이동")]);
        }
      }
      const setPressed = (action, on) => {
        const btn = group.querySelector('[data-hf-action="' + action + '"]');
        if (btn) btn.setAttribute("aria-pressed", on ? "true" : "false");
      };
      setPressed("toggle-first", isFirstDiff());
      setPressed("toggle-even-odd", isEvenOddDiff());
      setPressed("toggle-zones", Boolean(isZones));
    }

    function sync() {
      if (!editor) return;
      if (editing && !sectionOf(editing)) endEdit();
      for (const kind of ALL_KINDS) {
        const section = sectionOf(kind);
        if (!section) continue;
        if (section.dataset.eoHfZones === "true") ensureThreeZones(section);
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
      updateToolbar();
      paint();
    }

    function onMutations(records) {
      if (records.every((r) => layerOf()?.contains(r.target))) return;
      schedule();
    }

    // ---- caret & zone tracking ---------------------------------------------------------------

    function rememberSelection() {
      if (!editing || !editor) return;
      const section = sectionOf(editing);
      const sel = env.getSelection?.();
      if (!section || !sel?.rangeCount) return;
      const range = sel.getRangeAt(0);
      if (!section.contains(range.commonAncestorContainer)) return;
      savedRange = range.cloneRange();
      if (section.dataset.eoHfZones === "true") {
        const zones = Array.from(section.children);
        const idx = zones.findIndex((p) => p === range.startContainer || p.contains?.(range.startContainer));
        if (idx >= 0) lastZoneIdx = idx;
      }
    }

    function focusZone(section, zoneIdx = 0) {
      if (!section) return false;
      const zones = Array.from(section.children);
      const target = section.dataset.eoHfZones === "true"
        ? (zones[Math.max(0, Math.min(zones.length - 1, zoneIdx))] ?? lastParagraph(section))
        : lastParagraph(section);
      lastZoneIdx = Math.max(0, zones.indexOf(target));
      section.focus?.();
      const sel = env.getSelection?.();
      if (sel && typeof document.createRange === "function") {
        const range = document.createRange();
        range.selectNodeContents(target);
        if (target.childNodes.length === 1 && target.firstChild?.nodeType === 1 && target.firstChild.tagName === "BR") {
          range.setStart(target, 0);
        }
        range.collapse(false);
        sel.removeAllRanges();
        sel.addRange(range);
        savedRange = range.cloneRange();
      }
      return true;
    }

    function setEditState(kind) {
      editing = kind;
      savedRange = null;
      lastZoneIdx = 0;
      if (kind) editor.dataset.eoHfEdit = kind;
      else {
        delete editor.dataset.eoHfEdit;
        setVar("counter-reset", null);
        setVar("--hf-edit-sheet-y", null);
      }
      editor.setAttribute("contenteditable", kind ? "false" : "true");
      scrollOf()?.classList.toggle("hf-editing", Boolean(kind));
      const bar = toolbar();
      if (bar) bar.dataset.hfEditing = kind || "";
      updateToolbar();
      deps.onEditingChange?.(kind);
    }

    function pruneEmptySlot(kind) {
      const section = kind ? sectionOf(kind) : null;
      if (section && !hasNodeContent(section)) section.remove();
    }

    function openSlot(kind, undoable = true) {
      if (!editor || !ALL_KINDS.includes(kind)) return false;
      if (editor.classList.contains("manuscript-grid")) {
        deps.setFeedback?.(MSG_NOT_PAGED);
        return false;
      }
      if (editing && editing !== kind) {
        const prev = editing;
        setEditState(null);
        pruneEmptySlot(prev);
      }
      if (!sectionOf(kind)) {
        const siblingZones = sectionOf(bandOf(kind))?.dataset.eoHfZones === "true";
        const run = () => {
          if (variantOf(kind) === "first") editor.dataset.eoFirstPage = "true";
          if (variantOf(kind) === "even") editor.dataset.eoEvenOdd = "true";
          create(kind, siblingZones);
          return { changed: true };
        };
        if (undoable && deps.withUndo) deps.withUndo(run);
        else run();
      }
      const section = sectionOf(kind);
      setEditState(kind);
      sync();
      focusZone(section, 0);
      section.scrollIntoView?.({ block: "nearest" });
      deps.setFeedback?.(statusTitle(kind) + " 편집 중");
      return true;
    }

    function open(requestedKind) {
      if (!editor) return false;
      if (KINDS.includes(requestedKind)) {
        const curPage = Math.max(1, Number(pages?.currentPage?.()) || 1);
        return openSlot(targetSlotForSheet(requestedKind, curPage - 1), true);
      }
      return openSlot(requestedKind, true);
    }

    function activeSlots() {
      const slots = [];
      if (isFirstDiff()) slots.push("firstHeader", "firstFooter");
      slots.push("header", "footer");
      if (isEvenOddDiff()) slots.push("evenHeader", "evenFooter");
      return slots;
    }

    function navigateSlot(delta = 1) {
      if (!editing) return false;
      const slots = activeSlots();
      const cur = Math.max(0, slots.indexOf(editing));
      const next = slots[(cur + delta + slots.length * 4) % slots.length];
      return next && next !== editing ? openSlot(next, true) : true;
    }

    function switchBand() {
      if (!editing) return false;
      const otherBand = bandOf(editing) === "header" ? "footer" : "header";
      return openSlot(slotOf(otherBand, variantOf(editing)), true);
    }

    function toggleFirstPage() {
      if (!editor) return false;
      const nextOn = !isFirstDiff();
      const curBand = editing ? bandOf(editing) : "header";
      const run = () => {
        if (nextOn) {
          editor.dataset.eoFirstPage = "true";
          if (!sectionOf(slotOf(curBand, "first"))) {
            create(slotOf(curBand, "first"), sectionOf(curBand)?.dataset.eoHfZones === "true");
          }
        } else {
          delete editor.dataset.eoFirstPage;
        }
        return { changed: true };
      };
      if (deps.withUndo) deps.withUndo(run);
      else run();
      if (editing) {
        if (nextOn) return openSlot(slotOf(curBand, "first"), false);
        if (variantOf(editing) === "first") return openSlot(curBand, false);
      }
      sync();
      deps.setFeedback?.("첫 페이지를 다르게 지정");
      return true;
    }

    function toggleEvenOdd() {
      if (!editor) return false;
      const nextOn = !isEvenOddDiff();
      const curBand = editing ? bandOf(editing) : "header";
      const run = () => {
        if (nextOn) {
          editor.dataset.eoEvenOdd = "true";
          if (!sectionOf(slotOf(curBand, "even"))) {
            create(slotOf(curBand, "even"), sectionOf(curBand)?.dataset.eoHfZones === "true");
          }
        } else {
          delete editor.dataset.eoEvenOdd;
        }
        return { changed: true };
      };
      if (deps.withUndo) deps.withUndo(run);
      else run();
      if (editing) {
        if (nextOn) return openSlot(slotOf(curBand, "even"), false);
        if (variantOf(editing) === "even") return openSlot(curBand, false);
      }
      sync();
      deps.setFeedback?.("짝수와 홀수 페이지를 다르게 지정");
      return true;
    }

    function toggleZones() {
      const section = editing ? sectionOf(editing) : null;
      if (!section) return false;
      const enable = section.dataset.eoHfZones !== "true";
      const run = () => {
        if (enable) {
          section.dataset.eoHfZones = "true";
          ensureThreeZones(section);
        } else {
          const kept = Array.from(section.children).filter((p) => hasNodeContent(p));
          if (kept.length === 0) {
            const p = document.createElement("p");
            p.appendChild(document.createElement("br"));
            kept.push(p);
          } else {
            for (const p of kept) {
              const z = p.dataset?.eoZone;
              delete p.dataset.eoZone;
              if (z === "left") p.style.textAlign = "";
              else if (z === "center" || z === "right") p.style.textAlign = z;
            }
          }
          delete section.dataset.eoHfZones;
          replaceKids(section, kept);
        }
        return { changed: true };
      };
      if (deps.withUndo) deps.withUndo(run);
      else run();
      sync();
      focusZone(section, 0);
      deps.setFeedback?.(enable ? "빈 문서(3열)" : statusTitle(editing) + " 편집 중");
      return true;
    }

    function endEdit() {
      if (!editing) return false;
      const kind = editing;
      const title = statusTitle(kind);
      setEditState(null);
      pruneEmptySlot(kind);
      sync();
      editor.focus?.();
      deps.setFeedback?.(title + " 닫음");
      return true;
    }

    function removeEditing() {
      const kind = editing;
      if (!kind) return false;
      const title = statusTitle(kind);
      setEditState(null);
      const run = () => {
        sectionOf(kind)?.remove();
        return { changed: true };
      };
      if (deps.withUndo) deps.withUndo(run);
      else run();
      sync();
      editor.focus?.();
      deps.setFeedback?.(title + " 지움");
      return true;
    }

    function targetParagraphForInsert(section) {
      if (section.dataset.eoHfZones === "true") {
        ensureThreeZones(section);
        return section.children[Math.max(0, Math.min(2, lastZoneIdx))] ?? lastParagraph(section);
      }
      return lastParagraph(section);
    }

    function stripPlaceholderBr(p) {
      if (p?.childNodes?.length === 1 && p.firstChild?.nodeType === 1 && p.firstChild.tagName === "BR") {
        p.removeChild(p.firstChild);
      }
    }

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
        const liveRange = sel?.rangeCount ? sel.getRangeAt(0) : null;
        const range = liveRange && section.contains(liveRange.commonAncestorContainer)
          ? liveRange
          : (savedRange && section.contains(savedRange.commonAncestorContainer) ? savedRange.cloneRange() : null);
        if (range) {
          const hostP = Array.from(section.children).find((p) => p === range.startContainer || p.contains?.(range.startContainer));
          range.deleteContents();
          range.insertNode(field);
          if (hostP && hostP.childNodes.length === 2 && hostP.lastChild?.tagName === "BR") {
            hostP.removeChild(hostP.lastChild);
          }
          range.setStartAfter(field);
          range.collapse(true);
          if (sel) {
            sel.removeAllRanges();
            sel.addRange(range);
          }
          savedRange = range.cloneRange();
        } else {
          const target = targetParagraphForInsert(section);
          stripPlaceholderBr(target);
          target.appendChild(field);
        }
        return { changed: true };
      };
      if (deps.withUndo) deps.withUndo(run);
      else run();
      sync();
      return true;
    }

    // ---- wiring -------------------------------------------------------------------------------

    function insertZoneLineBreak(section) {
      const sel = env.getSelection?.();
      if (!sel?.rangeCount || typeof document.createRange !== "function") return;
      const range = sel.getRangeAt(0);
      if (!section.contains(range.commonAncestorContainer)) return;
      range.deleteContents();
      const br = document.createElement("br");
      range.insertNode(br);
      range.setStartAfter(br);
      range.collapse(true);
      sel.removeAllRanges();
      sel.addRange(range);
      savedRange = range.cloneRange();
      schedule();
    }

    function onKeydown(ev) {
      if (!editing) return;
      if (ev.key === "Escape") {
        ev.preventDefault?.();
        endEdit();
        return;
      }
      const section = sectionOf(editing);
      if (!section || section.dataset.eoHfZones !== "true") return;
      if (ev.key === "Tab") {
        rememberSelection();
        ev.preventDefault?.();
        const delta = ev.shiftKey ? -1 : 1;
        focusZone(section, (lastZoneIdx + delta + 3) % 3);
        return;
      }
      if (ev.key === "Enter" && !ev.ctrlKey && !ev.metaKey) {
        ev.preventDefault?.();
        insertZoneLineBreak(section);
      }
    }

    function listen(target, type, fn, options) {
      if (!target?.addEventListener) return;
      target.addEventListener(type, fn, options);
      bound.push(() => target.removeEventListener?.(type, fn, options));
    }

    function bindToolbar() {
      const group = toolbar()?.querySelector(".tb-headerfooter-group");
      if (!group) return;
      const actions = {
        prev: () => navigateSlot(-1),
        next: () => navigateSlot(1),
        "switch-band": switchBand,
        close: endEdit,
        delete: removeEditing,
        "page-number": () => insertField("page"),
        "total-pages": () => insertField("pages"),
        "file-name": () => insertField("filename"),
        "toggle-first": toggleFirstPage,
        "toggle-even-odd": toggleEvenOdd,
        "toggle-zones": toggleZones,
      };
      for (const btn of group.querySelectorAll("[data-hf-action]")) {
        listen(btn, "mousedown", (ev) => {
          rememberSelection();
          ev.preventDefault?.();
        });
        listen(btn, "click", () => actions[btn.dataset.hfAction]?.());
      }
    }

    function attach(root, pagesApi) {
      detach();
      editor = root;
      pages = pagesApi;
      const Observer = env.MutationObserver;
      if (typeof Observer === "function") {
        observer = new Observer(onMutations);
        observer.observe(editor, {
          childList: true,
          characterData: true,
          subtree: true,
          attributes: true,
          attributeFilter: ["contenteditable", "data-eo-first-page", "data-eo-even-odd", "data-eo-hf-zones"],
        });
      }
      listen(editor, "keydown", onKeydown);
      listen(editor, "keyup", rememberSelection);
      listen(editor, "mouseup", rememberSelection);
      listen(document, "selectionchange", rememberSelection);
      const offSheets = pages?.onSheets?.(() => paint());
      if (typeof offSheets === "function") bound.push(offSheets);
      bindToolbar();
      schedule();
    }

    function detach() {
      observer?.disconnect();
      observer = null;
      for (const off of bound.splice(0)) off();
      editor = null;
      pages = null;
      savedRange = null;
    }

    const api = {
      attach, detach, open, openSlot, endEdit, removeEditing, insertField, paint, sync, printCopy,
      navigateSlot, switchBand, toggleFirstPage, toggleEvenOdd, toggleZones, focusZone, activeSlots,
      isEditing: () => editing,
      has: (kind) => hasNodeContent(sectionOf(kind)),
      margins: () => marginsOf() ?? (ALL_KINDS.some((kind) => hasNodeContent(sectionOf(kind))) ? { ...DEFAULT_MARGINS } : null),
    };
    host.__EASYOFFICE_DOC_PAGE_FRAME__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_DOC_PAGE_FRAME__ = createPageFrame;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
