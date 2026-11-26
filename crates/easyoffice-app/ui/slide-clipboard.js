(function (host) {
  "use strict";

  // Slide canvas object clipboard (Cut / Copy / Paste / Duplicate) with offset placement
  // and 1-step undo. Only acts when focus is on the slide stage or canvas elements, never
  // hijacking <input>, <textarea>, <select>, or contenteditable zones.

  const MAX_ELEMENTS_PER_SLIDE = 256;
  const OFFSET_PT = 16;
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const round2 = (v) => Math.round(v * 100) / 100;

  function isEditableField(el) {
    const node = el?.nodeType === 3 ? el.parentElement : el;
    if (!node || node.nodeType !== 1) return false;
    const tag = String(node.tagName || "").toUpperCase();
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || node.isContentEditable) return true;
    const ceEl = node.closest?.("#slide-title-zone, #slide-lower-zone, [contenteditable]");
    if (!ceEl) return false;
    if (ceEl.id === "slide-title-zone" || ceEl.id === "slide-lower-zone") return true;
    const ce = ceEl.getAttribute?.("contenteditable");
    return ce === "" || ce === "true" || ce === "plaintext-only";
  }

  const cssEsc = (v) => (typeof host.CSS?.escape === "function" ? host.CSS.escape(String(v)) : String(v).replace(/["\\]/g, "\\$&"));

  function createSlideClipboard(deps) {
    const { document, getActiveModule, getCanvas, setFeedback } = deps;
    let clipItems = [];
    let pasteSeq = 0;

    function cloneElement(el) {
      return typeof structuredClone === "function" ? structuredClone(el) : JSON.parse(JSON.stringify(el));
    }

    function selectedElements(canvas) {
      const els = canvas?.getElements?.() || [];
      const ids = canvas?.getSelectedIds?.() || [];
      if (ids.length > 0) {
        const set = new Set(ids);
        return els.filter((e) => set.has(e.id));
      }
      const one = canvas?.getSelectedElement?.();
      return one ? [one] : [];
    }

    function copySelected() {
      const canvas = getCanvas?.();
      if (!canvas) return false;
      const items = selectedElements(canvas);
      if (items.length === 0) {
        setFeedback?.("복사할 슬라이드 개체를 먼저 선택하세요.");
        return false;
      }
      clipItems = items.map(cloneElement);
      pasteSeq = 0;
      setFeedback?.("슬라이드 개체 " + clipItems.length + "개 복사");
      return true;
    }

    function cutSelected() {
      const canvas = getCanvas?.();
      if (!canvas) return false;
      const items = selectedElements(canvas);
      if (items.length === 0) {
        setFeedback?.("잘라낼 슬라이드 개체를 먼저 선택하세요.");
        return false;
      }
      const cutIds = new Set(items.map((e) => e.id));
      clipItems = items.map(cloneElement);
      pasteSeq = 0;
      canvas.mutateWithUndo?.((els) => {
        for (let i = els.length - 1; i >= 0; i -= 1) {
          if (cutIds.has(els[i].id)) els.splice(i, 1);
        }
        canvas.clearSelection?.();
        return true;
      });
      document.getElementById("slide-stage-box")?.focus?.();
      setFeedback?.("슬라이드 개체 " + clipItems.length + "개 잘라내기");
      return true;
    }

    function insertOffsetBatch(items, stepCount, label) {
      const canvas = getCanvas?.();
      if (!canvas || !Array.isArray(items) || items.length === 0) return false;
      const stage = canvas.stageSize?.() || { w: 960, h: 540 };
      const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
      const delta = OFFSET_PT * stepCount;
      let addedIds = [];
      let blocked = false;
      const ok = canvas.mutateWithUndo?.((els) => {
        if (els.length + items.length > MAX_ELEMENTS_PER_SLIDE) {
          blocked = true;
          return false;
        }
        const used = new Set(els.map((e) => String(e.id || "")));
        let nextNum = els.length + 1;
        const freshId = () => {
          while (used.has("el-" + nextNum)) nextNum += 1;
          const id = "el-" + nextNum;
          used.add(id);
          return id;
        };
        addedIds = [];
        for (const src of items) {
          const copy = cloneElement(src);
          const id = freshId();
          const w = Number(copy.w) || 80;
          const h = Number(copy.h) || 40;
          const nx = round2(clamp((Number(copy.x) || 0) + delta, 0, Math.max(0, stage.w - w)));
          const ny = round2(clamp((Number(copy.y) || 0) + delta, 0, Math.max(0, stage.h - h)));
          copy.id = id;
          copy.x = nx;
          copy.y = ny;
          const aspect = document.getElementById("slide-stage-box")?.dataset.aspect || "16:9";
          const orientation = document.getElementById("slide-stage-box")?.dataset.orientation || "landscape";
          const norm = codec?.normalizeSlideElement ? codec.normalizeSlideElement(copy, aspect, id, orientation) : copy;
          if (norm) {
            els.push(norm);
            addedIds.push(norm.id);
          }
        }
        return addedIds.length > 0;
      });
      if (blocked) {
        setFeedback?.("슬라이드당 개체 수 한도(" + MAX_ELEMENTS_PER_SLIDE + "개)를 초과했습니다.");
        return false;
      }
      if (!ok) return false;
      canvas.setSelectedIds?.(addedIds);
      const lastCard = document.querySelector('#slide-canvas-layer .slide-el[data-element-id="' + cssEsc(addedIds.at(-1)) + '"]');
      lastCard?.focus?.();
      setFeedback?.("슬라이드 개체 " + addedIds.length + "개 " + label);
      return true;
    }

    function pasteClipboard() {
      if (clipItems.length === 0) {
        setFeedback?.("붙여넣을 슬라이드 개체가 없습니다.");
        return false;
      }
      pasteSeq += 1;
      return insertOffsetBatch(clipItems, pasteSeq, "붙여넣기");
    }

    function duplicateSelected() {
      const canvas = getCanvas?.();
      if (!canvas) return false;
      const items = selectedElements(canvas);
      if (items.length === 0) {
        setFeedback?.("복제할 슬라이드 개체를 먼저 선택하세요.");
        return false;
      }
      return insertOffsetBatch(items.map(cloneElement), 1, "복제");
    }

    function canHandleKey(ev) {
      if (getActiveModule?.() !== "SLIDE") return false;
      if (ev.isComposing || ev.altKey || !(ev.ctrlKey || ev.metaKey)) return false;
      if (document.querySelector("dialog[open]") || document.querySelector("#slide-show-overlay:not([hidden])")) return false;
      const target = ev.target || document.activeElement;
      if (isEditableField(target)) return false;
      const stageBox = document.getElementById("slide-stage-box");
      const activeEl = document.activeElement;
      const inStage = Boolean(stageBox && ((target && stageBox.contains?.(target)) || (activeEl && stageBox.contains?.(activeEl))));
      if (!inStage) return false;
      if (activeEl && isEditableField(activeEl)) return false;
      return true;
    }

    function handleKeydown(ev) {
      if (!canHandleKey(ev)) return false;
      const key = String(ev.key || "").toUpperCase();
      const code = String(ev.code || "");
      const letter = /^Key([XCVD])$/.exec(code)?.[1] || (["X", "C", "V", "D"].includes(key) ? key : "");
      if (!letter || ev.shiftKey) return false;
      const canvas = getCanvas?.();
      const hasSel = selectedElements(canvas).length > 0;
      if (letter === "C" && hasSel) {
        ev.preventDefault?.();
        return copySelected();
      }
      if (letter === "X" && hasSel) {
        ev.preventDefault?.();
        return cutSelected();
      }
      if (letter === "D" && hasSel) {
        ev.preventDefault?.();
        return duplicateSelected();
      }
      if (letter === "V" && clipItems.length > 0) {
        ev.preventDefault?.();
        return pasteClipboard();
      }
      return false;
    }

    function runMenuClipboard(cmd) {
      if (isEditableField(document.activeElement)) {
        const execName = { "clip-cut": "cut", "clip-copy": "copy", "clip-paste": "paste" }[cmd];
        if (!execName) return false;
        const util = host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__?.execTextFieldEdit;
        if (typeof util === "function") return util(execName, document);
        try {
          return typeof document.execCommand === "function" ? Boolean(document.execCommand(execName)) : false;
        } catch {
          return false;
        }
      }
      if (cmd === "clip-cut") return cutSelected();
      if (cmd === "clip-copy") return copySelected();
      if (cmd === "clip-paste") return pasteClipboard();
      return false;
    }

    return {
      copySelected,
      cutSelected,
      pasteClipboard,
      duplicateSelected,
      runMenuClipboard,
      handleKeydown,
      hasClipboard: () => clipItems.length > 0,
      clearClipboard: () => { clipItems = []; pasteSeq = 0; },
    };
  }

  host.__EASYOFFICE_CREATE_SLIDE_CLIPBOARD__ = createSlideClipboard;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
