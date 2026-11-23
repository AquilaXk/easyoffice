import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem, commandItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

test("WebUI file-open, save-payload, clipboard, paste and sheet-store handle ph metadata", async () => {
  const env = {};
  for (const name of [
    "sheet-utils.js",
    "sheet-refs.js",
    "sheet-look.js",
    "sheet-hidden-rows.js",
    "sheet-paste-core.js",
    "file-open.js",
    "save-payload.js",
    "sheet-store.js",
  ]) {
    loadUiModule(UI(name), env);
  }
  const { buildSheetStore } = env.__EASYOFFICE_FILE_OPEN_UTILS__;
  const { buildSheetsPayload } = env.__EASYOFFICE_SAVE_PAYLOAD__;
  const { planPaste } = env.__EASYOFFICE_SHEET_PASTE_CORE__;

  const { store } = buildSheetStore([
    {
      name: "Sheet1",
      cells: [
        { ref: "A1", raw: "東京都", ph: { r: "トウキョウト", p: [[0, 3, "トウキョウト"]], k: "fk" } },
        { ref: "B1", raw: "=PHONETIC(A1)" },
        { ref: "C1", raw: "大阪府", style: { bold: true } },
      ],
    },
  ]);

  assert.equal(store.Sheet1.A1.ph.r, "トウキョウト");
  assert.equal(store.Sheet1.A1.ph.b, "東京都");
  assert.equal(Object.isFrozen(store.Sheet1.A1.ph), true);

  // Stale `ph` after mutating `.value` directly is ignored by save-payload and sheet-store.
  const sent = [];
  const helpers = env.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: async (_cmd, payload) => {
      sent.push(JSON.parse(payload));
      return { values: { "Sheet1!B1": "トウキョウト" }, circular: false };
    },
    normalizeFormattedNumericCell: (v) => String(v ?? ""),
    tokenizeFormulaStringLiterals: () => [],
  });
  await helpers.computeEvaluatedSheetStore(structuredClone(store), "Sheet1");
  assert.equal(sent[0].cells.find((c) => c.cell === "A1").ph, "トウキョウト");

  const mutated = structuredClone(store);
  mutated.Sheet1.A1 = { value: "神奈川県", formula: "", ph: store.Sheet1.A1.ph };
  assert.equal(helpers.sheetCellDataChanged(store, mutated), true);
  await helpers.computeEvaluatedSheetStore(structuredClone(mutated), "Sheet1");
  assert.equal(sent[1].cells.find((c) => c.cell === "A1").ph, undefined);

  // Save payload strips internal `.b` key while preserving `.ph` for valid cells, and drops stale `.ph`.
  const savedValid = buildSheetsPayload(["Sheet1"], store, {}, {}, {});
  assert.deepEqual(savedValid[0].cells.find((c) => c.ref === "A1").ph, {
    r: "トウキョウト",
    p: [[0, 3, "トウキョウト"]],
    k: "fk",
  });
  const savedMutated = buildSheetsPayload(["Sheet1"], mutated, {}, {}, {});
  assert.equal(savedMutated[0].cells.find((c) => c.ref === "A1").ph, undefined);

  // Paste modes: "all" copies ph, "formats" keeps destination ph, "values" clears ph.
  const allEdits = planPaste({
    snap: { rows: [1], c1: 1, cols: 1, cells: [[{ ...store.Sheet1.A1 }]], widths: [80], merges: [] },
    mode: "all",
    transpose: false,
    target: { r1: 1, c1: 4, r2: 1, c2: 4 },
    getEntry: () => undefined,
    shiftFormula: (f) => f,
    isFormulaStr: (s) => s.startsWith("="),
    grid: env.__EASYOFFICE_SHEET_REFS__,
  }).edits;
  assert.equal(allEdits[0].ph.r, "トウキョウト");

  const fmtEdits = planPaste({
    snap: { rows: [1], c1: 3, cols: 1, cells: [[{ ...store.Sheet1.C1 }]], widths: [80], merges: [] },
    mode: "formats",
    transpose: false,
    target: { r1: 1, c1: 1, r2: 1, c2: 1 },
    getEntry: (ref) => store.Sheet1[ref],
    shiftFormula: (f) => f,
    isFormulaStr: (s) => s.startsWith("="),
    grid: env.__EASYOFFICE_SHEET_REFS__,
  }).edits;
  assert.equal(fmtEdits[0].ph.r, "トウキョウト");

  const valEdits = planPaste({
    snap: { rows: [1], c1: 1, cols: 1, cells: [[{ ...store.Sheet1.A1 }]], widths: [80], merges: [] },
    mode: "values",
    transpose: false,
    target: { r1: 1, c1: 1, r2: 1, c2: 1 },
    getEntry: (ref) => store.Sheet1[ref],
    shiftFormula: (f) => f,
    isFormulaStr: (s) => s.startsWith("="),
    grid: env.__EASYOFFICE_SHEET_REFS__,
  }).edits;
  assert.equal(valEdits[0].ph, null);
});

