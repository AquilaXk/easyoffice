(function (host) {
  "use strict";

  // HWP printing from the shell window (PD-104). The embedded editor cannot print on its own (its
  // frame is sandboxed without popups), so the shell asks it for the pages in small batches
  // (`getPrintPages`), puts each sanitized page SVG into a hidden root of this document and prints
  // the window. The approach follows the reference app's print dialog (all pages in a hidden root, one @page size
  // per paper size, a progress text), with a cancel button, a batch-by-batch fetch that never holds
  // every SVG string at once, and a hard budget for the text kept in the page. The window print
  // dialog is the shell's `print_webview` command (PD-138): `window.print()` does nothing in the
  // macOS webview, so the desktop shell prints the webview natively, as the reference app does; only
  // `--serve` in a browser uses the Web `print()`. Every rule that depends on the pages goes through a constructed stylesheet
  // (no inline style element, no style attribute).

  const CAPABILITY = "print-pages-v1";
  const ROOT_ID = "eo-print-root";
  const PROGRESS_ID = "eo-print-progress";
  const PAGE_CLASS = "eo-print-page";
  const BATCH_PAGES = 10;
  const BATCH_TIMEOUT_MS = 60000;
  // Total SVG text kept for one job (UTF-16 code units, about twice that in bytes).
  const MAX_SVG_CHARS = 96 * 1024 * 1024;
  const MIN_PAPER_MM = 10;
  const MAX_PAPER_MM = 2000;
  const MM_DECIMALS = 3;
  // The page box is a hair shorter than the paper so rounding never spills a blank page.
  const PAGE_HEIGHT_GUARD_MM = 0.1;
  const AFTERPRINT_FALLBACK_MS = 5 * 60 * 1000;
  const ID_REFERENCE = /url\((['"]?)#([^)'"\s]+)\1\)/g;
  const SVG_NS = "http://www.w3.org/2000/svg";
  // What the editor's page renderer writes, and nothing else. An element or attribute outside the
  // lists is removed: the page SVG comes from a frame the shell does not trust, and what is kept goes
  // into the shell's own document, where a script, a style, a link or a reference to another
  // document must never come from. `a` is unwrapped (its text stays). `style` and `class` have no
  // use here: the page's policy does not allow inline styles.
  const KEPT_ELEMENTS = new Set([
    "svg", "g", "defs", "title", "desc", "rect", "line", "polyline", "polygon", "circle", "ellipse", "path",
    "text", "tspan", "image", "clipPath", "marker", "pattern", "linearGradient", "radialGradient", "stop",
    "filter", "feColorMatrix", "feComponentTransfer", "feFuncR", "feFuncG", "feFuncB",
  ]);
  const KEPT_ATTRIBUTES = new Set([
    "id", "x", "y", "x1", "y1", "x2", "y2", "cx", "cy", "r", "rx", "ry", "fx", "fy", "width", "height", "d", "points",
    "transform", "viewBox", "preserveAspectRatio", "version", "overflow", "opacity", "fill", "fill-opacity", "fill-rule",
    "stroke", "stroke-width", "stroke-opacity", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap",
    "stroke-linejoin", "stroke-miterlimit", "font-size", "font-family", "font-weight", "font-style", "text-anchor",
    "dominant-baseline", "text-decoration", "letter-spacing", "word-spacing", "textLength", "lengthAdjust", "xml:space",
    "clip-path", "clip-rule", "filter", "marker-start", "marker-mid", "marker-end", "markerWidth", "markerHeight",
    "markerUnits", "refX", "refY", "orient", "offset", "stop-color", "stop-opacity", "gradientUnits", "gradientTransform",
    "patternUnits", "patternContentUnits", "patternTransform", "type", "values", "tableValues", "slope", "intercept",
    "amplitude", "exponent", "href", "xlink:href",
  ]);
  const LINK_ATTRIBUTES = new Set(["href", "xlink:href"]);
  const SAFE_IMAGE_PREFIXES = [
    "data:image/png;", "data:image/jpeg;", "data:image/jpg;", "data:image/gif;",
    "data:image/webp;", "data:image/bmp;", "data:image/svg+xml;",
  ];
  const DOCUMENT_TYPE = /<!(?:DOCTYPE|ENTITY)/i;
  // Text with its white space and control characters taken out, as a URL parser reads it: the
  // browser ignores tabs and line breaks inside a scheme (`java<TAB>script:`).
  const compact = (value) => String(value).replace(/[\u0000-\u0020\u007f-\u009f]+/g, "").toLowerCase();
  const MSG_UNSUPPORTED = "이 편집기 구성 요소는 인쇄를 지원하지 않습니다.";
  const MSG_BUSY = "이미 인쇄를 준비하고 있습니다.";
  const MSG_EMPTY = "인쇄할 쪽이 없습니다.";
  const MSG_TOO_BIG = "문서가 커서 한 번에 인쇄할 수 없습니다.";
  const MSG_CANCELLED = "인쇄를 취소했습니다.";
  const MSG_OPENING = "인쇄 대화상자를 여는 중…";
  const MSG_NOT_PREPARED = "인쇄 준비가 끝나지 않았습니다.";
  const MSG_BAD_PAGE = "편집기가 올바르지 않은 쪽을 보냈습니다.";

  function progressText(current, total) {
    return "인쇄 준비 중… (" + current + "/" + total + ")";
  }

  function formatMm(mm) {
    return String(Number(mm.toFixed(MM_DECIMALS)));
  }

  function isValidPaperMm(value) {
    return Number.isFinite(value) && value >= MIN_PAPER_MM && value <= MAX_PAPER_MM;
  }

  // ---- SVG sanitizing (adapted from the reference app's print dialog) ---------------------------------------

  function isSafeReference(value) {
    const text = compact(value);
    return text === "" || text.startsWith("#") || SAFE_IMAGE_PREFIXES.some((prefix) => text.startsWith(prefix));
  }

  // A value may point only inside the page: `url(#id)`; a scheme or another URL is refused.
  function isSafeValue(value) {
    const text = compact(value);
    // A backslash is a CSS escape (`\75rl(` is `url(`): none is read, the value is refused.
    if (text.includes("\\")) return false;
    return !/(?:javascript|vbscript|data|https?|file|blob):/.test(text) && !/url\((?!['"]?#)/.test(text);
  }

  function shouldDropAttribute(name, value) {
    if (!KEPT_ATTRIBUTES.has(name)) return true;
    return LINK_ATTRIBUTES.has(name) ? !isSafeReference(value) : !isSafeValue(value);
  }

  function isKeptElement(element) {
    return element.namespaceURI === SVG_NS && KEPT_ELEMENTS.has(element.localName);
  }

  function sanitizeSvg(root) {
    for (const element of Array.from(root.querySelectorAll("*"))) {
      if (isKeptElement(element)) continue;
      if (element.namespaceURI === SVG_NS && element.localName === "a") element.replaceWith(...element.childNodes);
      else element.remove();
    }
    for (const element of [root, ...Array.from(root.querySelectorAll("*"))]) {
      for (const attribute of Array.from(element.attributes)) {
        if (shouldDropAttribute(attribute.name, attribute.value)) element.removeAttribute(attribute.name);
      }
    }
  }

  // Every page is its own SVG in one document, so the ids that gradients, clip paths and patterns
  // define must not collide between pages.
  function namespaceIds(root, namespace) {
    const elements = [root, ...Array.from(root.querySelectorAll("[id]"))].filter((el) => el.hasAttribute("id"));
    if (elements.length === 0) return;
    const known = new Set(elements.map((el) => el.getAttribute("id")));
    const renamed = (id) => (known.has(id) ? namespace + "-" + id : id);
    for (const element of [root, ...Array.from(root.querySelectorAll("*"))]) {
      for (const attribute of Array.from(element.attributes)) {
        const next = rewriteAttribute(attribute, renamed);
        if (next !== attribute.value) element.setAttribute(attribute.name, next);
      }
    }
  }

  function rewriteAttribute(attribute, renamed) {
    if (attribute.name === "id") return renamed(attribute.value);
    if (!attribute.value.includes("#")) return attribute.value;
    if (LINK_ATTRIBUTES.has(attribute.name.toLowerCase()) && attribute.value.startsWith("#")) {
      return "#" + renamed(attribute.value.slice(1));
    }
    return attribute.value.replaceAll(ID_REFERENCE, (_, quote, id) => "url(#" + renamed(id) + ")");
  }

  // ---- per-size page rules --------------------------------------------------------------------

  // One @page rule and one page-box rule per distinct paper size, added as pages arrive. The first
  // size is also the unnamed default so an engine without named pages still prints the right paper.
  function createPageRules(env) {
    const sheet = new env.CSSStyleSheet();
    const sizes = new Map();
    return {
      sheet,
      classFor(widthMm, heightMm) {
        const key = formatMm(widthMm) + "x" + formatMm(heightMm);
        let entry = sizes.get(key);
        if (!entry) {
          entry = { name: "eo-paper-" + sizes.size, width: formatMm(widthMm), height: formatMm(heightMm) };
          if (sizes.size === 0) sheet.insertRule("@page { size: " + entry.width + "mm " + entry.height + "mm; margin: 0; }", sheet.cssRules.length);
          sheet.insertRule("@page " + entry.name + " { size: " + entry.width + "mm " + entry.height + "mm; margin: 0; }", sheet.cssRules.length);
          sheet.insertRule("." + entry.name + " { page: " + entry.name + "; width: " + entry.width + "mm; height: calc(" + entry.height + "mm - " + PAGE_HEIGHT_GUARD_MM + "mm); }", sheet.cssRules.length);
          sizes.set(key, entry);
        }
        return entry.name;
      },
    };
  }

  // ---- the job --------------------------------------------------------------------------------

  // Checks one getPrintPages reply against the batch size, the page count seen so far and the paper limits.
  function readBatch(result, expectedFrom, total) {
    const pages = Array.isArray(result?.pages) ? result.pages : [];
    const pageCount = Number(result?.pageCount);
    if (!Number.isSafeInteger(pageCount) || pageCount < 0 || (total !== null && pageCount !== total)) {
      throw new Error(MSG_BAD_PAGE);
    }
    if (pageCount > 0 && (pages.length === 0 || pages.length > BATCH_PAGES)) throw new Error(MSG_BAD_PAGE);
    for (const page of pages) {
      if (typeof page?.svg !== "string" || !isValidPaperMm(page.widthMm) || !isValidPaperMm(page.heightMm)) {
        throw new Error(MSG_BAD_PAGE);
      }
    }
    return { pages, pageCount, nextFrom: expectedFrom + pages.length };
  }

  function createHwpPrint(deps) {
    const { document, hwpHost } = deps;
    const env = deps.env || host;
    const notify = typeof deps.notify === "function" ? deps.notify : () => {};
    const bridge = deps.desktopBridge;
    const printWindow = deps.print || (() => (bridge?.isAvailable() ? bridge.windowCommand("printWebview") : env.print()));
    let job = null;
    let printed = null;

    // A document type declaration is refused before the text is parsed (entities would expand
    // into the tree); the root must be an `svg` of the SVG namespace.
    function parseSvg(text) {
      if (DOCUMENT_TYPE.test(text)) return null;
      const parsed = new env.DOMParser().parseFromString(text, "image/svg+xml");
      const svg = parsed.documentElement;
      if (parsed.querySelector("parsererror") || !svg || svg.namespaceURI !== SVG_NS || svg.localName !== "svg") return null;
      return svg;
    }

    function newPageElement(svgText, index, rules, size) {
      const svg = parseSvg(svgText);
      if (!svg) throw new Error(MSG_BAD_PAGE);
      sanitizeSvg(svg);
      namespaceIds(svg, "eo-p" + index);
      const page = document.createElement("div");
      page.className = PAGE_CLASS + " " + rules.classFor(size.widthMm, size.heightMm);
      page.appendChild(document.importNode(svg, true));
      return page;
    }

    function removePrintDocument() {
      document.getElementById(ROOT_ID)?.remove();
      if (printed?.sheet) {
        document.adoptedStyleSheets = document.adoptedStyleSheets.filter((sheet) => sheet !== printed.sheet);
      }
      printed = null;
    }

    function showProgress(current) {
      let box = document.getElementById(PROGRESS_ID);
      if (!box) {
        box = document.createElement("div");
        box.id = PROGRESS_ID;
        box.className = "eo-print-progress";
        box.setAttribute("role", "status");
        const text = document.createElement("span");
        text.className = "eo-print-progress-text";
        const cancel = document.createElement("button");
        cancel.type = "button";
        cancel.className = "eo-print-progress-cancel";
        cancel.textContent = "취소";
        cancel.addEventListener("click", () => cancelPrint());
        box.append(text, cancel);
        document.body.appendChild(box);
      }
      box.querySelector(".eo-print-progress-text").textContent = current;
    }

    function hideProgress() {
      document.getElementById(PROGRESS_ID)?.remove();
    }

    function onKeydown(event) {
      if (event.key === "Escape") cancelPrint();
    }

    function cancelPrint() {
      if (job) job.cancelled = true;
    }

    function startDocument() {
      removePrintDocument();
      const root = document.createElement("div");
      root.id = ROOT_ID;
      root.setAttribute("aria-hidden", "true");
      const rules = createPageRules(env);
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, rules.sheet];
      document.body.appendChild(root);
      printed = { root, sheet: rules.sheet, rules };
      return printed;
    }

    // Batches are fetched strictly one after another (one batch of page SVG in memory at a time), so
    // each step awaits the previous one and schedules the next.
    function appendBatch(state, batch, from, chars) {
      let total = chars;
      for (const [offset, page] of batch.pages.entries()) {
        total += page.svg.length;
        if (total > MAX_SVG_CHARS) throw new Error(MSG_TOO_BIG);
        state.root.appendChild(newPageElement(page.svg, from + offset, state.rules, page));
      }
      return total;
    }

    async function fetchPagesFrom(state, from, total, chars) {
      if (job.cancelled) return false;
      if (total !== null && from >= total) return true;
      const result = await hwpHost.call("getPrintPages", { from, count: BATCH_PAGES }, { timeoutMs: BATCH_TIMEOUT_MS });
      const batch = readBatch(result, from, total);
      if (batch.pageCount === 0) throw new Error(MSG_EMPTY);
      const used = appendBatch(state, batch, from, chars);
      showProgress(progressText(Math.min(batch.nextFrom, batch.pageCount), batch.pageCount));
      await nextTask();
      return fetchPagesFrom(state, batch.nextFrom, batch.pageCount, used);
    }

    function fetchPages(state) {
      return fetchPagesFrom(state, 0, null, 0);
    }

    function nextTask() {
      return new Promise((resolve) => env.setTimeout(resolve, 0));
    }

    function nextFrame() {
      return new Promise((resolve) => env.requestAnimationFrame(() => resolve()));
    }

    // The pages stay in the document until the print dialog is done (`afterprint`), with a late
    // cleanup in case the webview never reports it. The native command returns when the dialog is
    // requested, not when it is done, so the pages must outlive its answer.
    function scheduleCleanup() {
      let timer = null;
      const cleanup = () => {
        env.removeEventListener("afterprint", cleanup);
        if (timer !== null) env.clearTimeout(timer);
        removePrintDocument();
      };
      env.addEventListener("afterprint", cleanup, { once: true });
      timer = env.setTimeout(cleanup, AFTERPRINT_FALLBACK_MS);
      return cleanup;
    }

    async function printPrepared() {
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

    // `--smoke` step `print`: everything up to the window print call. The pages must be in the hidden
    // root and the page rules in the constructed sheet; then the document is removed again, so the
    // print dialog never opens.
    async function inspectPrepared() {
      const pages = printed.root.children.length;
      const rules = printed.sheet.cssRules.length;
      removePrintDocument();
      if (pages === 0 || rules === 0) throw new Error("인쇄 준비에 쪽이나 쪽 규칙이 없습니다.");
      return "pages=" + pages + " rules=" + rules;
    }

    // Fetches every page into the hidden root, then hands over to `finish`. Resolves with what
    // `finish` returns, or `false` when another job runs or this one was cancelled.
    async function runJob(finish) {
      if (job) {
        notify(MSG_BUSY);
        return false;
      }
      if (!hwpHost.getCapabilities().includes(CAPABILITY)) throw new Error(MSG_UNSUPPORTED);
      job = { cancelled: false };
      document.addEventListener("keydown", onKeydown);
      showProgress(progressText(0, 0));
      try {
        const done = await fetchPages(startDocument());
        if (!done || job.cancelled) {
          removePrintDocument();
          notify(MSG_CANCELLED);
          return false;
        }
        hideProgress();
        return await finish();
      } catch (err) {
        removePrintDocument();
        throw err;
      } finally {
        document.removeEventListener("keydown", onKeydown);
        hideProgress();
        job = null;
      }
    }

    const print = async () => (await runJob(printPrepared)) === true;
    const smoke = async () => {
      const detail = await runJob(inspectPrepared);
      if (detail === false) throw new Error(MSG_NOT_PREPARED);
      return detail;
    };

    return { print, smoke, cancel: cancelPrint, isBusy: () => job !== null };
  }

  host.__EASYOFFICE_CREATE_HWP_PRINT__ = createHwpPrint;
  host.__EASYOFFICE_HWP_PRINT_INTERNALS__ = {
    CAPABILITY, ROOT_ID, PROGRESS_ID, PAGE_CLASS, BATCH_PAGES, MAX_SVG_CHARS,
    sanitizeSvg, namespaceIds, createPageRules, progressText, isValidPaperMm,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
