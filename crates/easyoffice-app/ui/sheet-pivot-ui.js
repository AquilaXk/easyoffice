(function (host) {
  "use strict";

  // The pivot table dialog (PD-171): picks the range and the row, column, value and aggregation
  // fields, asks the engine to group the values and writes the answer into a new sheet.

  const rules = host.__EASYOFFICE_SHEET_RULES__;
  const PIVOT_MAX_FIELDS = 256;

  function createSheetPivotUi(deps, kit) {
    const {
      document,
      sheetDataStore,
      sheetRowCounts,
      getActiveSheetName,
      getRange,
      sheetStoreHelpers,
      enqueueWorkspaceHistoryOp,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
      flushFocusedSheetCellBeforeSwitch,
      performSwitchSheetTabAsync,
      selectionText,
    } = deps;
    const { $, val, checked, setVal, setChecked, setSelect, openDialog, closeDialog, optionsOf, fireChange, note } = kit;

    const pivot = { ready: false };

    function usedRangeText() {
      const map = sheetDataStore[getActiveSheetName()] || {};
      let r1 = Infinity;
      let c1 = Infinity;
      let r0 = 0;
      let c0 = 0;
      for (const [ref, entry] of Object.entries(map)) {
        if (!entry || !(entry.value || entry.formula)) continue;
        const p = rules.parseCell(ref);
        if (!p) continue;
        r1 = Math.min(r1, p.row);
        c1 = Math.min(c1, p.col);
        r0 = Math.max(r0, p.row);
        c0 = Math.max(c0, p.col);
      }
      return Number.isFinite(r1) ? rules.formatRanges([{ row0: r1, col0: c1, row1: r0, col1: c0 }]) : "";
    }

    function pivotBox() {
      const boxes = rules.parseRanges(val("sheet-pivot-range"));
      return boxes && boxes.length === 1 ? boxes[0] : null;
    }

    function cellText(sheet, row0, col0) {
      const entry = sheetDataStore[sheet]?.[rules.refOf(row0, col0)];
      return String(entry?.value || entry?.formula || "");
    }

    function refreshPivotFields() {
      const box = pivotBox();
      const sheet = getActiveSheetName();
      const width = box ? Math.min(box.col1 - box.col0 + 1, PIVOT_MAX_FIELDS) : 0;
      const header = checked("sheet-pivot-header");
      const names = [];
      for (let i = 0; i < width; i += 1) {
        const text = header ? cellText(sheet, box.row0, box.col0 + i).trim() : "";
        names.push(text || "열 " + rules.refOf(0, box.col0 + i).replace(/\d+$/, ""));
      }
      const fill = (id, withNone) => {
        const el = $(id);
        if (!el) return;
        const prev = el.value;
        el.replaceChildren();
        if (withNone) {
          const none = document.createElement("option");
          none.value = "";
          none.textContent = "(없음)";
          el.appendChild(none);
        }
        names.forEach((name, i) => {
          const opt = document.createElement("option");
          opt.value = String(i);
          opt.textContent = name;
          el.appendChild(opt);
        });
        const all = optionsOf(el);
        el.value = all.some((o) => o.value === prev) ? prev : (all[0]?.value ?? "");
        fireChange(el);
      };
      fill("sheet-pivot-row-select", false);
      fill("sheet-pivot-col-select", true);
      fill("sheet-pivot-value-select", false);
      if (names.length > 1) setSelect("sheet-pivot-value-select", String(names.length - 1));
      pivot.names = names;
    }

    function ensurePivotControls() {
      if (pivot.ready) return;
      pivot.ready = true;
      $("sheet-pivot-range")?.addEventListener("change", refreshPivotFields);
      $("sheet-pivot-header")?.addEventListener("change", refreshPivotFields);
      $("btn-sheet-pivot-apply")?.addEventListener("click", () => void createPivot());
    }

    function openPivotDialog() {
      ensurePivotControls();
      const r = getRange();
      const multi = r.r1 !== r.r2 || r.c1 !== r.c2;
      setVal("sheet-pivot-range", multi ? selectionText() : usedRangeText());
      setChecked("sheet-pivot-header", true);
      refreshPivotFields();
      note("sheet-pivot-status", "결과는 새 시트에 만듭니다.");
      return openDialog("sheet-pivot-dialog");
    }

    function sheetCellPayload(sheet) {
      const cells = [];
      for (const [cell, entry] of Object.entries(sheetDataStore[sheet] || {})) {
        const value = entry?.formula || entry?.value;
        if (value) cells.push({ cell, value });
      }
      return cells;
    }

    const numberText = (n) => (Number.isFinite(n) ? String(Math.round(n * 1e10) / 1e10) : "");

    // Cells of the new sheet: a header row, one row per group and the totals.
    function pivotCells(result, labels) {
      const cells = {};
      const put = (row, col, text) => {
        if (text !== "") cells[rules.refOf(row, col)] = { value: String(text), formula: "" };
      };
      const twoWay = result.columnKeys.length > 1 || labels.col !== "";
      put(0, 0, labels.row);
      if (twoWay) {
        result.columnKeys.forEach((key, i) => put(0, i + 1, key));
        put(0, result.columnKeys.length + 1, "합계");
      } else {
        put(0, 1, labels.value);
      }
      result.rowKeys.forEach((key, r) => {
        put(r + 1, 0, key);
        result.matrix[r].forEach((v, c) => put(r + 1, c + 1, numberText(v)));
        if (twoWay) put(r + 1, result.columnKeys.length + 1, numberText(result.rowTotals[r]));
      });
      const last = result.rowKeys.length + 1;
      put(last, 0, "합계");
      if (twoWay) {
        result.columnTotals.forEach((v, c) => put(last, c + 1, numberText(v)));
        put(last, result.columnKeys.length + 1, numberText(result.grandTotal));
      } else {
        put(last, 1, numberText(result.grandTotal));
      }
      return { cells, rows: last + 1 };
    }

    function uniqueSheetKey() {
      const taken = new Set(
        [...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((b) => b.dataset.sheetTab)
      );
      for (const key of Object.keys(sheetDataStore)) taken.add(key);
      let n = 1;
      while (taken.has("피벗" + n)) n += 1;
      return "피벗" + n;
    }

    async function createPivot() {
      const box = pivotBox();
      if (!box) {
        note("sheet-pivot-status", "데이터 범위를 A1:D90 형식으로 입력하세요.");
        return;
      }
      const sheet = getActiveSheetName();
      const colValue = val("sheet-pivot-col-select");
      const request = {
        sessionAction: "pivot",
        sheet,
        cells: sheetCellPayload(sheet),
        range: rules.formatRanges([box]),
        header: checked("sheet-pivot-header"),
        row: Number(val("sheet-pivot-row-select")),
        col: colValue === "" ? null : Number(colValue),
        value: Number(val("sheet-pivot-value-select")),
        agg: val("sheet-pivot-agg-select") || "sum",
      };
      let result;
      try {
        result = await sheetStoreHelpers.callSheetAction(request);
      } catch (err) {
        note("sheet-pivot-status", "피벗을 만들지 못했습니다: " + (err?.message || err));
        return;
      }
      if (!result || !Array.isArray(result.rowKeys)) {
        note("sheet-pivot-status", "피벗을 만들지 못했습니다.");
        return;
      }
      const names = pivot.names || [];
      const aggLabel = optionsOf($("sheet-pivot-agg-select")).find((o) => o.value === request.agg)?.textContent || "";
      const labels = {
        row: names[request.row] || "행",
        col: request.col === null ? "" : names[request.col] || "",
        value: aggLabel + ": " + (names[request.value] || "값"),
      };
      await enqueueWorkspaceHistoryOp(async () => {
        await flushFocusedSheetCellBeforeSwitch();
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        const key = uniqueSheetKey();
        const tab = document.createElement("button");
        tab.className = "sheet-tab-btn";
        tab.setAttribute("role", "tab");
        tab.dataset.sheetTab = key;
        tab.textContent = key;
        document.getElementById("btn-add-sheet-tab")?.before(tab);
        const built = pivotCells(result, labels);
        sheetDataStore[key] = built.cells;
        sheetRowCounts[key] = Math.max(12, built.rows);
        await performSwitchSheetTabAsync(key);
        return true;
      });
      closeDialog("sheet-pivot-dialog");
    }

    return { openPivotDialog, pivotCells };
  }

  host.__EASYOFFICE_CREATE_SHEET_PIVOT_UI__ = createSheetPivotUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
