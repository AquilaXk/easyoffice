(function (host) {
  "use strict";

  // The view commands of the sheet window (PD-208): freezing rows and columns, grouping rows and
  // columns and folding the groups. Each command changes the layout store only, as one undo step
  // that keeps the cells as they are, and redraws the grid.

  const MAX_FREEZE_ROWS = 1000;
  const MAX_FREEZE_COLS = 100;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const outlineApi = host.__EASYOFFICE_SHEET_OUTLINE__;

  const GROUP_ERRORS = {
    level: "묶을 수 있는 수준은 7까지",
    limit: "묶을 수 있는 행·열은 100,000개까지",
    none: "대상 그룹 없음",
  };

  function createSheetView(deps) {
    const {
      document,
      grid,
      getRange,
      getSelection,
      commitIfEditing,
      refreshChrome,
      getActiveSheetName,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
    } = deps;

    function say(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    // One undo step over the layout: `change(name)` returns a message to show, or null to leave the
    // sheet as it is.
    function step(change) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const name = getActiveSheetName();
        const record = sheetHistory.begin({ parts: ["sheetLayout"] });
        const message = change(name);
        if (message === null) return false;
        sheetHistory.commit(record);
        grid.layoutChanged();
        refreshChrome();
        say(message);
        return true;
      });
    }

    // ---- frozen rows and columns -----------------------------------------------------------------
    function freezeTo(rows, cols) {
      if (rows > MAX_FREEZE_ROWS || cols > MAX_FREEZE_COLS) {
        say("고정할 수 있는 행은 " + MAX_FREEZE_ROWS + "개, 열은 " + MAX_FREEZE_COLS + "개까지");
        return Promise.resolve(false);
      }
      if (rows === 0 && cols === 0) {
        say("고정할 행·열 없음");
        return Promise.resolve(false);
      }
      return step((name) => {
        layoutApi.setFreeze(name, { rows, cols });
        return "틀 고정";
      });
    }

    const unfreeze = () =>
      step((name) => {
        if (!layoutApi.freezeOf(name)) return null;
        layoutApi.setFreeze(name, null);
        return "틀 고정 해제";
      });

    function freezeAtSelection() {
      const rg = getRange();
      return freezeTo(rg.r1 - 1, rg.c1 - 1);
    }

    // Grouping rows or columns changes how they show, so a protected sheet needs 행 서식 or 열 서식 (PD-216).
    // True when the sheet refuses (the status line says so).
    function groupBlocked(axis) {
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      const name = getActiveSheetName();
      if (!guard || !guard.isOn(name)) return false;
      const flags = axis === "col" ? ["formatColumns"] : axis === "row" ? ["formatRows"] : ["formatRows", "formatColumns"];
      return !flags.every((flag) => guard.guardAction(name, flag));
    }

    // ---- groups -----------------------------------------------------------------------------------
    // The selection's axis: a selected header names it, a range of cells means rows.
    function target() {
      const sel = getSelection();
      const axis = sel.axis === "col" ? "col" : "row";
      const rg = getRange();
      return axis === "row" ? { axis, first: rg.r1, last: rg.r2 } : { axis, first: rg.c1, last: rg.c2 };
    }

    // Applies `edit(outline, target)` (`{ outline }` or `{ error }`) to the active sheet's outline.
    function editOutline(edit, message) {
      if (groupBlocked(target().axis)) return Promise.resolve(false);
      return step((name) => {
        const t = target();
        const result = edit(layoutApi.outlineOf(name) || {}, t);
        if (result.error) {
          say(GROUP_ERRORS[result.error]);
          return null;
        }
        layoutApi.setOutline(name, result.outline);
        return message;
      });
    }

    const group = () => editOutline((o, t) => outlineApi.group(o, t.axis, t.first, t.last), "그룹 묶기");
    const ungroup = () => editOutline((o, t) => outlineApi.ungroup(o, t.axis, t.first, t.last), "그룹 풀기");
    const hideDetail = () => editOutline((o, t) => outlineApi.fold(o, t.axis, t.first, t.last, true), "하위 수준 숨기기");
    const showDetail = () => editOutline((o, t) => outlineApi.fold(o, t.axis, t.first, t.last, false), "하위 수준 표시");
    const clearOutline = () =>
      groupBlocked(null) ? Promise.resolve(false) : step((name) => {
        const outline = layoutApi.outlineOf(name);
        if (!outline || !outlineApi.hasGroups(outline)) return null;
        layoutApi.setOutline(name, outlineApi.clear(outline));
        return "윤곽 지우기";
      });

    // The outline bar's buttons: one group, or a level button for a whole axis.
    function toggleGroup(axis, g, fold) {
      if (groupBlocked(axis)) return Promise.resolve(false);
      return step((name) => {
        layoutApi.setOutline(name, outlineApi.setFolded(layoutApi.outlineOf(name) || {}, axis, g, fold));
        return fold ? "하위 수준 숨기기" : "하위 수준 표시";
      });
    }

    const showLevel = (axis, n) =>
      groupBlocked(axis) ? Promise.resolve(false) : step((name) => {
        layoutApi.setOutline(name, outlineApi.showLevel(layoutApi.outlineOf(name) || {}, axis, n));
        return "수준 " + n;
      });

    const ACTIONS = {
      "freeze-first-row": () => freezeTo(1, 0),
      "freeze-first-col": () => freezeTo(0, 1),
      "freeze-selection": freezeAtSelection,
      "freeze-clear": unfreeze,
      group,
      ungroup,
      "hide-detail": hideDetail,
      "show-detail": showDetail,
      "outline-clear": clearOutline,
    };

    return { actions: ACTIONS, toggleGroup, showLevel };
  }

  host.__EASYOFFICE_CREATE_SHEET_VIEW__ = createSheetView;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
