/**
 * Hygiene rule: a `.sort()` call without a comparator is rejected in EasyOffice-owned sources.
 * The default sort converts items to strings and is a reliability smell on numbers and mixed data, so
 * every call must state its ordering (a comparator function).
 *
 * The check is deliberately tokenizer-light: comments, string literals, template literal text and
 * regular expression literals are blanked out (newlines are kept so line numbers stay correct, code
 * inside template `${...}` expressions is kept), then `.sort(` followed only by whitespace and `)` is
 * matched.
 */

const REGEX_PRECEDING_WORD = /\b(?:return|typeof|case|in|of|delete|void|throw|new|else|do|yield|await)$/;
const REGEX_PRECEDING_CHAR = /[(,=:[!&|?{};+\-*%<>~^]/;
const BARE_SORT = /\.sort\s*\(\s*\)/g;

function blank(text) {
  return text.replaceAll(/[^\r\n]/g, " ");
}

function lineCommentEnd(src, start) {
  const end = src.indexOf("\n", start);
  return end === -1 ? src.length : end;
}

function blockCommentEnd(src, start) {
  const end = src.indexOf("*/", start + 2);
  return end === -1 ? src.length : end + 2;
}

/** End index (exclusive) of a quoted string that starts at `start`; unterminated strings stop at the line end. */
function quotedEnd(src, start) {
  const quote = src[start];
  let i = start + 1;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\") i += 2;
    else if (c === quote) return i + 1;
    else if (c === "\n") return i;
    else i += 1;
  }
  return src.length;
}

/** End index (exclusive) of a regular expression literal that starts at `start`. */
function regexEnd(src, start) {
  let i = start + 1;
  let inClass = false;
  while (i < src.length) {
    const c = src[i];
    if (c === "\n") return i;
    if (c === "\\") i += 2;
    else {
      if (c === "[") inClass = true;
      else if (c === "]") inClass = false;
      else if (c === "/" && !inClass) return i + 1;
      i += 1;
    }
  }
  return src.length;
}

function startsRegex(emitted) {
  const tail = emitted.trimEnd();
  if (tail === "") return true;
  return REGEX_PRECEDING_CHAR.test(tail.at(-1)) || REGEX_PRECEDING_WORD.test(tail);
}

function skipTo(st, end) {
  st.out += blank(st.src.slice(st.i, end));
  st.i = end;
}

/** Template-literal text: consume one escape, closer, `${` opener or plain character. */
function stepTemplate(st, c, next) {
  if (c === "\\") skipTo(st, st.i + 2);
  else if (c === "`") {
    st.modes.pop();
    skipTo(st, st.i + 1);
  } else if (c === "$" && next === "{") {
    st.modes.push({ depth: 0 });
    skipTo(st, st.i + 2);
  } else skipTo(st, st.i + 1);
}

/** Skips a comment, string, template opener or regex literal at the cursor; false when it is plain code. */
function stepLiteral(st, c, next) {
  const { src } = st;
  if (c === "/" && next === "/") skipTo(st, lineCommentEnd(src, st.i));
  else if (c === "/" && next === "*") skipTo(st, blockCommentEnd(src, st.i));
  else if (c === '"' || c === "'") skipTo(st, quotedEnd(src, st.i));
  else if (c === "`") {
    st.modes.push({ template: true });
    skipTo(st, st.i + 1);
  } else if (c === "/" && startsRegex(st.out)) skipTo(st, regexEnd(src, st.i));
  else return false;
  return true;
}

/** Plain code: track `${ ... }` brace depth and copy the character through. */
function stepCode(st, c, mode) {
  if (mode && c === "{") mode.depth += 1;
  else if (mode && c === "}") {
    if (mode.depth === 0) {
      st.modes.pop();
      skipTo(st, st.i + 1);
      return;
    }
    mode.depth -= 1;
  }
  st.out += c;
  st.i += 1;
}

/** Replace comments, strings, template text and regex literals with spaces, keeping code and newlines. */
export function stripCommentsAndStrings(src) {
  // modes: stack of { template: true } or { depth: number } for `${ ... }` expressions
  const st = { src, out: "", i: 0, modes: [] };
  while (st.i < src.length) {
    const c = src[st.i];
    const next = src[st.i + 1];
    const mode = st.modes.at(-1);
    if (mode?.template) stepTemplate(st, c, next);
    else if (!stepLiteral(st, c, next)) stepCode(st, c, mode);
  }
  return st.out;
}

/** 1-based line numbers of every `.sort()` call without a comparator. */
export function findBareSorts(src) {
  const code = stripCommentsAndStrings(src);
  const lines = [];
  for (const match of code.matchAll(BARE_SORT)) {
    lines.push(code.slice(0, match.index).split("\n").length);
  }
  return lines;
}

const OWNED_PREFIXES = ["crates/easyoffice-app/ui/", "scripts/", "apps/hwp-studio/src/"];
const OWNED_EXCLUDED_PREFIXES = ["third_party/", "vendor/", "crates/easyoffice-app/ui/hwp-studio/"];

/** True for hand-written JS/MJS/TS sources the rule applies to (root-relative, forward-slash path). */
export function isSortRuleScoped(rel) {
  if (!/\.(?:m?js|ts)$/.test(rel)) return false;
  const segments = rel.split("/");
  if (segments.includes("node_modules")) return false;
  if (OWNED_EXCLUDED_PREFIXES.some((p) => rel.startsWith(p))) return false;
  return OWNED_PREFIXES.some((p) => rel.startsWith(p));
}
