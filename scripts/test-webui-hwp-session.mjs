import test from "node:test";
import assert from "node:assert/strict";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// HWP session (PD-085): open routing, size limit, text-only fallback, HWP/HWPX save choice,
// verify-before-deliver, dirty / notifySaved rules and the IndexedDB autosave. The editor host and
// IndexedDB are fakes; the bundle round trip runs in headless Chrome (scripts/hwp-roundtrip-cdp.mjs).

const SESSION_JS = new URL("../crates/easyoffice-app/ui/hwp-session.js", import.meta.url);
const HWP_MAGIC = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
const ZIP_MAGIC = [0x50, 0x4b, 0x03, 0x04];

function fakeIndexedDb() {
  const stores = { records: new Map(), meta: new Map() };
  const open = () => {
    const req = { result: null, onsuccess: null, onerror: null, onupgradeneeded: null };
    const db = {
      createObjectStore: () => {},
      transaction: () => {
        const tx = { oncomplete: null, onerror: null, onabort: null };
        let pending = 0;
        const finish = () => { pending -= 1; if (pending === 0) queueMicrotask(() => tx.oncomplete?.()); };
        const wrap = (name) => ({
          put: (value) => { stores[name].set(value.tabId, structuredClone(value)); return request(null); },
          delete: (key) => { stores[name].delete(key); return request(null); },
          get: (key) => request(stores[name].has(key) ? structuredClone(stores[name].get(key)) : undefined),
          getAll: () => request(Array.from(stores[name].values(), (v) => structuredClone(v))),
        });
        const request = (value) => {
          pending += 1;
          const r = { result: value, onsuccess: null };
          queueMicrotask(() => { r.onsuccess?.(); finish(); });
          return r;
        };
        tx.objectStore = wrap;
        queueMicrotask(() => { if (pending === 0) tx.oncomplete?.(); });
        return tx;
      },
    };
    queueMicrotask(() => { req.result = db; req.onupgradeneeded?.(); req.onsuccess?.(); });
    return req;
  };
  return { open, stores };
}

function createFakeHost({ status = "ready", capabilities = ["verify-bytes-v1", "save-protection-v1"] } = {}) {
  const calls = [];
  const changeListeners = new Set();
  const statusListeners = new Set();
  const state = { status, capabilities, handlers: {}, legacyUsed: 0, reconnects: 0, exports: { hwp: new Uint8Array([...HWP_MAGIC, 1, 2, 3]), hwpx: new Uint8Array([...ZIP_MAGIC, 9, 9]) } };
  const setStatus = (next) => { state.status = next; for (const l of statusListeners) l(next); };
  const api = {
    getStatus: () => state.status,
    getCapabilities: () => state.capabilities.slice(),
    isEmbedActive: () => state.status === "ready",
    usesLegacy: () => state.status === "legacy" || state.status === "unavailable",
    onChange: (l) => { changeListeners.add(l); return () => changeListeners.delete(l); },
    onStatus: (l) => { statusListeners.add(l); return () => statusListeners.delete(l); },
    whenSettled: async () => state.status,
    reconnect: async () => { state.reconnects += 1; setStatus("ready"); },
    useLegacyEditor: () => { state.legacyUsed += 1; setStatus("legacy"); },
    exportBytes: async (format) => {
      calls.push({ method: format === "hwpx" ? "exportHwpx" : "exportHwp" });
      if (state.handlers.exportBytes) return state.handlers.exportBytes(format);
      return state.exports[format];
    },
    call: async (method, params, options) => {
      calls.push({ method, params, options });
      const handler = state.handlers[method];
      if (handler) return handler(params, options);
      if (method === "loadFile") return { pageCount: 3 };
      if (method === "verifyBytes") return { ok: true, pageCount: 2, format: "hwp" };
      if (method === "getSaveProtection") return { passwordProtected: false };
      return null;
    },
  };
  return { api, calls, state, setStatus, emit: (payload) => { for (const l of changeListeners) l({ type: "rhwp-event", payload }); } };
}

const loadWithConfirm = (confirmImpl) => load({ confirmImpl });

function createDocument() {
  const byId = new Map();
  const tab = { dataset: { module: "HWP" }, textContent: "한글 문서" };
  const els = {};
  for (const id of ["status-command-feedback", "hwp-fallback-text"]) {
    els[id] = { id, textContent: "" };
    byId.set(id, els[id]);
  }
  return {
    els, tab,
    getElementById: (id) => byId.get(id) ?? null,
    querySelectorAll: () => [],
  };
}

function load(options = {}) {
  const env = {
    setTimeout: (fn, ms) => setTimeout(fn, Math.min(ms, 20)),
    clearTimeout,
    btoa: (s) => Buffer.from(s, "binary").toString("base64"),
    indexedDB: options.noIndexedDb ? null : fakeIndexedDb(),
    sessionStorage: { getItem: () => "tab-A", setItem: () => {} },
  };
  loadUiModule(SESSION_JS, env);
  const fake = createFakeHost(options.host);
  const document = createDocument();
  const ipc = [];
  const dirtyLog = [];
  const confirms = [];
  const answers = options.confirmAnswers || [];
  const session = env.__EASYOFFICE_CREATE_HWP_SESSION__({
    document, hwpHost: fake.api, env,
    getActiveModule: () => "HWP",
    setDocumentName: (name) => { document.tab.textContent = name; },
    activateHwp: async () => {},
    invokeIpc: async (cmd, payload) => {
      ipc.push({ cmd, payload: JSON.parse(payload) });
      if (options.detectFails) throw new Error("지원하지 않는 형식입니다");
      const name = JSON.parse(payload).fileName;
      return { format: name.split(".").pop().toLowerCase() };
    },
    confirm: options.noConfirm ? undefined : options.confirmImpl || (async (kind) => {
      confirms.push(kind);
      if (options.confirmThrows) throw new Error("확인 창을 열지 못했습니다");
      return answers.length > 0 ? answers.shift() : true;
    }),
    onDirtyChange: (d) => dirtyLog.push(d),
    resolveBaseName: () => "보고서",
    deliverSavedFile: options.deliver,
    pdfExport: options.pdfExport,
    setFeedback: options.setFeedback,
    defaultNotice: "기본 안내",
    autosaveIdleMs: 20, autosaveMaxWaitMs: 50,
  });
  return { session, fake, document, ipc, dirtyLog, confirms, env, utils: env.__EASYOFFICE_HWP_SESSION_UTILS__ };
}

const hwpBytes = (extra = 4) => new Uint8Array([...HWP_MAGIC, ...new Array(extra).fill(7)]);
const feedbackOf = (ctx) => ctx.document.els["status-command-feedback"].textContent;
const delay = (ms = 60) => new Promise((r) => setTimeout(r, ms));

