(function (host) {
  "use strict";

  // Printing for the document, sheet and slide modules (the Hangul module prints its own pages,
  // `hwp-print.js`). The approach is the reference app's print dialog (`print-dialog.ts`, MIT): the
  // pages go into a hidden print root of this window, `@page` rules set the paper, the window is
  // printed (the shell's `print_webview` command in the desktop app, `window.print()` in a browser)
  // and the root is removed on `afterprint`. The root and its static rules are the ones the Hangul
  // print uses (`#eo-print-root`, `shell.css`); the page rules go through a constructed stylesheet.
  //   - document: the pages the screen shows, each with its header, footer and page number (`doc-page-print.js`); an
  //     editor without a page layout flows over its paper (the browser breaks the pages)
  //   - sheet: the used range as a gridded table, landscape when it is wide
  //   - slide: one slide per page, in the deck's aspect ratio

  const { ROOT_ID, PAGE_CLASS, createPageRules } = host.__EASYOFFICE_HWP_PRINT_INTERNALS__;
  const FLOW_CLASS = "eo-print-flow";
  const PORTRAIT_MM = { width: 210, height: 297 };
  const DOC_MARGIN_MM = 20;
  const PX_PER_INCH = 96;
  const WIDE_SHEET_COLUMNS = 8;
  const SLIDE_PAPER_MM = { "16:9": [297, 167], "4:3": [280, 210], A4: [297, 210] };
  const DEFAULT_ASPECT = "16:9";
  const COLUMN_BITS = 32767;
  const ROW_STRIDE = 32768;
  const AFTERPRINT_FALLBACK_MS = 5 * 60 * 1000;
  const MSG_UNSUPPORTED = "이 모듈은 인쇄를 지원하지 않습니다.";
  const MSG_EMPTY = "인쇄할 내용이 없습니다.";
  const MSG_OPENING = "인쇄 대화상자를 여는 중…";
  const STYLE_CLASSES = [["bold", "eo-print-b"], ["italic", "eo-print-i"], ["underline", "eo-print-u"], ["strike", "eo-print-s"]];
  const TITLE_STYLE_PROPS = ["fontWeight", "fontStyle", "textDecoration", "color", "fontSize"];

  function createOfficePrint(deps) {
    const { document, getActiveModule, slideDeckState, sheetController, restoreDomChildren } = deps;
    const env = deps.env || host;
    const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
    const notify = typeof deps.notify === "function" ? deps.notify : () => {};
    const bridge = deps.desktopBridge;
    const printWindow = deps.print || (() => (bridge?.isAvailable() ? bridge.windowCommand("printWebview") : env.print()));
    let printed = null;

    function addPageRule(rules, size, margin) {
      rules.sheet.insertRule("@page { size: " + size.width + "mm " + size.height + "mm; margin: " + margin + "; }", rules.sheet.cssRules.length);
    }

    // The paper and margins the screen draws its pages with (`doc-pages-core.js`), so the browser breaks the same lines
    // onto the same pages; A4 with 20 mm margins when the window gives no padding.
    function documentPage(editor) {
      const cs = env.getComputedStyle?.(editor);
      const pads = ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"].map((key) => Number.parseFloat(cs?.[key]));
      const pages = host.__EASYOFFICE_DOC_PAGES_CORE__;
      if (!pages || !pads.every(Number.isFinite)) return { size: PORTRAIT_MM, margin: DOC_MARGIN_MM + "mm" };
      const [top, right, bottom, left] = pads;
      const geo = pages.pageGeometry({ ...pages.paperOf(editor), padding: { top, right, bottom, left } });
      const mm = (px) => Math.round((px * 25.4 * 100) / PX_PER_INCH) / 100;
      return { size: { width: mm(geo.width), height: mm(geo.height) }, margin: pads.map((px) => mm(px) + "mm").join(" ") };
    }

    // ---- document ---------------------------------------------------------------------------

    // The editor's content, without ids (they would repeat the live editor's) and without editing state.
    function cloneEditorContent(editor) {
      const flow = document.createElement("div");
      flow.className = FLOW_CLASS;
      for (const child of editor.childNodes) {
        if (!child.classList?.contains("doc-page-layer") && child.dataset?.eoHf === undefined) flow.appendChild(child.cloneNode(true));
      }
      for (const el of flow.querySelectorAll("[id], [contenteditable]")) {
        el.removeAttribute("id");
        el.removeAttribute("contenteditable");
      }
      for (const el of flow.querySelectorAll("script, .doc-table-toolbar")) el.remove();
      return flow;
    }

    // The editor's line height is a factor of each block's own font size (a heading's lines are taller than the body's), but
    // the computed value is in px; the factor keeps the flow's headings the height the screen gives them.
    function lineHeightFactor(lineHeight, fontSize) {
      const lh = Number.parseFloat(lineHeight);
      const fs = Number.parseFloat(fontSize);
      return String(lineHeight).endsWith("px") && lh > 0 && fs > 0 ? String(Math.round((lh / fs) * 10000) / 10000) : lineHeight;
    }

    // The editor's text face carries over to the flow through the stylesheet (no style attribute).
    function addTypeface(rules, editor) {
      const computed = env.getComputedStyle?.(editor);
      if (!computed) return;
      rules.sheet.insertRule("#" + ROOT_ID + " ." + FLOW_CLASS + " {}", rules.sheet.cssRules.length);
      const style = rules.sheet.cssRules[rules.sheet.cssRules.length - 1].style;
      for (const prop of ["font-family", "font-size", "line-height", "text-align"]) {
        const value = computed.getPropertyValue(prop);
        if (value) style.setProperty(prop, prop === "line-height" ? lineHeightFactor(value, computed.getPropertyValue("font-size")) : value);
      }
    }

    // The page layout is brought up to date (twice: the header and footer's room, which the first pass sets, moves the body
    // on the second), then the pages are drawn from it.
    function buildLaidOutPages(editor, rules) {
      const layout = host.__EASYOFFICE_DOC_PAGES__;
      if (!layout?.isActive?.()) return null;
      layout.layoutNow();
      layout.layoutNow();
      return host.__EASYOFFICE_DOC_PAGE_PRINT__.buildPages({
        document, editor, plan: layout.printPlan(), frame: host.__EASYOFFICE_DOC_PAGE_FRAME__, rules, pageClass: PAGE_CLASS,
      });
    }

    function buildDocument(rules) {
      const editor = document.getElementById("doc-editor");
      if (!editor) throw new Error(MSG_UNSUPPORTED);
      const pages = buildLaidOutPages(editor, rules);
      if (pages) {
        addTypeface(rules, editor);
        return pages;
      }
      const page = documentPage(editor);
      addPageRule(rules, page.size, page.margin);
      addTypeface(rules, editor);
      return [cloneEditorContent(editor)];
    }

    // ---- sheet ------------------------------------------------------------------------------

    function usedRange(map) {
      let range = null;
      const cells = [];
      for (const ref of Object.keys(map)) {
        const entry = map[ref];
        const packed = entry ? gridUtils.packRef(ref) : 0;
        const text = entry && entry.value != null ? String(entry.value) : "";
        if (packed <= 0 || (text === "" && !entry?.style?.border)) continue;
        const col = packed & COLUMN_BITS;
        const row = (packed - col) / ROW_STRIDE;
        cells.push({ row, col, text, entry });
        range = range
          ? { r1: Math.min(range.r1, row), r2: Math.max(range.r2, row), c1: Math.min(range.c1, col), c2: Math.max(range.c2, col) }
          : { r1: row, r2: row, c1: col, c2: col };
      }
      return { range, cells };
    }

    function cellClasses(entry, text) {
      const style = entry.style || {};
      const names = STYLE_CLASSES.filter(([flag]) => style[flag]).map(([, name]) => name);
      if (gridUtils.isNumericText(text)) names.push("eo-print-num");
      return names.join(" ");
    }

    // The page setup of the sheet (`sheet-page.js`) gives the paper, margins, scale, print area and repeated rows and columns;
    // a sheet without one prints its used range on A4, turned when it is wide (`sheet-page-print.js`).
    function buildSheet(rules) {
      const map = sheetController.snapshotDomSheetCells();
      const { range } = usedRange(map);
      const geometry = sheetController.printGeometry();
      const core = host.__EASYOFFICE_SHEET_PAGE_CORE__;
      const areas = core.areaBoxes(geometry.page);
      if (areas.length === 0 && !range) throw new Error(MSG_EMPTY);
      const boxes = areas.length > 0 ? areas : [{ r1: range.r1, c1: range.c1, r2: range.r2, c2: range.c2 }];
      const wide = range !== null && range.c2 - range.c1 + 1 > WIDE_SHEET_COLUMNS;
      const page = geometry.page ?? (wide ? { landscape: true } : null);
      const cellAt = (row, col) => {
        const entry = map[gridUtils.refOf(row, col)];
        const text = entry && entry.value != null ? String(entry.value) : "";
        return entry && (text !== "" || entry.style) ? { text, entry } : null;
      };
      const renderCell = ({ text, entry }, td) => {
        td.textContent = gridUtils.formatGeneral(text);
        td.className = cellClasses(entry, text);
        host.__EASYOFFICE_SHEET_LOOK__?.printStyle(td, entry.style);
      };
      return host.__EASYOFFICE_SHEET_PAGE_PRINT__.buildPages({
        document, boxes, geometry, page, core, cellAt, rules, pageClass: PAGE_CLASS, renderCell: (td, cell) => renderCell(cell, td),
      });
    }

    // ---- slides -----------------------------------------------------------------------------

    function slidePage(slide, aspect, paperClass) {
      const page = document.createElement("div");
      page.className = PAGE_CLASS + " " + paperClass;
      const stage = document.createElement("div");
      stage.className = "slide-stage";
      stage.dataset.aspect = aspect;
      const titleZone = document.createElement("div");
      titleZone.className = "slide-title-zone";
      const heading = document.createElement("h2");
      heading.textContent = slide.title || "";
      for (const prop of TITLE_STYLE_PROPS) if (slide.titleStyle?.[prop]) titleZone.style[prop] = slide.titleStyle[prop];
      titleZone.appendChild(heading);
      const lowerZone = document.createElement("div");
      lowerZone.className = "slide-lower-zone";
      const text = document.createElement("p");
      text.textContent = slide.lower || "";
      lowerZone.appendChild(text);
      if (slide.lowerColumnCount) lowerZone.style.columnCount = slide.lowerColumnCount;
      if (Array.isArray(slide.insertedNodes) && slide.insertedNodes.length > 0) restoreDomChildren(lowerZone, slide.insertedNodes, document);
      stage.appendChild(titleZone);
      stage.appendChild(lowerZone);
      page.appendChild(stage);
      return page;
    }

    function buildSlides(rules) {
      if (slideDeckState.length === 0) throw new Error(MSG_EMPTY);
      const aspect = Object.hasOwn(SLIDE_PAPER_MM, slideDeckState[0].aspect) ? slideDeckState[0].aspect : DEFAULT_ASPECT;
      const [widthMm, heightMm] = SLIDE_PAPER_MM[aspect];
      const paperClass = rules.classFor(widthMm, heightMm);
      return slideDeckState.map((slide) => slidePage(slide, aspect, paperClass));
    }

    const BUILDERS = { DOC: buildDocument, SHEET: buildSheet, SLIDE: buildSlides };

    // ---- the job ----------------------------------------------------------------------------

    function remove() {
      document.getElementById(ROOT_ID)?.remove();
      if (printed) document.adoptedStyleSheets = document.adoptedStyleSheets.filter((sheet) => sheet !== printed.sheet);
      printed = null;
    }

    // Builds the print root for the active module and puts it in the window; the caller prints and removes it.
    function prepare() {
      const build = BUILDERS[getActiveModule()];
      if (!build) throw new Error(MSG_UNSUPPORTED);
      remove();
      const rules = createPageRules(env);
      const nodes = build(rules);
      const root = document.createElement("div");
      root.id = ROOT_ID;
      root.setAttribute("aria-hidden", "true");
      for (const node of nodes) root.appendChild(node);
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, rules.sheet];
      document.body.appendChild(root);
      printed = { root, sheet: rules.sheet };
      return printed;
    }

    const nextFrame = () => new Promise((resolve) => env.requestAnimationFrame(() => resolve()));

    // The root stays until `afterprint` (the native command returns when the dialog is requested, not when it ends).
    function scheduleCleanup() {
      let timer = null;
      const cleanup = () => {
        env.removeEventListener("afterprint", cleanup);
        if (timer !== null) env.clearTimeout(timer);
        remove();
      };
      env.addEventListener("afterprint", cleanup, { once: true });
      timer = env.setTimeout(cleanup, AFTERPRINT_FALLBACK_MS);
      return cleanup;
    }

    async function print() {
      prepare();
      notify(MSG_OPENING);
      await nextFrame();
      const cleanup = scheduleCleanup();
      try {
        await printWindow();
      } catch (err) {
        cleanup();
        throw err;
      }
      return true;
    }

    return { prepare, print, remove, isSupported: (moduleCode) => Object.hasOwn(BUILDERS, moduleCode) };
  }

  host.__EASYOFFICE_CREATE_OFFICE_PRINT__ = createOfficePrint;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
