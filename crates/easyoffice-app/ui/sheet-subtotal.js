(function (host) {
  "use strict";

  // 데이터 > 개요 > 부분합 (PD-391): the plan of the subtotal rows of a list. Pure functions: the list goes in as rows of cells,
  // the new rows come out with their formulas (`=SUBTOTAL(9,B2:B5)`) and the outline groups that fold them. The command that
  // applies the plan (rows made by the structure edit, one undo step) is `sheet-subtotal-ui.js`.
  // Written from the spreadsheet program's behavior; no outside source is ported (the engine already has the SUBTOTAL function).

  // The functions of the dialog, with the SUBTOTAL code each one runs (the codes that count hidden rows too, as the program does).
  const FUNCTIONS = Object.freeze([
    { id: "sum", label: "합계", code: 9 },
    { id: "count", label: "개수", code: 3 },
    { id: "average", label: "평균", code: 1 },
    { id: "max", label: "최대값", code: 4 },
    { id: "min", label: "최소값", code: 5 },
    { id: "product", label: "곱", code: 6 },
    { id: "countnums", label: "숫자 개수", code: 2 },
    { id: "stdev", label: "표준 편차", code: 7 },
    { id: "stdevp", label: "모표준 편차", code: 8 },
    { id: "var", label: "분산", code: 10 },
    { id: "varp", label: "모분산", code: 11 },
  ]);
  const GRAND_TOTAL = "총합계";
  const MAX_ROWS = 200000;
  const SUBTOTAL_FORMULA = /^=\s*SUBTOTAL\s*\(/i;

  const functionOf = (id) => FUNCTIONS.find((fn) => fn.id === id) || null;
  const valueOf = (entry) => String(entry?.value ?? "");
  const isSubtotalRow = (cells) => cells.some((entry) => SUBTOTAL_FORMULA.test(String(entry?.formula ?? "")));
  const hasOtherFormula = (cells) => cells.some((entry) => entry?.formula && !SUBTOTAL_FORMULA.test(String(entry.formula)));

  // `rows[0]` is the header row; each row is the cells of the list's columns (`{ value, formula, style }` or null). `options`:
  // `{ firstRow, firstCol, groupCol, calcCols, fn, below, replace, colName }` with `groupCol` and `calcCols` as 0-based offsets in a row
  // and `firstRow`, `firstCol` the sheet position of the list. Answers `{ rows, groups, rowCount }` or `{ error }`; `rows` are the rows of
  // the new list, `groups` the outline runs `[first, last]` (sheet rows) from the outer group to the inner ones.
  function plan(rows, options) {
    const { firstRow, firstCol, groupCol, calcCols, fn, below = true, replace = true, colName } = options;
    if (!fn) return { error: "function" };
    if (rows.length < 2) return { error: "rows" };
    const width = rows[0].length;
    if (!Number.isInteger(groupCol) || groupCol < 0 || groupCol >= width) return { error: "group" };
    if (!Array.isArray(calcCols) || calcCols.length === 0 || calcCols.some((c) => !Number.isInteger(c) || c < 0 || c >= width)) return { error: "columns" };
    const header = rows[0];
    // The old subtotals go when they are replaced; with 대치 off the list must not hold any, as they would be counted twice.
    const body = rows.slice(1);
    const detail = body.filter((cells) => !isSubtotalRow(cells));
    if (!replace && detail.length !== body.length) return { error: "kept" };
    if (detail.some(hasOtherFormula)) return { error: "formulas" };
    if (detail.length === 0) return { error: "rows" };
    if (detail.length + detail.length + 2 > MAX_ROWS) return { error: "limit" };

    // The groups: runs of rows with the same text in the group column.
    const runs = [];
    for (const cells of detail) {
      const key = valueOf(cells[groupCol]);
      const last = runs[runs.length - 1];
      if (last && last.key === key) last.cells.push(cells);
      else runs.push({ key, cells: [cells] });
    }

    const out = [header];
    const groups = [];
    const subtotalCells = (label, first, last) => {
      const cells = Array.from({ length: width }, () => null);
      cells[groupCol] = { value: label, formula: "", style: { bold: true } };
      for (const c of calcCols) {
        const letters = colName(firstCol + c);
        cells[c] = { value: "", formula: `=SUBTOTAL(${fn.code},${letters}${first}:${letters}${last})`, style: { bold: true } };
      }
      return cells;
    };
    const rowAt = () => firstRow + out.length;
    const label = (key) => (key === "" ? fn.label : key + " " + fn.label);
    // The grand total sits below the list, or right under the header when the summaries sit above their groups.
    const grandAt = below ? null : out.push(null) - 1;
    const grandFirst = firstRow + out.length;
    for (const run of runs) {
      if (below) {
        const first = rowAt();
        out.push(...run.cells);
        const last = rowAt() - 1;
        groups.push({ first, last });
        out.push(subtotalCells(label(run.key), first, last));
      } else {
        const summaryRow = out.push(null) - 1;
        const first = rowAt();
        out.push(...run.cells);
        const last = rowAt() - 1;
        groups.push({ first, last });
        out[summaryRow] = subtotalCells(label(run.key), first, last);
      }
    }
    const grandLast = firstRow + out.length - 1;
    if (below) out.push(subtotalCells(GRAND_TOTAL, grandFirst, grandLast));
    else out[grandAt] = subtotalCells(GRAND_TOTAL, grandFirst, grandLast);
    // A group holds the rows its summary sums up and not the summary row itself, which sits below it (or above when `below` is off):
    // the outer group holds every group and its subtotal, and the grand total is its summary.
    return { rows: out, rowCount: out.length, groups: [[grandFirst, grandLast], ...groups.map((g) => [g.first, g.last])], below };
  }

  // 모두 제거: the list without its subtotal rows, `{ rows }`, or `{ error: "none" }` when it has none.
  function removeAll(rows) {
    const body = rows.slice(1);
    const detail = body.filter((cells) => !isSubtotalRow(cells));
    if (detail.length === body.length) return { error: "none" };
    return { rows: [rows[0], ...detail] };
  }

  host.__EASYOFFICE_SHEET_SUBTOTAL__ = Object.freeze({ FUNCTIONS, GRAND_TOTAL, functionOf, plan, removeAll, isSubtotalRow });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
