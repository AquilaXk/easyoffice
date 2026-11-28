(function (host) {
  "use strict";

  // 표 편집 of the slide editor (PD-398, 셀 병합 and 셀 분할 PD-402): rows and columns are added above, below, left and right of the
  // cell that was last clicked, and the row or column of that cell is deleted (the layout commands of the Korean presentation
  // program's 표 도구). 셀 병합 joins the cells from the clicked cell to the cell Shift+clicked, 셀 분할 takes a merged cell apart
  // (`slide-table-span.js`). The table is the element's `rows` (text per cell) and `merges`; the limits are the file engine's
  // (64 rows, 32 columns) and a command that would pass them says so and changes nothing. The word processor's `buildTable`
  // (`doc-table.js`) builds DOM tables of the document editor, which a slide table is not; the row and column arithmetic here is
  // the whole feature.
  const { MAX_ROWS, MAX_COLS, shiftMerges, mergeRange, splitAt } = host.__EASYOFFICE_SLIDE_TABLE_SPAN__;
  const MSG_NO_TABLE = "표를 먼저 선택하세요";
  const MSG_NO_RANGE = "병합할 셀을 선택하세요(Shift+클릭으로 범위 선택)";
  const MSG_NO_MERGE = "병합된 셀을 선택하세요";

  // The element's rows and merges after the command; `null` when the command does not apply (past a limit, or the last row or column).
  function edit(rows, at, command, merges = []) {
    const cols = rows[0]?.length ?? 0;
    const blankRow = () => Array.from({ length: cols }, () => "");
    const copy = rows.map((r) => [...r]);
    switch (command) {
      case "row-above": case "row-below": {
        if (rows.length >= MAX_ROWS) return null;
        const to = at.row + (command === "row-below" ? 1 : 0);
        copy.splice(to, 0, blankRow());
        return { rows: copy, merges: shiftMerges(merges, "row", to, 1) };
      }
      case "col-left": case "col-right": {
        if (cols >= MAX_COLS) return null;
        const to = at.col + (command === "col-right" ? 1 : 0);
        copy.forEach((r) => r.splice(to, 0, ""));
        return { rows: copy, merges: shiftMerges(merges, "col", to, 1) };
      }
      case "row-delete":
        if (rows.length <= 1) return null;
        copy.splice(at.row, 1);
        return { rows: copy, merges: shiftMerges(merges, "row", at.row, -1) };
      case "col-delete":
        if (cols <= 1) return null;
        copy.forEach((r) => r.splice(at.col, 1));
        return { rows: copy, merges: shiftMerges(merges, "col", at.col, -1) };
      default:
        return null;
    }
  }

  const COMMANDS = Object.freeze({
    "table-row-above": ["row-above", "위에 행을 삽입했습니다"],
    "table-row-below": ["row-below", "아래에 행을 삽입했습니다"],
    "table-col-left": ["col-left", "왼쪽에 열을 삽입했습니다"],
    "table-col-right": ["col-right", "오른쪽에 열을 삽입했습니다"],
    "table-row-delete": ["row-delete", "행을 삭제했습니다"],
    "table-col-delete": ["col-delete", "열을 삭제했습니다"],
  });

  function createSlideTable(deps, canvas) {
    const { document, pushUndoSnapshot } = deps;
    const say = (msg) => {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = msg;
    };
    // The cell last clicked (the table's id and the position) and, after a Shift+click, the opposite corner of the range.
    let active = null;
    let corner = null;

    function selectedTable() {
      const el = canvas.getSelectedElement();
      return el?.kind === "table" ? el : null;
    }

    function paintRange(el) {
      const layer = document.getElementById("slide-canvas-layer");
      layer?.querySelectorAll?.(".is-cell-selected").forEach((n) => n.classList.remove("is-cell-selected"));
      if (!active || !corner || active.id !== el?.id) return;
      const [r0, r1] = [Math.min(active.row, corner.row), Math.max(active.row, corner.row)];
      const [c0, c1] = [Math.min(active.col, corner.col), Math.max(active.col, corner.col)];
      layer?.querySelectorAll?.(`[data-element-id="${el.id}"] td, [data-element-id="${el.id}"] th`).forEach((cell) => {
        const r = Number(cell.dataset.rowIdx), c = Number(cell.dataset.colIdx);
        if (r >= r0 && r <= r1 && c >= c0 && c <= c1) cell.classList.add("is-cell-selected");
      });
    }

    function mergeCells(el) {
      const from = active?.id === el.id ? active : null;
      const to = from && corner ? corner : from;
      if (!from) return say(MSG_NO_RANGE);
      const range = { r0: Math.min(from.row, to.row), r1: Math.max(from.row, to.row), c0: Math.min(from.col, to.col), c1: Math.max(from.col, to.col) };
      const next = mergeRange(el.rows, el.merges || [], range);
      if (!next) return say(MSG_NO_RANGE);
      pushUndoSnapshot(false);
      canvas.updateElement(el.id, { rows: next.rows, merges: next.merges }, { recordUndo: false });
      active = { id: el.id, row: next.range.r0, col: next.range.c0 };
      corner = null;
      say("셀을 병합했습니다");
    }

    function splitCell(el) {
      const at = active?.id === el.id ? active : null;
      const merges = at ? splitAt(el.merges || [], el.rows.length, el.rows[0]?.length ?? 0, at) : null;
      if (!merges) return say(MSG_NO_MERGE);
      pushUndoSnapshot(false);
      canvas.updateElement(el.id, { merges }, { recordUndo: false });
      corner = null;
      say("셀을 분할했습니다");
    }

    function run(act) {
      const isEdit = Object.hasOwn(COMMANDS, act);
      if (!isEdit && act !== "table-merge" && act !== "table-split") return false;
      const el = selectedTable();
      if (!el) {
        say(MSG_NO_TABLE);
        return true;
      }
      if (act === "table-merge") mergeCells(el);
      else if (act === "table-split") splitCell(el);
      else {
        const [command, done] = COMMANDS[act];
        const cell = active?.id === el.id ? active : { row: el.rows.length - 1, col: (el.rows[0]?.length ?? 1) - 1 };
        const next = edit(el.rows, cell, command, el.merges || []);
        if (!next) {
          say(command.endsWith("delete") ? "마지막 행이나 열은 삭제할 수 없습니다" : "표가 가득 찼습니다 (행 " + MAX_ROWS + "개, 열 " + MAX_COLS + "개까지)");
          return true;
        }
        pushUndoSnapshot(false);
        canvas.updateElement(el.id, next, { recordUndo: false });
        active = null;
        corner = null;
        say(done);
      }
      paintRange(selectedTable());
      return true;
    }

    function bind() {
      document.getElementById("slide-canvas-layer")?.addEventListener("click", (ev) => {
        const cell = ev.target?.closest?.("td, th");
        const card = cell?.closest?.(".slide-el");
        if (!cell || !card) return;
        const here = { id: card.dataset.elementId, row: Number(cell.dataset.rowIdx), col: Number(cell.dataset.colIdx) };
        if (ev.shiftKey && active?.id === here.id) corner = here;
        else {
          active = here;
          corner = null;
        }
        paintRange({ id: here.id });
      });
    }

    return { bind, run, closeAll: () => { active = null; corner = null; }, activeCell: () => active };
  }

  host.__EASYOFFICE_SLIDE_TABLE_MATH__ = { edit, MAX_ROWS, MAX_COLS };
  host.__EASYOFFICE_CREATE_SLIDE_TABLE__ = createSlideTable;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