test("WebUI sheet session mirrors ph across edits, style-only updates, and Ctrl+Z undo", async () => {
  const requests = [];
  const mem = new Map();
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") {
        return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      }
      if (command === "convert_document_format") {
        const body = JSON.parse(args.payload);
        if (body.action === "decode") {
          return {
            status: "OK",
            module: "SHEET",
            format: "XLSX",
            kind: "workbook",
            title: "rph",
            itemCount: 2,
            warnings: [],
            sheets: [
              {
                name: "Sheet1",
                cells: [
                  { ref: "A1", raw: "東京都", ph: { r: "トウキョウト", p: [[0, 3, "トウキョウト"]], k: "fk" } },
                  { ref: "B1", raw: "=PHONETIC(A1)" },
                ],
                columns: [],
              },
            ],
          };
        }
      }
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        requests.push(body);
        if (body.sessionAction === "open") return { session: "s-ph" };
        if (body.sessionAction === "close") return { closed: true };
        if (body.sessionAction === "apply") {
          for (const c of body.changes || []) {
            if (!c.value) mem.delete(c.cell);
            else mem.set(c.cell, c);
          }
          const a1 = mem.get("A1");
          const val = a1 ? (a1.ph || a1.value) : "";
          const changed = mem.has("B1") ? [{ sheet: "Sheet1", cell: "B1", value: val }] : [];
          return { session: "s-ph", changed, spills: [], circular: false };
        }
        const a1 = (body.cells || []).find((c) => c.cell === "A1");
        const val = a1 ? (a1.ph || a1.value) : "";
        return { values: { "Sheet1!B1": val, "Sheet1!A1": a1 ? a1.value : "" }, circular: false };
      }
      return { status: "ok" };
    },
  });

  const { document, host, userClick } = harness;
  const press = (ref) => {
    const td = document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
    td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
    td.dispatchEvent({ type: "mouseup", target: td, bubbles: true });
  };
  switchModule(document, "SHEET");
  await flushAsyncTurns(6);

  // Open payload containing `ph` into the live workspace.
  const openRes = await host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(
    "rph.xlsx",
    new Uint8Array([0x50, 0x4b, 0x03, 0x04]),
  );
  await flushAsyncTurns(20);
  assert.equal(openRes.ok, true);

  assert.equal(document.querySelector('#pane-sheet td[data-cell="B1"]')?.textContent, "トウキョウト");
  assert.ok(requests.some((r) => r.sessionAction === "apply" && r.changes.some((c) => c.cell === "A1" && c.ph === "トウキョウト")));

  // Style-only edit keeps A1 `ph` intact; text edit clears `ph` and Undo brings it back.
  press("A1");
  userClick(commandItem(document, "sheet-home-bold"));
  await flushAsyncTurns(20);
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B1"]')?.textContent, "トウキョウト");

  press("A1");
  const fbar = document.getElementById("formula-input");
  fbar.focus();
  fbar.value = "大阪府";
  fbar.dispatchEvent({ type: "keydown", key: "Enter", target: fbar });
  await flushAsyncTurns(20);
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B1"]')?.textContent, "大阪府");

  userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(20);
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B1"]')?.textContent, "トウキョウト");
});
