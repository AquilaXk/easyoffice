(function (host) {
  "use strict";

  const astApi = host.__EASYOFFICE_SHEET_UTILS__;
  const isFormulaStr = astApi.isFormulaStringJs;
  const asciiUpper = astApi.asciiUpperJs;
  const quoteFormulaSheetName = astApi.quoteFormulaSheetName;

  function cloneStore(sheets) {
    const fast = host.__EASYOFFICE_SHEET_GRID_UTILS__?.cloneSheetStore;
    return typeof fast === "function" ? fast(sheets || {}) : structuredClone(sheets || {});
  }

  function styleFlagsMask(s) {
    if (!s) return 0;
    const td = s.textDecoration || "";
    const bold =
      s.bold ||
      s.fontWeight === "600" ||
      s.fontWeight === "700" ||
      s.fontWeight === "bold" ||
      Number(s.fontWeight) >= 600;
    const italic = s.italic || s.fontStyle === "italic";
    const underline = s.underline || td.includes("underline");
    const strike = s.strike || td.includes("line-through");
    return (
      (bold ? 1 : 0) |
      (italic ? 2 : 0) |
      (underline ? 4 : 0) |
      (strike ? 8 : 0) |
      (s.condfmtHigh ? 16 : 0) |
      (s.frozenPanes ? 32 : 0)
    );
  }

  function cellStyleChanged(sa, sb) {
    if (sa === sb) return false;
    if (styleFlagsMask(sa) !== styleFlagsMask(sb)) return true;
    return (sa?.sizePt || 0) !== (sb?.sizePt || 0) || (sa?.fontSize || "") !== (sb?.fontSize || "");
  }

  function sheetCellDataChanged(sheetsA, sheetsB, includeStyle = true) {
    const keysA = Object.keys(sheetsA || {});
    const keysB = Object.keys(sheetsB || {});
    if (keysA.length !== keysB.length) return true;
    for (const sh of keysA) {
      if (!Object.hasOwn(sheetsB || {}, sh)) return true;
      const mapA = sheetsA[sh] || {};
      const mapB = sheetsB[sh] || {};
      const refsA = Object.keys(mapA);
      const refsB = Object.keys(mapB);
      if (refsA.length !== refsB.length) return true;
      for (const ref of refsA) {
        const ea = mapA[ref];
        const eb = mapB[ref];
        if (
          !eb ||
          (ea?.value || "") !== (eb?.value || "") ||
          (ea?.formula || "") !== (eb?.formula || "") ||
          (includeStyle && cellStyleChanged(ea?.style, eb?.style))
        ) {
          return true;
        }
      }
    }
    return false;
  }

  function extractEvaluatedTargetValue(
    rpcRes,
    targetValues,
    batchValues,
    target,
    idx,
    totalTargets
  ) {
    if (targetValues && idx < targetValues.length) {
      return targetValues[idx];
    }
    if (batchValues) {
      return (
        batchValues[target.key] ??
        batchValues[asciiUpper(target.sheet) + "!" + asciiUpper(target.cell)]
      );
    }
    if (totalTargets === 1 && rpcRes?.value !== undefined) {
      return rpcRes.value;
    }
    return undefined;
  }

  function isScrubbableLiteral(seg) {
    return seg.isLiteral && seg.text.trim().length > 0;
  }

  function assertValidScrubResponse(origText, res) {
    if (
      !res ||
      typeof res.scrubbedText !== "string" ||
      (origText.length > 0 && res.scrubbedText.length === 0)
    ) {
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
          if (!entry) continue;
          if (isFormulaStr(entry.formula || "")) {
            formulaTargets.push({ sheet: shName, cell: ref, key: shPrefix + ref });
          }
          if (entry.formula || entry.value) {
            const val = entry.formula
              ? entry.formula
              : normalizeFormattedNumericCell(entry.value);
            cells.push({ sheet: shName, cell: ref, value: val });
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
      gen: 0,
    };
    let sessionChain = Promise.resolve();

    // Session actions travel in the `evaluate_sheet` command (the 15-command IPC contract).
    function sessionCall(body) {
      return invokeIpc("evaluate_sheet", JSON.stringify(body));
    }

    function resetSession() {
      session.id = null;
      session.sheetKey = "";
      session.raw = new Map();
      session.values = new Map();
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

    function sheetSetKey(names) {
      return names.slice().sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).join("\u0000");
    }

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
          if (!entry) continue;
          if (isFormulaStr(entry.formula || "")) {
            formulaTargets.push({ sheet: shName, cell: ref, key: shPrefix + ref });
          }
          if (!entry.formula && !entry.value) continue;
          const raw = entry.formula ? entry.formula : normalizeFormattedNumericCell(entry.value);
          const known = mirror.get(ref);
          if (known === undefined) {
            mirror.set(ref, { raw, gen });
            changes.push({ sheet: shName, cell: ref, value: raw });
          } else {
            known.gen = gen;
            if (known.raw !== raw) {
              known.raw = raw;
              changes.push({ sheet: shName, cell: ref, value: raw });
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
      const names = Object.keys(sheetsMap || {});
      const sheetKey = sheetSetKey(names);
      if (session.id === null || session.sheetKey !== sheetKey) {
        await openSession(names, sheetKey);
      }
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
      } while (at < changes.length);
      for (const c of Array.isArray(last.changed) ? last.changed : []) {
        session.values.set(c.sheet + "!" + c.cell, c.value);
      }
      return { formulaTargets, circular: last.circular === true };
    }

    // One retry after a reset covers an expired or evicted session. `null` sends the caller to the
    // stateless command: after two failures, and at once for a sheet set the engine refused for
    // size (the engine already dropped that session, so reopening it would only repeat the load).
    async function syncSession(sheetsMap) {
      return runSessionExclusive(async () => {
        const names = Object.keys(sheetsMap || {});
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
      const rawExtra = extractEvaluatedTargetValue(
        rpcRes,
        targetValues,
        batchValues,
        target,
        0,
        totalTargets
      );
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
        const hasFormula = sheetMapHasAnyFormula(sheets);
        if (!extraTarget && !hasFormula) return { sheets, extraValue: "", circular: false };
        const viaSession =
          extraIsFormula || hasFormula
            ? await syncSession(sheets)
            : { formulaTargets: [], circular: false };
        if (viaSession) {
          for (const target of viaSession.formulaTargets) {
            const stored = sheets[target.sheet]?.[target.cell];
            const value = sessionValue(target.key);
            if (stored && value !== undefined && value !== null) stored.value = String(value);
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
        return { sheets, extraValue: "", circular: false };
      }

      const rpcPayload = JSON.stringify({
        sheet: defaultSheet,
        sheets: Object.keys(sheets),
        targets: targetSpecs,
        cells,
      });
      const rpcRes = await invokeIpc("evaluate_sheet", rpcPayload);
      const targetValues = Array.isArray(rpcRes?.targetValues) ? rpcRes.targetValues : null;
      const batchValues =
        rpcRes && typeof rpcRes.values === "object" && rpcRes.values ? rpcRes.values : null;

      rpcTargets.forEach((target, idx) => {
        const rawVal = extractEvaluatedTargetValue(
          rpcRes,
          targetValues,
          batchValues,
          target,
          idx,
          rpcTargets.length
        );
        if (rawVal === undefined || rawVal === null) return;
        const storedEntry = sheets[target.sheet]?.[target.cell];
        if (storedEntry && isFormulaStr(storedEntry.formula || "")) {
          storedEntry.value = String(rawVal);
        }
      });

      const extraValue = extraTarget
        ? readExtraValue(rpcRes, sheets, extraTarget, rpcTargets.length)
        : "";
      return { sheets, extraValue, circular: rpcRes?.circular === true };
    }

    async function evaluateLiveFormulaTargets(sheetsMap, defaultSheet) {
      if (session.enabled) {
        if (!sheetMapHasAnyFormula(sheetsMap)) return null;
        const viaSession = await syncSession(sheetsMap);
        if (viaSession) {
          return {
            rpcRes: { circular: viaSession.circular },
            formulaTargets: viaSession.formulaTargets,
            targetValues: viaSession.formulaTargets.map((t) => sessionValue(t.key)),
            batchValues: null,
          };
        }
      }
      const { cells, formulaTargets } = buildCellsAndFormulaTargets(sheetsMap);
      if (formulaTargets.length === 0) {
        return null;
      }
      const rpcPayload = JSON.stringify({
        sheet: defaultSheet,
        sheets: Object.keys(sheetsMap),
        targets: formulaTargets.map(({ sheet, cell }) => ({ sheet, cell })),
        cells,
      });
      const rpcRes = await invokeIpc("evaluate_sheet", rpcPayload);
      const targetValues = Array.isArray(rpcRes?.targetValues) ? rpcRes.targetValues : null;
      const batchValues =
        rpcRes && typeof rpcRes.values === "object" && rpcRes.values ? rpcRes.values : null;
      return { rpcRes, formulaTargets, targetValues, batchValues };
    }

    function collectEntryScrubJob(entry, plainEntries, formulaJobs) {
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

    async function scrubSheetStoreMap(baseSheets, activeSheetName, trackAsyncFn) {
      const candidateSheets = cloneStore(baseSheets);
      const plainEntries = [];
      const formulaJobs = [];
      collectScrubJobs(candidateSheets, plainEntries, formulaJobs);

      const literalInputs = [];
      formulaJobs.forEach((job) => {
        job.literalSegs.forEach((seg) => literalInputs.push(seg));
      });

      const [plainResults, literalResults] = await Promise.all([
        Promise.all(plainEntries.map((entry) => invokeIpc("scrub_pii", entry.value))),
        Promise.all(literalInputs.map((seg) => invokeIpc("scrub_pii", seg.text))),
      ]);

      plainResults.forEach((res, idx) => {
        assertValidScrubResponse(plainEntries[idx].value, res);
      });
      literalResults.forEach((res, idx) => {
        assertValidScrubResponse(literalInputs[idx].text, res);
      });

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

    return {
      quoteFormulaSheetName,
      isFormulaStr,
      sheetMapHasAnyFormula,
      sheetCellDataChanged,
      extractEvaluatedTargetValue,
      computeEvaluatedSheetStore,
      evaluateLiveFormulaTargets,
      enableSheetSessions,
      scrubSheetStoreMap,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_STORE__ = createSheetStoreHelpers;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
