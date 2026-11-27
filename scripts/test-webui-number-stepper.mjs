import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// PD-377: every number field of the app is a field with a stacked up/down button cell (number-stepper.js) in place of the
// browser's spin button. A press steps by the field's `step`, clamps to `min` and `max`, keeps the decimals of the step, and
// tells the page with `input` and `change`; a disabled field does nothing.

const open = () => createWorkspaceHarness({ search: "?module=DOC&win=w-number-stepper" });
const byId = (h, id) => h.document.getElementById(id);
const buttons = (input) => [...input.parentNode.querySelectorAll(".eo-stepper-button")];
const [UP, DOWN] = [0, 1];

function watch(input) {
  const seen = [];
  for (const type of ["input", "change"]) input.addEventListener(type, () => seen.push(type));
  return seen;
}

test("every number field of the page is wrapped in the stepper, with the field's classes on the box and two buttons", async () => {
  const h = await open();
  const fields = [...h.document.querySelectorAll('input[type="number"]')];
  assert.ok(fields.length >= 50, "the pages have their number fields");
  for (const input of fields) {
    const root = input.parentNode;
    assert.equal(root.classList.contains("eo-stepper"), true, `${input.id} is in a stepper box`);
    assert.equal(input.classList.contains("eo-stepper-input"), true);
    for (const name of String(input.className).split(/\s+/).filter((n) => n && n !== "eo-stepper-input")) assert.equal(root.classList.contains(name), true, `${input.id}: the box takes ${name}`);
    assert.equal(buttons(input).length, 2, `${input.id} has an up and a down button`);
    assert.ok(buttons(input).every((b) => b.getAttribute("aria-label")), `${input.id}: the buttons have names`);
  }
});

test("a press steps by the field's step and tells the page with input and change", async () => {
  const h = await open();
  const size = byId(h, "doc-font-size");
  size.value = "10";
  const seen = watch(size);
  h.userClick(buttons(size)[UP]);
  assert.equal(size.value, "10.5", "the step of the size field is half a point");
  assert.deepEqual(seen, ["input", "change"]);
  h.userClick(buttons(size)[DOWN]);
  h.userClick(buttons(size)[DOWN]);
  assert.equal(size.value, "9.5");
  const scale = byId(h, "doc-font-scale");
  scale.value = "100";
  h.userClick(buttons(scale)[UP]);
  assert.equal(scale.value, "101", "a field without a step moves by one");
});

test("the decimals of the step are kept, so a tenth never becomes 0.30000000000000004", async () => {
  const h = await open();
  const value = byId(h, "doc-font-spacing-value");
  value.value = "0.2";
  h.userClick(buttons(value)[UP]);
  assert.equal(value.value, "0.3");
  h.userClick(buttons(value)[UP]);
  h.userClick(buttons(value)[UP]);
  assert.equal(value.value, "0.5");
});

test("min and max clamp the value, and a press that changes nothing tells nobody", async () => {
  const h = await open();
  const scale = byId(h, "doc-font-scale");
  scale.value = "600";
  const seen = watch(scale);
  h.userClick(buttons(scale)[UP]);
  assert.equal(scale.value, "600", "max");
  assert.deepEqual(seen, [], "no event when the value stays");
  scale.value = "1";
  h.userClick(buttons(scale)[DOWN]);
  assert.equal(scale.value, "1", "min");
  scale.value = "700";
  h.userClick(buttons(scale)[DOWN]);
  assert.equal(scale.value, "600", "a value above max comes back to max");
});

test("an empty field starts at its min (or 0)", async () => {
  const h = await open();
  const scale = byId(h, "doc-font-scale");
  scale.value = "";
  h.userClick(buttons(scale)[UP]);
  assert.equal(scale.value, "1", "min 1");
  const width = byId(h, "doc-ps-width");
  width.value = "";
  h.userClick(buttons(width)[DOWN]);
  assert.equal(width.value, "0", "min 0");
});

test("a disabled field switches its buttons off and a press does nothing", async () => {
  const h = await open();
  const value = byId(h, "doc-font-spacing-value");
  value.value = "";
  value.disabled = true;
  assert.equal(value.parentNode.classList.contains("disabled"), true);
  assert.ok(buttons(value).every((b) => b.disabled), "both buttons are off");
  h.userClick(buttons(value)[UP]);
  assert.equal(value.value, "");
  value.disabled = false;
  assert.ok(buttons(value).every((b) => !b.disabled), "and on again");
  assert.equal(value.parentNode.classList.contains("disabled"), false);
});

test("the stepper exposes the same step as a function, and a field that is already boxed is left alone", async () => {
  const h = await open();
  const api = h.host.__EASYOFFICE_NUMBER_STEPPER__;
  const size = byId(h, "doc-font-size");
  size.value = "20";
  assert.equal(api.stepField(size, 1), true);
  assert.equal(size.value, "20.5");
  const box = size.parentNode;
  api.enhanceNumberFields(h.document);
  assert.equal(size.parentNode, box, "a second pass adds no second box");
  assert.equal(box.querySelectorAll(".eo-stepper-button").length, 2);
});
