// EasyOffice-only (config/rhwp-studio-overrides.json, assetOverrides/index.html): the shell embeds
// this editor in `<iframe sandbox="allow-scripts allow-modals">` (PD-083). A frame without
// `allow-same-origin` has an opaque origin, where reading `window.localStorage` or
// `window.sessionStorage` throws a SecurityError and `indexedDB.open` throws synchronously.
// The editor reads and writes settings (theme, locale, recent symbols) at module top level, so one
// uncaught throw there stops the whole bundle. This classic script runs before every other script
// (no inline code: the CSP forbids it) and, only when the browser storage is unreachable, replaces
//   - localStorage / sessionStorage with an in-memory Storage (values live until the frame closes),
//   - indexedDB with undefined (the editor's `typeof indexedDB` checks pick their memory fallback).
// Where storage works (the editor opened on its own origin) nothing is touched.
(() => {
  function reachable(name) {
    try {
      return globalThis[name] != null;
    } catch {
      return false;
    }
  }

  function createMemoryStorage() {
    const items = new Map();
    return {
      get length() {
        return items.size;
      },
      key: (index) => [...items.keys()][index] ?? null,
      getItem: (key) => (items.has(String(key)) ? items.get(String(key)) : null),
      setItem: (key, value) => {
        items.set(String(key), String(value));
      },
      removeItem: (key) => {
        items.delete(String(key));
      },
      clear: () => {
        items.clear();
      },
    };
  }

  const blocked = ["localStorage", "sessionStorage"].filter((name) => !reachable(name));
  if (blocked.length === 0) return;
  for (const name of blocked) {
    const storage = createMemoryStorage();
    Object.defineProperty(globalThis, name, { configurable: true, enumerable: true, get: () => storage });
  }
  // The storage APIs are blocked together in an opaque origin, so IndexedDB goes with them.
  Object.defineProperty(globalThis, "indexedDB", { configurable: true, enumerable: true, value: undefined });
})();
