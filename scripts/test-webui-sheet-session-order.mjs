import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The engine resolves a 3D span (`Sheet1:Sheet3!A1`) and a deleted end of one by the order of the sheets in its session, so
// the page opens the session in tab order. The store's own key order is not that order: a name like "2024" is an integer
// key and always comes first.

async function openBook(sheets) {
  const requests = [];
  const model = { sheets: sheets.map(({ name, cells }) => ({ name, cells: Object.entries(cells).map(([ref, raw]) => ({ ref, raw })) })) };
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        requests.push(body);
        if (body.sessionAction === "open") return { session: "s1" };
        if (body.sessionAction === "apply") return { session: body.session, changed: [], circular: false };
        return {};
      }
      if (command === "convert_document_format") {
        return JSON.stringify({ status: "OK", module: "SHEET", format: "xlsx", kind: "workbook", itemCount: model.sheets.length, warnings: [], ...model });
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const result = await harness.sandbox.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("book.xlsx", new TextEncoder().encode("book"));
  await flushAsyncTurns(40);
  assert.equal(result.ok, true);
  return { harness, requests };
}

test("the session opens the sheets in tab order, not in the store's key order", async () => {
  const { harness, requests } = await openBook([
    { name: "Sheet1", cells: { A1: "1" } },
    { name: "2024", cells: { A1: "2" } },
    { name: "Sheet3", cells: { A1: "3", B1: "=SUM(Sheet1:Sheet3!A1)" } },
  ]);
  const key = Object.keys(harness.sandbox.__EASYOFFICE_SHEET_DATA_STORE__ ?? {});
  const opened = requests.find((r) => r.sessionAction === "open");
  assert.ok(opened, "a workbook with a formula opens a session");
  assert.deepEqual(opened.sheets, ["Sheet1", "2024", "Sheet3"], `store keys were ${JSON.stringify(key)}`);
});
