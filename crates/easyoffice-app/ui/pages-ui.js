(function (host) {
  "use strict";

  // Format Inspector (`#pages-inspector`), DOC/HWP/Pages UI terminology sync,
  // and visual slide-layout wireframe thumbnails inside `#slide-layout-select`.
  function createPagesUi(deps = {}) {
    const doc = deps.document || host.document;
    let inspectorOpen = true;
    let bound = false;

    const byId = (id) => doc?.getElementById?.(id) || null;
    const isPagesActive = () =>
      byId("app")?.dataset?.docFlavor === "pages";

    function triggerCmd(cmd) {
      const target =
        doc.querySelector(`.md-item[data-studio-menu-cmd="${cmd}"]:not([hidden])`) ||
        doc.querySelector(`#icon-toolbar button[data-studio-cmd="${cmd}"]:not([hidden])`) ||
        doc.querySelector(`.md-item[data-studio-menu-cmd="${cmd}"]`);
      target?.click?.();
    }

    function setBtnState(btn, on) {
      if (!btn) return;
      btn.classList[on ? "add" : "remove"]("active");
      btn.setAttribute("aria-pressed", on ? "true" : "false");
    }

    function dispatchChange(el) {
      const ev = typeof host.Event === "function" ? new host.Event("change", { bubbles: true }) : { type: "change", bubbles: true };
      el.dispatchEvent(ev);
    }

    function syncMarginSummary() {
      const box = byId("pages-insp-margin-summary");
      const raw = byId("doc-editor")?.dataset?.eoPage || "";
      if (!box) return;
      const core = host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__;
      const setup = core?.decode?.(raw, "DOC");
      if (!setup) {
        box.textContent = "위 3.0 · 아래 2.5 · 왼 2.5 · 오른 2.5 cm";
        return;
      }
      const cm = (tw) => (Math.round(((Number(tw) || 0) / 566.9291) * 10) / 10).toFixed(1);
      box.textContent = `위 ${cm(setup.top)} · 아래 ${cm(setup.bottom)} · 왼 ${cm(setup.left)} · 오른 ${cm(setup.right)} cm`;
    }

    function mirrorSelect(srcId, dstId) {
      const src = byId(srcId);
      const dst = byId(dstId);
      if (!src || !dst) return;
      const srcOpts = Array.from(src.options || []);
      const dstOpts = Array.from(dst.options || []);
      const same =
        srcOpts.length === dstOpts.length &&
        srcOpts.every((o, i) => dstOpts[i]?.value === o.value && dstOpts[i]?.textContent === o.textContent);
      if (!same) {
        while (dst.firstChild) dst.removeChild(dst.firstChild);
        for (const o of srcOpts) {
          const opt = doc.createElement("option");
          opt.value = o.value;
          opt.textContent = o.textContent;
          dst.appendChild(opt);
        }
      }
      if (dst.value !== src.value) dst.value = src.value;
      host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(dst);
    }

    function syncInspectorValues() {
      if (!byId("pages-inspector")) return;
      mirrorSelect("doc-style-select", "pages-insp-style");
      mirrorSelect("doc-font-select", "pages-insp-font");
      mirrorSelect("doc-linespacing-select", "pages-insp-linespacing");
      const sizeSrc = byId("doc-font-size-input");
      const sizeDst = byId("pages-insp-size");
      if (sizeSrc && sizeDst && doc.activeElement !== sizeDst) sizeDst.value = sizeSrc.value;
      for (const [key, srcId] of [
        ["bold", "btn-stylebar-bold"],
        ["italic", "btn-stylebar-italic"],
        ["underline", "btn-stylebar-underline"],
        ["strike", "btn-stylebar-strike"],
      ]) {
        const src = byId(srcId);
        const btn = doc.querySelector(`[data-pages-style-btn="${key}"]`);
        setBtnState(btn, src?.getAttribute("aria-pressed") === "true" || src?.classList?.contains("active"));
      }
      for (const mode of ["left", "center", "right", "justify"]) {
        const src = byId("btn-align-" + mode);
        const btn = doc.querySelector(`[data-pages-align-btn="${mode}"]`);
        setBtnState(btn, src?.classList?.contains("active"));
      }
      syncMarginSummary();
    }

    function syncProductLabels(mod, isPages) {
      const corner = byId("ruler-corner");
      if (corner) corner.textContent = mod === "HWP" ? "mm" : "cm";
      const hwp = mod === "HWP";
      const setTitle = (id, text) => {
        const el = byId(id);
        if (el) el.textContent = text;
      };
      setTitle("doc-cell-border-title", hwp ? "셀 테두리/배경" : "테두리 및 음영");
      setTitle("doc-table-props-title", hwp ? "표/셀 속성" : "표 속성");
      setTitle("symbol-picker-title", hwp ? "문자표" : "기호");
      const btnText = (sel, lbl, title) => {
        const btn = doc.querySelector(sel);
        if (!btn) return;
        const span = btn.querySelector(".tb-label");
        if (span) span.textContent = lbl;
        btn.setAttribute("title", title);
        btn.setAttribute("aria-label", title);
      };
      btnText('#icon-toolbar [data-studio-cmd="para-numbering"]', hwp ? "문단\n번호" : "번호\n매기기", hwp ? "문단 번호" : "번호 매기기");
      btnText('#icon-toolbar [data-studio-cmd="para-bullet"]', hwp ? "글머리\n표" : "글머리\n기호", hwp ? "글머리표" : "글머리 기호");
      btnText('#icon-toolbar [data-studio-cmd="mod-symbol"]', hwp ? "문자표" : "기호", hwp ? "문자표" : "기호");
      btnText('#icon-toolbar [data-studio-cmd="para-shape"]', hwp ? "양쪽\n정렬" : isPages ? "본문\n정렬" : "양쪽\n정렬", "양쪽 정렬");
    }

    function sync(mod) {
      const app = byId("app");
      const editor = byId("doc-editor");
      const embed = app?.dataset?.hwpEmbed === "true";
      const start = app?.dataset?.windowKind === "start";
      const isPages = mod === "DOC" && !embed && !start && editor?.dataset?.eoDocFlavor === "pages";
      if (app) {
        if (isPages) app.dataset.docFlavor = "pages";
        else delete app.dataset.docFlavor;
      }
      const insp = byId("pages-inspector");
      if (insp) insp.hidden = !(isPages && inspectorOpen);
      const toggleItem = doc.querySelector("[data-pages-inspector-toggle]");
      if (toggleItem) toggleItem.setAttribute("aria-checked", isPages && inspectorOpen ? "true" : "false");
      syncProductLabels(mod, isPages);
      if (isPages) syncInspectorValues();
      host.__EASYOFFICE_TOOLBAR_SCROLL__?.refreshAll?.(doc);
    }

    function enhanceSlideLayoutPicker() {
      const root = doc?.querySelector?.('.custom-select[data-select-id="slide-layout-select"]');
      if (!root) return;
      const menu = root.querySelector(".custom-select-menu");
      if (menu) menu.classList.add("slide-layout-grid-menu");
      for (const row of root.querySelectorAll(".custom-select-option")) {
        if (row.querySelector(".slide-layout-thumb")) continue;
        const thumb = doc.createElement("span");
        thumb.className = "slide-layout-thumb";
        thumb.dataset.thumb = row.dataset.value || "blank";
        thumb.setAttribute("aria-hidden", "true");
        const boxA = doc.createElement("span");
        boxA.className = "slt-a";
        const boxB = doc.createElement("span");
        boxB.className = "slt-b";
        thumb.appendChild(boxA);
        thumb.appendChild(boxB);
        row.insertBefore(thumb, row.firstChild || null);
      }
    }

    function switchTab(tabName) {
      for (const btn of doc.querySelectorAll("[data-pages-tab]")) {
        const active = btn.dataset.pagesTab === tabName;
        btn.classList[active ? "add" : "remove"]("active");
        btn.setAttribute("aria-selected", active ? "true" : "false");
      }
      for (const pane of doc.querySelectorAll("[data-pages-pane]")) {
        pane.hidden = pane.dataset.pagesPane !== tabName;
      }
    }

    function bind() {
      if (bound || !doc) return;
      bound = true;
      enhanceSlideLayoutPicker();
      for (const tabBtn of doc.querySelectorAll("[data-pages-tab]")) {
        tabBtn.addEventListener("click", () => switchTab(tabBtn.dataset.pagesTab));
      }
      byId("btn-pages-inspector-close")?.addEventListener("click", () => {
        inspectorOpen = false;
        if (isPagesActive()) sync("DOC");
      });
      doc.querySelector("[data-pages-inspector-toggle]")?.addEventListener("click", () => {
        inspectorOpen = !inspectorOpen;
        if (isPagesActive()) sync("DOC");
      });
      for (const [inspId, srcId] of [
        ["pages-insp-style", "doc-style-select"],
        ["pages-insp-font", "doc-font-select"],
        ["pages-insp-linespacing", "doc-linespacing-select"],
      ]) {
        byId(inspId)?.addEventListener("change", () => {
          const inspEl = byId(inspId);
          const srcEl = byId(srcId);
          if (!inspEl || !srcEl) return;
          srcEl.value = inspEl.value;
          dispatchChange(srcEl);
          syncInspectorValues();
        });
      }
      const inspSize = byId("pages-insp-size");
      inspSize?.addEventListener("change", () => {
        const srcSize = byId("doc-font-size-input");
        if (!srcSize || !inspSize) return;
        srcSize.value = inspSize.value;
        dispatchChange(srcSize);
      });
      for (const btn of doc.querySelectorAll("[data-pages-style-btn]")) {
        btn.addEventListener("click", () => {
          byId("btn-stylebar-" + btn.dataset.pagesStyleBtn)?.click?.();
          syncInspectorValues();
        });
      }
      for (const btn of doc.querySelectorAll("[data-pages-align-btn]")) {
        btn.addEventListener("click", () => {
          byId("btn-align-" + btn.dataset.pagesAlignBtn)?.click?.();
          syncInspectorValues();
        });
      }
      byId("btn-pages-insp-setup")?.addEventListener("click", () => triggerCmd("layout-paper"));
      byId("btn-pages-insp-header")?.addEventListener("click", () => triggerCmd("mod-header"));
      byId("btn-pages-insp-footer")?.addEventListener("click", () => triggerCmd("mod-footer"));
      doc.addEventListener("selectionchange", () => {
        if (isPagesActive()) syncInspectorValues();
      });
      doc.addEventListener("eo-doc-mutated", () => {
        if (isPagesActive()) syncInspectorValues();
      });
    }

    return { bind, sync, syncInspectorValues, enhanceSlideLayoutPicker, setInspectorOpen: (v) => { inspectorOpen = Boolean(v); } };
  }

  host.__EASYOFFICE_CREATE_PAGES_UI__ = createPagesUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
