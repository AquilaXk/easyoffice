import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// The native menu bar (PD-102), on every OS: the Rust table and the page's list name the same ids and actions, the
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
  assert.deepEqual(calls, [["studio-save", undefined], ["file-new", undefined], ["studio-cmd", "view-zoom"], ["find", undefined]]);
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

// The reference app's menu event ids (`menu.rs`: 표 Select Cells / Merge Cells / Split Cells, 보기 Zoom In / Zoom
// Out / Actual Size / Fit Page / Fit Width): inside the Hangul editor each native item is that editor command.
const EXPECTED_EDITOR_COMMANDS = {
  "view:zoom-in": "view:zoom-in",
  "view:zoom-out": "view:zoom-out",
  "view:zoom-100": "view:zoom-100",
  "view:zoom-fit-page": "view:zoom-fit-page",
  "view:zoom-fit-width": "view:zoom-fit-width",
  "table:cell-select": "table:cell-selection-enter",
  "table:merge": "table:cell-merge",
  "table:cell-split": "table:cell-split",
};

test("the native menu ids that reach the Hangul editor map to the reference app's editor command ids", () => {
  const { EMBED_EDITOR_COMMANDS, NATIVE_MENU_ACTIONS } = chromeTable();
  assert.deepEqual({ ...EMBED_EDITOR_COMMANDS }, EXPECTED_EDITOR_COMMANDS);
  const rust = new Map(rustItems().map((r) => [r.id, r]));
  for (const id of Object.keys(EXPECTED_EDITOR_COMMANDS)) assert.ok(rust.has(id), `${id} is in the Rust table`);
  // Items that exist only for the editor have no page action of their own: they are the `hwp-editor` ones.
  const editorOnly = Object.keys(NATIVE_MENU_ACTIONS).filter((id) => NATIVE_MENU_ACTIONS[id] === "hwp-editor");
  assert.deepEqual(editorOnly.sort((a, b) => a.localeCompare(b)), ["table:cell-select", "table:cell-split"]);
  assert.equal(rust.get("table:cell-select").accelerator, "CmdOrCtrl+Alt+T");
  assert.equal(rust.get("view:zoom-100").accelerator, "CmdOrCtrl+0");
  assert.deepEqual(["table:cell-select", "table:merge", "table:cell-split"].map((id) => rust.get(id).label), ["셀 선택", "셀 합치기", "셀 나누기"]);
});

test("outside the Hangul editor the editor-only items run nothing and the shared ones keep their page action", () => {
  const { runNativeMenuAction } = chromeTable();
  const calls = [];
  const actions = { "hwp-editor": (_arg, id) => calls.push(["editor", id]), "studio-cmd": (cmd) => calls.push(["page", cmd]) };
  for (const id of ["table:cell-select", "table:merge"]) assert.equal(runNativeMenuAction(id, actions), true, id);
  assert.deepEqual(calls, [["editor", "table:cell-select"], ["page", "table-merge"]]);
});

// The page-side behaviour behind each kind of native action: the page element a native item clicks.
const PAGE_ITEM_SELECTORS = {
  "studio-cmd": (cmd) => `.md-item[data-studio-menu-cmd="${cmd}"]`,
  "window-new": (kind) => `[data-window-new="${kind.toUpperCase()}"]`,
  theme: (mode) => `[data-theme-mode-choice="${mode}"]`,
  "update-check": () => "[data-update-check-toggle]",
};

test("every menu id is handled by the real page; the page-item ids reach an existing page item and the find, zoom and about items act", async () => {
  const { document } = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  const run = document.defaultView.__EASYOFFICE_NATIVE_MENU__;
  assert.equal(typeof run, "function");
  for (const { id, action } of rustItems()) {
    const [name, argument] = action.split(":");
    const selector = PAGE_ITEM_SELECTORS[name];
    if (selector) assert.ok(document.querySelector(selector(argument)), `${id} has its page item`);
  }
  for (const id of ["edit:find", "view:zoom-in", "view:zoom-out", "view:zoom-100", "app:about"]) assert.equal(run(id), true, id);
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("cmd-palette-modal").open, true, "Find opens the command palette");
  assert.equal(document.getElementById("status-zoom-label").textContent, "100%", "zoom in, zoom out, then the 100% command as Ctrl+0");
  assert.equal(document.getElementById("about-dialog").open, true, "About opens the dialog");
  assert.equal(run("nope"), false);
});

// How a page menu item is named in the native table: the generic kinds name themselves, the rest are
// the items that already had their own action (a shortcut handler or the About dialog).
const OWN_ACTIONS = {
  "data-file-open": "file-open",
  "data-window-close": "window-close",
  "data-about-open": "about",
  "data-recent-dialog-open": "recent-dialog",
  "data-save-as-generic": "file-save-as",
};
const OWN_STUDIO_COMMANDS = { "studio-save": "studio-save", palette: "find" };
// Visible only in the Hangul editor window, where the whole menu row is hidden.
const NEVER_VISIBLE = ["data-hwp-save-as"];

