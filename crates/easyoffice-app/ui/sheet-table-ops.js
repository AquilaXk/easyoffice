// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0, Copyright 2023-present DreamNum
// Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9): `packages/sheets-table/src/commands/commands/add-sheet-table.command.ts`
// lines 54-70, the default name of a new table: the index starts at the number of tables plus one and moves past every name with the
// prefix and a number at or over it; and IronCalc (https://github.com/ironcalc/IronCalc, MIT OR Apache-2.0, Copyright 2023-present
// IronCalc contributors, commit 6dc9e8bae4b8d066276f399de7b3e1cf991c220b): `base/src/functions/spill_functions.rs` lines 60-117
// (`fn_unique`: typed equality key over blank, canonical f64, boolean, error and case-folded text, keeping first occurrence).
// Source and license text: `THIRD_PARTY_NOTICES.md`.
// Modifications: a name the numbering lands on that is taken moves on to the next number; `planDedupe` hashes the selected columns'
// typed keys in a single O(R * K) pass with 32-bit FNV-1a buckets and compacts only the table's own columns; the rest of the plans
// follow the spreadsheet program and are written here.
//

(function (host) {
  "use strict";

  // The plans of the table commands of the sheet window (PD-231), as functions of plain data: making a table of a range
  // (`표 만들기`), giving a table a new range (`표 크기 조정`) and growing a table over what is typed under or beside it. A
  // plan says what changes (the table, the header cells to write, the row to insert) or why nothing may; the page applies it
  // as one undoable step. Rows and columns are 1-based. `ctx` is `{ tables, merges, filter, cellAt }`: the tables of the sheet
  // (their JSON), its merged ranges and filter range (`{ r1, c1, r2, c2 }`), and a reader of a cell's `{ value, formula }`.

  const MAX_TABLES = 1000;
  const MAX_COLUMNS_TOTAL = 65536;
  const MAX_ROW = 1048576;
  const MAX_COL = 16384;
  const MAX_TEXT = 255;
  const DEFAULT_COLUMN = "Column";
  const DEFAULT_TABLE = "표";
  const DEFAULT_STYLE = "TableStyleMedium2";

  const MESSAGES = {
    range: "범위가 올바르지 않음",
    sheet: "현재 시트의 범위만 가능",
    tables: "다른 표와 겹치는 범위에는 표를 만들 수 없음",
    merges: "병합된 칸이 있는 범위에는 표를 만들 수 없음",
    filter: "필터가 있는 범위에는 표를 만들 수 없음",
    limit: "표는 1,000개까지",
    columns: "표의 열은 모두 65,536개까지",
    bottom: "시트의 끝에는 표를 만들 수 없음",
    header: "머리글 행은 같은 행에 있어야 함",
    apart: "새 범위는 원래 범위와 겹쳐야 함",
    small: "범위가 너무 작음",
    resizeTables: "다른 표와 겹치는 범위로는 크기를 바꿀 수 없음",
    resizeMerges: "병합된 칸이 있는 범위로는 크기를 바꿀 수 없음",
    resizeFilter: "필터가 있는 범위로는 크기를 바꿀 수 없음",
    straddle: "아래에 걸친 표·병합·필터가 있어 셀을 내릴 수 없음",
  };

  const fold = (text) => String(text).toLowerCase();
  const chars = (text) => Array.from(text);
  const clip = (text) => (chars(text).length > MAX_TEXT ? chars(text).slice(0, MAX_TEXT).join("") : text);
  const refs = () => host.__EASYOFFICE_SHEET_REFS__;
  const rangeText = (r1, c1, r2, c2) => refs().refOf(r1, c1) + ":" + refs().refOf(r2, c2);
  const touches = (a, b) => a.r1 <= b.r2 && a.r2 >= b.r1 && a.c1 <= b.c2 && a.c2 >= b.c1;

  // The box of a range typed as `A1:C5`, `$A$1:$C$5` or a single cell, with `Sheet1!` or `'My Sheet'!` in front of a range on
  // `sheet` ("sheet" when it is another one, null when it is none).
  function parseRange(text, sheet) {
    let body = String(text).trim();
    const bang = body.lastIndexOf("!");
    if (bang >= 0) {
      const name = body.slice(0, bang).replace(/^'(.*)'$/, (_, inner) => inner.replaceAll("''", "'"));
      if (fold(name) !== fold(sheet)) return "sheet";
      body = body.slice(bang + 1);
    }
    const parts = body.replaceAll("$", "").split(":");
    if (parts.length > 2) return null;
    const parse = host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef;
    const a = parse(parts[0].trim().toUpperCase());
    const b = parts.length === 2 ? parse(parts[1].trim().toUpperCase()) : a;
    if (!a || !b) return null;
    return { r1: Math.min(a.row, b.row), c1: Math.min(a.col, b.col), r2: Math.max(a.row, b.row), c2: Math.max(a.col, b.col) };
  }

  // `표N` for a workbook that has `count` tables: N is one past the tables, and one past the biggest number a name that starts
  // `표` already ends in (names compare without case).
  //
  // The numbering is the reference engine's (see the header); a name the numbering lands on that is taken moves on to the next number.
  function defaultName(taken, count = 0) {
    const used = new Set([...taken].map(fold));
    let index = count + 1;
    for (const name of used) {
      if (!name.startsWith(DEFAULT_TABLE)) continue;
      const n = Number(name.slice(DEFAULT_TABLE.length));
      if (Number.isInteger(n) && n >= index) index = n + 1;
    }
    while (used.has(DEFAULT_TABLE + index)) index += 1;
    return DEFAULT_TABLE + index;
  }

  // `Column<n>` for the column at 1-based `position`, the first number from there that no column of `used` has.
  function defaultColumn(used, position) {
    for (let n = position; ; n += 1) if (!used.has(fold(DEFAULT_COLUMN + n))) return DEFAULT_COLUMN + n;
  }

  // The names of the columns of a new row of header cells `texts` (a cell's text; "" for none): an empty one is `Column<n>`
  // and a name an earlier column has gets the next free number (`Region`, `Region2`). `used` holds the folded names kept.
  function headerNames(texts, used, first = 1) {
    const out = [];
    texts.forEach((text, i) => {
      let name = clip(String(text ?? ""));
      if (name === "") name = defaultColumn(used, first + i);
      else if (used.has(fold(name))) {
        let n = 2;
        while (used.has(fold(clip(name) + n))) n += 1;
        name = clip(name).slice(0, Math.max(0, MAX_TEXT - String(n).length)) + n;
      }
      used.add(fold(name));
      out.push(name);
    });
    return out;
  }

  const cellText = (ctx, row, col) => {
    const entry = ctx.cellAt(row, col);
    return entry ? String(entry.value ?? "") : "";
  };

  // Whether `other` at or below `r1` overlaps columns `c1..c2` without lying wholly inside `c1..c2` (PD-237).
  const straddlesBelow = (r1, c1, c2, other) =>
    other.r2 >= r1 && other.c1 <= c2 && other.c2 >= c1 && !(other.c1 >= c1 && other.c2 <= c2);

  function straddleConflict(ctx, r1, c1, c2, skip = null) {
    if (ctx.tables.some((t) => t !== skip && straddlesBelow(r1, c1, c2, ctx.boxOf(t)))) return true;
    if (ctx.merges.some((m) => straddlesBelow(r1, c1, c2, m))) return true;
    if (ctx.filter && straddlesBelow(r1, c1, c2, ctx.filter)) return true;
    return false;
  }

  function normalizeCalc(list, len) {
    const out = Array.from({ length: len }, (_, i) => (typeof list?.[i] === "string" && list[i].startsWith("=") ? list[i] : null));
    return out.some(Boolean) ? out : undefined;
  }

  // The calculated-column formula of each column of `table`: explicit `calculatedColumns`, or an identical formula
  // already held by every existing data row of that column (PD-237).
  function effectiveCalc(table, b, cellAt) {
    const d1 = b.r1 + (table.header === false ? 0 : 1);
    const d2 = b.r2 - (table.totals === true ? 1 : 0);
    return table.columns.map((_, i) => {
      const explicit = table.calculatedColumns?.[i];
      if (typeof explicit === "string" && explicit.startsWith("=")) return explicit;
      if (d1 > d2) return null;
      const col = b.c1 + i;
      const first = cellAt(d1, col)?.formula;
      if (!first || !first.startsWith("=")) return null;
      for (let r = d1 + 1; r <= d2; r += 1) {
        if (cellAt(r, col)?.formula !== first) return null;
      }
      return first;
    });
  }

  // What keeps a table of `box` off the sheet: `"tables"`, `"merges"` or `"filter"` (tables other than `skip`); null when nothing.
  function conflict(ctx, box, skip) {
    if (ctx.tables.some((t) => t !== skip && touches(box, ctx.boxOf(t)))) return "tables";
    if (ctx.merges.some((m) => touches(box, m))) return "merges";
    if (ctx.filter && touches(box, ctx.filter)) return "filter";
    return null;
  }

  // ---- 표 만들기 ---------------------------------------------------------------------------------------------------------
  // `input`: `{ box, header, name?, ctx, totalTables, totalColumns }`. The plan: `{ table, headerWrites, insertRow }` (`insertRow`:
  // a header row goes in above the range, the rows below move down: the header cells are those of the row inserted) or `{ error }`.
  function planCreate({ box, header, name, ctx, totalTables = ctx.tables.length, totalColumns = 0, takenNames = [] }) {
    const width = box.c2 - box.c1 + 1;
    if (totalTables >= MAX_TABLES) return { error: MESSAGES.limit };
    if (totalColumns + width > MAX_COLUMNS_TOTAL) return { error: MESSAGES.columns };
    // The range as the user gave it must clear everything first; the one row a table needs below its header joins it.
    const lone = header && box.r1 === box.r2;
    const rows = box.r2 + (lone ? 1 : 0);
    if (rows > MAX_ROW || (!header && box.r2 >= MAX_ROW)) return { error: MESSAGES.bottom };
    const asked = { ...box, r2: rows };
    const refused = conflict(ctx, asked, null);
    if (refused) return { error: MESSAGES[refused] };
    // Without a header row the data in `box.c1..box.c2` moves down one row and the row that opens holds `Column<n>`; the table covers both.
    const insertRow = !header;
    if (insertRow) {
      if (straddleConflict(ctx, box.r1, box.c1, box.c2)) return { error: MESSAGES.straddle };
      for (let c = box.c1; c <= box.c2; c += 1) {
        const edge = ctx.cellAt(MAX_ROW, c);
        if (edge && (edge.value || edge.formula)) return { error: MESSAGES.bottom };
      }
    }
    const final = { r1: box.r1, c1: box.c1, r2: rows + (insertRow ? 1 : 0), c2: box.c2 };
    const used = new Set();
    const texts = [];
    for (let c = box.c1; c <= box.c2; c += 1) texts.push(insertRow ? "" : cellText(ctx, box.r1, c));
    const columns = headerNames(texts, used);
    const headerWrites = [];
    columns.forEach((text, i) => {
      const at = box.c1 + i;
      const entry = insertRow ? null : ctx.cellAt(box.r1, at);
      if (insertRow || String(entry?.value ?? "") !== text || entry?.formula) headerWrites.push({ row: box.r1, col: at, text });
    });
    const table = {
      name: name || defaultName([...ctx.tables.map((t) => t.name), ...takenNames], totalTables),
      range: rangeText(final.r1, final.c1, final.r2, final.c2),
      header: true,
      totals: false,
      columns,
      style: DEFAULT_STYLE,
      firstColumn: false,
      lastColumn: false,
      rowStripes: true,
      columnStripes: false,
      filter: [],
    };
    return { table, headerWrites, insertRow, box: final };
  }

  // ---- 표 크기 조정 ------------------------------------------------------------------------------------------------------
  // `input`: `{ table, box, ctx, totalColumns }`; the plan: `{ table (the changed JSON), headerWrites }` or `{ error }`.
  function planResize({ table, box, ctx, totalColumns = 0 }) {
    const old = ctx.boxOf(table);
    const header = table.header !== false;
    const totals = table.totals === true;
    if (header && box.r1 !== old.r1) return { error: MESSAGES.header };
    if (!touches(box, old)) return { error: MESSAGES.apart };
    if (box.r2 - box.r1 + 1 < 1 + (header ? 1 : 0) + (totals ? 1 : 0)) return { error: MESSAGES.small };
    if (box.r2 > MAX_ROW || box.c2 > MAX_COL) return { error: MESSAGES.bottom };
    const width = box.c2 - box.c1 + 1;
    if (totalColumns - table.columns.length + width > MAX_COLUMNS_TOTAL) return { error: MESSAGES.columns };
    const refused = conflict(ctx, box, table);
    if (refused) return { error: MESSAGES["resize" + refused[0].toUpperCase() + refused.slice(1)] };
    // The columns that stay keep their names; a column the range takes in is named by its header cell (or `Column<n>`).
    const used = new Set();
    const columns = [];
    const headerWrites = [];
    for (let c = box.c1; c <= box.c2; c += 1) {
      const kept = c >= old.c1 && c <= old.c2 ? table.columns[c - old.c1] : null;
      if (kept !== null) {
        used.add(fold(kept));
        columns.push(kept);
      } else columns.push(null);
    }
    columns.forEach((name, i) => {
      if (name !== null) return;
      const at = box.c1 + i;
      const [text] = headerNames([header ? cellText(ctx, box.r1, at) : ""], used, i + 1);
      columns[i] = text;
      const entry = header ? ctx.cellAt(box.r1, at) : null;
      if (header && (String(entry?.value ?? "") !== text || entry?.formula)) headerWrites.push({ row: box.r1, col: at, text });
    });
    const next = { ...structuredClone(table), range: rangeText(box.r1, box.c1, box.r2, box.c2), columns };
    const mappedCalc = [];
    for (let c = box.c1; c <= box.c2; c += 1) {
      mappedCalc.push(c >= old.c1 && c <= old.c2 ? table.calculatedColumns?.[c - old.c1] ?? null : null);
    }
    const normCalc = normalizeCalc(mappedCalc, columns.length);
    if (normCalc) {
      next.calculatedColumns = normCalc;
      const oldD2 = old.r2 - (totals ? 1 : 0);
      const newD1 = box.r1 + (header ? 1 : 0);
      const newD2 = box.r2 - (totals ? 1 : 0);
      for (let r = Math.max(newD1, oldD2 + 1); r <= newD2; r += 1) {
        normCalc.forEach((formula, i) => {
          if (!formula) return;
          const col = box.c1 + i;
          const cur = ctx.cellAt(r, col);
          if (!cur || (!cur.value && !cur.formula)) headerWrites.push({ row: r, col, text: "", formula });
        });
      }
    } else {
      delete next.calculatedColumns;
    }
    // The conditions of the columns that left go with them; the rows the rest hide are judged again by the page.
    if (Array.isArray(next.filter)) next.filter = next.filter.filter((cond) => cond.col >= box.c1 && cond.col <= box.c2);
    if (Array.isArray(next.hiddenButtons)) {
      const hb = next.hiddenButtons.filter((col) => col >= box.c1 && col <= box.c2);
      if (hb.length) next.hiddenButtons = hb;
      else delete next.hiddenButtons;
    }
    if (Array.isArray(next.slicers)) {
      const sl = header
        ? next.slicers
            .map((s) => ({ ...s, abs: old.c1 + s.col }))
            .filter((s) => s.abs >= box.c1 && s.abs <= box.c2)
            .map(({ abs, ...rest }) => ({ ...rest, col: abs - box.c1 }))
        : [];
      if (sl.length) next.slicers = sl;
      else delete next.slicers;
    }
    delete next.filterHidden;
    return { table: next, headerWrites };
  }

  // ---- growing over what is typed -----------------------------------------------------------------------------------------
  // The plan when the cell at `row`, `col` was just given content: the table of the sheet that has it right above (a row more) or
  // right beside it (a column more) takes it in, `{ table, headerWrites, to }` (`to`: `"row"` or `"column"`); null when none grows.
  // Not a table with a totals row below, not into another table, a merge or the filter, and not past the sheet. `candidates` are
  // the tables the cell may be next to (all of the sheet's by default): a caller with many cells and tables narrows them.
  function planExpand({ row, col, ctx, totalColumns = 0, candidates = ctx.tables }) {
    const typed = ctx.cellAt(row, col);
    if (!typed || (!typed.value && !typed.formula)) return null;
    for (const table of candidates) {
      const g = ctx.boxOf(table);
      const header = table.header !== false;
      const below = row === g.r2 + 1 && col >= g.c1 && col <= g.c2 && table.totals !== true && row <= MAX_ROW;
      const beside = col === g.c2 + 1 && row >= g.r1 && row <= g.r2 && col <= MAX_COL && totalColumns < MAX_COLUMNS_TOTAL;
      if (!below && !beside) continue;
      const grown = below ? { ...g, r2: g.r2 + 1 } : { ...g, c2: g.c2 + 1 };
      const added = below ? { r1: row, c1: g.c1, r2: row, c2: g.c2 } : { r1: g.r1, c1: col, r2: g.r2, c2: col };
      if (conflict(ctx, added, table)) return null;
      const next = { ...structuredClone(table), range: rangeText(grown.r1, grown.c1, grown.r2, grown.c2) };
      const headerWrites = [];
      if (beside) {
        // The header cell the column takes in names it (an empty one is `Column<n>`; a formula is fixed as the text of its result).
        const used = new Set(table.columns.map(fold));
        const [name] = headerNames([header ? cellText(ctx, g.r1, col) : ""], used, table.columns.length + 1);
        next.columns = [...table.columns, name];
        if (Array.isArray(next.calculatedColumns)) next.calculatedColumns = [...next.calculatedColumns, null];
        const entry = ctx.cellAt(g.r1, col);
        if (header && (String(entry?.value ?? "") !== name || entry?.formula)) headerWrites.push({ row: g.r1, col, text: name });
      } else {
        const calc = effectiveCalc(table, g, ctx.cellAt);
        const norm = normalizeCalc(calc, table.columns.length);
        if (norm) {
          next.calculatedColumns = norm;
          norm.forEach((formula, i) => {
            const c = g.c1 + i;
            if (!formula || c === col) return;
            const cur = ctx.cellAt(row, c);
            if (!cur || (!cur.value && !cur.formula)) headerWrites.push({ row, col: c, text: "", formula });
          });
        }
      }
      delete next.filterHidden;
      return { table: next, headerWrites, to: below ? "row" : "column" };
    }
    return null;
  }

  // ---- 중복된 항목 제거 (PD-242) ------------------------------------------------------------------------------------------
  const MAX_DEDUPE_ROWS = 200000;
  const ERROR_LITERALS = new Set(["#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A", "#GETTING_DATA", "#SPILL!", "#CALC!", "#CONNECT!", "#BLOCKED!"]);

  function fnv1aStep(h, str) {
    let x = h >>> 0;
    for (let i = 0; i < str.length; i += 1) x = Math.imul(x ^ str.charCodeAt(i), 0x01000193) >>> 0;
    return x;
  }

  function cellDedupeKey(entry) {
    const raw = String(entry?.value ?? "");
    if (raw === "") return [0, ""];
    const t = entry?.t || "";
    if (t === "e" || (!t && ERROR_LITERALS.has(raw.toUpperCase()))) return [4, raw.toUpperCase()];
    if (t === "b" || (!t && /^(?:true|false)$/i.test(raw))) return [3, raw.length === 4 ? "1" : "0"];
    if (t !== "s") {
      const norm = entry?.style?.numFmt ? host.__EASYOFFICE_SHEET_UTILS__?.normalizeFormattedNumericCell?.(raw) ?? raw : raw;
      const trimmed = norm.trim();
      if (trimmed !== "") {
        const n = Number(trimmed);
        if (Number.isFinite(n)) return [1, String(n === 0 ? 0 : n)];
      }
    }
    return [2, raw.toLowerCase()];
  }

  function sameTuple(a, b) {
    for (let i = 0; i < a.length; i += 1) if (a[i] !== b[i]) return false;
    return true;
  }

  function dedupeScanRows(d1, d2, c1, uniqueCols, ctx) {
    const totalRows = Math.max(0, d2 - d1 + 1);
    if (totalRows <= 1) return { removed: 0, remaining: totalRows, rowMoves: [], clearFrom: 0 };
    const kLen = uniqueCols.length * 2;
    const buckets = new Map();
    const keptRows = [];
    const tuple = new Array(kLen);

    for (let r = d1; r <= d2; r += 1) {
      let h = 0x811c9dc5;
      for (let j = 0; j < uniqueCols.length; j += 1) {
        const [tag, val] = cellDedupeKey(ctx.cellAt(r, c1 + uniqueCols[j]));
        tuple[j * 2] = tag;
        tuple[j * 2 + 1] = val;
        h = fnv1aStep(Math.imul(h ^ (tag + 17), 0x01000193) >>> 0, val);
      }
      const list = buckets.get(h);
      if (list) {
        let dup = false;
        for (let m = 0; m < list.length; m += 1) {
          if (sameTuple(list[m], tuple)) {
            dup = true;
            break;
          }
        }
        if (dup) continue;
        list.push(tuple.slice());
      } else {
        buckets.set(h, [tuple.slice()]);
      }
      keptRows.push(r);
    }

    const remaining = keptRows.length;
    const removed = totalRows - remaining;
    if (removed === 0) return { removed: 0, remaining, rowMoves: [], clearFrom: 0 };
    const rowMoves = [];
    for (let i = 0; i < remaining; i += 1) {
      const dst = d1 + i;
      if (keptRows[i] !== dst) rowMoves.push([keptRows[i], dst]);
    }
    return { removed, remaining, rowMoves, clearFrom: d1 + remaining };
  }

  // Single-pass O(R * K) duplicate scan over `cols` (0-based table column offsets). Keeps first occurrence.
  function planDedupe({ table, cols, ctx }) {
    const uniqueCols = [...new Set((cols || []).filter((c) => Number.isInteger(c) && c >= 0 && c < table.columns.length))].sort((a, b) => a - b);
    if (uniqueCols.length === 0) return { error: "열을 하나 이상 선택" };
    const b = ctx.boxOf(table);
    const header = table.header !== false;
    const totals = table.totals === true;
    const d1 = b.r1 + (header ? 1 : 0);
    const d2 = b.r2 - (totals ? 1 : 0);
    if (Math.max(0, d2 - d1 + 1) > MAX_DEDUPE_ROWS) return { error: "중복 제거 범위는 200,000행까지" };
    const scan = dedupeScanRows(d1, d2, b.c1, uniqueCols, ctx);
    if (scan.removed === 0) return { removed: 0, remaining: scan.remaining, table: null, rowMoves: [], clearFrom: 0, box: b };
    const newR2 = d1 + scan.remaining - 1 + (totals ? 1 : 0);
    if (totals) scan.rowMoves.push([b.r2, newR2]);
    const next = { ...structuredClone(table), range: rangeText(b.r1, b.c1, newR2, b.c2) };
    delete next.filterHidden;
    return {
      removed: scan.removed,
      remaining: scan.remaining,
      table: next,
      rowMoves: scan.rowMoves,
      clearFrom: newR2 + 1,
      box: b,
      reapply: Array.isArray(next.filter) && next.filter.length > 0,
    };
  }

  function planRangeDedupe({ box, header, cols, ctx }) {
    const numCols = Math.max(0, box.c2 - box.c1 + 1);
    if (numCols === 0 || numCols > 1000) return { error: "중복 제거 범위는 1,000열까지" };
    if (straddleConflict(ctx, box, ctx.tables)) return { error: "표와 겹쳐 실행할 수 없음" };
    if (ctx.merges?.some((m) => touches(box, m))) return { error: "병합된 칸이 있어 실행할 수 없음" };
    const uniqueCols = [...new Set((cols || []).filter((c) => Number.isInteger(c) && c >= 0 && c < numCols))].sort((a, b) => a - b);
    if (uniqueCols.length === 0) return { error: "열을 하나 이상 선택" };
    const d1 = box.r1 + (header ? 1 : 0);
    const d2 = box.r2;
    if (Math.max(0, d2 - d1 + 1) > MAX_DEDUPE_ROWS) return { error: "중복 제거 범위는 200,000행까지" };
    const scan = dedupeScanRows(d1, d2, box.c1, uniqueCols, ctx);
    return { ...scan, box };
  }

  host.__EASYOFFICE_SHEET_TABLE_OPS__ = {
    MESSAGES,
    MAX_TABLES,
    MAX_DEDUPE_ROWS,
    DEFAULT_STYLE,
    parseRange,
    defaultName,
    headerNames,
    planCreate,
    planResize,
    planExpand,
    planDedupe,
    planRangeDedupe,
    cellDedupeKey,
    rangeText,
    straddleConflict,
    normalizeCalc,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
