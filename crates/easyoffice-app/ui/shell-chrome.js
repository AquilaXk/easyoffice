(function (host) {
  "use strict";

  // Shell chrome around the embedded HWP editor (#158). While the editor owns a window it brings its
  // own menu row, theme and status line, so the shell keeps out of the way: the editor's 파일 menu
  // forwards seven whitelisted shell commands (see hwp-host.js), the shell theme (light, dark or system)
  // is pushed into the editor, and shell notices become toasts because the shell status bar is hidden
  // (the CSS keys off `data-hwp-embed`). The About dialog holds the diagnostics that used to crowd the
  // status bar.

  const TOAST_MS = 4500;
  const TOAST_ERROR_MS = 8000;
  const ERROR_HINT = /실패|오류|거부|불가|없습니다|취소|못했습니다/;

  // The macOS menu bar (`crates/easyoffice-desktop/src/native_menu.rs`, PD-102) runs an item by calling
  // `__EASYOFFICE_NATIVE_MENU__(id)` in the focused window. Each id names a page action: the name of an
  // entry of the shortcut action table (so a menu item and its keyboard shortcut run the same handler),
  // optionally `name:argument`. A test keeps this list equal to the Rust table.
  const NATIVE_MENU_ACTIONS = Object.freeze({
    "app:about": "about",
    "file:new": "file-new",
    "file:open": "file-open",
    "file:open-recent": "recent-dialog",
    "file:save": "studio-save",
    "file:save-as": "file-save-as",
    "file:close": "window-close",
    "edit:find": "find",
    "view:zoom-in": "zoom-in",
    "view:zoom-out": "zoom-out",
    "view:zoom-100": "ribbon:view-zoom",
  });

  // Runs the page action of a menu id; false for an unknown id or an action the page does not have.
  function runNativeMenuAction(id, actions) {
    const entry = Object.hasOwn(NATIVE_MENU_ACTIONS, id) ? NATIVE_MENU_ACTIONS[id] : null;
    if (!entry) return false;
    const [name, argument] = entry.split(":");
    const run = actions[name];
    if (typeof run !== "function") return false;
    run(argument);
    return true;
  }

  function createShellChrome(deps) {
    const { document, hwpHost } = deps;
    const env = deps.env || host;
    const actions = deps.actions || {};
    let toastTimer = null;

    const report = (promise) => {
      Promise.resolve(promise).catch((err) => toast((err?.message || String(err))));
    };

    // ---- theme -----------------------------------------------------------------------------

    const themeApi = () => host.__EASYOFFICE_THEME__;

    function currentTheme() {
      return themeApi().getEffective();
    }

    // Applies the stored theme mode (the first paint already follows it) and keeps the embedded
    // editor on the same mode: it follows every later change at once.
    function applyStoredTheme() {
      const api = themeApi();
      api.subscribe((effective, mode) => {
        if (deps.ribbonState) deps.ribbonState.darkThemeEnabled = effective === "dark";
        deps.icons?.syncThemeToggle(document, effective);
        report(hwpHost.setShellContext({ theme: mode }));
      });
      api.init();
    }

    // ---- toasts (shell notices while the shell status bar is hidden) -------------------------

    const embedActive = () => document.getElementById("app")?.dataset?.hwpEmbed === "true";

    function toast(message) {
      const text = String(message || "").trim();
      const el = document.getElementById("shell-toast");
      if (!el || !text) return;
      el.textContent = text;
      el.hidden = false;
      if (toastTimer !== null) env.clearTimeout(toastTimer);
      toastTimer = env.setTimeout(() => {
        el.hidden = true;
        toastTimer = null;
      }, ERROR_HINT.test(text) ? TOAST_ERROR_MS : TOAST_MS);
    }

    // Every module writes its notices into the status line; while the editor owns the window the same
    // text is also shown as a toast. The element keeps working as before for every other window.
    function mirrorFeedbackToToast() {
      const fb = document.getElementById("status-command-feedback");
      if (!fb) return false;
      let holder = fb;
      let descriptor = null;
      while (holder && !descriptor) {
        descriptor = Object.getOwnPropertyDescriptor(holder, "textContent");
        holder = descriptor ? holder : Object.getPrototypeOf(holder);
      }
      if (!descriptor) return false;
      let stored = descriptor.value;
      const read = descriptor.get ? () => descriptor.get.call(fb) : () => stored;
      const write = descriptor.set
        ? (value) => descriptor.set.call(fb, value)
        : (value) => {
            stored = value;
          };
      Object.defineProperty(fb, "textContent", {
        configurable: true,
        get: read,
        set(value) {
          write(value);
          if (embedActive()) toast(value);
        },
      });
      return true;
    }

    // ---- editor 파일 menu --------------------------------------------------------------------

    // Runs a command the editor forwarded. `hwp-host.js` has already checked it against the whitelist.
    function runShellCommand(request) {
      switch (request?.command) {
        case "new": return report(actions.newDocument?.());
        case "open": return report(actions.openFile?.());
        case "recent": return report(actions.openRecent?.(request.id));
        case "save": return report(actions.save?.());
        case "save-as": return report(actions.saveAs?.(request.format));
        case "export-pdf": return report(actions.exportPdf?.());
        case "print": return report(actions.print?.());
        case "close": return report(actions.close?.());
        default: return undefined;
      }
    }

    function pushRecents(entries) {
      return hwpHost.setShellContext({ recents: Array.isArray(entries) ? entries : [] });
    }

    // ---- About dialog -------------------------------------------------------------------------

    function openAbout() {
      const dialog = document.getElementById("about-dialog");
      if (dialog && typeof dialog.showModal === "function" && !dialog.open) dialog.showModal();
    }

    function bindAbout() {
      const dialog = document.getElementById("about-dialog");
      for (const el of document.querySelectorAll("[data-about-open]")) {
        el.addEventListener("click", (ev) => {
          if (ev.stopPropagation) ev.stopPropagation();
          for (const open of document.querySelectorAll(".menu-item.open")) open.classList.remove("open");
          if (dialog && typeof dialog.showModal === "function" && !dialog.open) dialog.showModal();
        });
      }
      const closeBtn = document.getElementById("btn-about-close");
      if (closeBtn && dialog) closeBtn.addEventListener("click", () => dialog.close?.());
    }

    // "EasyOffice x.y.z" under the dialog title (hop shows its version the same way).
    function setAppVersion(version) {
      const el = document.getElementById("about-version");
      const text = String(version || "").trim();
      if (!el || !/^[0-9A-Za-z.+-]{1,32}$/.test(text)) return;
      el.textContent = "EasyOffice " + text;
      el.hidden = false;
    }

    function bind() {
      bindAbout();
      mirrorFeedbackToToast();
      hwpHost.onCommand(runShellCommand);
    }

    return { bind, openAbout, setAppVersion, applyStoredTheme, pushRecents, toast, runShellCommand, currentTheme, getThemeMode: () => themeApi().getMode() };
  }

  host.__EASYOFFICE_CREATE_SHELL_CHROME__ = createShellChrome;
  host.__EASYOFFICE_NATIVE_MENU_TABLE__ = { NATIVE_MENU_ACTIONS, runNativeMenuAction };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
