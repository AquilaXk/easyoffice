import test from "node:test";
import assert from "node:assert/strict";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// HWP host (PD-083): a fake iframe + MessageChannel stands in for the embedded rhwp-studio. The
// fake editor answers the embed protocol (`rhwp-connect` handshake, `rhwp-request` -> response).

const HOST_JS = new URL("../crates/easyoffice-app/ui/hwp-host.js", import.meta.url);

class FakeElement {
  constructor(tag, registry) {
    this.tag = tag;
    this.registry = registry;
    this.children = [];
    this.attrs = new Map();
    this.listeners = new Map();
    this.hidden = false;
    this.textContent = "";
    this.className = "";
    this.id = "";
    this.removed = false;
  }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  getAttribute(k) { return this.attrs.get(k) ?? null; }
  appendChild(kid) { this.children.push(kid); return kid; }
  remove() { this.removed = true; }
  addEventListener(type, fn) { this.listeners.set(type, fn); }
  fire(type) { this.listeners.get(type)?.(); }
}

function createFakeDocument() {
  const byId = new Map();
  const frames = [];
  const make = (id, tag = "div") => {
    const el = new FakeElement(tag);
    el.id = id;
    byId.set(id, el);
    return el;
  };
  make("pane-hwp").hidden = true;
  make("pane-doc");
  make("hwp-fallback-notice").hidden = true;
  make("status-command-feedback");
  return {
    byId, frames,
    getElementById: (id) => byId.get(id) ?? null,
    createElement: (tag) => {
      const el = new FakeElement(tag);
      if (tag === "iframe") frames.push(el);
      return el;
    },
  };
}

function createFakeStudio({ answer = true, failMethods = new Set(), capabilities = ["transferable-array-buffer", "blank-document-v1"] } = {}) {
  const requests = [];
  const state = { connect: null, port2: null, silent: !answer, answerConnect: answer };
  class FakeMessageChannel {
    constructor() {
      const port1 = { onmessage: null, closed: false, postMessage: (msg, transfer) => deliver(msg, transfer), close() { this.closed = true; } };
      const port2 = { postMessage: (msg) => queueMicrotask(() => port1.onmessage?.({ data: msg })) };
      this.port1 = port1;
      this.port2 = port2;
      state.port2 = port2;
    }
  }
  function deliver(msg, transfer) {
    requests.push(transfer ? { ...msg, transfer } : msg);
    if (state.silent) return;
    const respond = (extra) => state.port2.postMessage({ type: "rhwp-response", version: 1, sessionId: msg.sessionId, id: msg.id, ...extra });
    if (failMethods.has(msg.method)) return respond({ error: { code: "E", message: "fail " + msg.method } });
    const results = {
      ready: true,
      createBlankDocument: { pageCount: 1 },
      exportHwp: new Uint8Array([0xd0, 0xcf, 0x11, 0xe0, 1, 2, 3]),
      exportHwpx: new Uint8Array([0x50, 0x4b, 3, 4]),
      getDocumentState: { dirty: true },
      verifyBytes: { ok: true, pageCount: 1, format: "hwp" },
      notifySaved: null,
      enableHostFonts: { enabled: true },
      hostFontsReply: { accepted: true },
    };
    respond({ result: results[msg.method] });
  }
  const attachWindow = (frame) => {
    // An opaque-origin frame (sandbox without allow-same-origin) exposes no document to the shell.
    Object.defineProperty(frame, "contentDocument", {
      get() { throw new Error("SecurityError: the editor frame is cross-origin"); },
    });
    frame.contentWindow = {
      postMessage(message, origin, ports) {
        state.connect = { message, origin, ports };
        if (state.answerConnect) queueMicrotask(() => state.port2.postMessage({ type: "rhwp-connected", version: 1, sessionId: message.sessionId, capabilities }));
      },
    };
  };
  return { requests, state, FakeMessageChannel, attachWindow };
}

const listeners = new Map();

