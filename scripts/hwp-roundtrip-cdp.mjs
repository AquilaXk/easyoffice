// Real round trip of the HWP open/save path (PD-085): headless Chrome + CDP only (Input.* events
// inside a browser started with its own profile; no OS-level input). The page is the --serve build with
// the generated editor bundle. Fixtures are produced with the vendored rhwp WASM (license-safe, no
// third-party documents), the saved files are re-parsed with the same WASM and compared with the
// expected character shapes, paragraph shapes and table placement.
//
//   node scripts/hwp-roundtrip-cdp.mjs --app <easyoffice-app.exe> --chrome <chrome.exe> [--port 8861] [--out <dir>]
//
// Needs `node scripts/checkout-rhwp.mjs && node scripts/build-hwp-studio.mjs` and a built easyoffice-app.
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { spawn } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = Object.fromEntries(process.argv.slice(2).reduce((acc, cur, i, all) => (cur.startsWith("--") ? [...acc, [cur.slice(2), all[i + 1]]] : acc), []));
const APP = args.app;
const CHROME = args.chrome;
const PORT = Number(args.port || 8861);
const DEBUG_PORT = PORT + 100;
const OUT = args.out || fs.mkdtempSync(path.join(os.tmpdir(), "eo-hwp-roundtrip-"));
const DOWNLOADS = path.join(OUT, "downloads");
fs.mkdirSync(OUT, { recursive: true });
let downloadDir = DOWNLOADS;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- rhwp WASM in Node (fixtures + verification) -----------------------------------------

async function loadWasm() {
  globalThis.measureTextWidth = (font, text) => text.length * 10;
  const mod = await import(pathToFileURL(path.join(REPO, "vendor/rhwp-core/rhwp.js")).href);
  await mod.default({ module_or_path: fs.readFileSync(path.join(REPO, "vendor/rhwp-core/rhwp_bg.wasm")) });
  return mod;
}

function buildFixtureDoc(mod) {
  const doc = mod.HwpDocument.createEmpty();
  doc.createBlankDocument();
  doc.insertText(0, 0, 0, "가나다라마바사 첫 문단입니다");
  doc.splitParagraph(0, 0, doc.getParagraphLength(0, 0));
  doc.insertText(0, 1, 0, "둘째 문단: 기울임과 정렬을 확인합니다");
  doc.applyParaFormat(0, 0, JSON.stringify({ alignment: "center" }));
  doc.applyCharFormat(0, 1, 0, 2, JSON.stringify({ italic: true }));
  doc.splitParagraph(0, 1, doc.getParagraphLength(0, 1));
  doc.createTable(0, 2, 0, 2, 3);
  return doc;
}

function paragraphSignature(doc, para) {
  const len = doc.getParagraphLength(0, para);
  const runs = [];
  for (let i = 0; i < len; i += 1) {
    const c = JSON.parse(doc.getCharPropertiesAt(0, para, i));
    const key = [c.fontFamily, c.fontSize, c.bold, c.italic, c.underline, c.textColor].join("|");
    if (runs.length > 0 && runs[runs.length - 1].key === key) runs[runs.length - 1].len += 1;
    else runs.push({ key, len: 1 });
  }
  const p = JSON.parse(doc.getParaPropertiesAt(0, para));
  const shape = { alignment: p.alignment, lineSpacing: p.lineSpacing, marginLeft: p.marginLeft, indent: p.indent, spacingAfter: p.spacingAfter };
  return { runs, shape };
}

function tableSignature(doc, para) {
  try {
    const dims = JSON.parse(doc.getTableDimensions(0, para, 0));
    const t = JSON.parse(doc.getTableProperties(0, para, 0));
    const place = {};
    for (const k of ["horzRelTo", "vertRelTo", "horzAlign", "vertAlign", "horzOffset", "vertOffset", "treatAsChar", "textWrap", "tableWidth", "tableHeight"]) place[k] = t[k];
    return { dims, place };
  } catch {
    return null;
  }
}

function signature(mod, bytes) {
  const doc = new mod.HwpDocument(bytes);
  const out = { format: doc.getSourceFormat(), paragraphs: [], tables: [] };
  const count = doc.getParagraphCount(0);
  for (let p = 0; p < count; p += 1) {
    out.paragraphs.push(paragraphSignature(doc, p));
    const table = tableSignature(doc, p);
    if (table) out.tables.push({ para: p, ...table });
  }
  doc.free();
  return out;
}

// What the UI edit (first three characters of the first paragraph: 20 pt + bold) must produce.
function expectedAfterEdit(sig) {
  const next = structuredClone(sig);
  const runs = next.paragraphs[0].runs;
  const [font, , , italic, underline, color] = runs[0].key.split("|");
  const edited = { key: [font, "2000", "true", italic, underline, color].join("|"), len: 3 };
  runs[0].len -= 3;
  runs.unshift(edited);
  if (runs[1].len === 0) runs.splice(1, 1);
  return next;
}

