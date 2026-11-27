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
      focusAnchor,
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

    // Sheets targeted by a tab command: the active multi-sheet group when `sheet` belongs to it, otherwise just `[sheet]`.
    function targetsFor(sheet) {
      const shown = shownSheets();
      if (!shown.includes(sheet)) return [];
      if (strip.isGrouped() && strip.hasInGroup(sheet)) {
        const group = strip.getGroup(sheetKeys(), getActiveSheetName()).filter((name) => shown.includes(name));
        if (group.length > 0) return group;
      }
      return [sheet];
    }

    // What each command may do now; the menus grey the rest out.
    // A workbook whose structure is protected (PD-216) allows none of them.
    function can(sheet) {
      const shown = shownSheets();
      const at = shown.indexOf(sheet);
      const targets = at >= 0 ? targetsFor(sheet) : [];
      const open = !host.__EASYOFFICE_SHEET_PROTECT__?.structureLocked();
      const protect = host.__EASYOFFICE_SHEET_PROTECT__;
      const group = strip.getGroup(sheetKeys(), getActiveSheetName());
      return {
        insert: open && allSheets().length < names.MAX_SHEETS,
        copy: open && at >= 0 && allSheets().length + targets.length <= names.MAX_SHEETS,
        delete: open && at >= 0 && shown.length > targets.length,
        rename: open && at >= 0 && targets.length === 1,
        moveCopy: open && at >= 0,
        // The colour of a tab is off under a protected structure and if any target sheet is protected (PD-229, PD-346).
        tabColor: open && at >= 0 && targets.every((name) => !protect?.isOn(name)),
        hide: open && at >= 0 && shown.length > targets.length,
        unhide: open && hiddenSheets().length > 0,
        selectAll: open && shown.length > 1 && group.length < shown.length,
        ungroup: open && strip.isGrouped(),
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
        if (Object.keys(renames).length === 0 && (!document.activeElement || document.activeElement === document.body)) {
          focusAnchor?.();
        }
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

    // Puts the sheets of `next` in `order` (every sheet name, `moved` the ones the step moves). A 3D span whose end sheet
    // goes past its other end changes its text, so the engine, whose session still holds the order before, rewrites those
    // formulas first (PD-373). False, with the reason in the status text, when it cannot.
    async function reorder(next, order, moved) {
      const result = await rewriteRefs({ op: "move", sheets: moved, order }, next);
      if (!result.ok) return false;
      applyRewrites(next, result.answer);
      next.sheetOrder = order;
      return true;
    }

    // ---- the engine rewrites the references to a renamed or deleted sheet -------------------------
    function rulesRequest(rulesMap = null) {
      const out = {};
      if (rulesMap) {
        for (const [sheet, r] of Object.entries(rulesMap)) {
          const hasCf = Array.isArray(r?.cf) && r.cf.length > 0;
          const hasDv = Array.isArray(r?.dv) && r.dv.length > 0;
          if (!hasCf && !hasDv) continue;
          out[sheet] = {
            cf: (r.cf || []).map((rule, i) => (rule.p !== undefined ? rule : { ...rule, p: i + 1 })),
            dv: r.dv || [],
            ...(r.today !== undefined ? { today: r.today } : {}),
          };
        }
        return out;
      }
      for (const sheet of Object.keys(sheetDataStore)) {
        if (rulesApi && !rulesApi.isEmpty(rulesApi.get(sheet))) out[sheet] = JSON.parse(rulesApi.payloadFor(sheet));
      }
      return out;
    }

    // `{ ok, answer }`; a workbook with no formula and no rule has nothing to rewrite, so it needs no
    // engine. A failure says why in the status text.
    async function rewriteRefs(request, model = null) {
      const sheetsMap = model ? model.sheets : sheetDataStore;
      const linkApi = host.__EASYOFFICE_SHEET_LINK__;
      const linkField = linkApi?.requestField(sheetsMap) ?? {};
      const hasRules = model ? Object.keys(rulesRequest(model.sheetRules)).length > 0 : Boolean(rulesApi?.hasAny());
      const hasNames = model ? Array.isArray(model.definedNames) && model.definedNames.length > 0 : Boolean(host.__EASYOFFICE_SHEET_NAMES__?.hasAny());
      if (!sheetStoreHelpers.sheetMapHasAnyFormula(sheetsMap) && !hasRules && !hasNames && !linkField.linkTargets) {
        return { ok: true, answer: { formulas: [], rules: {} } };
      }
      const result = await sheetStoreHelpers.rewriteSheetRefs(sheetsMap, { ...request, rules: rulesRequest(model?.sheetRules), ...linkField });
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
        viewState.remove(name);
        place(next, name, before ? next.sheetOrder.indexOf(before) : -1);
        activate(next, name);
        return "시트 삽입: " + names.labelOf(name);
      });
    }

    async function remove(sheet) {
      if (structureBlocked()) return false;
      const shown = shownSheets();
      const targets = targetsFor(sheet);
      if (targets.length === 0 || !can(sheet).delete) {
        say(shown.length <= 1 ? "마지막 시트는 삭제할 수 없음" : "모든 보이는 시트는 삭제할 수 없음");
        return false;
      }
      if (!(await ui.confirmDelete(targets.map(names.labelOf).join(", ")))) return false;
      return change(async (next) => {
        const deleteSet = new Set(targets);
        const curShown = shownIn(next);
        const surviving = curShown.filter((n) => !deleteSet.has(n));
        if (surviving.length === 0) {
          say(curShown.length <= 1 ? "마지막 시트는 삭제할 수 없음" : "모든 보이는 시트는 삭제할 수 없음");
          return false;
        }
        const curAt = curShown.indexOf(next.activeSheetName);
        const fallbackActive = curShown.slice(curAt + 1).find((n) => !deleteSet.has(n))
          ?? curShown.slice(0, curAt).reverse().find((n) => !deleteSet.has(n))
          ?? surviving[0];
        const namesApi = host.__EASYOFFICE_SHEET_NAMES__;
        const savedNames = namesApi?.getAll?.() ?? null;
        try {
          for (const s of targets) {
            const result = await rewriteRefs({ op: "delete", sheet: s }, next);
            if (!result.ok) {
              if (savedNames) namesApi.replaceAll(savedNames, true);
              return false;
            }
            applyRewrites(next, result.answer);
            if (namesApi && Array.isArray(result.answer?.names)) namesApi.replaceAll(result.answer.names, true);
            next.sheets = withoutKey(next.sheets, s);
            next.sheetRowCounts = withoutKey(next.sheetRowCounts, s);
            next.sheetColumnWidths = withoutKey(next.sheetColumnWidths, s);
            next.sheetRules = withoutKey(next.sheetRules, s);
            next.sheetLayout = withoutKey(next.sheetLayout, s);
            next.sheetOrder = next.sheetOrder.filter((n) => n !== s);
          }
        } catch (err) {
          if (savedNames) namesApi.replaceAll(savedNames, true);
          throw err;
        }
        if (deleteSet.has(next.activeSheetName)) activate(next, fallbackActive);
        // The view of a deleted sheet stays: an undo brings the sheet back and it opens as it was left.
        return "시트 삭제: " + targets.map(names.labelOf).join(", ");
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
        strip.renameInGroup(sheet, to);
        // The old name keeps its view for the undo that gives the name back.
        viewState.copy(sheet, to);
        return "시트 이름 바꿈: " + to;
      }, { [sheet]: to });
      if (done) ui.focusTab(to);
      return done;
    }

    function collectRuleSites(rulesObj, mentionsAny, pushSite) {
      for (const r of rulesObj?.cf || []) {
        if (r.t === "expr" && mentionsAny(r.formula)) pushSite(r.formula, (v) => { r.formula = v; });
        if (r.t === "cellIs") {
          for (const k of ["a", "b"]) if (r[k]?.startsWith("=") && mentionsAny(r[k])) pushSite(r[k], (v) => { r[k] = v; });
        }
        for (const sv of [...(r.stops || []), r.min, r.max, ...(r.thresholds || [])]) {
          if (sv?.k === "formula" && mentionsAny(sv.v)) pushSite(sv.v, (v) => { sv.v = v; });
        }
      }
      for (const r of rulesObj?.dv || []) {
        if (r.t === "checkbox" || ((r.t === "list" || r.t === "listNoArrow") && !r.a?.startsWith("="))) continue;
        for (const k of ["a", "b"]) if (r[k] && mentionsAny(r[k])) pushSite(r[k], (v) => { r[k] = v; });
      }
    }

    // `next` with a copy of `sheet` under a name of its own; returns the name. Its formulas keep their text: a
    // reference without a sheet name now points into the copy, one with a name still points where it did, and a
    // structured reference on the copy that named one of `sheet`'s tables moves to the copy's table (PD-237).
    async function cloneInto(next, sheet) {
      const name = names.copyName(names.labelOf(sheet), next.sheetOrder);
      let copies = host.__EASYOFFICE_SHEET_NAMES__.copiedFor(next.definedNames || [], sheet, name);
      if (copies === null) {
        say("이름 정의가 한도를 넘어 복사할 수 없음");
        return null;
      }
      next.sheets[name] = structuredClone(next.sheets[sheet] || {});
      if (next.sheetRowCounts[sheet]) next.sheetRowCounts[name] = next.sheetRowCounts[sheet];
      if (next.sheetColumnWidths[sheet]) next.sheetColumnWidths[name] = { ...next.sheetColumnWidths[sheet] };
      if (next.sheetRules[sheet]) next.sheetRules[name] = structuredClone(next.sheetRules[sheet]);
      let pairs = [];
      if (next.sheetLayout[sheet]) {
        const { hidden, veryHidden, ...kept } = structuredClone(next.sheetLayout[sheet]);
        // Table names are unique in the workbook: the copy's tables get names of their own (PD-224).
        if (kept.tables) {
          const sourceTables = kept.tables;
          kept.tables = layoutApi.copiedTables(sourceTables, next.sheetLayout);
          pairs = sourceTables.map((t, i) => [t.name, kept.tables[i].name]).filter(([a, b]) => a.toLowerCase() !== b.toLowerCase());
        }
        if (Object.keys(kept).length > 0) next.sheetLayout[name] = kept;
      }
      if (pairs.length > 0) {
        const needles = pairs.map(([from]) => from.toLowerCase());
        const mentionsAny = (s) => {
          if (!s) return false;
          const lo = String(s).toLowerCase();
          return needles.some((n) => lo.includes(n));
        };
        const formulas = [];
        for (const [cell, entry] of Object.entries(next.sheets[name] || {})) {
          if (entry?.formula?.startsWith("=") && mentionsAny(entry.formula)) formulas.push({ sheet: name, cell, text: entry.formula });
        }
        const texts = [];
        const sites = [];
        const pushSite = (txt, set) => { texts.push(txt); sites.push(set); };
        for (const t of next.sheetLayout[name]?.tables || []) {
          (t.calculatedColumns || []).forEach((f, j) => {
            if (mentionsAny(f)) pushSite(f, (v) => { t.calculatedColumns[j] = v; });
          });
        }
        collectRuleSites(next.sheetRules[name], mentionsAny, pushSite);
        if (formulas.length > 0 || texts.length > 0 || copies.some((d) => mentionsAny(d.refersTo))) {
          try {
            const answer = await sheetStoreHelpers.callSheetAction({
              sessionAction: "tableColumn",
              op: "renameBatch",
              renames: pairs,
              tables: {},
              formulas,
              names: copies,
              ...(texts.length ? { texts } : {}),
            });
            for (const w of Array.isArray(answer?.rewrites) ? answer.rewrites : []) {
              if (next.sheets[name]?.[w.cell]) next.sheets[name][w.cell] = { ...next.sheets[name][w.cell], formula: w.text };
            }
            if (Array.isArray(answer?.names)) copies = answer.names;
            for (const [idx, val] of Array.isArray(answer?.textRewrites) ? answer.textRewrites : []) sites[idx]?.(val);
          } catch {
            // falls through when no engine IPC is attached
          }
        }
      }
      if (copies.length > 0) next.definedNames = [...next.definedNames, ...copies];
      viewState.copy(sheet, name);
      return name;
    }

    // 이동/복사: `sheet` (or its active group), or a copy of it, goes before `before` (null for the end).
    function moveOrCopy(sheet, before, makeCopy) {
      if (structureBlocked()) return Promise.resolve(false);
      const targets = targetsFor(sheet);
      if (targets.length === 0) return Promise.resolve(false);
      if (makeCopy && !can(sheet).copy) {
        say("시트는 " + names.MAX_SHEETS + "개까지");
        return Promise.resolve(false);
      }
      return change(async (next) => {
        if (before !== null && !next.sheetOrder.includes(before)) return false;
        if (targets.length === 1 && !makeCopy && before === sheet) return false;
        if (makeCopy) {
          const copies = [];
          let at = before === null ? next.sheetOrder.length : next.sheetOrder.indexOf(before);
          for (const s of targets) {
            const name = await cloneInto(next, s);
            if (name === null) return false;
            place(next, name, at);
            at += 1;
            copies.push(name);
          }
          activate(next, copies[0]);
          return "시트 복사: " + copies.map(names.labelOf).join(", ");
        }
        const targetSet = new Set(targets);
        let anchor = before;
        if (anchor !== null && targetSet.has(anchor)) {
          const start = next.sheetOrder.indexOf(anchor);
          anchor = next.sheetOrder.slice(start + 1).find((n) => !targetSet.has(n)) ?? null;
        }
        const order = next.sheetOrder.filter((n) => !targetSet.has(n));
        order.splice(anchor === null ? order.length : order.indexOf(anchor), 0, ...targets);
        // A move to where the sheets already are makes no step.
        if (order.every((n, i) => n === next.sheetOrder[i])) return false;
        if (!(await reorder(next, order, targets))) return false;
        if (!targetSet.has(next.activeSheetName)) activate(next, targets[0]);
        return "시트 이동: " + targets.map(names.labelOf).join(", ");
      });
    }

    // The dialog of 이동/복사 for `sheet`: the sheets with a tab to go before, and the choice of a copy.
    async function movePlace(sheet) {
      if (structureBlocked()) return false;
      if (!can(sheet).moveCopy) return false;
      const shown = shownSheets();
      const targets = targetsFor(sheet);
      const tail = targets[targets.length - 1] ?? sheet;
      const after = shown[shown.indexOf(tail) + 1] ?? null;
      const answer = await ui.pickPlace(shown, after);
      return answer ? moveOrCopy(sheet, answer.before, answer.copy) : false;
    }

    // `sheet` takes the place of the tab that is now at visible position `target`.
    function moveTo(sheet, target) {
      if (structureBlocked()) return Promise.resolve(false);
      return change(async (next) => {
        const shown = shownIn(next);
        const from = shown.indexOf(sheet);
        const dest = shown[target];
        if (from < 0 || dest === undefined || from === target) return false;
        const after = target > from ? 1 : 0;
        const order = next.sheetOrder.filter((n) => n !== sheet);
        order.splice(order.indexOf(dest) + after, 0, sheet);
        if (!(await reorder(next, order, [sheet]))) return false;
        return "시트 이동: " + names.labelOf(sheet);
      });
    }

    // The colour of the tab of `sheet` (or its active group) ("rrggbb", null for none), one undo step.
    function setTabColor(sheet, rgb) {
      if (structureBlocked()) return Promise.resolve(false);
      const targets = targetsFor(sheet);
      if (targets.length === 0 || !can(sheet).tabColor) {
        say("보호된 시트의 탭 색은 바꿀 수 없음");
        return Promise.resolve(false);
      }
      return change((next) => {
        let changed = false;
        for (const s of targets) {
          const { tabColor, ...kept } = next.sheetLayout[s] || {};
          if ((tabColor || null) === (rgb || null)) continue;
          changed = true;
          if (rgb) kept.tabColor = rgb;
          if (Object.keys(kept).length > 0) next.sheetLayout[s] = kept;
          else delete next.sheetLayout[s];
        }
        if (!changed) return false;
        const label = targets.map(names.labelOf).join(", ");
        return rgb ? "탭 색: " + label : "탭 색 없음: " + label;
      });
    }

    function hide(sheet) {
      if (structureBlocked()) return Promise.resolve(false);
      const targets = targetsFor(sheet);
      if (targets.length === 0 || !can(sheet).hide) {
        say("모든 시트를 숨길 수 없음");
        return Promise.resolve(false);
      }
      return change((next) => {
        const hideSet = new Set(targets);
        const curShown = shownIn(next);
        const surviving = curShown.filter((n) => !hideSet.has(n));
        if (surviving.length === 0) {
          say("모든 시트를 숨길 수 없음");
          return false;
        }
        if (hideSet.has(next.activeSheetName)) {
          const at = curShown.indexOf(next.activeSheetName);
          const nextShown = curShown.slice(at + 1).find((n) => !hideSet.has(n))
            ?? curShown.slice(0, at).reverse().find((n) => !hideSet.has(n))
            ?? surviving[0];
          activate(next, nextShown);
        }
        for (const s of targets) next.sheetLayout[s] = { ...next.sheetLayout[s], hidden: true };
        return "시트 숨김: " + targets.map(names.labelOf).join(", ");
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

    function selectAll() {
      if (structureBlocked()) return Promise.resolve(false);
      return enqueueWorkspaceHistoryOp(() => {
        commitIfEditing();
        const keys = sheetKeys();
        const shown = shownSheets();
        const active = getActiveSheetName();
        if (shown.length <= 1 || strip.getGroup(keys, active).length >= shown.length) return false;
        strip.selectAllGroup(keys, active);
        strip.render({ document, keys, active });
        focusAnchor?.();
        say("모든 시트 선택");
        return true;
      });
    }

    function ungroup() {
      if (structureBlocked()) return Promise.resolve(false);
      return enqueueWorkspaceHistoryOp(() => {
        commitIfEditing();
        if (!strip.isGrouped()) return false;
        const keys = sheetKeys();
        const active = getActiveSheetName();
        strip.clearGroup(active);
        strip.render({ document, keys, active });
        focusAnchor?.();
        say("시트 그룹 해제");
        return true;
      });
    }

    // The same commands for the sheet menu and the command search: they act on the active sheet.
    const actions = {
      "tab-insert": () => insert(getActiveSheetName()),
      "tab-delete": () => remove(getActiveSheetName()),
      "tab-rename": () => {
        const active = getActiveSheetName();
        if (!structureBlocked() && targetsFor(active).length > 1) {
          say("여러 시트를 선택한 상태에서는 이름을 바꿀 수 없음");
          return Promise.resolve(false);
        }
        return Promise.resolve(ui.startRename(active));
      },
      "tab-move-copy": () => movePlace(getActiveSheetName()),
      "tab-hide": () => hide(getActiveSheetName()),
      "tab-unhide": () => unhide(),
    };

    return { can, insert, remove, rename, checkRename, setTabColor, moveOrCopy, movePlace, moveTo, hide, unhide, selectAll, ungroup, actions, shownSheets, hiddenSheets };
  }

  host.__EASYOFFICE_CREATE_SHEET_TABS__ = createSheetTabs;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
