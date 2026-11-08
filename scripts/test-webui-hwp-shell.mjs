import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// One window, one menu row, one theme, one status bar (#158): the HWP window hides the shell's menu
// row and status bar, the editor's 파일 menu is a whitelist of shell commands forwarded over the embed
// port, the shell theme reaches the editor, and the status bar keeps only what a reader needs.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (rel, base = UI_DIR) => fs.readFileSync(fileURLToPath(new URL(rel, base)), "utf8");
const HOST_JS = new URL("hwp-host.js", UI_DIR);
const CHROME_JS = new URL("shell-chrome.js", UI_DIR);
const ALL_CSS = `${read("styles.css")}\n${read("shell.css")}\n${read("content.css")}\n${read("home.css")}\n${read("menu-bar.css")}\n${read("toolbar.css")}`;
const OVERLAY = new URL("../apps/hwp-studio/", import.meta.url);

const COMMANDS = ["new", "open", "recent", "save", "save-as", "export-pdf", "print", "close"];

// ---- fake editor (embed protocol) -------------------------------------------------------------

class FakeElement {
  constructor() {
    this.attrs = new Map();
    this.listeners = new Map();
    this.children = [];
    this.hidden = false;
    this.textContent = "";
    this.dataset = {};
  }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  getAttribute(k) { return this.attrs.get(k) ?? null; }
  appendChild(kid) { this.children.push(kid); return kid; }
  remove() {
    /* detached-node stub: nothing to detach in this fake */
  }
  addEventListener(type, fn) { this.listeners.set(type, fn); }
  fire(type) { this.listeners.get(type)?.(); }
}

function loadHost(capabilities = ["blank-document-v1", "shell-context-v1"], extraDeps = {}) {
  const byId = new Map([["pane-hwp", new FakeElement()], ["pane-doc", new FakeElement()], ["status-command-feedback", new FakeElement()]]);
  const frames = [];
  const document = {
    getElementById: (id) => byId.get(id) ?? null,
    createElement: (tag) => {
      const el = new FakeElement();
      if (tag !== "iframe") return el;
      frames.push(el);
      el.contentWindow = {
        postMessage(message) {
          queueMicrotask(() => studio.port2.postMessage({ type: "rhwp-connected", version: 1, sessionId: message.sessionId, capabilities }));
        },
      };
      queueMicrotask(() => el.fire("load"));
      return el;
    },
  };
  const requests = [];
  const studio = { port2: null };
  class FakeMessageChannel {
    constructor() {
      const port1 = {
        onmessage: null,
        postMessage: (msg) => {
          requests.push(msg);
          const result = msg.method === "exportHwp" ? new Uint8Array([0xd0, 0xcf]) : { ok: true };
          queueMicrotask(() => studio.port2.postMessage({ type: "rhwp-response", version: 1, sessionId: msg.sessionId, id: msg.id, result }));
        },
        close() {},
      };
      this.port1 = port1;
      this.port2 = { postMessage: (msg) => queueMicrotask(() => port1.onmessage?.({ data: msg })) };
      studio.port2 = this.port2;
    }
  }
  const env = {
    MessageChannel: FakeMessageChannel,
    location: { origin: "http://127.0.0.1:8800" },
    crypto: { randomUUID: () => "session-1" },
    fetch: async () => ({ ok: true }),
    setTimeout: (fn, ms) => setTimeout(fn, Math.min(ms, 30)),
    clearTimeout,
    btoa: (s) => Buffer.from(s, "binary").toString("base64"),
    ArrayBuffer,
  };
  loadUiModule(HOST_JS, env);
  const api = env.__EASYOFFICE_CREATE_HWP_HOST__({
    document, env, getActiveModule: () => "HWP", invokeIpc: async () => ({}), ...extraDeps,
  });
  const sendEvent = (payload) => studio.port2.postMessage({ type: "rhwp-event", version: 1, payload });
  return { api, frames, requests, sendEvent, constants: env.__EASYOFFICE_HWP_HOST_CONSTANTS__ };
}

const settle = async () => { for (let i = 0; i < 12; i += 1) await new Promise((r) => setImmediate(r)); };

// ---- File menu commands ------------------------------------------------------------------------

