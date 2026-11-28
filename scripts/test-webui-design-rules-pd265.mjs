import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { readText } from "./test-support/read-text.mjs";

// PD-265: the BASE, BOARD and Copilot surfaces follow the reference app's rules (corners 0 where the reference has no
// counterpart part, no gradients, no shadows outside its dropdown and dialog tokens) and the sheet's values for the shared parts.

const UI = (name) => readText(fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url)));
const stripComments = (css) => css.replace(/\/\*[\s\S]*?\*\//g, "");
const declarations = (css, property) => [...stripComments(css).matchAll(new RegExp("(?:^|[;{\\s])" + property + "\\s*:\\s*([^;}]+)", "g"))].map((m) => m[1].trim());

// copilot.css: the rounded corners of the panel's bubble, composer card, chips and send button are the PD-387 exception, one token each.
const RADIUS_OF = {
  "base.css": ["0", "var(--radius-sm)"],
  "board.css": ["0"],
  "copilot.css": ["0", "var(--radius-sm)", "var(--radius-md)", "var(--copilot-bubble-radius)", "var(--copilot-composer-radius)", "var(--copilot-chip-radius)", "var(--copilot-send-radius)"],
};
// The one gradient of the panel is the shimmer of the waiting line (a moving band of light text over muted text, PD-387).
const SHIMMER_RULE = /\.eo-copilot-shimmer\s*\{[^}]*\}/;
const SHADOW_OF = { "base.css": [], "board.css": [], "copilot.css": ["var(--shadow-dropdown)"] };

for (const name of Object.keys(RADIUS_OF)) {
  test(`${name}: corners are 0 or the reference value, no gradient, no shadow beyond the reference tokens`, () => {
    const css = UI(name);
    for (const value of declarations(css, "border-radius")) assert.ok(RADIUS_OF[name].includes(value), `${name} border-radius: ${value}`);
    for (const value of declarations(css, "box-shadow")) assert.ok(SHADOW_OF[name].includes(value), `${name} box-shadow: ${value}`);
    assert.deepEqual(declarations(css, "filter").filter((v) => v.includes("drop-shadow")), []);
    assert.equal(/gradient\(/.test(stripComments(css).replace(SHIMMER_RULE, "")), false, `${name} has a gradient`);
  });
}

test("the sheet chart card is flat: no shadow, no gradient handle", () => {
  const css = stripComments(UI("content.css"));
  const block = (selector) => css.slice(css.indexOf(selector), css.indexOf("}", css.indexOf(selector)));
  assert.doesNotMatch(block(".sheet-chart-card {"), /box-shadow/);
  assert.doesNotMatch(block(".sheet-chart-card:focus-visible"), /box-shadow/);
  assert.doesNotMatch(block(".sheet-chart-resize {"), /gradient/);
  assert.doesNotMatch(block(".sheet-chart-btn {"), /border-radius/);
});

test("the BASE grid takes the sheet's header, cell and tab bar values", () => {
  const css = stripComments(UI("base.css"));
  const rule = (selector) => css.slice(css.indexOf(selector), css.indexOf("}", css.indexOf(selector)));
  const head = rule(".base-grid-table thead th");
  assert.match(head, /height:\s*26px/);
  assert.match(head, /font-size:\s*var\(--font-size-base\)/);
  assert.match(head, /font-weight:\s*700/);
  assert.match(head, /color:\s*var\(--ui-text-muted\)/);
  assert.match(rule(".base-grid-table th, .base-grid-table td"), /height:\s*24px/);
  assert.match(rule(".base-pane:not([hidden])"), /height:\s*100%/, "the pane fills the stage, so the tab bar sits at the bottom");
  assert.equal(css.includes(".base-sheet-tab-bar"), false, "the table tabs use the sheet's tab bar as it is");
  assert.equal(css.includes(".base-tb-btn"), false, "the toolbar buttons are the reference .tb-btn");
});

test("the BOARD canvas has no dot grid and the toolbar buttons are the reference .tb-btn", () => {
  const css = stripComments(UI("board.css"));
  assert.equal(/radial-gradient|background-size/.test(css), false);
  assert.equal(css.includes(".board-tb-btn"), false);
  const source = UI("board-studio.js");
  assert.doesNotMatch(source, /board-tb-btn/);
  assert.match(source, /className:\s*"tb-btn"/);
});

test("the toolbars of BASE and BOARD share the reference toolbar rules of the PDF and slide windows", () => {
  const toolbar = stripComments(UI("toolbar.css"));
  for (const id of ["#base-toolbar", "#board-toolbar"]) {
    assert.ok(toolbar.includes(id + " .tb-btn"), id + " takes the tall icon-above-label button");
    assert.ok(toolbar.includes(id + " .tb-group"), id);
  }
});
