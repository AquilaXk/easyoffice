(function (host) {
  "use strict";

  // Frozen rows and columns of a sheet (PD-208). The grid keeps the first rows and columns in its
  // window at all times (`view.fr`, `view.fc`); this module says how many there are and pins their
  // cells with `position: sticky` offsets, so they stay in place while the rest scrolls under them.
  // The counts are the first rows and columns of the sheet, as a file stores them (`pane`).

  function createFreeze({ layoutApi, getSheetName, rowTop, colLeft, headH, rowHeadW, maxRows, maxCols }) {
    let painted = false;

    function counts() {
      const f = layoutApi ? layoutApi.freezeOf(getSheetName()) : null;
      return f ? { rows: Math.min(f.rows, maxRows - 1), cols: Math.min(f.cols, maxCols - 1) } : { rows: 0, cols: 0 };
    }

    // Height of the first `n` rows (hidden ones count 0) and width of the first `n` columns.
    const height = (n) => rowTop(n + 1);
    const width = (n) => colLeft(n + 1);

    function setStyle(el, prop, value) {
      if (el.style[prop] !== value) el.style[prop] = value;
    }

    // The last frozen row or column that is not hidden carries the dividing line.
    function lastShown(n, hidden) {
      for (let i = n; i >= 1; i -= 1) if (!hidden(i)) return i;
      return 0;
    }

    function paintRow(tr, r, fr, fc, edge) {
      const rowFrozen = r >= 1 && r <= fr;
      const top = rowFrozen ? headH + rowTop(r) + "px" : "";
      for (const el of tr.children) {
        const rowHead = el.tagName === "TH";
        const c = rowHead ? 0 : el._col || -1;
        const colFrozen = c >= 1 && c <= fc;
        if (!rowFrozen && !colFrozen && !el._pinned) continue;
        el._pinned = rowFrozen || colFrozen;
        setStyle(el, "top", top);
        if (!rowHead) setStyle(el, "left", colFrozen ? rowHeadW + colLeft(c) + "px" : "");
        el.classList.toggle("frozen-row", rowFrozen);
        el.classList.toggle("frozen-col", colFrozen);
        el.classList.toggle("frozen-edge-row", rowFrozen && r === edge.row);
        el.classList.toggle("frozen-edge-col", colFrozen && c === edge.col);
      }
    }

    // `rows` are the table rows of the window (`[row number, tr]`), `head` the header row.
    function paint(head, rows, view) {
      const { rows: fr, cols: fc } = view.layout ? counts() : { rows: 0, cols: 0 };
      if (!painted && fr === 0 && fc === 0) return;
      painted = fr > 0 || fc > 0;
      const name = getSheetName();
      const edge = {
        row: lastShown(fr, (i) => Boolean(layoutApi?.rowHidden(name, i))),
        col: lastShown(fc, (i) => Boolean(layoutApi?.colHidden(name, i))),
      };
      for (let i = 0; i < head.children.length; i += 1) {
        const th = head.children[i];
        const c = th.dataset?.col ? host.__EASYOFFICE_SHEET_REFS__.colIndex(th.dataset.col) : 0;
        const colFrozen = c >= 1 && c <= fc;
        th.classList.toggle("frozen-col", colFrozen);
        th.classList.toggle("frozen-edge-col", colFrozen && c === edge.col);
        if (c > 0) setStyle(th, "left", colFrozen ? rowHeadW + colLeft(c) + "px" : "");
      }
      for (const [r, tr] of rows) paintRow(tr, r, fr, fc, edge);
    }

    return { counts, height, width, paint };
  }

  host.__EASYOFFICE_CREATE_SHEET_FREEZE__ = createFreeze;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
