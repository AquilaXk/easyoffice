// Row heights by hand (PD-277): dragging the bottom edge of a row header sets the height of that row, or of every selected
// row when it is one of them, and a double click on the edge gives the row (or the selected rows) back to its content. A
// drag is one undo step, and so is a fit.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 4767b3b309467c283bbd52b725536ca73861e0fc):
//   packages/sheets-ui/src/controllers/render-controllers/header-resize.render-controller.ts :293-495 (the press on the
//       edge of a row header, the distance of the move, the release that runs `DeltaRowHeightCommand`) and :497-560 (the
//       double click that runs `SetWorksheetRowIsAutoHeightCommand`)
//   packages/sheets/src/commands/commands/set-worksheet-row-height.command.ts (see `sheet-row-height.js`)
// Modifications: the drag redraws the grid on every move (the reference moves a helper line and sets the height at the
// release), as the column drag does; the undo step is the layout part of the sheet history, begun at the press and kept at the
// release when a height changed, instead of the reference's mutation lists; the double click fits every selected row when the
// pressed one is among them, as the spreadsheet programs do (the reference fits only the pressed row); a fit that ran out of
// the row-fit budget is undone whole and says why, as the column fit does.
(function (host) {
  "use strict";

  const RH = host.__EASYOFFICE_SHEET_ROW_HEIGHT__;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

  function createRowResize(deps) {
    const { document, grid, getActiveSheetName, getRange, getSelection, commitIfEditing, snapshotDomSheetCells, sheetDataStore, sheetHistory, enqueueWorkspaceHistoryOp, drainInFlightSheetWork } = deps;
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    let drag = null;

    function say(text) {
      const feedback = document.getElementById("status-command-feedback");
      if (feedback) feedback.textContent = text;
    }

    const heightsOf = (name) => layoutApi.rowHeightsOf(name) || new Map();
    const guarded = (name) => !protectApi || protectApi.guardAction(name, "formatRows");

    // The rows an edge of row `pressed` stands for.
    function spanOf(pressed) {
      const rg = getRange();
      return RH.rowsToSet(pressed, { axis: getSelection().axis, r1: rg.r1, r2: rg.r2 });
    }

    function apply(name, map) {
      layoutApi.setRowHeights(name, map);
      grid.layoutChanged();
    }

    function onMouseDown(ev) {
      const handle = ev.button ? null : ev.target?.closest?.(".row-resizer");
      if (!handle) return;
      ev.preventDefault();
      const name = getActiveSheetName();
      if (!guarded(name)) return;
      const pressed = Number(handle.parentElement?.dataset?.row);
      if (!(pressed >= 1)) return;
      commitIfEditing();
      const [first, last] = spanOf(pressed);
      drag = { name, first, last, startY: ev.clientY, startH: grid.rowHeight(pressed), before: heightsOf(name), step: sheetHistory.begin({ parts: ["sheetLayout"] }), moved: false };
    }

    function onMouseMove(ev) {
      if (!drag) return;
      const next = RH.withHeight(drag.before, drag.first, drag.last, drag.startH + ev.clientY - drag.startY);
      if (next === null) {
        say("행 높이를 지정한 행이 너무 많음");
        return;
      }
      drag.moved = true;
      apply(drag.name, next);
    }

    function onMouseUp() {
      const done = drag;
      drag = null;
      if (done?.moved) sheetHistory.commit(done.step);
    }

    // Gives rows `first..last` back to their content as ONE undoable step.
    function fitRows(first, last) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const name = getActiveSheetName();
        if (!guarded(name)) return false;
        sheetDataStore[name] = snapshotDomSheetCells();
        const before = heightsOf(name);
        const next = RH.withoutRows(before, first, last);
        if (next.size === before.size) return false;
        const step = sheetHistory.begin({ parts: ["sheetLayout"] });
        apply(name, next);
        if (grid.rowFitOverBudget()) {
          apply(name, before);
          say("행 높이를 맞추기에 시트가 너무 큼");
          return false;
        }
        sheetHistory.commit(step);
        say("행 높이 자동 맞춤: " + (last - first + 1) + "행");
        return true;
      });
    }

    function onDblClick(ev) {
      const handle = ev.target?.closest?.(".row-resizer");
      const pressed = Number(handle?.parentElement?.dataset?.row);
      if (!(pressed >= 1)) return;
      ev.preventDefault();
      void fitRows(...spanOf(pressed));
    }

    function bind() {
      const viewport = document.getElementById("sheet-viewport");
      viewport?.addEventListener("mousedown", onMouseDown);
      viewport?.addEventListener("dblclick", onDblClick);
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    }

    return { bind, fitRows };
  }

  host.__EASYOFFICE_CREATE_SHEET_ROW_RESIZE__ = createRowResize;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