function recordingDeliver(deliverResult) {
  const result = deliverResult ?? { ok: true, cancelled: false, name: "보고서.hwp", error: "" };
  const deliveries = [];
  const deliver = async (base64, name, ext) => {
    deliveries.push({ base64, name, ext });
    return typeof result === "function" ? result(ext) : result;
  };
  return { deliver, deliveries };
}

// ---- open ---------------------------------------------------------------------------------

test("open: detect_format with the file name hint and magic prefix, then loadFile with a transferred buffer", async () => {
  const ctx = load();
  const res = await ctx.session.openFromBytes(String.raw`C:\문서\계획서.hwp`, hwpBytes());
  assert.equal(res.ok, true);
  assert.equal(res.module, "HWP");
  assert.equal(ctx.ipc[0].cmd, "detect_format");
  assert.deepEqual(ctx.ipc[0].payload, { fileName: "계획서.hwp", hexPrefix: "d0cf11e0a1b11ae107070707" });
  const load1 = ctx.fake.calls.find((c) => c.method === "loadFile");
  assert.equal(load1.params.fileName, "계획서.hwp");
  assert.equal(load1.params.skipUnsavedGuard, true);
  assert.ok(load1.params.data instanceof ArrayBuffer);
  assert.equal(load1.options.transfer[0], load1.params.data);
  assert.ok(load1.options.timeoutMs >= 60000, "an encrypted document may wait for a password");
  assert.equal(ctx.document.tab.textContent, "계획서.hwp");
  assert.equal(JSON.stringify(ctx.session.getCurrent()), JSON.stringify({ name: "계획서.hwp", saveFormat: "hwp" }));
  assert.match(feedbackOf(ctx), /열기 완료: 계획서\.hwp/);
});

test("open: HWPX, HWTX and HWT name the save format; the caller's bytes stay usable (a copy is transferred)", async () => {
  const ctx = load();
  const source = new Uint8Array([...ZIP_MAGIC, 1, 2]);
  await ctx.session.openFromBytes("a.hwpx", source);
  assert.equal(ctx.session.getCurrent().saveFormat, "hwpx");
  assert.equal(source.length, 6, "the original array is not detached by the transfer");
  await ctx.session.openFromBytes("b.hwtx", source);
  assert.equal(ctx.session.getCurrent().saveFormat, "hwpx");
  await ctx.session.openFromBytes("c.hwt", hwpBytes());
  assert.equal(ctx.session.getCurrent().saveFormat, "hwp");
});

test("open: the 64 MiB limit gives a friendly error and never reaches the editor", async () => {
  const ctx = load();
  const big = new Uint8Array(64 * 1024 * 1024 + 1);
  const res = await ctx.session.openFromBytes("huge.hwp", big);
  assert.equal(res.ok, false);
  assert.match(feedbackOf(ctx), /너무 큽니다 \(64\.0 MiB\)\. 한글 문서는 최대 64 MiB/);
  assert.equal(ctx.ipc.length, 0);
  assert.equal(ctx.fake.calls.length, 0);
  const exact = await load().session.openFromBytes("edge.hwp", new Uint8Array(64 * 1024 * 1024));
  assert.equal(exact.ok, true, "exactly 64 MiB is accepted");
});

test("open: empty file and a detect_format rejection are refused without loading (fail-closed entry check)", async () => {
  const empty = load();
  assert.equal((await empty.session.openFromBytes("e.hwp", new Uint8Array(0))).ok, false);
  assert.match(feedbackOf(empty), /빈 파일/);
  const bad = load({ detectFails: true });
  const res = await bad.session.openFromBytes("fake.hwp", hwpBytes());
  assert.equal(res.ok, false);
  assert.match(feedbackOf(bad), /fake\.hwp — 지원하지 않는 형식/);
  assert.equal(bad.fake.calls.length, 0);
});

test("open: unsaved edits ask first and a refusal leaves the editor alone", async () => {
  const ctx = load({ confirmAnswers: [false] });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  const res = await ctx.session.openFromBytes("next.hwp", hwpBytes());
  assert.equal(res.cancelled, true);
  assert.equal(ctx.fake.calls.some((c) => c.method === "loadFile"), false);
  assert.equal(ctx.confirms[0], "replace-unsaved-hwp");
  assert.equal(ctx.session.isDirty(), true);
});

test("open: an editor parse failure falls back to the text-only decode with a clear notice", async () => {
  const ctx = load();
  ctx.fake.state.handlers.loadFile = async () => { throw new Error("파일 로드 실패: unexpected record"); };
  let fallbackCalls = 0;
  const res = await ctx.session.openFromBytes("old.hwp", hwpBytes(), { textFallback: async () => { fallbackCalls += 1; return { ok: true, module: "HWP", title: "old.hwp" }; } });
  assert.equal(fallbackCalls, 1);
  assert.equal(res.ok, true);
  assert.equal(res.textOnly, true);
  assert.equal(ctx.fake.state.legacyUsed, 1, "the text-only editor is shown");
  assert.match(feedbackOf(ctx), /텍스트만 열기: old\.hwp — .*서식·표·그림은 유지되지 않으며/);
  assert.match(ctx.document.els["hwp-fallback-text"].textContent, /텍스트만 불러왔습니다/);
});

test("open: password cancel is a cancel; DRM and unsupported encryption are refused, neither falls back to text", async () => {
  for (const [message, expectCancel] of [["문서 열기가 취소되었습니다.", true], ["DRM으로 보호된 문서는 지원하지 않습니다.", false], ["지원하지 않는 암호화 방식의 문서입니다.", false]]) {
    const ctx = load();
    ctx.fake.state.handlers.loadFile = async () => { throw new Error(message); };
    let fallback = 0;
    const res = await ctx.session.openFromBytes("secret.hwp", hwpBytes(), { textFallback: async () => { fallback += 1; return { ok: true }; } });
    assert.equal(fallback, 0, message);
    assert.equal(res.ok, false);
    assert.equal(Boolean(res.cancelled), expectCancel, message);
    assert.equal(ctx.fake.state.legacyUsed, 0);
    if (!expectCancel) assert.match(feedbackOf(ctx), /secret\.hwp — /);
  }
  const { classifyOpenError } = load().utils;
  assert.equal(classifyOpenError("암호화된 문서를 열 수 없습니다. 문서가 손상되었는지 확인하세요."), "refused");
  assert.equal(classifyOpenError("anything else"), "fallback");
});

test("open: a missing editor bundle goes straight to the text-only decode", async () => {
  const ctx = load({ host: { status: "unavailable" } });
  ctx.fake.api.whenSettled = async () => "unavailable";
  ctx.fake.api.reconnect = async () => { ctx.fake.state.reconnects += 1; };
  let fallback = 0;
  const res = await ctx.session.openFromBytes("x.hwp", hwpBytes(), { textFallback: async () => { fallback += 1; return { ok: true }; } });
  assert.equal(fallback, 1);
  assert.equal(res.textOnly, true);
  assert.equal(ctx.fake.calls.some((c) => c.method === "loadFile"), false);
  assert.equal(ctx.fake.state.reconnects, 1);
});

