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
  const COLS_RE = /^(\d+),([01]),(\d+)$/;
  const FRAME_KINDS = new Set(["header", "footer", "firstHeader", "firstFooter", "evenHeader", "evenFooter"]);
  const utf8Encoder = new TextEncoder();

  function isSkippedElement(el) {
    const tag = (el.tagName || "").toUpperCase();
    if (SKIP_TAGS.has(tag)) return true;
    if (el.id === "hwp-manuscript-preview") return true;
    if (el.classList?.contains("doc-table-toolbar")) return true;
    // The header/footer and note/comment metadata sections are read apart from body blocks.
    const ds = el.dataset;
    if (ds && (ds.eoHf !== undefined || ds.eoNotes !== undefined || ds.eoComments !== undefined)) return true;
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
    const ds = el.dataset || {};
    const isRev = ds.eoRevId !== undefined && (tag === "INS" || tag === "DEL");
    const rawCommentId = ds.eoCommentId !== undefined ? Number(ds.eoCommentId) : NaN;
    const commentId = Number.isInteger(rawCommentId) && rawCommentId >= 1 ? rawCommentId : fmt.commentId;
    let revision = fmt.revision;
    if (isRev) {
      const revId = Number(ds.eoRevId);
      if (Number.isInteger(revId) && revId >= 1) {
        revision = {
          id: revId,
          kind: tag === "DEL" ? "delete" : "insert",
          author: String(ds.eoRevAuthor || "작성자"),
          date: String(ds.eoRevDate || ""),
        };
      }
    }
    const weight = String(st.fontWeight || "").trim().toLowerCase();
    const deco = String(st.textDecoration || st.textDecorationLine || "").toLowerCase();
    const next = {
      bold: fmt.bold || tag === "STRONG" || tag === "B" || weight === "bold" || weight === "bolder" || Number(weight) >= 600,
      italic: fmt.italic || tag === "EM" || tag === "I" || String(st.fontStyle || "").toLowerCase() === "italic",
      underline: fmt.underline || tag === "U" || (tag === "INS" && !isRev) || deco.includes("underline"),
      strike: fmt.strike || tag === "S" || tag === "STRIKE" || (tag === "DEL" && !isRev) || deco.includes("line-through"),
      script: SCRIPT_TAGS[tag] || fmt.script,
      link: tag === "A" ? linkTarget(el) : fmt.link,
      commentId,
      revision,
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

  function readCellContent(cell) {
    const chars = [];
    let hasLink = false;
    function walkCell(node, url) {
      if (node.nodeType === 3) {
        const val = String(node.nodeValue || "");
        for (const ch of val) chars.push({ ch, url });
        return;
      }
      if (node.nodeType !== 1 || isSkippedElement(node)) return;
      const tag = (node.tagName || "").toUpperCase();
      const nextUrl = tag === "A" ? (linkTarget(node) ?? url) : url;
      if (nextUrl) hasLink = true;
      const children = node.childNodes;
      if (!children) return;
      for (let i = 0; i < children.length; i += 1) walkCell(children[i], nextUrl);
    }
    walkCell(cell, null);
    if (!hasLink) return { text: cellText(cell), links: [] };
    const norm = [];
    for (const item of chars) {
      if (/\s/.test(item.ch)) {
        if (norm.length > 0 && norm[norm.length - 1].ch !== " ") {
          norm.push({ ch: " ", url: item.url });
        }
      } else {
        norm.push(item);
      }
    }
    if (norm.length > 0 && norm[norm.length - 1].ch === " ") norm.pop();
    if (norm.length === 0) return { text: "", links: [] };
    let text = "";
    let bytePos = 0;
    const links = [];
    for (const item of norm) {
      const len = utf8Encoder.encode(item.ch).length;
      if (item.url) {
        const last = links[links.length - 1];
        if (last && last.end === bytePos && last.url === item.url) {
          last.end += len;
        } else {
          links.push({ start: bytePos, end: bytePos + len, url: item.url });
        }
      }
      text += item.ch;
      bytePos += len;
    }
    return { text, links };
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
  const BORDER_SIDE_NAMES = ["top", "right", "bottom", "left"];
  const BORDER_LINE_KINDS = new Set(["thin", "thick", "dashed", "dotted", "double"]);
  const CELL_HEX6_RE = /^#?([0-9a-fA-F]{6})$/;

  function readCellStyleEntry(cell, row, col) {
    const data = cell.dataset || {};
    const fillMatch = CELL_HEX6_RE.exec(String(data.eoFill || "").trim());
    const fill = fillMatch ? fillMatch[1].toUpperCase() : "";
    const borders = {};
    if (data.eoBorders) {
      const slots = String(data.eoBorders).split(",");
      BORDER_SIDE_NAMES.forEach((side, idx) => {
        const raw = String(slots[idx] || "").trim();
        if (!raw || raw === "-") return;
        if (raw === "none") {
          borders[side] = "none";
          return;
        }
        const [kindRaw, colorRaw] = raw.split(":");
        const kind = String(kindRaw || "").toLowerCase();
        const cm = CELL_HEX6_RE.exec(String(colorRaw || "").trim());
        if (BORDER_LINE_KINDS.has(kind) && cm) borders[side] = kind + ":" + cm[1].toUpperCase();
      });
    }
    const hasBorders = Object.keys(borders).length > 0;
    if (!fill && !hasBorders) return null;
    const entry = { row, col };
    if (fill) entry.fill = fill;
    if (hasBorders) entry.borders = borders;
    return entry;
  }

  function readCellImagesIn(cell, row, col) {
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    if (!utils || typeof cell.querySelectorAll !== "function") return [];
    const out = [];
    for (const img of cell.querySelectorAll("img[data-eo-image-id]")) {
      const data = img.dataset || {};
      const stored = utils.getDocImage(data.eoImageId);
      if (!stored) continue;
      const entry = { row, col, format: stored.format, base64: stored.base64, alt: String(data.eoImageAlt ?? "") };
      const w = Number(data.eoImageWidthEmu);
      const h = Number(data.eoImageHeightEmu);
      if (w > 0 && h > 0) Object.assign(entry, { widthEmu: w, heightEmu: h });
      out.push(entry);
    }
    return out;
  }

  function readColWidthsTwips(table, colCount) {
    if (colCount <= 0 || !table.children) return [];
    let cg = null;
    for (let i = 0; i < table.children.length; i += 1) {
      if ((table.children[i].tagName || "").toUpperCase() === "COLGROUP") {
        cg = table.children[i];
        break;
      }
    }
    if (!cg || !cg.children) return [];
    const raw = [];
    for (let i = 0; i < cg.children.length; i += 1) {
      const col = cg.children[i];
      if ((col.tagName || "").toUpperCase() !== "COL") continue;
      const stored = Math.round(Number(col.dataset?.eoColTwips) || 0);
      const fromStyle = pxValue(col.style?.width);
      const twips = stored > 0 ? stored : fromStyle !== null && fromStyle > 0 ? Math.round(fromStyle * TWIPS_PER_PX) : 0;
      if (twips > 0) raw.push(twips);
    }
    if (raw.length === 0) return [];
    const out = raw.slice(0, colCount);
    while (out.length < colCount) out.push(out.at(-1) || 2160);
    return out;
  }

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

  function sameRevision(ra, rb) {
    if (!ra && !rb) return true;
    if (!ra || !rb) return false;
    return ra.id === rb.id && ra.kind === rb.kind && ra.author === rb.author && ra.date === rb.date;
  }

  function sameFormat(a, b) {
    return Boolean(
      a &&
        !a.field &&
        !a.noteRef &&
        !a._keepEmpty &&
        runFlags(a) === runFlags(b) &&
        a.sizeHalfPt === b.size &&
        a.font === b.font &&
        a.color === b.color &&
        a.highlight === b.highlight &&
        a.link === b.link &&
        a.commentId === b.commentId &&
        sameRevision(a.revision, b.revision)
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
    const rawLinks = [];
    const rawStyles = [];
    const rawImages = [];
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
        const { text, links } = readCellContent(cell);
        texts[r][col] = text;
        links.forEach((l) => rawLinks.push({ row: r, col, start: l.start, end: l.end, url: l.url }));
        const styleEntry = readCellStyleEntry(cell, r, col);
        if (styleEntry) rawStyles.push(styleEntry);
        readCellImagesIn(cell, r, col).forEach((im) => rawImages.push(im));
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
    const links = rawLinks
      .filter((l) => rows[l.row]?.length > 0)
      .map((l) => ({ ...l, row: before[l.row] }));
    const cellStyles = rawStyles
      .filter((s) => rows[s.row]?.length > 0)
      .map((s) => ({ ...s, row: before[s.row] }));
    const cellImages = rawImages
      .filter((im) => rows[im.row]?.length > 0)
      .map((im) => ({ ...im, row: before[im.row] }));
    const keptRows = rows.filter((row) => row.length > 0);
    const colCount = keptRows.reduce((m, row) => Math.max(m, row.length), 0);
    const colWidthsTwips = readColWidthsTwips(table, colCount);
    return { texts: keptRows, merges: regions, links, colWidthsTwips, cellStyles, cellImages };
  }

  function buildTableBlock(table) {
    const { texts, merges, links, colWidthsTwips, cellStyles, cellImages } = readTableGrid(table);
    const values = evaluateDocTableFormulas(texts);
    const rows = texts.map((row, r) => row.map((text, c) => (values[r][c] === null ? { text } : { text, value: values[r][c] })));
    const keptLinks = links.filter((l) => values[l.row]?.[l.col] === null);
    const block = { type: "table", rows };
    if (merges.length > 0) block.merges = merges;
    if (keptLinks.length > 0) block.links = keptLinks;
    if (colWidthsTwips.length > 0) block.colWidthsTwips = colWidthsTwips;
    if (cellStyles.length > 0) block.cellStyles = cellStyles;
    if (cellImages.length > 0) block.cellImages = cellImages;
    return block;
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
      const kept = runs.filter((r) => r.text.length > 0 || r.field || r.noteRef || r._keepEmpty);
      kept.forEach((r) => {
        delete r._keepEmpty;
      });
      if (kept.length > 0 || cur.blank) {
        const block = { type: "paragraph", runs: kept };
        Object.assign(block, cur.ctx.layout);
        if (cur.ctx.align) block.align = cur.ctx.align;
        if (cur.ctx.heading) block.heading = cur.ctx.heading;
        if (cur.ctx.toc !== undefined) block.toc = cur.ctx.toc;
        if (cur.ctx.bookmark !== undefined) block.bookmark = cur.ctx.bookmark;
        if (cur.ctx.tocPage !== undefined) Object.assign(block, { tocPage: cur.ctx.tocPage, tocDots: cur.ctx.tocDots });
        if (cur.ctx.pageBreak) block.pageBreakBefore = true;
        if (cur.ctx.frameCopy) block.frameCopy = true;
        if (cur.ctx.list) {
          block.list = cur.ctx.list;
          if (cur.ctx.list === "task") block.checked = Boolean(cur.ctx.checked);
          if (cur.ctx.listLevel > 0) block.listLevel = cur.ctx.listLevel;
          if (cur.ctx.list === "ordered" && cur.ctx.listNumber) block.listNumber = cur.ctx.listNumber;
        }
        if (cur.ctx.columns) block.columns = cur.ctx.columns;
        blocks.push(block);
      }
      cur = null;
    }

    function applyFmtFields(run, fmt, includeLink) {
      if (fmt.script) run.script = fmt.script;
      if (includeLink && fmt.link) run.link = fmt.link;
      if (fmt.commentId !== undefined) run.commentId = fmt.commentId;
      if (fmt.revision) run.revision = { ...fmt.revision };
      BOOL_KEYS.forEach((k) => {
        if (fmt[k]) run[k] = true;
      });
      VALUE_KEYS.forEach((k) => {
        if (fmt[k] !== undefined) run[RUN_FIELDS[k]] = fmt[k];
      });
      return run;
    }

    function appendText(text, ctx, fmt) {
      if (!text) return;
      if (!cur) cur = { ctx, runs: [] };
      const last = cur.runs[cur.runs.length - 1];
      if (sameFormat(last, fmt)) {
        last.text += text;
        return;
      }
      cur.runs.push(applyFmtFields({ text }, fmt, true));
    }

    // A page-number field (`doc-page-frame.js`) is a run of its own with the format around it.
    function walkField(node, ctx, fmt) {
      if (!cur) cur = { ctx, runs: [] };
      cur.runs.push(applyFmtFields({ text: "", field: FIELD_KEYS[node.dataset.eoField] }, fmt, false));
    }

    function walkNoteRef(node, ctx, fmt) {
      const kind = node.dataset.eoNoteKind;
      const id = Number(node.dataset.eoNoteId);
      if (!Number.isInteger(id) || id < 1) return;
      if (!cur) cur = { ctx, runs: [] };
      cur.runs.push(applyFmtFields({ text: "", noteRef: { kind, id } }, fmt, false));
    }

    function walkPointComment(node, ctx, fmt) {
      const cid = Number(node.dataset.eoCommentId);
      if (!Number.isInteger(cid) || cid < 1) return;
      if (!cur) cur = { ctx, runs: [] };
      cur.runs.push(applyFmtFields({ text: "", commentId: cid, _keepEmpty: true }, { ...fmt, commentId: cid }, false));
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
      const isContainerList = tag === "UL" || tag === "OL";
      const inList = isContainerList || ctx.inList === true;
      const ulDepth = isContainerList ? (ctx.inList ? (ctx.ulDepth || 0) + 1 : 0) : (ctx.ulDepth || 0);
      const listParent = tag === "LI" ? node.parentElement : null;
      let nextList = ctx.list;
      if (listParent) {
        const parentTag = (listParent.tagName || "").toUpperCase();
        if (parentTag === "OL") nextList = "ordered";
        else if (parentTag === "UL") nextList = listParent.dataset?.eoList === "task" ? "task" : "bullet";
      }
      const checked = tag === "LI"
        ? (nextList === "task" && node.dataset?.eoChecked === "true")
        : Boolean(ctx.checked);
      const attrLevel = tag === "LI" ? Math.max(0, Number.parseInt(node.dataset?.eoListLevel || "0", 10) || 0) : 0;
      const listLevel = tag === "LI" ? Math.min(8, ulDepth + attrLevel) : (ctx.listLevel || 0);
      const listNumber = tag === "LI"
        ? (nextList === "ordered" && node.dataset?.eoListNum ? String(node.dataset.eoListNum) : undefined)
        : ctx.listNumber;
      const colsMatch = COLS_RE.exec(String(node.dataset?.eoCols || ""));
      const colsCount = colsMatch ? Number(colsMatch[1]) : 0;
      const columns = colsCount >= 2 && colsCount <= 16
        ? { count: colsCount, equal: colsMatch[2] === "1", gapTwips: Number(colsMatch[3]) }
        : ctx.columns;
      return {
        align: normalizeAlign(node.style?.textAlign) || normalizeAlign(node.dataset?.eoZone) || ctx.align,
        heading: headingMatch ? Number(headingMatch[1]) : ctx.heading,
        toc: tocRole(node) ?? ctx.toc,
        bookmark: headingMatch ? bookmarkNumber(node.id) : undefined,
        tocPage: tocPage(node),
        tocDots: node.dataset?.eoTocDots === "true",
        pageBreak: node.dataset?.eoBreak === "page",
        frameCopy: node.dataset?.eoFrameCopy === "1",
        inList,
        ulDepth,
        list: nextList,
        checked,
        listLevel,
        listNumber,
        columns,
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
      if (tag === "SUP" && (node.dataset?.eoNoteKind === "footnote" || node.dataset?.eoNoteKind === "endnote")) {
        return walkNoteRef(node, ctx, fmt);
      }
      if (tag === "SPAN" && node.dataset?.eoCommentId !== undefined && node.classList?.contains("eo-comment-point")) {
        return walkPointComment(node, ctx, fmt);
      }
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

  // The header and footer sections (including first/even page variants) of an editor root as paragraph blocks;
  // a section without text or field is left out, while explicit first/even flags are preserved.
  function collectPageFrame(root, options = {}) {
    const frame = {};
    for (const section of Array.from(root?.children || [])) {
      const kind = section.dataset?.eoHf;
      if (!FRAME_KINDS.has(kind) || Object.hasOwn(frame, kind)) continue;
      const isZones = section.dataset?.eoHfZones === "true";
      const rawBlocks = collectDocBlocks(section, options).filter((b) => b.type === "paragraph");
      const blocks = (isZones ? rawBlocks.filter((b) => b.runs.length > 0) : rawBlocks).slice(0, MAX_FRAME_PARAGRAPHS);
      if (blocks.some((b) => b.runs.length > 0)) frame[kind] = blocks;
    }
    if (options.firstPageDifferent === true || root?.dataset?.eoFirstPage === "true" || frame.firstHeader || frame.firstFooter) {
      frame.firstPageDifferent = true;
    }
    if (options.evenOddDifferent === true || root?.dataset?.eoEvenOdd === "true" || frame.evenHeader || frame.evenFooter) {
      frame.evenOddDifferent = true;
    }
    return frame;
  }

  const MAX_ANNOTATION_ITEMS = 2048;

  function collectDocAnnotations(root) {
    const out = {};
    if (!root) return out;
    if (root.dataset?.eoTrackChanges === "true") out.trackChanges = true;
    const refKeys = new Set();
    const commentIds = new Set();
    const scanBody = (node) => {
      for (const child of node.childNodes || []) {
        if (child.nodeType !== 1) continue;
        const tag = (child.tagName || "").toUpperCase();
        const ds = child.dataset;
        if (tag === "FOOTER" || child.id === "hwp-manuscript-preview" || (ds && (ds.eoNotes !== undefined || ds.eoComments !== undefined || ds.eoHf !== undefined))) continue;
        if (tag === "SUP" && (ds?.eoNoteKind === "footnote" || ds?.eoNoteKind === "endnote")) {
          const id = Number(ds.eoNoteId);
          if (Number.isInteger(id) && id >= 1) refKeys.add(ds.eoNoteKind + ":" + id);
          continue;
        }
        if (ds?.eoCommentId !== undefined) {
          const cid = Number(ds.eoCommentId);
          if (Number.isInteger(cid) && cid >= 1) commentIds.add(cid);
        }
        scanBody(child);
      }
    };
    scanBody(root);
    const notes = [];
    for (const kind of ["footnote", "endnote"]) {
      for (const sec of Array.from(root.children || [])) {
        if (sec.dataset?.eoNotes !== kind) continue;
        for (const item of Array.from(sec.children || [])) {
          if (notes.length >= MAX_ANNOTATION_ITEMS) break;
          if (!item.classList?.contains("eo-doc-note-item")) continue;
          const id = Number(item.dataset?.eoNoteId);
          if (!Number.isInteger(id) || id < 1 || !refKeys.has(kind + ":" + id)) continue;
          const bodies = item.querySelectorAll ? Array.from(item.querySelectorAll(".eo-doc-note-body")) : [];
          const text = (bodies.length > 0 ? bodies.map((b) => String(b.textContent || "")).join("\n") : String(item.textContent || "")).slice(0, 65536);
          notes.push({ id, kind, text });
        }
      }
    }
    if (notes.length > 0) out.notes = notes;
    const comments = [];
    for (const sec of Array.from(root.children || [])) {
      if (sec.dataset?.eoComments === undefined) continue;
      for (const art of Array.from(sec.children || [])) {
        if (comments.length >= MAX_ANNOTATION_ITEMS) break;
        const id = Number(art.dataset?.eoCommentId);
        if (!Number.isInteger(id) || id < 1 || !commentIds.has(id)) continue;
        comments.push({
          id,
          author: String(art.dataset?.eoAuthor || "작성자").slice(0, 128),
          date: String(art.dataset?.eoDate || "").slice(0, 64),
          text: String(art.textContent || "").slice(0, 65536),
        });
      }
    }
    if (comments.length > 0) out.comments = comments;
    return out;
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

  function internFonts() {
    const codec = host.__EASYOFFICE_SHEET_STYLE_CODEC__;
    const limit = codec?.MAX_FONTS ?? 512;
    const names = [];
    const entries = new Map();
    const intern = (raw) => {
      const clean = codec?.normalizeFontFamily ? codec.normalizeFontFamily(raw) : "";
      if (!clean) return 0;
      const key = clean.toLowerCase();
      let at = entries.get(key);
      if (at === undefined) {
        at = names.length < limit ? names.push(clean) : 0;
        entries.set(key, at);
      }
      return at;
    };
    return { names, intern };
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
      const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;
      const numFmts = internNumFmts();
      const fonts = internFonts();
      const cells = [];
      for (const ref of Object.keys(store)) {
        const entry = store[ref];
        const raw = String(entry?.formula || entry?.value || "");
        const style = codec ? codec.styleToPayload(entry?.style, numFmts.intern, fonts.intern) : null;
        if (!raw && !style) continue;
        const t = entry?.formula ? undefined : cellTypeApi?.wireTag(raw, entry?.t);
        const cell = { ref, raw };
        if (t) cell.t = t;
        if (style) cell.style = style;
        cells.push(cell);
      }
      const columns = buildColumnsPayload(columnWidths?.[name]);
      const sheet = columns.length > 0 ? { name, cells, columns } : { name, cells };
      if (numFmts.codes.length > 0) sheet.numFmts = numFmts.codes;
      if (fonts.names.length > 0) sheet.fonts = fonts.names;
      // The arrays that spill (PD-217): the cells they filled are in `cells` as values.
      const spillAreas = host.__EASYOFFICE_SHEET_SPILL__?.areasOf(store) || [];
      if (spillAreas.length > 0) sheet.spills = spillAreas;
      const links = host.__EASYOFFICE_SHEET_LINK__?.payloadOf(store) ?? [];
      if (links.length > 0) sheet.links = links;
      const rules = rulesBySheet?.[name];
      if (rules && ((rules.cf || []).length > 0 || (rules.dv || []).length > 0)) sheet.rules = rules;
      // Merged ranges and hidden rows and columns (PD-194): `merges`, `hiddenRows`, `hiddenCols`.
      Object.assign(sheet, layoutBySheet?.[name]);
      if (Array.isArray(sheet.images) && sheet.images.length > 0) {
        const utils = host.__EASYOFFICE_SHEET_UTILS__;
        sheet.images = sheet.images.map(({ imageId, naturalWidthPx, naturalHeightPx, ...img }) => ({
          ...img,
          data: (imageId ? utils?.getDocImage(imageId)?.base64 : "") || img.data || "",
        }));
      }
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
    collectDocAnnotations,
    plainParagraphBlocks,
    evaluateDocTableFormulas,
    buildSheetsPayload,
    buildColumnsPayload,
    sanitizeFileBaseName,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
