import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";

async function openSheet() {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  return h;
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const col = (d, letter, rows) => rows.map((r) => cell(d, letter + r)?.textContent);

const SORT_SHEET = {
  sheets: [
    {
      name: "Sheet1",
      cells: [
        { ref: "A1", raw: "이름" },
        { ref: "B1", raw: "값" },
        { ref: "A2", raw: "다" },
        { ref: "B2", raw: "30" },
        { ref: "C2", raw: "=B2*2" },
        { ref: "D2", raw: "=B3" },
        { ref: "I2", raw: "x3" },
        { ref: "A3", raw: "가" },
        { ref: "B3", raw: "10" },
        { ref: "C3", raw: "=B3*2" },
        { ref: "I3", raw: "x1" },
        { ref: "A4", raw: "나" },
        { ref: "B4", raw: "20" },
        { ref: "C4", raw: "=B4*2" },
        { ref: "I4", raw: "x2" },
      ],
    },
  ],
};

async function openSortSheet() {
  const h = await openSheet();
  await pickFile(h.document, makeFixtureFile("정렬.xlsx", SORT_SHEET));
  await flushAsyncTurns(12);
  return h;
}

async function sort(h) {
  h.userClick(commandItem(h.document, "sheet-data-sort"));
  await flushAsyncTurns(12);
}

test("sorting without a selection covers every data column, not only A-H, and recalculates formulas", async () => {
  const h = await openSortSheet();
  const { document } = h;
  await sort(h);
  assert.deepEqual(col(document, "B", [1, 2, 3, 4]), ["값", "10", "20", "30"]);
  assert.deepEqual(col(document, "A", [2, 3, 4]), ["가", "나", "다"]);
  assert.deepEqual(col(document, "I", [2, 3, 4]), ["x1", "x2", "x3"], "column I travels with its row");
  assert.deepEqual(col(document, "C", [2, 3, 4]), ["20", "40", "60"], "same-row formulas are recalculated");
  assert.equal(cell(document, "C2").dataset.formula, "=B2*2");
  assert.equal(cell(document, "D4").dataset.formula, "=B5", "a relative reference to another row shifts with the moved row");
  assert.equal(cell(document, "D2").dataset.formula ?? "", "");
});

test("sorting a selection reorders only the selected rows and columns by the first selected column", async () => {
  const h = await openSortSheet();
  const { document } = h;
  const a2 = cell(document, "A2");
  a2.focus();
  key(a2, "ArrowDown", { shiftKey: true });
  key(a2, "ArrowDown", { shiftKey: true });
  key(a2, "ArrowRight", { shiftKey: true });
  await sort(h);
  assert.deepEqual(col(document, "A", [1, 2, 3, 4]), ["이름", "가", "나", "다"]);
  assert.deepEqual(col(document, "B", [2, 3, 4]), ["10", "20", "30"], "B moves with A inside the selection");
  assert.equal(cell(document, "C2").dataset.formula, "=B2*2", "formulas outside the selection stay in place");
  assert.deepEqual(col(document, "C", [2, 3, 4]), ["20", "40", "60"]);
  assert.deepEqual(col(document, "I", [2, 3, 4]), ["x3", "x1", "x2"], "columns outside the selection are untouched");
});

test("sorting again reverses the order", async () => {
  const h = await openSortSheet();
  await sort(h);
  await sort(h);
  assert.deepEqual(col(h.document, "B", [2, 3, 4]), ["30", "20", "10"]);
});
