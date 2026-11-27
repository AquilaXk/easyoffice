(function (host) {
  "use strict";

  // Document-editor character formatting controller (PD-077): applies size, font, color,
  // highlight, bold, italic, underline, and strikethrough to the SELECTION only through the run
  // engine (doc-runs.js), keeps the toolbar in sync with the caret/selection, and acts on the
  // current paragraphs for indentation. Conventions for a collapsed caret:
  //   - caret strictly inside a word  -> the whole word is formatted;
  //   - caret at a word edge / empty paragraph -> a "pending" format applies to the next typed
  //     text (cleared when the caret moves).

  const BOOL_BUTTONS = {
    bold: "btn-stylebar-bold",
    italic: "btn-stylebar-italic",
    underline: "btn-stylebar-underline",
    strike: "btn-stylebar-strike",
    superscript: "btn-stylebar-sup",
    subscript: "btn-stylebar-sub",
  };
  const MIN_PT = 1;
  const MAX_PT = 4096;
  const DEFAULT_COLORS = { color: "FF0000", highlight: "FFFF00" };
  const INDENT_LIMITS = { paddingLeft: [0, 192], textIndent: [-96, 192], paddingRight: [0, 192] };
  const RULER_PINS = {
    "ruler-pin-first-indent": "textIndent",
    "ruler-pin-left-margin": "paddingLeft",
    "ruler-pin-right-margin": "paddingRight",
  };
  const INDENT_STEP_PX = 4;
  // Paragraph alignment buttons. The rich model has left, center, right and justify only, so the
  // distributed and divided modes are shown per paragraph and saved as justify.
  const ALIGN_CSS = { left: "left", center: "center", right: "right", justify: "justify", distribute: "justify", divide: "justify" };
  const ALIGN_BUTTON_MODES = Object.keys(ALIGN_CSS);
  const APPROXIMATED_ALIGN_LABEL = { distribute: "배분", divide: "나눔" };
  const MIN_LINE_PERCENT = 5;
  const MAX_LINE_PERCENT = 500;
  const DEFAULT_LINE_PERCENT = 108;
  const HWP_LINE_PERCENT = 160;
  const DOC_DEFAULT_FONT = "맑은 고딕";
  const HWP_DEFAULT_FONT = "함초롬바탕";
  const NON_PARAGRAPH_TAGS = new Set(["TD", "TH", "CAPTION"]);
  const HEADING_TAG_RE = /^H[1-6]$/;
  const STYLE_HEADING_RE = /^(?:개요|제목|머리말)\s+([1-6])$/;
  const PLAIN_STYLES = new Set(["바탕글", "표준", "본문"]);
  const PAGES_EXPLICIT_STYLES = { 제목: 1, 부제: 2, 부제목: 2, 머리말: 1 };
  const DOC_STYLE_OPTIONS = {
    doc: ["표준", "제목 1", "제목 2", "제목 3", "제목 4", "제목 5", "제목 6"],
    hwp: ["바탕글", "개요 1", "개요 2", "개요 3", "개요 4", "개요 5", "개요 6"],
    pages: ["본문", "제목", "부제", "머리말", "머리말 2", "머리말 3"],
  };
  const LIST_TAGS = { ordered: "OL", bullet: "UL", task: "UL" };
  const LIST_TAG_NAMES = new Set(Object.values(LIST_TAGS));
  const LIST_BUTTON_CMDS = { ordered: "para-numbering", bullet: "para-bullet", task: "para-checklist" };
  // Look carried over when a paragraph changes its tag (the style whitelist of the saved document state).
  const CARRIED_STYLE_PROPS = [
    "paddingLeft", "paddingRight", "textIndent", "marginTop", "marginBottom", "lineHeight", "textAlign",
    "fontWeight", "fontStyle", "textDecoration", "fontSize", "fontFamily", "color", "backgroundColor",
  ];
  const CARRIED_ATTRS = ["class", "id", "lang"];

  function plainRange(r) {
    return {
      startContainer: r.startContainer,
      startOffset: r.startOffset,
      endContainer: r.endContainer,
      endOffset: r.endOffset,
      collapsed: Boolean(r.collapsed),
    };
  }

  function readPx(blk, prop) {
    return Number.parseInt(blk.style[prop] || "0", 10) || 0;
  }

  const TWIPS_DATA_KEYS = {
    paddingLeft: "eoLeftTwips",
    paddingRight: "eoRightTwips",
    textIndent: "eoFirstLineTwips",
  };

  function writePx(blk, prop, px) {
    const [lo, hi] = INDENT_LIMITS[prop];
    const next = Math.max(lo, Math.min(hi, Math.round(px)));
    blk.style[prop] = next ? next + "px" : "";
    const key = TWIPS_DATA_KEYS[prop];
    if (key && blk.dataset) delete blk.dataset[key];
    return next;
  }

  function createDocRunsUi(deps) {
    const { document, getActiveModule, captureWorkspaceSnapshot, commitDeferredUndoSnapshot, saveDocState, isComposing } = deps;
    const R = host.__EASYOFFICE_DOC_RUNS__;
    let lastRange = null;
    let pending = null;
    let syncQueued = false;
    const lastColor = { ...DEFAULT_COLORS };
    const elCache = new Map();

    function byId(id) {
      const cached = elCache.get(id);
      if (cached && cached.isConnected !== false) return cached;
      const found = document.getElementById(id);
      if (found) elCache.set(id, found);
      return found;
    }

    const editor = () => byId("doc-editor");
    const isDocModule = () => ["DOC", "HWP"].includes(getActiveModule());
    const defaultLinePercent = () => (getActiveModule() === "HWP" ? HWP_LINE_PERCENT : DEFAULT_LINE_PERCENT);
    const defaultFontName = () => (getActiveModule() === "HWP" ? HWP_DEFAULT_FONT : DOC_DEFAULT_FONT);

    function setFeedback(text) {
      const fb = byId("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    function insideEditor(range) {
      const root = editor();
      return Boolean(root && range && root.contains(range.startContainer) && root.contains(range.endContainer));
    }

    function currentRange() {
      const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
      if (sel?.rangeCount > 0) {
        const live = plainRange(sel.getRangeAt(0));
        if (insideEditor(live)) return live;
      }
      return insideEditor(lastRange) ? lastRange : null;
    }

    function setSelection(range) {
      const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
      if (!sel || typeof document.createRange !== "function" || !range) return;
      try {
        const r = document.createRange();
        r.setStart(range.startContainer, range.startOffset);
        r.setEnd(range.endContainer, range.endOffset);
        sel.removeAllRanges();
        sel.addRange(r);
        lastRange = plainRange(r);
      } catch {
        // A stale node (e.g. after an undo restore) simply leaves the selection unchanged.
      }
    }

    function baseHalfPt() {
      const root = editor();
      return R.cssSizeToHalfPt(root?.style?.fontSize) || R.DEFAULT_HALF_PT;
    }

    function withPending(state) {
      if (!pending || !state) return state;
      const out = { ...state };
      Object.keys(pending.patch).forEach((p) => {
        const v = pending.patch[p];
        if (R.BOOL_PROP_SET.has(p)) {
          out[p] = v === true;
        } else {
          out[p] = v ?? (p === "size" ? baseHalfPt() : "");
        }
      });
      return out;
    }

    function getState() {
      const root = editor();
      const range = currentRange();
      if (!root || !range) return null;
      return withPending(R.queryFormat(root, range, { baseHalfPt: baseHalfPt() }));
    }

    function withUndo(exec) {
      const pre = typeof captureWorkspaceSnapshot === "function" ? captureWorkspaceSnapshot(false) : null;
      const res = exec();
      if (res.changed) {
        onMutated();
        if (pre && typeof commitDeferredUndoSnapshot === "function") commitDeferredUndoSnapshot(pre);
      }
      return res;
    }

    // Persists the editor state for the workspace snapshots and tells the autosave tracker.
    function onMutated() {
      saveDocState?.();
      const root = editor();
      // Not an `input` event: that would add a second undo step through the typing tracker.
      if (root && typeof host.Event === "function") root.dispatchEvent(new host.Event("eo-doc-mutated", { bubbles: true }));
    }

    function dropPending() {
      pending = null;
    }

    function setPending(range, patch) {
      const keep = pending && pending.node === range.startContainer && pending.offset === range.startOffset;
      const base = keep ? pending.patch : {};
      pending = { patch: { ...base, ...R.normalizePatch(patch) }, node: range.startContainer, offset: range.startOffset };
    }

    // Applies `patch` to the current selection (or a word / pending format for a caret).
    // `options.range` overrides the selection; `options.undo === false` skips the undo wrapper.
    function applyPatch(patch, options = {}) {
      const root = editor();
      const none = { mode: "none", changed: false };
      if (!root || !isDocModule()) return none;
      const range = options.range || currentRange();
      if (!range) {
        setFeedback("서식을 적용할 글자를 먼저 선택하세요");
        return none;
      }
      let target = range;
      let mode = "range";
      if (range.collapsed) {
        const word = R.wordRangeAt(root, range.startContainer, range.startOffset);
        if (!word) {
          setPending(range, patch);
          refocusEditor(root);
          syncToolbar();
          return { mode: "pending", changed: false };
        }
        target = word;
        mode = "word";
      }
      const caretOffset = mode === "word" ? R.textOffsetOf(root, range.startContainer, range.startOffset) : 0;
      const exec = () => {
        const res = R.applyFormat(root, target, patch);
        if (res.range && mode === "range") setSelection(res.range);
        if (mode === "word") setCaretAtTextOffset(root, caretOffset);
        return res;
      };
      const res = options.undo === false ? exec() : withUndo(exec);
      dropPending();
      refocusEditor(root);
      syncToolbar();
      return { mode, changed: res.changed };
    }

    function setCaretAtTextOffset(root, offset) {
      const p = R.pointAtTextOffset(root, offset, false);
      setSelection({ startContainer: p.node, startOffset: p.offset, endContainer: p.node, endOffset: p.offset, collapsed: true });
    }

    // Toolbar controls (size box, font list) take focus; typing and Ctrl+Z belong to the editor.
    function refocusEditor(root) {
      if (document.activeElement && document.activeElement !== root && typeof root.focus === "function") {
        root.focus({ preventScroll: true });
      }
    }

    function toggle(prop, options) {
      const state = getState();
      const on = state ? state[prop] === true : false;
      return applyPatch({ [prop]: on ? null : true }, options);
    }

    function setSize(pt, options) {
      const clamped = Math.max(MIN_PT, Math.min(MAX_PT, Number.isFinite(pt) ? pt : 10));
      return applyPatch({ size: R.ptToHalfPt(clamped) }, options);
    }

    function bumpSize(deltaPt, options) {
      const state = getState();
      const half = Number.isInteger(state?.size) ? state.size : baseHalfPt();
      return setSize(half / 2 + deltaPt, options);
    }

    const setFont = (font, options) => applyPatch({ font }, options);

    function setColor(prop, hex, options) {
      const norm = R.normalizeColor(hex);
      if (norm) lastColor[prop] = norm;
      return applyPatch({ [prop]: norm }, options);
    }

    // The color buttons apply (or, when already applied, remove) the last chosen color.
    function toggleColor(prop, options) {
      const state = getState();
      const same = state && state[prop] === lastColor[prop];
      return applyPatch({ [prop]: same ? null : lastColor[prop] }, options);
    }

    // ------------------------------------------------------------ toolbar state

    function setPressed(id, value) {
      const el = byId(id);
      if (!el) return;
      let pressed = "false";
      if (value === R.MIXED) pressed = "mixed";
      else if (value === true) pressed = "true";
      el.setAttribute("aria-pressed", pressed);
      el.classList.toggle("active", value === true);
    }

    function syncSizeControls(state) {
      const input = byId("doc-font-size-input");
      const badge = byId("doc-style-size-badge");
      const mixed = state.size === R.MIXED;
      const text = mixed ? "" : (state.size / 2).toFixed(1);
      if (input) {
        input.value = text;
        input.placeholder = mixed ? "혼합" : "";
        input.dataset.mixed = mixed ? "true" : "false";
      }
      if (badge) badge.textContent = mixed ? "혼합" : text + " pt";
    }

    function syncFontControls(state) {
      const select = byId("doc-font-select");
      const badge = byId("doc-style-font-badge");
      const fallback = editor()?.style?.fontFamily?.replace(/^["']|["']$/g, "") || defaultFontName();
      const font = state.font === R.MIXED ? "" : state.font || fallback;
      if (select) select.value = font;
      if (badge) badge.textContent = font || "혼합";
    }

    function syncColorButtons(state) {
      [["btn-stylebar-textcolor", "color"], ["btn-stylebar-highlight", "highlight"]].forEach(([id, prop]) => {
        const btn = byId(id);
        if (!btn) return;
        const v = state[prop];
        btn.setAttribute("aria-pressed", v && v !== R.MIXED ? "true" : "false");
        const shown = v && v !== R.MIXED ? "#" + v : "";
        btn.style.boxShadow = shown ? "inset 0 -3px 0 " + shown : "";
        btn.style.borderBottomColor = shown || "";
      });
    }

    function syncToolbar() {
      const state = getState();
      if (!state) return null;
      Object.entries(BOOL_BUTTONS).forEach(([prop, id]) => setPressed(id, state[prop]));
      syncSizeControls(state);
      syncFontControls(state);
      syncColorButtons(state);
      const blk = targetBlocks()[0];
      syncParagraphControls(blk);
      syncRuler(undefined, blk);
      return state;
    }

    function queueSync() {
      if (syncQueued) return;
      syncQueued = true;
      host.setTimeout?.(() => {
        syncQueued = false;
        if (isDocModule()) syncToolbar();
      }, 0);
    }

    // ------------------------------------------------------------ indentation / ruler

    function targetBlocks() {
      const root = editor();
      const range = currentRange();
      return root && range ? R.blocksInRange(root, range) : [];
    }

    // Indentation acts on the paragraphs the selection touches, never on the whole document.
    function shiftIndent(deltaPx, options = {}) {
      const blocks = targetBlocks();
      if (blocks.length === 0) return { changed: false, blocks: 0 };
      const exec = () => {
        blocks.forEach((b) => writePx(b, "paddingLeft", readPx(b, "paddingLeft") + deltaPx));
        return { changed: true };
      };
      if (options.undo === false) exec();
      else withUndo(exec);
      syncRuler();
      return { changed: true, blocks: blocks.length };
    }

    function setBlockIndent(prop, px, blocks) {
      const targets = blocks || targetBlocks();
      targets.forEach((b) => writePx(b, prop, px));
      return targets.length;
    }


    // ------------------------------------------------------------ paragraph blocks

    function paragraphTargets() {
      return targetBlocks().filter((b) => !NON_PARAGRAPH_TAGS.has((b.tagName || "").toUpperCase()));
    }

    function alignmentTargets() {
      return targetBlocks().filter((b) => (b.tagName || "").toUpperCase() !== "CAPTION");
    }

    // Runs `exec(blocks, range)` on the paragraphs the selection touches as one undo step; `hint` is
    // shown when nothing is selected. `exec` returns false when it found nothing to change, which
    // leaves no undo step.
    function runOnParagraphs(exec, hint, pick = paragraphTargets, options = {}) {
      if (!isDocModule()) return { changed: false, blocks: 0 };
      const blocks = pick();
      if (blocks.length === 0) {
        setFeedback(hint);
        return { changed: false, blocks: 0 };
      }
      const range = currentRange();
      const run = () => ({ changed: exec(blocks, range) !== false });
      const res = options.undo === false ? run() : withUndo(run);
      refocusEditor(editor());
      syncToolbar();
      return { changed: res.changed, blocks: blocks.length };
    }

    function setAlign(mode) {
      const css = ALIGN_CSS[mode];
      if (!css) return { changed: false, blocks: 0 };
      const mark = APPROXIMATED_ALIGN_LABEL[mode] ? mode : undefined;
      const res = runOnParagraphs((blocks) => {
        const stale = blocks.filter((b) => b.style.textAlign !== css || b.dataset.eoAlign !== mark);
        stale.forEach((b) => {
          b.style.textAlign = css;
          if (mark) b.dataset.eoAlign = mark;
          else delete b.dataset.eoAlign;
        });
        return stale.length > 0;
      }, "정렬할 문단을 먼저 선택하세요", alignmentTargets);
      if (res.changed && APPROXIMATED_ALIGN_LABEL[mode]) setFeedback(APPROXIMATED_ALIGN_LABEL[mode] + " 정렬은 저장할 때 양쪽 정렬로 기록됩니다");
      return res;
    }

    // The line spacing list gets an entry for a custom value, then shows it.
    function showLineSpacing(percent) {
      const select = byId("doc-linespacing-select");
      if (!select) return;
      const str = String(percent);
      // Only one custom value is kept in the list: the one that is showing.
      Array.from(select.querySelectorAll("option")).filter((o) => o.dataset.custom && o.value !== str).forEach((o) => o.remove());
      const options = Array.from(select.querySelectorAll("option"));
      if (!options.some((o) => o.value === str)) {
        const opt = document.createElement("option");
        opt.value = str;
        opt.dataset.custom = "true";
        opt.textContent = percent + " %";
        const next = options.find((o) => Number(o.value) > percent);
        select.insertBefore(opt, next || null);
      }
      select.value = str;
      host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(select);
    }

    function setLineSpacing(percent) {
      const clamped = Math.max(MIN_LINE_PERCENT, Math.min(MAX_LINE_PERCENT, Number.isFinite(percent) ? Math.round(percent) : defaultLinePercent()));
      const value = (clamped / 100).toFixed(2);
      const res = runOnParagraphs((blocks) => {
        const stale = blocks.filter((b) => b.style.lineHeight !== value);
        stale.forEach((b) => {
          b.style.lineHeight = value;
          delete b.dataset.eoLineSpacing; // the exact value read from a file no longer matches
        });
        return stale.length > 0;
      }, "줄 간격을 적용할 문단을 먼저 선택하세요", alignmentTargets);
      if (res.blocks === 0) showLineSpacing(lineSpacingOf(null)); // nothing selected: the box keeps showing what applies
      return res;
    }

    function setSpacing(which, rawPt) {
      const helper = host.__EASYOFFICE_DOC_TABLE_RUNS__;
      if (!helper) return { changed: false, blocks: 0 };
      const pt = Number(rawPt);
      if (!Number.isFinite(pt)) return { changed: false, blocks: 0 };
      const clamped = Math.max(0, Math.min(helper.MAX_PARA_SPACING_PT, pt));
      return runOnParagraphs((blocks) => {
        let any = false;
        blocks.forEach((b) => {
          if (helper.writeSpacingPt(b, which, clamped)) any = true;
        });
        return any;
      }, "단락 간격을 적용할 문단을 먼저 선택하세요", paragraphTargets);
    }

    // Replaces `el` by an element of another tag in the same place, keeping its look, data and children.
    function retag(el, tag) {
      const next = el.ownerDocument.createElement(tag);
      CARRIED_STYLE_PROPS.forEach((prop) => {
        if (el.style[prop]) next.style[prop] = el.style[prop];
      });
      Object.keys(el.dataset).forEach((key) => { next.dataset[key] = el.dataset[key]; });
      CARRIED_ATTRS.forEach((name) => {
        const value = el.getAttribute(name);
        if (value) next.setAttribute(name, value);
      });
      while (el.firstChild) next.appendChild(el.firstChild);
      el.parentNode.insertBefore(next, el);
      el.parentNode.removeChild(el);
      return next;
    }

    // Keeps the selection on the replacement elements after paragraphs changed their tag.
    function restoreSelection(range, replaced) {
      if (!range) return;
      const map = (node) => replaced.get(node) || node;
      setSelection({
        startContainer: map(range.startContainer), startOffset: range.startOffset,
        endContainer: map(range.endContainer), endOffset: range.endOffset, collapsed: range.collapsed,
      });
    }

    function hasElementChild(node) {
      return Array.from(node.childNodes).some((n) => n.nodeType === 1);
    }

    // Only an `<li>` inside `<ul>/<ol>` is a list item; a stray one (directly in the editor or in a
    // table cell) is a plain paragraph-like block.
    function isListItem(block) {
      return block.tagName === "LI" && LIST_TAG_NAMES.has(block.parentNode?.tagName);
    }

    function listKindOf(block) {
      if (!isListItem(block)) return null;
      if (block.parentNode.tagName === "OL") return "ordered";
      return block.parentNode.dataset?.eoList === "task" ? "task" : "bullet";
    }

    function sameListContainer(el, kind) {
      if (!el || el.nodeType !== 1 || el.tagName !== LIST_TAGS[kind]) return false;
      if (kind === "ordered") return true;
      return (el.dataset?.eoList === "task") === (kind === "task");
    }

    // A list item becomes a plain paragraph after the list; the items behind it continue in a new list.
    function itemToParagraph(li) {
      if (!isListItem(li)) {
        const plain = retag(li, "p");
        delete plain.dataset.eoListLevel;
        delete plain.dataset.eoListNum;
        delete plain.dataset.eoChecked;
        return plain;
      }
      const list = li.parentNode;
      const para = retag(li, "p");
      delete para.dataset.eoListLevel;
      delete para.dataset.eoListNum;
      delete para.dataset.eoChecked;
      const tail = [];
      for (let n = para.nextSibling; n; n = n.nextSibling) tail.push(n);
      const holder = list.parentNode;
      holder.insertBefore(para, list.nextSibling);
      if (tail.some((n) => n.nodeType === 1)) {
        const rest = list.cloneNode(false);
        tail.forEach((n) => rest.appendChild(n));
        holder.insertBefore(rest, para.nextSibling);
      }
      if (!hasElementChild(list)) holder.removeChild(list);
      return para;
    }

    // A paragraph becomes an item of the list just before it (or of a new list), joined to a list behind it.
    function paragraphToItem(block, kind) {
      const listTag = LIST_TAGS[kind];
      const item = retag(block, "li");
      if (kind === "task") {
        item.dataset.eoChecked = item.dataset.eoChecked === "true" ? "true" : "false";
      } else {
        delete item.dataset.eoChecked;
      }
      const holder = item.parentNode;
      const prev = item.previousSibling;
      let list = sameListContainer(prev, kind) ? prev : null;
      if (!list) {
        list = item.ownerDocument.createElement(listTag);
        if (kind === "task") {
          list.className = "eo-task-list";
          list.dataset.eoList = "task";
        }
        holder.insertBefore(list, item);
      }
      list.appendChild(item);
      const next = list.nextSibling;
      if (sameListContainer(next, kind)) {
        while (next.firstChild) list.appendChild(next.firstChild);
        holder.removeChild(next);
      }
      return item;
    }

    function isItemOf(block, kind) {
      return listKindOf(block) === kind;
    }

    // Numbered / bulleted / checklist paragraphs as `<ol>/<ul><li>`; applying the same kind again takes them out.
    function toggleList(kind) {
      if (!LIST_TAGS[kind]) return { changed: false, blocks: 0 };
      const pick = () => paragraphTargets().filter((b) => b.tagName === "P" || b.tagName === "LI");
      return runOnParagraphs((blocks, range) => {
        const replaced = new Map();
        const off = blocks.every((b) => isItemOf(b, kind));
        blocks.forEach((b) => {
          if (off) replaced.set(b, itemToParagraph(b));
          else if (isItemOf(b, kind)) return;
          else replaced.set(b, paragraphToItem(isListItem(b) ? itemToParagraph(b) : b, kind));
        });
        restoreSelection(range, replaced);
      }, "번호·글머리표·체크리스트를 적용할 문단을 먼저 선택하세요", pick);
    }

    function toggleTaskChecked(explicitItems) {
      const pick = Array.isArray(explicitItems)
        ? () => explicitItems.filter((b) => isItemOf(b, "task"))
        : () => paragraphTargets().filter((b) => isItemOf(b, "task"));
      return runOnParagraphs((items) => {
        const nextChecked = !items.every((li) => li.dataset.eoChecked === "true");
        items.forEach((li) => {
          li.dataset.eoChecked = nextChecked ? "true" : "false";
        });
        return items.length > 0;
      }, "체크 상태를 바꿀 체크리스트 항목을 먼저 선택하세요", pick);
    }

    function docFlavorOf(modOverride, flavorOverride) {
      const mod = modOverride ?? document.getElementById("app")?.dataset?.module;
      if (mod === "HWP") return "hwp";
      const fl = flavorOverride ?? editor()?.dataset?.eoDocFlavor;
      return fl === "pages" ? "pages" : "doc";
    }

    function syncStyleOptions(modOverride, flavorOverride) {
      const select = byId("doc-style-select");
      if (!select) return;
      const list = DOC_STYLE_OPTIONS[docFlavorOf(modOverride, flavorOverride)] || DOC_STYLE_OPTIONS.doc;
      const curList = Array.from(select.querySelectorAll("option")).map((o) => o.value);
      if (curList.length !== list.length || curList.some((v, i) => v !== list[i])) {
        select.replaceChildren();
        for (const name of list) {
          const opt = document.createElement("option");
          opt.value = name;
          opt.textContent = name;
          select.appendChild(opt);
        }
      }
      select.value = styleNameOf(null, modOverride, flavorOverride);
      host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(select);
    }

    // The style box: headings 1..6 and plain paragraph style per window/flavor.
    function setStyle(name, options = {}) {
      const explicitLevel = PAGES_EXPLICIT_STYLES[name];
      const level = explicitLevel || Number(STYLE_HEADING_RE.exec(name)?.[1] || 0);
      if (!level && !PLAIN_STYLES.has(name)) return { changed: false, blocks: 0 };
      const tag = level ? "h" + level : "p";
      const pick = () => paragraphTargets().filter((b) => b.tagName === "P" || HEADING_TAG_RE.test(b.tagName));
      const res = runOnParagraphs((blocks, range) => {
        const replaced = new Map();
        const roleChanged = !level && editor().dataset.styleRole !== name;
        if (!level) editor().dataset.styleRole = name;
        let metaChanged = false;
        blocks.forEach((b) => {
          const target = b.tagName.toLowerCase() !== tag ? retag(b, tag) : b;
          if (target !== b) replaced.set(b, target);
          if (explicitLevel) {
            if (target.dataset.eoPagesStyle !== name) { target.dataset.eoPagesStyle = name; metaChanged = true; }
          } else if (target.dataset?.eoPagesStyle) {
            delete target.dataset.eoPagesStyle;
            metaChanged = true;
          }
        });
        restoreSelection(range, replaced);
        return replaced.size > 0 || roleChanged || metaChanged;
      }, "스타일을 적용할 문단을 먼저 선택하세요", pick, options);
      if (res.blocks === 0) {
        const select = byId("doc-style-select");
        if (select) { select.value = styleNameOf(null); host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(select); }
      }
      return res;
    }

    function lineSpacingOf(block) {
      const raw = Number.parseFloat(block?.style?.lineHeight || editor()?.style?.lineHeight || "");
      return Number.isFinite(raw) && raw > 0 ? Math.round(raw * 100) : defaultLinePercent();
    }

    function styleNameOf(block, modOverride, flavorOverride) {
      const flavor = docFlavorOf(modOverride, flavorOverride);
      const level = HEADING_TAG_RE.test(block?.tagName || "") ? Number(block.tagName[1]) : 0;
      if (level >= 1 && level <= 6) {
        if (flavor === "hwp") return "개요 " + level;
        if (flavor === "pages") {
          if (level === 1 && block?.dataset?.eoPagesStyle === "제목") return "제목";
          if (level === 2 && (block?.dataset?.eoPagesStyle === "부제" || block?.dataset?.eoPagesStyle === "부제목")) return "부제";
          return level <= 1 ? "머리말" : level <= 3 ? "머리말 " + level : "머리말 3";
        }
        return "제목 " + level;
      }
      const fallback = flavor === "hwp" ? "바탕글" : flavor === "pages" ? "본문" : "표준";
      const role = editor()?.dataset?.styleRole;
      if (flavor === "hwp") return role === "바탕글" ? role : fallback;
      if (flavor === "pages") return role === "본문" ? role : fallback;
      return role === "표준" ? role : fallback;
    }

    function alignModeOf(block) {
      const mode = block?.dataset?.eoAlign;
      if (APPROXIMATED_ALIGN_LABEL[mode]) return mode;
      const def = docFlavorOf() === "hwp" ? "justify" : "left";
      const css = (block?.style?.textAlign || editor()?.style?.textAlign || def).trim().toLowerCase();
      return ALIGN_BUTTON_MODES.includes(css) ? css : def;
    }

    // Alignment buttons, list buttons, line spacing and style box show the first paragraph of the selection.
    function syncParagraphControls(blk) {
      if (!blk) return;
      const mode = alignModeOf(blk);
      ALIGN_BUTTON_MODES.forEach((m) => setPressed("btn-align-" + m, m === mode));
      const activeList = listKindOf(blk);
      Object.entries(LIST_BUTTON_CMDS).forEach(([kind, cmd]) => {
        const btn = document.querySelector('#icon-toolbar .tb-btn[data-studio-cmd="' + cmd + '"]');
        if (!btn) return;
        const on = kind === activeList;
        btn.setAttribute("aria-pressed", on ? "true" : "false");
        btn.classList.toggle("active", on);
      });
      showLineSpacing(lineSpacingOf(blk));
      const helper = host.__EASYOFFICE_DOC_TABLE_RUNS__;
      if (helper) {
        const beforeEl = byId("doc-space-before-input");
        if (beforeEl && document.activeElement !== beforeEl) beforeEl.value = String(helper.readSpacingPt(blk, "before"));
        const afterEl = byId("doc-space-after-input");
        if (afterEl && document.activeElement !== afterEl) afterEl.value = String(helper.readSpacingPt(blk, "after"));
      }
      const style = byId("doc-style-select");
      const name = styleNameOf(blk);
      if (style) style.value = name;
      const badge = byId("doc-style-role-badge");
      if (badge) badge.textContent = name;
    }

    let lastRulerKey = "";

    function currentZoomRatio(overrideZoom) {
      if (Number.isFinite(overrideZoom) && overrideZoom > 0) return overrideZoom;
      const stageZoom = Number.parseFloat(document.querySelector(".canvas-stage")?.style?.zoom || "100");
      return Number.isFinite(stageZoom) && stageZoom > 0 ? stageZoom / 100 : 1;
    }

    function syncRuler(overrideZoom, firstBlock) {
      const zoomRatio = currentZoomRatio(overrideZoom);
      const scale = resolveRulerScale(zoomRatio);
      const isHwp = docFlavorOf() === "hwp";
      const rulerKey = scale.labelStepMm + (isHwp ? ":mm" : ":cm");
      if (rulerKey !== lastRulerKey) {
        lastRulerKey = rulerKey;
        const unitEl = byId("ruler-corner");
        if (unitEl) unitEl.textContent = isHwp ? "mm" : "cm";
        const step = isHwp ? scale.labelStepMm : scale.labelStepMm / 10;
        const hTrack = document.querySelector("#h-ruler .ruler-track");
        if (hTrack) {
          const labels = [];
          for (let i = 1; i <= 40; i += 1) labels.push(String(i * step));
          hTrack.textContent = labels.join(" · · · ");
        }
        const vRuler = byId("v-ruler");
        if (vRuler) {
          const spans = vRuler.querySelectorAll("span");
          for (let i = 0; i < spans.length; i += 1) {
            spans[i].textContent = String((i + 1) * step);
          }
        }
      }
      const blk = firstBlock || targetBlocks()[0];
      Object.entries(RULER_PINS).forEach(([id, prop]) => {
        const pin = byId(id);
        if (!pin) return;
        const left = blk ? readPx(blk, "paddingLeft") : 0;
        const px = blk ? readPx(blk, prop) : 0;
        let shift = px;
        if (prop === "textIndent") shift = left + px;
        if (prop === "paddingRight") shift = -px;
        pin.style.transform = shift ? "translateX(" + shift + "px)" : "";
        pin.dataset.indentPx = String(px);
      });
    }

    function bindRulerPin(pin, prop) {
      let drag = null;
      pin.addEventListener("pointerdown", (ev) => {
        const blocks = targetBlocks();
        if (blocks.length === 0) return;
        ev.preventDefault?.();
        pin.setPointerCapture?.(ev.pointerId);
        const pre = typeof captureWorkspaceSnapshot === "function" ? captureWorkspaceSnapshot(false) : null;
        drag = { startX: ev.clientX, blocks, pre, base: blocks.map((b) => readPx(b, prop)), moved: false };
      });
      pin.addEventListener("pointermove", (ev) => {
        if (!drag) return;
        const zoomRatio = currentZoomRatio();
        const delta = Math.round((ev.clientX - drag.startX) / (INDENT_STEP_PX * zoomRatio)) * INDENT_STEP_PX;
        if (delta !== 0) drag.moved = true;
        const sign = prop === "paddingRight" ? -1 : 1;
        drag.blocks.forEach((b, i) => writePx(b, prop, drag.base[i] + sign * delta));
        syncRuler();
      });
      const finish = () => {
        if (!drag) return;
        const done = drag;
        drag = null;
        if (!done.moved) return;
        onMutated();
        if (done.pre && typeof commitDeferredUndoSnapshot === "function") commitDeferredUndoSnapshot(done.pre);
      };
      pin.addEventListener("pointerup", finish);
      pin.addEventListener("pointercancel", finish);
    }

    // ------------------------------------------------------------ pending format typing

    // A composition (`doc-ime.js`) writes the text itself and is one undo step at its end; the pending format waits for the next typed character.
    function handleBeforeInput(ev) {
      if (!pending || ev.isComposing || isComposing?.() || ev.inputType !== "insertText" || !ev.data) return;
      const root = editor();
      const range = currentRange();
      if (!root || !range?.collapsed || range.startContainer !== pending.node || range.startOffset !== pending.offset) {
        dropPending();
        return;
      }
      ev.preventDefault?.();
      const patch = pending.patch;
      const exec = () => {
        const res = R.insertFormattedText(root, range.startContainer, range.startOffset, ev.data, patch);
        setSelection({
          startContainer: res.caret.node, startOffset: res.caret.offset,
          endContainer: res.caret.node, endOffset: res.caret.offset, collapsed: true,
        });
        return { changed: true };
      };
      withUndo(exec);
      dropPending();
      syncToolbar();
    }

    function handleSelectionChange() {
      const root = editor();
      const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
      if (!root || !sel || sel.rangeCount === 0) return;
      const live = plainRange(sel.getRangeAt(0));
      if (!insideEditor(live)) return;
      lastRange = live;
      if (pending && !(live.collapsed && live.startContainer === pending.node && live.startOffset === pending.offset)) {
        dropPending();
      }
      queueSync();
    }

    function shiftListLevel(delta) {
      const step = Math.trunc(Number(delta)) || 0;
      if (step === 0) return { changed: false, blocks: 0 };
      const pick = () => paragraphTargets().filter((b) => isListItem(b));
      return runOnParagraphs((blocks) => {
        let changed = false;
        blocks.forEach((li) => {
          const cur = Math.max(0, Math.min(8, Number.parseInt(li.dataset.eoListLevel || "0", 10) || 0));
          const next = Math.max(0, Math.min(8, cur + step));
          if (next === cur) return;
          changed = true;
          if (next > 0) li.dataset.eoListLevel = String(next);
          else delete li.dataset.eoListLevel;
        });
        return changed;
      }, "단계를 변경할 목록 항목을 먼저 선택하세요", pick);
    }

    function handleKeydown(ev) {
      if (!isDocModule()) return;
      if (getActiveModule() === "DOC" && (ev.ctrlKey || ev.metaKey) && !ev.altKey && (ev.code === "Equal" || ev.key === "=" || ev.key === "+")) {
        const root = editor();
        const target = ev.target || document.activeElement;
        if (root && target && root.contains(target)) {
          ev.preventDefault?.();
          ev.stopPropagation?.();
          toggle(ev.shiftKey ? "superscript" : "subscript");
          return;
        }
      }
      if (ev.key === "Enter" && (ev.ctrlKey || ev.metaKey) && ev.altKey && !ev.shiftKey) {
        const taskItems = paragraphTargets().filter((b) => isItemOf(b, "task"));
        if (taskItems.length === 0) return;
        ev.preventDefault?.();
        toggleTaskChecked(taskItems);
        return;
      }
      if (ev.key !== "Tab" || ev.altKey || ev.ctrlKey || ev.metaKey) return;
      const items = paragraphTargets().filter((b) => isListItem(b));
      if (items.length > 0) {
        ev.preventDefault?.();
        shiftListLevel(ev.shiftKey ? -1 : 1);
        return;
      }
      if (ev.shiftKey || getActiveModule() !== "DOC") return;
      const root = editor();
      const range = currentRange();
      if (!root || !range || root.dataset?.eoHfEdit || root.dataset?.eoGhostActive === "true") return;
      const startEl = range.startContainer?.nodeType === 1 ? range.startContainer : range.startContainer?.parentElement;
      if (startEl?.closest?.("table")) return;
      ev.preventDefault?.();
      const exec = () => {
        if (!range.collapsed) {
          const liveSel = typeof host.getSelection === "function" ? host.getSelection() : null;
          if (liveSel?.rangeCount > 0) liveSel.getRangeAt(0).deleteContents?.();
        }
        const cur = currentRange() || range;
        if (pending && cur.startContainer === pending.node && cur.startOffset === pending.offset) {
          const res = R.insertFormattedText(root, cur.startContainer, cur.startOffset, "\t", pending.patch);
          setSelection({ startContainer: res.caret.node, startOffset: res.caret.offset, endContainer: res.caret.node, endOffset: res.caret.offset, collapsed: true });
          dropPending();
          return { changed: true };
        }
        dropPending();
        const tabNode = root.ownerDocument.createTextNode("\t");
        if (cur.startContainer.nodeType === 3) {
          const text = cur.startContainer;
          const val = String(text.data ?? text.nodeValue ?? "");
          const off = Math.max(0, Math.min(val.length, cur.startOffset));
          text.data = val.slice(0, off) + "\t" + val.slice(off);
          setSelection({ startContainer: text, startOffset: off + 1, endContainer: text, endOffset: off + 1, collapsed: true });
        } else {
          cur.startContainer.insertBefore(tabNode, cur.startContainer.childNodes[cur.startOffset] ?? null);
          setSelection({ startContainer: tabNode, startOffset: 1, endContainer: tabNode, endOffset: 1, collapsed: true });
        }
        return { changed: true };
      };
      withUndo(exec);
      syncToolbar();
    }

    function handleEditorClick(ev) {
      if (!isDocModule()) return;
      const target = ev.target;
      if (!target || target.nodeType !== 1 || target.tagName !== "LI" || !isItemOf(target, "task")) return;
      const x = typeof ev.offsetX === "number"
        ? ev.offsetX
        : typeof target.getBoundingClientRect === "function" && typeof ev.clientX === "number"
          ? ev.clientX - (target.getBoundingClientRect().left || 0)
          : NaN;
      if (!Number.isFinite(x) || x < -2 || x > 20) return;
      ev.preventDefault?.();
      toggleTaskChecked([target]);
    }

    function handleInput(ev) {
      if (ev?.inputType !== "insertParagraph" || !isDocModule()) return;
      const blk = paragraphTargets()[0];
      if (blk && isItemOf(blk, "task") && blk.dataset.eoChecked === "true" && !(blk.textContent || "").trim()) {
        blk.dataset.eoChecked = "false";
      }
    }

    function bind() {
      const root = editor();
      if (!root) return;
      document.addEventListener?.("selectionchange", handleSelectionChange);
      root.addEventListener("beforeinput", handleBeforeInput);
      root.addEventListener("keydown", handleKeydown);
      root.addEventListener("click", handleEditorClick);
      root.addEventListener("input", handleInput);
      Object.entries(RULER_PINS).forEach(([id, prop]) => {
        const pin = document.getElementById(id);
        if (pin) bindRulerPin(pin, prop);
      });
    }

    return {
      bind, applyPatch, toggle, setSize, bumpSize, setFont, setColor, toggleColor, getState, syncToolbar,
      shiftIndent, setBlockIndent, setAlign, setLineSpacing, showLineSpacing, setSpacing, toggleList, toggleTaskChecked, shiftListLevel, setStyle, styleNameOf, syncStyleOptions, syncRuler, resolveRulerScale, handleBeforeInput, handleSelectionChange,
      currentRange, withUndo,
      getPending: () => pending,
      getLastRange: () => lastRange,
      setLastRange: (r) => { lastRange = r ? plainRange(r) : null; },
    };
  }

  const PX_PER_MM = 96 / 25.4, MIN_LABEL_SPACING_PX = 30, MIN_TICK_SPACING_PX = 3.5, MIN_LABEL_STEP_MM = 10;

  function niceStepCeil(value) {
    const safeValue = Number.isFinite(value) && value > 0 ? value : 1;
    const power = 10 ** Math.floor(Math.log10(safeValue));
    const normalized = safeValue / power;
    if (normalized <= 1) return power;
    return normalized <= 2 ? 2 * power : normalized <= 5 ? 5 * power : 10 * power;
  }

  function resolveRulerScale(zoom) {
    const pixelsPerMm = PX_PER_MM * (Number.isFinite(zoom) && zoom > 0 ? zoom : 1);
    return { labelStepMm: niceStepCeil(Math.max(MIN_LABEL_STEP_MM, MIN_LABEL_SPACING_PX / pixelsPerMm)), tickStepMm: niceStepCeil(MIN_TICK_SPACING_PX / pixelsPerMm) };
  }

  host.__EASYOFFICE_DOC_RUNS_UI__ = { niceStepCeil, resolveRulerScale };
  host.__EASYOFFICE_CREATE_DOC_RUNS_UI__ = createDocRunsUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);

