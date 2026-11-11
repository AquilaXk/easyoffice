/**
 * Hygiene rule: WebUI scripts must not write markup into the DOM. `innerHTML` may only be cleared
 * (`= ""`), `innerHTML +=`, `outerHTML =` and `insertAdjacentHTML(...)` are always rejected.
 *
 * Matching runs on source with comments and string literals blanked (offsets are preserved), so
 * assignments split across lines (`.innerHTML =\n value`) are caught and prose in comments is not.
 */
import { stripCommentsAndStrings } from "./sort-comparator-rule.mjs";

const HTML_SINK = /\.(innerHTML|outerHTML)\s*(\+?=)(?!=)|\.insertAdjacentHTML\s*\(/g;
const EMPTY_STRING = /^\s*(?:""|'')/;

/** Returns one message per violation, each with the 1-based line of the sink. */
export function findHtmlSinkViolations(src) {
  const code = stripCommentsAndStrings(src);
  const violations = [];
  for (const match of code.matchAll(HTML_SINK)) {
    const line = src.slice(0, match.index).split("\n").length;
    const [text, property, operator] = match;
    if (property === "innerHTML" && operator === "=") {
      const rhs = src.slice(match.index + text.length);
      if (EMPTY_STRING.test(rhs)) continue;
    }
    violations.push(`line ${line}: ${text.replaceAll(/\s+/g, " ")}`);
  }
  return violations;
}
