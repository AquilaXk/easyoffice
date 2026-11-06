// Loads one WebUI IIFE module (crates/easyoffice-app/ui/*.js) against an injected host environment,
// the same way scripts/webui-dom-harness.mjs does: through Node's module loader, with the
// environment exposed as `globalThis.__EASYOFFICE_HOST_ENV__` only while the module evaluates.
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const requireModule = createRequire(import.meta.url);

/** `modulePath` is an absolute path string or a `file:` URL. */
export function loadUiModule(modulePath, env) {
  const asPath = modulePath instanceof URL ? fileURLToPath(modulePath) : String(modulePath);
  const resolved = requireModule.resolve(asPath);
  delete requireModule.cache[resolved];
  globalThis.__EASYOFFICE_HOST_ENV__ = env;
  try {
    requireModule(resolved);
  } finally {
    delete globalThis.__EASYOFFICE_HOST_ENV__;
  }
  return env;
}
