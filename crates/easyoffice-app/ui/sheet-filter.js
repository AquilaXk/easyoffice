// Auto filter of a sheet (PD-198): the filter range, the condition of each column and the rows
// the conditions hide. The engine judges the cells (`filter` session action: `apply` for the
// hidden rows, `values` for the list a column offers); this module builds the requests from the
// sheet's cells, keeps the filter in the sheet layout (so undo, open and save carry it) and
// runs each change as ONE undoable step.
//
// 참조 구현: Univer `packages/sheets-filter/src/commands/commands/sheets-filter.command.ts` lines
// 148-190 (`SmartToggleSheetsFilterCommand`: a single cell takes the region around it, a single
// row takes the rows below it, a selection is used as it is), 305-322 (`ReCalcSheetsFilterCommand`:
// re-applying is on request and needs no undo step) and `packages/sheets-filter-ui/src/models/
// conditions.ts` lines 79-727 (`FilterConditionItems`: how a condition in the dialog maps to the
// column's `customFilters`, wildcards for contains, starts with and ends with, a single space for
// "not empty"), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd., commit
// 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. Modifications: the form is a pair of rows (an
// operator and a text each) joined by AND or OR, "between" is not a separate operator, and the
// filter's requests carry the displayed text of the cells instead of the reference's worker.
(function (host) {
  "use strict";

  const MAX_FILTER_ROWS = 200000; // rows of a range, header row included
  const MAX_FILTER_VALUES = 10000; // values a column's list holds
  const MAX_FILTER_TEXT = 255; // characters of a condition's text
  const OPERATORS = [
    // [id, label, parameters]
    ["none", "없음", 0],
    ["equals", "같음", 1],
    ["notEquals", "같지 않음", 1],
    ["greaterThan", "큼", 1],
    ["greaterThanOrEqual", "크거나 같음", 1],
    ["lessThan", "작음", 1],
    ["lessThanOrEqual", "작거나 같음", 1],
    ["contains", "포함", 1],
    ["notContains", "포함하지 않음", 1],
    ["startsWith", "시작", 1],
    ["endsWith", "끝", 1],
    ["empty", "비어 있음", 0],
    ["notEmpty", "비어 있지 않음", 0],
  ];
  const NUMERIC_OPS = new Set(["greaterThan", "greaterThanOrEqual", "lessThan", "lessThanOrEqual"]);

  // ---- a condition in the dialog <-> the engine's custom condition ---------------------------------
  // The form is `{ op, text }`; the condition is `{ op?, text }` (no `op` is the wildcard text match).
  function toCondition(form) {
    const text = String(form.text ?? "");
    switch (form.op) {
      case "equals":
        return { text };
      case "notEquals":
        return { op: "notEqual", text };
      case "contains":
        return { text: "*" + text + "*" };
      case "notContains":
        return { op: "notEqual", text: "*" + text + "*" };
      case "startsWith":
        return { text: text + "*" };
      case "endsWith":
        return { text: "*" + text };
      case "empty":
        return { text: "" };
      case "notEmpty":
        return { op: "notEqual", text: " " };
      default:
        return NUMERIC_OPS.has(form.op) ? { op: form.op, text } : null;
    }
  }

  // The form a condition came from (the reverse of `toCondition`).
  function toForm(condition) {
    const { op, text } = condition;
    if (op === "notEqual") {
      if (text === " ") return { op: "notEmpty", text: "" };
      if (text.length >= 2 && text.startsWith("*") && text.endsWith("*")) {
        return { op: "notContains", text: text.slice(1, -1) };
      }
      return { op: "notEquals", text };
    }
    if (op === "equal") return { op: "equals", text };
    if (op) return { op, text };
    if (text === "") return { op: "empty", text: "" };
    if (text.length >= 2 && text.startsWith("*") && text.endsWith("*")) return { op: "contains", text: text.slice(1, -1) };
    if (text.endsWith("*") && !text.startsWith("*")) return { op: "startsWith", text: text.slice(0, -1) };
    if (text.startsWith("*") && !text.endsWith("*")) return { op: "endsWith", text: text.slice(1) };
    return { op: "equals", text };
  }

  const paramsOf = (op) => OPERATORS.find((o) => o[0] === op)?.[2] ?? 0;

  // `{ and, rows: [form, form?] }` -> the column's criteria, or an error text.
  function criteriaFromForms(and, forms) {
    const conditions = [];
    for (const form of forms) {
      if (!form || form.op === "none") continue;
      if (paramsOf(form.op) === 1 && String(form.text ?? "") === "") return { error: "값 필요" };
      const condition = toCondition(form);
      if (!condition) return { error: "조건 필요" };
      if (condition.text.length > MAX_FILTER_TEXT) return { error: MAX_FILTER_TEXT + "자까지" };
      conditions.push(condition);
    }
    if (conditions.length === 0) return { criteria: null };
    const criteria = { conditions };
    if (conditions.length === 2 && and) criteria.and = true;
    return { criteria };
  }

  // The two form rows of a column's custom criteria.
  function formsFromCriteria(criteria) {
    const rows = (criteria?.conditions || []).map(toForm);
    // A new condition (or a single one) joins with AND, as a spreadsheet's dialog starts.
    return { and: criteria?.conditions?.length === 2 ? criteria.and === true : true, rows: [rows[0] || { op: "none", text: "" }, rows[1] || { op: "none", text: "" }] };
  }

  // ---- the range a filter is created for --------------------------------------------------------
  // `has(r, c)` tells whether a cell holds something. A single cell takes the region around it
  // (blank rows and columns end it), a single row the rows below it, a block is used as it is.
  function regionFrom(sel, has, extent) {
    const { r1, c1, r2, c2 } = sel;
    if (r1 === r2 && c1 === c2) return expand({ r1, c1, r2, c2 }, has, extent, true);
    if (r1 === r2) return expand({ r1, c1, r2, c2 }, has, extent, false);
    return { r1, c1, r2, c2 };
  }

  function rowHas(has, r, c1, c2) {
    for (let c = c1; c <= c2; c += 1) if (has(r, c)) return true;
    return false;
  }

  function colHas(has, c, r1, r2) {
    for (let r = r1; r <= r2; r += 1) if (has(r, c)) return true;
    return false;
  }

  function expand(box, has, extent, sideways) {
    let { r1, c1, r2, c2 } = box;
    for (let changed = true; changed; ) {
      changed = false;
      if (sideways && c1 > 1 && colHas(has, c1 - 1, r1 - (r1 > 1 ? 1 : 0), r2 + 1)) {
        c1 -= 1;
        changed = true;
      }
      if (sideways && c2 < extent.cols && colHas(has, c2 + 1, r1 - (r1 > 1 ? 1 : 0), r2 + 1)) {
        c2 += 1;
        changed = true;
      }
      if (sideways && r1 > 1 && rowHas(has, r1 - 1, c1, c2)) {
        r1 -= 1;
        changed = true;
      }
      if (r2 < extent.rows && rowHas(has, r2 + 1, c1, c2)) {
        r2 += 1;
        changed = true;
      }
    }
    return { r1, c1, r2, c2 };
  }

  // The range a column selection or a row selection stands for.
  function rangeOfAxisSelection(sel, extent) {
    if (sel.axis === "col") return { r1: 1, c1: Math.min(sel.ac, sel.cc), r2: extent.rows, c2: Math.max(sel.ac, sel.cc) };
    return { r1: Math.min(sel.ar, sel.cr), c1: 1, r2: Math.max(sel.ar, sel.cr), c2: extent.cols };
  }

  host.__EASYOFFICE_SHEET_FILTER_CORE__ = {
    MAX_FILTER_ROWS,
    MAX_FILTER_VALUES,
    MAX_FILTER_TEXT,
    OPERATORS,
    NUMERIC_OPS,
    toCondition,
    toForm,
    paramsOf,
    criteriaFromForms,
    formsFromCriteria,
    regionFrom,
    rangeOfAxisSelection,
  };

  // ---- the commands ---------------------------------------------------------------------------------
  function createSheetFilter(deps) {
    const {
      document,
      sheetDataStore,
      sheetStoreHelpers,
      grid,
      gridUtils,
      getActiveSheetName,
      getRange,
      getSelection,
      getExtent,
      commitIfEditing,
      snapshotDomSheetCells,
      refreshWindowFromModel,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
      sortRange,
    } = deps;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const normalize = (raw) => host.__EASYOFFICE_SHEET_UTILS__.normalizeFormattedNumericCell(raw);
    const listeners = [];

    function say(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    const filterOf = (name) => layoutApi.filterOf(name);
    const active = () => filterOf(getActiveSheetName());
    const changed = () => listeners.forEach((fn) => fn());

    // The page's model of the filter as the engine's JSON.
    const jsonOf = (filter) => layoutApi.filterToJson(filter);

    // The stored text of the rows below the header in column `col`, with the number format runs.
    function columnData(map, filter, col) {
      const { r1, r2 } = filter.range;
      const name = gridUtils.colName(col);
      const values = new Array(r2 - r1);
      const fmt = [];
      let run = null;
      for (let r = r1 + 1, i = 0; r <= r2; r += 1, i += 1) {
        const entry = map[name + r];
        let value = entry?.value;
        value = value === undefined || value === null ? "" : String(value);
        const code = entry?.style?.numFmt || "";
        if (code && value !== "") value = normalize(value);
        values[i] = value;
        if (code && value !== "") {
          if (run && run[2] === code && run[1] === i - 1) run[1] = i;
          else {
            run = [i, i, code];
            fmt.push(run);
          }
        }
      }
      const item = { col, values };
      if (fmt.length) item.fmt = fmt;
      return item;
    }

    function requestFor(op, filter, extraCols, extra) {
      const map = sheetDataStore[getActiveSheetName()] || {};
      const cols = new Set(filter.columns.map((c) => c.col));
      for (const c of extraCols) cols.add(c);
      return {
        sessionAction: "filter",
        op,
        filter: jsonOf(filter),
        data: [...cols].sort((a, b) => a - b).map((c) => columnData(map, filter, c)),
        ...extra,
      };
    }

    // Reads the cells of the active sheet into the model, as the structure commands do.
    function syncCells() {
      commitIfEditing();
      const name = getActiveSheetName();
      sheetDataStore[name] = snapshotDomSheetCells();
      return name;
    }

    // Asks the engine which rows the conditions hide; `null` when it cannot say.
    async function evaluate(filter) {
      if (filter.columns.length === 0) return { hidden: [], count: 0 };
      try {
        const answer = await sheetStoreHelpers.callSheetAction(requestFor("apply", filter, [], {}));
        return Array.isArray(answer?.hidden) ? answer : null;
      } catch {
        return null;
      }
    }

    const dataRows = (filter) => filter.range.r2 - filter.range.r1;

    function summary(filter, count) {
      const total = dataRows(filter);
      return "필터: " + total + "행 중 " + (total - count) + "행 표시";
    }

    // Applies `next` (the range and conditions) to the sheet: the engine's hidden rows replace
    // the filter's. Returns whether it worked.
    async function install(name, next) {
      const answer = await evaluate(next);
      if (!answer) {
        say("필터를 적용하지 못함");
        return false;
      }
      layoutApi.setFilter(name, { ...next, hidden: answer.hidden });
      grid.layoutChanged();
      refreshWindowFromModel();
      say(summary(next, answer.count));
      changed();
      return true;
    }

    // One undoable change of the filter of the active sheet; `cells` when the change moves cells (a
    // sort), else the undo step leaves the cells out.
    function change(work, cells = false) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        const name = syncCells();
        const before = captureWorkspaceSnapshot(false, !cells);
        const done = await work(name);
        if (done) pushRawUndoSnapshot(before);
        return done;
      });
    }

    function newFilterRange(name) {
      const map = sheetDataStore[name] || {};
      const e = getExtent();
      const extent = { rows: e.dataRows, cols: e.dataCols };
      const has = (r, c) => {
        const entry = map[gridUtils.refOf(r, c)];
        return Boolean(entry && (entry.value || entry.formula));
      };
      const sel = getSelection();
      const range = sel.axis ? rangeOfAxisSelection(sel, extent) : regionFrom(getRange(), has, extent);
      return range;
    }

    function toggle() {
      return change(async (name) => {
        if (filterOf(name)) {
          layoutApi.setFilter(name, null);
          grid.layoutChanged();
          refreshWindowFromModel();
          say("필터 해제");
          changed();
          return true;
        }
        const range = newFilterRange(name);
        if (range.r2 <= range.r1) {
          say("필터는 두 행 이상에서 켤 수 있음");
          return false;
        }
        if (range.r2 - range.r1 >= MAX_FILTER_ROWS) {
          say("필터 범위는 " + MAX_FILTER_ROWS + "행까지");
          return false;
        }
        layoutApi.setFilter(name, { range, columns: [], hidden: [] });
        grid.layoutChanged();
        refreshWindowFromModel();
        say("필터 켬");
        changed();
        return true;
      });
    }

    // Sets (or with null removes) the condition of column `col` and applies the filter.
    function setCriteria(col, criteria) {
      return change(async (name) => {
        const filter = filterOf(name);
        if (!filter) return false;
        const columns = filter.columns.filter((c) => c.col !== col);
        if (criteria) columns.push({ col, ...criteria });
        columns.sort((a, b) => a.col - b.col);
        return install(name, { range: filter.range, columns, hidden: [] });
      });
    }

    function clearAll() {
      return change(async (name) => {
        const filter = filterOf(name);
        if (!filter || (filter.columns.length === 0 && filter.hidden.length === 0)) {
          say("지울 필터 조건 없음");
          return false;
        }
        layoutApi.setFilter(name, { range: filter.range, columns: [], hidden: [] });
        grid.layoutChanged();
        refreshWindowFromModel();
        say("필터 지움");
        changed();
        return true;
      });
    }

    // Re-applies the conditions to the cells as they are now. No undo step (the reference's rule).
    function reapply() {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        const name = syncCells();
        const filter = filterOf(name);
        if (!filter) {
          say("켜진 필터 없음");
          return false;
        }
        return install(name, { range: filter.range, columns: filter.columns, hidden: [] });
      });
    }

    // After a row or column edit that dropped a column's condition (inside the edit's own step).
    async function reapplyAfterEdit() {
      const name = getActiveSheetName();
      const filter = filterOf(name);
      if (filter) await install(name, { range: filter.range, columns: filter.columns, hidden: [] });
    }

    // The values `col` offers among the rows the other columns leave visible; `null` on failure.
    async function values(col, query) {
      const name = syncCells();
      const filter = filterOf(name);
      if (!filter) return null;
      try {
        const answer = await sheetStoreHelpers.callSheetAction(requestFor("values", filter, [col], { col, query: query || "" }));
        return Array.isArray(answer?.items) ? answer : null;
      } catch {
        return null;
      }
    }

    // Sorts the data rows by column `col` (descending when `descending`) and applies the filter
    // again, because the rows moved.
    function sortBy(col, descending) {
      return change(async (name) => {
        const filter = filterOf(name);
        if (!filter) return false;
        const { r1, r2, c1, c2 } = filter.range;
        const sorted = await sortRange({ range: { r1: r1 + 1, r2, c1, c2, keyCol: col }, direction: descending ? -1 : 1 });
        if (!sorted) return false;
        sheetDataStore[name] = snapshotDomSheetCells();
        const answer = await evaluate({ range: filter.range, columns: filter.columns, hidden: [] });
        if (answer) layoutApi.setFilter(name, { range: filter.range, columns: filter.columns, hidden: answer.hidden });
        grid.layoutChanged();
        refreshWindowFromModel();
        say(descending ? "내림차순 정렬" : "오름차순 정렬");
        changed();
        return true;
      }, true);
    }

    function run(action) {
      if (action === "filter-toggle") return toggle();
      if (action === "filter-reapply") return reapply();
      if (action === "filter-clear") return clearAll();
      return Promise.resolve(false);
    }

    return {
      run,
      toggle,
      reapply,
      clearAll,
      setCriteria,
      reapplyAfterEdit,
      values,
      sortBy,
      current: active,
      onChange: (fn) => listeners.push(fn),
      say,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_FILTER__ = createSheetFilter;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