test("the editor's 파일 menu forwards only whitelisted shell commands, re-checked by the shell", async () => {
  const { api, sendEvent, constants } = loadHost();
  assert.deepEqual([...constants.SHELL_COMMANDS], COMMANDS);
  await api.onModuleActivated("HWP");
  await settle();
  const seen = [];
  api.onCommand((c) => seen.push(c));
  const send = (payload) => sendEvent({ schemaVersion: 1, reason: "shell_command", ...payload });
  for (const command of ["new", "open", "save", "print", "close"]) send({ command });
  send({ command: "recent", id: "r-1_a" });
  send({ command: "save-as", format: "hwpx" });
  send({ command: "save-as" });
  send({ command: "export-pdf", path: String.raw`C:\x.pdf`, pages: ["<svg/>"] });
  await settle();
  assert.deepEqual(seen, [
    { command: "new" }, { command: "open" }, { command: "save" }, { command: "print" }, { command: "close" },
    { command: "recent", id: "r-1_a" },
    { command: "save-as", format: "hwpx" },
    { command: "save-as", format: "hwp" },
    { command: "export-pdf" },
  ]);

  seen.length = 0;
  send({ command: "exec" });
  send({ command: "../open" });
  send({ command: "recent" });
  send({ command: "recent", id: String.raw`C:\Users\me\a.hwp` });
  send({ command: "recent", id: "../../x" });
  send({ command: "save-as", format: "exe" });
  send({ command: ["save"] });
  send({});
  await settle();
  assert.deepEqual(seen, [], "unknown commands and malformed arguments never reach a handler");
});

test("shell commands are not treated as document changes", async () => {
  const { api, sendEvent } = loadHost();
  await api.onModuleActivated("HWP");
  await settle();
  const changes = [];
  api.onChange((e) => changes.push(e));
  sendEvent({ schemaVersion: 1, reason: "shell_command", command: "save" });
  sendEvent({ schemaVersion: 1, reason: "edited" });
  await settle();
  assert.equal(changes.length, 1);
});

test("forwarded commands run the matching shell action", () => {
  const calls = [];
  const env = { setTimeout, clearTimeout };
  const document = { getElementById: () => null, querySelectorAll: () => [], documentElement: { dataset: {} } };
  loadUiModule(CHROME_JS, env);
  let handler = null;
  const chrome = env.__EASYOFFICE_CREATE_SHELL_CHROME__({
    document, env,
    hwpHost: { onCommand: (fn) => { handler = fn; }, setShellContext: () => Promise.resolve(true) },
    actions: Object.fromEntries(["newDocument", "openFile", "openRecent", "save", "saveAs", "exportPdf", "close"].map((name) => [name, (...args) => { calls.push([name, ...args]); }])),
  });
  chrome.bind();
  for (const request of [{ command: "new" }, { command: "open" }, { command: "recent", id: "x1" }, { command: "save" }, { command: "save-as", format: "hwpx" }, { command: "export-pdf" }, { command: "close" }, { command: "bogus" }]) handler(request);
  assert.deepEqual(calls, [["newDocument"], ["openFile"], ["openRecent", "x1"], ["save"], ["saveAs", "hwpx"], ["exportPdf"], ["close"]]);
});

test("overlay 파일 menu lists exactly the shell commands and no editor file action", () => {
  const html = read("index.html", OVERLAY);
  const block = html.slice(html.indexOf('data-menu="file"'), html.indexOf('data-menu="edit"'));
  const commands = new Set([...block.matchAll(/data-shell-cmd="([^"]+)"/g)].map((m) => m[1]));
  assert.ok(block.includes("data-shell-recent"), "the recent submenu is filled by the shell's list");
  const byName = (a, b) => a.localeCompare(b);
  assert.deepEqual([...commands].sort(byName), COMMANDS.filter((c) => c !== "recent").sort(byName));
  assert.ok(!/data-cmd=/.test(block), "no editor-native file command is left in the 파일 menu");
  const ts = read("src/embed/shell-menu.ts", OVERLAY);
  const listed = /SHELL_COMMANDS = \[([^\]]+)\]/.exec(ts)[1].match(/'([^']+)'/g).map((s) => s.slice(1, -1));
  assert.deepEqual(listed, COMMANDS, "the editor and the shell share one command list");
  assert.ok(ts.includes("item.dataset.shellCmd = 'recent'"), "recent entries are forwarded as the recent command");
  assert.ok(!/display:\s*none/.test(read("src/eo-overrides.css", OVERLAY).split("\n").filter((l) => l.includes('data-menu="file"')).join("")), "the 파일 menu is shown");
});

// ---- theme -------------------------------------------------------------------------------------

