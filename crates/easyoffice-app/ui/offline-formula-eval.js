(function (host) {
  "use strict";

  const astApi = host.__EASYOFFICE_FORMULA_AST__;
  const {
    MAX_PARSE_DEPTH,
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
    for (let i = 0; i < knownSheets.length; i += 1) {
      if (canonicalSheetKeyJs(knownSheets[i]) === targetKey) {
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
      if (workbook.visiting.has(fullKey)) return valErr("#CYCLE!");
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

  function compareTypedValues(lhs, rhs) {
    const rank = (v) => {
      if (v.kind === "number" || v.kind === "empty") return 1;
      if (v.kind === "text") return 2;
      return 3;
    };
    if (lhs.kind === "empty" && rhs.kind === "empty") return 0;
    if (lhs.kind === "empty" && rhs.kind === "number") return 0 - rhs.num;
    if (lhs.kind === "number" && rhs.kind === "empty") return lhs.num - 0;
    if (lhs.kind === "empty" && rhs.kind === "text") return "".localeCompare(rhs.text);
    if (lhs.kind === "text" && rhs.kind === "empty") return lhs.text.localeCompare("");
    if (lhs.kind === "empty" && rhs.kind === "bool") return rhs.flag ? -1 : 0;
    if (lhs.kind === "bool" && rhs.kind === "empty") return lhs.flag ? 1 : 0;
    if (rank(lhs) !== rank(rhs)) return rank(lhs) - rank(rhs);
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

  function evalAggregateFunction(fnName, argItems, workbook, depth) {
    const nums = [];
    let countAll = 0;
    let countBlank = 0;
    for (let idx = 0; idx < argItems.length; idx += 1) {
      const item = argItems[idx];
      if (item.kind === "range") {
        if (!item.sheetUpper) return valErr("#REF!");
        const cellCount = (item.maxRow - item.minRow + 1) * (item.maxCol - item.minCol + 1);
        if (cellCount > MAX_EXPANDED_CELL_COUNT) return valErr("#REF!");
        for (let r = item.minRow; r <= item.maxRow; r += 1) {
          for (let c = item.minCol; c <= item.maxCol; c += 1) {
            const cellVal = readWorkbookTypedCell(
              workbook,
              item.sheetUpper,
              colIndexToLetters(c) + r,
              depth
            );
            if (cellVal.kind === "error") return cellVal;
            if (cellVal.kind === "number") nums.push(cellVal.num);
            if (cellVal.kind === "empty" || (cellVal.kind === "text" && cellVal.text === "")) {
              countBlank += 1;
            }
            if (cellVal.kind !== "empty") countAll += 1;
          }
        }
      } else {
        if (item.kind === "error") return item;
        if (fnName === "COUNT") {
          if (item.kind === "number") nums.push(item.num);
        } else if (fnName === "COUNTA") {
          if (item.kind !== "empty") countAll += 1;
        } else if (fnName === "COUNTBLANK") {
          if (item.kind === "empty" || (item.kind === "text" && item.text === "")) countBlank += 1;
        } else if (item.kind !== "empty") {
          const coerced = coerceScalarNumber(item);
          if (coerced === null) return valErr("#VALUE!");
          nums.push(coerced);
        }
      }
    }
    if (fnName === "COUNT") return valNum(nums.length);
    if (fnName === "COUNTA") return valNum(countAll);
    if (fnName === "COUNTBLANK") return valNum(countBlank);
    let sum = 0;
    for (let i = 0; i < nums.length; i += 1) {
      sum += nums[i];
    }
    if (fnName === "SUM") return valNum(sum);
    if (fnName === "PRODUCT") {
      if (nums.length === 0) return valNum(0);
      let prod = 1;
      for (let i = 0; i < nums.length; i += 1) {
        prod *= nums[i];
      }
      return valNum(prod);
    }
    if (fnName === "MIN") {
      if (nums.length === 0) return valNum(0);
      let minVal = nums[0];
      for (let i = 1; i < nums.length; i += 1) {
        if (nums[i] < minVal) minVal = nums[i];
      }
      return valNum(minVal);
    }
    if (fnName === "MAX") {
      if (nums.length === 0) return valNum(0);
      let maxVal = nums[0];
      for (let i = 1; i < nums.length; i += 1) {
        if (nums[i] > maxVal) maxVal = nums[i];
      }
      return valNum(maxVal);
    }
    if (fnName === "AVERAGE" || fnName === "AVG") {
      if (nums.length === 0) return valErr("#DIV/0!");
      return valNum(sum / nums.length);
    }
    if (nums.length < 2) return valErr("#DIV/0!");
    const mean = sum / nums.length;
    let m2 = 0;
    for (let i = 0; i < nums.length; i += 1) {
      const diff = nums[i] - mean;
      m2 += diff * diff;
    }
    const variance = m2 / (nums.length - 1);
    return valNum(fnName === "STDEV" ? Math.sqrt(variance) : variance);
  }

  function evalLogicalFunction(fnName, argItems, workbook, depth) {
    let trues = 0;
    let seen = 0;
    for (let idx = 0; idx < argItems.length; idx += 1) {
      const item = argItems[idx];
      if (item.kind === "range") {
        if (!item.sheetUpper) return valErr("#REF!");
        const cellCount = (item.maxRow - item.minRow + 1) * (item.maxCol - item.minCol + 1);
        if (cellCount > MAX_EXPANDED_CELL_COUNT) return valErr("#REF!");
        for (let r = item.minRow; r <= item.maxRow; r += 1) {
          for (let c = item.minCol; c <= item.maxCol; c += 1) {
            const cellVal = readWorkbookTypedCell(
              workbook,
              item.sheetUpper,
              colIndexToLetters(c) + r,
              depth
            );
            if (cellVal.kind === "error") return cellVal;
            if (cellVal.kind === "empty") continue;
            const b = coerceScalarBool(cellVal);
            if (b === null) return valErr("#VALUE!");
            seen += 1;
            if (b) trues += 1;
          }
        }
      } else {
        if (item.kind === "error") return item;
        const b = coerceScalarBool(item);
        if (b === null) return valErr("#VALUE!");
        seen += 1;
        if (b) trues += 1;
      }
    }
    if (seen === 0) return valErr("#VALUE!");
    if (fnName === "AND") return valBool(trues === seen);
    if (fnName === "OR") return valBool(trues > 0);
    return valBool(trues % 2 === 1);
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

  function splitOnceUnquotedStrict(src, delimiter) {
    let inQuotes = false;
    let splitIdx = -1;
    for (let i = 0; i < src.length; i += 1) {
      const ch = src[i];
      if (ch === "'") {
        if (inQuotes) {
          if (i + 1 < src.length && src[i + 1] === "'") {
            i += 1;
            continue;
          }
          inQuotes = false;
        } else {
          inQuotes = true;
        }
        continue;
      }
      if (!inQuotes && ch === delimiter) {
        if (splitIdx !== -1) return { ok: false };
        splitIdx = i;
      }
    }
    if (inQuotes) return { ok: false };
    if (splitIdx === -1) return { ok: true, parts: [src] };
    return { ok: true, parts: [src.slice(0, splitIdx), src.slice(splitIdx + 1)] };
  }

  function parseIndirectEndpoint(rawPart) {
    const bangSplit = splitOnceUnquotedStrict(String(rawPart ?? "").trim(), "!");
    if (!bangSplit.ok) return { ok: false };
    if (bangSplit.parts.length === 2) {
      const shRaw = bangSplit.parts[0].trim();
      const coordRaw = bangSplit.parts[1].trim();
      if (!shRaw || !coordRaw) return { ok: false };
      let sheetName = null;
      if (shRaw.startsWith("'")) {
        sheetName = unquoteFormulaSheetTokenOnce(shRaw);
        if (!sheetName || sheetName.trim().length === 0) return { ok: false };
      } else {
        if (!/^[\p{Alphabetic}_$][\p{Alphabetic}\p{N}_$]*$/u.test(shRaw)) {
          return { ok: false };
        }
        sheetName = shRaw;
      }
      const coord = parseStrictA1Coord(coordRaw);
      if (!coord.ok) return { ok: false };
      return { ok: true, sheet: sheetName, col: coord.col, row: coord.row };
    }
    const coord = parseStrictA1Coord(bangSplit.parts[0].trim());
    if (!coord.ok) return { ok: false };
    return { ok: true, sheet: null, col: coord.col, row: coord.row };
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
    if (colonSplit.parts.length === 1) {
      return {
        kind: "range",
        sheetUpper,
        minCol: lhs.col,
        maxCol: lhs.col,
        minRow: lhs.row,
        maxRow: lhs.row,
        cellCount: 1,
      };
    }
    const rhs = parseIndirectEndpoint(colonSplit.parts[1]);
    if (!rhs.ok) return valErr("#REF!");
    if (lhs.sheet === null && rhs.sheet !== null) return valErr("#REF!");
    if (rhs.sheet !== null) {
      if (!isSameSheetIdentityJs(lhs.sheet, rhs.sheet)) return valErr("#REF!");
      if (!resolveCanonicalSheetUpper(workbook.knownSheets, rhs.sheet)) return valErr("#REF!");
    }
    const minCol = Math.min(lhs.col, rhs.col);
    const maxCol = Math.max(lhs.col, rhs.col);
    const minRow = Math.min(lhs.row, rhs.row);
    const maxRow = Math.max(lhs.row, rhs.row);
    const cellCount = (maxRow - minRow + 1) * (maxCol - minCol + 1);
    if (cellCount > MAX_EXPANDED_CELL_COUNT) return valErr("#REF!");
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

  function evaluateFormulaTyped(exprStr, workbook, currentSheetUpper, currentCellRef, depth = 0) {
    if (depth > 64) {
      if (workbook) workbook.depthTruncated = true;
      return valErr("#REF!");
    }
    const parsed = parseFormulaAstJs(exprStr);
    if (parsed.kind === "Plain") return parsePlainCellTyped(parsed.raw);
    if (parsed.kind === "FormulaError") return valErr(parsed.code);

    function ensureScalar(v) {
      if (v && v.kind === "range") {
        if (v.minCol === v.maxCol && v.minRow === v.maxRow) {
          if (!v.sheetUpper) return valErr("#REF!");
          return readWorkbookTypedCell(
            workbook,
            v.sheetUpper,
            colIndexToLetters(v.minCol) + v.minRow,
            depth
          );
        }
        return valErr("#VALUE!");
      }
      return v;
    }

    function evalIndirectCall(args, astDepth) {
      if (args.length !== 1 && args.length !== 2) return valErr("#VALUE!");
      const refVal = ensureScalar(evalAst(args[0], astDepth));
      if (refVal.kind === "error") return refVal;
      if (args.length === 2) {
        const modeVal = ensureScalar(evalAst(args[1], astDepth));
        if (modeVal.kind === "error") return modeVal;
        const a1Flag = coerceScalarBool(modeVal);
        if (a1Flag !== true) return valErr("#VALUE!");
      }
      return resolveIndirectRange(formatFormulaValue(refVal), workbook, currentSheetUpper);
    }

    function evalAst(node, astDepth = 0) {
      if (astDepth > MAX_PARSE_DEPTH) return valErr("#VALUE!");
      if (!node || node.kind === "Empty") return valEmpty();
      if (isChainBinaryNode(node)) {
        const ops = [];
        let cur = node;
        while (isChainBinaryNode(cur)) {
          ops.push(cur);
          cur = cur.lhs;
        }
        let acc = ensureScalar(evalAst(cur, astDepth));
        if (acc.kind === "error") return acc;
        for (let i = ops.length - 1; i >= 0; i -= 1) {
          const step = ops[i];
          const rhs = ensureScalar(evalAst(step.rhs, astDepth + 1));
          if (rhs.kind === "error") return rhs;
          acc = applyChainBinaryStep(step, acc, rhs);
          if (acc.kind === "error") return acc;
        }
        return acc;
      }
      if (node.kind === "Number") return valNum(node.value);
      if (node.kind === "Str") return valText(node.value);
      if (node.kind === "Bool") return valBool(node.value);
      if (node.kind === "Error") return valErr(node.code);
      if (node.kind === "Cell") {
        const sheetUpper = validateRefTargetSheet(node, workbook, currentSheetUpper);
        if (!sheetUpper) return valErr("#REF!");
        return readWorkbookTypedCell(workbook, sheetUpper, node.ref, depth);
      }
      if (node.kind === "Range") {
        const sheetUpper = validateRefTargetSheet(node, workbook, currentSheetUpper);
        if (!sheetUpper) return valErr("#REF!");
        if (node.cellCount > MAX_EXPANDED_CELL_COUNT) return valErr("#REF!");
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
      if (node.kind === "Unary") {
        const inner = ensureScalar(evalAst(node.expr, astDepth + 1));
        if (inner.kind === "error") return inner;
        const n = coerceScalarNumber(inner);
        if (n === null) return valErr("#VALUE!");
        if (node.op === "-") return valNum(-n);
        if (node.op === "+") return valNum(n);
        return valNum(n / 100);
      }
      if (node.kind === "FuncCall") {
        const fnUpper = node.name;
        const args = node.args;
        if (fnUpper === "INDIRECT") {
          return evalIndirectCall(args, astDepth + 1);
        }
        if (fnUpper === "IF") {
          if (args.length < 2 || args.length > 3) return valErr("#VALUE!");
          const cond = ensureScalar(evalAst(args[0], astDepth + 1));
          if (cond.kind === "error") return cond;
          const flag = coerceScalarBool(cond);
          if (flag === null) return valErr("#VALUE!");
          if (flag) return ensureScalar(evalAst(args[1], astDepth + 1));
          if (args.length === 3) return ensureScalar(evalAst(args[2], astDepth + 1));
          return valBool(false);
        }
        if (fnUpper === "IFERROR") {
          if (args.length !== 2) return valErr("#VALUE!");
          const primary = ensureScalar(evalAst(args[0], astDepth + 1));
          if (primary.kind === "error") return ensureScalar(evalAst(args[1], astDepth + 1));
          return primary;
        }
        if (fnUpper === "ROW" || fnUpper === "COLUMN") {
          if (args.length === 0) {
            if (!currentSheetUpper) return valErr("#REF!");
            const parts = parseCellRefParts(currentCellRef || "A1");
            return valNum(fnUpper === "ROW" ? parts.row : parts.col + 1);
          }
          if (args.length !== 1) return valErr("#VALUE!");
          const target = args[0];
          if (target.kind === "Error") return valErr(target.code);
          if (target.kind === "Cell") {
            if (!validateRefTargetSheet(target, workbook, currentSheetUpper)) return valErr("#REF!");
            return valNum(fnUpper === "ROW" ? target.row : target.col + 1);
          }
          if (target.kind === "Range") {
            if (!validateRefTargetSheet(target, workbook, currentSheetUpper)) return valErr("#REF!");
            return valNum(fnUpper === "ROW" ? target.minRow : target.minCol + 1);
          }
          if (target.kind === "FuncCall" && target.name === "INDIRECT") {
            const ind = evalIndirectCall(target.args, astDepth + 1);
            if (ind.kind === "error") return ind;
            return valNum(fnUpper === "ROW" ? ind.minRow : ind.minCol + 1);
          }
          return valErr("#VALUE!");
        }
        if (fnUpper === "ROWS" || fnUpper === "COLUMNS") {
          if (args.length !== 1) return valErr("#VALUE!");
          const target = args[0];
          if (target.kind === "Error") return valErr(target.code);
          if (target.kind === "Cell") {
            if (!validateRefTargetSheet(target, workbook, currentSheetUpper)) return valErr("#REF!");
            return valNum(1);
          }
          if (target.kind === "Range") {
            if (!validateRefTargetSheet(target, workbook, currentSheetUpper)) return valErr("#REF!");
            const span =
              fnUpper === "ROWS"
                ? target.maxRow - target.minRow + 1
                : target.maxCol - target.minCol + 1;
            return valNum(span);
          }
          if (target.kind === "FuncCall" && target.name === "INDIRECT") {
            const ind = evalIndirectCall(target.args, astDepth + 1);
            if (ind.kind === "error") return ind;
            const span =
              fnUpper === "ROWS"
                ? ind.maxRow - ind.minRow + 1
                : ind.maxCol - ind.minCol + 1;
            return valNum(span);
          }
          const scalarTarget = ensureScalar(evalAst(target, astDepth + 1));
          if (scalarTarget.kind === "error") return scalarTarget;
          return valNum(1);
        }
        const evaluatedArgs = [];
        for (let i = 0; i < args.length; i += 1) {
          evaluatedArgs.push(evalAst(args[i], astDepth + 1));
        }
        if (AGGREGATE_FN_SET.has(fnUpper)) {
          return evalAggregateFunction(fnUpper, evaluatedArgs, workbook, depth);
        }
        if ((fnUpper === "AND" || fnUpper === "OR" || fnUpper === "XOR") && evaluatedArgs.length >= 1) {
          return evalLogicalFunction(fnUpper, evaluatedArgs, workbook, depth);
        }
        const scalarArgs = [];
        for (let i = 0; i < evaluatedArgs.length; i += 1) {
          const sv = ensureScalar(evaluatedArgs[i]);
          if (sv.kind === "error") return sv;
          scalarArgs.push(sv);
        }
        if (fnUpper === "ABS" && scalarArgs.length === 1) {
          const n = coerceScalarNumber(scalarArgs[0]);
          if (n === null) return valErr("#VALUE!");
          return valNum(Math.abs(n));
        }
        if (fnUpper === "ROUND" && (scalarArgs.length === 1 || scalarArgs.length === 2)) {
          const n = coerceScalarNumber(scalarArgs[0]);
          const digits = scalarArgs.length === 2 ? coerceScalarNumber(scalarArgs[1]) : 0;
          if (n === null || digits === null) return valErr("#VALUE!");
          const factor = 10 ** Math.trunc(digits);
          return valNum(Math.round(n * factor) / factor);
        }
        if (fnUpper === "LEN" && scalarArgs.length === 1) {
          return valNum(Array.from(formatFormulaValue(scalarArgs[0])).length);
        }
        if (fnUpper === "NOT" && scalarArgs.length === 1) {
          const b = coerceScalarBool(scalarArgs[0]);
          if (b === null) return valErr("#VALUE!");
          return valBool(!b);
        }
        return valErr("#NAME?");
      }
      return valErr("#VALUE!");
    }

    return ensureScalar(evalAst(parsed.root, 0));
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
      throw new Error("InvalidArgument: JSON 'sheets' 필드는 문자열 배열이어야 합니다.");
    }
    for (let i = 0; i < sheetsArr.length; i += 1) {
      const s = sheetsArr[i];
      if (typeof s !== "string") {
        throw new Error("InvalidArgument: JSON 'sheets' 항목은 문자열이어야 합니다.");
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

  function buildLocalWorkbookFromJson(doc, defaultSheet, seedDefaults) {
    const knownSheets = [];
    const hasExplicitSheets = doc.sheets !== undefined;
    if (hasExplicitSheets) {
      validateExplicitSheetsArrayJs(doc.sheets);
      for (let i = 0; i < doc.sheets.length; i += 1) {
        ensureKnownSheetUpper(knownSheets, doc.sheets[i]);
      }
    } else {
      ensureKnownSheetUpper(knownSheets, "Sheet1");
      ensureKnownSheetUpper(knownSheets, "Sheet2");
      ensureKnownSheetUpper(knownSheets, defaultSheet);
    }
    if (doc.cells !== undefined && !Array.isArray(doc.cells)) {
      throw new Error("InvalidArgument: JSON 'cells' 필드는 배열이어야 합니다.");
    }
    const cells = doc.cells || [];
    const rawValues = {};
    const formulaEntries = {};
    let seenSheet1 = false;
    let seenSheet2 = false;
    const triples = [];

    for (let i = 0; i < cells.length; i += 1) {
      const item = cells[i];
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
      if (isSameSheetIdentityJs(shName, "Sheet1")) seenSheet1 = true;
      if (isSameSheetIdentityJs(shName, "Sheet2")) seenSheet2 = true;
      const shUpper = hasExplicitSheets
        ? resolveCanonicalSheetUpper(knownSheets, shName)
        : ensureKnownSheetUpper(knownSheets, shName);
      if (!shUpper) continue;
      triples.push({ shUpper, addrUpper: addrNorm, strVal });
    }

    if (seedDefaults) {
      const s1Key = resolveCanonicalSheetUpper(knownSheets, "Sheet1");
      if (!seenSheet1 && s1Key) {
        rawValues[s1Key + "!A1"] = "1200";
        rawValues[s1Key + "!B1"] = "850";
      }
      const s2Key = resolveCanonicalSheetUpper(knownSheets, "Sheet2");
      if (!seenSheet2 && s2Key) {
        rawValues[s2Key + "!A1"] = "100";
      }
    }

    for (let i = 0; i < triples.length; i += 1) {
      const { shUpper, addrUpper, strVal } = triples[i];
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
