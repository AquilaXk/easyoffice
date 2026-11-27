// Static design guard (docs/design-system.md, "대화상자 부품 키트"): what scripts/test-design-guard.mjs checks, as functions.
// The colour guard of test-webui-theme.mjs keeps literal colours out of the UI CSS; this one keeps literal sizes, new
// control classes and text-glyph arrows out. Rules that the tree already breaks are listed in design-guard-known.mjs and the
// lists may only shrink.

const SIZE_PROPS = /^(height|min-height|font-size|line-height|gap|row-gap|column-gap|font|padding(-top|-right|-bottom|-left|-inline|-block)?(-start|-end)?)$/;
const LITERAL_PX = /(?<![\w.-])(?:\d+\.?\d*|\.\d+)px\b/g;
const SKIPPED_AT_RULES = /^@(?:-webkit-)?(?:keyframes|font-face|page)\b/;

/** Selectors of dialogs and controls: a dialog, or one of the control words (a whole word, so `table` and `selected` do not count). */
const CONTROL_SELECTOR = /dialog|\.(?:tb|sb|md|cp)-|(?<![a-z])(?:input|select|textarea|button|btn|combo|spin|checkbox|radio|swatch|picker|palette|menu|tab|tabs)(?![a-z])/;

/** Where a control is a widget of its own: class names that stand for a control (a new one must be in the kit list). */
export const WIDGET_CLASS = /(?<![a-z])(?:combo|select|spin|stepper|input|list|swatch|slider|toggle|switch|dropdown|picker|checkbox|radio|tab|tabs|btn|button|field|chip|palette|grid)(?![a-z])/;

const byName = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const STRING_LITERAL =/"((?:\\.|[^"\\\n])*)"|'((?:\\.|[^'\\\n])*)'|`((?:\\.|[^`\\])*)`/g;

