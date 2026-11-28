// Loads one WebUI IIFE module (crates/easyoffice-app/ui/*.js) against an injected host environment,
// the same way scripts/webui-dom-harness.mjs does: through Node's module loader, with the
// environment exposed as `globalThis.__EASYOFFICE_HOST_ENV__` only while the module evaluates.
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const requireModule = createRequire(import.meta.url);
const COMPILED = new Map();

/** `modulePath` is an absolute path string or a `file:` URL. */
export function loadUiModule(modulePath, env) {
  const asPath = modulePath instanceof URL ? fileURLToPath(modulePath) : String(modulePath);
  const resolved = requireModule.resolve(asPath);
  let entry = COMPILED.get(resolved);
  if (!entry) {
    const code = fs.readFileSync(resolved, "utf8");
    const dir = path.dirname(resolved);
    const req = createRequire(resolved);
    const fn = vm.compileFunction(code, ["exports", "require", "module", "__filename", "__dirname"], {
      filename: resolved,
    });
    entry = { fn, req, dir };
    COMPILED.set(resolved, entry);
  }
  const modObj = { exports: {} };
  globalThis.__EASYOFFICE_HOST_ENV__ = env;
  try {
    entry.fn(modObj.exports, entry.req, modObj, resolved, entry.dir);
  } finally {
    delete globalThis.__EASYOFFICE_HOST_ENV__;
  }
  return env;
}

const PERFORM_PARTS = ["copilot-perform-slide.js", "copilot-perform-sheet.js", "copilot-perform-ext.js", "copilot-perform-hwp.js", "copilot-perform-cite.js"];

/** The per-module parts the copilot performer (`copilot-perform.js`) reads when it is created; load them before it. */
export function loadPerformParts(env) {
  for (const name of PERFORM_PARTS) loadUiModule(fileURLToPath(new URL("../../crates/easyoffice-app/ui/" + name, import.meta.url)), env);
}
