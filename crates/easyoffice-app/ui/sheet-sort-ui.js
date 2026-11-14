(function (host) {
  "use strict";

  // The sort dialog (PD-203): the range, one row per sort key (column and direction), the header
  // row and case options. The sort itself is `sheet-sort.js` (through `sortRange` of the sheet
  // operations); this file reads and fills the static markup of `#sheet-sort-dialog`.

  const MAX_KEYS = 64;
  const MAX_COLUMN_CHOICES = 256; // a column list longer than the used columns is no help
  const FIRST_LABEL = "정렬 기준";
  const NEXT_LABEL = "다음 기준";

  const colName = (col) => host.__EASYOFFICE_SHEET_RULES__.refOf(0, col - 1).replace(/\d+$/, "");

  function createSheetSortUi(deps) {
    const {
      document,
      kit,
      sheetDataStore,
      getRange,
      getSelection,
      getExtent,
      getActiveSheetName,
      commitIfEditing,
      snapshotDomSheetCells,
      sortRange,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
    } = deps;
    const { $, setChecked, openDialog, closeDialog, note } = kit;
    const filterCore = host.__EASYOFFICE_SHEET_FILTER_CORE__;
    const sortCore = host.__EASYOFFICE_SHEET_SORT_CORE__;
    const refOf = (r, c) => host.__EASYOFFICE_SHEET_RULES__.refOf(r - 1, c - 1);

    // The dialog's state: the sorted range and one `{ col, asc }` per key row.
    const state = { range: null, keys: [], bound: false };

    const hasHeader = () => $("sheet-sort-header")?.checked === true;

    // The range a sort covers: the selection, or the region around a single cell, clipped to the
    // cells that hold something.
    function targetRange() {
      const name = getActiveSheetName();
      const map = sheetDataStore[name] || {};
      const e = getExtent();
      const has = (r, c) => {
        const entry = map[refOf(r, c)];
        return Boolean(entry && (entry.value || entry.formula));
      };
      const sel = getSelection();
      const extent = { rows: e.dataRows, cols: e.dataCols };
      const range = sel.axis ? filterCore.rangeOfAxisSelection(sel, extent) : filterCore.regionFrom(getRange(), has, extent);
      return { ...range, c2: Math.min(range.c2, range.c1 + MAX_COLUMN_CHOICES - 1) };
    }

    // The text of a column's choice: the header cell when the first row is a header.
    function columnLabel(col) {
      const head = hasHeader() ? String((sheetDataStore[getActiveSheetName()] || {})[refOf(state.range.r1, col)]?.value ?? "").trim() : "";
      return head || "열 " + colName(col);
    }

    function fillColumns(select, current) {
      select.replaceChildren();
      for (let c = state.range.c1; c <= state.range.c2; c += 1) {
        const option = document.createElement("option");
        option.value = String(c);
        option.textContent = columnLabel(c);
        select.appendChild(option);
      }
      select.value = String(current);
    }

    function rowFor(index) {
      const key = state.keys[index];
      const row = document.createElement("div");
      row.className = "dialog-row sheet-sort-row";
      const label = document.createElement("label");
      label.className = "dialog-label";
      label.textContent = index === 0 ? FIRST_LABEL : NEXT_LABEL;
      const col = document.createElement("select");
      col.className = "sb-combo dialog-select";
      col.id = "sheet-sort-col-" + index;
      col.dataset.sortCol = "1";
      label.htmlFor = col.id;
      fillColumns(col, key.col);
      const order = document.createElement("select");
      order.className = "sb-combo dialog-select";
      order.id = "sheet-sort-order-" + index;
      order.setAttribute("aria-label", "순서");
      for (const [value, text] of [["asc", "오름차순"], ["desc", "내림차순"]]) {
        const option = document.createElement("option");
        option.value = value;
        option.textContent = text;
        order.appendChild(option);
      }
      order.value = key.asc ? "asc" : "desc";
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "dialog-btn";
      remove.textContent = "삭제";
      remove.dataset.sortRemove = String(index);
      remove.disabled = state.keys.length === 1;
      col.addEventListener("change", () => {
        key.col = Number(col.value);
      });
      order.addEventListener("change", () => {
        key.asc = order.value === "asc";
      });
      for (const part of [label, col, order, remove]) row.appendChild(part);
      return row;
    }

    function render() {
      const box = $("sheet-sort-keys");
      if (!box) return;
      box.replaceChildren();
      state.keys.forEach((_, i) => box.appendChild(rowFor(i)));
      host.__EASYOFFICE_CUSTOM_SELECT__?.enhanceCustomSelects?.(document, box);
      const add = $("btn-sheet-sort-add");
      if (add) add.disabled = state.keys.length >= Math.min(MAX_KEYS, state.range.c2 - state.range.c1 + 1);
    }

    function addKey() {
      const used = new Set(state.keys.map((k) => k.col));
      for (let c = state.range.c1; c <= state.range.c2; c += 1) {
        if (!used.has(c)) {
          state.keys.push({ col: c, asc: true });
          note("sheet-sort-status", "");
          render();
          return;
        }
      }
    }

    function removeKey(index) {
      if (state.keys.length <= 1) return;
      state.keys.splice(index, 1);
      render();
    }

    function open() {
      commitIfEditing();
      const name = getActiveSheetName();
      sheetDataStore[name] = snapshotDomSheetCells();
      state.range = targetRange();
      if (state.range.r2 <= state.range.r1) {
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = "정렬할 두 행 이상의 데이터 없음";
        return false;
      }
      const active = getRange().c1;
      const first = active >= state.range.c1 && active <= state.range.c2 ? active : state.range.c1;
      state.keys = [{ col: first, asc: true }];
      setChecked("sheet-sort-header", sortCore.detectHeader(sheetDataStore[name] || {}, state.range, refOf, sortCore.numberOf));
      setChecked("sheet-sort-case", false);
      note("sheet-sort-status", "");
      render();
      openDialog("sheet-sort-dialog");
      return true;
    }

    // The sort as one undoable step; the dialog closes first.
    function apply() {
      const cols = state.keys.map((k) => k.col);
      if (new Set(cols).size !== cols.length) {
        note("sheet-sort-status", "같은 열을 두 번 고를 수 없음");
        return Promise.resolve(false);
      }
      const spec = {
        range: { ...state.range, keyCol: cols[0] },
        keys: state.keys.map((k) => ({ col: k.col, asc: k.asc })),
        hasHeader: hasHeader(),
        caseSensitive: $("sheet-sort-case")?.checked === true,
      };
      closeDialog("sheet-sort-dialog");
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        sheetDataStore[getActiveSheetName()] = snapshotDomSheetCells();
        const change = sheetHistory.begin({ range: spec.range });
        const moved = await sortRange(spec);
        if (moved) sheetHistory.commit(change);
        return moved;
      });
    }

    function bind() {
      if (state.bound) return;
      state.bound = true;
      $("btn-sheet-sort-apply")?.addEventListener("click", () => void apply());
      $("btn-sheet-sort-add")?.addEventListener("click", addKey);
      $("sheet-sort-keys")?.addEventListener("click", (ev) => {
        const button = ev.target.closest?.("[data-sort-remove]");
        if (button) removeKey(Number(button.dataset.sortRemove));
      });
      // The column names show the header cells when the first row is a header.
      $("sheet-sort-header")?.addEventListener("change", render);
    }

    return { open, apply, bind, state: () => state };
  }

  host.__EASYOFFICE_CREATE_SHEET_SORT_UI__ = createSheetSortUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
