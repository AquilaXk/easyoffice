(function (host) {
  "use strict";

  const astApi = host.__EASYOFFICE_SHEET_UTILS__;
  const isFormulaStr = astApi.isFormulaStringJs;
  const asciiUpper = astApi.asciiUpperJs;
  const quoteFormulaSheetName = astApi.quoteFormulaSheetName;
  // Conditional formatting / validation rules (PD-171); absent in pages that do not load them.
  const rulesApi = host.__EASYOFFICE_SHEET_RULES__;
  // The defined names of the workbook (PD-214); absent in pages that do not load them.
  const namesApi = host.__EASYOFFICE_SHEET_NAMES__;
  // The cells an array fills (PD-217): results the engine answers with, never input; absent in pages that do not load them.
  const spillApi = host.__EASYOFFICE_SHEET_SPILL__;
  const isInputEntry = (entry) => !spillApi || spillApi.isInput(entry);
  // The `names` field of a stateless request: only when there are names to send.
  const namesField = () => (namesApi?.hasAny() ? { names: namesApi.getAll() } : {});
  // The merged ranges go with a request that has no session (the engine places arrays around them, PD-222).
  const mergesField = (sheets) => {
    const names = Object.keys(sheets || {});
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    return { ...(spillApi?.mergesField(names, layoutApi) || {}), ...(spillApi?.hiddenRowsField(names, layoutApi) || {}), ...(host.__EASYOFFICE_SHEET_CELL_META__?.metaField?.(names, sheets) || {}) };
  };

  function cloneStore(sheets) {
    const fast = host.__EASYOFFICE_SHEET_GRID_UTILS__?.cloneSheetStore;
    return typeof fast === "function" ? fast(sheets || {}) : structuredClone(sheets || {});
  }

  function styleFlagsMask(s) {
    if (!s) return 0;
    const td = s.textDecoration || "";
    const bold = s.bold || s.fontWeight === "600" || s.fontWeight === "700" || s.fontWeight === "bold" || Number(s.fontWeight) >= 600;
    const italic = s.italic || s.fontStyle === "italic";
    const underline = s.underline || td.includes("underline");
    const strike = s.strike || td.includes("line-through");
    return (bold ? 1 : 0) | (italic ? 2 : 0) | (underline ? 4 : 0) | (strike ? 8 : 0);
  }

  function cellStyleChanged(sa, sb) {
    if (sa === sb) return false;
    if (styleFlagsMask(sa) !== styleFlagsMask(sb)) return true;
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    return (sa?.sizePt || 0) !== (sb?.sizePt || 0) || (sa?.fontSize || "") !== (sb?.fontSize || "") || (sa?.fontFamily || "") !== (sb?.fontFamily || "") || (look ? look.lookKey(sa) !== look.lookKey(sb) : false);
  }

  function sheetCellDataChanged(sheetsA, sheetsB, includeStyle = true) {
    const keysA = Object.keys(sheetsA || {}), keysB = Object.keys(sheetsB || {});
    if (keysA.length !== keysB.length) return true;
    for (const sh of keysA) {
      if (!Object.hasOwn(sheetsB || {}, sh)) return true;
      const mapA = sheetsA[sh] || {}, mapB = sheetsB[sh] || {};
      if (mapA === mapB) continue;
      const refsA = Object.keys(mapA), refsB = Object.keys(mapB);
      if (refsA.length !== refsB.length) return true;
      for (const ref of refsA) {
        const ea = mapA[ref], eb = mapB[ref];
        if (!eb || (ea?.value || "") !== (eb?.value || "") || (ea?.formula || "") !== (eb?.formula || "") || (ea?.t || "") !== (eb?.t || "") || (ea?.ph?.b === ea?.value ? ea?.ph?.r || "" : "") !== (eb?.ph?.b === eb?.value ? eb?.ph?.r || "" : "") || (includeStyle && cellStyleChanged(ea?.style, eb?.style))) {
          return true;
        }
      }
    }
    return false;
  }

  function extractEvaluatedTargetValue(rpcRes, targetValues, batchValues, target, idx, totalTargets) {
    if (targetValues && idx < targetValues.length) return targetValues[idx];
    if (batchValues) return batchValues[target.key] ?? batchValues[asciiUpper(target.sheet) + "!" + asciiUpper(target.cell)];
    if (totalTargets === 1 && rpcRes?.value !== undefined) return rpcRes.value;
    return undefined;
  }

  function isScrubbableLiteral(seg) {
    return seg.isLiteral && seg.text.trim().length > 0;
  }

  function assertValidScrubResponse(origText, res) {
    if (!res || typeof res.scrubbedText !== "string" || (origText.length > 0 && res.scrubbedText.length === 0)) {
      throw new Error("개인정보 마스킹 응답 검증에 실패했습니다.");
    }
  }

  function createSheetStoreHelpers(deps) {
    const {
      invokeIpc,
      normalizeFormattedNumericCell,
      tokenizeFormulaStringLiterals,
    } = deps;

    function buildCellsAndFormulaTargets(sheetsMap) {
      const cells = [];
      const formulaTargets = [];
      for (const shName of Object.keys(sheetsMap || {})) {
        const snap = sheetsMap[shName] || {};
        const shPrefix = shName + "!";
        for (const ref of Object.keys(snap)) {
          const entry = snap[ref];
          if (!entry || !isInputEntry(entry)) continue;
          if (isFormulaStr(entry.formula || "")) {
            formulaTargets.push({ sheet: shName, cell: ref, key: shPrefix + ref });
          }
          if (entry.formula || entry.value) {
            const val = entry.formula ? entry.formula : entry.t === "s" ? String(entry.value) : normalizeFormattedNumericCell(entry.value);
            const t = entry.formula ? undefined : host.__EASYOFFICE_SHEET_CELL_TYPE__?.wireTag(val, entry.t);
            const ph = !entry.formula && entry.ph?.b === entry.value ? (entry.ph.r || "") : "";
            cells.push({ sheet: shName, cell: ref, value: val, ...(t ? { t } : {}), ...(ph ? { ph } : {}) });
          }
        }
      }
      return { cells, formulaTargets };
    }

    function sheetMapHasAnyFormula(sheetsMap) {
      return Object.keys(sheetsMap || {}).some((shName) => {
        const snap = sheetsMap[shName] || {};
        return Object.keys(snap).some((ref) => isFormulaStr(snap[ref]?.formula || ""));
      });
    }

    // --- Engine sheet session (PD-164) -------------------------------------------------------
    // The engine keeps the workbook between edits. The page keeps a mirror of what the session
    // holds (`raw`: the text it was last sent per cell, `values`: the last value it reported per
    // formula cell), diffs every full store against that mirror and sends only the cells that
    // differ, then folds the reported changes into `values`. The callers still hand in a whole
    // store (undo, load and scrub restore arbitrary snapshots), so the diff also repairs a mirror
    // that drifted, e.g. after a discarded formula-bar preview.
    const SESSION_CHUNK_CHARS = 1_500_000;
    const session = {
      enabled: false,
      id: null,
      sheetKey: "",
      // Sheet set the engine refused for size (PD-164): evaluated without a session until it changes.
      refusedKey: null,
      raw: new Map(),
      values: new Map(),
      links: new Map(),
      // Rule payload the session holds per sheet (see `syncRules`).
      rules: new Map(),
      // The names payload the session holds (see `syncNames`), null before the first one is sent.
      names: null,
      // The lock payload of each protected sheet the session was told (see `syncLocks`).
      locks: new Map(),
      // The merges, the table ranges and the pivot tables of each sheet the session was told, and the layout version they
      // were read at (see `syncMerges`, PD-292).
      merges: new Map(),
      tables: new Map(),
      pivots: new Map(),
      // The hidden rows each sheet was told (see `syncMerges`, PD-232).
      hiddenRows: new Map(),
      cellMeta: null,
      mergesVersion: -1,
      gen: 0,
    };
    let sessionChain = Promise.resolve();
    const hiddenRows = host.__EASYOFFICE_CREATE_SHEET_HIDDEN_ROWS_SYNC__({ session, sessionCall: (body) => sessionCall(body), isFormulaStr, takeSpills: (m, r) => takeSpills(m, r), getColumnWidths: deps.getColumnWidths, getFileName: deps.getFileName });

    // Session actions travel in the `evaluate_sheet` command (the 15-command IPC contract).
    // Every request names the workbook's date system when it is 1904, so the engine calculates, formats and filters in it (PD-264).
    const date1904Field = () => ({ ...(host.__EASYOFFICE_SHEET_UTILS__?.getDate1904?.() ? { date1904: true } : {}), ...host.__EASYOFFICE_SHEET_CALC_OPTIONS__?.evalField?.() });

    function sessionCall(body) {
      return invokeIpc("evaluate_sheet", JSON.stringify({ ...body, ...date1904Field() }));
    }

    function resetSession() {
      session.id = null;
      session.sheetKey = "";
      session.raw = new Map();
      session.values = new Map();
      session.links = new Map();
      session.rules = new Map();
      session.names = null;
      session.locks = new Map();
      session.merges = new Map();
      session.tables = new Map();
      session.pivots = new Map();
      spillApi?.clearOmittedPivots();
      session.hiddenRows = new Map();
      session.cellMeta = null;
      session.mergesVersion = -1;
    }

    // Forgets the session and tells the engine to release it (an unknown id is harmless there).
    function dropSession() {
      const previous = session.id;
      resetSession();
      if (previous !== null) {
        void Promise.resolve(sessionCall({ sessionAction: "close", session: previous })).catch(() => {});
      }
    }

    // The shell tells the page at start-up whether the engine has session commands.
    function enableSheetSessions(enabled) {
      session.enabled = enabled === true;
      session.refusedKey = null;
      if (!session.enabled) dropSession();
    }

    function runSessionExclusive(work) {
      const next = sessionChain.then(work, work);
      sessionChain = next.catch(() => {});
      return next;
    }

    // The sheets in tab order: the engine resolves a 3D span (`Sheet1:Sheet3!A1`) and a deleted end of one by it, and the
    // store's key order (a name like "2024" is reordered) is not it. The order is part of the session key, so a moved tab reopens it.
    const orderedSheetNames = (sheetsMap) => [...new Set([
      ...(host.__EASYOFFICE_SHEET_TAB_STRIP__?.getOrder?.() || []).filter((name) => Object.hasOwn(sheetsMap || {}, name)),
      ...Object.keys(sheetsMap || {}),
    ])];
    const sheetSetKey = (names) => names.join("\u0000");

    async function openSession(names, sheetKey) {
      dropSession();
      const res = await sessionCall({ sessionAction: "open", sheets: names });
      if (!res || typeof res.session !== "string") throw new Error("시트 세션을 열지 못했습니다.");
      session.id = res.session;
      session.sheetKey = sheetKey;
    }

    // Brings the mirror up to `sheetsMap`; returns the cells to send and the formula targets.
    function diffAgainstMirror(sheetsMap, names) {
      session.gen += 1;
      const gen = session.gen;
      const changes = [];
      const formulaTargets = [];
      for (const shName of names) {
        const snap = sheetsMap[shName] || {};
        let mirror = session.raw.get(shName);
        if (!mirror) {
          mirror = new Map();
          session.raw.set(shName, mirror);
        }
        const shPrefix = shName + "!";
        for (const ref of Object.keys(snap)) {
          const entry = snap[ref];
          if (!entry || !isInputEntry(entry)) continue;
          if (isFormulaStr(entry.formula || "")) {
            formulaTargets.push({ sheet: shName, cell: ref, key: shPrefix + ref });
          }
          if (!entry.formula && !entry.value) continue;
          const raw = entry.formula ? entry.formula : entry.t === "s" ? String(entry.value) : normalizeFormattedNumericCell(entry.value);
          const t = entry.formula ? "" : (host.__EASYOFFICE_SHEET_CELL_TYPE__?.wireTag(raw, entry.t) || "");
          const ph = !entry.formula && entry.ph?.b === entry.value ? (entry.ph.r || "") : "";
          const known = mirror.get(ref);
          const mkChange = () => ({ sheet: shName, cell: ref, value: raw, ...(t ? { t } : {}), ...(ph ? { ph } : {}) });
          if (known === undefined) {
            mirror.set(ref, { raw, t, ph, gen });
            changes.push(mkChange());
          } else {
            known.gen = gen;
            if (known.raw !== raw || known.t !== t || (known.ph || "") !== ph) {
              known.raw = raw; known.t = t; known.ph = ph;
              changes.push(mkChange());
            }
          }
        }
        for (const [ref, known] of mirror) {
          if (known.gen !== gen) {
            mirror.delete(ref);
            changes.push({ sheet: shName, cell: ref, value: "" });
          }
        }
      }
      return { changes, formulaTargets };
    }

    async function syncSessionOnce(sheetsMap) {
      const names = orderedSheetNames(sheetsMap);
      const sheetKey = sheetSetKey(names);
      if (session.id === null || session.sheetKey !== sheetKey) {
        await openSession(names, sheetKey);
      }
      await syncNames(sheetsMap);
      const mergeChanged = await syncMerges(names, sheetsMap);
      const metaChanged = await hiddenRows.syncMeta(names, sheetsMap);
      const { changes, formulaTargets } = diffAgainstMirror(sheetsMap, names);
      let at = 0;
      let last = null;
      do {
        const chunk = [];
        let chars = 0;
        while (at < changes.length && (chunk.length === 0 || chars < SESSION_CHUNK_CHARS)) {
          const change = changes[at++];
          chunk.push(change);
          chars += change.value.length + change.sheet.length + change.cell.length + 40;
        }
        const finalChunk = at >= changes.length;
        last = await sessionCall({
          sessionAction: "apply",
          session: session.id,
          recalc: finalChunk,
          changes: chunk,
        });
        if (last?.refused) {
          const refusal = new Error("시트가 세션 상한을 넘었습니다.");
          refusal.refused = true;
          throw refusal;
        }
        if (!last || last.expired === true) throw new Error("시트 세션이 만료되었습니다.");
        takeRuleChanges(last);
      } while (at < changes.length);
      for (const c of Array.isArray(last.changed) ? last.changed : []) {
        session.values.set(c.sheet + "!" + c.cell, c.value);
      }
      const spillsChanged = takeSpills(sheetsMap, last);
      await syncRules(names);
      const lockChanged = await syncLocks(names, sheetsMap);
      return {
        formulaTargets,
        circular: last.circular === true,
        timedOut: last.timedOut === true,
        pending: last.pending,
        spillsChanged: spillsChanged || lockChanged || mergeChanged || metaChanged,
      };
    }

    // Folds what an answer says about arrays and `HYPERLINK` formula flags into the sheet model.
    function takeSpills(sheetsMap, answer) {
      if (!spillApi) return false;
      let changed = spillApi.applyChangedLinks(sheetsMap, answer.changed, session.links);
      for (const info of Array.isArray(answer.spills) ? answer.spills : []) {
        const map = sheetsMap?.[info.sheet];
        if (!map) continue;
        spillApi.applyInfo(
          map,
          info,
          (ref) => session.values.get(info.sheet + "!" + ref),
          (ref) => session.links.get(info.sheet + "!" + ref) === true
        );
        changed = true;
      }
      for (const c of Array.isArray(answer.changed) ? answer.changed : []) {
        if (spillApi.setValue(sheetsMap?.[c.sheet], c.cell, c.value, c.link === true)) changed = true;
      }
      return changed;
    }

    // Sends the whole list of defined names when it differs from what the session holds, before the cells, so the
    // formulas that use a name are compiled once. The answer lists the formulas whose value changed.
    async function syncNames(sheetsMap) {
      if (!namesApi) return;
      const payload = namesApi.payload();
      if (payload === session.names || (session.names === null && !namesApi.hasAny())) return;
      const res = await sessionCall({ sessionAction: "names", session: session.id, names: JSON.parse(payload) });
      if (res?.refused) {
        const refusal = new Error("시트가 세션 상한을 넘었습니다.");
        refusal.refused = true;
        throw refusal;
      }
      if (!res || res.expired === true) throw new Error("시트 세션이 만료되었습니다.");
      for (const c of Array.isArray(res.changed) ? res.changed : []) session.values.set(c.sheet + "!" + c.cell, c.value);
      takeRuleChanges(res);
      // A name can change the size of an array: the areas the answer lists leave the cells they filled.
      takeSpills(sheetsMap, res);
      session.names = payload;
    }

    // Tells the engine which cells of the protected sheets may be changed, when that differs from what it holds: an array is
    // not placed over the others (PD-217). Returns whether the answer changed the model.
    async function syncLocks(names, sheetsMap) {
      const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
      if (!spillApi || !layoutApi) return false;
      let changed = false;
      for (const name of names) {
        const lockObj = spillApi.lockPayload(name, sheetsMap[name], layoutApi);
        if (!lockObj && !session.locks.has(name)) continue;
        const payload = JSON.stringify(lockObj);
        if ((session.locks.get(name) ?? "null") === payload) continue;
        const res = await sessionCall({ sessionAction: "locks", session: session.id, sheet: name, lock: lockObj });
        if (!res || res.expired === true) throw new Error("시트 세션이 만료되었습니다.");
        if (res.refused) continue;
        session.locks.set(name, payload);
        for (const c of Array.isArray(res.changed) ? res.changed : []) {
          session.values.set(c.sheet + "!" + c.cell, c.value);
          const held = sheetsMap[c.sheet]?.[c.cell];
          if (held && isFormulaStr(held.formula || "")) held.value = String(c.value);
        }
        if (takeSpills(sheetsMap, res)) changed = true;
        changed = true;
      }
      return changed;
    }

    // Tells the engine the merged ranges and the table ranges of each sheet when they differ from what it holds: an array
    // is not placed across one (PD-222, PD-224). Before the cells, so an array is placed once. Returns whether the answer
    // changed the model.
    async function syncMerges(names, sheetsMap) {
      const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
      if (!spillApi || !layoutApi?.version) return false;
      const version = layoutApi.version();
      if (session.mergesVersion === version) return false;
      let changed = false;
      for (const name of names) {
        for (const kind of ["merges", "tables", "pivots"]) {
          const blockerObj = spillApi.blockerPayload(kind, name, layoutApi);
          const payload = JSON.stringify(blockerObj);
          if ((session[kind].get(name) ?? "[]") === payload) continue;
          const res = await sessionCall({ sessionAction: kind, session: session.id, sheet: name, [kind]: blockerObj });
          if (!res || res.expired === true) throw new Error("시트 세션이 만료되었습니다.");
          if (res.refused) continue;
          session[kind].set(name, payload);
          // The pivot tables the engine left out for their size: the status bar says why a `GETPIVOTDATA` into one is `#REF!` (PD-303).
          if (kind === "pivots") spillApi.setOmittedPivots(name, res.pivotsOmitted);
          for (const c of Array.isArray(res.changed) ? res.changed : []) {
            session.values.set(c.sheet + "!" + c.cell, c.value);
            const held = sheetsMap[c.sheet]?.[c.cell];
            if (held && isFormulaStr(held.formula || "")) held.value = String(c.value);
          }
          if (takeSpills(sheetsMap, res)) changed = true;
          changed = true;
        }
      }
      // The rows a user and a filter hid: `SUBTOTAL` and `AGGREGATE` are calculated again (PD-232).
      if (await hiddenRows.sync(names, sheetsMap)) changed = true;
      session.mergesVersion = version;
      return changed;
    }

    // A column of a table got a new name from its header cell (PD-225): the engine writes the new name into the formulas that
    // name the column (and the defined names), the page takes the text, and the table's column gets the name. True when
    // something changed. A request the engine refuses leaves that rename undone; the header keeps its text.
    async function renameTableColumns(renames, sheetsMap) {
      const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
      const namesApi = host.__EASYOFFICE_SHEET_NAMES__;
      if (!layoutApi || !spillApi) return false;
      let changed = false;
      for (const r of renames) {
        const tables = {};
        const formulas = [];
        const texts = [];
        const calcSites = [];
        for (const [sheet, map] of Object.entries(sheetsMap)) {
          const list = spillApi.blockerPayload("tables", sheet, layoutApi);
          if (list.length) tables[sheet] = list;
          for (const t of layoutApi.tablesOf(sheet)) {
            (t.calculatedColumns || []).forEach((f, j) => {
              if (f?.includes("[")) { texts.push(f); calcSites.push([t, j]); }
            });
          }
          for (const [cell, held] of Object.entries(map || {})) {
            if (held?.formula && held.formula.includes("[")) formulas.push({ sheet, cell, text: held.formula });
          }
        }
        let answer = null;
        try {
          answer = await sessionCall({ sessionAction: "tableColumn", table: r.table, from: r.from, to: r.to, tables, formulas, names: namesApi?.getAll() || [], ...(texts.length ? { texts } : {}) });
        } catch {
          answer = null;
        }
        if (!answer || !Array.isArray(answer.rewrites)) continue;
        for (const w of answer.rewrites) {
          const held = sheetsMap[w.sheet]?.[w.cell];
          if (held) held.formula = w.text;
        }
        for (const [idx, val] of answer.textRewrites || []) calcSites[idx][0].calculatedColumns[calcSites[idx][1]] = val;
        if (answer.names && namesApi) namesApi.replaceAll(answer.names);
        layoutApi.setTableColumn(r.sheet, r.table, r.index, r.to);
        changed = true;
      }
      return changed;
    }

    // Looks at the header cells of the tables for a column that got a new name; `onChange` runs when the formulas followed.
    function followTableHeaders(sheetsMap, onChange) {
      const renames = host.__EASYOFFICE_SHEET_LAYOUT__?.tableColumnRenames(sheetsMap) || [];
      if (renames.length > 0) void renameTableColumns(renames, sheetsMap).then((moved) => moved && onChange());
    }

    // Rules exist on the page, or the session still holds (or is syncing) rules the page has since cleared.
    function rulesInPlay() {
      return Boolean(rulesApi) && (rulesApi.hasAny() || session.rules.size > 0 || (session.rulesPending || 0) > 0);
    }

    // Appearance changes the engine reports with an `apply` answer.
    function takeRuleChanges(answer) {
      if (!rulesApi) return;
      for (const entry of Array.isArray(answer.cf) ? answer.cf : []) {
        rulesApi.applyPacked(entry.sheet, entry, false);
      }
      for (const name of Array.isArray(answer.cfDropped) ? answer.cfDropped : []) {
        rulesApi.clearFx(name);
        session.rules.delete(name);
      }
    }

    // Sends a sheet's rules when they differ from what the session holds, or when the page dropped the appearance the session
    // answered with (an undo or a redo of the rules clears it); the answer is the appearance of every formatted cell, which
    // replaces what the page showed.
    async function syncRules(names) {
      if (!rulesApi) return;
      for (const name of names) {
        const payload = rulesApi.payloadFor(name);
        const sent = session.rules.get(name);
        if ((sent === payload && rulesApi.hasFx(name)) || (sent === undefined && rulesApi.isEmpty(rulesApi.get(name)))) continue;
        const sentEmpty = rulesApi.isEmpty(rulesApi.get(name));
        session.rulesPending = (session.rulesPending || 0) + 1;
        let res;
        try {
          res = await sessionCall({
            sessionAction: "rules",
            session: session.id,
            sheet: name,
            rules: JSON.parse(payload),
          });
        } finally {
          session.rulesPending -= 1;
        }
        if (!res || res.expired === true) throw new Error("시트 세션이 만료되었습니다.");
        if (sentEmpty) session.rules.delete(name);
        else session.rules.set(name, payload);
        // Refused rules show nothing; the empty answer is still the session's answer, so they are not sent again.
        if (rulesApi.payloadFor(name) === payload) rulesApi.applyPacked(name, res.refused ? null : res.cf, true);
      }
    }

    // Asks the engine whether `text` may be entered in a cell; `null` when it cannot say.
    function validateEntry(sheet, cell, text) {
      return runSessionExclusive(async () => {
        if (!session.enabled || session.id === null) return null;
        try {
          const res = await sessionCall({
            sessionAction: "validate",
            session: session.id,
            sheet,
            cell,
            value: String(text ?? ""),
          });
          return res && res.expired !== true && typeof res.ok === "boolean" ? res : null;
        } catch {
          return null;
        }
      });
    }

    // Moves the mirror of the session after an `structure` answer: the cells and values of the
    // edited sheet change position and the rewritten formulas replace their text.
    function moveMirror(request, answer, sheetsMap) {
      const edit = { axis: request.axis, op: request.op, at: request.at, count: request.count, ...(request.cols ? { cols: request.cols } : {}) };
      const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
      const mirror = session.raw.get(request.sheet);
      if (mirror) {
        const moved = new Map();
        for (const [ref, known] of mirror) {
          const to = layoutApi.remapRef(edit, ref);
          if (to !== null) moved.set(to, known);
        }
        session.raw.set(request.sheet, moved);
      }
      const prefix = request.sheet + "!";
      const values = [];
      for (const [key, value] of session.values) {
        if (key.startsWith(prefix)) values.push([key, value]);
      }
      for (const [key] of values) session.values.delete(key);
      for (const [key, value] of values) {
        const to = layoutApi.remapRef(edit, key.slice(prefix.length));
        if (to !== null) session.values.set(prefix + to, value);
      }
      for (const f of Array.isArray(answer.formulas) ? answer.formulas : []) {
        let map = session.raw.get(f.sheet);
        if (!map) {
          map = new Map();
          session.raw.set(f.sheet, map);
        }
        map.set(f.cell, { raw: f.value, t: "", gen: session.gen });
      }
      for (const c of Array.isArray(answer.changed) ? answer.changed : []) {
        session.values.set(c.sheet + "!" + c.cell, c.value);
      }
      takeSpills(sheetsMap, answer);
      // The rebuilt session placed every array again: the page takes the whole picture (PD-217).
      if (spillApi && Array.isArray(answer.spillAreas)) spillApi.replaceAll(sheetsMap, answer.spillAreas);
      // The engine moved the names with the cells: the page takes its list.
      if (namesApi && Array.isArray(answer.names)) {
        namesApi.replaceAll(answer.names, true);
        session.names = namesApi.payload();
      }
      // The engine rebuilt its session without rules; the next sync sends them again.
      session.rules = new Map();
    }

    // Sends `request` as the session action `action` after the session holds `sheetsMap`; `done` folds
    // the answer into the page's mirror of the session. The result is `{ ok: true, answer }` or
    // `{ ok: false, reason }` (`limit`: the sheets are over the session limits, `structure`: the
    // change rewrites too many formulas, `error` with the engine's `message`).
    function runOnSession(sheetsMap, request, action, done) {
      return runSessionExclusive(async () => {
        const names = orderedSheetNames(sheetsMap);
        if (!session.enabled) return { ok: false, reason: "error" };
        if (session.refusedKey !== null && session.refusedKey === sheetSetKey(names)) {
          return { ok: false, reason: "limit" };
        }
        for (let attempt = 0; attempt < 2; attempt += 1) {
          try {
            await syncSessionOnce(sheetsMap);
            const answer = await sessionCall({ ...request, sessionAction: action, session: session.id });
            if (answer?.refused === "structure") return { ok: false, reason: "structure" };
            if (answer?.refused === "protected") return { ok: false, reason: "protected" };
            if (answer?.refused) {
              resetSession();
              session.refusedKey = sheetSetKey(names);
              return { ok: false, reason: "limit" };
            }
            if (!answer || answer.expired === true) throw new Error("시트 세션이 만료되었습니다.");
            done(request, answer, sheetsMap);
            return { ok: true, answer };
          } catch (err) {
            if (err?.refused === true) {
              resetSession();
              session.refusedKey = sheetSetKey(names);
              return { ok: false, reason: "limit" };
            }
            const expired = typeof err?.message === "string" && err.message.includes("만료");
            if (!expired || attempt === 1) return { ok: false, reason: "error", message: String(err?.message || err) };
            dropSession();
          }
        }
        return { ok: false, reason: "error" };
      });
    }

    // Inserts or deletes rows or columns in the engine session (PD-194): `request` is `{ sheet, axis,
    // op, at, count }` plus the layout, widths and rules that move with the cells.
    const editAxis = (sheetsMap, request) => runOnSession(sheetsMap, request, "structure", moveMirror);

    // The formulas and rules that name a sheet the page renames or deletes (PD-209): `request` is
    // `{ op: "rename"|"delete", sheet, newName, rules }`. The session keeps its old sheets; the
    // page's sheet set changes, so the page lets go of it and the next sync opens a new one.
    const rewriteSheetRefs = (sheetsMap, request) => runOnSession(sheetsMap, request, "sheetRefs", dropSession);

    // The address the HYPERLINK formula of a cell opens (PD-223), evaluated by the engine on the workbook: `{ target }`,
    // `{ error }` when the function gives an error, `{ none: true }` when the formula is not a HYPERLINK call. Without
    // a session the request carries the cells.
    async function linkTarget(sheetsMap, sheet, cell, formula, anchor = "") {
      const request = anchor ? { sheet, cell, anchor, formula } : { sheet, cell, formula };
      const viaSession = await runOnSession(sheetsMap, request, "linkTarget", () => {});
      if (viaSession.ok) return viaSession.answer;
      const { cells } = buildCellsAndFormulaTargets(sheetsMap);
      return sessionCall({
        ...request,
        sessionAction: "linkTarget",
        sheets: orderedSheetNames(sheetsMap),
        cells,
        ...namesField(),
        ...mergesField(sheetsMap),
      });
    }

    // The value text of every defined name, as the engine shows it in the 이름 관리 list (PD-214).
    const nameValues = (sheetsMap) => runOnSession(sheetsMap, {}, "nameValues", () => {});

    // One retry after a reset covers an expired or evicted session. `null` sends the caller to the
    // stateless command: after two failures, and at once for a sheet set the engine refused for
    // size (the engine already dropped that session, so reopening it would only repeat the load).
    async function syncSession(sheetsMap) {
      return runSessionExclusive(async () => {
        const names = orderedSheetNames(sheetsMap);
        if (session.refusedKey !== null && session.refusedKey === sheetSetKey(names)) return null;
        for (let attempt = 0; attempt < 2; attempt += 1) {
          try {
            return await syncSessionOnce(sheetsMap);
          } catch (err) {
            if (err?.refused === true) {
              resetSession();
              session.refusedKey = sheetSetKey(names);
              return null;
            }
            dropSession();
          }
        }
        return null;
      });
    }

    function sessionValue(key) {
      return session.values.get(key);
    }

    async function evaluatePlainCell(sheets, defaultSheet, extraTarget) {
      const { sheet, cell } = extraTarget;
      const { cells } = buildCellsAndFormulaTargets({ [sheet]: { [cell]: sheets[sheet]?.[cell] } });
      const rpcRes = await invokeIpc(
        "evaluate_sheet",
        JSON.stringify({
          sheet: defaultSheet,
          sheets: Object.keys(sheets),
          targets: [{ sheet, cell }],
          cells,
          ...namesField(),
          ...mergesField(sheets),
          ...date1904Field(),
        })
      );
      return readExtraValue(rpcRes, sheets, extraTarget, 1);
    }

    // The formula bar's value from a stateless response: the first target's value, else the
    // response's own `value`, else what the cell already holds.
    function readExtraValue(rpcRes, sheets, extraTarget, totalTargets) {
      const targetValues = Array.isArray(rpcRes?.targetValues) ? rpcRes.targetValues : null;
      const batchValues =
        rpcRes && typeof rpcRes.values === "object" && rpcRes.values ? rpcRes.values : null;
      const target = {
        ...extraTarget,
        key: extraTarget.sheet + "!" + extraTarget.cell,
      };
      const rawExtra = extractEvaluatedTargetValue(rpcRes, targetValues, batchValues, target, 0, totalTargets);
      if (rawExtra !== undefined && rawExtra !== null) return String(rawExtra);
      if (rpcRes?.value !== undefined && rpcRes.value !== null) return String(rpcRes.value);
      return sheets[extraTarget.sheet]?.[extraTarget.cell]?.value ?? "";
    }

    async function computeEvaluatedSheetStore(baseSheets, defaultSheet, extraTarget = null) {
      const sheets = cloneStore(baseSheets);
      // A plain-value extra target (formula bar) is evaluated by the stateless command so its
      // display text matches what that command has always returned. Such a cell depends on no
      // other cell, so only that cell is sent and the formulas still come from the session.
      const extraIsFormula =
        !extraTarget || isFormulaStr(sheets[extraTarget.sheet]?.[extraTarget.cell]?.formula || "");
      if (session.enabled) {
        // Rules count like formulas: the session has to hold the cells they judge.
        const needsSession = sheetMapHasAnyFormula(sheets) || rulesInPlay();
        if (!extraTarget && !needsSession) return { sheets, extraValue: "", circular: false };
        const viaSession =
          extraIsFormula || needsSession
            ? await syncSession(sheets)
            : { formulaTargets: [], circular: false };
        if (viaSession) {
          for (const target of viaSession.formulaTargets) {
            const stored = sheets[target.sheet]?.[target.cell];
            const value = sessionValue(target.key);
            if (stored && value !== undefined && value !== null) stored.value = String(value);
            if (stored && spillApi) spillApi.writeAnchorFormulaLink(stored, session.links.get(target.key) === true);
          }
          let extraValue = "";
          if (extraTarget && extraIsFormula) {
            const raw = sessionValue(extraTarget.sheet + "!" + extraTarget.cell);
            extraValue =
              raw !== undefined && raw !== null
                ? String(raw)
                : (sheets[extraTarget.sheet]?.[extraTarget.cell]?.value ?? "");
          } else if (extraTarget) {
            extraValue = await evaluatePlainCell(sheets, defaultSheet, extraTarget);
          }
          return { sheets, extraValue, circular: viaSession.circular };
        }
      }
      const { cells, formulaTargets } = buildCellsAndFormulaTargets(sheets);
      const rpcTargets = [];
      const targetSpecs = [];
      if (extraTarget) {
        rpcTargets.push({
          sheet: extraTarget.sheet,
          cell: extraTarget.cell,
          key: extraTarget.sheet + "!" + extraTarget.cell,
        });
        targetSpecs.push({ sheet: extraTarget.sheet, cell: extraTarget.cell });
      }
      for (const ft of formulaTargets) {
        if (!extraTarget || ft.sheet !== extraTarget.sheet || ft.cell !== extraTarget.cell) {
          rpcTargets.push(ft);
          targetSpecs.push({ sheet: ft.sheet, cell: ft.cell });
        }
      }

      if (rpcTargets.length === 0) {
        spillApi?.clearOmittedPivots();
        return { sheets, extraValue: "", circular: false };
      }

      const rpcPayload = JSON.stringify({
        sheet: defaultSheet,
        sheets: Object.keys(sheets),
        targets: targetSpecs,
        cells,
        ...namesField(),
        ...mergesField(sheets),
        ...date1904Field(),
      });
      const rpcRes = await invokeIpc("evaluate_sheet", rpcPayload);
      const targetValues = Array.isArray(rpcRes?.targetValues) ? rpcRes.targetValues : null;
      const batchValues =
        rpcRes && typeof rpcRes.values === "object" && rpcRes.values ? rpcRes.values : null;

      rpcTargets.forEach((target, idx) => {
        const rawVal = extractEvaluatedTargetValue(rpcRes, targetValues, batchValues, target, idx, rpcTargets.length);
        if (rawVal === undefined || rawVal === null) return;
        const storedEntry = sheets[target.sheet]?.[target.cell];
        if (storedEntry && isFormulaStr(storedEntry.formula || "")) {
          storedEntry.value = String(rawVal);
        }
      });

      if (spillApi) spillApi.replaceAll(sheets, rpcRes?.spills, rpcRes?.formulaLinks, rpcRes?.pivotsOmitted ?? null);
      const extraValue = extraTarget
        ? readExtraValue(rpcRes, sheets, extraTarget, rpcTargets.length)
        : "";
      return { sheets, extraValue, circular: rpcRes?.circular === true };
    }

    async function evaluateLiveFormulaTargets(sheetsMap, defaultSheet) {
      if (session.enabled) {
        if (!sheetMapHasAnyFormula(sheetsMap) && !rulesInPlay()) return null;
        const viaSession = await syncSession(sheetsMap);
        if (viaSession) {
          return {
            rpcRes: { circular: viaSession.circular, timedOut: viaSession.timedOut, pending: viaSession.pending },
            spillsChanged: viaSession.spillsChanged === true,
            formulaTargets: viaSession.formulaTargets,
            targetValues: viaSession.formulaTargets.map((t) => sessionValue(t.key)),
            batchValues: null,
          };
        }
      }
      const { cells, formulaTargets } = buildCellsAndFormulaTargets(sheetsMap);
      if (formulaTargets.length === 0) {
        spillApi?.clearOmittedPivots();
        return null;
      }
      const rpcPayload = JSON.stringify({
        sheet: defaultSheet,
        sheets: orderedSheetNames(sheetsMap),
        targets: formulaTargets.map(({ sheet, cell }) => ({ sheet, cell })),
        cells,
        ...namesField(),
        ...mergesField(sheetsMap),
        ...date1904Field(),
      });
      const rpcRes = await invokeIpc("evaluate_sheet", rpcPayload);
      const targetValues = Array.isArray(rpcRes?.targetValues) ? rpcRes.targetValues : null;
      const batchValues =
        rpcRes && typeof rpcRes.values === "object" && rpcRes.values ? rpcRes.values : null;
      if (spillApi) spillApi.replaceAll(sheetsMap, rpcRes?.spills, rpcRes?.formulaLinks, rpcRes?.pivotsOmitted ?? null);
      return { rpcRes, formulaTargets, targetValues, batchValues, spillsChanged: Boolean(spillApi) };
    }

    // A cell an array filled is a result: the next calculation writes it again, so masking it would
    // count a cell that comes back. The cells it is calculated from are masked instead.
    function collectEntryScrubJob(entry, plainEntries, formulaJobs) {
      if (!isInputEntry(entry)) return;
      const formula = entry.formula || "";
      if (!isFormulaStr(formula)) {
        if ((entry.value || "").trim()) plainEntries.push(entry);
        return;
      }
      const segments = tokenizeFormulaStringLiterals(formula);
      const literalSegs = segments.filter(isScrubbableLiteral);
      if (literalSegs.length > 0) formulaJobs.push({ entry, segments, literalSegs });
    }

    function collectScrubJobs(candidateSheets, plainEntries, formulaJobs) {
      for (const snap of Object.values(candidateSheets)) {
        for (const entry of Object.values(snap || {})) {
          if (entry) collectEntryScrubJob(entry, plainEntries, formulaJobs);
        }
      }
    }

    async function invokeScrubTexts(texts) {
      if (texts.length === 0) return [];
      if (texts.length <= 32) {
        return Promise.all(texts.map((text) => invokeIpc("scrub_pii", text)));
      }
      const out = [];
      for (let i = 0; i < texts.length; i += 1024) {
        const chunk = texts.slice(i, i + 1024);
        const resp = await invokeIpc("scrub_pii", JSON.stringify({ texts: chunk }));
        const items = resp && Array.isArray(resp.results) ? resp.results : null;
        if (!items || items.length !== chunk.length) {
          throw new Error("scrub_pii 배치 응답이 올바르지 않습니다.");
        }
        for (let j = 0; j < items.length; j += 1) out.push(items[j]);
      }
      return out;
    }

    async function scrubSheetStoreMap(baseSheets, activeSheetName, trackAsyncFn) {
      const candidateSheets = cloneStore(baseSheets);
      const plainEntries = [];
      const formulaJobs = [];
      collectScrubJobs(candidateSheets, plainEntries, formulaJobs);

      const literalInputs = [];
      formulaJobs.forEach((job) => {
        job.literalSegs.forEach((seg) => literalInputs.push(seg));
      });

      const allTexts = plainEntries.map((entry) => entry.value).concat(literalInputs.map((seg) => seg.text));
      const allResults = await invokeScrubTexts(allTexts);
      for (let i = 0; i < allTexts.length; i += 1) {
        assertValidScrubResponse(allTexts[i], allResults[i]);
      }
      const plainResults = allResults.slice(0, plainEntries.length);
      const literalResults = allResults.slice(plainEntries.length);

      let totalMasked = 0;
      plainResults.forEach((res, idx) => {
        const count = Number(res.totalMasked || 0);
        if (count > 0 && res.scrubbedText !== plainEntries[idx].value) {
          plainEntries[idx].value = res.scrubbedText;
        }
        totalMasked += count;
      });

      let litOffset = 0;
      formulaJobs.forEach((job) => {
        let changed = false;
        job.literalSegs.forEach((seg) => {
          const res = literalResults[litOffset];
          litOffset += 1;
          const count = Number(res.totalMasked || 0);
          if (count > 0 && res.scrubbedText !== seg.text) {
            seg.text = res.scrubbedText;
            changed = true;
          }
          totalMasked += count;
        });
        if (changed) {
          job.entry.formula = job.segments
            .map((seg) => {
              if (seg.isLiteral) {
                return '"' + seg.text.replaceAll('"', '""') + '"';
              }
              return seg.text;
            })
            .join("");
          job.entry.value = "";
        }
      });

      if (totalMasked === 0) {
        return { nextSheets: baseSheets, totalMasked: 0 };
      }

      let nextSheets = candidateSheets;
      if (sheetMapHasAnyFormula(candidateSheets)) {
        const evalPromise = computeEvaluatedSheetStore(candidateSheets, activeSheetName, null);
        const evaluated = trackAsyncFn ? await trackAsyncFn(evalPromise) : await evalPromise;
        nextSheets = evaluated.sheets;
      }
      return { nextSheets, totalMasked };
    }

    // The engine stops a recalculation after a few seconds and keeps the cells it did not reach at the
    // value shown (`timedOut`, `pending` of them). The status bar says "계산" (the spreadsheet status for
    // formulas not yet calculated) and the page asks again until none is left, a macrotask apart so
    // keys and clicks are served in between. Returns whether to ask again: not when a pass leaves as
    // many pending as the one before twice in a row, and not for an answer without `pending` (no
    // session, nothing to continue).
    const CALCULATING_LABEL = "계산";
    const calcProgress = { pending: Infinity, stalled: 0 };
    function followCalculation(rpcRes, feedback) {
      if (rpcRes?.timedOut !== true) {
        calcProgress.pending = Infinity;
        calcProgress.stalled = 0;
        if (feedback && feedback.textContent === CALCULATING_LABEL) feedback.textContent = "";
        return false;
      }
      if (feedback) feedback.textContent = CALCULATING_LABEL;
      const pending = Number(rpcRes.pending);
      if (!Number.isFinite(pending)) return false;
      calcProgress.stalled = pending < calcProgress.pending ? 0 : calcProgress.stalled + 1;
      calcProgress.pending = pending;
      return calcProgress.stalled < 2;
    }

    return {
      followCalculation,
      quoteFormulaSheetName,
      isFormulaStr,
      sheetMapHasAnyFormula,
      sheetCellDataChanged,
      extractEvaluatedTargetValue,
      computeEvaluatedSheetStore,
      evaluateLiveFormulaTargets,
      validateEntry,
      editAxis,
      rewriteSheetRefs,
      nameValues,
      linkTarget,
      callSheetAction: sessionCall,
      renameTableColumns,
      followTableHeaders,
      hiddenRowsStale: (name) => hiddenRows.stale(name),
      cellMetaStale: (sheetsMap) => hiddenRows.metaStale(sheetsMap),
      // Whether a sheet of the session has other pivot tables than the layout has now (a create, an edit, a delete or an undo).
      pivotsStale: () =>
        session.id !== null &&
        session.sheetKey.split("\u0000").some((name) => (session.pivots.get(name) ?? "[]") !== JSON.stringify(spillApi?.pivotPayload(name, host.__EASYOFFICE_SHEET_LAYOUT__) ?? [])),
      enableSheetSessions,
      scrubSheetStoreMap,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_STORE__ = createSheetStoreHelpers;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
