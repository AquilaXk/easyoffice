(function (host) {
  "use strict";

  // Contextual selection style bar (fill, stroke, stroke width, label/text), viewport centering,
  // and object clipboard (Copy/Paste/Duplicate with offset and 1-step undo) for BOARD.

  const MAX_NODES = 512;
  const MAX_CONNECTORS = 1024;
  const MIN_COORD = -100000;
  const MAX_COORD = 100000;
  const PASTE_OFFSET_PX = 16;
  const WIDTH_OPTIONS = ["0", "1", "1.5", "2", "3", "4", "6", "8"];

  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const round2 = (v) => Math.round((Number(v) || 0) * 100) / 100;
  const cloneJson = (obj) => JSON.parse(JSON.stringify(obj));

  function normalizeHex6(raw, fallback) {
    const clean = String(raw ?? "").trim().replace(/^#/, "").toUpperCase();
    return /^[0-9A-F]{6}$/.test(clean) ? clean : fallback;
  }

  function isEditableTarget(target, doc) {
    const raw = target || doc?.activeElement;
    const t = raw?.nodeType === 3 ? raw.parentElement : raw;
    if (!t || t === doc?.body || t === doc?.documentElement) return false;
    const tag = String(t.tagName || "").toUpperCase();
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || t.isContentEditable) return true;
    const ceEl = t.closest?.("[contenteditable]");
    if (!ceEl) return false;
    const ce = ceEl.getAttribute?.("contenteditable");
    return ce === "" || ce === "true" || ce === "plaintext-only";
  }

  function nearestWidthOption(val) {
    const num = Number(val);
    if (!Number.isFinite(num)) return "1.5";
    let best = WIDTH_OPTIONS[0];
    let bestDist = Infinity;
    for (const opt of WIDTH_OPTIONS) {
      const dist = Math.abs(Number(opt) - num);
      if (dist < bestDist) { bestDist = dist; best = opt; }
    }
    return best;
  }

  let activeHooks = null;
  let activeDoc = null;
  let clipData = null;

  function mutateSelectedItem(hooks, fn, opts = {}) {
    if (!hooks) return false;
    const state = hooks.getMutableState?.();
    const sel = hooks.getSelection?.() || {};
    const node = sel.node || null;
    const conn = sel.connector || null;
    if (!state || (!node && !conn)) return false;
    const snap = opts.recordUndo !== false ? {
      nodes: cloneJson(state.nodes),
      connectors: cloneJson(state.connectors),
      viewport: cloneJson(state.viewport),
    } : null;
    if (!fn(node, conn)) return false;
    if (snap) {
      const curNodes = state.nodes;
      const curConns = state.connectors;
      const curVp = state.viewport;
      state.nodes = snap.nodes;
      state.connectors = snap.connectors;
      state.viewport = snap.viewport;
      hooks.recordUndo?.();
      state.nodes = curNodes;
      state.connectors = curConns;
      state.viewport = curVp;
    }
    hooks.renderAll?.();
    hooks.noteActivity?.();
    return true;
  }

  function createBoardStyleBar(hooks, deps = {}) {
    if (hooks) activeHooks = hooks;
    const doc = deps.document || host.document;
    if (doc) activeDoc = doc;
    const bar = deps.toolbarEl;
    let groupEl = null;
    let fillWrap = null;
    let fillInput = null;
    let strokeWrap = null;
    let strokeInput = null;
    let widthWrap = null;
    let widthSelect = null;
    let zeroWidthOpt = null;
    let labelWrap = null;
    let labelInput = null;
    let dragUndoActive = false;

    const appendAll = (parent, ...kids) => {
      for (const k of kids) if (k) parent.appendChild(k);
      return parent;
    };
    const el = (tag, cls, attrs = {}, text) => {
      const node = doc.createElement(tag);
      if (cls) node.className = cls;
      for (const [k, v] of Object.entries(attrs)) {
        if (v !== undefined && v !== null) node.setAttribute(k, String(v));
      }
      if (text !== undefined) node.textContent = String(text);
      return node;
    };

    const syncSwatchChip = (input) => {
      const chip = input?.parentElement?.querySelector?.(".dialog-swatch-chip");
      chip?.style?.setProperty?.("--dialog-swatch", input?.value || "");
    };

    function bindColorControl(input, pickerMounted, onApply) {
      input.addEventListener("input", () => {
        syncSwatchChip(input);
        const changed = mutateSelectedItem(hooks, (n, c) => onApply(n, c, input.value), { recordUndo: !dragUndoActive });
        if (changed) dragUndoActive = true;
      });
      if (!pickerMounted) {
        input.addEventListener("change", () => {
          syncSwatchChip(input);
          const hadDragUndo = dragUndoActive;
          dragUndoActive = false;
          mutateSelectedItem(hooks, (n, c) => onApply(n, c, input.value), { recordUndo: !hadDragUndo });
        });
      }
      input.addEventListener("blur", () => { dragUndoActive = false; });
    }

    function createColorField(wrapId, hostId, inputId, paletteId, captionText, ariaLabel, onApply) {
      const wrap = el("div", "board-style-field", { id: wrapId });
      const hostEl = el("div", "sb-color-host dialog-color-host", { id: hostId });
      const chip = el("span", "dialog-swatch-chip", { "aria-hidden": "true" });
      const caret = el("span", "", { "aria-hidden": "true" }, "▾");
      const toggleBtn = appendAll(
        el("button", "dialog-swatch", { type: "button", "data-color-toggle": "true", "aria-haspopup": "true", "aria-expanded": "false", title: ariaLabel, "aria-label": ariaLabel }),
        chip, caret
      );
      const input = el("input", "", { id: inputId, type: "color", "aria-label": ariaLabel });
      const palette = el("div", "sb-hl-palette", { id: paletteId });
      palette.hidden = true;
      appendAll(hostEl, toggleBtn, input, palette);
      appendAll(wrap, el("span", "board-style-caption", {}, captionText), hostEl);
      const commitColor = (raw) => {
        if (raw) input.value = raw;
        syncSwatchChip(input);
        const hadDragUndo = dragUndoActive;
        dragUndoActive = false;
        mutateSelectedItem(hooks, (n, c) => onApply(n, c, input.value), { recordUndo: !hadDragUndo });
      };
      const picker = host.__EASYOFFICE_COLOR_PICKER__?.setupColorPicker?.(hostEl, commitColor);
      toggleBtn.addEventListener("click", () => { dragUndoActive = false; });
      bindColorControl(input, Boolean(picker), onApply);
      return { wrap, input };
    }

    if (bar && doc) {
      groupEl = el("div", "eo-doc-style-bar board-style-bar board-style-group", { id: "board-style-bar", role: "toolbar", "aria-label": "선택 요소 서식" });
      groupEl.hidden = true;

      ({ wrap: fillWrap, input: fillInput } = createColorField(
        "board-style-fill-wrap", "board-style-fill-host", "board-style-fill", "board-style-fill-palette", "채우기", "채우기 색",
        (node, _conn, raw) => {
          if (!node || (node.kind !== "shape" && node.kind !== "text")) return false;
          const next = normalizeHex6(raw, node.fill || "EFF6FF");
          if (node.fill === next) return false;
          node.fill = next;
          return true;
        }
      ));

      ({ wrap: strokeWrap, input: strokeInput } = createColorField(
        "board-style-stroke-wrap", "board-style-stroke-host", "board-style-stroke", "board-style-stroke-palette", "선 색", "선 색",
        (node, conn, raw) => {
          const next = normalizeHex6(raw, node ? "2563EB" : "335095");
          if (conn) {
            if (conn.stroke === next) return false;
            conn.stroke = next;
            return true;
          }
          if (node?.kind === "shape") {
            if (node.stroke === next) return false;
            node.stroke = next;
            if ((Number(node.strokeWidthPt) || 0) === 0) node.strokeWidthPt = 1.5;
            return true;
          }
          return false;
        }
      ));

      widthSelect = el("select", "sb-select sb-combo board-style-select", { id: "board-style-width", "aria-label": "선 두께" });
      for (const optVal of WIDTH_OPTIONS) {
        const opt = el("option", "", { value: optVal }, optVal === "0" ? "없음" : `${optVal}pt`);
        if (optVal === "0") zeroWidthOpt = opt;
        widthSelect.appendChild(opt);
      }
      widthWrap = appendAll(el("label", "board-style-field", { id: "board-style-width-wrap", for: "board-style-width" }),
        el("span", "board-style-caption", {}, "선 두께"), widthSelect
      );

      labelInput = el("input", "board-style-input", { id: "board-style-label", type: "text", "aria-label": "이름표", placeholder: "텍스트·이름표", maxlength: "2000" });
      labelWrap = appendAll(el("label", "board-style-field", { id: "board-style-label-wrap", for: "board-style-label" }),
        el("span", "board-style-caption", {}, "이름표"), labelInput
      );

      appendAll(groupEl, fillWrap, strokeWrap, widthWrap, labelWrap);

      if (typeof bar.after === "function") bar.after(groupEl);
      else bar.parentNode?.appendChild?.(groupEl);
      host.__EASYOFFICE_CUSTOM_SELECT__?.enhanceCustomSelects?.(doc, groupEl);

      widthSelect.addEventListener("change", () => {
        const val = Number(widthSelect.value);
        if (!Number.isFinite(val)) return;
        mutateSelectedItem(hooks, (node, conn) => {
          if (conn) {
            const next = clamp(val, 0.5, 24);
            if (conn.strokeWidthPt === next) return false;
            conn.strokeWidthPt = next;
            return true;
          }
          if (node?.kind === "shape") {
            const next = clamp(val, 0, 24);
            if (node.strokeWidthPt === next) return false;
            node.strokeWidthPt = next;
            return true;
          }
          return false;
        }, { recordUndo: true });
      });

      const commitLabel = () => {
        const text = String(labelInput.value ?? "");
        mutateSelectedItem(hooks, (node, conn) => {
          if (conn) {
            const next = text.slice(0, 128);
            if ((conn.label || "") === next) return false;
            conn.label = next;
            return true;
          }
          if (!node) return false;
          if (node.kind === "chart") {
            const next = text.slice(0, 160);
            if ((node.title || "") === next) return false;
            node.title = next;
            return true;
          }
          if (node.kind === "image") {
            const next = text.slice(0, 512);
            if ((node.alt || "") === next) return false;
            node.alt = next;
            return true;
          }
          const next = text.slice(0, 16384);
          if ((node.text || "") === next) return false;
          node.text = next;
          return true;
        }, { recordUndo: true });
      };
      labelInput.addEventListener("change", commitLabel);
      labelInput.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault?.();
          commitLabel();
          labelInput.blur?.();
        }
      });
    }

    function sync() {
      if (!groupEl) return;
      dragUndoActive = false;
      const closeIn = host.__EASYOFFICE_COLOR_PICKER__?.closePopupIn;
      const sel = hooks.getSelection?.() || {};
      const node = sel.node || null;
      const conn = sel.connector || null;
      if (!node && !conn) {
        closeIn?.(groupEl);
        groupEl.hidden = true;
        return;
      }
      groupEl.hidden = false;
      if (conn) {
        if (fillWrap) { closeIn?.(fillWrap); fillWrap.hidden = true; }
        if (strokeWrap) strokeWrap.hidden = false;
        if (widthWrap) widthWrap.hidden = false;
        if (zeroWidthOpt) zeroWidthOpt.disabled = true;
        if (strokeInput) { strokeInput.value = "#" + normalizeHex6(conn.stroke, "335095"); syncSwatchChip(strokeInput); }
        if (widthSelect) { widthSelect.value = nearestWidthOption(conn.strokeWidthPt ?? 1.5); host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(widthSelect); }
        if (labelInput && doc.activeElement !== labelInput) labelInput.value = conn.label || "";
        return;
      }
      const isShape = node.kind === "shape";
      const hasFill = isShape || node.kind === "text";
      if (fillWrap) { if (!hasFill) closeIn?.(fillWrap); fillWrap.hidden = !hasFill; }
      if (strokeWrap) { if (!isShape) closeIn?.(strokeWrap); strokeWrap.hidden = !isShape; }
      if (widthWrap) widthWrap.hidden = !isShape;
      if (zeroWidthOpt) zeroWidthOpt.disabled = false;
      if (hasFill && fillInput) { fillInput.value = "#" + normalizeHex6(node.fill, isShape ? "EFF6FF" : "FFFFFF"); syncSwatchChip(fillInput); }
      if (isShape && strokeInput) { strokeInput.value = "#" + normalizeHex6(node.stroke, "2563EB"); syncSwatchChip(strokeInput); }
      if (isShape && widthSelect) { widthSelect.value = nearestWidthOption(node.strokeWidthPt ?? 1.5); host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(widthSelect); }
      if (labelInput && doc.activeElement !== labelInput) {
        labelInput.value = node.kind === "chart" ? (node.title || "") : node.kind === "image" ? (node.alt || "") : (node.text || "");
      }
    }

    return { sync };
  }

  function centerOnBoardItem(hooks, kind, id) {
    const h = hooks || activeHooks;
    const state = h?.getMutableState?.();
    if (!state) return false;
    let cx = 0;
    let cy = 0;
    if (kind === "node") {
      const node = state.nodes.find((n) => n.id === id);
      if (!node) return false;
      cx = node.x + node.w * 0.5;
      cy = node.y + node.h * 0.5;
    } else if (kind === "conn") {
      const conn = state.connectors.find((c) => c.id === id);
      const from = conn && state.nodes.find((n) => n.id === conn.fromId);
      const to = conn && state.nodes.find((n) => n.id === conn.toId);
      if (!from || !to) return false;
      cx = (from.x + from.w * 0.5 + to.x + to.w * 0.5) * 0.5;
      cy = (from.y + from.h * 0.5 + to.y + to.h * 0.5) * 0.5;
    } else {
      return false;
    }
    const vpEl = h.getViewportEl?.();
    const rect = vpEl?.getBoundingClientRect?.();
    const vw = rect?.width > 0 ? rect.width : 960;
    const vh = rect?.height > 0 ? rect.height : 540;
    const scale = Math.max(0.25, (Number(state.viewport?.zoom) || 100) / 100);
    state.viewport.panX = round2(clamp(vw * 0.5 - cx * scale, MIN_COORD, MAX_COORD));
    state.viewport.panY = round2(clamp(vh * 0.5 - cy * scale, MIN_COORD, MAX_COORD));
    h.applyViewportTransform?.({ syncHost: false });
    return true;
  }

  function resolveHooks() {
    return activeHooks || host.__EASYOFFICE_BOARD_STUDIO__?._styleHooks || null;
  }

  function buildClipboardSnapshot(hooks) {
    const state = hooks?.getMutableState?.();
    const sel = hooks?.getSelection?.() || {};
    if (!state) return null;
    const selIds = Array.isArray(sel.nodeIds) && sel.nodeIds.length > 0 ? new Set(sel.nodeIds) : (sel.node ? new Set([sel.node.id]) : null);
    if (selIds && selIds.size > 0) {
      const nodes = state.nodes.filter((n) => selIds.has(n.id)).map(cloneJson);
      if (nodes.length === 0) return null;
      const connectors = state.connectors.filter((c) => selIds.has(c.fromId) && selIds.has(c.toId)).map(cloneJson);
      return { kind: "nodes", nodes, connectors, pasteCount: 0 };
    }
    if (sel.connector) {
      return { kind: "connector", connector: cloneJson(sel.connector), pasteCount: 0 };
    }
    return null;
  }

  function applySnapshotPaste(hooks, source, offsetStep) {
    const state = hooks?.getMutableState?.();
    const codec = host.__EASYOFFICE_BOARD_CODEC__;
    if (!state || !source) return false;
    if (source.kind === "connector") {
      const c = source.connector;
      if (!state.nodes.some((n) => n.id === c.fromId) || !state.nodes.some((n) => n.id === c.toId)) {
        hooks.notify?.("연결선 대상 요소를 찾을 수 없어 붙여넣을 수 없습니다.");
        return false;
      }
      if (state.connectors.length + 1 > MAX_CONNECTORS) {
        hooks.notify?.("연결선 수 한도(1,024개)를 초과했습니다.");
        return false;
      }
      hooks.recordUndo?.();
      const cid = hooks.nextId("bc", state.connectors);
      state.connectors.push({ ...cloneJson(c), id: cid });
      hooks.setSelection?.([], cid);
      hooks.noteActivity?.();
      return true;
    }
    const nodes = source.nodes || [];
    const conns = source.connectors || [];
    if (nodes.length === 0) return false;
    if (state.nodes.length + nodes.length > MAX_NODES) {
      hooks.notify?.("보드 요소 수 한도(512개)를 초과했습니다.");
      return false;
    }
    if (state.connectors.length + conns.length > MAX_CONNECTORS) {
      hooks.notify?.("연결선 수 한도(1,024개)를 초과했습니다.");
      return false;
    }
    const delta = PASTE_OFFSET_PX * Math.max(1, offsetStep);
    hooks.recordUndo?.();
    const idMap = new Map();
    const newIds = [];
    for (const src of nodes) {
      const nid = hooks.nextId("bn", state.nodes);
      idMap.set(src.id, nid);
      const spec = { ...cloneJson(src), id: nid, x: round2(clamp(src.x + delta, MIN_COORD, MAX_COORD)), y: round2(clamp(src.y + delta, MIN_COORD, MAX_COORD)) };
      const norm = codec?.normalizeBoardNode ? codec.normalizeBoardNode(spec, nid) : spec;
      state.nodes.push(norm);
      newIds.push(nid);
    }
    for (const c of conns) {
      const fromId = idMap.get(c.fromId);
      const toId = idMap.get(c.toId);
      if (fromId && toId) {
        state.connectors.push({ ...cloneJson(c), id: hooks.nextId("bc", state.connectors), fromId, toId });
      }
    }
    hooks.setSelection?.(newIds, null);
    hooks.noteActivity?.();
    return true;
  }

  function copySelected() {
    const h = resolveHooks();
    const snap = buildClipboardSnapshot(h);
    if (!snap) return false;
    clipData = snap;
    return true;
  }

  function pasteClipboard() {
    const h = resolveHooks();
    if (!h || !clipData) return false;
    clipData.pasteCount = (clipData.pasteCount || 0) + 1;
    return applySnapshotPaste(h, clipData, clipData.pasteCount);
  }

  function duplicateSelected() {
    const h = resolveHooks();
    const snap = buildClipboardSnapshot(h);
    if (!h || !snap) return false;
    return applySnapshotPaste(h, snap, 1);
  }

  function cutSelected() {
    const studio = host.__EASYOFFICE_BOARD_STUDIO__;
    if (!studio || !copySelected()) return false;
    return studio.deleteSelected();
  }

  function runClipboardCommand(cmd, writeClip) {
    const doc = activeDoc || resolveHooks()?.getViewportEl?.()?.ownerDocument || host.document;
    if (isEditableTarget(null, doc)) {
      const execName = { "clip-cut": "cut", "clip-copy": "copy", "clip-paste": "paste" }[cmd];
      if (!execName) return false;
      const util = host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__?.execTextFieldEdit;
      if (typeof util === "function") return util(execName, doc, writeClip);
      try {
        return typeof doc?.execCommand === "function" ? Boolean(doc.execCommand(execName)) : false;
      } catch {
        return false;
      }
    }
    if (cmd === "clip-copy") return copySelected();
    if (cmd === "clip-paste") return pasteClipboard();
    if (cmd === "clip-cut") return cutSelected();
    return false;
  }

  function handleKeydown(ev, doc) {
    if (!ev || ev.defaultPrevented) return false;
    if (!(ev.ctrlKey || ev.metaKey) || ev.altKey || ev.shiftKey) return false;
    if (doc?.querySelector?.("dialog[open]")) return false;
    if (isEditableTarget(ev.target, doc)) return false;
    const code = String(ev.code || "");
    const key = String(ev.key || "").toUpperCase();
    const letter = /^Key([XCVD])$/.exec(code)?.[1] || (["X", "C", "V", "D"].includes(key) ? key : "");
    if (!letter) return false;
    let handled = false;
    if (letter === "C") handled = copySelected();
    else if (letter === "V") handled = pasteClipboard();
    else if (letter === "D") handled = duplicateSelected();
    else if (letter === "X") handled = cutSelected();
    if (handled) ev.preventDefault?.();
    return handled;
  }

  host.__EASYOFFICE_BOARD_STYLE_BAR__ = {
    registerHooks: (h) => { activeHooks = h; },
    create: createBoardStyleBar,
    centerOnItem: centerOnBoardItem,
    copySelected,
    pasteClipboard,
    duplicateSelected,
    cutSelected,
    runClipboardCommand,
    handleKeydown,
    hasClipboard: () => Boolean(clipData),
    clearClipboard: () => { clipData = null; },
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