function loadHost({ fetchOk = true, studioOptions, activeModule = "HWP", timeouts = {}, extraDeps = {} } = {}) {
  const document = createFakeDocument();
  const studio = createFakeStudio(studioOptions);
  const env = {
    MessageChannel: studio.FakeMessageChannel,
    location: { origin: "http://127.0.0.1:8800" },
    crypto: { randomUUID: () => "session-1" },
    fetch: async () => ({ ok: fetchOk }),
    setTimeout: (fn, ms) => setTimeout(fn, Math.min(ms, 30)),
    clearTimeout,
    btoa: (s) => Buffer.from(s, "binary").toString("base64"),
    ArrayBuffer,
    addEventListener: (type, fn) => { (listeners.get(type) ?? listeners.set(type, new Set()).get(type)).add(fn); },
    removeEventListener: (type, fn) => { listeners.get(type)?.delete(fn); },
  };
  loadUiModule(HOST_JS, env);
  let mod = activeModule;
  const states = [];
  const ipcCalls = [];
  const hostApi = env.__EASYOFFICE_CREATE_HWP_HOST__({
    document, env, getActiveModule: () => mod, invokeIpc: async (cmd, payload) => { ipcCalls.push({ cmd, payload }); return {}; }, onStateChange: () => states.push(hostApi.getStatus()),
    ...timeouts,
    ...extraDeps,
  });
  document.createElementOriginal = document.createElement;
  const origCreate = document.createElement;
  document.createElement = (tag) => {
    const el = origCreate(tag);
    if (tag === "iframe") {
      studio.attachWindow(el);
      queueMicrotask(() => el.fire("load"));
    }
    return el;
  };
  const post = (source, data) => { for (const fn of [...(listeners.get("message") ?? [])]) fn({ source, data }); };
  return { hostApi, document, studio, states, ipcCalls, post, listenerCount: () => listeners.get("message")?.size ?? 0, setModule: (m) => { mod = m; }, constants: env.__EASYOFFICE_HWP_HOST_CONSTANTS__ };
}

const settle = async () => { for (let i = 0; i < 12; i += 1) await new Promise((r) => setImmediate(r)); };

test("lazy start: no frame until HWP is activated, then connect handshake, ready and blank document", async () => {
  const { hostApi, document, studio, constants } = loadHost();
  assert.equal(document.frames.length, 0);
  assert.equal(hostApi.getStatus(), "idle");
  await hostApi.onModuleActivated("DOC");
  assert.equal(document.frames.length, 0, "other modules never create the frame");
  await hostApi.onModuleActivated("HWP");
  await settle();
  assert.equal(document.frames.length, 1);
  const frame = document.frames[0];
  assert.equal(frame.getAttribute("src"), "./hwp-studio/index.html");
  assert.equal(frame.getAttribute("sandbox"), constants.FRAME_SANDBOX);
  assert.deepEqual(frame.getAttribute("sandbox").split(" "), ["allow-scripts", "allow-modals"], "exactly these two tokens");
  assert.equal(frame.getAttribute("allow"), "clipboard-write", "the editor may write the clipboard, nothing else is delegated");
  assert.equal(constants.FRAME_ALLOW, "clipboard-write");
  const { message, origin, ports } = studio.state.connect;
  assert.equal(message.type, "rhwp-connect");
  assert.equal(message.version, 1);
  assert.equal(message.sessionId, "session-1");
  assert.ok(message.capabilities.includes("transferable-array-buffer"));
  assert.equal(origin, "*", "an opaque-origin frame can only be addressed with * (the target is the frame this host created)");
  assert.deepEqual(Object.keys(message), ["type", "version", "sessionId", "capabilities"], "the handshake carries no shell data");
  assert.equal(ports.length, 1);
  assert.equal(studio.requests.map((r) => r.method).join(","), "ready,createBlankDocument");
  assert.equal(JSON.stringify(studio.requests[1].params), JSON.stringify({ skipUnsavedGuard: true }));
  assert.equal(hostApi.getStatus(), "ready");
  assert.ok(hostApi.getCapabilities().includes("blank-document-v1"));
  assert.equal(document.byId.get("status-command-feedback").textContent.includes("준비 완료"), true);
});

test("request/response uses incrementing ids and surfaces editor errors", async () => {
  const { hostApi, studio } = loadHost({ studioOptions: { failMethods: new Set(["exportHwpx"]) } });
  await hostApi.onModuleActivated("HWP");
  await settle();
  const bytes = await hostApi.exportHwp();
  assert.equal(bytes.length, 7);
  const ids = studio.requests.map((r) => r.id);
  assert.deepEqual(ids, [...ids].sort((a, b) => a - b));
  assert.equal(new Set(ids).size, ids.length);
  await assert.rejects(hostApi.call("exportHwpx"), /fail exportHwpx/);
  assert.equal(await hostApi.isDirty(), true);
});

