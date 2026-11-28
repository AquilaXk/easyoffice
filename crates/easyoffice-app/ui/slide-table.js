(function (host) {
  "use strict";

  // 표 편집 of the slide editor (PD-398): rows and columns are added above, below, left and right of the cell that was
  // last clicked, and the row or column of that cell is deleted (the layout commands of the Korean presentation program's
  // 표 도구). The table is the element's `rows` (text per cell); the limits are the file engine's (64 rows, 32 columns) and a
  // command that would pass them says so and changes nothing. The word processor's `buildTable` (`doc-table.js`) builds
  // DOM tables of the document editor, which a slide table is not; the row and column arithmetic here is the whole feature.
  const MAX_ROWS = 64;
  const MAX_COLS = 32;
  const MSG_NO_TABLE = "표를 먼저 선택하세요";

  // The rows after the command; `null` when the command does not apply (past a limit, or the last row or column).
  function edit(rows, at, command) {
    const cols = rows[0]?.length ?? 0;
    const blankRow = () => Array.from({ length: cols }, () => "");
    const copy = rows.map((r) => [...r]);
    switch (command) {
      case "row-above": case "row-below": {
        if (rows.length >= MAX_ROWS) return null;
        copy.splice(at.row + (command === "row-below" ? 1 : 0), 0, blankRow());
        return copy;
      }
      case "col-left": case "col-right": {
        if (cols >= MAX_COLS) return null;
        const to = at.col + (command === "col-right" ? 1 : 0);
        copy.forEach((r) => r.splice(to, 0, ""));
        return copy;
      }
      case "row-delete":
        if (rows.length <= 1) return null;
        copy.splice(at.row, 1);
        return copy;
      case "col-delete":
        if (cols <= 1) return null;
        copy.forEach((r) => r.splice(at.col, 1));
        return copy;
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
    // The cell last clicked: the table's id and the position.
    let active = null;

    function selectedTable() {
      const el = canvas.getSelectedElement();
      return el?.kind === "table" ? el : null;
    }

    function run(act) {
      if (!Object.hasOwn(COMMANDS, act)) return false;
      const el = selectedTable();
      if (!el) {
        say(MSG_NO_TABLE);
        return true;
      }
      const [command, done] = COMMANDS[act];
      const cell = active?.id === el.id ? active : { row: el.rows.length - 1, col: (el.rows[0]?.length ?? 1) - 1 };
      const next = edit(el.rows, cell, command);
      if (!next) {
        say(command.endsWith("delete") ? "마지막 행이나 열은 삭제할 수 없습니다" : "표가 가득 찼습니다 (행 " + MAX_ROWS + "개, 열 " + MAX_COLS + "개까지)");
        return true;
      }
      pushUndoSnapshot(false);
      canvas.updateElement(el.id, { rows: next }, { recordUndo: false });
      active = null;
      say(done);
      return true;
    }

    function bind() {
      document.getElementById("slide-canvas-layer")?.addEventListener("click", (ev) => {
        const cell = ev.target?.closest?.("td, th");
        const card = cell?.closest?.(".slide-el");
        if (!cell || !card) return;
        active = { id: card.dataset.elementId, row: Number(cell.dataset.rowIdx), col: Number(cell.dataset.colIdx) };
      });
    }

    return { bind, run, closeAll: () => { active = null; }, activeCell: () => active };
  }

  host.__EASYOFFICE_SLIDE_TABLE_MATH__ = { edit, MAX_ROWS, MAX_COLS };
  host.__EASYOFFICE_CREATE_SLIDE_TABLE__ = createSlideTable;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
