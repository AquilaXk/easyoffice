import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readText } from "./test-support/read-text.mjs";

// frame-probe.js (smoke step `frame_ipc`): what the probe page reports for the Tauri objects a
// webview may put into a frame.

const SOURCE = readText(new URL("../crates/easyoffice-app/ui/frame-probe.js", import.meta.url));
const PAGE = readText(new URL("../crates/easyoffice-app/ui/frame-probe.html", import.meta.url));

async function runProbe(globals) {
  const posted = [];
  const window = {
    parent: { postMessage: (data, origin) => posted.push({ data, origin }) },
    setTimeout: (fn) => setTimeout(fn, 5),
    ...globals,
  };
  window.window = window;
  vm.runInNewContext(SOURCE, window);
  for (let i = 0; i < 40 && posted.length === 0; i += 1) await new Promise((r) => setTimeout(r, 5));
  assert.equal(posted.length, 1);
  assert.equal(posted[0].origin, "*", "the shell is cross-origin to the probe frame");
  return JSON.parse(JSON.stringify(posted[0].data));
}

test("no Tauri objects in the frame: nothing to call", async () => {
  assert.deepEqual(await runProbe({}), { type: "eo-frame-probe", internals: "absent", webview: "absent", invoke: "absent" });
});

test("a command the shell refuses is reported as rejected", async () => {
  const calls = [];
  const data = await runProbe({
    chrome: { webview: {} },
    __TAURI_INTERNALS__: { invoke: (command) => { calls.push(command); return Promise.reject("refused"); } },
  });
  assert.deepEqual(calls, ["get_update_notice"], "a read-only command");
  assert.deepEqual(data, { type: "eo-frame-probe", internals: "present", webview: "present", invoke: "rejected" });
});

test("a command that runs is reported as allowed (the smoke step fails on it)", async () => {
  const data = await runProbe({ __TAURI_INTERNALS__: { invoke: () => Promise.resolve({ available: false }) } });
  assert.equal(data.invoke, "allowed");
});

test("a command nobody answers is reported as no-answer", async () => {
  const data = await runProbe({ __TAURI_INTERNALS__: { invoke: () => new Promise(() => {}) } });
  assert.equal(data.invoke, "no-answer");
});

test("the probe page is an external script only (the editor's CSP forbids inline code)", () => {
  assert.match(PAGE, /<script src="\.\/frame-probe\.js"><\/script>/);
  assert.equal([...PAGE.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)].every((m) => m[1].trim() === ""), true);
});
