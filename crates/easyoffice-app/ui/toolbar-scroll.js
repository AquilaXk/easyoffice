// 참조 구현: rhwp-studio `src/ui/icon-toolbar-scroller.ts` (MIT, Copyright (c) 2025-2026 Edward Kim;
// see THIRD_PARTY_NOTICES.md). Keeps multi-group toolbars single-row and adds `«` / `»` scroll buttons.
(function (host) {
  "use strict";

  const EPSILON = 1;
  const controllers = new Map();

  function stepBoundaryTarget(viewport, track, dir) {
    const boundaries = [];
    const origin = track.getBoundingClientRect().left;
    for (const sep of Array.from(track.children)) {
      const cls = String(sep.className || "").split(/\s+/);
      if (!cls.includes("tb-sep") && !cls.includes("eo-style-sep")) continue;
      if (sep.hidden || sep.closest("[hidden]")) continue;
      boundaries.push(Math.round(sep.getBoundingClientRect().left - origin));
    }
    const maxScroll = Math.max(0, track.scrollWidth - viewport.clientWidth);
    const cur = viewport.scrollLeft;
    if (dir > 0) return Math.min(maxScroll, boundaries.find((b) => b > cur + EPSILON) ?? maxScroll);
    for (let i = boundaries.length - 1; i >= 0; i -= 1) {
      if (boundaries[i] < cur - EPSILON) return Math.max(0, boundaries[i]);
    }
    return 0;
  }

  function attach(toolbar, docArg) {
    const doc = docArg || toolbar?.ownerDocument || host.document;
    if (!toolbar || !doc || toolbar.dataset.tbScrollBound === "true") return null;
    toolbar.dataset.tbScrollBound = "true";
    toolbar.classList.add("tb-scrollable");

    const viewport = doc.createElement("div");
    viewport.className = "tb-scroll-viewport";
    viewport.tabIndex = 0;
    const track = doc.createElement("div");
    track.className = "tb-scroll-track";
    while (toolbar.firstChild) track.appendChild(toolbar.firstChild);
    viewport.appendChild(track);

    const makeNav = (cls, text, label) => {
      const btn = doc.createElement("button");
      btn.type = "button";
      btn.className = "tb-scroll-nav " + cls;
      btn.hidden = true;
      btn.setAttribute("aria-label", label);
      btn.title = label;
      const glyph = doc.createElement("span");
      glyph.className = "tb-scroll-nav-icon";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = text;
      btn.appendChild(glyph);
      return btn;
    };

    const prev = makeNav("tb-scroll-nav-prev", "«", "이전 도구");
    const next = makeNav("tb-scroll-nav-next", "»", "다음 도구");
    toolbar.appendChild(prev);
    toolbar.appendChild(viewport);
    toolbar.appendChild(next);

    function refresh() {
      if (toolbar.hidden) return;
      const maxScroll = Math.max(0, track.scrollWidth - viewport.clientWidth);
      const overflowing = maxScroll > EPSILON;
      viewport.classList[overflowing ? "add" : "remove"]("tb-scroll-viewport-enabled");
      if (!overflowing) {
        if (viewport.scrollLeft !== 0) viewport.scrollLeft = 0;
        prev.hidden = true;
        next.hidden = true;
        return;
      }
      prev.hidden = false;
      next.hidden = false;
      const cur = viewport.scrollLeft;
      const atStart = cur <= EPSILON;
      const atEnd = cur >= maxScroll - EPSILON;
      prev.disabled = atStart;
      next.disabled = atEnd;
      prev.setAttribute("aria-disabled", String(atStart));
      next.setAttribute("aria-disabled", String(atEnd));
    }

    prev.addEventListener("click", () => {
      if (prev.disabled) return;
      viewport.scrollLeft = stepBoundaryTarget(viewport, track, -1);
      refresh();
    });
    next.addEventListener("click", () => {
      if (next.disabled) return;
      viewport.scrollLeft = stepBoundaryTarget(viewport, track, 1);
      refresh();
    });
    viewport.addEventListener("scroll", refresh);
    toolbar.addEventListener("wheel", (ev) => {
      const target = (toolbar.scrollWidth || 0) > (toolbar.clientWidth || 0)
        ? toolbar
        : (track.scrollWidth || 0) > (viewport.clientWidth || 0)
          ? viewport
          : null;
      if (!target) return;
      const delta = ev.deltaY || ev.deltaX || 0;
      if (!delta) return;
      target.scrollLeft = Math.max(0, Math.min(target.scrollWidth - target.clientWidth, (target.scrollLeft || 0) + delta));
      ev.preventDefault?.();
      refresh();
    });
    toolbar.addEventListener("focusin", (ev) => {
      const btn = ev.target?.closest?.(".tb-btn");
      if (!btn) return;
      const target = (toolbar.scrollWidth || 0) > (toolbar.clientWidth || 0) ? toolbar : viewport;
      if ((target.scrollWidth || 0) > (target.clientWidth || 0) && typeof target.getBoundingClientRect === "function" && typeof btn.getBoundingClientRect === "function") {
        const barRect = target.getBoundingClientRect(), btnRect = btn.getBoundingClientRect();
        if (btnRect.left < barRect.left) target.scrollLeft -= barRect.left - btnRect.left;
        else if (btnRect.right > barRect.right) target.scrollLeft += btnRect.right - barRect.right;
      }
      btn.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    });

    if (typeof host.ResizeObserver === "function") {
      const ro = new host.ResizeObserver(refresh);
      ro.observe(toolbar);
      ro.observe(track);
    }
    if (typeof host.MutationObserver === "function") {
      const mo = new host.MutationObserver(refresh);
      mo.observe(toolbar, { attributes: true, attributeFilter: ["hidden"] });
      mo.observe(track, { attributes: true, subtree: true, attributeFilter: ["hidden", "style"] });
    }
    refresh();
    const api = { refresh };
    controllers.set(toolbar, api);
    return api;
  }

  function refreshAll(docArg) {
    const doc = docArg || host.document;
    if (!doc) return;
    for (const id of ["icon-toolbar", "doc-style-bar", "sheet-table-bar", "board-toolbar"]) {
      const el = doc.getElementById(id);
      const ctrl = el && controllers.get(el);
      if (ctrl) ctrl.refresh();
    }
  }

  function attachAll(docArg) {
    const doc = docArg || host.document;
    if (!doc) return [];
    const list = [];
    for (const id of ["icon-toolbar", "doc-style-bar", "sheet-table-bar", "board-toolbar"]) {
      const el = doc.getElementById(id);
      if (el) list.push(attach(el, doc));
    }
    refreshAll(doc);
    return list;
  }

  if (host.document?.readyState === "loading") {
    host.document.addEventListener("DOMContentLoaded", () => attachAll(host.document));
  } else if (host.document?.getElementById) {
    attachAll(host.document);
  }

  host.__EASYOFFICE_TOOLBAR_SCROLL__ = { attach, attachAll, refreshAll };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
