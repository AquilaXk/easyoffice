import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// PD-309: workbook calculation options (`수식 > 계산 옵션`), manual pending recalculation
// (`지금 계산` F9, `시트 계산` Shift+F9, `"계산"` status), undo/redo, open/save round-trip,
// and iterative circular convergence in WebUI.

async function openSheet() {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const converts = [];
  const evaluations = [];
  const testIpc = async (command, payload) => {
    const body = typeof payload === "string" && payload.trimStart().startsWith("{") ? JSON.parse(payload) : null;
    if (command === "convert_document_format" && body && !body.action) converts.push(body);
    if (command === "evaluate_sheet" && body) evaluations.push(body);
    return offline[command](payload);
  };
  const h = await createWorkspaceHarness({ transformHtml: demoHtml, testIpc });
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(6);
  return { h, converts, evaluations };
}

async function load(h, model, name) {
  const bytes = new TextEncoder().encode("EOFIXTURE:" + JSON.stringify(model));
  const res = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(name, bytes);
  await flushAsyncTurns(12);
  assert.equal(res.ok, true);
}

async function save(h, converts) {
  switchModule(h.document, "CONV");
  await flushAsyncTurns(4);
  h.document.getElementById("conv-src-fmt").value = "xlsx";
  h.document.getElementById("conv-dst-fmt").value = "xlsx";
  h.userClick(h.document.getElementById("btn-run-convert"));
  await flushAsyncTurns(12);
  return converts.at(-1);
}

async function editCell(h, ref, text) {
  const td = h.document.querySelector('#pane-sheet td[data-cell="' + ref + '"]');
  assert.ok(td, "cell " + ref + " exists");
  td.focus();
  td.textContent = text;
  td.dispatchEvent({ type: "input", target: td });
  td.dispatchEvent({ type: "keydown", key: "Enter", target: td });
  await flushAsyncTurns(12);
}

