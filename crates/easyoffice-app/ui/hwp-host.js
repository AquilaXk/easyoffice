(function (host) {
  "use strict";

  // HWP module host (PD-083): embeds the generated rhwp-studio bundle (`./hwp-studio/index.html`,
  // PD-081) in a same-origin <iframe> and talks to it over a MessagePort using the rhwp embed
  // protocol (`rhwp-connect` handshake, `rhwp-request` / `rhwp-response`, `rhwp-event`). The iframe
  // never receives Tauri IPC or the engine bridge. The frame is created lazily on the first HWP
  // activation. When the bundle is missing the old contenteditable HWP editor stays available as a
  // text-only fallback, and a failed connection offers a reconnect.

  const STUDIO_SRC = "./hwp-studio/index.html";
  const PROTOCOL_VERSION = 1;
  const CAPABILITIES = [
    "transferable-array-buffer", "document-state-v1", "document-change-events-v1", "notify-saved-v1",
  ];
  const LONG_METHODS = new Set(["ready", "loadFile", "createBlankDocument", "exportHwp", "exportHwpx"]);
  const SHORT_TIMEOUT_MS = 15000;
  const LONG_TIMEOUT_MS = 60000;
  const CONNECT_TIMEOUT_MS = 60000;
  // Same-origin and scripts are required (the editor is an app bundle that uses storage and WASM);
  // modals keep alert/confirm dialogs working. Popups, forms, downloads and top navigation stay
  // denied, which also neutralizes the editor's hyperlink preview and print preview windows.
  const FRAME_SANDBOX = "allow-same-origin allow-scripts allow-modals";
  const MSG_UNAVAILABLE = "한글 편집기 구성 요소가 포함되지 않은 빌드입니다. 텍스트 전용 편집기로 대체합니다.";
  const MSG_LOADING = "한글 편집기를 불러오는 중입니다…";
  const MSG_FAILED = "한글 편집기에 연결하지 못했습니다.";
  const TOOL_HINT = "한글 편집기 도구줄을 사용하세요";

  function newSessionId(host) {
    const c = host.crypto;
    if (c && typeof c.randomUUID === "function") return c.randomUUID();
    return "eo-hwp-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
  }

  function bytesToBase64(bytes, btoaFn) {
    let bin = "";
    for (let i = 0; i < bytes.length; i += 0x8000) {
      bin += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
    }
    return btoaFn(bin);
  }

  function createHwpHost(deps) {
    const { document, getActiveModule } = deps;
    const env = deps.env || host;
    const studioSrc = deps.studioSrc || STUDIO_SRC;
    const timers = {
      set: (fn, ms) => env.setTimeout(fn, ms),
      clear: (id) => env.clearTimeout(id),
    };
    const limits = {
      short: deps.shortTimeoutMs || SHORT_TIMEOUT_MS,
      long: deps.longTimeoutMs || LONG_TIMEOUT_MS,
      connect: deps.connectTimeoutMs || CONNECT_TIMEOUT_MS,
    };

    // idle | probing | connecting | ready | error | unavailable | legacy
    let status = "idle";
    let frame = null;
    let port = null;
    let sessionId = "";
    let nextId = 1;
    let attempt = 0;
    let capabilities = [];
    let lastError = "";
    let ui = null;
    const pending = new Map();
    const changeListeners = new Set();

    function pane() {
      return document.getElementById("pane-hwp");
    }

    function legacyPane() {
      return document.getElementById("pane-doc");
    }

    function notify(msg) {
      if (typeof deps.setFeedback === "function") deps.setFeedback(msg);
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = msg;
    }

    function isHwpActive() {
      return getActiveModule() === "HWP";
    }

    // The embed owns the HWP module while it is loading, connected or failed; the legacy editor
    // only takes over once the bundle is known to be missing or the user chose it explicitly.
    function usesLegacy() {
      return status === "unavailable" || status === "legacy";
    }

    function isEmbedActive() {
      return isHwpActive() && !usesLegacy();
    }

    function paneForModule() {
      return usesLegacy() ? legacyPane() : pane();
    }

    function ensureUi() {
      if (ui) return ui;
      const root = pane();
      if (!root) return null;
      const overlay = document.createElement("div");
      overlay.className = "hwp-host-status";
      overlay.setAttribute("role", "status");
      const text = document.createElement("p");
      text.className = "hwp-host-status-text";
      const actions = document.createElement("div");
      actions.className = "hwp-host-status-actions";
      const retryBtn = document.createElement("button");
      retryBtn.type = "button";
      retryBtn.className = "tb-btn";
      retryBtn.id = "btn-hwp-reconnect";
      retryBtn.textContent = "다시 연결";
      retryBtn.addEventListener("click", () => void reconnect());
      const legacyBtn = document.createElement("button");
      legacyBtn.type = "button";
      legacyBtn.className = "tb-btn";
      legacyBtn.id = "btn-hwp-use-legacy";
      legacyBtn.textContent = "텍스트 전용 편집기 사용";
      legacyBtn.addEventListener("click", () => useLegacyEditor());
      actions.appendChild(retryBtn);
      actions.appendChild(legacyBtn);
      overlay.appendChild(text);
      overlay.appendChild(actions);
      root.appendChild(overlay);
      ui = { overlay, text, retryBtn, legacyBtn };
      return ui;
    }

    function render() {
      const parts = ensureUi();
      if (!parts) return;
      const showOverlay = status === "probing" || status === "connecting" || status === "error";
      parts.overlay.hidden = !showOverlay;
      parts.text.textContent = status === "error" ? (lastError || MSG_FAILED) : MSG_LOADING;
      parts.retryBtn.hidden = status !== "error";
      parts.legacyBtn.hidden = status !== "error";
      const notice = document.getElementById("hwp-fallback-notice");
      if (notice) notice.hidden = !(usesLegacy() && isHwpActive());
    }

    function setStatus(next, error) {
      status = next;
      lastError = error || "";
      render();
      if (typeof deps.onStateChange === "function") deps.onStateChange();
    }

    function showActivePane() {
      const target = paneForModule();
      for (const el of [pane(), legacyPane()]) {
        if (el && el !== target) el.hidden = true;
      }
      if (target) target.hidden = false;
    }

    function failPending(reason) {
      for (const [id, waiter] of pending) {
        timers.clear(waiter.timer);
        waiter.reject(new Error(reason));
        pending.delete(id);
      }
    }

    function closeConnection(reason) {
      failPending(reason);
      if (port) {
        port.onmessage = null;
        port.onmessageerror = null;
        if (typeof port.close === "function") port.close();
      }
      port = null;
      capabilities = [];
      if (frame && typeof frame.remove === "function") frame.remove();
      frame = null;
    }

    function handlePortMessage(data) {
      if (!data || typeof data !== "object") return;
      if (data.type === "rhwp-response") {
        const waiter = pending.get(data.id);
        if (!waiter) return;
        pending.delete(data.id);
        timers.clear(waiter.timer);
        if (data.error) {
          waiter.reject(new Error(String(data.error.message || data.error.code || "한글 편집기 오류")));
        } else {
          waiter.resolve(data.result);
        }
      } else if (data.type === "rhwp-event") {
        for (const listener of changeListeners) listener(data);
      }
    }

    function call(method, params, options) {
      if (!port) return Promise.reject(new Error("한글 편집기에 연결되어 있지 않습니다."));
      const id = nextId;
      nextId += 1;
      const timeoutMs = (options && options.timeoutMs) || (LONG_METHODS.has(method) ? limits.long : limits.short);
      return new Promise((resolve, reject) => {
        const timer = timers.set(() => {
          pending.delete(id);
          reject(new Error("한글 편집기 응답 시간 초과 (" + method + ")"));
        }, timeoutMs);
        pending.set(id, { resolve, reject, timer });
        const envelope = {
          type: "rhwp-request", version: PROTOCOL_VERSION, sessionId, id, method,
          ...(params === undefined ? {} : { params }),
        };
        try {
          port.postMessage(envelope, (options && options.transfer) || []);
        } catch (err) {
          pending.delete(id);
          timers.clear(timer);
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      });
    }

    function connect(channelFrame) {
      const MessageChannelCtor = env.MessageChannel;
      if (typeof MessageChannelCtor !== "function") {
        return Promise.reject(new Error("이 환경은 MessageChannel을 지원하지 않습니다."));
      }
      const channel = new MessageChannelCtor();
      sessionId = newSessionId(env);
      port = channel.port1;
      return new Promise((resolve, reject) => {
        const timer = timers.set(
          () => reject(new Error("한글 편집기 연결 시간 초과")),
          limits.connect,
        );
        port.onmessage = (ev) => {
          const data = ev && ev.data;
          if (data && data.type === "rhwp-connected") {
            timers.clear(timer);
            capabilities = Array.isArray(data.capabilities) ? data.capabilities.slice() : [];
            resolve(capabilities);
            return;
          }
          handlePortMessage(data);
        };
        port.onmessageerror = () => failPending("한글 편집기 메시지를 해석하지 못했습니다.");
        const send = () => {
          const target = channelFrame.contentWindow;
          if (!target) {
            timers.clear(timer);
            reject(new Error("한글 편집기 창을 찾지 못했습니다."));
            return;
          }
          target.postMessage(
            { type: "rhwp-connect", version: PROTOCOL_VERSION, sessionId, capabilities: CAPABILITIES },
            env.location.origin,
            [channel.port2],
          );
        };
        channelFrame.addEventListener("load", send, { once: true });
        channelFrame.addEventListener("error", () => {
          timers.clear(timer);
          reject(new Error("한글 편집기 문서를 불러오지 못했습니다."));
        }, { once: true });
      });
    }

    async function probeBundle() {
      if (typeof env.fetch !== "function") return false;
      try {
        const res = await env.fetch(studioSrc, { cache: "no-store" });
        return Boolean(res && res.ok);
      } catch {
        return false;
      }
    }

    function buildFrame() {
      const el = document.createElement("iframe");
      el.className = "hwp-frame";
      el.id = "hwp-frame";
      el.title = "한글 편집기";
      el.setAttribute("sandbox", FRAME_SANDBOX);
      el.setAttribute("src", studioSrc);
      pane().appendChild(el);
      return el;
    }

    async function startConnection(token) {
      const stale = () => token !== attempt;
      try {
        frame = buildFrame();
        setStatus("connecting");
        const handshake = connect(frame);
        await handshake;
        if (stale()) return;
        await call("ready");
        if (stale()) return;
        await call("createBlankDocument", { skipUnsavedGuard: true });
        if (stale()) return;
        setStatus("ready");
        notify("한글 편집기 준비 완료: 빈 문서에서 시작합니다");
      } catch (err) {
        if (stale()) return;
        closeConnection("연결 실패");
        const reason = err instanceof Error ? err.message : String(err);
        setStatus("error", MSG_FAILED + " (" + reason + ")");
        notify(MSG_FAILED);
      }
    }

    async function start() {
      attempt += 1;
      const token = attempt;
      setStatus("probing");
      const present = await probeBundle();
      if (token !== attempt) return;
      if (!present) {
        setStatus("unavailable", MSG_UNAVAILABLE);
        notify(MSG_UNAVAILABLE);
        if (isHwpActive()) showActivePane();
        return;
      }
      await startConnection(token);
    }

    // Called after the module pane switched: loads the editor lazily on the first HWP activation.
    function onModuleActivated(mod) {
      render();
      if (mod !== "HWP") return Promise.resolve();
      if (status === "idle") return start();
      return Promise.resolve();
    }

    async function reconnect() {
      attempt += 1;
      closeConnection("다시 연결합니다.");
      status = "idle";
      await start();
    }

    function useLegacyEditor() {
      attempt += 1;
      closeConnection("텍스트 전용 편집기로 전환했습니다.");
      setStatus("legacy");
      if (isHwpActive()) showActivePane();
      notify(MSG_UNAVAILABLE);
    }

    async function exportHwp() {
      const bytes = await call("exportHwp");
      if (bytes instanceof Uint8Array) return bytes;
      if (bytes instanceof env.ArrayBuffer) return new Uint8Array(bytes);
      throw new Error("한글 편집기가 올바른 HWP 바이트를 돌려주지 않았습니다.");
    }

    async function exportHwpBase64() {
      return bytesToBase64(await exportHwp(), (s) => env.btoa(s));
    }

    // Save for the embedded editor: HWP 5.0 bytes from the editor, delivered like every other save
    // (native dialog in the desktop shell, download in --serve). Then the editor's dirty flag is cleared.
    async function saveDocument(deliver, baseName) {
      const base64 = await exportHwpBase64();
      const delivery = await deliver(base64, baseName, "hwp");
      if (delivery && delivery.ok) {
        await call("notifySaved").catch(() => null);
      }
      return delivery;
    }

    function isDirty() {
      if (status !== "ready") return Promise.resolve(false);
      return call("getDocumentState").then((s) => Boolean(s && s.dirty)).catch(() => false);
    }

    function onChange(listener) {
      changeListeners.add(listener);
      return () => changeListeners.delete(listener);
    }

    // `EasyOffice.exe --smoke`: activate the HWP module, wait for the editor, type into a blank
    // document, export HWP bytes and verify them. The shell watches the engine call made by `report`.
    function reportSmoke(ok, message) {
      const marker = ok ? "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_OK " : "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_FAIL ";
      return Promise.resolve(deps.invokeIpc("scrub_pii", marker + message)).catch(() => null);
    }

    async function runSmoke(activate) {
      const report = reportSmoke;
      try {
        await activate();
        const started = Date.now();
        while (status !== "ready") {
          if (status === "error" || usesLegacy()) throw new Error(lastError || MSG_UNAVAILABLE);
          if (Date.now() - started > limits.connect) throw new Error("편집기 준비 시간 초과");
          await new Promise((resolve) => timers.set(resolve, 100));
        }
        const before = await exportHwp();
        const typed = await typeIntoEditor("한글 스모크 테스트");
        const after = await exportHwp();
        const dirty = await isDirty();
        if (!typed || !dirty) throw new Error("편집기에 입력이 반영되지 않았습니다.");
        if (after.length === 0 || after[0] !== 0xd0 || after[1] !== 0xcf) {
          throw new Error("내보낸 HWP 바이트가 올바르지 않습니다.");
        }
        report(true, "bytes=" + after.length + " blank=" + before.length);
      } catch (err) {
        report(false, err instanceof Error ? err.message : String(err));
      }
    }

    // Same-origin test input for the smoke run only: click the first page, then feed the editor's
    // hidden text input (the same path the browser uses for typed text).
    async function typeIntoEditor(text) {
      const doc = frame && frame.contentDocument;
      const win = frame && frame.contentWindow;
      const canvas = doc && doc.querySelector("#scroll-container canvas");
      if (!canvas) return false;
      const rect = canvas.getBoundingClientRect();
      const point = { bubbles: true, cancelable: true, button: 0, view: win, clientX: rect.left + rect.width / 2, clientY: rect.top + 160 };
      for (const type of ["mousedown", "mouseup", "click"]) {
        canvas.dispatchEvent(new win.MouseEvent(type, point));
      }
      await new Promise((resolve) => timers.set(resolve, 300));
      const input = doc.querySelector("textarea");
      if (!input) return false;
      input.focus();
      input.value = text;
      input.dispatchEvent(new win.InputEvent("input", { bubbles: true, data: text, inputType: "insertText" }));
      await new Promise((resolve) => timers.set(resolve, 300));
      return true;
    }

    return {
      onModuleActivated, paneForModule, isEmbedActive, usesLegacy, call, reconnect, useLegacyEditor,
      exportHwp, exportHwpBase64, saveDocument, isDirty, onChange, runSmoke,
      getStatus: () => status,
      getCapabilities: () => capabilities.slice(),
      getFrame: () => frame,
      toolHint: TOOL_HINT,
    };
  }

  host.__EASYOFFICE_CREATE_HWP_HOST__ = createHwpHost;
  host.__EASYOFFICE_HWP_HOST_CONSTANTS__ = { STUDIO_SRC, FRAME_SANDBOX, CAPABILITIES, MSG_UNAVAILABLE };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
