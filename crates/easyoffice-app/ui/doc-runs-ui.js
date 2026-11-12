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
  const DEFAULT_LINE_PERCENT = 160;
  const NON_PARAGRAPH_TAGS = new Set(["TD", "TH", "CAPTION"]);
  const HEADING_TAG_RE = /^H[1-6]$/;
  const STYLE_HEADING_RE = /^개요 ([1-3])$/;
  const PLAIN_STYLES = new Set(["바탕글", "공문서 바탕글"]);
  const LIST_TAGS = { ordered: "OL", bullet: "UL" };
  const LIST_TAG_NAMES = new Set(Object.values(LIST_TAGS));
  // Look carried over when a paragraph changes its tag (the style whitelist of the saved document state).
  const CARRIED_STYLE_PROPS = [
    "paddingLeft", "paddingRight", "textIndent", "lineHeight", "textAlign",
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

  function writePx(blk, prop, px) {
    const [lo, hi] = INDENT_LIMITS[prop];
    const next = Math.max(lo, Math.min(hi, Math.round(px)));
    blk.style[prop] = next ? next + "px" : "";
    return next;
  }

  function createDocRunsUi(deps) {
    const { document, getActiveModule, captureWorkspaceSnapshot, commitDeferredUndoSnapshot, saveDocState } = deps;
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
      const clamped = Math.max(MIN_PT, Math.min(MAX_PT, Number.isFinite(pt) ? pt : 11));
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
      const fallback = editor()?.style?.fontFamily?.replace(/^["']|["']$/g, "") || "휴먼명조";
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

    // Runs `exec(blocks, range)` on the paragraphs the selection touches as one undo step; `hint` is
    // shown when nothing is selected. `exec` returns false when it found nothing to change, which
    // leaves no undo step.
    function runOnParagraphs(exec, hint, pick = paragraphTargets) {
      if (!isDocModule()) return { changed: false, blocks: 0 };
      const blocks = pick();
      if (blocks.length === 0) {
        setFeedback(hint);
        return { changed: false, blocks: 0 };
      }
      const range = currentRange();
      const res = withUndo(() => ({ changed: exec(blocks, range) !== false }));
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
      }, "정렬할 문단을 먼저 선택하세요");
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
      const clamped = Math.max(MIN_LINE_PERCENT, Math.min(MAX_LINE_PERCENT, Number.isFinite(percent) ? Math.round(percent) : DEFAULT_LINE_PERCENT));
      const value = (clamped / 100).toFixed(2);
      const res = runOnParagraphs((blocks) => {
        const stale = blocks.filter((b) => b.style.lineHeight !== value);
        stale.forEach((b) => {
          b.style.lineHeight = value;
          delete b.dataset.eoLineSpacing; // the exact value read from a file no longer matches
        });
        return stale.length > 0;
      }, "줄 간격을 적용할 문단을 먼저 선택하세요");
      if (res.blocks === 0) showLineSpacing(lineSpacingOf(null)); // nothing selected: the box keeps showing what applies
      return res;
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

    // A list item becomes a plain paragraph after the list; the items behind it continue in a new list.
    function itemToParagraph(li) {
      if (!isListItem(li)) return retag(li, "p");
      const list = li.parentNode;
      const para = retag(li, "p");
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
    function paragraphToItem(block, listTag) {
      const item = retag(block, "li");
      const holder = item.parentNode;
      const prev = item.previousSibling;
      let list = prev && prev.nodeType === 1 && prev.tagName === listTag ? prev : null;
      if (!list) {
        list = item.ownerDocument.createElement(listTag);
        holder.insertBefore(list, item);
      }
      list.appendChild(item);
      const next = list.nextSibling;
      if (next && next.nodeType === 1 && next.tagName === listTag) {
        while (next.firstChild) list.appendChild(next.firstChild);
        holder.removeChild(next);
      }
      return item;
    }

    function isItemOf(block, listTag) {
      return block.tagName === "LI" && block.parentNode?.tagName === listTag;
    }

    // Numbered / bulleted paragraphs as `<ol>/<ul><li>`; applying the same kind to list items again takes them out.
    function toggleList(kind) {
      const listTag = LIST_TAGS[kind];
      if (!listTag) return { changed: false, blocks: 0 };
      const pick = () => paragraphTargets().filter((b) => b.tagName === "P" || b.tagName === "LI");
      return runOnParagraphs((blocks, range) => {
        const replaced = new Map();
        const off = blocks.every((b) => isItemOf(b, listTag));
        blocks.forEach((b) => {
          if (off) replaced.set(b, itemToParagraph(b));
          else if (isItemOf(b, listTag)) return;
          else replaced.set(b, paragraphToItem(isListItem(b) ? itemToParagraph(b) : b, listTag));
        });
        restoreSelection(range, replaced);
      }, "번호·글머리표를 적용할 문단을 먼저 선택하세요", pick);
    }

    // The style box: outline levels are headings, the plain styles are paragraphs.
    function setStyle(name) {
      const level = STYLE_HEADING_RE.exec(name)?.[1];
      if (!level && !PLAIN_STYLES.has(name)) return { changed: false, blocks: 0 };
      const tag = level ? "h" + level : "p";
      const pick = () => paragraphTargets().filter((b) => b.tagName === "P" || HEADING_TAG_RE.test(b.tagName));
      const res = runOnParagraphs((blocks, range) => {
        const replaced = new Map();
        // The plain styles are also the document's default paragraph style (the box shows it for
        // every plain paragraph, and the Hangul module starts on 공문서 바탕글), so choosing one
        // sets that document-level name on purpose.
        const roleChanged = !level && editor().dataset.styleRole !== name;
        if (!level) editor().dataset.styleRole = name;
        blocks.forEach((b) => { if (b.tagName.toLowerCase() !== tag) replaced.set(b, retag(b, tag)); });
        restoreSelection(range, replaced);
        return replaced.size > 0 || roleChanged;
      }, "스타일을 적용할 문단을 먼저 선택하세요", pick);
      if (res.blocks === 0) {
        const select = byId("doc-style-select");
        if (select) select.value = styleNameOf(null);
      }
      return res;
    }

    function lineSpacingOf(block) {
      const raw = Number.parseFloat(block?.style?.lineHeight || editor()?.style?.lineHeight || "");
      return Number.isFinite(raw) && raw > 0 ? Math.round(raw * 100) : DEFAULT_LINE_PERCENT;
    }

    function styleNameOf(block) {
      const level = HEADING_TAG_RE.test(block?.tagName || "") ? Number(block.tagName[1]) : 0;
      if (level >= 1 && level <= 3) return "개요 " + level;
      const role = editor()?.dataset?.styleRole;
      return PLAIN_STYLES.has(role) ? role : "바탕글";
    }

    function alignModeOf(block) {
      const mode = block?.dataset?.eoAlign;
      if (APPROXIMATED_ALIGN_LABEL[mode]) return mode;
      const css = (block?.style?.textAlign || editor()?.style?.textAlign || "justify").trim().toLowerCase();
      return ALIGN_BUTTON_MODES.includes(css) ? css : "justify";
    }

    // Alignment buttons, line spacing and style box show the first paragraph of the selection.
    function syncParagraphControls(blk) {
      if (!blk) return;
      const mode = alignModeOf(blk);
      ALIGN_BUTTON_MODES.forEach((m) => setPressed("btn-align-" + m, m === mode));
      showLineSpacing(lineSpacingOf(blk));
      const style = byId("doc-style-select");
      const name = styleNameOf(blk);
      if (style) style.value = name;
      const badge = byId("doc-style-role-badge");
      if (badge) badge.textContent = name;
    }

    let lastLabelStepMm = 10;

    function currentZoomRatio(overrideZoom) {
      if (Number.isFinite(overrideZoom) && overrideZoom > 0) return overrideZoom;
      const stageZoom = Number.parseFloat(document.querySelector(".canvas-stage")?.style?.zoom || "100");
      return Number.isFinite(stageZoom) && stageZoom > 0 ? stageZoom / 100 : 1;
    }

    function syncRuler(overrideZoom, firstBlock) {
      const zoomRatio = currentZoomRatio(overrideZoom);
      const scale = resolveRulerScale(zoomRatio);
      if (scale.labelStepMm !== lastLabelStepMm) {
        lastLabelStepMm = scale.labelStepMm;
        const stepCm = scale.labelStepMm / 10;
        const hTrack = document.querySelector("#h-ruler .ruler-track");
        if (hTrack) {
          const labels = [];
          for (let i = 1; i <= 40; i += 1) labels.push(String(i * stepCm));
          hTrack.textContent = labels.join(" · · · ");
        }
        const vRuler = byId("v-ruler");
        if (vRuler) {
          const spans = vRuler.querySelectorAll("span");
          for (let i = 0; i < spans.length; i += 1) {
            spans[i].textContent = String((i + 1) * stepCm);
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

    function handleBeforeInput(ev) {
      if (!pending || ev.isComposing || ev.inputType !== "insertText" || !ev.data) return;
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

    function bind() {
      const root = editor();
      if (!root) return;
      document.addEventListener?.("selectionchange", handleSelectionChange);
      root.addEventListener("beforeinput", handleBeforeInput);
      Object.entries(RULER_PINS).forEach(([id, prop]) => {
        const pin = document.getElementById(id);
        if (pin) bindRulerPin(pin, prop);
      });
    }

    return {
      bind, applyPatch, toggle, setSize, bumpSize, setFont, setColor, toggleColor, getState, syncToolbar,
      shiftIndent, setBlockIndent, setAlign, setLineSpacing, showLineSpacing, toggleList, setStyle, syncRuler, resolveRulerScale, handleBeforeInput, handleSelectionChange,
      currentRange, withUndo,
      getPending: () => pending,
      getLastRange: () => lastRange,
      setLastRange: (r) => { lastRange = r ? plainRange(r) : null; },
    };
  }

  const PX_PER_MM = 96 / 25.4;
  const MIN_LABEL_SPACING_PX = 30;
  const MIN_TICK_SPACING_PX = 3.5;
  const MIN_LABEL_STEP_MM = 10;

  function niceStepCeil(value) {
    const safeValue = Number.isFinite(value) && value > 0 ? value : 1;
    const exponent = Math.floor(Math.log10(safeValue));
    const power = 10 ** exponent;
    const normalized = safeValue / power;
    if (normalized <= 1) return power;
    if (normalized <= 2) return 2 * power;
    if (normalized <= 5) return 5 * power;
    return 10 * power;
  }

  function resolveRulerScale(zoom) {
    const safeZoom = Number.isFinite(zoom) && zoom > 0 ? zoom : 1;
    const pixelsPerMm = PX_PER_MM * safeZoom;
    return {
      labelStepMm: niceStepCeil(Math.max(MIN_LABEL_STEP_MM, MIN_LABEL_SPACING_PX / pixelsPerMm)),
      tickStepMm: niceStepCeil(MIN_TICK_SPACING_PX / pixelsPerMm),
    };
  }

  host.__EASYOFFICE_DOC_RUNS_UI__ = { niceStepCeil, resolveRulerScale };
  host.__EASYOFFICE_CREATE_DOC_RUNS_UI__ = createDocRunsUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);