test("a request that gets no response times out and is removed", async () => {
  const { hostApi, studio } = loadHost();
  await hostApi.onModuleActivated("HWP");
  await settle();
  studio.state.silent = true;
  await assert.rejects(hostApi.call("getDocumentState", undefined, { timeoutMs: 20 }), /응답 시간 초과 \(getDocumentState\)/);
});

test("connect failure shows an error state; reconnect creates a fresh frame and recovers", async () => {
  const { hostApi, document, studio, states } = loadHost({ studioOptions: { answer: false }, timeouts: { connectTimeoutMs: 20 } });
  await hostApi.onModuleActivated("HWP");
  await new Promise((r) => setTimeout(r, 120));
  assert.equal(hostApi.getStatus(), "error");
  assert.ok(states.includes("error"));
  assert.equal(document.frames[0].removed, true, "failed frame is torn down");
  assert.equal(hostApi.isEmbedActive(), true, "an error keeps the embed pane (with reconnect), not the fallback");
  const status = document.byId.get("pane-hwp").children.find((c) => c.className === "hwp-host-status");
  assert.equal(status.hidden, false);
  assert.match(status.children[0].textContent, /연결하지 못했습니다/);
  studio.state.silent = false;
  studio.state.answerConnect = true;
  await hostApi.reconnect();
  await settle();
  assert.equal(document.frames.length, 2, "reconnect builds a new frame");
  assert.equal(hostApi.getStatus(), "ready");
  assert.equal(studio.requests.at(-1).method, "createBlankDocument");
});

test("missing bundle falls back to the text-only editor with a clear message", async () => {
  const { hostApi, document, states } = loadHost({ fetchOk: false });
  await hostApi.onModuleActivated("HWP");
  await settle();
  assert.equal(hostApi.getStatus(), "unavailable");
  assert.equal(document.frames.length, 0, "no iframe is created without a bundle");
  assert.equal(hostApi.usesLegacy(), true);
  assert.equal(hostApi.isEmbedActive(), false);
  assert.equal(hostApi.paneForModule(), document.byId.get("pane-doc"));
  assert.equal(document.byId.get("hwp-fallback-notice").hidden, false);
  assert.match(document.byId.get("status-command-feedback").textContent, /구성 요소가 포함되지 않은 빌드/);
  assert.ok(states.includes("unavailable"));
});

test("missing fetch (unit environments) also uses the fallback, never throws", async () => {
  const document = createFakeDocument();
  const env = { setTimeout, clearTimeout };
  loadUiModule(HOST_JS, env);
  const hostApi = env.__EASYOFFICE_CREATE_HWP_HOST__({ document, getActiveModule: () => "HWP" });
  await hostApi.onModuleActivated("HWP");
  assert.equal(hostApi.getStatus(), "unavailable");
});

test("the user can switch to the text-only editor after an error", async () => {
  const { hostApi, document } = loadHost({ studioOptions: { answer: false }, timeouts: { connectTimeoutMs: 20 } });
  await hostApi.onModuleActivated("HWP");
  await new Promise((r) => setTimeout(r, 120));
  hostApi.useLegacyEditor();
  assert.equal(hostApi.getStatus(), "legacy");
  assert.equal(hostApi.paneForModule(), document.byId.get("pane-doc"));
  assert.equal(document.byId.get("pane-hwp").hidden, true);
  assert.equal(document.byId.get("pane-doc").hidden, false);
});

test("save exports HWP bytes, delivers them as .hwp and clears the editor dirty flag", async () => {
  const { hostApi, studio } = loadHost();
  await hostApi.onModuleActivated("HWP");
  await settle();
  const delivered = [];
  const delivery = await hostApi.saveDocument(async (b64, base, ext) => { delivered.push({ b64, base, ext }); return { ok: true, name: base + "." + ext }; }, "문서");
  assert.equal(delivery.ok, true);
  assert.equal(delivered[0].ext, "hwp");
  assert.equal(Buffer.from(delivered[0].b64, "base64")[0], 0xd0);
  assert.equal(studio.requests.at(-1).method, "notifySaved");
});