// ---- save ---------------------------------------------------------------------------------

test("save: an HWP document is exported as HWP, verified, delivered, then notifySaved clears the dirty state", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = load({ deliver });
  await ctx.session.openFromBytes("a.hwp", hwpBytes());
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.calls.length = 0;
  const res = await ctx.session.save({ deliver, baseName: "보고서" });
  assert.equal(res.ok, true);
  assert.deepEqual(ctx.fake.calls.map((c) => c.method), ["getSaveProtection", "exportHwp", "verifyBytes", "notifySaved"]);
  assert.equal(deliveries[0].ext, "hwp");
  assert.equal(Buffer.from(deliveries[0].base64, "base64")[0], 0xd0);
  assert.equal(ctx.session.isDirty(), false);
  assert.deepEqual(ctx.dirtyLog, [true, false]);
  assert.match(res.message, /문서 저장 완료 \(보고서\.hwp · HWP/);
});

test("save: a document opened as HWPX saves as HWPX, and a later save-as HWP switches the format", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = load({ deliver });
  await ctx.session.openFromBytes("a.hwpx", new Uint8Array([...ZIP_MAGIC, 1]));
  const first = await ctx.session.save({ deliver, baseName: "a" });
  assert.equal(first.format, "hwpx");
  assert.equal(deliveries[0].ext, "hwpx");
  assert.match(first.message, /HWPX는 일부 서식이 HWP와 다르게/);
  const second = await ctx.session.save({ deliver, baseName: "a", format: "hwp" });
  assert.equal(second.format, "hwp");
  assert.equal(ctx.session.getCurrent().saveFormat, "hwp");
  await ctx.session.save({ deliver, baseName: "a" });
  assert.deepEqual(deliveries.map((d) => d.ext), ["hwpx", "hwp", "hwp"]);
});

// ---- preview picture (PD-132) ---------------------------------------------------------------

const PAGE_SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="794" height="1123"/>';
const withPrint = { capabilities: ["verify-bytes-v1", "save-protection-v1", "print-pages-v1"] };

function recordingDeliverWithOptions() {
  const deliveries = [];
  const deliver = async (base64, name, ext, options) => {
    deliveries.push({ base64, name, ext, options });
    return { ok: true, cancelled: false, name: name + "." + ext, error: "" };
  };
  return { deliver, deliveries };
}

test("save: the first page is requested after the export and handed to the shell with the bytes", async () => {
  const { deliver, deliveries } = recordingDeliverWithOptions();
  const ctx = load({ deliver, host: withPrint });
  ctx.fake.state.handlers.getPrintPages = async () => ({ pageCount: 3, pages: [{ svg: PAGE_SVG, widthMm: 210, heightMm: 297 }] });
  await ctx.session.openFromBytes("a.hwp", hwpBytes());
  ctx.fake.calls.length = 0;
  const res = await ctx.session.save({ deliver, baseName: "보고서" });
  assert.equal(res.ok, true);
  assert.deepEqual(ctx.fake.calls.map((c) => c.method), ["getSaveProtection", "exportHwp", "verifyBytes", "getPrintPages", "notifySaved"]);
  const request = ctx.fake.calls.find((c) => c.method === "getPrintPages");
  assert.deepEqual(request.params, { from: 0, count: 1 });
  assert.ok(request.options.timeoutMs <= 15000, "a stuck editor does not hold the save for long");
  assert.deepEqual(deliveries[0].options, { thumbnailSvg: PAGE_SVG });
});

test("save: without print support, or when the page cannot be had, the document is delivered without a picture", async () => {
  const bad = [
    ["throws", async () => { throw new Error("렌더링 실패"); }],
    ["no pages", async () => ({ pageCount: 0, pages: [] })],
    ["not a string", async () => ({ pageCount: 1, pages: [{ svg: 42 }] })],
    ["empty", async () => ({ pageCount: 1, pages: [{ svg: "" }] })],
    ["oversized", async () => ({ pageCount: 1, pages: [{ svg: "x".repeat(16 * 1024 * 1024 + 1) }] })],
    ["garbage", async () => null],
  ];
  for (const [label, handler] of bad) {
    const { deliver, deliveries } = recordingDeliverWithOptions();
    const ctx = load({ deliver, host: withPrint });
    ctx.fake.state.handlers.getPrintPages = handler;
    const res = await ctx.session.save({ deliver, baseName: "a" });
    assert.equal(res.ok, true, label);
    assert.equal(deliveries[0].options, undefined, label);
  }
  const { deliver, deliveries } = recordingDeliverWithOptions();
  const ctx = load({ deliver });
  const res = await ctx.session.save({ deliver, baseName: "a" });
  assert.equal(res.ok, true);
  assert.equal(ctx.fake.calls.some((c) => c.method === "getPrintPages"), false, "no call to an editor that cannot print");
  assert.equal(deliveries[0].options, undefined);
});

test("save: a password document closing the window goes through the save-as delivery with the same picture", async () => {
  const { deliver, deliveries } = recordingDeliverWithOptions();
  const inPlace = recordingDeliverWithOptions();
  const ctx = load({ deliver: inPlace.deliver, host: withPrint });
  ctx.fake.state.handlers.getSaveProtection = async () => ({ passwordProtected: true });
  ctx.fake.state.handlers.getPrintPages = async () => ({ pageCount: 1, pages: [{ svg: PAGE_SVG }] });
  const res = await ctx.session.save({ deliver, baseName: "a", inPlace: true, closing: true });
  assert.equal(res.ok, true);
  assert.equal(deliveries.length, 0);
  assert.deepEqual(inPlace.deliveries[0].options, { thumbnailSvg: PAGE_SVG });
});

test("save: a verification failure refuses delivery and keeps the dirty baseline", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = load({ deliver });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.state.handlers.verifyBytes = async () => { throw new Error("parse error"); };
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.ok, false);
  assert.match(res.message, /문서 저장 실패 \(HWP\): 저장 전 검증에 실패했습니다: parse error/);
  assert.equal(deliveries.length, 0);
  assert.equal(ctx.fake.calls.some((c) => c.method === "notifySaved"), false);
  assert.equal(ctx.session.isDirty(), true);
  assert.equal(ctx.confirms.length, 0, "HWP failures do not offer an alternative format");
});

