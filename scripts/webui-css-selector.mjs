const NOT_OPEN = ":not(";

function isQuoteChar(ch) {
  return ch === '"' || ch === "'";
}

/** Index just after the quoted run that starts at `start` (honours backslash escapes; text.length if unterminated). */
function quotedEnd(text, start) {
  const quote = text[start];
  let j = start + 1;
  while (j < text.length) {
    const c = text[j];
    if (c === "\\" && j + 1 < text.length) j += 2;
    else if (c === quote) return j + 1;
    else j += 1;
  }
  return text.length;
}

/** Index of the `)` closing a `:not(` whose body starts at `start`, or -1 when unbalanced. */
function findNotClauseEnd(input, start) {
  let j = start;
  let depth = 1;
  while (j < input.length) {
    const c = input[j];
    if (isQuoteChar(c)) {
      j = quotedEnd(input, j);
      continue;
    }
    if (c === "(") depth += 1;
    else if (c === ")") depth -= 1;
    if (depth === 0) return j;
    j += 1;
  }
  return -1;
}

function nextBracketDepth(ch, depth) {
  if (ch === "[") return depth + 1;
  if (ch === "]" && depth > 0) return depth - 1;
  return depth;
}

function extractBalancedNotClauses(input) {
  let out = "";
  const negInners = [];
  let i = 0;
  let bracketDepth = 0;
  while (i < input.length) {
    const ch = input[i];
    if (isQuoteChar(ch)) {
      const end = quotedEnd(input, i);
      out += input.slice(i, end);
      i = end;
      continue;
    }
    if (bracketDepth === 0 && input.slice(i, i + NOT_OPEN.length).toLowerCase() === NOT_OPEN) {
      const close = findNotClauseEnd(input, i + NOT_OPEN.length);
      if (close >= 0) {
        negInners.push(input.slice(i + NOT_OPEN.length, close));
        i = close + 1;
        continue;
      }
    }
    bracketDepth = nextBracketDepth(ch, bracketDepth);
    out += ch;
    i += 1;
  }
  return { rest: out, negInners };
}

function parseSingleAttributeClause(rawInner) {
  const eqIdx = rawInner.indexOf("=");
  if (eqIdx < 0) {
    return rawInner ? { name: rawInner.toLowerCase(), op: null, value: undefined } : null;
  }
  const prevChar = eqIdx > 0 ? rawInner[eqIdx - 1] : "";
  const isCompoundOp =
    prevChar === "^" ||
    prevChar === "$" ||
    prevChar === "*" ||
    prevChar === "~" ||
    prevChar === "|";
  const op = isCompoundOp ? prevChar + "=" : "=";
  const nameEnd = isCompoundOp ? eqIdx - 1 : eqIdx;
  const name = rawInner.slice(0, nameEnd).trim().toLowerCase();
  if (!name) return null;
  let val = rawInner.slice(eqIdx + 1).trim();
  if (
    val.length >= 2 &&
    ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'")))
  ) {
    val = val.slice(1, -1).replaceAll(/\\(.)/g, "$1");
  }
  return { name, op, value: val };
}

/** Index of the `]` closing the attribute clause opened at `open`, or -1. */
function findAttributeEnd(input, open) {
  let j = open + 1;
  while (j < input.length) {
    const c = input[j];
    if (isQuoteChar(c)) j = quotedEnd(input, j);
    else if (c === "]") return j;
    else j += 1;
  }
  return -1;
}

function extractAttributeClauses(input) {
  let out = "";
  const attrs = [];
  let i = 0;
  while (i < input.length) {
    const close = input[i] === "[" ? findAttributeEnd(input, i) : -1;
    if (close < 0) {
      out += input[i];
      i += 1;
      continue;
    }
    const parsedAttr = parseSingleAttributeClause(input.slice(i + 1, close).trim());
    if (parsedAttr) attrs.push(parsedAttr);
    i = close + 1;
  }
  return { rest: out, attrs };
}

function parseSimpleSelector(rawSel) {
  const { rest: withoutNot, negInners } = extractBalancedNotClauses(rawSel.trim());
  const { rest: withoutAttrs, attrs } = extractAttributeClauses(withoutNot);
  let rest = withoutAttrs;
  let id = null;
  rest = rest.replaceAll(/#([A-Za-z0-9_-]+)/g, (_, m) => {
    id = m;
    return "";
  });
  const classes = [];
  rest = rest.replaceAll(/\.([A-Za-z0-9_-]+)/g, (_, m) => {
    classes.push(m);
    return "";
  });
  const tag = rest.trim() ? rest.trim().toUpperCase() : null;
  return { tag, id, classes, attrs, notSelectorLists: negInners };
}

const ATTRIBUTE_MATCHERS = {
  "=": (actual, expected) => actual === expected,
  "^=": (actual, expected) => expected.length > 0 && actual.startsWith(expected),
  "$=": (actual, expected) => expected.length > 0 && actual.endsWith(expected),
  "*=": (actual, expected) => expected.length > 0 && actual.includes(expected),
  "~=": (actual, expected) =>
    expected.length > 0 && !/\s/.test(expected) && actual.split(/\s+/).filter(Boolean).includes(expected),
  "|=": (actual, expected) => actual === expected || actual.startsWith(expected + "-"),
};

