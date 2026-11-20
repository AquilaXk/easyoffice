import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// PD-263: the 1904 date system of a workbook file is kept by the page: a save sends `date1904` back, and a workbook without it
// resets the flag.

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
  const testIpc = async (command, payload) => {
    const body = typeof payload === "string" && payload.trimStart().startsWith("{") ? JSON.parse(payload) : null;
    if (command === "convert_document_format" && body && !body.action) converts.push(body);
    return offline[command](payload);
  };
  const h = await createWorkspaceHarness({ transformHtml: demoHtml, testIpc });
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(6);
  return { h, converts };
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

const sheets = [{ name: "Sheet1", cells: [{ ref: "A1", raw: "43000" }] }];

test("a 1904 workbook is saved with date1904 and a 1900 workbook opened after it is not", async () => {
  const { h, converts } = await openSheet();
  await load(h, { sheets, date1904: true }, "a.xlsx");
  assert.equal(h.host.__EASYOFFICE_SHEET_UTILS__.getDate1904(), true);
  assert.equal((await save(h, converts)).date1904, true);

  await load(h, { sheets }, "b.xlsx");
  assert.equal(h.host.__EASYOFFICE_SHEET_UTILS__.getDate1904(), false);
  assert.equal((await save(h, converts)).date1904, undefined);
});

const AUTOSAVE_KEY = "easyoffice.workspace.autosave.v1";

test("the autosave record and the undo snapshot keep the date system, so a restored workbook saves in 1904 again (PD-264)", async () => {
  const { createMemoryStorage } = await import("./test-support/modules.mjs");
  const storage = createMemoryStorage();
  const h = await createWorkspaceHarness({ localStorage: storage, transformHtml: demoHtml });
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(6);
  h.host.__EASYOFFICE_SHEET_UTILS__.setDate1904(true);
  const td = h.document.querySelector('#pane-sheet td[data-cell="A1"]');
  td.focus();
  td.textContent = "43000";
  td.dispatchEvent({ type: "input", target: td });
  td.dispatchEvent({ type: "keydown", key: "Enter", target: td });
  await flushAsyncTurns(12);
  h.advanceTimers(5000);
  await flushAsyncTurns(10);
  const record = storage.getItem(AUTOSAVE_KEY);
  assert.ok(record, "the record is written");

  const next = await createWorkspaceHarness({ localStorage: storage });
  await flushAsyncTurns(15);
  assert.equal(next.host.__EASYOFFICE_SHEET_UTILS__.getDate1904(), true, "the restored workbook counts from 1904");
});
