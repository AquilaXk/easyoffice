(function (host) {
  "use strict";

  // 보기 > 수식 입력줄 and 보기 > 머리글 of the sheet window (PD-391). The formula bar (name box and formula field) is a setting
  // of the window, like the spreadsheet program's: it comes back the next time a sheet window opens. The headings (the column
  // letters and row numbers) belong to a sheet, as there: each sheet keeps its own choice for as long as the window is open.
  const FORMULA_BAR_KEY = "easyoffice.sheet.formulaBarShown";

  function readFormulaBar() {
    try {
      return host.localStorage?.getItem(FORMULA_BAR_KEY) !== "0";
    } catch {
      return true;
    }
  }

  function writeFormulaBar(shown) {
    try {
      host.localStorage?.setItem(FORMULA_BAR_KEY, shown ? "1" : "0");
    } catch {
      // A page without storage keeps the choice for this window only.
    }
  }

  function createSheetViewOptions({ document, grid, getActiveSheetName }) {
    let formulaBarShown = readFormulaBar();
    const headingsHidden = new Set();
    // 수식 > 수식 표시 (Ctrl+`, PD-400) belongs to a sheet like the headings; the cells draw it (`sheet-grid-text.js`).
    const formulasShown = new Set();
    const gridText = host.__EASYOFFICE_SHEET_GRID_TEXT__;

    function say(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    function setChecked(act, on) {
      for (const el of document.querySelectorAll('.md-item[data-sheet-act="' + act + '"]')) el.setAttribute("aria-checked", on ? "true" : "false");
    }

    // Puts the window as the two settings say; runs when the window opens and after another sheet is shown.
    function refresh() {
      const bar = document.getElementById("sheet-formula-bar");
      if (bar) bar.hidden = !formulaBarShown;
      const headings = !headingsHidden.has(getActiveSheetName());
      grid.setHeadersShown(headings);
      setChecked("view-formula-bar", formulaBarShown);
      setChecked("view-headings", headings);
      const formulas = formulasShown.has(getActiveSheetName());
      setChecked("view-formulas", formulas);
      if (gridText && gridText.showsFormulas() !== formulas) {
        gridText.setShowFormulas(formulas);
        grid.repaintFormulas();
      }
    }

    function toggleFormulas() {
      const name = getActiveSheetName();
      if (formulasShown.has(name)) formulasShown.delete(name);
      else formulasShown.add(name);
      refresh();
      say(formulasShown.has(name) ? "수식 표시" : "수식 표시 해제");
      return Promise.resolve(true);
    }

    function toggleFormulaBar() {
      formulaBarShown = !formulaBarShown;
      writeFormulaBar(formulaBarShown);
      refresh();
      say(formulaBarShown ? "수식 입력줄 표시" : "수식 입력줄 숨김");
      return Promise.resolve(true);
    }

    function toggleHeadings() {
      const name = getActiveSheetName();
      if (headingsHidden.has(name)) headingsHidden.delete(name);
      else headingsHidden.add(name);
      refresh();
      say(headingsHidden.has(name) ? "머리글 숨김" : "머리글 표시");
      return Promise.resolve(true);
    }

    return { actions: { "view-formula-bar": toggleFormulaBar, "view-headings": toggleHeadings, "view-formulas": toggleFormulas }, refresh };
  }

  host.__EASYOFFICE_CREATE_SHEET_VIEW_OPTIONS__ = createSheetViewOptions;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
