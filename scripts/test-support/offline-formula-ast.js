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
    const hasNonAlphaNum = !/^\w+$/.test(raw);
    const startsWithDigit = /^\d/.test(raw);
    const looksLikeCell = /^[A-Za-z]{1,3}\d+$/.test(raw);
    const up = asciiUpperJs(raw);
    const isReserved = up === "TRUE" || up === "FALSE";
    if (hasEdgeSpace || hasNonAlphaNum || startsWithDigit || looksLikeCell || isReserved) {
      return "'" + raw.replaceAll("'", "''") + "'";
    }
    return raw;
  }

  function parseStrictA1Coord(rawToken) {
    const str = String(rawToken ?? "").trim();
    const candidateMatch = /^\$?([A-Za-z]{1,3})\$?(\d+)$/.exec(str);
    if (!candidateMatch) {
      return { ok: false, errorCode: "#NAME?" };
    }
    const letters = asciiUpperJs(candidateMatch[1]);
    const digits = candidateMatch[2];
    if (digits.startsWith("0")) {
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

  /** Handles quote toggling at body[i]; returns the next index, or -1 when body[i] is not an active quote. */
  function consumeQuoteJs(body, i, st) {
    const ch = body[i];
    if (ch === '"' && !st.inSingle) {
      st.inDouble = !st.inDouble;
      st.out += ch;
      return i + 1;
    }
    if (ch !== "'" || st.inDouble) return -1;
    if (st.inSingle && body[i + 1] === "'") {
      st.out += "''";
      return i + 2;
    }
    st.inSingle = !st.inSingle;
    st.out += ch;
    return i + 1;
  }

  function findBracketCloseJs(body, startInner) {
    let bracketQuote = false;
    for (let j = startInner; j < body.length; j += 1) {
      if (body[j] === "'") bracketQuote = !bracketQuote;
      else if (body[j] === "]" && !bracketQuote) return j;
    }
    return -1;
  }

  function normalizeOpenFormulaReferencesJs(body) {
    if (!body.includes("[")) return body;
    const st = { out: "", inDouble: false, inSingle: false };
    let i = 0;
    while (i < body.length) {
      const afterQuote = consumeQuoteJs(body, i, st);
      if (afterQuote >= 0) {
        i = afterQuote;
        continue;
      }
      if (!st.inDouble && !st.inSingle && body[i] === "[") {
        const closeIdx = findBracketCloseJs(body, i + 1);
        if (closeIdx < 0) return st.out + body.slice(i);
        st.out += normalizeBracketedOpenFormulaRefJs(body.slice(i + 1, closeIdx));
        i = closeIdx + 1;
        continue;
      }
      st.out += body[i];
      i += 1;
    }
    return st.out;
  }

  const LEX_ERROR = { error: "#VALUE!" };
  const NUMBER_TOKEN_RE = /^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?/;
  const NUMBER_TRAILER_RE = /^(?:\.|[\p{Alphabetic}_$])/u;
  const SHEET_WORD_RE = /^[\p{Alphabetic}_$][\p{Alphabetic}\p{N}_$]*$/u;
  const WORD_BREAK_RE = /[\s!+\-*/^&=<>%(),;:'"#]/;
  const TWO_CHAR_OPS = new Set(["<=", ">=", "<>"]);
  const PUNCT_CHARS = "+-*/^&=<>%(),:";

  /** Reads a double-quoted string literal starting at src[i]. */
  function lexStringJs(src, i) {
    let pos = i + 1;
    let buf = "";
    while (pos < src.length) {
      if (src[pos] !== '"') {
        buf += src[pos];
        pos += 1;
      } else if (src[pos + 1] === '"') {
        buf += '"';
        pos += 2;
      } else {
        return { tok: { type: "str", value: buf }, end: pos + 1 };
      }
    }
    return LEX_ERROR;
  }

  /** Reads a quoted sheet name ('Name'!) starting at src[i]. */
  function lexQuotedSheetJs(src, i) {
    let pos = i + 1;
    let closed = false;
    while (pos < src.length && !closed) {
      if (src[pos] !== "'") pos += 1;
      else if (src[pos + 1] === "'") pos += 2;
      else {
        pos += 1;
        closed = true;
      }
    }
    if (!closed || src[pos] !== "!") return LEX_ERROR;
    const unquoted = unquoteFormulaSheetTokenOnce(src.slice(i, pos));
    if (unquoted === null) return LEX_ERROR;
    return { tok: { type: "sheet", name: unquoted }, end: pos + 1 };
  }

  function lexErrorLiteralJs(src, i) {
    const restUpper = asciiUpperJs(src.slice(i));
    const errHit = FORMULA_ERROR_LIST.find((code) => restUpper.startsWith(code));
    if (!errHit) return LEX_ERROR;
    return { tok: { type: "err", code: errHit }, end: i + errHit.length };
  }

  function lexNumberJs(src, i) {
    const numMatch = NUMBER_TOKEN_RE.exec(src.slice(i));
    if (!numMatch) return LEX_ERROR;
    const parsedNum = Number(numMatch[0]);
    if (!Number.isFinite(parsedNum)) return { error: "#NUM!" };
    const end = i + numMatch[0].length;
    if (NUMBER_TRAILER_RE.test(src.slice(end))) return LEX_ERROR;
    return { tok: { type: "num", value: parsedNum }, end };
  }

  function lexWordJs(src, i) {
    let end = i;
    while (end < src.length && !WORD_BREAK_RE.test(src[end])) end += 1;
    if (end === i) return LEX_ERROR;
    const word = src.slice(i, end);
    if (src[end] !== "!") return { tok: { type: "ident", value: word }, end };
    if (!SHEET_WORD_RE.test(word)) return LEX_ERROR;
    return { tok: { type: "sheet", name: word }, end: end + 1 };
  }

  function lexOperatorOrWordJs(src, i) {
    const ch = src[i];
    const twoChar = src.slice(i, i + 2);
    if (TWO_CHAR_OPS.has(twoChar)) return { tok: { type: "op", value: twoChar }, end: i + 2 };
    if (ch === ";") return { tok: { type: "punct", value: "," }, end: i + 1 };
    if (PUNCT_CHARS.includes(ch)) return { tok: { type: "punct", value: ch }, end: i + 1 };
    return lexWordJs(src, i);
  }

  function lexTokenAtJs(src, i) {
    const ch = src[i];
    if (ch === '"') return lexStringJs(src, i);
    if (ch === "'") return lexQuotedSheetJs(src, i);
    if (ch === "#") return lexErrorLiteralJs(src, i);
    if ((ch >= "0" && ch <= "9") || ch === ".") return lexNumberJs(src, i);
    return lexOperatorOrWordJs(src, i);
  }

  function tokenizeFormulaJs(rawBody) {
    if (rawBody.length > MAX_FORMULA_LEN) return LEX_ERROR;
    const src = normalizeOpenFormulaReferencesJs(rawBody);
    if (src.length > MAX_FORMULA_LEN) return LEX_ERROR;
    const tokens = [];
    let i = 0;
    while (i < src.length) {
      if (/\s/.test(src[i])) {
        i += 1;
        continue;
      }
      const lexed = lexTokenAtJs(src, i);
      if (lexed.error) return { error: lexed.error };
      if (tokens.length >= MAX_TOKENS) return LEX_ERROR;
      tokens.push(lexed.tok);
      i = lexed.end;
    }
    return { tokens };
  }

  function psPeek(ps) {
    return ps.tokens[ps.pos] || null;
  }

  function psNext(ps) {
    const tok = ps.tokens[ps.pos] || null;
    ps.pos += 1;
    return tok;
  }

  function psFail(ps, code) {
    if (!ps.fatal) ps.fatal = code;
    return null;
  }

  function psCheckDepth(ps, depth) {
    if (depth <= MAX_PARSE_DEPTH) return true;
    psFail(ps, "#VALUE!");
    return false;
  }

  function peekIsPunct(ps, ...values) {
    const t = psPeek(ps);
    return t?.type === "punct" && values.includes(t.value);
  }

  function buildRangeNode(explicitSheet, endSheet, startCoord, endCoord) {
    const minCol = Math.min(startCoord.col, endCoord.col);
    const maxCol = Math.max(startCoord.col, endCoord.col);
    const minRow = Math.min(startCoord.row, endCoord.row);
    const maxRow = Math.max(startCoord.row, endCoord.row);
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
      cellCount: (maxRow - minRow + 1) * (maxCol - minCol + 1),
    };
  }

  function parseRangeTail(ps, explicitSheet, startCoord) {
    psNext(ps);
    let endSheet = null;
    if (psPeek(ps)?.type === "sheet") {
      if (explicitSheet === null) return psFail(ps, "#REF!");
      endSheet = psNext(ps).name;
    }
    const endTok = psNext(ps);
    if (endTok?.type !== "ident") return psFail(ps, "#REF!");
    const endCoord = parseStrictA1Coord(endTok.value);
    if (!startCoord.ok || !endCoord.ok) return { kind: "Error", code: "#REF!" };
    return buildRangeNode(explicitSheet, endSheet, startCoord, endCoord);
  }

  function parseRefOrRangeAst(ps, explicitSheet, firstWord) {
    const startCoord = parseStrictA1Coord(firstWord);
    if (peekIsPunct(ps, ":")) return parseRangeTail(ps, explicitSheet, startCoord);
    if (!startCoord.ok) {
      return { kind: "Error", code: explicitSheet === null ? startCoord.errorCode : "#REF!" };
    }
    return {
      kind: "Cell",
      sheet: explicitSheet,
      ref: startCoord.normRef,
      col: startCoord.col,
      row: startCoord.row,
    };
  }

  function isCompareToken(t) {
    if (t?.type === "op") return true;
    return t?.type === "punct" && (t.value === "=" || t.value === "<" || t.value === ">");
  }

  function parseComparison(ps, depth) {
    if (!psCheckDepth(ps, depth)) return null;
    let lhs = parseConcat(ps, depth);
    if (!lhs) return null;
    while (isCompareToken(psPeek(ps))) {
      const op = psNext(ps).value;
      const rhs = parseConcat(ps, depth);
      if (!rhs) return null;
      lhs = { kind: "Compare", op, lhs, rhs };
    }
    return lhs;
  }

  const makeBinaryNode = (op, lhs, rhs) => ({ kind: "Binary", op, lhs, rhs });
  const makeConcatNode = (_op, lhs, rhs) => ({ kind: "Concat", lhs, rhs });

  /** Left-associative chain: operand (op operand)*, with ops limited to the given punctuation. */
  function parseBinaryChain(ps, depth, parseOperand, ops, makeNode) {
    let lhs = parseOperand(ps, depth);
    if (!lhs) return null;
    while (peekIsPunct(ps, ...ops)) {
      const op = psNext(ps).value;
      const rhs = parseOperand(ps, depth);
      if (!rhs) return null;
      lhs = makeNode(op, lhs, rhs);
    }
    return lhs;
  }

  const parseConcat = (ps, depth) => parseBinaryChain(ps, depth, parseAdditive, ["&"], makeConcatNode);
  const parseAdditive = (ps, depth) => parseBinaryChain(ps, depth, parseMultiplicative, ["+", "-"], makeBinaryNode);
  const parseMultiplicative = (ps, depth) => parseBinaryChain(ps, depth, parsePower, ["*", "/"], makeBinaryNode);
  const parsePower = (ps, depth) => parseBinaryChain(ps, depth, parseUnary, ["^"], makeBinaryNode);

  function applyPercentSuffixes(ps, node, depth) {
    let current = node;
    let pctDepth = depth;
    while (peekIsPunct(ps, "%")) {
      pctDepth += 1;
      if (!psCheckDepth(ps, pctDepth)) return null;
      psNext(ps);
      current = { kind: "Unary", op: "%", expr: current };
    }
    return current;
  }

  function parseUnary(ps, depth) {
    if (!psCheckDepth(ps, depth)) return null;
    if (peekIsPunct(ps, "+", "-")) {
      const op = psNext(ps).value;
      const expr = parseUnary(ps, depth + 1);
      return expr ? { kind: "Unary", op, expr } : null;
    }
    const node = parsePrimary(ps, depth);
    return node ? applyPercentSuffixes(ps, node, depth) : null;
  }

  /** Consumes the closing `)`; fails the parse when it is missing. */
  function expectCloseParen(ps) {
    if (!peekIsPunct(ps, ")")) return false;
    psNext(ps);
    return true;
  }

  function parseParenthesized(ps, depth) {
    const inner = parseComparison(ps, depth + 1);
    if (!inner) return null;
    return expectCloseParen(ps) ? inner : psFail(ps, "#VALUE!");
  }

  /** Parses `arg (, arg)*` into args; false when an argument failed. */
  function parseCallArgs(ps, depth, args) {
    while (true) {
      const argAst = parseComparison(ps, depth + 1);
      if (!argAst) return false;
      args.push(argAst);
      if (!peekIsPunct(ps, ",")) return true;
      psNext(ps);
    }
  }

  function parseFuncCall(ps, depth, name) {
    psNext(ps);
    const args = [];
    if (!peekIsPunct(ps, ")") && !parseCallArgs(ps, depth, args)) return null;
    return expectCloseParen(ps) ? { kind: "FuncCall", name, args } : psFail(ps, "#VALUE!");
  }

  function parseIdentifier(ps, tok, depth) {
    const up = asciiUpperJs(tok.value);
    if (peekIsPunct(ps, "(")) return parseFuncCall(ps, depth, up);
    if (up === "TRUE") return { kind: "Bool", value: true };
    if (up === "FALSE") return { kind: "Bool", value: false };
    return parseRefOrRangeAst(ps, null, tok.value);
  }

  function parseSheetPrefixed(ps, sheetTok) {
    const refTok = psNext(ps);
    if (refTok?.type !== "ident") return psFail(ps, "#REF!");
    return parseRefOrRangeAst(ps, sheetTok.name, refTok.value);
  }

  const LITERAL_NODE_BUILDERS = {
    num: (t) => ({ kind: "Number", value: t.value }),
    str: (t) => ({ kind: "Str", value: t.value }),
    err: (t) => ({ kind: "Error", code: t.code }),
  };

  function parsePrimary(ps, depth) {
    if (!psCheckDepth(ps, depth)) return null;
    const t = psNext(ps);
    if (!t) return psFail(ps, "#VALUE!");
    const build = LITERAL_NODE_BUILDERS[t.type];
    if (build) return build(t);
    if (t.type === "punct" && t.value === "(") return parseParenthesized(ps, depth);
    if (t.type === "sheet") return parseSheetPrefixed(ps, t);
    if (t.type === "ident") return parseIdentifier(ps, t, depth);
    return psFail(ps, "#VALUE!");
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
    if (lexed.tokens.length === 0) return { kind: "Ast", root: { kind: "Empty" } };
    const ps = { tokens: lexed.tokens, pos: 0, fatal: null };
    const root = parseComparison(ps, 0);
    if (ps.fatal) return { kind: "FormulaError", code: ps.fatal };
    if (!root || ps.pos < ps.tokens.length) return { kind: "FormulaError", code: "#VALUE!" };
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
