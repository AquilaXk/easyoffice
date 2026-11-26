(function (host) {
  "use strict";

  // Slide canvas right-click context menu (#slide-context-menu canvas mode) and multi-object
  // alignment / distribution (`맞춤`, official Korean office suite terminology).

  const round2 = (v) => Math.round(v * 100) / 100;
  const ALIGN_LABELS = Object.freeze({
    "align-left": "왼쪽 맞춤",
    "align-center": "가운데 맞춤",
    "align-right": "오른쪽 맞춤",
    "align-top": "위쪽 맞춤",
    "align-middle": "중간 맞춤",
    "align-bottom": "아래쪽 맞춤",
    "distribute-h": "가로 간격 균등하게",
    "distribute-v": "세로 간격 균등하게",
  });

  function createSlideContextMenu(deps) {
    const { document, getActiveModule, getCanvas, getClipboard, setFeedback } = deps;

    const byId = (id) => document.getElementById(id);

    const isAnyMenuOpen = () => {
      const menu = byId("slide-context-menu");
      return Boolean(menu && !menu.hidden);
    };

    const isCanvasMenuOpen = () => {
      const menu = byId("slide-context-menu");
      return Boolean(menu && !menu.hidden && menu.dataset.mode === "canvas");
    };

    function closeCanvasMenu() {
      const menu = byId("slide-context-menu");
      if (menu && !menu.hidden) {
        menu.hidden = true;
        delete menu.dataset.mode;
      }
    }

    function selectedList(canvas) {
      const els = canvas?.getElements?.() || [];
      const ids = canvas?.getSelectedIds?.() || [];
      if (ids.length > 0) {
        const idSet = new Set(ids);
        return els.filter((e) => idSet.has(e.id));
      }
      const one = canvas?.getSelectedElement?.();
      return one ? [one] : [];
    }

    function alignElements(mode) {
      const canvas = getCanvas?.();
      if (!canvas || !Object.hasOwn(ALIGN_LABELS, mode)) return false;
      const targets = selectedList(canvas);
      if (mode === "distribute-h" || mode === "distribute-v") {
        if (targets.length < 3) {
          setFeedback?.("균등 배치는 개체를 3개 이상 선택하세요.");
          return false;
        }
        const horiz = mode === "distribute-h";
        let changed = false;
        canvas.mutateWithUndo?.(() => {
          const sorted = targets.slice().sort((a, b) => (horiz ? a.x - b.x : a.y - b.y));
          const first = sorted[0];
          const last = sorted[sorted.length - 1];
          const start = horiz ? first.x : first.y;
          const end = horiz ? last.x + last.w : last.y + last.h;
          const totalSize = sorted.reduce((acc, el) => acc + (horiz ? el.w : el.h), 0);
          const gap = (end - start - totalSize) / (sorted.length - 1);
          let cursor = start;
          for (let i = 0; i < sorted.length; i += 1) {
            const el = sorted[i];
            if (i > 0 && i < sorted.length - 1) {
              const nextCoord = round2(cursor);
              if (horiz && el.x !== nextCoord) { el.x = nextCoord; changed = true; }
              if (!horiz && el.y !== nextCoord) { el.y = nextCoord; changed = true; }
            }
            cursor += (horiz ? el.w : el.h) + gap;
          }
          return changed;
        });
        if (changed) setFeedback?.(ALIGN_LABELS[mode] + " 적용");
        return changed;
      }

      if (targets.length === 0) {
        setFeedback?.("맞춤할 슬라이드 개체를 먼저 선택하세요.");
        return false;
      }
      const stage = canvas.stageSize?.() || { w: 960, h: 540 };
      const box = targets.length === 1
        ? { minX: 0, maxX: stage.w, minY: 0, maxY: stage.h }
        : {
            minX: Math.min(...targets.map((e) => e.x)),
            maxX: Math.max(...targets.map((e) => e.x + e.w)),
            minY: Math.min(...targets.map((e) => e.y)),
            maxY: Math.max(...targets.map((e) => e.y + e.h)),
          };
      const midX = (box.minX + box.maxX) / 2;
      const midY = (box.minY + box.maxY) / 2;
      let changed = false;
      canvas.mutateWithUndo?.(() => {
        for (const el of targets) {
          let nx = el.x;
          let ny = el.y;
          if (mode === "align-left") nx = round2(box.minX);
          else if (mode === "align-center") nx = round2(midX - el.w / 2);
          else if (mode === "align-right") nx = round2(box.maxX - el.w);
          else if (mode === "align-top") ny = round2(box.minY);
          else if (mode === "align-middle") ny = round2(midY - el.h / 2);
          else if (mode === "align-bottom") ny = round2(box.maxY - el.h);
          nx = Math.max(0, Math.min(Math.max(0, stage.w - el.w), nx));
          ny = Math.max(0, Math.min(Math.max(0, stage.h - el.h), ny));
          if (nx !== el.x || ny !== el.y) {
            el.x = nx;
            el.y = ny;
            changed = true;
          }
        }
        return changed;
      });
      if (changed) setFeedback?.(ALIGN_LABELS[mode] + " 적용");
      return changed;
    }

    function openCanvasMenu(ev) {
      const menu = byId("slide-context-menu");
      const thumbGrp = byId("slide-ctx-thumb-group");
      const canvasGrp = byId("slide-ctx-canvas-group");
      const canvas = getCanvas?.();
      if (!menu || !thumbGrp || !canvasGrp || !canvas) return;
      thumbGrp.hidden = true;
      canvasGrp.hidden = false;
      menu.dataset.mode = "canvas";
      const hasSel = selectedList(canvas).length > 0;
      const canPaste = Boolean(getClipboard?.()?.hasClipboard?.());
      canvasGrp.querySelectorAll("[data-slide-canvas-act]").forEach((item) => {
        const act = item.dataset.slideCanvasAct;
        const ok = act === "paste" ? canPaste : hasSel;
        item.setAttribute("aria-disabled", ok ? "false" : "true");
      });
      const vw = Number(host.innerWidth) || 1280;
      const vh = Number(host.innerHeight) || 760;
      const left = Math.max(0, Math.min(vw - 216, Number(ev.clientX) || 0));
      const top = Math.max(0, Math.min(vh - 280, Number(ev.clientY) || 0));
      menu.style.left = left + "px";
      menu.style.top = top + "px";
      menu.hidden = false;
    }

    function runCanvasAction(act) {
      const canvas = getCanvas?.();
      const clip = getClipboard?.();
      if (!canvas) return false;
      if (act === "cut") return Boolean(clip?.cutSelected?.());
      if (act === "copy") return Boolean(clip?.copySelected?.());
      if (act === "paste") return Boolean(clip?.pasteClipboard?.());
      if (act === "duplicate") return Boolean(clip?.duplicateSelected?.());
      if (act === "bring-front") return Boolean(canvas.bringToFront?.());
      if (act === "bring-forward") return Boolean(canvas.bringForward?.());
      if (act === "send-backward") return Boolean(canvas.sendBackward?.());
      if (act === "send-back") return Boolean(canvas.sendToBack?.());
      if (act === "delete") return Boolean(canvas.deleteElement?.());
      return false;
    }

    function bind() {
      const stage = byId("slide-stage-box");
      const menu = byId("slide-context-menu");
      const canvasGrp = byId("slide-ctx-canvas-group");
      stage?.addEventListener("contextmenu", (ev) => {
        if (getActiveModule?.() !== "SLIDE") return;
        const card = ev.target?.closest?.(".slide-el");
        if (!card && (ev.target?.closest?.("#slide-title-zone") || ev.target?.closest?.("#slide-lower-zone"))) return;
        const canvas = getCanvas?.();
        if (card) {
          const elId = card.dataset.elementId;
          const ids = canvas?.getSelectedIds?.() || [];
          if (elId && !ids.includes(elId)) canvas?.selectElement?.(elId);
        }
        ev.preventDefault?.();
        openCanvasMenu(ev);
      });
      canvasGrp?.addEventListener("click", (ev) => {
        const item = ev.target?.closest?.("[data-slide-canvas-act]");
        if (!item || item.getAttribute("aria-disabled") === "true") return;
        ev.stopPropagation?.();
        const act = item.dataset.slideCanvasAct;
        closeCanvasMenu();
        runCanvasAction(act);
      });
      document.addEventListener("mousedown", (ev) => {
        if (isAnyMenuOpen() && menu && !menu.contains(ev.target)) closeCanvasMenu();
      });
      const closeIfOutside = (ev) => {
        if (isAnyMenuOpen() && (ev.target?.closest?.("dialog[open]") || (menu && !menu.contains?.(ev.target)))) {
          closeCanvasMenu();
        }
      };
      document.addEventListener("focusin", closeIfOutside);
      document.addEventListener("focus", closeIfOutside, true);
      for (const dlg of document.querySelectorAll?.("dialog") || []) {
        for (const m of ["showModal", "show"]) {
          const orig = dlg[m];
          if (typeof orig === "function" && !orig._eoSlideCtxWrapped) {
            const wrapped = function (...args) { closeCanvasMenu(); return orig.apply(this, args); };
            wrapped._eoSlideCtxWrapped = true;
            dlg[m] = wrapped;
          }
        }
      }
      if (typeof host.MutationObserver === "function" && document.body) {
        const obs = new host.MutationObserver(() => {
          if (isAnyMenuOpen() && document.querySelector("dialog[open]")) closeCanvasMenu();
        });
        obs.observe(document.body, { subtree: true, attributes: true, attributeFilter: ["open"] });
      }
      byId("menu-bar")?.addEventListener("mousedown", () => {
        if (isAnyMenuOpen()) closeCanvasMenu();
      });
      host.addEventListener?.("blur", () => {
        if (isAnyMenuOpen()) closeCanvasMenu();
      });
      document.addEventListener("keydown", (ev) => {
        if (isCanvasMenuOpen() && ev.key === "Escape") {
          ev.preventDefault?.();
          ev.stopPropagation?.();
          closeCanvasMenu();
        }
      }, true);
    }

    return {
      bind,
      alignElements,
      openCanvasMenu,
      closeCanvasMenu,
      runCanvasAction,
      isCanvasMenuOpen,
    };
  }

  host.__EASYOFFICE_CREATE_SLIDE_CONTEXT_MENU__ = createSlideContextMenu;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
