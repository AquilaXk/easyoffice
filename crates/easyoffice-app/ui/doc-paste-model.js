(function (host) {
  "use strict";

  // Reads pasted HTML (untrusted) into a small model of text blocks, tables and pictures, and nothing else: the page never
  // receives a node of the pasted document. The HTML is parsed by the browser's `DOMParser` into an inert document (no
  // script runs, no picture or style sheet is fetched), and the model is built by walking that tree: tag names pick the
  // block kind, text is the only content that is copied, and every attribute that is read is read through a validator
  // (`doc-runs.js` for character formats, `doc-link.js` for addresses, the picture header check of `doc-image.js`).
  // Scripts, event attributes, external pictures (`http`, `file`, `blob`, and every other `src` that is not a PNG or JPEG
  // `data:` address) and everything outside the whitelist are dropped. A paste that goes over a limit is refused as a whole
  // (fail-closed) and the caller pastes the plain text instead.
  //
  // The reference (rhwp-studio, `engine/input-handler-keyboard.ts` `pasteExternalHtml`, lines 2130-2165) hands the cleaned
  // HTML to its document core; this is the adapter for a DOM document: a block is a `p`, `h1`-`h6` or `li`, a table keeps its
  // spans, a run keeps size, font, colour, highlight, bold, italic, underline and strike, a link keeps its address.
  // Not carried: widths, borders, shading, paragraph spacing and indents, list numbers (the editor numbers its lists).
  // The model:
  //   { type: "text", tag: "p" | "h1".."h6" | "li", list: "ul" | "ol" | "task" | null, level, checked, align, runs }
  //   { type: "table", rows: [[{ colSpan, rowSpan, blocks }]] }       blocks: text blocks and pictures of the cell
  //   { type: "image", bytes, hint }
  // A run is `{ text, fmt, href }` or `{ br: true }`.

  const LIMITS = {
    MAX_HTML_CHARS: 4 * 1024 * 1024,
    MAX_NODES: 300000,
    MAX_FORMAT_READS: 150000,
    MAX_DEPTH: 100,
    MAX_BLOCKS: 20000,
    MAX_RUNS: 200000,
    MAX_TEXT_CHARS: 2 * 1024 * 1024,
    MAX_IMAGES: 32,
    MAX_IMAGE_BYTES: 32 * 1024 * 1024,
    MAX_TOTAL_IMAGE_BYTES: 64 * 1024 * 1024,
    MAX_TABLE_ROWS: 2000,
    MAX_TABLE_COLS: 100,
    MAX_TABLE_CELLS: 20000,
    MAX_SPAN: 100,
    MAX_LIST_LEVEL: 8,
  };

  const DROPPED_TAGS = new Set([
    "script", "style", "head", "title", "meta", "link", "base", "noscript", "template", "iframe", "frame", "frameset", "object",
    "embed", "applet", "svg", "math", "audio", "video", "canvas", "map", "select", "option", "datalist", "input", "button",
    "textarea", "xml",
  ]);
  const HEADINGS = new Set(["h1", "h2", "h3", "h4", "h5", "h6"]);
  const TEXT_BLOCK_TAGS = new Set(["p", "pre", "blockquote", "dt", "dd", "address", "figcaption", "caption", "listing", "xmp"]);
  const CONTAINER_TAGS = new Set(["center", "div", "section", "article", "header", "footer", "main", "aside", "nav", "figure", "dl", "form", "fieldset", "body", "html", "details", "summary", "hgroup"]);
  const TABLE_PARTS = new Set(["thead", "tbody", "tfoot", "tr", "td", "th", "colgroup", "col"]);
  const ALIGNS = new Set(["left", "center", "right", "justify"]);
  const IMAGE_DATA_URL = /^data:image\/(png|jpe?g);base64,/i;
  const CONTROL_CHARS = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g;
  const COLLAPSIBLE_SPACE = /[ \t\r\n\f]+/g;

  const NO_FORMAT = Object.freeze({});

  class PasteLimitError extends Error {}

  // ---------------------------------------------------------------- reading one document

  function createReader() {
    const R = host.__EASYOFFICE_DOC_RUNS__;
    const links = host.__EASYOFFICE_DOC_LINK__;
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const spent = { formatReads: 0, nodes: 0, blocks: 0, runs: 0, chars: 0, images: 0, imageBytes: 0, cells: 0 };

    const over = (key, limit, add = 1) => {
      spent[key] += add;
      if (spent[key] > limit) throw new PasteLimitError(key);
    };
    const tagOf = (node) => String(node.tagName || "").toLowerCase();

    // The character format an element adds to the one it is inside: `ctx.fmt` is carried down the walk, so each element is read
    // once (reading every ancestor of every text again is quadratic in the depth). Null when it adds nothing.
    function ownFormat(el) {
      over("formatReads", LIMITS.MAX_FORMAT_READS);
      const own = R.isRunElement(el) ? R.readRunFormat(el) : R.readLegacyFormat(el);
      return Object.keys(own).length > 0 ? own : null;
    }

    function alignOf(el) {
      const css = String(el.style?.textAlign || el.getAttribute?.("align") || "").toLowerCase();
      return ALIGNS.has(css) ? css : null;
    }

    function newBlock(tag, props = {}) {
      over("blocks", LIMITS.MAX_BLOCKS);
      return { type: "text", tag, list: null, level: 0, checked: false, align: null, runs: [], pre: false, ...props };
    }

    const sameFormat = (a, b) => {
      const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
      for (const key of keys) if (a[key] !== b[key]) return false;
      return true;
    };

    function addRun(block, run) {
      over("runs", LIMITS.MAX_RUNS);
      const last = block.runs.at(-1);
      if (run.text !== undefined && last?.text !== undefined && last.href === run.href && sameFormat(last.fmt, run.fmt)) last.text += run.text;
      else block.runs.push(run);
    }

    function addText(ctx, node) {
      let data = String(node.data ?? "").replace(CONTROL_CHARS, "");
      if (data === "") return;
      if (!ctx.pre) {
        data = data.replace(COLLAPSIBLE_SPACE, " ");
        // A space after a space, or at the start of a block, is not shown.
        const before = ctx.cur?.runs.at(-1);
        if (data[0] === " " && (!before || before.br || before.text.endsWith(" "))) data = data.slice(1);
        if (data === "") return;
      }
      over("chars", LIMITS.MAX_TEXT_CHARS, data.length);
      const block = ensureBlock(ctx);
      if (ctx.pre) {
        data.split(/\r\n|\r|\n/).forEach((line, i) => {
          if (i > 0) addRun(block, { br: true });
          if (line) addRun(block, { text: line, fmt: ctx.fmt, href: ctx.href });
        });
      } else {
        addRun(block, { text: data, fmt: ctx.fmt, href: ctx.href });
      }
    }

    // The properties of a block that starts inside the context: the list item it is in, or the alignment of its container.
    const propsOf = (ctx) => (ctx.listItem ? { ...ctx.listProps, pre: ctx.pre } : { align: ctx.align, pre: ctx.pre });

    function ensureBlock(ctx) {
      ctx.cur ??= newBlock(ctx.listItem ?? "p", propsOf(ctx));
      return ctx.cur;
    }

    // Ends the open block; a block with nothing in it is not kept (`<p></p>` shows nothing).
    function flush(ctx) {
      const block = ctx.cur;
      ctx.cur = null;
      if (!block) return;
      const last = block.runs.at(-1);
      if (last?.text !== undefined && last.text.endsWith(" ") && !block.pre) {
        last.text = last.text.slice(0, -1);
        if (last.text === "") block.runs.pop();
      }
      if (block.runs.length > 0) ctx.blocks.push(block);
    }

    function imageOf(el) {
      const src = String(el.getAttribute?.("src") || "").trim();
      if (!IMAGE_DATA_URL.test(src)) return null;
      const payload = src.slice(src.indexOf(",") + 1);
      if (payload.length > Math.ceil((LIMITS.MAX_IMAGE_BYTES * 4) / 3) + 4) throw new PasteLimitError("imageBytes");
      let bytes;
      try {
        bytes = utils.base64ToBytes(payload.replace(/\s+/g, ""));
      } catch {
        return null;
      }
      over("images", LIMITS.MAX_IMAGES);
      over("imageBytes", LIMITS.MAX_TOTAL_IMAGE_BYTES, bytes.length);
      return { type: "image", bytes, hint: String(el.getAttribute?.("alt") || "").trim().slice(0, 120) };
    }

    // ------------------------------------------------------------ tables

    function spanOf(el, name) {
      const n = Math.trunc(Number(el.getAttribute?.(name)));
      return Number.isFinite(n) && n > 1 ? Math.min(n, LIMITS.MAX_SPAN) : 1;
    }

    function rowsOf(table) {
      const rows = [];
      for (const child of Array.from(table.childNodes)) {
        const tag = child.nodeType === 1 ? tagOf(child) : "";
        if (tag === "tr") rows.push(child);
        else if (tag === "thead" || tag === "tbody" || tag === "tfoot") rows.push(...Array.from(child.childNodes).filter((n) => n.nodeType === 1 && tagOf(n) === "tr"));
      }
      return rows;
    }

    function readTable(table, depth, fmt) {
      const trs = rowsOf(table);
      if (trs.length > LIMITS.MAX_TABLE_ROWS) throw new PasteLimitError("rows");
      const rows = [];
      for (const tr of trs) {
        const cells = Array.from(tr.childNodes).filter((n) => n.nodeType === 1 && (tagOf(n) === "td" || tagOf(n) === "th"));
        if (cells.length === 0) continue;
        let width = 0;
        rows.push(cells.map((cell) => {
          over("cells", LIMITS.MAX_TABLE_CELLS);
          const colSpan = spanOf(cell, "colspan");
          width += colSpan;
          if (width > LIMITS.MAX_TABLE_COLS) throw new PasteLimitError("cols");
          const inner = newContext({ inCell: true, fmt });
          visitChildren(cell, inner, depth + 1);
          flush(inner);
          return { colSpan, rowSpan: spanOf(cell, "rowspan"), blocks: inner.blocks };
        }));
      }
      return rows.length > 0 ? { type: "table", rows } : null;
    }

    // ------------------------------------------------------------ the walk

    function newContext(extra = {}) {
      return { blocks: [], cur: null, fmt: NO_FORMAT, pre: false, href: null, align: null, listItem: null, listProps: null, lists: [], inCell: false, ...extra };
    }

    function visitChildren(el, ctx, depth) {
      for (const child of Array.from(el.childNodes)) visit(child, ctx, depth);
    }

    function openTextBlock(el, ctx, depth, tag) {
      flush(ctx);
      const keepAlign = ctx.align;
      ctx.align = alignOf(el) ?? keepAlign;
      ctx.cur = newBlock(tag, propsOf(ctx));
      visitChildren(el, ctx, depth);
      flush(ctx);
      ctx.align = keepAlign;
    }

    function openList(el, ctx, depth, tag) {
      flush(ctx);
      const task = el.dataset?.eoList === "task";
      ctx.lists.push({ kind: tag === "ol" ? "ol" : task ? "task" : "ul" });
      visitChildren(el, ctx, depth);
      flush(ctx);
      ctx.lists.pop();
    }

    function openListItem(el, ctx, depth) {
      flush(ctx);
      const list = ctx.lists.at(-1) ?? { kind: "ul" };
      const props = {
        list: list.kind,
        level: Math.min(Math.max(ctx.lists.length - 1, 0), LIMITS.MAX_LIST_LEVEL),
        checked: list.kind === "task" && el.dataset?.eoChecked === "true",
        align: alignOf(el) ?? ctx.align,
      };
      const saved = [ctx.listItem, ctx.listProps];
      ctx.listItem = "li";
      ctx.listProps = props;
      ctx.cur = newBlock("li", props);
      visitChildren(el, ctx, depth);
      flush(ctx);
      [ctx.listItem, ctx.listProps] = saved;
    }

    function visitElement(el, ctx, depth) {
      if (DROPPED_TAGS.has(tagOf(el)) || el.hasAttribute?.("hidden") || el.style?.display === "none") return;
      const outer = ctx.fmt;
      const own = ownFormat(el);
      if (own) ctx.fmt = { ...outer, ...own };
      dispatchElement(el, ctx, depth);
      ctx.fmt = outer;
    }

    function dispatchElement(el, ctx, depth) {
      const tag = tagOf(el);
      if (tag === "br") {
        addRun(ensureBlock(ctx), { br: true });
      } else if (tag === "img") {
        const image = imageOf(el);
        if (image) {
          flush(ctx);
          ctx.blocks.push(image);
        }
      } else if (tag === "table") {
        flush(ctx);
        if (ctx.inCell) return visitChildren(el, ctx, depth);
        const table = readTable(el, depth, ctx.fmt);
        if (table) ctx.blocks.push(table);
      } else if (tag === "td" || tag === "th" || TABLE_PARTS.has(tag)) {
        flush(ctx);
        visitChildren(el, ctx, depth);
        flush(ctx);
      } else if (tag === "ul" || tag === "ol") {
        openList(el, ctx, depth, tag);
      } else if (tag === "li") {
        openListItem(el, ctx, depth);
      } else if (HEADINGS.has(tag)) {
        openTextBlock(el, ctx, depth, tag);
      } else if (TEXT_BLOCK_TAGS.has(tag)) {
        const keepPre = ctx.pre;
        ctx.pre = ctx.pre || tag === "pre" || tag === "listing" || tag === "xmp";
        openTextBlock(el, ctx, depth, ctx.listItem ?? "p");
        ctx.pre = keepPre;
      } else if (CONTAINER_TAGS.has(tag)) {
        flush(ctx);
        const keepAlign = ctx.align;
        ctx.align = alignOf(el) ?? keepAlign;
        visitChildren(el, ctx, depth);
        flush(ctx);
        ctx.align = keepAlign;
      } else if (tag === "a") {
        const keepHref = ctx.href;
        ctx.href = links.normalizeUrl(el.getAttribute?.("href")) ?? keepHref;
        visitChildren(el, ctx, depth);
        ctx.href = keepHref;
      } else {
        visitChildren(el, ctx, depth);
      }
    }

    function visit(node, ctx, depth) {
      over("nodes", LIMITS.MAX_NODES);
      if (depth > LIMITS.MAX_DEPTH) throw new PasteLimitError("depth");
      if (node.nodeType === 3) addText(ctx, node);
      else if (node.nodeType === 1) visitElement(node, ctx, depth + 1);
    }

    function read(body) {
      const ctx = newContext();
      visitChildren(body, ctx, 0);
      flush(ctx);
      return ctx.blocks;
    }

    return { read };
  }

  // `{ blocks }` for pasted HTML, or `{ error }`: "empty", "parse" or "limit" (the HTML is refused, not cut).
  function parseHtml(html, env) {
    if (typeof html !== "string" || html.length === 0) return { error: "empty" };
    // The bound is on what the clipboard held (`doc-paste.js` looks at that); pictures filled in from the RTF come on top of it.
    if (html.length > LIMITS.MAX_HTML_CHARS + Math.ceil((LIMITS.MAX_TOTAL_IMAGE_BYTES * 4) / 3)) return { error: "limit" };
    try {
      const parsed = new env.DOMParser().parseFromString(html, "text/html");
      if (!parsed?.body) return { error: "parse" };
      const blocks = createReader().read(parsed.body);
      return blocks.length > 0 ? { blocks } : { error: "empty" };
    } catch (error) {
      return { error: error instanceof PasteLimitError ? "limit" : "parse" };
    }
  }

  // Plain text as blocks: one paragraph per line (the reference splits the paragraph at each line break, `pastePlainText`).
  function parseText(text) {
    if (typeof text !== "string" || text === "") return { error: "empty" };
    if (text.length > LIMITS.MAX_TEXT_CHARS) return { error: "limit" };
    const lines = text.replace(CONTROL_CHARS, "").split(/\r\n|\r|\n/);
    if (lines.length > LIMITS.MAX_BLOCKS) return { error: "limit" };
    const blocks = lines.map((line) => ({ type: "text", tag: "p", list: null, level: 0, checked: false, align: null, pre: false, runs: line ? [{ text: line, fmt: {}, href: null }] : [] }));
    return { blocks };
  }

  host.__EASYOFFICE_DOC_PASTE_MODEL__ = { LIMITS, parseHtml, parseText };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