test("save: HWPX verification failure offers HWP; accepting saves HWP, declining saves nothing", async () => {
  for (const [accept, expectedDeliveries] of [[true, ["hwp"]], [false, []]]) {
    const { deliver, deliveries } = recordingDeliver();
    const ctx = load({ deliver, confirmAnswers: [accept] });
    await ctx.session.openFromBytes("a.hwpx", new Uint8Array([...ZIP_MAGIC, 1]));
    ctx.fake.emit({ reason: "dirty_changed", dirty: true });
    ctx.fake.state.handlers.verifyBytes = async (params) => {
      const first = ctx.fake.calls.some((c) => c.method === "exportHwpx") && ctx.fake.calls.filter((c) => c.method === "exportHwp").length === 0;
      if (first) throw new Error("hwpx unreadable");
      return { ok: true, pageCount: 1, format: "hwp", params };
    };
    const res = await ctx.session.save({ deliver, baseName: "a" });
    assert.equal(ctx.confirms[0], "hwpx-fallback-hwp");
    assert.match(feedbackOf(ctx), /HWPX로 저장하기 전 검증에 실패했습니다/);
    assert.deepEqual(deliveries.map((d) => d.ext), expectedDeliveries);
    assert.equal(res.ok, accept);
    if (accept) {
      assert.match(res.message, /HWPX 검증 실패로 HWP로 저장/);
      assert.equal(ctx.session.isDirty(), false);
      assert.equal(ctx.session.getCurrent().saveFormat, "hwp");
    } else {
      assert.equal(ctx.session.isDirty(), true);
    }
  }
});

test("save: cancelled dialog and delivery failure both keep the dirty baseline and skip notifySaved", async () => {
  for (const [result, pattern] of [
    [{ ok: false, cancelled: true, name: "", error: "" }, /문서 저장 취소 \(HWP\): 저장 위치를 선택하지 않았습니다/],
    [{ ok: false, cancelled: false, name: "", error: "디스크 가득 참" }, /문서 저장 실패 \(HWP\): 디스크 가득 참/],
  ]) {
    const { deliver } = recordingDeliver(result);
    const ctx = load({ deliver });
    ctx.fake.emit({ reason: "dirty_changed", dirty: true });
    const res = await ctx.session.save({ deliver, baseName: "x" });
    assert.equal(res.ok, false);
    assert.match(res.message, pattern);
    assert.equal(ctx.fake.calls.some((c) => c.method === "notifySaved"), false);
    assert.equal(ctx.session.isDirty(), true);
  }
});

test("save: edits made while the save runs keep the document marked as modified", async () => {
  const ctx = load();
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  const deliver = async () => {
    ctx.fake.emit({ reason: "edited" });
    return { ok: true, cancelled: false, name: "p.hwp", error: "" };
  };
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.ok, true);
  assert.match(res.message, /수정됨 상태를 유지합니다/);
  assert.equal(ctx.fake.calls.some((c) => c.method === "notifySaved"), false);
  assert.equal(ctx.session.isDirty(), true);
});

test("save: an editor without verify support is refused (fail-closed) and a not-ready editor reports it", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = load({ deliver, host: { capabilities: ["save-protection-v1"] } });
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.ok, false);
  assert.match(res.message, /저장 검증 기능을 사용할 수 없습니다/);
  assert.equal(deliveries.length, 0);
  const idle = load({ deliver, host: { status: "error" } });
  assert.match((await idle.session.save({ deliver, baseName: "x" })).message, /준비되지 않았습니다/);
});

test("saveAs from the menu delegates to save with the chosen format and reports the message", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = load({ deliver });
  const res = await ctx.session.saveAs("hwpx");
  assert.equal(res.ok, true);
  assert.equal(deliveries[0].ext, "hwpx");
  assert.equal(deliveries[0].name, "보고서");
  assert.match(feedbackOf(ctx), /문서 저장 완료/);
});

// ---- password documents (PD-120) -------------------------------------------------------------

function loadProtected(options = {}) {
  const ctx = load(options);
  ctx.protection = { passwordProtected: true, cleared: 0 };
  ctx.fake.state.handlers.getSaveProtection = async () => ({ passwordProtected: ctx.protection.passwordProtected });
  ctx.fake.state.handlers.clearSaveProtection = async () => {
    ctx.protection.cleared += 1;
    ctx.protection.passwordProtected = false;
    return { cleared: true };
  };
  return ctx;
}

const methodsOf = (ctx) => ctx.fake.calls.map((c) => c.method);

test("password document: a save over the opened file is refused with the save-as hint, nothing is exported or delivered", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = loadProtected({ deliver });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.calls.length = 0;
  const res = await ctx.session.save({ deliver, inPlace: true, baseName: "x" });
  assert.equal(res.ok, false);
  assert.equal(res.cancelled, false);
  assert.match(res.message, /암호 문서의 원본 저장은 아직 지원하지 않습니다\. 다른 이름으로 저장에서 암호 없는 HWP 파일로 저장하세요/);
  assert.deepEqual(methodsOf(ctx), ["getSaveProtection"]);
  assert.equal(deliveries.length, 0);
  assert.equal(ctx.confirms.length, 0, "a refused overwrite asks nothing");
  assert.equal(ctx.session.isDirty(), true);
});

test("password document: save-as asks once, delivers after consent, clears the editor flag and the dirty state", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = loadProtected({ deliver });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.calls.length = 0;
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.ok, true);
  assert.deepEqual(ctx.confirms, ["password-free-save"]);
  assert.deepEqual(methodsOf(ctx), ["getSaveProtection", "exportHwp", "verifyBytes", "clearSaveProtection", "notifySaved"]);
  assert.equal(deliveries.length, 1);
  assert.match(res.message, /저장한 파일에는 암호가 없습니다/);
  assert.equal(ctx.protection.cleared, 1);
  assert.equal(ctx.session.isDirty(), false);
  const again = await ctx.session.save({ deliver, inPlace: true, baseName: "x" });
  assert.equal(again.ok, true, "the saved copy has no password, so saving over it is allowed");
  assert.equal(ctx.confirms.length, 1);
});

test("password document: declining the save-as confirmation saves nothing and keeps everything as it was", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = loadProtected({ deliver, confirmAnswers: [false] });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.calls.length = 0;
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.ok, false);
  assert.equal(res.cancelled, true);
  assert.match(res.message, /문서 저장 취소 \(HWP\): 암호 없이 저장하지 않았습니다/);
  assert.deepEqual(methodsOf(ctx), ["getSaveProtection"]);
  assert.equal(deliveries.length, 0);
  assert.equal(ctx.protection.passwordProtected, true);
  assert.equal(ctx.session.isDirty(), true);
});

