// 참조 구현 (reference implementation): the criteria range of the spreadsheet programs' advanced filter, as the open-source
// spreadsheet suite's help ("Advanced Filter": the first row of the criteria names columns of the list, the rows below are
// alternatives (OR), the cells of one row all have to hold (AND)) and the Korean spreadsheet help (데이터 > 정렬 및 필터 > 고급)
// describe it. The comparison itself is not ported: each criteria row becomes the conditions of the auto filter's columns and
// the engine judges them (`sheet_filter_ipc.rs`, `easyoffice-sheet/src/filter`). The two spreadsheet engines that were searched for a
// ported condition evaluation (Univer, Apache-2.0, `packages/sheets-filter`; FortuneSheet, MIT, `packages/core/src/modules/filter.ts`)
// only filter by the values and the conditions of one column (their `FilterModel`/`filterColumn` rules), they have no criteria
// range, so nothing could be taken from them (see THIRD_PARTY_NOTICES.md, PD-400).
(function (host) {
  "use strict";

  // 데이터 > 정렬 및 필터 > 고급 of the sheet window (PD-400): the 고급 필터 dialog (현재 위치에 필터 / 다른 장소에 복사, 목록 범위, 조건 범위,
  // 복사 위치, 고유 레코드만) and the plan behind it.
  // Criteria: a plain text matches the cells that begin with it (a number matches by its value), `=text` the cells that equal it,
  // `<>`, `>`, `>=`, `<`, `<=` compare, `*` and `?` stand for any text and one character. A heading may stand twice in the criteria to
  // give a column two conditions (and); more conditions on one column, and a heading that is not in the list (a calculated
  // criterion), are refused, as the engine's custom condition holds two.
  const MAX_CRITERIA_ROWS = 64;
  const MAX_COPY_CELLS = 2000000;
  const MAX_CONDITIONS = 2;
  const MESSAGES = {
    list: "목록 범위가 올바르지 않습니다",
    criteria: "조건 범위가 올바르지 않습니다",
    target: "복사 위치가 올바르지 않습니다",
    rows: "목록에 데이터가 없습니다",
    heading: "조건 범위의 머리글이 목록에 없습니다",
    conditions: "한 열에는 조건을 둘까지 쓸 수 있습니다",
    many: "조건 행이 너무 많습니다",
    large: "목록이 너무 큽니다",
    overlap: "복사 위치가 목록 범위와 겹칩니다",
    bound: "복사 위치가 시트 범위를 벗어납니다",
    engine: "필터를 적용하지 못했습니다",
    cells: "복사할 칸이 너무 많습니다",
    protectedTarget: "보호된 칸에는 복사할 수 없습니다",
  };
  const OPERATORS = [[">=", "greaterThanOrEqual"], ["<=", "lessThanOrEqual"], ["<>", "notEqual"], [">", "greaterThan"], ["<", "lessThan"], ["=", null]];
  const NUMBER = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;

  // The condition a criteria cell stands for, `null` for an empty cell (it has no condition).
  function criterionOf(raw) {
    const text = String(raw ?? "");
    if (text === "") return null;
    for (const [mark, op] of OPERATORS) {
      if (!text.startsWith(mark)) continue;
      const rest = text.slice(mark.length);
      if (mark === "<>" && rest === "") return { op: "notEqual", text: " " };
      if (mark === "=") return { text: rest };
      return rest === "" ? { text } : { op, text: rest };
    }
    return NUMBER.test(text.trim()) ? { text } : { text: text + "*" };
  }

  const headingKey = (text) => String(text ?? "").trim().toLowerCase();

  // The condition sets of a criteria block: `lists` the list's headings, `criteria` the block (rows of texts, the headings first), `firstCol`
  // the sheet column of the list's first column. `{ groups }` (one array of `{ col, conditions, and? }` per criteria row, an empty one
  // for a row with no condition: it keeps every row) or `{ error }`.
  function plan(headings, criteria, firstCol) {
    if (criteria.length < 1 || headings.length === 0) return { error: "criteria" };
    if (criteria.length - 1 > MAX_CRITERIA_ROWS) return { error: "many" };
    const at = new Map();
    headings.forEach((text, i) => {
      if (headingKey(text) !== "" && !at.has(headingKey(text))) at.set(headingKey(text), i);
    });
    const own = criteria[0].map((text) => at.get(headingKey(text)));
    if (own.some((index) => index === undefined)) return { error: "heading" };
    const groups = [];
    for (const row of criteria.slice(1)) {
      const byCol = new Map();
      for (let j = 0; j < own.length; j += 1) {
        const condition = criterionOf(row[j]);
        if (!condition) continue;
        byCol.set(own[j], [...(byCol.get(own[j]) || []), condition]);
      }
      const columns = [];
      for (const [index, conditions] of [...byCol].sort((a, b) => a[0] - b[0])) {
        if (conditions.length > MAX_CONDITIONS) return { error: "conditions" };
        columns.push(conditions.length === 2 ? { col: firstCol + index, conditions, and: true } : { col: firstCol + index, conditions });
      }
      groups.push(columns);
    }
    // A criteria block with headings only keeps every row.
    return { groups: groups.length > 0 ? groups : [[]] };
  }

  // The rows (sheet rows `first..last`) that stay: a row stays when any of the criteria rows keeps it. `hiddenPerGroup[g]` are the runs
  // `[first, last]` that group `g` hides.
  function visibleRows(first, last, hiddenPerGroup) {
    const kept = new Uint8Array(last - first + 1);
    for (const runs of hiddenPerGroup) {
      const out = new Uint8Array(last - first + 1).fill(1);
      for (const [a, b] of runs) out.fill(0, Math.max(a, first) - first, Math.min(b, last) - first + 1);
      for (let i = 0; i < kept.length; i += 1) kept[i] |= out[i];
    }
    const rows = [];
    for (let i = 0; i < kept.length; i += 1) if (kept[i]) rows.push(first + i);
    return rows;
  }

  // The runs of rows `first..last` that `kept` (ascending rows) leaves out.
  function runsOutside(first, last, kept) {
    const runs = [];
    let next = first;
    for (const row of kept) {
      if (row > next) runs.push([next, row - 1]);
      next = row + 1;
    }
    if (next <= last) runs.push([next, last]);
    return runs;
  }

  // 고유 레코드만: the first of the rows that read the same (the texts compared without case, as the program does).
  function uniqueRows(rows, textsOf) {
    const seen = new Set();
    return rows.filter((row) => {
      const key = textsOf(row).map((t) => String(t).toLowerCase()).join("\u0001");
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }

  function createSheetAdvFilter(deps) {
    const { document, kit, say, getActiveSheetName, getRange, getExtent, snapshotDomSheetCells, commitIfEditing, applyCellEdits, grid, getFilter } = deps;
    const names = host.__EASYOFFICE_SHEET_NAMES__;
    const special = host.__EASYOFFICE_SHEET_SPECIAL_CELLS__;
    const refsApi = host.__EASYOFFICE_SHEET_REFS__;
    const filterCore = host.__EASYOFFICE_SHEET_FILTER_CORE__;
    const $ = (id) => document.getElementById(id);
    let bound = false;

    // An address of the active sheet in the dialog: `$A$1:$D$9` or `Sheet1!A1:D9`.
    function resolve(text) {
      const raw = String(text ?? "").trim();
      const bang = raw.lastIndexOf("!");
      if (bang >= 0) {
        const sheet = raw.slice(0, bang).replace(/^'|'$/g, "").replaceAll("''", "'");
        if (names.fold(sheet) !== names.fold(getActiveSheetName())) return null;
        return names.parseAddress(raw.slice(bang + 1));
      }
      return names.parseAddress(raw);
    }

    const absolute = (r) => "$" + refsApi.colName(r.c1) + "$" + r.r1 + (r.r1 === r.r2 && r.c1 === r.c2 ? "" : ":$" + refsApi.colName(r.c2) + "$" + r.r2);
    const textAt = (map, row, col) => String(map[refsApi.refOf(row, col)]?.value ?? "");

    // The list the command starts from: the selection, or the region around the active cell.
    function guessList() {
      const map = snapshotDomSheetCells();
      const rg = getRange();
      if (rg.r1 !== rg.r2 || rg.c1 !== rg.c2) return rg;
      const filled = (row, col) => {
        const entry = map[refsApi.refOf(row, col)];
        return Boolean(entry) && (Boolean(entry.formula) || String(entry.value ?? "") !== "");
      };
      return special.regionAround({ maxRows: grid.MAX_ROWS, maxCols: grid.MAX_COLS, isFilled: filled }, rg.r1, rg.c1);
    }

    function syncTarget() {
      const copy = kit.checked("sheet-advfilter-copy");
      const field = $("sheet-advfilter-target");
      if (field) field.disabled = !copy;
    }

    function open() {
      commitIfEditing();
      bound ||= bind();
      kit.setChecked("sheet-advfilter-inplace", true);
      kit.setChecked("sheet-advfilter-copy", false);
      kit.setVal("sheet-advfilter-list", absolute(guessList()));
      kit.setVal("sheet-advfilter-criteria", "");
      kit.setVal("sheet-advfilter-target", "");
      kit.setChecked("sheet-advfilter-unique", false);
      kit.note("sheet-advfilter-status", "");
      syncTarget();
      kit.openDialog("sheet-advfilter-dialog");
      return true;
    }

    function bind() {
      $("btn-sheet-advfilter-ok")?.addEventListener("click", () => void apply());
      for (const id of ["sheet-advfilter-inplace", "sheet-advfilter-copy"]) $(id)?.addEventListener("change", syncTarget);
      return true;
    }

    const fail = (key) => {
      kit.note("sheet-advfilter-status", MESSAGES[key]);
      return false;
    };

    async function apply() {
      const copy = kit.checked("sheet-advfilter-copy");
      const list = resolve(kit.val("sheet-advfilter-list"));
      if (!list) return fail("list");
      const block = resolve(kit.val("sheet-advfilter-criteria"));
      if (!block) return fail("criteria");
      if (list.r2 - list.r1 < 1) return fail("rows");
      if (list.r2 - list.r1 >= filterCore.MAX_FILTER_ROWS) return fail("large");
      const map = snapshotDomSheetCells();
      const headings = [];
      for (let c = list.c1; c <= list.c2; c += 1) headings.push(textAt(map, list.r1, c));
      const rows = [];
      for (let r = block.r1; r <= block.r2; r += 1) {
        const cells = [];
        for (let c = block.c1; c <= block.c2; c += 1) cells.push(textAt(map, r, c));
        rows.push(cells);
      }
      const made = plan(headings, rows, list.c1);
      if (made.error) return fail(made.error);
      const filter = getFilter();
      const hiddenPerGroup = [];
      for (const columns of made.groups) {
        const answer = columns.length === 0 ? { hidden: [] } : await filter.judge(list, columns);
        if (!answer) return fail("engine");
        hiddenPerGroup.push(answer.hidden);
      }
      let kept = visibleRows(list.r1 + 1, list.r2, hiddenPerGroup);
      if (kit.checked("sheet-advfilter-unique")) {
        kept = uniqueRows(kept, (row) => headings.map((_, i) => textAt(map, row, list.c1 + i)));
      }
      if (!copy) {
        kit.closeDialog("sheet-advfilter-dialog");
        // The rows the criteria leave out, and the duplicates of 고유 레코드만, go out of view.
        const done = await filter.showOnly(list, runsOutside(list.r1 + 1, list.r2, kept));
        say(done ? "고급 필터: " + kept.length + "행 표시" : MESSAGES.engine);
        return done;
      }
      const target = resolve(kit.val("sheet-advfilter-target"));
      if (!target) return fail("target");
      return copyOut(list, kept, target, map);
    }

    async function copyOut(list, kept, target, map) {
      const cols = list.c2 - list.c1 + 1;
      const total = kept.length + 1;
      if (total * cols > MAX_COPY_CELLS) return fail("cells");
      const area = { r1: target.r1, c1: target.c1, r2: target.r1 + total - 1, c2: target.c1 + cols - 1 };
      if (area.r2 > grid.MAX_ROWS || area.c2 > grid.MAX_COLS) return fail("bound");
      if (area.r1 <= list.r2 && area.r2 >= list.r1 && area.c1 <= list.c2 && area.c2 >= list.c1) return fail("overlap");
      const edits = [];
      [list.r1, ...kept].forEach((row, i) => {
        for (let j = 0; j < cols; j += 1) {
          const entry = map[refsApi.refOf(row, list.c1 + j)];
          const ref = refsApi.refOf(area.r1 + i, area.c1 + j);
          // The values and the formats are copied; a formula is written as the value it gives.
          edits.push(entry ? { ref, value: entry.value ?? "", formula: "", style: entry.style ?? null } : { ref, value: "", formula: "", style: null });
        }
      });
      kit.closeDialog("sheet-advfilter-dialog");
      const done = await applyCellEdits(edits, "고급 필터");
      if (!done) say(MESSAGES.protectedTarget);
      return done;
    }

    return { open, actions: { "adv-filter": () => Promise.resolve(open()) } };
  }

  host.__EASYOFFICE_SHEET_ADV_FILTER__ = { MESSAGES, criterionOf, plan, visibleRows, runsOutside, uniqueRows, MAX_CRITERIA_ROWS };
  host.__EASYOFFICE_CREATE_SHEET_ADV_FILTER__ = createSheetAdvFilter;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
