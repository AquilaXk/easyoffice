// DOM cost of the newer WebUI paths in a real browser (docs/performance.md, "PD-397"): the field and bookmark
// reads of the document window (`doc-word-fields.js`) on a 10,000 paragraph editor with 1,000 bookmarks and 1,000
// cross references, and the lookups of the copilot's real operation of the board (`copilot-real-ext.js`).
//
//   EO_PW_DIR=<dir with playwright-core> PLAYWRIGHT_BROWSERS_PATH=<cache> \
//     node scripts/bench-ui-dom.mjs [--base <git ref>] [--runs 9] [--paras 10000] [--nodes 300] [--json out.json]
//   EO_BROWSER=chromium [EO_CHROMIUM=<executable>] runs the same cases in Chromium instead of WebKit.
//
// The page is built with `setContent`, no engine is needed. Each case is timed inside the page with
// `performance.now()`; the working-tree script and the script of `--base` run in turns on the same DOM, so load
// on the machine slows both alike. The table has the best and the median of each side.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const args = Object.fromEntries(process.argv.slice(2).reduce((acc, cur, i, all) => (cur.startsWith("--") ? [...acc, [cur.slice(2), all[i + 1]]] : acc), []));
const RUNS = Number(args.runs || 9);
const PARAS = Number(args.paras || 10000);
const NODES = Number(args.nodes || 300);
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const uiFile = (name) => path.join(repo, "crates/easyoffice-app/ui", name);
const source = (name, ref) => (ref ? execFileSync("git", ["show", `${ref}:crates/easyoffice-app/ui/${name}`], { cwd: repo, maxBuffer: 1 << 26 }).toString() : fs.readFileSync(uiFile(name), "utf8"));

if (!process.env.EO_PW_DIR) throw new Error("EO_PW_DIR is not set: point it at a directory with playwright-core installed");
const playwright = createRequire(path.join(path.resolve(process.env.EO_PW_DIR), "package.json"))("playwright-core");
const chromium = process.env.EO_BROWSER === "chromium";
const browser = await (chromium ? playwright.chromium.launch(process.env.EO_CHROMIUM ? { executablePath: process.env.EO_CHROMIUM } : {}) : playwright.webkit.launch());
const page = await browser.newPage({ viewport: { width: 1100, height: 700 } });

// The editor: PARAS paragraphs, a bookmark around the text of every tenth, a `REF` field in the paragraph after it, a `SEQ`
// field every hundredth, and a section of plain text between.
const html = [];
for (let i = 0; i < PARAS; i += 1) {
  if (i % 10 === 0) html.push(`<p><span class="eo-bookmark-start" data-eo-bookmark-start="bm_${i}" contenteditable="false"></span>제${i}항 기준<span class="eo-bookmark-end" data-eo-bookmark-end="bm_${i}" contenteditable="false"></span></p>`);
  else if (i % 10 === 1) html.push(`<p>위 내용은 <span data-eo-field-code="REF bm_${i - 1} \\h" contenteditable="false">?</span> 입니다.</p>`);
  else if (i % 100 === 2) html.push(`<p>그림 <span data-eo-field-code="SEQ Figure" contenteditable="false">0</span></p>`);
  else html.push(`<p>${i}. 예산 집행과 <b>성과</b> 점검을 분기별로 나누어 보고한다.</p>`);
}
await page.setContent(`<!doctype html><meta charset="utf-8"><div id="doc-editor">${html.join("")}</div><div id="board-node-layer"></div>`);

// Both versions of the fields module, each kept under its own name.
await page.evaluate(({ paras, nodes }) => {
  globalThis.__bench = { paras, nodes };
}, { paras: PARAS, nodes: NODES });
const versions = [["tree", null]];
if (args.base) versions.unshift(["base", args.base]);
for (const [name, ref] of versions) {
  await page.addScriptTag({ content: source("doc-word-fields.js", ref) });
  await page.evaluate((key) => {
    globalThis.__bench[key] = globalThis.__EASYOFFICE_DOC_WORD_FIELDS__;
  }, name);
}

