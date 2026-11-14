(function (host) {
  "use strict";

  // Range clipboard for the sheet editor: TSV text through the system clipboard (clipboard
  // events when the browser provides them, `navigator.clipboard` for toolbar commands) with an
  // in-app copy so paste keeps working when the system clipboard is unavailable. The in-app copy
  // is a snapshot of the cells (content, formulas, formats, column widths), so a paste of it
  // carries the formats, and the paste special modes (sheet-paste-core.js) can pick a part of it.
  // Relative A1 references in pasted formulas shift by the copy -> paste offset when the clipboard
  // text is the in-app copy (see shiftFormula for what counts as a reference). Rows hidden by an
  // auto filter are not copied.

  // One formula token is either a string literal (copied as is) or a reference: an optional sheet
  // prefix (`Name!` or a quoted name with '' escapes) followed by a cell, a cell range, a column
  // range or a row range, so a prefixed range moves as a unit. Names that merely look like a cell
  // (`A1B`, `LOG10(`, `my_name1`) are rejected by the boundary checks.
  const CELL = String.raw`\$?[A-Za-z]{1,3}\$?[1-9]\d{0,6}`;
  const COL_RANGE = String.raw`\$?[A-Za-z]{1,3}:\$?[A-Za-z]{1,3}`;
  const ROW_RANGE = String.raw`\$?[1-9]\d{0,6}:\$?[1-9]\d{0,6}`;
  const TOKEN_RE = new RegExp(
    String.raw`"(?:[^"]|"")*"|(?<![\p{L}\p{N}_.$!])((?:'(?:[^']|'')+'|[\p{L}\p{N}_.]+)!)?` +
      `(${CELL}(?::${CELL})?|${COL_RANGE}|${ROW_RANGE})` +
      String.raw`(?![\p{L}\p{N}_(!])`,
    "gu"
  );
  const REF_END_RE = /^(\$?)([A-Za-z]*)(\$?)(\d*)$/;
  const CELL_END_RE = /^([A-Za-z]+)(\d+)$/;

  function entryRaw(entry) {
    if (!entry) return "";
    return entry.formula || entry.value || "";
  }

  function dataOf(ev) {
    return ev.clipboardData || ev.dataTransfer || null;
  }

  async function writeSystem(text) {
    const clip = host.navigator?.clipboard;
    if (!clip || typeof clip.writeText !== "function") return false;
    try {
      await clip.writeText(text);
      return true;
    } catch {
      return false;
    }
  }

  async function readSystem() {
    const clip = host.navigator?.clipboard;
    if (!clip || typeof clip.readText !== "function") return "";
    try {
      return (await clip.readText()) || "";
    } catch {
      return "";
    }
  }

  function quoteField(text) {
    return /[\t\r\n"]/.test(text) ? '"' + text.replaceAll('"', '""') + '"' : text;
  }

  function parseTsv(text) {
    const src = String(text ?? "").replace(/\r\n?/g, "\n");
    const rows = [];
    let row = [];
    let field = "";
    let inQuotes = false;
    let fieldStart = true;
    for (let i = 0; i < src.length; i += 1) {
      const ch = src[i];
      if (inQuotes) {
        if (ch === '"' && src[i + 1] === '"') {
          field += '"';
          i += 1;
        } else if (ch === '"') {
          inQuotes = false;
        } else {
          field += ch;
        }
      } else if (ch === '"' && fieldStart) {
        inQuotes = true;
        fieldStart = false;
      } else if (ch === "\t") {
        row.push(field);
        field = "";
        fieldStart = true;
      } else if (ch === "\n") {
        row.push(field);
        rows.push(row);
        row = [];
        field = "";
        fieldStart = true;
      } else {
        field += ch;
        fieldStart = false;
      }
    }
    if (field !== "" || row.length > 0) {
      row.push(field);
      rows.push(row);
    }
    return rows;
  }

  function createSheetClipboard(api) {
    const { grid, isFormulaStr } = api;
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    const core = host.__EASYOFFICE_SHEET_PASTE_CORE__;
    let internal = null;

    const sheetName = () => api.getSheetName?.() ?? "";
    const say = (text) => api.feedback?.(text);

    // The rows an auto filter hides are left out of a copy.
    function filteredOut(row) {
      const filter = layout?.get(sheetName())?.filter;
      return Boolean(filter) && layout.runContains(filter.hidden, row);
    }

    // The in-app copy of a range: the cells as they are now and the text the system clipboard gets.
    function snapshotOf(range) {
      const rows = [];
      const cells = [];
      for (let r = range.r1; r <= range.r2; r += 1) {
        if (filteredOut(r)) continue;
        rows.push(r);
        const line = [];
        for (let c = range.c1; c <= range.c2; c += 1) {
          const e = api.getEntry(grid.refOf(r, c));
          // A protected sheet copies the value, not the formula, of a cell whose format hides it (PD-216).
          const hidden = e?.formula && host.__EASYOFFICE_SHEET_PROTECT__?.formulaHidden(sheetName(), r, c, e.style);
          const cell = e && (e.value || e.formula || e.style) ? { value: e.value || "", formula: hidden ? "" : e.formula || "", style: e.style } : null;
          if (cell && e.link) cell.link = e.link;
          // A cell an array filled whose formula is copied too comes back with the formula (PD-217): pasting the formula
          // recreates it, pasting values still writes it.
          const source = e?.spillOf ? host.__EASYOFFICE_SHEET_SPILL__?.parseRef(e.spillOf) : null;
          if (cell && source && source.row >= range.r1 && source.row <= range.r2 && source.col >= range.c1 && source.col <= range.c2) {
            cell.spillChild = true;
          }
          line.push(cell);
        }
        cells.push(line);
      }
      const cols = range.c2 - range.c1 + 1;
      const widths = [];
      for (let c = range.c1; c <= range.c2; c += 1) widths.push(grid.colHidden?.(c) ? 0 : grid.colWidth(c));
      const text = cells.map((line) => line.map((cell) => quoteField(cell ? cell.formula || cell.value : "")).join("\t")).join("\n");
      return { text, rows, c1: range.c1, cols, cells, widths };
    }

    // Moves one end of a reference; null when it leaves the grid.
    function shiftRefEnd(end, dRow, dCol) {
      const [, first, letters, second, digits] = REF_END_RE.exec(end);
      const colAbs = letters ? first : "";
      const rowAbs = letters ? second : first;
      let out = "";
      if (letters) {
        const col = grid.colIndex(letters.toUpperCase()) + (colAbs ? 0 : dCol);
        if (col < 1 || col > grid.MAX_COLS) return null;
        out += colAbs + grid.colName(col);
      }
      if (digits) {
        const row = Number(digits) + (rowAbs ? 0 : dRow);
        if (row < 1 || row > grid.MAX_ROWS) return null;
        out += rowAbs + row;
      }
      return out;
    }

    // A turned-over paste (rows and columns swapped) swaps the row and column offsets of a
    // reference from its formula: a cell whose row and column are both relative moves to the swapped
    // offset from the formula's new cell; a range follows only when both ends are such cells. Every
    // other reference (a `$` part, a whole row or column) stays as written. `turn` is
    // `{ fromRow, fromCol, toRow, toCol }`, the formula's cell before and after.
    function turnRefEnd(end, turn) {
      const [, letters, digits] = CELL_END_RE.exec(end);
      const row = turn.toRow + (grid.colIndex(letters.toUpperCase()) - turn.fromCol);
      const col = turn.toCol + (Number(digits) - turn.fromRow);
      return row < 1 || row > grid.MAX_ROWS || col < 1 || col > grid.MAX_COLS ? null : grid.colName(col) + row;
    }

    // Standard copy semantics: relative parts of every reference (other sheets included, both
    // ends of a range) move by the offset, `$` parts stay, and a reference pushed off the grid
    // becomes `#REF!`. String literals are never touched.
    function shiftFormula(formula, dRow, dCol, turn) {
      return formula.replace(TOKEN_RE, (token, sheet, ref) => {
        if (ref === undefined) return token;
        if (turn) {
          const parts = ref.split(":");
          if (!parts.every((end) => CELL_END_RE.test(end))) return token;
          const turned = parts.map((end) => turnRefEnd(end, turn));
          return (sheet || "") + (turned.includes(null) ? "#REF!" : turned.join(":"));
        }
        const ends = ref.split(":").map((end) => shiftRefEnd(end, dRow, dCol));
        return (sheet || "") + (ends.includes(null) ? "#REF!" : ends.join(":"));
      });
    }

    // Delete clears the value and leaves the cell's hyperlink (as the spreadsheet programs do); a cut moves the link
    // with the cell, so its source loses it (`dropLinks`, PD-218).
    function clearEdits(range, dropLinks) {
      const edits = [];
      for (let r = range.r1; r <= range.r2; r += 1) {
        for (let c = range.c1; c <= range.c2; c += 1) {
          edits.push(dropLinks ? { ref: grid.refOf(r, c), value: "", formula: "", link: null } : { ref: grid.refOf(r, c), value: "", formula: "" });
        }
      }
      return edits;
    }

    function clearRange(range, dropLinks = false) {
      const area = (range.r2 - range.r1 + 1) * (range.c2 - range.c1 + 1);
      if (area > 200000) return api.applyCellEdits(api.clearExisting(range, dropLinks), "범위 지우기");
      return api.applyCellEdits(clearEdits(range, dropLinks), "범위 지우기");
    }

    // Clipboard text as a block without a source: a quoted field keeps tabs and line breaks.
    function textBlock(text) {
      const rows = parseTsv(text);
      return {
        rows: null,
        c1: 0,
        cols: rows.reduce((n, row) => Math.max(n, row.length), 0),
        cells: rows.map((row) => row.map((value) => ({ value, formula: "", style: undefined }))),
        widths: [],
      };
    }

    // A paste may not cut through a merged range (a range inside the pasted area is overwritten).
    function cutsMerge(target, rowsN, colsN) {
      const r2 = target.r1 + rowsN - 1;
      const c2 = target.c1 + colsN - 1;
      return (layout?.mergesIn(sheetName(), target.r1, target.c1, r2, c2) || []).some(
        (m) => m.r1 < target.r1 || m.c1 < target.c1 || m.r2 > r2 || m.c2 > c2
      );
    }

    // Clipboard text carries no formula of an array: written over cells an array filled but not over the cell of its formula, it
    // would change a part of the array (PD-222). The in-app copy knows which cells are results, so it is not asked.
    function cutsArray(target, rowsN, colsN) {
      const spill = host.__EASYOFFICE_SHEET_SPILL__;
      if (!spill) return false;
      const r2 = target.r1 + rowsN - 1;
      const c2 = target.c1 + colsN - 1;
      const inside = (at) => at && at.row >= target.r1 && at.row <= r2 && at.col >= target.c1 && at.col <= c2;
      for (let r = target.r1; r <= r2; r += 1) {
        for (let c = target.c1; c <= c2; c += 1) {
          const e = api.getEntry(grid.refOf(r, c));
          if (spill.isChild(e) && !inside(spill.parseRef(e.spillOf))) return true;
        }
      }
      return false;
    }

    // Pastes a block (the in-app copy or clipboard text) at the range: `mode` is one of
    // `core.MODES`, `transpose` turns rows and columns over.
    function pasteBlock(snap, range, options) {
      if (snap.cells.length === 0) return Promise.resolve(false);
      const mode = options?.mode || "all";
      const transpose = Boolean(options?.transpose);
      const target = { r1: range.r1, c1: range.c1, r2: range.r2, c2: range.c2 };
      const plan = core.planPaste({
        snap,
        mode,
        transpose,
        target,
        getEntry: (ref) => api.getEntry(ref),
        shiftFormula,
        isFormulaStr,
        grid,
      });
      if (plan.edits.length === 0 && !plan.widths) {
        say("붙일 " + (mode === "widths" ? "열 너비" : "서식") + " 없음");
        return Promise.resolve(null);
      }
      const single = snap.cells.length === 1 && snap.cells[0].length === 1;
      const rowsN = transpose ? snap.cols : snap.cells.length;
      const colsN = transpose ? snap.cells.length : snap.cols;
      const spread = single && (range.r2 > range.r1 || range.c2 > range.c1);
      const pasteRows = spread ? range.r2 - range.r1 + 1 : rowsN;
      const pasteCols = spread ? range.c2 - range.c1 + 1 : colsN;
      if (cutsMerge(target, pasteRows, pasteCols)) {
        say("병합 셀과 겹침");
        return Promise.resolve(null);
      }
      const writesContent = mode !== "formats" && mode !== "widths";
      if (snap.rows === null && writesContent && cutsArray(target, pasteRows, pasteCols)) {
        say("배열의 일부분을 변경할 수 없음");
        return Promise.resolve(null);
      }
      const extra = plan.clipped > 0 ? " (시트 범위를 벗어난 " + plan.clipped + "칸 제외)" : "";
      const label = mode === "all" && !transpose ? "붙여넣기" : "골라 붙이기";
      return api.applyCellEdits(plan.edits, label + extra, plan.widths);
    }

    function remember(snap) {
      internal = snap;
    }

    // The block a paste of `text` uses: the in-app copy when the clipboard still holds its text.
    function blockOf(text) {
      return internal && internal.text === text ? internal : textBlock(text);
    }

    function onCopy(ev, range, cut) {
      const snap = snapshotOf(range);
      remember(snap);
      const data = dataOf(ev);
      if (!data || typeof data.setData !== "function") return false;
      data.setData("text/plain", snap.text);
      ev.preventDefault();
      if (cut) void clearRange(range, true).catch(() => {});
      return true;
    }

    function onPaste(ev, range) {
      const data = dataOf(ev);
      const text = data && typeof data.getData === "function" ? data.getData("text/plain") || data.getData("text") : "";
      ev.preventDefault();
      if (!text) return Promise.resolve(false);
      return pasteBlock(blockOf(text), range);
    }

    // Menu commands read the system clipboard; the in-app copy answers when it is unavailable.
    // False when there is nothing to paste, null when the paste was refused (and said so).
    async function pasteFromClipboard(range, options) {
      const text = (await readSystem()) || internal?.text || "";
      return text ? pasteBlock(blockOf(text), range, options) : false;
    }

    async function pasteSpecial(range, options) {
      const done = await pasteFromClipboard(range, options);
      if (done === false) say("붙일 클립보드 내용이 없습니다");
      return done;
    }

    // Toolbar / menu commands (no clipboard event available).
    async function runCommand(cmd, range) {
      if (cmd === "clip-paste") return pasteFromClipboard(range);
      const snap = snapshotOf(range);
      remember(snap);
      const toSystem = await writeSystem(snap.text);
      if (cmd === "clip-cut") await clearRange(range, true);
      return toSystem ? "system" : "internal";
    }

    return {
      toTsv: (range) => snapshotOf(range).text,
      parseTsv,
      shiftFormula,
      onCopy,
      onPaste,
      clearRange,
      runCommand,
      pasteText: (text, range) => pasteBlock(blockOf(text), range),
      pasteSpecial,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_CLIPBOARD__ = createSheetClipboard;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
