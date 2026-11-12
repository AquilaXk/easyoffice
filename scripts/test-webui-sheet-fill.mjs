import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// The fill handle of the sheet window (PD-206): the handle, a drag, a double click, what is written,
// one undo step, and the refusals.

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const text = (d, refs) => refs.map((ref) => cell(d, ref)?.textContent);
const handleOf = (d) => d.querySelector("#sheet-viewport .sheet-fill-handle");
const feedback = (d) => d.getElementById("status-command-feedback").textContent;

const MODEL = {
  sheets: [
    {
      name: "채우기",
      cells: [
        { ref: "A1", raw: "항목1" },
        { ref: "A2", raw: "월" },
        { ref: "B1", raw: "1" },
        { ref: "B2", raw: "2" },
        { ref: "C1", raw: "10", style: { bold: true } },
        { ref: "D1", raw: "=B1*2" },
        { ref: "E1", raw: "7" },
        { ref: "G1", raw: "k1" },
        { ref: "G2", raw: "k2" },
        { ref: "G3", raw: "k3" },
        { ref: "G4", raw: "k4" },
        { ref: "G5", raw: "k5" },
        { ref: "H1", raw: "허브" },
        { ref: "H3", raw: "막음" },
        { ref: "I5", raw: "z5" },
        { ref: "I6", raw: "z6" },
      ],
    },
  ],
};

async function open() {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(h.document, makeFixtureFile("채우기.xlsx", MODEL));
  await flushAsyncTurns(12);
  return h;
}

async function select(d, from, moves) {
  const start = cell(d, from);
  start.dispatchEvent({ type: "mousedown", target: start, button: 0, bubbles: true });
  start.dispatchEvent({ type: "mouseup", target: start, bubbles: true });
  for (const [k, n] of moves) for (let i = 0; i < n; i += 1) key(d.activeElement, k, { shiftKey: true });
  await flushAsyncTurns(2);
}

// The user's drag: the handle is pressed, the pointer passes over `target`, the button is released.
async function drag(d, target, release = {}) {
  const handle = handleOf(d);
  handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
  const td = cell(d, target);
  td.dispatchEvent({ type: "mouseover", target: td, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, bubbles: true, ...release });
  await flushAsyncTurns(15);
}

test("the handle sits at the corner of the selection and is gone for a whole row or column", async () => {
  const { document } = await open();
  await select(document, "B1", [["ArrowDown", 1]]);
  const handle = handleOf(document);
  assert.ok(handle, "the handle exists in the viewport");
  assert.equal(handle.hidden, false);
  const before = [handle.style.left, handle.style.top];
  await select(document, "B1", [["ArrowDown", 3]]);
  assert.notDeepEqual([handle.style.left, handle.style.top], before, "it follows the selection");
  const header = document.querySelector('#pane-sheet th[data-col="B"]');
  header.dispatchEvent({ type: "mousedown", target: header, button: 0, bubbles: true });
  assert.equal(handle.hidden, true, "a whole column has no corner");
});

test("dragging down continues a number series and one undo step takes it back", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "B1", [["ArrowDown", 1]]);
  await drag(document, "B6");
  assert.deepEqual(text(document, ["B1", "B2", "B3", "B4", "B5", "B6", "B7"]), ["1", "2", "3", "4", "5", "6", ""]);
  assert.equal(feedback(document), "채우기: 4칸");
  assert.equal(document.getElementById("active-cell-ref").textContent, "B1", "the anchor stays");
  h.userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(15);
  assert.deepEqual(text(document, ["B1", "B2", "B3", "B6"]), ["1", "2", "", ""], "one undo step");
});

test("dragging counts text that ends in a number, copies one number, and the modifiers swap or force a copy", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "A1", []);
  await drag(document, "A4");
  assert.deepEqual(text(document, ["A1", "A2", "A3", "A4"]), ["항목1", "항목2", "항목3", "항목4"], "the fill replaced the cells it passed");
  await select(document, "E1", []);
  await drag(document, "E3");
  assert.deepEqual(text(document, ["E1", "E2", "E3"]), ["7", "7", "7"], "one number is copied");
  await select(document, "E1", []);
  await drag(document, "E5", { ctrlKey: true });
  assert.deepEqual(text(document, ["E1", "E2", "E3", "E4", "E5"]), ["7", "8", "9", "10", "11"], "with Ctrl the number continues");
  await select(document, "A1", []);
  await drag(document, "A6", { altKey: true });
  assert.deepEqual(text(document, ["A2", "A3", "A4", "A5", "A6"]), ["항목1", "항목1", "항목1", "항목1", "항목1"], "with Alt the text is copied, not counted");
});

