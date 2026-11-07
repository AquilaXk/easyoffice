import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// PD-098 step 4a: the sheet screen wears rhwp-studio's classes and tokens (no new components).

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (name) => fs.readFileSync(fileURLToPath(new URL(name, UI_DIR)), "utf8");
const HEX_DIGITS = "0123456789abcdefABCDEF";
const MIN_HEX_COLOR_DIGITS = 3;
const COLOR_FUNCTIONS = ["rgb(", "rgba(", "hsl(", "hsla("];

function hasColorLiteral(text) {
  if (COLOR_FUNCTIONS.some((fn) => text.includes(fn))) return true;
  for (let at = text.indexOf("#"); at >= 0; at = text.indexOf("#", at + 1)) {
    let digits = 0;
    while (HEX_DIGITS.includes(text[at + 1 + digits] ?? "x")) digits += 1;
    if (digits >= MIN_HEX_COLOR_DIGITS) return true;
  }
  return false;
}

/** Rule blocks (`selector { body }`) of a stylesheet whose selector mentions one of `needles`. */
function rulesFor(css, needles) {
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

test("the formula bar is a separate row built from rhwp style-bar and toolbar classes", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-sheet-chrome" });
  const bar = document.getElementById("sheet-formula-bar");
  assert.ok(bar, "#sheet-formula-bar");
  assert.equal(bar.closest("#pane-sheet") !== null, true);
  assert.equal(bar.getAttribute("role"), "toolbar");
  assert.ok(bar.classList.contains("eo-doc-style-bar"), "uses the rhwp style-bar surface");
  assert.ok(bar.querySelector(".sb-field-grid"), "rhwp .sb-* field group");
  assert.ok(document.getElementById("formula-input").classList.contains("sb-select"));
  const evalBtn = document.getElementById("btn-eval-formula");
  assert.ok(evalBtn.classList.contains("tb-btn"));
  assert.ok(evalBtn.parentElement.classList.contains("tb-group"));
  for (const id of ["active-cell-ref", "formula-input", "btn-eval-formula", "sheet-eval-result"]) {
    assert.ok(document.getElementById(id), `${id} keeps its id`);
  }
});

test("the sheet tabs are a tablist directly above the status bar, inside the sheet pane", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-sheet-tabs" });
  const pane = document.getElementById("pane-sheet");
  const tabs = document.getElementById("sheet-tab-bar");
  assert.equal(tabs.parentElement, pane, "the tab bar is the last row of the sheet pane");
  assert.equal(tabs.getAttribute("role"), "tablist");
  const children = [...pane.children];
  assert.equal(children.at(-1), tabs);
  assert.ok(children.indexOf(document.querySelector("#pane-sheet .sheet-surface")) < children.indexOf(tabs));
  assert.ok(tabs.querySelector('.sheet-tab-btn.active[aria-selected="true"]'));
});

test("sheet, formula bar and tab rules use design tokens only (no literal colours)", () => {
  const needles = [".sheet-", "#sheet-", ".formula-bar", ".cell-ref", ".sb-formula", "#formula-input"];
  const rules = ["styles.css", "toolbar.css", "shell.css"].flatMap((name) => rulesFor(read(name), needles));
  assert.ok(rules.length >= 10, "the sheet rules were found");
  for (const { selector, body } of rules) assert.equal(hasColorLiteral(body), false, `${selector} has a literal colour`);
  const tabRule = rulesFor(read("styles.css"), [".sheet-tab-bar"]).map((r) => r.body).join(" ");
  assert.ok(tabRule.includes("var(--ui-bg-light)") && tabRule.includes("var(--ui-border-strong)"), "tab bar takes the status-bar surface");
  const header = rulesFor(read("styles.css"), [".sheet-table th, .doc-inline-table th"]).map((r) => r.body).join(" ");
  assert.ok(header.includes("var(--ui-bg-light)"), "grid headers use the toolbar background token");
  const selection = rulesFor(read("styles.css"), [".sheet-grid td.selected-cell"]).map((r) => r.body).join(" ");
  assert.ok(selection.includes("var(--accent-strong)"), "selection uses the accent token");
});