test("password document: when the window closes, save turns into a confirmed save-as through the dialog delivery", async () => {
  const inPlace = recordingDeliver();
  const dialog = recordingDeliver();
  const ctx = loadProtected({ deliver: dialog.deliver });
  const res = await ctx.session.save({ deliver: inPlace.deliver, inPlace: true, closing: true, baseName: "x" });
  assert.equal(res.ok, true);
  assert.equal(ctx.confirms.length, 1);
  assert.equal(inPlace.deliveries.length, 0, "never the overwrite delivery");
  assert.equal(dialog.deliveries.length, 1);
});

test("password document: declining while the window closes ends the save as cancelled, so the close or quit is cancelled", async () => {
  const inPlace = recordingDeliver();
  const dialog = recordingDeliver();
  const ctx = loadProtected({ deliver: dialog.deliver, confirmAnswers: [false] });
  const res = await ctx.session.save({ deliver: inPlace.deliver, inPlace: true, closing: true, baseName: "x" });
  assert.equal(res.ok, false);
  assert.equal(res.cancelled, true);
  assert.equal(inPlace.deliveries.length + dialog.deliveries.length, 0);
});

test("password document: a cancelled file dialog after consent keeps the flag", async () => {
  const { deliver } = recordingDeliver({ ok: false, cancelled: true, name: "", error: "" });
  const ctx = loadProtected({ deliver });
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.cancelled, true);
  assert.equal(ctx.protection.cleared, 0);
  assert.equal(ctx.protection.passwordProtected, true);
});

test("password document: edits during the save keep the dirty state but the flag is still cleared", async () => {
  const ctx = loadProtected();
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  const deliver = async () => {
    ctx.fake.emit({ reason: "edited" });
    return { ok: true, cancelled: false, name: "p.hwp", error: "" };
  };
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.ok, true);
  assert.equal(ctx.protection.cleared, 1);
  assert.equal(ctx.session.isDirty(), true);
});

test("plain document: save, save-as and closing saves are not affected and ask nothing", async () => {
  const inPlace = recordingDeliver();
  const dialog = recordingDeliver();
  const ctx = load({ deliver: dialog.deliver });
  for (const options of [{ inPlace: true }, {}, { inPlace: true, closing: true }]) {
    const res = await ctx.session.save({ deliver: inPlace.deliver, baseName: "x", ...options });
    assert.equal(res.ok, true);
  }
  assert.equal(ctx.confirms.length, 0);
  assert.equal(inPlace.deliveries.length, 3);
  assert.equal(dialog.deliveries.length, 0);
  assert.equal(methodsOf(ctx).includes("clearSaveProtection"), false);
});

test("password document: an editor that cannot tell, or answers badly, fails the save closed", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const missing = load({ deliver, host: { capabilities: ["verify-bytes-v1"] } });
  assert.match((await missing.session.save({ deliver, baseName: "x" })).message, /암호 문서 확인 기능을 사용할 수 없습니다/);
  const bad = load({ deliver });
  bad.fake.state.handlers.getSaveProtection = async () => ({ passwordProtected: "no" });
  assert.match((await bad.session.save({ deliver, baseName: "x" })).message, /암호 문서 확인 결과를 받지 못했습니다/);
  const broken = load({ deliver });
  broken.fake.state.handlers.getSaveProtection = async () => { throw new Error("port closed"); };
  assert.match((await broken.session.save({ deliver, baseName: "x" })).message, /port closed/);
  assert.equal(deliveries.length, 0);
});

test("password document: autosave never writes a plain copy, removes an old record and says so once", async () => {
  const ctx = loadProtected();
  ctx.env.indexedDB.stores.records.set("tab-A", { tabId: "tab-A" });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.emit({ reason: "edited" });
  await delay(100);
  assert.equal(ctx.env.indexedDB.stores.records.size, 0);
  assert.equal(methodsOf(ctx).some((m) => m === "exportHwp" || m === "exportHwpx"), false);
  assert.match(feedbackOf(ctx), /암호 문서는 자동 저장하지 않습니다/);
});

test("password document: after a save-as that dropped the password, autosave works again", async () => {
  const { deliver } = recordingDeliver();
  const ctx = loadProtected({ deliver });
  await ctx.session.save({ deliver, baseName: "x" });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.emit({ reason: "edited" });
  await delay(100);
  assert.equal(ctx.env.indexedDB.stores.records.size, 1);
});

test("confirmations default to no: without a way to ask, nothing is overwritten, replaced or restored", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = loadProtected({ deliver, noConfirm: true });
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.cancelled, true);
  assert.equal(deliveries.length, 0);
  const open = load({ noConfirm: true });
  open.fake.emit({ reason: "dirty_changed", dirty: true });
  const opened = await open.session.openFromBytes("b.hwp", hwpBytes());
  assert.equal(opened.cancelled, true);
  assert.equal(open.fake.calls.some((c) => c.method === "loadFile"), false);
  const restore = load({ noConfirm: true });
  seedRecord(restore);
  assert.equal((await restore.session.restore()).restored, false);
  assert.equal(restore.fake.calls.some((c) => c.method === "loadFile"), false);
});

test("a confirmation that cannot be shown counts as no, and keeps the autosaved copy for the next start", async () => {
  const { deliver, deliveries } = recordingDeliver();
  const ctx = loadProtected({ deliver, confirmThrows: true });
  const res = await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(res.cancelled, true);
  assert.equal(deliveries.length, 0);
  assert.match(feedbackOf(ctx), /확인 창을 열지 못했습니다/);
  const restore = load({ confirmThrows: true });
  seedRecord(restore);
  assert.equal((await restore.session.restore()).restored, false);
  assert.equal(restore.env.indexedDB.stores.records.size, 1, "the recovery copy is not deleted when the question could not be asked");
  const declined = load({ confirmAnswers: [false] });
  seedRecord(declined);
  assert.equal((await declined.session.restore()).restored, false);
  assert.equal(declined.env.indexedDB.stores.records.size, 1, "a no cannot be told from a closed dialog, so the copy stays");
  assert.match(feedbackOf(declined), /자동 저장본은 남겨 두었습니다/);
});

test("while a confirmation is up the session is busy: no second open and no autosave", async () => {
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const answers = [];
  const waiting = loadWithConfirm(async (kind) => { answers.push(kind); await gate; return true; });
  waiting.fake.emit({ reason: "dirty_changed", dirty: true });
  const first = waiting.session.openFromBytes("a.hwp", hwpBytes());
  await delay(5);
  const second = await waiting.session.openFromBytes("b.hwp", hwpBytes());
  assert.equal(second.ok, false);
  assert.match(second.error, /다른 문서를 여는 중/);
  waiting.fake.emit({ reason: "edited" });
  await delay(80);
  assert.equal(waiting.fake.calls.some((c) => c.method === "exportHwp"), false, "no autosave under the dialog");
  release();
  assert.equal((await first).ok, true);
  assert.deepEqual(answers, ["replace-unsaved-hwp"]);
});

