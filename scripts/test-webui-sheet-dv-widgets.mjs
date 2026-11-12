import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// The check box cell and the multiple list (PD-204): the values a click writes, how the cells
// paint, the arrow menu with chosen items, and the dates the rules count from.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const core = loadUiModule(UI("sheet-dv-widgets.js"), {}).__EASYOFFICE_SHEET_DV_CORE__;

test("a multiple list cell keeps its chosen items in list order and a check box flips between its values", () => {
  const items = ["가", "나", "다"];
  assert.deepEqual(core.chosenOf(" 나, 가 ,,"), ["나", "가"]);
  assert.equal(core.toggledText(items, "", "나"), "나");
  assert.equal(core.toggledText(items, "나", "가"), "가,나");
  assert.equal(core.toggledText(items, "가,나", "나"), "가");
  assert.equal(core.toggledText(items, "라,나", "가"), "가,나,라", "an item outside the list stays");
  assert.equal(core.toggledText(items, "나", "나"), "");
  assert.deepEqual(core.boxValues({ a: "", b: "" }), ["TRUE", "FALSE"]);
  assert.deepEqual(core.boxValues({ a: "완료", b: "" }), ["완료", "FALSE"]);
  assert.equal(core.nextCheckValue("TRUE", "TRUE", "FALSE"), "FALSE");
  assert.equal(core.nextCheckValue("true", "TRUE", "FALSE"), "FALSE", "case does not matter");
  assert.equal(core.nextCheckValue("FALSE", "TRUE", "FALSE"), "TRUE");
  assert.equal(core.nextCheckValue("", "TRUE", "FALSE"), "TRUE");
  assert.equal(core.nextCheckValue("기타", "TRUE", "FALSE"), "TRUE");
});

const MODEL = {
  sheets: [
    {
      name: "표",
      cells: [
        { ref: "A1", raw: "할 일" },
        { ref: "B1", raw: "완료" },
        { ref: "C1", raw: "분류" },
        { ref: "A2", raw: "가" },
        { ref: "B2", raw: "TRUE" },
        { ref: "A3", raw: "나" },
        { ref: "B3", raw: "FALSE" },
        { ref: "A4", raw: "다" },
        { ref: "B4", raw: "기타" },
        { ref: "A5", raw: "라" },
        { ref: "C2", raw: "나" },
      ],
    },
  ],
};

