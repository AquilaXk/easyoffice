import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { readText } from "./test-support/read-text.mjs";

// Menu bar and command palette keyboard access, find wording, and user-facing error text.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (name) => readText(fileURLToPath(new URL(name, UI_DIR)));

const titleOf = (document, name) => document.querySelector(`.menu-item[data-menu="${name}"] > .menu-title`);
const press = (el, key, extra = {}) => el.dispatchEvent({ type: "keydown", key, target: el, ...extra });
const menuOpen = (document) => document.querySelectorAll(".menu-item.open").map((el) => el.dataset.menu);

test("menu items are focusable menuitems and menu titles announce their popup", async () => {
  const { document } = await createWorkspaceHarness();
  const items = document.querySelectorAll(".md-item, .md-sub");
  assert.ok(items.length >= 95);
  for (const item of items) {
    assert.match(item.getAttribute("role"), /^menuitem/, item.textContent);
    assert.equal(item.getAttribute("tabindex"), "-1", item.textContent);
  }
  for (const title of document.querySelectorAll(".menu-title")) {
    assert.equal(title.getAttribute("aria-haspopup"), "menu");
    assert.equal(title.getAttribute("aria-expanded"), "false");
  }
});

test("ArrowDown on a title opens the menu on its first item; arrows, Home and End move and wrap", async () => {
  const { document } = await createWorkspaceHarness();
  const file = titleOf(document, "file");
  file.focus();
  press(file, "ArrowDown");
  assert.deepEqual(menuOpen(document), ["file"]);
  assert.equal(file.getAttribute("aria-expanded"), "true");
  const dropdown = document.querySelector('.menu-item[data-menu="file"] > [role="menu"]');
  const direct = dropdown.querySelectorAll(".md-item, .md-sub").filter((el) => el.parentElement === dropdown && !el.hidden);
  assert.ok(document.activeElement === direct[0], "first item has focus");
  press(document.activeElement, "ArrowDown");
  assert.ok(document.activeElement === direct[1]);
  press(document.activeElement, "End");
  assert.ok(document.activeElement === direct[direct.length - 1]);
  press(document.activeElement, "ArrowDown");
  assert.ok(document.activeElement === direct[0], "wraps to the first item");
  press(document.activeElement, "ArrowUp");
  assert.ok(document.activeElement === direct[direct.length - 1], "wraps to the last item");
  press(document.activeElement, "Home");
  assert.ok(document.activeElement === direct[0]);
});

test("ArrowRight opens a submenu, ArrowLeft and Escape leave it, Escape closes the menu and returns focus to the title", async () => {
  const { document, sandbox } = await createWorkspaceHarness();
  const file = titleOf(document, "file");
  file.focus();
  press(file, "ArrowDown");
  const sub = document.activeElement;
  assert.ok(sub.classList.contains("md-sub"), "새 문서 is the first item");
  press(sub, "ArrowRight");
  const first = document.activeElement;
  assert.equal(first.dataset.windowNew, "DOC");
  press(first, "ArrowLeft");
  assert.ok(document.activeElement === sub);
  press(sub, "Enter");
  assert.equal(document.activeElement.dataset.windowNew, "DOC", "Enter opens the submenu as well");
  sandbox.dispatchWindowKeydown({ key: "Escape", target: document.activeElement });
  assert.ok(document.activeElement === sub, "Escape in a submenu returns to its parent item");
  assert.deepEqual(menuOpen(document), ["file"]);
  sandbox.dispatchWindowKeydown({ key: "Escape", target: sub });
  assert.deepEqual(menuOpen(document), []);
  assert.equal(file.getAttribute("aria-expanded"), "false");
  assert.ok(document.activeElement === file, "focus returns to the menu title");
});

test("ArrowLeft and ArrowRight move between menus; titles move with the menu open", async () => {
  const { document } = await createWorkspaceHarness();
  const file = titleOf(document, "file");
  const edit = titleOf(document, "edit");
  file.focus();
  press(file, "ArrowRight");
  assert.ok(document.activeElement === edit);
  assert.deepEqual(menuOpen(document), [], "a closed menu bar stays closed");
  press(edit, "ArrowDown");
  const lastMenu = document.querySelectorAll(".studio-menubar > .menu-item").pop();
  press(edit, "ArrowLeft");
  assert.ok(document.activeElement === file);
  assert.deepEqual(menuOpen(document), ["file"], "moving with a menu open opens the neighbour");
  press(file, "ArrowLeft");
  assert.ok(document.activeElement === lastMenu.querySelector(".menu-title"), "wraps to the last menu");
  press(document.activeElement, "Home");
  assert.ok(document.activeElement === file);
  press(file, "End");
  assert.ok(document.activeElement === lastMenu.querySelector(".menu-title"));
  const editFirst = (() => {
    edit.focus();
    press(edit, "ArrowDown");
    return document.activeElement;
  })();
  press(editFirst, "ArrowRight");
  assert.equal(menuOpen(document).length, 1);
  assert.ok(document.activeElement !== editFirst, "ArrowRight on a plain item goes to the next menu");
});

