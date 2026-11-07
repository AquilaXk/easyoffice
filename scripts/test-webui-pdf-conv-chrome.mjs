import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";
import { cssDeclarations } from "./test-support/css-declarations.mjs";
import { hasColorLiteral, rulesFor, ruleWithSelector } from "./test-support/css-rules.mjs";

// PD-098 step 4c: the PDF studio and the converter wear rhwp-studio's classes and tokens.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (name) => fs.readFileSync(fileURLToPath(new URL(name, UI_DIR)), "utf8");

test("the PDF toolbar is an rhwp toolbar: icon button, sb-* fields, tb-btn action, no legacy button classes", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=PDF&win=w-pdf-chrome" });
  const bar = document.getElementById("pdf-toolbar");
  assert.equal(bar.getAttribute("role"), "toolbar");
  assert.ok(bar.classList.contains("eo-doc-style-bar"));
  const rotate = document.getElementById("btn-pdf-rotate");
  assert.ok(rotate.classList.contains("tb-btn"));
  assert.ok(rotate.getAttribute("aria-label"));
  assert.ok(rotate.querySelector("svg"), "the rotate button shows an icon");
  assert.ok(document.getElementById("btn-pdf-apply").classList.contains("tb-btn"));
  assert.ok(bar.querySelector(".sb-field-grid"));
  for (const id of ["pdf-watermark-input", "pdf-form-applicant", "pdf-form-dept"]) {
    assert.ok(document.getElementById(id).classList.contains("sb-select"), `${id} uses .sb-select`);
  }
  assertCombo(document, "pdf-stamp-select");
  for (const legacy of [".secondary-btn", ".primary-btn", ".cmd-input", ".ctx-badge"]) {
    assert.equal(document.querySelector(`#pane-pdf ${legacy}`), null, `${legacy} is gone from the PDF screen`);
  }
});

// Format selects are hop combo boxes: the native select stays as the hidden state holder.
function assertCombo(document, id) {
  const select = document.getElementById(id);
  assert.ok(select.classList.contains("sb-combo"), `${id} is a combo box`);
  assert.ok(select.classList.contains("native-select-hidden"), `${id} stays as the hidden state holder`);
  assert.equal(select.parentElement.dataset.selectId, id, `${id} is wrapped by its custom-select root`);
}

test("the converter uses sb-select fields and dialog buttons and keeps every id", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=CONV&win=w-conv-chrome" });
  assert.ok(document.getElementById("conv-source-file").classList.contains("sb-select"));
  for (const id of ["conv-src-fmt", "conv-dst-fmt"]) assertCombo(document, id);
  const run = document.getElementById("btn-run-convert");
  assert.ok(run.classList.contains("dialog-btn") && run.classList.contains("dialog-btn-primary"));
  assert.ok(document.getElementById("btn-conv-clear-source").classList.contains("dialog-btn"));
  assert.ok(document.getElementById("conv-result-box").classList.contains("security-card"));
  for (const legacy of [".secondary-btn", ".primary-btn", ".cmd-input"]) {
    assert.equal(document.querySelector(`#pane-converter ${legacy}`), null, `${legacy} is gone from the converter`);
  }
});

test("PDF and converter rules use design tokens only; the PDF page card is the fixed light paper", () => {
  const needles = ["#pdf-toolbar", ".pdf-", ".conv-", ".converter-surface", ".security-card"];
  const rules = ["styles.css", "toolbar.css", "shell.css"].flatMap((name) => rulesFor(read(name), needles));
  assert.ok(rules.length >= 8, "the rules were found");
  for (const { selector, body } of rules) assert.equal(hasColorLiteral(body), false, `${selector} has a literal colour`);
  const card = rulesFor(read("styles.css"), [".pdf-page-card"]).find((r) => r.selector === ".pdf-page-card");
  assert.ok(card.body.includes("var(--doc-paper)"), "the page card is the document paper");
  const fixed = ruleWithSelector(read("content.css"), ".pdf-page-card");
  const tokens = read("tokens.css");
  const light = new Map(cssDeclarations(tokens.slice(tokens.indexOf("\n:root {"), tokens.indexOf('\n:root[data-theme-effective="dark"] {'))));
  const redeclared = cssDeclarations(fixed.body);
  assert.ok(redeclared.length >= 3);
  for (const [name, value] of redeclared) assert.equal(value, light.get(name), `${name} equals the light token`);
  assert.ok(rulesFor(read("toolbar.css"), ["#pdf-toolbar"]).length > 0, "the PDF toolbar takes the style-bar surface");
});