function sameStructure(label, actual, expected) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  return { label, same: a === e, ...(a === e ? {} : { actual, expected }) };
}

// ---- CDP ---------------------------------------------------------------------------------

async function connect(wsUrl) {
  const sock = new WebSocket(wsUrl);
  let id = 0;
  const pending = new Map();
  const logs = [];
  const dialogs = [];
  const downloads = [];
  await new Promise((resolve, reject) => { sock.onopen = resolve; sock.onerror = reject; });
  sock.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
    if (m.method === "Runtime.exceptionThrown") logs.push("[exception] " + (m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text));
    if (m.method === "Page.javascriptDialogOpening") {
      dialogs.push(`${m.params.type}: ${m.params.message}`);
      sock.send(JSON.stringify({ id: ++id, method: "Page.handleJavaScriptDialog", params: { accept: true } }));
    }
    if (m.method === "Page.downloadWillBegin") downloads.push(m.params.suggestedFilename);
  };
  const send = (method, params = {}) => new Promise((resolve) => {
    const i = ++id;
    pending.set(i, resolve);
    sock.send(JSON.stringify({ id: i, method, params }));
  });
  const evalJs = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error("page exception: " + (r.result.exceptionDetails.exception?.description || r.result.exceptionDetails.text));
    return r.result?.result?.value;
  };
  return { send, evalJs, logs, dialogs, downloads, close: () => sock.close() };
}

const KEYS = {
  Home: { code: "Home", vk: 36 }, End: { code: "End", vk: 35 }, ArrowRight: { code: "ArrowRight", vk: 39 },
  Enter: { code: "Enter", vk: 13 }, a: { code: "KeyA", vk: 65 },
};

async function pressKey(cdp, name, modifiers = 0, withCtrlHome = false) {
  const k = KEYS[name];
  const base = { key: name, code: k.code, windowsVirtualKeyCode: k.vk, nativeVirtualKeyCode: k.vk, modifiers };
  await cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  if (withCtrlHome) await sleep(30);
}

async function click(cdp, x, y) {
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
}

async function rectOf(cdp, selector, inFrame) {
  const expr = inFrame
    ? `(() => { const f = document.getElementById("hwp-frame"); const fr = f.getBoundingClientRect(); const el = f.contentDocument.querySelector(${JSON.stringify(selector)}); if (!el) return null; const r = el.getBoundingClientRect(); return { x: fr.left + r.left, y: fr.top + r.top, w: r.width, h: r.height }; })()`
    : `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; })()`;
  const rect = await cdp.evalJs(expr);
  if (!rect) throw new Error("element not found: " + selector);
  return rect;
}

const centerOf = (r) => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });

async function openInUi(cdp, name, bytes) {
  const b64 = Buffer.from(bytes).toString("base64");
  const started = Date.now();
  const res = await cdp.evalJs(`(async () => { const bin = atob(${JSON.stringify(b64)}); const u = new Uint8Array(bin.length); for (let i = 0; i < bin.length; i += 1) u[i] = bin.charCodeAt(i); const r = await window.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(${JSON.stringify(name)}, u); return JSON.stringify(r); })()`);
  return { result: JSON.parse(res), ms: Date.now() - started };
}

async function editFirstThreeChars(cdp) {
  const canvas = await rectOf(cdp, "#scroll-container canvas", true);
  await click(cdp, canvas.x + canvas.w / 2, canvas.y + 150);
  await sleep(300);
  await pressKey(cdp, "Home", 2); // Ctrl+Home
  await pressKey(cdp, "Home");
  for (let i = 0; i < 3; i += 1) await pressKey(cdp, "ArrowRight", 8); // Shift+Right
  await sleep(200);
  const size = centerOf(await rectOf(cdp, "#font-size", true));
  await click(cdp, size.x, size.y);
  await pressKey(cdp, "a", 2); // Ctrl+A inside the size field
  await cdp.send("Input.insertText", { text: "20" });
  await pressKey(cdp, "Enter");
  await sleep(300);
  const bold = centerOf(await rectOf(cdp, "#btn-bold", true));
  await click(cdp, bold.x, bold.y);
  await sleep(300);
}

async function clickMenu(cdp, itemSelector) {
  const title = centerOf(await rectOf(cdp, '.menu-item[data-menu="file"] .menu-title', false));
  await click(cdp, title.x, title.y);
  await sleep(200);
  const item = centerOf(await rectOf(cdp, itemSelector, false));
  await click(cdp, item.x, item.y);
}

async function waitForDownload(name, since) {
  const file = path.join(downloadDir, name);
  for (let i = 0; i < 100; i += 1) {
    if (fs.existsSync(file) && !fs.existsSync(file + ".crdownload")) return { file, ms: Date.now() - since };
    await sleep(100);
  }
  throw new Error("download did not arrive: " + name);
}

