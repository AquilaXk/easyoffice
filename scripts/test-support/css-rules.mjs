// Stylesheet helpers for the WebUI contract tests (no declaration parsing by regex).

const HEX_DIGITS = "0123456789abcdefABCDEF";
const MIN_HEX_COLOR_DIGITS = 3;
const COLOR_FUNCTIONS = ["rgb(", "rgba(", "hsl(", "hsla("];

export function hasColorLiteral(text) {
  if (COLOR_FUNCTIONS.some((fn) => text.includes(fn))) return true;
  for (let at = text.indexOf("#"); at >= 0; at = text.indexOf("#", at + 1)) {
    let digits = 0;
    while (HEX_DIGITS.includes(text[at + 1 + digits] ?? "x")) digits += 1;
    if (digits >= MIN_HEX_COLOR_DIGITS) return true;
  }
  return false;
}

/** Rule blocks (`selector { body }`) of a stylesheet whose selector mentions one of `needles`. */
export function rulesFor(css, needles) {
  const rules = [];
  const stripped = css.split("*/").map((part) => part.slice(0, part.includes("/*") ? part.indexOf("/*") : part.length)).join("");
  for (const chunk of stripped.split("}")) {
    const open = chunk.indexOf("{");
    if (open < 0) continue;
    const selector = chunk.slice(0, open).trim();
    if (needles.some((needle) => selector.includes(needle))) rules.push({ selector, body: chunk.slice(open + 1) });
  }
  return rules;
}


/** The rule block whose selector list (`a, b`) contains exactly `selector`. */
export function ruleWithSelector(css, selector) {
  return rulesFor(css, [selector]).find((rule) => rule.selector.split(",").some((part) => part.trim() === selector));
}
