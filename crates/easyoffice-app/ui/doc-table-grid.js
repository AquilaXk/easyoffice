(function (host) {
  "use strict";

  // Structure edits of one table of the document editor: rows and columns added and deleted,
  // cells merged and split (colspan/rowspan), the block sum. The table is read as the HTML table model lays it
  // out (a slot grid: a cell with a span owns the slots to its right and below), the same model the save payload
  // reads (`save-payload.js` `readTableGrid`), so what these edits make is what is written to the file.
  // Every edit returns `{ changed, cell }` (`cell`: where the caret goes) or `{ changed: false, error }`.

  const CELL_TAGS = new Set(["TD", "TH"]);
  const ROW_TAGS = new Set(["THEAD", "TBODY", "TFOOT"]);
  const MSG_NO_MERGE = "합칠 셀 없음";
  const MSG_NO_SPLIT = "나눌 셀 없음";
  const MSG_NO_NUMBERS = "합할 숫자 없음";
  const NUMBER_RE = /^[+-]?(\d+(?:\.\d*)?|\.\d+)([eE][+-]?\d+)?$/;
  const MAX_FRACTION_DIGITS = 10;

  const tagOf = (el) => String(el?.tagName || "").toUpperCase();
  const isCell = (el) => CELL_TAGS.has(tagOf(el));
  const spanOf = (value) => Math.max(1, Math.trunc(Number(value)) || 1);

  function emptyCell(document) {
    const td = document.createElement("td");
    td.appendChild(document.createElement("br"));
    return td;
  }

  // The `tr` elements of a table in order.
  function rowsOf(table) {
    const rows = [];
    for (const child of Array.from(table.children)) {
      if (tagOf(child) === "TR") rows.push(child);
      else if (ROW_TAGS.has(tagOf(child))) rows.push(...Array.from(child.children).filter((el) => tagOf(el) === "TR"));
    }
    return rows;
  }

  // `{ trs, slots, at, width }`: `slots[r][c]` is the cell that covers the slot, `at.get(cell)` is
  // `{ row, col, rowSpan, colSpan }`. A span stops at the last row and at slots another cell already covers.
  function layoutOf(table) {
    const trs = rowsOf(table);
    const slots = trs.map(() => []);
    const at = new Map();
    trs.forEach((tr, row) => {
      let col = 0;
      for (const cell of Array.from(tr.children).filter(isCell)) {
        while (slots[row][col]) col += 1;
        const colSpan = spanOf(cell.colSpan);
        let rowSpan = Math.min(spanOf(cell.rowSpan), trs.length - row);
        let fit = 1;
        while (fit < colSpan && !slots[row][col + fit]) fit += 1;
        rowSpan = Math.max(1, rowSpan);
        for (let dr = 0; dr < rowSpan; dr += 1) {
          for (let dc = 0; dc < fit; dc += 1) slots[row + dr][col + dc] = cell;
        }
        at.set(cell, { row, col, rowSpan, colSpan: fit });
        col += fit;
      }
    });
    const width = slots.reduce((most, row) => Math.max(most, row.length), 0);
    return { trs, slots, at, width };
  }

  // Puts `cell` into row `tr` before the first cell that starts at `col` or later (else last).
  function placeCell(layout, tr, cell, col) {
    const next = Array.from(tr.children).find((el) => isCell(el) && layout.at.has(el) && layout.at.get(el).col >= col);
    tr.insertBefore(cell, next ?? null);
  }

  function insertRow(table, cell, above) {
    const layout = layoutOf(table);
    const pos = layout.at.get(cell);
    if (!pos) return { changed: false };
    const line = above ? pos.row : pos.row + pos.rowSpan;
    const document = table.ownerDocument;
    const tr = document.createElement("tr");
    const grown = new Set();
    for (let col = 0; col < layout.width; col += 1) {
      const over = line > 0 && line < layout.trs.length ? layout.slots[line][col] : null;
      const crossing = over && layout.at.get(over).row < line ? over : null;
      if (!crossing) tr.appendChild(emptyCell(document));
      else if (!grown.has(crossing)) {
        grown.add(crossing);
        crossing.rowSpan = layout.at.get(crossing).rowSpan + 1;
      }
    }
    const anchor = layout.trs[line] ?? null;
    const parent = (anchor ?? layout.trs.at(-1)).parentNode;
    parent.insertBefore(tr, anchor);
    return { changed: true, cell: tr.firstChild ?? cell };
  }

  function insertColumn(table, cell, left) {
    const layout = layoutOf(table);
    const pos = layout.at.get(cell);
    if (!pos) return { changed: false };
    const line = left ? pos.col : pos.col + pos.colSpan;
    const document = table.ownerDocument;
    const grown = new Set();
    let made = null;
    layout.trs.forEach((tr, row) => {
      const over = line < layout.width ? layout.slots[row][line] : null;
      const crossing = over && layout.at.get(over).col < line ? over : null;
      if (crossing) {
        if (!grown.has(crossing)) {
          grown.add(crossing);
          crossing.colSpan = layout.at.get(crossing).colSpan + 1;
        }
        return;
      }
      const fresh = emptyCell(document);
      placeCell(layout, tr, fresh, line);
      if (row === pos.row) made = fresh;
    });
    return { changed: true, cell: made ?? cell };
  }

  // The cells of rows `first..last` that start inside and run past the last one go on into the row after it.
  function moveSpanningCells(layout, first, last) {
    for (const [cell, pos] of layout.at) {
      if (pos.row < first || pos.row > last || pos.row + pos.rowSpan - 1 <= last) continue;
      cell.rowSpan = pos.row + pos.rowSpan - 1 - last;
      placeCell(layout, layout.trs[last + 1], cell, pos.col);
    }
  }

  // Removes the table; the caret goes to the block after it (else before it).
  function removeTable(table) {
    const target = table.nextSibling ?? table.previousSibling;
    table.remove();
    return { changed: true, removed: true, target };
  }

  function deleteRows(table, { top, bottom }) {
    const layout = layoutOf(table);
    if (top === 0 && bottom === layout.trs.length - 1) return removeTable(table);
    moveSpanningCells(layout, top, bottom);
    for (const [cell, pos] of layout.at) {
      const end = pos.row + pos.rowSpan - 1;
      if (pos.row < top && end >= top) cell.rowSpan = pos.rowSpan - (Math.min(bottom, end) - top + 1);
    }
    for (let row = top; row <= bottom; row += 1) layout.trs[row].remove();
    const keep = layout.trs[bottom + 1] ?? layout.trs[top - 1];
    return { changed: true, cell: Array.from(keep.children).find(isCell) };
  }

  function deleteColumns(table, { left, right }, row) {
    const layout = layoutOf(table);
    if (left === 0 && right === layout.width - 1) return removeTable(table);
    for (const [cell, pos] of layout.at) {
      const overlap = Math.min(right, pos.col + pos.colSpan - 1) - Math.max(left, pos.col) + 1;
      if (overlap <= 0) continue;
      if (overlap >= pos.colSpan) cell.remove();
      else cell.colSpan = pos.colSpan - overlap;
    }
    const after = layoutOf(table);
    return { changed: true, cell: after.slots[row][Math.min(left, after.slots[row].length - 1)] };
  }

  // The rectangle of slots covered by cells `from` and `to` of a table, grown to hold every cell it touches.
  function rectOf(layout, from, to) {
    const a = layout.at.get(from);
    const b = layout.at.get(to);
    if (!a || !b) return null;
    const rect = {
      top: Math.min(a.row, b.row),
      left: Math.min(a.col, b.col),
      bottom: Math.max(a.row + a.rowSpan, b.row + b.rowSpan) - 1,
      right: Math.max(a.col + a.colSpan, b.col + b.colSpan) - 1,
    };
    let grew = true;
    while (grew) {
      grew = false;
      for (const pos of layout.at.values()) {
        const hits = pos.row <= rect.bottom && pos.row + pos.rowSpan - 1 >= rect.top && pos.col <= rect.right && pos.col + pos.colSpan - 1 >= rect.left;
        if (!hits) continue;
        const next = {
          top: Math.min(rect.top, pos.row), left: Math.min(rect.left, pos.col),
          bottom: Math.max(rect.bottom, pos.row + pos.rowSpan - 1), right: Math.max(rect.right, pos.col + pos.colSpan - 1),
        };
        if (next.top !== rect.top || next.left !== rect.left || next.bottom !== rect.bottom || next.right !== rect.right) {
          Object.assign(rect, next);
          grew = true;
        }
      }
    }
    return rect;
  }

  const isBlank = (cell) => String(cell.textContent || "").trim() === "";

  // Moves the text of `cell` after the anchor's, one line each.
  function appendContent(anchor, cell) {
    if (isBlank(cell)) return;
    if (isBlank(anchor)) anchor.replaceChildren();
    else anchor.appendChild(anchor.ownerDocument.createElement("br"));
    for (const node of Array.from(cell.childNodes)) anchor.appendChild(node);
  }

  function mergeCells(table, from, to) {
    const layout = layoutOf(table);
    const rect = rectOf(layout, from, to);
    if (!rect) return { changed: false, error: MSG_NO_MERGE };
    const anchor = layout.slots[rect.top][rect.left];
    const covered = new Set();
    for (let r = rect.top; r <= rect.bottom; r += 1) {
      for (let c = rect.left; c <= rect.right; c += 1) covered.add(layout.slots[r][c]);
    }
    if (covered.size < 2) return { changed: false, error: MSG_NO_MERGE };
    for (const cell of covered) {
      if (cell === anchor) continue;
      appendContent(anchor, cell);
      cell.remove();
    }
    anchor.colSpan = rect.right - rect.left + 1;
    anchor.rowSpan = rect.bottom - rect.top + 1;
    return { changed: true, cell: anchor };
  }

  function splitCell(table, cell) {
    const layout = layoutOf(table);
    const pos = layout.at.get(cell);
    if (!pos || pos.rowSpan * pos.colSpan < 2) return { changed: false, error: MSG_NO_SPLIT };
    const document = table.ownerDocument;
    cell.colSpan = 1;
    cell.rowSpan = 1;
    for (let row = pos.row; row < pos.row + pos.rowSpan; row += 1) {
      for (let col = pos.col; col < pos.col + pos.colSpan; col += 1) {
        if (row !== pos.row || col !== pos.col) placeCell(layout, layout.trs[row], emptyCell(document), col);
      }
    }
    return { changed: true, cell };
  }

  function numberOf(cell) {
    const text = String(cell.textContent || "").replaceAll(",", "").replaceAll(/\s+/g, "");
    return NUMBER_RE.test(text) ? Number(text) : null;
  }

  function showNumber(n) {
    return Number.isInteger(n) ? String(n) : n.toFixed(MAX_FRACTION_DIGITS).replace(/0+$/, "").replace(/\.$/, "");
  }

  function setNumber(cell, n) {
    cell.replaceChildren(cell.ownerDocument.createTextNode(showNumber(n)));
  }

  // The sum of the numbers in `cells`; null when there is none.
  function sumOf(cells) {
    const numbers = cells.map(numberOf).filter((n) => n !== null);
    return numbers.length === 0 ? null : numbers.reduce((a, b) => a + b, 0);
  }

  // The distinct cells of one column (rows `top..bottom`) or one row (columns `left..right`), merged cells once.
  function distinctCells(layout, slotAt, from, to) {
    const cells = new Set();
    for (let i = from; i <= to; i += 1) if (slotAt(i)) cells.add(slotAt(i));
    return Array.from(cells);
  }

  const columnCells = (layout, col, top, bottom) => distinctCells(layout, (r) => layout.slots[r][col], top, bottom);
  const rowCells = (layout, row, left, right) => distinctCells(layout, (c) => layout.slots[row][c], left, right);

  // 블록 합계. A block of one row puts its sum in the cell to its right, any other block puts one sum per column
  // in the row below it (rows and columns are added when the block is at the edge of the table). A single cell
  // sums the numbers above it, else the ones to its left, into itself.
  function blockSum(table, from, to) {
    const layout = layoutOf(table);
    const rect = rectOf(layout, from, to);
    if (!rect) return { changed: false, error: MSG_NO_NUMBERS };
    if (from === to) return sumIntoCell(layout, from);
    if (rect.top === rect.bottom) return sumRowOut(table, layout, rect);
    return sumColumnsOut(table, layout, rect);
  }

  function sumIntoCell(layout, cell) {
    const pos = layout.at.get(cell);
    const total = sumOf(columnCells(layout, pos.col, 0, pos.row - 1)) ?? sumOf(rowCells(layout, pos.row, 0, pos.col - 1));
    if (total === null) return { changed: false, error: MSG_NO_NUMBERS };
    setNumber(cell, total);
    return { changed: true, cell };
  }

  function sumRowOut(table, layout, rect) {
    const total = sumOf(rowCells(layout, rect.top, rect.left, rect.right));
    if (total === null) return { changed: false, error: MSG_NO_NUMBERS };
    const last = layout.slots[rect.top][rect.right];
    let target = layout.slots[rect.top][rect.right + 1];
    if (!target) target = insertColumn(table, last, false).cell;
    setNumber(target, total);
    return { changed: true, cell: target };
  }

  function sumColumnsOut(table, layout, rect) {
    const totals = [];
    for (let col = rect.left; col <= rect.right; col += 1) totals.push([col, sumOf(columnCells(layout, col, rect.top, rect.bottom))]);
    if (totals.every(([, total]) => total === null)) return { changed: false, error: MSG_NO_NUMBERS };
    if (rect.bottom + 1 >= layout.trs.length) insertRow(table, layout.slots[rect.bottom][rect.left], false);
    const fresh = layoutOf(table);
    let first = null;
    for (const [col, total] of totals) {
      if (total === null) continue;
      const target = fresh.slots[rect.bottom + 1][col];
      setNumber(target, total);
      first ??= target;
    }
    return { changed: true, cell: first };
  }

  // The cell that holds `node` (a text node, an element, or a row with an offset), as far as `table`.
  function cellIn(table, node, offset = 0) {
    let el = node;
    if (el && tagOf(el) === "TR") el = Array.from(el.children).filter(isCell)[Math.min(offset, el.children.length - 1)] ?? null;
    while (el && el !== table) {
      if (isCell(el) && rowsOf(table).some((tr) => tr === el.parentNode)) return el;
      el = el.parentNode;
    }
    return null;
  }

  // The innermost table of the editor `root` that holds `node`.
  function tableAt(root, node) {
    let el = node;
    while (el && el !== root) {
      if (tagOf(el) === "TABLE") return el;
      el = el.parentNode;
    }
    return null;
  }

  host.__EASYOFFICE_DOC_TABLE_GRID__ = {
    layoutOf, rectOf, insertRow, insertColumn, deleteRows, deleteColumns, mergeCells, splitCell, blockSum, cellIn, tableAt,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
