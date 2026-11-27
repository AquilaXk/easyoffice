import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { readText } from "./test-support/read-text.mjs";
import {
  SANCTIONED_CARETS, dialogClasses, glyphArrows, hasLiteralPx, kitClasses, kitRows, sanctionedCaretCounts,
  scriptSizeViolations, selectorKey, sizeViolations, styleRules, unregisteredWidgets,
} from "./design-guard.mjs";
import { KNOWN_GLYPH_ARROWS, KNOWN_SCRIPT_SIZE_VIOLATIONS, KNOWN_SIZE_VIOLATIONS, KNOWN_WIDGET_CLASSES } from "./design-guard-known.mjs";
import { analyzeDialog, tokenSizes } from "./webkit-dom/dialog-layout.mjs";

// The design standard as a machine check (PD-098, docs/design-system.md "대화상자 부품 키트"). The colour guard of
// test-webui-theme.mjs keeps literal colours out; this keeps literal sizes, new control classes and text-glyph arrows out.
// What the tree already breaks sits in design-guard-known.mjs. The counts below are the lock: fixing a line lowers its
// count in the same commit, and a line that is added raises one and shows in the diff, so the lists only shrink.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const ui = (name) => readText(fileURLToPath(new URL(name, UI_DIR)));
const uiFiles = (extension) => fs.readdirSync(fileURLToPath(UI_DIR)).filter((name) => name.endsWith(extension));
const DESIGN_SYSTEM = readText(fileURLToPath(new URL("../docs/design-system.md", import.meta.url)));
const TOKENS = ui("tokens.css");

const SIZE_VIOLATION_COUNTS = {
  "base.css": 9, "board.css": 8, "content.css": 2, "copilot.css": 6, "home.css": 4, "menu-bar.css": 11, "shell.css": 64, "styles.css": 22, "toolbar.css": 27,
};
const GLYPH_ARROW_COUNT = 8;
const WIDGET_CLASS_COUNT = 24;

const sameList = (actual, known, what) => {
  const added = actual.filter((item) => !known.includes(item));
  const stale = known.filter((item) => !actual.includes(item));
  assert.deepEqual(added, [], `${what}: new violations (use the tokens and kit classes of docs/design-system.md instead of listing them)`);
  assert.deepEqual(stale, [], `${what}: fixed, remove these lines from design-guard-known.mjs and lower the count in this test`);
};

test("size rules of dialogs and controls use tokens, not literal px (only the known list is left)", () => {
  const files = uiFiles(".css").filter((name) => name !== "tokens.css");
  for (const name of files) {
    const actual = sizeViolations(ui(name)).map((v) => v.key);
    const known = KNOWN_SIZE_VIOLATIONS[name] ?? [];
    sameList(actual, known, name);
    assert.equal(known.length, SIZE_VIOLATION_COUNTS[name] ?? 0, `${name}: the known list and its count in this test differ`);
  }
  assert.deepEqual(Object.keys(KNOWN_SIZE_VIOLATIONS).filter((name) => !files.includes(name)), [], "a known file is gone");
});

test("CSS written inside scripts has no literal px sizes either", () => {
  for (const name of uiFiles(".js")) {
    const actual = scriptSizeViolations(ui(name)).map((v) => `${v.selector} {${v.properties.join(",")}}`);
    sameList(actual, KNOWN_SCRIPT_SIZE_VIOLATIONS[name] ?? [], name);
  }
});