test("dragging right copies formulas with moved references and the cell formats along", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "D1", []);
  await drag(document, "D3");
  assert.equal(cell(document, "D2").dataset.formula, "=B2*2");
  assert.equal(cell(document, "D3").dataset.formula, "=B3*2");
  assert.equal(cell(document, "D2").textContent, "4", "recalculated: B2 is 2");
  await select(document, "C1", []);
  await drag(document, "F1");
  assert.deepEqual(text(document, ["C1", "D1", "E1", "F1"]), ["10", "10", "10", "10"]);
  assert.equal(cell(document, "F1").style.fontWeight, "600", "the format travels");
  assert.equal(cell(document, "E1").dataset.formula ?? "", "");
});

test("filling up continues away from the source: the cell above the first one gets the previous value", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "I5", [["ArrowDown", 1]]);
  await drag(document, "I3");
  assert.deepEqual(text(document, ["I3", "I4", "I5", "I6"]), ["z3", "z4", "z5", "z6"]);
});

test("a double click fills down as far as the data beside the selection and stops above data of its own column", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "H1", []);
  const handle = handleOf(document);
  handle.dispatchEvent({ type: "dblclick", target: handle, bubbles: true });
  await flushAsyncTurns(15);
  assert.equal(cell(document, "H2").textContent, "허브", "the column on the left has data down to row 5, H3 holds data");
  assert.equal(cell(document, "H3").textContent, "막음", "it stops before data of the selected column");
  assert.equal(cell(document, "H4").textContent, "");
  await select(document, "C1", []);
  const c = handleOf(document);
  c.dispatchEvent({ type: "dblclick", target: c, bubbles: true });
  await flushAsyncTurns(15);
  assert.equal(cell(document, "C2").textContent, "10", "B has data in row 2 only");
  assert.equal(cell(document, "C2").style.fontWeight, "600");
  assert.equal(cell(document, "C3").textContent, "");
  await select(document, "F8", []);
  const none = handleOf(document);
  none.dispatchEvent({ type: "dblclick", target: none, bubbles: true });
  await flushAsyncTurns(5);
  assert.equal(cell(document, "F9").textContent, "", "no data beside the selection: nothing happens");
});

test("a fill that is too long or crosses a merged range is refused and changes nothing", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "B1", [["ArrowDown", 1]]);
  const far = document.createElement("td");
  far.dataset.cell = "B100005";
  far.setAttribute("contenteditable", "true");
  document.querySelector("#pane-sheet tbody").appendChild(far);
  const handle = handleOf(document);
  handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
  far.dispatchEvent({ type: "mouseover", target: far, bubbles: true });
  far.dispatchEvent({ type: "mouseup", target: far, bubbles: true });
  await flushAsyncTurns(10);
  assert.equal(feedback(document), "채우기 범위 초과");
  assert.deepEqual(text(document, ["B1", "B2", "B3"]), ["1", "2", ""]);
  const layout = document.defaultView.__EASYOFFICE_SHEET_LAYOUT__;
  layout.addMerge("채우기", 5, 2, 6, 2);
  await select(document, "B1", [["ArrowDown", 1]]);
  await drag(document, "B6");
  assert.equal(feedback(document), "병합 셀과 겹침");
  assert.equal(cell(document, "B3").textContent, "");
});

test("Escape while dragging leaves the cells as they are", async () => {
  const h = await open();
  const { document } = h;
  await select(document, "B1", [["ArrowDown", 1]]);
  const handle = handleOf(document);
  handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
  const td = cell(document, "B6");
  td.dispatchEvent({ type: "mouseover", target: td, bubbles: true });
  assert.equal(document.querySelector("#sheet-viewport .sheet-fill-preview").hidden, false, "the outline shows the cells the drag covers");
  document.dispatchEvent({ type: "keydown", key: "Escape", target: document });
  td.dispatchEvent({ type: "mouseup", target: td, bubbles: true });
  await flushAsyncTurns(10);
  assert.equal(cell(document, "B3").textContent, "");
  assert.equal(document.querySelector("#sheet-viewport .sheet-fill-preview").hidden, true);
});
