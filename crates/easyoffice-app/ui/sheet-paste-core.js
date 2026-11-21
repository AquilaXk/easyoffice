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
//   packages/sheets-ui/src/services/clipboard/utils.ts :31-88 (`getRepeatRange`: a copied block
//       repeats over a selection that is a whole multiple of it; `isStrictMode` needs both the rows
//       and the columns to divide, as the spreadsheet programs do, otherwise one copy is pasted at
//       the top left), and controllers/clipboard/utils.ts :388-461 (`getClearAndSetMergeMutations`:
//       the merges inside the pasted area go, the merges of the block are added)
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

  // Where the copies of a block of `source` size land in `target`: one `{ startRange }` (a one-cell range at the
  // copy's top left) per copy. Ported with the reference's shape, ranges are 0-based `{ startRow, endRow,
  // startColumn, endColumn }`.
  function getRepeatRange(source, target, isStrictMode = false) {
    const getRowLength = (range) => range.endRow - range.startRow + 1;
    const getColLength = (range) => range.endColumn - range.startColumn + 1;
    const rowMod = getRowLength(target) % getRowLength(source);
    const colMod = getColLength(target) % getColLength(source);
    const repeatRelativeRange = { startRow: 0, endRow: getRowLength(source) - 1, startColumn: 0, endColumn: getColLength(source) - 1 };
    const repeatRow = Math.floor(getRowLength(target) / getRowLength(source));
    const repeatCol = Math.floor(getColLength(target) / getColLength(source));
    const repeatList = [];
    const at = (row, col) => ({ startRow: row + target.startRow, endRow: row + target.startRow, startColumn: col + target.startColumn, endColumn: col + target.startColumn });
    if (!rowMod && !colMod) {
      for (let countRow = 1; countRow <= repeatRow; countRow += 1) {
        for (let countCol = 1; countCol <= repeatCol; countCol += 1) {
          repeatList.push({ repeatRelativeRange, startRange: at(getRowLength(source) * (countRow - 1), getColLength(source) * (countCol - 1)) });
        }
      }
    } else if (!rowMod && colMod && !isStrictMode) {
      for (let countRow = 1; countRow <= repeatRow; countRow += 1) {
        repeatList.push({ repeatRelativeRange, startRange: at(getRowLength(source) * (countRow - 1), 0) });
      }
    } else if (rowMod && !colMod && !isStrictMode) {
      for (let countCol = 1; countCol <= repeatCol; countCol += 1) {
        repeatList.push({ repeatRelativeRange, startRange: at(0, getColLength(source) * (countCol - 1)) });
      }
    } else {
      repeatList.push({ startRange: at(0, 0), repeatRelativeRange });
    }
    return repeatList;
  }

  // The size of the block as it lands (rows and columns turned over when asked) and the copies of it
  // the `target` selection takes: `{ outRows, outCols, tiles }`, each tile `{ row, col }` its top left cell (1-based).
  function layoutOf(snap, transpose, target) {
    const outRows = transpose ? snap.cols : snap.cells.length;
    const outCols = transpose ? snap.cells.length : snap.cols;
    if (outRows === 0 || outCols === 0) return { outRows, outCols, tiles: [] };
    const source = { startRow: 0, endRow: outRows - 1, startColumn: 0, endColumn: outCols - 1 };
    const aim = { startRow: target.r1, endRow: target.r2, startColumn: target.c1, endColumn: target.c2 };
    const tiles = getRepeatRange(source, aim, true).map((t) => ({ row: t.startRange.startRow, col: t.startRange.startColumn }));
    return { outRows, outCols, tiles };
  }

  // The area a paste covers: the copies together, from the target's top left.
  function areaOf(snap, transpose, target) {
    const { outRows, outCols, tiles } = layoutOf(snap, transpose, target);
    if (tiles.length === 0) return { r1: target.r1, c1: target.c1, r2: target.r1, c2: target.c1 };
    const last = tiles[tiles.length - 1];
    return { r1: target.r1, c1: target.c1, r2: last.row + outRows - 1, c2: last.col + outCols - 1 };
  }

  // The merges a paste of `mode` writes: `clear` is the area whose merges go, `add` the merges of the copies (the grid's
  // own, 1-based); none for the modes that leave the format alone. A merge that leaves the grid is not written.
  function planMerges(opts) {
    const { snap, mode, transpose, target, grid } = opts;
    // Text from outside has no format: it leaves the merges where they are.
    if ((mode !== "all" && mode !== "formats") || !snap.cells.length || (snap.rows === null && snap.rich !== true)) return null;
    const area = areaOf(snap, transpose, target);
    const add = [];
    const { tiles } = layoutOf(snap, transpose, target);
    for (const tile of tiles) {
      for (const m of snap.merges || []) {
        const [r1, c1, r2, c2] = transpose ? [m.c1, m.r1, m.c2, m.r2] : [m.r1, m.c1, m.r2, m.c2];
        const rect = { r1: tile.row + r1, c1: tile.col + c1, r2: tile.row + r2, c2: tile.col + c2 };
        if (rect.r2 <= grid.MAX_ROWS && rect.c2 <= grid.MAX_COLS) add.push(rect);
      }
    }
    return { clear: area, add };
  }

  // The writes of pasting `snap` at `target` ({ r1, c1, r2, c2 }): `{ edits, widths, clipped }`.
  // `getEntry(ref)` reads the target cell (the format mode keeps its content); `shiftFormula` moves
  // the relative references of a formula; `isFormulaStr` tells a typed formula from text.
  function planPaste(opts) {
    const { snap, mode, transpose, target, getEntry, shiftFormula, isFormulaStr, grid } = opts;
    const copied = snap.rows !== null;
    const rich = copied || snap.rich === true;
    const cells = transpose ? transposed(snap) : snap.cells;
    const { outRows, outCols, tiles } = layoutOf(snap, transpose, target);
    const result = { edits: [], widths: null, clipped: 0 };
    if (mode === "widths") {
      result.widths = rich && snap.widths.length > 0 ? widthsOf(snap, target, grid) : null;
      return result;
    }
    for (const tile of tiles) {
      for (let i = 0; i < outRows; i += 1) {
        for (let j = 0; j < outCols; j += 1) {
          const row = tile.row + i;
          const col = tile.col + j;
          if (row > grid.MAX_ROWS || col > grid.MAX_COLS) {
            result.clipped += 1;
            continue;
          }
          const edit = writeOf(cells[i][j], mode, copied, rich, isFormulaStr);
          if (!edit) continue;
          const ref = grid.refOf(row, col);
          if (copied && edit.formula) {
            const srcRow = snap.rows[transpose ? j : i];
            const srcCol = snap.c1 + (transpose ? i : j);
            const turn = transpose ? { fromRow: srcRow, fromCol: srcCol, toRow: row, toCol: col } : undefined;
            edit.formula = shiftFormula(edit.formula, row - srcRow, col - srcCol, turn);
          }
          if (mode === "formats") {
            const now = getEntry(ref);
            edit.value = now?.value ?? "";
            edit.formula = now?.formula ?? "";
            if (now?.t) edit.t = now.t;
          }
          edit.ref = ref;
          result.edits.push(edit);
        }
      }
    }
    return result;
  }

  // One cell's write for a mode. `from` is null for an empty source cell: it empties what it
  // covers. Null when the mode has nothing to write for the block.
  function writeOf(from, mode, copied, rich, isFormulaStr) {
    if (!rich && (mode === "formats" || mode === "widths")) return null;
    // A result of a copied array formula is not pasted next to it (PD-217); a values paste writes it.
    if (from?.spillChild && (mode === "all" || mode === "formulas")) return null;
    const value = String(from?.value ?? "");
    // A cell the source marks as text (`t: "s"`) stays text even when it reads like a formula.
    const pinned = rich && from?.t === "s";
    const formula = from?.formula || (!copied && mode !== "values" && !pinned && isFormulaStr(value) ? value : "");
    const text = formula ? "" : value;
    const t = rich && !formula && from?.t ? from.t : undefined;
    if (mode === "values") return t ? { value, formula: "", t } : { value, formula: "" };
    if (mode === "formats") return { value: "", formula: "", style: plainStyle(from?.style) };
    if (mode === "formulas") return t ? { value: text, formula, t } : { value: text, formula };
    // A block from outside with formats (a table of HTML) brings its styles and leaves the cell's hyperlink.
    if (!copied) {
      if (!rich) return { value: text, formula };
      return t ? { value: text, formula, t, style: plainStyle(from?.style) } : { value: text, formula, style: plainStyle(from?.style) };
    }
    // A plain paste brings the source's hyperlink or none; the special pastes of values, formulas and formats leave the
    // hyperlink of the cell they land on (PD-218).
    return t
      ? { value: text, formula, t, style: plainStyle(from?.style), link: from?.link ?? null }
      : { value: text, formula, style: plainStyle(from?.style), link: from?.link ?? null };
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

  host.__EASYOFFICE_SHEET_PASTE_CORE__ = { MODES, planPaste, planMerges, areaOf, getRepeatRange, transposed, plainStyle };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