const stripComments = (css) => css.replaceAll(/\/\*[\s\S]*?\*\//g, "");

/** Splits `text` at `separator` outside quotes and parentheses. */
function splitTop(text, separator) {
  const parts = [];
  let part = "";
  let depth = 0;
  let quote = "";
  for (const ch of text) {
    if (quote) {
      if (ch === quote) quote = "";
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (ch === "(") depth += 1;
    else if (ch === ")") depth -= 1;
    else if (ch === separator && depth === 0) {
      parts.push(part);
      part = "";
      continue;
    }
    part += ch;
  }
  parts.push(part);
  return parts;
}

/** `{ selector, declarations: [[property, value]] }` of every style rule of a stylesheet, at-rules like `@media` unwrapped, `@keyframes` and `@font-face` left out. */
export function styleRules(css) {
  const rules = [];
  const open = [];
  let head = "";
  let depth = 0;
  let quote = "";
  const text = stripComments(css);
  for (let at = 0; at < text.length; at += 1) {
    const ch = text[at];
    if (quote) {
      if (ch === quote && text[at - 1] !== "\\") quote = "";
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (ch === "(") depth += 1;
    else if (ch === ")") depth -= 1;
    else if (ch === "{" && depth === 0) {
      open.push(head.trim());
      head = "";
      continue;
    } else if (ch === "}" && depth === 0) {
      const selector = open.pop() ?? "";
      const skipped = selector.startsWith("@") || open.some((outer) => SKIPPED_AT_RULES.test(outer));
      if (!skipped) {
        const declarations = splitTop(head, ";").map((d) => [d.slice(0, d.indexOf(":")).trim().toLowerCase(), d.slice(d.indexOf(":") + 1).trim()]).filter(([name]) => name && !name.includes(" "));
        rules.push({ selector: selector.replaceAll(/\s+/g, " "), declarations });
      }
      head = "";
      continue;
    }
    head += ch;
  }
  return rules;
}

const KEY_LENGTH = 110;
/** The name a selector has in the known-violation lists: the selector, cut at 110 characters (the long toolbar lists stay one line). */
export const selectorKey = (selector) => (selector.length > KEY_LENGTH ? `${selector.slice(0, KEY_LENGTH)}…` : selector);

/** True when `value` holds a px length other than 0. */
export const hasLiteralPx = (value) => (value.match(LITERAL_PX) ?? []).some((px) => Number.parseFloat(px) !== 0);

/**
 * The size rules of a stylesheet that break the token rule: a dialog or control selector with a literal px in
 * `height`, `min-height`, `font-size`, `line-height`, `gap` or `padding`. One entry per `selector`, `properties` sorted.
 */
export function sizeViolations(css) {
  const found = new Map();
  for (const { selector, declarations } of styleRules(css)) {
    if (!CONTROL_SELECTOR.test(selector)) continue;
    for (const [name, value] of declarations) {
      if (SIZE_PROPS.test(name) && hasLiteralPx(value)) found.set(selector, new Set([...(found.get(selector) ?? []), name]));
    }
  }
  return [...found].map(([selector, properties]) => ({ selector, key: selectorKey(selector), properties: [...properties].sort(byName) }));
}

/** CSS written inside JS: `el.style.height = "22px"`, `style.setProperty("font-size", "12px")` and string literals that hold a rule. Entries are `{ selector, properties }` too (`style` for the first two). */
export function scriptSizeViolations(js) {
  const found = [];
  const camel = (name) => name.replaceAll(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
  for (const m of js.matchAll(/\.style\.(\w+)\s*=\s*(["'`])([^"'`]*)\2/g)) {
    if (SIZE_PROPS.test(camel(m[1])) && hasLiteralPx(m[3])) found.push({ selector: "style", properties: [camel(m[1])] });
  }
  for (const m of js.matchAll(/\.setProperty\(\s*(["'])([a-z-]+)\1\s*,\s*(["'`])([^"'`$]*)\3/g)) {
    if (SIZE_PROPS.test(m[2]) && hasLiteralPx(m[4])) found.push({ selector: "style", properties: [m[2]] });
  }
  for (const m of js.matchAll(STRING_LITERAL)) {
    const text = m[1] ?? m[2] ?? m[3];
    if (text.includes("{") && text.includes("}") && text.includes(":")) found.push(...sizeViolations(text));
  }
  return found;
}

/** Text-glyph arrows. `▾` is the one the standard keeps (docs/design-system.md 3), the rest stand in for icons. */
const ARROW_GLYPHS = "←↑→↓↔↕↗↘↙↖⇐⇑⇒⇓⇔▲▼◀▶►◄▴▾▸◂▹◃▽△⏴⏵⏶⏷⌄˄˅‹›«»";

/** Where the `▾` of the standard sits: the exact line of text and how often it may occur there. */
export const SANCTIONED_CARETS = [
  { file: "index.html", text: '<span aria-hidden="true">▾</span>', count: 28, why: "caret of a color select button (.sb-color-arrow, .dialog-swatch)" },
  { file: "custom-select.js", text: 'arrow.textContent = "▾";', count: 1, why: "arrow cell of the combo box" },
  { file: "board-style-bar.js", text: 'el("span", "", { "aria-hidden": "true" }, "▾")', count: 1, why: "color button caret of the board style bar" },
];

const SCRIPT_COMMENT = /^\s*(\/\/|\/\*|\*)/;
const quotedGlyphs = (line) => [...line.matchAll(/(["'`])([^"'`\n]{1,3})\1/g)].map((m) => m[2]).filter((text) => [...text].some((ch) => ARROW_GLYPHS.includes(ch)));

/**
 * The arrow glyphs a file draws as widgets, as `glyph -> count`, `sanctioned` caret lines left out. In HTML that is text
 * between tags, in CSS a `content:` value, in scripts a string of at most three characters (a longer sentence or a
 * symbol table is prose or data, not a drawn arrow).
 */
export function glyphArrows(file, text, kind) {
  const counts = new Map();
  const note = (glyph) => counts.set(glyph, (counts.get(glyph) ?? 0) + 1);
  const carets = SANCTIONED_CARETS.filter((s) => s.file === file);
  let body = text;
  for (const { text: sanctioned } of carets) body = body.replaceAll(sanctioned, "");
  if (kind === "html") {
    for (const m of body.matchAll(/>([^<>]*)</g)) for (const ch of m[1]) if (ARROW_GLYPHS.includes(ch)) note(ch);
  } else if (kind === "css") {
    for (const m of stripComments(body).matchAll(/content\s*:\s*([^;}]*)/g)) for (const ch of m[1]) if (ARROW_GLYPHS.includes(ch)) note(ch);
  } else {
    for (const line of body.split("\n")) {
      if (SCRIPT_COMMENT.test(line)) continue;
      for (const glyphs of quotedGlyphs(line)) for (const ch of glyphs) if (ARROW_GLYPHS.includes(ch)) note(ch);
    }
  }
  return counts;
}

/** How often the sanctioned `▾` lines occur in `text` of `file` (the standard names the exact place). */
export const sanctionedCaretCounts = (file, text) => SANCTIONED_CARETS.filter((s) => s.file === file).map((s) => ({ ...s, found: text.split(s.text).length - 1 }));

/** Classes of the kit list: every backticked `.class` in the first cell of a table row under the "대화상자 부품 키트" heading, with its row (which must name a PD). */
export function kitRows(markdown) {
  const section = markdown.split(/^## /m).find((part) => /^(?:[\d-]+\.\s*)?대화상자 부품 키트/.test(part)) ?? "";
  return section.split("\n").filter((line) => line.startsWith("| `.")).map((line) => {
    const cells = line.split("|").map((cell) => cell.trim());
    return { classes: [...cells[1].matchAll(/`\.([A-Za-z0-9_-]+)`/g)].map((m) => m[1]), row: line };
  });
}

export const kitClasses = (markdown) => new Set(kitRows(markdown).flatMap((row) => row.classes));

/** The control-like classes of a list of class tokens that are neither in the kit nor in the grandfathered list. */
export function unregisteredWidgets(classes, kit, legacy) {
  return [...new Set(classes)].filter((name) => WIDGET_CLASS.test(name) && !kit.has(name) && !legacy.has(name)).sort(byName);
}

/** The class tokens used inside the `<dialog>` elements of an HTML page. */
export function dialogClasses(html) {
  const out = new Set();
  for (const dialog of html.matchAll(/<dialog\b[\s\S]*?<\/dialog>/g)) {
    for (const attr of dialog[0].matchAll(/\bclass="([^"]*)"/g)) for (const name of attr[1].split(/\s+/).filter(Boolean)) out.add(name);
  }
  return out;
}
