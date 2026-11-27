import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";
import { rulesFor, ruleWithSelector } from "./test-support/css-rules.mjs";
import { readText } from "./test-support/read-text.mjs";

// The combo boxes of every dialog share one rule: the text starts a control padding inside the frame and ends a picker text gap
// before the chevron cell, and the box is as wide as its longest choice.

const MIN_TEXT_GAP_PX = 4;
const shellCss = readText(fileURLToPath(new URL("../crates/easyoffice-app/ui/shell.css", import.meta.url)));
const toolbarCss = readText(fileURLToPath(new URL("../crates/easyoffice-app/ui/toolbar.css", import.meta.url)));
const tokensCss = readText(fileURLToPath(new URL("../crates/easyoffice-app/ui/tokens.css", import.meta.url)));
const px = (body, property) => {
  const declaration = body.split(";").map((part) => part.trim()).find((part) => part.startsWith(`${property}:`));
  return declaration?.slice(property.length + 1).trim().split(/\s+/).map((v) => Number.parseFloat(v));
};

test("one rule gives every dialog combo box its text gap, and no dialog repeats it", () => {
  // PD-378: the Spectrum S picker's text start (9px), chevron cell (10px) and the 5px between them are tokens, set once in toolbar.css.
  const trigger = ruleWithSelector(toolbarCss, ".custom-select-trigger");
  assert.ok(trigger, "the common rule exists");
  const declared = (property) => trigger.body.split(";").map((part) => part.trim()).find((part) => part.startsWith(`${property}:`))?.slice(property.length + 1).trim();
  assert.equal(declared("padding"), "0 var(--picker-padding-end) 0 var(--control-padding-x)", "the text starts a control padding inside the frame");
  assert.equal(declared("column-gap"), "var(--picker-text-gap)");
  assert.equal(declared("grid-template-columns"), "minmax(0, 1fr) var(--chevron-size)", "the chevron cell is the icon size, with no divider");
  const token = (name) => Number.parseFloat(new RegExp(`${name}:\\s*([\\d.]+)px`).exec(tokensCss)[1]);
  assert.ok(token("--picker-text-gap") >= MIN_TEXT_GAP_PX, "at least 4px between the text and the chevron");
  assert.ok(token("--control-padding-x") >= MIN_TEXT_GAP_PX, "at least 4px before the text");
  const perDialog = rulesFor(shellCss, ["custom-select-trigger", "custom-select-value", "custom-select-arrow", "custom-select-sizer"])
    .filter((rule) => rule.selector.split(",").some((part) => !part.trim().startsWith(".tool-dialog ")));
  assert.deepEqual(perDialog.map((rule) => rule.selector), [], "no combo part is styled for one dialog by id");
});

test("every combo box in a dialog lists all its choices in the width-giving cell", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-dialog-combos" });
  const combos = [...document.querySelectorAll(".tool-dialog .custom-select")];
  assert.ok(combos.length > 20, "the dialogs have combo boxes");
  for (const root of combos) {
    const select = root.querySelector("select");
    const choices = [...select.querySelectorAll("option")].map((option) => option.textContent.trim());
    const sized = [...root.querySelectorAll(".custom-select-sizer > span")].map((span) => span.textContent);
    assert.deepEqual(sized, choices, `${root.dataset.selectId} sizes to its longest choice`);
  }
  const paper = document.getElementById("doc-ps-paper").parentElement;
  const sizes = [...paper.querySelectorAll(".custom-select-sizer > span")].map((span) => span.textContent);
  assert.ok(sizes.includes("Letter") && sizes.includes("사용자 정의") && sizes.includes("A4"));
});

test("hwp-studio overlay dialog combo boxes carry right padding and sizer width so labels never touch the arrow divider", () => {
  const studioCss = readText(fileURLToPath(new URL("../apps/hwp-studio/src/styles/custom-select.css", import.meta.url)));
  const studioTs = readText(fileURLToPath(new URL("../apps/hwp-studio/src/ui/custom-select.ts", import.meta.url)));
  const bodyOf = (sel) => rulesFor(studioCss, [sel]).filter((r) => r.selector.split(",").some((p) => p.trim() === sel)).map((r) => r.body).join(";");
  const valBody = bodyOf(".custom-select-value");
  const sizerBody = bodyOf(".dialog-wrap .custom-select-sizer");
  assert.ok(valBody && sizerBody, "hwp-studio value and sizer rules exist");
  assert.ok(px(valBody, "padding-right")[0] >= MIN_TEXT_GAP_PX, "hwp-studio value right gap >= 4px");
  assert.ok(px(sizerBody, "padding-right")[0] >= MIN_TEXT_GAP_PX, "hwp-studio sizer right gap >= 4px");
  assert.ok(studioTs.includes("custom-select-sizer"), "hwp-studio custom-select builds hidden sizer");
});
