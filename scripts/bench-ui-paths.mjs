// Cost of the newer sheet paths of the WebUI on big input (docs/performance.md, "PD-397"), measured in Node
// against the real modules: the subtotal plan, the reference test of the subtotal command and the 이동 옵션
// search (`sheet-subtotal.js`, `sheet-special-cells.js`).
//
//   node scripts/bench-ui-paths.mjs [--rows 50000] [--runs 7] [--base <git ref>] [--json out.json]
//
// Each case runs `runs` times after one warm-up, and the working-tree module is timed next to the module of
// `--base` (a git ref, default none) in turns inside one process, so a busy machine slows both alike. The
// table shows the best and the median time of each side. Without `--base` only the working tree runs.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

const args = Object.fromEntries(process.argv.slice(2).reduce((acc, cur, i, all) => (cur.startsWith("--") ? [...acc, [cur.slice(2), all[i + 1]]] : acc), []));
const ROWS = Number(args.rows || 50000);
const RUNS = Number(args.runs || 7);
const REGION_ROWS = Number(args["region-rows"] || 10000);
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const MODULES = ["sheet-refs.js", "sheet-subtotal.js", "sheet-special-cells.js"];

/** The three modules of the working tree, or of the git ref `ref` (written to a temporary directory and loaded from there). */
function load(ref) {
  const dir = ref ? fs.mkdtempSync(path.join(os.tmpdir(), "eo-bench-ui-")) : path.join(repo, "crates/easyoffice-app/ui");
  const env = {};
  for (const name of MODULES) {
    if (ref) fs.writeFileSync(path.join(dir, name), execFileSync("git", ["show", `${ref}:crates/easyoffice-app/ui/${name}`], { cwd: repo, maxBuffer: 1 << 26 }));
    loadUiModule(path.join(dir, name), env);
  }
  if (ref) fs.rmSync(dir, { recursive: true, force: true });
  return { refs: env.__EASYOFFICE_SHEET_REFS__, subtotal: env.__EASYOFFICE_SHEET_SUBTOTAL__, special: env.__EASYOFFICE_SHEET_SPECIAL_CELLS__ };
}

const mine = load(null);
const base = args.base ? load(args.base) : null;
const { refs } = mine;
const inferTag = (raw) => (raw === "TRUE" || raw === "FALSE" ? "b" : raw.startsWith("#") ? "e" : raw !== "" && !Number.isNaN(Number(raw)) ? "n" : "s");
const results = [];

const stats = (times) => {
  const sorted = [...times].sort((a, b) => a - b);
  return { best: sorted[0], median: sorted[sorted.length >> 1] };
};
const fmt = (s) => `${s.best.toFixed(1).padStart(9)} / ${s.median.toFixed(1).padStart(9)}`;

/** `work(modules)` is timed for the working tree and, when there is a base, for the base, one after the other. */
function measure(label, work) {
  const sides = base ? [["base", base], ["tree", mine]] : [["tree", mine]];
  const times = Object.fromEntries(sides.map(([name]) => [name, []]));
  const notes = {};
  for (const [name, modules] of sides) notes[name] = work(modules);
  for (let i = 0; i < RUNS; i += 1) {
    for (const [name, modules] of sides) {
      const t0 = performance.now();
      work(modules);
      times[name].push(performance.now() - t0);
    }
  }
  const row = { label, tree: stats(times.tree), base: base ? stats(times.base) : null, result: String(notes.tree) };
  results.push(row);
  const ratio = base ? `  x${(row.base.best / row.tree.best).toFixed(2)}` : "";
  console.log(`${label.padEnd(60)} ${base ? fmt(row.base) + "  ->" : ""}${fmt(row.tree)} ms (best / median)${ratio}  ${row.result}`);
}

