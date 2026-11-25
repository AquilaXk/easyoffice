(function (host) {
  "use strict";

  // Delimited Text-to-Columns (`데이터 ▸ 텍스트 나누기…`, PD-356) with preview and row/column bounds.
  const MAX_SPLIT_ROWS = 10000;
  const MAX_SPLIT_COLS = 64;
  const MAX_LINE_CHARS = 8192;
  const PREVIEW_ROWS = 8;

  // Splits a single string along enabled delimiters using linear code-point scan (zero ReDoS risk).
  function splitDelimitedText(text, opts = {}) {
    const raw = String(text ?? "");
    if (raw.length > MAX_LINE_CHARS) return { refused: "line" };
    const delims = new Set();
    if (opts.tab) delims.add("\t");
    if (opts.semicolon) delims.add(";");
    if (opts.comma) delims.add(",");
    if (opts.space) delims.add(" ");
    if (opts.other) {
      const first = Array.from(String(opts.otherChar || ""))[0];
      if (first) delims.add(first);
    }
    if (delims.size === 0) return { cols: [raw] };

    const collapse = Boolean(opts.collapse);
    const cols = [];
    let cur = "";
    let prevDelim = false;
    for (const ch of raw) {
      if (delims.has(ch)) {
        if (collapse && prevDelim) continue;
        if (!collapse || cur.length > 0 || cols.length > 0) {
          cols.push(cur);
          if (cols.length > MAX_SPLIT_COLS) return { refused: "cols" };
          cur = "";
        }
        prevDelim = true;
      } else {
        cur += ch;
        prevDelim = false;
      }
    }
    if (!collapse || cur.length > 0 || cols.length === 0) {
      cols.push(cur);
      if (cols.length > MAX_SPLIT_COLS) return { refused: "cols" };
    }
    return { cols };
  }

  function createTextColumnsUi(deps) {
    const {
      document,
      kit,
      grid,
      sheetDataStore,
      getActiveSheetName,
      getRange,
      commitIfEditing,
      snapshotDomSheetCells,
      applyCellEdits,
      focusAnchor,
      say,
    } = deps;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;
    const { $, val, checked, setVal, setChecked, openDialog, closeDialog, note } = kit;
    let bound = false;
    let activeSpan = null;
    let activeRows = [];

    function readOptions() {
      return {
        tab: checked("sheet-tc-tab"),
        semicolon: checked("sheet-tc-semicolon"),
        comma: checked("sheet-tc-comma"),
        space: checked("sheet-tc-space"),
        other: checked("sheet-tc-other"),
        otherChar: val("sheet-tc-other-char"),
        collapse: checked("sheet-tc-collapse"),
        overwrite: checked("sheet-tc-overwrite"),
      };
    }

    function sourceSpanFor(map, rg) {
      if (rg.r1 !== rg.r2 || rg.c1 !== rg.c2) return { r1: rg.r1, r2: rg.r2, c1: rg.c1 };
      let r1 = rg.r1;
      let r2 = rg.r1;
      const textAt = (r) => {
        const e = map[grid.refOf(r, rg.c1)];
        return e ? String(e.formula || e.value || "") : "";
      };
      if (textAt(r1) === "") return { r1, r2, c1: rg.c1 };
      while (r1 > 1 && textAt(r1 - 1) !== "" && r2 - r1 + 1 < MAX_SPLIT_ROWS) r1 -= 1;
      while (r2 < grid.MAX_ROWS && textAt(r2 + 1) !== "" && r2 - r1 + 1 < MAX_SPLIT_ROWS) r2 += 1;
      return { r1, r2, c1: rg.c1 };
    }

    function computePlan(opts) {
      if (!activeSpan) return { refused: "empty" };
      const sheet = getActiveSheetName();
      const map = sheetDataStore[sheet] || {};
      const splitRows = [];
      let maxCols = 1;
      let hasData = false;
      for (let i = 0; i < activeRows.length; i += 1) {
        const { row, raw } = activeRows[i];
        if (raw !== "") hasData = true;
        const res = splitDelimitedText(raw, opts);
        if (res.refused) return res;
        if (res.cols.length > maxCols) maxCols = res.cols.length;
        splitRows.push({ row, cols: res.cols });
      }
      if (!hasData) return { refused: "empty" };
      if (activeSpan.c1 + maxCols - 1 > grid.MAX_COLS) return { refused: "grid" };
      let collision = false;
      if (maxCols > 1) {
        for (let r = activeSpan.r1; r <= activeSpan.r2 && !collision; r += 1) {
          for (let c = activeSpan.c1 + 1; c <= activeSpan.c1 + maxCols - 1; c += 1) {
            const e = map[grid.refOf(r, c)];
            if (e && (String(e.value ?? "") !== "" || String(e.formula ?? "") !== "")) {
              collision = true;
              break;
            }
          }
        }
      }
      return { splitRows, maxCols, collision };
    }

    function renderPreview() {
      const table = $("sheet-tc-preview-table");
      if (!table) return;
      const opts = readOptions();
      const otherInput = $("sheet-tc-other-char");
      if (otherInput) otherInput.disabled = !opts.other;
      const plan = computePlan(opts);
      const thead = table.querySelector("thead");
      const tbody = table.querySelector("tbody");
      thead?.replaceChildren();
      tbody?.replaceChildren();
      if (plan.refused) {
        const msg =
          plan.refused === "cols"
            ? "분할 열 수는 최대 " + MAX_SPLIT_COLS + "개"
            : plan.refused === "line"
              ? "셀 길이는 최대 " + MAX_LINE_CHARS + "자"
              : plan.refused === "grid"
                ? "시트 열 범위를 초과했습니다."
                : "나눌 데이터 없음";
        note("sheet-tc-status", msg);
        return;
      }
      if (thead) {
        const tr = document.createElement("tr");
        for (let c = 0; c < plan.maxCols; c += 1) {
          const th = document.createElement("th");
          th.textContent = grid.colName(activeSpan.c1 + c);
          tr.appendChild(th);
        }
        thead.appendChild(tr);
      }
      if (tbody) {
        const shown = plan.splitRows.slice(0, PREVIEW_ROWS);
        for (let i = 0; i < shown.length; i += 1) {
          const tr = document.createElement("tr");
          for (let c = 0; c < plan.maxCols; c += 1) {
            const td = document.createElement("td");
            td.textContent = shown[i].cols[c] ?? "";
            tr.appendChild(td);
          }
          tbody.appendChild(tr);
        }
      }
      if (plan.collision && !opts.overwrite) {
        note("sheet-tc-status", "대상 셀에 데이터가 있음 · 덮어쓰기 선택 필요");
      } else {
        note("sheet-tc-status", "");
      }
    }

    function open() {
      ensureBound();
      commitIfEditing();
      const sheet = getActiveSheetName();
      if (protectApi && !protectApi.guardAction(sheet, "formatCells")) return false;
      const rg = getRange();
      if (rg.c1 !== rg.c2) {
        say("텍스트 나누기는 한 열만 선택");
        return false;
      }
      const map = snapshotDomSheetCells();
      sheetDataStore[sheet] = map;
      const span = sourceSpanFor(map, rg);
      if (span.r2 - span.r1 + 1 > MAX_SPLIT_ROWS) {
        say("텍스트 나누기는 최대 " + MAX_SPLIT_ROWS + "행까지");
        return false;
      }
      const rows = [];
      let hasData = false;
      for (let r = span.r1; r <= span.r2; r += 1) {
        const entry = map[grid.refOf(r, span.c1)];
        const raw = entry ? String(entry.formula || entry.value || "") : "";
        if (raw.length > MAX_LINE_CHARS) {
          say("셀 길이는 최대 " + MAX_LINE_CHARS + "자");
          return false;
        }
        if (raw !== "") hasData = true;
        rows.push({ row: r, raw });
      }
      if (!hasData) {
        say("나눌 데이터가 없습니다");
        return false;
      }
      activeSpan = span;
      activeRows = rows;
      setChecked("sheet-tc-tab", true);
      setChecked("sheet-tc-semicolon", false);
      setChecked("sheet-tc-comma", false);
      setChecked("sheet-tc-space", false);
      setChecked("sheet-tc-other", false);
      setVal("sheet-tc-other-char", "");
      setChecked("sheet-tc-collapse", false);
      setChecked("sheet-tc-overwrite", false);
      renderPreview();
      openDialog("sheet-text-columns-dialog");
      return true;
    }

    function submit() {
      const opts = readOptions();
      const plan = computePlan(opts);
      if (plan.refused) {
        renderPreview();
        return;
      }
      if (plan.collision && !opts.overwrite) {
        note("sheet-tc-status", "대상 셀에 데이터가 있음 · 덮어쓰기 선택 필요");
        return;
      }
      const sheet = getActiveSheetName();
      const r1 = activeSpan.r1, r2 = activeSpan.r2, c1 = activeSpan.c1, c2 = c1 + plan.maxCols - 1;
      if (layoutApi?.mergesIn(sheet, r1, c1, r2, c2).length > 0) {
        note("sheet-tc-status", "병합된 셀과 겹쳐 나눌 수 없음");
        return;
      }
      const map = sheetDataStore[sheet] || {};
      const edits = [];
      for (let i = 0; i < plan.splitRows.length; i += 1) {
        const { row, cols } = plan.splitRows[i];
        for (let k = 0; k < cols.length; k += 1) {
          const ref = grid.refOf(row, c1 + k);
          if (map[ref]?.spillOf) {
            note("sheet-tc-status", "배열 수식 셀과 겹쳐 나눌 수 없음");
            return;
          }
          const parsed = cellTypeApi.parseTypedInput(cols[k], map[ref]?.style?.numFmt || "");
          const ed = { ref, value: parsed.value, formula: parsed.formula };
          if (parsed.t) ed.t = parsed.t;
          edits.push(ed);
        }
      }
      closeDialog("sheet-text-columns-dialog");
      void applyCellEdits(edits, "텍스트 나누기").then(() => focusAnchor()).catch(() => {});
    }

    function ensureBound() {
      if (bound) return;
      bound = true;
      for (const id of [
        "sheet-tc-tab", "sheet-tc-semicolon", "sheet-tc-comma",
        "sheet-tc-space", "sheet-tc-other", "sheet-tc-collapse", "sheet-tc-overwrite",
      ]) {
        $(id)?.addEventListener("change", renderPreview);
      }
      $("sheet-tc-other-char")?.addEventListener("input", renderPreview);
      $("btn-sheet-tc-apply")?.addEventListener("click", submit);
    }

    return { open, ensureBound };
  }

  host.__EASYOFFICE_SPLIT_DELIMITED_TEXT__ = splitDelimitedText;
  host.__EASYOFFICE_CREATE_SHEET_TEXT_COLUMNS__ = createTextColumnsUi;
  host.__EASYOFFICE_TEXT_COLUMNS_LIMITS__ = { MAX_SPLIT_ROWS, MAX_SPLIT_COLS, MAX_LINE_CHARS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