test("restore: an accepted restore is dropped when the window was torn down while the question was up", async () => {
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const ctx = loadWithConfirm(async () => { await gate; return true; });
  seedRecord(ctx);
  const pending = ctx.session.restore();
  await delay(5);
  ctx.session.discardAutosave();
  release();
  assert.equal((await pending).restored, false);
  assert.equal(ctx.fake.calls.some((c) => c.method === "loadFile"), false);
});

// ---- dirty state and autosave -----------------------------------------------------------

test("dirty state follows the editor events and resets when the editor goes away", async () => {
  const ctx = load();
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  assert.equal(ctx.session.isDirty(), true);
  ctx.fake.setStatus("error");
  assert.equal(ctx.session.isDirty(), false);
  assert.deepEqual(ctx.dirtyLog, [true, false]);
});

test("autosave: debounced edits write one record per tab with the exported bytes", async () => {
  const ctx = load();
  await ctx.session.openFromBytes("a.hwpx", new Uint8Array([...ZIP_MAGIC, 1]));
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  for (let i = 0; i < 5; i += 1) ctx.fake.emit({ reason: "edited" });
  await delay(120);
  const record = ctx.env.indexedDB.stores.records.get("tab-A");
  assert.ok(record, "record stored under the tab id");
  assert.equal(record.format, "hwpx");
  assert.equal(record.fileName, "a.hwpx");
  assert.deepEqual(Array.from(new Uint8Array(record.bytes)), Array.from(ctx.fake.state.exports.hwpx));
  assert.equal(ctx.env.indexedDB.stores.records.size, 1);
  assert.equal(ctx.fake.calls.filter((c) => c.method === "exportHwpx").length, 1, "five edits, one export");
});

test("autosave: not dirty means no record; a size above the cap is skipped and the old record removed", async () => {
  const clean = load();
  clean.fake.emit({ reason: "edited" });
  await delay(80);
  assert.equal(clean.env.indexedDB.stores.records.size, 0);

  const big = load();
  big.env.indexedDB.stores.records.set("tab-A", { tabId: "tab-A" });
  big.fake.emit({ reason: "dirty_changed", dirty: true });
  big.fake.state.handlers.exportBytes = async () => new Uint8Array(64 * 1024 * 1024 + 1);
  big.fake.emit({ reason: "edited" });
  await delay(100);
  assert.equal(big.env.indexedDB.stores.records.size, 0);
  assert.match(feedbackOf(big), /자동 저장 건너뜀/);
});

test("autosave: a successful save clears the record", async () => {
  const { deliver } = recordingDeliver();
  const ctx = load({ deliver });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.emit({ reason: "edited" });
  await delay(100);
  assert.equal(ctx.env.indexedDB.stores.records.size, 1);
  await ctx.session.save({ deliver, baseName: "x" });
  assert.equal(ctx.env.indexedDB.stores.records.size, 0);
  assert.equal(ctx.env.indexedDB.stores.meta.size, 0);
});

test("autosave failures never throw into the editor: storage errors become a status message", async () => {
  const ctx = load({ noIndexedDb: true });
  ctx.fake.emit({ reason: "dirty_changed", dirty: true });
  ctx.fake.emit({ reason: "edited" });
  await delay(100);
  assert.match(feedbackOf(ctx), /자동 저장 실패: IndexedDB를 사용할 수 없습니다/);
  assert.equal(ctx.session.isDirty(), true);
});

// ---- restore ----------------------------------------------------------------------------

function seedRecord(ctx, overrides = {}) {
  const bytes = overrides.bytes ?? hwpBytes().buffer;
  const record = { tabId: "tab-A", fileName: "복구.hwp", format: "hwp", savedAt: Date.now(), size: 12, bytes, ...overrides };
  ctx.env.indexedDB.stores.records.set("tab-A", record);
  ctx.env.indexedDB.stores.meta.set("tab-A", { tabId: "tab-A", savedAt: record.savedAt });
}

test("restore: a valid record is validated with detect_format, loaded and marked as modified", async () => {
  const ctx = load();
  seedRecord(ctx);
  const res = await ctx.session.restore();
  assert.equal(JSON.stringify(res), JSON.stringify({ restored: true, name: "복구.hwp" }));
  assert.equal(ctx.ipc[0].payload.fileName, "autosave.hwp");
  assert.equal(ctx.fake.calls.some((c) => c.method === "loadFile"), true);
  assert.equal(ctx.session.isDirty(), true);
  assert.equal(ctx.document.tab.textContent, "복구.hwp");
  assert.equal(ctx.env.indexedDB.stores.records.size, 1, "the record stays until the document is saved");
});

test("restore: corrupted records are ignored and deleted (bad shape, wrong type, failed format check)", async () => {
  const shapes = [
    { bytes: undefined }, { bytes: new ArrayBuffer(0) }, { format: "docx" }, { fileName: "" }, { savedAt: "x" },
  ];
  for (const bad of shapes) {
    const ctx = load();
    seedRecord(ctx, bad);
    if (!("bytes" in bad)) seedRecord(ctx, { bytes: null });
    const res = await ctx.session.restore();
    assert.equal(res.restored, false, JSON.stringify(Object.keys(bad)));
    assert.equal(ctx.env.indexedDB.stores.records.size, 0);
    assert.equal(ctx.fake.calls.some((c) => c.method === "loadFile"), false);
  }
  const rejected = load({ detectFails: true });
  seedRecord(rejected);
  assert.equal((await rejected.session.restore()).restored, false);
  assert.equal(rejected.env.indexedDB.stores.records.size, 0);
  assert.match(feedbackOf(rejected), /올바른 형식이 아니어서 무시/);
});

test("restore: declining keeps the record, no record is a no-op, stale foreign records are purged", async () => {
  const declined = load({ confirmAnswers: [false] });
  seedRecord(declined);
  assert.equal((await declined.session.restore()).restored, false);
  assert.equal(declined.env.indexedDB.stores.records.size, 1);
  assert.equal(declined.fake.calls.length, 0);

  const none = load();
  assert.equal((await none.session.restore()).restored, false);

  const purge = load();
  const old = Date.now() - 8 * 24 * 3600 * 1000;
  purge.env.indexedDB.stores.records.set("tab-old", { tabId: "tab-old" });
  purge.env.indexedDB.stores.meta.set("tab-old", { tabId: "tab-old", savedAt: old });
  purge.env.indexedDB.stores.records.set("tab-new", { tabId: "tab-new" });
  purge.env.indexedDB.stores.meta.set("tab-new", { tabId: "tab-new", savedAt: Date.now() });
  await purge.session.restore();
  assert.deepEqual(Array.from(purge.env.indexedDB.stores.records.keys()), ["tab-new"]);
});

// ---- converter ----------------------------------------------------------------------------

