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
      sheetHistory,
      restoreWorkspaceSnapshot,
      viewLive,
      ui,
    } = deps;
    const viewState = host.__EASYOFFICE_SHEET_VIEW_STATE__;

    function say(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    // ---- what the strip shows ---------------------------------------------------------------------
    const sheetKeys = () => [...new Set([...Object.keys(sheetDataStore), getActiveSheetName()])];
    const allSheets = () => strip.reconcile(sheetKeys());
    const shownSheets = () => strip.visibleOf(sheetKeys());
    // The sheets 숨기기 취소 offers: a very hidden sheet (a file's) is not among them (PD-209).
    const hiddenSheets = () => allSheets().filter((name) => layoutApi.hiddenOf(name) && !layoutApi.veryHiddenOf(name));

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
        moveCopy: open && at >= 0,
        moveLeft: open && at > 0,
        moveRight: open && at >= 0 && at < shown.length - 1,
        hide: open && shown.length > 1 && at >= 0,
        unhide: open && hiddenSheets().length > 0,
      };
    }

    // The status line says so when the workbook's structure is protected.
    const structureBlocked = () => host.__EASYOFFICE_SHEET_PROTECT__?.guardStructure() === false;

    // ---- one undoable step ------------------------------------------------------------------------
    // `work(next)` changes `next`, the sheet state of the workspace whose per-sheet maps are the model's own: it
    // replaces a map or an entry and never edits one in place. It returns the status text, or false to stop with
    // nothing changed. `renames` ({ from: to }) names the sheets the step renames.
    function change(work, renames = {}) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        sheetDataStore[getActiveSheetName()] = snapshotDomSheetCells();
        viewLive.capture();
        const before = getActiveSheetName();
        const record = sheetHistory.begin({ sheets: "all", parts: "all", sync: false });
        const state = sheetHistory.current();
        const next = {
          ...state,
          sheets: { ...sheetDataStore },
          sheetColumnWidths: { ...state.sheetColumnWidths },
          sheetRules: { ...state.sheetRules },
          sheetLayout: { ...state.sheetLayout },
          sheetOrder: [...state.sheetOrder],
        };
        const text = await work(next);
        if (typeof text !== "string") return false;
        await restoreWorkspaceSnapshot(next, null, { adopt: true });
        // A sheet the window now shows is shown as it was left, a copy as its source was (PD-208).
        if (getActiveSheetName() !== before) viewLive.restore();
        sheetHistory.commit(record, { renames });
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
      const linkApi = host.__EASYOFFICE_SHEET_LINK__;
      const linkField = linkApi?.requestField(sheetDataStore) ?? {};
      if (!sheetStoreHelpers.sheetMapHasAnyFormula(sheetDataStore) && !rulesApi?.hasAny() && !host.__EASYOFFICE_SHEET_NAMES__?.hasAny() && !linkField.linkTargets) {
        return { ok: true, answer: { formulas: [], rules: {} } };
      }
      const result = await sheetStoreHelpers.rewriteSheetRefs(sheetDataStore, { ...request, rules: rulesRequest(), ...linkField });
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
      // The in-workbook targets of the cell links follow the sheet (PD-218).
      if (answer.linkTargets) next.sheets = host.__EASYOFFICE_SHEET_LINK__.retarget(next.sheets, answer.linkTargets);
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
        viewState.remove(sheet);
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
        viewState.rename(sheet, to);
        return "시트 이름 바꿈: " + to;
      }, { [sheet]: to });
      if (done) ui.focusTab(to);
      return done;
    }

    // `next` with a copy of `sheet` under a name of its own; returns the name. Its formulas keep their text: a
    // reference without a sheet name now points into the copy, one with a name still points where it did. The
    // sheet's own names (those scoped to it) are copied to the copy and refer to it (PD-214).
    function cloneInto(next, sheet) {
      const name = names.copyName(names.labelOf(sheet), next.sheetOrder);
      next.sheets[name] = structuredClone(next.sheets[sheet] || {});
      if (next.sheetRowCounts[sheet]) next.sheetRowCounts[name] = next.sheetRowCounts[sheet];
      if (next.sheetColumnWidths[sheet]) next.sheetColumnWidths[name] = { ...next.sheetColumnWidths[sheet] };
      if (next.sheetRules[sheet]) next.sheetRules[name] = structuredClone(next.sheetRules[sheet]);
      if (next.sheetLayout[sheet]) {
        const { hidden, veryHidden, ...kept } = structuredClone(next.sheetLayout[sheet]);
        // Table names are unique in the workbook: the copy's tables get names of their own (PD-224).
        if (kept.tables) kept.tables = layoutApi.copiedTables(kept.tables, next.sheetLayout);
        if (Object.keys(kept).length > 0) next.sheetLayout[name] = kept;
      }
      const scoped = host.__EASYOFFICE_SHEET_NAMES__.copiedFor(next.definedNames || [], sheet, name);
      if (scoped.length > 0) next.definedNames = [...next.definedNames, ...scoped];
      viewState.copy(sheet, name);
      return name;
    }

    // The copy goes right after its source and becomes the active sheet.
    function copy(sheet) {
      if (structureBlocked()) return Promise.resolve(false);
      if (!can(sheet).copy) {
        say("시트는 " + names.MAX_SHEETS + "개까지");
        return Promise.resolve(false);
      }
      return change((next) => {
        const name = cloneInto(next, sheet);
        place(next, name, next.sheetOrder.indexOf(sheet) + 1);
        activate(next, name);
        return "시트 복사: " + names.labelOf(name);
      });
    }

    // 이동/복사: `sheet`, or a copy of it, goes before the sheet `before` (null for the end) and is the active sheet.
    function moveOrCopy(sheet, before, makeCopy) {
      if (structureBlocked()) return Promise.resolve(false);
      if (makeCopy && !can(sheet).copy) {
        say("시트는 " + names.MAX_SHEETS + "개까지");
        return Promise.resolve(false);
      }
      return change((next) => {
        if (before !== null && !next.sheetOrder.includes(before)) return false;
        if (!makeCopy && before === sheet) return false;
        const name = makeCopy ? cloneInto(next, sheet) : sheet;
        next.sheetOrder = next.sheetOrder.filter((n) => n !== name);
        next.sheetOrder.splice(before === null ? next.sheetOrder.length : next.sheetOrder.indexOf(before), 0, name);
        if (makeCopy || next.activeSheetName !== name) activate(next, name);
        return (makeCopy ? "시트 복사: " : "시트 이동: ") + names.labelOf(name);
      });
    }

    // The dialog of 이동/복사 for `sheet`: the sheets with a tab to go before, and the choice of a copy.
    async function movePlace(sheet) {
      if (structureBlocked()) return false;
      if (!can(sheet).moveCopy) return false;
      const shown = shownSheets();
      const after = shown[shown.indexOf(sheet) + 1] ?? null;
      const answer = await ui.pickPlace(shown, after);
      return answer ? moveOrCopy(sheet, answer.before, answer.copy) : false;
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
      "tab-move-copy": () => movePlace(getActiveSheetName()),
      "tab-left": () => move(getActiveSheetName(), -1),
      "tab-right": () => move(getActiveSheetName(), 1),
      "tab-hide": () => hide(getActiveSheetName()),
      "tab-unhide": () => unhide(),
    };

    return { can, insert, remove, rename, checkRename, copy, moveOrCopy, movePlace, move, moveTo, hide, unhide, actions, shownSheets, hiddenSheets };
  }

  host.__EASYOFFICE_CREATE_SHEET_TABS__ = createSheetTabs;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
