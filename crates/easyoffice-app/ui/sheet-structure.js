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
  // The protection flag a row or column edit needs (PD-216).
  const EDIT_FLAGS = { "row:insert": "insertRows", "row:delete": "deleteRows", "col:insert": "insertColumns", "col:delete": "deleteColumns" };

  function createSheetStructure(deps) {
    const {
      document,
      sheetDataStore,
      sheetRowCounts,
      sheetStoreHelpers,
      grid,
      getActiveSheetName,
      getActiveModule,
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
      getExtent,
      sortRange,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
    } = deps;
    const rulesApi = host.__EASYOFFICE_SHEET_RULES__;
    const linkApi = host.__EASYOFFICE_SHEET_LINK__;
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    const menu = () => document.getElementById("sheet-header-menu");
    // The height of a row by hand: the drag of the edge of its header and the fit (PD-278).
    const rowResize = host.__EASYOFFICE_CREATE_SHEET_ROW_RESIZE__({ ...deps, sheetDataStore });
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

    // Frozen panes and groups (PD-208).
    const view = host.__EASYOFFICE_CREATE_SHEET_VIEW__({ ...deps, kit: host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document) });
    const outlineUi = host.__EASYOFFICE_CREATE_SHEET_OUTLINE_UI__({ document, grid, view, getActiveSheetName });
    // Page setup and print area (PD-213).
    const pageUi = host.__EASYOFFICE_CREATE_SHEET_PAGE__(deps);

    // The name box and the defined names dialogs (PD-214).
    const namesUi = host.__EASYOFFICE_CREATE_SHEET_NAMES_UI__({ ...deps, kit: host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document) });

    // The tables: 표 만들기, the 표 디자인 bar and the growth of a table over what is typed (PD-231).
    const tableUi = host.__EASYOFFICE_CREATE_SHEET_TABLE_UI__({
      ...deps,
      kit: host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document),
      editAxis: (...args) => editAxis(...args),
      filter,
    });

    // The hyperlink dialog and the click on a link cell (PD-218).
    const linkUi = host.__EASYOFFICE_CREATE_SHEET_LINK_UI__(deps);

    // Sheet and workbook protection and the ranges a protected sheet allows (PD-216).
    const protectUi = host.__EASYOFFICE_CREATE_SHEET_PROTECT_UI__(deps);
    const rangesUi = host.__EASYOFFICE_CREATE_SHEET_PROTECT_RANGES__({ ...deps, ui: protectUi });

    // Sheet tabs (PD-209): insert, delete, rename, copy, move, hide and show again.
    const tabUi = host.__EASYOFFICE_CREATE_SHEET_TAB_UI__({ document, kit: host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document), getTabs: () => tabs, say });
    const tabs = host.__EASYOFFICE_CREATE_SHEET_TABS__({ ...deps, ui: tabUi });
    const tabMenu = host.__EASYOFFICE_CREATE_SHEET_TAB_MENU__({ document, getTabs: () => tabs, ui: tabUi });
    const mediaUi = host.__EASYOFFICE_CREATE_SHEET_MEDIA_UI__?.(deps) ?? null;
    const noteUi = host.__EASYOFFICE_CREATE_SHEET_NOTE_UI__?.(deps) ?? null;
    const viewOptions = host.__EASYOFFICE_CREATE_SHEET_VIEW_OPTIONS__?.(deps) ?? null;
    const gotoUi = host.__EASYOFFICE_CREATE_SHEET_GOTO__?.(deps) ?? null;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);

    function say(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    const sizeDialogs = host.__EASYOFFICE_CREATE_SHEET_SIZE_DIALOGS__?.({
      ...deps,
      kit,
      fitColumns,
      fitRows: rowResize.fitRows,
      say,
    }) ?? null;
    const textColumnsUi = host.__EASYOFFICE_CREATE_SHEET_TEXT_COLUMNS__?.({ ...deps, kit, say }) ?? null;
    // 데이터 > 개요 > 부분합 (PD-391).
    const subtotalUi = host.__EASYOFFICE_CREATE_SHEET_SUBTOTAL_UI__?.({ ...deps, kit, say, editAxis: (...args) => editAxis(...args) }) ?? null;
    const pickList = host.__EASYOFFICE_CREATE_SHEET_PICK_LIST__?.({ ...deps, say }) ?? null;
    const contextMenu = host.__EASYOFFICE_CREATE_SHEET_CONTEXT_MENU__?.({
      ...deps,
      kit,
      filter,
      quickSort: (asc) => sortUi.quick(asc),
      pickList: pickList ?? { open: () => false },
      runStructureAct: (act) => run(act),
      editAxis,
      say,
    }) ?? null;

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

    // The charts of the other sheets point at the edited one with ranges like `S!$B$2:$B$5`. The engine moves those references by
    // the rule of a formula, so the page sends the references of the charts and not their anchors (PD-303).
    function otherChartRefs(name) {
      const out = {};
      for (const sheet of Object.keys(sheetDataStore)) {
        const charts = sheet === name ? [] : layoutApi.chartsOf(sheet);
        if (charts.length === 0) continue;
        out[sheet] = charts.map((c) => ({
          id: c.id,
          dataRange: c.dataRange,
          switchRowCol: c.switchRowCol,
          series: (c.series || []).map(({ nameRef, categoriesRef, valuesRef }) => ({ nameRef, categoriesRef, valuesRef })),
        }));
      }
      return out;
    }

    // The answer names the sheets whose charts changed, with all the charts of each in the order the request had them (ids can repeat
    // in a sheet, so the place pairs them); each chart takes the moved references and keeps the rest. A sheet whose charts are not
    // the ones the request had (another count or another id at a place) is left as it is.
    function applyChartRefs(refs) {
      for (const [sheet, moved] of Object.entries(refs || {})) {
        const charts = structuredClone(layoutApi.chartsOf(sheet));
        if (!Array.isArray(moved) || moved.length !== charts.length || moved.some((m, k) => m?.id !== charts[k].id)) continue;
        moved.forEach((m, k) => {
          if (typeof m.dataRange === "string") charts[k].dataRange = m.dataRange;
          (m.series || []).forEach((s, i) => {
            for (const key of ["nameRef", "categoriesRef", "valuesRef"]) if (charts[k].series?.[i] && typeof s[key] === "string") charts[k].series[i][key] = s[key];
          });
        });
        layoutApi.setCharts(sheet, charts);
      }
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
        rowHeights: json.rowHeights || [],
        // The rows below the default height too: the engine places pictures and charts on the rows after the edit (PD-299).
        defaultRowHeight: json.defaultRowHeight,
        // The columns without a width keep the sheet's width after the edit (PD-305), in digits of the workbook's font (PD-306).
        defaultColWidth: json.defaultColWidth,
        ...host.__EASYOFFICE_SHEET_DEFAULT_FONT__?.field(),
        outline: json.outline,
        page: json.page,
        protect: json.protect || null,
        unlockedRows: json.unlockedRows || [],
        unlockedCols: json.unlockedCols || [],
        tables: json.tables || [],
        // The charts move with their cells as the pictures do, and their data ranges follow the edit (PD-302).
        charts: json.charts || [],
        chartRefs: otherChartRefs(name),
        images: (json.images || []).map(({ data, ...rest }) => rest),
        notes: json.notes || [],
        widths: pairsOf(grid.getAllWidths()[name]),
        rules,
        // The in-workbook targets of the cell links move with the cells (PD-218).
        ...linkApi?.requestField(sheetDataStore),
      };
    }

    // An inserted row or column takes the formats, not the contents, of the one before it, as the spreadsheet
    // program's insert does (PD-227); the first row and column have nothing before them. The copies are bounded.
    const MAX_INHERITED_CELLS = 200000;
    function inheritFormats(moved, edit) {
      if (edit.op !== "insert" || edit.at <= 1) return;
      const before = edit.at - 1;
      const byRow = edit.axis === "row";
      const copies = {};
      let n = 0;
      for (const ref of Object.keys(moved)) {
        const p = grid.parseRef(ref);
        const style = moved[ref].style;
        if (!p || !style || (byRow ? p.row : p.col) !== before) continue;
        if (byRow && edit.cols && (p.col < edit.cols[0] || p.col > edit.cols[1])) continue;
        for (let i = 1; i <= edit.count; i += 1) {
          copies[byRow ? grid.refOf(before + i, p.col) : grid.refOf(p.row, before + i)] = { value: "", formula: "", style };
        }
        n += edit.count;
        if (n > MAX_INHERITED_CELLS) return;
      }
      Object.assign(moved, copies);
    }

    function fillInsertedCalculatedRows(name, edit) {
      if (edit.axis !== "row" || edit.op !== "insert") return;
      const map = sheetDataStore[name] || (sheetDataStore[name] = {});
      for (const t of layoutApi.tablesOf(name)) {
        if (!Array.isArray(t.calculatedColumns)) continue;
        const b = layoutApi.tableBox(t);
        if (edit.cols && (b.c1 < edit.cols[0] || b.c2 > edit.cols[1])) continue;
        const d1 = b.r1 + (t.header === false ? 0 : 1);
        const d2 = b.r2 - (t.totals === true ? 1 : 0);
        const r0 = Math.max(d1, edit.at);
        const r1 = Math.min(d2, edit.at + edit.count - 1);
        for (let r = r0; r <= r1; r += 1) {
          t.calculatedColumns.forEach((formula, i) => {
            if (!formula) return;
            const ref = grid.refOf(r, b.c1 + i);
            const cur = map[ref];
            if (!cur || (!cur.value && !cur.formula)) map[ref] = { ...cur, value: "", formula };
          });
        }
      }
    }

    // The answer applied to the model: cells move, rewritten formulas replace their text, the layout,
    // widths and rules of the engine replace the page's.
    function applyAnswer(name, edit, answer) {
      let nextSheets = {};
      for (const sheet of Object.keys(sheetDataStore)) nextSheets[sheet] = sheetDataStore[sheet];
      const old = sheetDataStore[name] || {};
      const moved = {};
      for (const ref of Object.keys(old)) {
        const to = layoutApi.remapRef(edit, ref);
        if (to !== null) moved[to] = old[ref];
      }
      inheritFormats(moved, edit);
      nextSheets[name] = moved;
      for (const f of Array.isArray(answer.formulas) ? answer.formulas : []) {
        const entry = nextSheets[f.sheet]?.[f.cell];
        if (entry) entry.formula = f.value;
      }
      if (linkApi && answer.linkTargets) nextSheets = linkApi.retarget(nextSheets, answer.linkTargets);
      const rowCounts = { ...sheetRowCounts };
      if (edit.axis === "row") {
        const current = getSheetRowCount(name);
        rowCounts[name] = edit.op === "insert" ? current + edit.count : Math.max(12, current - edit.count);
      }
      replaceLiveSheetDataStore(nextSheets, rowCounts);
      // The answer moves what sits at a position; the frozen counts and the tab colour stay as they are.
      const kept = layoutApi.get(name);
      layoutApi.setSheet(name, { ...answer, freeze: kept?.freeze, tabColor: kept?.tabColor, hidden: kept?.hidden, defaultColWidth: kept?.defaultColWidth, defaultColPx: kept?.defaultColPx });
      applyChartRefs(answer.chartRefs);
      const widths = {};
      for (const [col, px] of Array.isArray(answer.widths) ? answer.widths : []) widths[col] = px;
      grid.replaceAllWidths({ ...grid.getAllWidths(), [name]: widths });
      if (rulesApi && answer.rules && Object.keys(answer.rules).length > 0) {
        rulesApi.replaceAll({ ...rulesApi.getAll(), ...answer.rules });
      }
    }

    // Whether the sheet's protection lets the rows or columns be inserted or deleted: the flag, and for a
    // deletion that no locked cell with content goes with them (PD-216).
    function editAllowed(name, axis, op, at, count, cols = null) {
      if (!protectApi.guardAction(name, EDIT_FLAGS[axis + ":" + op])) return false;
      if (op !== "delete" || !protectApi.isOn(name)) return true;
      const map = sheetDataStore[name] || {};
      for (const ref of Object.keys(map)) {
        const p = grid.parseRef(ref);
        if (axis === "row" && cols && p && (p.col < cols[0] || p.col > cols[1])) continue;
        const index = p && (axis === "row" ? p.row : p.col);
        const entry = map[ref];
        if (index >= at && index < at + count && (entry.value || entry.formula) && !protectApi.cellEditable(name, p.row, p.col, entry.style)) {
          protectApi.alertBlocked();
          return false;
        }
      }
      return true;
    }

    // `inStep(sheet)` runs in the step after the answer is applied, so a command that opens a row for its own cells is one undo step.
    function editAxis(axis, op, at, count, inStep, cols = null) {
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
        if (protectApi && !editAllowed(name, axis, op, at, count, cols)) return false;
        const edit = cols ? { axis, op, at, count, cols } : { axis, op, at, count };
        // Every cell may move or be rewritten by the edit, and the engine's answer lands on the model before the cells
        // move: the step keeps the cells the edit changed or deleted, read against where the edit puts them (PD-216).
        const change = sheetHistory.begin({ sheets: "all", parts: "all", shift: { sheet: name, ...edit } });
        const result = await sheetStoreHelpers.editAxis(sheetDataStore, { sheet: name, ...edit, ...layoutRequest(name) });
        if (!result.ok) {
          if (result.reason === "protected") protectApi.alertBlocked();
          else say(result.message || FAILURES[result.reason] || FAILURES.error);
          return false;
        }
        applyAnswer(name, edit, result.answer);
        fillInsertedCalculatedRows(name, edit);
        // The rebuilt session placed every array again: the cells they fill are taken from it (PD-217).
        if (Array.isArray(result.answer.spillAreas)) host.__EASYOFFICE_SHEET_SPILL__?.replaceAll(sheetDataStore, result.answer.spillAreas);
        inStep?.(name);
        sheetHistory.commit(change);
        grid.layoutChanged();
        refreshWindowFromModel();
        await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
        refreshChrome();
        // A deleted column took its condition along: the rows it hid are judged again by the rest.
        if (result.answer.filterReapply === true || (axis === "col" && op === "delete" && filter.hasTableFilters(name))) {
          await filter.reapplyAfterEdit(result.answer.filterReapply === true);
        }
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
        if (protectApi && !protectApi.guardAction(name, axis === "row" ? "formatRows" : "formatColumns")) return false;
        const change = sheetHistory.begin({ parts: ["sheetLayout"] });
        layoutApi.setHidden(name, axis, first, last, hidden);
        sheetHistory.commit(change);
        grid.layoutChanged();
        refreshChrome();
        say((axis === "row" ? "행 " : "열 ") + (hidden ? "숨김" : "숨기기 취소"));
        return true;
      });
    }

    // Fits the width of `cols` to their content as ONE undoable step (`sheet-col-fit.js`); the widths are the grid's own,
    // so a fitted column saves like a dragged one.
    function fitColumns(cols) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const name = getActiveSheetName();
        if (protectApi && !protectApi.guardAction(name, "formatColumns")) return false;
        sheetDataStore[name] = snapshotDomSheetCells();
        const hasFilters = Boolean(layoutApi.filterOf(name)) || layoutApi.tablesOf(name).length > 0;
        const fit = host.__EASYOFFICE_CREATE_SHEET_COL_FIT__({
          getMap: () => sheetDataStore[name],
          packRef: grid.packRef,
          measure: grid.measureText,
          isColHidden: (c) => layoutApi.colHidden(name, c),
          isRowHidden: (r) => Boolean(layoutApi.rowHidden(name, r)),
          mergeAt: (r, c) => layoutApi.mergeAt(name, r, c),
          headerCells: (r, c) => hasFilters && layoutApi.filterOwnerAt(name, r, c) !== undefined,
          formatGeneral: host.__EASYOFFICE_SHEET_GRID_UTILS__.formatGeneral,
          defaultWidth: grid.DEFAULT_COL_W,
          minWidth: grid.MIN_COL_W,
          maxWidth: grid.MAX_COL_W,
        });
        const display = host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__;
        const fitted = await fit.widthsFor(cols, display?.resolveAll ? (wanted) => display.resolveAll(wanted) : null);
        if (fitted.refused) {
          say(fitted.refused === "budget" ? "열 너비를 맞추기에 시트가 너무 큼" : "글꼴을 잴 수 없어 열 너비를 맞추지 못함");
          return false;
        }
        const changed = fitted.widths.filter(({ col, width }) => grid.colWidth(col) !== Math.max(grid.MIN_COL_W, Math.min(grid.MAX_COL_W, Math.round(width))));
        if (changed.length === 0) return false;
        const change = sheetHistory.begin({ parts: ["sheetColumnWidths"] });
        grid.applyColWidths(changed.map(({ col, width }) => [col, width]));
        sheetHistory.commit(change);
        refreshChrome();
        say("열 너비 자동 맞춤: " + changed.length + "열");
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
      // The spreadsheet program does not merge cells of a table (PD-224).
      if (layoutApi.tablesIn(getActiveSheetName(), rg.r1, rg.c1, rg.r2, rg.c2).length > 0) {
        say("표 안의 칸은 병합할 수 없음");
        return Promise.resolve(false);
      }
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const name = getActiveSheetName();
        const map = snapshotDomSheetCells();
        sheetDataStore[name] = map;
        if (protectApi && !(protectApi.guardAction(name, "formatCells") && protectApi.guardCells(name, rg, (r, c) => map[grid.refOf(r, c)]?.style))) return false;
        const change = sheetHistory.begin({ range: rg, parts: ["sheetLayout"] });
        const parse = grid.parseRef;
        for (const ref of Object.keys(map)) {
          const p = parse(ref);
          if (p && p.row >= rg.r1 && p.row <= rg.r2 && p.col >= rg.c1 && p.col <= rg.c2 && !(p.row === rg.r1 && p.col === rg.c1)) {
            delete map[ref];
          }
        }
        layoutApi.addMerge(name, rg.r1, rg.c1, rg.r2, rg.c2);
        sheetHistory.commit(change);
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
        if (protectApi && !protectApi.guardAction(name, "formatCells")) return false;
        const change = sheetHistory.begin({ parts: ["sheetLayout"] });
        layoutApi.removeMerges(name, rg.r1, rg.c1, rg.r2, rg.c2);
        sheetHistory.commit(change);
        grid.layoutChanged();
        // The merge no longer stops an array from filling its cells (PD-222).
        await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
        refreshChrome();
        say("병합 해제");
        return true;
      });
    }

    function setCalcModeAction(targetMode, label) {
      const calcApi = host.__EASYOFFICE_SHEET_CALC_OPTIONS__;
      if (!calcApi) return Promise.resolve(false);
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        if (calcApi.mode() === targetMode) {
          calcApi.syncMenu(document);
          say("계산 옵션: " + label);
          return true;
        }
        const change = sheetHistory.begin({ parts: ["calcOptions"] });
        calcApi.setMode(targetMode, document);
        sheetHistory.commit(change);
        if (targetMode !== "manual") {
          await recalculateActiveSheetFormulas({ force: "all", updateFocusedDisplay: true });
        }
        say("계산 옵션: " + label);
        return true;
      });
    }

    function runCalcForce(scope, label) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        await recalculateActiveSheetFormulas({ force: scope, updateFocusedDisplay: true });
        say(label);
        return true;
      });
    }

    const selectedAxis = () => (getSelection().axis === "col" ? "col" : "row");

    function hideSelection(axis, hidden) {
      const [first, last] = span(axis);
      if (hidden) return setHidden(axis, first, last, true);
      // A hidden run right before or after the selection is part of it.
      const layout = layoutApi.get(getActiveSheetName());
      const runs = (axis === "row" ? layout?.hiddenRows : layout?.hiddenCols) || [];
      const before = layoutApi.runAt(runs, first - 1);
      const after = layoutApi.runAt(runs, last + 1);
      return setHidden(axis, before ? before[0] : first, after ? after[1] : last, false);
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
      "link-edit": () => Promise.resolve(linkUi.open()),
      "link-clear": linkUi.remove,
      // The header menu hides what the selected header names; the 서식 menu names the axis (행 숨기기, 열 숨기기).
      hide: () => hideSelection(selectedAxis(), true),
      unhide: () => hideSelection(selectedAxis(), false),
      "row-hide": () => hideSelection("row", true),
      "row-unhide": () => hideSelection("row", false),
      "col-hide": () => hideSelection("col", true),
      "col-unhide": () => hideSelection("col", false),
      merge,
      unmerge,
      ...view.actions,
      ...pageUi.actions,
      ...namesUi.actions,
      ...protectUi.actions,
      ...rangesUi.actions,
      ...linkUi.actions,
      ...tableUi.actions,
      ...(mediaUi?.actions || {}),
      ...(noteUi?.actions || {}),
      ...(viewOptions?.actions || {}),
      ...(gotoUi?.actions || {}),
      ...(subtotalUi?.actions || {}),
      "tab-add": () => tabs.insert(null),
      "tab-color": () => Promise.resolve(tabMenu.openColor(getActiveSheetName())),
      ...tabs.actions,
      "sort-dialog": () => Promise.resolve(sortUi.open()),
      "filter-toggle": () => filter.run("filter-toggle"),
      "filter-reapply": () => filter.run("filter-reapply"),
      "filter-clear": () => filter.run("filter-clear"),
      "row-height": () => Promise.resolve(sizeDialogs?.openRowHeight() ?? false),
      "row-autofit": () => (sizeDialogs ? sizeDialogs.autofitRows() : Promise.resolve(false)),
      "col-width": () => Promise.resolve(sizeDialogs?.openColWidth() ?? false),
      "col-autofit": () => (sizeDialogs ? sizeDialogs.autofitCols() : Promise.resolve(false)),
      "text-to-columns": () => Promise.resolve(textColumnsUi?.open() ?? false),
      "calc-auto": () => setCalcModeAction("auto", "자동"),
      "calc-auto-no-table": () => setCalcModeAction("autoNoTable", "데이터 테이블을 제외한 자동"),
      "calc-manual": () => setCalcModeAction("manual", "수동"),
      "calc-now": () => runCalcForce("all", "지금 계산 완료"),
      "calc-sheet": () => runCalcForce("sheet", "시트 계산 완료: " + getActiveSheetName()),
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
      contextMenu?.close(false);
      for (const item of el.querySelectorAll("[data-axis]")) item.hidden = !item.dataset.axis.split(" ").includes(axis);
      el.hidden = false;
      placeMenu(el, x, y);
      visibleItems()[0]?.focus();
    }

    // A right click on a cell opens the hyperlink items (`하이퍼링크 고치기...`, `하이퍼링크 지우기`), the cell note items
    // (`메모 편집`, `메모 삭제`, `메모 표시/숨기기`), or both when the cell carries both (PD-223, PD-256).
    function openCellMenu(ev) {
      const td = ev.target?.closest?.("#pane-sheet td[data-cell]");
      if (!td) return false;
      const hasLink = Boolean(td.dataset?.link && td._link);
      const hasNote = td.dataset?.note === "true";
      if (!hasLink && !hasNote) return false;
      ev.preventDefault();
      commitIfEditing();
      deps.selectRange({ r1: td._row, c1: td._col, r2: td._row, c2: td._col });
      openMenu(hasLink && hasNote ? "link-note" : hasLink ? "link" : "note", ev.clientX, ev.clientY);
      return true;
    }

    function onContextMenu(ev) {
      const th = ev.target?.closest?.("#pane-sheet th[data-col], #pane-sheet th[data-row]");
      if (!th) {
        if (!openCellMenu(ev)) contextMenu?.onCellContextMenu(ev);
        return;
      }
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

    // A double click on the edge of a column header fits the column (the selected columns when it is one of them).
    function fitFromEdge(ev) {
      const handle = ev.target?.closest?.(".col-resizer");
      if (!handle) return false;
      ev.preventDefault();
      const pressed = grid.colIndex(handle.dataset.colResizer || "A");
      const rg = getRange();
      void fitColumns(host.__EASYOFFICE_SHEET_COL_FIT__.columnsToFit(pressed, { axis: getSelection().axis, c1: rg.c1, c2: rg.c2 }));
      return true;
    }

    // A double click on the thick edge of a header after a hidden run shows that run again.
    function onDblClick(ev) {
      if (fitFromEdge(ev)) return;
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

    function onCalcKeydown(ev) {
      if (ev.defaultPrevented || ev.__eoCalcKeyHandled) return;
      if (ev.ctrlKey || ev.metaKey || ev.altKey) return;
      if ((typeof getActiveModule === "function" && getActiveModule() !== "SHEET") || document.getElementById("pane-sheet")?.hidden) return;
      if (document.querySelector?.("dialog[open]")) return;
      if (ev.key === "F3" && !ev.shiftKey) {
        if (ev.target?.closest?.(".sheet-tab-edit, .sheet-table-bar input, #active-cell-ref")) return;
        ev.__eoCalcKeyHandled = true;
        ev.preventDefault?.();
        ev.stopPropagation?.();
        void run("name-paste");
        return;
      }
      if (ev.key !== "F9") return;
      ev.__eoCalcKeyHandled = true;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      void run(ev.shiftKey ? "calc-sheet" : "calc-now");
    }

    function bind() {
      filterUi.bind();
      sortUi.bind();
      tabUi.bind();
      tabMenu.bind();
      outlineUi.bind();
      pageUi.bind();
      namesUi.bind();
      protectUi.bind();
      rangesUi.bind();
      linkUi.bind();
      tableUi.bind();
      mediaUi?.bind();
      noteUi?.bind();
      rowResize.bind();
      sizeDialogs?.ensureBound();
      textColumnsUi?.ensureBound();
      contextMenu?.bind();
      pickList?.bind();
      host.__EASYOFFICE_ON_MENUS_CLOSED__?.(document, () => closeMenu(false));
      const viewport = document.getElementById("sheet-viewport");
      viewport?.addEventListener("dblclick", onDblClick);
      viewport?.addEventListener("contextmenu", onContextMenu);
      viewport?.addEventListener("keydown", onKeydown);
      viewport?.addEventListener("keydown", onCalcKeydown);
      viewport?.addEventListener("scroll", () => closeMenu(false));
      document.getElementById("pane-sheet")?.addEventListener("keydown", onCalcKeydown);
      document.addEventListener("keydown", onCalcKeydown);
      if (typeof host.addEventListener === "function") host.addEventListener("keydown", onCalcKeydown);
      const el = menu();
      el?.addEventListener("keydown", onMenuKeydown);
      el?.addEventListener("contextmenu", (ev) => ev.preventDefault());
      document.addEventListener("mousedown", (ev) => {
        if (el && !el.hidden && !el.contains(ev.target)) closeMenu(false);
      });
      document.addEventListener("keydown", (ev) => {
        if (ev.key === "Escape" && el && !el.hidden) {
          ev.preventDefault();
          ev.stopPropagation?.();
          closeMenu(true);
        }
      });
      host.addEventListener?.("resize", () => closeMenu(false));
      host.addEventListener?.("blur", () => closeMenu(false));
      for (const item of document.querySelectorAll(".md-item[data-sheet-act]")) {
        item.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          closeMenu(true);
          if (typeof globalThis.__EASYOFFICE_CLOSE_MENUS__ === "function") {
            globalThis.__EASYOFFICE_CLOSE_MENUS__(document);
          } else {
            document.querySelectorAll(".menu-item.open").forEach((open) => {
              open.classList.remove("open");
              open.querySelector?.(".menu-title")?.setAttribute?.("aria-expanded", "false");
            });
          }
          if (item.getAttribute("aria-disabled") === "true") return;
          void run(item.dataset.sheetAct);
        });
      }
      host.__EASYOFFICE_SHEET_CALC_OPTIONS__?.syncMenu(document);
    }

    function syncLayers() {
      mediaUi?.syncLayer();
      noteUi?.syncLayer();
    }

    // What follows the active sheet: the filter menu's check, the defined names submenu, and the outline bars.
    function refreshSheetChrome() {
      protectUi.refreshMenu();
      filterUi.refreshMenu();
      noteUi?.refreshMenu();
      namesUi.refreshUseMenu();
      host.__EASYOFFICE_SHEET_CALC_OPTIONS__?.syncMenu(document);
      outlineUi.render();
      tableUi.sync();
      viewOptions?.refresh();
      syncLayers();
    }

    // The text of a cell was typed: it may become a link, a table next to it grows over it, and a calculated column fills.
    function autoLink(sheet, ref, replace) {
      const linked = linkUi.autoLink(sheet, ref, replace);
      void tableUi.afterCellInput(sheet, ref);
      return linked;
    }

    return { run, bind, closeMenu, refreshSheetChrome, syncLayers, openHyperlink: () => linkUi.open(), autoLink, expandAfter: tableUi.expandAfter };
  }

  host.__EASYOFFICE_CREATE_SHEET_STRUCTURE__ = createSheetStructure;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
