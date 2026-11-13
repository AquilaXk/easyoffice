import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { cssDeclarations } from "./test-support/css-declarations.mjs";
import { hasColorLiteral, rulesFor, ruleWithSelector } from "./test-support/css-rules.mjs";
import { readText } from "./test-support/read-text.mjs";

// PD-098 step 4b: the slide screen wears rhwp-studio's toolbar classes and the reference app's custom select.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (name) => readText(fileURLToPath(new URL(name, UI_DIR)));
const SLIDE_SEARCH = "?module=SLIDE&win=w-slide-chrome";
const TOOL_BUTTONS = [
  "btn-add-slide", "btn-slide-duplicate", "btn-slide-delete", "btn-slide-move-up", "btn-slide-move-down",
  "btn-slide-show", "btn-slide-show-current", "btn-slide-laser",
];

async function openSlide() {
  const harness = await createWorkspaceHarness({ search: SLIDE_SEARCH });
  await flushAsyncTurns(10);
  return harness;
}

test("the slide toolbar is an rhwp toolbar: icon-above-label buttons with accessible names", async () => {
  const { document } = await openSlide();
  const bar = document.getElementById("slide-toolbar");
  assert.equal(bar.getAttribute("role"), "toolbar");
  assert.ok(bar.classList.contains("eo-doc-style-bar"));
  assert.ok(bar.querySelectorAll(".tb-group").length >= 2);
  for (const id of TOOL_BUTTONS) {
    const btn = document.getElementById(id);
    assert.ok(btn.classList.contains("tb-btn"), `${id} is a .tb-btn`);
    assert.ok(btn.getAttribute("aria-label"), `${id} has an accessible name`);
    assert.ok(btn.querySelector(".tb-sprite"), `${id} shows a cell of the reference sprite`);
    assert.ok(btn.querySelector(".tb-label")?.textContent.trim(), `${id} has its label under the icon`);
    assert.equal(btn.querySelector("svg"), null, `${id} draws no icon of its own`);
  }
  assert.equal(bar.querySelector(".secondary-btn"), null);
});

test("aspect and transition selects are custom selects over the native state holders", async () => {
  const { document, userClick } = await openSlide();
  for (const [id, label] of [["slide-aspect-select", "16:9"], ["slide-transition-select", "페이드"]]) {
    const select = document.getElementById(id);
    assert.ok(select.classList.contains("native-select-hidden"), `${id} stays as the hidden state holder`);
    const root = select.parentElement;
    assert.ok(root.classList.contains("custom-select"));
    assert.equal(root.dataset.selectId, id);
    assert.equal(root.querySelector(".custom-select-value").textContent, label);
    assert.equal(root.querySelectorAll(".custom-select-option").length, select.querySelectorAll("option").length);
  }
  const aspect = document.getElementById("slide-aspect-select");
  const root = aspect.parentElement;
  root.querySelector(".custom-select-trigger").dispatchEvent({ type: "mousedown", preventDefault() {}, stopPropagation() {} });
  assert.ok(root.classList.contains("open"));
  const rows = root.querySelectorAll(".custom-select-option");
  rows[1].dispatchEvent({ type: "mousedown", preventDefault() {}, stopPropagation() {} });
  await flushAsyncTurns();
  assert.equal(aspect.value, "4:3");
  assert.equal(document.getElementById("slide-stage-box").dataset.aspect, "4:3", "the change event reached the slide code");
  assert.equal(root.querySelector(".custom-select-value").textContent, "4:3");
  assert.ok(!root.classList.contains("open"));
  assert.ok(root.querySelectorAll(".custom-select-option")[1].classList.contains("selected"));
  userClick(document.getElementById("btn-add-slide"));
});

test("assigning select.value in code refreshes the visible label", async () => {
  const { document } = await openSlide();
  const transition = document.getElementById("slide-transition-select");
  transition.value = "zoom";
  assert.equal(transition.parentElement.querySelector(".custom-select-value").textContent, "확대");
});

test("the laser pointer button reports its state through aria-pressed and its title", async () => {
  const { document, userClick } = await openSlide();
  const laser = document.getElementById("btn-slide-laser");
  assert.equal(laser.getAttribute("aria-pressed"), "false");
  userClick(laser);
  assert.equal(laser.getAttribute("aria-pressed"), "true");
  assert.equal(laser.title, "레이저 포인터: 켬");
  assert.equal(document.getElementById("slide-laser-dot").hidden, false);
  userClick(laser);
  assert.equal(laser.title, "레이저 포인터: 끔");
});

test("custom-select.js names the reference app as its source and is in the notices; combo rules ship in toolbar.css", () => {
  const source = read("custom-select.js");
  assert.match(source, /Adapted from the reference app/);
  assert.ok(source.includes("Edward Kim"));
  const notices = readText(new URL("../THIRD_PARTY_NOTICES.md", import.meta.url));
  assert.ok(notices.includes("ui/custom-select.js"));
  const css = read("toolbar.css");
  for (const sel of [".custom-select-trigger", ".custom-select-menu", ".custom-select-option", ".native-select-hidden"]) {
    assert.ok(css.includes(sel), sel);
  }
});

test("slide chrome rules use tokens only; the stage paper is the fixed light palette from content.css", () => {
  const needles = [".slide-toolbar", "#slide-toolbar", ".slide-thumb", ".slide-stage", ".slide-workspace", ".slide-notes", ".custom-select"];
  const rules = ["styles.css", "toolbar.css", "shell.css"].flatMap((name) => rulesFor(read(name), needles));
  assert.ok(rules.length >= 15, "the slide rules were found");
  for (const { selector, body } of rules) assert.equal(hasColorLiteral(body), false, `${selector} has a literal colour`);
  const stage = rulesFor(read("styles.css"), [".slide-stage"]).find((r) => r.selector === ".slide-stage");
  assert.ok(stage.body.includes("var(--doc-paper)"), "the stage is the document paper");
  const fixed = ruleWithSelector(read("content.css"), ".slide-stage");
  const redeclared = cssDeclarations(fixed.body);
  assert.ok(redeclared.length >= 3);
  const tokens = read("tokens.css");
  const light = new Map(cssDeclarations(tokens.slice(tokens.indexOf("\n:root {"), tokens.indexOf('\n:root[data-theme-effective="dark"] {'))));
  for (const [name, value] of redeclared) assert.equal(value, light.get(name), `${name} equals the light token`);
});

test("custom-select.js enhances the same select classes as the reference app", () => {
  const source = read("custom-select.js");
  for (const cls of ["sb-combo", "sb-ls-select", "dialog-select", "formula-select"]) {
    assert.ok(source.includes(`"select.${cls}"`), `select.${cls} is enhanced`);
  }
});