async function saveVia(cdp, itemSelector, expectedName) {
  const started = Date.now();
  await clickMenu(cdp, itemSelector);
  const got = await waitForDownload(expectedName, started);
  await sleep(200);
  const state = await cdp.evalJs(`JSON.stringify({ dirty: document.getElementById("app").dataset.dirty, feedback: document.getElementById("status-command-feedback").textContent })`);
  return { ...got, ...JSON.parse(state) };
}

// ---- scenarios ---------------------------------------------------------------------------

async function startProcesses() {
  fs.mkdirSync(DOWNLOADS, { recursive: true });
  const server = spawn(APP, ["--serve", String(PORT)], { stdio: "ignore" });
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "eo-chrome-"));
  const chrome = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${DEBUG_PORT}`, `--user-data-dir=${profile}`, "--no-first-run", "--window-size=1440,900", "about:blank"], { stdio: "ignore" });
  let target = null;
  for (let i = 0; i < 100 && !target; i += 1) {
    await sleep(200);
    try {
      const list = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`)).json();
      target = list.find((t) => t.type === "page");
    } catch { /* still starting */ }
  }
  if (!target) throw new Error("Chrome did not start");
  return { server, chrome, wsUrl: target.webSocketDebuggerUrl };
}

async function runFlow(cdp, mod, flow) {
  const outcome = { flow: flow.name, steps: {} };
  const source = flow.bytes;
  const sourceSig = signature(mod, source);
  const opened = await openInUi(cdp, flow.name, source);
  outcome.steps.open = { ok: opened.result.ok, ms: opened.ms, format: opened.result.format };
  await sleep(500);
  await editFirstThreeChars(cdp);
  const dirty = await cdp.evalJs('document.getElementById("app").dataset.dirty');
  outcome.steps.dirtyAfterEdit = dirty;
  const first = await saveVia(cdp, '[data-studio-menu-cmd="studio-save"]', flow.sameExt);
  const second = await saveVia(cdp, `[data-hwp-save-as="${flow.otherFormat}"]`, flow.otherExt);
  outcome.steps.saves = [flow.sameExt, flow.otherExt].map((n, i) => ({ name: n, ms: [first, second][i].ms, dirtyAfter: [first, second][i].dirty, message: [first, second][i].feedback }));
  const expected = expectedAfterEdit(sourceSig);
  outcome.checks = [];
  for (const [file, label] of [[first.file, "same format"], [second.file, "other format"]]) {
    const bytes = fs.readFileSync(file);
    const sig = signature(mod, bytes);
    outcome.checks.push({ file: path.basename(file), bytes: bytes.length, format: sig.format });
    outcome.checks.push(sameStructure(label + ": character shapes", sig.paragraphs.map((p) => p.runs), expected.paragraphs.map((p) => p.runs)));
    outcome.checks.push(sameStructure(label + ": paragraph shapes", sig.paragraphs.map((p) => p.shape), expected.paragraphs.map((p) => p.shape)));
    outcome.checks.push(sameStructure(label + ": table placement", sig.tables, expected.tables));
    const again = await openInUi(cdp, path.basename(file), bytes);
    outcome.checks.push({ label: label + ": reopen in UI", ok: again.result.ok, ms: again.ms });
  }
  return outcome;
}

