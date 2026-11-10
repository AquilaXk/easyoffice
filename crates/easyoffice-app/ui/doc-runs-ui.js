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
      syncRuler();
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

    let lastLabelStepMm = 10;

    function currentZoomRatio(overrideZoom) {
      if (Number.isFinite(overrideZoom) && overrideZoom > 0) return overrideZoom;
      const stageZoom = Number.parseFloat(document.querySelector(".canvas-stage")?.style?.zoom || "100");
      return Number.isFinite(stageZoom) && stageZoom > 0 ? stageZoom / 100 : 1;
    }

    function syncRuler(overrideZoom) {
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
      const blk = targetBlocks()[0];
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
      shiftIndent, setBlockIndent, syncRuler, resolveRulerScale, handleBeforeInput, handleSelectionChange,
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

