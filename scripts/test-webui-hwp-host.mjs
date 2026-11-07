import test from "node:test";
import assert from "node:assert/strict";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

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

function createFakeStudio({ answer = true, failMethods = new Set() } = {}) {
  const requests = [];
  const state = { connect: null, port2: null, silent: !answer, answerConnect: answer };
  class FakeMessageChannel {
    constructor() {
      const port1 = { onmessage: null, closed: false, postMessage: (msg) => deliver(msg), close() { this.closed = true; } };
      const port2 = { postMessage: (msg) => queueMicrotask(() => port1.onmessage?.({ data: msg })) };
      this.port1 = port1;
      this.port2 = port2;
      state.port2 = port2;
    }
  }
  function deliver(msg) {
    requests.push(msg);
    if (state.silent) return;
    const respond = (extra) => state.port2.postMessage({ type: "rhwp-response", version: 1, sessionId: msg.sessionId, id: msg.id, ...extra });
    if (failMethods.has(msg.method)) return respond({ error: { code: "E", message: "fail " + msg.method } });
    const results = {
      ready: true,
      createBlankDocument: { pageCount: 1 },
      exportHwp: new Uint8Array([0xd0, 0xcf, 0x11, 0xe0, 1, 2, 3]),
      exportHwpx: new Uint8Array([0x50, 0x4b, 3, 4]),
      getDocumentState: { dirty: true },
      notifySaved: null,
    };
    respond({ result: results[msg.method] });
  }
  const attachWindow = (frame) => {
    frame.contentWindow = {
      postMessage(message, origin, ports) {
        state.connect = { message, origin, ports };
        if (state.answerConnect) queueMicrotask(() => state.port2.postMessage({ type: "rhwp-connected", version: 1, sessionId: message.sessionId, capabilities: ["transferable-array-buffer", "blank-document-v1"] }));
      },
    };
  };
  return { requests, state, FakeMessageChannel, attachWindow };
}

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
  return { hostApi, document, studio, states, ipcCalls, setModule: (m) => { mod = m; }, constants: env.__EASYOFFICE_HWP_HOST_CONSTANTS__ };
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
  assert.match(frame.getAttribute("sandbox"), /allow-same-origin/);
  assert.match(frame.getAttribute("sandbox"), /allow-scripts/);
  assert.doesNotMatch(frame.getAttribute("sandbox"), /allow-popups|allow-top-navigation|allow-forms/);
  const { message, origin, ports } = studio.state.connect;
  assert.equal(message.type, "rhwp-connect");
  assert.equal(message.version, 1);
  assert.equal(message.sessionId, "session-1");
  assert.ok(message.capabilities.includes("transferable-array-buffer"));
  assert.equal(origin, "http://127.0.0.1:8800", "target origin is the page origin, never *");
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

test("smoke round trip fails (and says so through the engine call) when typed input cannot reach the editor", async () => {
  const { hostApi, ipcCalls } = loadHost();
  await hostApi.runSmoke(() => hostApi.onModuleActivated("HWP"));
  assert.equal(ipcCalls.length, 1);
  assert.equal(ipcCalls[0].cmd, "scrub_pii");
  assert.match(ipcCalls[0].payload, /^EASYOFFICE_SMOKE_HWP_ROUNDTRIP_FAIL /);
});

test("smoke round trip fails when the bundle is missing", async () => {
  const { hostApi, ipcCalls } = loadHost({ fetchOk: false });
  await hostApi.runSmoke(() => hostApi.onModuleActivated("HWP"));
  assert.match(ipcCalls[0].payload, /ROUNDTRIP_FAIL .*구성 요소가 포함되지 않은 빌드/);
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
  assert.equal(document.getElementById("qat-save").disabled, false);
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
