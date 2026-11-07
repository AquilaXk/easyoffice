(function (host) {
  "use strict";

  // Theme mode (system / light / dark) and the effective theme on <html> (PD-098).
  //
  // Adapted from rhwp-studio `src/core/theme.ts` and `public/theme-init.js` (MIT, Copyright (c)
  // 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md): the same `data-theme-mode` /
  // `data-theme-effective` attributes, the same `only <scheme>` colour scheme, and the same OS
  // change tracking. The preference is stored in this app's own localStorage key. The script is
  // loaded synchronously in <head> so the first paint already has the right theme.

  const STORAGE_KEY = "easyoffice.theme-mode";
  const MODES = ["system", "light", "dark"];
  const DARK_QUERY = "(prefers-color-scheme: dark)";
  const THEME_COLOR = { dark: "#2b3037", light: "#f5f5f5" };
  const listeners = [];
  let mediaQuery = null;

  const root = () => host.document?.documentElement ?? null;

  function readStoredMode() {
    try {
      const stored = host.localStorage?.getItem(STORAGE_KEY);
      return MODES.includes(stored) ? stored : "system";
    } catch {
      return "system";
    }
  }

  function getMode() {
    const el = root();
    const current = el?.dataset.themeMode;
    return MODES.includes(current) ? current : readStoredMode();
  }

  function prefersDark() {
    return Boolean(host.matchMedia?.(DARK_QUERY)?.matches);
  }

  function effectiveOf(mode) {
    if (mode === "system") return prefersDark() ? "dark" : "light";
    return mode === "dark" ? "dark" : "light";
  }

  function setMeta(name, content) {
    const meta = host.document?.querySelector(`meta[name="${name}"]`);
    if (meta) meta.setAttribute("content", content);
  }

  // Writes the attributes the CSS and the other modules read; returns the effective theme.
  function apply(mode) {
    const effective = effectiveOf(mode);
    const el = root();
    if (el) {
      el.dataset.themeMode = mode;
      el.dataset.themeEffective = effective;
      el.style.colorScheme = `only ${effective}`;
    }
    setMeta("color-scheme", `only ${effective}`);
    setMeta("theme-color", THEME_COLOR[effective]);
    return effective;
  }

  // Menu items `data-theme-mode-choice="system|light|dark"` behave as a radio group.
  function syncMenu(mode) {
    const items = host.document?.querySelectorAll("[data-theme-mode-choice]") ?? [];
    for (const item of items) {
      const active = item.dataset.themeModeChoice === mode;
      item.classList.toggle("active", active);
      item.setAttribute("aria-checked", String(active));
    }
  }

  function notify() {
    const mode = getMode();
    const effective = apply(mode);
    syncMenu(mode);
    for (const listener of listeners) listener(effective, mode);
  }

  function setMode(mode) {
    if (!MODES.includes(mode)) return getEffective();
    try {
      host.localStorage?.setItem(STORAGE_KEY, mode);
    } catch {
      // The choice still applies to this window when storage is unavailable.
    }
    const el = root();
    if (el) el.dataset.themeMode = mode;
    notify();
    return getEffective();
  }

  function getEffective() {
    return root()?.dataset.themeEffective === "dark" ? "dark" : "light";
  }

  // Switches to the other theme explicitly (the choice is no longer "system").
  function toggle() {
    return setMode(getEffective() === "dark" ? "light" : "dark");
  }

  function subscribe(listener) {
    listeners.push(listener);
  }

  // Applies the stored mode, wires the menu and follows OS changes while the mode is "system".
  function init() {
    notify();
    if (mediaQuery || typeof host.matchMedia !== "function") return;
    mediaQuery = host.matchMedia(DARK_QUERY);
    mediaQuery?.addEventListener?.("change", () => {
      if (getMode() === "system") notify();
    });
  }

  apply(readStoredMode());
  host.__EASYOFFICE_THEME__ = { MODES, getMode, getEffective, setMode, toggle, subscribe, init, syncMenu };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
