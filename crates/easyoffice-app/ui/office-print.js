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
  //   - sheet: the used range as a gridded table, landscape when it is wide; a cell with a number format shows the text the
  //     engine's formatter gives (the grid's text, PD-193), asked for before the pages are built because the build is synchronous
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
  const MAX_PRINT_CELLS = 200000;
  const FILL_REPEATS = 300;
  const MSG_UNSUPPORTED = "이 모듈은 인쇄를 지원하지 않습니다.";
  const MSG_EMPTY = "인쇄할 내용이 없습니다.";
  const MSG_OPENING = "인쇄 대화상자를 여는 중…";
  const RULE_EDGE_SIDES = ["t", "r", "b", "l"];
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
    // The display of the formatted cells of the sheet being printed: Map code -> Map(stored value -> { t, c, f } or null).
    let sheetFormats = null;

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
        if (packed <= 0 || (text === "" && !entry?.style?.border && !entry?.style?.fill)) continue;
        const col = packed & COLUMN_BITS;
        const row = (packed - col) / ROW_STRIDE;
        cells.push({ row, col, text, entry });
        range = range
          ? { r1: Math.min(range.r1, row), r2: Math.max(range.r2, row), c1: Math.min(range.c1, col), c2: Math.max(range.c2, col) }
          : { r1: row, r2: row, c1: col, c2: col };
      }
      return { range, cells };
    }

    // The format code of a cell (a text format shows the stored text as typed) and whether a page sets its text at the right.
    const codeOf = (entry) => entry?.style?.numFmt || "";
    const isTextEntry = (entry) => entry?.t === "s" || codeOf(entry) === "@";

    function cellClasses(entry, text) {
      const style = entry.style || {};
      const names = STYLE_CLASSES.filter(([flag]) => style[flag]).map(([, name]) => name);
      if (!isTextEntry(entry) && gridUtils.isNumericText(text)) names.push("eo-print-num");
      return names.join(" ");
    }

    // What a sheet prints: the cells, the boxes (the page setup's print areas or the used range) and the page setup. The page
    // setup of the sheet (`sheet-page.js`) gives the paper, margins, scale, print area and repeated rows and columns; a sheet
    // without one prints its used range on A4, turned when it is wide (`sheet-page-print.js`).
    function sheetPlan() {
      const map = sheetController.snapshotDomSheetCells();
      let { range } = usedRange(map);
      const geometry = sheetController.printGeometry();
      const core = host.__EASYOFFICE_SHEET_PAGE_CORE__;
      const areas = core.areaBoxes(geometry.page);
      const anchored = [...(geometry.charts || []), ...(geometry.images || [])];
      if (areas.length === 0 && anchored.length > 0) {
        for (const obj of anchored) {
          const a = obj?.anchor;
          if (!a) continue;
          const r1 = Math.max(1, (Number(a.fromRow) || 0) + 1);
          const c1 = Math.max(1, (Number(a.fromCol) || 0) + 1);
          const r2 = Math.max(r1, (Number(a.toRow) || 0) + 1);
          const c2 = Math.max(c1, (Number(a.toCol) || 0) + 1);
          range = range
            ? { r1: Math.min(range.r1, r1), c1: Math.min(range.c1, c1), r2: Math.max(range.r2, r2), c2: Math.max(range.c2, c2) }
            : { r1, c1, r2, c2 };
        }
      }
      if (areas.length === 0 && !range) throw new Error(MSG_EMPTY);
      const boxes = areas.length > 0 ? areas : [{ r1: range.r1, c1: range.c1, r2: range.r2, c2: range.c2 }];
      const wide = range !== null && range.c2 - range.c1 + 1 > WIDE_SHEET_COLUMNS;
      return { map, geometry, core, boxes, page: geometry.page ?? (wide ? { landscape: true } : null) };
    }

    // Asks the engine for the display of every formatted cell the pages will hold, so `buildSheet` reads them from memory.
    // A sheet this cannot plan, or an engine that does not answer, prints its cells in the General display.
    async function warmSheetFormats() {
      sheetFormats = null;
      const display = host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__;
      if (getActiveModule() !== "SHEET" || !display?.resolveAll) return;
      let plan;
      try {
        plan = sheetPlan();
      } catch {
        return;
      }
      const wanted = new Map();
      let held = 0;
      for (const ref in plan.map) {
        if (!Object.hasOwn(plan.map, ref)) continue;
        const entry = plan.map[ref];
        const code = codeOf(entry);
        const packed = code && !(entry?.t === "s" && !code.includes("@")) ? gridUtils.packRef(ref) : 0;
        if (packed <= 0 || entry.value == null || entry.value === "") continue;
        const col = packed & COLUMN_BITS;
        const row = (packed - col) / ROW_STRIDE;
        if (!plan.boxes.some((b) => row >= b.r1 && row <= b.r2 && col >= b.c1 && col <= b.c2)) continue;
        if ((held += 1) > MAX_PRINT_CELLS) return;
        const fmtKey = entry?.t === "s" && code !== "@" ? "\u0000" + code : code;
        let values = wanted.get(fmtKey);
        if (!values) wanted.set(fmtKey, (values = new Set()));
        values.add(String(entry.value));
      }
      if (wanted.size > 0) sheetFormats = await display.resolveAll(wanted);
    }

    // A cell whose format has a `*x` fill: the text before it, the repeats (drawn, `eo-print-fill` in `shell.css`), the text after.
    function fillCell(td, text, [index, char]) {
      const holder = document.createElement("span");
      holder.className = "eo-print-fill";
      const part = (value) => {
        const el = document.createElement("span");
        el.textContent = value;
        return el;
      };
      const repeats = document.createElement("i");
      repeats.dataset.fill = char.repeat(FILL_REPEATS);
      for (const child of [part(text.slice(0, index)), repeats, part(text.slice(index))]) holder.appendChild(child);
      td.appendChild(holder);
    }

    function spanSum(sizes, fromIdx, toIdx) {
      let s = 0;
      const lo = Math.max(0, fromIdx);
      const hi = Math.min(sizes.length - 1, toIdx);
      for (let i = lo; i <= hi; i += 1) s += sizes[i];
      return s;
    }

    function appendSheetPrintImages(pages, boxes, geometry, page, core) {
      const images = geometry?.images;
      const pagePrint = host.__EASYOFFICE_SHEET_PAGE_PRINT__;
      const utils = host.__EASYOFFICE_SHEET_UTILS__;
      if (!Array.isArray(images) || images.length === 0 || !pagePrint?.cut || !pagePrint?.scaleOf) return;
      const px = (v) => Math.round(v * 100) / 100 + "px";
      const paper = core.paperMm(page);
      const m = page?.margins;
      const margin = m
        ? { top: m.top * 0.0254, bottom: m.bottom * 0.0254, left: m.left * 0.0254, right: m.right * 0.0254 }
        : { top: 10, bottom: 10, left: 10, right: 10 };
      const pxPerMm = PX_PER_INCH / 25.4;
      const boxW = (paper.width - margin.left - margin.right) * pxPerMm;
      const boxH = (paper.height - margin.top - margin.bottom) * pxPerMm;
      const headW = page?.headings === true ? 36 : 0;
      const headH = page?.headings === true ? 22 : 0;
      const inside = (t, size) => (t && t[0] >= 0 && t[1] < size ? t : null);
      let pageIdx = 0;
      for (const box of boxes) {
        const colSizes = new Float64Array(box.c2 - box.c1 + 1);
        for (let c = box.c1; c <= box.c2; c += 1) colSizes[c - box.c1] = geometry.colHidden(c) ? 0 : geometry.colWidth(c);
        const rowSizes = new Float64Array(box.r2 - box.r1 + 1);
        for (let r = box.r1; r <= box.r2; r += 1) rowSizes[r - box.r1] = geometry.rowHidden(r) ? 0 : geometry.rowTop(r + 1) - geometry.rowTop(r);
        const totalW = colSizes.reduce((a, b) => a + b, 0) + headW;
        const totalH = rowSizes.reduce((a, b) => a + b, 0) + headH;
        const scale = pagePrint.scaleOf(page, totalW, totalH, boxW, boxH);
        const titleRows = inside(page?.titleRows ? [page.titleRows[0] - box.r1, page.titleRows[1] - box.r1] : null, rowSizes.length);
        const titleCols = inside(page?.titleCols ? [page.titleCols[0] - box.c1, page.titleCols[1] - box.c1] : null, colSizes.length);
        const colCut = pagePrint.cut(colSizes, boxW / scale - headW, titleCols);
        const rowCut = pagePrint.cut(rowSizes, boxH / scale - headH, titleRows);
        for (const cg of colCut.groups) {
          for (const rg of rowCut.groups) {
            const holder = pages[pageIdx++]?.querySelector?.(".eo-print-sheet-box");
            if (!holder) continue;
            const tileC1 = box.c1 + cg.first, tileC2 = box.c1 + cg.last;
            const tileR1 = box.r1 + rg.first, tileR2 = box.r1 + rg.last;
            const ownStartX = headW + (cg.title && titleCols ? spanSum(colSizes, titleCols[0], titleCols[1]) : 0);
            const ownStartY = headH + (rg.title && titleRows ? spanSum(rowSizes, titleRows[0], titleRows[1]) : 0);
            for (const item of images) {
              const a = item?.anchor;
              if (!a) continue;
              const c1 = Math.max(1, (Number(a.fromCol) || 0) + 1);
              const r1 = Math.max(1, (Number(a.fromRow) || 0) + 1);
              const c2 = Math.max(c1, (Number(a.toCol) || 0) + 1);
              const r2 = Math.max(r1, (Number(a.toRow) || 0) + 1);
              if (r2 < tileR1 || r1 > tileR2 || c2 < tileC1 || c1 > tileC2) continue;
              const relX = c1 >= tileC1 ? ownStartX + spanSum(colSizes, cg.first, c1 - box.c1 - 1) : ownStartX - spanSum(colSizes, Math.max(0, c1 - box.c1), cg.first - 1);
              const relY = r1 >= tileR1 ? ownStartY + spanSum(rowSizes, rg.first, r1 - box.r1 - 1) : ownStartY - spanSum(rowSizes, Math.max(0, r1 - box.r1), rg.first - 1);
              const subX = Math.round((Number(a.colOffEmu) || 0) / 9525);
              const subY = Math.round((Number(a.rowOffEmu) || 0) / 9525);
              const w = Math.max(1, Number(a.widthPx) || 240);
              const h = Math.max(1, Number(a.heightPx) || 160);
              const imgEl = document.createElement("img");
              imgEl.className = "eo-print-image";
              imgEl.dataset.imageId = item.id || "";
              const url = utils?.docImageUrl?.(item.imageId) || "";
              const b64 = !url && item.imageId ? utils?.getDocImage?.(item.imageId)?.base64 || "" : "";
              if (url) imgEl.setAttribute("src", url);
              else if (b64) imgEl.setAttribute("src", "data:" + (item.mime || "image/png") + ";base64," + b64);
              imgEl.setAttribute("alt", item.alt || "");
              imgEl.style.left = px((relX + subX) * scale);
              imgEl.style.top = px((relY + subY) * scale);
              imgEl.style.width = px(w * scale);
              imgEl.style.height = px(h * scale);
              holder.appendChild(imgEl);
            }
          }
        }
      }
    }

    function buildSheet(rules) {
      const { map, geometry, core, boxes, page } = sheetPlan();
      const defaultFont = host.__EASYOFFICE_SHEET_DEFAULT_FONT__;
      const sheetFamily = defaultFont?.baseFontFamily?.() || "'맑은 고딕', 'Malgun Gothic', 'Apple SD Gothic Neo', 'Noto Sans KR', sans-serif";
      const sheetSize = defaultFont?.cssFontSize?.() || "11pt";
      rules.sheet.insertRule(
        "#eo-print-root .eo-print-sheet-page .eo-print-sheet, .print-preview-pane .eo-print-sheet-page .eo-print-sheet { font-family: " + sheetFamily + "; font-size: " + sheetSize + "; }",
        rules.sheet.cssRules.length
      );
      // What the conditional formats give a cell (PD-289, PD-300): the answer the grid paints from, and the edges of the cell with the
      // rules' laid over. Null where the rules give the cell nothing to print.
      const ruleApi = host.__EASYOFFICE_SHEET_RULES__;
      const ruleOf = (row, col, own) => {
        const fx = ruleApi?.lookAt(geometry.sheetName, row, col);
        const edges = ruleApi?.withEdges(geometry.sheetName, row, col, own?.border);
        const ruledEdges = edges !== own?.border && RULE_EDGE_SIDES.some((side) => edges?.[side]);
        return fx?.bg || fx?.fg || fx?.flags || fx?.bar || fx?.icon || ruledEdges ? { fx, edges } : null;
      };
      const cellAt = (row, col) => {
        const entry = map[gridUtils.refOf(row, col)];
        const text = entry && entry.value != null ? String(entry.value) : "";
        const rule = ruleOf(row, col, entry?.style);
        if (entry && (text !== "" || entry.style)) return { text, entry, rule };
        return rule ? { text: "", entry: {}, rule } : null;
      };
      const renderCell = ({ text, entry, rule }, td) => {
        const code = codeOf(entry);
        const fmtKey = entry?.t === "s" && code !== "@" ? "\u0000" + code : code;
        const shown = code && !(entry?.t === "s" && !code.includes("@")) ? sheetFormats?.get(fmtKey)?.get(text) : null;
        td.className = cellClasses(entry, text);
        if (shown?.f) fillCell(td, shown.t, shown.f);
        else td.textContent = shown ? shown.t : isTextEntry(entry) ? text : gridUtils.formatGeneral(text);
        host.__EASYOFFICE_SHEET_LOOK__?.printStyle(td, entry.style);
        if (rule) {
          host.__EASYOFFICE_SHEET_LOOK__?.printRule(td, rule.fx, rule.edges);
          ruleApi?.printFx(td, rule.fx);
        }
        if (shown?.c) td.style.color = shown.c;
      };
      const pages = host.__EASYOFFICE_SHEET_PAGE_PRINT__.buildPages({
        document, boxes, geometry, page, core, cellAt, rules, pageClass: PAGE_CLASS, renderCell: (td, cell) => renderCell(cell, td),
      });
      appendSheetPrintImages(pages, boxes, geometry, page, core);
      return pages;
    }

    // ---- slides -----------------------------------------------------------------------------

    function slidePage(slide, aspect, paperClass) {
      const page = document.createElement("div");
      page.className = PAGE_CLASS + " " + paperClass;
      const stage = document.createElement("div");
      stage.className = "slide-stage";
      stage.dataset.aspect = aspect;
      stage.dataset.orientation = slide.orientation === "portrait" ? "portrait" : "landscape";
      stage.dataset.layout = slide.layout || "titleAndContent";
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
      if (Array.isArray(slide.elements) && slide.elements.length > 0) {
        const layer = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__?.renderStaticElementsLayer?.(document, slide, aspect, { forPrint: true });
        if (layer) stage.appendChild(layer);
      }
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

    const BUILDERS = {
      DOC: buildDocument,
      SHEET: buildSheet,
      SLIDE: buildSlides,
      BASE: (rules) => host.__EASYOFFICE_BASE_BOARD_HOST__.buildPrintPages("BASE", document, rules),
      BOARD: (rules) => host.__EASYOFFICE_BASE_BOARD_HOST__.buildPrintPages("BOARD", document, rules),
    };

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
      let nodes;
      try {
        nodes = build(rules);
      } finally {
        sheetFormats = null;
      }
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
      await warmSheetFormats();
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

    function createPreviewRuleEnv(baseEnv) {
      if (typeof baseEnv.CSSStyleSheet === "function") return baseEnv;
      return {
        CSSStyleSheet: class PreviewStyleSheet {
          constructor() {
            this.cssRules = [];
          }
          insertRule(text, at = this.cssRules.length) {
            this.cssRules.splice(at, 0, {
              cssText: text,
              style: {
                props: {},
                setProperty(k, v) {
                  this.props[k] = v;
                },
              },
            });
            return at;
          }
        },
      };
    }

    async function buildPreview() {
      const mod = getActiveModule();
      const build = BUILDERS[mod];
      if (!build) throw new Error(MSG_UNSUPPORTED);
      await warmSheetFormats();
      const baseRules = createPageRules(createPreviewRuleEnv(env));
      const paperByClass = new Map();
      const rules = {
        sheet: baseRules.sheet,
        classFor(w, h) {
          const cls = baseRules.classFor(w, h);
          paperByClass.set(cls, { width: Number(w), height: Number(h) });
          return cls;
        },
      };
      let pages;
      try {
        pages = build(rules);
      } finally {
        sheetFormats = null;
      }
      const rulesCss = Array.from(rules.sheet.cssRules, (r) => r.cssText);
      let fallbackPaper = PORTRAIT_MM;
      let flowMargin = DOC_MARGIN_MM + "mm";
      for (const cssText of rulesCss) {
        const m = /^@page\s*\{\s*size:\s*([\d.]+)mm\s+([\d.]+)mm;\s*margin:\s*([^;]+);/.exec(cssText);
        if (m) {
          fallbackPaper = { width: Number(m[1]), height: Number(m[2]) };
          flowMargin = m[3].trim();
        }
      }
      const pageSizesMm = pages.map((page) => {
        const tokens = String(page?.className || "").split(/\s+/);
        for (const tok of tokens) {
          if (paperByClass.has(tok)) return paperByClass.get(tok);
        }
        return fallbackPaper;
      });
      return {
        module: mod,
        pages,
        paperMm: pageSizesMm[0] || fallbackPaper,
        pageSizesMm,
        flowMargin,
        rulesCss,
      };
    }

    const api = {
      prepare,
      print,
      remove,
      buildPreview,
      isSupported: (moduleCode) => Object.hasOwn(BUILDERS, moduleCode),
      getActiveModule,
      slideDeckState,
      sheetController,
      notify,
    };
    host.__EASYOFFICE_OFFICE_PRINT__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_OFFICE_PRINT__ = createOfficePrint;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
