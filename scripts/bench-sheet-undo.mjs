// Cost of the sheet undo history on a large sheet (docs/performance.md, PD-219).
//
//   node --expose-gc scripts/bench-sheet-undo.mjs [--rows 100000] [--cols 20] [--steps 100] [--json out.json]
//
// A rows x cols sheet of numbers is opened through the offline engine of the test page, then the
// undo history is exercised the way a user does: one cell edit, its undo and redo, a 10,000 cell
// paste with its undo and redo, and `steps` single cell edits whose history is weighed. Each row is
// the wall time of the step and the heap the page holds afterwards (a collection runs first). It runs
// against any checkout, so a before/after pair is two runs.
import fs from "node:fs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

const args = Object.fromEntries(process.argv.slice(2).reduce((acc, cur, i, all) => (cur.startsWith("--") ? [...acc, [cur.slice(2), all[i + 1]]] : acc), []));
const ROWS = Number(args.rows || 100000);
const COLS = Number(args.cols || 20);
const STEPS = Number(args.steps || 100);
const PASTE_ROWS = 500;
const PASTE_COLS = 20;
const MB = 1024 * 1024;

const colName = (c) => (c > 26 ? String.fromCharCode(64 + Math.floor((c - 1) / 26)) : "") + String.fromCharCode(65 + ((c - 1) % 26));
const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });

function heapMb() {
  globalThis.gc?.();
  return process.memoryUsage().heapUsed / MB;
}

// A csv, because the open limit (10 MiB) is far below the JSON a 2,000,000 cell fixture takes;
// the values stay short so that 100,000 rows of 20 fit.
function csvFile() {
  const lines = [];
  for (let r = 1; r <= ROWS; r += 1) {
    const row = [];
    for (let c = 1; c <= COLS; c += 1) row.push((r * 7 + c) % 1000);
    lines.push(row.join(","));
  }
  const bytes = new TextEncoder().encode(lines.join("\n"));
  return { name: "큰시트.csv", size: bytes.length, arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) };
}

const rows = [];
async function step(label, work) {
  const before = heapMb();
  const t0 = performance.now();
  await work();
  const ms = performance.now() - t0;
  const after = heapMb();
  const row = { label, ms: Number(ms.toFixed(1)), heapMb: Number(after.toFixed(1)), deltaMb: Number((after - before).toFixed(1)) };
  rows.push(row);
  console.log(`${label.padEnd(44)} ${String(row.ms).padStart(10)} ms   heap ${String(row.heapMb).padStart(8)} MB   delta ${String(row.deltaMb).padStart(8)} MB`);
}

async function typeInto(h, ref, text) {
  const td = typeof ref === "number" ? h.document.querySelectorAll("#pane-sheet td[data-cell]")[ref] : cell(h.document, ref);
  td.focus();
  td.textContent = text;
  td.dispatchEvent({ type: "input", target: td });
  td.dispatchEvent({ type: "keydown", key: "Enter", target: td });
  await flushAsyncTurns(8);
}

async function undoRedo(h, k) {
  h.sandbox.dispatchWindowKeydown({ key: k, ctrlKey: true });
  await flushAsyncTurns(8);
}

const h = await createWorkspaceHarness();
switchModule(h.document, "SHEET");
await flushAsyncTurns();
const file = csvFile();
await step(`open ${ROWS}x${COLS}`, () => pickFile(h.document, file, 200));
console.log("window cells:", h.document.querySelectorAll("#pane-sheet td[data-cell]").length);

await step("edit one cell", () => typeInto(h, "B5", "9001"));
await step("undo (one cell)", () => undoRedo(h, "z"));
await step("redo (one cell)", () => undoRedo(h, "y"));
await step("undo again (one cell)", () => undoRedo(h, "z"));

// 10,000 cells: A1 to T500 copied and pasted at the first cell the window shows.
const start = cell(h.document, "A1");
start.dispatchEvent({ type: "mousedown", target: start, button: 0, bubbles: true });
start.dispatchEvent({ type: "mouseup", target: start, bubbles: true });
for (let i = 1; i < PASTE_ROWS; i += 1) key(h.document.activeElement, "ArrowDown", { shiftKey: true });
for (let i = 1; i < PASTE_COLS; i += 1) key(h.document.activeElement, "ArrowRight", { shiftKey: true });
await flushAsyncTurns(4);
const clip = {};
const copyTarget = h.document.activeElement;
copyTarget.dispatchEvent({ type: "copy", target: copyTarget, clipboardData: { setData: (t, v) => (clip[t] = v), getData: (t) => clip[t] || "" } });
const at = h.document.querySelector("#pane-sheet td[data-cell]");
at.dispatchEvent({ type: "mousedown", target: at, button: 0, bubbles: true });
at.dispatchEvent({ type: "mouseup", target: at, bubbles: true });
await step(`paste ${PASTE_ROWS * PASTE_COLS} cells`, async () => {
  at.dispatchEvent({ type: "paste", target: at, clipboardData: { getData: (t) => clip[t] || "", setData() {} } });
  await flushAsyncTurns(30);
});
console.log("paste status:", h.document.getElementById("status-command-feedback").textContent);
await step("undo (paste)", () => undoRedo(h, "z"));
await step("redo (paste)", () => undoRedo(h, "y"));
await step("undo again (paste)", () => undoRedo(h, "z"));

const baseline = heapMb();
await step(`${STEPS} single cell edits (history weight)`, async () => {
  for (let i = 0; i < STEPS; i += 1) await typeInto(h, 3 + (i % 40), String(5000 + i));
});
const held = heapMb() - baseline;
console.log(`history of ${STEPS} edits holds ${held.toFixed(1)} MB (${(held / STEPS).toFixed(2)} MB a step)`);
await step("undo (last of the steps)", () => undoRedo(h, "z"));

if (args.json) fs.writeFileSync(args.json, JSON.stringify({ rows: ROWS, cols: COLS, steps: STEPS, results: rows, historyMb: held }, null, 2));
process.exit(0);
