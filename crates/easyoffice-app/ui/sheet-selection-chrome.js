(function (host) {
  "use strict";

  // What a change of the selection updates around the grid (`sheet-input.js` owns the selection): the classes on the cells and headers,
  // the border of an array's area, the name box, the formula bar text and the aggregate of the status bar.
  const { normalizeFormattedNumericCell } = host.__EASYOFFICE_SHEET_UTILS__;

  function numericOf(raw) {
    const t = normalizeFormattedNumericCell(raw);
    if (t === "") return null;
    const n = Number(t);
    return Number.isFinite(n) ? n : null;
  }

  // The status bar of the spreadsheet program: 평균, 개수 (cells that are not empty), 숫자 개수, 최소값, 최대값, 합계, in
  // that order; the number ones only when the selection holds a number, and nothing for an empty selection.
  function formatAggregate({ nums, filled }) {
    const parts = [];
    const fmt = (n) => Number(n.toPrecision(12)).toLocaleString();
    if (nums.length > 0) {
      const { count, sum, min, max, avg } = host.__EASYOFFICE_SHEET_UTILS__.aggregateNumbers(nums);
      parts.push("평균: " + (Math.round(avg * 10) / 10).toLocaleString(), "개수: " + filled, "숫자 개수: " + count, "최소값: " + fmt(min), "최대값: " + fmt(max), "합계: " + fmt(sum));
    } else if (filled > 0) {
      parts.push("개수: " + filled);
    }
    return parts.join("  ");
  }

  // `sel` is the selection of the input module (`ar`, `ac` the anchor, `cr`, `cc` the other corner), read as it changes.
  function createSelectionChrome({ document, api, grid, sel, protectApi, point, fill, getRange, getRanges, getExtraRanges, stateOf, tdAt }) {
    const multiApi = host.__EASYOFFICE_SHEET_MULTI_RANGE__;
    // ---- aggregate -------------------------------------------------------------------------

    const collectNumbers = (ranges, liveTd) => multiApi.collectNumbersMulti(ranges, liveTd, api, grid, numericOf);

    // ---- selection painting ----------------------------------------------------------------
    // The border of the area of the array the active cell belongs to, with the token of the selection stroke (PD-217).
    function paintSpillArea() {
      const spill = host.__EASYOFFICE_SHEET_SPILL__;
      if (!spill) return;
      const area = spill.areaOf((ref) => api.getEntry(ref), grid.refOf(sel.ar, sel.ac));
      const tbody = document.querySelector("#pane-sheet tbody");
      if (!tbody) return;
      if (!area && !tbody.querySelector("td.spill-t, td.spill-b, td.spill-l, td.spill-r")) return;
      tbody.querySelectorAll("td[data-cell]").forEach((td) => {
        const r = td._row;
        const c = td._col;
        const inside = Boolean(area) && r >= area.r0 && r <= area.r1 && c >= area.c0 && c <= area.c1;
        td.classList.toggle("spill-t", inside && r === area.r0);
        td.classList.toggle("spill-b", inside && r === area.r1);
        td.classList.toggle("spill-l", inside && c === area.c0);
        td.classList.toggle("spill-r", inside && c === area.c1);
      });
    }

    function paintSelection() {
      paintSpillArea();
      const rg = getRange();
      const extraList = typeof getExtraRanges === "function" ? getExtraRanges() : sel.extraRanges;
      const extras = Array.isArray(extraList) && extraList.length > 0 ? extraList : undefined;
      if (typeof grid.paintSelectionRange === "function") {
        grid.paintSelectionRange({ r1: rg.r1, r2: rg.r2, c1: rg.c1, c2: rg.c2, ar: sel.ar, ac: sel.ac, extraRanges: extras });
        fill.sync();
        return;
      }
      const ranges = typeof getRanges === "function" ? getRanges() : [rg];
      const multi = ranges.length > 1 || rg.r1 !== rg.r2 || rg.c1 !== rg.c2;
      const inAny = (r, c) => (multiApi ? multiApi.anyContainsCoord(ranges, r, c) : r >= rg.r1 && r <= rg.r2 && c >= rg.c1 && c <= rg.c2);
      const tbody = document.querySelector("#pane-sheet tbody");
      if (!tbody) return;
      tbody.querySelectorAll("td[data-cell]").forEach((td) => {
        let r = td._row;
        let c = td._col;
        if (!(r > 0 && c > 0)) {
          const p = grid.parseRef(td.dataset.cell);
          if (!p) return;
          r = p.row;
          c = p.col;
          td._row = r;
          td._col = c;
        }
        const active = r === sel.ar && c === sel.ac;
        const inside = multi && inAny(r, c);
        td.classList.toggle("selected-cell", active);
        td.classList.toggle("in-range", inside);
      });
      document.querySelectorAll("#pane-sheet th[data-col]").forEach((th) => {
        const idx = grid.colIndex(th.dataset.col);
        th.classList.toggle("active-axis", ranges.some((rect) => idx >= rect.c1 && idx <= rect.c2));
      });
      document.querySelectorAll("#pane-sheet th[data-row]").forEach((th) => {
        const row = Number(th.dataset.row);
        th.classList.toggle("active-axis", ranges.some((rect) => row >= rect.r1 && row <= rect.r2));
      });
    }

    function refreshChrome() {
      const ref = grid.refOf(sel.ar, sel.ac);
      api.setActiveSheetCellRef(ref);
      paintSelection();
      const activeTd = tdAt(sel.ar, sel.ac);
      const badge = document.getElementById("active-cell-ref");
      // The name box (PD-214) shows the name of a definition that refers to exactly the selection, else the address.
      if (badge && document.activeElement !== badge) {
        badge.textContent = host.__EASYOFFICE_SHEET_NAMES__?.nameOfRange(api.getSheetName(), getRange()) || ref;
      }
      const formulaInput = document.getElementById("formula-input");
      if (formulaInput && document.activeElement !== formulaInput && !point.holdsBar()) {
        // A protected sheet does not show the formula of a cell whose format hides it (PD-216).
        const hidden = protectApi?.formulaHidden(api.getSheetName(), sel.ar, sel.ac, activeTd ? activeTd._st : api.getEntry(ref)?.style);
        // A cell an array filled shows the formula of the array, dimmed: it cannot be edited on its own (PD-217).
        const filledBy = host.__EASYOFFICE_SHEET_SPILL__?.isChild(api.getEntry(ref)) ? api.getEntry(api.getEntry(ref).spillOf) : null;
        formulaInput.classList.toggle("spill-ghost", Boolean(filledBy));
        const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;
        if (filledBy) {
          // The formula of a cell whose format hides it stays hidden on the cells it fills (PD-222).
          const concealed = hidden || protectApi?.formulaHidden(api.getSheetName(), sel.ar, sel.ac, filledBy.style);
          formulaInput.value = concealed ? api.getEntry(ref)?.value || "" : filledBy.formula || "";
        } else if (activeTd) {
          const live = grid.readTdValue(activeTd);
          const st = stateOf(activeTd);
          formulaInput.value = st?.userEdited || (st?.mode && st?.editBaseline !== null)
            ? live
            : cellTypeApi.editableRaw(live, hidden ? "" : activeTd.dataset.formula || "", activeTd._ct, activeTd._nf);
        } else {
          const entry = api.getEntry(ref);
          formulaInput.value = cellTypeApi.editableRaw(entry?.value || "", hidden ? "" : entry?.formula || "", entry?.t, entry?.style?.numFmt);
        }
      }
      const aggEl = document.getElementById("status-sheet-agg");
      if (aggEl) {
        const editingTd = activeTd && stateOf(activeTd)?.userEdited ? activeTd : null;
        // A `GETPIVOTDATA` into a pivot table the engine left out for its size answers `#REF!`: the status says why (PD-303).
        const ranges = typeof getRanges === "function" ? getRanges() : [getRange()];
        const rg = ranges[0];
        const single = ranges.length === 1 && rg.r1 === rg.r2 && rg.c1 === rg.c2;
        const note = single && !editingTd ? host.__EASYOFFICE_SHEET_SPILL__?.pivotOmittedNote(api.getSheetName(), api.getEntry(ref)) : "";
        aggEl.textContent = note || formatAggregate(collectNumbers(ranges, editingTd));
      }
      const out = document.getElementById("sheet-eval-result");
      if (out) out.setAttribute("title", out.textContent || "");
      point.refresh();
      api.onSelectionChanged?.();
    }

    return { paintSelection, refreshChrome };
  }

  host.__EASYOFFICE_CREATE_SHEET_SELECTION_CHROME__ = createSelectionChrome;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
