#!/usr/bin/env node
/**
 * Standalone browser harness for the built hwp-studio bundle (PD-081).
 *
 * Serves crates/easyoffice-app/ui/hwp-studio under /hwp-studio/ with the CSP planned for the
 * embedded editor, hosts it in a same-origin <iframe> of a tiny parent page that speaks the
 * rhwp embed protocol over a MessagePort, and drives a headless Chrome over the DevTools
 * protocol with real input events:
 *   blank document -> type Korean text -> change the font size of a partial selection only ->
 *   exportHwp -> reload the bytes (vendored WASM in Node and the editor itself) -> same character shapes.
 *
 * Usage: node scripts/hwp-studio-harness.mjs [--keep-profile]
 * Chrome is located via CHROME_PATH or the usual install locations (absolute paths only).
 */

import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
const BUNDLE = path.join(REPO, "crates/easyoffice-app/ui/hwp-studio");
const WASM_JS = path.join(REPO, "vendor/rhwp-core/rhwp.js");
const WASM_BIN = path.join(REPO, "vendor/rhwp-core/rhwp_bg.wasm");

export const STUDIO_CSP =
  "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; img-src 'self' data: blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'self'";
const PARENT_CSP = "default-src 'self'; frame-src 'self'; object-src 'none'; base-uri 'none'";
const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".woff2": "font/woff2",
  ".txt": "text/plain; charset=utf-8",
  ".json": "application/json",
};

const TYPED_TEXT = "안녕하세요 한글 편집기";
const SELECTED_CHARS = 3;
const BIG_PT = 20;

const PARENT_HTML = `<!doctype html><html lang="ko"><head><meta charset="utf-8"><title>harness</title>
<link rel="stylesheet" href="/harness.css"></head><body>
<iframe id="studio" title="studio" src="/hwp-studio/index.html"></iframe>
<script src="/harness.js"></script></body></html>`;
const PARENT_CSS = "html,body{margin:0;height:100%}#studio{border:0;width:1280px;height:800px}";
const PARENT_JS = `
const frame = document.getElementById('studio');
const channel = new MessageChannel();
const sessionId = crypto.randomUUID();
let nextId = 1;
const pending = new Map();
let onConnected;
const connected = new Promise((resolve) => { onConnected = resolve; });
const events = [];
channel.port1.onmessage = ({ data }) => {
  if (data.type === 'rhwp-connected') onConnected(data);
  else if (data.type === 'rhwp-event') events.push(data);
  else if (data.type === 'rhwp-response') {
    const waiter = pending.get(data.id);
    pending.delete(data.id);
    if (data.error) waiter.reject(new Error(data.error.message));
    else waiter.resolve(data.result);
  }
};
frame.addEventListener('load', () => {
  frame.contentWindow.postMessage({
    type: 'rhwp-connect', version: 1, sessionId,
    capabilities: ['transferable-array-buffer', 'document-state-v1', 'selection-context-v1', 'document-change-events-v1'],
  }, location.origin, [channel.port2]);
});
function call(method, params) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    channel.port1.postMessage({ type: 'rhwp-request', version: 1, sessionId, id, method, params });
  });
}
function toBase64(bytes) {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}
function fromBase64(text) {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}
window.hwp = {
  connected, events, call,
  async exportBase64() { return toBase64(await call('exportHwp')); },
  async loadBase64(text, fileName) { const data = fromBase64(text); return call('loadFile', { data, fileName, skipUnsavedGuard: true }); },
};
`;

/** Absolute path of a bundle file for a URL-encoded relative path, or null when it escapes BUNDLE. */
function resolveBundleFile(encodedRel) {
  let rel;
  try {
    rel = decodeURIComponent(encodedRel);
  } catch {
    return null;
  }
  if (rel.includes("\0")) return null;
  const file = path.resolve(BUNDLE, rel);
  const within = path.relative(BUNDLE, file);
  if (!within || within.startsWith("..") || path.isAbsolute(within)) return null;
  return file;
}

