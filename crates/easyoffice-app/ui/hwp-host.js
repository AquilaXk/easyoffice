(function (host) {
  "use strict";

  // HWP module host (PD-083): embeds the generated rhwp-studio bundle (`./hwp-studio/index.html`,
  // PD-081) in a same-origin <iframe> and talks to it over a MessagePort using the rhwp embed
  // protocol (`rhwp-connect` handshake, `rhwp-request` / `rhwp-response`, `rhwp-event`). The iframe
  // never receives Tauri IPC or the engine bridge. The frame is created lazily on the first HWP
  // activation. When the bundle is missing the old contenteditable HWP editor stays available as a
  // text-only fallback, and a failed connection offers a reconnect.

  const STUDIO_SRC = "./hwp-studio/index.html";
  // Smoke only (step `frame_ipc`): a page that behaves like the editor frame (same sandbox, same
  // response policy) and reports whether it can run an app command.
  const PROBE_SRC = "./frame-probe.html";
  const PROBE_TIMEOUT_MS = 20000;
  const PROBE_VALUES = Object.freeze({
    internals: ["present", "absent"],
    webview: ["present", "absent"],
    invoke: ["absent", "rejected", "no-answer", "allowed"],
  });
  const PROTOCOL_VERSION = 1;
  const CAPABILITIES = [
    "transferable-array-buffer", "document-state-v1", "document-change-events-v1", "notify-saved-v1",
  ];
  const LONG_METHODS = new Set(["ready", "loadFile", "createBlankDocument", "exportHwp", "exportHwpx", "verifyBytes"]);
  const SHORT_TIMEOUT_MS = 15000;
  const LONG_TIMEOUT_MS = 60000;
  const CONNECT_TIMEOUT_MS = 60000;
  // Scripts are required (the editor is an app bundle with WASM); modals keep alert/confirm dialogs
  // working. `allow-same-origin` is deliberately absent: with it the frame would share this
  // window's origin and could call `window.parent.__TAURI_INTERNALS__.invoke` (and, on Windows, its
  // own injected copy) against every command the window may call. Without it the frame gets an
  // opaque origin: no parent DOM or global access, and IPC requests with `Origin: null` are rejected
  // by Tauri. Popups, forms, downloads and top navigation stay denied, which also neutralizes the
  // editor's hyperlink preview and print preview windows (printing is done by hwp-print.js from
  // the shell window).
  const FRAME_SANDBOX = "allow-scripts allow-modals";
  // An opaque origin can only be addressed with "*". The target is `contentWindow` of the frame this
  // host created, and the handshake carries only a protocol version, a session id and capability
  // names; everything after it travels over the transferred MessagePort.
  const OPAQUE_TARGET_ORIGIN = "*";
  const MSG_UNAVAILABLE = "한글 편집기 구성 요소가 포함되지 않은 빌드입니다. 텍스트 전용 편집기로 대체합니다.";
  const MSG_LOADING = "한글 편집기를 불러오는 중입니다…";
  const MSG_FAILED = "한글 편집기에 연결하지 못했습니다.";
  const TOOL_HINT = "한글 편집기 도구줄을 사용하세요";
  const SHORTCUT_REASON = "shell_shortcut";
  // The editor's 파일 menu forwards shell commands by name only (#158); the shell re-checks each one
  // here before anything runs, so a compromised frame can only ask for these seven actions.
  const SHELL_COMMAND_REASON = "shell_command";
  const SHELL_COMMANDS = Object.freeze(["new", "open", "recent", "save", "save-as", "export-pdf", "print", "close"]);
  const SAVE_AS_FORMATS = Object.freeze(["hwp", "hwpx"]);
  const SHELL_THEMES = Object.freeze(["light", "dark", "system"]);
  const RECENT_ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/;
  const SHELL_CONTEXT_METHOD = "setShellContext";

  // Returns `{ command, id?, format? }` for a whitelisted command with valid arguments, else null.
  function sanitizeShellCommand(payload) {
    if (!payload || typeof payload !== "object") return null;
    const command = payload.command;
    if (typeof command !== "string" || !SHELL_COMMANDS.includes(command)) return null;
    const out = { command };
    if (command === "recent") {
      if (typeof payload.id !== "string" || !RECENT_ID_PATTERN.test(payload.id)) return null;
      out.id = payload.id;
    }
    if (command === "save-as") {
      const format = payload.format === undefined ? "hwp" : payload.format;
      if (typeof format !== "string" || !SAVE_AS_FORMATS.includes(format)) return null;
      out.format = format;
    }
    return out;
  }

  let sessionCounter = 0;

  function newSessionId(host) {
    const c = host.crypto;
    if (c && typeof c.randomUUID === "function") return c.randomUUID();
    sessionCounter += 1;
    return "eo-hwp-" + Date.now().toString(36) + "-" + sessionCounter.toString(36);
  }

  function bytesToBase64(bytes, btoaFn) {
    let bin = "";
    for (let i = 0; i < bytes.length; i += 0x8000) {
      bin += String.fromCodePoint.apply(null, bytes.subarray(i, i + 0x8000));
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
    const shortcutListeners = new Set();
    const commandListeners = new Set();
    // Pushed into the editor (`setShellContext`): the shell theme and the recent-documents list.
    const shellContext = {
      theme: SHELL_THEMES.includes(deps.initialTheme) ? deps.initialTheme : "system",
      recents: [],
      pdfExport: deps.canExportPdf === true,
    };
    // Combos the editor forwards to the shell (from the shell's single shortcut table). They travel
    // in the frame URL so the editor knows which keys to hand over; the editor sends combo strings only.
    const shellShortcuts = Array.isArray(deps.shellShortcuts) ? deps.shellShortcuts.filter((k) => typeof k === "string") : [];
    const statusListeners = new Set();
    let returnBound = false;

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
      const back = document.getElementById("btn-hwp-use-embed");
      if (back) {
        back.hidden = status !== "legacy";
        if (!returnBound) {
          returnBound = true;
          back.addEventListener("click", () => void reconnect());
        }
      }
    }

    function setStatus(next, error) {
      status = next;
      lastError = error || "";
      render();
      if (typeof deps.onStateChange === "function") deps.onStateChange();
      for (const listener of statusListeners) listener(next);
    }

    function onStatus(listener) {
      statusListeners.add(listener);
      return () => statusListeners.delete(listener);
    }

    // Resolves with the status once the editor is ready or has definitively failed / is unavailable.
    function whenSettled(timeoutMs) {
      const settled = () => status === "ready" || status === "error" || usesLegacy();
      if (settled()) return Promise.resolve(status);
      return new Promise((resolve) => {
        let off = () => {};
        const timer = timers.set(() => { off(); resolve(status); }, timeoutMs || limits.connect + limits.long);
        off = onStatus(() => {
          if (!settled()) return;
          timers.clear(timer);
          off();
          resolve(status);
        });
      });
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

    function handleResponseMessage(data) {
      const waiter = pending.get(data.id);
      if (!waiter) return;
      pending.delete(data.id);
      timers.clear(waiter.timer);
      if (data.error) {
        waiter.reject(new Error(String(data.error.message || data.error.code || "한글 편집기 오류")));
      } else {
        waiter.resolve(data.result);
      }
    }

    function handleEventMessage(data) {
      const payload = data.payload;
      if (payload?.reason === SHELL_COMMAND_REASON) {
        const command = sanitizeShellCommand(payload);
        if (command) for (const listener of commandListeners) listener(command);
        return;
      }
      if (payload?.reason === SHORTCUT_REASON) {
        if (typeof payload.shortcut === "string") for (const listener of shortcutListeners) listener(payload.shortcut);
        return;
      }
      for (const listener of changeListeners) listener(data);
    }

    function handlePortMessage(data) {
      if (!data || typeof data !== "object") return;
      if (data.type === "rhwp-response") handleResponseMessage(data);
      else if (data.type === "rhwp-event") handleEventMessage(data);
    }

    function call(method, params, options) {
      if (!port) return Promise.reject(new Error("한글 편집기에 연결되어 있지 않습니다."));
      const id = nextId;
      nextId += 1;
      const timeoutMs = options?.timeoutMs || (LONG_METHODS.has(method) ? limits.long : limits.short);
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
          port.postMessage(envelope, options?.transfer || []);
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
          const data = ev?.data;
          if (data?.type === "rhwp-connected") {
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
            OPAQUE_TARGET_ORIGIN,
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
        return Boolean(res?.ok);
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
      const query = [];
      if (shellShortcuts.length > 0) query.push("shellKeys=" + encodeURIComponent(shellShortcuts.join(",")));
      if (shellContext.theme !== "system") query.push("theme=" + shellContext.theme);
      el.setAttribute("src", query.length > 0 ? studioSrc + "?" + query.join("&") : studioSrc);
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
        await pushShellContext();
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
      if (isHwpActive()) showActivePane();
    }

    function useLegacyEditor() {
      attempt += 1;
      closeConnection("텍스트 전용 편집기로 전환했습니다.");
      setStatus("legacy");
      if (isHwpActive()) showActivePane();
      notify(MSG_UNAVAILABLE);
    }

    async function exportBytes(format) {
      const method = format === "hwpx" ? "exportHwpx" : "exportHwp";
      const bytes = await call(method);
      if (bytes instanceof Uint8Array) return bytes;
      if (bytes instanceof env.ArrayBuffer) return new Uint8Array(bytes);
      throw new Error("한글 편집기가 올바른 " + format.toUpperCase() + " 바이트를 돌려주지 않았습니다.");
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
      if (delivery?.ok) {
        await call("notifySaved").catch(() => null);
      }
      return delivery;
    }

    function isDirty() {
      if (status !== "ready") return Promise.resolve(false);
      return call("getDocumentState").then((s) => Boolean(s?.dirty)).catch(() => false);
    }

    // Theme and recent documents travel to the editor as plain values (recents: opaque id + name).
    function pushShellContext() {
      if (!port || !capabilities.includes("shell-context-v1")) return Promise.resolve(false);
      return call(SHELL_CONTEXT_METHOD, { theme: shellContext.theme, recents: shellContext.recents, pdfExport: shellContext.pdfExport })
        .then(() => true, () => false);
    }

    function setShellContext(next) {
      if (next && SHELL_THEMES.includes(next.theme)) shellContext.theme = next.theme;
      if (next && Array.isArray(next.recents)) {
        shellContext.recents = next.recents
          .filter((r) => r && RECENT_ID_PATTERN.test(String(r.id)) && typeof r.name === "string" && r.name)
          .slice(0, 10)
          .map((r) => ({ id: String(r.id), name: r.name }));
      }
      return status === "ready" ? pushShellContext() : Promise.resolve(false);
    }

    function onCommand(listener) {
      commandListeners.add(listener);
      return () => commandListeners.delete(listener);
    }

    function onShortcut(listener) {
      shortcutListeners.add(listener);
      return () => shortcutListeners.delete(listener);
    }

    function onChange(listener) {
      changeListeners.add(listener);
      return () => changeListeners.delete(listener);
    }

    // `EasyOffice.exe --smoke` step `hwp`: activate the HWP module, wait for the editor, export the
    // blank document and have the editor re-parse the bytes. Everything goes over the embed port
    // because the sandboxed frame exposes no DOM (typing is covered by
    // `scripts/hwp-studio-harness.mjs`). Resolves with the detail text the bridge reports; rejects
    // with the reason it failed.
    async function runSmoke(activate) {
      await activate();
      const started = Date.now();
      while (status !== "ready") {
        if (status === "error" || usesLegacy()) throw new Error(lastError || MSG_UNAVAILABLE);
        if (Date.now() - started > limits.connect) throw new Error("편집기 준비 시간 초과");
        await new Promise((resolve) => timers.set(resolve, 100));
      }
      const bytes = await exportHwp();
      if (bytes.length === 0 || bytes[0] !== 0xd0 || bytes[1] !== 0xcf) {
        throw new Error("내보낸 HWP 바이트가 올바르지 않습니다.");
      }
      const copy = bytes.slice().buffer;
      const verdict = await call("verifyBytes", { data: copy }, { transfer: [copy] });
      if (!verdict?.ok || !(verdict.pageCount >= 1)) throw new Error("편집기가 내보낸 HWP를 다시 열지 못했습니다.");
      return "bytes=" + bytes.length + " pages=" + verdict.pageCount;
    }

    // `EasyOffice.exe --smoke` step `frame_ipc`: puts a probe page in a frame sandboxed like the
    // editor and reports what it could reach. The Tauri IPC objects may be present in the frame
    // (Windows adds the initialization scripts to every frame); what must hold is that an app
    // command called from there does not run. Resolves with the detail text, rejects otherwise.
    function runFrameProbe(options) {
      const root = pane();
      if (!root || typeof env.addEventListener !== "function") {
        return Promise.reject(new Error("격리 점검을 시작할 수 없는 환경입니다."));
      }
      const probe = document.createElement("iframe");
      probe.hidden = true;
      probe.title = "격리 점검";
      probe.setAttribute("sandbox", FRAME_SANDBOX);
      probe.setAttribute("src", PROBE_SRC);
      let onMessage = null;
      let timer = null;
      const cleanup = () => {
        if (onMessage) env.removeEventListener("message", onMessage);
        if (timer !== null) timers.clear(timer);
        if (typeof probe.remove === "function") probe.remove();
      };
      return new Promise((resolve, reject) => {
        onMessage = (ev) => {
          if (!probe.contentWindow || ev?.source !== probe.contentWindow) return;
          const data = ev.data;
          const valid = data && typeof data === "object" && data.type === "eo-frame-probe"
            && Object.entries(PROBE_VALUES).every(([key, allowed]) => allowed.includes(data[key]));
          if (!valid) {
            reject(new Error("격리 점검 결과를 해석하지 못했습니다."));
          } else if (data.invoke === "allowed") {
            reject(new Error("편집기와 같은 샌드박스 프레임이 앱 명령을 실행했습니다 (internals=" + data.internals + " webview=" + data.webview + ")."));
          } else {
            resolve("internals=" + data.internals + " webview=" + data.webview + " invoke=" + data.invoke);
          }
        };
        env.addEventListener("message", onMessage);
        timer = timers.set(() => reject(new Error("격리 점검 프레임이 응답하지 않았습니다.")), options?.timeoutMs || PROBE_TIMEOUT_MS);
        root.appendChild(probe);
      }).finally(cleanup);
    }

    return {
      onModuleActivated, paneForModule, isEmbedActive, usesLegacy, call, reconnect, useLegacyEditor,
      exportHwp, exportBytes, exportHwpBase64, saveDocument, isDirty, onChange, onShortcut, onCommand, setShellContext, onStatus, whenSettled, runSmoke, runFrameProbe,
      getStatus: () => status,
      getCapabilities: () => capabilities.slice(),
      getFrame: () => frame,
      toolHint: TOOL_HINT,
    };
  }

  host.__EASYOFFICE_CREATE_HWP_HOST__ = createHwpHost;
  host.__EASYOFFICE_HWP_HOST_CONSTANTS__ = { STUDIO_SRC, FRAME_SANDBOX, CAPABILITIES, MSG_UNAVAILABLE, SHELL_COMMANDS, sanitizeShellCommand };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
