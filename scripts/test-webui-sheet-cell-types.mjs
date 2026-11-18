import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

test("PD-241: apostrophe-prefixed and explicit-text cells preserve type across grid, formula bar, undo/redo, fill, paste, open, and save", async () => {
  const { document, sandbox, host } = await createWorkspaceHarness();
  switchModule(document, "SHEET");
  await flushAsyncTurns();

  const td = (ref) => document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  const formulaBar = document.getElementById("formula-input");
  const aggBar = document.getElementById("status-sheet-agg");

  // 1. Enter `'123` into A1 and `'=1+1` into A2, and `=SUM(A1:A3)` into B1, `10` into A3.
  const a1 = td("A1");
  a1.focus();
  a1.textContent = "'123";
  a1.dispatchEvent({ type: "input", target: a1 });
  a1.blur();
  await flushAsyncTurns();
  assert.equal(a1.textContent, "123", "leading apostrophe is stripped from displayed cell text");
  assert.equal(a1.classList.contains("num"), false, "text-typed numeric cell stays left-aligned");

  const a2 = td("A2");
  a2.focus();
  a2.textContent = "'=1+1";
  a2.dispatchEvent({ type: "input", target: a2 });
  a2.blur();
  await flushAsyncTurns();
  assert.equal(a2.textContent, "=1+1", "apostrophe-prefixed formula stays literal text");
  assert.equal(a2.dataset.formula, undefined);

  const a3 = td("A3");
  a3.focus();
  a3.textContent = "10";
  a3.dispatchEvent({ type: "input", target: a3 });
  a3.blur();
  await flushAsyncTurns();
  assert.equal(a3.classList.contains("num"), true);

  const b1 = td("B1");
  b1.focus();
  b1.textContent = "=SUM(A1:A3)";
  b1.dispatchEvent({ type: "input", target: b1 });
  b1.blur();
  await flushAsyncTurns();
  assert.equal(b1.textContent, "10", "SUM ignores explicit-text `'123` and `'=1+1`");

  // 2. Selecting A1 shows `'123` in formula bar and excludes A1 from numeric aggregate.
  a1.focus();
  await flushAsyncTurns();
  assert.equal(formulaBar.value, "'123");
  assert.match(aggBar.textContent, /숫자 데이터 없음/);

  // 3. F2 unfolds `'123` into A1; Escape keeps `'123` intact; editing `'123` -> `123` updates SUM to 133, and Undo restores `'123` + SUM 10!
  const vp = document.getElementById("sheet-viewport");
  vp.dispatchEvent({ type: "keydown", key: "F2", target: a1, preventDefault() {} });
  assert.equal(a1.textContent, "'123");
  vp.dispatchEvent({ type: "keydown", key: "Escape", target: a1, preventDefault() {} });
  assert.equal(a1.textContent, "123");
  assert.equal(a1._ct, "s");

  a1.focus();
  vp.dispatchEvent({ type: "keydown", key: "F2", target: a1, preventDefault() {} });
  a1.textContent = "123";
  a1.dispatchEvent({ type: "input", target: a1 });
  a1.blur();
  await flushAsyncTurns();
  assert.equal(a1.classList.contains("num"), true);
  assert.equal(td("B1").textContent, "133");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(td("A1").textContent, "123");
  assert.equal(td("A1").classList.contains("num"), false);
  assert.equal(td("B1").textContent, "10", "Undo restores explicit text kind and recomputes SUM");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(td("B1").textContent, "133", "Redo restores numeric kind and recomputes SUM");

  // 4. Auto-fill and paste core preserve explicit `t: "s"` while plain paste over text clears it.
  const fillCore = host.__EASYOFFICE_SHEET_FILL_CORE__;
  const filled = fillCore.planFill({
    source: { r1: 1, c1: 1, r2: 1, c2: 1 },
    direction: "down",
    count: 2,
    cellAt: () => ({ value: "001", formula: "", t: "s" }),
    shiftFormula: (f) => f,
    refOf: (r, c) => `A${r}`,
  });
  assert.deepEqual(
    filled.edits.map((e) => [e.value, e.t]),
    [["002", "s"], ["003", "s"]]
  );

  const pasteCore = host.__EASYOFFICE_SHEET_PASTE_CORE__;
  const pasted = pasteCore.planPaste({
    snap: { rows: [1], c1: 1, cols: 1, cells: [[{ value: "123", formula: "", t: "s" }]], widths: [96] },
    mode: "values",
    transpose: false,
    target: { r1: 2, c1: 1, r2: 2, c2: 1 },
    getEntry: () => undefined,
    shiftFormula: (f) => f,
    isFormulaStr: (s) => s.startsWith("="),
    grid: { MAX_ROWS: 1048576, MAX_COLS: 16384, DEFAULT_COL_W: 96, refOf: (r) => `A${r}` },
  });
  assert.equal(pasted.edits[0].t, "s");

  // 5. Open + Save payload round-trips `t: "s"` on numeric-looking and formula-looking text cells.
  const h2 = await createWorkspaceHarness();
  await pickFile(
    h2.document,
    makeFixtureFile("타입.xlsx", {
      sheets: [{
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "456", t: "s" },
          { ref: "A2", raw: "=9+9", t: "s" },
          { ref: "A3", raw: "10" },
          { ref: "B1", raw: "=SUM(A1:A3)" },
        ],
      }],
    })
  );
  await flushAsyncTurns(12);
  const td2 = (ref) => h2.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  assert.equal(td2("A1").textContent, "456");
  assert.equal(td2("A1").classList.contains("num"), false);
  assert.equal(td2("A2").textContent, "=9+9");
  assert.equal(td2("B1").textContent, "10");

  const payload = host.__EASYOFFICE_SAVE_PAYLOAD__.buildSheetsPayload(
    ["Sheet1"],
    {
      Sheet1: {
        A1: { value: "456", formula: "", t: "s" },
        A2: { value: "=9+9", formula: "", t: "s" },
        A3: { value: "10", formula: "" },
        A4: { value: "일반텍스트", formula: "", t: "s" },
      },
    },
    {},
    {},
    {}
  );
  assert.deepEqual(payload[0].cells, [
    { ref: "A1", raw: "456", t: "s" },
    { ref: "A2", raw: "=9+9", t: "s" },
    { ref: "A3", raw: "10" },
    { ref: "A4", raw: "일반텍스트" },
  ]);
});
