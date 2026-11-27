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
  const BOOL_PROPS = ["bold", "italic", "underline", "strike", "superscript", "subscript"];
  const BOOL_PROP_SET = new Set(BOOL_PROPS);
  const VALUE_PROPS = ["size", "font", "eastfont", "face", "color", "highlight", "spacing", "scale", "position", "kerning", "opentype", "nogrid", "textfx", "ucolor"];
  const PROPS = [...VALUE_PROPS, ...BOOL_PROPS];
  // Letter spacing (`spacing`): twips added after each character, either way up to 1,584 pt (PD-367).
  const MAX_SPACING_TWIPS = 31680;
  // Advanced settings of the font dialog (PD-372): `scale` is the glyph width in percent (1-600, never 100), `position` the
  // raise (positive) or lowering in half-points (either way up to 1,584 pt, never 0) and `kerning` the smallest kerned size in
  // half-points (1-3,276 = 1,638 pt).
  // PD-375 adds `opentype` (the packed OpenType features, see `doc-open-type.js`) and `nogrid` (1: the run does not snap to the
  // document's line grid, 문서 눈금이 지정되어 있으면 줄 눈금에 맞춤 off; the save carries it as `snapToGrid: false`).
  // `textfx` is the packed text effects (double strikethrough, capitals, hidden, underline style, emphasis; `doc-text-fx.js`) and
  // `ucolor` the underline colour, a hex colour like `color`.
  const ADVANCED_RANGE = { scale: [1, 600], position: [-3168, 3168], kerning: [1, 3276], opentype: [1, 2 ** 29 - 1], nogrid: [1, 1], textfx: [1, 2 ** 12 - 1] };
  const TWIPS_PER_PX = 15;
  const DATA_KEY = {
    size: "eoSize", font: "eoFont", eastfont: "eoEastfont", face: "eoFace", color: "eoColor", highlight: "eoHighlight", spacing: "eoSpacing",
    scale: "eoScale", position: "eoPosition", kerning: "eoKerning", opentype: "eoOpentype", nogrid: "eoNogrid", textfx: "eoTextfx", ucolor: "eoUcolor",
    bold: "eoBold", italic: "eoItalic", underline: "eoUnderline", strike: "eoStrike",
    superscript: "eoSuperscript", subscript: "eoSubscript",
  };
  const MIN_HALF_PT = 2;
  const MAX_HALF_PT = 8192;
  const DEFAULT_HALF_PT = 20;
  const MIXED = "mixed";
  const SKIP_TAGS = new Set(["BUTTON", "SCRIPT", "STYLE", "TEMPLATE", "NOSCRIPT", "FOOTER"]);
  const BLOCK_TAGS = new Set([
    "P", "DIV", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "TD", "TH", "BLOCKQUOTE", "PRE",
    "SECTION", "ARTICLE", "HEADER", "FIGCAPTION", "DD", "DT", "ASIDE", "CAPTION",
  ]);
  // Runs are flattened inside the nearest container; links and tracked revisions keep their own run scope.
  const CONTAINER_TAGS = new Set([...BLOCK_TAGS, "A", "INS", "DEL"]);
  const STRUCTURAL_TAGS = new Set(["TABLE", "THEAD", "TBODY", "TFOOT", "TR", "UL", "OL", "DL"]);
  const NAMED_HIGHLIGHTS = { yellow: "FFFF00", lime: "00FF00", cyan: "00FFFF", pink: "FF00FF" };
  const FONT_RE = /^[^\u0000-\u001f"'\\;{}<>`]{1,64}$/u;
  // The variant of the font (`face`, PD-368): a PostScript name, printable ASCII without quotes, brackets, slashes and the
  // other characters CSS and the document formats treat specially, at most 128 characters (the engine's limit in bytes).
  const FACE_RE = /^[A-Za-z0-9._+~!#$&*,:=?@^|-]{1,128}$/;
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

  // An advanced setting inside its range as an integer, `null` for the neutral value (scale 100, position 0, kerning 0), or
  // `undefined` for anything else (not an integer, out of range).
  function advancedValue(prop, value) {
    const n = typeof value === "number" || typeof value === "string" ? Number(value) : Number.NaN;
    if (n === 0 || (prop === "scale" && n === 100)) return null;
    const [min, max] = ADVANCED_RANGE[prop];
    if (prop === "opentype" && host.__EASYOFFICE_DOC_OPEN_TYPE__?.valid(n) === false) return undefined;
    if (prop === "textfx" && host.__EASYOFFICE_DOC_TEXT_FX__?.valid(n) === false) return undefined;
    return Number.isInteger(n) && n >= min && n <= max ? n : undefined;
  }

  // `opentype` and `textfx` pack several settings in one integer. A patch may name only some bits as `{ mask, value }`: each run
  // takes `value`'s bits inside `mask` and keeps its own bits outside it (a mixed selection keeps what the user did not touch).
  const BIT_PROPS = new Set(["opentype", "textfx"]);
  function bitsValue(prop, { mask, value }) {
    const [, max] = ADVANCED_RANGE[prop];
    const whole = (n) => Number.isInteger(n) && n >= 0 && n <= max;
    return whole(mask) && mask > 0 && whole(value) ? { mask, value: value & mask } : undefined;
  }

  // The run value after a `{ mask, value }` patch, `undefined` when the result is not a value the engine accepts.
  function applyBits(prop, old, { mask, value }) {
    const next = ((old || 0) & ~mask) | value;
    if (next === 0) return null;
    return advancedValue(prop, next) ?? undefined;
  }

  // Returns the normalized value of one property, `null` to clear it, or `undefined` if invalid.
  function normalizeProp(prop, value) {
    if (value === null || value === false || value === "") return null;
    if (BOOL_PROP_SET.has(prop)) return value === true ? true : undefined;
    if (prop === "size") {
      const n = Number(value);
      return Number.isInteger(n) && n >= MIN_HALF_PT && n <= MAX_HALF_PT ? n : undefined;
    }
    if (prop === "spacing") {
      const n = Number(value);
      if (n === 0) return null;
      return Number.isInteger(n) && Math.abs(n) <= MAX_SPACING_TWIPS ? n : undefined;
    }
    if (BIT_PROPS.has(prop) && value && typeof value === "object") return bitsValue(prop, value);
    if (ADVANCED_RANGE[prop]) return advancedValue(prop, value);
    if (prop === "font" || prop === "eastfont") return normalizeFont(value) ?? undefined;
    if (prop === "face") return FACE_RE.test(String(value)) ? String(value) : undefined;
    if (prop === "color" || prop === "highlight" || prop === "ucolor") return normalizeColor(value) ?? undefined;
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
    // A font named without the East Asian one is for both (the style bar's font list, a pasted font); the 글꼴 dialog names both.
    // `keepEast`: only the Latin font changes, so each run keeps the East Asian face it has (see `mergePatch`).
    if (patch?.keepEast === true && Object.hasOwn(out, "font") && !Object.hasOwn(out, "eastfont")) out.keepEast = true;
    else if (Object.hasOwn(out, "font") && !Object.hasOwn(out, "eastfont")) out.eastfont = null;
    if (out.superscript === true && !Object.hasOwn(out, "subscript")) out.subscript = null;
    if (out.subscript === true && !Object.hasOwn(out, "superscript")) out.superscript = null;
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
    const spacing = Number(el.dataset[DATA_KEY.spacing]);
    if (Number.isInteger(spacing) && spacing !== 0 && Math.abs(spacing) <= MAX_SPACING_TWIPS) fmt.spacing = spacing;
    Object.keys(ADVANCED_RANGE).forEach((p) => {
      const value = advancedValue(p, el.dataset[DATA_KEY[p]]);
      if (value) fmt[p] = value;
    });
    const font = el.dataset[DATA_KEY.font];
    if (font && FONT_RE.test(font)) fmt.font = font;
    const eastFont = el.dataset[DATA_KEY.eastfont];
    if (eastFont && FONT_RE.test(eastFont)) fmt.eastfont = eastFont;
    const face = el.dataset[DATA_KEY.face];
    if (face && FACE_RE.test(face)) fmt.face = face;
    ["color", "highlight", "ucolor"].forEach((p) => {
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
    SUP: { superscript: true }, SUB: { subscript: true },
  };

  function readInlineStyleFormat(st) {
    const weight = String(st.fontWeight || "").toLowerCase();
    const deco = String(st.textDecoration || st.textDecorationLine || "").toLowerCase();
    const va = String(st.verticalAlign || "").toLowerCase();
    const candidates = {
      bold: weight === "bold" || weight === "bolder" || Number(weight) >= 600,
      italic: String(st.fontStyle || "").toLowerCase() === "italic",
      underline: deco.includes("underline"),
      strike: deco.includes("line-through"),
      superscript: va === "super",
      subscript: va === "sub",
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
    if (tag === "SUP" && el.dataset?.eoNoteKind !== undefined) return {};
    if ((tag === "INS" || tag === "DEL") && el.dataset?.eoRevId !== undefined) return {};
    const fmt = { ...TAG_FORMAT[tag] };
    if (tag !== "SPAN" && tag !== "FONT") return fmt;
    return Object.assign(fmt, readInlineStyleFormat(el.style || {}));
  }
  function halfPtToCss(halfPt) {
    return (halfPt / 2) + "pt";
  }

  // The face (PostScript name) first, so a machine that matches fonts by it draws the variant; the font name after it.
  function familyToCss(fmt) {
    return [fmt.face, fmt.font, fmt.eastfont].filter(Boolean).map((name) => '"' + name + '"').join(", ");
  }

  function spacingToCss(twips) {
    return twips ? String(Math.round((twips / TWIPS_PER_PX) * 1000) / 1000) + "px" : "";
  }

  // Screen approximation of the advanced settings (PD-372). A script keeps `vertical-align`; otherwise the raise or lowering
  // is a length. The width scale is `font-stretch` (limited to 50-200 %, and only fonts with width variants change), and a
  // kerning threshold turns kerning on (the size threshold has no CSS counterpart).
  function advancedCss(fmt) {
    return {
      verticalAlign: fmt.superscript ? "super" : fmt.subscript ? "sub" : fmt.position ? halfPtToCss(fmt.position) : "",
      fontStretch: fmt.scale ? `${Math.min(200, Math.max(50, fmt.scale))}%` : "",
      fontKerning: fmt.kerning ? "normal" : "",
      ...host.__EASYOFFICE_DOC_OPEN_TYPE__?.css(fmt.opentype || 0),
    };
  }

  // CSSOM property values mirroring a (validated) format; empty string = property not set.
  function styleFor(fmt) {
    const deco = [fmt.underline ? "underline" : "", fmt.strike ? "line-through" : ""].filter(Boolean).join(" ");
    return {
      fontSize: fmt.size ? halfPtToCss(fmt.size) : "",
      fontFamily: familyToCss(fmt),
      color: fmt.color ? "#" + fmt.color : "",
      backgroundColor: fmt.highlight ? "#" + fmt.highlight : "",
      letterSpacing: spacingToCss(fmt.spacing),
      fontWeight: fmt.bold ? "bold" : "",
      fontStyle: fmt.italic ? "italic" : "",
      textDecoration: deco,
      ...advancedCss(fmt),
      ...Object.fromEntries(Object.entries(host.__EASYOFFICE_DOC_TEXT_FX__?.css(fmt) || {}).filter(([, v]) => v)),
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
    const ds = span.dataset;
    for (let i = 0; i < PROPS.length; i += 1) {
      const p = PROPS[i];
      const key = DATA_KEY[p];
      const val = fmt[p];
      if (val !== undefined && val !== null && val !== false) ds[key] = String(val);
      else delete ds[key];
    }
    const st = span.style;
    st.fontSize = fmt.size ? halfPtToCss(fmt.size) : "";
    st.fontFamily = familyToCss(fmt);
    st.color = fmt.color ? "#" + fmt.color : "";
    st.backgroundColor = fmt.highlight ? "#" + fmt.highlight : "";
    st.letterSpacing = spacingToCss(fmt.spacing);
    st.fontWeight = fmt.bold ? "bold" : "";
    st.fontStyle = fmt.italic ? "italic" : "";
    if (fmt.underline && fmt.strike) st.textDecoration = "underline line-through";
    else if (fmt.underline) st.textDecoration = "underline";
    else if (fmt.strike) st.textDecoration = "line-through";
    else st.textDecoration = "";
    const advanced = advancedCss(fmt);
    st.verticalAlign = advanced.verticalAlign;
    st.fontStretch = advanced.fontStretch;
    st.fontKerning = advanced.fontKerning;
    st.fontVariantLigatures = advanced.fontVariantLigatures || "";
    st.fontVariantNumeric = advanced.fontVariantNumeric || "";
    st.fontFeatureSettings = advanced.fontFeatureSettings || "";
    // The text effects (PD-375) set their keys either way, so a run that lost one clears it; the plain decoration stays unless an effect owns it.
    for (const [key, value] of Object.entries(host.__EASYOFFICE_DOC_TEXT_FX__?.css(fmt) || {})) {
      if (key !== "textDecoration" || value) st[key] = value;
    }
  }

  // Format of a structured-payload / decoded model run ({sizeHalfPt, font, color, highlight,
  // bold, italic, underline, strike, script}); invalid values are dropped.
  function fromModelRun(run) {
    const patch = normalizePatch({
      size: run?.sizeHalfPt, spacing: run?.letterSpacingTwips, scale: run?.scalePercent, position: run?.positionHalfPt, kerning: run?.kerningHalfPt, opentype: run?.openType, nogrid: run?.snapToGrid === false ? 1 : undefined, textfx: run?.textFx, ucolor: run?.underlineColor, font: run?.font, eastfont: run?.fontEastAsia, face: run?.fontFace, color: run?.color, highlight: run?.highlight,
      bold: run?.bold === true ? true : undefined, italic: run?.italic === true ? true : undefined,
      underline: run?.underline === true ? true : undefined, strike: run?.strike === true ? true : undefined,
      superscript: run?.superscript === true || run?.script === "superscript" ? true : undefined,
      subscript: run?.subscript === true || run?.script === "subscript" ? true : undefined,
    });
    const fmt = {};
    PROPS.forEach((p) => {
      const value = patch?.[p];
      if (value !== null && value !== undefined) fmt[p] = value;
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

  function runFlags(fmt) {
    return (
      (fmt.bold ? 1 : 0) |
      (fmt.italic ? 2 : 0) |
      (fmt.underline ? 4 : 0) |
      (fmt.strike ? 8 : 0) |
      (fmt.superscript || fmt.script === "superscript" ? 16 : 0) |
      (fmt.subscript || fmt.script === "subscript" ? 32 : 0)
    );
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
    if (parent) node.remove();
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

  // A block that holds a list or a table (a cell with a list in it) is a container, not a leaf paragraph.
  function holdsBlocks(el) {
    return hasBlockChild(el) || Array.from(el.childNodes).some((c) => c.nodeType === 1 && STRUCTURAL_TAGS.has((c.tagName || "").toUpperCase()));
  }

  function pathTo(root, node) {
    const path = [];
    let cur = node;
    while (cur && cur !== root) {
      const parent = cur.parentElement;
      if (!parent) break;
      path.push(indexOfChild(parent, cur));
      cur = parent;
    }
    path.reverse();
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
    const path = pathTo(root, scope);
    const walk = (node) => {
      node.childNodes.forEach((child, i) => {
        path.push(i);
        if (child.nodeType === 3) out.push({ node: child, path: path.slice() });
        else if (child.nodeType === 1 && !isSkippedElement(child, extra)) walk(child);
        path.pop();
      });
    };
    walk(scope);
    return out;
  }

  function collectTextNodes(scope, root, extra) {
    const out = [];
    if (isExcluded(scope, root, extra)) return out;
    const walk = (node) => {
      node.childNodes.forEach((child) => {
        if (child.nodeType === 3) out.push(child);
        else if (child.nodeType === 1 && !isSkippedElement(child, extra)) walk(child);
      });
    };
    walk(scope);
    return out;
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
    if (node?.nodeType === 3 && !isExcluded(scope, root, extraExclude)) {
      let acc = 0;
      let hit = -1;
      const walk = (cur) => {
        const children = cur.childNodes;
        for (let i = 0; i < children.length; i += 1) {
          const child = children[i];
          if (child === node) {
            hit = acc + offset;
            return true;
          }
          if (child.nodeType === 3) {
            acc += textLength(child);
          } else if (child.nodeType === 1 && !isSkippedElement(child, extraExclude) && walk(child)) {
            return true;
          }
        }
        return false;
      };
      if (walk(scope)) return hit;
    }
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
      if (after) after.before(node);
      else grand.append(node);
      if (tail.childNodes.length > 0) grand.insertBefore(tail, node.nextSibling ?? null);
      if (parent.childNodes.length === 0) parent.remove();
    }
  }

  function formatOfText(text, root) {
    const container = containerOf(text, root);
    const chain = [];
    let cur = text.parentElement;
    while (cur && cur !== container && cur !== root) {
      chain.push(cur);
      cur = cur.parentElement;
    }
    chain.reverse();
    const fmt = {};
    for (let i = 0; i < chain.length; i += 1) {
      const el = chain[i];
      Object.assign(fmt, isRunElement(el) ? readRunFormat(el) : readLegacyFormat(el));
    }
    return fmt;
  }

  function mergePatch(fmt, patch) {
    const next = { ...fmt };
    // A run with one font keeps it for its East Asian text when only the Latin font changes.
    if (patch.keepEast === true && next.eastfont === undefined && next.font) next.eastfont = next.font;
    PROPS.forEach((p) => {
      if (!Object.hasOwn(patch, p)) return;
      const value = BIT_PROPS.has(p) && patch[p] && typeof patch[p] === "object" ? applyBits(p, fmt[p], patch[p]) : patch[p];
      if (value === null) delete next[p];
      else if (value !== undefined) next[p] = value;
    });
    return next;
  }

  function sameFormat(a, b) {
    return (
      runFlags(a) === runFlags(b) &&
      (a.size || "") === (b.size || "") &&
      (a.spacing || 0) === (b.spacing || 0) &&
      (a.scale || 0) === (b.scale || 0) &&
      (a.position || 0) === (b.position || 0) &&
      (a.kerning || 0) === (b.kerning || 0) &&
      (a.opentype || 0) === (b.opentype || 0) &&
      (a.nogrid || 0) === (b.nogrid || 0) &&
      (a.textfx || 0) === (b.textfx || 0) &&
      (a.ucolor || "") === (b.ucolor || "") &&
      (a.font || "") === (b.font || "") &&
      (a.eastfont || "") === (b.eastfont || "") &&
      (a.face || "") === (b.face || "") &&
      (a.color || "") === (b.color || "") &&
      (a.highlight || "") === (b.highlight || "")
    );
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
    if (!el.parentElement) return;
    Array.from(el.childNodes).forEach((c) => el.before(c));
    el.remove();
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
    let childFmt = isRunElement(child) ? readRunFormat(child) : null;
    while (child) {
      const next = child.nextSibling ?? null;
      const nextFmt = isRunElement(next) ? readRunFormat(next) : null;
      if (childFmt && nextFmt && sameFormat(childFmt, nextFmt)) {
        Array.from(next.childNodes).forEach((c) => child.appendChild(c));
        removeNode(next);
        mergeAdjacentText(child);
        continue;
      }
      child = next;
      childFmt = nextFmt;
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
    text.before(span);
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

  // The text nodes of `range` with the text split at both ends, so every selected character sits in its
  // own nodes, and the one container they share. Null for an empty range or one over several containers.
  function isolateRangeText(root, range, extraExclude) {
    if (!range || !root.contains(range.startContainer) || !root.contains(range.endContainer)) return null;
    const [rawStart, rawEnd] = orderPoints(
      root,
      toPoint(range.startContainer, range.startOffset),
      toPoint(range.endContainer, range.endOffset)
    );
    const scope = containerOf(commonScope(root, rawStart.node, rawEnd.node), root);
    const startOff = textOffsetOf(root, rawStart.node, rawStart.offset, extraExclude, scope);
    const endOff = textOffsetOf(root, rawEnd.node, rawEnd.offset, extraExclude, scope);
    if (startOff === endOff) return null;
    const { start, end } = splitBoundaries(rawStart, rawEnd);
    const texts = selectedTextNodes(root, start, end, extraExclude);
    const containers = new Set(texts.map((t) => containerOf(t, root)));
    return containers.size === 1 ? { texts, container: [...containers][0] } : null;
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
      if (tail) tail.before(fresh);
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
    const path = [];
    const walk = (el) => {
      el.childNodes.forEach((child, i) => {
        if (child.nodeType !== 1 || isSkippedElement(child, extraExclude)) return;
        path.push(i);
        if (BLOCK_TAGS.has((child.tagName || "").toUpperCase()) && !holdsBlocks(child)) {
          const startsBeforeEnd = cmpPathOffset(path, 0, ep, end.offset) <= 0;
          const endsAfterStart = cmpPathOffset(path, child.childNodes.length, sp, start.offset) >= 0;
          if (startsBeforeEnd && endsAfterStart) out.push(child);
        } else {
          walk(child);
        }
        path.pop();
      });
    };
    walk(root);
    if (out.length === 0) {
      const own = blockOf(start.node, root);
      if (own !== root) out.push(own);
    }
    return out;
  }

  host.__EASYOFFICE_DOC_RUNS__ = {
    RUN_CLASS, PROPS, BOOL_PROPS, BOOL_PROP_SET, VALUE_PROPS, DATA_KEY, MIXED, DEFAULT_HALF_PT, MIN_HALF_PT, MAX_HALF_PT,
    normalizeColor, normalizeFont, normalizeProp, normalizePatch, ptToHalfPt, halfPtToCss,
    isRunElement, readRunFormat, readLegacyFormat, createRun, fromModelRun, buildRunSpec, formatOfText, applyFormat, isolateRangeText, queryFormat, wordRangeAt,
    insertFormattedText, blocksInRange, blockOf, containerOf, collectTextNodes, textOffsetOf,
    pointAtTextOffset, normalizeContainer, comparePoints, cssSizeToHalfPt,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
