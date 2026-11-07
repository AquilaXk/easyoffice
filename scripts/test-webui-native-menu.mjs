import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// The macOS menu bar (PD-102): the Rust table and the page's list name the same ids and actions, the
// page runs them through the shortcut handlers, and the shell grants the page nothing for it.

const REPO = new URL("../", import.meta.url);
const read = (rel) => fs.readFileSync(fileURLToPath(new URL(rel, REPO)), "utf8");
const CHROME_JS = new URL("crates/easyoffice-app/ui/shell-chrome.js", REPO);

function rustItems() {
  const source = read("crates/easyoffice-desktop/src/native_menu.rs");
  const start = source.indexOf("pub const ITEMS");
  const table = source.slice(start, source.indexOf("];", start));
  const items = [...table.matchAll(/item\(\s*"([^"]+)",\s*"([^"]+)",\s*(None|Some\("([^"]+)"\)),\s*"([^"]+)",?\s*\)/g)];
  return items.map((m) => ({ id: m[1], label: m[2], accelerator: m[4] ?? null, action: m[5] }));
}

function chromeTable() {
  const env = { setTimeout, clearTimeout };
  loadUiModule(CHROME_JS, env);
  return env.__EASYOFFICE_NATIVE_MENU_TABLE__;
}

test("the Rust menu table and the page's native menu list are the same ids with the same actions", () => {
  const rust = rustItems();
  assert.ok(rust.length >= 10, "the Rust table was parsed");
  const page = chromeTable().NATIVE_MENU_ACTIONS;
  const byName = (a, b) => a.localeCompare(b);
  assert.deepEqual(Object.keys(page).sort(byName), rust.map((r) => r.id).sort(byName));
  for (const { id, action } of rust) assert.equal(page[id], action, id);
});

test("a menu id runs the page action of the same name as its shortcut, with its argument", () => {
  const { runNativeMenuAction } = chromeTable();
  const calls = [];
  const actions = new Proxy({}, { get: (_t, name) => (arg) => calls.push([String(name), arg]) });
  for (const id of ["file:save", "file:new", "view:zoom-100", "edit:find"]) assert.equal(runNativeMenuAction(id, actions), true, id);
  assert.deepEqual(calls, [["studio-save", undefined], ["file-new", undefined], ["ribbon", "view-zoom"], ["palette", undefined]]);
});

test("unknown ids, inherited property names and missing actions run nothing", () => {
  const { runNativeMenuAction } = chromeTable();
  const calls = [];
  const actions = { "studio-save": () => calls.push("save") };
  for (const id of ["", "file:unknown", "constructor", "__proto__", "toString", "file:save\u0000", "FILE:SAVE"]) {
    assert.equal(runNativeMenuAction(id, actions), false, JSON.stringify(id));
  }
  assert.equal(runNativeMenuAction("file:close", actions), false, "the page has no handler for it");
  assert.deepEqual(calls, []);
});

test("every menu id is handled by the real page and the zoom, find and about items act", async () => {
  const { document } = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  const run = document.defaultView.__EASYOFFICE_NATIVE_MENU__;
  assert.equal(typeof run, "function");
  for (const { id } of rustItems()) {
    if (id === "file:close" || id === "file:new" || id === "file:open" || id === "file:open-recent" || id === "file:save" || id === "file:save-as") continue;
    assert.equal(run(id), true, id);
  }
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("cmd-palette-modal").open, true, "Find opens the command palette");
  assert.equal(document.getElementById("status-zoom-label").textContent, "125%", "zoom in, zoom out, then the same 100% command as Ctrl+0");
  assert.equal(document.getElementById("about-dialog").open, true, "About opens the dialog");
  assert.equal(run("nope"), false);
});

test("file items reach the same handlers as their shortcuts", async () => {
  const { document } = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  for (const id of ["file:new", "file:open", "file:open-recent", "file:save", "file:save-as", "file:close"]) {
    assert.equal(document.defaultView.__EASYOFFICE_NATIVE_MENU__(id), true, id);
  }
  await flushAsyncTurns(6);
});

test("the shell installs the menu on macOS only and grants the page no event permission", () => {
  const main = read("crates/easyoffice-desktop/src/main.rs");
  assert.match(main, /#\[cfg\(target_os = "macos"\)\]\s*native_menu::install\(app\)\?;/);
  const capability = JSON.parse(read("crates/easyoffice-desktop/capabilities/main-window.json"));
  assert.ok(capability.permissions.every((p) => !p.includes("event") && !p.startsWith("core:")), "no event or core permission is added for the menu");
  const menu = read("crates/easyoffice-desktop/src/native_menu.rs");
  assert.match(menu, /window\.eval\(&script\)/, "items are delivered by evaluating a fixed script in the focused window");
  assert.ok(!/emit_to|\.emit\(/.test(menu), "no event is emitted to the page");
});
