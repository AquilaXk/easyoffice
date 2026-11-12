// WebUI cost per screen path, measured in a real headless browser against the real engine
// (docs/performance.md, "WebUI"). Not part of the fast CI.
//
//   node scripts/bench-webui.mjs --app <easyoffice-app> --chrome <browser> [--ui <ui dir>] [--only sheet,doc,slide,common]
//        [--rows 100000 --cols 20] [--paras 10000] [--slides 300] [--json out.json]
//   --only numfmt: a 5,000 x 20 sheet whose cells all carry a number format (bench-webui-numfmt.mjs).
//
// Each row is one user action: wall time, then the browser's own counters for it (script, layout and style
// recalculation time, layout and recalc counts), DOM nodes and JS heap afterwards. `ui` overrides the served
// directory, so a before/after pair runs the same scenarios against two checkouts.
import fs from "node:fs";
import path from "node:path";
import { REPO, sleep, startEngine, startUiServer, startBrowser, measure } from "./bench-webui-support.mjs";
import { profile } from "./bench-webui-actions.mjs";
import { makeWorkbook, documentContent, slidesContent, encodeWithEngine } from "./bench-webui-fixtures.mjs";

const args = Object.fromEntries(process.argv.slice(2).reduce((acc, cur, i, all) => (cur.startsWith("--") ? [...acc, [cur.slice(2), all[i + 1]]] : acc), []));
const ENGINE_PORT = Number(args.port || 18461);
const ROWS = Number(args.rows || 100000);
const COLS = Number(args.cols || 20);
const PARAS = Number(args.paras || 10000);
const SLIDES = Number(args.slides || 300);
const ONLY = new Set(String(args.only || "sheet,doc,slide,common").split(","));
const UI = path.resolve(args.ui || path.join(REPO, "crates/easyoffice-app/ui"));

const frames = (n = 2) => `new Promise((r) => { let k = ${n}; const f = () => (--k > 0 ? requestAnimationFrame(f) : r()); requestAnimationFrame(f); })`;

// Counters the page-side probes fill: selector scans, listener adds/removes, forced layout reads.
const PROBE = `(() => {
  if (window.__probe) return;
  const p = (window.__probe = { qsa: 0, qsaNodes: 0, qs: 0, add: 0, remove: 0, reads: 0, ipc: [] });
  const wrapQuery = (proto, name, key) => {
    const orig = proto[name];
    proto[name] = function (...a) { p[key] += 1; const r = orig.apply(this, a); if (key === "qsa") p.qsaNodes += r.length; return r; };
  };
  for (const proto of [Document.prototype, Element.prototype]) { wrapQuery(proto, "querySelectorAll", "qsa"); wrapQuery(proto, "querySelector", "qs"); }
  const add = EventTarget.prototype.addEventListener;
  const remove = EventTarget.prototype.removeEventListener;
  EventTarget.prototype.addEventListener = function (...a) { p.add += 1; return add.apply(this, a); };
  EventTarget.prototype.removeEventListener = function (...a) { p.remove += 1; return remove.apply(this, a); };
  for (const name of ["getBoundingClientRect", "getClientRects"]) {
    const orig = Element.prototype[name];
    Element.prototype[name] = function (...a) { p.reads += 1; return orig.apply(this, a); };
  }
  for (const name of ["offsetHeight", "offsetWidth", "offsetTop", "offsetLeft", "clientHeight", "clientWidth", "scrollHeight", "scrollWidth"]) {
    const d = Object.getOwnPropertyDescriptor(Element.prototype, name) || Object.getOwnPropertyDescriptor(HTMLElement.prototype, name);
    if (!d || !d.get) continue;
    Object.defineProperty(Element.prototype, name, { configurable: true, get() { p.reads += 1; return d.get.call(this); } });
  }
  const origFetch = window.fetch;
  window.fetch = async function (url, init) {
    const t0 = performance.now();
    const res = await origFetch.call(this, url, init);
    if (!String(url).startsWith("/api/")) return res;
    const out = res.clone();
    out.arrayBuffer().then(() => p.ipc.push({ start: t0, end: performance.now(), cmd: String(init?.body || "").slice(0, 60) }));
    return res;
  };
})()`;
const probeReset = `(() => { const p = window.__probe; p.qsa = p.qsaNodes = p.qs = p.add = p.remove = p.reads = 0; p.ipc = []; })()`;
const probeRead = `JSON.stringify((({ qsa, qsaNodes, qs, add, remove, reads, ipc }) => ({ qsa, qsaNodes, qs, add, remove, reads, ipcCalls: ipc.length, ipcEnd: ipc.length ? Math.max(...ipc.map((x) => x.end)) : 0 }))(window.__probe))`;

