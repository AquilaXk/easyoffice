import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

// Storage shim of the embedded editor: in a frame without allow-same-origin the browser
// storage throws, and the editor reads it at module top level. The shim runs first and swaps in
// memory storage only where the real one is unreachable.

const SHIM = fs.readFileSync(new URL("../apps/hwp-studio/static/storage-shim.js", import.meta.url), "utf8");
const INDEX_HTML = fs.readFileSync(new URL("../apps/hwp-studio/index.html", import.meta.url), "utf8");

function opaqueOriginGlobal() {
  const global = {};
  for (const name of ["localStorage", "sessionStorage"]) {
    Object.defineProperty(global, name, {
      configurable: true,
      get() {
        throw new DOMException("The document is sandboxed and lacks the 'allow-same-origin' flag.", "SecurityError");
      },
    });
  }
  Object.defineProperty(global, "indexedDB", {
    configurable: true,
    get: () => ({ open() { throw new DOMException("denied", "SecurityError"); } }),
  });
  return global;
}

function run(global) {
  const context = vm.createContext(global);
  vm.runInContext(SHIM, context);
  return vm.runInContext("globalThis", context);
}

test("opaque origin: both storages become working in-memory storages and indexedDB is undefined", () => {
  const g = run(opaqueOriginGlobal());
  for (const name of ["localStorage", "sessionStorage"]) {
    const storage = g[name];
    assert.equal(storage.getItem("rhwp-settings"), null);
    storage.setItem("rhwp-settings", '{"theme":{"mode":"dark"}}');
    storage.setItem("n", 5);
    assert.equal(storage.getItem("rhwp-settings"), '{"theme":{"mode":"dark"}}');
    assert.equal(storage.getItem("n"), "5", "values are stored as strings");
    assert.equal(storage.length, 2);
    assert.equal(storage.key(0), "rhwp-settings");
    assert.equal(storage.key(5), null);
    storage.removeItem("n");
    assert.equal(storage.getItem("n"), null);
    storage.clear();
    assert.equal(storage.length, 0);
  }
  g.localStorage.setItem("only-local", "1");
  assert.equal(g.sessionStorage.getItem("only-local"), null, "the two storages are separate");
  assert.equal(typeof g.indexedDB, "undefined", "the editor's `typeof indexedDB` checks pick their memory fallback");
});

test("working storage is left alone", () => {
  const local = { getItem: () => "real", setItem() {} };
  const session = { getItem: () => "real", setItem() {} };
  const indexedDB = { open() {} };
  const g = run({ localStorage: local, sessionStorage: session, indexedDB });
  assert.equal(g.localStorage, local);
  assert.equal(g.sessionStorage, session);
  assert.equal(g.indexedDB, indexedDB);
});

test("a storage that is only partly reachable: the blocked one is replaced, the other is kept", () => {
  const session = { getItem: () => "real" };
  const g = opaqueOriginGlobal();
  Object.defineProperty(g, "sessionStorage", { configurable: true, value: session });
  const out = run(g);
  assert.equal(out.sessionStorage, session);
  out.localStorage.setItem("a", "b");
  assert.equal(out.localStorage.getItem("a"), "b");
});

test("index.html loads the shim as an external classic script before every other script", () => {
  const scripts = [...INDEX_HTML.matchAll(/<script\b([^>]*)>/g)].map((m) => m[1]);
  assert.match(scripts[0], /src="\.\/storage-shim\.js"/);
  assert.doesNotMatch(scripts[0], /module|defer|async/, "it must run synchronously, before the bundle");
  assert.ok(scripts.length > 1);
});
