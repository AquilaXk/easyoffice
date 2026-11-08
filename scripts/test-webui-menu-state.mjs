import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// What the native menu bar shows for a window (PD-102, menu_state.rs): the page reports its theme, the
// items its module cannot use (the page's own `studioCommandApplies` rule) and whether the Hangul
// editor's own menu row is shown. The Rust side applies the focused window's report to the bar.

const REPO = new URL("../", import.meta.url);
const byName = (a, b) => a.localeCompare(b);
const read = (rel) => fs.readFileSync(fileURLToPath(new URL(rel, REPO)), "utf8");

async function shellPage(search = "?module=DOC&win=w-menu-state") {
  const reports = [];
  const closes = [];
  const harness = await createWorkspaceHarness({
    search,
    shellCommands: {
      report_menu_state: (args) => { reports.push(args); return null; },
      end_close_prompt: () => { closes.push("end-prompt"); return null; },
      report_window_state: () => null,
      close_current_window: () => { closes.push("close"); return null; },
      take_pending_open: () => null,
      list_recent_documents: () => [],
    },
  });
  await flushAsyncTurns(20);
  return { ...harness, reports, closes };
}

function tableCommands(page) {
  const { NATIVE_MENU_ACTIONS } = page.__EASYOFFICE_NATIVE_MENU_TABLE__;
  return Object.entries(NATIVE_MENU_ACTIONS)
    .filter(([, action]) => action.startsWith("studio-cmd:"))
    .map(([id, action]) => [id, action.slice("studio-cmd:".length)]);
}

test("a window reports its theme, module and editor state to the shell", async () => {
  const { reports } = await shellPage();
  const last = reports.at(-1);
  assert.ok(last, "the window reported its state at start");
  assert.deepEqual(Object.keys(last).sort(byName), ["disabled", "embed", "theme"]);
  assert.equal(last.theme, "system");
  assert.equal(last.embed, false);
  assert.ok(Array.isArray(last.disabled));
});

test("the disabled ids are exactly the table items the page's own rule refuses in the window's module", async () => {
  const { document, reports } = await shellPage();
  const page = document.defaultView;
  const rule = page.__EASYOFFICE_WORKSPACE_SESSION_UTILS__.studioCommandApplies;
  const commands = tableCommands(page);
  assert.ok(commands.length >= 60);
  for (const mod of ["DOC", "SHEET", "SLIDE", "PDF", "HWP"]) {
    await switchModule(document, mod);
    await flushAsyncTurns(10);
    const editorOnly = Object.entries(page.__EASYOFFICE_NATIVE_MENU_TABLE__.NATIVE_MENU_ACTIONS)
      .filter(([, action]) => action === "hwp-editor")
      .map(([id]) => id);
    assert.deepEqual(editorOnly.sort(byName), ["table:cell-select", "table:cell-split"]);
    const expected = [...commands.filter(([, command]) => !rule(command, mod)).map(([id]) => id), ...editorOnly];
    assert.ok(reports.length > 0, `${mod} reported`);
    assert.deepEqual([...reports.at(-1).disabled].sort(byName), expected.sort(byName), mod);
  }
});

test("the same rule grays out different items per module: sheet-only items are off in a document and on in a sheet", async () => {
  const { document, reports } = await shellPage();
  await switchModule(document, "DOC");
  await flushAsyncTurns(10);
  assert.ok(reports.at(-1).disabled.includes("table:sheet-pivot"));
  assert.ok(!reports.at(-1).disabled.includes("insert:toc"));
  await switchModule(document, "SHEET");
  await flushAsyncTurns(10);
  assert.ok(!reports.at(-1).disabled.includes("table:sheet-pivot"));
  assert.ok(reports.at(-1).disabled.includes("insert:toc"));
});

test("the editor's menu row and the native bar are never both shown: the Hangul embed reports embed=true", () => {
  const session = read("crates/easyoffice-app/ui/workspace-session.js");
  assert.match(session, /deps\.onMenuContext\(commandModule, embed\)/, "the module context carries the embed flag");
  const app = read("crates/easyoffice-app/ui/app.js");
  assert.match(app, /onMenuContext: \(commandModule, embed\) => shellChrome\.pushMenuState\(commandModule, embed\)/);
  const rust = read("crates/easyoffice-desktop/src/menu_state.rs");
  assert.match(rust, /set_bar_visible\(&window, !embed\)/);
  assert.match(rust, /window\.hide_menu\(\)/);
});

test("a theme change reaches the shell with the new mode, and an unchanged state is not sent twice", async () => {
  const { document, reports } = await shellPage();
  const before = reports.length;
  document.querySelector('[data-theme-mode-choice="dark"]').click();
  await flushAsyncTurns(10);
  assert.ok(reports.length > before, "the change was reported");
  assert.equal(reports.at(-1).theme, "dark");
  const after = reports.length;
  document.querySelector('[data-theme-mode-choice="dark"]').click();
  await flushAsyncTurns(10);
  assert.equal(reports.length, after, "the same state is not sent again");
  document.querySelector('[data-theme-mode-choice="light"]').click();
  await flushAsyncTurns(10);
  assert.equal(reports.at(-1).theme, "light");
});