// ---- the subtotal plan: ROWS rows of 4 cells, a group column of ROWS / 100 groups ------------------------------
const cell = (value) => ({ value: String(value), formula: "" });
const list = [[cell("부서"), cell("항목"), cell("수량"), cell("금액")]];
for (let r = 0; r < ROWS; r += 1) list.push([cell("부서" + Math.floor(r / 100)), cell("항목" + r), cell(r % 97), cell((r * 31) % 10007)]);
const planOptions = (m) => ({ firstRow: 1, firstCol: 1, groupCol: 0, calcCols: [2, 3], fn: m.subtotal.functionOf("sum"), colName: refs.colName });
const planned = mine.subtotal.plan(list, planOptions(mine));
measure(`subtotal.plan ${ROWS} rows / ${ROWS / 100} groups, summaries below`, (m) => m.subtotal.plan(list, planOptions(m)).rowCount);
measure(`subtotal.plan ${ROWS} rows, summaries above`, (m) => m.subtotal.plan(list, { ...planOptions(m), below: false }).rowCount);
measure(`subtotal.plan over a list that holds subtotals (replace)`, (m) => m.subtotal.plan(planned.rows, planOptions(m)).rowCount);
measure(`subtotal.removeAll over the planned list`, (m) => m.subtotal.removeAll(planned.rows).rows.length);

// ---- the reference test of the command: every formula of the workbook against the list's region ----------------
const TEXTS = [
  (r) => `=SUM(C2:C${r + 2})`,
  (r) => `=A${r + 2}*B${r + 2}+Sheet2!D${r + 3}`,
  (r) => `=IF(A${r + 2}>0,"x",'가 나'!E${r + 5})`,
  (r) => `=VLOOKUP(A${r + 2},$H$2:$K$500,3,FALSE)`,
  (r) => `=SUMIFS(M:M,N:N,"a",O${r + 2}:O${r + 9})`,
  (r) => `=A${r + 2}+B${r + 2}`,
  (r) => `=ROUND(C${r + 2}*1.1,0)`,
  (r) => `=IF(ISBLANK(A${r + 2}),0,B${r + 2})`,
];
const formulas = Array.from({ length: ROWS }, (_, r) => TEXTS[r % TEXTS.length](r + 100));
const far = { r1: 1048000, c1: 30, r2: 1048500, c2: 33 };
const near = { r1: 1, c1: 1, r2: ROWS + 1, c2: 4 };
for (const [label, region] of [["no hit", far], ["early hit", near]]) {
  measure(`subtotal.refersInto ${ROWS} formulas (${label})`, (m) => {
    let hits = 0;
    for (const text of formulas) if (m.subtotal.refersInto(text, { sheet: "Sheet1", home: "Sheet1", region, colIndex: refs.colIndex })) hits += 1;
    return `hits ${hits}`;
  });
}

// ---- 이동 옵션: a sheet of ROWS x 10 cells held as `{ A1: entry }` like the page's data store ------------------
const COLS = 10;
// Columns 1-8 are constants, 9-10 formulas; a few cells of column 8 are empty so the used range has blanks.
function sheetOf(rowCount) {
  const cells = {};
  for (let r = 1; r <= rowCount; r += 1) {
    for (let c = 1; c <= COLS; c += 1) {
      if (c === 8 && r % 5000 === 0) continue;
      const ref = refs.colName(c) + r;
      cells[ref] = c >= 9 ? { value: String(r + c), formula: `=A${r}+B${r}` } : { value: c % 3 === 0 ? "텍스트" + r : String(r * c) };
    }
  }
  return cells;
}
const hiddenRows = new Set(Array.from({ length: 40 }, (_, i) => 100 + i * 3));
const notes = Array.from({ length: 50 }, (_, i) => ({ ref: "A" + (i * 900 + 1) }));
const noteSet = new Set(notes.map((n) => n.ref));

