(function (host) {
  "use strict";

  // Printing for the document, sheet and slide modules (the Hangul module prints its own pages,
  // `hwp-print.js`). The approach is the reference app's print dialog (`print-dialog.ts`, MIT): the
  // pages go into a hidden print root of this window, `@page` rules set the paper, the window is
  // printed (the shell's `print_webview` command in the desktop app, `window.print()` in a browser)
  // and the root is removed on `afterprint`. The root and its static rules are the ones the Hangul
  // print uses (`#eo-print-root`, `shell.css`); the page rules go through a constructed stylesheet.
  //   - document: the editor's content flows over A4 pages (the browser breaks the pages)
  //   - sheet: the used range as a gridded table, landscape when it is wide
  //   - slide: one slide per page, in the deck's aspect ratio

  const { ROOT_ID, PAGE_CLASS, createPageRules } = host.__EASYOFFICE_HWP_PRINT_INTERNALS__;
  const FLOW_CLASS = "eo-print-flow";
  const SHEET_CLASS = "eo-print-sheet";
  const PORTRAIT_MM = { width: 210, height: 297 };
  const LANDSCAPE_MM = { width: 297, height: 210 };
  const DOC_MARGIN_MM = 20;
  const SHEET_MARGIN_MM = 10;
  const WIDE_SHEET_COLUMNS = 8;
  // A sheet range past this many cells is refused instead of building the table (fail-closed).
  const MAX_PRINT_CELLS = 200000;
  const SLIDE_PAPER_MM = { "16:9": [297, 167], "4:3": [280, 210], A4: [297, 210] };
  const DEFAULT_ASPECT = "16:9";
  const COLUMN_BITS = 32767;
  const ROW_STRIDE = 32768;
  const AFTERPRINT_FALLBACK_MS = 5 * 60 * 1000;
  const MSG_UNSUPPORTED = "이 모듈은 인쇄를 지원하지 않습니다.";
  const MSG_EMPTY = "인쇄할 내용이 없습니다.";
  const MSG_TOO_BIG = "인쇄 범위가 너무 커서 한 번에 인쇄할 수 없습니다.";
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

    function addPageRule(rules, size, marginMm) {
      rules.sheet.insertRule("@page { size: " + size.width + "mm " + size.height + "mm; margin: " + marginMm + "mm; }", rules.sheet.cssRules.length);
    }

    // ---- document ---------------------------------------------------------------------------

    // The editor's content, without ids (they would repeat the live editor's) and without editing state.
    function cloneEditorContent(editor) {
      const flow = document.createElement("div");
      flow.className = FLOW_CLASS;
      for (const child of editor.childNodes) flow.appendChild(child.cloneNode(true));
      for (const el of flow.querySelectorAll("[id], [contenteditable]")) {
        el.removeAttribute("id");
        el.removeAttribute("contenteditable");
      }
      for (const el of flow.querySelectorAll("script, .doc-table-toolbar")) el.remove();
      return flow;
    }

    // The editor's text face carries over to the flow through the stylesheet (no style attribute).
    function addTypeface(rules, editor) {
      const computed = env.getComputedStyle?.(editor);
      if (!computed) return;
      rules.sheet.insertRule("#" + ROOT_ID + " ." + FLOW_CLASS + " {}", rules.sheet.cssRules.length);
      const style = rules.sheet.cssRules[rules.sheet.cssRules.length - 1].style;
      for (const prop of ["font-family", "font-size", "line-height", "text-align"]) {
        const value = computed.getPropertyValue(prop);
        if (value) style.setProperty(prop, value);
      }
    }

    function buildDocument(rules) {
      const editor = document.getElementById("doc-editor");
      if (!editor) throw new Error(MSG_UNSUPPORTED);
      addPageRule(rules, PORTRAIT_MM, DOC_MARGIN_MM);
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
        if (packed <= 0 || text === "") continue;
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

    function buildSheet(rules) {
      const { range, cells } = usedRange(sheetController.snapshotDomSheetCells());
      if (!range) throw new Error(MSG_EMPTY);
      const rows = range.r2 - range.r1 + 1;
      const cols = range.c2 - range.c1 + 1;
      if (rows * cols > MAX_PRINT_CELLS) throw new Error(MSG_TOO_BIG);
      const table = document.createElement("table");
      table.className = SHEET_CLASS;
      const body = document.createElement("tbody");
      const tds = [];
      for (let r = 0; r < rows; r += 1) {
        const tr = document.createElement("tr");
        for (let c = 0; c < cols; c += 1) {
          const td = document.createElement("td");
          tds.push(td);
          tr.appendChild(td);
        }
        body.appendChild(tr);
      }
      for (const { row, col, text, entry } of cells) {
        const td = tds[(row - range.r1) * cols + (col - range.c1)];
        td.textContent = gridUtils.formatGeneral(text);
        td.className = cellClasses(entry, text);
      }
      table.appendChild(body);
      addPageRule(rules, cols > WIDE_SHEET_COLUMNS ? LANDSCAPE_MM : PORTRAIT_MM, SHEET_MARGIN_MM);
      return [table];
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
