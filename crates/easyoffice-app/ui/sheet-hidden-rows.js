(function (host) {
  "use strict";

  // The session is told the rows a user and a filter hid (PD-232): `SUBTOTAL` and `AGGREGATE` leave them out, and the engine calculates
  // only the formulas that use them again. `session.hiddenRows` holds what each sheet was told; `sync` sends the sheets whose rows
  // differ (before the cells, with the merges and tables) and `stale` says whether one differs now.
  function createSheetHiddenRowsSync({ session, sessionCall, isFormulaStr }) {
    const layoutApi = () => host.__EASYOFFICE_SHEET_LAYOUT__;

    // Returns whether the answers changed a cell of `sheetsMap`.
    async function sync(names, sheetsMap) {
      const layout = layoutApi();
      let changed = false;
      for (const name of names) {
        const runs = layout.hiddenRunsOf(name);
        if (layout.sameHiddenRuns(session.hiddenRows.get(name) ?? layout.NO_RUNS, runs)) continue;
        const res = await sessionCall({ sessionAction: "hiddenRows", session: session.id, sheet: name, hiddenRows: runs });
        if (!res || res.expired === true) throw new Error("시트 세션이 만료되었습니다.");
        if (res.refused) continue;
        session.hiddenRows.set(name, runs);
        for (const c of Array.isArray(res.changed) ? res.changed : []) {
          session.values.set(c.sheet + "!" + c.cell, c.value);
          const held = sheetsMap[c.sheet]?.[c.cell];
          if (held && isFormulaStr(held.formula || "")) held.value = String(c.value);
          changed = true;
        }
      }
      return changed;
    }

    // Whether the session holds other hidden rows of the sheet than the layout has now (a hide, a filter or an undo changed them).
    // False without a session or for a sheet it does not hold.
    function stale(name) {
      const layout = layoutApi();
      if (session.id === null || !layout?.hiddenRunsOf || !session.sheetKey.split("\u0000").includes(name)) return false;
      return !layout.sameHiddenRuns(session.hiddenRows.get(name) ?? layout.NO_RUNS, layout.hiddenRunsOf(name));
    }

    return { sync, stale };
  }

  host.__EASYOFFICE_CREATE_SHEET_HIDDEN_ROWS_SYNC__ = createSheetHiddenRowsSync;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