test("the smoke hwp step runs over the embed port only and resolves with the exported byte count (no frame DOM access)", async () => {
  const { hostApi, ipcCalls, studio } = loadHost();
  assert.equal(await hostApi.runSmoke(() => hostApi.onModuleActivated("HWP")), "bytes=7 pages=1");
  assert.equal(ipcCalls.length, 0, "the window bridge reports the result, the host makes no engine call");
  assert.deepEqual(studio.requests.map((r) => r.method), ["ready", "createBlankDocument", "exportHwp", "verifyBytes"]);
});

test("the smoke hwp step rejects with the reason when the editor cannot re-open the exported bytes", async () => {
  const { hostApi } = loadHost({ studioOptions: { failMethods: new Set(["verifyBytes"]) } });
  await assert.rejects(hostApi.runSmoke(() => hostApi.onModuleActivated("HWP")), /fail verifyBytes/);
});

test("the smoke hwp step rejects when the bundle is missing", async () => {
  const { hostApi } = loadHost({ fetchOk: false });
  await assert.rejects(hostApi.runSmoke(() => hostApi.onModuleActivated("HWP")), /구성 요소가 포함되지 않은 빌드/);
});

async function createIntegration({ bundle }) {
  const studio = createFakeStudio();
  const harness = await createWebUiDom({
    hostEnvExtras: {
      MessageChannel: studio.FakeMessageChannel,
      crypto: { randomUUID: () => "session-int" },
      location: { protocol: "http:", origin: "http://127.0.0.1:8800" },
      fetch: async () => ({ ok: bundle }),
    },
  });
  const { document } = harness;
  const origCreate = document.createElement;
  const frames = [];
  document.createElement = (tag) => {
    const el = origCreate(tag);
    if (String(tag).toLowerCase() === "iframe") {
      frames.push(el);
      studio.attachWindow(el);
      queueMicrotask(() => el.dispatchEvent?.({ type: "load" }));
      el.addEventListener?.("load", () => {});
    }
    return el;
  };
  return { ...harness, studio, frames };
}

test("webui: the HWP module is hosted by #pane-hwp and the document-only toolbar is hidden while it is active", async () => {
  const { document, studio, frames } = await createIntegration({ bundle: true });
  await flushAsyncTurns(8);
  const styleBar = document.getElementById("doc-style-bar");
  assert.equal(styleBar.hidden, false, "DOC shows the style bar");
  switchModule(document, "HWP");
  await flushAsyncTurns(20);
  assert.equal(frames.length, 1, "frame is created on the first HWP activation");
  assert.equal(document.getElementById("pane-hwp").hidden, false);
  assert.equal(document.getElementById("pane-doc").hidden, true);
  assert.equal(styleBar.hidden, true, "document style bar is hidden for the embedded editor");
  const toolbarBtn = document.querySelector('#icon-toolbar button[data-studio-cmd="insert-table"]');
  assert.equal(toolbarBtn.disabled, true, "document-only toolbar items are disabled");
  assert.equal(studioItem(document, "studio-save").disabled, false);
  assert.equal(document.getElementById("app").dataset.hwpEmbed, "true", "only the editor's own toolbar is shown");
  switchModule(document, "DOC");
  await flushAsyncTurns(8);
  assert.equal(document.getElementById("pane-hwp").hidden, true);
  assert.equal(document.getElementById("pane-doc").hidden, false);
  assert.equal(styleBar.hidden, false);
  assert.equal(document.getElementById("app").dataset.hwpEmbed, "false");
  assert.ok(studio.requests.length >= 0);
});