test("Enter runs an enabled item, closes the menu and returns focus to the title; a disabled item does nothing", async () => {
  const { document } = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  const edit = titleOf(document, "edit");
  edit.focus();
  press(edit, "ArrowDown");
  const palette = document.querySelector('.menu-item[data-menu="edit"] .md-item[data-studio-menu-cmd="palette"]');
  palette.focus();
  press(palette, "Enter");
  await flushAsyncTurns(4);
  assert.deepEqual(menuOpen(document), []);
  assert.equal(document.getElementById("cmd-palette-modal").open, true, "Find (command search) ran");

  const disabled = document.querySelectorAll(".md-item[data-studio-menu-cmd]").find((el) => el.getAttribute("aria-disabled") === "true");
  assert.ok(disabled, "some item is disabled in this module");
  const menuEl = disabled.closest(".menu-item");
  const title = menuEl.querySelector(".menu-title");
  title.focus();
  press(title, "Enter");
  const feedback = document.getElementById("status-command-feedback").textContent;
  disabled.focus();
  press(disabled, " ");
  assert.deepEqual(menuOpen(document), [menuEl.dataset.menu], "a disabled item keeps the menu open");
  assert.equal(document.getElementById("status-command-feedback").textContent, feedback);
});

test("the recent-documents menu entries are menuitems as well", async () => {
  const bridge = read("window-bridge.js");
  assert.match(bridge, /setAttribute\("role", "menuitem"\)/);
});

test("command palette is a listbox: options carry ids and aria-selected, the input follows the selection", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  const input = document.getElementById("cmd-search-input");
  const list = document.getElementById("cmd-palette-list");
  assert.equal(list.getAttribute("role"), "listbox");
  assert.equal(input.getAttribute("role"), "combobox");
  assert.equal(input.getAttribute("aria-controls"), "cmd-palette-list");
  userClick(document.getElementById("btn-command-palette"));
  const options = list.querySelectorAll("li");
  assert.ok(options.length > 2);
  for (const li of options) assert.equal(li.getAttribute("role"), "option");
  assert.equal(options[0].getAttribute("aria-selected"), "true");
  assert.equal(options[1].getAttribute("aria-selected"), "false");
  assert.equal(input.getAttribute("aria-activedescendant"), options[0].id);
  sandbox.dispatchWindowKeydown({ key: "ArrowDown" });
  const moved = list.querySelectorAll("li");
  assert.equal(moved[1].getAttribute("aria-selected"), "true");
  assert.equal(moved[0].getAttribute("aria-selected"), "false");
  assert.equal(input.getAttribute("aria-activedescendant"), moved[1].id);
  assert.equal(new Set(moved.map((li) => li.id)).size, moved.length, "option ids are unique");
  sandbox.dispatchWindowKeydown({ key: "ArrowUp" });
  assert.equal(input.getAttribute("aria-activedescendant"), list.querySelectorAll("li")[0].id);
  input.value = "zzzz-no-such-command";
  input.dispatchEvent({ type: "input", target: input });
  assert.equal(input.getAttribute("aria-activedescendant"), null, "no selection when nothing matches");
  assert.equal(list.querySelectorAll("li")[0].getAttribute("aria-disabled"), "true");
});

test("Find in the menu and on the toolbar is labelled for what it does: it opens the command palette", async () => {
  const { document, sandbox } = await createWorkspaceHarness();
  const item = document.querySelector('.md-item[data-studio-menu-cmd="palette"]');
  assert.match(item.querySelector(".md-label").textContent, /명령 검색/);
  const button = document.querySelector('#icon-toolbar [data-studio-cmd="palette"]');
  assert.match(button.getAttribute("aria-label"), /명령 검색/);
  assert.match(button.getAttribute("title"), /Ctrl\+F/);
  const ev = sandbox.dispatchWindowKeydown({ key: "f", ctrlKey: true });
  assert.equal(ev.defaultPrevented, true);
  assert.equal(document.getElementById("cmd-palette-modal").open, true);
});

test("user-facing text never shows internal terms", () => {
  // Text a person can see: error messages and notices in the transport, desktop bridge, file open and session.
  const files = ["ipc-transport.js", "desktop-bridge.js", "file-open.js", "workspace-session.js"];
  const internal = /--serve|IPC|엔진 응답|데스크톱 셸|백엔드|로컬 서버|호출 토큰|데스크톱 호스트/;
  for (const name of files) {
    const literals = read(name).split("\n").filter((line) => !/^\s*(\/\/|\*)/.test(line)).join("\n");
    const strings = literals.match(/"[^"\n]*[가-힣][^"\n]*"/g) || [];
    for (const text of strings) assert.doesNotMatch(text, internal, `${name}: ${text}`);
  }
  const html = read("index.html");
  assert.doesNotMatch(html.replace(/<!--[\s\S]*?-->/g, ""), />[^<]*(--serve|IPC)[^<]*</);
});