test("size rules are read per rule: px, zero, tokens, at-rules and keyframes", () => {
  const css = `
    .dialog-row { gap: 8px; margin-bottom: 6px; }
    .dialog-input { height: var(--control-height-sm); padding: 0 var(--space-2); font-size: var(--font-size-sm); }
    .dialog-label { padding: 0px; min-height: 0; line-height: 1.4; }
    @media (max-width: 520px) { .dialog-wrap > .dialog-footer { padding: 0 16px 16px; } }
    @keyframes spin { from { height: 4px; } }
    .grid-cell { height: 24px; }
    .custom-select-trigger { height: calc(100% - 2px); font: 12px/1.4 sans-serif; }
  `;
  assert.deepEqual(sizeViolations(css).map((v) => [v.selector, v.properties]), [
    [".dialog-row", ["gap"]],
    [".dialog-wrap > .dialog-footer", ["padding"]],
    [".custom-select-trigger", ["font", "height"]],
  ]);
  assert.equal(styleRules(css).some((rule) => rule.selector === "from"), false, "keyframe steps are not rules");
  assert.equal(hasLiteralPx("0px"), false);
  assert.equal(hasLiteralPx("var(--space-2) 4px"), true);
  assert.equal(hasLiteralPx("1.5"), false);
  assert.equal(selectorKey("a".repeat(200)).length, 111);
});

test("a literal px size in a script is found: style properties and rule text", () => {
  const js = `
    el.style.height = "22px";
    el.style.setProperty("font-size", "12px");
    el.style.setProperty("--dialog-swatch", "#fff");
    sheet.insertRule(".custom-select { height: 22px; }");
    el.style.width = "40px";
  `;
  assert.deepEqual(scriptSizeViolations(js).map((v) => `${v.selector} ${v.properties}`), ["style height", "style font-size", ".custom-select height"]);
});

test("arrows made of text glyphs are known and nothing new is added; the caret of the standard keeps its places", () => {
  const files = [...uiFiles(".html").map((n) => [n, "html"]), ...uiFiles(".css").map((n) => [n, "css"]), ...uiFiles(".js").map((n) => [n, "js"])];
  const actual = {};
  for (const [name, kind] of files) {
    const text = ui(name);
    const hits = Object.fromEntries(glyphArrows(name, text, kind));
    if (Object.keys(hits).length) actual[name] = hits;
    for (const caret of sanctionedCaretCounts(name, text)) assert.equal(caret.found, caret.count, `${name}: ${caret.why} (${caret.text}) occurs ${caret.found} times, the standard lists ${caret.count}`);
  }
  assert.deepEqual(actual, KNOWN_GLYPH_ARROWS, "text-glyph arrows: draw an icon of ui/icons.js; a fixed one leaves design-guard-known.mjs");
  assert.equal(Object.values(KNOWN_GLYPH_ARROWS).flatMap((glyphs) => Object.values(glyphs)).reduce((a, b) => a + b, 0), GLYPH_ARROW_COUNT);
});

test("glyph arrows are told from prose and data", () => {
  assert.deepEqual([...glyphArrows("x.html", '<button title="Alt+↑">위</button><span>→</span><span aria-hidden="true">▾</span>', "html")], [["→", 1], ["▾", 1]]);
  assert.deepEqual([...glyphArrows("index.html", '<span aria-hidden="true">▾</span>', "html")], [["▾", 1]], "no place keeps the text caret since PD-377");
  assert.deepEqual([...glyphArrows("custom-select.js", 'arrow.textContent = "▾";\nconst glyphs = Array.from("→←↑↓↔↕");\n// "▸"\nconst a = "▸";', "js")], [["▾", 1], ["▸", 1]]);
  assert.deepEqual([...glyphArrows("x.css", '.a::after { content: "›"; }\n/* content: "»" */', "css")], [["›", 1]]);
  assert.deepEqual([...glyphArrows("x.css", '.a::after { content: "\\25BE"; }\n.b::after { content: "\\25BC"; }', "css")], [["▾", 1], ["▼", 1]], "a CSS escape is the glyph too");
  assert.deepEqual(SANCTIONED_CARETS, [], "every drop-down arrow is the chevron of toolbar.css");
});