test("webui: without the bundle the old editor stays usable with the explanatory notice", async () => {
  const { document, frames } = await createIntegration({ bundle: false });
  await flushAsyncTurns(8);
  switchModule(document, "HWP");
  await flushAsyncTurns(20);
  assert.equal(frames.length, 0);
  assert.equal(document.getElementById("pane-doc").hidden, false);
  assert.equal(document.getElementById("pane-hwp").hidden, true);
  assert.equal(document.getElementById("hwp-fallback-notice").hidden, false);
  assert.equal(document.getElementById("doc-style-bar").hidden, false, "text-only editor keeps its style bar");
  switchModule(document, "DOC");
  await flushAsyncTurns(8);
  assert.equal(document.getElementById("hwp-fallback-notice").hidden, true);
});
test("onStatus and whenSettled report the connection outcome; exportBytes picks the export method", async () => {
  const { hostApi, studio } = loadHost();
  const seen = [];
  hostApi.onStatus((s) => seen.push(s));
  const settled = hostApi.whenSettled(5000);
  await hostApi.onModuleActivated("HWP");
  assert.equal(await settled, "ready");
  assert.deepEqual(seen.slice(-1), ["ready"]);
  assert.equal((await hostApi.exportBytes("hwp")).length, 7);
  await hostApi.exportBytes("hwpx");
  assert.deepEqual(studio.requests.map((r) => r.method).filter((m) => m.startsWith("export")), ["exportHwp", "exportHwpx"]);
  assert.equal(await hostApi.whenSettled(5000), "ready", "already settled resolves immediately");
});

test("whenSettled resolves with the failure status when the editor cannot connect", async () => {
  const { hostApi } = loadHost({ studioOptions: { answer: false }, timeouts: { connectTimeoutMs: 20 } });
  const settled = hostApi.whenSettled(5000);
  await hostApi.onModuleActivated("HWP");
  assert.equal(await settled, "error");
});

test("shell shortcuts: the frame URL names the combos and forwarded combos reach only the shortcut listeners", async () => {
  const { hostApi, document, studio, constants } = loadHost({ extraDeps: { shellShortcuts: ["Ctrl+S", "Ctrl+O", 42, "Ctrl+K"] } });
  const changes = [];
  const shortcuts = [];
  hostApi.onChange((ev) => changes.push(ev));
  hostApi.onShortcut((combo) => shortcuts.push(combo));
  await hostApi.onModuleActivated("HWP");
  await settle();
  assert.equal(document.frames[0].getAttribute("src"), constants.STUDIO_SRC + "?shellKeys=" + encodeURIComponent("Ctrl+S,Ctrl+O,Ctrl+K"));
  const event = (payload) => studio.state.port2.postMessage({ type: "rhwp-event", version: 1, sessionId: "session-1", event: "documentChanged", payload });
  event({ schemaVersion: 1, reason: "shell_shortcut", shortcut: "Ctrl+S" });
  event({ schemaVersion: 1, reason: "shell_shortcut", shortcut: { evil: true } });
  event({ schemaVersion: 1, reason: "edited" });
  await settle();
  assert.deepEqual(shortcuts, ["Ctrl+S"], "only string combos are delivered");
  assert.equal(changes.length, 1, "a forwarded shortcut is never treated as an edit (no autosave, no dirty state)");
  assert.equal(changes[0].payload.reason, "edited");
});

test("shell shortcuts: without a shortcut table the frame URL stays plain", async () => {
  const { hostApi, document, constants } = loadHost();
  await hostApi.onModuleActivated("HWP");
  await settle();
  assert.equal(document.frames[0].getAttribute("src"), constants.STUDIO_SRC);
});

