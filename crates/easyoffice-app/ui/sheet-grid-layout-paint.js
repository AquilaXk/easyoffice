(function (host) {
  "use strict";

  // The classes the sheet's layout puts on the materialized cells and headers (PD-194, PD-226, PD-232): hidden rows and columns,
  // the marks that show where a hidden run was, filter buttons, merged ranges and the totals row of a table.

  // The header right after a hidden run carries a thick edge on that side and the run's bounds
  // (`data-hidden-run="first-last"`), which a double click on the edge shows again.
  function markHiddenEdge(th, axis, run) {
    th.classList.toggle("hidden-edge-" + axis, Boolean(run));
    if (run) th.dataset.hiddenRun = run[0] + "-" + run[1];
    else if (th.dataset.hiddenRun) delete th.dataset.hiddenRun;
  }

  // The arguments are what the grid knows of its window: the cell and header maps, `seedDomMaps`, the column and row offsets and the layout
  // of the active sheet. Returns the function that brings the classes up to date.
  function createLayoutPaint({ layoutApi, layoutOf, getSheetName, tdByPacked, colHeadByCol, rowHeadByRow, seedDomMaps, colLeft, rowTop }) {
    let layoutApplied = false;

    // Hidden rows and columns get `sheet-hidden`; a merged range paints from its anchor cell
    // (`merge-anchor` with its size in `--mw`/`--mh`) over the cells it covers (`merge-cover`).
    return function applyLayoutClasses() {
      const layout = layoutOf();
      const empty = !layout || layoutApi.isEmpty(layout);
      if (empty && !layoutApplied) return;
      layoutApplied = !empty;
      const name = getSheetName();
      const runs = layout?.hiddenRows || [];
      const colRuns = layout?.hiddenCols || [];
      const filter = layout?.filter || null;
      const filterCols = filter ? new Set(filter.columns.map((x) => x.col)) : null;
      const tableStyle = layout?.tables?.length > 0 ? host.__EASYOFFICE_SHEET_TABLE_STYLE__ : null;
      const filtered = filter !== null || tableStyle !== null;
      const hasColRuns = !empty && colRuns.length > 0;
      const hiddenColSet = hasColRuns ? new Set() : null;
      for (const [c, th] of colHeadByCol) {
        const hid = hasColRuns && layoutApi.colHidden(name, c);
        if (hid) hiddenColSet.add(c);
        th.classList.toggle("sheet-hidden", hid);
        markHiddenEdge(th, "col", hasColRuns ? layoutApi.runAt(colRuns, c - 1) : null);
      }
      const skipped = filter !== null && filter.hidden.length > 0;
      for (const [r, th] of rowHeadByRow) {
        markHiddenEdge(th, "row", empty ? null : layoutApi.runAt(runs, r - 1));
        th.classList.toggle("filter-row", skipped && r > filter.range.r1 && r <= filter.range.r2);
      }
      seedDomMaps();
      const hasMerges = Boolean(!empty && layout?.merges && layout.merges.length > 0);
      for (const td of tdByPacked.values()) {
        const r = td._row;
        const c = td._col;
        td.classList.toggle("sheet-hidden", hiddenColSet ? hiddenColSet.has(c) : false);
        // The header cells of the sheet's filter and of the tables with filter buttons carry the button (PD-226, PD-232); a button on
        // each header cell of a table that has them (the reference of the table filter, `THIRD_PARTY_NOTICES.md`).
        // The panel opens by the same answer (`filterOwnerAt`).
        const owner = filtered ? layoutApi.filterOwnerAt(name, r, c) : undefined;
        const head = owner !== undefined;
        const on = head && (owner === null ? filterCols.has(c) : layoutApi.tablesOf(name).find((t) => t.name === owner).filter.some((x) => x.col === c));
        td.classList.toggle("filter-head", head);
        td.classList.toggle("filter-on", on);
        // The totals-row cells of a table carry the function list's arrow while selected (PD-232).
        const table = tableStyle?.tableAt(name, r, c);
        td.classList.toggle("totals-cell", Boolean(table && table.totals === true && layoutApi.tableBox(table).r2 === r));
        const m = hasMerges ? layoutApi.mergeAt(name, r, c) : null;
        const anchor = Boolean(m) && m.r1 === r && m.c1 === c;
        td.classList.toggle("merge-anchor", anchor);
        td.classList.toggle("merge-cover", Boolean(m) && !anchor);
        if (anchor) {
          td.style.setProperty("--mw", colLeft(m.c2 + 1) - colLeft(m.c1) + "px");
          td.style.setProperty("--mh", rowTop(m.r2 + 1) - rowTop(m.r1) + "px");
        } else if (td.style.getPropertyValue("--mw")) {
          td.style.removeProperty("--mw");
          td.style.removeProperty("--mh");
        }
      }
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_GRID_LAYOUT_PAINT__ = createLayoutPaint;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
