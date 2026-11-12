(function (host) {
  "use strict";

  // Row and column insertion, deletion, hiding and cell merging (PD-194), driven from the "시트"
  // menu and the row and column header context menu. The engine rewrites the formulas and moves
  // the rules, merges, hidden rows and widths (`structure` session action); this module sends the
  // request, applies the answer to the model as ONE undoable step and redraws.

  const MAX_ROWS_PER_EDIT = 10000;
  const MAX_COLS_PER_EDIT = 256;
  const MAX_HIDDEN_ROWS = 100000;
  const EDGE_HIT_PX = 6; // the thick edge of a header after a hidden run
  const MENU_MARGIN = 2; // rhwp-studio's context menu keeps 2px from the window edge
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

  const FAILURES = {
    limit: "시트가 커서 실행할 수 없음",
    structure: "바뀌는 수식이 너무 많음",
    error: "실행하지 못함",
  };

  function createSheetStructure(deps) {
    const {
      document,
      sheetDataStore,
      sheetRowCounts,
      sheetStoreHelpers,
      grid,
      getActiveSheetName,
      getSheetRowCount,
      getRange,
      getSelection,
      selectAxis,
      commitIfEditing,
      focusAnchor,
      refreshChrome,
      snapshotDomSheetCells,
      replaceLiveSheetDataStore,
      refreshWindowFromModel,
      recalculateActiveSheetFormulas,
      applyFrozenFlag,
      getExtent,
      sortRange,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
    } = deps;
    const rulesApi = host.__EASYOFFICE_SHEET_RULES__;
    const menu = () => document.getElementById("sheet-header-menu");
    // The auto filter (PD-198) shares this module's menu wiring and undo steps.
    const filter = host.__EASYOFFICE_CREATE_SHEET_FILTER__({ ...deps, sheetDataStore, sheetStoreHelpers });
    const filterUi = host.__EASYOFFICE_CREATE_SHEET_FILTER_UI__({
      document,
      filter,
      kit: host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document),
      getSelection,
      focusAnchor,
      getActiveSheetName,
    });

    const sortUi = host.__EASYOFFICE_CREATE_SHEET_SORT_UI__({
      ...deps,
      kit: host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document),
    });

    function say(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    // The rows or columns an action covers: the selection's own axis, else one row or column.
    function span(axis) {
      const rg = getRange();
      const sel = getSelection();
      if (axis === "row") return sel.axis === "col" ? [sel.ar, sel.ar] : [rg.r1, rg.r2];
      return sel.axis === "row" ? [sel.ac, sel.ac] : [rg.c1, rg.c2];
    }

    function pairsOf(widths) {
      return Object.keys(widths || {}).map((c) => [Number(c), Number(widths[c])]);
    }

    function layoutRequest(name) {
      const json = layoutApi.toJson(layoutApi.get(name) || { merges: [], hiddenRows: [], hiddenCols: [] });
      const rules = {};
      for (const sheet of Object.keys(sheetDataStore)) {
        if (rulesApi && !rulesApi.isEmpty(rulesApi.get(sheet))) rules[sheet] = JSON.parse(rulesApi.payloadFor(sheet));
      }
      return {
        merges: json.merges || [],
        hiddenRows: json.hiddenRows || [],
        hiddenCols: json.hiddenCols || [],
        filter: json.filter || null,
        widths: pairsOf(grid.getAllWidths()[name]),
        rules,
      };
    }

    // The answer applied to the model: cells move, rewritten formulas replace their text, the layout,
    // widths and rules of the engine replace the page's.
    function applyAnswer(name, edit, answer) {
      const nextSheets = {};
      for (const sheet of Object.keys(sheetDataStore)) nextSheets[sheet] = sheetDataStore[sheet];
      const old = sheetDataStore[name] || {};
      const frozen = Boolean(old.A1?.style?.frozenPanes);
      if (frozen) applyFrozenFlag(old, false);
      const moved = {};
      for (const ref of Object.keys(old)) {
        const to = layoutApi.remapRef(edit, ref);
        if (to !== null) moved[to] = old[ref];
      }
      if (frozen) applyFrozenFlag(moved, true);
      nextSheets[name] = moved;
      for (const f of Array.isArray(answer.formulas) ? answer.formulas : []) {
        const entry = nextSheets[f.sheet]?.[f.cell];
        if (entry) entry.formula = f.value;
      }
      const rowCounts = { ...sheetRowCounts };
      if (edit.axis === "row") {
        const current = getSheetRowCount(name);
        rowCounts[name] = edit.op === "insert" ? current + edit.count : Math.max(12, current - edit.count);
      }
      replaceLiveSheetDataStore(nextSheets, rowCounts);
      layoutApi.setSheet(name, answer);
      const widths = {};
      for (const [col, px] of Array.isArray(answer.widths) ? answer.widths : []) widths[col] = px;
      grid.replaceAllWidths({ ...grid.getAllWidths(), [name]: widths });
      if (rulesApi && answer.rules && Object.keys(answer.rules).length > 0) {
        rulesApi.replaceAll({ ...rulesApi.getAll(), ...answer.rules });
      }
    }

    function editAxis(axis, op, at, count) {
      const unit = axis === "row" ? "행" : "열";
      const limit = axis === "row" ? MAX_ROWS_PER_EDIT : MAX_COLS_PER_EDIT;
      if (count > limit) {
        say(unit + " " + limit + "개까지 가능");
        return Promise.resolve(false);
      }
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const name = getActiveSheetName();
        sheetDataStore[name] = snapshotDomSheetCells();
        const before = captureWorkspaceSnapshot(false);
        const edit = { axis, op, at, count };
        const result = await sheetStoreHelpers.editAxis(sheetDataStore, { sheet: name, ...edit, ...layoutRequest(name) });
        if (!result.ok) {
          say(result.message || FAILURES[result.reason] || FAILURES.error);
          return false;
        }
        pushRawUndoSnapshot(before);
        applyAnswer(name, edit, result.answer);
        grid.layoutChanged();
        refreshWindowFromModel();
        await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
        refreshChrome();
        // A deleted column took its condition along: the rows it hid are judged again by the rest.
        if (result.answer.filterReapply === true) await filter.reapplyAfterEdit();
        say(unit + " " + count + "개 " + (op === "insert" ? "삽입" : "삭제"));
        return true;
      });
    }

    // Hiding and unhiding touch the layout only; one undo step.
    function setHidden(axis, first, last, hidden) {
      const name = getActiveSheetName();
      const layout = layoutApi.get(name);
      if (hidden && axis === "row" && (layout ? layoutApi.hiddenTotal(layout.hiddenRows) : 0) + (last - first + 1) > MAX_HIDDEN_ROWS) {
        say("숨길 수 있는 행은 " + MAX_HIDDEN_ROWS + "개까지");
        return Promise.resolve(false);
      }
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        layoutApi.setHidden(name, axis, first, last, hidden);
        grid.layoutChanged();
        refreshChrome();
        say((axis === "row" ? "행 " : "열 ") + (hidden ? "숨김" : "숨기기 취소"));
        return true;
      });
    }

    // Merging keeps the anchor cell's content and clears the other cells of the range.
    function merge() {
      const rg = getRange();
      if (rg.r1 === rg.r2 && rg.c1 === rg.c2) {
        say("병합할 범위를 선택");
        return Promise.resolve(false);
      }
      if (getSelection().axis) {
        say("셀 범위를 선택");
        return Promise.resolve(false);
      }
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const name = getActiveSheetName();
        const map = snapshotDomSheetCells();
        sheetDataStore[name] = map;
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        const parse = grid.parseRef;
        for (const ref of Object.keys(map)) {
          const p = parse(ref);
          if (p && p.row >= rg.r1 && p.row <= rg.r2 && p.col >= rg.c1 && p.col <= rg.c2 && !(p.row === rg.r1 && p.col === rg.c1)) {
            delete map[ref];
          }
        }
        layoutApi.addMerge(name, rg.r1, rg.c1, rg.r2, rg.c2);
        grid.layoutChanged();
        refreshWindowFromModel();
        await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
        refreshChrome();
        say("병합");
        return true;
      });
    }

    function unmerge() {
      const rg = getRange();
      const name = getActiveSheetName();
      if (layoutApi.mergesIn(name, rg.r1, rg.c1, rg.r2, rg.c2).length === 0) {
        say("병합된 셀 없음");
        return Promise.resolve(false);
      }
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        layoutApi.removeMerges(name, rg.r1, rg.c1, rg.r2, rg.c2);
        grid.layoutChanged();
        refreshChrome();
        say("병합 해제");
        return true;
      });
    }

    const ACTIONS = {
      "row-insert-above": () => {
        const [first, last] = span("row");
        return editAxis("row", "insert", first, last - first + 1);
      },
      "row-insert-below": () => {
        const [first, last] = span("row");
        return editAxis("row", "insert", last + 1, last - first + 1);
      },
      "col-insert-left": () => {
        const [first, last] = span("col");
        return editAxis("col", "insert", first, last - first + 1);
      },
      "col-insert-right": () => {
        const [first, last] = span("col");
        return editAxis("col", "insert", last + 1, last - first + 1);
      },
      "row-delete": () => {
        const [first, last] = span("row");
        return editAxis("row", "delete", first, last - first + 1);
      },
      "col-delete": () => {
        const [first, last] = span("col");
        return editAxis("col", "delete", first, last - first + 1);
      },
      hide: () => {
        const axis = getSelection().axis === "col" ? "col" : "row";
        const [first, last] = span(axis);
        return setHidden(axis, first, last, true);
      },
      unhide: () => {
        const axis = getSelection().axis === "col" ? "col" : "row";
        const [first, last] = span(axis);
        // A hidden run right before or after the selection is part of it.
        const layout = layoutApi.get(getActiveSheetName());
        const runs = (axis === "row" ? layout?.hiddenRows : layout?.hiddenCols) || [];
        const before = layoutApi.runAt(runs, first - 1);
        const after = layoutApi.runAt(runs, last + 1);
        return setHidden(axis, before ? before[0] : first, after ? after[1] : last, false);
      },
      merge,
      unmerge,
      "sort-dialog": () => Promise.resolve(sortUi.open()),
      "filter-toggle": () => filter.run("filter-toggle"),
      "filter-reapply": () => filter.run("filter-reapply"),
      "filter-clear": () => filter.run("filter-clear"),
    };

    function run(action) {
      const fn = ACTIONS[action];
      return fn ? fn() : Promise.resolve(false);
    }

    // ---- header context menu ----------------------------------------------------------------------
    const visibleItems = () => Array.from(menu().querySelectorAll(".md-item")).filter((el) => !el.hidden);

    function closeMenu(refocus) {
      const el = menu();
      if (!el || el.hidden) return;
      el.hidden = true;
      if (refocus) focusAnchor();
    }

    function placeMenu(el, x, y) {
      el.style.maxHeight = "";
      el.style.left = "0px";
      el.style.top = "0px";
      const w = el.offsetWidth;
      const h = el.offsetHeight;
      const vw = host.innerWidth || document.documentElement.clientWidth;
      const vh = host.innerHeight || document.documentElement.clientHeight;
      if (h > vh - MENU_MARGIN) el.style.maxHeight = vh - MENU_MARGIN + "px";
      const height = Math.min(h, vh - MENU_MARGIN);
      el.style.left = Math.max(0, Math.min(x, vw - w - MENU_MARGIN)) + "px";
      el.style.top = Math.max(0, Math.min(y, vh - height - MENU_MARGIN)) + "px";
    }

    function openMenu(axis, x, y) {
      const el = menu();
      if (!el) return;
      for (const item of el.querySelectorAll("[data-axis]")) item.hidden = item.dataset.axis !== axis;
      el.hidden = false;
      placeMenu(el, x, y);
      visibleItems()[0]?.focus();
    }

    function onContextMenu(ev) {
      const th = ev.target?.closest?.("#pane-sheet th[data-col], #pane-sheet th[data-row]");
      if (!th) return;
      ev.preventDefault();
      commitIfEditing();
      const axis = th.dataset.col ? "col" : "row";
      const index = axis === "col" ? grid.colIndex(th.dataset.col) : Number(th.dataset.row);
      const rg = getRange();
      const inside =
        getSelection().axis === axis &&
        index >= (axis === "col" ? rg.c1 : rg.r1) &&
        index <= (axis === "col" ? rg.c2 : rg.r2);
      if (!inside) selectAxis(axis, index, false);
      openMenu(axis, ev.clientX, ev.clientY);
    }

    // Shift+F10 or the menu key on a selected header opens the menu at that header.
    function onKeydown(ev) {
      const sel = getSelection();
      if (!sel.axis || !(ev.key === "ContextMenu" || (ev.key === "F10" && ev.shiftKey))) return;
      const th = sel.axis === "col"
        ? document.querySelector('#pane-sheet th[data-col="' + grid.colName(Math.min(sel.ac, sel.cc)) + '"]')
        : document.querySelector('#pane-sheet th[data-row="' + Math.min(sel.ar, sel.cr) + '"]');
      const box = th?.getBoundingClientRect?.();
      ev.preventDefault();
      openMenu(sel.axis, box ? box.left : 0, box ? box.bottom : 0);
    }

    function onMenuKeydown(ev) {
      const items = visibleItems();
      const index = items.indexOf(document.activeElement);
      const move = (to) => {
        ev.preventDefault();
        items[(to + items.length) % items.length]?.focus();
      };
      if (ev.key === "ArrowDown") move(index + 1);
      else if (ev.key === "ArrowUp") move(index - 1);
      else if (ev.key === "Home") move(0);
      else if (ev.key === "End") move(-1);
      else if (ev.key === "Escape") {
        ev.preventDefault();
        closeMenu(true);
      } else if (ev.key === "Tab") {
        ev.preventDefault();
        closeMenu(true);
      } else if (ev.key === "Enter" || ev.key === " ") {
        ev.preventDefault();
        document.activeElement?.click?.();
      }
    }

    // A double click on the thick edge of a header after a hidden run shows that run again.
    function onDblClick(ev) {
      const th = ev.target?.closest?.("#pane-sheet th[data-hidden-run]");
      if (!th) return;
      const box = th.getBoundingClientRect();
      const axis = th.dataset.col ? "col" : "row";
      const offset = axis === "col" ? ev.clientX - box.left : ev.clientY - box.top;
      if (offset > EDGE_HIT_PX) return;
      ev.preventDefault();
      const [first, last] = th.dataset.hiddenRun.split("-").map(Number);
      void setHidden(axis, first, last, false);
    }

    function bind() {
      filterUi.bind();
      sortUi.bind();
      const viewport = document.getElementById("sheet-viewport");
      viewport?.addEventListener("dblclick", onDblClick);
      viewport?.addEventListener("contextmenu", onContextMenu);
      viewport?.addEventListener("keydown", onKeydown);
      viewport?.addEventListener("scroll", () => closeMenu(false));
      const el = menu();
      el?.addEventListener("keydown", onMenuKeydown);
      el?.addEventListener("contextmenu", (ev) => ev.preventDefault());
      document.addEventListener("mousedown", (ev) => {
        if (el && !el.hidden && !el.contains(ev.target)) closeMenu(false);
      });
      host.addEventListener?.("resize", () => closeMenu(false));
      host.addEventListener?.("blur", () => closeMenu(false));
      for (const item of document.querySelectorAll(".md-item[data-sheet-act]")) {
        item.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          closeMenu(true);
          document.querySelectorAll(".menu-item.open").forEach((open) => open.classList.remove("open"));
          if (item.getAttribute("aria-disabled") === "true") return;
          void run(item.dataset.sheetAct);
        });
      }
    }

    return { run, bind, closeMenu, refreshFilterMenu: filterUi.refreshMenu };
  }

  host.__EASYOFFICE_CREATE_SHEET_STRUCTURE__ = createSheetStructure;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