function pageMenuItemActions() {
  const html = read("crates/easyoffice-app/ui/index.html");
  const menu = html.slice(html.indexOf('<div class="studio-menubar"'), html.indexOf('class="topbar-actions"'));
  const found = [];
  for (const m of menu.matchAll(/<(?:div|button)\b[^>]*>/g)) {
    const tag = m[0];
    const attr = (name) => tag.match(new RegExp(`\\b${name}="([^"]*)"`))?.[1];
    if (attr("data-studio-menu-cmd")) found.push(attr("data-save-as-generic") ? OWN_ACTIONS["data-save-as-generic"] : OWN_STUDIO_COMMANDS[attr("data-studio-menu-cmd")] ?? `studio-cmd:${attr("data-studio-menu-cmd")}`);
    else if (attr("data-window-new")) found.push(`window-new:${attr("data-window-new").toLowerCase()}`);
    else if (attr("data-theme-mode-choice")) found.push(`theme:${attr("data-theme-mode-choice")}`);
    else if (tag.includes("data-update-check-toggle")) found.push("update-check");
    else {
      const own = Object.keys(OWN_ACTIONS).find((name) => tag.includes(name));
      const hidden = NEVER_VISIBLE.find((name) => tag.includes(name));
      if (own) found.push(OWN_ACTIONS[own]);
      else assert.ok(hidden || !/data-(?:studio|window|file|hwp|theme|update|about|recent)/.test(tag), `unclassified page menu item ${tag}`);
    }
  }
  return found;
}

test("every item of the page's menu bar is in the native menu (nothing is lost when the page menu is hidden)", () => {
  const native = new Set(rustItems().map((r) => r.action));
  // The edit items macOS draws as system items still keep their table entry (Windows and Linux use it).
  const missing = pageMenuItemActions().filter((action) => !native.has(action));
  assert.deepEqual([...new Set(missing)], []);
  assert.ok(pageMenuItemActions().length >= 60, "the page menu was parsed");
});

test("the page menu bar gives way once the desktop shell is present, and not otherwise", async () => {
  const shell = await createWorkspaceHarness({
    shellCommands: { get_workspace_bootstrap: () => null },
  });
  await flushAsyncTurns(10);
  assert.equal(shell.document.getElementById("app").dataset.nativeMenu, "true");
  const plain = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  assert.notEqual(plain.document.getElementById("app").dataset.nativeMenu, "true", "--serve and browsers keep the page menu");
  const css = read("crates/easyoffice-app/ui/shell.css");
  assert.match(css, /\.app-shell\[data-native-menu="true"\] \.studio-menubar \{ display: none; \}/);
});

test("Actual size (view:zoom-100 and the page menu's 100% item) resets any zoom to 100%, not the next step", async () => {
  const { document } = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  const run = document.defaultView.__EASYOFFICE_NATIVE_MENU__;
  const label = () => document.getElementById("status-zoom-label").textContent;
  const stageZoom = () => document.querySelector(".canvas-stage").style.zoom;
  const pageItem = document.querySelector('[data-studio-menu-cmd="view-zoom"]');
  assert.ok(pageItem, "the page menu has the 100% item");
  for (const [name, reset] of [["native view:zoom-100", () => run("view:zoom-100")], ["page menu item", () => pageItem.click()]]) {
    for (const steps of [0, 1, 2, 3]) {
      for (let i = 0; i < steps; i += 1) run("view:zoom-in");
      reset();
      await flushAsyncTurns(6);
      assert.equal(label(), "100%", `${name} after ${steps} zoom-in`);
      assert.equal(stageZoom(), "100%", `${name} stage after ${steps} zoom-in`);
    }
    reset();
    await flushAsyncTurns(6);
    assert.equal(label(), "100%", `${name} twice in a row stays at 100%`);
  }
});

test("Find goes to the editor's own find command in the Hangul window and to the palette elsewhere", () => {
  const app = read("crates/easyoffice-app/ui/app.js");
  assert.match(app, /activeModule === "HWP" && hwpHost\.isEmbedActive\(\)/);
  assert.match(app, /hwpHost\.call\("automation\.execute", \{ id: "edit:find", allowDialog: true \}\)/);
  const router = read("apps/hwp-studio/src/embed/rpc-router.ts");
  assert.match(router, /case 'automation\.execute'/, "the embed port already carries editor commands");
});

test("in the Hangul window the zoom and cell items are sent to the editor over the embed port, Actual Size (Cmd+0) included", () => {
  const app = read("crates/easyoffice-app/ui/app.js");
  assert.match(app, /Object\.hasOwn\(EMBED_EDITOR_COMMANDS, id\) && editorCommand\(id\)/);
  assert.match(app, /activeModule !== "HWP" \|\| !hwpHost\.isEmbedActive\(\)/);
  assert.match(app, /hwpHost\.call\("automation\.execute", \{ id: EMBED_EDITOR_COMMANDS\[menuId\], allowDialog: true \}\)/);
  assert.equal(chromeTable().EMBED_EDITOR_COMMANDS["view:zoom-100"], "view:zoom-100");
});

test("file items reach the same handlers as their shortcuts", async () => {
  const { document } = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  for (const id of ["file:new", "file:open", "file:open-recent", "file:save", "file:save-as", "file:close"]) {
    assert.equal(document.defaultView.__EASYOFFICE_NATIVE_MENU__(id), true, id);
  }
  await flushAsyncTurns(6);
});

test("the shell installs the menu on every OS and grants the page no event permission", () => {
  const main = read("crates/easyoffice-desktop/src/main.rs");
  assert.match(main, /native_menu::install\(app\)\?;/);
  assert.ok(!/cfg\(target_os = "macos"\)\]\s*native_menu::install/.test(main), "not limited to macOS");
  const capability = JSON.parse(read("crates/easyoffice-desktop/capabilities/main-window.json"));
  assert.ok(capability.permissions.every((p) => !p.includes("event") && !p.startsWith("core:")), "no event or core permission is added for the menu");
  const menu = read("crates/easyoffice-desktop/src/native_menu.rs");
  assert.match(menu, /window\.eval\(&script\)/, "items are delivered by evaluating a fixed script in the focused window");
  assert.ok(!/emit_to|\.emit\(/.test(menu), "no event is emitted to the page");
});