// The board: NODES cards of the node layer, the way the board draws them.
await page.evaluate((count) => {
  const layer = document.getElementById("board-node-layer");
  for (let i = 0; i < count; i += 1) {
    const card = document.createElement("div");
    card.className = "board-node";
    card.dataset.nodeId = "n" + i;
    card.innerHTML = '<div class="board-node-label">노드</div><div class="board-resize-handle" data-dir="se"></div>';
    layer.append(card);
  }
}, NODES);

const rows = [];
async function measure(label, body, reps = 1) {
  // `body` is the text of a function of `(version)` that runs the case once and returns a short note. The page clock is coarse
  // (a millisecond in WebKit), so a fast case runs `reps` times per sample and the sample is the time of one run.
  const timed = await page.evaluate(
    ({ body, runs, names, reps }) => {
      const work = new Function("v", `return (${body})(v);`);
      const out = {};
      let note = "";
      for (const name of names) note = work(name);
      for (const name of names) out[name] = [];
      for (let i = 0; i < runs; i += 1) {
        for (const name of names) {
          const t0 = performance.now();
          for (let k = 0; k < reps; k += 1) work(name);
          out[name].push((performance.now() - t0) / reps);
        }
      }
      return { out, note };
    },
    { body: body.toString(), runs: RUNS, names: versions.map(([name]) => name), reps },
  );
  const stat = (times) => {
    const sorted = [...times].sort((a, b) => a - b);
    return { best: sorted[0], median: sorted[sorted.length >> 1] };
  };
  const tree = stat(timed.out.tree);
  const base = timed.out.base ? stat(timed.out.base) : null;
  rows.push({ label, tree, base, note: timed.note });
  const f = (s) => `${s.best.toFixed(2).padStart(9)} / ${s.median.toFixed(2).padStart(9)}`;
  console.log(`${label.padEnd(54)} ${base ? f(base) + "  ->" : ""}${f(tree)} ms (best / median)${base ? `  x${(base.best / tree.best).toFixed(2)}` : ""}  ${timed.note}`);
}

console.log(`${chromium ? "Chromium" : "WebKit"} ${browser.version()}, ${PARAS} paragraphs, ${NODES} board nodes`);
await measure("updateFields (1,000 REF + SEQ fields)", (v) => String(globalThis.__bench[v].updateFields(document.getElementById("doc-editor"))), 4);
await measure("visibleBookmarks (bookmark dialog list)", (v) => String(globalThis.__bench[v].visibleBookmarks(document.getElementById("doc-editor")).length), 10);
await measure("bookmarkText of the last bookmark", (v) => String(globalThis.__bench[v].bookmarkText(document.getElementById("doc-editor"), "bm_" + (Math.floor(globalThis.__bench.paras / 10) - 1) * 10).length), 4);
await measure("removeBookmark + add back (a move of a bookmark)", (v) => {
  const root = document.getElementById("doc-editor");
  const start = root.querySelector('[data-eo-bookmark-start="bm_0"]');
  const end = root.querySelector('[data-eo-bookmark-end="bm_0"]');
  const [startParent, startNext] = [start.parentNode, start.nextSibling];
  const [endParent, endNext] = [end.parentNode, end.nextSibling];
  globalThis.__bench[v].removeBookmark(root, "bm_0");
  startParent.insertBefore(start, startNext);
  endParent.insertBefore(end, endNext);
  return "";
}, 10);
// The board card of a node as the real operation finds it, for the last node: the expression that was in `copilot-real-ext.js`
// (every card of the layer into an array, then `find`) and the lookup by the card's id.
await measure("board card lookup, all cards then find (before)", () => {
  const id = "n" + (globalThis.__bench.nodes - 1);
  const found = Array.from(document.querySelectorAll("#board-node-layer .board-node")).find((el) => el.dataset.nodeId === id);
  return found ? "found" : "missing";
}, 200);
await measure("board card lookup, selector by id (after)", () => {
  const id = "n" + (globalThis.__bench.nodes - 1);
  const found = document.querySelector(`#board-node-layer .board-node[data-node-id="${CSS.escape(id)}"]`);
  return found ? "found" : "missing";
}, 200);

await browser.close();
if (args.json) fs.writeFileSync(args.json, JSON.stringify({ paras: PARAS, nodes: NODES, runs: RUNS, base: args.base ?? null, browser: chromium ? "chromium" : "webkit", rows }, null, 2));
