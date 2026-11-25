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

    // Materialized row or column intervals for a selection: frozen prefix `[1..frozen]` plus visible window `[w0..w1]`.
    function axisBands(lo, hi, frozen, w0, w1) {
      const bands = [];
      if (frozen > 0) {
        const fHi = Math.min(hi, frozen, w0 - 1);
        if (lo <= fHi) bands.push([lo, fHi]);
      }
      const mLo = Math.max(lo, w0);
      const mHi = Math.min(hi, w1);
      if (mLo <= mHi) bands.push([mLo, mHi]);
      return bands;
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
      const multiApi = host.__EASYOFFICE_SHEET_MULTI_RANGE__;
      const rects = multiApi ? multiApi.rangesOf(rg, rg.extraRanges) : [rg];
      const paintCells = rects.length > 1 || rg.r1 !== rg.r2 || rg.c1 !== rg.c2;
      const v = getView();
      const fr = v && v.layout ? v.fr || 0 : 0;
      const fc = v && v.layout ? v.fc || 0 : 0;
      for (let i = 0; i < rects.length; i += 1) {
        const rect = rects[i];
        const rowBands = v ? axisBands(rect.r1, rect.r2, fr, v.r0, v.r1) : [[rect.r1, rect.r2]];
        const colBands = v ? axisBands(rect.c1, rect.c2, fc, v.c0, v.c1) : [[rect.c1, rect.c2]];
        if (paintCells) {
          for (let rb = 0; rb < rowBands.length; rb += 1) {
            const [rLo, rHi] = rowBands[rb];
            for (let r = rLo; r <= rHi; r += 1) {
              const rowBase = r * PACK_STRIDE;
              for (let cb = 0; cb < colBands.length; cb += 1) {
                const [cLo, cHi] = colBands[cb];
                for (let c = cLo; c <= cHi; c += 1) {
                  const td = tdByPacked.get(rowBase + c);
                  if (td && !td.classList.contains("in-range")) {
                    td.classList.add("in-range");
                    rangeTds.push(td);
                  }
                }
              }
            }
          }
        }
        for (let cb = 0; cb < colBands.length; cb += 1) {
          const [cLo, cHi] = colBands[cb];
          for (let c = cLo; c <= cHi; c += 1) {
            const th = colHeadByCol.get(c);
            if (th && !th.classList.contains("active-axis")) {
              th.classList.add("active-axis");
              activeColHeads.push(th);
            }
          }
        }
        for (let rb = 0; rb < rowBands.length; rb += 1) {
          const [rLo, rHi] = rowBands[rb];
          for (let r = rLo; r <= rHi; r += 1) {
            const th = rowHeadByRow.get(r);
            if (th && !th.classList.contains("active-axis")) {
              th.classList.add("active-axis");
              activeRowHeads.push(th);
            }
          }
        }
      }
    }

    return paintSelectionRange;
  }

  host.__EASYOFFICE_CREATE_SHEET_GRID_SELECTION__ = createSelectionPaint;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