async function createHarness(dv) {
  const harness = await createWorkspaceHarness();
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(harness.document, makeFixtureFile("체크.xlsx", MODEL));
  await flushAsyncTurns(8);
  const rules = harness.host.__EASYOFFICE_SHEET_RULES__;
  rules.set("표", { cf: [], dv });
  await flushAsyncTurns(10);
  return { ...harness, rules };
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const text = (d, ref) => cell(d, ref).textContent;
// The keys of the page are caught on the viewport before they reach a cell (the real page captures; the test document has no capture phase).
const viewport = (d) => d.getElementById("sheet-viewport");

function click(h, td, x, y) {
  td.getBoundingClientRect = () => ({ left: 0, right: 100, top: 0, bottom: 24 });
  td.dispatchEvent({ type: "click", target: td, button: 0, clientX: x, clientY: y, bubbles: true });
  return flushAsyncTurns(14);
}

test("a check box cell draws its box from the value and hides the two values", async () => {
  const h = await createHarness([{ r: "B2:B5", t: "checkbox", a: "", b: "" }]);
  const d = h.document;
  assert.equal(cell(d, "B2").dataset.dvBox, "on");
  assert.equal(cell(d, "B3").dataset.dvBox, "off");
  assert.equal(cell(d, "B5").dataset.dvBox, "off", "a blank cell shows an empty box");
  assert.equal(cell(d, "B2").classList.contains("dv-box-hide-value"), true);
  assert.equal(cell(d, "B5").classList.contains("dv-box-hide-value"), true);
  assert.equal(cell(d, "B4").classList.contains("dv-box-hide-value"), false, "another value stays visible");
  assert.equal(cell(d, "A2").dataset.dvBox, undefined, "outside the rule");
  // Removing the rule takes the box away.
  h.rules.set("표", { cf: [], dv: [] });
  await flushAsyncTurns(10);
  assert.equal(cell(d, "B2").dataset.dvBox, undefined);
  assert.equal(cell(d, "B2").classList.contains("dv-box-hide-value"), false);
});

test("clicking the box or pressing Space writes the other value", async () => {
  const h = await createHarness([{ r: "B2:B5", t: "checkbox", a: "", b: "" }]);
  const d = h.document;
  await click(h, cell(d, "B2"), 50, 12);
  assert.equal(text(d, "B2"), "FALSE");
  assert.equal(cell(d, "B2").dataset.dvBox, "off");
  assert.equal(d.getElementById("status-command-feedback").textContent, "", "no status text stays after a click on the box");
  await click(h, cell(d, "B3"), 50, 12);
  assert.equal(text(d, "B3"), "TRUE");
  assert.equal(cell(d, "B3").dataset.dvBox, "on");
  // A click away from the box does nothing.
  await click(h, cell(d, "B3"), 95, 12);
  assert.equal(text(d, "B3"), "TRUE");
  // Space on the selected cell.
  cell(d, "B5").focus();
  viewport(d).dispatchEvent({ type: "keydown", key: " ", target: cell(d, "B5"), bubbles: true });
  await flushAsyncTurns(14);
  assert.equal(text(d, "B5"), "TRUE");
  // Each write is its own undo step, and the undo puts the cell back even while rules exist (the
  // restore used to evaluate the rules on the cells of before and keep the new value).
  const undo = d.querySelector('.md-item[data-studio-menu-cmd="studio-undo"]');
  for (const [ref, want] of [["B5", ""], ["B3", "FALSE"], ["B2", "TRUE"]]) {
    h.userClick(undo);
    await flushAsyncTurns(14);
    assert.equal(text(d, ref), want, ref);
  }
});

test("custom values: the box is checked on the checked value only", async () => {
  const h = await createHarness([{ r: "B2:B4", t: "checkbox", a: "TRUE", b: "기타" }]);
  const d = h.document;
  assert.equal(cell(d, "B4").dataset.dvBox, "off");
  assert.equal(cell(d, "B4").classList.contains("dv-box-hide-value"), true, "the unchecked value is hidden");
  await click(h, cell(d, "B4"), 50, 12);
  assert.equal(text(d, "B4"), "TRUE");
  await click(h, cell(d, "B4"), 50, 12);
  assert.equal(text(d, "B4"), "기타");
});

test("a multiple list shows its arrow, marks the chosen items and toggles them in the cell", async () => {
  const h = await createHarness([{ r: "C2:C4", t: "listMultiple", a: "가,나,다" }]);
  const d = h.document;
  cell(d, "C2").focus();
  await flushAsyncTurns(6);
  const button = d.getElementById("sheet-dv-dropdown-btn");
  assert.ok(button && button.hidden === false, "the arrow beside the selected cell");
  cell(d, "C2").getBoundingClientRect = () => ({ left: 0, right: 100, top: 0, bottom: 24 });
  button.getBoundingClientRect = () => ({ left: 80, right: 98, top: 3, bottom: 21 });
  h.userClick(button);
  const menu = d.getElementById("sheet-dv-dropdown-menu");
  assert.equal(menu.hidden, false);
  assert.equal(menu.getAttribute("aria-multiselectable"), "true");
  const rows = () => [...menu.querySelectorAll("li")];
  assert.deepEqual(rows().map((li) => [li.textContent, li.getAttribute("aria-checked")]), [["가", "false"], ["나", "true"], ["다", "false"]]);
  h.userClick(rows()[0]);
  await flushAsyncTurns(14);
  assert.equal(text(d, "C2"), "가,나");
  assert.equal(d.getElementById("status-command-feedback").textContent, "", "no status text stays after a pick");
  assert.equal(menu.hidden, false, "the menu stays open for more choices");
  assert.deepEqual(rows().map((li) => li.getAttribute("aria-checked")), ["true", "true", "false"]);
  h.userClick(rows()[1]);
  await flushAsyncTurns(14);
  assert.equal(text(d, "C2"), "가");
  // A plain list closes after one pick and writes the item.
  h.rules.set("표", { cf: [], dv: [{ r: "C2:C4", t: "list", a: "가,나,다" }] });
  await flushAsyncTurns(10);
  cell(d, "C3").focus();
  await flushAsyncTurns(6);
  h.userClick(button);
  h.userClick(rows()[2]);
  await flushAsyncTurns(14);
  assert.equal(text(d, "C3"), "다");
  assert.equal(menu.hidden, true);
});

test("Alt+Down opens the arrow menu on the selected cell and Escape closes it", async () => {
  // Moving in the menu and picking with the keys is checked on the real page: its capture phase keeps
  // the keys from the cells, which the test document does not have.
  const h = await createHarness([{ r: "C2:C4", t: "listMultiple", a: "가,나,다" }]);
  const d = h.document;
  cell(d, "C3").focus();
  await flushAsyncTurns(6);
  const button = d.getElementById("sheet-dv-dropdown-btn");
  button.getBoundingClientRect = () => ({ left: 80, right: 98, top: 3, bottom: 21 });
  const menu = d.getElementById("sheet-dv-dropdown-menu");
  const key = (k, extra = {}) => viewport(d).dispatchEvent({ type: "keydown", key: k, target: cell(d, "C3"), bubbles: true, ...extra });
  assert.equal(menu.hidden, true);
  key("ArrowDown", { altKey: true });
  assert.equal(menu.hidden, false);
  assert.equal([...menu.querySelectorAll("li")].findIndex((li) => li.getAttribute("aria-selected") === "true"), 0);
  key("Escape");
  assert.equal(menu.hidden, true);
});

test("the rules the page sends carry the local date as a serial number", async () => {
  const h = await createHarness([]);
  const sent = JSON.parse(h.rules.payloadFor("표"));
  const now = new Date();
  const want = Math.round((Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()) - Date.UTC(1899, 11, 30)) / 86400000);
  assert.equal(sent.today, want);
  assert.ok(sent.today > 46000 && sent.today < 60000);
});
