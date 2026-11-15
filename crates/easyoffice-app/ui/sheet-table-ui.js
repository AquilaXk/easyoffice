(function (host) {
  "use strict";

  // The table commands of the sheet window (PD-231): the 표 만들기 dialog (the insert menu, the toolbar and Ctrl+T), the 표 디자인 bar that
  // shows while the active cell is in a table (the table's name, 표 크기 조정, 범위로 변환, the options of the style, the style
  // gallery) and the growth of a table over what is typed under or beside it. The plans are `sheet-table-ops.js`; each command is
  // ONE undoable step of the layout (and of the cells it writes), refused on a protected sheet.

  const STATUS = {
    created: (name) => "표 만들기: " + name,
    renamed: (name) => "표 이름: " + name,
    resized: "표 크기 조정",
    toRange: "범위로 변환",
    expanded: "표 자동 확장",
    style: "표 스타일",
    option: "표 스타일 옵션",
    header: "머리글 행",
    totals: "요약 행",
    sheetEnd: "시트의 끝에는 행을 넣을 수 없음",
    noHeaderRoom: "머리글 행을 넣을 수 없음",
    tableName: { empty: "표 이름을 입력", tooLong: "이름은 255자까지", name: "표 이름이 올바르지 않음", reserved: "함수나 시트 이름은 쓸 수 없음", duplicate: "이미 있는 이름", limit: "이름은 10,000개까지", unchecked: "이름을 확인하지 못함" },
  };
  const TOTAL_LABEL = "요약";
  const SUM = 109;
  const COUNT = 103;
  const NUMERIC = /^\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*$/;
  const OPTIONS = {
    header: (t) => t.header !== false,
    totals: (t) => t.totals === true,
    rowStripes: (t) => t.rowStripes === true,
    firstColumn: (t) => t.firstColumn === true,
    lastColumn: (t) => t.lastColumn === true,
    columnStripes: (t) => t.columnStripes === true,
    filterButton: (t) => Array.isArray(t.filter),
  };
  const MAX_EXPAND_STEPS = 100000;

  function createSheetTableUi(deps) {
    const {
      document,
      kit,
      sheetDataStore,
      sheetStoreHelpers,
      grid,
      getActiveSheetName,
      getSelection,
      getRange,
      getExtent,
      selectRange,
      commitIfEditing,
      focusAnchor,
      snapshotDomSheetCells,
      refreshWindowFromModel,
      recalculateActiveSheetFormulas,
      refreshChrome,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
      editAxis,
      filter,
    } = deps;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const styleApi = host.__EASYOFFICE_SHEET_TABLE_STYLE__;
    const ops = host.__EASYOFFICE_SHEET_TABLE_OPS__;
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    const namesApi = host.__EASYOFFICE_SHEET_NAMES__;
    const refOf = host.__EASYOFFICE_SHEET_REFS__.refOf;
    const { $, setChecked, openDialog, closeDialog, note } = kit;
    const state = { mode: "create", table: null, bound: false };
    let gallery = null;

    const say = (text) => {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    };

    // ---- what the plans read ------------------------------------------------------------------------------------------
    function contextOf(name, tables = layoutApi.tablesOf(name)) {
      const layout = layoutApi.get(name);
      const map = sheetDataStore[name] || {};
      return {
        tables,
        merges: layout?.merges || [],
        filter: layout?.filter?.range || null,
        cellAt: (row, col) => map[refOf(row, col)],
        boxOf: (table) => layoutApi.tableBox(table),
      };
    }

    const everyTable = () => Object.keys(layoutApi.getAll()).flatMap((sheet) => layoutApi.tablesOf(sheet));
    const columnTotal = () => everyTable().reduce((n, t) => n + t.columns.length, 0);
    const usedNames = () => [...everyTable().map((t) => t.name), ...namesApi.getAll().map((d) => d.name)];

    // The table that has the active cell of the active sheet.
    function tableAtActive() {
      const sel = getSelection();
      return styleApi.tableAt(getActiveSheetName(), sel.ar, sel.ac);
    }

    // The sheet may be changed: a protected sheet answers with its message (PD-216).
    const allowed = (name) => !protectApi || protectApi.guardUnprotected(name);

    // Writes header (or label) texts into the model, keeping each cell's format and link.
    function writeCells(name, writes) {
      const map = sheetDataStore[name] || (sheetDataStore[name] = {});
      for (const w of writes) {
        const ref = refOf(w.row, w.col);
        const prev = map[ref];
        const next = { ...prev, value: w.text ?? "", formula: w.formula ?? "" };
        if (next.value === "" && next.formula === "" && !next.style && !next.link) delete map[ref];
        else map[ref] = next;
      }
    }

    const withTable = (name, replaced, next) => layoutApi.tablesOf(name).map((t) => (t === replaced ? next : t));

    // The last step of a command that changed the layout or the cells: the grid, the formulas, the bar.
    async function settle(name, text, recalc = true) {
      grid.layoutChanged();
      refreshWindowFromModel();
      if (recalc) await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
      refreshChrome();
      sync();
      if (text) say(text);
    }

    // ---- 표 만들기 ---------------------------------------------------------------------------------------------------------
    const absolute = (box) => ops.rangeText(box.r1, box.c1, box.r2, box.c2).replace(/([A-Z]+)(\d+)/g, "$$$1$$$2");

    function initialBox(name) {
      const map = sheetDataStore[name] || {};
      const e = getExtent();
      const has = (r, c) => {
        const entry = map[refOf(r, c)];
        return Boolean(entry && (entry.value || entry.formula));
      };
      const sel = getSelection();
      const filterCore = host.__EASYOFFICE_SHEET_FILTER_CORE__;
      const extent = { rows: e.dataRows, cols: e.dataCols };
      return sel.axis ? filterCore.rangeOfAxisSelection(sel, extent) : filterCore.regionFrom(getRange(), has, extent);
    }

    function openCreate() {
      commitIfEditing();
      const name = getActiveSheetName();
      if (!allowed(name)) return false;
      sheetDataStore[name] = snapshotDomSheetCells();
      const box = initialBox(name);
      const map = sheetDataStore[name] || {};
      const sort = host.__EASYOFFICE_SHEET_SORT_CORE__;
      state.mode = "create";
      state.table = null;
      $("sheet-table-dialog-title").textContent = "표 만들기";
      $("sheet-table-range-label").textContent = "표에 사용할 데이터";
      $("sheet-table-header-row").hidden = false;
      $("sheet-table-range").value = absolute(box);
      setChecked("sheet-table-header", box.r2 > box.r1 && sort.detectHeader(map, box, refOf, sort.numberOf));
      note("sheet-table-status", "");
      openDialog("sheet-table-dialog");
      const input = $("sheet-table-range");
      input.focus?.();
      input.select?.();
      return true;
    }

    // The plan of a create (or resize) from the dialog; the error is shown in the dialog.
    function planFromDialog() {
      const name = getActiveSheetName();
      const box = ops.parseRange($("sheet-table-range").value, name);
      if (box === "sheet" || !box) {
        note("sheet-table-status", box === "sheet" ? ops.MESSAGES.sheet : ops.MESSAGES.range);
        return null;
      }
      sheetDataStore[name] = snapshotDomSheetCells();
      const ctx = contextOf(name);
      const plan =
        state.mode === "create"
          ? ops.planCreate({ box, header: kit.checked("sheet-table-header"), ctx, totalTables: everyTable().length, totalColumns: columnTotal(), takenNames: usedNames() })
          : ops.planResize({ table: state.table, box, ctx, totalColumns: columnTotal() });
      if (plan.error) {
        note("sheet-table-status", plan.error);
        return null;
      }
      return plan;
    }

    // The table the plan makes joins the layout inside the step that wrote its header cells.
    function addTable(name, plan) {
      writeCells(name, plan.headerWrites);
      layoutApi.setTables(name, [...layoutApi.tablesOf(name), plan.table]);
    }

    function selectTable(table) {
      const b = layoutApi.tableBox(table);
      selectRange({ r1: b.r1, c1: b.c1, r2: b.r2, c2: b.c2 });
    }

    function runCreate(plan) {
      const name = getActiveSheetName();
      if (plan.insertRow) {
        // The row for the header opens above the range (the rows below move down) in the same step.
        return editAxis("row", "insert", plan.box.r1, 1, (sheet) => addTable(sheet, plan)).then((done) => {
          if (done) afterCreate(name, plan.table);
          return done;
        });
      }
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        sheetDataStore[name] = snapshotDomSheetCells();
        const change = sheetHistory.begin({ refs: plan.headerWrites.map((w) => refOf(w.row, w.col)), parts: ["sheetLayout"] });
        addTable(name, plan);
        sheetHistory.commit(change);
        await settle(name, null);
        afterCreate(name, plan.table);
        return true;
      });
    }

    function afterCreate(name, table) {
      const made = layoutApi.tablesOf(name).find((t) => t.name === table.name);
      if (made) selectTable(made);
      sync();
      say(STATUS.created(table.name));
      focusAnchor();
    }

    // ---- 표 크기 조정 ------------------------------------------------------------------------------------------------------
    function openResize() {
      commitIfEditing();
      const name = getActiveSheetName();
      const table = tableAtActive();
      if (!table || !allowed(name)) return false;
      state.mode = "resize";
      state.table = table;
      $("sheet-table-dialog-title").textContent = "표 크기 조정";
      $("sheet-table-range-label").textContent = "새 데이터 범위";
      $("sheet-table-header-row").hidden = true;
      const b = layoutApi.tableBox(table);
      $("sheet-table-range").value = absolute(b);
      note("sheet-table-status", "");
      openDialog("sheet-table-dialog");
      $("sheet-table-range").focus?.();
      return true;
    }

    // One step that replaces `table` of the active sheet by `next` and writes `writes`; `clear` empties cells (the totals row
    // that leaves). `change(table, ctx)` is asked inside the step and answers `{ table, writes?, clear?, text, recalc? }` or null.
    function changeTable(table, change) {
      const name = getActiveSheetName();
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        if (!allowed(name)) return false;
        sheetDataStore[name] = snapshotDomSheetCells();
        const held = layoutApi.tablesOf(name).find((t) => t.name === table.name);
        const result = held && change(held, contextOf(name));
        if (!result) return false;
        if (result.error) {
          say(result.error);
          return false;
        }
        const touched = [...(result.writes || []), ...(result.clear || [])].map((w) => refOf(w.row, w.col));
        const record = sheetHistory.begin({ refs: touched, parts: ["sheetLayout"] });
        writeCells(name, [...(result.clear || []).map((c) => ({ ...c, text: "" })), ...(result.writes || [])]);
        layoutApi.setTables(name, withTable(name, held, result.table));
        sheetHistory.commit(record);
        await settle(name, result.text, result.recalc !== false);
        if (result.reapply) await filter.reapplyAfterEdit(false);
        if (result.select) selectTable(result.table);
        return true;
      });
    }

    function runResize(plan) {
      const table = state.table;
      return changeTable(table, () => ({ table: plan.table, writes: plan.headerWrites, text: STATUS.resized, reapply: Array.isArray(plan.table.filter) && plan.table.filter.length > 0, select: true }));
    }

    function submit() {
      const plan = planFromDialog();
      if (!plan) return Promise.resolve(false);
      closeDialog("sheet-table-dialog");
      return state.mode === "create" ? runCreate(plan) : runResize(plan);
    }

    // ---- 표 이름 ----------------------------------------------------------------------------------------------------------
    // `null` when `to` may name `table`, else the message: no other table (anywhere) has the name, and the engine accepts it as
    // a name (syntax, reserved words, defined names).
    async function nameProblem(table, to) {
      if (to === "") return STATUS.tableName.empty;
      if (Array.from(to).length > 255) return STATUS.tableName.tooLong;
      if (to.includes("?")) return STATUS.tableName.name;
      if (everyTable().some((t) => t !== table && t.name.toLowerCase() === to.toLowerCase())) return STATUS.tableName.duplicate;
      try {
        const res = await sheetStoreHelpers.callSheetAction({
          sessionAction: "nameCheck",
          sheets: Object.keys(sheetDataStore),
          names: namesApi.getAll(),
          candidate: { name: to, refersTo: "=1" },
          replace: null,
        });
        if (res && "problem" in res) return res.problem === null ? null : STATUS.tableName[res.problem] || STATUS.tableName.name;
      } catch {
        // falls through
      }
      return STATUS.tableName.unchecked;
    }

    // The formulas of every sheet that may name a table called `name`, as the engine's request wants them.
    function formulasNaming(name) {
      const lower = name.toLowerCase();
      const out = [];
      for (const [sheet, map] of Object.entries(sheetDataStore)) {
        for (const [cell, held] of Object.entries(map || {})) {
          const text = held?.formula;
          if (text && (text.includes("[") || text.toLowerCase().includes(lower))) out.push({ sheet, cell, text });
        }
      }
      return out;
    }

    function tablesPayload() {
      const spill = host.__EASYOFFICE_SHEET_SPILL__;
      const out = {};
      for (const sheet of Object.keys(layoutApi.getAll())) {
        const list = spill.blockerPayload("tables", sheet, layoutApi);
        if (list.length > 0) out[sheet] = list;
      }
      return out;
    }

    // The engine writes the edit into the formulas and the names that name the table; null when it cannot.
    async function rewritesFor(extra) {
      try {
        const answer = await sheetStoreHelpers.callSheetAction({ sessionAction: "tableColumn", tables: tablesPayload(), names: namesApi.getAll(), ...extra });
        return Array.isArray(answer?.rewrites) ? answer : null;
      } catch {
        return null;
      }
    }

    // Applies the rewrites to the model; the cells they touch, by sheet, are what the step keeps.
    function refsOf(rewrites) {
      const by = {};
      for (const w of rewrites) (by[w.sheet] ||= []).push(w.cell);
      return by;
    }

    function applyRewrites(answer) {
      for (const w of answer.rewrites) {
        const held = sheetDataStore[w.sheet]?.[w.cell];
        if (held) held.formula = w.text;
      }
      if (answer.names) namesApi.replaceAll(answer.names, true);
    }

    async function renameTable(table, to) {
      const name = getActiveSheetName();
      if (to === table.name) return false;
      if (!allowed(name)) return false;
      const problem = await nameProblem(table, to);
      if (problem) {
        say(problem);
        return false;
      }
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        sheetDataStore[name] = snapshotDomSheetCells();
        const held = layoutApi.tablesOf(name).find((t) => t.name === table.name);
        if (!held) return false;
        const answer = await rewritesFor({ op: "rename", table: held.name, to, formulas: formulasNaming(held.name) });
        if (!answer) {
          say(STATUS.tableName.unchecked);
          return false;
        }
        const record = sheetHistory.begin({ bySheet: refsOf(answer.rewrites), parts: ["sheetLayout", "definedNames"] });
        applyRewrites(answer);
        layoutApi.setTables(name, withTable(name, held, { ...held, name: to }));
        sheetHistory.commit(record);
        await settle(name, STATUS.renamed(to));
        return true;
      });
    }

    // ---- 범위로 변환 -------------------------------------------------------------------------------------------------------
    function convertToRange(table) {
      const name = getActiveSheetName();
      if (!allowed(name)) return Promise.resolve(false);
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        sheetDataStore[name] = snapshotDomSheetCells();
        const held = layoutApi.tablesOf(name).find((t) => t.name === table.name);
        if (!held) return false;
        const answer = await rewritesFor({ op: "range", table: held.name, formulas: formulasNaming(held.name) });
        if (!answer) {
          say("범위로 변환하지 못함");
          return false;
        }
        const b = layoutApi.tableBox(held);
        const record = sheetHistory.begin({ bySheet: refsOf(answer.rewrites), parts: ["sheetLayout", "definedNames"] });
        applyRewrites(answer);
        layoutApi.setTables(name, layoutApi.tablesOf(name).filter((t) => t !== held));
        sheetHistory.commit(record);
        await settle(name, STATUS.toRange);
        selectRange({ r1: b.r1, c1: b.c1, r2: b.r2, c2: b.c2 });
        // The rows the table's own filter hid show again.
        return true;
      });
    }

    // ---- the options of the style, the style ---------------------------------------------------------------------------------
    function toggleOption(table, key, on) {
      if (key === "header") return toggleHeader(table, on);
      if (key === "totals") return toggleTotals(table, on);
      if (key === "filterButton") return filter.setTableButtons(table.name, on);
      return changeTable(table, (held) => ({ table: { ...held, [key]: on }, text: STATUS.option, recalc: false }));
    }

    const rowEmpty = (ctx, row, c1, c2) => {
      for (let c = c1; c <= c2; c += 1) {
        const entry = ctx.cellAt(row, c);
        if (entry && (entry.value || entry.formula)) return false;
      }
      return true;
    };

    // Turning the header row off takes the first row out of the table (its cells stay as they are) and removes the filter, as
    // the spreadsheet program does; turning it on writes the column names into the row above, or opens a row for them.
    function toggleHeader(table, on) {
      if (!on) {
        return changeTable(table, (held, ctx) => {
          const b = ctx.boxOf(held);
          if (b.r2 - b.r1 < 1 + (held.totals ? 1 : 0)) return { error: ops.MESSAGES.small };
          const next = { ...held, header: false, range: ops.rangeText(b.r1 + 1, b.c1, b.r2, b.c2) };
          delete next.filter;
          delete next.filterHidden;
          return { table: next, text: STATUS.header };
        });
      }
      const name = getActiveSheetName();
      const ctx = contextOf(name);
      const b = ctx.boxOf(table);
      const above = { r1: b.r1 - 1, c1: b.c1, r2: b.r1 - 1, c2: b.c2 };
      // The row above takes the header when it holds nothing or only the names the columns have (the header cells a turn-off left),
      // and no other table, merge or filter is there.
      // A formula there is content even when its result is empty.
      const fits = b.r1 > 1 && table.columns.every((name, i) => {
        const entry = ctx.cellAt(above.r1, b.c1 + i);
        return !entry?.formula && (!entry?.value || entry.value === name);
      });
      const room = fits && !ctx.tables.some((t) => t !== table && touchesBox(layoutApi.tableBox(t), above)) && !ctx.merges.some((m) => touchesBox(m, above)) && !(ctx.filter && touchesBox(ctx.filter, above));
      // The header goes into `row`, the table's range grows up to it.
      const headerOf = (held, row) => {
        const now = layoutApi.tableBox(held);
        return {
          table: { ...held, header: true, filter: [], range: ops.rangeText(row, now.c1, now.r2, now.c2) },
          writes: held.columns.map((text, i) => ({ row, col: now.c1 + i, text })),
          text: STATUS.header,
        };
      };
      if (room) return changeTable(table, (held) => headerOf(held, above.r1));
      // No room above: a row opens at the table's first row (the table moves down) and the header goes into it.
      if (!allowed(name)) return Promise.resolve(false);
      return editAxis("row", "insert", b.r1, 1, (sheet) => {
        const held = layoutApi.tablesOf(sheet).find((t) => t.name === table.name);
        if (!held) return;
        const made = headerOf(held, layoutApi.tableBox(held).r1 - 1);
        writeCells(sheet, made.writes);
        layoutApi.setTables(sheet, withTable(sheet, held, made.table));
      }).then((done) => {
        if (done) {
          sync();
          say(STATUS.header);
        }
        return done;
      });
    }

    // The label and the function of the last column of a new totals row: a sum when the data of the last column are numbers,
    // else a count of its cells (`SUBTOTAL`, which the filter's hidden rows do not count).
    function totalsCells(held, b, ctx) {
      const last = b.c2;
      const first = b.r1 + (held.header === false ? 0 : 1);
      let numbers = 0;
      let filled = 0;
      for (let r = first; r <= b.r2; r += 1) {
        const text = ctx.cellAt(r, last)?.value;
        if (text !== undefined && text !== "") {
          filled += 1;
          if (NUMERIC.test(String(text))) numbers += 1;
        }
      }
      const column = held.columns[held.columns.length - 1];
      const special = /[^\p{L}\p{N}_ ]/u.test(column) ? "[" + escapeColumn(column) + "]" : escapeColumn(column);
      const code = filled > 0 && numbers === filled ? SUM : COUNT;
      const cells = [{ row: b.r2 + 1, col: last, text: "", formula: "=SUBTOTAL(" + code + "," + held.name + "[" + special + "])" }];
      if (held.columns.length > 1) cells.unshift({ row: b.r2 + 1, col: b.c1, text: TOTAL_LABEL });
      return cells;
    }

    const escapeColumn = (text) => text.replace(/(['\[\]#])/g, "'$1");

    // Turning the totals row on puts a row under the table (its cells when they are empty, else a row opens there); turning it
    // off takes the last row out of the table and empties it.
    function toggleTotals(table, on) {
      if (!on) {
        return changeTable(table, (held, ctx) => {
          const b = ctx.boxOf(held);
          if (b.r2 - b.r1 < 1 + (held.header === false ? 0 : 1)) return { error: ops.MESSAGES.small };
          const clear = [];
          for (let c = b.c1; c <= b.c2; c += 1) clear.push({ row: b.r2, col: c });
          const next = { ...held, totals: false, range: ops.rangeText(b.r1, b.c1, b.r2 - 1, b.c2) };
          return { table: next, clear, text: STATUS.totals };
        });
      }
      const name = getActiveSheetName();
      const ctx = contextOf(name);
      const b = ctx.boxOf(table);
      const below = { r1: b.r2 + 1, c1: b.c1, r2: b.r2 + 1, c2: b.c2 };
      const free = rowEmpty(ctx, below.r1, b.c1, b.c2) && !ctx.tables.some((t) => t !== table && touchesBox(layoutApi.tableBox(t), below)) && !ctx.merges.some((m) => touchesBox(m, below)) && !(ctx.filter && touchesBox(ctx.filter, below));
      const totalsOf = (held) => ({ table: { ...held, totals: true, range: ops.rangeText(b.r1, b.c1, b.r2 + 1, b.c2) }, writes: totalsCells(held, b, ctx), text: STATUS.totals });
      if (b.r2 >= 1048576) {
        say(STATUS.sheetEnd);
        return Promise.resolve(false);
      }
      if (free) return changeTable(table, (held) => totalsOf(held));
      if (!allowed(name)) return Promise.resolve(false);
      return editAxis("row", "insert", b.r2 + 1, 1, (sheet) => {
        const held = layoutApi.tablesOf(sheet).find((t) => t.name === table.name);
        if (!held) return;
        const made = totalsOf(held);
        writeCells(sheet, made.writes.map((w) => ({ ...w, formula: w.formula ?? "" })));
        layoutApi.setTables(sheet, withTable(sheet, held, made.table));
      }).then((done) => {
        if (done) {
          sync();
          say(STATUS.totals);
        }
        return done;
      });
    }

    const touchesBox = (a, c) => a.r1 <= c.r2 && a.r2 >= c.r1 && a.c1 <= c.c2 && a.c2 >= c.c1;

    // The style the gallery chose: a built-in one by its name, or one the file defined (its definition goes along).
    function setStyle(table, name, def) {
      return changeTable(table, (held) => {
        const next = { ...held };
        if (name) next.style = name;
        else delete next.style;
        if (name && def) next.styleDef = structuredClone(def);
        else delete next.styleDef;
        return { table: next, text: STATUS.style, recalc: false };
      });
    }

    // The styles of the workbook's tables that are not built in, by name.
    function customStyles() {
      const out = {};
      for (const t of everyTable()) if (t.style && t.styleDef && !styleApi.isBuiltIn(t.style) && !out[t.style]) out[t.style] = t.styleDef;
      return out;
    }

    // ---- growth over what is typed -----------------------------------------------------------------------------------------
    // The cells `refs` of `sheet` were just given content: each table of the sheet with such a cell right under or beside it
    // takes it in (a row or a column at a time, as far as the cells go). The growth is a step of its own, so one undo takes it
    // back and leaves what was typed.
    function expandAfter(sheet, refs) {
      if (sheet !== getActiveSheetName() || layoutApi.tablesOf(sheet).length === 0 || (protectApi && protectApi.isOn(sheet))) return Promise.resolve(false);
      const cells = refs.map((ref) => host.__EASYOFFICE_SHEET_GRID_UTILS__.parseRef(ref)).filter(Boolean);
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        sheetDataStore[sheet] = snapshotDomSheetCells();
        const grown = growth(sheet, cells);
        if (!grown) return false;
        const record = sheetHistory.begin({ refs: grown.writes.map((w) => refOf(w.row, w.col)), parts: ["sheetLayout"] });
        writeCells(sheet, grown.writes);
        layoutApi.setTables(sheet, grown.tables);
        sheetHistory.commit(record);
        await settle(sheet, STATUS.expanded);
        if (grown.reapply) await filter.reapplyAfterEdit(false);
        return true;
      });
    }

    // The tables of `sheet` after every growth the cells call for, with the header cells to write; null when none grows. A cell
    // is only looked at against the tables that end in the row above it or the column before it (a table by the row under it and
    // the column beside it), so many cells and many tables cost one lookup per cell; the cells go in order of row and column, so
    // a pasted block grows a table row by row in one pass.
    function growth(sheet, cells) {
      const tables = [...layoutApi.tablesOf(sheet)];
      const at = new Map(tables.map((t, i) => [t, i]));
      const below = new Map();
      const beside = new Map();
      const put = (map, key, t) => {
        const held = map.get(key);
        if (held) held.add(t);
        else map.set(key, new Set([t]));
      };
      const link = (t) => {
        const b = layoutApi.tableBox(t);
        put(below, b.r2 + 1, t);
        put(beside, b.c2 + 1, t);
      };
      const unlink = (t) => {
        const b = layoutApi.tableBox(t);
        below.get(b.r2 + 1)?.delete(t);
        beside.get(b.c2 + 1)?.delete(t);
      };
      tables.forEach(link);
      const ctx = contextOf(sheet, tables);
      const writes = [];
      let columns = columnTotal();
      let steps = 0;
      const ordered = [...cells].sort((a, b) => a.row - b.row || a.col - b.col);
      let again = true;
      while (again && steps < MAX_EXPAND_STEPS) {
        again = false;
        for (const cell of ordered) {
          const candidates = [...(below.get(cell.row) || []), ...(beside.get(cell.col) || [])];
          if (candidates.length === 0) continue;
          const plan = ops.planExpand({ row: cell.row, col: cell.col, ctx, totalColumns: columns, candidates });
          if (!plan) continue;
          const source = candidates.find((t) => t.name === plan.table.name);
          const i = at.get(source);
          unlink(source);
          tables[i] = plan.table;
          at.delete(source);
          at.set(plan.table, i);
          link(plan.table);
          writes.push(...plan.headerWrites);
          columns += plan.to === "column" ? 1 : 0;
          steps += 1;
          again = true;
        }
      }
      if (steps === 0) return null;
      return { tables, writes, reapply: tables.some((t) => Array.isArray(t.filter) && t.filter.length > 0) };
    }

    // ---- the bar ----------------------------------------------------------------------------------------------------------
    function sync() {
      const bar = $("sheet-table-bar");
      if (!bar) return;
      // The 필터 menu item is checked while the active cell is in a table with buttons.
      filter.notify();
      const table = tableAtActive();
      bar.hidden = !table;
      if (!table) {
        gallery?.close();
        return;
      }
      const input = $("sheet-table-name");
      if (document.activeElement !== input) input.value = table.name;
      for (const box of bar.querySelectorAll("[data-table-opt]")) box.checked = OPTIONS[box.dataset.tableOpt](table);
      const locked = Boolean(protectApi && protectApi.isOn(getActiveSheetName()));
      for (const el of bar.querySelectorAll("input, button")) el.disabled = locked;
      // Without a header row there is nothing to carry the buttons.
      const buttons = bar.querySelector('[data-table-opt="filterButton"]');
      if (buttons && table.header === false) buttons.disabled = true;
    }

    function openGallery() {
      const table = tableAtActive();
      if (!table) return;
      const options = Object.fromEntries(Object.keys(OPTIONS).filter((key) => key !== "filterButton").map((key) => [key, OPTIONS[key](table)]));
      gallery.open($("btn-table-style"), table.style || null, options, customStyles());
    }

    function bind() {
      if (state.bound) return;
      state.bound = true;
      gallery = host.__EASYOFFICE_CREATE_SHEET_TABLE_GALLERY__({
        document,
        onPick: (name, def) => {
          const table = tableAtActive();
          if (table) void setStyle(table, name, def);
        },
        onClear: () => {
          const table = tableAtActive();
          if (table) void setStyle(table, null, null);
        },
      });
      gallery.bind();
      $("btn-sheet-table-ok")?.addEventListener("click", () => void submit());
      $("sheet-table-range")?.addEventListener("keydown", (ev) => {
        if (ev.key !== "Enter") return;
        ev.preventDefault();
        void submit();
      });
      $("btn-table-style")?.addEventListener("click", () => (gallery.isOpen() ? gallery.close() : openGallery()));
      const bar = $("sheet-table-bar");
      bar?.addEventListener("change", (ev) => {
        const box = ev.target?.closest?.("[data-table-opt]");
        const table = tableAtActive();
        if (box && table) void toggleOption(table, box.dataset.tableOpt, box.checked);
      });
      bar?.addEventListener("click", (ev) => {
        const button = ev.target?.closest?.("[data-table-act]");
        const table = tableAtActive();
        if (!button || !table) return;
        if (button.dataset.tableAct === "resize") openResize();
        else if (button.dataset.tableAct === "to-range") void convertToRange(table);
      });
      const input = $("sheet-table-name");
      input?.addEventListener("keydown", (ev) => {
        const table = tableAtActive();
        if (ev.key === "Enter" && table) {
          ev.preventDefault();
          const to = input.value.trim();
          void renameTable(table, to).then((done) => {
            if (!done) input.value = tableAtActive()?.name ?? "";
            focusAnchor();
          });
        } else if (ev.key === "Escape") {
          ev.preventDefault();
          input.value = table?.name ?? "";
          focusAnchor();
        }
      });
      input?.addEventListener("blur", () => {
        const table = tableAtActive();
        if (!table || input.value.trim() === table.name) return;
        const to = input.value.trim();
        void renameTable(table, to).then((done) => {
          if (!done) input.value = tableAtActive()?.name ?? "";
        });
      });
      layoutApi.onChange(sync);
      host.__EASYOFFICE_SHEET_TABLE_BAR__ = { sync, expandAfter, growth };
    }

    return {
      bind,
      sync,
      expandAfter,
      actions: { "table-create": () => Promise.resolve(openCreate()), "table-resize": () => Promise.resolve(openResize()) },
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_TABLE_UI__ = createSheetTableUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
