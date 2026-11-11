import assert from "node:assert/strict";
import test from "node:test";
import { createMemoryStorage, typeIntoDocParagraph } from "./test-support/modules.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// Keyboard and shell fixes: one Tab stop per dropdown, the recent-documents modal, the outline dialog,
// start-window restore, the Hangul embed's blind tools and the start screen's right-click guard.

const key = (type, k, extra = {}) => ({ type, key: k, preventDefault() {}, stopPropagation() {}, ...extra });
const RECENT = [{ id: "a1", name: "예산.xlsx", kind: "SHEET" }, { id: "b2", name: "계획.hwpx", kind: "HWP" }];

function recentShell() {
  const commands = {};
  for (const name of [
    "open_document_window", "open_file_in_new_window", "take_pending_open", "report_window_state", "close_current_window",
    "clear_recent_documents", "open_recent_document", "report_menu_state", "end_close_prompt", "ack_close_prompt", "confirm_close", "quit_app",
  ]) commands[name] = () => null;
  commands.list_recent_documents = () => RECENT;
  return commands;
}

test("a custom select is one Tab stop: the native select is out of the tab order and the accessibility tree", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-a11y-select" });
  await flushAsyncTurns(10);
  const select = document.getElementById("slide-aspect-select");
  assert.equal(select.tabIndex, -1);
  assert.equal(select.getAttribute("aria-hidden"), "true");
  assert.ok(select.parentElement.querySelector(".custom-select-trigger").getAttribute("aria-label"), "the trigger carries the label");
});

test("the recent-documents dialog traps Tab, makes the page inert and hands focus back on close", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=DOC&win=w-a11y-recent", shellCommands: recentShell() });
  await flushAsyncTurns(10);
  const opener = document.getElementById("recent-dialog-open");
  opener.focus();
  userClick(opener);
  await flushAsyncTurns();
  const dialog = document.querySelector(".recent-documents-dialog");
  const overlay = document.querySelector(".modal-overlay");
  const others = Array.from(document.body.children).filter((el) => el !== overlay);
  assert.ok(others.length > 0 && others.every((el) => el.inert === true), "everything behind the dialog is inert");
  const stops = dialog.querySelectorAll("button").filter((el) => !el.disabled);
  stops[stops.length - 1].focus();
  document.dispatchEvent(key("keydown", "Tab"));
  assert.equal(document.activeElement, stops[0], "Tab from the last control wraps to the first");
  document.dispatchEvent(key("keydown", "Tab", { shiftKey: true }));
  assert.equal(document.activeElement, stops[stops.length - 1], "Shift+Tab from the first wraps to the last");
  document.dispatchEvent(key("keydown", "Escape"));
  await flushAsyncTurns();
  assert.ok(others.every((el) => !el.inert), "closing lifts the inert state");
  assert.equal(document.activeElement, opener, "focus returns to the opener");
});

test("outline entries are keyboard operable: arrows move, Enter picks and closes the dialog", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-a11y-outline" });
  await flushAsyncTurns(10);
  const list = document.getElementById("outline-list");
  const rows = () => list.querySelectorAll(".outline-item");
  assert.ok(rows().length >= 1);
  assert.equal(list.getAttribute("role"), "listbox");
  assert.equal(rows().filter((r) => r.tabIndex === 0).length, 1, "one roving tab stop");
  const dialog = document.getElementById("outline-dialog");
  userClick(document.getElementById("status-studio-pos"));
  assert.equal(Boolean(dialog.open), true);
  rows()[0].focus();
  if (rows().length > 1) {
    list.dispatchEvent(key("keydown", "ArrowDown", { target: rows()[0] }));
    assert.equal(document.activeElement, rows()[1]);
  }
  const current = document.activeElement;
  list.dispatchEvent(key("keydown", "Enter", { target: current }));
  await flushAsyncTurns();
  assert.equal(Boolean(dialog.open), false, "picking an entry closes the dialog");
});

test("the start window does not restore a recovery record; a document window does", async () => {
  const storage = createMemoryStorage();
  const first = await createWorkspaceHarness({ search: "?module=DOC&win=w-a11y-restore", localStorage: storage });
  typeIntoDocParagraph(first.document, "복원되면 안 되는 문단");
  first.advanceTimers(5000);
  await flushAsyncTurns(10);
  assert.ok(storage.getItem("easyoffice.workspace.autosave.v1.w-a11y-restore"), "a record exists for the window id");
  const start = await createWorkspaceHarness({ search: "?module=START&win=w-a11y-restore", localStorage: storage, shellCommands: recentShell() });
  await flushAsyncTurns(10);
  assert.doesNotMatch(start.document.getElementById("status-command-feedback").textContent, /복원/);
  assert.doesNotMatch(start.document.getElementById("doc-editor").textContent, /복원되면 안 되는 문단/);
  const doc = await createWorkspaceHarness({ search: "?module=DOC&win=w-a11y-restore", localStorage: storage });
  await flushAsyncTurns(10);
  assert.match(doc.document.getElementById("status-command-feedback").textContent, /복원/);
});

test("with the embedded Hangul editor active, 개인정보 지우기 and 보안 점검 do not apply", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-a11y-embed" });
  const { studioCommandApplies } = document.defaultView.__EASYOFFICE_WORKSPACE_SESSION_UTILS__;
  assert.equal(studioCommandApplies("scrub-pii", "HWP_EMBED"), false);
  assert.equal(studioCommandApplies("security-audit", "HWP_EMBED"), false);
  assert.equal(studioCommandApplies("scrub-pii", "DOC"), true);
  assert.equal(studioCommandApplies("studio-save", "HWP_EMBED"), true);
});

test("the start screen blocks the web view's right-click menu", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=START&win=w-a11y-ctx", shellCommands: recentShell() });
  await flushAsyncTurns(10);
  const home = document.getElementById("home-screen");
  assert.equal(!home.dispatchEvent({ type: "contextmenu", target: home }), true);
});
