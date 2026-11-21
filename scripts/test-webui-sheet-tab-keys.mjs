import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// The keys to the next and previous sheet (PD-290): Ctrl+PageDown and Ctrl+PageUp, and on a Mac Option+Right and Option+Left,
// through the real input handler of the sheet and the tab strip.

const SHEETS = ["가", "나", "다", "라"];

async function open({ mac = false, hidden = [] } = {}) {
  const h = await createWorkspaceHarness();
  const d = h.document;
  const utils = h.host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__;
  utils.guessClientPlatform = () => ({ platform: mac ? "macOS" : "Windows", shortcutModifier: mac ? "⌘" : "Ctrl" });
  switchModule(d, "SHEET");
  await flushAsyncTurns();
  const sheets = SHEETS.map((name) => ({ name, cells: [{ ref: "B2", raw: name + "시트" }], ...(hidden.includes(name) ? { hidden: true } : {}) }));
  await pickFile(d, makeFixtureFile("여러.xlsx", { sheets }));
  await flushAsyncTurns(15);
  const active = () => d.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab;
  const shown = () => [...d.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((t) => t.dataset.sheetTab);
  const cell = (ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  const press = async (key, extra = {}) => {
    const td = d.activeElement;
    const kept = td.dispatchEvent({ type: "keydown", key, target: td, ...extra });
    await flushAsyncTurns(15);
    return { defaultPrevented: !kept };
  };
  cell("A1").focus();
  return { ...h, active, shown, cell, press, at: () => d.getElementById("active-cell-ref").textContent };
}

test("Ctrl+PageDown and Ctrl+PageUp step through the tabs in order, and the ends stay", async () => {
  const t = await open();
  assert.deepEqual(t.shown(), SHEETS);
  assert.equal(t.active(), "가");
  await t.press("PageUp", { ctrlKey: true });
  assert.equal(t.active(), "가", "no wrap before the first sheet");
  const seen = [];
  for (let i = 0; i < 4; i += 1) {
    await t.press("PageDown", { ctrlKey: true });
    seen.push(t.active());
  }
  assert.deepEqual(seen, ["나", "다", "라", "라"], "no wrap after the last sheet");
  for (let i = 0; i < 2; i += 1) await t.press("PageUp", { ctrlKey: true });
  assert.equal(t.active(), "나");
  assert.equal(t.cell("B2").textContent, "나시트", "the cells of the sheet are shown");
});

test("the key is taken over: it does not move the cell and the browser gets no default", async () => {
  const t = await open();
  t.cell("C3").focus();
  const event = await t.press("PageDown", { ctrlKey: true });
  assert.equal(event.defaultPrevented, true);
  assert.equal(t.active(), "나");
  await t.press("PageUp", { ctrlKey: true });
  assert.equal(t.active(), "가");
  assert.equal(t.at(), "C3", "the first sheet's selection is where it was left");
});

test("a hidden sheet is passed over", async () => {
  const t = await open({ hidden: ["나", "다"] });
  assert.deepEqual(t.shown(), ["가", "라"]);
  await t.press("PageDown", { ctrlKey: true });
  assert.equal(t.active(), "라");
  await t.press("PageUp", { ctrlKey: true });
  assert.equal(t.active(), "가");
});

test("Option+Right and Option+Left step through the tabs on a Mac", async () => {
  const t = await open({ mac: true });
  await t.press("ArrowRight", { altKey: true });
  assert.equal(t.active(), "나");
  await t.press("ArrowRight", { altKey: true });
  assert.equal(t.active(), "다");
  await t.press("ArrowLeft", { altKey: true });
  assert.equal(t.active(), "나");
  assert.equal(t.at(), "A1", "the cell did not move");
});

test("Option+Right and Option+Left do nothing to the sheets elsewhere", async () => {
  const t = await open({ mac: false });
  await t.press("ArrowRight", { altKey: true });
  assert.equal(t.active(), "가");
});

test("Ctrl+Shift+PageDown, the page keys and Ctrl+Arrow keep their own meaning", async () => {
  const t = await open();
  await t.press("PageDown", { ctrlKey: true, shiftKey: true });
  assert.equal(t.active(), "가");
  await t.press("PageDown");
  assert.equal(t.active(), "가");
  assert.notEqual(t.at(), "A1", "PageDown moves the cell");
  await t.press("PageDown", { altKey: true });
  assert.equal(t.active(), "가");
  await t.press("ArrowRight", { ctrlKey: true });
  assert.equal(t.active(), "가");
});

test("a cell being edited keeps the keys for its text", async () => {
  const mac = await open({ mac: true });
  mac.cell("B2").focus();
  await mac.press("F2");
  await mac.press("ArrowLeft", { altKey: true });
  assert.equal(mac.active(), "가", "Option+Left moves the caret by a word");
  const win = await open();
  win.cell("B2").focus();
  await win.press("F2");
  await win.press("PageDown", { ctrlKey: true });
  assert.equal(win.active(), "가");
});

test("the sheet switch is the same one a click on the tab makes: the edit in the first sheet is kept", async () => {
  const t = await open();
  const d = t.document;
  t.cell("D4").focus();
  // What the browser does after an un-prevented printable keydown on a focused editable cell.
  const td = d.activeElement;
  assert.equal(td.dispatchEvent({ type: "keydown", key: "x", target: td }), true);
  td.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: "x", target: td });
  td.textContent = "x";
  td.dispatchEvent({ type: "input", inputType: "insertText", data: "x", target: td });
  await t.press("Enter");
  await t.press("PageDown", { ctrlKey: true });
  assert.equal(t.active(), "나");
  assert.equal(t.cell("D4").textContent, "");
  await t.press("PageUp", { ctrlKey: true });
  assert.equal(t.active(), "가");
  assert.equal(t.cell("D4").textContent, "x");
});
