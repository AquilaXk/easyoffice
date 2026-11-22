import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// PD-303: the sheet-wide column width of an XLSX file (`defaultColWidth`, `baseColWidth`) lives in the layout of its sheet, so a
// save sends it back. The page draws every column without a width of its own at 96 px whatever it says.

test("the width of a sheet comes in with the layout, goes out with it and keeps the sheet in the layout", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ S: { defaultColWidth: { width: 20.7109375, base: 10 } }, T: { defaultColWidth: { base: 12 } }, U: {} });
  assert.deepEqual(layout.getAll(), { S: { defaultColWidth: { width: 20.7109375, base: 10 } }, T: { defaultColWidth: { base: 12 } } }, "a sheet with only a width is not empty");
  // What the layout hands out is a copy.
  layout.getAll().S.defaultColWidth.width = 1;
  assert.equal(layout.getAll().S.defaultColWidth.width, 20.7109375);
});

test("something that is not a width object is no width", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ A: { defaultColWidth: null }, B: { defaultColWidth: 20 }, C: { defaultColWidth: "20" }, D: { defaultColWidth: false } });
  assert.deepEqual(layout.getAll(), {});
});

test("the layout the engine answers for an edit does not take the width of the sheet away when the page keeps it", async () => {
  const { host } = await createWorkspaceHarness();
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ S: { defaultColWidth: { width: 15 }, tabColor: "336699" } });
  // The page puts back what the answer does not carry (`sheet-structure.js`).
  const kept = layout.getAll().S;
  layout.setSheet("S", { merges: [], freeze: kept.freeze, tabColor: kept.tabColor, hidden: kept.hidden, defaultColWidth: kept.defaultColWidth });
  assert.deepEqual(layout.getAll().S, { tabColor: "336699", defaultColWidth: { width: 15 } });
});
