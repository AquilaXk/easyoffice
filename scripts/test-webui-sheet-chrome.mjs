import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";
import { hasColorLiteral, rulesFor } from "./test-support/css-rules.mjs";
import { readText } from "./test-support/read-text.mjs";

// PD-098 step 4a: the sheet screen wears rhwp-studio's classes and tokens (no new components).

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (name) => readText(fileURLToPath(new URL(name, UI_DIR)));
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
  const tabRules = rulesFor(read("shell.css"), [".sheet-tab-bar", ".sheet-tab-btn"]).map((r) => r.body).join(" ");
  assert.ok(tabRules.includes("var(--color-bg)") && tabRules.includes("var(--ui-border-strong)") && tabRules.includes("var(--color-surface-raised)"), "tabs take the reference dialog tab values");
  assert.equal(/border-radius/.test(tabRules), false, "tabs are square");
  const header = rulesFor(read("styles.css"), [".sheet-table th, .doc-inline-table th"]).map((r) => r.body).join(" ");
  assert.ok(header.includes("var(--ui-bg-light)"), "grid headers use the toolbar background token");
  const selection = rulesFor(read("styles.css"), [".sheet-grid td.selected-cell"]).map((r) => r.body).join(" ");
  assert.ok(selection.includes("var(--table-selection-stroke)"), "selection uses the table selection stroke token");
});

// The toolbars of the document, sheet, slide and PDF windows are one format: the reference's tall button with the icon above its label.
const TOOLBAR_BUTTONS = "#icon-toolbar .tb-btn, #slide-toolbar .tb-btn, #pdf-toolbar .tb-btn";
const CHARACTER_ICONS = ["icon-ctrl-mark", "icon-para-mark"];

test("the toolbars are the reference toolbar format: the icon above a label, on the reference sprite", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-sheet-toolbar" });
  const buttons = [...document.querySelectorAll(TOOLBAR_BUTTONS)];
  assert.equal(buttons.length, 20 + 5 + 12 + 8 + 2, "document groups, the header and footer edit group, sheet groups, slide row, PDF row");
  const css = read("toolbar.css");
  for (const button of buttons) {
    const name = button.getAttribute("title");
    assert.ok(button.querySelector(".tb-label")?.textContent.trim(), `${name} has its label`);
    assert.ok(button.getAttribute("aria-label"), `${name} keeps its name on the button`);
    assert.equal(button.querySelector("svg"), null, `${name} draws no icon of its own`);
    assert.equal(button.hasAttribute("data-icon"), false);
    if (button.querySelector(".tb-icon-text")) continue;
    const icon = button.querySelector(".tb-sprite")?.className.split(" ").find((cls) => cls.startsWith("icon-"));
    assert.ok(icon, `${name} shows an icon`);
    assert.ok(CHARACTER_ICONS.includes(icon) || css.includes(`.${icon} { background-position:`), `${icon} has its cell of the sprite`);
  }
  const shown = buttons.filter((button) => !button.hidden && !button.closest("[hidden]") && button.closest("#icon-toolbar"));
  assert.equal(shown.length, 21, "the sheet window shows the sheet commands only");
  const rule = rulesFor(css, ["#icon-toolbar .tb-btn"]).map((r) => r.body).join(" ");
  assert.ok(rule.includes("flex-direction: column") && rule.includes("min-width: 44px") && rule.includes("padding: 2px 4px"));
  assert.match(css, /\.tb-sprite \{[^}]*width: 18px;[^}]*height: 18px;[^}]*var\(--ui-icon-sprite-url\)/);
  assert.ok(read("tokens.css").includes("--ui-icon-sprite-url: url(/hwp-studio/images/icon_small_ko_dark.svg)"));
});
