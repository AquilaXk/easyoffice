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

  // The native menu bar (macOS only; `crates/easyoffice-desktop/src/native_menu.rs`, PD-102, PD-121) runs an item by calling
  // `__EASYOFFICE_NATIVE_MENU__(id)` in the focused window. Each id names a page action: the name of an
  // entry of the shortcut action table (so a menu item and its keyboard shortcut run the same handler)
  // or of `pageMenuActions` (the page's own menu item), optionally `name:argument`. A test keeps this
  // list equal to the Rust table and every item of the page's menu bar inside it.
  const NATIVE_MENU_ACTIONS = Object.freeze({
    "app:about": "about",
    "file:new": "file-new",
    "file:new-doc": "window-new:doc",
    "file:new-hwp": "window-new:hwp",
    "file:new-sheet": "window-new:sheet",
    "file:new-slide": "window-new:slide",
    "file:new-pdf": "window-new:pdf",
    "file:new-conv": "window-new:conv",
    "file:open": "file-open",
    "file:open-recent": "recent-dialog",
    "file:save": "studio-save",
    "file:save-as": "file-save-as",
    "file:export-pdf": "hwp-export-pdf",
    "file:print": "print",
    "file:close": "window-close",
    "edit:undo": "studio-cmd:studio-undo",
    "edit:redo": "studio-cmd:studio-redo",
    "edit:cut": "studio-cmd:clip-cut",
    "edit:copy": "studio-cmd:clip-copy",
    "edit:paste": "studio-cmd:clip-paste",
    "edit:find": "find",
    "edit:replace": "replace",
    "view:zoom-in": "studio-cmd:zoom-in",
    "view:zoom-out": "studio-cmd:zoom-out",
    "view:zoom-fit-page": "studio-cmd:zoom-fit-page",
    "view:zoom-fit-width": "studio-cmd:zoom-fit-width",
    "view:zoom-dialog": "hwp-editor",
    "view:zoom-100": "studio-cmd:view-zoom",
    "view:split": "studio-cmd:view-split",
    "view:outline": "studio-cmd:view-outline",
    "view:theme-system": "theme:system",
    "view:theme-light": "theme:light",
    "view:theme-dark": "theme:dark",
    "view:theme-toggle": "studio-cmd:view-theme",
    "view:toolbar-labels": "hwp-editor",
    "view:form-mode": "hwp-editor",
    "view:ctrl-marks": "studio-cmd:ctrl-marks",
    "view:para-marks": "studio-cmd:para-marks",
    "view:doc-grid": "studio-cmd:doc-grid",
    "insert:shape": "studio-cmd:insert-shape",
    "insert:chart": "studio-cmd:insert-chart",
    "insert:footnote": "studio-cmd:mod-footnote",
    "insert:symbol": "studio-cmd:mod-symbol",
    "insert:link": "studio-cmd:insert-link",
    "insert:toc": "studio-cmd:doc-mod-toc",
    "insert:mailmerge": "studio-cmd:doc-mod-mailmerge",
    "insert:field": "studio-cmd:hwp-mod-field",
    "format:bold": "studio-cmd:home-bold",
    "format:italic": "studio-cmd:home-italic",
    "format:underline": "studio-cmd:home-underline",
    "format:style": "studio-cmd:home-style",
    "format:justify": "studio-cmd:para-shape",
    "format:para-numbering": "studio-cmd:para-numbering",
    "format:para-bullet": "studio-cmd:para-bullet",
    "format:indent-in": "studio-cmd:indent-in",
    "format:indent-out": "studio-cmd:indent-out",
    "page:paper": "studio-cmd:layout-paper",
    "page:orientation": "studio-cmd:layout-orientation",
    "page:margins": "studio-cmd:layout-margins",
    "page:header": "studio-cmd:mod-header",
    "page:columns": "studio-cmd:layout-columns",
    "page:hwp-header": "studio-cmd:hwp-mod-header",
    "page:manuscript": "studio-cmd:hwp-mod-manuscript",
    "page:slide-master": "studio-cmd:slide-mod-master",
    "page:slide-snap": "studio-cmd:slide-mod-snap",
    "page:slide-transition": "studio-cmd:slide-mod-transition",
    "page:slide-presenter": "studio-cmd:slide-mod-presenter",
    "table:insert": "studio-cmd:insert-table",
    "table:add-row": "studio-cmd:table-add-row",
    "table:cell-select": "hwp-editor",
    "table:merge": "studio-cmd:table-merge",
    "table:cell-split": "hwp-editor",
    "table:sum": "studio-cmd:table-sum",
    "table:doc-formula": "studio-cmd:doc-mod-tableformula",
    "table:hwp-border": "studio-cmd:hwp-mod-border",
    "table:autosum": "studio-cmd:data-autosum",
    "table:fn-browser": "studio-cmd:data-fn-browser",
    "table:sort": "studio-cmd:data-sort",
    "table:validation": "studio-cmd:data-validation",
    "table:sheet-pivot": "studio-cmd:sheet-mod-pivot",
    "table:sheet-chart": "studio-cmd:sheet-mod-chart",
    "table:sheet-condfmt": "studio-cmd:sheet-mod-condfmt",
    "table:sheet-freeze": "studio-cmd:sheet-mod-freeze",
    "tool:diagnose": "studio-cmd:smart-diagnose",
    "tool:scrub-pii": "studio-cmd:scrub-pii",
    "tool:security-audit": "studio-cmd:security-audit",
    "tool:bench-run": "studio-cmd:bench-run",
    "tool:review-track": "studio-cmd:review-track",
    "tool:review-comment": "studio-cmd:review-comment",
    "tool:review-verify": "studio-cmd:review-verify",
    "tool:review-stats": "studio-cmd:review-stats",
    "tool:pdf-highlight": "studio-cmd:pdf-mod-highlight",
    "tool:pdf-rotate": "studio-cmd:pdf-mod-rotate",
    "tool:pdf-splitmerge": "studio-cmd:pdf-mod-splitmerge",
    "tool:pdf-tooffice": "studio-cmd:pdf-mod-tooffice",
    "tool:palette": "studio-cmd:palette",
    "help:update-check": "update-check",
  });

  // Actions that work on the Hangul editor's pages (the shell's PDF export). It has no item in the
  // page's own menu row: the editor's 파일 menu carries it in the Hangul window, and no other window
  // has it, so the native item is grayed out everywhere but in the editor window. Print is in the
  // page's menu row too, for the modules that print (`studioCommandApplies("print", ...)`).
  const EMBED_ONLY_ACTIONS = Object.freeze(["hwp-export-pdf"]);
  const EMBED_ONLY_SET = new Set(EMBED_ONLY_ACTIONS);
  const START_DISABLED_ACTIONS = Object.freeze(["studio-save", "file-save-as", "find", "replace"]);
  const START_DISABLED_SET = new Set(START_DISABLED_ACTIONS);
  const PARSED_NATIVE_MENU_ACTIONS = Object.freeze(
    Object.fromEntries(
      Object.entries(NATIVE_MENU_ACTIONS).map(([id, entry]) => {
        const [name, argument] = entry.split(":");
        return [id, Object.freeze({ id, name, argument })];
      })
    )
  );
  const PARSED_NATIVE_MENU_LIST = Object.freeze(Object.values(PARSED_NATIVE_MENU_ACTIONS));

  // Menu id -> command id of the Hangul editor (the rhwp-studio command registry, over the embed port's
  // `automation.execute`). While the editor owns the window these items go to it, the zoom and cell
  // items using the same command ids as the reference app's menu events; the table's `hwp-editor` items
  // exist only for it and are grayed out everywhere else.
  const EMBED_EDITOR_COMMANDS = Object.freeze({
    "view:zoom-in": "view:zoom-in",
    "view:zoom-out": "view:zoom-out",
    "view:zoom-100": "view:zoom-100",
    "view:zoom-fit-page": "view:zoom-fit-page",
    "view:zoom-fit-width": "view:zoom-fit-width",
    "view:zoom-dialog": "view:zoom-dialog",
    "view:toolbar-labels": "view:toolbar-labels",
    "view:form-mode": "view:form-mode",
    "table:cell-select": "table:cell-selection-enter",
    "table:merge": "table:cell-merge",
    "table:cell-split": "table:cell-split",
    // The edit menu (macOS binds ⌘Z, ⇧⌘Z, ⌘X, ⌘C to these four, as the reference app does): the editor's
    // own undo stack and selection, which the system items cannot reach inside the editor's frame.
    "edit:undo": "edit:undo",
    "edit:redo": "edit:redo",
    "edit:cut": "edit:cut",
    "edit:copy": "edit:copy",
  });

  // The same four items while a plain text field of the page has the focus (a bound ⌘ key never reaches the
  // field): the browser's own editing command for that field.
  const TEXT_FIELD_EDIT_COMMANDS = Object.freeze({
    "edit:undo": "undo",
    "edit:redo": "redo",
    "edit:cut": "cut",
    "edit:copy": "copy",
  });

  // Actions that click the page's own (hidden) menu item, so a native item does exactly what the page
  // menu did, including doing nothing where the item does not apply to the window's module. The
  // arguments come from the static table above only.
  function pageMenuActions(document) {
    const click = (selector) => document.querySelector(selector)?.click();
    return {
      "studio-cmd": (cmd) => click(`.md-item[data-studio-menu-cmd="${cmd}"]`),
      "window-new": (kind) => click(`[data-window-new="${kind.toUpperCase()}"]`),
      theme: (mode) => click(`[data-theme-mode-choice="${mode}"]`),
      "update-check": () => click("[data-update-check-toggle]"),
    };
  }

  // Runs the page action of a menu id; false for an unknown id or an action the page does not have.
  function runNativeMenuAction(id, actions) {
    const parsed = Object.hasOwn(PARSED_NATIVE_MENU_ACTIONS, id) ? PARSED_NATIVE_MENU_ACTIONS[id] : null;
    if (!parsed) return false;
    const run = actions[parsed.name];
    if (typeof run !== "function") return false;
    run(parsed.argument, id);
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
        pushMenuState();
      });
      api.init();
    }

    // ---- native menu state ---------------------------------------------------------------------

    // The native menu bar (macOS only, PD-121) is shared by all windows, so each window tells the shell
    // what the bar should show while it has the focus (`menu_state.rs`): its theme mode and the items its
    // module cannot use (the page's own rule, `studioCommandApplies`, applied to the table above).
    let menuContext = null;
    let lastMenuState = "";

    function disabledMenuIds(commandModule) {
      const applies = host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__.studioCommandApplies;
      const out = [];
      for (let i = 0; i < PARSED_NATIVE_MENU_LIST.length; i += 1) {
        const { id, name, argument } = PARSED_NATIVE_MENU_LIST[i];
        let isDisabled;
        if (commandModule === "START" && START_DISABLED_SET.has(name)) {
          isDisabled = true;
        } else if (Object.hasOwn(EMBED_EDITOR_COMMANDS, id) && commandModule === "HWP_EMBED") {
          isDisabled = false;
        } else if (EMBED_ONLY_SET.has(name)) {
          isDisabled = commandModule !== "HWP_EMBED";
        } else {
          isDisabled = name === "hwp-editor" || (name === "studio-cmd" && !applies(argument, commandModule)) || (name === "print" && !applies("print", commandModule));
        }
        if (isDisabled) out.push(id);
      }
      return out;
    }

    // Without a new module context the last one is reused (a theme change); nothing is sent before the
    // first one, and an unchanged state is not sent twice.
    function pushMenuState(commandModule) {
      const bridge = deps.desktopBridge || host.__EASYOFFICE_DESKTOP_BRIDGE__;
      if (commandModule !== undefined) menuContext = { commandModule };
      if (!menuContext || !bridge?.isAvailable() || !deps.hasNativeMenu()) return;
      const state = { theme: themeApi().getMode(), disabled: disabledMenuIds(menuContext.commandModule) };
      const key = JSON.stringify(state);
      if (key === lastMenuState) return;
      lastMenuState = key;
      // A report that did not arrive is sent again the next time.
      report(bridge.windowCommand("reportMenuState", state).catch((err) => {
        lastMenuState = "";
        throw err;
      }));
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
        case "recent-dialog": return report(actions.recentDialog?.());
        case "save": return report(actions.save?.());
        case "save-as": return report(actions.saveAs?.(request.format));
        case "export-pdf": return report(actions.exportPdf?.());
        case "print": return report(actions.print?.());
        case "close": return report(actions.close?.());
        case "about": return openAbout();
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
          host.__EASYOFFICE_CLOSE_MENUS__(document);
          if (dialog && typeof dialog.showModal === "function" && !dialog.open) dialog.showModal();
        });
      }
      const closeBtn = document.getElementById("btn-about-close");
      if (closeBtn && dialog) closeBtn.addEventListener("click", () => dialog.close?.());
      dialog?.addEventListener?.("click", (ev) => {
        if (ev.target === dialog) dialog.close?.();
      });
    }

    // "Version x.y.z" under the product name (the reference app's wording).
    function setAppVersion(version) {
      const el = document.getElementById("about-version");
      const text = String(version || "").trim();
      if (!el || !/^[0-9A-Za-z.+-]{1,32}$/.test(text)) return;
      el.textContent = "Version " + text;
      el.hidden = false;
    }

    function bindIconToolbarScroller() {
      const bar = document.getElementById("icon-toolbar");
      if (!bar?.addEventListener) return;
      bar.addEventListener("wheel", (ev) => {
        if ((bar.scrollWidth || 0) <= (bar.clientWidth || 0)) return;
        const delta = ev.deltaY || ev.deltaX || 0;
        if (!delta) return;
        bar.scrollLeft = Math.max(0, Math.min(bar.scrollWidth - bar.clientWidth, (bar.scrollLeft || 0) + delta));
        ev.preventDefault?.();
      });
      bar.addEventListener("focusin", (ev) => {
        const btn = ev.target?.closest?.(".tb-btn");
        if (!btn) return;
        if ((bar.scrollWidth || 0) > (bar.clientWidth || 0) && typeof bar.getBoundingClientRect === "function" && typeof btn.getBoundingClientRect === "function") {
          const barRect = bar.getBoundingClientRect();
          const btnRect = btn.getBoundingClientRect();
          if (btnRect.left < barRect.left) bar.scrollLeft -= barRect.left - btnRect.left;
          else if (btnRect.right > barRect.right) bar.scrollLeft += btnRect.right - barRect.right;
        }
        btn.scrollIntoView?.({ block: "nearest", inline: "nearest" });
      });
    }

    function enableDialogDrag(dialog) {
      const wrap = dialog?.querySelector?.(".dialog-wrap");
      const title = dialog?.querySelector?.(".dialog-title");
      if (!wrap || !title) return;
      let dx = 0, dy = 0, dragging = false, startX = 0, startY = 0;
      const onMove = (ev) => {
        if (!dragging) return;
        dx = (Number(ev.clientX) || 0) - startX;
        dy = (Number(ev.clientY) || 0) - startY;
        wrap.style.transform = `translate(${dx}px, ${dy}px)`;
      };
      const stopDrag = () => {
        if (!dragging) return;
        dragging = false;
        document.removeEventListener?.("mousemove", onMove);
        document.removeEventListener?.("mouseup", stopDrag);
      };
      const reset = () => {
        stopDrag();
        dx = 0;
        dy = 0;
        wrap.style.transform = "";
      };
      title.addEventListener("mousedown", (ev) => {
        if (ev.button !== undefined && ev.button !== 0) return;
        if (ev.target?.closest?.(".dialog-close, button, [data-dialog-close]")) return;
        startX = (Number(ev.clientX) || 0) - dx;
        startY = (Number(ev.clientY) || 0) - dy;
        dragging = true;
        document.addEventListener?.("mousemove", onMove);
        document.addEventListener?.("mouseup", stopDrag);
        ev.preventDefault?.();
      });
      dialog.addEventListener?.("close", reset);
      const origClose = dialog.close;
      if (typeof origClose === "function") {
        dialog.close = function (...args) {
          const out = origClose.apply(this, args);
          reset();
          return out;
        };
      }
    }

    function bind() {
      bindAbout();
      bindIconToolbarScroller();
      for (const dlg of document.querySelectorAll("dialog.tool-dialog")) enableDialogDrag(dlg);
      mirrorFeedbackToToast();
      hwpHost.onCommand(runShellCommand);
    }

    return { bind, openAbout, setAppVersion, applyStoredTheme, pushMenuState, pushRecents, toast, runShellCommand, currentTheme, enableDialogDrag, getThemeMode: () => themeApi().getMode() };
  }

  host.__EASYOFFICE_CREATE_SHELL_CHROME__ = createShellChrome;
  host.__EASYOFFICE_NATIVE_MENU_TABLE__ = { NATIVE_MENU_ACTIONS, PARSED_NATIVE_MENU_ACTIONS, EMBED_EDITOR_COMMANDS, EMBED_ONLY_ACTIONS, EMBED_ONLY_SET, START_DISABLED_SET, TEXT_FIELD_EDIT_COMMANDS, runNativeMenuAction, pageMenuActions };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
