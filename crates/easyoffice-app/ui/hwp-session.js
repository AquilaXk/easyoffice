(function (host) {
  "use strict";

  // HWP document lifecycle (PD-085): open (HWP / HWPX / HWT / HWTX into the embedded rhwp editor),
  // save (HWP or HWPX bytes exported by the editor, re-parsed before delivery), unsaved-changes state
  // driven by the editor's events, and the IndexedDB autosave store. The parent owns all of it; the
  // editor iframe only answers the embed protocol (see hwp-host.js).

  const MAX_HWP_BYTES = 64 * 1024 * 1024;
  const MAX_HWP_LABEL = "64 MiB";
  // Same page ceiling as the shell's PDF export (`pdf_export.rs`, PD-101), and the batch size of its
  // `pdf_export_pages` call (`MAX_PAGES_PER_CALL` in `pdf_job.rs`, the print path's 10 pages too).
  const MAX_PDF_PAGES = 2000;
  const PDF_BATCH_PAGES = 10;
  const PDF_PAGE_TIMEOUT_MS = 120000;
  const SMOKE_PDF_NAME = "smoke.pdf";
  const MSG_PDF_BUSY = "이미 PDF를 내보내고 있습니다";
  const MSG_PDF_UNAVAILABLE = "PDF로 내보내기는 데스크톱 앱의 한글 편집기에서만 사용할 수 있습니다";
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
  // A document opened with a password is exported without it (PD-120). The editor says so over the
  // port (a flag, never the password); the shell refuses an overwrite and asks before a new copy.
  const PROTECTION_CAPABILITY = "save-protection-v1";
  // The first page goes to the shell next to the saved bytes, which draws it small and stores it as the
  // file's preview picture (PD-132). The page is the print profile SVG the PDF export uses; whatever
  // goes wrong here leaves the save as it was.
  const PRINT_CAPABILITY = "print-pages-v1";
  const THUMBNAIL_PAGE_TIMEOUT_MS = 3000;
  // `MAX_THUMBNAIL_SVG_BYTES` (16 MiB) in save_thumbnail.rs counts UTF-8 bytes; a UTF-16 unit is at most 3 of them.
  const MAX_THUMBNAIL_SVG_CHARS = Math.floor((16 * 1024 * 1024) / 3);
  const MSG_PASSWORD_NO_OVERWRITE = "암호 문서의 원본 저장은 아직 지원하지 않습니다. 다른 이름으로 저장에서 암호 없는 HWP 파일로 저장하세요.";
  const MSG_PASSWORD_DECLINED = "암호 없이 저장하지 않았습니다";
  const MSG_PASSWORD_NO_AUTOSAVE = "암호 문서는 자동 저장하지 않습니다. 저장(다른 이름으로 저장)으로 보관하세요";

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
        await Promise.all(stale.map((row) => run("readwrite", (records, meta) => { records.delete(row.tabId); meta.delete(row.tabId); })));
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

  function failure(format, error, cancelled, cancelReason) {
    const tag = String(format).toUpperCase();
    const message = cancelled
      ? "문서 저장 취소 (" + tag + "): " + (cancelReason || "저장 위치를 선택하지 않았습니다")
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
    // Confirmations are named kinds answered by the native dialog (`confirm_dialog` in the shell); the
    // wording lives in Rust. Without a way to ask, the answer is no.
    const confirmFn = deps.confirm || (async () => false);
    const store = deps.store || createAutosaveStore(env.indexedDB);
    const idleMs = deps.autosaveIdleMs || AUTOSAVE_IDLE_MS;
    const maxWaitMs = deps.autosaveMaxWaitMs || AUTOSAVE_MAX_WAIT_MS;

    let current = null; // { name, saveFormat }
    let dirty = false;
    let editSeq = 0;
    let busy = false; // open / restore in flight
    let exportingPdf = false; // a PDF export is running (its save dialog included)
    let saving = false;
    let autosaving = false;
    let idleTimer = null;
    let maxTimer = null;
    let wasReady = hwpHost.getStatus() === "ready";
    let saveChain = Promise.resolve();
    let discarded = false;
    let passwordNoticeShown = false;
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

    // true: accepted, false: declined, null: the question could not be asked.
    async function ask(kind) {
      try {
        return (await confirmFn(kind)) === true;
      } catch (err) {
        feedback("확인 창을 열지 못했습니다: " + describeError(err));
        return null;
      }
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
        if (await readPasswordProtected()) {
          await store.remove(tabId).catch(() => null);
          if (!passwordNoticeShown) feedback(MSG_PASSWORD_NO_AUTOSAVE);
          passwordNoticeShown = true;
          return;
        }
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
      passwordNoticeShown = false;
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
      // `busy` is on before the question, so the autosave and a second open leave the editor alone
      // while the native dialog is up; the answer is applied only if the window is still in use.
      if (busy) {
        feedback("파일 열기 실패: 다른 문서를 여는 중입니다 (" + name + ")");
        return { ok: false, error: "다른 문서를 여는 중입니다" };
      }
      busy = true;
      try {
        if (dirty && ((await ask("replace-unsaved-hwp")) !== true || discarded)) {
          feedback("파일 열기를 취소했습니다 (" + name + ")");
          return { ok: false, cancelled: true };
        }
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
        const hasPages = verdict?.pageCount > 0;
        if (verdict?.ok !== true || !hasPages) return { error: "저장 전 검증에 실패했습니다" };
      } catch (err) {
        return { error: "저장 전 검증에 실패했습니다: " + describeError(err) };
      }
      return { bytes };
    }

    // The first page as SVG for the file's preview picture, or null (no print support in the editor,
    // an error, a timeout or an oversized page) so that the document is saved without one.
    async function readThumbnailSvg() {
      try {
        if (!hwpHost.getCapabilities().includes(PRINT_CAPABILITY)) return null;
        const reply = await hwpHost.call("getPrintPages", { from: 0, count: 1 }, { timeoutMs: THUMBNAIL_PAGE_TIMEOUT_MS });
        const svg = reply?.pages?.[0]?.svg;
        return typeof svg === "string" && svg.length > 0 && svg.length <= MAX_THUMBNAIL_SVG_CHARS ? svg : null;
      } catch {
        return null;
      }
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
        if (fmt === "hwpx") feedback("HWPX로 저장하기 전 검증에 실패했습니다 (" + outcome.error + ")");
        const offer = fmt === "hwpx" && (await ask("hwpx-fallback-hwp")) === true;
        if (!offer) return { error: outcome.error, format: fmt };
        fmt = "hwp";
      }
    }

    // Whether the editor opened the document with a password. Fails closed: without the answer the
    // shell cannot tell a protected document from a plain one.
    async function readPasswordProtected() {
      if (!hwpHost.getCapabilities().includes(PROTECTION_CAPABILITY)) throw new Error("암호 문서 확인 기능을 사용할 수 없습니다");
      const state = await hwpHost.call("getSaveProtection");
      if (typeof state?.passwordProtected !== "boolean") throw new Error("암호 문서 확인 결과를 받지 못했습니다");
      return state.passwordProtected;
    }

    // A protected document is never overwritten by an unprotected copy: "save" is refused (closing
    // the window or quitting turns it into a save-as), and every save-as asks first. `deliver` is
    // what writes the bytes; `protectedDoc` makes the shell clear the editor's flag after delivery.
    async function passwordGate(options, format) {
      if (!(await readPasswordProtected())) return { deliver: options.deliver, protectedDoc: false };
      if (options.inPlace && !options.closing) return { failure: failure(format, MSG_PASSWORD_NO_OVERWRITE) };
      if ((await ask("password-free-save")) !== true) return { failure: failure(format, "", true, MSG_PASSWORD_DECLINED) };
      return { deliver: options.inPlace ? deps.deliverSavedFile : options.deliver, protectedDoc: true };
    }

    async function afterDelivery(delivery, exported, baseTitle, startSeq, protectedDoc) {
      const tag = exported.format.toUpperCase();
      let tail = exported.downgraded ? " · HWPX 검증 실패로 HWP로 저장" : "";
      if (exported.format === "hwpx") tail += " · HWPX는 일부 서식이 HWP와 다르게 저장될 수 있어 한글에서 열어 확인하세요";
      if (protectedDoc) {
        tail += " · 저장한 파일에는 암호가 없습니다";
        try {
          await hwpHost.call("clearSaveProtection");
        } catch (err) {
          tail += " · 암호 표시를 해제하지 못했습니다 (" + describeError(err) + ")";
        }
      }
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
        const gate = await passwordGate(options, wanted);
        if (gate.failure) return gate.failure;
        const exported = await exportForSave(wanted);
        if (!exported.bytes) return failure(exported.format, exported.error);
        const base64 = bytesToBase64(exported.bytes, (s) => env.btoa(s));
        const thumbnailSvg = await readThumbnailSvg();
        const delivery = await gate.deliver(base64, options.baseName, exported.format, thumbnailSvg ? { thumbnailSvg } : undefined);
        if (delivery?.cancelled) return failure(exported.format, "", true);
        if (!delivery?.ok) return failure(exported.format, delivery?.error || "파일 다운로드를 시작하지 못했습니다");
        return await afterDelivery(delivery, exported, options.baseName, startSeq, gate.protectedDoc);
      } catch (err) {
        return failure(wanted, describeError(err));
      } finally {
        saving = false;
      }
    }

    // `options`: { deliver(base64, baseName, ext), baseName, format?, inPlace?, closing? }. `inPlace`
    // marks a save that overwrites the opened file, `closing` one that runs for a window close or quit.
    // Saves are serialized.
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
      busy = true;
      try {
        // The native dialog cannot tell "복원 안 함" from a closed dialog or no dialog at all, so
        // nothing but an accepted restore touches the recovery copy: it stays for the next start.
        if ((await ask("restore-hwp-autosave")) !== true) {
          feedback("자동 저장된 한글 문서를 복원하지 않았습니다. 자동 저장본은 남겨 두었습니다");
          return { restored: false };
        }
        // Edits or a newly opened document made while the question was up are not overwritten.
        if (discarded || dirty || current) return { restored: false };
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
          host.__EASYOFFICE_CLOSE_MENUS__(document);
          void saveAs(el.dataset.hwpSaveAs);
        });
      }
    }

    // ---- searchable PDF (PD-101, PD-114, PD-123) --------------------------------------------------

    function pdfAvailable() {
      return hwpHost.isEmbedActive() && deps.pdfExport && typeof deps.pdfExport.start === "function";
    }

    async function readPdfPageCount() {
      const count = await hwpHost.call("pageCount");
      if (!Number.isInteger(count) || count < 1 || count > MAX_PDF_PAGES) {
        throw new Error("쪽 수(" + count + ")가 1~" + MAX_PDF_PAGES + " 범위를 벗어납니다");
      }
      return count;
    }

    // One batch of page SVGs out of the editor over the embed port; `onPage()` hears each page as it
    // arrives.
    async function readPageBatch(first, count, onPage) {
      const options = { timeoutMs: PDF_PAGE_TIMEOUT_MS };
      const svgs = await Promise.all(Array.from({ length: count }, async (_, offset) => {
        const svg = await hwpHost.call("getPageSvg", { page: first + offset }, options);
        onPage();
        return svg;
      }));
      if (!svgs.every((svg) => typeof svg === "string" && svg !== "")) throw new Error("쪽 그림을 받지 못했습니다");
      return svgs;
    }

    // The PDF is built in the shell while the pages come: the shell asks where to save first (so a
    // cancelled dialog renders nothing), then gets the pages `PDF_BATCH_PAGES` at a time like the print
    // path, so no more than one batch of SVG text is alive here or there. `onProgress(done, count)`
    // hears 0 of the page count first and then each page as it arrives. Any failure drops the shell's
    // export, which writes the file only at the end, so a partial PDF never exists.
    async function runPdfExport(fileName, onProgress) {
      const count = await readPdfPageCount();
      const started = await deps.pdfExport.start(fileName);
      if (started.status !== "started") return { cancelled: true, count };
      let done = 0;
      try {
        onProgress(0, count);
        for (let first = 0; first < count; first += PDF_BATCH_PAGES) {
          const size = Math.min(PDF_BATCH_PAGES, count - first);
          const svgs = await readPageBatch(first, size, () => onProgress(++done, count));
          await deps.pdfExport.pages(started.jobId, svgs);
        }
        return { cancelled: false, count, result: await deps.pdfExport.finish(started.jobId, true) };
      } catch (err) {
        await deps.pdfExport.finish(started.jobId, false).catch(() => null);
        throw err;
      }
    }

    function pdfDoneText(result, count) {
      const opened = result.opened === false ? " · 저장했으나 기본 앱으로 열지 못했습니다" : "";
      return "PDF 내보내기 완료 (" + result.name + " · " + count + "쪽)" + opened;
    }

    // Searchable PDF: the page SVGs go to the desktop shell as text; Rust builds the PDF and owns the
    // file dialog, the path and opening the result.
    async function exportPdf() {
      if (!pdfAvailable()) {
        feedback(MSG_PDF_UNAVAILABLE);
        return null;
      }
      if (exportingPdf) {
        feedback(MSG_PDF_BUSY);
        return null;
      }
      exportingPdf = true;
      try {
        const run = await runPdfExport(deps.resolveBaseName() + ".pdf", (done, count) => feedback("PDF 만드는 중: " + done + " / " + count + "쪽"));
        if (run.cancelled) {
          feedback("PDF 내보내기 취소: 저장 위치를 선택하지 않았습니다");
          return { status: "cancelled", name: null, bytes: 0 };
        }
        feedback(run.result.status === "saved" ? pdfDoneText(run.result, run.count) : "PDF 내보내기 취소: 저장하지 않았습니다");
        return run.result;
      } catch (err) {
        feedback("PDF 내보내기 실패: " + describeError(err));
        return null;
      } finally {
        exportingPdf = false;
      }
    }

    // `--smoke` step `pdf`: the same pages go to the shell, which in the smoke run checks the PDF in
    // memory (no dialog, no file) and answers as if it had saved `smoke.pdf`.
    async function smokePdf() {
      if (!pdfAvailable()) throw new Error(MSG_PDF_UNAVAILABLE);
      const run = await runPdfExport(SMOKE_PDF_NAME, () => {});
      const result = run.result;
      if (result?.status !== "saved" || result.name !== SMOKE_PDF_NAME) throw new Error("셸이 PDF 검사 결과를 돌려주지 않았습니다");
      return "pages=" + run.count + " bytes=" + result.bytes;
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
      openFromBytes, save, saveAs, exportPdf, smokePdf, restore, bindUi, getConverterSource, runAutosave,
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