test("the host never reaches into the editor frame or the shell's own Tauri objects", async () => {
  const { readFileSync } = await import("node:fs");
  const source = readFileSync(HOST_JS, "utf8").replace(/\/\/.*$/gm, "");
  assert.doesNotMatch(source, /contentDocument/);
  assert.doesNotMatch(source, /allow-same-origin/);
  assert.doesNotMatch(source, /__TAURI/);
  assert.doesNotMatch(source, /contentWindow\s*(\.|\[)/, "the frame's window is only handed to postMessage or compared, never read");
  assert.match(source, /target\.postMessage\(/);
});

// ---- frame IPC probe (smoke step `frame_ipc`) -------------------------------------------

function startProbe(h, timeouts) {
  const done = h.hostApi.runFrameProbe(timeouts);
  const frame = h.document.frames.at(-1);
  return { done, frame };
}

test("frame probe: loads frame-probe.html in a sandbox without allow-same-origin and passes when the frame cannot run a command", async () => {
  const h = loadHost();
  const { done, frame } = startProbe(h);
  assert.equal(frame.getAttribute("src"), "./frame-probe.html");
  assert.deepEqual(frame.getAttribute("sandbox").split(" "), ["allow-scripts", "allow-modals"]);
  assert.equal(frame.getAttribute("allow"), "clipboard-write", "the probe frame is delegated exactly like the editor frame");
  h.post({}, { type: "eo-frame-probe", internals: "present", webview: "present", invoke: "rejected" });
  assert.equal(h.listenerCount(), 1, "a message from another window is ignored");
  h.post(frame.contentWindow, { type: "eo-frame-probe", internals: "present", webview: "present", invoke: "rejected" });
  assert.equal(await done, "internals=present webview=present invoke=rejected");
  assert.equal(frame.removed, true, "the probe frame is removed");
  assert.equal(h.listenerCount(), 0, "and the listener with it");
});

test("frame probe: absent Tauri objects and an unanswered call also pass", async () => {
  for (const [internals, invoke] of [["absent", "absent"], ["present", "no-answer"]]) {
    const h = loadHost();
    const { done, frame } = startProbe(h);
    h.post(frame.contentWindow, { type: "eo-frame-probe", internals, webview: "absent", invoke });
    assert.equal(await done, `internals=${internals} webview=absent invoke=${invoke}`);
  }
});

test("frame probe: fails when the frame ran an app command", async () => {
  const h = loadHost();
  const { done, frame } = startProbe(h);
  h.post(frame.contentWindow, { type: "eo-frame-probe", internals: "present", webview: "present", invoke: "allowed" });
  await assert.rejects(done, /앱 명령을 실행했습니다/);
  assert.equal(frame.removed, true);
});

test("frame probe: fails closed on a malformed report and on silence", async () => {
  const h = loadHost();
  const first = startProbe(h);
  h.post(first.frame.contentWindow, { type: "eo-frame-probe", internals: "present", webview: "present", invoke: "maybe" });
  await assert.rejects(first.done, /점검 결과를 해석하지 못했습니다/);
  const second = startProbe(loadHost(), { timeoutMs: 20 });
  await assert.rejects(second.done, /응답하지 않았습니다/);
});

// ---- installed fonts (host-fonts-v1, PD-128) ----------------------------------------------------

const FONT_CAPABILITIES = ["transferable-array-buffer", "blank-document-v1", "host-fonts-v1"];
const rawFace = (file, faceIndex, family, extra = {}) => ({
  file, faceIndex, family, fullName: family, postscriptName: family.replaceAll(" ", ""), style: "Regular", weight: 400, slant: "normal", aliases: [], ...extra,
});

function fontsApi({ faces, revision = "00ff00ff00ff00ff", sizes = {}, failRead = false } = {}) {
  const calls = { list: 0, read: [] };
  return {
    calls,
    api: {
      list: async () => { calls.list += 1; return { revision, faces }; },
      read: async (rev, file) => {
        calls.read.push([rev, file]);
        if (failRead) throw new Error("/Users/someone/Library/Fonts/secret.ttf: denied");
        return new Uint8Array(sizes[file] ?? 8).fill(file + 1).buffer;
      },
    },
  };
}

const sendFontEvent = (studio, payload) => studio.state.port2.postMessage({ type: "rhwp-event", version: 1, sessionId: "session-1", event: "documentChanged", payload: { schemaVersion: 1, reason: "shell_fonts", ...payload } });
const fontReplies = (studio) => studio.requests.filter((r) => r.method === "hostFontsReply");

test("fonts: the shell enables the editor's host fonts after the shell context, only with the capability and the bridge", async () => {
  const withBoth = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES }, extraDeps: { systemFonts: fontsApi({ faces: [] }).api } });
  await withBoth.hostApi.onModuleActivated("HWP");
  await settle();
  assert.equal(withBoth.studio.requests.map((r) => r.method).join(","), "ready,enableHostFonts,createBlankDocument");
  const noCapability = loadHost({ extraDeps: { systemFonts: fontsApi({ faces: [] }).api } });
  await noCapability.hostApi.onModuleActivated("HWP");
  await settle();
  assert.equal(noCapability.studio.requests.map((r) => r.method).join(","), "ready,createBlankDocument");
  const noBridge = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES } });
  await noBridge.hostApi.onModuleActivated("HWP");
  await settle();
  assert.equal(noBridge.studio.requests.map((r) => r.method).join(","), "ready,createBlankDocument", "outside the desktop shell there is no font bridge");
});

