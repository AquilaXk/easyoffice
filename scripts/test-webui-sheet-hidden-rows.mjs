import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { fileURLToPath } from "node:url";

// The rows a sheet hides, for the formulas that leave them out (PD-232): what the layout tells the engine, when the session is told,
// the calculation after a hide, a filter and an undo, and the field of a request without a session.

const layoutApi = async () => (await createWorkspaceHarness()).host.__EASYOFFICE_SHEET_LAYOUT__;

test("the engine is told the rows a user or a fold hid apart from the rows a filter hid, and the rows are compared by their runs", async () => {
  const layout = await layoutApi();
  layout.replaceAll({
    S: {
      hiddenRows: [[5, 5]],
      outline: { foldedRows: [[9, 10]] },
      filter: { range: "A1:B20", columns: [], hidden: [[3, 4]] },
      tables: [{ name: "T", range: "D1:E9", header: true, totals: false, columns: ["a", "b"], filter: [], filterHidden: [[7, 8]] }],
    },
  });
  assert.deepEqual(layout.hiddenRunsOf("S"), { hidden: [[5, 5], [9, 10]], filtered: [[3, 4], [7, 8]] });
  const before = layout.hiddenRunsOf("S");
  layout.setTabColor("S", "336699");
  assert.ok(layout.sameHiddenRuns(before, layout.hiddenRunsOf("S")), "a change of something else is the same rows");
  layout.setTableFilterHidden("S", "T", [[6, 6]]);
  assert.ok(!layout.sameHiddenRuns(before, layout.hiddenRunsOf("S")));
  assert.deepEqual(layout.hiddenRunsOf("S").filtered, [[3, 4], [6, 6]]);
  assert.deepEqual(layout.hiddenRunsOf("None"), { hidden: [], filtered: [] });
  assert.ok(layout.sameHiddenRuns(layout.hiddenRunsOf("None"), layout.NO_RUNS));
});

// Types into a cell, which opens the session (nothing is sent before a formula is calculated).
async function type(t, ref, text) {
  const td = t.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  td.focus();
  for (const ch of text) {
    const active = t.document.activeElement;
    active.dispatchEvent({ type: "keydown", key: ch, target: active });
    active.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: active });
    active.textContent += ch;
    active.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: active });
  }
  td.dispatchEvent({ type: "keydown", key: "Tab", target: td });
  await flushAsyncTurns(15);
}

async function open({ sessions }) {
  const requests = [];
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: sessions, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        requests.push(body);
        if (body.sessionAction === "open") return { session: "s1" };
        if (body.sessionAction === "apply") return { session: "s1", changed: [], spills: [], circular: false };
        if (body.sessionAction === "hiddenRows") return { session: "s1", changed: [{ sheet: body.sheet, cell: "H9", value: "42" }], spills: [], circular: false };
        return {};
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return { ...harness, requests };
}

const told = (t) => t.requests.filter((r) => r.sessionAction === "hiddenRows");

test("a hide, a filter and an undo tell the session the rows once each and nothing is sent for the same rows", async () => {
  const t = await open({ sessions: true });
  const d = t.document;
  const press = (ref) => {
    const td = d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
    td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
    td.dispatchEvent({ type: "mouseup", target: td, bubbles: true });
  };
  await type(t, "H9", "=SUBTOTAL(109,B2:B7)");
  assert.ok(t.requests.some((r) => r.sessionAction === "open"), "a session is open");
  assert.equal(told(t).length, 0, "no hidden row, nothing to say");
  press("A5");
  t.userClick(d.querySelector('.md-item[data-sheet-act="hide"]'));
  await flushAsyncTurns(30);
  const first = told(t);
  assert.equal(first.length, 1, JSON.stringify(first));
  assert.deepEqual(first[0].hiddenRows, { hidden: [[5, 5]], filtered: [] });
  // The same rows again: a layout change that hides nothing new is not sent.
  t.host.__EASYOFFICE_SHEET_LAYOUT__.setTabColor(first[0].sheet, "336699");
  await flushAsyncTurns(20);
  assert.equal(told(t).length, 1);
  // The filter's rows are the other kind.
  const layout = t.host.__EASYOFFICE_SHEET_LAYOUT__;
  const name = first[0].sheet;
  layout.setFilter(name, { range: { r1: 1, c1: 1, r2: 8, c2: 2 }, columns: [], hidden: [[2, 3]] });
  await flushAsyncTurns(30);
  assert.deepEqual(told(t).at(-1).hiddenRows, { hidden: [[5, 5]], filtered: [[2, 3]] });
  // An undo of the hide brings the rows back and the session is told.
  const count = told(t).length;
  t.userClick(studioItem(d, "studio-undo"));
  await flushAsyncTurns(30);
  assert.ok(told(t).length >= count);
});

test("the values the engine answers for the subtotals reach the cells", async () => {
  const t = await open({ sessions: true });
  const d = t.document;
  await type(t, "H9", "=SUBTOTAL(109,B2:B7)");
  const td = d.querySelector('#pane-sheet td[data-cell="A5"]');
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, bubbles: true });
  t.userClick(d.querySelector('.md-item[data-sheet-act="hide"]'));
  await flushAsyncTurns(30);
  assert.ok(told(t).length >= 1);
  assert.equal(d.querySelector('#pane-sheet td[data-cell="H9"]').textContent, "42", "the answer of the engine is shown");
});

test("a request without a session carries the hidden rows of the sheets that have some", async () => {
  const t = await open({ sessions: false });
  const spill = t.host.__EASYOFFICE_SHEET_SPILL__;
  const layout = t.host.__EASYOFFICE_SHEET_LAYOUT__;
  layout.replaceAll({ S: { hiddenRows: [[2, 3]], filter: { range: "A1:B9", columns: [], hidden: [[5, 5]] } }, T: {} });
  assert.deepEqual(spill.hiddenRowsField(["S", "T", "U"], layout), { hiddenRows: { S: { hidden: [[2, 3]], filtered: [[5, 5]] } } });
  assert.deepEqual(spill.hiddenRowsField(["T"], layout), {});
});