// A valid PNG made of incompressible noise (stored deflate blocks) so the document stays above 10 MiB.
function noisePng(width, height) {
  const row = width * 3 + 1;
  const raw = crypto.randomBytes(row * height);
  for (let i = 0; i < raw.length; i += row) raw[i] = 0; // PNG filter type "none" per scanline
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const head = Buffer.alloc(4);
    head.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(zlib.crc32(body) >>> 0);
    return Buffer.concat([head, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", ihdr), chunk("IDAT", zlib.deflateSync(raw, { level: 0 })), chunk("IEND", Buffer.alloc(0))]);
}

async function bigOpen(cdp, mod) {
  const doc = buildFixtureDoc(mod);
  doc.insertPicture(0, 0, 0, "", new Uint8Array(noisePng(2000, 1900)), 20000, 19000, 2000, 1900, "png", "noise");
  const bytes = doc.exportHwp();
  doc.free();
  fs.writeFileSync(path.join(OUT, "big.hwp"), bytes);
  const opened = await openInUi(cdp, "big.hwp", bytes);
  await sleep(500);
  const edit = Date.now();
  const saved = await saveVia(cdp, '[data-hwp-save-as="hwp"]', "big.hwp");
  return { bytes: bytes.length, mib: (bytes.length / 1048576).toFixed(1), ok: opened.result.ok, openMs: opened.ms, saveMs: saved.ms, savedBytes: fs.statSync(saved.file).size, sinceEditMs: Date.now() - edit, message: saved.feedback };
}
// Autosave: edit, wait for the debounced IndexedDB write, reload the page, accept the restore prompt
// and check that the restored document is modified again and saves with the same edit.
async function autosaveReload(cdp, mod, bytes, name) {
  const dir = path.join(OUT, "downloads-restore");
  fs.mkdirSync(dir, { recursive: true });
  downloadDir = dir;
  await cdp.send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: dir });
  await openInUi(cdp, name, bytes);
  await sleep(500);
  await editFirstThreeChars(cdp);
  await sleep(7000);
  const idb = `(async () => { const db = await new Promise((res, rej) => { const q = indexedDB.open("easyoffice.hwp.autosave.v1"); q.onsuccess = () => res(q.result); q.onerror = () => rej(q.error); }); const rows = await new Promise((res) => { const r = db.transaction("meta").objectStore("meta").getAll(); r.onsuccess = () => res(r.result); }); return JSON.stringify(rows); })()`;
  const stored = JSON.parse(await cdp.evalJs(idb));
  await cdp.send("Page.navigate", { url: `http://127.0.0.1:${PORT}/?module=HWP&win=cdp-roundtrip` });
  await sleep(9000);
  const state = JSON.parse(await cdp.evalJs(`JSON.stringify({ dirty: document.getElementById("app").dataset.dirty, feedback: document.getElementById("status-command-feedback").textContent, tab: window.__EASYOFFICE_WINDOW_BRIDGE__.instance.getDocumentName("HWP") })`));
  const saved = await saveVia(cdp, '[data-studio-menu-cmd="studio-save"]', name);
  const sig = signature(mod, fs.readFileSync(path.join(dir, name)));
  const expected = expectedAfterEdit(signature(mod, bytes));
  const recordsAfterSave = JSON.parse(await cdp.evalJs(idb));
  return {
    recordBytes: stored.map((r) => r.size), restoredState: state, dirtyAfterSave: saved.dirty, recordsAfterSave: recordsAfterSave.length,
    sameCharacterShapes: JSON.stringify(sig.paragraphs.map((p) => p.runs)) === JSON.stringify(expected.paragraphs.map((p) => p.runs)),
  };
}

async function main() {
  if (!APP || !CHROME) throw new Error("usage: --app <easyoffice-app.exe> --chrome <chrome.exe>");
  const mod = await loadWasm();
  const doc = buildFixtureDoc(mod);
  const hwp = doc.exportHwp();
  const hwpx = doc.exportHwpx();
  doc.free();
  fs.writeFileSync(path.join(OUT, "a-source.hwp"), hwp);
  fs.writeFileSync(path.join(OUT, "b-source.hwpx"), hwpx);
  const procs = await startProcesses();
  const report = { out: OUT, flows: [] };
  let cdp = null;
  try {
    cdp = await connect(procs.wsUrl);
    for (const m of ["Runtime.enable", "Page.enable"]) await cdp.send(m);
    await cdp.send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: DOWNLOADS });
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
    await cdp.send("Page.navigate", { url: `http://127.0.0.1:${PORT}/?module=HWP&win=cdp-roundtrip` });
    await sleep(2500);
    report.flows.push(await runFlow(cdp, mod, { name: "a-source.hwp", bytes: hwp, sameExt: "a-source.hwp", otherFormat: "hwpx", otherExt: "a-source.hwpx" }));
    await cdp.send("Page.navigate", { url: `http://127.0.0.1:${PORT}/?module=HWP&win=cdp-roundtrip` });
    await sleep(2500);
    report.flows.push(await runFlow(cdp, mod, { name: "b-source.hwpx", bytes: hwpx, sameExt: "b-source.hwpx", otherFormat: "hwp", otherExt: "b-source.hwp" }));
    await cdp.send("Page.navigate", { url: `http://127.0.0.1:${PORT}/?module=HWP&win=cdp-roundtrip` });
    await sleep(2500);
    report.big = await bigOpen(cdp, mod);
    await cdp.send("Page.navigate", { url: `http://127.0.0.1:${PORT}/?module=HWP&win=cdp-roundtrip` });
    await sleep(2500);
    report.autosave = await autosaveReload(cdp, mod, hwp, "restore-me.hwp");
    report.dialogs = cdp.dialogs;
    report.pageErrors = cdp.logs;
  } finally {
    cdp?.close();
    procs.chrome.kill();
    procs.server.kill();
  }
  fs.writeFileSync(path.join(OUT, "report.json"), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
  const failed = report.flows.some((f) => f.checks.some((c) => c.same === false || c.ok === false)) || report.big.ok !== true || !report.autosave.sameCharacterShapes || report.autosave.restoredState.dirty !== "true";
  process.exit(failed ? 1 : 0);
}

main().catch((err) => { console.error(err); process.exit(2); });
