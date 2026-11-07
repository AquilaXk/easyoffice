(function (host) {
  "use strict";

  // HWP document lifecycle (PD-085): open (HWP / HWPX / HWT / HWTX into the embedded rhwp editor),
  // save (HWP or HWPX bytes exported by the editor, re-parsed before delivery), unsaved-changes state
  // driven by the editor's events, and the IndexedDB autosave store. The parent owns all of it; the
  // editor iframe only answers the embed protocol (see hwp-host.js).

  const MAX_HWP_BYTES = 64 * 1024 * 1024;
  const MAX_HWP_LABEL = "64 MiB";
  const HWP_EXTENSIONS = new Set(["hwp", "hwpx", "hwt", "hwtx"]);
  const OPEN_TIMEOUT_MS = 10 * 60 * 1000; // an encrypted document waits for the user's password
  const AUTOSAVE_IDLE_MS = 5000;
  const AUTOSAVE_MAX_WAIT_MS = 30000;
  const AUTOSAVE_STALE_MS = 7 * 24 * 3600 * 1000;
  const DB_NAME = "easyoffice.hwp.autosave.v1";
  const RECORD_STORE = "records";
  const META_STORE = "meta";
  const TAB_ID_KEY = "easyoffice.hwp.tabId";
  const PREFIX_BYTES = 16;
  const BASE64_CHUNK = 0x8000;
  const MSG_TEXT_ONLY = "한글 편집기로 열지 못해 텍스트만 불러왔습니다. 서식·표·그림은 유지되지 않으며 이 상태로 저장하면 서식이 사라집니다.";
  const REFUSED = /DRM|지원하지 않는 암호화|암호화된 문서를 열 수 없습니다/;
  const CANCELLED = /취소/;

  function baseName(raw) {
    return String(raw || "").split(/[\\/]/).pop().trim();
  }

  function extensionOf(name) {
    const m = /\.([A-Za-z0-9]+)$/.exec(name);
    return m ? m[1].toLowerCase() : "";
  }

  function isHwpName(name) {
    return HWP_EXTENSIONS.has(extensionOf(baseName(name)));
  }

  function mib(bytes) {
    return (bytes / 1048576).toFixed(1) + " MiB";
  }

  function bytesToBase64(bytes, btoaFn) {
    let bin = "";
    for (let i = 0; i < bytes.length; i += BASE64_CHUNK) {
      bin += String.fromCodePoint.apply(null, bytes.subarray(i, i + BASE64_CHUNK));
    }
    return btoaFn(bin);
  }

  function toHex(bytes, count) {
    let out = "";
    for (let i = 0; i < Math.min(count, bytes.length); i += 1) out += bytes[i].toString(16).padStart(2, "0");
    return out;
  }

  function copyBuffer(bytes) {
    return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  }

  function describeError(err) {
    return err && typeof err.message === "string" ? err.message : String(err);
  }

  function classifyOpenError(message) {
    if (CANCELLED.test(message)) return "cancelled";
    return REFUSED.test(message) ? "refused" : "fallback";
  }

  // ---- IndexedDB autosave store ----------------------------------------------------------

  function readOne(store, key) {
    const box = { value: undefined };
    const req = store.get(key);
    req.onsuccess = () => { box.value = req.result; };
    return box;
  }

  // One record per HWP tab: { tabId, fileName, format, savedAt, size, bytes }. A tiny `meta` store
  // lets stale records be found without loading document bytes.
  function createAutosaveStore(indexedDB) {
    let dbPromise = null;

    function open() {
      if (dbPromise) return dbPromise;
      dbPromise = new Promise((resolve, reject) => {
        if (!indexedDB) {
          reject(new Error("IndexedDB를 사용할 수 없습니다."));
          return;
        }
        const req = indexedDB.open(DB_NAME, 1);
        req.onupgradeneeded = () => {
          const db = req.result;
          db.createObjectStore(RECORD_STORE, { keyPath: "tabId" });
          db.createObjectStore(META_STORE, { keyPath: "tabId" });
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(new Error("자동 저장소를 열지 못했습니다."));
      });
      dbPromise.catch(() => { dbPromise = null; });
      return dbPromise;
    }

    async function run(mode, work) {
      const db = await open();
      return new Promise((resolve, reject) => {
        const tx = db.transaction([RECORD_STORE, META_STORE], mode);
        let result;
        tx.oncomplete = () => resolve(result);
        tx.onerror = () => reject(new Error("자동 저장소 작업에 실패했습니다."));
        tx.onabort = () => reject(new Error("자동 저장소 작업이 중단되었습니다."));
        result = work(tx.objectStore(RECORD_STORE), tx.objectStore(META_STORE));
      });
    }

    return {
      put: (record) => run("readwrite", (records, meta) => {
        records.put(record);
        meta.put({ tabId: record.tabId, savedAt: record.savedAt, size: record.size, fileName: record.fileName });
      }),
      get: async (tabId) => {
        const box = await run("readonly", (records) => readOne(records, tabId));
        return box.value || null;
      },
      remove: (tabId) => run("readwrite", (records, meta) => {
        records.delete(tabId);
        meta.delete(tabId);
      }),
      purgeStale: async (nowMs, exceptTabId) => {
        const box = { rows: [] };
        await run("readonly", (records, meta) => {
          const req = meta.getAll();
          req.onsuccess = () => { box.rows = req.result || []; };
        });
        const stale = box.rows.filter((r) => r && r.tabId !== exceptTabId && nowMs - Number(r.savedAt || 0) > AUTOSAVE_STALE_MS);
        for (const row of stale) await run("readwrite", (records, meta) => { records.delete(row.tabId); meta.delete(row.tabId); });
        return stale.length;
      },
    };
  }

  // ---- session ---------------------------------------------------------------------------

  function validate(name, size) {
    if (!name) return "파일 이름을 확인할 수 없습니다";
    if (size === 0) return "빈 파일은 열 수 없습니다";
    if (size > MAX_HWP_BYTES) {
      return "파일이 너무 큽니다 (" + mib(size) + "). 한글 문서는 최대 " + MAX_HWP_LABEL + "까지 열 수 있습니다";
    }
    return "";
  }

  function failure(format, error, cancelled) {
    const tag = String(format).toUpperCase();
    const message = cancelled
      ? "문서 저장 취소 (" + tag + "): 저장 위치를 선택하지 않았습니다"
      : "문서 저장 실패 (" + tag + "): " + error;
    return { ok: false, cancelled: Boolean(cancelled), error: error || "", message };
  }

  function isUsableRecord(rec) {
    const bytes = rec?.bytes;
    const length = bytes instanceof Uint8Array ? bytes.length : bytes?.byteLength;
    return Boolean(rec) && typeof rec.fileName === "string" && rec.fileName !== "" &&
      (rec.format === "hwp" || rec.format === "hwpx") && Number.isFinite(rec.savedAt) &&
      Number.isInteger(length) && length > 0 && length <= MAX_HWP_BYTES;
  }

  function createHwpSession(deps) {
    const { document, hwpHost } = deps;
    const env = deps.env || host;
    const timers = { set: (fn, ms) => env.setTimeout(fn, ms), clear: (id) => env.clearTimeout(id) };
    const confirmFn = deps.confirm || ((msg) => (typeof env.confirm === "function" ? env.confirm(msg) : true));
    const store = deps.store || createAutosaveStore(env.indexedDB);
    const idleMs = deps.autosaveIdleMs || AUTOSAVE_IDLE_MS;
    const maxWaitMs = deps.autosaveMaxWaitMs || AUTOSAVE_MAX_WAIT_MS;

    let current = null; // { name, saveFormat }
    let dirty = false;
    let editSeq = 0;
    let busy = false; // open / restore in flight
    let saving = false;
    let autosaving = false;
    let idleTimer = null;
    let maxTimer = null;
    let wasReady = hwpHost.getStatus() === "ready";
    let saveChain = Promise.resolve();
    let discarded = false;
    const tabId = resolveTabId();

    // One autosave record per window: the window id keeps simultaneous HWP windows apart.
    function resolveTabId() {
      const windowId = typeof deps.getDocumentId === "function" ? deps.getDocumentId() : "";
      if (windowId) return "hwp-" + windowId;
      try {
        const storage = env.sessionStorage;
        const existing = storage?.getItem(TAB_ID_KEY);
        if (existing) return existing;
        const fresh = "hwp-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8);
        if (storage) storage.setItem(TAB_ID_KEY, fresh);
        return fresh;
      } catch {
        return "hwp-main";
      }
    }

    function feedback(msg) {
      if (typeof deps.setFeedback === "function") deps.setFeedback(msg);
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = msg;
    }

    function setNoticeText(text) {
      const el = document.getElementById("hwp-fallback-text");
      if (el) el.textContent = text;
    }

    function setTabTitle(name) {
      if (typeof deps.setDocumentName === "function") deps.setDocumentName(name);
    }

    function setDirty(next) {
      if (dirty === next) return;
      dirty = next;
      if (!next) clearAutosaveTimers();
      if (typeof deps.onDirtyChange === "function") deps.onDirtyChange(next);
    }

    function clearAutosaveTimers() {
      if (idleTimer !== null) timers.clear(idleTimer);
      if (maxTimer !== null) timers.clear(maxTimer);
      idleTimer = null;
      maxTimer = null;
    }

    function resetDocumentState() {
      current = null;
      setDirty(false);
    }

    // ---- autosave ----------------------------------------------------------------------

    function scheduleAutosave() {
      if (idleTimer !== null) timers.clear(idleTimer);
      idleTimer = timers.set(() => void runAutosave(), idleMs);
      if (maxTimer === null) maxTimer = timers.set(() => void runAutosave(), maxWaitMs);
    }

    async function runAutosave() {
      clearAutosaveTimers();
      if (!dirty || busy || saving || autosaving || hwpHost.getStatus() !== "ready") return;
      if (discarded) return;
      autosaving = true;
      const seq = editSeq;
      try {
        const format = current?.saveFormat || "hwp";
        const bytes = await hwpHost.exportBytes(format);
        if (bytes.length > MAX_HWP_BYTES) {
          await store.remove(tabId).catch(() => null);
          feedback("자동 저장 건너뜀: 문서가 " + MAX_HWP_LABEL + "를 넘습니다. 저장(Ctrl+S)으로 보관하세요");
          return;
        }
        if (!dirty) return;
        await store.put({
          tabId, fileName: current?.name || "문서", format, savedAt: Date.now(),
          size: bytes.length, bytes: copyBuffer(bytes),
        });
      } catch (err) {
        feedback("한글 문서 자동 저장 실패: " + describeError(err));
      } finally {
        autosaving = false;
      }
      if (dirty && editSeq !== seq) scheduleAutosave();
    }

    // ---- editor events -----------------------------------------------------------------

    hwpHost.onChange((ev) => {
      const payload = ev?.payload;
      if (!payload || busy) return;
      if (payload.reason === "dirty_changed") {
        setDirty(payload.dirty === true);
        return;
      }
      editSeq += 1;
      scheduleAutosave();
    });

    hwpHost.onStatus((status) => {
      const ready = status === "ready";
      if (wasReady && !ready) resetDocumentState();
      wasReady = ready;
    });

    // ---- open --------------------------------------------------------------------------

    // Entry gate: EasyOffice FormatDetector signature/size check through the existing IPC. Only the
    // first bytes are sent: the extension hint selects the format and its magic bytes are verified.
    async function detect(name, data) {
      const res = await deps.invokeIpc("detect_format", JSON.stringify({
        fileName: name, hexPrefix: toHex(data, PREFIX_BYTES),
      }));
      const format = String(res?.format || "").toLowerCase();
      if (!HWP_EXTENSIONS.has(format)) throw new Error("한글 문서 형식이 아닙니다 (" + (format || "알 수 없음") + ")");
      return format;
    }

    async function ensureEditor() {
      if (deps.getActiveModule() !== "HWP") await deps.activateHwp();
      const first = hwpHost.getStatus();
      if (first === "error" || first === "legacy" || first === "unavailable") await hwpHost.reconnect();
      const status = await hwpHost.whenSettled();
      return status === "ready" ? "" : "한글 편집기를 사용할 수 없습니다";
    }

    async function loadIntoEditor(name, data, format) {
      const copy = copyBuffer(data);
      const result = await hwpHost.call(
        "loadFile",
        { data: copy, fileName: name, skipUnsavedGuard: true, suppressDialogs: false },
        { transfer: [copy], timeoutMs: OPEN_TIMEOUT_MS },
      );
      current = { name, saveFormat: format === "hwpx" || format === "hwtx" ? "hwpx" : "hwp" };
      setTabTitle(name);
      setNoticeText(deps.defaultNotice || "");
      return result;
    }

    async function textFallback(name, data, why, hooks) {
      if (!hooks || typeof hooks.textFallback !== "function") {
        feedback("파일 열기 실패: " + name + " — " + why);
        return { ok: false, error: why };
      }
      await store.remove(tabId).catch(() => null);
      if (hwpHost.getStatus() === "ready") hwpHost.useLegacyEditor();
      const res = await hooks.textFallback();
      if (res?.ok) {
        setNoticeText(MSG_TEXT_ONLY);
        feedback("텍스트만 열기: " + name + " — " + why + ". " + MSG_TEXT_ONLY);
        return { ...res, textOnly: true };
      }
      return res;
    }

    async function openFromBytes(fileName, bytes, hooks) {
      const name = baseName(fileName);
      const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      const invalid = validate(name, data.length);
      if (invalid) {
        feedback("파일 열기 실패: " + invalid + (name ? " (" + name + ")" : ""));
        return { ok: false, error: invalid };
      }
      let format;
      try {
        format = await detect(name, data);
      } catch (err) {
        feedback("파일 열기 실패: " + name + " — " + describeError(err));
        return { ok: false, error: describeError(err) };
      }
      if (dirty && !confirmFn("저장하지 않은 한글 문서 편집 내용이 있습니다.\n'" + name + "'을(를) 열면 현재 내용이 사라집니다. 계속할까요?")) {
        feedback("파일 열기를 취소했습니다 (" + name + ")");
        return { ok: false, cancelled: true };
      }
      busy = true;
      try {
        return await openChecked(name, data, format, hooks);
      } finally {
        busy = false;
      }
    }

    async function openChecked(name, data, format, hooks) {
      const unavailable = await ensureEditor();
      if (unavailable) return textFallback(name, data, unavailable, hooks);
      feedback("파일 여는 중: " + name);
      const started = Date.now();
      try {
        const res = await loadIntoEditor(name, data, format);
        setDirty(false);
        await store.remove(tabId).catch(() => null);
        const pages = res?.pageCount ? " · " + res.pageCount + "쪽" : "";
        feedback("열기 완료: " + name + " (" + format.toUpperCase() + " → 한글 편집기" + pages + " · " + mib(data.length) + " · " + (Date.now() - started) + "ms)");
        return { ok: true, module: "HWP", format, title: name, warnings: [] };
      } catch (err) {
        return handleOpenError(err, name, data, hooks);
      }
    }

    function handleOpenError(err, name, data, hooks) {
      const message = describeError(err);
      const kind = classifyOpenError(message);
      if (kind === "cancelled") {
        feedback("파일 열기를 취소했습니다 (" + name + ")");
        return { ok: false, cancelled: true };
      }
      if (kind === "refused") {
        feedback("파일 열기 실패: " + name + " — " + message);
        return { ok: false, error: message };
      }
      return textFallback(name, data, message, hooks);
    }

    // ---- save --------------------------------------------------------------------------

    // Export, then re-parse the exact bytes with the editor's own WASM before they are delivered.
    async function exportVerified(format) {
      const bytes = await hwpHost.exportBytes(format);
      if (bytes.length === 0) return { error: "내보낸 파일이 비어 있습니다" };
      if (!hwpHost.getCapabilities().includes("verify-bytes-v1")) return { error: "저장 검증 기능을 사용할 수 없습니다" };
      const copy = copyBuffer(bytes);
      try {
        const verdict = await hwpHost.call("verifyBytes", { data: copy }, { transfer: [copy] });
        if (verdict?.ok !== true || !(verdict.pageCount > 0)) return { error: "저장 전 검증에 실패했습니다" };
      } catch (err) {
        return { error: "저장 전 검증에 실패했습니다: " + describeError(err) };
      }
      return { bytes };
    }

    async function exportForSave(format) {
      let fmt = format;
      for (;;) {
        let outcome;
        try {
          outcome = await exportVerified(fmt);
        } catch (err) {
          outcome = { error: describeError(err) };
        }
        if (outcome.bytes) return { bytes: outcome.bytes, format: fmt, downgraded: fmt !== format };
        const offer = fmt === "hwpx" && confirmFn(
          "HWPX로 저장하기 전 검증에 실패했습니다 (" + outcome.error + ").\nHWP 형식으로 대신 저장할까요?",
        );
        if (!offer) return { error: outcome.error, format: fmt };
        fmt = "hwp";
      }
    }

    async function afterDelivery(delivery, exported, baseTitle, startSeq) {
      const tag = exported.format.toUpperCase();
      let tail = exported.downgraded ? " · HWPX 검증 실패로 HWP로 저장" : "";
      if (exported.format === "hwpx") tail += " · HWPX는 일부 서식이 HWP와 다르게 저장될 수 있어 한글에서 열어 확인하세요";
      current = { name: baseTitle + "." + exported.format, saveFormat: exported.format };
      if (editSeq !== startSeq) {
        tail += " · 저장하는 동안 편집이 있어 수정됨 상태를 유지합니다";
      } else {
        try {
          await hwpHost.call("notifySaved");
          setDirty(false);
          await store.remove(tabId).catch(() => null);
        } catch (err) {
          tail += " · 수정됨 표시를 해제하지 못했습니다 (" + describeError(err) + ")";
        }
      }
      return {
        ok: true, cancelled: false, name: delivery.name, format: exported.format,
        message: "문서 저장 완료 (" + delivery.name + " · " + tag + " · " + mib(exported.bytes.length) + ")" + tail,
      };
    }

    async function doSave(options) {
      if (hwpHost.getStatus() !== "ready") return failure(options.format || "hwp", "한글 편집기가 준비되지 않았습니다");
      const wanted = options.format || current?.saveFormat || "hwp";
      const startSeq = editSeq;
      saving = true;
      try {
        const exported = await exportForSave(wanted);
        if (!exported.bytes) return failure(exported.format, exported.error);
        const base64 = bytesToBase64(exported.bytes, (s) => env.btoa(s));
        const delivery = await options.deliver(base64, options.baseName, exported.format);
        if (delivery?.cancelled) return failure(exported.format, "", true);
        if (!delivery?.ok) return failure(exported.format, delivery?.error || "파일 다운로드를 시작하지 못했습니다");
        return await afterDelivery(delivery, exported, options.baseName, startSeq);
      } catch (err) {
        return failure(wanted, describeError(err));
      } finally {
        saving = false;
      }
    }

    // `options`: { deliver(base64, baseName, ext), baseName, format? }. Saves are serialized.
    function save(options) {
      const run = saveChain.then(() => doSave(options), () => doSave(options));
      saveChain = run.then(() => null, () => null);
      return run;
    }

    // ---- restore -----------------------------------------------------------------------

    async function discardRecord(why) {
      await store.remove(tabId).catch(() => null);
      feedback(why);
      return { restored: false };
    }

    // On reload: bring back the autosaved document of this tab once its bytes pass the format check.
    async function restore() {
      await store.purgeStale(Date.now(), tabId).catch(() => 0);
      let rec = null;
      try {
        rec = await store.get(tabId);
      } catch {
        return { restored: false };
      }
      if (!rec) return { restored: false };
      if (!isUsableRecord(rec)) return discardRecord("자동 저장된 한글 문서 데이터가 손상되어 무시했습니다");
      const data = rec.bytes instanceof Uint8Array ? rec.bytes : new Uint8Array(rec.bytes);
      try {
        await detect("autosave." + rec.format, data);
      } catch {
        return discardRecord("자동 저장된 한글 문서가 올바른 형식이 아니어서 무시했습니다");
      }
      if (!confirmFn("이전 세션의 저장하지 않은 한글 문서 '" + rec.fileName + "'의 자동 저장본이 있습니다. 복원할까요?")) {
        return discardRecord("자동 저장된 한글 문서를 복원하지 않고 삭제했습니다");
      }
      busy = true;
      try {
        if (await ensureEditor()) return { restored: false };
        await loadIntoEditor(rec.fileName, data, rec.format);
        current = { name: rec.fileName, saveFormat: rec.format };
        setDirty(true);
        feedback("이전 세션의 한글 문서를 자동 저장본에서 복원했습니다: " + rec.fileName);
        return { restored: true, name: rec.fileName };
      } catch (err) {
        return discardRecord("자동 저장된 한글 문서를 복원하지 못했습니다: " + describeError(err));
      } finally {
        busy = false;
      }
    }

    // ---- converter source ----------------------------------------------------------------

    // The open HWP document as converter input: bytes exported by the editor, not editor text.
    async function getConverterSource(srcFormat) {
      if (!(srcFormat === "hwp" || srcFormat === "hwpx") || hwpHost.getStatus() !== "ready") return null;
      if (!current && !dirty) return null;
      try {
        const bytes = await hwpHost.exportBytes(srcFormat);
        const stem = (current?.name || "변환_문서").replace(/\.[A-Za-z0-9]+$/, "") || "변환_문서";
        return {
          baseName: stem,
          fields: { fileName: stem + "." + srcFormat, sourceBase64: bytesToBase64(bytes, (s) => env.btoa(s)) },
        };
      } catch (err) {
        feedback("한글 문서를 변환용으로 내보내지 못했습니다: " + describeError(err));
        return null;
      }
    }

    // ---- DOM ---------------------------------------------------------------------------

    function bindUi() {
      for (const el of document.querySelectorAll("[data-hwp-save-as]")) {
        el.addEventListener("click", (ev) => {
          if (ev.stopPropagation) ev.stopPropagation();
          for (const open of document.querySelectorAll(".menu-item.open")) open.classList.remove("open");
          void saveAs(el.dataset.hwpSaveAs);
        });
      }
    }

    async function saveAs(format) {
      if (!hwpHost.isEmbedActive()) {
        feedback("이 저장 방식은 한글 편집기에서만 사용할 수 있습니다");
        return null;
      }
      const result = await save({ format, deliver: deps.deliverSavedFile, baseName: deps.resolveBaseName() });
      feedback(result.message);
      return result;
    }

    return {
      openFromBytes, save, saveAs, restore, bindUi, getConverterSource, runAutosave,
      isDirty: () => dirty,
      getCurrent: () => (current ? { ...current } : null),
      getTabId: () => tabId,
      // Closing a window discards its recovery copy; `save` and `open` keep it current while it lives.
      discardAutosave: () => {
        clearAutosaveTimers();
        discarded = true;
        void store.remove(tabId).catch(() => null);
      },
    };
  }

  host.__EASYOFFICE_CREATE_HWP_SESSION__ = createHwpSession;
  host.__EASYOFFICE_HWP_SESSION_UTILS__ = {
    MAX_HWP_BYTES, HWP_EXTENSIONS, DB_NAME, isHwpName, createAutosaveStore, classifyOpenError,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
