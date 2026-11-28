(function (host) {
  "use strict";

  // How the paragraph and block features of PD-401 are kept in the editor, and the two directions between that and the
  // engine's document JSON (`convert_structured_blocks.rs`):
  //  - tab stops are the paragraph's `data-eo-tabs` ("8000:right:dot;2000:center:none"), painted by `doc-word-tabs.js`;
  //  - the border is `data-eo-border` ("top=single,6,FF0000,4;bottom=..."), shown by inline border styles, and the shading
  //    is `data-eo-shade` with the background color; both follow `parseBorderProperties` of the DOCX renderer docx-preview;
  //  - a drop cap paragraph has `data-eo-dropcap` ("3,drop"), which floats it left as docx-preview's `parseFrame` does;
  //  - a column break is `data-eo-col-break`, a style of the user `data-eo-style`;
  //  - a text box is `div.eo-textbox` inside the paragraph that holds its anchor, with its own paragraphs;
  //  - an equation is `span.eo-math[data-eo-omml]`: the OMML stays as the file wrote it, and `doc-word-math.js` draws it as
  //    MathML.
  // Open side: `paragraph`, `mathSpec`. Save side: `paragraphKeys`, `mathRun`. The document's styles and column look ride in
  // `eoWordMeta` (`save-payload-doc-meta.js`): `cleanStyles`, `cleanColumns`.

  const MAX_TABS = 64;
  const MAX_TAB_POSITION = 31680;
  const TAB_ALIGNS = ["left", "center", "right", "decimal"];
  const TAB_LEADERS = ["none", "dot", "hyphen", "underscore"];
  const BORDER_STYLES = ["single", "double", "dotted", "dashed"];
  const BORDER_SIDES = ["top", "right", "bottom", "left"];
  const WRAPS = ["square", "topBottom", "behind", "front"];
  const MAX_STYLES = 64;
  const MAX_STYLE_NAME = 64;
  const MAX_TEXT_BOX_EMU = 20116800;
  const MIN_TEXT_BOX_EMU = 36000;
  const HEX6 = /^[0-9A-F]{6}$/;
  const PX_PER_PT = 96 / 72;
  const PX_PER_EMU = 1 / 9525;
  const CSS_BORDER = { single: "solid", double: "double", dotted: "dotted", dashed: "dashed" };

  const int = (v) => Number.isInteger(v);
  const hex = (v) => (typeof v === "string" && HEX6.test(v.toUpperCase()) ? v.toUpperCase() : "");

  // ---- tab stops: "pos:align:leader;..."

  function decodeTabs(text) {
    const out = [];
    for (const part of String(text ?? "").split(";")) {
      const [pos, align, leader] = part.split(":");
      const p = Number(pos);
      if (!/^\d+$/.test(pos ?? "") || p > MAX_TAB_POSITION || !TAB_ALIGNS.includes(align) || !TAB_LEADERS.includes(leader)) continue;
      if (out.some((t) => t.pos === p)) continue;
      out.push({ pos: p, align, leader });
      if (out.length >= MAX_TABS) break;
    }
    return out.sort((a, b) => a.pos - b.pos);
  }

  const encodeTabs = (stops) => stops.map((t) => t.pos + ":" + t.align + ":" + t.leader).join(";");

  // ---- borders: "side=style,eighths,color|-,space;..."

  function decodeBorders(text) {
    const out = {};
    for (const part of String(text ?? "").split(";")) {
      const [side, rest] = part.split("=");
      const [style, eighths, color, space] = String(rest ?? "").split(",");
      const e = Number(eighths);
      const s = Number(space);
      if (!BORDER_SIDES.includes(side) || !BORDER_STYLES.includes(style) || !int(e) || e < 2 || e > 96 || !int(s) || s < 0 || s > 31) continue;
      if (color !== "-" && !hex(color)) continue;
      out[side] = { style, eighths: e, color: color === "-" ? "" : color, space: s };
    }
    return out;
  }

  const encodeBorders = (b) =>
    BORDER_SIDES.filter((side) => b[side])
      .map((side) => side + "=" + [b[side].style, b[side].eighths, b[side].color || "-", b[side].space].join(","))
      .join(";");

  // The CSS the border of a side shows as (docx-preview: width in points, the line style, the color or black).
  function borderCss(line) {
    const width = Math.max(line.style === "double" ? 3 : 1, Math.round((line.eighths / 8) * PX_PER_PT));
    return width + "px " + CSS_BORDER[line.style] + " #" + (line.color || "000000");
  }

  // Puts the border, the shading, the tab stops and the other data of a paragraph on `el` (an element or a spec's style and
  // data holders: `target.style` and `target.data`/`dataset` are assigned through the two small adapters below).
  function showLook(style, data, look) {
    for (const side of BORDER_SIDES) style["border" + side[0].toUpperCase() + side.slice(1)] = look.borders?.[side] ? borderCss(look.borders[side]) : "";
    const space = BORDER_SIDES.map((s) => look.borders?.[s]?.space ?? 0).reduce((a, b) => Math.max(a, b), 0);
    style.padding = look.borders && Object.keys(look.borders).length > 0 ? Math.round(space * PX_PER_PT) + "px" : "";
    style.backgroundColor = look.shade ? "#" + look.shade : "";
    const set = (key, value) => {
      if (value) data[key] = value;
      else delete data[key];
    };
    set("eoBorder", look.borders && Object.keys(look.borders).length > 0 ? encodeBorders(look.borders) : "");
    set("eoShade", look.shade);
    set("eoTabs", look.tabs?.length > 0 ? encodeTabs(look.tabs) : "");
  }

  // ---- open: the decoded paragraph to a spec

  const el = (tag, attrs, data, children, style) => ({ t: "el", tag, attrs: attrs || {}, data: data || {}, style: style || {}, children: children || [] });

  function textBoxSpec(box, buildNodes) {
    const w = box.widthEmu;
    const h = box.heightEmu;
    const wrap = WRAPS.includes(box.wrap) ? box.wrap : "square";
    if (!int(w) || !int(h) || w < MIN_TEXT_BOX_EMU || h < MIN_TEXT_BOX_EMU || w > MAX_TEXT_BOX_EMU || h > MAX_TEXT_BOX_EMU) return null;
    const children = buildNodes(Array.isArray(box.paragraphs) ? box.paragraphs : []);
    if (children.length === 0) children.push(el("p", {}, {}, [el("br")]));
    const style = { width: Math.round(w * PX_PER_EMU) + "px", minHeight: "", height: Math.round(h * PX_PER_EMU) + "px" };
    const fill = hex(box.fill);
    if (fill) style.backgroundColor = "#" + fill;
    const data = { eoTextbox: "1", eoTbWrap: wrap, eoTbW: String(w), eoTbH: String(h) };
    if (fill) data.eoTbFill = fill;
    if (box.border === true) data.eoTbBorder = "1";
    return el("div", { class: "eo-textbox", contenteditable: "true" }, data, children, style);
  }

  // The look of a decoded paragraph on its spec; `buildNodes` makes the specs of the paragraphs of a text box.
  function paragraph(spec, block, buildNodes) {
    spec.style ??= {};
    spec.data ??= {};
    spec.children ??= [];
    const tabs = Array.isArray(block.tabs) ? decodeTabs(block.tabs.map((t) => t?.pos + ":" + t?.align + ":" + t?.leader).join(";")) : [];
    const borders = {};
    for (const side of BORDER_SIDES) {
      const line = block.border?.[side];
      if (line) Object.assign(borders, decodeBorders(side + "=" + [line.style, line.eighths, line.color || "-", line.space ?? 0].join(",")));
    }
    showLook(spec.style, spec.data, { tabs, borders, shade: hex(block.shade) });
    const cap = block.dropCap;
    if (cap && int(cap.lines) && cap.lines >= 2 && cap.lines <= 10) spec.data.eoDropcap = cap.lines + "," + (cap.margin === true ? "margin" : "drop");
    if (block.columnBreak === true) spec.data.eoColBreak = "1";
    if (typeof block.styleName === "string" && block.styleName && block.styleName.length <= MAX_STYLE_NAME) spec.data.eoStyle = block.styleName;
    const look = block.columnLook;
    if (look && (look.separator === true || look.narrow === "first" || look.narrow === "last")) spec.data.eoColLook = (look.separator === true ? 1 : 0) + "," + (look.narrow ?? "none");
    for (const box of Array.isArray(block.textBoxes) ? block.textBoxes.slice(0, 256) : []) {
      const boxSpec = buildNodes ? textBoxSpec(box, buildNodes) : null;
      if (boxSpec) spec.children.push(boxSpec);
    }
    return spec;
  }

  // The inline span of an equation: it holds the OMML and its linear text; `doc-word-math.js` adds the MathML.
  const mathSpec = (run) => el("span", { class: "eo-math", contenteditable: "false" }, { eoOmml: run.math, eoMathText: String(run.text ?? "") });

  // ---- save: the editor to the payload

  // The run an equation stands for.
  function mathRun(node) {
    const omml = node.dataset?.eoOmml;
    if (typeof omml !== "string" || !omml.startsWith("<m:oMath")) return null;
    return { text: String(node.dataset.eoMathText ?? ""), math: omml, _keepEmpty: true };
  }

  // A text box element as the payload's `textBoxes` entry; `collect` reads the paragraphs inside (`collectDocBlocks`).
  function textBoxEntry(box, collect) {
    const d = box.dataset;
    const w = Number(d.eoTbW);
    const h = Number(d.eoTbH);
    if (!int(w) || !int(h) || w < MIN_TEXT_BOX_EMU || h < MIN_TEXT_BOX_EMU || w > MAX_TEXT_BOX_EMU || h > MAX_TEXT_BOX_EMU) return null;
    const entry = { widthEmu: w, heightEmu: h, wrap: WRAPS.includes(d.eoTbWrap) ? d.eoTbWrap : "square", border: d.eoTbBorder === "1" };
    if (hex(d.eoTbFill)) entry.fill = hex(d.eoTbFill);
    entry.paragraphs = collect(box)
      .filter((b) => b.type === "paragraph")
      .slice(0, 64)
      .map(({ textBoxes: _boxes, columns: _columns, ...rest }) => rest);
    return entry;
  }

  // The payload keys of PD-401 that a paragraph element carries.
  function paragraphKeys(node, collect) {
    const d = node.dataset || {};
    const out = {};
    const tabs = decodeTabs(d.eoTabs);
    if (tabs.length > 0) out.tabs = tabs;
    const borders = decodeBorders(d.eoBorder);
    if (Object.keys(borders).length > 0) {
      out.border = Object.fromEntries(Object.entries(borders).map(([side, l]) => [side, { style: l.style, eighths: l.eighths, space: l.space, ...(l.color ? { color: l.color } : {}) }]));
    }
    if (hex(d.eoShade)) out.shade = hex(d.eoShade);
    const cap = /^(\d+),(drop|margin)$/.exec(d.eoDropcap ?? "");
    if (cap && Number(cap[1]) >= 2 && Number(cap[1]) <= 10) out.dropCap = { lines: Number(cap[1]), margin: cap[2] === "margin" };
    if (d.eoColBreak === "1") out.columnBreak = true;
    if (d.eoStyle && d.eoStyle.length <= MAX_STYLE_NAME) out.styleName = d.eoStyle;
    const look = /^([01]),(none|first|last)$/.exec(d.eoColLook ?? "");
    if (look && (look[1] === "1" || look[2] !== "none")) out.columnLook = { separator: look[1] === "1", narrow: look[2] };
    if (collect) {
      const boxes = Array.from(node.children || []).filter((c) => c.dataset?.eoTextbox === "1").map((b) => textBoxEntry(b, collect)).filter(Boolean);
      if (boxes.length > 0) out.textBoxes = boxes.slice(0, 256);
    }
    return out;
  }

  // ---- the document settings in `eoWordMeta`

  // The names the word processor gives its own styles; the engine refuses them for a style of the user.
  // The same rule as `is_reserved_style_name` and `heading_level_from_name` of the engine (`rich_word_blocks.rs`, `rich_xml.rs`):
  // the names compare without regard to ASCII case, a heading is `Title` or `heading`, `outline`, `제목`, `개요` and a number up
  // to 255 with white space, `_` or `-` between and around them, and `TOC` starts the table-of-contents styles.
  const RESERVED = ["표준", "본문", "normal", "caption", "hyperlink", "list bullet", "list number", "table normal", "header", "footer", "default paragraph font"];
  const asciiLower = (s) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());
  const SEPARATORS = /^[\s_-]+|[\s_-]+$/g;
  function isHeadingName(name) {
    const s = String(name).replace(SEPARATORS, "");
    if (asciiLower(s) === "title") return true;
    const m = /^(?:heading|outline|제목|개요)[\s_-]*\+?(\d+)$/i.exec(s);
    return Boolean(m) && Number(m[1]) <= 255;
  }
  const isReservedStyleName = (name) => RESERVED.includes(asciiLower(String(name))) || isHeadingName(name) || asciiLower(String(name)).startsWith("toc");

  const RUN_KEYS = ["font", "color", "align"];
  const LINE_KINDS = ["atLeast", "exact", "between"];
  function cleanStyles(raw) {
    const out = [];
    for (const s of Array.isArray(raw) ? raw : []) {
      // A name with white space around it is taken without it, as the engine does.
      const name = String(s?.name ?? "").trim();
      if (!name || Array.from(name).length > MAX_STYLE_NAME || /[\u0000-\u001f\u007f-\u009f]/.test(name) || isReservedStyleName(name) || out.some((o) => o.name === name)) continue;
      const style = { name };
      for (const key of RUN_KEYS) if (typeof s[key] === "string" && s[key]) style[key] = s[key].slice(0, 64);
      if (style.color && !hex(style.color)) delete style.color;
      if (!["left", "center", "right", "justify"].includes(style.align)) delete style.align;
      for (const [key, lo, hi] of [["sizeHalfPt", 2, 1638], ["leftIndentTwips", 0, 288000], ["firstLineIndentTwips", -288000, 288000], ["spaceBeforeTwips", 0, 8000], ["spaceAfterTwips", 0, 8000], ["lineSpacingPercent", 1, 1000]]) {
        if (int(s[key]) && s[key] >= lo && s[key] <= hi) style[key] = s[key];
      }
      // The other line spacing rules (at least, exactly, between lines) name their kind and give twips; a multiple has no kind.
      if (LINE_KINDS.includes(s.lineSpacingKind) && int(s.lineSpacingTwips) && s.lineSpacingTwips >= 1 && s.lineSpacingTwips <= 31680) {
        delete style.lineSpacingPercent;
        style.lineSpacingKind = s.lineSpacingKind;
        style.lineSpacingTwips = s.lineSpacingTwips;
      }
      for (const key of ["bold", "italic", "underline"]) if (s[key] === true) style[key] = true;
      out.push(style);
      if (out.length >= MAX_STYLES) break;
    }
    return out;
  }

  function cleanColumns(raw) {
    if (!raw || typeof raw !== "object") return null;
    const gap = int(raw.gap) && raw.gap >= 0 && raw.gap <= 5000 ? raw.gap : 720;
    const narrow = raw.narrow === "first" || raw.narrow === "last" ? raw.narrow : "none";
    const sep = raw.sep === true;
    return gap === 720 && !sep && narrow === "none" ? null : { gap, sep, narrow };
  }

  host.__EASYOFFICE_DOC_WORD_BLOCKS__ = {
    TAB_ALIGNS, TAB_LEADERS, BORDER_STYLES, BORDER_SIDES, decodeTabs, encodeTabs, decodeBorders, encodeBorders, borderCss, showLook,
    paragraph, mathSpec, mathRun, textBoxEntry, paragraphKeys, cleanStyles, cleanColumns, isReservedStyleName,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
