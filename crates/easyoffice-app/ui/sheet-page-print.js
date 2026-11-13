(function (host) {
  "use strict";

  // The pages of a printed sheet (PD-213). The page setup (`sheet-page.js`) decides the paper, the margins, the scale, the
  // centring, the repeated rows and columns, the grid lines and the headings; the print area (or the used range) decides the
  // cells. The pages are cut from the same column widths and row heights the grid shows, so a printed page holds the cells
  // its screen size says it holds; nothing is measured in the browser. Pages go down first, then over (the usual print order).
  //
  // Cost: one pass over the rows and columns of a range to size them, one greedy pass to cut each axis, and one table cell per
  // printed cell; a range over the cell cap or the page cap is refused (fail-closed).

  const PX_PER_MM = 96 / 25.4;
  const DEFAULT_MARGIN_MM = 10;
  const MM_PER_MIL = 0.0254;
  const HEADING_WIDTH_PX = 36;
  const HEADING_HEIGHT_PX = 22;
  const MAX_PRINT_CELLS = 200000;
  const MAX_PRINT_PAGES = 5000;
  const MSG_TOO_BIG = "인쇄 범위가 너무 커서 한 번에 인쇄할 수 없습니다.";
  const PAGE_CLASS = "eo-print-sheet-page";
  const SHEET_CLASS = "eo-print-sheet";

  const mm = (v) => Math.round(v * 100) / 100;
  const px = (v) => Math.round(v * 100) / 100 + "px";

  // The sizes (px) of the lines `first` to `last` of one axis; a hidden line is 0.
  function sizesOf(first, last, sizeOf) {
    const out = new Float64Array(last - first + 1);
    for (let i = first; i <= last; i += 1) out[i - first] = sizeOf(i);
    return out;
  }

  // Cuts `sizes` into groups of at most `budget` (a line wider than the budget has a group of its own); the groups after the
  // first one carry the repeated lines `title` (`[first, last]`, indexes into `sizes`) when those lie before the group.
  function cut(sizes, budget, title) {
    const prefix = new Float64Array(sizes.length + 1);
    for (let i = 0; i < sizes.length; i += 1) prefix[i + 1] = prefix[i] + sizes[i];
    const titleSize = title ? prefix[title[1] + 1] - prefix[title[0]] : 0;
    const groups = [];
    let i = 0;
    while (i < sizes.length) {
      const repeated = groups.length > 0 && title && i > title[1];
      const room = budget - (repeated ? titleSize : 0);
      let j = i + 1;
      while (j < sizes.length && prefix[j + 1] - prefix[i] <= room) j += 1;
      groups.push({ first: i, last: j - 1, title: Boolean(repeated) });
      i = j;
    }
    return { groups, total: prefix[sizes.length] };
  }

  // The scale a setup prints at for a range of `width` by `height` px on a content box `boxW` by `boxH`: its percentage, or the
  // largest scale (never above 100%) at which the range fits the pages across and down (0 leaves that direction free).
  function scaleOf(page, width, height, boxW, boxH) {
    const s = page?.scale;
    if (!s || s.percent !== undefined) return (s?.percent ?? 100) / 100;
    let fit = 1;
    if (s.fitWidth > 0 && width > 0) fit = Math.min(fit, (s.fitWidth * boxW) / width);
    if (s.fitHeight > 0 && height > 0) fit = Math.min(fit, (s.fitHeight * boxH) / height);
    return Math.max(fit, 0.1);
  }

  const paperOf = (page, core) => core.paperMm(page);

  function marginsMm(page) {
    const m = page?.margins;
    if (!m) return { top: DEFAULT_MARGIN_MM, bottom: DEFAULT_MARGIN_MM, left: DEFAULT_MARGIN_MM, right: DEFAULT_MARGIN_MM };
    return { top: m.top * MM_PER_MIL, bottom: m.bottom * MM_PER_MIL, left: m.left * MM_PER_MIL, right: m.right * MM_PER_MIL };
  }

  // The printed pages of `boxes` (1-based `{ r1, c1, r2, c2 }`), or throws when a range is too big. `cellAt(row, col)` is the
  // cell's `{ text, entry }` or null; `renderCell(td, cell)` fills a printed cell.
  function buildPages({ document, boxes, geometry, page, core, cellAt, renderCell, rules, pageClass }) {
    const paper = paperOf(page, core);
    const margin = marginsMm(page);
    const paperClass = rules.classFor(mm(paper.width), mm(paper.height));
    const boxW = (paper.width - margin.left - margin.right) * PX_PER_MM;
    const boxH = (paper.height - margin.top - margin.bottom) * PX_PER_MM;
    const headings = page?.headings === true;
    const headW = headings ? HEADING_WIDTH_PX : 0;
    const headH = headings ? HEADING_HEIGHT_PX : 0;
    const out = [];
    let cells = 0;
    for (const box of boxes) {
      cells += (box.r2 - box.r1 + 1) * (box.c2 - box.c1 + 1);
      if (cells > MAX_PRINT_CELLS) throw new Error(MSG_TOO_BIG);
      const colSizes = sizesOf(box.c1, box.c2, (c) => (geometry.colHidden(c) ? 0 : geometry.colWidth(c)));
      const rowSizes = sizesOf(box.r1, box.r2, (r) => (geometry.rowHidden(r) ? 0 : geometry.rowTop(r + 1) - geometry.rowTop(r)));
      const totalW = colSizes.reduce((a, b) => a + b, 0) + headW;
      const totalH = rowSizes.reduce((a, b) => a + b, 0) + headH;
      const scale = scaleOf(page, totalW, totalH, boxW, boxH);
      const titleRows = page?.titleRows ? [page.titleRows[0] - box.r1, page.titleRows[1] - box.r1] : null;
      const titleCols = page?.titleCols ? [page.titleCols[0] - box.c1, page.titleCols[1] - box.c1] : null;
      const inside = (t, size) => (t && t[0] >= 0 && t[1] < size ? t : null);
      const colCut = cut(colSizes, boxW / scale - headW, inside(titleCols, colSizes.length));
      const rowCut = cut(rowSizes, boxH / scale - headH, inside(titleRows, rowSizes.length));
      if (out.length + colCut.groups.length * rowCut.groups.length > MAX_PRINT_PAGES) throw new Error(MSG_TOO_BIG);
      for (const cg of colCut.groups) {
        for (const rg of rowCut.groups) {
          out.push(tilePage({ document, box, cg, rg, colSizes, rowSizes, titleRows: inside(titleRows, rowSizes.length), titleCols: inside(titleCols, colSizes.length), page, scale, margin, paperClass, pageClass, headW, headH, cellAt, renderCell }));
        }
      }
    }
    return out;
  }

  // The lines a group shows: its own and, after the first group, the repeated ones before them.
  function linesOf(group, title) {
    const own = [];
    if (group.title && title) for (let i = title[0]; i <= title[1]; i += 1) own.push(i);
    for (let i = group.first; i <= group.last; i += 1) own.push(i);
    return own;
  }

  function tilePage(t) {
    const { document, box, page, scale, margin } = t;
    const el = document.createElement("div");
    el.className = t.pageClass + " " + t.paperClass + " " + PAGE_CLASS;
    el.style.padding = [margin.top, margin.right, margin.bottom, margin.left].map((v) => mm(v) + "mm").join(" ");
    el.style.justifyContent = page?.centerH ? "center" : "flex-start";
    el.style.alignItems = page?.centerV ? "center" : "flex-start";
    const cols = linesOf(t.cg, t.titleCols);
    const rows = linesOf(t.rg, t.titleRows);
    const table = document.createElement("table");
    table.className = SHEET_CLASS + (page?.grid ? " eo-print-grid" : "");
    let width = t.headW;
    let height = t.headH;
    const colgroup = document.createElement("colgroup");
    if (t.headW > 0) colgroup.appendChild(sized(document, "col", "width", t.headW));
    for (const i of cols) {
      width += t.colSizes[i];
      if (t.colSizes[i] > 0) colgroup.appendChild(sized(document, "col", "width", t.colSizes[i]));
    }
    table.appendChild(colgroup);
    const body = document.createElement("tbody");
    if (t.headH > 0) body.appendChild(headingRow(document, cols, box, t));
    for (const i of rows) {
      if (t.rowSizes[i] <= 0) continue;
      height += t.rowSizes[i];
      const r = box.r1 + i;
      const tr = sized(document, "tr", "height", t.rowSizes[i]);
      if (t.headW > 0) tr.appendChild(heading(document, "td", String(r)));
      for (const k of cols) {
        if (t.colSizes[k] <= 0) continue;
        const td = document.createElement("td");
        const cell = t.cellAt(r, box.c1 + k);
        if (cell) t.renderCell(td, cell);
        tr.appendChild(td);
      }
      body.appendChild(tr);
    }
    table.appendChild(body);
    table.style.width = px(width);
    const holder = document.createElement("div");
    holder.className = "eo-print-sheet-box";
    holder.style.width = px(width * scale);
    holder.style.height = px(height * scale);
    if (scale !== 1) {
      table.style.transform = "scale(" + scale + ")";
      table.style.transformOrigin = "0 0";
    }
    holder.appendChild(table);
    el.appendChild(holder);
    return el;
  }

  function sized(document, tag, prop, size) {
    const el = document.createElement(tag);
    el.style[prop] = px(size);
    return el;
  }

  function heading(document, tag, text) {
    const el = document.createElement(tag);
    el.className = "eo-print-head";
    el.textContent = text;
    return el;
  }

  // The row of column letters above the cells.
  function headingRow(document, cols, box, t) {
    const tr = sized(document, "tr", "height", t.headH);
    if (t.headW > 0) tr.appendChild(heading(document, "td", ""));
    for (const k of cols) {
      if (t.colSizes[k] <= 0) continue;
      tr.appendChild(heading(document, "td", host.__EASYOFFICE_SHEET_REFS__.colName(box.c1 + k)));
    }
    return tr;
  }

  host.__EASYOFFICE_SHEET_PAGE_PRINT__ = { buildPages, cut, scaleOf, MAX_PRINT_CELLS, MAX_PRINT_PAGES, MSG_TOO_BIG };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