test("converter: the open HWP document is exported by the editor in the chosen source format", async () => {
  const ctx = load();
  assert.equal(await ctx.session.getConverterSource("hwp"), null, "an untouched blank editor converts the editor text path as before");
  await ctx.session.openFromBytes("제안서.hwpx", new Uint8Array([...ZIP_MAGIC, 1]));
  const hwp = await ctx.session.getConverterSource("hwp");
  assert.equal(hwp.baseName, "제안서");
  assert.equal(hwp.fields.fileName, "제안서.hwp");
  assert.equal(Buffer.from(hwp.fields.sourceBase64, "base64")[0], 0xd0);
  const hwpx = await ctx.session.getConverterSource("hwpx");
  assert.equal(hwpx.fields.fileName, "제안서.hwpx");
  assert.equal(Buffer.from(hwpx.fields.sourceBase64, "base64")[0], 0x50);
  assert.equal(await ctx.session.getConverterSource("docx"), null);
});

test("utils: only the HWP family is routed and the limit is 64 MiB", () => {
  const { utils } = load();
  assert.equal(utils.MAX_HWP_BYTES, 64 * 1024 * 1024);
  for (const name of ["a.hwp", "a.HWPX", "a.hwt", String.raw`dir\a.hwtx`]) assert.equal(utils.isHwpName(name), true, name);
  for (const name of ["a.docx", "hwp", "a.hwp.txt"]) assert.equal(utils.isHwpName(name), false, name);
});

// ---- searchable PDF export (PD-101, PD-123) ------------------------------------------------------

const PDF_SAVED = Object.freeze({ status: "saved", name: "보고서.pdf", bytes: 10, pages: 3, opened: true });

// A fake of the shell's three PDF commands (`desktop-bridge.js` `pdfExport`). `log` is what the shell
// saw in order; `renderedAt` is how many page SVGs the editor had been asked for at each shell call, so
// a test can tell when a page was rendered relative to the dialog and to the batch before it.
function pdfShell(ctx, { start = { status: "started", jobId: 7 }, finish = PDF_SAVED, failPages = null } = {}) {
  const log = [];
  const renderedAt = [];
  const rendered = () => ctx.fake.calls.filter((c) => c.method === "getPageSvg").length;
  const record = (entry) => { log.push(entry); renderedAt.push(rendered()); };
  return {
    log, renderedAt,
    start: async (fileName) => {
      record({ op: "start", fileName });
      if (start instanceof Error) throw start;
      return start;
    },
    pages: async (jobId, svgs) => {
      record({ op: "pages", jobId, svgs });
      if (failPages) throw failPages;
      return { pages: log.filter((e) => e.op === "pages").reduce((n, e) => n + e.svgs.length, 0) };
    },
    finish: async (jobId, commit) => {
      record({ op: "finish", jobId, commit });
      if (!commit) return { status: "cancelled" };
      if (finish instanceof Error) throw finish;
      return finish;
    },
  };
}

function pdfPages(ctx, count, pageFor = (i) => `<svg id="p${i}"/>`) {
  ctx.fake.state.handlers.pageCount = () => count;
  ctx.fake.state.handlers.getPageSvg = ({ page }) => pageFor(page);
}

// `load` needs the shell before the fake host exists, so the shell is bound afterwards.
function loadPdf(shellOptions, loadOptions = {}) {
  let shell = null;
  const proxy = {
    start: (...args) => shell.start(...args),
    pages: (...args) => shell.pages(...args),
    finish: (...args) => shell.finish(...args),
  };
  const ctx = load({ ...loadOptions, pdfExport: proxy });
  shell = pdfShell(ctx, shellOptions);
  return { ctx, shell };
}

test("PDF export: the location is asked first, then the pages go to the shell ten at a time and the file is finished", async () => {
  const { ctx, shell } = loadPdf();
  pdfPages(ctx, 23);
  const result = await ctx.session.exportPdf();
  assert.equal(result.status, "saved");
  assert.deepEqual(shell.log.map((e) => [e.op, e.svgs?.length ?? e.fileName ?? e.commit]), [
    ["start", "보고서.pdf"], ["pages", 10], ["pages", 10], ["pages", 3], ["finish", true],
  ]);
  assert.ok(shell.log.every((e) => e.jobId === undefined || e.jobId === 7), "every call names the job the shell opened");
  assert.deepEqual(shell.log.filter((e) => e.op === "pages").flatMap((e) => e.svgs), Array.from({ length: 23 }, (_, i) => `<svg id="p${i}"/>`), "all pages, in order");
  const fetched = ctx.fake.calls.filter((c) => c.method === "getPageSvg");
  assert.deepEqual(fetched.map((c) => c.params.page), Array.from({ length: 23 }, (_, i) => i));
  assert.ok(fetched.every((c) => c.options.timeoutMs >= 60000), "page rendering gets a long timeout");
  assert.match(feedbackOf(ctx), /^PDF 내보내기 완료 \(보고서\.pdf · 23쪽\)$/);
});

test("PDF export: cancelling the save dialog renders nothing and sends nothing", async () => {
  const { ctx, shell } = loadPdf({ start: { status: "cancelled" } });
  pdfPages(ctx, 40);
  const result = await ctx.session.exportPdf();
  assert.equal(result.status, "cancelled");
  assert.deepEqual(shell.log.map((e) => e.op), ["start"], "no pages, no finish");
  assert.equal(ctx.fake.calls.filter((c) => c.method === "getPageSvg").length, 0, "not one page was rendered");
  assert.match(feedbackOf(ctx), /^PDF 내보내기 취소: 저장 위치를 선택하지 않았습니다$/);
});

test("PDF export: a batch is rendered only after the shell took the one before, so at most ten page SVGs wait at a time", async () => {
  const { ctx, shell } = loadPdf();
  pdfPages(ctx, 23);
  await ctx.session.exportPdf();
  assert.deepEqual(shell.log.map((e) => e.op), ["start", "pages", "pages", "pages", "finish"]);
  assert.deepEqual(shell.renderedAt, [0, 10, 20, 23, 23], "page 11 is asked for after batch 1 was handed over, and so on");
});

test("PDF export: the status line counts the pages as they arrive and ends with the result", async () => {
  const lines = [];
  const { ctx } = loadPdf({ finish: { ...PDF_SAVED, pages: 3 } }, { setFeedback: (line) => lines.push(line) });
  pdfPages(ctx, 3);
  await ctx.session.exportPdf();
  assert.deepEqual(lines, [
    "PDF 만드는 중: 0 / 3쪽",
    "PDF 만드는 중: 1 / 3쪽",
    "PDF 만드는 중: 2 / 3쪽",
    "PDF 만드는 중: 3 / 3쪽",
    "PDF 내보내기 완료 (보고서.pdf · 3쪽)",
  ]);
});

