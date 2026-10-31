(function (host) {
  "use strict";

  const MAX_FORMULA_LEN = 8192;
  const MAX_TOKENS = 2048;
  const MAX_PARSE_DEPTH = 64;
  const MAX_ROWS = 1000000;
  const MAX_COLS = 16384;
  const MAX_EXPANDED_CELL_COUNT = 100000;

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

  function asciiUpperJs(str) {
    return String(str ?? "").replaceAll(/[a-z]/g, (c) =>
      String.fromCodePoint(c.codePointAt(0) - 32)
    );
  }

  function jsonSheetNameOrDefaultJs(raw, fallback = "Sheet1") {
    const str = String(raw ?? "");
    if (str.trim().length === 0) return fallback;
    return str;
  }

  function canonicalSheetKeyJs(rawToken) {
    const str = String(rawToken ?? "");
    if (str.trim().length === 0) return "";
    return asciiUpperJs(str);
  }

  function isSameSheetIdentityJs(a, b) {
    const ka = canonicalSheetKeyJs(a);
    const kb = canonicalSheetKeyJs(b);
    return ka.length > 0 && ka === kb;
  }

  function unquoteFormulaSheetTokenOnce(rawQuoted) {
    const s = String(rawQuoted ?? "");
    if (s.length < 2 || !s.startsWith("'") || !s.endsWith("'")) return null;
    const inner = s.slice(1, -1);
    let out = "";
    for (let i = 0; i < inner.length; i += 1) {
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

  function quoteFormulaSheetName(sheetName) {
    const raw = String(sheetName ?? "");
    if (!raw) return "''";
    const hasEdgeSpace = raw !== raw.trim();
    const hasNonAlphaNum = !/^[A-Za-z0-9_]+$/.test(raw);
    const startsWithDigit = /^[0-9]/.test(raw);
    const looksLikeCell = /^[A-Za-z]{1,3}[0-9]+$/.test(raw);
    const up = asciiUpperJs(raw);
    const isReserved = up === "TRUE" || up === "FALSE";
    if (hasEdgeSpace || hasNonAlphaNum || startsWithDigit || looksLikeCell || isReserved) {
      return "'" + raw.replaceAll("'", "''") + "'";
    }
    return raw;
  }

  function parseStrictA1Coord(rawToken) {
    const str = String(rawToken ?? "").trim();
    const candidateMatch = /^\$?([A-Za-z]{1,3})\$?([0-9]+)$/.exec(str);
    if (!candidateMatch) {
      return { ok: false, errorCode: "#NAME?" };
    }
    const letters = asciiUpperJs(candidateMatch[1]);
    const digits = candidateMatch[2];
    if (digits[0] === "0") {
      return { ok: false, errorCode: "#REF!" };
    }
    let col1 = 0;
    for (let i = 0; i < letters.length; i += 1) {
      col1 = col1 * 26 + (letters.codePointAt(i) - 64);
    }
    const row1 = Number(digits);
    if (!Number.isSafeInteger(row1) || row1 < 1 || row1 > MAX_ROWS || col1 < 1 || col1 > MAX_COLS) {
      return { ok: false, errorCode: "#REF!" };
    }
    return {
      ok: true,
      col: col1 - 1,
      row: row1,
      normRef: letters + String(row1),
    };
  }

  function normalizeA1Token(rawToken) {
    const parsed = parseStrictA1Coord(rawToken);
    if (!parsed.ok) return null;
    return parsed.normRef;
  }

  function parseCellRefParts(ref) {
    const parsed = parseStrictA1Coord(String(ref ?? ""));
    if (!parsed.ok) return { col: 0, row: 1 };
    return { col: parsed.col, row: parsed.row };
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

  function stripFormulaNamespaceJs(rawSrc) {
    const trimmed = String(rawSrc ?? "").trim();
    const up = asciiUpperJs(trimmed.slice(0, 6));
    if (up.startsWith("OF:")) return trimmed.slice(3).trimStart();
    if (up.startsWith("OOOC:")) return trimmed.slice(5).trimStart();
    if (up.startsWith("MSOXL:")) return trimmed.slice(6).trimStart();
    return trimmed;
  }

  function stripFormulaBodyJs(rawSrc) {
    const afterNs = stripFormulaNamespaceJs(rawSrc);
    if (!afterNs.startsWith("=")) return null;
    return afterNs.slice(1);
  }

  function isFormulaStringJs(rawSrc) {
    return stripFormulaBodyJs(rawSrc) !== null;
  }

  function splitOnceUnquotedJs(str, delimiter) {
    let inSingleQuote = false;
    for (let i = 0; i < str.length; i += 1) {
      const ch = str[i];
      if (ch === "'") {
        inSingleQuote = !inSingleQuote;
      } else if (ch === delimiter && !inSingleQuote) {
        return [str.slice(0, i), str.slice(i + 1)];
      }
    }
    return null;
  }

  function rfindUnquotedDotJs(str) {
    let inSingleQuote = false;
    let lastDot = -1;
    for (let i = 0; i < str.length; i += 1) {
      const ch = str[i];
      if (ch === "'") {
        inSingleQuote = !inSingleQuote;
      } else if (ch === "." && !inSingleQuote) {
        lastDot = i;
      }
    }
    return lastDot;
  }

  function formatOpenFormulaEndpointJs(part) {
    const trimmed = part.trim();
    if (trimmed.startsWith(".")) {
      return trimmed.slice(1).trim();
    }
    const dotIdx = rfindUnquotedDotJs(trimmed);
    if (dotIdx >= 0) {
      const rawSheet = trimmed.slice(0, dotIdx).trim().replace(/^\$+/, "");
      const cellPart = trimmed.slice(dotIdx + 1).trim();
      if (!rawSheet) return cellPart;
      if (rawSheet.startsWith("'") && rawSheet.endsWith("'")) {
        return rawSheet + "!" + cellPart;
      }
      return quoteFormulaSheetName(rawSheet) + "!" + cellPart;
    }
    return trimmed;
  }

  function normalizeBracketedOpenFormulaRefJs(inner) {
    const split = splitOnceUnquotedJs(inner, ":");
    if (split) {
      return formatOpenFormulaEndpointJs(split[0]) + ":" + formatOpenFormulaEndpointJs(split[1]);
    }
    return formatOpenFormulaEndpointJs(inner);
  }

  function normalizeOpenFormulaReferencesJs(body) {
    if (!body.includes("[")) return body;
    let out = "";
    let inDoubleQuote = false;
    let inSingleQuote = false;
    let i = 0;
    while (i < body.length) {
      const ch = body[i];
      if (ch === '"' && !inSingleQuote) {
        inDoubleQuote = !inDoubleQuote;
        out += ch;
        i += 1;
        continue;
      }
      if (ch === "'" && !inDoubleQuote) {
        if (inSingleQuote && i + 1 < body.length && body[i + 1] === "'") {
          out += "''";
          i += 2;
        } else {
          inSingleQuote = !inSingleQuote;
          out += ch;
          i += 1;
        }
        continue;
      }
      if (!inDoubleQuote && !inSingleQuote && ch === "[") {
        const startInner = i + 1;
        let closeIdx = -1;
        let bracketQuote = false;
        let j = startInner;
        while (j < body.length) {
          if (body[j] === "'") {
            bracketQuote = !bracketQuote;
          } else if (body[j] === "]" && !bracketQuote) {
            closeIdx = j;
            break;
          }
          j += 1;
        }
        if (closeIdx >= 0) {
          out += normalizeBracketedOpenFormulaRefJs(body.slice(startInner, closeIdx));
          i = closeIdx + 1;
          continue;
        }
        out += body.slice(i);
        break;
      }
      out += ch;
      i += 1;
    }
    return out;
  }

  function tokenizeFormulaJs(rawBody) {
    if (rawBody.length > MAX_FORMULA_LEN) return { error: "#VALUE!" };
    const src = normalizeOpenFormulaReferencesJs(rawBody);
    if (src.length > MAX_FORMULA_LEN) return { error: "#VALUE!" };
    const tokens = [];
    let i = 0;

    function pushTok(tok) {
      if (tokens.length >= MAX_TOKENS) return false;
      tokens.push(tok);
      return true;
    }

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
        if (!pushTok({ type: "str", value: buf })) return { error: "#VALUE!" };
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
        const unquoted = unquoteFormulaSheetTokenOnce(src.slice(start, i));
        if (unquoted === null) return { error: "#VALUE!" };
        i += 1;
        if (!pushTok({ type: "sheet", name: unquoted })) return { error: "#VALUE!" };
        continue;
      }
      if (ch === "#") {
        const restUpper = asciiUpperJs(src.slice(i));
        const errHit = FORMULA_ERROR_LIST.find((code) => restUpper.startsWith(code));
        if (!errHit) return { error: "#VALUE!" };
        if (!pushTok({ type: "err", code: errHit })) return { error: "#VALUE!" };
        i += errHit.length;
        continue;
      }
      if ((ch >= "0" && ch <= "9") || ch === ".") {
        const numMatch = /^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?/.exec(src.slice(i));
        if (!numMatch) return { error: "#VALUE!" };
        const parsedNum = Number(numMatch[0]);
        if (!Number.isFinite(parsedNum)) return { error: "#NUM!" };
        if (/^(?:\.|[\p{Alphabetic}_$])/u.test(src.slice(i + numMatch[0].length))) {
          return { error: "#VALUE!" };
        }
        if (!pushTok({ type: "num", value: parsedNum })) return { error: "#VALUE!" };
        i += numMatch[0].length;
        continue;
      }
      const twoChar = src.slice(i, i + 2);
      if (twoChar === "<=" || twoChar === ">=" || twoChar === "<>") {
        if (!pushTok({ type: "op", value: twoChar })) return { error: "#VALUE!" };
        i += 2;
        continue;
      }
      if (ch === ";") {
        if (!pushTok({ type: "punct", value: "," })) return { error: "#VALUE!" };
        i += 1;
        continue;
      }
      if ("+-*/^&=<>%(),:".includes(ch)) {
        if (!pushTok({ type: "punct", value: ch })) return { error: "#VALUE!" };
        i += 1;
        continue;
      }
      let end = i;
      while (end < src.length && !/[\s!+\-*/^&=<>%(),;:'"#]/.test(src[end])) {
        end += 1;
      }
      if (end === i) return { error: "#VALUE!" };
      const word = src.slice(i, end);
      if (end < src.length && src[end] === "!") {
        if (!/^[\p{Alphabetic}_$][\p{Alphabetic}\p{N}_$]*$/u.test(word)) return { error: "#VALUE!" };
        if (!pushTok({ type: "sheet", name: word })) return { error: "#VALUE!" };
        i = end + 1;
      } else {
        if (!pushTok({ type: "ident", value: word })) return { error: "#VALUE!" };
        i = end;
      }
    }
    return { tokens };
  }

  function parseFormulaAstJs(exprStr) {
    const raw = String(exprStr ?? "");
    if (raw.length > MAX_FORMULA_LEN) return { kind: "FormulaError", code: "#VALUE!" };
    const body = stripFormulaBodyJs(raw);
    if (body === null) return { kind: "Plain", raw: raw.trim() };
    const trimmedBody = body.trim();
    if (!trimmedBody) return { kind: "Ast", root: { kind: "Empty" } };
    const lexed = tokenizeFormulaJs(trimmedBody);
    if (lexed.error) return { kind: "FormulaError", code: lexed.error };
    const tokens = lexed.tokens;
    if (tokens.length === 0) return { kind: "Ast", root: { kind: "Empty" } };

    let pos = 0;
    let fatalError = null;

    function peek() {
      return tokens[pos] || null;
    }

    function next() {
      const tok = tokens[pos] || null;
      pos += 1;
      return tok;
    }

    function fail(code) {
      if (!fatalError) fatalError = code;
      return null;
    }

    function checkDepth(depth) {
      if (depth > MAX_PARSE_DEPTH) {
        fail("#VALUE!");
        return false;
      }
      return true;
    }

    function parseRefOrRangeAst(explicitSheet, firstWord) {
      const startCoord = parseStrictA1Coord(firstWord);
      if (peek()?.type === "punct" && peek().value === ":") {
        next();
        let endSheet = null;
        if (peek()?.type === "sheet") {
          if (explicitSheet === null) return fail("#REF!");
          endSheet = next().name;
        }
        const endTok = next();
        if (!endTok || endTok.type !== "ident") return fail("#REF!");
        const endCoord = parseStrictA1Coord(endTok.value);
        if (!startCoord.ok || !endCoord.ok) {
          return { kind: "Error", code: "#REF!" };
        }
        const minCol = Math.min(startCoord.col, endCoord.col);
        const maxCol = Math.max(startCoord.col, endCoord.col);
        const minRow = Math.min(startCoord.row, endCoord.row);
        const maxRow = Math.max(startCoord.row, endCoord.row);
        const cellCount = (maxRow - minRow + 1) * (maxCol - minCol + 1);
        if (cellCount > MAX_EXPANDED_CELL_COUNT) {
          return { kind: "Error", code: "#REF!" };
        }
        return {
          kind: "Range",
          sheet: explicitSheet,
          endSheet,
          startRef: startCoord.normRef,
          endRef: endCoord.normRef,
          minCol,
          maxCol,
          minRow,
          maxRow,
          cellCount,
        };
      }
      if (!startCoord.ok) {
        const code = explicitSheet !== null ? "#REF!" : startCoord.errorCode;
        return { kind: "Error", code };
      }
      return {
        kind: "Cell",
        sheet: explicitSheet,
        ref: startCoord.normRef,
        col: startCoord.col,
        row: startCoord.row,
      };
    }

    function parseComparison(depth) {
      if (!checkDepth(depth)) return null;
      let lhs = parseConcat(depth);
      if (!lhs) return null;
      while (true) {
        const t = peek();
        const isCmpPunct =
          t && t.type === "punct" && (t.value === "=" || t.value === "<" || t.value === ">");
        const isCmpOp = t && t.type === "op";
        if (!isCmpPunct && !isCmpOp) break;
        const op = next().value;
        const rhs = parseConcat(depth);
        if (!rhs) return null;
        lhs = { kind: "Compare", op, lhs, rhs };
      }
      return lhs;
    }

    function parseConcat(depth) {
      let lhs = parseAdditive(depth);
      if (!lhs) return null;
      while (peek()?.type === "punct" && peek().value === "&") {
        next();
        const rhs = parseAdditive(depth);
        if (!rhs) return null;
        lhs = { kind: "Concat", lhs, rhs };
      }
      return lhs;
    }

    function parseAdditive(depth) {
      let lhs = parseMultiplicative(depth);
      if (!lhs) return null;
      while (peek()?.type === "punct" && (peek().value === "+" || peek().value === "-")) {
        const op = next().value;
        const rhs = parseMultiplicative(depth);
        if (!rhs) return null;
        lhs = { kind: "Binary", op, lhs, rhs };
      }
      return lhs;
    }

    function parseMultiplicative(depth) {
      let lhs = parsePower(depth);
      if (!lhs) return null;
      while (peek()?.type === "punct" && (peek().value === "*" || peek().value === "/")) {
        const op = next().value;
        const rhs = parsePower(depth);
        if (!rhs) return null;
        lhs = { kind: "Binary", op, lhs, rhs };
      }
      return lhs;
    }

    function parsePower(depth) {
      let lhs = parseUnary(depth);
      if (!lhs) return null;
      while (peek()?.type === "punct" && peek().value === "^") {
        next();
        const rhs = parseUnary(depth);
        if (!rhs) return null;
        lhs = { kind: "Binary", op: "^", lhs, rhs };
      }
      return lhs;
    }

    function parseUnary(depth) {
      if (!checkDepth(depth)) return null;
      const t = peek();
      if (t?.type === "punct" && (t.value === "+" || t.value === "-")) {
        next();
        const expr = parseUnary(depth + 1);
        if (!expr) return null;
        return { kind: "Unary", op: t.value, expr };
      }
      let node = parsePrimary(depth);
      if (!node) return null;
      let pctDepth = depth;
      while (peek()?.type === "punct" && peek().value === "%") {
        pctDepth += 1;
        if (!checkDepth(pctDepth)) return null;
        next();
        node = { kind: "Unary", op: "%", expr: node };
      }
      return node;
    }

    function parsePrimary(depth) {
      if (!checkDepth(depth)) return null;
      const t = next();
      if (!t) return fail("#VALUE!");
      if (t.type === "num") return { kind: "Number", value: t.value };
      if (t.type === "str") return { kind: "Str", value: t.value };
      if (t.type === "err") return { kind: "Error", code: t.code };
      if (t.type === "punct" && t.value === "(") {
        const inner = parseComparison(depth + 1);
        if (!inner) return null;
        if (!(peek()?.type === "punct" && peek().value === ")")) return fail("#VALUE!");
        next();
        return inner;
      }
      if (t.type === "sheet") {
        const refTok = next();
        if (!refTok || refTok.type !== "ident") return fail("#REF!");
        return parseRefOrRangeAst(t.name, refTok.value);
      }
      if (t.type === "ident") {
        const up = asciiUpperJs(t.value);
        if (peek()?.type === "punct" && peek().value === "(") {
          next();
          if (!checkDepth(depth + 1)) return null;
          const args = [];
          if (!(peek()?.type === "punct" && peek().value === ")")) {
            while (true) {
              const argAst = parseComparison(depth + 1);
              if (!argAst) return null;
              args.push(argAst);
              if (peek()?.type === "punct" && peek().value === ",") {
                next();
                continue;
              }
              break;
            }
          }
          if (!(peek()?.type === "punct" && peek().value === ")")) return fail("#VALUE!");
          next();
          return { kind: "FuncCall", name: up, args };
        }
        if (up === "TRUE") return { kind: "Bool", value: true };
        if (up === "FALSE") return { kind: "Bool", value: false };
        return parseRefOrRangeAst(null, t.value);
      }
      return fail("#VALUE!");
    }

    const root = parseComparison(0);
    if (fatalError) return { kind: "FormulaError", code: fatalError };
    if (!root || pos < tokens.length) return { kind: "FormulaError", code: "#VALUE!" };
    return { kind: "Ast", root };
  }

  host.__EASYOFFICE_FORMULA_AST__ = {
    MAX_FORMULA_LEN,
    MAX_TOKENS,
    MAX_PARSE_DEPTH,
    MAX_ROWS,
    MAX_COLS,
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
    stripFormulaBodyJs,
    isFormulaStringJs,
    tokenizeFormulaJs,
    parseFormulaAstJs,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
