(function (host) {
  "use strict";

  // The tables of the document editor. `표 만들기` puts an empty grid after the block the caret is in, else at
  // the end of the document, so a blank document gets one too. The other table commands (rows and columns,
  // merge and split, block sum, cell formulas) act on the table the caret is in, one undo step each, and Tab
  // moves between cells (`doc-table-tab.js`). The save payload and the file reader use the same
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

  // Menu command -> label for HWP (`command.table.*` in ko locale) vs DOC/Pages (Word/Pages Korean terms).
  const HWP_COMMAND_LABELS = {
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
  const DOC_COMMAND_LABELS = {
    "table-row-above": "위쪽에 행 추가",
    "table-add-row": "아래쪽에 행 추가",
    "table-col-left": "왼쪽에 열 추가",
    "table-col-right": "오른쪽에 열 추가",
    "table-row-delete": "행 삭제",
    "table-col-delete": "열 삭제",
    "table-split": "셀 분할",
    "table-merge": "셀 병합",
    "table-sum": "블록 합계",
  };
  const PAGES_COMMAND_LABELS = {
    "table-row-above": "위에 행 추가",
    "table-add-row": "아래에 행 추가",
    "table-col-left": "앞에 열 추가",
    "table-col-right": "뒤에 열 추가",
    "table-row-delete": "행 삭제",
    "table-col-delete": "열 삭제",
    "table-split": "셀 병합 해제",
    "table-merge": "셀 병합",
    "table-sum": "블록 합계",
  };
  // Items of the table menu that mean something only with the caret in a table.
  const TABLE_ONLY_COMMANDS = new Set([
    ...Object.keys(DOC_COMMAND_LABELS),
    "table-border-bg",
    "table-cell-props",
    "table-col-equal",
    "doc-mod-tableformula",
    "hwp-mod-border",
  ]);
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

    function placeCaretAt(node, offset) {
      const sel = env.getSelection?.();
      const range = document.createRange?.();
      if (!node || !sel || !range) return;
      range.setStart(node, offset);
      range.setEnd(node, offset);
      sel.removeAllRanges();
      sel.addRange(range);
    }
    const placeCaret = (node) => placeCaretAt(node, 0);

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

    function sortTableAt(asc = true, options = {}) {
      const sel = selectionAt();
      const table = sel?.table || null;
      if (!table) {
        setFeedback?.(MSG_OUTSIDE);
        return { changed: false, error: MSG_OUTSIDE };
      }
      const colIdx = grid.layoutOf(table).at.get(sel.from)?.col ?? 0;
      const tbody = table.tBodies?.[0] || table.querySelector("tbody") || table;
      const allRows = Array.from(tbody.children || []).filter((r) => r.tagName === "TR");
      const rowCells = (r) => Array.from(r.children || []).filter((c) => c.tagName === "TD" || c.tagName === "TH");
      let start = 0;
      if (!table.tHead && tbody.tagName !== "THEAD") {
        while (start < allRows.length) {
          const cells = rowCells(allRows[start]);
          if (cells.length > 0 && cells.every((c) => c.tagName === "TH")) start += 1;
          else break;
        }
      }
      const sumRow = allRows.find((r) => r.id === "doc-table-sum-row") || null;
      const dataRows = allRows.slice(start).filter((r) => r !== sumRow);
      if (dataRows.length < 2) {
        const error = "정렬할 표 행이 부족합니다";
        setFeedback?.(error);
        return { changed: false, error };
      }
      if (dataRows.some((r) => rowCells(r).some((c) => (c.rowSpan || 1) > 1 || (c.colSpan || 1) > 1))) {
        const error = "병합된 셀이 있는 표는 정렬할 수 없습니다";
        setFeedback?.(error);
        return { changed: false, error };
      }
      const parseNum = (s) => (/^[-+]?\d[\d,]*(?:\.\d+)?$/.test(s) ? Number(s.replaceAll(",", "")) : NaN);
      const exec = () => {
        const sorted = [...dataRows].sort((a, b) => {
          const ca = rowCells(a);
          const cb = rowCells(b);
          const sa = cellText(ca[colIdx] || ca[0] || a);
          const sb = cellText(cb[colIdx] || cb[0] || b);
          const na = parseNum(sa);
          const nb = parseNum(sb);
          if (Number.isFinite(na) && Number.isFinite(nb)) return asc ? na - nb : nb - na;
          return asc ? sa.localeCompare(sb, "ko") : sb.localeCompare(sa, "ko");
        });
        for (const row of sorted) {
          if (sumRow) sumRow.before(row);
          else tbody.appendChild(row);
        }
        const firstCells = sorted[0] ? rowCells(sorted[0]) : [];
        const firstTarget = firstCells[colIdx] || firstCells[0];
        if (firstTarget) placeCaret(firstTarget);
        return { changed: true };
      };
      const res = options.undo === false ? exec() : withUndo(exec);
      if (res?.changed) setFeedback?.(asc ? "표 오름차순 정렬 완료" : "표 내림차순 정렬 완료");
      return res;
    }

    const recalculate = () => applyAtCaret(calculate);
    const autoSum = () => applyAtCaret(OPERATIONS["table-sum"]);
    const docImage = host.__EASYOFFICE_CREATE_DOC_IMAGE__?.(deps) ?? null;
    const docNotes = host.__EASYOFFICE_CREATE_DOC_NOTES_UI__?.(deps) ?? null;
    const tableStyle = host.__EASYOFFICE_CREATE_DOC_TABLE_STYLE__?.({ ...deps, grid, selectionAt, rectOf, placeCaret }) ?? null;

    const handles = (cmd) => Object.hasOwn(OPERATIONS, cmd) || Boolean(docImage?.handles(cmd)) || Boolean(docNotes?.handles(cmd)) || Boolean(tableStyle?.handles(cmd));

    // Runs a table menu command (or document image / notes / table style command) at the caret.
    function run(cmd) {
      if (docImage?.handles(cmd)) return docImage.run(cmd);
      if (docNotes?.handles(cmd)) return docNotes.run(cmd);
      if (tableStyle?.handles(cmd)) return tableStyle.run(cmd);
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
      const mod = document.getElementById("app")?.dataset?.module;
      const isPages = mod !== "HWP" && document.getElementById("doc-editor")?.dataset?.eoDocFlavor === "pages";
      const labels = mod === "HWP" ? HWP_COMMAND_LABELS : isPages ? PAGES_COMMAND_LABELS : DOC_COMMAND_LABELS;
      setFeedback?.(labels[cmd] + MSG_DONE);
      return result;
    }

    const tableTab = host.__EASYOFFICE_CREATE_DOC_TABLE_TAB__({ grid, withUndo, placeCaretAt });

    // Tab and Shift+Tab move between cells, and Tab in the last cell adds a row (`doc-table-tab.js`).
    function onKeydown(ev) {
      if (ev.key !== "Tab" || ev.ctrlKey || ev.metaKey || ev.altKey || ev.isComposing) return;
      const sel = selectionAt();
      if (!sel) return;
      ev.preventDefault?.();
      tableTab.handleTab(sel, ev.shiftKey);
    }

    function syncMenu() {
      const inTable = selectionAt() !== null;
      for (const item of document.querySelectorAll(".md-item[data-studio-menu-cmd]")) {
        if (TABLE_ONLY_COMMANDS.has(item.dataset.studioMenuCmd)) item.setAttribute("aria-disabled", inTable ? "false" : "true");
      }
      docImage?.syncResizer?.();
      docNotes?.syncAll?.();
    }

    function bind() {
      editor()?.addEventListener("keydown", onKeydown);
      document.addEventListener?.("selectionchange", syncMenu);
      for (const menu of document.querySelectorAll('.menu-item[data-menu="table"], .menu-item[data-menu="arrange"]')) {
        for (const type of ["mouseenter", "focusin", "click"]) menu.addEventListener(type, syncMenu);
      }
      docImage?.bind();
      docNotes?.bind();
      tableStyle?.bind();
      syncMenu();
    }

    return { bind, run, recalculate, autoSum, sortTableAt, handles, syncMenu, selectionAt };
  }

  host.__EASYOFFICE_DOC_TABLE__ = { DEFAULT_ROWS, DEFAULT_COLUMNS, insertTable };
  host.__EASYOFFICE_CREATE_DOC_TABLE__ = createDocTable;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
