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

function createFakeHost({ status = "ready", capabilities = ["verify-bytes-v1"] } = {}) {
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
      return null;
    },
  };
  return { api, calls, state, setStatus, emit: (payload) => { for (const l of changeListeners) l({ type: "rhwp-event", payload }); } };
}

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
    confirm: (msg) => { confirms.push(msg); return answers.length > 0 ? answers.shift() : true; },
    onDirtyChange: (d) => dirtyLog.push(d),
    resolveBaseName: () => "보고서",
    deliverSavedFile: options.deliver,
    exportSearchablePdf: options.exportPdf,
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
  assert.match(ctx.confirms[0], /저장하지 않은 한글 문서 편집 내용/);
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
  assert.deepEqual(ctx.fake.calls.map((c) => c.method), ["exportHwp", "verifyBytes", "notifySaved"]);
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
    assert.match(ctx.confirms[0], /HWPX로 저장하기 전 검증에 실패했습니다.*HWP 형식으로 대신 저장/s);
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
  const ctx = load({ deliver, host: { capabilities: [] } });
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

test("restore: declining deletes the record, no record is a no-op, stale foreign records are purged", async () => {
  const declined = load({ confirmAnswers: [false] });
  seedRecord(declined);
  assert.equal((await declined.session.restore()).restored, false);
  assert.equal(declined.env.indexedDB.stores.records.size, 0);
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

// ---- searchable PDF export (PD-101) --------------------------------------------------------------

const PDF_SAVED = Object.freeze({ status: "saved", name: "보고서.pdf", bytes: 10 });
const pdfRecorder = (result = PDF_SAVED) => {
  const sent = [];
  const exportPdf = async (fileName, pages) => {
    sent.push({ fileName, pages });
    if (result instanceof Error) throw result;
    return result;
  };
  return { sent, exportPdf };
};

function pdfPages(ctx, count, pageFor = (i) => `<svg id="p${i}"/>`) {
  ctx.fake.state.handlers.pageCount = () => count;
  ctx.fake.state.handlers.getPageSvg = ({ page }) => pageFor(page);
}

test("PDF export: every page SVG is fetched over the embed port and sent to the shell as an array of strings", async () => {
  const { sent, exportPdf } = pdfRecorder();
  const ctx = load({ exportPdf });
  pdfPages(ctx, 3);
  const result = await ctx.session.exportPdf();
  assert.equal(result.status, "saved");
  assert.deepEqual(sent, [{ fileName: "보고서.pdf", pages: ['<svg id="p0"/>', '<svg id="p1"/>', '<svg id="p2"/>'] }]);
  const fetched = ctx.fake.calls.filter((c) => c.method === "getPageSvg");
  assert.deepEqual(fetched.map((c) => c.params), [{ page: 0 }, { page: 1 }, { page: 2 }]);
  assert.ok(fetched.every((c) => c.options.timeoutMs >= 60000), "page rendering gets a long timeout");
  assert.match(feedbackOf(ctx), /^PDF 내보내기 완료 \(보고서\.pdf · 3쪽\)$/);
});

test("PDF export: the status line counts the pages as they arrive and ends with the result", async () => {
  const { exportPdf } = pdfRecorder();
  const lines = [];
  const ctx = load({ exportPdf, setFeedback: (line) => lines.push(line) });
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

test("PDF export: not available without the desktop shell or while the editor is not ready", async () => {
  const plain = load();
  pdfPages(plain, 2);
  assert.equal(await plain.session.exportPdf(), null);
  assert.match(feedbackOf(plain), /데스크톱 앱의 한글 편집기에서만/);
  assert.equal(plain.fake.calls.filter((c) => c.method === "getPageSvg").length, 0);
  const { sent, exportPdf } = pdfRecorder();
  const legacy = load({ exportPdf, host: { status: "legacy" } });
  assert.equal(await legacy.session.exportPdf(), null);
  assert.deepEqual(sent, []);
});

test("PDF export: the page count is checked against the shell's 2,000 page ceiling before any page is rendered", async () => {
  const { sent, exportPdf } = pdfRecorder();
  const rejects = async (count) => {
    const ctx = load({ exportPdf });
    pdfPages(ctx, count);
    assert.equal(await ctx.session.exportPdf(), null, String(count));
    assert.match(feedbackOf(ctx), /^PDF 내보내기 실패: 쪽 수/);
    assert.equal(ctx.fake.calls.filter((c) => c.method === "getPageSvg").length, 0);
  };
  await Promise.all([0, 2001, -1, 1.5, "3", null].map(rejects));
  assert.deepEqual(sent, []);
  const edge = load({ exportPdf });
  pdfPages(edge, 2000);
  assert.equal((await edge.session.exportPdf()).status, "saved");
  assert.equal(sent[0].pages.length, 2000);
});

test("PDF export: a page that is not text stops the export before the shell is called", async () => {
  const { sent, exportPdf } = pdfRecorder();
  const stops = async (bad) => {
    const ctx = load({ exportPdf });
    pdfPages(ctx, 2, (i) => (i === 1 ? bad : "<svg/>"));
    assert.equal(await ctx.session.exportPdf(), null);
    assert.match(feedbackOf(ctx), /쪽 그림을 받지 못했습니다/);
  };
  await Promise.all([null, "", 42, { svg: "x" }].map(stops));
  assert.deepEqual(sent, []);
});

test("PDF export: cancelling the save dialog and shell errors are reported without a path", async () => {
  const cancelled = load({ exportPdf: pdfRecorder({ status: "cancelled", name: null, bytes: 0 }).exportPdf });
  pdfPages(cancelled, 1);
  assert.equal((await cancelled.session.exportPdf()).status, "cancelled");
  assert.match(feedbackOf(cancelled), /PDF 내보내기 취소/);
  const failing = load({ exportPdf: pdfRecorder(new Error("1쪽 SVG를 읽지 못했습니다")).exportPdf });
  pdfPages(failing, 1);
  assert.equal(await failing.session.exportPdf(), null);
  assert.match(feedbackOf(failing), /^PDF 내보내기 실패: .*1쪽 SVG를 읽지 못했습니다/);
});

test("PDF smoke step: sends the page SVGs as smoke.pdf and reports pages and bytes from the shell's answer", async () => {
  const { sent, exportPdf } = pdfRecorder({ status: "saved", name: "smoke.pdf", bytes: 321 });
  const ctx = load({ exportPdf });
  pdfPages(ctx, 2);
  assert.equal(await ctx.session.smokePdf(), "pages=2 bytes=321");
  assert.deepEqual(sent, [{ fileName: "smoke.pdf", pages: ['<svg id="p0"/>', '<svg id="p1"/>'] }]);
  assert.equal(feedbackOf(ctx), "", "the smoke step shows no status text");
});

test("PDF smoke step: rejects when the shell is missing, the page count is wrong or the answer is not the smoke save", async () => {
  const plain = load();
  pdfPages(plain, 1);
  await assert.rejects(plain.session.smokePdf(), /데스크톱 앱의 한글 편집기에서만/);
  const { exportPdf } = pdfRecorder();
  const wrongName = load({ exportPdf });
  pdfPages(wrongName, 1);
  await assert.rejects(wrongName.session.smokePdf(), /검사 결과를 돌려주지 않았습니다/);
  const dialogAnswer = load({ exportPdf: pdfRecorder({ status: "cancelled", name: null, bytes: 0 }).exportPdf });
  pdfPages(dialogAnswer, 1);
  await assert.rejects(dialogAnswer.session.smokePdf(), /검사 결과를 돌려주지 않았습니다/);
  const empty = load({ exportPdf });
  pdfPages(empty, 0);
  await assert.rejects(empty.session.smokePdf(), /쪽 수\(0\)/);
  const failing = load({ exportPdf: pdfRecorder(new Error("1쪽 SVG를 읽지 못했습니다")).exportPdf });
  pdfPages(failing, 1);
  await assert.rejects(failing.session.smokePdf(), /1쪽 SVG를 읽지 못했습니다/);
});
