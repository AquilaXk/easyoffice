(function (host) {
  "use strict";

  // Structured save payload (PD-073): ordered paragraph/table blocks (tables with their merged
  // regions, PD-130) with run formatting, every workbook sheet, and slides as JSON fields. User text is never encoded as line markup.

  const BLOCK_TAGS = new Set([
    "P", "DIV", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "UL", "OL",
    "BLOCKQUOTE", "SECTION", "ARTICLE", "PRE", "HEADER", "ASIDE", "FIGURE", "FIGCAPTION", "DL", "DT", "DD",
  ]);
  const SKIP_TAGS = new Set(["FOOTER", "SCRIPT", "STYLE", "BUTTON", "TEMPLATE", "NOSCRIPT"]);
  const CSS_ALIGN = { left: "left", start: "left", center: "center", right: "right", end: "right", justify: "justify" };
  const TABLE_FUNCS = new Set(["SUM", "AVERAGE", "COUNT", "MAX", "MIN", "PRODUCT"]);
  const TABLE_DIRS = new Set(["ABOVE", "BELOW", "LEFT", "RIGHT"]);
  const BOOL_KEYS = ["bold", "italic", "underline", "strike"];
  const VALUE_KEYS = ["size", "font", "color", "highlight"];
  // Run fields of the structured payload (PD-077): `size` is sent as `sizeHalfPt`.
  const RUN_FIELDS = { size: "sizeHalfPt", font: "font", color: "color", highlight: "highlight" };
  const SCRIPT_TAGS = { SUP: "superscript", SUB: "subscript" };
  const LAYOUT_TAGS = new Set(["P", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "ASIDE"]);
  const TWIPS_PER_PX = 15; // 1 px = 3/4 pt = 15 twips
  const MAX_LINE_PERCENT = 1000;
  const MAX_TABLE_COLUMNS = 1024; // the readers' padded-column cap (PD-117)
  const WINDOWS_RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i;
  const PX_VALUE_RE = /^(-?\d+(?:\.\d+)?)px$/;
  const LINE_SPACING_RE = /^(\d+(?:\.\d+)?)(%?)$/;
  const TABLE_NUM_RE = /^[+-]?(\d+(?:\.\d*)?|\.\d+)([eE][+-]?\d+)?$/;
  const TABLE_FORMULA_RE = /^=\s*([A-Za-z]+)\s*\(\s*([A-Za-z]+)\s*\)\s*$/;
  const CELL_TAG_RE = /^(TD|TH)$/i;
  const HEADING_TAG_RE = /^H([1-6])$/;

  function isSkippedElement(el) {
    const tag = (el.tagName || "").toUpperCase();
    if (SKIP_TAGS.has(tag)) return true;
    if (el.id === "hwp-manuscript-preview") return true;
    if (el.classList?.contains("doc-table-toolbar")) return true;
    // The header and footer of the page are read apart from the body (`collectPageFrame`).
    if (el.dataset?.eoHf !== undefined) return true;
    return typeof el.hasAttribute === "function" && el.hasAttribute("hidden");
  }

  function normalizeAlign(css) {
    return CSS_ALIGN[String(css || "").trim().toLowerCase()] || null;
  }

  // The in-document bookmark of a table-of-contents block (`doc-toc.js`): heading `eo-h-N` is bookmark `_TocN`, an
  // entry's `#eo-h-N` link is `#_TocN` (the engine's `toc_bookmark_number`: 1 to 4294967295, no leading zero).
  const MAX_FRAME_PARAGRAPHS = 64; // the engine's MAX_PAGE_FRAME_PARAGRAPHS
  // The editor's field marks (`data-eo-field`) as the payload's run `field` values.
  const FIELD_KEYS = { page: "page", pages: "pages", filename: "filename" };
  const MAX_BOOKMARK = 4294967295;
  const HEADING_ID_RE = /^eo-h-([1-9]\d{0,9})$/;
  const bookmarkNumber = (id) => {
    const n = Number(HEADING_ID_RE.exec(String(id || ""))?.[1]);
    return n >= 1 && n <= MAX_BOOKMARK ? n : undefined;
  };

  // The stored address of an editor link (`doc-link.js`); a link whose address the policy refuses saves as plain text.
  function linkTarget(el) {
    const links = host.__EASYOFFICE_DOC_LINK__;
    if (el.parentElement?.dataset?.eoToc !== undefined) {
      const number = bookmarkNumber(String(el.getAttribute("href") || "").slice(1));
      return number === undefined ? undefined : "#_Toc" + number;
    }
    return links && el.dataset?.eoLink ? links.normalizeUrl(el.getAttribute("href")) ?? undefined : undefined;
  }

  function styleFormat(el, fmt) {
    const tag = (el.tagName || "").toUpperCase();
    const st = el.style || {};
    const weight = String(st.fontWeight || "").trim().toLowerCase();
    const deco = String(st.textDecoration || st.textDecorationLine || "").toLowerCase();
    const next = {
      bold: fmt.bold || tag === "STRONG" || tag === "B" || weight === "bold" || weight === "bolder" || Number(weight) >= 600,
      italic: fmt.italic || tag === "EM" || tag === "I" || String(st.fontStyle || "").toLowerCase() === "italic",
      underline: fmt.underline || tag === "U" || tag === "INS" || deco.includes("underline"),
      strike: fmt.strike || tag === "S" || tag === "STRIKE" || tag === "DEL" || deco.includes("line-through"),
      script: SCRIPT_TAGS[tag] || fmt.script,
      link: tag === "A" ? linkTarget(el) : fmt.link,
    };
    return mergeRunValues(el, fmt, next);
  }

  // Character-run values (size in half-points, font, color, highlight) from run spans and pasted
  // inline styles; the innermost element wins.
  function mergeRunValues(el, outer, next) {
    const runs = host.__EASYOFFICE_DOC_RUNS__;
    VALUE_KEYS.forEach((k) => {
      if (outer[k] !== undefined) next[k] = outer[k];
    });
    if (!runs) return next;
    const own = runs.isRunElement(el) ? runs.readRunFormat(el) : runs.readLegacyFormat(el);
    VALUE_KEYS.forEach((k) => {
      if (own[k] !== undefined) next[k] = own[k];
    });
    BOOL_KEYS.forEach((k) => {
      if (own[k]) next[k] = true;
    });
    return next;
  }

  // Paragraph indents (px -> twips) and line height of a paragraph-like element. The exact twip
  // values written when a file was opened win while the style still matches them.
  function pxValue(raw) {
    const m = PX_VALUE_RE.exec(String(raw || "").trim());
    return m ? Number(m[1]) : null;
  }

  function twipsFor(stylePx, exact) {
    const px = pxValue(stylePx);
    if (px === null) return null;
    const computed = Math.round(px * TWIPS_PER_PX);
    const stored = Number(exact);
    return Number.isFinite(stored) && Math.abs(stored - computed) <= 1 ? stored : computed;
  }

  function paragraphLayout(node, baseLineHeight) {
    const st = node.style || {};
    const data = node.dataset || {};
    const out = {};
    const left = twipsFor(st.paddingLeft, data.eoLeftTwips);
    if (left !== null && left > 0) out.leftIndentTwips = left;
    const first = twipsFor(st.textIndent, data.eoFirstLineTwips);
    if (first !== null && first !== 0) out.firstLineIndentTwips = first;
    const rawLineHeight = String(st.lineHeight || baseLineHeight || "").trim();
    const spacing = LINE_SPACING_RE.exec(rawLineHeight);
    if (spacing) {
      const percent = Math.round(Number(spacing[1]) * (spacing[2] ? 1 : 100));
      if (percent >= 1 && percent <= MAX_LINE_PERCENT && percent !== 100) out.lineSpacingPercent = percent;
    }
    return out;
  }

  function parseTableNumber(text) {
    const cleaned = String(text || "").replaceAll(",", "").replaceAll(/\s+/g, "");
    if (!cleaned || !TABLE_NUM_RE.test(cleaned)) return null;
    const n = Number(cleaned);
    return Number.isFinite(n) ? n : null;
  }

  function formatTableNumber(n) {
    if (Number.isInteger(n) && Math.abs(n) < 1e15) return String(n);
    return n.toFixed(10).replace(/(?<!0)0+$/, "").replace(/\.$/, "");
  }

  function parseTableFormula(text) {
    const m = TABLE_FORMULA_RE.exec(String(text || "").trim());
    if (!m) return null;
    const func = m[1].toUpperCase();
    const dir = m[2].toUpperCase();
    return TABLE_FUNCS.has(func) && TABLE_DIRS.has(dir) ? { func, dir } : null;
  }

  // Mirrors easyoffice_formats::evaluate_table_formulas: row-major, so a later formula sees the
  // computed value of an earlier one. Returns a same-shaped grid of computed values (null = none).
  function evaluateDocTableFormulas(textRows) {
    const { aggregateNumbers } = host.__EASYOFFICE_SHEET_UTILS__;
    const rows = textRows.map((r) => r.slice());
    const values = textRows.map((r) => r.map(() => null));
    rows.forEach((row, r) => {
      row.forEach((cell, c) => {
        const f = parseTableFormula(cell);
        if (!f) return;
        let operands = [];
        if (f.dir === "ABOVE") operands = rows.slice(0, r).map((rr) => rr[c]);
        else if (f.dir === "BELOW") operands = rows.slice(r + 1).map((rr) => rr[c]);
        else if (f.dir === "LEFT") operands = row.slice(0, c);
        else operands = row.slice(c + 1);
        const nums = operands.map(parseTableNumber).filter((n) => n !== null);
        const { count, sum, min, max, avg } = aggregateNumbers(nums);
        let v = 0;
        if (f.func === "SUM") v = sum;
        else if (f.func === "COUNT") v = count;
        else if (count > 0 && f.func === "AVERAGE") v = avg;
        else if (count > 0 && f.func === "MAX") v = max;
        else if (count > 0 && f.func === "MIN") v = min;
        else if (count > 0 && f.func === "PRODUCT") v = nums.reduce((a, b) => a * b, 1);
        const shown = formatTableNumber(v);
        rows[r][c] = shown;
        values[r][c] = shown;
      });
    });
    return values;
  }

  function cellText(cell) {
    return String(cell.textContent || "").replaceAll(/\s+/g, " ").trim();
  }

  // A span attribute as a positive integer (`rowspan="0"` and junk count as 1).
  function spanOf(value) {
    return Math.max(1, Math.trunc(Number(value)) || 1);
  }

  // Rows that a cell spanning `wanted` rows covers, starting at row `r`. A table opened from a file
  // marks each row with its original index (`data-eo-row`), so a row deleted inside a region
  // shortens it instead of letting it reach the row after the region.
  function coveredRowCount(trs, r, wanted) {
    const limit = Math.min(wanted, trs.length - r);
    const origin = Number(trs[r].dataset?.eoRow);
    if (!Number.isInteger(origin)) return limit;
    let count = 1;
    while (count < limit && Number(trs[r + count].dataset?.eoRow) < origin + wanted) count += 1;
    return count;
  }

  // A span stops at the first slot another cell already covers, so regions never overlap and each
  // slot is visited once. Free slots in a row from `col`, at most `wanted`.
  function freeRun(takenRow, col, wanted) {
    let count = 0;
    while (count < wanted && !takenRow[col + count]) count += 1;
    return count;
  }

  // Rows from `r` (whose slots from `col` are free) that stay free over `colSpan` columns.
  function freeRows(taken, r, col, colSpan, wanted) {
    let count = 1;
    while (count < wanted && freeRun(taken[r + count], col, colSpan) === colSpan) count += 1;
    return count;
  }

  const TABLE_SECTION_TAGS = new Set(["THEAD", "TBODY", "TFOOT"]);

  function collectTableRows(table) {
    const trs = [];
    const tableChildren = table.children;
    if (!tableChildren) return trs;
    for (let i = 0; i < tableChildren.length; i += 1) {
      const child = tableChildren[i];
      const tag = (child.tagName || "").toUpperCase();
      if (tag === "TR") {
        trs.push(child);
      } else if (TABLE_SECTION_TAGS.has(tag)) {
        const secChildren = child.children;
        if (secChildren) {
          for (let j = 0; j < secChildren.length; j += 1) {
            const row = secChildren[j];
            if ((row.tagName || "").toUpperCase() === "TR") trs.push(row);
          }
        }
      }
    }
    return trs;
  }

  function runFlags(fmt) {
    return (
      (fmt.bold ? 1 : 0) |
      (fmt.italic ? 2 : 0) |
      (fmt.underline ? 4 : 0) |
      (fmt.strike ? 8 : 0) |
      (fmt.script === "superscript" ? 16 : 0) |
      (fmt.script === "subscript" ? 32 : 0)
    );
  }

  function sameFormat(a, b) {
    return Boolean(
      a &&
        runFlags(a) === runFlags(b) &&
        a.sizeHalfPt === b.size &&
        a.font === b.font &&
        a.color === b.color &&
        a.highlight === b.highlight &&
        a.link === b.link
    );
  }

  // Cell text grid and merged regions of a table, read as the HTML table model lays it out: a cell
  // with colspan/rowspan covers the slots to its right and below, and the cells after it start
  // behind them. Covered slots are empty strings. Spans stop at the table's last row, at
  // MAX_TABLE_COLUMNS and at slots another cell covers; hidden cells are not part of the grid; rows
  // without any slot are dropped.
  function readTableGrid(table) {
    const trs = collectTableRows(table);
    const taken = trs.map(() => []);
    const texts = trs.map(() => []);
    const merges = [];
    trs.forEach((tr, r) => {
      let col = 0;
      const cells = tr.children;
      if (!cells) return;
      for (let i = 0; i < cells.length; i += 1) {
        const cell = cells[i];
        if (!CELL_TAG_RE.test(cell.tagName || "") || isSkippedElement(cell)) continue;
        while (taken[r][col]) col += 1;
        if (col >= MAX_TABLE_COLUMNS) break;
        const colSpan = freeRun(taken[r], col, Math.min(spanOf(cell.colSpan), MAX_TABLE_COLUMNS - col));
        const rowSpan = freeRows(taken, r, col, colSpan, coveredRowCount(trs, r, spanOf(cell.rowSpan)));
        for (let dr = 0; dr < rowSpan; dr += 1) {
          for (let dc = 0; dc < colSpan; dc += 1) {
            taken[r + dr][col + dc] = true;
            texts[r + dr][col + dc] = "";
          }
        }
        texts[r][col] = cellText(cell);
        if (rowSpan * colSpan > 1) merges.push({ row: r, col, rowSpan, colSpan });
        col += colSpan;
      }
    });
    const rows = texts.map((row) => Array.from(row, (text) => text ?? ""));
    const before = [0];
    rows.forEach((row) => before.push(before.at(-1) + (row.length > 0 ? 1 : 0)));
    const regions = merges
      .map((m) => ({ ...m, row: before[m.row], rowSpan: before[m.row + m.rowSpan] - before[m.row] }))
      .filter((m) => m.rowSpan * m.colSpan > 1);
    return { texts: rows.filter((row) => row.length > 0), merges: regions };
  }

  function buildTableBlock(table) {
    const { texts, merges } = readTableGrid(table);
    const values = evaluateDocTableFormulas(texts);
    const rows = texts.map((row, r) => row.map((text, c) => (values[r][c] === null ? { text } : { text, value: values[r][c] })));
    return merges.length > 0 ? { type: "table", rows, merges } : { type: "table", rows };
  }

  // Walks an editor root in document order and returns paragraph/table blocks.
  function collectDocBlocks(root, options = {}) {
    const blocks = [];
    const baseAlign = normalizeAlign(options.baseAlign);
    const baseLineHeight = String(options.baseLineHeight || root?.style?.lineHeight || "").trim();
    let cur = null;

    function flush() {
      if (!cur) return;
      const runs = cur.runs;
      if (runs.length > 0) {
        // Text nodes are collapsed to single spaces; a line break in the run is content, and one
        // at the very end is the placeholder `<br>` of a blank line.
        runs[0].text = runs[0].text.replace(/^ +/, "");
        runs[runs.length - 1].text = runs[runs.length - 1].text.replace(/ +$/, "").replace(/\n$/, "").replace(/ +$/, "");
      }
      const kept = runs.filter((r) => r.text.length > 0 || r.field);
      if (kept.length > 0 || cur.blank) {
        const block = { type: "paragraph", runs: kept };
        Object.assign(block, cur.ctx.layout);
        if (cur.ctx.align) block.align = cur.ctx.align;
        if (cur.ctx.heading) block.heading = cur.ctx.heading;
        if (cur.ctx.toc !== undefined) block.toc = cur.ctx.toc;
        if (cur.ctx.bookmark !== undefined) block.bookmark = cur.ctx.bookmark;
        if (cur.ctx.tocPage !== undefined) Object.assign(block, { tocPage: cur.ctx.tocPage, tocDots: cur.ctx.tocDots });
        if (cur.ctx.pageBreak) block.pageBreakBefore = true;
        if (cur.ctx.list) block.list = cur.ctx.list;
        blocks.push(block);
      }
      cur = null;
    }

    function appendText(text, ctx, fmt) {
      if (!text) return;
      if (!cur) cur = { ctx, runs: [] };
      const last = cur.runs[cur.runs.length - 1];
      if (!last?.field && sameFormat(last, fmt)) {
        last.text += text;
        return;
      }
      const run = { text };
      if (fmt.script) run.script = fmt.script;
      if (fmt.link) run.link = fmt.link;
      BOOL_KEYS.forEach((k) => {
        if (fmt[k]) run[k] = true;
      });
      VALUE_KEYS.forEach((k) => {
        if (fmt[k] !== undefined) run[RUN_FIELDS[k]] = fmt[k];
      });
      cur.runs.push(run);
    }

    // A page-number field (`doc-page-frame.js`) is a run of its own with the format around it.
    function walkField(node, ctx, fmt) {
      if (!cur) cur = { ctx, runs: [] };
      const run = { text: "", field: FIELD_KEYS[node.dataset.eoField] };
      if (fmt.script) run.script = fmt.script;
      BOOL_KEYS.forEach((k) => {
        if (fmt[k]) run[k] = true;
      });
      VALUE_KEYS.forEach((k) => {
        if (fmt[k] !== undefined) run[RUN_FIELDS[k]] = fmt[k];
      });
      cur.runs.push(run);
    }

    function walkTextNode(node, ctx, fmt) {
      const prev = cur?.runs[cur.runs.length - 1]?.text || "";
      let text = String(node.nodeValue || "").replaceAll(/\s+/g, " ");
      if ((!cur || /[ \n]$/.test(prev)) && text.startsWith(" ")) text = text.slice(1);
      appendText(text, ctx, fmt);
    }

    // A `<br>` is a line break inside the paragraph (`\n` in the run text); as the only content
    // of a block it is a blank paragraph.
    function walkLineBreak(ctx, fmt) {
      if (!cur) cur = { ctx, runs: [], blank: true };
      else appendText("\n", cur.ctx, fmt);
    }

    function walkTable(node) {
      flush();
      const tbl = buildTableBlock(node);
      if (tbl.rows.length > 0) blocks.push(tbl);
    }

    // The table-of-contents role of a block (`doc-toc.js`): 0 is the title, 1-6 an entry level.
  function tocRole(node) {
    const role = Number(node.dataset?.eoToc);
    return node.dataset?.eoToc !== undefined && Number.isInteger(role) ? role : undefined;
  }

  // The page number a table-of-contents entry shows (`doc-toc.js`), within the engine's seven digits.
  const MAX_TOC_PAGE = 9999999;
  function tocPage(node) {
    const page = Number(node.dataset?.eoTocPage);
    return tocRole(node) > 0 && Number.isInteger(page) && page >= 1 && page <= MAX_TOC_PAGE ? page : undefined;
  }

  function blockContext(node, tag, ctx) {
      const headingMatch = HEADING_TAG_RE.exec(tag);
      const listParent = tag === "LI" ? node.parentElement : null;
      let nextList = ctx.list;
      if (listParent) nextList = (listParent.tagName || "").toUpperCase() === "OL" ? "ordered" : "bullet";
      return {
        align: normalizeAlign(node.style?.textAlign) || ctx.align,
        heading: headingMatch ? Number(headingMatch[1]) : ctx.heading,
        toc: tocRole(node) ?? ctx.toc,
        bookmark: headingMatch ? bookmarkNumber(node.id) : undefined,
        tocPage: tocPage(node),
        tocDots: node.dataset?.eoTocDots === "true",
        pageBreak: node.dataset?.eoBreak === "page",
        list: nextList,
        layout: LAYOUT_TAGS.has(tag) ? paragraphLayout(node, baseLineHeight) : ctx.layout,
      };
    }

    function pushImage(node) {
      flush();
      const utils = host.__EASYOFFICE_SHEET_UTILS__;
      const data = node.dataset || {};
      const stored = utils ? utils.getDocImage(data.eoImageId) : null;
      const alt = String(data.eoImageAlt ?? "");
      if (!stored) {
        // The bytes are gone (for example after a crash recovery): keep the alternative text.
        blocks.push({ type: "paragraph", runs: [{ text: alt || "[그림]" }] });
        return;
      }
      const block = { type: "image", format: stored.format, base64: stored.base64, alt };
      const width = Number(data.eoImageWidthEmu);
      const height = Number(data.eoImageHeightEmu);
      if (width > 0 && height > 0) Object.assign(block, { widthEmu: width, heightEmu: height });
      blocks.push(block);
    }

    function walk(node, ctx, fmt) {
      if (node.nodeType === 3) {
        walkTextNode(node, ctx, fmt);
        return;
      }
      if (node.nodeType !== 1 || isSkippedElement(node)) return;
      const tag = (node.tagName || "").toUpperCase();
      if (tag === "BR") return walkLineBreak(ctx, fmt);
      if (tag === "TABLE") return walkTable(node);
      if (tag === "IMG" && node.dataset?.eoImageId) return pushImage(node);
      if (FIELD_KEYS[node.dataset?.eoField]) return walkField(node, ctx, fmt);
      const nextFmt = styleFormat(node, fmt);
      const childNodes = node.childNodes;
      if (!BLOCK_TAGS.has(tag)) {
        if (childNodes) {
          for (let i = 0; i < childNodes.length; i += 1) walk(childNodes[i], ctx, nextFmt);
        }
        return;
      }
      flush();
      const nextCtx = blockContext(node, tag, ctx);
      if (childNodes) {
        for (let i = 0; i < childNodes.length; i += 1) walk(childNodes[i], nextCtx, nextFmt);
      }
      flush();
    }

    const rootFmt = { bold: false, italic: false, underline: false, strike: false };
    const rootChildren = root?.childNodes;
    if (rootChildren) {
      const rootCtx = { align: baseAlign, heading: 0, list: null };
      for (let i = 0; i < rootChildren.length; i += 1) walk(rootChildren[i], rootCtx, rootFmt);
    }
    flush();
    return blocks;
  }

  // The header and footer sections of an editor root as paragraph blocks, as the `header` and `footer` fields of
  // the payload; a section without text or field is left out.
  function collectPageFrame(root, options = {}) {
    const frame = {};
    for (const section of Array.from(root?.children || [])) {
      const kind = section.dataset?.eoHf;
      if ((kind !== "header" && kind !== "footer") || Object.hasOwn(frame, kind)) continue;
      const blocks = collectDocBlocks(section, options).filter((b) => b.type === "paragraph").slice(0, MAX_FRAME_PARAGRAPHS);
      if (blocks.some((b) => b.runs.length > 0)) frame[kind] = blocks;
    }
    return frame;
  }

  function plainParagraphBlocks(lines) {
    return lines
      .map((line) => String(line || "").trim())
      .filter(Boolean)
      .map((text) => ({ type: "paragraph", runs: [{ text }] }));
  }

  // The number format codes of one sheet's payload: `intern(code)` is the code's 1-based entry, or 0
  // when the code is over the bounds or the list is full (the cell then saves without a format).
  function internNumFmts() {
    const numfmt = host.__EASYOFFICE_SHEET_NUMFMT__;
    const codes = [];
    const entries = new Map();
    const intern = (code) => {
      let at = entries.get(code);
      if (at === undefined) {
        const ok = (!numfmt || numfmt.codeWithinBounds(code)) && codes.length < (numfmt?.MAX_CODES ?? 512);
        at = ok ? codes.push(code) : 0;
        entries.set(code, at);
      }
      return at;
    };
    return { codes, intern };
  }

  // Every sheet in tab order (then any stored sheet without a tab); keys are the formula-visible
  // sheet names. `raw` is the formula when present, otherwise the typed value.
  function buildSheetsPayload(sheetKeys, sheetDataStore, columnWidths, rulesBySheet, layoutBySheet) {
    const ordered = [];
    const seen = new Set();
    [...sheetKeys, ...Object.keys(sheetDataStore || {})].forEach((key) => {
      const name = String(key || "").trim();
      if (!name || seen.has(name.toUpperCase())) return;
      seen.add(name.toUpperCase());
      ordered.push(name);
    });
    return ordered.map((name) => {
      const store = sheetDataStore?.[name] || {};
      const codec = host.__EASYOFFICE_SHEET_STYLE_CODEC__;
      const numFmts = internNumFmts();
      const cells = [];
      for (const ref of Object.keys(store)) {
        const entry = store[ref];
        const raw = String(entry?.formula || entry?.value || "");
        const style = codec ? codec.styleToPayload(entry?.style, numFmts.intern) : null;
        if (!raw && !style) continue;
        cells.push(style ? { ref, raw, style } : { ref, raw });
      }
      const columns = buildColumnsPayload(columnWidths?.[name]);
      const sheet = columns.length > 0 ? { name, cells, columns } : { name, cells };
      if (numFmts.codes.length > 0) sheet.numFmts = numFmts.codes;
      // The arrays that spill (PD-217): the cells they filled are in `cells` as values.
      const spillAreas = host.__EASYOFFICE_SHEET_SPILL__?.areasOf(store) || [];
      if (spillAreas.length > 0) sheet.spills = spillAreas;
      const links = host.__EASYOFFICE_SHEET_LINK__?.payloadOf(store) ?? [];
      if (links.length > 0) sheet.links = links;
      const rules = rulesBySheet?.[name];
      if (rules && ((rules.cf || []).length > 0 || (rules.dv || []).length > 0)) sheet.rules = rules;
      // Merged ranges and hidden rows and columns (PD-194): `merges`, `hiddenRows`, `hiddenCols`.
      Object.assign(sheet, layoutBySheet?.[name]);
      // The selection, zoom and scroll position of the sheet (PD-208).
      const view = host.__EASYOFFICE_SHEET_VIEW_STATE__?.get(name);
      if (view) sheet.view = view;
      return sheet;
    });
  }

  // Custom column widths of one sheet (`{ [1-based column]: px }`) as `[{ index, widthPx }]`, in
  // column order (PD-089).
  function buildColumnsPayload(widths) {
    return Object.keys(widths || {})
      .map(Number)
      .filter((index) => Number.isInteger(index) && index >= 1 && Number(widths[index]) > 0)
      .sort((a, b) => a - b)
      .map((index) => ({ index, widthPx: Math.round(Number(widths[index])) }));
  }

  const KNOWN_DOC_EXTENSIONS = [
    "docx?", "hwpx?", "hwtx?", "hwt", "xlsx?", "ods", "csv", "tsv", "pptx?", "ppsx", "odp", "odt", "pdf", "rtf", "md", "markdown", "txt", "pages",
  ];
  const KNOWN_DOC_EXT_RE = new RegExp(String.raw`\.(?:${KNOWN_DOC_EXTENSIONS.join("|")})$`, "i");

  // File name from the document/tab title, safe on Windows and macOS: no extension, no reserved
  // characters or device names, no trailing dots/spaces, at most 120 characters.
  function sanitizeFileBaseName(rawTitle, fallback) {
    let name = String(rawTitle || "")
      .replace(/×$/, "")
      .trim()
      .replace(KNOWN_DOC_EXT_RE, "");
    name = name
      .replaceAll(/[\u0000-\u001f\u007f<>:"/\\|?*]/g, "_")
      .replaceAll(/\s+/g, " ")
      .replace(/^[\s.]+/, "")
      .replace(/(?<![\s.])[\s.]+$/, "");
    if (Array.from(name).length > 120) name = Array.from(name).slice(0, 120).join("").replace(/(?<![\s.])[\s.]+$/, "");
    if (WINDOWS_RESERVED.test(name.split(".")[0] || "")) name = "_" + name;
    return name || fallback;
  }

  host.__EASYOFFICE_SAVE_PAYLOAD__ = {
    collectDocBlocks,
    collectPageFrame,
    plainParagraphBlocks,
    evaluateDocTableFormulas,
    buildSheetsPayload,
    buildColumnsPayload,
    sanitizeFileBaseName,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
