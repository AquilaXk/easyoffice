(function (host) {
  "use strict";

  // The view of the active sheet as the window has it (PD-208): which cells are selected, how far the
  // grid is scrolled and the zoom. `capture` writes it into the sheets' view store
  // (`sheet-view-state.js`) before the window leaves the sheet or the workbook is saved, `restore` gives a
  // sheet its own view back when the window comes to it. A sheet that has none shows A1 selected, unscrolled,
  // at 100 percent. Switching sheets lives here because it is the one place both happen.
  //
  // The zoom a sheet keeps and gives back when its tab is chosen follows another reference spreadsheet, whose sheet
  // item sets the zoom of the sheet it switches to (`packages/react/src/components/SheetTab/SheetItem.tsx` lines
  // 185 and 199, MIT, Copyright (c) 2022 Suzhou Ruilisi Technology Co., Ltd, commit
  // 94346608877db4747406707a177c4b8f3bacdbf9). That reference keeps no selection or scroll position; those and the
  // file side are written here.

  const viewApi = host.__EASYOFFICE_SHEET_VIEW_STATE__;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

  function createSheetViewLive(deps) {
    const {
      document,
      grid,
      input,
      sheetDataStore,
      getActiveModule,
      getActiveSheetName,
      setActiveSheetName,
      flushFocusedSheetCellBeforeSwitch,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      restoreDomSheetCells,
      refreshFocusedSheetCellPendingSnapshot,
      afterSwitch,
    } = deps;

    const viewport = () => document.getElementById("sheet-viewport");
    const zoomApi = () => host.__EASYOFFICE_ZOOM__;

    function frozen(name) {
      const freeze = layoutApi.freezeOf(name);
      return { rows: freeze?.rows || 0, cols: freeze?.cols || 0 };
    }

    // The first row and column the scrolling part shows, below and right of the frozen ones.
    function topLeft(name) {
      const vp = viewport();
      const { rows, cols } = frozen(name);
      const y = vp.scrollTop + grid.rowTop(rows + 1);
      let lo = rows + 1;
      let hi = grid.MAX_ROWS;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (grid.rowTop(mid) <= y) lo = mid;
        else hi = mid - 1;
      }
      return { row: lo, col: Math.max(cols + 1, grid.colAtX(vp.scrollLeft + grid.colLeft(cols + 1))) };
    }

    function scrollTo(name, { row, col }) {
      const vp = viewport();
      const { rows, cols } = frozen(name);
      vp.scrollTop = Math.max(0, grid.rowTop(Math.max(row, rows + 1)) - grid.rowTop(rows + 1));
      vp.scrollLeft = Math.max(0, grid.colLeft(Math.max(col, cols + 1)) - grid.colLeft(cols + 1));
      grid.renderWindow(false);
    }

    // A window that is not showing the sheet has nothing to read: its grid has no size.
    const showing = () => getActiveModule() === "SHEET" && Number(viewport()?.clientHeight) > 0;

    function capture(name = getActiveSheetName()) {
      if (!name || !showing()) return;
      const zoom = zoomApi()?.current() ?? 100;
      viewApi.set(name, viewApi.fromLive({ selection: input.getSelection(), top: topLeft(name), zoom }));
    }

    function restore(name = getActiveSheetName()) {
      if (!showing()) return;
      const live = viewApi.toLive(viewApi.get(name));
      zoomApi()?.apply(live.zoom);
      scrollTo(name, live.top);
      input.restoreSelection(live.selection);
    }

    async function switchTo(target) {
      await flushFocusedSheetCellBeforeSwitch();
      if (!sheetDataStore.Sheet1 && getActiveSheetName() === "Sheet1") {
        sheetDataStore.Sheet1 = snapshotDomSheetCells();
      }
      capture();
      saveActiveSheetCellsToWorkbook();
      setActiveSheetName(target);
      if (!sheetDataStore[target]) sheetDataStore[target] = {};
      restoreDomSheetCells(target);
      refreshFocusedSheetCellPendingSnapshot();
      document.getElementById("sheet-tab-bar")?.querySelectorAll(".sheet-tab-btn[data-sheet-tab]").forEach((b) => {
        const isAct = b.dataset.sheetTab === target;
        b.classList.toggle("active", isAct);
        b.setAttribute("role", "tab");
        b.setAttribute("aria-selected", isAct ? "true" : "false");
      });
      restore(target);
      afterSwitch(target);
    }

    return { capture, restore, switchTo, topLeft };
  }

  host.__EASYOFFICE_CREATE_SHEET_VIEW_LIVE__ = createSheetViewLive;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