test("fonts: a list request is answered with ids, names and styles, never a path or file number", async () => {
  const fonts = fontsApi({ faces: [rawFace(7, 0, "Batang", { aliases: ["바탕"] }), rawFace(7, 1, "Batang Bold", { weight: 700, style: "Bold" }), rawFace(9, 0, "Dotum")] });
  const { hostApi, studio } = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES }, extraDeps: { systemFonts: fonts.api } });
  await hostApi.onModuleActivated("HWP");
  await settle();
  sendFontEvent(studio, { requestId: 1, op: "list" });
  sendFontEvent(studio, { requestId: 2, op: "list" });
  await settle();
  const [first, second] = fontReplies(studio);
  assert.equal(first.params.requestId, 1);
  assert.equal(fonts.calls.list, 1, "one list per connection, the second request reuses it");
  assert.deepEqual(first.params.list, second.params.list);
  assert.equal(first.params.list.revision, "00ff00ff00ff00ff");
  assert.deepEqual(first.params.list.faces.map((f) => f.id), ["7:0", "7:1", "9:0"]);
  assert.deepEqual(first.params.list.faces[0], { id: "7:0", family: "Batang", fullName: "Batang", postscriptName: "Batang", style: "Regular", weight: 400, slant: "normal", aliases: ["바탕"] });
  for (const face of first.params.list.faces) {
    assert.ok(!("file" in face) && !("faceIndex" in face) && !("path" in face), "only the contract fields reach the editor");
  }
});

test("fonts: a read is checked against the list, read once per file, and the bytes are transferred", async () => {
  const fonts = fontsApi({ faces: [rawFace(7, 0, "Batang"), rawFace(7, 1, "Batang Bold", { weight: 700 }), rawFace(9, 0, "Dotum")] });
  const { hostApi, studio } = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES }, extraDeps: { systemFonts: fonts.api } });
  await hostApi.onModuleActivated("HWP");
  await settle();
  const revision = "00ff00ff00ff00ff";
  sendFontEvent(studio, { requestId: 1, op: "read", face: "7:0", revision });
  sendFontEvent(studio, { requestId: 2, op: "read", face: "7:1", revision });
  await settle();
  const [one, two] = fontReplies(studio);
  assert.deepEqual(fonts.calls.read, [[revision, 7]], "the faces of one collection read the file once");
  assert.equal(one.params.data.faceIndex, 0);
  assert.equal(two.params.data.faceIndex, 1);
  assert.equal(one.params.data.bytes.byteLength, 8);
  assert.deepEqual(one.transfer, [one.params.data.bytes], "the bytes move, they are not copied again");
  assert.notEqual(one.params.data.bytes, two.params.data.bytes, "each reply owns its buffer");
  assert.equal(new Uint8Array(one.params.data.bytes)[0], 8);
});

test("fonts: unknown ids, a wrong revision, malformed requests and a failing read are refused without detail", async () => {
  const fonts = fontsApi({ faces: [rawFace(7, 0, "Batang")], failRead: true });
  const { hostApi, studio } = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES }, extraDeps: { systemFonts: fonts.api } });
  await hostApi.onModuleActivated("HWP");
  await settle();
  const revision = "00ff00ff00ff00ff";
  const bad = [
    { requestId: 1, op: "read", face: "8:0", revision },
    { requestId: 2, op: "read", face: "7:0", revision: "ffffffffffffffff" },
    { requestId: 3, op: "read", face: "../../etc/passwd", revision },
    { requestId: 4, op: "read", face: "7:0" },
    { requestId: 5, op: "read", face: 70, revision },
    { requestId: 6, op: "write" },
    { requestId: 7, op: "read", face: "7:0", revision },
  ];
  for (const payload of bad) sendFontEvent(studio, payload);
  sendFontEvent(studio, { requestId: 0, op: "list" });
  sendFontEvent(studio, { requestId: 1.5, op: "list" });
  sendFontEvent(studio, { op: "list" });
  await settle();
  const replies = fontReplies(studio);
  assert.equal(replies.length, bad.length, "malformed request numbers get no answer");
  for (const reply of replies) {
    assert.ok("error" in reply.params && !("data" in reply.params));
    assert.ok(!JSON.stringify(reply.params).includes("/Users"), "no system text in the answer");
  }
  assert.deepEqual(fonts.calls.read, [[revision, 7]], "only the valid request reached the shell command");
});

