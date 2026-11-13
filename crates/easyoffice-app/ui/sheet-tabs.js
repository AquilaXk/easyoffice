(function (host) {
  "use strict";

  // The commands of a sheet tab (PD-209): insert, delete, rename, copy, move, hide and show again.
  // Each one is ONE undoable step built the way a file open is: the snapshot before is the undo
  // step, the snapshot after is made from it and restored through the same path as an undo, so
  // every per-sheet store (cells, row counts, widths, rules, layout, the tab order) follows. The
  // formulas and rules that name a renamed or deleted sheet are rewritten by the engine
  // (`sheetRefs`, the same session the row and column edits use).

  const names = host.__EASYOFFICE_SHEET_TAB_NAMES__;
  const strip = host.__EASYOFFICE_SHEET_TAB_STRIP__;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const rulesApi = host.__EASYOFFICE_SHEET_RULES__;

  const FAILURES = {
    limit: "시트가 커서 실행할 수 없음",
    structure: "바뀌는 수식이 너무 많음",
    error: "실행하지 못함",
  };

  // `map` with the key `from` named `to`, at the same place.
  function renamedKey(map, from, to) {
    const out = {};
    for (const key of Object.keys(map)) out[key === from ? to : key] = map[key];
    return out;
  }

  function withoutKey(map, name) {
    const out = { ...map };
    delete out[name];
    return out;
  }

  function createSheetTabs(deps) {
    const {
      document,
      sheetDataStore,
      sheetStoreHelpers,
      getActiveSheetName,
      commitIfEditing,
      snapshotDomSheetCells,
      recalculateActiveSheetFormulas,
      refreshChrome,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
      restoreWorkspaceSnapshot,
      ui,
    } = deps;

    function say(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    // ---- what the strip shows ---------------------------------------------------------------------
    const sheetKeys = () => [...new Set([...Object.keys(sheetDataStore), getActiveSheetName()])];
    const allSheets = () => strip.reconcile(sheetKeys());
    const shownSheets = () => strip.visibleOf(sheetKeys());
    const hiddenSheets = () => allSheets().filter((name) => layoutApi.hiddenOf(name));

    // What each command may do now; the menus grey the rest out.
    // A workbook whose structure is protected (PD-216) allows none of them.
    function can(sheet) {
      const shown = shownSheets();
      const at = shown.indexOf(sheet);
      const open = !host.__EASYOFFICE_SHEET_PROTECT__?.structureLocked();
      return {
        insert: open && allSheets().length < names.MAX_SHEETS,
        copy: open && allSheets().length < names.MAX_SHEETS,
        delete: open && shown.length > 1 && at >= 0,
        rename: open && at >= 0,
        moveLeft: open && at > 0,
        moveRight: open && at >= 0 && at < shown.length - 1,
        hide: open && shown.length > 1 && at >= 0,
        unhide: open && hiddenSheets().length > 0,
      };
    }

    // The status line says so when the workbook's structure is protected.
    const structureBlocked = () => host.__EASYOFFICE_SHEET_PROTECT__?.guardStructure() === false;

    // ---- one undoable step ------------------------------------------------------------------------
    // `work(next, before)` changes `next`, a copy of the workspace snapshot `before` whose per-sheet
    // maps are new objects (their entries are shared, so it replaces an entry and never edits one),
    // and returns the status text, or false to stop with nothing changed.
    function change(work) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        sheetDataStore[getActiveSheetName()] = snapshotDomSheetCells();
        const before = captureWorkspaceSnapshot(false);
        const next = {
          ...before,
          sheets: { ...before.sheets },
          sheetRowCounts: { ...before.sheetRowCounts },
          sheetColumnWidths: { ...before.sheetColumnWidths },
          sheetRules: { ...before.sheetRules },
          sheetLayout: { ...before.sheetLayout },
          sheetOrder: [...before.sheetOrder],
        };
        const text = await work(next, before);
        if (typeof text !== "string") return false;
        pushRawUndoSnapshot(before);
        await restoreWorkspaceSnapshot(next);
        await recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
        refreshChrome();
        say(text);
        return true;
      });
    }

    const hiddenIn = (next, name) => next.sheetLayout[name]?.hidden === true;
    const shownIn = (next) => next.sheetOrder.filter((name) => !hiddenIn(next, name));

    // The sheet to show when `name` goes: the next tab, else the one before it.
    function neighbourOf(next, name) {
      const shown = shownIn(next);
      const at = shown.indexOf(name);
      return shown[at + 1] ?? shown[at - 1] ?? shown[0];
    }

    function activate(next, name) {
      next.activeSheetName = name;
      next.activeSheetCellRef = "A1";
    }

    function place(next, name, index) {
      next.sheetOrder = next.sheetOrder.filter((n) => n !== name);
      next.sheetOrder.splice(index < 0 ? next.sheetOrder.length : index, 0, name);
    }

    // ---- the engine rewrites the references to a renamed or deleted sheet -------------------------
    function rulesRequest() {
      const out = {};
      for (const sheet of Object.keys(sheetDataStore)) {
        if (rulesApi && !rulesApi.isEmpty(rulesApi.get(sheet))) out[sheet] = JSON.parse(rulesApi.payloadFor(sheet));
      }
      return out;
    }

    // `{ ok, answer }`; a workbook with no formula and no rule has nothing to rewrite, so it needs no
    // engine. A failure says why in the status text.
    async function rewriteRefs(request) {
      if (!sheetStoreHelpers.sheetMapHasAnyFormula(sheetDataStore) && !rulesApi?.hasAny() && !host.__EASYOFFICE_SHEET_NAMES__?.hasAny()) {
        return { ok: true, answer: { formulas: [], rules: {} } };
      }
      const result = await sheetStoreHelpers.rewriteSheetRefs(sheetDataStore, { ...request, rules: rulesRequest() });
      if (!result.ok) say(result.message || FAILURES[result.reason] || FAILURES.error);
      return result;
    }

    // The rewritten formula texts and rules of the answer, in `next`.
    function applyRewrites(next, answer) {
      const own = new Set();
      for (const f of Array.isArray(answer.formulas) ? answer.formulas : []) {
        const entry = next.sheets[f.sheet]?.[f.cell];
        if (!entry) continue;
        if (!own.has(f.sheet)) {
          next.sheets[f.sheet] = { ...next.sheets[f.sheet] };
          own.add(f.sheet);
        }
        next.sheets[f.sheet][f.cell] = { ...entry, formula: f.value };
      }
      if (answer.rules && Object.keys(answer.rules).length > 0) next.sheetRules = { ...next.sheetRules, ...answer.rules };
      // The names that pointed at the sheet follow it, a deleted sheet's own names go (PD-214).
      if (Array.isArray(answer.names)) next.definedNames = answer.names;
    }

    // ---- the commands -----------------------------------------------------------------------------
    // A new empty sheet before `before`, or after the last one; it becomes the active sheet.
    function insert(before) {
      if (structureBlocked()) return Promise.resolve(false);
      if (!can(before).insert) {
        say("시트는 " + names.MAX_SHEETS + "개까지");
        return Promise.resolve(false);
      }
      return change((next) => {
        const name = names.nextDefault(next.sheetOrder);
        next.sheets[name] = {};
        place(next, name, before ? next.sheetOrder.indexOf(before) : -1);
        activate(next, name);
        return "시트 삽입: " + names.labelOf(name);
      });
    }

    async function remove(sheet) {
      if (structureBlocked()) return false;
      if (!can(sheet).delete) {
        say("마지막 시트는 삭제할 수 없음");
        return false;
      }
      if (!(await ui.confirmDelete(names.labelOf(sheet)))) return false;
      return change(async (next) => {
        const result = await rewriteRefs({ op: "delete", sheet });
        if (!result.ok) return false;
        if (next.activeSheetName === sheet) activate(next, neighbourOf(next, sheet));
        applyRewrites(next, result.answer);
        next.sheets = withoutKey(next.sheets, sheet);
        next.sheetRowCounts = withoutKey(next.sheetRowCounts, sheet);
        next.sheetColumnWidths = withoutKey(next.sheetColumnWidths, sheet);
        next.sheetRules = withoutKey(next.sheetRules, sheet);
        next.sheetLayout = withoutKey(next.sheetLayout, sheet);
        next.sheetOrder = next.sheetOrder.filter((n) => n !== sheet);
        return "시트 삭제: " + names.labelOf(sheet);
      });
    }

    // The result of asking for `newName` for `sheet`: "" when it is fine, else the reason.
    const checkRename = (sheet, newName) =>
      names.labelOf(sheet) === newName || sheet === newName ? "" : names.checkName(newName, allSheets(), sheet);

    async function rename(sheet, newName) {
      if (structureBlocked()) return false;
      const to = String(newName).trim();
      if (to === names.labelOf(sheet) || to === sheet) return true;
      const problem = checkRename(sheet, to);
      if (problem) {
        say(problem);
        return false;
      }
      const done = await change(async (next) => {
        const result = await rewriteRefs({ op: "rename", sheet, newName: to });
        if (!result.ok) return false;
        applyRewrites(next, result.answer);
        next.sheets = renamedKey(next.sheets, sheet, to);
        next.sheetRowCounts = renamedKey(next.sheetRowCounts, sheet, to);
        next.sheetColumnWidths = renamedKey(next.sheetColumnWidths, sheet, to);
        next.sheetRules = renamedKey(next.sheetRules, sheet, to);
        next.sheetLayout = renamedKey(next.sheetLayout, sheet, to);
        next.sheetOrder = next.sheetOrder.map((n) => (n === sheet ? to : n));
        if (next.activeSheetName === sheet) next.activeSheetName = to;
        return "시트 이름 바꿈: " + to;
      });
      if (done) ui.focusTab(to);
      return done;
    }

    // The copy goes right after its source and becomes the active sheet. Its formulas keep their
    // text: a reference without a sheet name now points into the copy, one with a name still
    // points where it did.
    function copy(sheet) {
      if (structureBlocked()) return Promise.resolve(false);
      if (!can(sheet).copy) {
        say("시트는 " + names.MAX_SHEETS + "개까지");
        return Promise.resolve(false);
      }
      return change((next) => {
        const name = names.copyName(names.labelOf(sheet), next.sheetOrder);
        next.sheets[name] = structuredClone(next.sheets[sheet] || {});
        if (next.sheetRowCounts[sheet]) next.sheetRowCounts[name] = next.sheetRowCounts[sheet];
        if (next.sheetColumnWidths[sheet]) next.sheetColumnWidths[name] = { ...next.sheetColumnWidths[sheet] };
        if (next.sheetRules[sheet]) next.sheetRules[name] = structuredClone(next.sheetRules[sheet]);
        if (next.sheetLayout[sheet]) {
          const { hidden, ...kept } = structuredClone(next.sheetLayout[sheet]);
          if (Object.keys(kept).length > 0) next.sheetLayout[name] = kept;
        }
        place(next, name, next.sheetOrder.indexOf(sheet) + 1);
        activate(next, name);
        return "시트 복사: " + names.labelOf(name);
      });
    }

    // `sheet` takes the place of the tab that is now at visible position `target`.
    function moveTo(sheet, target) {
      if (structureBlocked()) return Promise.resolve(false);
      return change((next) => {
        const shown = shownIn(next);
        const from = shown.indexOf(sheet);
        const dest = shown[target];
        if (from < 0 || dest === undefined || from === target) return false;
        const after = target > from ? 1 : 0;
        next.sheetOrder = next.sheetOrder.filter((n) => n !== sheet);
        next.sheetOrder.splice(next.sheetOrder.indexOf(dest) + after, 0, sheet);
        return "시트 이동: " + names.labelOf(sheet);
      });
    }

    const move = (sheet, delta) => {
      const at = shownSheets().indexOf(sheet);
      return at < 0 ? Promise.resolve(false) : moveTo(sheet, at + delta);
    };

    function hide(sheet) {
      if (structureBlocked()) return Promise.resolve(false);
      if (!can(sheet).hide) {
        say("모든 시트를 숨길 수 없음");
        return Promise.resolve(false);
      }
      return change((next) => {
        if (next.activeSheetName === sheet) activate(next, neighbourOf(next, sheet));
        next.sheetLayout[sheet] = { ...next.sheetLayout[sheet], hidden: true };
        return "시트 숨김: " + names.labelOf(sheet);
      });
    }

    async function unhide() {
      if (structureBlocked()) return false;
      const picked = await ui.pickHidden(hiddenSheets());
      if (!picked || picked.length === 0) return false;
      return change((next) => {
        for (const sheet of picked) {
          const { hidden, ...kept } = next.sheetLayout[sheet] || {};
          if (Object.keys(kept).length > 0) next.sheetLayout[sheet] = kept;
          else delete next.sheetLayout[sheet];
        }
        return "시트 숨기기 취소: " + picked.map(names.labelOf).join(", ");
      });
    }

    // The same commands for the sheet menu and the command search: they act on the active sheet.
    const actions = {
      "tab-insert": () => insert(getActiveSheetName()),
      "tab-delete": () => remove(getActiveSheetName()),
      "tab-rename": () => Promise.resolve(ui.startRename(getActiveSheetName())),
      "tab-copy": () => copy(getActiveSheetName()),
      "tab-left": () => move(getActiveSheetName(), -1),
      "tab-right": () => move(getActiveSheetName(), 1),
      "tab-hide": () => hide(getActiveSheetName()),
      "tab-unhide": () => unhide(),
    };

    return { can, insert, remove, rename, checkRename, copy, move, moveTo, hide, unhide, actions, shownSheets, hiddenSheets };
  }

  host.__EASYOFFICE_CREATE_SHEET_TABS__ = createSheetTabs;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