test("수식 > 계산 옵션 menu items, aria-checked state, and manual sheet/workbook recalc (F9 / Shift+F9)", async () => {
  const { h, evaluations } = await openSheet();
  const doc = h.document;

  for (const act of ["calc-auto", "calc-auto-no-table", "calc-manual", "calc-now", "calc-sheet"]) {
    assert.ok(doc.querySelector('.md-item[data-sheet-act="' + act + '"]'), act);
  }
  const checkedOf = (act) => doc.querySelector('.md-item[data-sheet-act="' + act + '"]').getAttribute("aria-checked");
  assert.equal(checkedOf("calc-auto"), "true");
  assert.equal(checkedOf("calc-manual"), "false");

  const model = {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "10" },
          { ref: "B1", raw: "=A1*2" },
          { ref: "D1", raw: '=INFO("RECALC")' },
        ],
      },
      {
        name: "Sheet2",
        cells: [{ ref: "A1", raw: "=Sheet1!A1+1" }],
      },
    ],
  };
  await load(h, model, "calc.xlsx");
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="B1"]').textContent, "20");
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="D1"]').textContent, "Automatic");

  // Switch to 수동.
  h.userClick(doc.querySelector('.md-item[data-sheet-act="calc-manual"]'));
  await flushAsyncTurns(10);
  assert.equal(h.host.__EASYOFFICE_SHEET_CALC_OPTIONS__.isManual(), true);
  assert.equal(checkedOf("calc-manual"), "true");
  assert.equal(checkedOf("calc-auto"), "false");

  // Edit A1 -> unedited formula B1 and Sheet2!A1 stay stale; status shows "계산".
  evaluations.length = 0;
  await editCell(h, "A1", "30");
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="B1"]').textContent, "20");
  assert.equal(doc.getElementById("status-command-feedback").textContent, "계산");
  assert.equal(evaluations.length, 0);

  // Enter a new formula C1 = "=A1+5" -> only C1 evaluates; B1 stays at 20.
  await editCell(h, "C1", "=A1+5");
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="C1"]').textContent, "35");
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="B1"]').textContent, "20");

  // Shift+F9 (시트 계산): active Sheet1 formulas update (B1=60, D1=Manual), Sheet2!A1 stays at 11.
  const store = h.host.__EASYOFFICE_WORKSPACE_API__.sheetDataStore ?? null;
  const vp = doc.getElementById("sheet-viewport");
  vp.dispatchEvent({ type: "keydown", key: "F9", shiftKey: true, ctrlKey: false, metaKey: false, altKey: false });
  await flushAsyncTurns(12);
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="B1"]').textContent, "60");
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="D1"]').textContent, "Manual");
  // Switch to Sheet2 to inspect cached A1 before F9 -> still 11.
  h.userClick(doc.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]'));
  await flushAsyncTurns(10);
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="A1"]').textContent, "11");

  // F9 (지금 계산): recalculates every sheet -> Sheet2!A1 becomes 31.
  doc.dispatchEvent({ type: "keydown", key: "F9", shiftKey: false, ctrlKey: false, metaKey: false, altKey: false });
  await flushAsyncTurns(12);
  assert.equal(doc.querySelector('#pane-sheet td[data-cell="A1"]').textContent, "31");

  // Undo mode switch or edit cleanly -> restore back and verify autosave/manual state.
  h.userClick(doc.querySelector('.md-item[data-sheet-act="calc-auto"]'));
  await flushAsyncTurns(10);
  assert.equal(checkedOf("calc-auto"), "true");
  assert.equal(h.host.__EASYOFFICE_SHEET_CALC_OPTIONS__.isManual(), false);
  // Undo back to 수동!
  h.userClick(doc.querySelector('.md-item[data-studio-menu-cmd="studio-undo"]'));
  await flushAsyncTurns(12);
  assert.equal(h.host.__EASYOFFICE_SHEET_CALC_OPTIONS__.isManual(), true);
  assert.equal(checkedOf("calc-manual"), "true");
  void store;
});

test("calcOptions round-trips through open, save, and iterative circular convergence", async () => {
  const { h, converts } = await openSheet();
  const opts = {
    mode: "manual",
    calcOnSave: false,
    fullCalcOnLoad: false,
    iterate: true,
    iterateCount: 100,
    iterateDelta: 0.0001,
  };
  await load(
    h,
    {
      calcOptions: opts,
      sheets: [
        {
          name: "Sheet1",
          cells: [
            { ref: "A1", raw: "=B1+1" },
            { ref: "B1", raw: "=A1/2" },
            { ref: "C1", raw: '=INFO("RECALC")' },
          ],
        },
      ],
    },
    "iter.xlsx"
  );
  const a1 = Number(h.document.querySelector('#pane-sheet td[data-cell="A1"]').textContent);
  const b1 = Number(h.document.querySelector('#pane-sheet td[data-cell="B1"]').textContent);
  assert.ok(Math.abs(a1 - 2) < 1e-3, "A1 converged near 2: " + a1);
  assert.ok(Math.abs(b1 - 1) < 1e-3, "B1 converged near 1: " + b1);
  assert.equal(h.document.querySelector('#pane-sheet td[data-cell="C1"]').textContent, "Manual");

  const saved = await save(h, converts);
  assert.deepEqual(saved.calcOptions, opts);

  // Workbook without calcOptions resets state back to default automatic.
  await load(h, { sheets: [{ name: "Sheet1", cells: [{ ref: "A1", raw: "1" }] }] }, "plain.xlsx");
  assert.equal(h.host.__EASYOFFICE_SHEET_CALC_OPTIONS__.get(), null);
  assert.equal(h.host.__EASYOFFICE_SHEET_CALC_OPTIONS__.isManual(), false);
  const savedPlain = await save(h, converts);
  assert.equal(savedPlain.calcOptions, undefined);
});
