(function (host) {
  "use strict";

  // Format Inspector (`#pages-inspector`), DOC/HWP/Pages UI terminology sync,
  // and visual slide-layout wireframe thumbnails inside `#slide-layout-select`.
  function createPagesUi(deps = {}) {
    const doc = deps.document || host.document;
    let inspectorOpen = true;
    let sidebarKind = "format";
    let bound = false;

    const byId = (id) => doc?.getElementById?.(id) || null;
    // The indent fields (cm, the ruler's unit) of the 레이아웃 pane and the paragraph style each one writes.
    const PX_PER_CM = 96 / 2.54;
    const INDENT_FIELDS = [["pages-insp-indent-first", "textIndent"], ["pages-insp-indent-left", "paddingLeft"]];
    const selectedBlocks = () => {
      const root = byId("doc-editor");
      const range = deps.getDocRuns?.()?.currentRange?.();
      return root && range ? host.__EASYOFFICE_DOC_RUNS__.blocksInRange(root, range) : [];
    };
    // The list kind of a paragraph: "ordered", "bullet", "task" or "" (not a list item).
    const listKindOf = (block) => {
      if (block?.tagName !== "LI") return "";
      if (block.parentNode?.tagName === "OL") return "ordered";
      return block.parentNode?.dataset?.eoList === "task" ? "task" : "bullet";
    };
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
        box.textContent = "상단 3.0 · 하단 2.5 · 왼쪽 2.5 · 오른쪽 2.5 cm";
        return;
      }
      const cm = (tw) => (Math.round(((Number(tw) || 0) / 566.9291) * 10) / 10).toFixed(1);
      box.textContent = `상단 ${cm(setup.top)} · 하단 ${cm(setup.bottom)} · 왼쪽 ${cm(setup.left)} · 오른쪽 ${cm(setup.right)} cm`;
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
      for (const [dstId, srcId] of [["pages-insp-space-before", "doc-space-before-input"], ["pages-insp-space-after", "doc-space-after-input"]]) {
        const src = byId(srcId);
        const dst = byId(dstId);
        if (src && dst && doc.activeElement !== dst) dst.value = src.value;
      }
      const block = selectedBlocks()[0];
      const listSel = byId("pages-insp-list");
      if (listSel && doc.activeElement !== listSel) {
        const kind = listKindOf(block);
        listSel.value = kind === "task" ? "" : kind;
        host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(listSel);
      }
      for (const [id, prop] of INDENT_FIELDS) {
        const el = byId(id);
        if (el && block && doc.activeElement !== el) el.value = String(Math.round((Number.parseInt(block.style[prop] || "0", 10) / PX_PER_CM) * 10) / 10);
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
      btnText(
        '#icon-toolbar [data-studio-cmd="para-shape"]',
        hwp ? "양쪽\n정렬" : isPages ? "좌우\n정렬" : "양쪽\n맞춤",
        hwp ? "양쪽 정렬" : isPages ? "좌우 정렬" : "양쪽 맞춤",
      );
      btnText(
        '#icon-toolbar [data-studio-cmd="indent-in"]',
        hwp ? "한 수준\n증가" : isPages ? "들여쓰기\n증가" : "들여쓰기\n늘림",
        hwp ? "한 수준 증가" : isPages ? "들여쓰기 단계 증가" : "들여쓰기 늘림",
      );
      btnText(
        '#icon-toolbar [data-studio-cmd="indent-out"]',
        hwp ? "한 수준\n감소" : isPages ? "들여쓰기\n감소" : "들여쓰기\n줄임",
        hwp ? "한 수준 감소" : isPages ? "들여쓰기 단계 감소" : "들여쓰기 줄임",
      );
      btnText('#icon-toolbar #btn-toggle-ctrl-marks', hwp ? "조판\n부호" : "서식\n기호", hwp ? "조판 부호 표시" : "서식 기호 표시");
      btnText(
        '#icon-toolbar #btn-toggle-para-marks',
        hwp ? "문단\n부호" : isPages ? "보이지\n않는 요소" : "편집\n기호",
        hwp ? "문단 부호 표시" : isPages ? "보이지 않는 요소 보기" : "편집 기호 표시",
      );
      btnText('#icon-toolbar #btn-toggle-doc-grid', hwp ? "격자\n보기" : "눈금선", hwp ? "격자 보기" : "눈금선 보기");
      btnText('#icon-toolbar [data-studio-cmd="insert-table"]', "표", hwp ? "표 만들기" : "표 삽입");
      btnText('#icon-toolbar [data-studio-cmd="insert-image"]', isPages ? "이미지" : "그림", isPages ? "이미지" : "그림");
      const alignSuffix = hwp || isPages ? "정렬" : "맞춤";
      const justifyWord = hwp ? "양쪽 정렬" : isPages ? "좌우 정렬" : "양쪽 맞춤";
      for (const [id, text] of [
        ["btn-align-left", `왼쪽 ${alignSuffix}`],
        ["btn-align-center", `가운데 ${alignSuffix}`],
        ["btn-align-right", `오른쪽 ${alignSuffix}`],
        ["btn-align-justify", justifyWord],
      ]) {
        const el = byId(id);
        if (!el) continue;
        el.setAttribute("title", text);
        el.setAttribute("aria-label", text);
      }
      for (const [key, title] of [["bold", "볼드체"], ["italic", "이탤릭체"], ["underline", "밑줄체"], ["strike", "취소선"]]) {
        const el = doc.querySelector(`[data-pages-style-btn="${key}"]`);
        if (!el) continue;
        el.setAttribute("title", title);
        el.setAttribute("aria-label", title);
      }
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
      syncSidebarKind();
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

    // 포맷 sidebar: the 스타일 and 레이아웃 buttons at its top each show their own sections.
    function switchTab(tabName) {
      for (const btn of doc.querySelectorAll("[data-pages-tab]")) {
        const active = btn.dataset.pagesTab === tabName;
        btn.classList[active ? "add" : "remove"]("active");
        btn.setAttribute("aria-selected", active ? "true" : "false");
      }
      for (const pane of doc.querySelectorAll("#pages-format-sidebar [data-pages-pane]")) {
        pane.hidden = pane.dataset.pagesPane !== tabName;
      }
    }

    // The toolbar's 포맷 and 문서 buttons each open their own sidebar; the button of the open one closes it.
    function showSidebar(kind) {
      inspectorOpen = !(inspectorOpen && sidebarKind === kind);
      sidebarKind = kind;
      if (isPagesActive()) sync("DOC");
    }

    function syncSidebarKind() {
      const insp = byId("pages-inspector");
      if (insp) insp.dataset.pagesSidebar = sidebarKind;
      for (const pane of doc.querySelectorAll("[data-pages-sidebar-pane]")) pane.hidden = pane.dataset.pagesSidebarPane !== sidebarKind;
      const title = byId("pages-inspector-title");
      if (title) title.textContent = sidebarKind === "document" ? "문서" : "포맷";
      for (const btn of doc.querySelectorAll("[data-pages-sidebar-toggle]")) {
        btn.setAttribute("aria-pressed", inspectorOpen && btn.dataset.pagesSidebarToggle === sidebarKind ? "true" : "false");
      }
    }

    function bind() {
      if (bound || !doc) return;
      bound = true;
      enhanceSlideLayoutPicker();
      for (const tabBtn of doc.querySelectorAll("[data-pages-tab]")) {
        tabBtn.addEventListener("click", () => switchTab(tabBtn.dataset.pagesTab));
      }
      for (const btn of doc.querySelectorAll("[data-pages-sidebar-toggle]")) {
        btn.addEventListener("click", () => showSidebar(btn.dataset.pagesSidebarToggle));
      }
      byId("btn-pages-inspector-close")?.addEventListener("click", () => {
        inspectorOpen = false;
        if (isPagesActive()) sync("DOC");
      });
      doc.querySelector("[data-pages-inspector-toggle]")?.addEventListener("click", () => {
        inspectorOpen = !inspectorOpen;
        if (isPagesActive()) sync("DOC");
      });
      byId("pages-insp-textcolor")?.addEventListener("change", (ev) => {
        deps.getDocRuns?.()?.setColor?.("color", ev.target.value);
        syncInspectorValues();
      });
      // 구분점 및 목록: the list kind the popup names (없음 takes the paragraphs out of their list).
      byId("pages-insp-list")?.addEventListener("change", (ev) => {
        const runs = deps.getDocRuns?.();
        const now = listKindOf(selectedBlocks()[0]);
        const want = ev.target.value;
        if (runs && want !== now) runs.toggleList(want || now);
        syncInspectorValues();
      });
      for (const [id, which] of [["pages-insp-space-before", "before"], ["pages-insp-space-after", "after"]]) {
        byId(id)?.addEventListener("change", (ev) => {
          deps.getDocRuns?.()?.setSpacing?.(which, ev.target.value);
          syncInspectorValues();
        });
      }
      for (const [id, prop] of INDENT_FIELDS) {
        byId(id)?.addEventListener("change", (ev) => {
          const runs = deps.getDocRuns?.();
          const px = Number(ev.target.value) * PX_PER_CM;
          if (runs && Number.isFinite(px)) runs.withUndo(() => ({ changed: runs.setBlockIndent(prop, px) > 0 }));
          syncInspectorValues();
        });
      }
      for (const btn of doc.querySelectorAll("[data-pages-indent-btn]")) {
        btn.addEventListener("click", () => {
          triggerCmd(btn.dataset.pagesIndentBtn);
          syncInspectorValues();
        });
      }
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
