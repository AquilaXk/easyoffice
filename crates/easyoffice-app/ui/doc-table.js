(function (host) {
  "use strict";

  // The tables of the document editor. `표 만들기` puts an empty grid after the block the caret is in, else at
  // the end of the document, so a blank document gets one too. The other table commands (rows and columns,
  // merge and split, block sum, cell formulas) act on the table the caret is in, one undo step each, and Tab
  // moves between cells. The save payload and the file reader use the same
  // `table.doc-inline-table > tbody > tr > td` shape; the structure edits are in `doc-table-grid.js`.

  const TABLE_CLASS = "doc-inline-table";
  const DEFAULT_ROWS = 3;
  const DEFAULT_COLUMNS = 3;

  function emptyBlock(document, tag) {
    const el = document.createElement(tag);
    el.appendChild(document.createElement("br"));
    return el;
  }

  function buildTable(document, rows, columns) {
    const table = document.createElement("table");
    table.className = TABLE_CLASS;
    const body = document.createElement("tbody");
    for (let r = 0; r < rows; r += 1) {
      const tr = document.createElement("tr");
      for (let c = 0; c < columns; c += 1) tr.appendChild(emptyBlock(document, "td"));
      body.appendChild(tr);
    }
    table.appendChild(body);
    return table;
  }

  // The block child of `root` that holds the range; none when there is no caret in the editor.
  function blockOf(root, range) {
    let node = range?.startContainer ?? null;
    if (!node || !root.contains(node)) return null;
    while (node.parentNode !== root) node = node.parentNode;
    return node;
  }

  // Puts the table after the caret's block (a paragraph follows it when it ends the document, so the text can
  // go on) and returns the first cell. `root` is `#doc-editor`.
  function insertTable(root, range, rows = DEFAULT_ROWS, columns = DEFAULT_COLUMNS) {
    const document = root.ownerDocument;
    const table = buildTable(document, rows, columns);
    root.insertBefore(table, blockOf(root, range)?.nextSibling ?? null);
    if (!table.nextSibling) root.appendChild(emptyBlock(document, "p"));
    return { changed: true, table, firstCell: table.querySelector("td") };
  }

  // Menu command -> label rhwp-studio shows for it (`command.table.*` of its ko locale), the status line says it was done.
  const COMMAND_LABELS = {
    "table-row-above": "위쪽에 줄 추가하기",
    "table-add-row": "아래쪽에 줄 추가하기",
    "table-col-left": "왼쪽에 칸 추가하기",
    "table-col-right": "오른쪽에 칸 추가하기",
    "table-row-delete": "줄 지우기",
    "table-col-delete": "칸 지우기",
    "table-split": "셀 나누기",
    "table-merge": "셀 합치기",
    "table-sum": "블록 합계",
  };
  // Items of the table menu that mean something only with the caret in a table.
  const TABLE_ONLY_COMMANDS = new Set([...Object.keys(COMMAND_LABELS), "doc-mod-tableformula", "hwp-mod-border"]);
  const MSG_DONE = " 완료";
  const MSG_OUTSIDE = "표 안에서 사용";
  const MSG_NO_FORMULA = "수식 없음";
  const FORMULA_RE = /^=/;

  const cellText = (cell) => String(cell.textContent || "").replaceAll(/\s+/g, " ").trim();

  // The text of each slot of the table with the cell's text at its first slot (a covered slot is empty).
  function textGrid(layout) {
    return layout.slots.map((row, r) =>
      Array.from(row, (cell, c) => {
        const pos = layout.at.get(cell);
        return pos && pos.row === r && pos.col === c ? cellText(cell) : "";
      }));
  }

  function createDocTable(deps) {
    const { document, env, getRange, withUndo, setFeedback } = deps;
    const grid = host.__EASYOFFICE_DOC_TABLE_GRID__;
    const editor = () => document.getElementById("doc-editor");

    // `{ table, from, to }`: the table the caret (or the selection) is in and the cells at the ends of the selection.
    function selectionAt() {
      const root = editor();
      const range = getRange();
      const table = root && range ? grid.tableAt(root, range.startContainer) : null;
      const from = table ? grid.cellIn(table, range.startContainer, range.startOffset) : null;
      if (!from || !root.contains(table)) return null;
      return { table, from, to: grid.cellIn(table, range.endContainer, range.endOffset) ?? from };
    }

    function placeCaret(node) {
      const sel = env.getSelection?.();
      const range = document.createRange?.();
      if (!node || !sel || !range) return;
      range.setStart(node, 0);
      range.setEnd(node, 0);
      sel.removeAllRanges();
      sel.addRange(range);
    }

    const rectOf = ({ table, from, to }) => grid.rectOf(grid.layoutOf(table), from, to);

    function calculate({ table }) {
      const layout = grid.layoutOf(table);
      const values = host.__EASYOFFICE_SAVE_PAYLOAD__.evaluateDocTableFormulas(textGrid(layout));
      let cell = null;
      layout.slots.forEach((row, r) => row.forEach((slot, c) => {
        const value = values[r]?.[c];
        if (value === null || value === undefined || !FORMULA_RE.test(cellText(slot))) return;
        slot.replaceChildren(document.createTextNode(value));
        cell ??= slot;
      }));
      return cell ? { changed: true, cell } : { changed: false, error: MSG_NO_FORMULA };
    }

    const OPERATIONS = {
      "table-row-above": (sel) => grid.insertRow(sel.table, sel.from, true),
      "table-add-row": (sel) => grid.insertRow(sel.table, sel.from, false),
      "table-col-left": (sel) => grid.insertColumn(sel.table, sel.from, true),
      "table-col-right": (sel) => grid.insertColumn(sel.table, sel.from, false),
      "table-row-delete": (sel) => grid.deleteRows(sel.table, rectOf(sel)),
      "table-col-delete": (sel) => grid.deleteColumns(sel.table, rectOf(sel), grid.layoutOf(sel.table).at.get(sel.from).row),
      "table-split": (sel) => grid.splitCell(sel.table, sel.from),
      "table-merge": (sel) => grid.mergeCells(sel.table, sel.from, sel.to),
      "table-sum": (sel) => grid.blockSum(sel.table, sel.from, sel.to),
    };

    // 표 셀 수식 계산 and 자동 합계 are ribbon commands (the command's own undo step covers them): the same edit, no step of its own.
    function applyAtCaret(operation) {
      const sel = selectionAt();
      const result = sel ? operation(sel) : { changed: false, error: MSG_OUTSIDE };
      if (result.changed) placeCaret(result.cell);
      else setFeedback?.(result.error);
      return result;
    }

    const recalculate = () => applyAtCaret(calculate);
    const autoSum = () => applyAtCaret(OPERATIONS["table-sum"]);

    const handles = (cmd) => Object.hasOwn(OPERATIONS, cmd);

    // Runs a table menu command at the caret as one undo step; the status line tells the result.
    function run(cmd) {
      const sel = selectionAt();
      if (!sel) {
        setFeedback?.(MSG_OUTSIDE);
        return { changed: false };
      }
      const result = withUndo(() => OPERATIONS[cmd](sel));
      if (!result.changed) {
        setFeedback?.(result.error ?? "");
        return result;
      }
      placeCaret(result.cell ?? result.target);
      setFeedback?.(COMMAND_LABELS[cmd] + MSG_DONE);
      return result;
    }

    // Tab goes to the next cell, Shift+Tab to the previous one; Tab in the last cell adds a row (document editor habit).
    function onKeydown(ev) {
      if (ev.key !== "Tab" || ev.ctrlKey || ev.metaKey || ev.altKey || ev.isComposing) return;
      const sel = selectionAt();
      if (!sel) return;
      ev.preventDefault?.();
      const cells = Array.from(grid.layoutOf(sel.table).at.keys());
      const next = cells.indexOf(sel.from) + (ev.shiftKey ? -1 : 1);
      if (next >= 0 && next < cells.length) placeCaret(cells[next]);
      else if (next >= cells.length) {
        const added = withUndo(() => grid.insertRow(sel.table, cells.at(-1), false));
        placeCaret(added.cell);
      }
    }

    function syncMenu() {
      const inTable = selectionAt() !== null;
      for (const item of document.querySelectorAll(".md-item[data-studio-menu-cmd]")) {
        if (TABLE_ONLY_COMMANDS.has(item.dataset.studioMenuCmd)) item.setAttribute("aria-disabled", inTable ? "false" : "true");
      }
    }

    function bind() {
      editor()?.addEventListener("keydown", onKeydown);
      document.addEventListener?.("selectionchange", syncMenu);
      const menu = document.querySelector('.menu-item[data-menu="table"]');
      for (const type of ["mouseenter", "focusin", "click"]) menu?.addEventListener(type, syncMenu);
      syncMenu();
    }

    return { bind, run, recalculate, autoSum, handles, syncMenu, selectionAt };
  }

  host.__EASYOFFICE_DOC_TABLE__ = { DEFAULT_ROWS, DEFAULT_COLUMNS, insertTable };
  host.__EASYOFFICE_CREATE_DOC_TABLE__ = createDocTable;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