test("fonts: only a connection the shell enabled fonts for answers, and a reconnect starts anew", async () => {
  const fonts = fontsApi({ faces: [rawFace(7, 0, "Batang")] });
  const { hostApi, studio } = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES }, extraDeps: { systemFonts: fonts.api } });
  await hostApi.onModuleActivated("HWP");
  await settle();
  await hostApi.reconnect();
  await settle();
  const before = fontReplies(studio).length;
  sendFontEvent(studio, { requestId: 1, op: "list" });
  await settle();
  assert.equal(fontReplies(studio).length, before + 1, "the new connection answers");
  const silent = loadHost({ studioOptions: { capabilities: ["transferable-array-buffer"] }, extraDeps: { systemFonts: fonts.api } });
  await silent.hostApi.onModuleActivated("HWP");
  await settle();
  sendFontEvent(silent.studio, { requestId: 1, op: "list" });
  await settle();
  assert.equal(fontReplies(silent.studio).length, 0, "not enabled: ignored");
});

test("fonts: an invalid list from the shell command is not forwarded", async () => {
  const hostile = { revision: "00ff", faces: [rawFace(1, 0, "Fine"), rawFace(2, 0, ""), { file: -1, faceIndex: 0 }, null, rawFace(3, 0, "X", { slant: "sideways" }), rawFace(4, 0, "Y", { aliases: Array(40).fill("a") })] };
  const { hostApi, studio } = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES }, extraDeps: { systemFonts: { list: async () => hostile, read: async () => new ArrayBuffer(1) } } });
  await hostApi.onModuleActivated("HWP");
  await settle();
  sendFontEvent(studio, { requestId: 1, op: "list" });
  await settle();
  assert.deepEqual(fontReplies(studio)[0].params.list.faces.map((f) => f.family), ["Fine"]);
  const broken = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES }, extraDeps: { systemFonts: { list: async () => ({ revision: "../x", faces: [] }), read: async () => new ArrayBuffer(1) } } });
  await broken.hostApi.onModuleActivated("HWP");
  await settle();
  sendFontEvent(broken.studio, { requestId: 1, op: "list" });
  await settle();
  assert.ok("error" in fontReplies(broken.studio)[0].params);
});

test("fonts: a file evicted while it loads is not counted in the cache size when it arrives", async () => {
  const MiB = 1024 * 1024;
  const sizes = { 1: 50 * MiB, 2: 100 * MiB, 3: 40 * MiB, 4: 80 * MiB };
  const reads = [];
  const gates = new Map();
  const faces = [1, 2, 3, 4].map((n) => rawFace(n, 0, "Face " + n));
  const systemFonts = {
    list: async () => ({ revision: "00ff", faces }),
    read: async (_revision, file) => {
      reads.push(file);
      if (gates.has(file)) await gates.get(file);
      const buffer = new ArrayBuffer(8);
      Object.defineProperty(buffer, "byteLength", { value: sizes[file] }); // counted size without the memory
      return buffer;
    },
  };
  const { hostApi, studio } = loadHost({ studioOptions: { capabilities: FONT_CAPABILITIES }, extraDeps: { systemFonts } });
  await hostApi.onModuleActivated("HWP");
  await settle();
  const ask = (requestId, file) => sendFontEvent(studio, { requestId, op: "read", face: file + ":0", revision: "00ff" });
  let openGate;
  gates.set(1, new Promise((resolve) => { openGate = resolve; }));
  ask(1, 1); // stays loading
  await settle();
  ask(2, 2); // 100 MiB
  await settle();
  ask(3, 3); // 140 MiB in all: evicts file 1 (still loading) and file 2
  await settle();
  openGate(); // file 1 arrives after it was evicted: 50 MiB must not be counted
  await settle();
  ask(4, 4); // 40 + 80 = 120 MiB fits the 128 MiB cache only if file 1 was not counted
  await settle();
  const before = reads.length;
  ask(5, 3);
  await settle();
  assert.equal(reads.length, before, "file 3 is still cached");
  assert.equal(fontReplies(studio).filter((r) => "data" in r.params).length, 5);
});
