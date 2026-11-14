(function (host) {
  "use strict";

  // Undo steps of the sheet that keep what a change replaced, not the workbook (PD-221). A step is a record:
  //   { kind: "delta", view, sheets: [{ name, delta, absent?, alias? }], parts, shift?, bytes, isRibbon?, ribbonModule? }
  // `sheets` lists the cells (a `sheet-delta.js` delta) of each sheet as they were before the change, `absent` is a sheet
  // that did not exist, `alias` the name a renamed sheet has now; `parts` are the small whole-sheet settings (layout, rules,
  // names, widths, row counts, tab order) as they were; `shift` is the row or column edit that leads back; `view` the
  // active module, sheet and cell. Undoing a record writes it and builds the record of what it replaced, which is the redo.
  //
  // Reference implementation: the diff list of the reference spreadsheet engine B (Apache-2.0 or MIT; `base/src/user_model`
  // `history.rs`, commit d50f26e0bc618271c0f3b398238b04ad6ba12c05): one list of changes per user action, each holding the old
  // value, applied in reverse to undo; the row and column variants keep the cells they delete. Here the list is the cell
  // delta of each sheet plus the settings that changed, and the step the engine B stores as "new value" is built when the
  // record is applied (so a redo always holds what the undo found). The rest follows the reference engine A, see
  // `sheet-delta.js`.

  const PART_READERS = {
    sheetRowCounts: (ctx) => ({ ...ctx.sheetRowCounts }),
    sheetColumnWidths: (ctx) => ctx.controller().getColumnWidths(),
    sheetRules: (ctx) => ctx.controller().getSheetRules(),
    sheetLayout: () => host.__EASYOFFICE_SHEET_LAYOUT__.getAll(),
    definedNames: () => host.__EASYOFFICE_SHEET_NAMES__.getAll(),
    sheetOrder: (ctx) => host.__EASYOFFICE_SHEET_TAB_STRIP__.reconcile(Object.keys(ctx.sheetDataStore)),
  };
  const ALL_PARTS = Object.keys(PART_READERS);
  const rawOf = (e) => e?.formula || e?.value || "";

  function createSheetHistory(deps) {
    const { sheetDataStore, sheetRowCounts, getActiveModule, getActiveSheetName, getActiveSheetCellRef, getController, push } = deps;
    const D = host.__EASYOFFICE_SHEET_DELTA__;
    const ctx = { sheetDataStore, sheetRowCounts, controller: getController };

    const readView = () => ({ activeModule: getActiveModule(), activeSheetName: getActiveSheetName(), activeSheetCellRef: getActiveSheetCellRef() });

    function readParts(names) {
      const out = {};
      for (const name of names) out[name] = PART_READERS[name](ctx);
      return out;
    }

    // The part values that differ now from `before`: a part nobody changed is not kept.
    function changedParts(before) {
      const out = {};
      for (const name of Object.keys(before)) {
        const now = PART_READERS[name](ctx);
        if (JSON.stringify(now) !== JSON.stringify(before[name])) out[name] = before[name];
      }
      return out;
    }

    const bytesOfParts = (parts) => JSON.stringify(parts).length * 2;

    // ---- recording ---------------------------------------------------------------------------------------------
    // Starts a change. `scope` names what it may touch:
    //   refs: [ref], range: { r1, c1, r2, c2 }  cells of `sheet` (the active sheet by default), kept as they are now;
    //   bySheet: { sheet: [ref] }               cells of several sheets;
    //   addsSheets: true                        the change may add sheets (their undo removes them);
    //   sheets: "all"                           every cell of every sheet (a change whose cells are not known before);
    //   parts: [name] | "all"                   the settings (names of PART_READERS);
    //   shift: { axis, op, at, count }          the row or column edit the change makes;
    //   sync: false                             leave the page's window out of the model (a cell the page has not changed).
    // `before` ({ ref: entry }) gives the entries a cell held before it was edited, for the cell being typed in, whose
    // text the page has not merged into the model yet. The page's window is merged into the model first, so the cells
    // captured are the cells as the user sees them.
    function begin(scope = {}) {
      if (scope.sync !== false) getController().snapshotDomSheetCells();
      const sheet = scope.sheet ?? getActiveSheetName();
      const pending = { scope, sheet, view: readView(), cells: new Map(), listed: null, parts: null, done: false };
      if (scope.sheets === "all" || scope.addsSheets) pending.listed = new Set(Object.keys(sheetDataStore));
      if (scope.sheets === "all") {
        for (const name of pending.listed) pending.cells.set(name, D.captureAll(sheetDataStore[name]));
      } else if (scope.bySheet) {
        for (const [name, refs] of Object.entries(scope.bySheet)) pending.cells.set(name, D.captureRefs(sheetDataStore[name], refs));
      } else if (scope.refs) {
        pending.cells.set(sheet, D.captureRefs(scope.before ?? sheetDataStore[sheet], scope.refs));
      } else if (scope.range) {
        pending.cells.set(sheet, D.captureRange(sheetDataStore[sheet], scope.range));
      }
      const names = scope.parts === "all" ? ALL_PARTS : scope.parts || [];
      if (names.length > 0) pending.parts = readParts(names);
      return pending;
    }

    // Ends the change and pushes its record. `compact: false` keeps every cell named (the caller knows they changed).
    // `renames`: { from: to } for sheets the change renamed. `ribbonModule`: the step belongs to a ribbon command of that
    // module (the engine keeps a step for it too). Returns the record.
    function commit(pending, { compact = true, renames = {}, ribbonModule } = {}) {
      if (pending.done) return null;
      pending.done = true;
      const sheets = [];
      const reverse = Object.fromEntries(Object.entries(renames).map(([from, to]) => [to, from]));
      const shift = pending.scope.shift;
      for (const [name, pre] of pending.cells) {
        const now = renames[name] ?? name;
        const map = sheetDataStore[now];
        let delta = pre;
        if (compact && map) {
          const scan = pending.scope.sheets === "all" ? () => true : pending.scope.range ? (row, col) => inRange(pending.scope.range, row, col) : null;
          delta = D.reconcile(pre, map, { edit: shift && shift.sheet === name ? shift : null, scan });
        }
        // The sheet an edit moves is always listed: the record moves its cells back even when none of them changed.
        if (delta.n > 0 || !map || now !== name || shift?.sheet === name) sheets.push({ name, delta, ...(now !== name ? { alias: now } : {}) });
      }
      if (pending.listed) {
        for (const name of Object.keys(sheetDataStore)) {
          if (!pending.listed.has(name) && !(name in reverse)) sheets.push({ name, delta: D.emptyDelta(), absent: true });
        }
      }
      const parts = pending.parts ? changedParts(pending.parts) : {};
      const record = {
        kind: "delta",
        view: pending.view,
        sheets,
        parts,
        ...(shift ? { shift: D.inverseEdit(shift) } : {}),
        ...(ribbonModule ? { isRibbon: true, ribbonModule } : {}),
        bytes: 256 + bytesOfParts(parts),
      };
      for (const entry of record.sheets) record.bytes += entry.delta.bytes;
      push(record);
      return record;
    }

    const inRange = (rg, row, col) => row >= rg.r1 && row <= rg.r2 && col >= rg.c1 && col <= rg.c2;

    // ---- applying ----------------------------------------------------------------------------------------------
    // The cells an array filled are results, not part of a step: an array the step overwrites forgets its cells, and the
    // calculation after the step fills the ones of the formula that comes back (PD-217).
    function forgetAreas(map, delta) {
      const spill = host.__EASYOFFICE_SHEET_SPILL__;
      if (!spill) return;
      for (const ref of D.refsOf(delta)) if (map[ref]?.spillRows > 0) spill.clearArea(map, ref);
    }

    // Writes the cells of one sheet entry; returns the entry that leads back.
    function swapSheet(entry, shift, notes) {
      const now = entry.alias ?? entry.name;
      if (entry.absent) {
        const map = sheetDataStore[now];
        delete sheetDataStore[now];
        notes.extent.set(now, { written: [], removed: [] });
        return { name: now, delta: D.captureAll(map || {}) };
      }
      let map = sheetDataStore[now];
      const existed = map !== undefined;
      if (!existed) map = {};
      let current = D.emptyDelta();
      if (shift && shift.sheet === entry.name) {
        // The cells now sit where the edit leads from: they are read there before the edit moves them.
        const here = D.inverseEdit(shift);
        const refs = D.refsOf(entry.delta).map((ref) => D.shiftedRef(here, ref)).filter((ref) => ref !== null);
        current = D.captureRefs(map, refs);
        const moved = D.shiftMap(map, shift);
        map = moved.map;
        current = D.concat(current, moved.gone);
      }
      if (entry.alias && existed) delete sheetDataStore[entry.alias];
      sheetDataStore[entry.name] = map;
      const undone = shift && shift.sheet === entry.name ? current : null;
      forgetAreas(map, entry.delta);
      const replaced = D.apply(map, entry.delta);
      notes.extent.set(entry.name, { ...D.extentChange(entry.delta), moved: shift?.sheet === entry.name });
      return { name: now, ...(entry.alias ? { alias: entry.name } : {}), ...(existed ? {} : { absent: true }), delta: undone ?? replaced };
    }

    // The protection each sheet has now, under the name it has once the record is applied (a renamed sheet comes back under
    // its old name, a sheet the record deletes has none). A sheet the record brings back is not listed: the protection of the
    // record is its own. Read before the cells move, because the layout is still keyed by the names of now.
    function protectionNow(record) {
      const now = host.__EASYOFFICE_SHEET_LAYOUT__.getAll();
      const renamedTo = {};
      const deleted = new Set();
      for (const entry of record.sheets) {
        if (entry.absent) deleted.add(entry.alias ?? entry.name);
        else if (entry.alias) renamedTo[entry.alias] = entry.name;
      }
      const out = {};
      for (const name of Object.keys(sheetDataStore)) if (!deleted.has(name)) out[renamedTo[name] ?? name] = now[name]?.protect ?? null;
      return out;
    }

    // Applies `record` to the page's model and returns the record that undoes it (its redo). The settings of the record are
    // put back by the caller (`snapshotOf`), after the cells.
    function swap(record) {
      const notes = { extent: new Map(), protection: protectionNow(record) };
      const inverse = { kind: "delta", view: readView(), sheets: [], parts: readParts(Object.keys(record.parts)), bytes: 256 };
      if (record.isRibbon) Object.assign(inverse, { isRibbon: true, ribbonModule: record.ribbonModule });
      if (record.shift) inverse.shift = D.inverseEdit(record.shift);
      for (const entry of record.sheets) inverse.sheets.push(swapSheet(entry, record.shift, notes));
      inverse.bytes += bytesOfParts(inverse.parts);
      for (const entry of inverse.sheets) inverse.bytes += entry.delta.bytes;
      return { inverse, notes };
    }

    // The settings of `record` as a partial workspace snapshot for `restoreWorkspaceSnapshot`; `notes` come from `swap`.
    function snapshotOf(record, notes) {
      const snap = { scope: "sheet", ...record.view, ...record.parts, protectionNow: notes.protection };
      delete snap.sheetRowCounts;
      return snap;
    }

    function restoreRowCounts(record) {
      const counts = record.parts.sheetRowCounts;
      if (!counts) return;
      for (const key of Object.keys(sheetRowCounts)) delete sheetRowCounts[key];
      Object.assign(sheetRowCounts, counts);
    }

    // Whether applying `record` would change a locked cell of a sheet that is protected now. Formula cells compare by their
    // text, because their value follows the cells they read. The cells of a row or column edit are the edit's own and are
    // not compared (PD-216).
    function wouldChange(record) {
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      const { parseRef } = host.__EASYOFFICE_SHEET_REFS__;
      for (const entry of record.sheets) {
        const name = entry.alias ?? entry.name;
        if (entry.absent || !guard.isOn(name) || record.shift?.sheet === entry.name) continue;
        const map = sheetDataStore[name] || {};
        for (const [ref, then] of D.cells(entry.delta)) {
          const now = map[ref];
          if (rawOf(now) === rawOf(then)) continue;
          const at = parseRef(ref);
          if (at && !guard.cellEditable(name, at.row, at.col, now?.style)) return true;
        }
      }
      return false;
    }

    // The sheet's state as a partial snapshot, for a change that builds the state it leads to from this one.
    const current = () => ({ scope: "sheet", ...readView(), ...readParts(ALL_PARTS) });

    return { begin, commit, swap, snapshotOf, restoreRowCounts, wouldChange, current };
  }

  host.__EASYOFFICE_CREATE_SHEET_HISTORY__ = createSheetHistory;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
