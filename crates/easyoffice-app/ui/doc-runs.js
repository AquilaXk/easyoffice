(function (host) {
  "use strict";

  // Character-run engine for the document editor (PD-077). A "run" is a flat <span class="eo-run">
  // that carries character properties in its dataset (eoSize, eoFont, eoColor, eoHighlight,
  // eoBold, eoItalic, eoUnderline, eoStrike) and mirrors them through CSSOM style properties.
  // Applying a property to a selection splits the text nodes at the selection boundaries, wraps
  // only the selected text in runs, merges identical neighbours and removes empty runs. Nothing
  // here uses execCommand, innerHTML, or style strings built from user input: every value is
  // validated first and assigned through element.style / dataset.

  const RUN_CLASS = "eo-run";
  const BOOL_PROPS = ["bold", "italic", "underline", "strike"];
  const VALUE_PROPS = ["size", "font", "color", "highlight"];
  const PROPS = [...VALUE_PROPS, ...BOOL_PROPS];
  const DATA_KEY = {
    size: "eoSize", font: "eoFont", color: "eoColor", highlight: "eoHighlight",
    bold: "eoBold", italic: "eoItalic", underline: "eoUnderline", strike: "eoStrike",
  };
  const MIN_HALF_PT = 2;
  const MAX_HALF_PT = 1638;
  const DEFAULT_HALF_PT = 22;
  const MIXED = "mixed";
  const SKIP_TAGS = new Set(["BUTTON", "SCRIPT", "STYLE", "TEMPLATE", "NOSCRIPT", "FOOTER"]);
  const BLOCK_TAGS = new Set([
    "P", "DIV", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "TD", "TH", "BLOCKQUOTE", "PRE",
    "SECTION", "ARTICLE", "HEADER", "FIGCAPTION", "DD", "DT", "ASIDE", "CAPTION",
  ]);
  // Runs are flattened inside the nearest container; links keep their own run scope.
  const CONTAINER_TAGS = new Set([...BLOCK_TAGS, "A"]);
  const STRUCTURAL_TAGS = new Set(["TABLE", "THEAD", "TBODY", "TFOOT", "TR", "UL", "OL", "DL"]);
  const NAMED_HIGHLIGHTS = { yellow: "FFFF00", lime: "00FF00", cyan: "00FFFF", pink: "FF00FF" };
  const FONT_RE = /^[^\u0000-\u001f"'\\;{}<>`]{1,64}$/u;
  const HEX_RE = /^#?([0-9a-f]{6})$/i;
  const SHORT_HEX_RE = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i;
  const WORD_RE = /[\p{L}\p{N}_]/u;

  // ---------------------------------------------------------------- value validation

  function normalizeColor(value) {
    const raw = String(value ?? "").trim();
    const full = HEX_RE.exec(raw);
    if (full) return full[1].toUpperCase();
    const short = SHORT_HEX_RE.exec(raw);
    if (short) return (short[1] + short[1] + short[2] + short[2] + short[3] + short[3]).toUpperCase();
    const rgb = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})/i.exec(raw);
    if (rgb) {
      return rgb.slice(1, 4).map((n) => Math.min(255, Number(n)).toString(16).padStart(2, "0")).join("").toUpperCase();
    }
    return NAMED_HIGHLIGHTS[raw.toLowerCase()] || null;
  }

  function normalizeFont(value) {
    const first = String(value ?? "").split(",")[0].trim().replace(/^["']|["']$/g, "").trim();
    return FONT_RE.test(first) ? first : null;
  }

  function ptToHalfPt(pt) {
    const n = Number(pt);
    if (!Number.isFinite(n)) return null;
    return Math.max(MIN_HALF_PT, Math.min(MAX_HALF_PT, Math.round(n * 2)));
  }

  // Returns the normalized value of one property, `null` to clear it, or `undefined` if invalid.
  function normalizeProp(prop, value) {
    if (value === null || value === false || value === "") return null;
    if (BOOL_PROPS.includes(prop)) return value === true ? true : undefined;
    if (prop === "size") {
      const n = Number(value);
      return Number.isInteger(n) && n >= MIN_HALF_PT && n <= MAX_HALF_PT ? n : undefined;
    }
    if (prop === "font") return normalizeFont(value) ?? undefined;
    if (prop === "color" || prop === "highlight") return normalizeColor(value) ?? undefined;
    return undefined;
  }

  function normalizePatch(patch) {
    const out = {};
    let any = false;
    PROPS.forEach((prop) => {
      if (!patch || !Object.hasOwn(patch, prop) || patch[prop] === undefined) return;
      const v = normalizeProp(prop, patch[prop]);
      if (v === undefined) return;
      out[prop] = v;
      any = true;
    });
    return any ? out : null;
  }

  // ---------------------------------------------------------------- run elements

  function isRunElement(el) {
    return el?.nodeType === 1 && (el.tagName || "").toUpperCase() === "SPAN" && Boolean(el.classList?.contains(RUN_CLASS));
  }

  function readRunFormat(el) {
    const fmt = {};
    BOOL_PROPS.forEach((p) => {
      if (el.dataset[DATA_KEY[p]] === "true") fmt[p] = true;
    });
    const size = Number(el.dataset[DATA_KEY.size]);
    if (Number.isInteger(size) && size >= MIN_HALF_PT && size <= MAX_HALF_PT) fmt.size = size;
    const font = el.dataset[DATA_KEY.font];
    if (font && FONT_RE.test(font)) fmt.font = font;
    ["color", "highlight"].forEach((p) => {
      const c = normalizeColor(el.dataset[DATA_KEY[p]]);
      if (c) fmt[p] = c;
    });
    return fmt;
  }

  function cssSizeToHalfPt(raw) {
    const m = /^([\d.]+)(pt|px)$/i.exec(String(raw || "").trim());
    if (!m) return null;
    return ptToHalfPt(m[2].toLowerCase() === "px" ? Number(m[1]) * 0.75 : Number(m[1]));
  }

  const TAG_FORMAT = {
    B: { bold: true }, STRONG: { bold: true }, I: { italic: true }, EM: { italic: true },
    U: { underline: true }, INS: { underline: true },
    S: { strike: true }, STRIKE: { strike: true }, DEL: { strike: true },
  };

  function readInlineStyleFormat(st) {
    const weight = String(st.fontWeight || "").toLowerCase();
    const deco = String(st.textDecoration || st.textDecorationLine || "").toLowerCase();
    const candidates = {
      bold: weight === "bold" || weight === "bolder" || Number(weight) >= 600,
      italic: String(st.fontStyle || "").toLowerCase() === "italic",
      underline: deco.includes("underline"),
      strike: deco.includes("line-through"),
      size: cssSizeToHalfPt(st.fontSize),
      font: st.fontFamily ? normalizeFont(st.fontFamily) : null,
      color: st.color ? normalizeColor(st.color) : null,
      highlight: st.backgroundColor ? normalizeColor(st.backgroundColor) : null,
    };
    return Object.fromEntries(Object.entries(candidates).filter(([, v]) => v));
  }

  // Formatting of elements that were not created by the run engine (pasted or opened content).
  function readLegacyFormat(el) {
    const tag = (el.tagName || "").toUpperCase();
    const fmt = { ...(TAG_FORMAT[tag] || {}) };
    if (tag !== "SPAN" && tag !== "FONT") return fmt;
    return Object.assign(fmt, readInlineStyleFormat(el.style || {}));
  }
  function halfPtToCss(halfPt) {
    return (halfPt / 2) + "pt";
  }

  // CSSOM property values mirroring a (validated) format; empty string = property not set.
  function styleFor(fmt) {
    const deco = [fmt.underline ? "underline" : "", fmt.strike ? "line-through" : ""].filter(Boolean).join(" ");
    return {
      fontSize: fmt.size ? halfPtToCss(fmt.size) : "",
      fontFamily: fmt.font ? '"' + fmt.font + '"' : "",
      color: fmt.color ? "#" + fmt.color : "",
      backgroundColor: fmt.highlight ? "#" + fmt.highlight : "",
      fontWeight: fmt.bold ? "bold" : "",
      fontStyle: fmt.italic ? "italic" : "",
      textDecoration: deco,
    };
  }

  function dataFor(fmt) {
    const out = {};
    PROPS.forEach((p) => {
      if (fmt[p] !== undefined && fmt[p] !== null && fmt[p] !== false) out[DATA_KEY[p]] = String(fmt[p]);
    });
    return out;
  }

  // Writes `fmt` onto a run span through dataset + CSSOM (all values already validated).
  function writeRun(span, fmt) {
    PROPS.forEach((p) => {
      delete span.dataset[DATA_KEY[p]];
    });
    Object.entries(dataFor(fmt)).forEach(([k, v]) => {
      span.dataset[k] = v;
    });
    Object.entries(styleFor(fmt)).forEach(([k, v]) => {
      span.style[k] = v;
    });
  }

  // Format of a structured-payload / decoded model run ({sizeHalfPt, font, color, highlight,
  // bold, italic, underline, strike}); invalid values are dropped.
  function fromModelRun(run) {
    const patch = normalizePatch({
      size: run?.sizeHalfPt, font: run?.font, color: run?.color, highlight: run?.highlight,
      bold: run?.bold === true ? true : undefined, italic: run?.italic === true ? true : undefined,
      underline: run?.underline === true ? true : undefined, strike: run?.strike === true ? true : undefined,
    });
    const fmt = {};
    PROPS.forEach((p) => {
      if (patch && patch[p] !== null && patch[p] !== undefined) fmt[p] = patch[p];
    });
    return fmt;
  }

  // Node spec (snapshot/restore shape) of a run span holding `children`.
  function buildRunSpec(fmt, children) {
    const style = {};
    Object.entries(styleFor(fmt)).forEach(([k, v]) => {
      if (v) style[k] = v;
    });
    return { t: "el", tag: "span", attrs: { class: RUN_CLASS }, data: dataFor(fmt), style, children };
  }

  function hasAnyProp(fmt) {
    return PROPS.some((p) => fmt[p] !== undefined && fmt[p] !== null && fmt[p] !== false);
  }

  function runKey(fmt) {
    return PROPS.map((p) => (fmt[p] === undefined || fmt[p] === null || fmt[p] === false ? "" : String(fmt[p]))).join("|");
  }

  function createRun(doc, fmt) {
    const span = doc.createElement("span");
    span.className = RUN_CLASS;
    writeRun(span, fmt);
    return span;
  }

  // ---------------------------------------------------------------- tree helpers

  function indexOfChild(parent, node) {
    return Array.prototype.indexOf.call(parent.childNodes, node);
  }

  function removeNode(node) {
    const parent = node.parentElement;
    if (parent) parent.removeChild(node);
  }

  function isSkippedElement(el, extra) {
    const tag = (el.tagName || "").toUpperCase();
    if (SKIP_TAGS.has(tag)) return true;
    if (el.getAttribute?.("contenteditable") === "false" || el.hasAttribute?.("hidden")) return true;
    if (el.classList?.contains("doc-table-toolbar") || el.id === "hwp-manuscript-preview") return true;
    return typeof extra === "function" && extra(el);
  }

  function isExcluded(node, root, extra) {
    let cur = node.nodeType === 1 ? node : node.parentElement;
    while (cur && cur !== root) {
      if (isSkippedElement(cur, extra)) return true;
      cur = cur.parentElement;
    }
    return false;
  }

  // Spans that carry a class (for example the boxed-heading title) are structure, not formatting.
  function isContainerElement(el) {
    if (CONTAINER_TAGS.has((el.tagName || "").toUpperCase())) return true;
    return (el.tagName || "").toUpperCase() === "SPAN" && !isRunElement(el) && Boolean(el.getAttribute?.("class"));
  }

  function containerOf(node, root) {
    let cur = node.nodeType === 1 ? node : node.parentElement;
    while (cur && cur !== root) {
      if (isContainerElement(cur)) return cur;
      cur = cur.parentElement;
    }
    return root;
  }

  function blockOf(node, root) {
    let cur = node.nodeType === 1 ? node : node.parentElement;
    while (cur && cur !== root) {
      if (BLOCK_TAGS.has((cur.tagName || "").toUpperCase())) return cur;
      cur = cur.parentElement;
    }
    return root;
  }

  function hasBlockChild(el) {
    return Array.from(el.childNodes).some((c) => c.nodeType === 1 && BLOCK_TAGS.has((c.tagName || "").toUpperCase()));
  }

  function pathTo(root, node) {
    const path = [];
    let cur = node;
    while (cur && cur !== root) {
      const parent = cur.parentElement;
      if (!parent) break;
      path.unshift(indexOfChild(parent, cur));
      cur = parent;
    }
    return path;
  }

  // Compares the points (pathA, offA) and (pathB, offB); a point is its index path plus offset.
  function cmpPathOffset(a, ao, b, bo) {
    const la = a.length + 1;
    const lb = b.length + 1;
    const n = Math.min(la, lb);
    for (let i = 0; i < n; i += 1) {
      const va = i < a.length ? a[i] : ao;
      const vb = i < b.length ? b[i] : bo;
      if (va !== vb) return va < vb ? -1 : 1;
    }
    if (la === lb) return 0;
    return la < lb ? -1 : 1;
  }

  function comparePoints(root, aNode, aOff, bNode, bOff) {
    return cmpPathOffset(pathTo(root, aNode), aOff, pathTo(root, bNode), bOff);
  }

  // Text nodes under `scope` in document order with their index paths from `root`.
  function textEntries(scope, root, extra) {
    const out = [];
    if (isExcluded(scope, root, extra)) return out;
    const walk = (node, path) => {
      node.childNodes.forEach((child, i) => {
        if (child.nodeType === 3) out.push({ node: child, path: path.concat([i]) });
        else if (child.nodeType === 1 && !isSkippedElement(child, extra)) walk(child, path.concat([i]));
      });
    };
    walk(scope, pathTo(root, scope));
    return out;
  }

  function collectTextNodes(scope, root, extra) {
    return textEntries(scope, root, extra).map((e) => e.node);
  }

  // Whitespace-only text between blocks (or inside tables/lists) is layout, not content.
  function isLayoutWhitespace(text) {
    if (String(text.data ?? text.nodeValue ?? "").trim() !== "") return false;
    const parent = text.parentElement;
    if (!parent) return true;
    return STRUCTURAL_TAGS.has((parent.tagName || "").toUpperCase()) || hasBlockChild(parent);
  }

  function textLength(node) {
    return String(node.data ?? node.nodeValue ?? "").length;
  }

  // Characters of document text before the point (stable across splits and merges).
  function textOffsetOf(root, node, offset, extraExclude, scope = root) {
    const point = pathTo(root, node);
    let total = 0;
    for (const entry of textEntries(scope, root, extraExclude)) {
      if (entry.node === node) return total + offset;
      const len = textLength(entry.node);
      if (cmpPathOffset(entry.path, len, point, offset) > 0) break;
      total += len;
    }
    return total;
  }

  // The text position for an offset; `preferStart` picks the next node's start on a boundary.
  function pointAtTextOffset(root, offset, preferStart, extraExclude, scope = root) {
    const texts = collectTextNodes(scope, root, extraExclude);
    let acc = 0;
    let last = null;
    for (const t of texts) {
      const len = textLength(t);
      if (offset < acc + len || (offset === acc + len && !preferStart)) {
        return { node: t, offset: offset - acc };
      }
      acc += len;
      last = t;
    }
    return last ? { node: last, offset: textLength(last) } : { node: root, offset: 0 };
  }

  function splitTextAt(text, offset) {
    const len = textLength(text);
    if (offset <= 0 || offset >= len) return null;
    return text.splitText(offset);
  }

  // Splits text nodes at the range boundaries so every selected character sits in its own nodes.
  function splitBoundaries(startIn, endIn) {
    const start = { node: startIn.node, offset: startIn.offset };
    const end = { node: endIn.node, offset: endIn.offset };
    if (end.node.nodeType === 3) {
      splitTextAt(end.node, end.offset);
    }
    if (start.node.nodeType === 3) {
      const tail = splitTextAt(start.node, start.offset);
      if (tail) {
        if (end.node === start.node) {
          end.node = tail;
          end.offset -= start.offset;
        }
        start.node = tail;
        start.offset = 0;
      }
    }
    return { start, end };
  }

  // Makes `node` a direct child of `container`, splitting every inline ancestor around it.
  function liftOut(node, container) {
    while (node.parentElement && node.parentElement !== container) {
      const parent = node.parentElement;
      const grand = parent.parentElement;
      if (!grand) return;
      const idx = indexOfChild(parent, node);
      const tail = parent.cloneNode(false);
      Array.from(parent.childNodes).slice(idx + 1).forEach((c) => tail.appendChild(c));
      const after = parent.nextSibling ?? null;
      grand.insertBefore(node, after);
      if (tail.childNodes.length > 0) grand.insertBefore(tail, node.nextSibling ?? null);
      if (parent.childNodes.length === 0) grand.removeChild(parent);
    }
  }

  function formatOfText(text, root) {
    const container = containerOf(text, root);
    const chain = [];
    let cur = text.parentElement;
    while (cur && cur !== container && cur !== root) {
      chain.unshift(cur);
      cur = cur.parentElement;
    }
    const fmt = {};
    chain.forEach((el) => Object.assign(fmt, isRunElement(el) ? readRunFormat(el) : readLegacyFormat(el)));
    return fmt;
  }

  function mergePatch(fmt, patch) {
    const next = { ...fmt };
    PROPS.forEach((p) => {
      if (!Object.hasOwn(patch, p)) return;
      if (patch[p] === null) delete next[p];
      else next[p] = patch[p];
    });
    return next;
  }

  function sameFormat(a, b) {
    return runKey(a) === runKey(b);
  }

  // ---------------------------------------------------------------- normalization

  function mergeAdjacentText(container) {
    let child = container.firstChild ?? container.childNodes[0] ?? null;
    while (child) {
      const next = child.nextSibling ?? null;
      if (child.nodeType === 3 && textLength(child) === 0) removeNode(child);
      else if (child.nodeType === 3 && next?.nodeType === 3) {
        child.data += next.data;
        removeNode(next);
        continue;
      }
      child = next;
    }
  }

  function unwrapElement(el) {
    const parent = el.parentElement;
    if (!parent) return;
    Array.from(el.childNodes).forEach((c) => parent.insertBefore(c, el));
    parent.removeChild(el);
  }

  function isEmptyRun(el) {
    return Array.from(el.childNodes).every((c) => c.nodeType === 3 && textLength(c) === 0);
  }

  // Unwraps property-less runs, drops empty runs, merges identical neighbours and adjacent text.
  function normalizeContainer(container) {
    Array.from(container.childNodes).forEach((child) => {
      if (!isRunElement(child)) return;
      mergeAdjacentText(child);
      if (isEmptyRun(child)) removeNode(child);
      else if (!hasAnyProp(readRunFormat(child))) unwrapElement(child);
    });
    let child = container.childNodes[0] ?? null;
    while (child) {
      const next = child.nextSibling ?? null;
      if (isRunElement(child) && isRunElement(next) && runKey(readRunFormat(child)) === runKey(readRunFormat(next))) {
        Array.from(next.childNodes).forEach((c) => child.appendChild(c));
        removeNode(next);
        mergeAdjacentText(child);
        continue;
      }
      child = next;
    }
    mergeAdjacentText(container);
  }

  // ---------------------------------------------------------------- applying formats

  function toPoint(node, offset) {
    return { node, offset };
  }

  function orderPoints(root, a, b) {
    return comparePoints(root, a.node, a.offset, b.node, b.offset) <= 0 ? [a, b] : [b, a];
  }

  function selectedTextNodes(root, start, end, extraExclude) {
    const scope = commonScope(root, start.node, end.node);
    const sp = pathTo(root, start.node);
    const ep = pathTo(root, end.node);
    return textEntries(scope, root, extraExclude)
      .filter(({ node, path }) => {
        const len = textLength(node);
        if (len === 0 || isLayoutWhitespace(node)) return false;
        return cmpPathOffset(path, 0, sp, start.offset) >= 0 && cmpPathOffset(path, len, ep, end.offset) <= 0;
      })
      .map((e) => e.node);
  }

  function commonScope(root, a, b) {
    let cur = a.nodeType === 1 ? a : a.parentElement;
    while (cur && cur !== root && !cur.contains(b)) cur = cur.parentElement;
    return cur || root;
  }

  function wrapText(text, container, fmt) {
    liftOut(text, container);
    if (!hasAnyProp(fmt)) return;
    const span = createRun(text.ownerDocument, fmt);
    container.insertBefore(span, text);
    span.appendChild(text);
  }

  function applyToText(text, root, patch) {
    const container = containerOf(text, root);
    const before = formatOfText(text, root);
    const after = mergePatch(before, patch);
    if (sameFormat(before, after)) return null;
    const parent = text.parentElement;
    if (isRunElement(parent) && parent.parentElement === container && parent.childNodes.length === 1) {
      if (hasAnyProp(after)) writeRun(parent, after);
      else {
        writeRun(parent, {});
      }
      return container;
    }
    wrapText(text, container, after);
    return container;
  }

  function rangeFromOffsets(root, scope, startOff, endOff, extraExclude) {
    const s = pointAtTextOffset(root, startOff, true, extraExclude, scope);
    const e = pointAtTextOffset(root, endOff, false, extraExclude, scope);
    return {
      startContainer: s.node, startOffset: s.offset, endContainer: e.node, endOffset: e.offset,
      collapsed: startOff === endOff,
    };
  }

  // Applies `patchIn` ({size,font,color,highlight,bold,italic,underline,strike}; null clears) to
  // the characters of `range` ({startContainer,startOffset,endContainer,endOffset}) inside `root`.
  function applyFormat(root, range, patchIn, options = {}) {
    const patch = normalizePatch(patchIn);
    const none = { changed: false, containers: [], range: null };
    if (!patch || !range || !root.contains(range.startContainer) || !root.contains(range.endContainer)) return none;
    const extra = options.isExcluded;
    const [rawStart, rawEnd] = orderPoints(
      root,
      toPoint(range.startContainer, range.startOffset),
      toPoint(range.endContainer, range.endOffset)
    );
    // Offsets are measured inside the container holding the range, so edits stay local.
    const scope = containerOf(commonScope(root, rawStart.node, rawEnd.node), root);
    const startOff = textOffsetOf(root, rawStart.node, rawStart.offset, extra, scope);
    const endOff = textOffsetOf(root, rawEnd.node, rawEnd.offset, extra, scope);
    if (startOff === endOff) return none;
    const { start, end } = splitBoundaries(rawStart, rawEnd);
    const touched = new Set();
    let changed = false;
    selectedTextNodes(root, start, end, extra).forEach((text) => {
      touched.add(containerOf(text, root));
      if (applyToText(text, root, patch)) changed = true;
    });
    touched.forEach((c) => normalizeContainer(c));
    return { changed, containers: Array.from(touched), range: rangeFromOffsets(root, scope, startOff, endOff, extra) };
  }

  // ---------------------------------------------------------------- querying

  function foldValue(values) {
    if (values.length === 0) return null;
    const first = values[0];
    return values.every((v) => v === first) ? first : MIXED;
  }

  function overlappingTextNodes(root, range, extraExclude) {
    const [start, end] = orderPoints(
      root,
      toPoint(range.startContainer, range.startOffset),
      toPoint(range.endContainer, range.endOffset)
    );
    const scope = commonScope(root, start.node, end.node);
    const sp = pathTo(root, start.node);
    const ep = pathTo(root, end.node);
    return textEntries(scope, root, extraExclude)
      .filter(({ node, path }) => {
        const len = textLength(node);
        if (len === 0 || isLayoutWhitespace(node)) return false;
        return cmpPathOffset(path, len, sp, start.offset) > 0 && cmpPathOffset(path, 0, ep, end.offset) < 0;
      })
      .map((e) => e.node);
  }

  function caretTextNode(root, point, extraExclude) {
    const container = containerOf(point.node, root);
    const off = textOffsetOf(root, point.node, point.offset, extraExclude, container);
    let acc = 0;
    let before = null;
    let after = null;
    collectTextNodes(container, root, extraExclude).forEach((t) => {
      const len = textLength(t);
      if (len > 0 && !isLayoutWhitespace(t)) {
        if (acc < off && off <= acc + len) before = t;
        if (acc <= off && off < acc + len && !after) after = t;
      }
      acc += len;
    });
    const same = (t) => t && containerOf(t, root) === container;
    if (same(before)) return before;
    return same(after) ? after : null;
  }

  function foldProp(prop, formats, base) {
    return foldValue(formats.map((f) => {
      if (prop === "size") return f.size || base;
      return BOOL_PROPS.includes(prop) ? Boolean(f[prop]) : (f[prop] ?? "");
    }));
  }

  function emptyValue(prop, base) {
    if (prop === "size") return base;
    return BOOL_PROPS.includes(prop) ? false : null;
  }

  function textNodesForQuery(root, range, extra) {
    if (!range.collapsed) return overlappingTextNodes(root, range, extra);
    const t = caretTextNode(root, toPoint(range.startContainer, range.startOffset), extra);
    return t ? [t] : [];
  }

  // Format state of the selection: each property is its value, `mixed`, or null (not set).
  // `size` falls back to `baseHalfPt` for text without an explicit run size.
  function queryFormat(root, range, options = {}) {
    const base = options.baseHalfPt || DEFAULT_HALF_PT;
    if (!range || !root.contains(range.startContainer)) return null;
    const nodes = textNodesForQuery(root, range, options.isExcluded);
    const formats = nodes.map((t) => formatOfText(t, root));
    const empty = nodes.length === 0;
    const state = { empty };
    PROPS.forEach((p) => {
      state[p] = empty ? emptyValue(p, base) : foldProp(p, formats, base);
    });
    return state;
  }
  // ---------------------------------------------------------------- caret helpers

  // The word around a collapsed caret (inside one container), or null when the caret is not
  // surrounded by word characters on both sides.
  function wordRangeAt(root, node, offset, extraExclude) {
    const container = containerOf(node, root);
    const texts = collectTextNodes(container, root, extraExclude).filter((t) => textLength(t) > 0);
    const full = texts.map((t) => String(t.data ?? t.nodeValue)).join("");
    const caret = textOffsetOf(container, node, offset, extraExclude);
    if (!(caret > 0 && caret < full.length && WORD_RE.test(full[caret - 1]) && WORD_RE.test(full[caret]))) return null;
    let a = caret;
    let b = caret;
    while (a > 0 && WORD_RE.test(full[a - 1])) a -= 1;
    while (b < full.length && WORD_RE.test(full[b])) b += 1;
    const locate = (pos, preferStart) => pointAtTextOffset(container, pos, preferStart, extraExclude);
    const s = locate(a, true);
    const e = locate(b, false);
    return { startContainer: s.node, startOffset: s.offset, endContainer: e.node, endOffset: e.offset, collapsed: false };
  }

  // Inserts `text` at the caret point and formats exactly the inserted characters with `patch`.
  function insertFormattedText(root, node, offset, text, patch, options = {}) {
    const doc = root.ownerDocument;
    const fresh = doc.createTextNode(text);
    if (node.nodeType === 3) {
      const tail = splitTextAt(node, offset);
      const parent = node.parentElement;
      if (tail) parent.insertBefore(fresh, tail);
      else parent.insertBefore(fresh, offset <= 0 ? node : node.nextSibling ?? null);
    } else {
      node.insertBefore(fresh, node.childNodes[offset] ?? null);
    }
    const len = textLength(fresh);
    const scope = containerOf(fresh, root);
    const startOff = textOffsetOf(root, fresh, 0, options.isExcluded, scope);
    const res = applyFormat(
      root,
      { startContainer: fresh, startOffset: 0, endContainer: fresh, endOffset: len },
      patch,
      options
    );
    const caret = pointAtTextOffset(root, startOff + len, false, options.isExcluded, scope);
    return { changed: res.changed, caret };
  }

  // Leaf paragraph-like blocks touched by a range (including empty paragraphs).
  function blocksInRange(root, range, extraExclude) {
    if (!range || !root.contains(range.startContainer)) return [];
    const [start, end] = orderPoints(
      root,
      toPoint(range.startContainer, range.startOffset),
      toPoint(range.endContainer, range.endOffset)
    );
    const out = [];
    const sp = pathTo(root, start.node);
    const ep = pathTo(root, end.node);
    const walk = (el, path) => {
      el.childNodes.forEach((child, i) => {
        if (child.nodeType !== 1 || isSkippedElement(child, extraExclude)) return;
        const childPath = path.concat([i]);
        if (BLOCK_TAGS.has((child.tagName || "").toUpperCase()) && !hasBlockChild(child)) {
          const startsBeforeEnd = cmpPathOffset(childPath, 0, ep, end.offset) <= 0;
          const endsAfterStart = cmpPathOffset(childPath, child.childNodes.length, sp, start.offset) >= 0;
          if (startsBeforeEnd && endsAfterStart) out.push(child);
        } else {
          walk(child, childPath);
        }
      });
    };
    walk(root, []);
    if (out.length === 0) {
      const own = blockOf(start.node, root);
      if (own !== root) out.push(own);
    }
    return out;
  }

  host.__EASYOFFICE_DOC_RUNS__ = {
    RUN_CLASS, PROPS, BOOL_PROPS, VALUE_PROPS, DATA_KEY, MIXED, DEFAULT_HALF_PT, MIN_HALF_PT, MAX_HALF_PT,
    normalizeColor, normalizeFont, normalizeProp, normalizePatch, ptToHalfPt, halfPtToCss,
    isRunElement, readRunFormat, readLegacyFormat, fromModelRun, buildRunSpec, formatOfText, applyFormat, queryFormat, wordRangeAt,
    insertFormattedText, blocksInRange, blockOf, containerOf, collectTextNodes, textOffsetOf,
    pointAtTextOffset, normalizeContainer, comparePoints, cssSizeToHalfPt,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
