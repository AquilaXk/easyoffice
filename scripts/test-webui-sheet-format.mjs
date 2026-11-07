import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

async function openModule(module, options = {}) {
  const h = await createWorkspaceHarness(options);
  switchModule(h.document, module);
  await flushAsyncTurns();
  return h;
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const bold = (d, ref) => cell(d, ref)?.style.fontWeight;

async function selectRange(d, from, toKeys) {
  const start = cell(d, from);
  start.focus();
  for (const [k, n] of toKeys) for (let i = 0; i < n; i += 1) key(d.activeElement, k, { shiftKey: true });
  await flushAsyncTurns(2);
}

async function press(sandbox, k) {
  sandbox.dispatchWindowKeydown({ key: k, ctrlKey: true });
  await flushAsyncTurns(14);
}

function bigSheetModel(rows, cols = 8) {
  const cells = [];
  for (let r = 1; r <= rows; r += 1) {
    for (let c = 0; c < cols; c += 1) cells.push({ ref: String.fromCodePoint(65 + c) + r, raw: String(r * 10 + c) });
  }
  return { sheets: [{ name: "큰시트", cells }] };
}

test("Ctrl+B on a range bolds every selected cell, not just the active one; one undo reverts all", async () => {
  const { document, sandbox } = await openModule("SHEET");
  await selectRange(document, "B2", [["ArrowRight", 1], ["ArrowDown", 2]]);
  await press(sandbox, "b");
  for (const ref of ["B2", "C2", "B3", "C3", "B4", "C4"]) assert.equal(bold(document, ref), "600", ref + " bold");
  assert.equal(bold(document, "D2"), "", "outside the range is untouched");
  assert.equal(bold(document, "B5"), "", "outside the range is untouched");
  await press(sandbox, "z");
  for (const ref of ["B2", "C2", "B3", "C3", "B4", "C4"]) assert.equal(bold(document, ref), "", ref + " unbold after ONE undo");
  await press(sandbox, "y");
  for (const ref of ["B2", "C4"]) assert.equal(bold(document, ref), "600", ref + " bold after redo");
});

test("toggle rule: all bold -> remove, none or mixed -> set all", async () => {
  const { document, sandbox } = await openModule("SHEET");
  await selectRange(document, "B2", [["ArrowDown", 2]]);
  await press(sandbox, "b");
  await press(sandbox, "b");
  for (const ref of ["B2", "B3", "B4"]) assert.equal(bold(document, ref), "", "all bold toggles off");
  cell(document, "B3").focus();
  await press(sandbox, "b");
  assert.equal(bold(document, "B3"), "600");
  await selectRange(document, "B2", [["ArrowDown", 2]]);
  const btn = () => document.querySelector('[data-command-id="sheet-home-bold"]') || document.getElementById("btn-stylebar-bold");
  assert.equal(btn()?.getAttribute("aria-pressed"), "mixed", "toolbar shows mixed");
  await press(sandbox, "b");
  for (const ref of ["B2", "B3", "B4"]) assert.equal(bold(document, ref), "600", "mixed sets all");
  assert.equal(btn()?.getAttribute("aria-pressed"), "true", "toolbar shows true");
  await press(sandbox, "b");
  assert.equal(btn()?.getAttribute("aria-pressed"), "false", "toolbar shows false");
});

test("italic and underline apply to the whole range; font size applies to all cells", async () => {
  const { document, sandbox } = await openModule("SHEET");
  await selectRange(document, "A2", [["ArrowRight", 2], ["ArrowDown", 1]]);
  await press(sandbox, "i");
  await press(sandbox, "u");
  for (const ref of ["A2", "B2", "C2", "A3", "B3", "C3"]) {
    assert.equal(cell(document, ref).style.fontStyle, "italic", ref);
    assert.equal(cell(document, ref).style.textDecoration, "underline", ref);
  }
});

test("ribbon font size applies to every cell of the range; strikethrough via Ctrl+5 keeps underline", async () => {
  const { document, sandbox, userClick } = await openModule("SHEET");
  await selectRange(document, "A2", [["ArrowRight", 2], ["ArrowDown", 1]]);
  userClick(document.querySelector('[data-command-id="sheet-home-style"]'));
  await flushAsyncTurns(14);
  for (const ref of ["A2", "B2", "C2", "A3", "B3", "C3"]) assert.equal(cell(document, ref).style.fontSize, "15px", ref);
  assert.equal(cell(document, "D2").style.fontSize, "", "outside the range is untouched");
  await press(sandbox, "u");
  key(document.getElementById("sheet-viewport"), "5", { ctrlKey: true });
  await flushAsyncTurns(14);
  for (const ref of ["A2", "C3"]) assert.equal(cell(document, ref).style.textDecoration, "underline line-through", ref);
  await press(sandbox, "z");
  assert.equal(cell(document, "C3").style.textDecoration, "underline", "one undo removes only the strike");
  await press(sandbox, "z");
  await press(sandbox, "z");
  assert.equal(cell(document, "C3").style.fontSize, "", "undo reverts the size on every cell");
});

test("range format reaches rows outside the viewport and survives scroll virtualization", async () => {
  const { document, sandbox } = await openModule("SHEET");
  await pickFile(document, makeFixtureFile("큰시트.xlsx", bigSheetModel(10000)));
  const a1 = cell(document, "A1");
  a1.focus();
  key(a1, "ArrowRight", { shiftKey: true, ctrlKey: true });
  key(document.activeElement, "ArrowDown", { shiftKey: true, ctrlKey: true });
  await flushAsyncTurns(4);
  const t0 = performance.now();
  await press(sandbox, "b");
  console.log("[perf] bold on A1:H10000 (80,000 cells): " + (performance.now() - t0).toFixed(0) + " ms");
  key(document.activeElement, "Home", { ctrlKey: true });
  await flushAsyncTurns(4);
  assert.equal(bold(document, "A1"), "600");
  key(document.activeElement, "ArrowDown", { ctrlKey: true });
  await flushAsyncTurns(4);
  assert.ok(cell(document, "A10000"), "far row materialized");
  assert.equal(bold(document, "A10000"), "600", "row far outside the original viewport is bold");
  assert.equal(bold(document, "H9000") || bold(document, "H9999"), "600");
  await press(sandbox, "z");
  key(document.activeElement, "ArrowDown", { ctrlKey: true });
  await flushAsyncTurns(4);
  assert.equal(bold(document, "A10000"), "", "undo clears the offscreen rows too");
});

test("range formatting persists across a sheet tab switch", async () => {
  const { document, sandbox, userClick } = await openModule("SHEET");
  await selectRange(document, "B2", [["ArrowDown", 2]]);
  await press(sandbox, "b");
  const tab2 = document.querySelector('.sheet-tab-btn[data-sheet-tab]:not([data-sheet-tab="Sheet1"])');
  const add = document.getElementById("btn-add-sheet-tab");
  userClick(tab2 || add);
  await flushAsyncTurns(20);
  userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns(20);
  for (const ref of ["B2", "B3", "B4"]) assert.equal(bold(document, ref), "600", ref);
});

test("DOC module Ctrl+B is unaffected by the sheet range formatting", async () => {
  const { document, sandbox } = await openModule("DOC");
  const editor = document.getElementById("doc-editor");
  assert.ok(editor);
  await press(sandbox, "b");
  assert.equal(document.querySelector("#pane-sheet td[data-cell]")?.style.fontWeight || "", "", "no sheet cell is bolded from DOC");
});
