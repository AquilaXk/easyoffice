(function (host) {
  "use strict";

  // Range clipboard for the sheet editor: TSV text through the system clipboard (clipboard
  // events when the browser provides them, `navigator.clipboard` for toolbar commands) with an
  // in-app copy so paste keeps working when the system clipboard is unavailable. Relative A1
  // references in pasted formulas shift by the copy -> paste offset when the clipboard text is
  // the in-app copy (plain A1 refs, ranges and `$` anchors; text that merely looks like a
  // reference inside quoted sheet names is not recognised).
  const REF_RE = /(?<![\w.!])(\$?)([A-Za-z]{1,3})(\$?)([1-9]\d{0,6})(?![\w(])/g;

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

  function createSheetClipboard(api) {
    const { grid, isFormulaStr } = api;
    let internal = null;

    function quoteField(text) {
      return /[\t\r\n"]/.test(text) ? '"' + text.replaceAll('"', '""') + '"' : text;
    }

    function toTsv(range) {
      const lines = [];
      for (let r = range.r1; r <= range.r2; r += 1) {
        const fields = [];
        for (let c = range.c1; c <= range.c2; c += 1) {
          fields.push(quoteField(entryRaw(api.getEntry(grid.refOf(r, c)))));
        }
        lines.push(fields.join("\t"));
      }
      return lines.join("\n");
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

    function shiftFormula(formula, dRow, dCol) {
      return formula
        .split(/("(?:[^"]|"")*")/)
        .map((part, idx) => {
          if (idx % 2 === 1) return part;
          return part.replace(REF_RE, (m, cAbs, letters, rAbs, digits) => {
            const col = grid.colIndex(letters.toUpperCase()) + (cAbs ? 0 : dCol);
            const row = Number(digits) + (rAbs ? 0 : dRow);
            if (col < 1 || col > grid.MAX_COLS || row < 1 || row > grid.MAX_ROWS) return "#REF!";
            return cAbs + grid.colName(col) + rAbs + row;
          });
        })
        .join("");
    }

    function buildEdits(grid2d, target, origin) {
      const edits = [];
      let clipped = 0;
      const dRow = origin ? target.row - origin.row : 0;
      const dCol = origin ? target.col - origin.col : 0;
      grid2d.forEach((cols, i) => {
        cols.forEach((raw, j) => {
          const row = target.row + i;
          const col = target.col + j;
          if (row > grid.MAX_ROWS || col > grid.MAX_COLS) {
            clipped += 1;
            return;
          }
          const text = String(raw);
          const isFormula = isFormulaStr(text);
          const finalText = isFormula && origin ? shiftFormula(text, dRow, dCol) : text;
          edits.push({
            ref: grid.refOf(row, col),
            value: isFormula ? "" : finalText,
            formula: isFormula ? finalText : "",
          });
        });
      });
      return { edits, clipped };
    }

    function clearEdits(range) {
      const edits = [];
      for (let r = range.r1; r <= range.r2; r += 1) {
        for (let c = range.c1; c <= range.c2; c += 1) {
          edits.push({ ref: grid.refOf(r, c), value: "", formula: "" });
        }
      }
      return edits;
    }

    function clearRange(range) {
      const area = (range.r2 - range.r1 + 1) * (range.c2 - range.c1 + 1);
      if (area > 200000) return api.applyCellEdits(api.clearExisting(range), "범위 지우기");
      return api.applyCellEdits(clearEdits(range), "범위 지우기");
    }

    function remember(range, text) {
      internal = { text, origin: { row: range.r1, col: range.c1 } };
    }

    function pasteText(text, range) {
      let rows = parseTsv(text);
      if (rows.length === 0) return Promise.resolve(false);
      const origin = internal && internal.text === text ? internal.origin : null;
      const target = { row: range.r1, col: range.c1 };
      const single = rows.length === 1 && rows[0].length === 1;
      const multiSel = range.r2 > range.r1 || range.c2 > range.c1;
      if (single && multiSel) {
        const fill = [];
        for (let r = range.r1; r <= range.r2; r += 1) {
          fill.push(new Array(range.c2 - range.c1 + 1).fill(rows[0][0]));
        }
        rows = fill;
      }
      const { edits, clipped } = buildEdits(rows, target, origin);
      const extra = clipped > 0 ? " (시트 범위를 벗어난 " + clipped + "칸 제외)" : "";
      return api.applyCellEdits(edits, "붙여넣기" + extra);
    }

    function onCopy(ev, range, cut) {
      const text = toTsv(range);
      remember(range, text);
      const data = dataOf(ev);
      if (!data || typeof data.setData !== "function") return false;
      data.setData("text/plain", text);
      ev.preventDefault();
      if (cut) void clearRange(range).catch(() => {});
      return true;
    }

    function onPaste(ev, range) {
      const data = dataOf(ev);
      const text = data && typeof data.getData === "function" ? data.getData("text/plain") || data.getData("text") : "";
      ev.preventDefault();
      if (!text) return Promise.resolve(false);
      return pasteText(text, range);
    }

    // Toolbar / menu commands (no clipboard event available).
    async function runCommand(cmd, range) {
      if (cmd === "clip-paste") {
        const sys = await readSystem();
        const text = sys || internal?.text || "";
        if (!text) return false;
        return pasteText(text, range);
      }
      const text = toTsv(range);
      remember(range, text);
      const toSystem = await writeSystem(text);
      if (cmd === "clip-cut") await clearRange(range);
      return toSystem ? "system" : "internal";
    }

    return { toTsv, parseTsv, shiftFormula, onCopy, onPaste, clearRange, runCommand, pasteText };
  }

  host.__EASYOFFICE_CREATE_SHEET_CLIPBOARD__ = createSheetClipboard;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
