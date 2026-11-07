(function (host) {
  "use strict";

  const astApi = host.__EASYOFFICE_FORMULA_AST__;
  const {
    MAX_EXPANDED_CELL_COUNT,
    asciiUpperJs,
    jsonSheetNameOrDefaultJs,
    canonicalSheetKeyJs,
    isSameSheetIdentityJs,
    unquoteFormulaSheetTokenOnce,
    quoteFormulaSheetName,
    parseStrictA1Coord,
    normalizeA1Token,
    parseCellRefParts,
    colIndexToLetters,
    isFormulaStringJs,
    parseFormulaAstJs,
  } = astApi;

  function normalizeFormattedNumericCell(raw) {
    const trimmed = String(raw || "").trim();
    if (/^-?\d{1,3}(,\d{3})+(\.\d+)?$/.test(trimmed)) {
      return trimmed.replaceAll(",", "");
    }
    return trimmed;
  }

  function resolveCanonicalSheetUpper(knownSheets, rawName) {
    const targetKey = canonicalSheetKeyJs(rawName);
    if (!targetKey) return null;
    for (const knownSheet of knownSheets) {
      if (canonicalSheetKeyJs(knownSheet) === targetKey) {
        return targetKey;
      }
    }
    return null;
  }

  function ensureKnownSheetUpper(knownSheets, rawName) {
    const str = String(rawName ?? "");
    if (str.trim().length === 0) return null;
    const key = canonicalSheetKeyJs(str);
    if (!key) return null;
    const existing = resolveCanonicalSheetUpper(knownSheets, str);
    if (existing) return existing;
    knownSheets.push(str);
    return key;
  }

  function valEmpty() {
    return { kind: "empty" };
  }
  function valNum(num) {
    if (!Number.isFinite(num)) return { kind: "error", code: "#NUM!" };
    return { kind: "number", num };
  }
  function valText(text) {
    return { kind: "text", text: String(text ?? "") };
  }
  function valBool(flag) {
    return { kind: "bool", flag: Boolean(flag) };
  }
  function valErr(code) {
    return { kind: "error", code };
  }

  function formatFormulaValue(val) {
    if (!val || val.kind === "empty") return "";
    if (val.kind === "error") return val.code;
    if (val.kind === "bool") return val.flag ? "TRUE" : "FALSE";
    if (val.kind === "text") return val.text;
    const rounded = Math.round(val.num * 1e10) / 1e10;
    return Object.is(rounded, -0) ? "0" : String(rounded);
  }

  function coerceScalarNumber(val) {
    if (!val || val.kind === "empty") return 0;
    if (val.kind === "number") return val.num;
    if (val.kind === "bool") return val.flag ? 1 : 0;
    if (val.kind === "text") {
      const cleaned = normalizeFormattedNumericCell(val.text);
      if (!cleaned) return null;
      const parsed = Number(cleaned);
      return Number.isFinite(parsed) ? parsed : null;
    }
    return null;
  }

  function coerceScalarBool(val) {
    if (!val || val.kind === "empty") return false;
    if (val.kind === "bool") return val.flag;
    if (val.kind === "number") return val.num !== 0;
    if (val.kind === "text") {
      const up = asciiUpperJs(val.text.trim());
      if (up === "TRUE") return true;
      if (up === "FALSE") return false;
    }
    return null;
  }

  function parsePlainCellTyped(rawStr) {
    const raw = String(rawStr ?? "");
    const trimmed = raw.trim();
    if (!trimmed) return valEmpty();
    const cleanedNum = normalizeFormattedNumericCell(trimmed);
    const num = Number(cleanedNum);
    if (cleanedNum !== "" && Number.isFinite(num)) return valNum(num);
    const up = asciiUpperJs(trimmed);
    if (up === "TRUE") return valBool(true);
    if (up === "FALSE") return valBool(false);
    return valText(raw);
  }

  function readWorkbookTypedCell(workbook, shKey, cellRef, depth = 0) {
    if (!shKey) return valErr("#REF!");
    const normRef = normalizeA1Token(cellRef);
    if (!normRef) return valErr("#REF!");
    if (depth > 64) {
      if (workbook) workbook.depthTruncated = true;
      return valErr("#REF!");
    }
    const fullKey = shKey + "!" + normRef;
    if (workbook.memo.has(fullKey)) return workbook.memo.get(fullKey);
    const formula = workbook.formulaEntries[fullKey];
    if (formula !== undefined) {
      if (workbook.visiting.has(fullKey)) {
        workbook.circular = true;
        return valErr("#NUM!");
      }
      const savedTruncated = Boolean(workbook.depthTruncated);
      workbook.depthTruncated = false;
      workbook.visiting.add(fullKey);
      const computed = evaluateFormulaTyped(formula, workbook, shKey, normRef, depth + 1);
      workbook.visiting.delete(fullKey);
      const childTruncated = Boolean(workbook.depthTruncated);
      workbook.depthTruncated = savedTruncated || childTruncated;
      if (!childTruncated) {
        workbook.memo.set(fullKey, computed);
      }
      return computed;
    }
    const plain = parsePlainCellTyped(workbook.rawValues[fullKey] ?? "");
    workbook.memo.set(fullKey, plain);
    return plain;
  }

  const KIND_RANK = { number: 1, empty: 1, text: 2 };

  function kindRank(v) {
    return KIND_RANK[v.kind] ?? 3;
  }

  // Ordering of an empty cell against a number/text/bool; null for other kinds.
  function compareWithEmpty(other, emptyIsLeft) {
    if (other.kind === "number") return emptyIsLeft ? 0 - other.num : other.num - 0;
    if (other.kind === "text") {
      return emptyIsLeft ? "".localeCompare(other.text) : other.text.localeCompare("");
    }
    if (other.kind === "bool") {
      if (emptyIsLeft) return other.flag ? -1 : 0;
      return other.flag ? 1 : 0;
    }
    return null;
  }

  // Ordering when exactly one side is empty; null when neither is (or the other kind is unordered).
  function emptySideOrdering(lhs, rhs) {
    if (lhs.kind === "empty") return compareWithEmpty(rhs, true);
    if (rhs.kind === "empty") return compareWithEmpty(lhs, false);
    return null;
  }

  function compareTypedValues(lhs, rhs) {
    if (lhs.kind === "empty" && rhs.kind === "empty") return 0;
    const emptyOrdering = emptySideOrdering(lhs, rhs);
    if (emptyOrdering !== null) return emptyOrdering;
    if (kindRank(lhs) !== kindRank(rhs)) return kindRank(lhs) - kindRank(rhs);
    if (lhs.kind === "number") return lhs.num - rhs.num;
    if (lhs.kind === "text") {
      const a = lhs.text.toLowerCase();
      const b = rhs.text.toLowerCase();
      if (a === b) return 0;
      return a < b ? -1 : 1;
    }
    if (lhs.flag === rhs.flag) return 0;
    return lhs.flag ? 1 : -1;
  }

  // Mirrors the Rust engine: wide ranges stream only populated cells (sorted row-major) instead
  // of materializing every empty coordinate; MAX_EXPANDED_CELL_COUNT is the dense-walk threshold.
  function populatedRangeCoords(item, workbook) {
    const prefix = item.sheetUpper + "!";
    const coords = [];
    for (const key of Object.keys(workbook.rawValues)) {
      if (!key.startsWith(prefix)) continue;
      const parts = parseCellRefParts(key.slice(prefix.length));
      if (!parts) continue;
      const { col, row } = parts;
      const inside =
        row >= item.minRow && row <= item.maxRow && col >= item.minCol && col <= item.maxCol;
      if (inside) coords.push({ row, col });
    }
    coords.sort((a, b) => a.row - b.row || a.col - b.col);
    return coords;
  }

  function visitRangeCells(item, workbook, depth, visit) {
    const cellCount = (item.maxRow - item.minRow + 1) * (item.maxCol - item.minCol + 1);
    if (cellCount <= MAX_EXPANDED_CELL_COUNT) {
      for (let r = item.minRow; r <= item.maxRow; r += 1) {
        for (let c = item.minCol; c <= item.maxCol; c += 1) {
          const v = readWorkbookTypedCell(workbook, item.sheetUpper, colIndexToLetters(c) + r, depth);
          if (visit(v) === false) return { cellCount, visited: cellCount };
        }
      }
      return { cellCount, visited: cellCount };
    }
    const coords = populatedRangeCoords(item, workbook);
    for (const { row, col } of coords) {
      const v = readWorkbookTypedCell(workbook, item.sheetUpper, colIndexToLetters(col) + row, depth);
      if (visit(v) === false) break;
    }
    return { cellCount, visited: coords.length };
  }

  function isBlankValue(v) {
    return v.kind === "empty" || (v.kind === "text" && v.text === "");
  }

  // Folds one scalar argument into the aggregate state; returns an error value or null.
  function foldScalarAggregateArg(fnName, item, state) {
    if (item.kind === "error") return item;
    if (fnName === "COUNT") {
      if (item.kind === "number") state.nums.push(item.num);
    } else if (fnName === "COUNTA") {
      if (item.kind !== "empty") state.countAll += 1;
    } else if (fnName === "COUNTBLANK") {
      if (isBlankValue(item)) state.countBlank += 1;
    } else if (item.kind !== "empty") {
      const coerced = coerceScalarNumber(item);
      if (coerced === null) return valErr("#VALUE!");
      state.nums.push(coerced);
    }
    return null;
  }

  function foldRangeAggregateArg(item, workbook, depth, state) {
    if (!item.sheetUpper) return valErr("#REF!");
    let rangeErr = null;
    const { cellCount, visited } = visitRangeCells(item, workbook, depth, (cellVal) => {
      if (cellVal.kind === "error") {
        rangeErr = cellVal;
        return false;
      }
      if (cellVal.kind === "number") state.nums.push(cellVal.num);
      if (isBlankValue(cellVal)) state.countBlank += 1;
      if (cellVal.kind !== "empty") state.countAll += 1;
      return true;
    });
    if (rangeErr) return rangeErr;
    state.countBlank += cellCount - visited;
    return null;
  }

  function finishAggregate(fnName, nums) {
    const sum = nums.reduce((acc, num) => acc + num, 0);
    if (fnName === "SUM") return valNum(sum);
    if (fnName === "PRODUCT") {
      return valNum(nums.length === 0 ? 0 : nums.reduce((acc, num) => acc * num, 1));
    }
    if (fnName === "MIN") return valNum(nums.length === 0 ? 0 : nums.reduce((lo, num) => Math.min(lo, num)));
    if (fnName === "MAX") return valNum(nums.length === 0 ? 0 : nums.reduce((hi, num) => Math.max(hi, num)));
    if (fnName === "AVERAGE" || fnName === "AVG") {
      return nums.length === 0 ? valErr("#DIV/0!") : valNum(sum / nums.length);
    }
    if (nums.length < 2) return valErr("#DIV/0!");
    const mean = sum / nums.length;
    const m2 = nums.reduce((acc, num) => acc + (num - mean) * (num - mean), 0);
    const variance = m2 / (nums.length - 1);
    return valNum(fnName === "STDEV" ? Math.sqrt(variance) : variance);
  }

  function evalAggregateFunction(fnName, argItems, workbook, depth) {
    const state = { nums: [], countAll: 0, countBlank: 0 };
    for (const item of argItems) {
      const failure =
        item.kind === "range"
          ? foldRangeAggregateArg(item, workbook, depth, state)
          : foldScalarAggregateArg(fnName, item, state);
      if (failure) return failure;
    }
    if (fnName === "COUNT") return valNum(state.nums.length);
    if (fnName === "COUNTA") return valNum(state.countAll);
    if (fnName === "COUNTBLANK") return valNum(state.countBlank);
    return finishAggregate(fnName, state.nums);
  }

  // Counts one boolean-coercible value; returns an error value or null.
  function tallyLogicalValue(item, tally) {
    if (item.kind === "error") return item;
    const b = coerceScalarBool(item);
    if (b === null) return valErr("#VALUE!");
    tally.seen += 1;
    if (b) tally.trues += 1;
    return null;
  }

  function tallyLogicalRange(item, workbook, depth, tally) {
    if (!item.sheetUpper) return valErr("#REF!");
    let rangeErr = null;
    visitRangeCells(item, workbook, depth, (cellVal) => {
      if (cellVal.kind === "empty") return true;
      rangeErr = tallyLogicalValue(cellVal, tally);
      return rangeErr === null;
    });
    return rangeErr;
  }

  function evalLogicalFunction(fnName, argItems, workbook, depth) {
    const tally = { trues: 0, seen: 0 };
    for (const item of argItems) {
      const failure =
        item.kind === "range"
          ? tallyLogicalRange(item, workbook, depth, tally)
          : tallyLogicalValue(item, tally);
      if (failure) return failure;
    }
    if (tally.seen === 0) return valErr("#VALUE!");
    if (fnName === "AND") return valBool(tally.trues === tally.seen);
    if (fnName === "OR") return valBool(tally.trues > 0);
    return valBool(tally.trues % 2 === 1);
  }

  function validateRefTargetSheet(refNode, workbook, currentSheetUpper) {
    const resolved =
      refNode.sheet === null
        ? currentSheetUpper
        : resolveCanonicalSheetUpper(workbook.knownSheets, refNode.sheet);
    if (!resolved) return null;
    if (refNode.kind === "Range" && refNode.endSheet !== null) {
      const lhsId = refNode.sheet === null ? currentSheetUpper : refNode.sheet;
      if (!isSameSheetIdentityJs(lhsId, refNode.endSheet)) return null;
      if (!resolveCanonicalSheetUpper(workbook.knownSheets, refNode.endSheet)) return null;
    }
    return resolved;
  }

  const AGGREGATE_FN_SET = new Set([
    "SUM",
    "AVERAGE",
    "AVG",
    "MIN",
    "MAX",
    "COUNT",
    "COUNTA",
    "COUNTBLANK",
    "PRODUCT",
    "STDEV",
    "VAR",
  ]);

  // Advances over a `'` at index i; returns { inQuotes, next } (a doubled quote stays quoted).
  function stepQuoteChar(src, i, inQuotes) {
    if (!inQuotes) return { inQuotes: true, next: i };
    if (src[i + 1] === "'") return { inQuotes: true, next: i + 1 };
    return { inQuotes: false, next: i };
  }

  function splitOnceUnquotedStrict(src, delimiter) {
    let inQuotes = false;
    let splitIdx = -1;
    let i = 0;
    while (i < src.length) {
      const ch = src[i];
      if (ch === "'") {
        const step = stepQuoteChar(src, i, inQuotes);
        inQuotes = step.inQuotes;
        i = step.next;
      } else if (!inQuotes && ch === delimiter) {
        if (splitIdx !== -1) return { ok: false };
        splitIdx = i;
      }
      i += 1;
    }
    if (inQuotes) return { ok: false };
    if (splitIdx === -1) return { ok: true, parts: [src] };
    return { ok: true, parts: [src.slice(0, splitIdx), src.slice(splitIdx + 1)] };
  }

  const BARE_SHEET_NAME_PATTERN = /^[\p{Alphabetic}_$][\p{Alphabetic}\p{N}_$]*$/u;

  // Sheet name of an INDIRECT endpoint prefix, or null when it is invalid.
  function parseIndirectSheetName(shRaw) {
    if (shRaw.startsWith("'")) {
      const unquoted = unquoteFormulaSheetTokenOnce(shRaw);
      return unquoted && unquoted.trim().length > 0 ? unquoted : null;
    }
    return BARE_SHEET_NAME_PATTERN.test(shRaw) ? shRaw : null;
  }

  function parseIndirectEndpoint(rawPart) {
    const bangSplit = splitOnceUnquotedStrict(String(rawPart ?? "").trim(), "!");
    if (!bangSplit.ok) return { ok: false };
    let sheetName = null;
    let coordRaw = bangSplit.parts[0].trim();
    if (bangSplit.parts.length === 2) {
      const shRaw = bangSplit.parts[0].trim();
      coordRaw = bangSplit.parts[1].trim();
      if (!shRaw || !coordRaw) return { ok: false };
      sheetName = parseIndirectSheetName(shRaw);
      if (sheetName === null) return { ok: false };
    }
    const coord = parseStrictA1Coord(coordRaw);
    if (!coord.ok) return { ok: false };
    return { ok: true, sheet: sheetName, col: coord.col, row: coord.row };
  }

  // Validates the end endpoint sheet of an INDIRECT range against the start endpoint.
  function isValidIndirectEndSheet(lhs, rhs, workbook) {
    if (rhs.sheet === null) return true;
    if (lhs.sheet === null) return false;
    if (!isSameSheetIdentityJs(lhs.sheet, rhs.sheet)) return false;
    return Boolean(resolveCanonicalSheetUpper(workbook.knownSheets, rhs.sheet));
  }

  function resolveIndirectRange(refText, workbook, currentSheetUpper) {
    const trimmed = String(refText ?? "").trim();
    if (!trimmed) return valErr("#REF!");
    const colonSplit = splitOnceUnquotedStrict(trimmed, ":");
    if (!colonSplit.ok) return valErr("#REF!");
    const lhs = parseIndirectEndpoint(colonSplit.parts[0]);
    if (!lhs.ok) return valErr("#REF!");
    const sheetUpper =
      lhs.sheet === null
        ? currentSheetUpper
        : resolveCanonicalSheetUpper(workbook.knownSheets, lhs.sheet);
    if (!sheetUpper) return valErr("#REF!");
    const rhs = colonSplit.parts.length === 1 ? lhs : parseIndirectEndpoint(colonSplit.parts[1]);
    if (!rhs.ok) return valErr("#REF!");
    if (rhs !== lhs && !isValidIndirectEndSheet(lhs, rhs, workbook)) return valErr("#REF!");
    const minCol = Math.min(lhs.col, rhs.col);
    const maxCol = Math.max(lhs.col, rhs.col);
    const minRow = Math.min(lhs.row, rhs.row);
    const maxRow = Math.max(lhs.row, rhs.row);
    const cellCount = (maxRow - minRow + 1) * (maxCol - minCol + 1);
    return { kind: "range", sheetUpper, minCol, maxCol, minRow, maxRow, cellCount };
  }

  function isChainBinaryNode(node) {
    return (
      Boolean(node) &&
      (node.kind === "Binary" || node.kind === "Concat" || node.kind === "Compare")
    );
  }

  function applyChainBinaryStep(step, lhs, rhs) {
    if (step.kind === "Concat") {
      return valText(formatFormulaValue(lhs) + formatFormulaValue(rhs));
    }
    if (step.kind === "Compare") {
      const cmp = compareTypedValues(lhs, rhs);
      if (step.op === "=") return valBool(cmp === 0);
      if (step.op === "<>") return valBool(cmp !== 0);
      if (step.op === "<") return valBool(cmp < 0);
      if (step.op === "<=") return valBool(cmp <= 0);
      if (step.op === ">") return valBool(cmp > 0);
      return valBool(cmp >= 0);
    }
    const a = coerceScalarNumber(lhs);
    const b = coerceScalarNumber(rhs);
    if (a === null || b === null) return valErr("#VALUE!");
    if (step.op === "+") return valNum(a + b);
    if (step.op === "-") return valNum(a - b);
    if (step.op === "*") return valNum(a * b);
    if (step.op === "/") {
      if (b === 0) return valErr("#DIV/0!");
      return valNum(a / b);
    }
    return valNum(a ** b);
  }

  function ensureScalarIn(ec, v) {
    if (v?.kind !== "range") return v;
    if (v.minCol !== v.maxCol || v.minRow !== v.maxRow) return valErr("#VALUE!");
    if (!v.sheetUpper) return valErr("#REF!");
    return readWorkbookTypedCell(
      ec.workbook,
      v.sheetUpper,
      colIndexToLetters(v.minCol) + v.minRow,
      ec.depth
    );
  }

  function evalIndirectCallIn(ec, args) {
    if (args.length !== 1 && args.length !== 2) return valErr("#VALUE!");
    const refVal = ensureScalarIn(ec, evalAstIn(ec, args[0]));
    if (refVal.kind === "error") return refVal;
    if (args.length === 2) {
      const modeVal = ensureScalarIn(ec, evalAstIn(ec, args[1]));
      if (modeVal.kind === "error") return modeVal;
      if (coerceScalarBool(modeVal) !== true) return valErr("#VALUE!");
    }
    return resolveIndirectRange(formatFormulaValue(refVal), ec.workbook, ec.currentSheetUpper);
  }

  function evalChainIn(ec, node) {
    const ops = [];
    let cur = node;
    while (isChainBinaryNode(cur)) {
      ops.push(cur);
      cur = cur.lhs;
    }
    let acc = ensureScalarIn(ec, evalAstIn(ec, cur));
    if (acc.kind === "error") return acc;
    for (let i = ops.length - 1; i >= 0; i -= 1) {
      const step = ops[i];
      const rhs = ensureScalarIn(ec, evalAstIn(ec, step.rhs));
      if (rhs.kind === "error") return rhs;
      acc = applyChainBinaryStep(step, acc, rhs);
      if (acc.kind === "error") return acc;
    }
    return acc;
  }

  function evalRangeNodeIn(ec, node) {
    const sheetUpper = validateRefTargetSheet(node, ec.workbook, ec.currentSheetUpper);
    if (!sheetUpper) return valErr("#REF!");
    return {
      kind: "range",
      sheetUpper,
      minCol: node.minCol,
      maxCol: node.maxCol,
      minRow: node.minRow,
      maxRow: node.maxRow,
      cellCount: node.cellCount,
    };
  }

  function evalUnaryIn(ec, node) {
    const inner = ensureScalarIn(ec, evalAstIn(ec, node.expr));
    if (inner.kind === "error") return inner;
    const n = coerceScalarNumber(inner);
    if (n === null) return valErr("#VALUE!");
    if (node.op === "-") return valNum(-n);
    if (node.op === "+") return valNum(n);
    return valNum(n / 100);
  }

  function evalIfIn(ec, args) {
    if (args.length < 2 || args.length > 3) return valErr("#VALUE!");
    const cond = ensureScalarIn(ec, evalAstIn(ec, args[0]));
    if (cond.kind === "error") return cond;
    const flag = coerceScalarBool(cond);
    if (flag === null) return valErr("#VALUE!");
    if (flag) return ensureScalarIn(ec, evalAstIn(ec, args[1]));
    if (args.length === 3) return ensureScalarIn(ec, evalAstIn(ec, args[2]));
    return valBool(false);
  }

  function evalIfErrorIn(ec, args) {
    if (args.length !== 2) return valErr("#VALUE!");
    const primary = ensureScalarIn(ec, evalAstIn(ec, args[0]));
    if (primary.kind === "error") return ensureScalarIn(ec, evalAstIn(ec, args[1]));
    return primary;
  }

  // Shared target handling of ROW/COLUMN/ROWS/COLUMNS: reference nodes are inspected without
  // evaluating them. `pick(shape)` maps a { minRow, maxRow, minCol, maxCol } shape to a number.
  function evalReferenceInfoIn(ec, target, pick, cellShape) {
    if (target.kind === "Error") return valErr(target.code);
    if (target.kind === "Cell" || target.kind === "Range") {
      if (!validateRefTargetSheet(target, ec.workbook, ec.currentSheetUpper)) {
        return valErr("#REF!");
      }
      return valNum(pick(target.kind === "Cell" ? cellShape(target) : target));
    }
    if (target.kind === "FuncCall" && target.name === "INDIRECT") {
      const ind = evalIndirectCallIn(ec, target.args);
      return ind.kind === "error" ? ind : valNum(pick(ind));
    }
    return null;
  }

  function evalRowColumnIn(ec, fnUpper, args) {
    const isRow = fnUpper === "ROW";
    if (args.length === 0) {
      if (!ec.currentSheetUpper) return valErr("#REF!");
      const parts = parseCellRefParts(ec.currentCellRef || "A1");
      return valNum(isRow ? parts.row : parts.col + 1);
    }
    if (args.length !== 1) return valErr("#VALUE!");
    const info = evalReferenceInfoIn(
      ec,
      args[0],
      (shape) => (isRow ? shape.minRow : shape.minCol + 1),
      (cell) => ({ minRow: cell.row, minCol: cell.col })
    );
    return info ?? valErr("#VALUE!");
  }

  function evalRowsColumnsIn(ec, fnUpper, args) {
    if (args.length !== 1) return valErr("#VALUE!");
    const isRows = fnUpper === "ROWS";
    const info = evalReferenceInfoIn(
      ec,
      args[0],
      (shape) => (isRows ? shape.maxRow - shape.minRow + 1 : shape.maxCol - shape.minCol + 1),
      () => ({ minRow: 0, maxRow: 0, minCol: 0, maxCol: 0 })
    );
    if (info) return info;
    const scalarTarget = ensureScalarIn(ec, evalAstIn(ec, args[0]));
    return scalarTarget.kind === "error" ? scalarTarget : valNum(1);
  }

  function scalarAbs(args) {
    const n = coerceScalarNumber(args[0]);
    return n === null ? valErr("#VALUE!") : valNum(Math.abs(n));
  }

  function scalarRound(args) {
    const n = coerceScalarNumber(args[0]);
    const digits = args.length === 2 ? coerceScalarNumber(args[1]) : 0;
    if (n === null || digits === null) return valErr("#VALUE!");
    const factor = 10 ** Math.trunc(digits);
    return valNum(Math.round(n * factor) / factor);
  }

  function scalarLen(args) {
    return valNum(Array.from(formatFormulaValue(args[0])).length);
  }

  function scalarNot(args) {
    const b = coerceScalarBool(args[0]);
    return b === null ? valErr("#VALUE!") : valBool(!b);
  }

  // Scalar functions: [min arity, max arity, implementation].
  const SCALAR_FUNCTIONS = new Map([
    ["ABS", [1, 1, scalarAbs]],
    ["ROUND", [1, 2, scalarRound]],
    ["LEN", [1, 1, scalarLen]],
    ["NOT", [1, 1, scalarNot]],
  ]);

  function evalScalarFunction(fnUpper, scalarArgs) {
    const spec = SCALAR_FUNCTIONS.get(fnUpper);
    if (!spec) return valErr("#NAME?");
    const [minArity, maxArity, impl] = spec;
    if (scalarArgs.length < minArity || scalarArgs.length > maxArity) return valErr("#NAME?");
    return impl(scalarArgs);
  }

  // Functions that need their unevaluated argument nodes (lazy / reference semantics).
  const SPECIAL_FORM_HANDLERS = new Map([
    ["INDIRECT", (ec, _fn, args) => evalIndirectCallIn(ec, args)],
    ["IF", (ec, _fn, args) => evalIfIn(ec, args)],
    ["IFERROR", (ec, _fn, args) => evalIfErrorIn(ec, args)],
    ["ROW", evalRowColumnIn],
    ["COLUMN", evalRowColumnIn],
    ["ROWS", evalRowsColumnsIn],
    ["COLUMNS", evalRowsColumnsIn],
  ]);

  function evalFuncCallIn(ec, node) {
    const fnUpper = node.name;
    const args = node.args;
    const specialForm = SPECIAL_FORM_HANDLERS.get(fnUpper);
    if (specialForm) return specialForm(ec, fnUpper, args);
    const evaluatedArgs = args.map((arg) => evalAstIn(ec, arg));
    if (AGGREGATE_FN_SET.has(fnUpper)) {
      return evalAggregateFunction(fnUpper, evaluatedArgs, ec.workbook, ec.depth);
    }
    const isLogical = fnUpper === "AND" || fnUpper === "OR" || fnUpper === "XOR";
    if (isLogical && evaluatedArgs.length >= 1) {
      return evalLogicalFunction(fnUpper, evaluatedArgs, ec.workbook, ec.depth);
    }
    const scalarArgs = [];
    for (const evalArg of evaluatedArgs) {
      const sv = ensureScalarIn(ec, evalArg);
      if (sv.kind === "error") return sv;
      scalarArgs.push(sv);
    }
    return evalScalarFunction(fnUpper, scalarArgs);
  }

  function evalAstIn(ec, node) {
    if (!node || node.kind === "Empty") return valEmpty();
    if (isChainBinaryNode(node)) return evalChainIn(ec, node);
    if (node.kind === "Number") return valNum(node.value);
    if (node.kind === "Str") return valText(node.value);
    if (node.kind === "Bool") return valBool(node.value);
    if (node.kind === "Error") return valErr(node.code);
    if (node.kind === "Cell") {
      const sheetUpper = validateRefTargetSheet(node, ec.workbook, ec.currentSheetUpper);
      if (!sheetUpper) return valErr("#REF!");
      return readWorkbookTypedCell(ec.workbook, sheetUpper, node.ref, ec.depth);
    }
    if (node.kind === "Range") return evalRangeNodeIn(ec, node);
    if (node.kind === "Unary") return evalUnaryIn(ec, node);
    if (node.kind === "FuncCall") return evalFuncCallIn(ec, node);
    return valErr("#VALUE!");
  }

  function evaluateFormulaTyped(exprStr, workbook, currentSheetUpper, currentCellRef, depth = 0) {
    if (depth > 64) {
      if (workbook) workbook.depthTruncated = true;
      return valErr("#REF!");
    }
    const parsed = parseFormulaAstJs(exprStr);
    if (parsed.kind === "Plain") return parsePlainCellTyped(parsed.raw);
    if (parsed.kind === "FormulaError") return valErr(parsed.code);
    const ec = { workbook, currentSheetUpper, currentCellRef, depth };
    return ensureScalarIn(ec, evalAstIn(ec, parsed.root));
  }

  function evaluateFormulaExpression(exprStr, workbook, currentSheetUpper, currentCellRef) {
    if (workbook) workbook.depthTruncated = false;
    return formatFormulaValue(
      evaluateFormulaTyped(exprStr, workbook, currentSheetUpper, currentCellRef, 0)
    );
  }

  function scalarJsonCellValue(rawVal) {
    if (rawVal === undefined || rawVal === null) return "";
    if (typeof rawVal === "string") return rawVal;
    if (typeof rawVal === "number" && Number.isFinite(rawVal)) return String(rawVal);
    if (typeof rawVal === "boolean") return rawVal ? "TRUE" : "FALSE";
    throw new Error("InvalidArgument: JSON 셀 'value'는 스칼라여야 합니다.");
  }

  function validateExplicitSheetsArrayJs(sheetsArr) {
    if (!Array.isArray(sheetsArr)) {
      throw new TypeError("InvalidArgument: JSON 'sheets' 필드는 문자열 배열이어야 합니다.");
    }
    for (const s of sheetsArr) {
      if (typeof s !== "string") {
        throw new TypeError("InvalidArgument: JSON 'sheets' 항목은 문자열이어야 합니다.");
      }
      if (s.trim().length === 0) {
        throw new Error("InvalidArgument: JSON 'sheets' 항목은 비어 있거나 공백만일 수 없습니다.");
      }
    }
  }

  function resolveDefaultSheetFromDocJs(doc) {
    if (doc.sheet !== undefined && doc.sheet !== null && typeof doc.sheet !== "string") {
      throw new Error("InvalidArgument: JSON 'sheet' 필드는 문자열이어야 합니다.");
    }
    if (doc.sheets !== undefined) {
      validateExplicitSheetsArrayJs(doc.sheets);
    }
    const rawTop = String(doc.sheet ?? "");
    if (rawTop.trim().length > 0) return rawTop;
    if (doc.sheets !== undefined) {
      return doc.sheets.length > 0 ? doc.sheets[0] : "";
    }
    return "Sheet1";
  }

  function buildLocalWorkbookFromJson(doc, defaultSheet) {
    const knownSheets = [];
    const hasExplicitSheets = doc.sheets !== undefined;
    if (hasExplicitSheets) {
      validateExplicitSheetsArrayJs(doc.sheets);
      for (const sheetName of doc.sheets) {
        ensureKnownSheetUpper(knownSheets, sheetName);
      }
    } else {
      ensureKnownSheetUpper(knownSheets, defaultSheet);
    }
    if (doc.cells !== undefined && !Array.isArray(doc.cells)) {
      throw new Error("InvalidArgument: JSON 'cells' 필드는 배열이어야 합니다.");
    }
    const cells = doc.cells || [];
    const rawValues = {};
    const formulaEntries = {};
    const triples = [];

    for (const item of cells) {
      if (!item || typeof item !== "object" || Array.isArray(item)) {
        throw new Error("InvalidArgument: JSON 'cells' 항목은 객체여야 합니다.");
      }
      if (item.sheet !== undefined && item.sheet !== null && typeof item.sheet !== "string") {
        throw new Error("InvalidArgument: JSON 셀 'sheet' 필드는 문자열이어야 합니다.");
      }
      if (typeof item.cell !== "string" || !item.cell.trim()) {
        throw new Error("InvalidArgument: JSON 셀 'cell' 주소가 비어 있습니다.");
      }
      const shName = jsonSheetNameOrDefaultJs(item.sheet ?? defaultSheet, defaultSheet);
      const addrNorm = normalizeA1Token(item.cell);
      if (!addrNorm) {
        throw new Error("InvalidArgument: 유효하지 않은 셀 주소입니다: " + item.cell);
      }
      const strVal = scalarJsonCellValue(item.value);
      const shUpper = hasExplicitSheets
        ? resolveCanonicalSheetUpper(knownSheets, shName)
        : ensureKnownSheetUpper(knownSheets, shName);
      if (!shUpper) continue;
      triples.push({ shUpper, addrUpper: addrNorm, strVal });
    }

    for (const triple of triples) {
      const { shUpper, addrUpper, strVal } = triple;
      const fullKey = shUpper + "!" + addrUpper;
      rawValues[fullKey] = strVal;
      if (isFormulaStringJs(strVal)) {
        formulaEntries[fullKey] = strVal.trim();
      } else {
        delete formulaEntries[fullKey];
      }
    }

    return {
      rawValues,
      formulaEntries,
      knownSheets,
      memo: new Map(),
      visiting: new Set(),
      depthTruncated: false,
      circular: false,
    };
  }

  host.__EASYOFFICE_OFFLINE_FORMULA_EVAL__ = {
    normalizeA1Token,
    parseCellRefParts,
    colIndexToLetters,
    normalizeFormattedNumericCell,
    jsonSheetNameOrDefaultJs,
    canonicalSheetKeyJs,
    isSameSheetIdentityJs,
    quoteFormulaSheetName,
    isFormulaStringJs,
    resolveCanonicalSheetUpper,
    resolveDefaultSheetFromDocJs,
    buildLocalWorkbookFromJson,
    readWorkbookTypedCell,
    formatFormulaValue,
    evaluateFormulaExpression,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
