// Support for scripts/bench-webui.mjs: the WebUI measured in a real headless browser against the real
// engine. A small Node server serves `crates/easyoffice-app/ui` from disk (so a working-tree edit needs
// no rebuild) and forwards `POST /api/ipc` to `easyoffice-app --serve`; the browser is driven over CDP.
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

export const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const UI_DIR = path.join(REPO, "crates/easyoffice-app/ui");
const MIME = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".svg": "image/svg+xml" };

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Serves the UI directory and proxies the engine endpoint. `ui` overrides the directory (before/after runs). */
export function startUiServer({ enginePort, port, ui = UI_DIR, inputs }) {
  const server = http.createServer((req, res) => {
    if (req.method === "POST" && req.url === "/api/ipc") {
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => {
        const body = Buffer.concat(chunks);
        const up = http.request(
          { host: "127.0.0.1", port: enginePort, path: "/api/ipc", method: "POST", headers: { "Content-Type": "application/json", "Content-Length": body.length, Origin: `http://127.0.0.1:${enginePort}` } },
          (r) => { res.writeHead(r.statusCode, r.headers); r.pipe(res); },
        );
        up.on("error", () => { if (res.headersSent) res.destroy(); else { res.writeHead(502); res.end(); } });
        up.end(body);
      });
      return;
    }
    if (inputs && req.url.startsWith("/__bench/")) {
      const file = path.join(inputs, path.basename(decodeURIComponent(req.url.slice(9))));
      if (!fs.existsSync(file)) { res.writeHead(404); res.end(); return; }
      res.writeHead(200, { "Content-Type": "application/octet-stream" });
      fs.createReadStream(file).pipe(res);
      return;
    }
    const pathname = decodeURIComponent(req.url.split("?")[0]);
    const name = pathname === "/" ? "index.html" : pathname.slice(1);
    const file = path.resolve(ui, name);
    if (!file.startsWith(ui + path.sep) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { "Content-Type": MIME[path.extname(file)] || "application/octet-stream", "Cache-Control": "no-store" });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((resolve) => server.listen(port, "127.0.0.1", () => resolve(server)));
}

async function connect(wsUrl) {
  const sock = new WebSocket(wsUrl);
  let id = 0;
  const pending = new Map();
  const events = [];
  await new Promise((resolve, reject) => { sock.onopen = resolve; sock.onerror = reject; });
  sock.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
    if (m.method === "Runtime.exceptionThrown") events.push("[exception] " + (m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text));
    if (m.method === "Page.javascriptDialogOpening") sock.send(JSON.stringify({ id: ++id, method: "Page.handleJavaScriptDialog", params: { accept: true } }));
  };
  const send = (method, params = {}, timeoutMs = Number(process.env.BENCH_CDP_TIMEOUT_MS || 120000)) => new Promise((resolve, reject) => {
    const i = ++id;
    const timer = setTimeout(() => { pending.delete(i); reject(new Error(`CDP ${method} timed out: ${JSON.stringify(params).slice(0, 160)}`)); }, timeoutMs);
    pending.set(i, (m) => { clearTimeout(timer); resolve(m); });
    sock.send(JSON.stringify({ id: i, method, params }));
  });
  const evalJs = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error("page exception: " + (r.result.exceptionDetails.exception?.description || r.result.exceptionDetails.text));
    return r.result?.result?.value;
  };
  return { send, evalJs, events, close: () => sock.close() };
}

/** Starts the headless browser with its own profile; returns `{ cdp, stop }` (only this PID is ended). */
export async function startBrowser({ chrome, debugPort, url, preload, width = 1440, height = 900 }) {
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "eo-bench-profile-"));
  const proc = spawn(chrome, ["--headless=new", `--remote-debugging-port=${debugPort}`, `--user-data-dir=${profile}`, "--no-first-run", "--disable-extensions", "--disable-renderer-backgrounding", "--disable-backgrounding-occluded-windows", "--disable-background-timer-throttling", "--enable-precise-memory-info", "--js-flags=--expose-gc", `--window-size=${width},${height}`, "about:blank"], { stdio: "ignore" });
  let target = null;
  for (let i = 0; i < 100 && !target; i += 1) {
    await sleep(200);
    try {
      const list = await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json();
      target = list.find((t) => t.type === "page");
    } catch { /* still starting */ }
  }
  if (!target) throw new Error("the browser did not start");
  const cdp = await connect(target.webSocketDebuggerUrl);
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Performance.enable");
  await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await cdp.send("Page.bringToFront");
  await cdp.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  if (preload) await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: preload });
  await cdp.send("Page.navigate", { url });
  await sleep(1500);
  return {
    cdp,
    kill: () => proc.kill(),
    stop: async () => {
      cdp.close();
      const exited = new Promise((resolve) => proc.once("exit", resolve));
      proc.kill();
      const timer = setTimeout(() => proc.kill("SIGKILL"), 3000);
      await exited;
      clearTimeout(timer);
      fs.rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
    },
  };
}

export function startEngine(app, port) {
  return spawn(app, ["--serve", String(port)], { stdio: "ignore" });
}

const METRIC_KEYS = ["ScriptDuration", "LayoutDuration", "RecalcStyleDuration", "TaskDuration", "LayoutCount", "RecalcStyleCount", "Nodes", "JSHeapUsedSize", "LayoutObjects"];

export async function metrics(cdp) {
  const { result } = await cdp.send("Performance.getMetrics");
  const out = {};
  for (const m of result.metrics) if (METRIC_KEYS.includes(m.name)) out[m.name] = m.value;
  return out;
}

/** Runs `step` and returns wall time plus the browser's own counters for it (seconds -> ms). */
export async function measure(cdp, step) {
  const before = await metrics(cdp);
  const started = performance.now();
  const value = await step();
  const wall = performance.now() - started;
  const after = await metrics(cdp);
  const d = (k) => (after[k] ?? 0) - (before[k] ?? 0);
  return {
    wallMs: wall,
    scriptMs: d("ScriptDuration") * 1000,
    taskMs: d("TaskDuration") * 1000,
    layoutMs: d("LayoutDuration") * 1000,
    styleMs: d("RecalcStyleDuration") * 1000,
    layouts: d("LayoutCount"),
    recalcs: d("RecalcStyleCount"),
    nodes: after.Nodes,
    heapMB: after.JSHeapUsedSize / 1048576,
    value,
  };
}