export function startServer() {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://127.0.0.1");
    const send = (status, type, body, csp) => {
      res.writeHead(status, { "Content-Type": type, "Content-Security-Policy": csp, "X-Content-Type-Options": "nosniff" });
      res.end(body);
    };
    if (url.pathname === "/harness.html") return send(200, MIME[".html"], PARENT_HTML, PARENT_CSP);
    if (url.pathname === "/harness.js") return send(200, MIME[".js"], PARENT_JS, PARENT_CSP);
    if (url.pathname === "/harness.css") return send(200, MIME[".css"], PARENT_CSS, PARENT_CSP);
    if (url.pathname.startsWith("/hwp-studio/")) {
      const file = resolveBundleFile(url.pathname.slice("/hwp-studio/".length));
      if (!file || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
        return send(404, "text/plain", "not found", STUDIO_CSP);
      }
      return send(200, MIME[path.extname(file)] ?? "application/octet-stream", fs.readFileSync(file), STUDIO_CSP);
    }
    return send(404, "text/plain", "not found", PARENT_CSP);
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

export function findChrome() {
  const candidates = [
    process.env.CHROME_PATH,
    String.raw`C:\Program Files\Google\Chrome\Application\chrome.exe`,
    String.raw`C:\Program Files (x86)\Google\Chrome\Application\chrome.exe`,
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
  ].filter((candidate) => candidate && path.isAbsolute(candidate));
  const found = candidates.find((candidate) => fs.existsSync(candidate));
  if (!found) throw new Error("Chrome not found; set CHROME_PATH to an absolute path");
  return found;
}

const CHROME_ENDPOINT_TIMEOUT_MS = 60000;
const CHROME_LAUNCH_ATTEMPTS = 3;

/** Chrome writes "<port>\n<browser path>" to DevToolsActivePort once the endpoint is live. */
function readActivePortFile(profileDir) {
  try {
    const [port, wsPath] = fs.readFileSync(path.join(profileDir, "DevToolsActivePort"), "utf8").split(/\r?\n/);
    if (/^\d+$/.test(port) && wsPath?.startsWith("/devtools/browser/")) return `ws://127.0.0.1:${port}${wsPath}`;
  } catch {
    // Not written yet.
  }
  return null;
}

function launchChrome(profileDir) {
  const child = spawn(
    findChrome(),
    ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profileDir}`, "--no-first-run", "--disable-gpu", "--window-size=1280,800", "about:blank"],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  // The endpoint is taken from stderr or from the DevToolsActivePort file, whichever appears
  // first: on loaded CI runners the stderr line can arrive late or not at all.
  const endpoint = new Promise((resolve, reject) => {
    let buffer = "";
    let settled = false;
    const finish = (fn, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      clearInterval(poll);
      fn(value);
    };
    const timer = setTimeout(() => finish(reject, new Error("Chrome did not report a DevTools endpoint")), CHROME_ENDPOINT_TIMEOUT_MS);
    const poll = setInterval(() => {
      const fromFile = readActivePortFile(profileDir);
      if (fromFile) finish(resolve, fromFile);
    }, 250);
    child.stderr.on("data", (chunk) => {
      buffer += chunk.toString();
      const m = /DevTools listening on (ws:\/\/[^\s]+)/.exec(buffer);
      if (m) finish(resolve, m[1]);
    });
    child.on("exit", (code) => finish(reject, new Error(`Chrome exited early (${code})`)));
  });
  return { child, endpoint };
}

/** Best-effort removal of a temporary Chrome profile; a profile still locked by an exiting Chrome
 * must not turn a passing harness run into a failure. */
function removeProfile(profileDir, log) {
  try {
    fs.rmSync(profileDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 });
  } catch (err) {
    log(`note: temporary profile not removed (${err.code || err.message}): ${profileDir}`);
  }
}

/** Launches Chrome with a fresh profile, retrying when the DevTools endpoint never appears. */
async function startChrome(log) {
  let lastError = null;
  for (let attempt = 1; attempt <= CHROME_LAUNCH_ATTEMPTS; attempt += 1) {
    const profile = fs.mkdtempSync(path.join(os.tmpdir(), "eo-hwp-harness-"));
    const chrome = launchChrome(profile);
    try {
      const browserWs = await chrome.endpoint;
      return { chrome, profile, browserWs };
    } catch (err) {
      lastError = err;
      log(`Chrome start attempt ${attempt}/${CHROME_LAUNCH_ATTEMPTS} failed: ${err.message}`);
      chrome.child.kill();
      await sleep(500);
      removeProfile(profile, log);
    }
  }
  throw lastError;
}

class Cdp {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = [];
    this.opened = new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve);
      this.ws.addEventListener("error", () => reject(new Error("DevTools connection failed")));
    });
    this.ws.addEventListener("message", ({ data }) => {
      const msg = JSON.parse(data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(`${msg.error.message}`));
        else resolve(msg.result);
      } else if (msg.method) {
        for (const listener of this.listeners) listener(msg);
      }
    });
  }

  send(method, params, sessionId) {
    const id = this.nextId++;
    this.ws.send(JSON.stringify({ id, method, params: params ?? {}, ...(sessionId ? { sessionId } : {}) }));
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
  }

  onEvent(listener) {
    this.listeners.push(listener);
  }

  close() {
    this.ws.close();
  }
}

async function evaluate(cdp, session, expression) {
  const res = await cdp.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }, session);
  if (res.exceptionDetails) throw new Error(`page error: ${res.exceptionDetails.exception?.description ?? res.exceptionDetails.text}`);
  return res.result.value;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function waitFor(check, label, timeoutMs = 30000) {
  const start = Date.now();
  for (;;) {
    const value = await check();
    if (value) return value;
    if (Date.now() - start > timeoutMs) throw new Error(`timed out waiting for ${label}`);
    await sleep(100);
  }
}

async function loadVendoredWasm() {
  const mod = await import(pathToFileURL(WASM_JS).href);
  mod.initSync({ module: fs.readFileSync(WASM_BIN) });
  return mod;
}

/** Character shapes (font size per UTF-16 offset) and text of the first paragraph. */
export function readParagraphShapes(wasm, bytes) {
  const doc = new wasm.HwpDocument(new Uint8Array(bytes));
  try {
    const length = doc.getParagraphLength(0, 0);
    const text = doc.getTextRange(0, 0, 0, length);
    const sizes = [];
    for (let i = 0; i < length; i++) sizes.push(JSON.parse(doc.getCharPropertiesAt(0, 0, i)).fontSize);
    return { text, sizes };
  } finally {
    doc.free();
  }
}

async function click(cdp, session, x, y, clickCount = 1) {
  const base = { x, y, button: "left", clickCount };
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y }, session);
  await cdp.send("Input.dispatchMouseEvent", { ...base, type: "mousePressed" }, session);
  await cdp.send("Input.dispatchMouseEvent", { ...base, type: "mouseReleased" }, session);
}

async function key(cdp, session, name, code, vk, modifiers = 0) {
  const base = { key: name, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  await cdp.send("Input.dispatchKeyEvent", { ...base, type: "rawKeyDown" }, session);
  await cdp.send("Input.dispatchKeyEvent", { ...base, type: "keyUp" }, session);
}

const rectOf = (selector) =>
  `(() => { const r = document.getElementById('studio').contentDocument.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();
    const f = document.getElementById('studio').getBoundingClientRect(); return { x: f.left + r.left, y: f.top + r.top, w: r.width, h: r.height }; })()`;

export async function runHarness({ log = console.log } = {}) {
  if (!fs.existsSync(path.join(BUNDLE, "index.html"))) {
    throw new Error("hwp-studio bundle is missing; run node scripts/build-hwp-studio.mjs first");
  }
  const wasm = await loadVendoredWasm();
  const server = await startServer();
  const problems = [];
  let cdp = null;
  let started = null;
  try {
    started = await startChrome(log);
    cdp = new Cdp(started.browserWs);
    await cdp.opened;
    const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    const s = sessionId;
    const requests = [];
    cdp.onEvent((msg) => {
      if (msg.sessionId !== s) return;
      if (msg.method === "Runtime.exceptionThrown") problems.push(`exception: ${msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text}`);
      if (msg.method === "Log.entryAdded" && /Content Security Policy|Refused to/.test(msg.params.entry.text)) problems.push(`csp: ${msg.params.entry.text}`);
      if (msg.method === "Network.requestWillBeSent") requests.push(msg.params.request.url);
    });
    for (const domain of ["Page", "Runtime", "Log", "Network"]) await cdp.send(`${domain}.enable`, {}, s);
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, s);

    const origin = `http://127.0.0.1:${server.address().port}`;
    await cdp.send("Page.navigate", { url: `${origin}/harness.html` }, s);
    const connected = await waitFor(() => evaluate(cdp, s, "window.hwp ? hwp.connected.then((m) => m.capabilities) : null"), "embed connection");
    log(`connected, capabilities: ${connected.join(", ")}`);
    for (const forbidden of ["plugin-loader-v1", "hwpctrl-v1"]) {
      if (connected.includes(forbidden)) problems.push(`capability ${forbidden} must not be advertised`);
    }
    if (!connected.includes("blank-document-v1")) problems.push("capability blank-document-v1 missing");
    await waitFor(() => evaluate(cdp, s, "hwp.call('ready')"), "studio ready", 60000);

    const created = await evaluate(cdp, s, "hwp.call('createBlankDocument')");
    log(`blank document created, pages: ${created.pageCount}`);
    await waitFor(() => evaluate(cdp, s, "document.getElementById('studio').contentDocument.querySelectorAll('canvas').length > 0"), "page canvas");
    await sleep(800);

    // Real input: click into the first page, type Korean text.
    const page = await evaluate(cdp, s, rectOf("#scroll-container canvas"));
    await click(cdp, s, page.x + page.w / 2, page.y + 160);
    await sleep(300);
    await cdp.send("Input.insertText", { text: TYPED_TEXT }, s);
    await sleep(500);

    // Select the first SELECTED_CHARS characters: Home, Shift+Right x N.
    await key(cdp, s, "Home", "Home", 36);
    for (let i = 0; i < SELECTED_CHARS; i++) await key(cdp, s, "ArrowRight", "ArrowRight", 39, 8);
    await sleep(300);

    // Font size 20 through the toolbar input (select the field, type, Enter).
    const input = await evaluate(cdp, s, rectOf("#font-size"));
    await click(cdp, s, input.x + input.w / 2, input.y + input.h / 2, 3);
    await cdp.send("Input.insertText", { text: String(BIG_PT) }, s);
    await key(cdp, s, "Enter", "Enter", 13);
    await sleep(600);

    const exported = await evaluate(cdp, s, "hwp.exportBase64()");
    const bytes = Buffer.from(exported, "base64");
    log(`exportHwp: ${bytes.length} bytes`);

    const first = readParagraphShapes(wasm, bytes);
    log(`reloaded text: ${JSON.stringify(first.text)}`);
    if (first.text !== TYPED_TEXT) problems.push(`text mismatch: ${JSON.stringify(first.text)}`);
    const big = BIG_PT * 100;
    const head = first.sizes.slice(0, SELECTED_CHARS);
    const tail = first.sizes.slice(SELECTED_CHARS);
    if (!head.every((v) => v === big)) problems.push(`selected characters are not ${BIG_PT}pt: ${head.join(",")}`);
    if (tail.length === 0 || new Set(tail).size !== 1 || tail[0] === big) problems.push(`unselected characters changed: ${tail.join(",")}`);
    log(`font sizes (1/100 pt): selected ${head.join(",")} | rest ${[...new Set(tail)].join(",")}`);

    // Reload the exported bytes in the editor itself and export again: shapes must be identical.
    await evaluate(cdp, s, `hwp.loadBase64(${JSON.stringify(exported)}, 'harness.hwp')`);
    await sleep(500);
    const again = readParagraphShapes(wasm, Buffer.from(await evaluate(cdp, s, "hwp.exportBase64()"), "base64"));
    if (JSON.stringify(again.sizes) !== JSON.stringify(first.sizes) || again.text !== first.text) {
      problems.push("character shapes differ after reloading the exported bytes in the editor");
    }

    const loadedFonts = await evaluate(
      cdp,
      s,
      "[...new Set([...document.getElementById('studio').contentDocument.fonts].filter((f) => f.status === 'loaded').map((f) => f.family.replaceAll(String.fromCharCode(34), '')))]",
    );
    log(`bundled fonts loaded: ${loadedFonts.join(", ") || "(none)"}`);
    if (loadedFonts.length === 0) problems.push("no bundled font was loaded through the FontFace API");

    const foreign = requests.filter((url) => !url.startsWith(origin) && !url.startsWith("data:") && !url.startsWith("blob:") && url !== "about:blank");
    if (foreign.length > 0) problems.push(`non-local requests: ${foreign.slice(0, 5).join(", ")}`);
    log(`requests: ${requests.length}, all local: ${foreign.length === 0}`);
  } finally {
    cdp?.close();
    started?.chrome.child.kill();
    server.close();
    await sleep(300);
    if (started) removeProfile(started.profile, log);
  }
  return problems;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const watchdog = setTimeout(() => {
    console.error('hwp-studio harness timed out');
    process.exit(1);
  }, 180000);
  watchdog.unref();
  try {
    const problems = await runHarness();
    if (problems.length > 0) {
      console.error("hwp-studio harness failed:");
      for (const p of problems) console.error(`  - ${p}`);
      process.exit(1);
    }
    console.log("hwp-studio harness passed.");
    process.exit(0);
  } catch (err) {
    console.error(err instanceof Error ? err.stack : String(err));
    process.exit(1);
  }
}
