import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";
import { acceptProposal } from "./test-support/copilot-accept.mjs";
import { open, answer } from "./test-support/copilot-open.mjs";

// In 수동 계산 mode a formula that is new (typed, pasted, filled) is calculated at once, as in the reference spreadsheet app; the formulas that
// were already there keep their old value until F9 (PD-309).

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
  const h = await createWorkspaceHarness({ transformHtml: demoHtml, testIpc: async (command, payload) => offline[command](payload) });
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(6);
  const model = {
    sheets: [
      {
        name: "Sheet1",
        cells: [
          { ref: "A1", raw: "1" },
          { ref: "A2", raw: "2" },
          { ref: "A3", raw: "3" },
          { ref: "B1", raw: "=A1*10" },
          { ref: "D1", raw: "=A1+100" },
        ],
      },
    ],
  };
  const res = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("manual.xlsx", new TextEncoder().encode("EOFIXTURE:" + JSON.stringify(model)));
  await flushAsyncTurns(12);
  assert.equal(res.ok, true);
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="calc-manual"]'));
  await flushAsyncTurns(10);
  assert.equal(h.host.__EASYOFFICE_SHEET_CALC_OPTIONS__.isManual(), true);
  return h;
}

const text = (h, ref) => h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`).textContent;

async function select(h, range) {
  const box = h.document.getElementById("active-cell-ref");
  box.textContent = range;
  box.dispatchEvent({ type: "keydown", key: "Enter", target: box });
  await flushAsyncTurns(6);
}

test("a formula the copilot writes is calculated at once in manual mode and the old formulas stay stale", async () => {
  let sheetName = "";
  const h = await open({
    module: "SHEET",
    run: (req) => {
      sheetName = req.context.sheet.name;
      return answer([
        { kind: "sheet.write", sheet: sheetName, range: "A1", cells: [{ coord: "A1", raw: "7" }, { coord: "B1", raw: "=A1*3" }] },
      ]);
    },
  });
  h.host.__EASYOFFICE_SHEET_CALC_OPTIONS__.setMode("manual", h.document);
  await h.send("수식 적어 주세요", 20);
  assert.ok(await acceptProposal(h.document, h.userClick));
  const cellText = (ref) => h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`).textContent;
  assert.equal(cellText("A1"), "7");
  assert.equal(cellText("B1"), "21", "the formula the copilot wrote is calculated");
});

test("a pasted formula is calculated at once in manual mode", async () => {
  const h = await openSheet();
  const target = h.document.querySelector('#pane-sheet td[data-cell="F5"]');
  target.focus();
  const clip = { "text/plain": "=A2*100\n=A3*100" };
  target.dispatchEvent({ type: "paste", target, clipboardData: { getData: (t) => clip[t] || "", setData() {} } });
  await flushAsyncTurns(16);
  assert.equal(h.document.querySelector('#pane-sheet td[data-cell="F5"]').dataset.formula, "=A2*100", "the formula was pasted");
  assert.equal(text(h, "F5"), "200", "the pasted formula is calculated");
  assert.equal(text(h, "F6"), "300");
});

test("a filled-down formula is calculated at once in manual mode and the old formulas stay stale", async () => {
  const h = await openSheet();
  // A1 changes: B1 and D1 are stale until F9.
  const a1 = h.document.querySelector('#pane-sheet td[data-cell="A1"]');
  a1.focus();
  a1.textContent = "5";
  a1.dispatchEvent({ type: "input", target: a1 });
  a1.dispatchEvent({ type: "keydown", key: "Enter", target: a1 });
  await flushAsyncTurns(12);
  assert.equal(text(h, "B1"), "10");
  assert.equal(text(h, "D1"), "101");
  await select(h, "B1:B3");
  h.document.getElementById("sheet-viewport").dispatchEvent({ type: "keydown", key: "d", code: "KeyD", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false });
  await flushAsyncTurns(16);
  assert.equal(h.document.querySelector('#pane-sheet td[data-cell="B2"]').dataset.formula, "=A2*10", "the formula was filled");
  assert.equal(text(h, "B2"), "20", "the filled formula is calculated");
  assert.equal(text(h, "B3"), "30");
  assert.equal(text(h, "B1"), "10", "the source formula keeps its old value");
  assert.equal(text(h, "D1"), "101", "an unrelated old formula keeps its old value");
});
