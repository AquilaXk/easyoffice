// Paste special of sheet cells (PD-206): which cell writes a copied block makes in each paste
// mode, with the rows and columns turned over when asked. Pure data logic, no DOM and no engine call.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9):
//   packages/sheets-ui/src/services/clipboard/clipboard.service.ts :135-140
//       (PREDEFINED_HOOK_NAME_PASTE: all, value, format, column width, all but border, formula)
//   packages/sheets-ui/src/controllers/clipboard/clipboard.controller.ts :665-862
//       (the special paste hooks: value writes the cell values, format writes the styles, column
//        width writes the widths of the pasted columns and cycles when the target is wider)
//   packages/sheets-ui/src/services/clipboard/clipboard.service.ts :894-896
//       (the transpose step sits between the copied matrix and the paste; this reference leaves it
//        unwritten, so the turn-over follows the behaviour of another office engine: behaviour
//        only, no code is taken from it)
// Differences: the cell model holds no borders, so there is no "all but border" mode; a column
// width paste does not cycle; when rows and columns are turned over a formula's references follow
// the rule of the reference office engine: a cell whose row and column are both relative (and a
// range whose ends both are) swaps its row and column offsets from the formula, any other
// reference stays as written; a paste that cuts through a merged range is refused (the reference
// refuses it too, `sheets-ui.clipboard.paste.overlappingMergedCells`).
(function (host) {
  "use strict";

  const MODES = Object.freeze(["all", "values", "formats", "formulas", "widths"]);

  // A style as a paste or fill writes it: the frozen-panes flag (kept on A1) does not travel.
  function plainStyle(style) {
    if (!style || !style.frozenPanes) return style || null;
    const { frozenPanes, ...rest } = style;
    return Object.values(rest).some(Boolean) ? rest : null;
  }

  // The block copied from the sheet: `rows` are the copied source rows (the rows a filter hides
  // are left out), `cols` the number of columns from `c1`, `cells[i][j]` a `{ value, formula,
  // style, link }` copy or null for an empty cell, `widths[j]` the pixel width of source column j.
  // A block read from the clipboard text has `rows: null`: nothing to measure a reference move by.
  function transposed(snap) {
    const cells = [];
    for (let j = 0; j < snap.cols; j += 1) cells.push(snap.cells.map((row) => row[j]));
    return cells;
  }

  // The writes of pasting `snap` at `target` ({ r1, c1, r2, c2 }): `{ edits, widths, clipped }`.
  // `getEntry(ref)` reads the target cell (the format mode keeps its content); `shiftFormula` moves
  // the relative references of a formula; `isFormulaStr` tells a typed formula from text.
  function planPaste(opts) {
    const { snap, mode, transpose, target, getEntry, shiftFormula, isFormulaStr, grid } = opts;
    const copied = snap.rows !== null;
    const cells = transpose ? transposed(snap) : snap.cells;
    const outRows = cells.length;
    const outCols = outRows > 0 ? cells[0].length : 0;
    const spread = outRows === 1 && outCols === 1 && (target.r2 > target.r1 || target.c2 > target.c1);
    const rowsN = spread ? target.r2 - target.r1 + 1 : outRows;
    const colsN = spread ? target.c2 - target.c1 + 1 : outCols;
    const result = { edits: [], widths: null, clipped: 0 };
    if (mode === "widths") {
      result.widths = copied ? widthsOf(snap, target, grid) : null;
      return result;
    }
    for (let i = 0; i < rowsN; i += 1) {
      for (let j = 0; j < colsN; j += 1) {
        const row = target.r1 + i;
        const col = target.c1 + j;
        if (row > grid.MAX_ROWS || col > grid.MAX_COLS) {
          result.clipped += 1;
          continue;
        }
        const [si, sj] = spread ? [0, 0] : [i, j];
        const edit = writeOf(cells[si][sj], mode, copied, isFormulaStr);
        if (!edit) continue;
        const ref = grid.refOf(row, col);
        if (copied && edit.formula) {
          const srcRow = snap.rows[transpose ? sj : si];
          const srcCol = snap.c1 + (transpose ? si : sj);
          const turn = transpose ? { fromRow: srcRow, fromCol: srcCol, toRow: row, toCol: col } : undefined;
          edit.formula = shiftFormula(edit.formula, row - srcRow, col - srcCol, turn);
        }
        if (mode === "formats") {
          const now = getEntry(ref);
          edit.value = now?.value ?? "";
          edit.formula = now?.formula ?? "";
        }
        edit.ref = ref;
        result.edits.push(edit);
      }
    }
    return result;
  }

  // One cell's write for a mode. `from` is null for an empty source cell: it empties what it
  // covers. Null when the mode has nothing to write for the block.
  function writeOf(from, mode, copied, isFormulaStr) {
    if (!copied && (mode === "formats" || mode === "widths")) return null;
    // A result of a copied array formula is not pasted next to it (PD-217); a values paste writes it.
    if (from?.spillChild && (mode === "all" || mode === "formulas")) return null;
    const value = String(from?.value ?? "");
    const formula = from?.formula || (!copied && mode !== "values" && isFormulaStr(value) ? value : "");
    const text = formula ? "" : value;
    if (mode === "values") return { value, formula: "" };
    if (mode === "formats") return { value: "", formula: "", style: plainStyle(from?.style) };
    if (mode === "formulas" || !copied) return { value: text, formula };
    // A plain paste brings the source's hyperlink or none; the special pastes of values, formulas and formats leave the
    // hyperlink of the cell they land on (PD-218).
    return { value: text, formula, style: plainStyle(from?.style), link: from?.link ?? null };
  }

  // Column widths of the pasted columns by column; a source column at the default width resets the
  // target column. Hidden source columns carry the default width.
  function widthsOf(snap, target, grid) {
    const out = {};
    for (let j = 0; j < snap.cols; j += 1) {
      const col = target.c1 + j;
      if (col > grid.MAX_COLS) break;
      out[col] = snap.widths[j] || grid.DEFAULT_COL_W;
    }
    return out;
  }

  host.__EASYOFFICE_SHEET_PASTE_CORE__ = { MODES, planPaste, transposed, plainStyle };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