test("the kit list names its classes with a PD, and dialogs use only kit classes or the grandfathered ones", () => {
  const rows = kitRows(DESIGN_SYSTEM);
  assert.ok(rows.length >= 12, "the kit table is missing");
  for (const row of rows) assert.match(row.row, /PD-\d+/, `kit row without a PD: ${row.row.slice(0, 60)}`);
  const kit = kitClasses(DESIGN_SYSTEM);
  const legacy = new Set(KNOWN_WIDGET_CLASSES);
  assert.equal(KNOWN_WIDGET_CLASSES.length, WIDGET_CLASS_COUNT);
  assert.deepEqual(KNOWN_WIDGET_CLASSES.filter((name) => kit.has(name)), [], "a class in the kit leaves the grandfathered list");
  assert.deepEqual(unregisteredWidgets(dialogClasses(ui("index.html")), kit, legacy), [], "register the new control class in docs/design-system.md 4-2 (one line, with a PD) or use a kit class");
  const everything = [...uiFiles(".html"), ...uiFiles(".css"), ...uiFiles(".js")].map(ui).join("\n");
  assert.deepEqual(KNOWN_WIDGET_CLASSES.filter((name) => !everything.includes(name)), [], "a grandfathered class that no file uses any more leaves the list");
});

test("a new control class in a dialog is refused until the kit lists it", () => {
  const kit = new Set(["dialog-input"]);
  const html = '<dialog class="tool-dialog"><div class="dialog-row"><select class="dialog-combo"></select><input class="dialog-input"><button class="dialog-spin-btn dialog-row"></button></div></dialog><div class="outside-combo"></div>';
  assert.deepEqual([...dialogClasses(html)].sort((a, b) => a.localeCompare(b)), ["dialog-combo", "dialog-input", "dialog-row", "dialog-spin-btn", "tool-dialog"]);
  assert.deepEqual(unregisteredWidgets(dialogClasses(html), kit, new Set()), ["dialog-combo", "dialog-spin-btn"]);
  assert.deepEqual(unregisteredWidgets(dialogClasses(html), new Set([...kit, "dialog-combo", "dialog-spin-btn"]), new Set()), []);
  assert.deepEqual(kitRows("## 4-2. 대화상자 부품 키트 (PD-1)\n\n| 클래스 | 부품 |\n| --- | --- |\n| `.a` `.b-c` | x PD-1 |\n").map((r) => r.classes), [["a", "b-c"]]);
});

test("every token the kit table names is defined in tokens.css", () => {
  const section = DESIGN_SYSTEM.split(/^## /m).find((part) => part.startsWith("4-2."));
  const named = new Set([...section.matchAll(/`(--[a-z0-9-]+)`/g)].map((m) => m[1]));
  assert.ok(named.size >= 10);
  for (const token of named) assert.ok(new RegExp(`${token}\\s*:`).test(TOKENS), `${token} is named by the kit table and missing from tokens.css`);
});

test("the control heights and font sizes of the layout guard are the tokens of tokens.css", () => {
  assert.deepEqual(tokenSizes("control-height-"), [24]);
  assert.deepEqual(tokenSizes("control-font-size"), [12]);
});

const dialogOf = (controls, clipped = []) => ({ dialogId: "x-dialog", controls, clipped });
const field = (name, row, top, height, fontSize = 12) => ({ name, row, top, height, fontSize });
const tokens = { height: [24], font: [12] };

test("layout guard: a row of equal controls passes, mismatches name the dialog, the element and the numbers", () => {
  assert.deepEqual(analyzeDialog(dialogOf([field("input#a", 0, 100, 24), field("div.custom-select", 0, 100, 24), field("button.dialog-btn", 1, 140, 24, 12)]), tokens), []);
  const found = analyzeDialog(dialogOf([
    field("input#a", 0, 100, 24), field("select#b", 0, 102, 28),
    field("input#c", 1, 140, 25, 15),
    field("input#d", 2, 180, 24), field("input#e", 2, 183, 24),
  ], [{ name: "span#f", scrollWidth: 140, clientWidth: 100, scrollHeight: 18, clientHeight: 18 }]), tokens);
  assert.deepEqual(found.map((v) => v.key), [
    "x-dialog height select#b", "x-dialog height input#c", "x-dialog font input#c", "x-dialog same-height select#b", "x-dialog same-top input#e", "x-dialog clip span#f",
  ]);
  assert.match(found[3].message, /#x-dialog same-height select#b: row 0: 28px beside input#a 24px/);
  assert.match(found[5].message, /content 140x18 in a 100x18 box/);
});
