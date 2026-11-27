(function (host) {
  "use strict";

  // The totals row of a table (PD-232): the function list of a totals-row cell (없음, 평균, 개수, 숫자 개수, 최대, 최소, 합계,
  // 표준 편차, 분산, 추가 함수) and the `SUBTOTAL` formula each one writes. The function of a cell is read from
  // the cell itself (`functionOf`), so a cell edited by hand shows what it holds now. The file's `totalsRowFunction` is the
  // same word list (ECMA-376 18.18.80); the writer derives it from the cell too (`xlsx_tables.rs`).
  //
  // No reference implementation holds a totals-row function list: the reference tables have no totals row or read no
  // `totalsRowFunction` (`THIRD_PARTY_NOTICES.md`), so the list and its `SUBTOTAL` numbers follow the spreadsheet program's support
  // page on totaling table data and ECMA-376 18.18.80.
  //
  // The menu is the page's `.context-menu` with `.md-item` rows (the sheet tab menu's markup); it opens from the arrow on the
  // selected totals-row cell (`td.totals-cell.selected-cell::after`) and with Alt+Down.

  // [id (`totalsRowFunction`), label, `SUBTOTAL` code]: the 101-111 codes skip the rows a filter hides.
  const FUNCTIONS = [
    ["none", "없음", 0],
    ["average", "평균", 101],
    ["count", "개수", 103],
    ["countNums", "숫자 개수", 102],
    ["max", "최대", 104],
    ["min", "최소", 105],
    ["sum", "합계", 109],
    ["stdDev", "표준 편차", 107],
    ["var", "분산", 110],
  ];
  const MORE_LABEL = "추가 함수…";
  const CODE_OF = new Map(FUNCTIONS.filter((f) => f[2]).map((f) => [f[0], f[2]]));
  const ID_OF = new Map(FUNCTIONS.filter((f) => f[2]).map((f) => [f[2], f[0]]));
  const HIT_PX = 18; // the arrow at the right edge of the selected cell
  const MENU_MARGIN = 2;
  const MAX_CELL_TEXT = 8192; // characters of a formula the cell takes (the spreadsheet program's limit)

  const escapeColumn = (text) => text.replace(/(['\[\]#])/g, "'$1");
  const isSpecial = (name) => /[^\p{L}\p{N}_ ]/u.test(name);

  // `표1[열]`, or `표1[[열 이름]]` when the name has a character that needs the double brackets.
  function columnRef(tableName, column) {
    const inner = escapeColumn(column);
    return tableName + "[" + (isSpecial(column) ? "[" + inner + "]" : inner) + "]";
  }

  // The formula of function `id` over the data of column `index` (0-based) of `table`, or "" for `none` and `custom`.
  function formulaFor(table, index, id) {
    const code = CODE_OF.get(id);
    return code ? "=SUBTOTAL(" + code + "," + columnRef(table.name, table.columns[index]) + ")" : "";
  }

  // The function the totals-row cell `entry` (`{ value, formula }`) of column `index` stands for: a function id, `custom` for
  // another formula and `none` for a cell that holds no formula.
  function functionOf(table, index, entry) {
    const formula = String(entry?.formula ?? "");
    if (!formula.startsWith("=")) return "none";
    const found = /^=\s*SUBTOTAL\(\s*(\d+)\s*,\s*(.+?)\s*\)\s*$/i.exec(formula);
    const id = found && ID_OF.get(Number(found[1]));
    if (!id) return "custom";
    const inner = escapeColumn(table.columns[index]);
    const own = columnRef(table.name, table.columns[index]).toLowerCase();
    const bare = own.slice(table.name.length);
    const ref = found[2].toLowerCase();
    // `[열]` without the table's name is the same column (the file's own form).
    return ref === own || ref === bare || ref === (table.name + "[[#data],[" + inner + "]]").toLowerCase() ? id : "custom";
  }

  // The label and the function of the last column of a new totals row: a sum when the data of the last column are numbers, else
  // a count of its cells. `cells` are the data cells' texts of that column.
  const NUMERIC = /^\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*$/;
  function defaultFunction(texts) {
    const filled = texts.filter((t) => t !== undefined && t !== "");
    return filled.length > 0 && filled.every((t) => NUMERIC.test(String(t))) ? "sum" : "count";
  }

  host.__EASYOFFICE_SHEET_TABLE_TOTALS__ = { FUNCTIONS, MORE_LABEL, MAX_CELL_TEXT, escapeColumn, columnRef, formulaFor, functionOf, defaultFunction };

  // ---- the menu ---------------------------------------------------------------------------------------------------------------
  // `deps.pick(table, index, id)` runs a choice; `id` is a function id or `more`. `deps.cellAt(row, col)` reads a cell.
  function createSheetTableTotalsMenu({ document, getSheetName, getSelection, tableAt, boxOf, cellAt, canEdit, pick }) {
    const menu = () => document.getElementById("sheet-totals-menu");
    let target = null;

    const items = () => [...menu().querySelectorAll(".md-item")].filter((el) => el.getAttribute("aria-disabled") !== "true");

    function build() {
      const el = menu();
      if (el.children.length > 0) return;
      const add = (id, label, role) => {
        const row = document.createElement("div");
        row.className = "md-item";
        row.setAttribute("role", role);
        row.tabIndex = -1;
        row.dataset.totalsFn = id;
        const text = document.createElement("span");
        text.className = "md-label";
        text.textContent = label;
        row.appendChild(text);
        el.appendChild(row);
      };
      for (const [id, label] of FUNCTIONS) add(id, label, "menuitemradio");
      const sep = document.createElement("div");
      sep.className = "md-sep";
      el.appendChild(sep);
      add("more", MORE_LABEL, "menuitem");
    }

    function close(refocus) {
      const el = menu();
      target = null;
      if (!el || el.hidden) return;
      el.hidden = true;
      if (refocus) document.querySelector("#pane-sheet td.selected-cell")?.focus?.();
    }

    // The totals-row cell `row`, `col` of the active sheet: its table and the column index; null for any other cell.
    function totalsCell(row, col) {
      const table = tableAt(getSheetName(), row, col);
      if (!table || table.totals !== true) return null;
      const b = boxOf(table);
      return b.r2 === row ? { table, index: col - b.c1, row, col } : null;
    }

    function open(row, col, x, y) {
      const hit = totalsCell(row, col);
      const el = menu();
      if (!hit || !el || !canEdit()) return;
      build();
      close(false);
      target = hit;
      const now = functionOf(hit.table, hit.index, cellAt(row, col));
      for (const item of el.querySelectorAll("[data-totals-fn]")) {
        const id = item.dataset.totalsFn;
        if (id === "more") continue;
        item.setAttribute("aria-checked", String(id === now));
      }
      el.hidden = false;
      el.style.left = "0px";
      el.style.top = "0px";
      const vw = host.innerWidth || document.documentElement.clientWidth || 0;
      const vh = host.innerHeight || document.documentElement.clientHeight || 0;
      const w = el.offsetWidth || 0;
      const h = el.offsetHeight || 0;
      // The menu opens below the cell; a window edge that would cut it moves it up.
      el.style.left = Math.max(0, vw > 0 ? Math.min(x, vw - w - MENU_MARGIN) : x) + "px";
      el.style.top = Math.max(0, vh > 0 ? Math.min(y, vh - h - MENU_MARGIN) : y) + "px";
      (items().find((i) => i.getAttribute("aria-checked") === "true") || items()[0])?.focus?.();
    }

    function choose(id) {
      const hit = target;
      close(true);
      if (hit) void pick(hit.table, hit.index, id);
    }

    function onMouseDown(ev) {
      const td = ev.target?.closest?.("td.totals-cell.selected-cell");
      if (!td || (ev.button ?? 0) !== 0) return;
      const box = td.getBoundingClientRect();
      if (ev.clientX < box.right - HIT_PX) return;
      ev.preventDefault();
      ev.stopPropagation();
      open(td._row, td._col, box.left, box.bottom);
    }

    // Alt+Down on the active totals-row cell opens the menu, as the arrow does.
    function onKeydown(ev) {
      if (!ev.altKey || ev.key !== "ArrowDown") return;
      const sel = getSelection();
      if (!sel || sel.axis || !totalsCell(sel.ar, sel.ac)) return;
      const td = document.querySelector('#pane-sheet td.selected-cell');
      const box = td?.getBoundingClientRect?.() || { left: 0, bottom: 0 };
      ev.preventDefault();
      open(sel.ar, sel.ac, box.left, box.bottom);
    }

    function onMenuKeydown(ev) {
      const list = items();
      const at = list.indexOf(document.activeElement);
      if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
        ev.preventDefault();
        list[(at + (ev.key === "ArrowDown" ? 1 : -1) + list.length) % list.length]?.focus();
      } else if (ev.key === "Home" || ev.key === "End") {
        ev.preventDefault();
        list[ev.key === "Home" ? 0 : list.length - 1]?.focus();
      } else if (ev.key === "Escape" || ev.key === "Tab") {
        ev.preventDefault();
        close(true);
      } else if (ev.key === "Enter" || ev.key === " ") {
        ev.preventDefault();
        const id = document.activeElement?.dataset?.totalsFn;
        if (id) choose(id);
      }
    }

    function bind() {
      const el = menu();
      const viewport = document.getElementById("sheet-viewport");
      viewport?.addEventListener("mousedown", onMouseDown, true);
      viewport?.addEventListener("keydown", onKeydown);
      viewport?.addEventListener("scroll", () => close(false));
      el?.addEventListener("keydown", onMenuKeydown);
      el?.addEventListener("click", (ev) => {
        const item = ev.target?.closest?.("[data-totals-fn]");
        if (item) choose(item.dataset.totalsFn);
      });
      el?.addEventListener("contextmenu", (ev) => ev.preventDefault());
      document.addEventListener("mousedown", (ev) => {
        if (el && !el.hidden && !el.contains(ev.target)) close(false);
      });
      host.addEventListener?.("resize", () => close(false));
      host.addEventListener?.("blur", () => close(false));
    }

    return { bind, open, close, isOpen: () => Boolean(target) };
  }

  host.__EASYOFFICE_CREATE_SHEET_TABLE_TOTALS_MENU__ = createSheetTableTotalsMenu;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