test("PDF export: a file that was saved but could not be opened says so", async () => {
  const { ctx } = loadPdf({ finish: { ...PDF_SAVED, opened: false } });
  pdfPages(ctx, 2);
  assert.equal((await ctx.session.exportPdf()).status, "saved");
  assert.equal(feedbackOf(ctx), "PDF 내보내기 완료 (보고서.pdf · 2쪽) · 저장했으나 기본 앱으로 열지 못했습니다");
});

test("PDF export: not available without the desktop shell or while the editor is not ready", async () => {
  const plain = load();
  pdfPages(plain, 2);
  assert.equal(await plain.session.exportPdf(), null);
  assert.match(feedbackOf(plain), /데스크톱 앱의 한글 편집기에서만/);
  assert.equal(plain.fake.calls.filter((c) => c.method === "getPageSvg").length, 0);
  const { ctx: legacy, shell } = loadPdf({}, { host: { status: "legacy" } });
  assert.equal(await legacy.session.exportPdf(), null);
  assert.deepEqual(shell.log, []);
});

test("PDF export: the page count is checked against the 2,000 page ceiling before the dialog and before any page is rendered", async () => {
  for (const count of [0, 2001, -1, 1.5, "3", null]) {
    const { ctx, shell } = loadPdf();
    pdfPages(ctx, count);
    assert.equal(await ctx.session.exportPdf(), null, String(count));
    assert.match(feedbackOf(ctx), /^PDF 내보내기 실패: 쪽 수/);
    assert.equal(ctx.fake.calls.filter((c) => c.method === "getPageSvg").length, 0);
    assert.deepEqual(shell.log, [], "no dialog for a document the shell would refuse");
  }
  const { ctx: edge, shell } = loadPdf();
  pdfPages(edge, 2000);
  assert.equal((await edge.session.exportPdf()).status, "saved");
  const batches = shell.log.filter((e) => e.op === "pages");
  assert.equal(batches.length, 200);
  assert.ok(batches.every((e) => e.svgs.length === 10));
});

test("PDF export: a page that is not text drops the shell's export before anything is written", async () => {
  for (const bad of [null, "", 42, { svg: "x" }]) {
    const { ctx, shell } = loadPdf();
    pdfPages(ctx, 12, (i) => (i === 11 ? bad : "<svg/>"));
    assert.equal(await ctx.session.exportPdf(), null);
    assert.match(feedbackOf(ctx), /쪽 그림을 받지 못했습니다/);
    assert.deepEqual(shell.log.map((e) => [e.op, e.commit]), [["start", undefined], ["pages", undefined], ["finish", false]], "batch 1 went, batch 2 never, then the job was dropped");
  }
});

test("PDF export: a shell error on a batch stops the export, drops the job and is reported without a path", async () => {
  const { ctx, shell } = loadPdf({ failPages: new Error("12쪽을 PDF로 바꾸지 못했습니다") });
  pdfPages(ctx, 25);
  assert.equal(await ctx.session.exportPdf(), null);
  assert.match(feedbackOf(ctx), /^PDF 내보내기 실패: 12쪽을 PDF로 바꾸지 못했습니다/);
  assert.deepEqual(shell.log.map((e) => [e.op, e.commit]), [["start", undefined], ["pages", undefined], ["finish", false]]);
  assert.equal(ctx.fake.calls.filter((c) => c.method === "getPageSvg").length, 10, "no further page was rendered");
  const failing = loadPdf({ start: new Error("저장 대화상자를 열지 못했습니다") });
  pdfPages(failing.ctx, 1);
  assert.equal(await failing.ctx.session.exportPdf(), null);
  assert.match(feedbackOf(failing.ctx), /저장 대화상자를 열지 못했습니다/);
  assert.deepEqual(failing.shell.log.map((e) => e.op), ["start"]);
});

test("PDF export: a failed finish is reported", async () => {
  const { ctx } = loadPdf({ finish: new Error("파일을 저장하지 못했습니다") });
  pdfPages(ctx, 2);
  assert.equal(await ctx.session.exportPdf(), null);
  assert.match(feedbackOf(ctx), /^PDF 내보내기 실패: 파일을 저장하지 못했습니다/);
});

test("PDF export: a second request while one runs (menu, shortcut or editor menu) is refused and opens no second dialog", async () => {
  const lines = [];
  const { ctx, shell } = loadPdf({}, { setFeedback: (line) => lines.push(line) });
  pdfPages(ctx, 12);
  const first = ctx.session.exportPdf();
  assert.equal(await ctx.session.exportPdf(), null);
  assert.equal(lines.at(-1), "이미 PDF를 내보내고 있습니다");
  assert.equal((await first).status, "saved");
  assert.equal(shell.log.filter((e) => e.op === "start").length, 1, "one dialog, one job");
  assert.equal((await ctx.session.exportPdf()).status, "saved", "after it ended the next export runs");
});

test("PDF smoke step: sends the page SVGs as smoke.pdf and reports pages and bytes from the shell's answer", async () => {
  const { ctx, shell } = loadPdf({ finish: { status: "saved", name: "smoke.pdf", bytes: 321, pages: 2, opened: false } });
  pdfPages(ctx, 2);
  assert.equal(await ctx.session.smokePdf(), "pages=2 bytes=321");
  assert.deepEqual(shell.log.map((e) => e.op), ["start", "pages", "finish"]);
  assert.equal(shell.log[0].fileName, "smoke.pdf");
  assert.deepEqual(shell.log[1].svgs, ['<svg id="p0"/>', '<svg id="p1"/>']);
  assert.equal(feedbackOf(ctx), "", "the smoke step shows no status text");
});

test("PDF smoke step: rejects when the shell is missing, the page count is wrong or the answer is not the smoke save", async () => {
  const plain = load();
  pdfPages(plain, 1);
  await assert.rejects(plain.session.smokePdf(), /데스크톱 앱의 한글 편집기에서만/);
  const wrongName = loadPdf();
  pdfPages(wrongName.ctx, 1);
  await assert.rejects(wrongName.ctx.session.smokePdf(), /검사 결과를 돌려주지 않았습니다/);
  const dialogAnswer = loadPdf({ start: { status: "cancelled" } });
  pdfPages(dialogAnswer.ctx, 1);
  await assert.rejects(dialogAnswer.ctx.session.smokePdf(), /검사 결과를 돌려주지 않았습니다/);
  const empty = loadPdf();
  pdfPages(empty.ctx, 0);
  await assert.rejects(empty.ctx.session.smokePdf(), /쪽 수\(0\)/);
  const failing = loadPdf({ failPages: new Error("1쪽 SVG를 읽지 못했습니다") });
  pdfPages(failing.ctx, 1);
  await assert.rejects(failing.ctx.session.smokePdf(), /1쪽 SVG를 읽지 못했습니다/);
});
