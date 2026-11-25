// 참조 구현 (reference implementation): IronCalc (https://github.com/ironcalc/IronCalc, MIT OR Apache-2.0, Copyright 2023-present
// IronCalc contributors, commit 6dc9e8bae4b8d066276f399de7b330b9): `base/src/functions/spill_functions.rs` lines 60-117 (`fn_unique`
// typed equality key). Source and license text: `THIRD_PARTY_NOTICES.md`.
// Modifications: column checklist dialog (`중복된 항목 제거`) driving `sheet-table-ops.js` `planDedupe` as a single undoable step.

(function (host) {
  "use strict";

  function createSheetTableDedupeUi(deps) {
    const {
      document,
      kit,
      sheetDataStore,
      getActiveSheetName,
      contextOf,
      allowed,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      commitIfEditing,
      snapshotDomSheetCells,
      sheetHistory,
      withTable,
      settle,
      filter,
      selectTable,
      say,
    } = deps;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const ops = host.__EASYOFFICE_SHEET_TABLE_OPS__;
    const refsApi = host.__EASYOFFICE_SHEET_REFS__;
    const refOf = refsApi.refOf;
    const { $, openDialog, closeDialog, note } = kit;
    let activeMode = null;
    let bound = false;

    function checkboxes() {
      return [...($("sheet-dedupe-columns")?.querySelectorAll("input[data-dedupe-col]") || [])];
    }

    function setAll(checked) {
      for (const box of checkboxes()) box.checked = checked;
      note("sheet-dedupe-status", "");
    }

    function renderColumns(names, keepChecked = null) {
      const list = $("sheet-dedupe-columns");
      if (!list) return;
      list.replaceChildren();
      names.forEach((colName, idx) => {
        const item = document.createElement("li");
        const label = document.createElement("label");
        label.className = "dialog-checkbox";
        const input = document.createElement("input");
        input.setAttribute("type", "checkbox");
        input.type = "checkbox";
        input.checked = keepChecked ? keepChecked.has(idx) : true;
        input.dataset.dedupeCol = String(idx);
        label.appendChild(input);
        label.appendChild(document.createTextNode(" " + colName));
        item.appendChild(label);
        list.appendChild(item);
      });
    }

    function rangeColumnNames(box, hasHeader) {
      const map = sheetDataStore[getActiveSheetName()] || {};
      const names = [];
      for (let c = box.c1; c <= box.c2; c += 1) {
        const fallback = "열 " + refsApi.colName(c);
        if (hasHeader) {
          const raw = String(map[refOf(box.r1, c)]?.value ?? "").trim();
          names.push(raw || fallback);
        } else {
          names.push(fallback);
        }
      }
      return names;
    }

    function open(table) {
      if (!table) return false;
      ensureBound();
      activeMode = { kind: "table", table };
      const hdrRow = $("sheet-dedupe-header-row");
      if (hdrRow) hdrRow.hidden = true;
      renderColumns(table.columns);
      note("sheet-dedupe-status", "");
      openDialog("sheet-table-dedupe-dialog");
      checkboxes()[0]?.focus?.();
      return true;
    }

    function openRange(box, hasHeader) {
      if (!box) return false;
      ensureBound();
      activeMode = { kind: "range", box };
      const hdrRow = $("sheet-dedupe-header-row");
      const hdrBox = $("sheet-dedupe-header");
      if (hdrRow) hdrRow.hidden = false;
      if (hdrBox) hdrBox.checked = Boolean(hasHeader);
      renderColumns(rangeColumnNames(box, Boolean(hasHeader)));
      note("sheet-dedupe-status", "");
      openDialog("sheet-table-dedupe-dialog");
      checkboxes()[0]?.focus?.();
      return true;
    }

    function applyRowCompaction(map, b, plan) {
      const copied = plan.rowMoves.map(([srcRow, dstRow]) => {
        const cells = [];
        for (let c = b.c1; c <= b.c2; c += 1) {
          const cur = map[refOf(srcRow, c)];
          cells.push([c, cur ? structuredClone(cur) : undefined]);
        }
        return [dstRow, cells];
      });
      for (const [dstRow, cells] of copied) {
        for (const [c, entry] of cells) {
          const ref = refOf(dstRow, c);
          if (entry) map[ref] = entry;
          else delete map[ref];
        }
      }
      for (let r = plan.clearFrom; r <= b.r2; r += 1) {
        for (let c = b.c1; c <= b.c2; c += 1) delete map[refOf(r, c)];
      }
    }

    function execute(table, cols) {
      const name = getActiveSheetName();
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        if (!allowed(name)) return false;
        const map = snapshotDomSheetCells();
        sheetDataStore[name] = map;
        const held = layoutApi.tablesOf(name).find((t) => t.name === table.name);
        if (!held) return false;
        const plan = ops.planDedupe({ table: held, cols, ctx: contextOf(name) });
        if (plan.error) {
          say(plan.error);
          return false;
        }
        if (plan.removed === 0) {
          say("중복된 값이 없습니다.");
          return false;
        }
        const b = plan.box;
        const record = sheetHistory.begin({ range: { r1: b.r1, c1: b.c1, r2: b.r2, c2: b.c2 }, parts: ["sheetLayout"] });
        applyRowCompaction(map, b, plan);
        layoutApi.setTables(name, withTable(name, held, plan.table));
        sheetHistory.commit(record);
        const msg = plan.removed + "개의 중복된 값이 검색되어 제거되었습니다. " + plan.remaining + "개의 고유 값이 남습니다.";
        await settle(name, msg);
        if (plan.reapply) await filter.reapplyAfterEdit(false);
        selectTable(plan.table);
        return true;
      });
    }

    function executeRange(box, hasHeader, cols) {
      const name = getActiveSheetName();
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        if (!allowed(name)) return false;
        const map = snapshotDomSheetCells();
        sheetDataStore[name] = map;
        const plan = ops.planRangeDedupe({ box, header: hasHeader, cols, ctx: contextOf(name) });
        if (plan.error) {
          say(plan.error);
          return false;
        }
        if (plan.removed === 0) {
          say("중복된 값이 없습니다.");
          return false;
        }
        const record = sheetHistory.begin({ range: { r1: box.r1, c1: box.c1, r2: box.r2, c2: box.c2 }, parts: ["sheetLayout"] });
        applyRowCompaction(map, box, plan);
        sheetHistory.commit(record);
        const msg = plan.removed + "개의 중복된 값이 검색되어 제거되었습니다. " + plan.remaining + "개의 고유 값이 남습니다.";
        await settle(name, msg);
        deps.selectRange?.(box);
        return true;
      });
    }

    function submit() {
      if (!activeMode) return;
      const cols = checkboxes()
        .filter((b) => b.checked)
        .map((b) => Number(b.dataset.dedupeCol));
      if (cols.length === 0) {
        note("sheet-dedupe-status", "열을 하나 이상 선택");
        return;
      }
      const mode = activeMode;
      const hasHeader = Boolean($("sheet-dedupe-header")?.checked);
      closeDialog("sheet-table-dedupe-dialog");
      if (mode.kind === "table") void execute(mode.table, cols);
      else void executeRange(mode.box, hasHeader, cols);
    }

    function ensureBound() {
      if (bound) return;
      bound = true;
      $("btn-sheet-dedupe-all")?.addEventListener("click", () => setAll(true));
      $("btn-sheet-dedupe-none")?.addEventListener("click", () => setAll(false));
      $("btn-sheet-dedupe-ok")?.addEventListener("click", submit);
      $("sheet-dedupe-header")?.addEventListener("change", () => {
        if (activeMode?.kind !== "range") return;
        const checkedSet = new Set(checkboxes().filter((b) => b.checked).map((b) => Number(b.dataset.dedupeCol)));
        renderColumns(rangeColumnNames(activeMode.box, Boolean($("sheet-dedupe-header")?.checked)), checkedSet);
      });
    }

    return { open, openRange, execute, executeRange, ensureBound };
  }

  host.__EASYOFFICE_CREATE_SHEET_TABLE_DEDUPE_UI__ = createSheetTableDedupeUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
