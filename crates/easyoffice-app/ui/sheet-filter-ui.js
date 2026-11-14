(function (host) {
  "use strict";

  // The auto filter on screen (PD-198): the drop-down button of a header cell of the filter range,
  // the panel it opens (sort, custom condition, clear, a searchable checkbox list) and the custom
  // condition dialog. The panel is a `.context-menu` (rhwp-studio `dialogs.css` 516-525) whose rows
  // are `.md-item` rows; the dialog is a `tool-dialog`. The list is virtual: only the rows in view
  // exist, however many values the column has. Labels only, no explanatory sentences.
  //
  // PD-226: a column's dates are a tree (year > month > day, folded) at the top of the list, as the spreadsheet
  // program's list shows them: the same `.md-item` rows with the menu's own chevron to fold, and a checked node is a
  // date group of the column's condition. `색 기준 필터` is a `.md-sub` of the panel (the menu bar's submenu) holding
  // `.sb-hl-swatch` swatches (the style bar's highlight palette); it lists the colours the conditional formats give the
  // column and is off when the column has fewer than two kinds.

  const ROW_H = 22; // the height of a field in a dialog (`.dialog-input`)
  const LIST_ROWS = 10;
  const BUFFER_ROWS = 3;
  const HIT_PX = 22; // the corner of a header cell that opens the panel (its button is 18px)
  const MENU_MARGIN = 2; // rhwp-studio's context menu keeps 2px from the window edge
  const SEARCH_DELAY_MS = 120;
  const TREE_INDENT = 16; // px a tree level moves a row right
  const SWATCHES_PER_ROW = 8;
  const MAX_SWATCHES = 64; // colours one list shows

  function createSheetFilterUi(deps) {
    const { document, filter, kit, getSelection, focusAnchor, getActiveSheetName } = deps;
    const core = host.__EASYOFFICE_SHEET_FILTER_CORE__;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const { $, setVal, setSelect, openDialog, closeDialog } = kit;
    const panel = () => $("sheet-filter-panel");
    let state = null;
    let timer = 0;
    let rowEls = [];

    const filterNow = () => layoutApi.filterOf(getActiveSheetName());
    const columnOf = (filterModel, col) => filterModel?.columns.find((c) => c.col === col) || null;

    // ---- the drop-down button on the header cells ------------------------------------------------
    // The cell paints the button itself (`td.filter-head::after`); a press on that corner opens
    // the panel instead of selecting the cell.
    function onMouseDown(ev) {
      const td = ev.target?.closest?.("td.filter-head");
      if (!td || (ev.button ?? 0) !== 0) return;
      const box = td.getBoundingClientRect();
      if (ev.clientX < box.right - HIT_PX || ev.clientY < box.bottom - HIT_PX) return;
      ev.preventDefault();
      ev.stopPropagation();
      void open(td._col, box.left, box.bottom);
    }

    // Alt+Down on a header cell of the range opens the panel, as the button does.
    function onKeydown(ev) {
      if (!ev.altKey || ev.key !== "ArrowDown") return;
      const f = filterNow();
      const sel = getSelection();
      if (!f || sel.axis || sel.ar !== f.range.r1 || sel.ac < f.range.c1 || sel.ac > f.range.c2) return;
      const td = document.querySelector('#pane-sheet td[data-cell="' + host.__EASYOFFICE_SHEET_GRID_UTILS__.refOf(sel.ar, sel.ac) + '"]');
      const box = td?.getBoundingClientRect?.() || { left: 0, bottom: 0 };
      ev.preventDefault();
      void open(sel.ac, box.left, box.bottom);
    }

    // ---- the panel -----------------------------------------------------------------------------------
    // The rows of the list, top to bottom: "모두 선택", the date tree (folded nodes hide their children), the other
    // texts, the empty cells. `state.flat` holds them; a node of the tree is `{ checked }` on a day and is judged by
    // its days above.
    const leavesOf = (node) => (node.months ? node.months.flatMap((m) => m.days) : node.days || [node]);
    const treeDays = (s) => s.tree.flatMap(leavesOf);
    const checkedOf = (s) => s.items.reduce((n, item) => n + (item.checked ? 1 : 0), 0) + (s.blank?.checked ? 1 : 0) + treeDays(s).reduce((n, d) => n + (d.checked ? 1 : 0), 0);
    const totalOf = (s) => s.items.length + (s.blank ? 1 : 0) + treeDays(s).length;
    const rowCount = (s) => s.flat.length;
    const readOnly = (s) => s.more && s.query === "";
    const nodeState = (node) => {
      const days = leavesOf(node);
      const on = days.reduce((n, d) => n + (d.checked ? 1 : 0), 0);
      return on === 0 ? "false" : on === days.length ? "true" : "mixed";
    };

    function rebuildFlat() {
      const flat = [{ kind: "all" }];
      for (const y of state.tree) {
        flat.push({ kind: "year", node: y, level: 0 });
        if (!y.open) continue;
        for (const m of y.months) {
          flat.push({ kind: "month", node: m, level: 1 });
          if (!m.open) continue;
          for (const d of m.days) flat.push({ kind: "day", node: d, level: 2 });
        }
      }
      for (const item of state.items) flat.push({ kind: "item", item });
      if (state.blank) flat.push({ kind: "blank", item: state.blank });
      state.flat = flat;
    }

    const rowData = (i) => state.flat[i];

    function allState() {
      const checked = checkedOf(state);
      const total = totalOf(state);
      if (checked === 0) return "false";
      return checked === total ? "true" : "mixed";
    }

    function makeRow() {
      const row = document.createElement("div");
      row.className = "md-item sheet-filter-row";
      row.setAttribute("role", "option");
      const arrow = document.createElement("span");
      arrow.className = "md-arrow md-caret sheet-filter-arrow";
      arrow.hidden = true;
      const box = document.createElement("input");
      box.type = "checkbox";
      box.tabIndex = -1;
      box.setAttribute("aria-hidden", "true");
      const label = document.createElement("span");
      label.className = "md-label";
      const count = document.createElement("span");
      count.className = "md-hint";
      row.appendChild(arrow);
      row.appendChild(box);
      row.appendChild(label);
      row.appendChild(count);
      row._arrow = arrow;
      row._box = box;
      row._label = label;
      row._count = count;
      return row;
    }

    const NODE_LABEL = {
      year: (n) => String(n.year),
      month: (n) => n.month + "월",
      day: (n) => String(n.day),
    };

    // The chevron of a year or a month: down while it is open, right while it is folded (the menu's own icons).
    function paintArrow(row, data) {
      const arrow = row._arrow;
      arrow.hidden = data.kind !== "year" && data.kind !== "month";
      if (arrow.hidden) return;
      const name = data.node.open ? "chevron-down" : "chevron-right";
      if (arrow.dataset.icon !== name) {
        arrow.dataset.icon = name;
        host.__EASYOFFICE_ICONS__?.setIcon?.(document, arrow, name);
      }
    }

    function paintRow(row, i) {
      const data = rowData(i);
      row.dataset.index = String(i);
      row.id = "sheet-filter-row-" + i;
      row.style.top = i * ROW_H + "px";
      row.classList.toggle("active", state.cursor === i);
      row.style.paddingLeft = data.level ? 8 + data.level * TREE_INDENT + "px" : "";
      row.dataset.kind = data.kind;
      paintArrow(row, data);
      const locked = readOnly(state);
      if (data.kind === "all") {
        const mixed = allState();
        row._label.textContent = "(모두 선택)";
        row._count.textContent = "";
        row._box.checked = mixed === "true";
        row._box.indeterminate = mixed === "mixed";
        row.setAttribute("aria-checked", mixed);
        row.removeAttribute("aria-expanded");
      } else if (data.level !== undefined) {
        const node = data.node;
        const mixed = nodeState(node);
        row._label.textContent = NODE_LABEL[data.kind](node);
        row._count.textContent = String(node.count);
        row._box.checked = mixed === "true";
        row._box.indeterminate = mixed === "mixed";
        row.setAttribute("aria-checked", mixed);
        row.setAttribute("aria-level", String(data.level + 1));
        if (data.kind === "day") row.removeAttribute("aria-expanded");
        else row.setAttribute("aria-expanded", node.open ? "true" : "false");
      } else {
        row._label.textContent = data.kind === "blank" ? "(빈 칸)" : data.item.text;
        row._count.textContent = String(data.item.count);
        row._box.checked = data.item.checked;
        row._box.indeterminate = false;
        row.setAttribute("aria-checked", data.item.checked ? "true" : "false");
        row.removeAttribute("aria-expanded");
        row.removeAttribute("aria-level");
      }
      if (locked) row.setAttribute("aria-disabled", "true");
      else row.removeAttribute("aria-disabled");
    }

    function renderRows() {
      const list = $("sheet-filter-list");
      const rows = $("sheet-filter-rows");
      if (!state || !list || !rows) return;
      const n = rowCount(state);
      rows.style.height = n * ROW_H + "px";
      list.style.height = Math.min(n, LIST_ROWS) * ROW_H + 2 + "px";
      const first = Math.max(0, Math.floor((list.scrollTop || 0) / ROW_H) - BUFFER_ROWS);
      const last = Math.min(n - 1, first + LIST_ROWS + BUFFER_ROWS * 2);
      const need = Math.max(0, last - first + 1);
      while (rowEls.length < need) {
        const row = makeRow();
        rowEls.push(row);
        rows.appendChild(row);
      }
      rowEls.forEach((row, k) => {
        if (k >= need) {
          row.hidden = true;
          return;
        }
        row.hidden = false;
        paintRow(row, first + k);
      });
      const checked = checkedOf(state);
      const count = $("sheet-filter-count");
      if (count) count.textContent = checked + " / " + totalOf(state) + (state.more ? "+" : "");
      const ok = $("btn-sheet-filter-ok");
      if (ok) {
        const disabled = readOnly(state) || checked === 0;
        ok.disabled = disabled;
        ok.setAttribute("aria-disabled", disabled ? "true" : "false");
      }
      list.setAttribute("aria-activedescendant", state.cursor >= 0 ? "sheet-filter-row-" + state.cursor : "");
    }

    function ensureVisible(i) {
      const list = $("sheet-filter-list");
      if (!list) return;
      const top = i * ROW_H;
      if (top < list.scrollTop) list.scrollTop = top;
      else if (top + ROW_H > list.scrollTop + LIST_ROWS * ROW_H) list.scrollTop = top + ROW_H - LIST_ROWS * ROW_H;
    }

    function setAll(checked) {
      for (const item of state.items) item.checked = checked;
      if (state.blank) state.blank.checked = checked;
      for (const day of treeDays(state)) day.checked = checked;
    }

    function toggleRow(i) {
      if (!state || readOnly(state)) return;
      const data = rowData(i);
      if (data.kind === "all") setAll(allState() !== "true");
      else if (data.level !== undefined) {
        const on = nodeState(data.node) !== "true";
        for (const day of leavesOf(data.node)) day.checked = on;
      } else data.item.checked = !data.item.checked;
      state.cursor = i;
      renderRows();
    }

    // Folds or unfolds a year or a month (the cursor stays on it).
    function foldRow(i, open) {
      const data = state && rowData(i);
      if (!data || (data.kind !== "year" && data.kind !== "month")) return;
      const was = Boolean(data.node.open);
      data.node.open = open === undefined ? !was : open;
      if (data.node.open === was) return;
      rebuildFlat();
      state.cursor = i;
      renderRows();
    }

    const covers = (groups, y, m, d) =>
      groups.some(
        (g) => g.year === y && (g.unit === "year" || (g.month === m && (g.unit === "month" || (g.unit === "day" && g.day === d)))),
      );

    // The tree of the engine's `dates` answer, its days checked as the column's date groups say.
    function buildTree(dates, allChecked, groups) {
      return (dates?.years || []).map(([year, count, months]) => ({
        year,
        count,
        open: false,
        months: months.map(([month, mcount, days]) => ({
          month,
          count: mcount,
          open: false,
          days: days.map(([day, n]) => ({ day, count: n, checked: allChecked || covers(groups, year, month, day) })),
        })),
      }));
    }

    // The date groups of the checked part of the tree: a whole year, else a whole month, else single days.
    function datesFromTree(tree) {
      const out = [];
      for (const y of tree) {
        if (nodeState(y) === "true") {
          out.push({ unit: "year", year: y.year });
          continue;
        }
        for (const m of y.months) {
          if (nodeState(m) === "true") out.push({ unit: "month", year: y.year, month: m.month });
          else for (const d of m.days) if (d.checked) out.push({ unit: "day", year: y.year, month: m.month, day: d.day });
        }
      }
      return out;
    }

    // The panel's list for the column: the values the engine offers, checked as the column's
    // condition says (everything when it has none or a custom one).
    async function load(query) {
      const token = ++state.token;
      const answer = await filter.values(state.col, query);
      if (!state || token !== state.token) return;
      if (!answer) {
        filter.say("값 목록을 가져오지 못함");
        close(true);
        return;
      }
      const criteria = columnOf(filterNow(), state.col);
      // A list condition (values, dates or both) says what is checked; any other condition shows everything checked.
      const listed = Boolean(criteria && (criteria.values || criteria.dates));
      const picked = listed ? new Set(criteria.values || []) : null;
      const searching = query !== "";
      state.query = query;
      state.more = answer.more === true || answer.dates?.more === true;
      state.items = answer.items.map(([text, count]) => ({
        text,
        count,
        checked: searching || !picked || picked.has(text),
      }));
      state.blank = answer.blank > 0 ? { count: answer.blank, checked: !picked || criteria.blank === true } : null;
      state.tree = buildTree(answer.dates, !listed, criteria?.dates || []);
      state.cursor = -1;
      rebuildFlat();
      renderColors(answer.colors, criteria);
      const list = $("sheet-filter-list");
      if (list) list.scrollTop = 0;
      renderRows();
    }

    function place(el, x, y) {
      el.style.maxHeight = "";
      el.style.overflowY = "";
      el.style.left = "0px";
      el.style.top = "0px";
      const w = el.offsetWidth || 0;
      const h = el.offsetHeight || 0;
      const vw = host.innerWidth || document.documentElement.clientWidth || 0;
      const vh = host.innerHeight || document.documentElement.clientHeight || 0;
      const tall = vh > 0 && h > vh - MENU_MARGIN;
      if (tall) el.style.maxHeight = vh - MENU_MARGIN + "px";
      // The panel lets its submenu out of its box; only a panel taller than the window scrolls.
      el.style.overflowY = tall ? "auto" : "";
      const height = vh > 0 ? Math.min(h, vh - MENU_MARGIN) : h;
      el.style.left = Math.max(0, vw > 0 ? Math.min(x, vw - w - MENU_MARGIN) : x) + "px";
      el.style.top = Math.max(0, vh > 0 ? Math.min(y, vh - height - MENU_MARGIN) : y) + "px";
    }

    async function open(col, x, y) {
      const f = filterNow();
      const el = panel();
      if (!f || !el) return;
      close(false);
      state = { col, items: [], blank: null, tree: [], flat: [{ kind: "all" }], more: false, query: "", cursor: -1, token: 0 };
      renderColors(null, null);
      const has = Boolean(columnOf(f, col));
      const clear = el.querySelector('[data-filter-act="clear-column"]');
      if (clear) {
        if (has) clear.removeAttribute("aria-disabled");
        else clear.setAttribute("aria-disabled", "true");
      }
      const search = $("sheet-filter-search");
      if (search) search.value = "";
      el.hidden = false;
      place(el, x, y);
      renderRows();
      search?.focus();
      await load("");
      place(el, x, y);
    }

    function close(refocus) {
      const el = panel();
      if (state) state.token += 1;
      state = null;
      host.clearTimeout?.(timer);
      if (!el || el.hidden) return;
      el.hidden = true;
      if (refocus) focusAnchor();
    }

    function confirm() {
      if (!state || readOnly(state)) return;
      const picked = state.items.filter((i) => i.checked).map((i) => i.text);
      const blank = Boolean(state.blank?.checked);
      const dates = datesFromTree(state.tree);
      if (picked.length === 0 && !blank && dates.length === 0) return;
      const treeAll = state.tree.every((y) => nodeState(y) === "true");
      const everything = state.query === "" && !state.more && picked.length === state.items.length && (!state.blank || blank) && treeAll;
      const col = state.col;
      close(true);
      const criteria = everything ? null : { ...(blank ? { blank: true } : {}), values: picked, ...(dates.length ? { dates } : {}) };
      void filter.setCriteria(col, criteria);
    }

    // The colours the conditional formats give the column, for the `색 기준 필터` submenu: the fill colours and the
    // text colours, each with its "none" row, a list shown only when the column has two kinds or more. The submenu is
    // off when neither is.
    function renderColors(colors, criteria) {
      const sub = $("sheet-filter-color");
      const menu = $("sheet-filter-color-menu");
      if (!sub || !menu) return;
      menu.textContent = "";
      const current = criteria?.color;
      const labelOf = (text, className = "md-label") => {
        const span = document.createElement("span");
        span.className = className;
        span.textContent = text;
        return span;
      };
      const heading = (text) => {
        const el = document.createElement("div");
        el.className = "md-item";
        el.setAttribute("role", "presentation");
        el.setAttribute("aria-disabled", "true");
        el.appendChild(labelOf(text));
        menu.appendChild(el);
      };
      const pick = (el, font, rgb) => {
        el.dataset.colorPick = "1";
        el.dataset.font = font ? "1" : "";
        el.dataset.rgb = rgb;
      };
      let shown = 0;
      for (const [font, title, none, key, noneKey] of [
        [false, "셀 색 기준 필터", "채우기 없음", "fill", "noFill"],
        [true, "글꼴 색 기준 필터", "자동", "font", "noFont"],
      ]) {
        const list = colors?.[key] || [];
        const noneCount = colors?.[noneKey] || 0;
        if (list.length + (noneCount > 0 ? 1 : 0) < 2) continue;
        shown += 1;
        heading(title);
        const on = (rgb) => Boolean(current) && Boolean(current.font) === font && (current.rgb || "") === rgb;
        for (let at = 0; at < Math.min(list.length, MAX_SWATCHES); at += SWATCHES_PER_ROW) {
          const row = document.createElement("div");
          row.className = "sb-hl-palette-row";
          for (const [rgb, count] of list.slice(at, Math.min(at + SWATCHES_PER_ROW, MAX_SWATCHES))) {
            const swatch = document.createElement("button");
            swatch.type = "button";
            swatch.className = "sb-hl-swatch" + (on(rgb) ? " selected" : "");
            swatch.style.background = "#" + rgb;
            swatch.title = "#" + rgb + " (" + count + ")";
            swatch.setAttribute("aria-label", "#" + rgb);
            pick(swatch, font, rgb);
            row.appendChild(swatch);
          }
          menu.appendChild(row);
        }
        if (noneCount > 0) {
          const item = document.createElement("button");
          item.type = "button";
          item.className = "md-item";
          item.setAttribute("role", "menuitemcheckbox");
          item.setAttribute("aria-checked", on("") ? "true" : "false");
          item.appendChild(labelOf(none));
          item.appendChild(labelOf(String(noneCount), "md-hint"));
          pick(item, font, "");
          menu.appendChild(item);
        }
      }
      if (shown) sub.removeAttribute("aria-disabled");
      else sub.setAttribute("aria-disabled", "true");
    }

    // A colour (or "none") picked in the submenu.
    function pickColor(el) {
      if (!state) return;
      const col = state.col;
      const color = { ...(el.dataset.font ? { font: true } : {}), ...(el.dataset.rgb ? { rgb: el.dataset.rgb } : {}) };
      close(true);
      void filter.setCriteria(col, { color });
    }

    // Runs one of the panel's menu rows.
    function act(name) {
      if (!state) return;
      const col = state.col;
      if (name === "sort-asc" || name === "sort-desc") {
        close(true);
        void filter.sortBy(col, name === "sort-desc");
      } else if (name === "custom") {
        close(false);
        openCustom(col);
      } else if (name === "clear-column") {
        if (panel().querySelector('[data-filter-act="clear-column"]')?.getAttribute("aria-disabled") === "true") return;
        close(true);
        void filter.setCriteria(col, null);
      } else if (name === "cancel") {
        close(true);
      }
    }

    function onPanelClick(ev) {
      const colorEl = ev.target?.closest?.("[data-color-pick]");
      if (colorEl) {
        pickColor(colorEl);
        return;
      }
      const row = ev.target?.closest?.(".sheet-filter-row");
      if (row) {
        if (ev.target?.closest?.(".sheet-filter-arrow")) foldRow(Number(row.dataset.index));
        else toggleRow(Number(row.dataset.index));
        return;
      }
      const item = ev.target?.closest?.("[data-filter-act]");
      if (item) act(item.dataset.filterAct);
      else if (ev.target?.closest?.("#btn-sheet-filter-ok")) confirm();
    }

    // Keyboard order of the panel: the menu rows, the search field, the list, then the buttons
    // (Tab). Arrow keys walk the menu rows and the search field; in the list they move its cursor.
    const menuRows = () => [...panel().querySelectorAll('.md-item[data-filter-act]:not([aria-disabled="true"]), .md-sub:not([aria-disabled="true"])')];

    function focusList(cursor) {
      const list = $("sheet-filter-list");
      if (!list || !state) return;
      state.cursor = Math.max(0, Math.min(rowCount(state) - 1, cursor));
      list.focus();
      ensureVisible(state.cursor);
      renderRows();
    }

    function onListKey(ev) {
      const last = rowCount(state) - 1;
      const move = (to) => {
        ev.preventDefault();
        if (to < 0) {
          state.cursor = -1;
          renderRows();
          $("sheet-filter-search")?.focus();
          return;
        }
        state.cursor = Math.min(last, to);
        ensureVisible(state.cursor);
        renderRows();
      };
      if (ev.key === "ArrowDown") move(state.cursor + 1);
      else if (ev.key === "ArrowUp") move(state.cursor - 1);
      else if (ev.key === "PageDown") move(state.cursor + LIST_ROWS);
      else if (ev.key === "PageUp") move(Math.max(0, state.cursor - LIST_ROWS));
      else if (ev.key === "Home") move(0);
      else if (ev.key === "End") move(last);
      else if (ev.key === "ArrowRight" && state.cursor >= 0) {
        ev.preventDefault();
        foldRow(state.cursor, true);
      } else if (ev.key === "ArrowLeft" && state.cursor >= 0) {
        ev.preventDefault();
        foldRow(state.cursor, false);
      } else if (ev.key === " " && state.cursor >= 0) {
        ev.preventDefault();
        toggleRow(state.cursor);
      } else if (ev.key === "Enter") {
        ev.preventDefault();
        confirm();
      }
    }

    function onPanelKeydown(ev) {
      if (!state) return;
      const target = ev.target;
      if (ev.key === "Escape") {
        ev.preventDefault();
        close(true);
      } else if (target?.id === "sheet-filter-list") {
        onListKey(ev);
      } else if (target?.id === "sheet-filter-search") {
        if (ev.key === "ArrowDown") {
          ev.preventDefault();
          focusList(0);
        } else if (ev.key === "ArrowUp") {
          ev.preventDefault();
          menuRows().at(-1)?.focus();
        } else if (ev.key === "Enter") {
          ev.preventDefault();
          confirm();
        }
      } else if (target?.classList?.contains("md-sub")) {
        const rows = menuRows();
        const at = rows.indexOf(target);
        if (ev.key === "ArrowRight" || ev.key === "Enter" || ev.key === " ") {
          ev.preventDefault();
          target.querySelector("[data-color-pick]")?.focus();
        } else if (ev.key === "ArrowDown") {
          ev.preventDefault();
          rows[(at + 1) % rows.length]?.focus();
        } else if (ev.key === "ArrowUp") {
          ev.preventDefault();
          rows[(at - 1 + rows.length) % rows.length]?.focus();
        }
      } else if (target?.dataset?.colorPick && ev.key === "ArrowLeft") {
        ev.preventDefault();
        target.closest(".md-sub")?.focus();
      } else if (target?.dataset?.filterAct && target.classList.contains("md-item")) {
        const rows = menuRows();
        const at = rows.indexOf(target);
        if (ev.key === "ArrowDown") {
          ev.preventDefault();
          if (at === rows.length - 1) $("sheet-filter-search")?.focus();
          else rows[at + 1]?.focus();
        } else if (ev.key === "ArrowUp") {
          ev.preventDefault();
          rows[(at - 1 + rows.length) % rows.length]?.focus();
        } else if (ev.key === "Enter" || ev.key === " ") {
          ev.preventDefault();
          act(target.dataset.filterAct);
        }
      }
    }

    function onSearchInput() {
      const text = $("sheet-filter-search")?.value ?? "";
      host.clearTimeout?.(timer);
      timer = host.setTimeout?.(() => {
        if (state) void load(text);
      }, SEARCH_DELAY_MS);
    }

    // Test and keyboard entry: search now.
    function search(text) {
      setVal("sheet-filter-search", text);
      host.clearTimeout?.(timer);
      return state ? load(text) : Promise.resolve();
    }

    // ---- the custom condition dialog ---------------------------------------------------------------------
    let customCol = 0;
    const rowIds = [
      ["sheet-filter-op1", "sheet-filter-text1"],
      ["sheet-filter-op2", "sheet-filter-text2"],
    ];

    function syncTextFields() {
      for (const [opId, textId] of rowIds) {
        const field = $(textId);
        if (field) field.hidden = core.paramsOf($(opId)?.value ?? "none") === 0;
      }
    }

    function openCustom(col) {
      const f = filterNow();
      if (!f) return;
      customCol = col;
      const forms = core.formsFromCriteria(columnOf(f, col));
      forms.rows.forEach((form, k) => {
        setSelect(rowIds[k][0], form.op);
        setVal(rowIds[k][1], form.text);
      });
      const and = $("sheet-filter-and");
      const or = $("sheet-filter-or");
      if (and) and.checked = forms.and;
      if (or) or.checked = !forms.and;
      kit.note("sheet-filter-status", "");
      syncTextFields();
      openDialog("sheet-filter-dialog");
      $(rowIds[0][1])?.focus();
    }

    function applyCustom() {
      const forms = rowIds.map(([opId, textId]) => ({ op: $(opId)?.value ?? "none", text: $(textId)?.value ?? "" }));
      const result = core.criteriaFromForms($("sheet-filter-and")?.checked === true, forms);
      if (result.error) {
        kit.note("sheet-filter-status", result.error);
        return;
      }
      closeDialog("sheet-filter-dialog");
      void filter.setCriteria(customCol, result.criteria);
    }

    // ---- state shown elsewhere ----------------------------------------------------------------------------
    // The "필터" menu item is checked while the sheet has a filter.
    function refreshMenu() {
      const on = filterNow() ? "true" : "false";
      for (const el of document.querySelectorAll('[data-sheet-act="filter-toggle"]')) el.setAttribute("aria-checked", on);
    }

    function bind() {
      const viewport = $("sheet-viewport");
      viewport?.addEventListener("mousedown", onMouseDown, true);
      viewport?.addEventListener("keydown", onKeydown);
      viewport?.addEventListener("scroll", () => close(false));
      const el = panel();
      el?.addEventListener("click", onPanelClick);
      el?.addEventListener("keydown", onPanelKeydown);
      el?.addEventListener("contextmenu", (ev) => ev.preventDefault());
      $("sheet-filter-search")?.addEventListener("input", onSearchInput);
      $("sheet-filter-list")?.addEventListener("scroll", renderRows);
      document.addEventListener("mousedown", (ev) => {
        if (el && !el.hidden && !el.contains(ev.target)) close(false);
      });
      // Tab past the last button leaves the panel for the sheet.
      el?.addEventListener("focusout", (ev) => {
        const next = ev.relatedTarget;
        if (state && next && !el.contains(next)) close(false);
      });
      host.addEventListener?.("resize", () => close(false));
      host.addEventListener?.("blur", () => close(false));
      for (const [opId] of rowIds) $(opId)?.addEventListener("change", syncTextFields);
      $("btn-sheet-filter-apply")?.addEventListener("click", applyCustom);
      $("sheet-filter-dialog")?.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" && ev.target?.tagName === "INPUT" && ev.target.type === "text") {
          ev.preventDefault();
          applyCustom();
        }
      });
      filter.onChange(refreshMenu);
      layoutApi.onChange(refreshMenu);
      refreshMenu();
    }

    return { bind, open, close, search, confirm, openCustom, applyCustom, refreshMenu, state: () => state, rowCount: () => (state ? rowCount(state) : 0) };
  }

  host.__EASYOFFICE_CREATE_SHEET_FILTER_UI__ = createSheetFilterUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
