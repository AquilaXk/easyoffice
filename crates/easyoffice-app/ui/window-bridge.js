(function (host) {
  "use strict";

  // Per-document windows (PD-088). Every window hosts exactly one document, the converter, or the
  // start screen, chosen by its launch parameters (`?module=DOC|HWP|SHEET|SLIDE|PDF|CONV|START` and a
  // window id `win`). In the desktop shell the Rust side creates windows, shows the native open
  // dialog, reads files and tracks paths; this page only ever sees display names, kinds and bytes.
  // In the developer-only `--serve` mode the same model is emulated with `window.open`.

  const MODULES = Object.freeze(["DOC", "HWP", "SHEET", "SLIDE", "PDF", "CONV"]);
  const START = "START";
  const APP_TITLE = "EasyOffice";
  const UNTITLED = "제목 없음";
  const DIRTY_MARK = "● ";
  const CLOSE_CHOICES = Object.freeze(["save", "discard", "cancel"]);
  const ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/;
  const EXTENSION_KINDS = Object.freeze({
    DOC: ["docx", "doc", "odt", "rtf", "md", "markdown", "txt", "pages"],
    HWP: ["hwp", "hwpx", "hwt", "hwtx"],
    SHEET: ["xlsx", "xls", "ods", "csv", "tsv"],
    SLIDE: ["pptx", "ppt", "ppsx", "odp"],
    PDF: ["pdf"],
  });
  // Smoke step the second window reports once it has loaded (`smoke_steps.rs`).
  const SMOKE_WINDOW_STEP = "window_loaded";
  const POPUP_FEATURES = "popup=yes,width=1280,height=860";

  function kindForFileName(name) {
    const m = /\.([A-Za-z0-9]+)$/.exec(String(name || ""));
    const ext = m ? m[1].toLowerCase() : "";
    return Object.keys(EXTENSION_KINDS).find((kind) => EXTENSION_KINDS[kind].includes(ext)) || null;
  }

  function parseLaunch(search) {
    const params = new URLSearchParams(String(search || ""));
    const raw = String(params.get("module") || "").toUpperCase();
    const windowId = String(params.get("win") || "");
    const handoff = String(params.get("handoff") || "");
    return {
      module: MODULES.includes(raw) || raw === START ? raw : null,
      windowId: ID_PATTERN.test(windowId) ? windowId : "",
      handoff: ID_PATTERN.test(handoff) ? handoff : "",
    };
  }

  function formatWindowTitle(name, dirty) {
    const shown = String(name || "").trim() || UNTITLED;
    return (dirty ? DIRTY_MARK : "") + shown + " — " + APP_TITLE;
  }

  function baseName(raw) {
    return String(raw || "").split(/[\\/]/).pop().trim();
  }

  let fallbackIdCounter = 0;

  function randomId(env) {
    const c = env.crypto;
    if (c && typeof c.randomUUID === "function") return "w" + c.randomUUID().replaceAll("-", "").slice(0, 16);
    if (c && typeof c.getRandomValues === "function") {
      const bytes = c.getRandomValues(new Uint8Array(8));
      return "w" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
    }
    fallbackIdCounter += 1;
    return "w" + Date.now().toString(36) + fallbackIdCounter.toString(36);
  }

  function isRecentEntry(item) {
    return Boolean(item) && ID_PATTERN.test(String(item.id)) && typeof item.name === "string" &&
      item.name !== "" && MODULES.includes(item.kind);
  }

  function popupUrl(kind, id, handoff) {
    return "?module=" + kind + "&win=" + id + (handoff ? "&handoff=" + handoff : "");
  }

  function createWindowBridge(deps) {
    const { document } = deps;
    const env = deps.env || host;
    const desktop = deps.desktopBridge;
    const search = env.location && typeof env.location.search === "string" ? env.location.search : null;
    // A host without a query string (unit-test hosts) behaves as one unmanaged document window.
    const managed = search !== null;
    const launch = managed ? parseLaunch(search) : { module: null, windowId: "", handoff: "" };
    const module = launch.module || (managed ? START : "DOC");
    const isStart = module === START;
    const handoffs = new Map();
    // Document names (opened or saved file names) per module; a managed window only ever has its own.
    let titles = {};
    let dirty = false;
    let lastReported = "";
    let closePrompt = null;

    // Window commands exist only for managed windows; an unmanaged host never calls them.
    const desktopAvailable = () => managed && Boolean(desktop?.isAvailable());
    const popupAvailable = () => {
      const proto = env.location?.protocol;
      return managed && (proto === "http:" || proto === "https:") && typeof env.open === "function";
    };
    const canSpawnWindows = () => managed && (desktopAvailable() || popupAvailable());
    const shell = (key, args) => desktop.windowCommand(key, args);

    function feedback(message) {
      if (typeof deps.setFeedback === "function") deps.setFeedback(message);
    }

    function closeMenus() {
      host.__EASYOFFICE_CLOSE_MENUS__(document);
    }

    // ---- title and state -------------------------------------------------------------------

    const activeKind = () => (typeof deps.getActiveModule === "function" ? deps.getActiveModule() : module);

    // Managed windows name an unsaved document "제목 없음"; the unmanaged single-page host keeps
    // the older fallback so saved files still get their module-specific default name.
    function getDocumentName(mod) {
      return titles[mod || activeKind()] || (managed ? UNTITLED : "");
    }

    function refresh() {
      const name = getDocumentName() || UNTITLED;
      document.title = isStart ? APP_TITLE : formatWindowTitle(name, dirty);
      if (isStart || !desktopAvailable()) return;
      const key = JSON.stringify([name, dirty]);
      if (key === lastReported) return;
      lastReported = key;
      void shell("reportWindowState", { name, dirty }).catch(() => {
        lastReported = "";
      });
    }

    function setDocumentName(next, mod) {
      const cleaned = baseName(next);
      titles = { ...titles, [mod || activeKind()]: cleaned };
      if (!cleaned) delete titles[mod || activeKind()];
      refresh();
    }

    // Undo/redo and restored sessions carry the names with the rest of the document state.
    const getTitles = () => ({ ...titles });
    function applyTitles(next) {
      if (!next || typeof next !== "object") return;
      titles = {};
      for (const [mod, title] of Object.entries(next)) {
        if (typeof title === "string" && title) titles[mod] = baseName(title);
      }
      refresh();
    }

    function setDirty(next) {
      dirty = Boolean(next);
      refresh();
    }

    // ---- spawning windows ------------------------------------------------------------------

    function openPopup(kind, handoff) {
      const id = randomId(env);
      env.open(popupUrl(kind, id, handoff), "eo-" + id, POPUP_FEATURES);
    }

    // The start window has done its job once a document window exists.
    function finishStartWindow() {
      if (isStart && desktopAvailable()) void closeWindow();
    }

    async function newDocument(kind) {
      const wanted = String(kind || "").toUpperCase();
      if (!MODULES.includes(wanted)) throw new Error("알 수 없는 문서 종류입니다: " + kind);
      if (desktopAvailable()) {
        await shell("openDocumentWindow", { kind: wanted });
        finishStartWindow();
        return true;
      }
      if (popupAvailable()) {
        openPopup(wanted, "");
        return true;
      }
      return false;
    }

    async function spawnWithFile(file) {
      const fileName = baseName(file?.name);
      const kind = kindForFileName(fileName);
      if (!kind) throw new Error("지원하지 않는 파일 형식입니다 (" + (fileName || "이름 없음") + ")");
      const bytes = new Uint8Array(await file.arrayBuffer());
      const handoff = randomId(env);
      handoffs.set(handoff, { name: fileName, bytes });
      openPopup(kind, handoff);
    }

    function handlePickedFile(input, resolve, reject) {
      const file = input.files?.[0];
      if (!file) {
        resolve({ status: "cancelled" });
        return;
      }
      spawnWithFile(file).then(() => resolve({ status: "opened" }), reject);
    }

    function pickFileForPopup() {
      return new Promise((resolve, reject) => {
        const input = document.createElement("input");
        input.type = "file";
        input.addEventListener("change", () => handlePickedFile(input, resolve, reject));
        input.click();
      });
    }

    async function openFile() {
      if (desktopAvailable()) {
        const res = await shell("openFileInNewWindow", {});
        if (res && (res.status === "opened" || res.status === "focused")) finishStartWindow();
        return res;
      }
      if (popupAvailable()) return pickFileForPopup();
      return null;
    }

    async function openRecent(id) {
      if (!desktopAvailable()) return null;
      const res = await shell("openRecentDocument", { id: String(id) });
      if (res && (res.status === "opened" || res.status === "focused")) finishStartWindow();
      return res;
    }

    // Serve-mode child windows pick their bytes up from the opener; the desktop shell queues them in Rust.
    async function takePendingOpen() {
      if (desktopAvailable()) {
        const res = await shell("takePendingOpen", {});
        if (!res || typeof res.base64 !== "string" || !res.name) return null;
        // The shared decoder uses the webview's `Uint8Array.fromBase64` when it has one and an
        // unrolled byte loop otherwise (`sheet-utils.js`, loaded before this file).
        const bytes = host.__EASYOFFICE_SHEET_UTILS__.base64ToBytes(res.base64);
        return { name: baseName(res.name), bytes };
      }
      const opener = env.opener;
      if (!launch.handoff || !opener || typeof opener.__EASYOFFICE_TAKE_HANDOFF__ !== "function") return null;
      return opener.__EASYOFFICE_TAKE_HANDOFF__(launch.handoff) || null;
    }

    function takeHandoff(id) {
      const entry = handoffs.get(id);
      handoffs.delete(id);
      return entry || null;
    }

    // ---- recent documents ------------------------------------------------------------------

    function renderRecentButtons(container, entries, className) {
      for (const old of Array.from(container.querySelectorAll("[data-recent-id]"))) old.remove();
      for (const entry of entries) {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = className;
        btn.dataset.recentId = entry.id;
        btn.title = entry.name;
        btn.textContent = entry.name;
        if (container.getAttribute("role") === "menu") {
          btn.setAttribute("role", "menuitem");
          btn.tabIndex = -1;
        }
        btn.addEventListener("click", (ev) => {
          if (ev.stopPropagation) ev.stopPropagation();
          closeMenus();
          openRecent(entry.id).catch((err) => feedback("최근 문서 열기 실패: " + (err.message || err)));
        });
        container.appendChild(btn);
      }
    }

    let recentEntries = [];
    let home = null;

    async function refreshRecents() {
      let entries = [];
      if (desktopAvailable()) {
        try {
          const list = await shell("listRecentDocuments", {});
          entries = Array.isArray(list) ? list.filter(isRecentEntry).slice(0, 10) : [];
        } catch {
          entries = [];
        }
      }
      const menuList = document.getElementById("recent-menu-list");
      if (menuList) {
        renderRecentButtons(menuList, entries, "md-item md-recent");
        const empty = document.getElementById("recent-menu-empty");
        if (empty) empty.hidden = entries.length > 0;
      }
      recentEntries = entries;
      for (const id of ["recent-menu-sep", "recent-dialog-open"]) {
        const el = document.getElementById(id);
        if (el) el.hidden = entries.length === 0;
      }
      if (home) home.setRecents(entries);
      return entries;
    }

    // 목록 비우기: the shell forgets the list, then the start screen, menus and editor menu follow.
    async function clearRecents() {
      await shell("clearRecentDocuments", {});
      const entries = await refreshRecents();
      if (typeof deps.onRecentsChanged === "function") await deps.onRecentsChanged(entries);
    }

    // The full list as a dialog (the reference app's recent-documents dialog); a pick opens like a menu item.
    async function chooseRecent() {
      const dialogs = host.__EASYOFFICE_RECENT_DIALOG__;
      if (!dialogs) return;
      if (recentEntries.length === 0) {
        if (typeof host.__EASYOFFICE_NOTICE__ === "function") host.__EASYOFFICE_NOTICE__("최근 문서가 없습니다.");
        return;
      }
      const picked = await dialogs.open(document, recentEntries, { clearRecentDocuments: clearRecents });
      if (picked) await openRecent(picked.id);
    }

    // ---- closing ---------------------------------------------------------------------------

    async function closeWindow() {
      if (typeof deps.discardStoredState === "function") deps.discardStoredState();
      if (desktopAvailable()) await shell("closeCurrentWindow", {});
      else if (typeof env.close === "function") env.close();
      return true;
    }

    // Quits the app: the shell closes the windows one at a time, each through its own close prompt.
    async function requestQuit() {
      if (desktopAvailable()) await shell("quitApp", {});
    }

    // The save prompt is the shell's native dialog (`confirm_close`, PD-136): it names the document
    // itself and answers save / discard / cancel. The page sends no wording and no file name. Any
    // other answer, a refused call included, counts as cancel so a close never discards by accident.
    // Without the shell (the `--serve` preview) the browser's own confirmation stands in.
    async function askCloseChoice() {
      if (!desktopAvailable()) {
        const ok = typeof env.confirm === "function" ? env.confirm("저장하지 않은 변경 사항이 있습니다. 저장하지 않고 닫을까요?") : true;
        return ok ? "discard" : "cancel";
      }
      try {
        const choice = await shell("confirmClose", {});
        return CLOSE_CHOICES.includes(choice) ? choice : "cancel";
      } catch (err) {
        feedback("저장 확인 창을 열지 못했습니다: " + (err?.message || String(err)));
        return "cancel";
      }
    }

    // The window stays open: its next close asks again, and a running "끝내기" (quit from the menu)
    // ends here instead of closing the rest.
    async function keepOpen() {
      if (desktopAvailable()) await shell("endClosePrompt", {}).catch((err) => feedback("닫기 확인을 마치지 못했습니다: " + (err?.message || String(err))));
      return false;
    }

    // Tells the shell the prompt is on its way to the screen. Until it hears this, a second close
    // request (Cmd+W, the close button, Cmd+Q) treats the page as hung and closes the window once
    // the shell's timeout has passed; after it, a second request only brings the window forward.
    function ackPrompt() {
      if (!desktopAvailable()) return;
      shell("ackClosePrompt", {}).catch((err) => feedback("닫기 확인을 알리지 못했습니다: " + (err?.message || String(err))));
    }

    async function runCloseRequest() {
      if (!deps.isDirty()) return closeWindow();
      ackPrompt();
      const choice = await askCloseChoice();
      if (choice === "cancel") return keepOpen();
      if (choice === "save") {
        let saved;
        try {
          saved = await deps.save();
        } catch {
          saved = false;
        }
        if (saved === false) {
          feedback("저장하지 못해 창을 닫지 않았습니다");
          return keepOpen();
        }
      }
      return closeWindow();
    }

    // One prompt at a time: a second close request while the dialog is open returns the same answer
    // and confirms it to the shell again (the shell may have asked for a prompt this page opened
    // itself, and has not heard about it).
    function requestClose() {
      if (closePrompt) {
        ackPrompt();
        return closePrompt;
      }
      closePrompt = runCloseRequest().finally(() => {
        closePrompt = null;
      });
      return closePrompt;
    }

    // ---- DOM -------------------------------------------------------------------------------

    function report(promise, label) {
      promise.catch((err) => feedback(label + ": " + (err?.message || String(err))));
    }

    function bindUi() {
      for (const el of document.querySelectorAll("[data-window-new]")) {
        el.addEventListener("click", (ev) => {
          if (ev.stopPropagation) ev.stopPropagation();
          closeMenus();
          report(newDocument(el.dataset.windowNew), "새 문서를 열지 못했습니다");
        });
      }
      for (const el of document.querySelectorAll("[data-window-close]")) {
        el.addEventListener("click", (ev) => {
          if (ev.stopPropagation) ev.stopPropagation();
          closeMenus();
          report(requestClose(), "창을 닫지 못했습니다");
        });
      }
      for (const el of document.querySelectorAll("[data-recent-dialog-open]")) {
        el.addEventListener("click", (ev) => {
          if (ev.stopPropagation) ev.stopPropagation();
          closeMenus();
          report(chooseRecent(), "최근 문서를 열지 못했습니다");
        });
      }
      env.__EASYOFFICE_CLOSE_REQUEST__ = managed ? () => requestClose() : undefined;
      if (env.__EASYOFFICE_TAKE_HANDOFF__ === undefined) env.__EASYOFFICE_TAKE_HANDOFF__ = takeHandoff;
    }

    // Menu, start screen and Ctrl+O open files in a NEW window (returns whether the bridge handled it).
    function requestOpenFile() {
      if (!canSpawnWindows()) return false;
      openFile().catch((err) => feedback("파일 열기 실패: " + (err?.message || String(err))));
      return true;
    }

    // Picker and drop in the --serve emulation spawn a window too; the desktop shell handles drops in Rust.
    function routeFile(file) {
      if (!canSpawnWindows() || desktopAvailable()) return false;
      spawnWithFile(file).catch((err) => feedback("파일 열기 실패: " + (err?.message || String(err))));
      return true;
    }

    // A window shows the one module chosen at launch (or the start screen); no module switcher exists.
    async function applyLaunch(activatePane) {
      if (!isStart) {
        if (module !== activeKind()) await activatePane(module);
        return;
      }
      const shellEl = document.getElementById("app");
      if (shellEl) shellEl.dataset.windowKind = "start";
      const create = host.__EASYOFFICE_CREATE_HOME_SCREEN__;
      if (!create || home) return;
      home = create({
        document,
        onOpenRecent: (id) => openRecent(id).catch((err) => feedback("최근 문서 열기 실패: " + (err?.message || err))),
        loadPreview: desktopAvailable() ? (id) => shell("recentPreview", { id }) : null,
        env,
      });
      home.mount(shellEl || document.body);
      home.show();
    }

    // The desktop shell queues the bytes of the file this window was opened for; take them once.
    // `openBytes` resolves `{ ok }`. A file the page could not show (too large for its kind, damaged,
    // a password cancelled) leaves an empty window, and the shell forgets the file: Save from that
    // window must ask where to write, never replace the original. Only that result decides it;
    // `afterShown` (a step for a shown document, such as marking it saved) runs outside of it, so its
    // failure never takes the tracking of an opened file away.
    async function loadPending(openBytes, afterShown) {
      if (isStart) return;
      const pending = await takePendingOpen().catch(() => null);
      if (!pending) return;
      let shown = false;
      try {
        shown = (await openBytes(pending.name, pending.bytes))?.ok === true;
      } finally {
        if (!shown && desktopAvailable()) await shell("abandonTrackedOpen", {}).catch(() => {});
      }
      if (shown) await afterShown?.();
    }

    // Long status texts are clipped with an ellipsis; hovering or focusing one shows the full text.
    function bindStatusTooltips() {
      const bar = document.getElementById("status-bar");
      for (const type of ["mouseover", "focusin"]) {
        if (!bar) break;
        bar.addEventListener(type, (ev) => {
          const el = ev.target;
          if (el?.tagName === "SPAN" && el.id && el.textContent) el.setAttribute("title", el.textContent.trim());
        });
      }
    }

    // `EasyOffice.exe --smoke`: the first window runs the steps in order (each resolves with its
    // detail text or rejects with the reason), then opens a second window; that window reports that
    // it loaded and closes itself again. Every result goes to the shell as a marker over an
    // ordinary engine call (`smoke.rs` lists the accepted names); the first failure stops the run.
    function reportSmoke(invokeIpc, ok, name, text) {
      const marker = ok ? "EASYOFFICE_SMOKE_STEP_OK " : "EASYOFFICE_SMOKE_STEP_FAIL ";
      return Promise.resolve(invokeIpc("scrub_pii", marker + name + " " + text)).catch(() => null);
    }

    function failureText(err) {
      return err instanceof Error ? err.message : String(err);
    }

    async function runSmokeStep(invokeIpc, name, run) {
      let detail;
      try {
        detail = String(await run());
      } catch (err) {
        await reportSmoke(invokeIpc, false, name, failureText(err));
        return false;
      }
      await reportSmoke(invokeIpc, true, name, detail);
      return true;
    }

    async function runSmoke({ invokeIpc, steps }) {
      const role = env.__EASYOFFICE_SMOKE__;
      if (!role) return;
      if (role === "secondary") {
        await reportSmoke(invokeIpc, true, SMOKE_WINDOW_STEP, module);
        await closeWindow();
        return;
      }
      for (const [name, run] of Object.entries(steps)) {
        if (!(await runSmokeStep(invokeIpc, name, run))) return;
      }
      await newDocument("DOC").catch((err) =>
        reportSmoke(invokeIpc, false, SMOKE_WINDOW_STEP, "두 번째 창: " + failureText(err))
      );
    }

    async function start() {
      bindUi();
      refresh();
      return refreshRecents();
    }

    return {
      launch, module, isStart, isManaged: () => managed, canSpawnWindows, desktopAvailable,
      start, bindUi, refresh, setDocumentName, setDirty, getDocumentName, getTitles, applyTitles,
      newDocument, openFile, openRecent, spawnWithFile, takePendingOpen, refreshRecents,
      requestClose, requestQuit, closeWindow, requestOpenFile, chooseRecent, clearRecents, routeFile, applyLaunch, loadPending, bindStatusTooltips, runSmoke,
    };
  }

  host.__EASYOFFICE_WINDOW_BRIDGE__ = {
    MODULES, START, UNTITLED, createWindowBridge, parseLaunch, formatWindowTitle, kindForFileName,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
