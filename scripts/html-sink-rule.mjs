/**
 * Hygiene rule: WebUI scripts must not write markup into the DOM. `innerHTML` and `srcdoc` may only
 * be cleared (`= ""`); `innerHTML +=`, `outerHTML =`, `insertAdjacentHTML(...)`,
 * `document.write(...)`, `Range.createContextualFragment(...)`, `setHTMLUnsafe(...)`,
 * `parseHTMLUnsafe(...)` and `DOMParser.parseFromString(...)` are rejected. The last one is a sink
 * because the parsed tree is meant to be moved into the page; a use that has to stay is listed in
 * `PARSE_ALLOWED` with the reason it is safe.
 *
 * Matching runs on source with comments and string literals blanked (offsets are preserved), so
 * assignments split across lines (`.innerHTML =\n value`) are caught and prose in comments is not.
 * A property reached by a string key (`el["innerHTML"]`) is the same sink; a key built at run time
 * cannot be seen by a source rule.
 */
import { stripCommentsAndStrings } from "./sort-comparator-rule.mjs";

const SINK_PROPERTIES = "innerHTML|outerHTML|insertAdjacentHTML|srcdoc";
const STRING_KEY = new RegExp(`\\[[ \\t]*(["'\`])(${SINK_PROPERTIES})\\1[ \\t]*\\]`, "g");
const HTML_SINK = new RegExp(
  [
    String.raw`\.(innerHTML|outerHTML|srcdoc)\s*(\+?=)(?!=)`,
    String.raw`\.insertAdjacentHTML\s*\(`,
    String.raw`\bdocument\s*\.\s*write(?:ln)?\s*\(`,
    String.raw`\.createContextualFragment\s*\(`,
    String.raw`\.(?:setHTMLUnsafe|parseHTMLUnsafe)\s*\(`,
    String.raw`\.parseFromString\s*\(`,
  ].join("|"),
  "g"
);
const SET_SRCDOC = /\.setAttribute\s*\(/g;
const EMPTY_STRING = /^\s*(?:""|'')/;
const CLEARABLE = new Set(["innerHTML", "srcdoc"]);

/**
 * `parseFromString` uses that stay, by root-relative path. `hwp-print.js` reads one page SVG that the
 * editor frame sends, refuses a document type declaration, keeps only listed elements and attributes,
 * and only then imports the tree.
 */
export const PARSE_ALLOWED = new Map([
  ["crates/easyoffice-app/ui/hwp-print.js", "page SVG, whitelist sanitizer"],
  ["apps/hwp-studio/src/command/print-pages.ts", "editor print SVG page"],
]);

/** Returns one message per violation, each with the 1-based line of the sink. */
export function findHtmlSinkViolations(source, rel = "") {
  const src = source.replace(STRING_KEY, ".$2");
  const code = stripCommentsAndStrings(src);
  const lineOf = (index) => src.slice(0, index).split("\n").length;
  const violations = [];
  for (const match of code.matchAll(HTML_SINK)) {
    const [text, property, operator] = match;
    if (text.startsWith(".parseFromString") && PARSE_ALLOWED.has(rel)) continue;
    if (CLEARABLE.has(property) && operator === "=") {
      const rhs = src.slice(match.index + text.length);
      if (EMPTY_STRING.test(rhs)) continue;
    }
    violations.push(`line ${lineOf(match.index)}: ${text.replaceAll(/\s+/g, " ")}`);
  }
  for (const match of code.matchAll(SET_SRCDOC)) {
    const literal = src.slice(match.index + match[0].length).match(/^\s*(["'`])srcdoc\1/i);
    if (literal) violations.push(`line ${lineOf(match.index)}: .setAttribute(${literal[0].trim()}`);
  }
  return violations.sort((a, b) => Number(a.match(/\d+/)) - Number(b.match(/\d+/)));
}
