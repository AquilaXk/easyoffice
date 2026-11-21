// Reference implementation (ported, safe JS): Univer `sheets-ui` cell editor sizing, Apache License 2.0,
// Copyright 2023-present DreamNum Co., Ltd. (https://github.com/dream-num/univer, commit
// eaf4cfa04fabdbcfc922c692f3450459f8a29546):
//   packages/sheets-ui/src/services/editor/cell-editor-resize.service.ts lines 47-49 (`EDITOR_INPUT_SELF_EXTEND_GAP`,
//   `EDITOR_BORDER_SIZE`), 203-262 (`_predictingSize`: wrapped cells keep the cell width, other cells widen to the text
//   plus a gap), 264-316 (`_getEditorMaxSize`: the room to the right edge of the view minus the borders, never under
//   the cell width) and 110-185 (`fitTextSize`: the editor is the larger of the cell and the content).
// x-spreadsheet `src/component/editor.js` lines 10-36 (`resetTextareaSize`, MIT, commit
// 0c55f3ee1e708002eaef280432d6efeaeabdc886) was read and compared: it has the same rule (the text and 5px, the room to
// the right, then lines of the row height). Nothing of it is taken.
// Modifications: the reference reads the text size from an embedded document; this grid edits in the cell's own `td`,
// so the text is measured by the browser (the cell as a block as wide as its text, `offsetWidth` and `offsetHeight`)
// and the box is the `td` itself switched to a block (class `edit-grow`) that lies over the cells beside and below it.
// The box follows the text both ways (the reference keeps its widest size for the edit). The reference's skeleton,
// scroll bar and alignment offsets are not used.
(function (host) {
  "use strict";

  // The room the text keeps to the right of its last glyph (EDITOR_INPUT_SELF_EXTEND_GAP) and the border of the box.
  const GAP = 5;
  const BORDER = 2;

  // The size of the box of a cell being edited. `naturalW` is the width of the text on one line (padding included),
  // `availW` the room from the cell's left edge to the right edge of the view, `heightAt(w)` the height of the text
  // wrapped at `w`. A wrapped cell keeps its width and grows down; any other cell grows right until the view ends and
  // then wraps. `grow` is false while the cell holds the text as it is.
  function fitEditBox({ cellW, cellH, naturalW, availW, wrap, heightAt }) {
    const keep = { grow: false, wrap: false, w: cellW, h: cellH };
    if (wrap) {
      const h = Math.ceil(heightAt(cellW));
      return h > cellH ? { grow: true, wrap: true, w: cellW, h } : keep;
    }
    const want = Math.ceil(naturalW) + GAP;
    if (want <= cellW) return keep;
    const maxW = Math.max(cellW, availW - BORDER * 2);
    if (want <= maxW) return { grow: true, wrap: false, w: want, h: cellH };
    const h = Math.max(cellH, Math.ceil(heightAt(maxW)));
    return { grow: true, wrap: true, w: maxW, h };
  }

  const finite = (n) => typeof n === "number" && Number.isFinite(n);

  // The box of the cell being edited, drawn by the cell itself: `fit(td)` after each change of its text, `release(td)`
  // when the edit ends. A page that cannot measure (no layout) leaves the cell as it is. A wrapped cell in a row that
  // follows its content makes the row taller as it is typed in (the row of a spreadsheet does the same), so only a row
  // whose height a user set (`isFixedRow`, its height `rowHeightOf`) keeps its height under a growing wrapped cell.
  function createEditBox({ document, isFixedRow, rowHeightOf }) {
    const viewport = () => document.getElementById("sheet-viewport");

    function release(td) {
      if (!td.classList.contains("edit-grow")) return;
      td.classList.remove("edit-grow", "edit-wrap");
      for (const name of ["--ew", "--eh", "--ch"]) td.style.removeProperty(name);
    }

    // The width of the text on one line with the cell's padding: the cell as a block that is as wide as its content.
    function naturalWidth(td) {
      td.classList.add("edit-measure");
      const width = td.offsetWidth;
      td.classList.remove("edit-measure");
      return width;
    }

    // The room to the right of the cell inside the visible part of the grid (its scroll bar excluded).
    function roomRight(td) {
      const vp = viewport();
      if (!vp || typeof vp.getBoundingClientRect !== "function") return NaN;
      const bar = (vp.offsetWidth || 0) - (vp.clientWidth || 0);
      return vp.getBoundingClientRect().right - bar - td.getBoundingClientRect().left;
    }

    function fit(td) {
      if (td.classList.contains("merge-anchor")) return;
      release(td);
      const wrap = Boolean(td._st?.wrap);
      if (wrap && !isFixedRow(td)) return;
      const cellW = td.offsetWidth;
      const cellH = wrap ? rowHeightOf(td) : td.offsetHeight;
      if (!finite(cellW) || !finite(cellH) || cellW <= 0) return;
      const naturalW = wrap ? 0 : naturalWidth(td);
      if (!finite(naturalW)) return;
      const availW = roomRight(td);
      // The height of the text at a width: the cell as a block of that width, wrapping, measured by the browser.
      const heightAt = (w) => {
        td.classList.add("edit-grow", "edit-wrap");
        td.style.setProperty("--ew", w + "px");
        td.style.removeProperty("--eh");
        return td.offsetHeight;
      };
      const box = fitEditBox({ cellW, cellH, naturalW, availW: finite(availW) ? availW : cellW, wrap, heightAt });
      release(td);
      if (!box.grow) return;
      td.classList.add("edit-grow");
      td.classList.toggle("edit-wrap", box.wrap);
      td.style.setProperty("--ew", box.w + "px");
      td.style.setProperty("--eh", box.h + "px");
      td.style.setProperty("--ch", cellH + "px");
    }

    return { fit, release };
  }

  host.__EASYOFFICE_SHEET_EDIT_BOX__ = { GAP, BORDER, fitEditBox, createEditBox };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
