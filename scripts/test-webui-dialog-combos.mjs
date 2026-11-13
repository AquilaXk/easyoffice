import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";
import { rulesFor, ruleWithSelector } from "./test-support/css-rules.mjs";
import { readText } from "./test-support/read-text.mjs";

// The combo boxes of every dialog share one rule: the text starts 4px inside the frame and ends 4px before the arrow
// cell's divider, and the box is as wide as its longest choice.

const MIN_TEXT_GAP_PX = 4;
const shellCss = readText(fileURLToPath(new URL("../crates/easyoffice-app/ui/shell.css", import.meta.url)));
const px = (body, property) => {
  const declaration = body.split(";").map((part) => part.trim()).find((part) => part.startsWith(`${property}:`));
  return declaration?.slice(property.length + 1).trim().split(/\s+/).map((v) => Number.parseFloat(v));
};

test("one rule gives every dialog combo box its text gap, and no dialog repeats it", () => {
  const trigger = ruleWithSelector(shellCss, ".tool-dialog .custom-select-trigger");
  const value = ruleWithSelector(shellCss, ".tool-dialog .custom-select-value");
  assert.ok(trigger && value, "the common rules exist");
  assert.deepEqual(px(trigger.body, "padding"), [0, 0, 0, MIN_TEXT_GAP_PX], "4px before the text");
  assert.ok(px(value.body, "padding-right")[0] >= MIN_TEXT_GAP_PX, "at least 4px between the text and the arrow cell");
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
