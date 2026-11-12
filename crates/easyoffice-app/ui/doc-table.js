(function (host) {
  "use strict";

  // A new table of the document editor (`표 만들기`): an empty grid after the block the caret is in, else at
  // the end of the document, so a blank document gets one too. The save payload and the file reader use the
  // same `table.doc-inline-table > tbody > tr > td` shape.

  const TABLE_CLASS = "doc-inline-table";
  const DEFAULT_ROWS = 3;
  const DEFAULT_COLUMNS = 3;

  function emptyBlock(document, tag) {
    const el = document.createElement(tag);
    el.appendChild(document.createElement("br"));
    return el;
  }

  function buildTable(document, rows, columns) {
    const table = document.createElement("table");
    table.className = TABLE_CLASS;
    const body = document.createElement("tbody");
    for (let r = 0; r < rows; r += 1) {
      const tr = document.createElement("tr");
      for (let c = 0; c < columns; c += 1) tr.appendChild(emptyBlock(document, "td"));
      body.appendChild(tr);
    }
    table.appendChild(body);
    return table;
  }

  // The block child of `root` that holds the range; none when there is no caret in the editor.
  function blockOf(root, range) {
    let node = range?.startContainer ?? null;
    if (!node || !root.contains(node)) return null;
    while (node.parentNode !== root) node = node.parentNode;
    return node;
  }

  // Puts the table after the caret's block (a paragraph follows it when it ends the document, so the text can
  // go on) and returns the first cell. `root` is `#doc-editor`.
  function insertTable(root, range, rows = DEFAULT_ROWS, columns = DEFAULT_COLUMNS) {
    const document = root.ownerDocument;
    const table = buildTable(document, rows, columns);
    root.insertBefore(table, blockOf(root, range)?.nextSibling ?? null);
    if (!table.nextSibling) root.appendChild(emptyBlock(document, "p"));
    return { changed: true, table, firstCell: table.querySelector("td") };
  }

  host.__EASYOFFICE_DOC_TABLE__ = { DEFAULT_ROWS, DEFAULT_COLUMNS, insertTable };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
