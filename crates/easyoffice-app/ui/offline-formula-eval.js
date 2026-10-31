(function (host) {
  "use strict";

  function parseCellRefParts(ref) {
    const m = /^([A-Z]+)(\d+)$/.exec(String(ref || "").toUpperCase());
    if (!m) return { col: 0, row: 1 };
    let col = 0;
    for (let i = 0; i < m[1].length; i++) {
      col = col * 26 + (m[1].codePointAt(i) - 64);
    }
    return { col: col - 1, row: Number(m[2]) };
  }

  function colIndexToLetters(colIdx) {
    let n = Math.max(0, Number(colIdx) || 0) + 1;
    let out = "";
    while (n > 0) {
      const rem = (n - 1) % 26;
      out = String.fromCodePoint(65 + rem) + out;
      n = Math.floor((n - 1) / 26);
    }
    return out || "A";
  }

  function normalizeFormattedNumericCell(raw) {
    const trimmed = String(raw || "").trim();
    if (/^-?\d{1,3}(,\d{3})+(\.\d+)?$/.test(trimmed)) {
      return trimmed.replaceAll(",", "");
    }
    return trimmed;
  }

  function jsonSheetNameOrDefaultJs(raw, fallback = "Sheet1") {
    const trimmed = String(raw ?? "").trim();
    return trimmed.length > 0 ? trimmed : fallback;
  }

  function asciiUpperJs(str) {
    return String(str ?? "").replaceAll(/[a-z]/g, (c) =>
      String.fromCodePoint(c.codePointAt(0) - 32)
    );
  }

  function unquoteBalancedSingleQuotesOnceJs(s) {
    if (s.length < 2 || !s.startsWith("'") || !s.endsWith("'")) return null;
    const inner = s.slice(1, -1);
    let out = "";
    for (let i = 0; i < inner.length; i++) {
      if (inner[i] === "'") {
        if (i + 1 < inner.length && inner[i + 1] === "'") {
          out += "'";
          i += 1;
        } else {
          return null;
        }
      } else {
        out += inner[i];
      }
    }
    return out;
  }

  function canonicalSheetKeyJs(rawToken) {
    let cur = String(rawToken ?? "").trim();
    while (true) {
      const unquoted = unquoteBalancedSingleQuotesOnceJs(cur);
      if (unquoted === null) break;
      cur = unquoted.trim();
    }
    return asciiUpperJs(cur);
  }

  function isSameSheetIdentityJs(a, b) {
    const ka = canonicalSheetKeyJs(a);
    const kb = canonicalSheetKeyJs(b);
    return ka.length > 0 && ka === kb;
  }

  function resolveCanonicalSheetUpper(knownSheets, rawName) {
    const targetKey = canonicalSheetKeyJs(rawName);
    if (!targetKey) return null;
    for (const stored of knownSheets) {
      if (canonicalSheetKeyJs(stored) === targetKey) return targetKey;
    }
    return null;
  }

  function ensureKnownSheetUpper(knownSheets, name) {
    const trimmed = String(name ?? "").trim();
    const key = canonicalSheetKeyJs(trimmed);
    if (!key) return null;
    const existing = resolveCanonicalSheetUpper(knownSheets, trimmed);
    if (existing) return existing;
    knownSheets.push(trimmed);
    return key;
  }

  const FORMULA_ERROR_LIST = [
    "#DIV/0!",
    "#VALUE!",
    "#CYCLE!",
    "#ERROR!",
    "#NAME?",
    "#NULL!",
    "#REF!",
    "#NUM!",
    "#N/A",
  ];

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

  function normalizeA1Token(rawToken) {
    const cleaned = asciiUpperJs(String(rawToken ?? "").trim()).replaceAll("$", "");
    return /^[A-Z]+\d+$/.test(cleaned) ? cleaned : null;
  }

  function tokenizeFormulaJs(src) {
    const tokens = [];
    let i = 0;
    while (i < src.length) {
      const ch = src[i];
      if (/\s/.test(ch)) {
        i += 1;
        continue;
      }
      if (ch === '"') {
        i += 1;
        let buf = "";
        let closed = false;
        while (i < src.length) {
          if (src[i] === '"') {
            if (i + 1 < src.length && src[i + 1] === '"') {
              buf += '"';
              i += 2;
              continue;
            }
            i += 1;
            closed = true;
            break;
          }
          buf += src[i];
          i += 1;
        }
        if (!closed) return { error: "#VALUE!" };
        tokens.push({ type: "str", value: buf });
        continue;
      }
      if (ch === "'") {
        const start = i;
        i += 1;
        let closed = false;
        while (i < src.length) {
          if (src[i] === "'") {
            if (i + 1 < src.length && src[i + 1] === "'") {
              i += 2;
              continue;
            }
            i += 1;
            closed = true;
            break;
          }
          i += 1;
        }
        if (!closed || i >= src.length || src[i] !== "!") return { error: "#VALUE!" };
        const rawQuoted = src.slice(start, i);
        i += 1;
        tokens.push({ type: "sheet", name: rawQuoted });
        continue;
      }
      if (ch === "#") {
        const restUpper = asciiUpperJs(src.slice(i));
        const errHit = FORMULA_ERROR_LIST.find((code) => restUpper.startsWith(code));
        if (!errHit) return { error: "#VALUE!" };
        tokens.push({ type: "err", code: errHit });
        i += errHit.length;
        continue;
      }
      const numMatch = /^\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/.exec(src.slice(i));
      if (numMatch) {
        const nextChar = src[i + numMatch[0].length] || "";
        if (nextChar !== "!") {
          tokens.push({ type: "num", value: Number(numMatch[0]) });
          i += numMatch[0].length;
          continue;
        }
      }
      const twoChar = src.slice(i, i + 2);
      if (twoChar === "<=" || twoChar === ">=" || twoChar === "<>") {
        tokens.push({ type: "op", value: twoChar });
        i += 2;
        continue;
      }
      if ("+-*/^&=<>%(),:".includes(ch)) {
        tokens.push({ type: "punct", value: ch });
        i += 1;
        continue;
      }
      let end = i;
      while (end < src.length && !/[\s!+\-*/^&=<>%(),:'"#]/.test(src[end])) {
        end += 1;
      }
      if (end === i) return { error: "#VALUE!" };
      const word = src.slice(i, end);
      if (end < src.length && src[end] === "!") {
        tokens.push({ type: "sheet", name: word });
        i = end + 1;
      } else {
        tokens.push({ type: "ident", value: word });
        i = end;
      }
    }
    return { tokens };
  }

  function readWorkbookTypedCell(workbook, shKey, cellRef, depth = 0) {
    if (!shKey) return valErr("#REF!");
    const normRef = normalizeA1Token(cellRef);
    if (!normRef) return valErr("#REF!");
    if (depth > 64) return valErr("#REF!");
    const fullKey = shKey + "!" + normRef;
    if (workbook.memo.has(fullKey)) return workbook.memo.get(fullKey);
    const formula = workbook.formulaEntries[fullKey];
    if (formula !== undefined) {
      if (workbook.visiting.has(fullKey)) return valErr("#CYCLE!");
      workbook.visiting.add(fullKey);
      const computed = evaluateFormulaTyped(formula, workbook, shKey, normRef, depth + 1);
      workbook.visiting.delete(fullKey);
      workbook.memo.set(fullKey, computed);
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
    for (const item of argItems) {
      if (item.kind === "range") {
        if (!item.sheetUpper) return valErr("#REF!");
        const p1 = parseCellRefParts(item.startRef);
        const p2 = parseCellRefParts(item.endRef);
        const minR = Math.min(p1.row, p2.row);
        const maxR = Math.max(p1.row, p2.row);
        const minC = Math.min(p1.col, p2.col);
        const maxC = Math.max(p1.col, p2.col);
        for (let r = minR; r <= maxR; r++) {
          for (let c = minC; c <= maxC; c++) {
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
    if (fnName === "SUM") return valNum(nums.reduce((a, b) => a + b, 0));
    if (fnName === "PRODUCT") {
      return valNum(nums.length === 0 ? 0 : nums.reduce((a, b) => a * b, 1));
    }
    if (fnName === "MIN") return valNum(nums.length === 0 ? 0 : Math.min(...nums));
    if (fnName === "MAX") return valNum(nums.length === 0 ? 0 : Math.max(...nums));
    if (fnName === "AVERAGE" || fnName === "AVG") {
      if (nums.length === 0) return valErr("#DIV/0!");
      return valNum(nums.reduce((a, b) => a + b, 0) / nums.length);
    }
    if (nums.length < 2) return valErr("#DIV/0!");
    const mean = nums.reduce((a, b) => a + b, 0) / nums.length;
    const m2 = nums.reduce((acc, v) => acc + (v - mean) * (v - mean), 0);
    const variance = m2 / (nums.length - 1);
    return valNum(fnName === "STDEV" ? Math.sqrt(variance) : variance);
  }

  function evaluateFormulaTyped(exprStr, workbook, currentSheetUpper, currentCellRef, depth = 0) {
    if (depth > 64) return valErr("#REF!");
    const clean = String(exprStr ?? "").trim();
    if (!clean.startsWith("=")) return parsePlainCellTyped(clean);
    const body = clean.slice(1).trim();
    if (!body) return valEmpty();
    const lexed = tokenizeFormulaJs(body);
    if (lexed.error) return valErr(lexed.error);
    const tokens = lexed.tokens;
    if (tokens.length === 0) return valEmpty();
    let pos = 0;

    const peek = () => tokens[pos] || null;
    const next = () => {
      const t = tokens[pos] || null;
      pos += 1;
      return t;
    };
    const ensureScalar = (v) => (v && v.kind === "range" ? valErr("#VALUE!") : v);

    function parseComparison() {
      let lhs = parseConcat();
      while (true) {
        const t = peek();
        const isCmpPunct =
          t && t.type === "punct" && (t.value === "=" || t.value === "<" || t.value === ">");
        const isCmpOp = t && t.type === "op";
        if (!isCmpPunct && !isCmpOp) break;
        lhs = ensureScalar(lhs);
        const op = next().value;
        const rhs = ensureScalar(parseConcat());
        if (lhs.kind === "error") return lhs;
        if (rhs.kind === "error") return rhs;
        const cmp = compareTypedValues(lhs, rhs);
        if (op === "=") lhs = valBool(cmp === 0);
        else if (op === "<>") lhs = valBool(cmp !== 0);
        else if (op === "<") lhs = valBool(cmp < 0);
        else if (op === "<=") lhs = valBool(cmp <= 0);
        else if (op === ">") lhs = valBool(cmp > 0);
        else lhs = valBool(cmp >= 0);
      }
      return lhs;
    }

    function parseConcat() {
      let lhs = parseAdditive();
      while (peek()?.type === "punct" && peek().value === "&") {
        lhs = ensureScalar(lhs);
        next();
        const rhs = ensureScalar(parseAdditive());
        if (lhs.kind === "error") return lhs;
        if (rhs.kind === "error") return rhs;
        lhs = valText(formatFormulaValue(lhs) + formatFormulaValue(rhs));
      }
      return lhs;
    }

    function parseAdditive() {
      let lhs = parseMultiplicative();
      while (peek()?.type === "punct" && (peek().value === "+" || peek().value === "-")) {
        lhs = ensureScalar(lhs);
        const op = next().value;
        const rhs = ensureScalar(parseMultiplicative());
        if (lhs.kind === "error") return lhs;
        if (rhs.kind === "error") return rhs;
        const a = coerceScalarNumber(lhs);
        const b = coerceScalarNumber(rhs);
        if (a === null || b === null) return valErr("#VALUE!");
        lhs = valNum(op === "+" ? a + b : a - b);
      }
      return lhs;
    }

    function parseMultiplicative() {
      let lhs = parsePower();
      while (peek()?.type === "punct" && (peek().value === "*" || peek().value === "/")) {
        lhs = ensureScalar(lhs);
        const op = next().value;
        const rhs = ensureScalar(parsePower());
        if (lhs.kind === "error") return lhs;
        if (rhs.kind === "error") return rhs;
        const a = coerceScalarNumber(lhs);
        const b = coerceScalarNumber(rhs);
        if (a === null || b === null) return valErr("#VALUE!");
        if (op === "/" && b === 0) return valErr("#DIV/0!");
        lhs = valNum(op === "*" ? a * b : a / b);
      }
      return lhs;
    }

    function parsePower() {
      let lhs = parseUnary();
      while (peek()?.type === "punct" && peek().value === "^") {
        lhs = ensureScalar(lhs);
        next();
        const rhs = ensureScalar(parseUnary());
        if (lhs.kind === "error") return lhs;
        if (rhs.kind === "error") return rhs;
        const a = coerceScalarNumber(lhs);
        const b = coerceScalarNumber(rhs);
        if (a === null || b === null) return valErr("#VALUE!");
        lhs = valNum(a ** b);
      }
      return lhs;
    }

    function parseUnary() {
      const t = peek();
      if (t?.type === "punct" && (t.value === "+" || t.value === "-")) {
        next();
        const inner = ensureScalar(parseUnary());
        if (inner.kind === "error") return inner;
        const n = coerceScalarNumber(inner);
        if (n === null) return valErr("#VALUE!");
        return valNum(t.value === "-" ? -n : n);
      }
      let val = parsePrimary();
      while (peek()?.type === "punct" && peek().value === "%") {
        next();
        val = ensureScalar(val);
        if (val.kind === "error") return val;
        const n = coerceScalarNumber(val);
        if (n === null) return valErr("#VALUE!");
        val = valNum(n / 100);
      }
      return val;
    }

    function parseCellOrRange(explicitSheetToken, firstRefToken) {
      const startRef = normalizeA1Token(firstRefToken);
      if (!startRef) return valErr("#VALUE!");
      const resolvedSheet =
        explicitSheetToken === null
          ? currentSheetUpper
          : resolveCanonicalSheetUpper(workbook.knownSheets, explicitSheetToken);
      if (peek()?.type === "punct" && peek().value === ":") {
        next();
        let endSheetToken = null;
        if (peek()?.type === "sheet") {
          endSheetToken = next().name;
        }
        const endTok = next();
        if (!endTok || endTok.type !== "ident") return valErr("#VALUE!");
        const endRef = normalizeA1Token(endTok.value);
        if (!endRef) return valErr("#VALUE!");
        if (endSheetToken !== null) {
          const lhsIdentity = explicitSheetToken === null ? currentSheetUpper : explicitSheetToken;
          if (!isSameSheetIdentityJs(lhsIdentity, endSheetToken)) {
            return valErr("#REF!");
          }
        }
        return { kind: "range", sheetUpper: resolvedSheet, startRef, endRef };
      }
      return readWorkbookTypedCell(workbook, resolvedSheet, startRef, depth);
    }

    function parseFunctionCall(fnUpper) {
      next();
      if (fnUpper === "ROW" || fnUpper === "COLUMN") {
        let targetRef = currentCellRef || "A1";
        if (!(peek()?.type === "punct" && peek().value === ")")) {
          if (peek()?.type === "sheet") next();
          const refTok = next();
          if (!refTok || refTok.type !== "ident") return valErr("#VALUE!");
          const norm = normalizeA1Token(refTok.value);
          if (!norm) return valErr("#VALUE!");
          targetRef = norm;
        }
        if (!(peek()?.type === "punct" && peek().value === ")")) return valErr("#VALUE!");
        next();
        const parts = parseCellRefParts(targetRef);
        return valNum(fnUpper === "ROW" ? parts.row : parts.col + 1);
      }
      if (fnUpper === "IF") {
        const cond = ensureScalar(parseComparison());
        if (!(peek()?.type === "punct" && peek().value === ",")) return valErr("#VALUE!");
        next();
        const whenTrue = ensureScalar(parseComparison());
        let whenFalse = valBool(false);
        if (peek()?.type === "punct" && peek().value === ",") {
          next();
          whenFalse = ensureScalar(parseComparison());
        }
        if (!(peek()?.type === "punct" && peek().value === ")")) return valErr("#VALUE!");
        next();
        if (cond.kind === "error") return cond;
        const flag = coerceScalarBool(cond);
        if (flag === null) return valErr("#VALUE!");
        return flag ? whenTrue : whenFalse;
      }
      if (fnUpper === "IFERROR") {
        const primary = ensureScalar(parseComparison());
        if (!(peek()?.type === "punct" && peek().value === ",")) return valErr("#VALUE!");
        next();
        const fallback = ensureScalar(parseComparison());
        if (!(peek()?.type === "punct" && peek().value === ")")) return valErr("#VALUE!");
        next();
        return primary.kind === "error" ? fallback : primary;
      }
      const args = [];
      if (!(peek()?.type === "punct" && peek().value === ")")) {
        while (true) {
          args.push(parseComparison());
          if (peek()?.type === "punct" && peek().value === ",") {
            next();
            continue;
          }
          break;
        }
      }
      if (!(peek()?.type === "punct" && peek().value === ")")) return valErr("#VALUE!");
      next();
      const aggSet = new Set([
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
      if (aggSet.has(fnUpper)) {
        return evalAggregateFunction(fnUpper, args, workbook, depth);
      }
      const scalarArgs = args.map(ensureScalar);
      const firstErr = scalarArgs.find((a) => a.kind === "error");
      if (firstErr) return firstErr;
      if (fnUpper === "ABS" && scalarArgs.length === 1) {
        const n = coerceScalarNumber(scalarArgs[0]);
        return n === null ? valErr("#VALUE!") : valNum(Math.abs(n));
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
      if ((fnUpper === "AND" || fnUpper === "OR") && scalarArgs.length >= 1) {
        const bools = [];
        for (const a of scalarArgs) {
          const b = coerceScalarBool(a);
          if (b === null) return valErr("#VALUE!");
          bools.push(b);
        }
        return valBool(fnUpper === "AND" ? bools.every(Boolean) : bools.some(Boolean));
      }
      if (fnUpper === "NOT" && scalarArgs.length === 1) {
        const b = coerceScalarBool(scalarArgs[0]);
        return b === null ? valErr("#VALUE!") : valBool(!b);
      }
      return valErr("#NAME?");
    }

    function parsePrimary() {
      const t = next();
      if (!t) return valErr("#VALUE!");
      if (t.type === "num") return valNum(t.value);
      if (t.type === "str") return valText(t.value);
      if (t.type === "err") return valErr(t.code);
      if (t.type === "punct" && t.value === "(") {
        const inner = ensureScalar(parseComparison());
        if (!(peek()?.type === "punct" && peek().value === ")")) return valErr("#VALUE!");
        next();
        return inner;
      }
      if (t.type === "sheet") {
        const refTok = next();
        if (!refTok || refTok.type !== "ident") return valErr("#VALUE!");
        return parseCellOrRange(t.name, refTok.value);
      }
      if (t.type === "ident") {
        const up = asciiUpperJs(t.value);
        if (peek()?.type === "punct" && peek().value === "(") {
          return parseFunctionCall(up);
        }
        if (up === "TRUE") return valBool(true);
        if (up === "FALSE") return valBool(false);
        if (normalizeA1Token(up)) {
          return parseCellOrRange(null, up);
        }
        return valErr("#NAME?");
      }
      return valErr("#VALUE!");
    }

    const out = ensureScalar(parseComparison());
    if (pos < tokens.length) {
      return out.kind === "error" ? out : valErr("#VALUE!");
    }
    return out;
  }

  function evaluateFormulaExpression(exprStr, workbook, currentSheetUpper, currentCellRef) {
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

  function resolveDefaultSheetFromDocJs(doc) {
    if (doc.sheet !== undefined && doc.sheet !== null && typeof doc.sheet !== "string") {
      throw new Error("InvalidArgument: JSON 'sheet' 필드는 문자열이어야 합니다.");
    }
    const explicitTop = String(doc.sheet ?? "").trim();
    if (explicitTop) return explicitTop;
    if (doc.sheets !== undefined) {
      if (!Array.isArray(doc.sheets)) {
        throw new Error("InvalidArgument: JSON 'sheets' 필드는 배열이어야 합니다.");
      }
      for (const s of doc.sheets) {
        if (typeof s !== "string") {
          throw new Error("InvalidArgument: JSON 'sheets' 항목은 문자열이어야 합니다.");
        }
        if (canonicalSheetKeyJs(s)) return s.trim();
      }
      return "";
    }
    return "Sheet1";
  }

  function buildLocalWorkbookFromJson(doc, defaultSheet, seedDefaults) {
    const knownSheets = [];
    const hasExplicitSheets = doc.sheets !== undefined;
    if (hasExplicitSheets) {
      if (!Array.isArray(doc.sheets)) {
        throw new Error("InvalidArgument: JSON 'sheets' 필드는 배열이어야 합니다.");
      }
      for (const s of doc.sheets) {
        if (typeof s !== "string") {
          throw new Error("InvalidArgument: JSON 'sheets' 항목은 문자열이어야 합니다.");
        }
        if (s.trim()) ensureKnownSheetUpper(knownSheets, s.trim());
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

    for (const { shUpper, addrUpper, strVal } of triples) {
      const fullKey = shUpper + "!" + addrUpper;
      rawValues[fullKey] = strVal;
      if (strVal.trim().startsWith("=")) {
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
    };
  }

  host.__EASYOFFICE_OFFLINE_FORMULA_EVAL__ = {
    parseCellRefParts,
    colIndexToLetters,
    normalizeFormattedNumericCell,
    jsonSheetNameOrDefaultJs,
    resolveCanonicalSheetUpper,
    resolveDefaultSheetFromDocJs,
    buildLocalWorkbookFromJson,
    readWorkbookTypedCell,
    formatFormulaValue,
    evaluateFormulaExpression,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
