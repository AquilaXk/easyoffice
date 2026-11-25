// 참조 구현 (reference implementation): rhwp-studio `src/i18n/locales/ko.ts` & `src/command/commands/file.ts`
// (MIT, Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md).
(function (host) {
  "use strict";

  const PX_PER_MM = 96 / 25.4;
  const DEBOUNCE_MS = 80;
  const MIN_ZOOM = 25;
  const MAX_ZOOM = 200;
  const ZOOM_STEPS = [50, 67, 75, 90, 100, 110, 125, 150, 175, 200];
  const DEFAULT_STAGE_W = 420;
  const DEFAULT_STAGE_H = 560;
  const STAGE_PAD_PX = 28;
  const SPREAD_GAP_PX = 14;
  const SUPPORTED_MODULES = new Set(["DOC", "SHEET", "SLIDE", "BASE", "BOARD", "PDF"]);

  const doc = host.document;
  if (!doc) return;

  const setTimer = typeof host.setTimeout === "function" ? host.setTimeout.bind(host) : setTimeout;
  const clearTimer = typeof host.clearTimeout === "function" ? host.clearTimeout.bind(host) : clearTimeout;
  const $ = (id) => doc.getElementById(id);

  let openState = false;
  let arrangement = "single";
  let zoomMode = "custom";
  let zoomPercent = 100;
  let currentPage = 1;
  let totalPages = 1;
  let lastPageSizesMm = [{ width: 210, height: 297 }];
  let debounceTimer = null;
  let renderSeq = 0;
  let bound = false;

  function activeModuleCode() {
    const officePrint = host.__EASYOFFICE_OFFICE_PRINT__;
    const appEl = $("app");
    if (appEl?.dataset?.hwpEmbed === "true") return "HWP_EMBED";
    return officePrint?.getActiveModule?.() || appEl?.dataset?.module || "DOC";
  }

  function isCurrentModuleSupported() {
    return SUPPORTED_MODULES.has(activeModuleCode());
  }

  function cancelScheduledRefresh() {
    if (debounceTimer !== null) {
      clearTimer(debounceTimer);
      debounceTimer = null;
    }
  }

  function clampZoom(val) {
    return Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, Math.round(Number(val) || 100)));
  }

  function computeEffectiveZoom(paperMm) {
    if (zoomMode === "custom") return clampZoom(zoomPercent);
    const stage = $("print-preview-stage");
    const stageW = Math.max(180, (Number(stage?.clientWidth) || DEFAULT_STAGE_W) - STAGE_PAD_PX);
    const stageH = Math.max(180, (Number(stage?.clientHeight) || DEFAULT_STAGE_H) - STAGE_PAD_PX);
    const cols = arrangement === "spread" ? 2 : 1;
    const availW = cols === 2 ? (stageW - SPREAD_GAP_PX) / 2 : stageW;
    const natW = Math.max(1, (paperMm?.width || 210) * PX_PER_MM);
    const natH = Math.max(1, (paperMm?.height || 297) * PX_PER_MM);
    if (zoomMode === "fit-width") return clampZoom((availW / natW) * 100);
    return clampZoom(Math.min(availW / natW, stageH / natH) * 100);
  }

  function applyToolbarAndSheetState() {
    const stage = $("print-preview-stage");
    if (!stage) return;
    stage.dataset.arrangement = arrangement;
    const singleBtn = $("btn-pp-single");
    const spreadBtn = $("btn-pp-spread");
    singleBtn?.setAttribute("aria-pressed", arrangement === "single" ? "true" : "false");
    spreadBtn?.setAttribute("aria-pressed", arrangement === "spread" ? "true" : "false");

    const refPaper = lastPageSizesMm[Math.max(0, currentPage - 1)] || lastPageSizesMm[0] || { width: 210, height: 297 };
    const effective = computeEffectiveZoom(refPaper);
    zoomPercent = effective;
    const zoomLabel = $("pp-zoom-label");
    if (zoomLabel) zoomLabel.textContent = effective + "%";

    const sheets = stage.querySelectorAll(".print-preview-sheet");
    for (let i = 0; i < sheets.length; i += 1) {
      const card = sheets[i];
      const size = lastPageSizesMm[i] || refPaper;
      const wPx = Math.round(size.width * PX_PER_MM);
      const hPx = Math.round(size.height * PX_PER_MM);
      card.style.width = wPx + "px";
      if (card.dataset.flowPage === "true") {
        card.style.minHeight = hPx + "px";
      } else {
        card.style.height = hPx + "px";
      }
      card.style.zoom = effective + "%";
      if (i + 1 === currentPage) card.dataset.currentPage = "true";
      else delete card.dataset.currentPage;
    }

    const pageStatus = $("pp-page-status");
    if (pageStatus) {
      pageStatus.textContent = (totalPages > 0 ? currentPage : 0) + " / " + totalPages + " 쪽";
    }
    const prevBtn = $("btn-pp-prev");
    const nextBtn = $("btn-pp-next");
    if (prevBtn) prevBtn.disabled = totalPages <= 1 || currentPage <= 1;
    if (nextBtn) nextBtn.disabled = totalPages <= 1 || currentPage >= totalPages;
  }

  async function refreshNow() {
    cancelScheduledRefresh();
    const pane = $("print-preview-pane");
    const stage = $("print-preview-stage");
    if (!pane || !stage) return false;
    if (!isCurrentModuleSupported()) {
      if (openState) close();
      return false;
    }
    const officePrint = host.__EASYOFFICE_OFFICE_PRINT__;
    if (!officePrint?.buildPreview) return false;

    const seq = ++renderSeq;
    let built;
    try {
      built = await officePrint.buildPreview();
    } catch (err) {
      if (seq !== renderSeq) return false;
      if (!isCurrentModuleSupported()) {
        if (openState) close();
        return false;
      }
      const empty = doc.createElement("div");
      empty.className = "print-preview-empty";
      empty.textContent = err?.message || "인쇄할 내용이 없습니다.";
      stage.replaceChildren(empty);
      totalPages = 0;
      currentPage = 0;
      applyToolbarAndSheetState();
      return false;
    }
    if (seq !== renderSeq || !openState) return false;

    const pages = Array.isArray(built.pages) ? built.pages : [];
    lastPageSizesMm = Array.isArray(built.pageSizesMm) && built.pageSizesMm.length > 0
      ? built.pageSizesMm
      : [{ width: 210, height: 297 }];
    totalPages = pages.length;
    if (totalPages === 0) {
      currentPage = 0;
      stage.replaceChildren();
      applyToolbarAndSheetState();
      return true;
    }
    currentPage = Math.max(1, Math.min(currentPage || 1, totalPages));

    const cards = pages.map((pageNode, idx) => {
      const card = doc.createElement("div");
      card.className = "print-preview-sheet";
      card.dataset.pageNumber = String(idx + 1);
      if (pageNode.classList?.contains("eo-print-flow")) {
        card.dataset.flowPage = "true";
        if (built.flowMargin) pageNode.style.padding = built.flowMargin;
      }
      card.appendChild(pageNode);
      return card;
    });
    stage.replaceChildren(...cards);
    applyToolbarAndSheetState();
    return true;
  }

  function scheduleRefresh() {
    if (!openState) return;
    if (!isCurrentModuleSupported()) {
      close();
      return;
    }
    cancelScheduledRefresh();
    debounceTimer = setTimer(() => {
      debounceTimer = null;
      void refreshNow();
    }, DEBOUNCE_MS);
  }

  async function open() {
    const pane = $("print-preview-pane");
    if (!pane) return false;
    if (!isCurrentModuleSupported()) {
      host.__EASYOFFICE_OFFICE_PRINT__?.notify?.("현재 창에서는 인쇄를 지원하지 않습니다.");
      return false;
    }
    openState = true;
    pane.hidden = false;
    await refreshNow();
    return true;
  }

  function close() {
    cancelScheduledRefresh();
    openState = false;
    renderSeq += 1;
    const pane = $("print-preview-pane");
    if (pane) pane.hidden = true;
    $("print-preview-stage")?.replaceChildren();
  }

  async function toggle() {
    if (openState) {
      close();
      return false;
    }
    return open();
  }

  function setArrangement(mode) {
    arrangement = mode === "spread" ? "spread" : "single";
    applyToolbarAndSheetState();
  }

  function setZoomMode(mode) {
    if (mode === "fit-width" || mode === "fit-page") {
      zoomMode = mode;
    } else {
      zoomMode = "custom";
      if (typeof mode === "number" && Number.isFinite(mode)) zoomPercent = clampZoom(mode);
    }
    applyToolbarAndSheetState();
  }

  function stepZoom(delta) {
    const cur = clampZoom(zoomPercent);
    let next = cur;
    if (delta > 0) {
      next = ZOOM_STEPS.find((z) => z > cur) ?? MAX_ZOOM;
    } else {
      for (let i = ZOOM_STEPS.length - 1; i >= 0; i -= 1) {
        if (ZOOM_STEPS[i] < cur) {
          next = ZOOM_STEPS[i];
          break;
        }
      }
      if (next === cur) next = MIN_ZOOM;
    }
    zoomMode = "custom";
    zoomPercent = next;
    applyToolbarAndSheetState();
  }

  function goToPage(pageNumber) {
    if (totalPages <= 0) return;
    currentPage = Math.max(1, Math.min(Math.trunc(Number(pageNumber) || 1), totalPages));
    applyToolbarAndSheetState();
    const target = $("print-preview-stage")?.querySelector(`.print-preview-sheet[data-page-number="${currentPage}"]`);
    target?.scrollIntoView?.({ block: "nearest" });
  }

  function syncModuleVisibility() {
    if (!openState) return;
    if (!isCurrentModuleSupported()) {
      close();
      return;
    }
    scheduleRefresh();
  }

  function isInsidePreviewPane(target) {
    const pane = $("print-preview-pane");
    return Boolean(pane && target?.nodeType === 1 && pane.contains(target));
  }

  function bind() {
    if (bound) return;
    const pane = $("print-preview-pane");
    if (!pane) return;
    bound = true;

    for (const item of doc.querySelectorAll('.md-item[data-verify-act="print-preview"]')) {
      item.addEventListener("click", () => {
        host.__EASYOFFICE_CLOSE_MENUS__?.(doc);
        void toggle();
      });
    }

    $("btn-pp-single")?.addEventListener("click", () => setArrangement("single"));
    $("btn-pp-spread")?.addEventListener("click", () => setArrangement("spread"));
    $("btn-pp-zoom-in")?.addEventListener("click", () => stepZoom(1));
    $("btn-pp-zoom-out")?.addEventListener("click", () => stepZoom(-1));
    $("btn-pp-fit-width")?.addEventListener("click", () => setZoomMode("fit-width"));
    $("btn-pp-fit-page")?.addEventListener("click", () => setZoomMode("fit-page"));
    $("btn-pp-prev")?.addEventListener("click", () => goToPage(currentPage - 1));
    $("btn-pp-next")?.addEventListener("click", () => goToPage(currentPage + 1));
    $("btn-pp-print")?.addEventListener("click", () => {
      const officePrint = host.__EASYOFFICE_OFFICE_PRINT__;
      if (!officePrint?.print) return;
      Promise.resolve(officePrint.print()).catch((err) => {
        officePrint.notify?.(err?.message || String(err));
      });
    });
    $("btn-pp-close")?.addEventListener("click", () => close());

    const eventRoot = doc.body || doc;
    const onEditEvent = (ev) => {
      if (!openState || isInsidePreviewPane(ev?.target)) return;
      scheduleRefresh();
    };
    for (const evtName of ["input", "change", "keyup", "mouseup", "eo-doc-mutated"]) {
      eventRoot.addEventListener(evtName, onEditEvent);
    }

    eventRoot.addEventListener("click", (ev) => {
      if (!openState || isInsidePreviewPane(ev?.target)) return;
      Promise.resolve().then(syncModuleVisibility);
    });

    const origActivate = host.__EASYOFFICE_ACTIVATE_MODULE__;
    if (typeof origActivate === "function" && !origActivate.__eoPrintPreviewWrapped) {
      const wrapped = (mod) => {
        const res = origActivate(mod);
        syncModuleVisibility();
        Promise.resolve(res).then(syncModuleVisibility, () => {});
        return res;
      };
      wrapped.__eoPrintPreviewWrapped = true;
      host.__EASYOFFICE_ACTIVATE_MODULE__ = wrapped;
    }

    const appEl = $("app");
    if (appEl && typeof host.MutationObserver === "function") {
      const obs = new host.MutationObserver(() => syncModuleVisibility());
      obs.observe(appEl, { attributes: true, attributeFilter: ["data-module", "data-hwp-embed"] });
    }
  }

  const api = {
    open,
    close,
    toggle,
    isOpen: () => openState,
    refreshNow,
    setArrangement,
    setZoomMode,
    goToPage,
  };
  host.__EASYOFFICE_PRINT_PREVIEW__ = api;

  bind();
  doc.addEventListener("DOMContentLoaded", bind);
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