test("the shell theme is passed to the editor at start and follows later changes at once", async () => {
  const { api, frames, requests } = loadHost(undefined, { initialTheme: "dark" });
  await api.onModuleActivated("HWP");
  await settle();
  assert.match(frames[0].getAttribute("src"), /[?&]theme=dark(&|$)/);
  const pushed = requests.filter((r) => r.method === "setShellContext");
  assert.equal(pushed.length, 1, "pushed once before the first document");
  assert.equal(pushed[0].params.theme, "dark");

  await api.setShellContext({ theme: "light" });
  await api.setShellContext({ theme: "system" });
  await api.setShellContext({ theme: "<script>" });
  await settle();
  assert.deepEqual(requests.filter((r) => r.method === "setShellContext").slice(1).map((r) => r.params.theme), ["light", "system", "system"]);
});

test("system theme leaves the frame URL alone, recents travel as id and name only", async () => {
  const { api, frames, requests } = loadHost();
  await api.setShellContext({ recents: [{ id: "a1", name: "보고서.hwp", path: String.raw`C:\x\보고서.hwp` }, { id: "../bad", name: "x" }, { id: "b2", name: "" }] });
  await api.onModuleActivated("HWP");
  await settle();
  assert.ok(!/theme=/.test(frames[0].getAttribute("src")));
  const push = requests.find((r) => r.method === "setShellContext");
  assert.deepEqual(push.params.recents, [{ id: "a1", name: "보고서.hwp" }]);
  assert.ok(!JSON.stringify(push).includes("C:"), "no path reaches the editor");
});

test("an editor without the shell-context capability is not sent the RPC", async () => {
  const { api, requests } = loadHost(["blank-document-v1"]);
  await api.onModuleActivated("HWP");
  await settle();
  assert.equal(requests.filter((r) => r.method === "setShellContext").length, 0);
  assert.equal(api.getStatus(), "ready");
});

test("the theme toggle tells the editor the explicit choice; first paint stays 'system'", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=DOC&win=w-theme-fwd" });
  const html = document.documentElement;
  const before = html.dataset.themeEffective;
  userClick(document.getElementById("btn-theme-toggle"));
  await flushAsyncTurns(10);
  assert.notEqual(html.dataset.themeEffective, before);
});

// ---- one menu row, one status bar --------------------------------------------------------------

function rule(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, String.raw`\$&`);
  const m = new RegExp(String.raw`${escaped}\s*\{([^}]*)\}`).exec(read("shell.css"));
  return m ? m[1] : "";
}

test("while the editor owns the window, the shell menu row and status bar are hidden", () => {
  for (const sel of ['.app-shell[data-hwp-embed="true"] #menu-bar', '.app-shell[data-hwp-embed="true"] #status-bar']) {
    assert.ok(ALL_CSS.includes(sel), sel);
  }
  const m = /\.app-shell\[data-hwp-embed="true"\] #menu-bar,\s*\.app-shell\[data-hwp-embed="true"\] #status-bar\s*\{\s*display:\s*none/.exec(ALL_CSS);
  assert.ok(m, "both are display:none under data-hwp-embed");
  assert.ok(!/studio-menubar \.menu-item:not\(\[data-menu="file"\]\)/.test(ALL_CSS), "no partial menu row is left behind");
});

test("shell notices become toasts while the editor is active, and stay in the status line otherwise", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-toast" });
  const app = document.getElementById("app");
  const fb = document.getElementById("status-command-feedback");
  const toast = document.getElementById("shell-toast");
  assert.ok(toast?.hidden, "toast element exists and starts hidden");
  assert.equal(toast.tagName, "OUTPUT", "the toast is an <output> (implicit status role)");
  fb.textContent = "일반 안내";
  assert.equal(fb.textContent, "일반 안내");
  assert.ok(toast.hidden, "no toast outside the HWP editor");
  app.dataset.hwpEmbed = "true";
  fb.textContent = "저장 완료";
  assert.equal(toast.hidden, false);
  assert.equal(toast.textContent, "저장 완료");
  assert.equal(fb.textContent, "저장 완료", "the status line keeps the text too");
});

test("status bar keeps page, position, input mode, file name, message and zoom only", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-statusbar" });
  const bar = document.getElementById("status-bar");
  for (const id of ["status-studio-pos", "status-insert-mode", "status-doc-filename", "status-command-feedback", "status-zoom-label", "doc-page-zone-indicator"]) {
    assert.ok(bar.querySelector(`#${id}`), `${id} stays`);
  }
  for (const id of ["platform-pill", "status-module", "status-format-spec", "ime-badge", "status-engine"]) {
    assert.equal(bar.querySelector(`#${id}`), null, `${id} left the status bar`);
    assert.ok(document.getElementById("about-dialog").querySelector(`#${id}`), `${id} moved to the About dialog`);
  }
  assert.doesNotMatch(bar.textContent, /로컬 서버|모듈:|포맷:|IME|엔진:/);
  assert.ok(document.querySelector('[data-about-open]'), "Help menu opens the About dialog");
  // The items that must stay readable at 1024px never shrink; the file name and message give way first.
  for (const sel of [".statusbar-zoom-group", ".statusbar-pos-metrics, .statusbar-mode-btn, .eo-style-sep"]) assert.match(rule(sel), /flex-shrink:\s*0/);
  assert.match(rule(".statusbar-doc-path"), /min-width:\s*0/);
  assert.doesNotMatch(ALL_CSS, /#status-engine|#status-format-spec|\.platform-pill \{ display: none/);
});