test("without the desktop shell nothing is reported", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-plain" });
  await flushAsyncTurns(10);
  const page = document.defaultView;
  assert.equal(page.__EASYOFFICE_DESKTOP_BRIDGE__.isAvailable(), false);
});

test("a cancelled or failed close tells the shell to end a running quit; a close that goes through does not", async () => {
  const bridge = read("crates/easyoffice-app/ui/window-bridge.js");
  assert.match(bridge, /if \(choice === "cancel"\) return keepOpen\(\);/);
  assert.match(bridge, /feedback\("저장하지 못해 창을 닫지 않았습니다"\);\s*return keepOpen\(\);/);
  const { document, closes } = await shellPage();
  const close = document.defaultView.__EASYOFFICE_CLOSE_REQUEST__;
  assert.equal(typeof close, "function");
  await close();
  await flushAsyncTurns(6);
  assert.deepEqual(closes, ["close"], "a clean window closes and does not cancel the quit");
});

test("a menu report that did not arrive is sent again", async () => {
  let fail = true;
  const reports = [];
  const { document } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-retry",
    shellCommands: {
      report_menu_state: (args) => {
        reports.push(args);
        if (fail) { fail = false; throw new Error("전송 실패"); }
        return null;
      },
      report_window_state: () => null, take_pending_open: () => null, list_recent_documents: () => [],
    },
  });
  await flushAsyncTurns(20);
  document.querySelector('[data-theme-mode-choice="system"]').click();
  await flushAsyncTurns(10);
  assert.ok(reports.length >= 2, "the state is sent again after the failure");
  assert.deepEqual(reports[1], reports[0], "the same state, not a changed one");
});

test("both new commands are window commands of the bridge, called with the shell token", () => {
  const bridge = read("crates/easyoffice-app/ui/desktop-bridge.js");
  assert.match(bridge, /reportMenuState: "report_menu_state"/);
  assert.match(bridge, /endClosePrompt: "end_close_prompt"/);
  const commands = read("crates/easyoffice-desktop/src/app_commands.rs");
  const capability = JSON.parse(read("crates/easyoffice-desktop/capabilities/main-window.json"));
  for (const command of ["report_menu_state", "end_close_prompt"]) {
    assert.ok(commands.includes(`"${command}"`), command);
    assert.ok(capability.permissions.includes(`allow-${command.replaceAll("_", "-")}`), command);
  }
});

test("the quit item is Rust-only: 끝내기 ends the 파일 menu on Windows and Linux and has no page action", () => {
  const menu = read("crates/easyoffice-desktop/src/native_menu.rs");
  assert.match(menu, /lines\.extend\(entries\(&\[SEPARATOR, QUIT_ID\]\)\)/);
  assert.match(menu, /const QUIT_LABEL: &str = "끝내기";/);
  const chrome = read("crates/easyoffice-app/ui/shell-chrome.js");
  assert.ok(!chrome.includes("file:quit"), "the page cannot quit the app");
});

test("the check items are the three themes and the update switch", () => {
  const menu = read("crates/easyoffice-desktop/src/native_menu.rs");
  assert.match(menu, /CheckMenuItemBuilder::with_id\(spec\.id, spec\.label\)/);
  for (const id of ["view:theme-system", "view:theme-light", "view:theme-dark", "UPDATE_CHECK_ID"]) {
    assert.ok(menu.slice(menu.indexOf("pub const CHECK_IDS"), menu.indexOf("fn is_check_item")).includes(id), id);
  }
});

test("in the Hangul editor window the zoom and cell items are enabled, and the editor-only ones are grayed out elsewhere", async () => {
  const { document, reports } = await shellPage();
  const page = document.defaultView;
  const { EMBED_EDITOR_COMMANDS } = page.__EASYOFFICE_NATIVE_MENU_TABLE__;
  await switchModule(document, "DOC");
  await flushAsyncTurns(10);
  assert.ok(reports.at(-1).disabled.includes("table:cell-select") && reports.at(-1).disabled.includes("table:cell-split"));
  assert.ok(!reports.at(-1).disabled.includes("view:zoom-100"), "zoom is the page's own in a document window");
  const sent = [];
  const shellChrome = page.__EASYOFFICE_CREATE_SHELL_CHROME__({
    document,
    hwpHost: { setShellContext: () => Promise.resolve() },
    desktopBridge: { isAvailable: () => true, windowCommand: (_name, state) => { sent.push(state); return Promise.resolve(); } },
  });
  shellChrome.applyStoredTheme();
  shellChrome.pushMenuState("HWP_EMBED", true);
  const last = sent.at(-1);
  assert.equal(last.embed, true);
  for (const id of Object.keys(EMBED_EDITOR_COMMANDS)) assert.ok(!last.disabled.includes(id), `${id} is enabled in the editor window`);
});
