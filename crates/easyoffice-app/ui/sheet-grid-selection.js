(function (host) {
  "use strict";

  // The selection of the grid as classes on the materialized cells: `selected-cell` on the anchor, `in-range` on the other
  // cells of the range and `active-axis` on the headers of its rows and columns. The cells and headers marked last time
  // are remembered, so a new selection clears them without scanning the window.
  const { PACK_STRIDE, packCoord } = host.__EASYOFFICE_SHEET_REFS__;

  // The maps are the grid's own (packed address to cell, column and row number to header); `getView` answers the window the grid has
  // drawn (null before the first draw). Returns the function that paints a range (`{ r1, c1, r2, c2 }`, anchor `ar`, `ac`) or clears it.
  function createSelectionPaint({ tdByPacked, colHeadByCol, rowHeadByRow, seedDomMaps, getView }) {
    let selectedTd = null;
    const rangeTds = [], activeColHeads = [], activeRowHeads = [];

    function clear() {
      if (selectedTd) {
        selectedTd.classList.remove("selected-cell");
        selectedTd = null;
      }
      for (let i = 0; i < rangeTds.length; i += 1) rangeTds[i].classList.remove("in-range");
      rangeTds.length = 0;
      for (let i = 0; i < activeColHeads.length; i += 1) activeColHeads[i].classList.remove("active-axis");
      activeColHeads.length = 0;
      for (let i = 0; i < activeRowHeads.length; i += 1) activeRowHeads[i].classList.remove("active-axis");
      activeRowHeads.length = 0;
    }

    function paintSelectionRange(rg) {
      seedDomMaps();
      clear();
      if (!rg) return;
      const ar = rg.ar ?? rg.r1;
      const ac = rg.ac ?? rg.c1;
      const anchor = tdByPacked.get(packCoord(ar, ac));
      if (anchor) {
        anchor.classList.add("selected-cell");
        selectedTd = anchor;
      }
      const v = getView();
      const mr1 = v ? Math.max(rg.r1, v.r0) : rg.r1;
      const mr2 = v ? Math.min(rg.r2, v.r1) : rg.r2;
      const mc1 = v ? Math.max(rg.c1, v.c0) : rg.c1;
      const mc2 = v ? Math.min(rg.c2, v.c1) : rg.c2;
      if (rg.r1 !== rg.r2 || rg.c1 !== rg.c2) {
        for (let r = mr1; r <= mr2; r += 1) {
          const rowBase = r * PACK_STRIDE;
          for (let c = mc1; c <= mc2; c += 1) {
            const td = tdByPacked.get(rowBase + c);
            if (td) {
              td.classList.add("in-range");
              rangeTds.push(td);
            }
          }
        }
      }
      for (let c = mc1; c <= mc2; c += 1) {
        const th = colHeadByCol.get(c);
        if (th) {
          th.classList.add("active-axis");
          activeColHeads.push(th);
        }
      }
      for (let r = mr1; r <= mr2; r += 1) {
        const th = rowHeadByRow.get(r);
        if (th) {
          th.classList.add("active-axis");
          activeRowHeads.push(th);
        }
      }
    }

    return paintSelectionRange;
  }

  host.__EASYOFFICE_CREATE_SHEET_GRID_SELECTION__ = createSelectionPaint;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