// ---- start window icons ------------------------------------------------------------------------

test("home actions use one SVG icon per format and one shared accent token", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=START&win=w-start-icons" });
  const names = {};
  for (const card of document.querySelectorAll(".home-action[data-window-new]")) {
    names[card.dataset.windowNew] = card.querySelector(".home-action-icon").dataset.icon;
  }
  assert.equal(new Set(Object.values(names)).size, Object.keys(names).length, "every card has its own icon");
  assert.deepEqual(Object.keys(names).sort((a, b) => a.localeCompare(b)), ["CONV", "DOC", "HWP", "PDF", "SHEET", "SLIDE"]);
  const source = read("icons.js");
  for (const name of Object.values(names)) assert.ok(new RegExp(String.raw`"?${name}"?:\s*\[`).test(source), `${name} is drawn in icons.js`);
  // PD-098: no per-module colours; every card icon uses the single accent token.
  assert.ok(!ALL_CSS.includes("--card-accent"), "no per-module card accent");
  assert.match(ALL_CSS, /\.home-action-icon\s*\{[^}]*color:\s*var\(--accent-strong\)/);
  assert.equal(document.querySelector('[data-window-new="DOC"] .home-action-icon svg') !== null, true, "icons are rendered as SVG");
});

// ---- PDF export menu item (PD-101) ---------------------------------------------------------------

test("the PDF item is in the overlay 파일 menu, hidden until the shell says it can export", () => {
  const html = read("index.html", OVERLAY);
  const item = /<div class="md-item" data-shell-cmd="export-pdf"([^>]*)>/.exec(html);
  assert.ok(item, "the 파일 menu has the export-pdf item");
  assert.match(item[1], /\bhidden\b/, "hidden by default");
  assert.ok(!/data-shell-/.test(item[1].replace(/hidden/, "")), "no extra arguments travel with it");
  assert.match(read("src/eo-overrides.css", OVERLAY), /\.md-item\[hidden\]\s*\{\s*display:\s*none/);
  const ts = read("src/embed/shell-menu.ts", OVERLAY);
  assert.match(ts, /pdfExport = typeof params\.pdfExport === 'boolean'/, "only a boolean flag is accepted from the shell");
  const main = read("src/main.ts", OVERLAY);
  assert.match(main, /context\.pdfExport !== null\) pdfItem\.hidden = !context\.pdfExport/);
});

test("the shell tells the editor whether it can export a PDF (desktop app only)", async () => {
  const pushes = async ([canExportPdf, expected]) => {
    const { api, requests } = loadHost(undefined, { canExportPdf });
    await api.onModuleActivated("HWP");
    await settle();
    const pushed = requests.filter((r) => r.method === "setShellContext");
    assert.equal(pushed.at(-1).params.pdfExport, expected);
  };
  await Promise.all([[true, true], [undefined, false]].map(pushes));
});

test("the desktop bridge sends page SVG text only and no path to export_searchable_pdf", async () => {
  const calls = [];
  const env = {
    __EASYOFFICE_SHELL_TOKEN__: "token",
    __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push({ command, args }); return { status: "saved", name: "a.pdf", bytes: 3 }; } },
  };
  loadUiModule(new URL("../crates/easyoffice-app/ui/desktop-bridge.js", import.meta.url), env);
  const bridge = env.__EASYOFFICE_DESKTOP_BRIDGE__;
  const res = await bridge.exportSearchablePdf("보고서.pdf", ["<svg/>", "<svg/>"]);
  assert.equal(res.status, "saved");
  assert.deepEqual(calls, [{ command: "export_searchable_pdf", args: { fileName: "보고서.pdf", pages: ["<svg/>", "<svg/>"] } }]);
  env.__TAURI_INTERNALS__.invoke = async () => ({ status: "weird" });
  await assert.rejects(bridge.exportSearchablePdf("a.pdf", ["<svg/>"]), /PDF 저장 결과를 확인하지 못했습니다/);
});