function matchesAttribute(el, attr) {
  if (!el._attrs.has(attr.name)) return false;
  if (attr.op === null) return true;
  const matcher = ATTRIBUTE_MATCHERS[attr.op];
  if (!matcher) return false;
  return matcher(el._attrs.get(attr.name) ?? "", attr.value ?? "");
}

function matchesSimple(el, spec, scopeEl) {
  if (el?.nodeType !== 1) return false;
  if (spec.tag && spec.tag !== "*" && el.tagName !== spec.tag) return false;
  if (spec.id && el.id !== spec.id) return false;
  for (const cls of spec.classes) {
    if (!el.classList.contains(cls)) return false;
  }
  for (const attr of spec.attrs) {
    if (!matchesAttribute(el, attr)) return false;
  }
  for (const negList of spec.notSelectorLists) {
    if (matchesSelectorList(el, negList, scopeEl)) return false;
  }
  return true;
}

function pushSelectorToken(tokens, rawBuf) {
  const trimmed = rawBuf.trim();
  if (!trimmed) return;
  if (trimmed === ":scope") {
    tokens.push({ type: "scope" });
    return;
  }
  tokens.push({ type: "sel", spec: parseSimpleSelector(trimmed) });
}

function updateDepths(depth, ch) {
  if (ch === "[") depth.bracket += 1;
  else if (ch === "]" && depth.bracket > 0) depth.bracket -= 1;
  else if (ch === "(") depth.paren += 1;
  else if (ch === ")" && depth.paren > 0) depth.paren -= 1;
}

/**
 * Walks `text`, keeping quoted runs intact and tracking () / [] depth. Calls onSeparator(ch, buffered)
 * for every separator character at depth 0 and returns the trailing buffer.
 */
function scanTopLevel(text, isSeparator, onSeparator) {
  const depth = { paren: 0, bracket: 0 };
  let buf = "";
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    if (isQuoteChar(ch)) {
      const end = quotedEnd(text, i);
      buf += text.slice(i, end);
      i = end;
      continue;
    }
    updateDepths(depth, ch);
    if (depth.paren === 0 && depth.bracket === 0 && isSeparator(ch)) {
      onSeparator(ch, buf);
      buf = "";
    } else {
      buf += ch;
    }
    i += 1;
  }
  return buf;
}

function isCombinatorChar(ch) {
  return ch === ">" || /\s/.test(ch);
}

function pushCombinator(tokens, ch) {
  const last = tokens.at(-1);
  if (ch === ">") {
    if (last?.type === "comb") last.comb = ">";
    else tokens.push({ type: "comb", comb: ">" });
  } else if (last && last.type !== "comb") {
    tokens.push({ type: "comb", comb: " " });
  }
}

function tokenizeComplexSelector(sel) {
  const tokens = [];
  const tail = scanTopLevel(sel.trim(), isCombinatorChar, (ch, buf) => {
    pushSelectorToken(tokens, buf);
    pushCombinator(tokens, ch);
  });
  pushSelectorToken(tokens, tail);
  if (tokens.at(-1)?.type === "comb") tokens.pop();
  return tokens;
}

function matchesToken(el, tok, scopeEl) {
  if (el?.nodeType !== 1 || !tok) return false;
  if (tok.type === "scope") return el === scopeEl;
  if (tok.type === "sel") return matchesSimple(el, tok.spec, scopeEl);
  return false;
}

function matchChainAt(el, idx, tokens, scopeEl) {
  if (idx < 0 || !matchesToken(el, tokens[idx], scopeEl)) return false;
  if (idx === 0) return true;
  const combTok = tokens[idx - 1];
  if (combTok?.type !== "comb") return false;
  if (combTok.comb === ">") {
    return Boolean(el.parentElement) && matchChainAt(el.parentElement, idx - 2, tokens, scopeEl);
  }
  for (let p = el.parentElement; p; p = p.parentElement) {
    if (matchChainAt(p, idx - 2, tokens, scopeEl)) return true;
  }
  return false;
}

// A selector list is parsed once per distinct text: the DOM walks of the suites match the same few
// selectors against every element, so parsing per element dominated the harness setup time.
const parsedSelectorLists = new Map();

function parseSelectorList(selectorList) {
  let chains = parsedSelectorLists.get(selectorList);
  if (chains) return chains;
  const parts = [];
  const collect = (buf) => {
    if (buf.trim()) parts.push(buf.trim());
  };
  collect(scanTopLevel(selectorList, (ch) => ch === ",", (_ch, buf) => collect(buf)));
  chains = parts.map(tokenizeComplexSelector).filter((tokens) => tokens.length > 0);
  parsedSelectorLists.set(selectorList, chains);
  return chains;
}

export function matchesSelectorList(el, selectorList, scopeEl) {
  return parseSelectorList(selectorList).some((tokens) => matchChainAt(el, tokens.length - 1, tokens, scopeEl));
}
