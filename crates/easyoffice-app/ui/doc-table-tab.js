(function (host) {
  "use strict";

  // Tab and Shift+Tab inside a table of the document editor. Reference implementation: rhwp-studio
  // (https://github.com/edwardkim/rhwp, commit 1a76570e833917d15817415a53c09ad61ab3203f, MIT, Copyright (c) 2025-2026
  // Edward Kim): `case 'Tab'` of `rhwp-studio/src/engine/input-handler-keyboard.ts` (lines 1541-1560),
  // `insertRowAfterLastTableCellByTab` (lines 211-244), `CursorState.getCellReadingOrder`, `moveToCellNext`,
  // `moveToCellPrev` and `exitTable` of `rhwp-studio/src/engine/cursor.ts` (lines 1161-1245 and 1332-1345). The branches
  // keep the reference order: Shift+Tab goes to the previous cell and puts the caret at its end; Tab in the last cell of
  // the reading order adds a row after the last row of that cell and puts the caret in the new row's first cell; any
  // other Tab goes to the next cell in reading order and puts the caret at its start. A cell that a span covers is not
  // in the reading order (the cell that owns it is, once). Tab in the last cell of a table inside a cell, and Shift+Tab in
  // the first cell, leave the table: to the start of the block after it, or to the block before it.
  // The adapter differences: the reference reads the order from the layout engine (`getTableCellBboxes`, sorted by row,
  // then column), here `layoutOf` of `doc-table-grid.js` is that order already (cells are listed row by row, left to
  // right); the reference's paragraph that holds the table is, in the DOM, the block next to the table.

  const LEAF_TAGS = new Set(["BR", "IMG"]);
  const tagOf = (el) => String(el?.tagName || "").toUpperCase();
  const isElement = (node) => node?.nodeType === 1;

  // Text blocks, lists and tables of the body, not the pictures and the page frame (header, footer, notes) around it.
  const isBodyFlow = (el) => isElement(el) && tagOf(el) !== "IMG" && !host.__EASYOFFICE_DOC_PAGE_META__.isTrailingMetaNode(el);

  const canEnter = (node) => isElement(node) && !LEAF_TAGS.has(tagOf(node));

  // The first caret place of `node`: inside the first block when there is one, else at the start of the node.
  function startPoint(node) {
    let cur = node;
    while (canEnter(cur.firstChild)) cur = cur.firstChild;
    return { node: cur, offset: 0 };
  }

  // The last caret place of `node`: before a trailing line break, which only keeps an empty block open.
  function endPoint(node) {
    let cur = node;
    while (canEnter(cur.lastChild)) cur = cur.lastChild;
    const count = cur.childNodes.length;
    return { node: cur, offset: tagOf(cur.lastChild) === "BR" ? count - 1 : count };
  }

  function createDocTableTab({ grid, withUndo, placeCaretAt }) {
    // Ref `getCellReadingOrder`.
    const readingOrder = (table) => Array.from(grid.layoutOf(table).at.keys());

    const outermost = (table) => {
      let outer = table;
      for (let el = table.parentNode; el; el = el.parentNode) {
        if (tagOf(el) === "TABLE") outer = el;
      }
      return outer;
    };
    const isNested = (table) => outermost(table) !== table;

    function neighbour(table, delta) {
      const outer = outermost(table);
      for (let el = delta > 0 ? outer.nextSibling : outer.previousSibling; el; el = delta > 0 ? el.nextSibling : el.previousSibling) {
        if (isBodyFlow(el)) return el;
      }
      return null;
    }

    // Ref `exitTable`: no block to go to leaves the caret where it is.
    function exitTable(table, delta) {
      const target = neighbour(table, delta);
      if (!target) return;
      const point = delta > 0 ? startPoint(target) : endPoint(target);
      placeCaretAt(point.node, point.offset);
    }

    function moveToCellNext({ table, from }) {
      const order = readingOrder(table);
      const at = order.indexOf(from);
      if (at < 0 || at + 1 >= order.length) return exitTable(table, 1);
      const point = startPoint(order[at + 1]);
      placeCaretAt(point.node, point.offset);
    }

    function moveToCellPrev({ table, from }) {
      const order = readingOrder(table);
      const at = order.indexOf(from);
      if (at <= 0) return exitTable(table, -1);
      const point = endPoint(order[at - 1]);
      placeCaretAt(point.node, point.offset);
    }

    // Ref `insertRowAfterLastTableCellByTab`: false when the caret is not in the last cell, or the table is nested.
    function insertRowAfterLastTableCellByTab({ table, from }) {
      if (isNested(table) || readingOrder(table).at(-1) !== from) return false;
      const added = withUndo(() => grid.insertRow(table, from, false));
      if (!added.changed) return false;
      const point = startPoint(added.cell);
      placeCaretAt(point.node, point.offset);
      return true;
    }

    // Ref `case 'Tab'`: `selection` is `{ table, from }`, the cell the caret is in.
    function handleTab(selection, shiftKey) {
      if (shiftKey) moveToCellPrev(selection);
      else if (!insertRowAfterLastTableCellByTab(selection)) moveToCellNext(selection);
    }

    return { handleTab, readingOrder };
  }

  host.__EASYOFFICE_CREATE_DOC_TABLE_TAB__ = createDocTableTab;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