const rows = [];
let lastProfile = null;
/** `measure`, and with --profile the top self-time functions of the step (printed by `record`). */
async function measureStep(cdp, step) {
  if (!args.profile) return measure(cdp, step);
  let value;
  const m = await measure(cdp, async () => {
    lastProfile = await profile(cdp, async () => { value = await step(); }, Number(args.profile) || 10);
    return value;
  });
  return m;
}

function record(area, name, m, extra = {}) {
  const row = { area, name, ...Object.fromEntries(Object.entries(m).filter(([k]) => k !== "value")), ...extra };
  rows.push(row);
  const f = (v) => (typeof v === "number" ? v.toFixed(1) : v);
  console.log(`${area.padEnd(6)} ${name.padEnd(44)} wall ${f(row.wallMs).padStart(8)}  busy ${f(row.taskMs).padStart(8)}  script ${f(row.scriptMs).padStart(7)}  layout ${f(row.layoutMs).padStart(7)} x${String(row.layouts).padEnd(4)} style ${f(row.styleMs).padStart(7)} x${String(row.recalcs).padEnd(4)} nodes ${row.nodes}  heap ${f(row.heapMB)}MB` + (extra.note ? "  " + extra.note : ""));
  if (lastProfile) { for (const line of lastProfile) console.log("         " + line); lastProfile = null; }
}

async function main() {
  const engine = startEngine(args.app, ENGINE_PORT);
  cleanups.push(() => engine.kill());
  await sleep(800);
  const cache = path.join(process.env.BENCH_CACHE || "/tmp", "eo-webui-bench");
  fs.mkdirSync(cache, { recursive: true });
  const server = await startUiServer({ enginePort: ENGINE_PORT, port: ENGINE_PORT + 1, ui: UI, inputs: cache });
  const invokeEngine = async (cmd, payload) => {
    const r = await fetch(`http://127.0.0.1:${ENGINE_PORT}/api/ipc`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ command: cmd, payload }) });
    return r.json();
  };
  const cached = async (name, make) => {
    const file = path.join(cache, name);
    if (!fs.existsSync(file)) fs.writeFileSync(file, await make());
    return file;
  };
  let browser = null;
  try {
    const sheetFile = ONLY.has("sheet") ? await cached(`sheet-${ROWS}x${COLS}.xlsx`, () => makeWorkbook(ROWS, COLS)) : null;
    const docFile = ONLY.has("doc") ? await cached(`doc-${PARAS}.docx`, async () => encodeWithEngine(invokeEngine, documentContent(PARAS), "docx")) : null;
    const slideFile = ONLY.has("slide") ? await cached(`slides-${SLIDES}.pptx`, async () => encodeWithEngine(invokeEngine, slidesContent(SLIDES), "pptx")) : null;
    for (const f of [sheetFile, docFile, slideFile].filter(Boolean)) console.log("input", path.basename(f), (fs.statSync(f).size / 1048576).toFixed(2), "MiB");

    const fresh = async (module = "DOC") => {
      if (browser) await browser.stop();
      browser = await startBrowser({ chrome: args.chrome, debugPort: ENGINE_PORT + 2, url: `http://127.0.0.1:${ENGINE_PORT + 1}/?module=${module}&win=bench`, preload: PROBE });
      cleanups.push(browser.kill);
      return browser.cdp;
    };
    const ctx = { fresh, frames, probeReset, probeRead, record, measure: measureStep, sleep, invokeEngine, files: { sheetFile, docFile, slideFile }, args: { ROWS, COLS, PARAS, SLIDES } };
    if (ONLY.has("sheet")) await (await import("./bench-webui-sheet.mjs")).run(ctx);
    if (ONLY.has("numfmt")) await (await import("./bench-webui-numfmt.mjs")).run(ctx);
    if (ONLY.has("doc")) await (await import("./bench-webui-doc.mjs")).run(ctx);
    if (ONLY.has("slide")) await (await import("./bench-webui-doc.mjs")).runSlides(ctx);
    if (ONLY.has("common")) await (await import("./bench-webui-common.mjs")).run(ctx);
  } finally {
    if (browser) await browser.stop();
    server.close();
    engine.kill();
  }
  if (args.json) fs.writeFileSync(args.json, JSON.stringify(rows, null, 2));
}

const cleanups = [];
process.on("uncaughtException", (err) => { console.error(err); for (const fn of cleanups) fn(); process.exit(1); });
main().then(() => process.exit(0), (err) => { console.error(err); process.exit(1); });
