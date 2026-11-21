// Cells from the HTML a spreadsheet program puts on the clipboard (a table with classes, a style
// sheet and per-cell attributes): text, number, text-typed marks, merged ranges, fonts, colours,
// alignment, wrapping, borders, number formats and column widths. Pure logic over a DOM tree that the
// caller parses without running it; this file writes no markup anywhere.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 4767b3b309467c283bbd52b725536ca73861e0fc):
//   packages/sheets-ui/src/services/clipboard/utils.ts :97-118 (`htmlIsFromExcel`: the markers of
//       a spreadsheet program's clipboard HTML)
//   packages/sheets-ui/src/services/clipboard/html-to-usm/converter.ts
//       :292-322 (`_parseMsoNumfmtFromCssText`: number formats are read from the raw style text, a
//       style sheet object drops the proprietary property), :324-334 (`_getMsoNumfmtForNode`: class, then id,
//       then tag), :360-413 (`_getStyle`: inline style over id over class over tag over the parent),
//       :483-603 (`_parseTableByHtml`: rows and cells, the column a cell starts in skips the cells a
//       row span covers, the borders two neighbours share are kept once, a merged cell's style goes
//       to every cell it covers), :938-971 (`parseColGroup`), :992-999 (`normalizePlainCellText`),
//       :1001-1023 (`isIgnoredColSpan`: `mso-ignore:colspan` is text overflow, not a merge), :1035-1075
//       (`setMergedCellStyle`), :1077-1097 (`decodeMsoNumberFormat`, `cleanMsoSpaceRun`).
// Modifications: the tree comes from an inert document (`DOMParser`), the style sheet is read by a
// text scanner of its own because an inert document has no style sheet object, cells are plain text
// (the sheet has no rich text) so a break becomes a line feed, the style is the sheet's cell style
// (`styleFromPayload`) instead of the reference's style data, a number comes from the program's own
// attribute (`x:num`, `x:str`, `x:bool`, `x:err`) before the displayed text, a formula attribute is not
// carried (its references belong to the copied place, which the clipboard does not tell), and every
// size is bounded: the HTML, the style sheet, the rules, the declarations, a cell's text, the cell
// count and the span of a merge. A block over a bound is refused whole (the caller pastes the text).
(function (host) {
  "use strict";

  const LIMITS = Object.freeze({
    HTML_CHARS: 8 * 1024 * 1024,
    MARKER_CHARS: 64 * 1024, // the start of the HTML the markers are looked for in
    STYLE_CHARS: 512 * 1024, // all style elements together
    STYLE_ELEMENTS: 16,
    RULES: 5000,
    DECLARATIONS: 64, // properties one rule or one element holds
    VALUE_CHARS: 256, // a declaration value, an attribute value
    SELECTOR_CHARS: 256,
    CLASS_ATTR_CHARS: 1024,
    STYLE_ATTR_CHARS: 65536,
    CELLS: 1000000, // cells of the block, covered cells of a merge included
    CELL_CHARS: 32767, // what a cell holds
    NUMBER_FORMATS: 64, // distinct formats one paste brings
  });
  const DEFAULT_MAX_MERGES = 100000; // the sheet layout's own bound, when the layout is not loaded
  const MAX_ROWS = 1048576;
  const MAX_COLS = 16384;
  const PX_PER_PT = 4 / 3;
  const CELL_TAGS = new Set(["TD", "TH"]);
  const SECTION_TAGS = new Set(["THEAD", "TBODY", "TFOOT"]);
  const SKIPPED_TAGS = new Set(["STYLE", "SCRIPT", "TEMPLATE", "NOSCRIPT", "IFRAME", "OBJECT", "EMBED"]);
  const BLOCK_TAGS = new Set(["DIV", "P", "LI", "TR", "H1", "H2", "H3", "H4", "H5", "H6"]);
  const SIDES = ["t", "r", "b", "l"];
  const SIDE_PROP = { t: "border-top", r: "border-right", b: "border-bottom", l: "border-left" };
  // What a spreadsheet program's clipboard HTML carries near its start, in lower case: its XML namespaces, the ProgId value,
  // the workbook element and the prefix of its style properties (`scripts/oss-names.mjs` lists them as identifiers).
  const MARKERS = [
    "urn:schemas-microsoft-com:office:excel",
    "urn:schemas-microsoft-com:office:office",
    "excel.sheet",
    "excelworkbook",
    "mso-excel",
    "@mso-",
  ];
  // The class of a cell (`class=xl65`, quoted or not); run on the first 64 KiB only and closed at both ends.
  const CELL_CLASS = /\sclass\s*=\s*["']?xl\d/;

  /** Whether `html` is what a spreadsheet program copies (an app's own copy never is). Looks at the first 64 KiB only. */
  function htmlIsFromSpreadsheet(html) {
    if (!html) return false;
    const head = html.slice(0, LIMITS.MARKER_CHARS).toLowerCase();
    return MARKERS.some((marker) => head.includes(marker)) || CELL_CLASS.test(head);
  }

  // ---- the style text ----------------------------------------------------------------------------------

  // Splits `text` at the semicolons outside quotes and outside a backslash escape (a number format holds `\;`).
  function splitDeclarations(text) {
    const parts = [];
    let quote = "";
    let from = 0;
    for (let i = 0; i < text.length; i += 1) {
      const ch = text[i];
      if (ch === "\\") i += 1;
      else if (quote) {
        if (ch === quote) quote = "";
      } else if (ch === '"' || ch === "'") quote = ch;
      else if (ch === ";") {
        parts.push(text.slice(from, i));
        from = i + 1;
      }
    }
    parts.push(text.slice(from));
    return parts;
  }

  // `{ property: value }` of a declaration list; later declarations win. A prototype-less object: a property named
  // `__proto__` is just a key. A value over 256 characters is dropped, more than 64 properties refuse the whole block.
  function parseDeclarations(text) {
    const out = Object.create(null);
    let count = 0;
    for (const part of splitDeclarations(text)) {
      const at = part.indexOf(":");
      if (at <= 0) continue;
      const name = part.slice(0, at).trim().toLowerCase();
      if (name === "") continue;
      if (!(name in out) && (count += 1) > LIMITS.DECLARATIONS) throw new RangeError("style");
      const value = part.slice(at + 1).trim();
      if (value === "" || value.length > LIMITS.VALUE_CHARS) continue;
      out[name] = value.replace(/!important$/i, "").trim();
    }
    return out;
  }

  // `text` without its comments, in one pass; a comment that never ends takes the rest.
  function withoutComments(text) {
    const pieces = [];
    let pos = 0;
    for (;;) {
      const open = text.indexOf("/*", pos);
      if (open < 0) break;
      pieces.push(text.slice(pos, open));
      const close = text.indexOf("*/", open + 2);
      if (close < 0) return pieces.join("");
      pos = close + 2;
    }
    pieces.push(text.slice(pos));
    return pieces.join("");
  }

  // The position after the block that opens at `open` (a `{`) with nested blocks, or the end of the text.
  function afterBlock(text, open) {
    let depth = 0;
    for (let i = open; i < text.length; i += 1) {
      const ch = text.charCodeAt(i);
      if (ch === 123) depth += 1;
      else if (ch === 125 && (depth -= 1) === 0) return i + 1;
    }
    return text.length;
  }

  // The rules of the style sheets as `selector -> declarations` (a rule with a selector list is one rule per selector).
  // A single pass over the text: comments and at-rules are skipped, and the raw text is kept because a style sheet object would
  // drop the properties of the program (`mso-number-format`). Over the bounds (elements, characters, rules, properties of a
  // rule) the block is refused whole.
  function readStyleRules(sheets) {
    if (sheets.length > LIMITS.STYLE_ELEMENTS) throw new RangeError("style");
    const rules = new Map();
    let count = 0;
    let chars = 0;
    for (const raw of sheets) {
      if ((chars += raw.length) > LIMITS.STYLE_CHARS) throw new RangeError("style");
      const text = withoutComments(raw).split("<!--").join("").split("-->").join("");
      let pos = 0;
      while (pos < text.length) {
        const open = text.indexOf("{", pos);
        if (open < 0) break;
        const before = text.slice(pos, open);
        const selectors = before.slice(before.lastIndexOf("}") + 1).trim();
        if (selectors.startsWith("@")) {
          pos = afterBlock(text, open);
          continue;
        }
        const close = text.indexOf("}", open + 1);
        if (close < 0) break;
        const body = text.slice(open + 1, close);
        pos = close + 1;
        if (selectors === "" || selectors.length > LIMITS.SELECTOR_CHARS) continue;
        const declarations = parseDeclarations(body);
        for (const selector of selectors.split(",")) {
          const key = selector.trim().toLowerCase();
          if (key === "") continue;
          if ((count += 1) > LIMITS.RULES) throw new RangeError("style");
          rules.set(key, Object.assign(rules.get(key) || Object.create(null), declarations));
        }
      }
    }
    return rules;
  }

  const RESOLVED_CELLS = 50000; // looks one parse remembers
  // An attribute value; null when it is absent or longer than its bound (256 characters; the class list 1,024; the style 64 Ki).
  function attrOf(el, name) {
    const value = typeof el.getAttribute === "function" ? el.getAttribute(name) : null;
    if (value === null || value === undefined) return null;
    const bound = name === "style" ? LIMITS.STYLE_ATTR_CHARS : name === "class" ? LIMITS.CLASS_ATTR_CHARS : LIMITS.VALUE_CHARS;
    return value.length > bound ? null : value;
  }
  const elementKey = (el) => `${el.tagName}|${attrOf(el, "class") ?? ""}|${attrOf(el, "id") ?? ""}|${attrOf(el, "style") ?? ""}`;
  const classesOf = (el) => (attrOf(el, "class") || "").split(/\s+/).filter(Boolean);

  // The declarations of an element: the parent's, then the tag's rule, `tag.class` and `.class`, the `#id` rule, and the
  // element's own style attribute last.
  function declarationsOf(el, rules, parent) {
    const out = Object.assign(Object.create(null), parent);
    const tag = String(el.tagName || "").toLowerCase();
    const classes = classesOf(el).map((c) => c.toLowerCase());
    const id = (attrOf(el, "id") || "").toLowerCase();
    const sources = [tag, ...classes.map((c) => tag + "." + c), ...classes.map((c) => "." + c), id ? "#" + id : ""];
    for (const key of sources) if (key && rules.has(key)) Object.assign(out, rules.get(key));
    Object.assign(out, parseDeclarations(attrOf(el, "style") || ""));
    return out;
  }

  // ---- values of the style ---------------------------------------------------------------------------

  const NAMED_COLORS = Object.freeze({
    black: "000000", white: "ffffff", red: "ff0000", lime: "00ff00", blue: "0000ff", yellow: "ffff00", aqua: "00ffff",
    cyan: "00ffff", fuchsia: "ff00ff", magenta: "ff00ff", silver: "c0c0c0", gray: "808080", grey: "808080", maroon: "800000",
    olive: "808000", green: "008000", purple: "800080", teal: "008080", navy: "000080", orange: "ffa500",
  });
  const hex2 = (n) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");

  /** `rrggbb` of a CSS colour; null for the automatic colour, transparent, and anything unknown. */
  function colorOf(text) {
    const t = String(text ?? "").trim().toLowerCase();
    if (t === "" || t === "windowtext" || t === "window" || t === "transparent" || t === "auto" || t === "none" || t === "inherit") return null;
    if (NAMED_COLORS[t]) return NAMED_COLORS[t];
    const hex = /^#([0-9a-f]{6}|[0-9a-f]{3})$/.exec(t);
    if (hex) return hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join("") : hex[1];
    const rgb = /^rgba?\(\s*(\d+(?:\.\d+)?)[\s,]+(\d+(?:\.\d+)?)[\s,]+(\d+(?:\.\d+)?)/.exec(t);
    return rgb ? hex2(rgb[1]) + hex2(rgb[2]) + hex2(rgb[3]) : null;
  }

  // Length in px; null when the text is not a length.
  function pxOf(text) {
    const m = /^(\d+(?:\.\d+)?|\.\d+)\s*(px|pt|in|cm|mm|pc)?$/i.exec(String(text ?? "").trim());
    if (!m) return null;
    const n = Number(m[1]);
    const unit = (m[2] || "px").toLowerCase();
    const per = { px: 1, pt: PX_PER_PT, in: 96, cm: 96 / 2.54, mm: 96 / 25.4, pc: 16 }[unit];
    return n * per;
  }

  // The words of a value: split at white space outside parentheses, one linear pass.
  function tokensOf(text) {
    const tokens = [];
    let depth = 0;
    let start = -1;
    for (let i = 0; i < text.length; i += 1) {
      const ch = text.charCodeAt(i);
      if (ch === 40) depth += 1;
      else if (ch === 41 && depth > 0) depth -= 1;
      const blank = depth === 0 && (ch === 32 || (ch >= 9 && ch <= 13));
      if (blank) {
        if (start >= 0) tokens.push(text.slice(start, i));
        start = -1;
      } else if (start < 0) start = i;
    }
    if (start >= 0) tokens.push(text.slice(start));
    return tokens;
  }

  // The edge text of a border value (`.5pt solid windowtext`): `"<line>:<rrggbb>"`, or null for no border.
  function edgeOf(value) {
    const text = String(value ?? "").trim().toLowerCase();
    if (text === "" || text === "none") return null;
    let px = 1;
    let line = "thin";
    let color = "000000";
    for (const token of tokensOf(text)) {
      const width = { thin: 1, medium: 3, thick: 5, hairline: 1 }[token] ?? pxOf(token);
      if (width !== null && width !== undefined) px = width;
      else if (token === "none" || token === "hidden") return null;
      else if (token === "dashed" || token === "dotted" || token === "double") line = token;
      else if (token === "solid") line = "thin";
      else color = colorOf(token) ?? color;
    }
    if (px === 0) return null;
    if (line === "thin" && px >= 2) line = "thick";
    return line + ":" + color;
  }

  const FONT_SIZE_PT = { "xx-small": 7, "x-small": 7.5, small: 10, medium: 12, large: 13.5, "x-large": 18, "xx-large": 24 };
  const MIN_SIZE_PT = 1;
  const MAX_SIZE_PT = 409;

  function sizePtOf(value) {
    const named = FONT_SIZE_PT[String(value).trim().toLowerCase()];
    const px = named ? null : pxOf(value);
    const pt = named ?? (px === null ? null : px / PX_PER_PT);
    return pt !== null && pt >= MIN_SIZE_PT && pt <= MAX_SIZE_PT ? Math.round(pt * 100) / 100 : null;
  }

  // The first family of a font-family list, without its quotes.
  const familyOf = (value) => String(value).split(",")[0].trim().replace(/^["']|["']$/g, "").trim();

  // The sheet's save-payload style (`styleFromPayload` reads it) of the declarations of a cell; `bd` holds the edges.
  function payloadOf(decl) {
    const out = {};
    const weight = decl["font-weight"];
    if (weight === "bold" || weight === "bolder" || Number(weight) >= 600) out.bold = true;
    if (decl["font-style"] === "italic" || decl["font-style"] === "oblique") out.italic = true;
    const lines = `${decl["text-decoration"] ?? ""} ${decl["text-decoration-line"] ?? ""}`;
    if (/underline/i.test(lines)) out.underline = true;
    if (/line-through/i.test(lines)) out.strike = true;
    const size = decl["font-size"] ? sizePtOf(decl["font-size"]) : null;
    if (size !== null) out.sizePt = size;
    const family = decl["font-family"] ? familyOf(decl["font-family"]) : "";
    if (family) out.font = family;
    const fontColor = colorOf(decl.color);
    if (fontColor) out.fc = fontColor;
    const fill = colorOf(decl["background-color"] ?? decl.background);
    if (fill) out.fill = fill;
    const align = String(decl["text-align"] ?? "").toLowerCase();
    if (align === "left" || align === "center" || align === "right") out.ha = align;
    const valign = String(decl["vertical-align"] ?? "").toLowerCase();
    if (valign === "top" || valign === "middle") out.va = valign;
    if (/^(normal|pre-wrap|pre-line|break-spaces)$/i.test(String(decl["white-space"] ?? ""))) out.wrap = true;
    const edges = {};
    for (const side of SIDES) {
      const edge = edgeOf(decl[SIDE_PROP[side]] ?? decl.border);
      if (edge) edges[side] = edge;
    }
    if (Object.keys(edges).length > 0) out.bd = edges;
    return out;
  }

  // Number formats ---------------------------------------------------------------------------------------

  /** The format code of an `mso-number-format` value: escapes undone (`\0022` is a quote, `\\` a backslash). */
  function decodeMsoNumberFormat(value) {
    let result = String(value).replace(/\\([0-9a-fA-F]{4})/g, (_, hex) => String.fromCharCode(Number.parseInt(hex, 16)));
    result = result.replace(/\\\\/g, "\u0000BACKSLASH\u0000");
    result = result.replace(/\\([^\\])/g, "$1");
    return result.replace(/\u0000BACKSLASH\u0000/g, "\\");
  }

  // The names the program gives to its built-in formats.
  const NAMED_FORMATS = Object.freeze({ general: "", percent: "0%", fixed: "0.00", standard: "#,##0.00", scientific: "0.00E+00" });

  function numFmtOf(decl) {
    const raw = decl["mso-number-format"];
    if (!raw) return "";
    const unquoted = raw.startsWith('"') && raw.endsWith('"') && raw.length > 1 ? raw.slice(1, -1) : raw;
    const named = NAMED_FORMATS[unquoted.trim().toLowerCase()];
    const code = named !== undefined ? named : decodeMsoNumberFormat(unquoted);
    const fmt = host.__EASYOFFICE_SHEET_NUMFMT__;
    if (code === "" || fmt?.isDefaultFormat?.(code) || (fmt && !fmt.codeWithinBounds(code))) return "";
    return code;
  }

  // ---- cells -----------------------------------------------------------------------------------------

  const isSpaceRun = (el) => /mso-spacerun\s*:\s*yes/i.test(attrOf(el, "style") || "");

  // The text of a cell: text nodes with the white space of HTML collapsed, a break as a line feed, a block
  // boundary as a line feed between two lines, the no-break space as a space. Iterative: a deep tree does not deepen the stack.
  function textOf(cell) {
    let out = "";
    const stack = [[cell, 0]];
    while (stack.length > 0 && out.length <= LIMITS.CELL_CHARS) {
      const frame = stack[stack.length - 1];
      const children = frame[0].childNodes || [];
      if (frame[1] >= children.length) {
        stack.pop();
        continue;
      }
      const node = children[frame[1]];
      frame[1] += 1;
      if (node.nodeType === 3) {
        out += String(node.nodeValue ?? node.textContent ?? "").replace(/[\t\r\n\f ]+/g, " ").replace(/ /g, " ");
      } else if (node.nodeType === 1) {
        const tag = String(node.tagName).toUpperCase();
        if (SKIPPED_TAGS.has(tag) || (tag === "SPAN" && isSpaceRun(node))) continue;
        if (tag === "BR") out += "\n";
        else if (BLOCK_TAGS.has(tag) && out !== "" && !out.endsWith("\n")) out += "\n";
        stack.push([node, 0]);
      }
    }
    return out.slice(0, LIMITS.CELL_CHARS).trim();
  }

  const NUMBER_RE = /^-?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;

  // The value of a cell from the program's own attributes, else its text. `{ value, t }`.
  function valueOf(cell) {
    const num = attrOf(cell, "x:num");
    if (num !== null && NUMBER_RE.test(num.trim())) return { value: num.trim() };
    const bool = attrOf(cell, "x:bool");
    if (bool !== null && /^(true|false)$/i.test(bool.trim())) return { value: bool.trim().toUpperCase() };
    const err = attrOf(cell, "x:err");
    if (err !== null && /^#[A-Z0-9/!?]+$/i.test(err.trim())) return { value: err.trim().toUpperCase() };
    const text = textOf(cell);
    return attrOf(cell, "x:str") !== null && text !== "" ? { value: text, t: "s" } : { value: text };
  }

  // `mso-ignore:colspan` marks the overflow of a text into its neighbours: the cell is as wide as it looks, not merged.
  function isIgnoredColSpan(cell) {
    const style = attrOf(cell, "style");
    if (!style) return false;
    return splitDeclarations(style).some((declaration) => {
      const at = declaration.indexOf(":");
      if (at === -1 || declaration.slice(0, at).trim().toLowerCase() !== "mso-ignore") return false;
      return declaration.slice(at + 1).trim().toLowerCase().split(/[,\s]+/).includes("colspan");
    });
  }

  const spanOf = (cell, name) => {
    const n = Number.parseInt(attrOf(cell, name) ?? "", 10);
    return Number.isFinite(n) && n > 1 ? n : 1;
  };

  // Widths of the columns in px from `<col width=... span=...>`; 0 where the table says nothing.
  function columnWidths(table) {
    const widths = [];
    for (const col of table.querySelectorAll("col")) {
      const style = parseDeclarations(attrOf(col, "style") || "");
      const px = pxOf(attrOf(col, "width") ?? "") ?? pxOf(style.width ?? "");
      const span = Math.min(spanOf(col, "span"), MAX_COLS - widths.length);
      for (let i = 0; i < span; i += 1) widths.push(px === null ? 0 : Math.round(px));
    }
    return widths;
  }

  const rowsOf = (table) => {
    const rows = [];
    for (const child of table.childNodes || []) {
      const tag = child.nodeType === 1 ? String(child.tagName).toUpperCase() : "";
      if (tag === "TR") rows.push(child);
      else if (SECTION_TAGS.has(tag)) for (const row of child.childNodes || []) if (row.nodeType === 1 && String(row.tagName).toUpperCase() === "TR") rows.push(row);
    }
    return rows;
  };

  const cellsOf = (row) => (row.childNodes || []).filter((c) => c.nodeType === 1 && CELL_TAGS.has(String(c.tagName).toUpperCase()));

  // A table inside a cell belongs to that cell's text, not to the paste.
  function isNested(table) {
    for (let node = table.parentNode ?? table.parentElement; node; node = node.parentNode ?? node.parentElement) {
      if (node.nodeType === 1 && CELL_TAGS.has(String(node.tagName).toUpperCase())) return true;
    }
    return false;
  }

  // A border two neighbours share is kept once, on the cell before (the reference drops the later side).
  function dropSharedEdges(edges, row, col, bd) {
    if (!bd) return bd;
    const above = edges.get((row - 1) * MAX_COLS + col);
    const left = edges.get(row * MAX_COLS + col - 1);
    const out = { ...bd };
    if (above?.b && out.t) delete out.t;
    if (left?.r && out.l) delete out.l;
    return Object.keys(out).length > 0 ? out : undefined;
  }

  /**
   * The block of the tables in `root` (a parsed document's element), or a refusal.
   * `{ block }` with `block` null when there is no cell; `{ refused: "cells" }` over a bound.
   * The block is `{ rows: null, c1: 0, cols, cells, widths, merges, rich: true }` with `cells[i][j]` a
   * `{ value, formula: "", style, t }` or null for an empty cell, and `merges` offsets `{ r1, c1, r2, c2 }` (0-based).
   */
  function parseTables(root, styleFromPayload) {
    const rules = readStyleRules(Array.from(root.querySelectorAll("style"), (s) => s.textContent || ""));
    const tables = Array.from(root.querySelectorAll("table")).filter((table) => !isNested(table));
    const cells = new Map(); // row * MAX_COLS + col -> { value, t, payload }
    const covered = new Set();
    const bds = new Map();
    const merges = [];
    const maxMerges = host.__EASYOFFICE_SHEET_LAYOUT__?.MAX_MERGES ?? DEFAULT_MAX_MERGES;
    const formats = [];
    const payloadStyles = new Map();
    // A cell with the same tag, class, id and style attribute under the same row and table looks the same: resolved once.
    const resolvedCells = new Map();
    const resolveCell = (td, rowKey, rowDecl) => {
      const key = rowKey + "\u0002" + elementKey(td);
      let resolved = resolvedCells.get(key);
      if (resolved === undefined) {
        const decl = declarationsOf(td, rules, rowDecl);
        const payload = payloadOf(decl);
        // The font the tag's rule gives every cell is the source's normal font: a cell the source formats itself carries a
        // font, the others take the font of the cells they land among.
        const normal = payloadOf(rules.get(String(td.tagName).toLowerCase()) || {});
        for (const key of ["sizePt", "font"]) if (payload[key] === normal[key]) delete payload[key];
        resolved = { payload, format: numFmtOf(decl) };
        if (resolvedCells.size >= RESOLVED_CELLS) resolvedCells.clear();
        resolvedCells.set(key, resolved);
      }
      return resolved;
    };
    let rowBase = 0;
    let rowCount = 0;
    let colCount = 0;
    let used = 0;
    let widths = [];

    for (const table of tables) {
      const tableDecl = declarationsOf(table, rules, null);
      const tableKey = elementKey(table);
      if (widths.length === 0) widths = columnWidths(table);
      const rows = rowsOf(table);
      rows.forEach((tr, ri) => {
        const row = rowBase + ri;
        if (row >= MAX_ROWS) return;
        const rowDecl = declarationsOf(tr, rules, tableDecl);
        const rowKey = tableKey + "\u0001" + elementKey(tr);
        let col = 0;
        for (const td of cellsOf(tr)) {
          while (covered.has(row * MAX_COLS + col)) col += 1;
          if (col >= MAX_COLS) break;
          const rowSpan = Math.min(spanOf(td, "rowspan"), MAX_ROWS - row);
          const colSpan = Math.min(isIgnoredColSpan(td) ? 1 : spanOf(td, "colspan"), MAX_COLS - col);
          used += rowSpan * colSpan;
          if (used > LIMITS.CELLS) throw new RangeError("cells");
          const resolved = resolveCell(td, rowKey, rowDecl);
          let nf = 0;
          if (resolved.format) {
            nf = formats.indexOf(resolved.format) + 1;
            if (nf === 0 && formats.length < LIMITS.NUMBER_FORMATS) nf = formats.push(resolved.format);
          }
          const payload = { ...resolved.payload };
          if (nf > 0) payload.nf = nf;
          const bd = dropSharedEdges(bds, row, col, payload.bd);
          if (bd) payload.bd = bd;
          else delete payload.bd;
          const { value, t } = valueOf(td);
          for (let r = 0; r < rowSpan; r += 1) {
            for (let c = 0; c < colSpan; c += 1) {
              const key = (row + r) * MAX_COLS + col + c;
              covered.add(key);
              const anchor = r === 0 && c === 0;
              if (anchor) cells.set(key, { value, t, payload });
              else if (Object.keys(payload).length > 0) cells.set(key, { value: "", payload });
              if (payload.bd) bds.set(key, payload.bd);
            }
          }
          if (rowSpan > 1 || colSpan > 1) {
            if (merges.length >= maxMerges) throw new RangeError("merges");
            merges.push({ r1: row, c1: col, r2: row + rowSpan - 1, c2: col + colSpan - 1 });
          }
          rowCount = Math.max(rowCount, row + rowSpan);
          colCount = Math.max(colCount, col + colSpan);
          col += colSpan;
        }
      });
      rowBase += rows.length;
    }
    if (rowCount === 0 || colCount === 0) return { block: null };

    // The block is a dense rectangle: a few cells far apart would otherwise cost a grid of millions.
    if (rowCount * colCount > LIMITS.CELLS) throw new RangeError("cells");
    const grid = Array.from({ length: rowCount }, () => new Array(colCount).fill(null));
    for (const [key, cell] of cells) {
      const col = key % MAX_COLS;
      const row = (key - col) / MAX_COLS;
      if (cell.value === "" && Object.keys(cell.payload).length === 0) continue;
      const json = JSON.stringify(cell.payload);
      let style = payloadStyles.get(json);
      if (style === undefined) {
        style = Object.keys(cell.payload).length > 0 ? styleFromPayload(cell.payload, formats, null) : null;
        payloadStyles.set(json, style);
      }
      const out = { value: cell.value, formula: "", style: style || undefined };
      if (cell.t) out.t = cell.t;
      grid[row][col] = out;
    }
    return { block: { rows: null, c1: 0, cols: colCount, cells: grid, widths: widths.slice(0, colCount), merges, rich: true } };
  }

  // The inert document of `html`: `DOMParser` makes one without a browsing context, so no script runs and nothing loads, and
  // the tree is only read here, never moved into the page (the hygiene rule lists this file for that reason).
  function inertDocument(html) {
    return new host.DOMParser().parseFromString(html, "text/html");
  }

  /**
   * Cells of the clipboard HTML. `parseDocument(html)` parses it into an inert document (scripts do not run, nothing
   * loads); `styleFromPayload` is the sheet's style codec. Returns `{ block }` (null when the HTML holds no table) or
   * `{ refused }` when a bound is passed.
   */
  function parseClipboardHtml(html, { parseDocument = inertDocument, styleFromPayload }) {
    if (typeof html !== "string" || html === "") return { block: null };
    if (html.length > LIMITS.HTML_CHARS) return { refused: "size" };
    if (!/<t[dh][\s>]/i.test(html)) return { block: null };
    try {
      const doc = parseDocument(html);
      const root = doc?.documentElement || doc?.body;
      return root ? parseTables(root, styleFromPayload) : { block: null };
    } catch (err) {
      if (err instanceof RangeError) return { refused: err.message };
      throw err;
    }
  }

  host.__EASYOFFICE_SHEET_CLIP_HTML__ = Object.freeze({ LIMITS, htmlIsFromSpreadsheet, parseClipboardHtml, readStyleRules, decodeMsoNumberFormat, colorOf, edgeOf, pxOf });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