// The view of `sheet-goto.js` (`specialView`); `packed` picks how `forEachEntry` reads a key: into an object
// (`parseRef`) or into one number (`packRef`).
function view(cells, usedRows, packed) {
  const at = (row, col) => cells[refs.refOf(row, col)];
  return {
    maxRows: refs.MAX_ROWS,
    maxCols: refs.MAX_COLS,
    usedRows,
    usedCols: COLS,
    forEachEntry: packed
      ? (visit) => {
          for (const ref of Object.keys(cells)) {
            const p = refs.packRef(ref);
            if (p !== 0) visit(Math.floor(p / refs.PACK_STRIDE), p % refs.PACK_STRIDE, cells[ref]);
          }
        }
      : (visit) => {
          for (const ref of Object.keys(cells)) {
            const p = refs.parseRef(ref);
            if (p) visit(p.row, p.col, cells[ref]);
          }
        },
    forEachNote: (visit) => notes.forEach((n) => visit(...Object.values(refs.parseRef(n.ref)))),
    isFilled: (row, col) => {
      const entry = at(row, col);
      return Boolean(entry) && (Boolean(entry.formula) || (entry.value !== undefined && entry.value !== null && entry.value !== ""));
    },
    hasNote: (row, col) => noteSet.has(refs.refOf(row, col)),
    rowHidden: (row) => hiddenRows.has(row),
    colHidden: () => false,
    arrayAt: () => null,
    inferTag,
  };
}
const count = (found) => (found.error ? `error ${found.error}` : `${found.ranges.length} ranges`);
const sheet = sheetOf(ROWS);
const parsed = view(sheet, ROWS, false);
const packed = view(sheet, ROWS, true);
measure(`findSpecial constants, parseRef view, ${ROWS}x${COLS}`, (m) => count(m.special.findSpecial(parsed, { kind: "constants" })));
measure(`findSpecial formulas, parseRef view`, (m) => count(m.special.findSpecial(parsed, { kind: "formulas" })));
measure(`findSpecial formulas (numbers only), parseRef view`, (m) => count(m.special.findSpecial(parsed, { kind: "formulas", types: ["number"] })));
measure(`findSpecial constants, packRef view`, (m) => count(m.special.findSpecial(packed, { kind: "constants" })));
measure(`findSpecial blanks`, (m) => count(m.special.findSpecial(packed, { kind: "blanks" })));
measure(`findSpecial notes`, (m) => count(m.special.findSpecial(packed, { kind: "notes" })));
measure(`findSpecial visible rows`, (m) => count(m.special.findSpecial(packed, { kind: "visible" })));
// The region search grows a box one line at a time; its cost depends on the height of the filled block, so it has its own
// sheet (`--region-rows`, default 10,000).
const regionView = view(sheetOf(REGION_ROWS), REGION_ROWS, true);
measure(`findSpecial region around a cell, ${REGION_ROWS}x${COLS} block`, (m) => count(m.special.findSpecial(regionView, { kind: "region", active: { row: Math.floor(REGION_ROWS / 2), col: 3 } })));
// 100,000 cells that are all found, in a pattern that makes many rectangles, and a block of 200,000.
const keyed = (r, c) => r * 32768 + c;
const pairs = (r, c) => [r, c];
const cellsFor = (make) => ({
  scatter: Array.from({ length: 100000 }, (_, i) => make(i * 2 + 1, 1 + (i % 3))),
  dense: Array.from({ length: 200000 }, (_, i) => make(1 + (i >> 1), 1 + (i & 1))),
});
const keyedCells = cellsFor(keyed);
const pairCells = cellsFor(pairs);
const cellsOf = (m, which) => (m === base ? pairCells[which] : keyedCells[which]);
measure(`rectsOf 100,000 scattered cells`, (m) => m.special.rectsOf(cellsOf(m, "scatter")).length);
measure(`rectsOf 200,000 cells in one block`, (m) => m.special.rectsOf(cellsOf(m, "dense")).length);

console.log(`heap ${(process.memoryUsage().heapUsed / 1048576).toFixed(0)} MB`);
if (args.json) fs.writeFileSync(args.json, JSON.stringify({ rows: ROWS, runs: RUNS, base: args.base ?? null, node: process.version, results }, null, 2));
