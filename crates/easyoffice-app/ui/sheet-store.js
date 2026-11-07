(function (host) {
  "use strict";

  const astApi = host.__EASYOFFICE_SHEET_UTILS__ || {};
  function fallbackIsFormulaStr(raw) {
    const s = String(raw ?? "").trim();
    return s.startsWith("=") || /^(?:of|oooc|msoxl):\s*=/i.test(s);
  }
  function fallbackAsciiUpper(str) {
    return String(str ?? "").replaceAll(/[a-z]/g, (c) =>
      String.fromCodePoint(c.codePointAt(0) - 32)
    );
  }
  function fallbackQuoteFormulaSheetName(sheetName) {
    const raw = String(sheetName ?? "");
    if (!raw) return "''";
    const needsQuote =
      raw !== raw.trim() ||
      !/^\w+$/.test(raw) ||
      /^\d/.test(raw) ||
      /^[A-Za-z]{1,3}\d+$/.test(raw) ||
      /^(?:TRUE|FALSE)$/i.test(raw);
    if (needsQuote) {
      return "'" + raw.replaceAll("'", "''") + "'";
    }
    return raw;
  }
  const isFormulaStr =
    typeof astApi.isFormulaStringJs === "function"
      ? astApi.isFormulaStringJs
      : fallbackIsFormulaStr;
  const asciiUpper =
    typeof astApi.asciiUpperJs === "function" ? astApi.asciiUpperJs : fallbackAsciiUpper;
  const quoteFormulaSheetName =
    typeof astApi.quoteFormulaSheetName === "function"
      ? astApi.quoteFormulaSheetName
      : fallbackQuoteFormulaSheetName;

  function cloneStore(sheets) {
    const fast = host.__EASYOFFICE_SHEET_GRID_UTILS__?.cloneSheetStore;
    return typeof fast === "function" ? fast(sheets || {}) : structuredClone(sheets || {});
  }

  function sheetCellDataChanged(sheetsA, sheetsB) {
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
          (ea?.formula || "") !== (eb?.formula || "")
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

  function createSheetStoreHelpers(deps) {
    const {
      invokeIpc,
      normalizeFormattedNumericCell,
      tokenizeFormulaStringLiterals,
    } = deps;

    function buildCellsAndFormulaTargets(sheetsMap) {
      const cells = [];
      const formulaTargets = [];
      Object.keys(sheetsMap || {}).forEach((shName) => {
        const snap = sheetsMap[shName] || {};
        Object.keys(snap).forEach((ref) => {
          const entry = snap[ref];
          if (!entry) return;
          if (isFormulaStr(entry.formula || "")) {
            formulaTargets.push({ sheet: shName, cell: ref, key: shName + "!" + ref });
          }
          if (entry.formula || entry.value) {
            const val = entry.formula
              ? entry.formula
              : normalizeFormattedNumericCell(entry.value);
            cells.push({ sheet: shName, cell: ref, value: val });
          }
        });
      });
      return { cells, formulaTargets };
    }

    function sheetMapHasAnyFormula(sheetsMap) {
      return Object.keys(sheetsMap || {}).some((shName) => {
        const snap = sheetsMap[shName] || {};
        return Object.keys(snap).some((ref) => isFormulaStr(snap[ref]?.formula || ""));
      });
    }

    async function computeEvaluatedSheetStore(baseSheets, defaultSheet, extraTarget = null) {
      const sheets = cloneStore(baseSheets);
      const { cells, formulaTargets } = buildCellsAndFormulaTargets(sheets);
      const rpcTargets = [];
      if (extraTarget) {
        rpcTargets.push({
          sheet: extraTarget.sheet,
          cell: extraTarget.cell,
          key: extraTarget.sheet + "!" + extraTarget.cell,
        });
      }
      formulaTargets.forEach((ft) => {
        if (!extraTarget || ft.sheet !== extraTarget.sheet || ft.cell !== extraTarget.cell) {
          rpcTargets.push(ft);
        }
      });

      if (rpcTargets.length === 0) {
        return { sheets, extraValue: "", circular: false };
      }

      const rpcPayload = JSON.stringify({
        sheet: defaultSheet,
        sheets: Object.keys(sheets),
        targets: rpcTargets.map(({ sheet, cell }) => ({ sheet, cell })),
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

      let extraValue = "";
      if (extraTarget) {
        const rawExtra = extractEvaluatedTargetValue(
          rpcRes,
          targetValues,
          batchValues,
          rpcTargets[0],
          0,
          rpcTargets.length
        );
        if (rawExtra !== undefined && rawExtra !== null) {
          extraValue = String(rawExtra);
        } else if (rpcRes?.value !== undefined && rpcRes.value !== null) {
          extraValue = String(rpcRes.value);
        } else {
          extraValue = sheets[extraTarget.sheet]?.[extraTarget.cell]?.value ?? "";
        }
      }
      return { sheets, extraValue, circular: rpcRes?.circular === true };
    }

    async function evaluateLiveFormulaTargets(sheetsMap, defaultSheet) {
      if (!sheetMapHasAnyFormula(sheetsMap)) return null;
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

    function assertValidScrubResponse(origText, res) {
      if (
        !res ||
        typeof res.scrubbedText !== "string" ||
        (origText.length > 0 && res.scrubbedText.length === 0)
      ) {
        throw new Error("개인정보 마스킹 응답 검증에 실패했습니다.");
      }
    }

    function isScrubbableLiteral(seg) {
      return seg.isLiteral && seg.text.trim().length > 0;
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
      scrubSheetStoreMap,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_STORE__ = createSheetStoreHelpers;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
