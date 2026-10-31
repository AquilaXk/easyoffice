function extractBalancedNotClauses(input) {
  let out = "";
  const negInners = [];
  let i = 0;
  let inQuote = null;
  let bracketDepth = 0;
  while (i < input.length) {
    const ch = input[i];
    if (inQuote) {
      out += ch;
      if (ch === "\\" && i + 1 < input.length) {
        out += input[i + 1];
        i += 2;
        continue;
      }
      if (ch === inQuote) inQuote = null;
      i += 1;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inQuote = ch;
      out += ch;
      i += 1;
      continue;
    }
    if (ch === "[") {
      bracketDepth += 1;
      out += ch;
      i += 1;
      continue;
    }
    if (ch === "]" && bracketDepth > 0) {
      bracketDepth -= 1;
      out += ch;
      i += 1;
      continue;
    }
    if (bracketDepth === 0 && input.slice(i, i + 5).toLowerCase() === ":not(") {
      let j = i + 5;
      let pDepth = 1;
      let q = null;
      while (j < input.length && pDepth > 0) {
        const c = input[j];
        if (q) {
          if (c === "\\" && j + 1 < input.length) {
            j += 2;
            continue;
          }
          if (c === q) q = null;
        } else if (c === '"' || c === "'") {
          q = c;
        } else if (c === "(") {
          pDepth += 1;
        } else if (c === ")") {
          pDepth -= 1;
          if (pDepth === 0) break;
        }
        j += 1;
      }
      if (pDepth === 0) {
        negInners.push(input.slice(i + 5, j));
        i = j + 1;
        continue;
      }
    }
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

function extractAttributeClauses(input) {
  let out = "";
  const attrs = [];
  let i = 0;
  while (i < input.length) {
    if (input[i] !== "[") {
      out += input[i];
      i += 1;
      continue;
    }
    let j = i + 1;
    let q = null;
    while (j < input.length) {
      const c = input[j];
      if (q) {
        if (c === "\\" && j + 1 < input.length) {
          j += 2;
          continue;
        }
        if (c === q) q = null;
      } else if (c === '"' || c === "'") {
        q = c;
      } else if (c === "]") {
        break;
      }
      j += 1;
    }
    if (j < input.length && input[j] === "]") {
      const parsedAttr = parseSingleAttributeClause(input.slice(i + 1, j).trim());
      if (parsedAttr) attrs.push(parsedAttr);
      i = j + 1;
      continue;
    }
    out += input[i];
    i += 1;
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

function matchesAttribute(el, attr) {
  if (!el._attrs.has(attr.name)) return false;
  if (attr.op === null) return true;
  const actual = el._attrs.get(attr.name) ?? "";
  const expected = attr.value ?? "";
  if (attr.op === "=") return actual === expected;
  if (attr.op === "^=") return expected.length > 0 && actual.startsWith(expected);
  if (attr.op === "$=") return expected.length > 0 && actual.endsWith(expected);
  if (attr.op === "*=") return expected.length > 0 && actual.includes(expected);
  if (attr.op === "~=") {
    return (
      expected.length > 0 &&
      !/\s/.test(expected) &&
      actual.split(/\s+/).filter(Boolean).includes(expected)
    );
  }
  if (attr.op === "|=") {
    return actual === expected || actual.startsWith(expected + "-");
  }
  return false;
}

function matchesSimple(el, spec, scopeEl) {
  if (!el || el.nodeType !== 1) return false;
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

function tokenizeComplexSelector(sel, scopeEl) {
  const working = sel.trim();
  const tokens = [];
  let buf = "";
  let bracketDepth = 0;
  let parenDepth = 0;
  let inQuote = null;
  for (let i = 0; i < working.length; i++) {
    const ch = working[i];
    if (inQuote) {
      buf += ch;
      if (ch === "\\" && i + 1 < working.length) {
        buf += working[i + 1];
        i += 1;
        continue;
      }
      if (ch === inQuote) inQuote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inQuote = ch;
      buf += ch;
      continue;
    }
    if (ch === "[") bracketDepth += 1;
    else if (ch === "]" && bracketDepth > 0) bracketDepth -= 1;
    else if (ch === "(") parenDepth += 1;
    else if (ch === ")" && parenDepth > 0) parenDepth -= 1;

    if (bracketDepth === 0 && parenDepth === 0 && (ch === ">" || /\s/.test(ch))) {
      if (buf.trim()) {
        pushSelectorToken(tokens, buf);
        buf = "";
      }
      if (ch === ">") {
        if (tokens.length > 0 && tokens[tokens.length - 1].type === "comb") {
          tokens[tokens.length - 1].comb = ">";
        } else {
          tokens.push({ type: "comb", comb: ">" });
        }
      } else if (tokens.length > 0 && tokens[tokens.length - 1].type !== "comb") {
        tokens.push({ type: "comb", comb: " " });
      }
      continue;
    }
    buf += ch;
  }
  if (buf.trim()) {
    pushSelectorToken(tokens, buf);
  }
  if (tokens.length > 0 && tokens[tokens.length - 1].type === "comb") {
    tokens.pop();
  }
  return { scopeEl, tokens };
}

function matchesToken(el, tok, scopeEl) {
  if (!el || el.nodeType !== 1 || !tok) return false;
  if (tok.type === "scope") return el === scopeEl;
  if (tok.type === "sel") return matchesSimple(el, tok.spec, scopeEl);
  return false;
}

function matchChainAt(el, idx, tokens, scopeEl) {
  if (idx < 0 || !matchesToken(el, tokens[idx], scopeEl)) return false;
  if (idx === 0) return true;
  const combTok = tokens[idx - 1];
  if (!combTok || combTok.type !== "comb") return false;
  if (combTok.comb === ">") {
    return Boolean(el.parentElement) && matchChainAt(el.parentElement, idx - 2, tokens, scopeEl);
  }
  for (let p = el.parentElement; p; p = p.parentElement) {
    if (matchChainAt(p, idx - 2, tokens, scopeEl)) return true;
  }
  return false;
}

function matchesParsedChain(el, parsed) {
  const { scopeEl, tokens } = parsed;
  if (tokens.length === 0) return false;
  return matchChainAt(el, tokens.length - 1, tokens, scopeEl);
}

export function matchesSelectorList(el, selectorList, scopeEl) {
  const parts = [];
  let buf = "";
  let parenDepth = 0;
  let bracketDepth = 0;
  let inQuote = null;
  for (let i = 0; i < selectorList.length; i++) {
    const ch = selectorList[i];
    if (inQuote) {
      buf += ch;
      if (ch === "\\" && i + 1 < selectorList.length) {
        buf += selectorList[i + 1];
        i += 1;
        continue;
      }
      if (ch === inQuote) inQuote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inQuote = ch;
      buf += ch;
      continue;
    }
    if (ch === "(") parenDepth += 1;
    else if (ch === ")" && parenDepth > 0) parenDepth -= 1;
    else if (ch === "[") bracketDepth += 1;
    else if (ch === "]" && bracketDepth > 0) bracketDepth -= 1;
    if (ch === "," && parenDepth === 0 && bracketDepth === 0) {
      if (buf.trim()) parts.push(buf.trim());
      buf = "";
      continue;
    }
    buf += ch;
  }
  if (buf.trim()) parts.push(buf.trim());
  return parts.some((p) => matchesParsedChain(el, tokenizeComplexSelector(p, scopeEl)));
}
