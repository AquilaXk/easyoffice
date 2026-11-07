import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { switchModule, typeIntoDocParagraph, createMemoryStorage } from "./test-support/modules.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// Per-document windows (PD-088): every window hosts exactly one document, the converter or the
// start screen, chosen by its launch parameters. There is no module tab strip and no module switcher.

const WINDOW_BRIDGE_JS = new URL("../crates/easyoffice-app/ui/window-bridge.js", import.meta.url);
const STORAGE_PREFIX = "easyoffice.workspace.autosave.v1";

function recordingShell(overrides = {}) {
  const calls = [];
  const commands = {};
  for (const name of [
    "open_document_window", "open_file_in_new_window", "take_pending_open", "report_window_state",
    "close_current_window", "list_recent_documents", "open_recent_document",
  ]) {
    commands[name] = (args) => {
      calls.push({ name, args });
      return overrides[name] ? overrides[name](args) : null;
    };
  }
  commands.save_document_file = (args) => {
    calls.push({ name: "save_document_file", args });
    return overrides.save_document_file ? overrides.save_document_file(args) : { status: "saved", name: "보고서.docx", bytes: 1 };
  };
  const named = (name) => calls.filter((c) => c.name === name);
  return { commands, calls, named };
}

async function openWindow(search, shellOverrides, options = {}) {
  const shell = recordingShell(shellOverrides);
  const harness = await createWorkspaceHarness({
    search,
    shellCommands: shell.commands,
    localStorage: options.localStorage,
    open: options.open,
    ...options.harness,
  });
  return { ...harness, shell };
}

test("launch parameters are parsed strictly", () => {
  const env = {};
  loadUiModule(WINDOW_BRIDGE_JS, env);
  const { parseLaunch, formatWindowTitle } = env.__EASYOFFICE_WINDOW_BRIDGE__;
  assert.deepEqual(parseLaunch("?module=hwp&win=doc-1-ab12"), { module: "HWP", windowId: "doc-1-ab12", handoff: "" });
  assert.equal(parseLaunch("?module=START").module, "START");
  assert.equal(parseLaunch("?module=EVIL").module, null);
  assert.equal(parseLaunch("?win=../../x").windowId, "");
  assert.equal(parseLaunch("").module, null);
  assert.equal(formatWindowTitle("보고서.docx", false), "보고서.docx — EasyOffice");
  assert.equal(formatWindowTitle("보고서.docx", true), "● 보고서.docx — EasyOffice");
  assert.equal(formatWindowTitle("", false), "제목 없음 — EasyOffice");
});

test("a window shows exactly one module and has no tab strip or module switcher", async () => {
  const { document } = await openWindow("?module=SHEET&win=w-sheet");
  assert.equal(document.getElementById("app").dataset.module, "SHEET");
  assert.equal(document.getElementById("app").dataset.windowKind, "document");
  assert.equal(document.getElementById("pane-sheet").hidden, false);
  assert.equal(document.getElementById("pane-doc").hidden, true);
  assert.equal(document.getElementById("tab-strip"), null);
  assert.equal(document.querySelectorAll(".tab-pill").length, 0);
  assert.equal(document.querySelectorAll(".mod-btn").length, 0);
  assert.equal(document.querySelector(".module-switcher"), null);
  assert.equal(document.getElementById("format-filters"), null);
  assert.equal(document.querySelector('[data-studio-menu-cmd^="module:"]'), null);
  assert.equal(document.title, "제목 없음 — EasyOffice");
});

test("each launch module opens its own pane", async () => {
  for (const [mod, pane] of [["DOC", "pane-doc"], ["SLIDE", "pane-slide"], ["PDF", "pane-pdf"], ["CONV", "pane-converter"]]) {
    const { document } = await openWindow(`?module=${mod}&win=w-${mod}`);
    assert.equal(document.getElementById(pane).hidden, false, mod);
    assert.equal(document.getElementById("app").dataset.module, mod);
  }
});

test("the start window shows creation buttons and the recent list", async () => {
  const { document, shell } = await openWindow("?module=START&win=main", {
    list_recent_documents: () => [{ id: "a1", name: "예산.xlsx", kind: "SHEET" }, { id: "b2", name: "계획.hwpx", kind: "HWP" }],
  });
  assert.equal(document.getElementById("app").dataset.windowKind, "start");
  assert.equal(document.getElementById("start-screen").hidden, false);
  const kinds = document.querySelectorAll("#start-screen [data-window-new]").map((b) => b.dataset.windowNew);
  assert.deepEqual(kinds, ["DOC", "HWP", "SHEET", "SLIDE", "PDF", "CONV"]);
  assert.ok(document.querySelector("#start-screen [data-file-open]"));
  const recent = document.querySelectorAll("#start-recent-list [data-recent-id]");
  assert.deepEqual(recent.map((b) => b.textContent.trim()), ["예산.xlsx", "계획.hwpx"]);
  assert.equal(document.title, "EasyOffice");
  recent[1].click();
  await flushAsyncTurns();
  assert.deepEqual(shell.named("open_recent_document")[0].args, { id: "b2" });
});

test("the start window hides the recent section when there are no recent files", async () => {
  const { document } = await openWindow("?module=START&win=main");
  assert.equal(document.getElementById("start-recent").hidden, true);
});

test("File menu: new document kinds, open, recent, save and close call the window bridge", async () => {
  const { document, shell, userClick } = await openWindow("?module=DOC&win=w-menu", {
    open_file_in_new_window: () => ({ status: "opened" }),
    list_recent_documents: () => [{ id: "r1", name: "지난 문서.docx", kind: "DOC" }],
  });
  const fileMenu = document.querySelector('.menu-item[data-menu="file"]');
  const labels = fileMenu.querySelectorAll(".md-label").map((l) => l.textContent.trim());
  for (const wanted of ["새 문서", "열기…", "최근 문서", "저장", "다른 이름으로 저장", "닫기"]) {
    assert.ok(labels.includes(wanted), `File menu has ${wanted}: ${labels.join("|")}`);
  }
  const kinds = fileMenu.querySelectorAll("[data-window-new]").map((b) => b.dataset.windowNew);
  assert.deepEqual(kinds, ["DOC", "HWP", "SHEET", "SLIDE", "PDF", "CONV"]);

  userClick(fileMenu.querySelector('[data-window-new="HWP"]'));
  await flushAsyncTurns();
  assert.deepEqual(shell.named("open_document_window")[0].args, { kind: "HWP" });

  userClick(fileMenu.querySelector("[data-file-open]"));
  await flushAsyncTurns();
  assert.equal(shell.named("open_file_in_new_window").length, 1);
  assert.equal(shell.named("close_current_window").length, 0, "a document window stays open after opening another file");

  const recent = document.querySelectorAll("#recent-menu-list [data-recent-id]");
  assert.deepEqual(recent.map((b) => b.dataset.recentId), ["r1"]);
});

test("the window bridge never hands a file system path to the page", async () => {
  const { shell, document, userClick } = await openWindow("?module=DOC&win=w-path", {
    open_file_in_new_window: () => ({ status: "opened", path: String.raw`C:\비밀\a.docx` }),
  });
  userClick(document.querySelector("[data-file-open]"));
  await flushAsyncTurns();
  const sent = JSON.stringify(shell.calls);
  assert.ok(!/\\\\|C:/.test(sent.replaceAll("save_document_file", "")), "no path-like argument is ever sent to the shell");
  assert.deepEqual(shell.named("open_file_in_new_window")[0].args, {});
});

test("opening a file from the start window closes the start window", async () => {
  const { document, shell, userClick } = await openWindow("?module=START&win=main", {
    open_file_in_new_window: () => ({ status: "opened" }),
  });
  userClick(document.querySelector("#start-screen [data-file-open]"));
  await flushAsyncTurns();
  assert.equal(shell.named("close_current_window").length, 1);
});

test("a cancelled open dialog keeps the start window", async () => {
  const { document, shell, userClick } = await openWindow("?module=START&win=main", {
    open_file_in_new_window: () => ({ status: "cancelled" }),
  });
  userClick(document.querySelector("#start-screen [data-file-open]"));
  await flushAsyncTurns();
  assert.equal(shell.named("close_current_window").length, 0);
});

test("File menu 닫기 asks before discarding unsaved changes", async () => {
  const { document, shell, userClick, advanceTimers } = await openWindow("?module=DOC&win=w-close", {}, { localStorage: createMemoryStorage() });
  userClick(document.querySelector("[data-window-close]"));
  await flushAsyncTurns();
  assert.equal(shell.named("close_current_window").length, 1, "a clean window closes immediately");

  typeIntoDocParagraph(document, "저장하지 않은 편집");
  advanceTimers(3000);
  await flushAsyncTurns(10);
  userClick(document.querySelector("[data-window-close]"));
  await flushAsyncTurns();
  assert.equal(document.getElementById("close-confirm-dialog").open, true);
  assert.equal(shell.named("close_current_window").length, 1);
});

test("close request: 저장 안 함 closes, 취소 keeps the window, 저장 saves first", async () => {
  const { document, shell, host, userClick, advanceTimers } = await openWindow("?module=DOC&win=w-flow", {}, { localStorage: createMemoryStorage() });
  assert.equal(typeof host.__EASYOFFICE_CLOSE_REQUEST__, "function");
  typeIntoDocParagraph(document, "편집 내용");
  advanceTimers(3000);
  await flushAsyncTurns(10);
  const dialog = document.getElementById("close-confirm-dialog");

  const first = host.__EASYOFFICE_CLOSE_REQUEST__();
  await flushAsyncTurns();
  assert.equal(dialog.open, true);
  userClick(document.getElementById("btn-close-cancel"));
  await first;
  assert.equal(dialog.open, false);
  assert.equal(shell.named("close_current_window").length, 0);

  const second = host.__EASYOFFICE_CLOSE_REQUEST__();
  await flushAsyncTurns();
  userClick(document.getElementById("btn-close-save"));
  await second;
  await flushAsyncTurns(10);
  assert.equal(shell.named("save_document_file").length, 1, "저장 runs the native save first");
  assert.equal(shell.named("close_current_window").length, 1);

  typeIntoDocParagraph(document, "다시 편집");
  advanceTimers(3000);
  await flushAsyncTurns(10);
  const third = host.__EASYOFFICE_CLOSE_REQUEST__();
  await flushAsyncTurns();
  userClick(document.getElementById("btn-close-discard"));
  await third;
  assert.equal(shell.named("close_current_window").length, 2);
});

test("a cancelled save dialog keeps the window open", async () => {
  const { document, shell, host, userClick, advanceTimers } = await openWindow(
    "?module=DOC&win=w-cancel-save",
    { save_document_file: () => ({ status: "cancelled" }) },
    { localStorage: createMemoryStorage() },
  );
  typeIntoDocParagraph(document, "편집 내용");
  advanceTimers(3000);
  await flushAsyncTurns(10);
  const pending = host.__EASYOFFICE_CLOSE_REQUEST__();
  await flushAsyncTurns();
  userClick(document.getElementById("btn-close-save"));
  await pending;
  await flushAsyncTurns(10);
  assert.equal(shell.named("close_current_window").length, 0);
});

test("dirty state and the document name are reported to the shell for the native title", async () => {
  const { document, shell, advanceTimers } = await openWindow("?module=DOC&win=w-title", {}, { localStorage: createMemoryStorage() });
  const states = () => shell.named("report_window_state").map((c) => c.args);
  assert.deepEqual(states().at(-1), { name: "제목 없음", dirty: false });
  typeIntoDocParagraph(document, "수정");
  advanceTimers(3000);
  await flushAsyncTurns(10);
  assert.deepEqual(states().at(-1), { name: "제목 없음", dirty: true });
  assert.equal(document.title, "● 제목 없음 — EasyOffice");
});

test("a pending open (bytes and display name only) is loaded into the new window", async () => {
  const docx = Buffer.from("PK\u0003\u0004fake").toString("base64");
  const { shell, document } = await openWindow("?module=DOC&win=w-pending", {
    take_pending_open: () => ({ name: "받은 문서.docx", base64: docx }),
  });
  assert.equal(shell.named("take_pending_open").length, 1);
  const fb = document.getElementById("status-command-feedback").textContent;
  assert.match(fb, /받은 문서\.docx/);
});

test("windows keep separate autosave records keyed by their window id", async () => {
  const storage = createMemoryStorage();
  const a = await openWindow("?module=DOC&win=w-a", {}, { localStorage: storage });
  const b = await openWindow("?module=DOC&win=w-b", {}, { localStorage: storage });
  typeIntoDocParagraph(a.document, "A 창에서만 편집");
  a.advanceTimers(3000);
  await flushAsyncTurns(10);
  assert.ok(storage.map.has(`${STORAGE_PREFIX}.w-a`), "window A has its own record");
  assert.ok(!storage.map.has(`${STORAGE_PREFIX}.w-b`), "window B has none");
  assert.ok(!storage.map.has(STORAGE_PREFIX), "the shared legacy key is not used by managed windows");
  assert.notEqual(b.document.getElementById("doc-editor").querySelector("p").textContent, "A 창에서만 편집");
  const reopened = await openWindow("?module=DOC&win=w-a", {}, { localStorage: storage });
  assert.equal(reopened.document.getElementById("doc-editor").querySelector("p").textContent, "A 창에서만 편집");
  const fresh = await openWindow("?module=DOC&win=w-z", {}, { localStorage: storage });
  assert.notEqual(fresh.document.getElementById("doc-editor").querySelector("p").textContent, "A 창에서만 편집");
});

test("--serve emulation opens a new browser window with the same page and launch parameters", async () => {
  const opened = [];
  const { document, userClick } = await openWindow("?module=DOC&win=w-serve", undefined, {
    harness: { protocol: "http:", shellCommands: undefined },
    open: (url, target, features) => { opened.push({ url, target, features }); return {}; },
  });
  userClick(document.querySelector('[data-window-new="SLIDE"]'));
  await flushAsyncTurns();
  assert.equal(opened.length, 1);
  assert.match(opened[0].url, /^\/?\?module=SLIDE&win=[A-Za-z0-9_-]+$/);
  assert.match(opened[0].target, /^eo-/);
});

test("a host without window support keeps the single-document behaviour", async () => {
  const { document } = await createWorkspaceHarness({});
  assert.equal(document.getElementById("app").dataset.module, "DOC");
  switchModule(document, "SHEET");
  await flushAsyncTurns();
  assert.equal(document.getElementById("pane-sheet").hidden, false);
});

// ---- shortcut forwarding from the embedded HWP editor ------------------------------------------

test("the shortcut table marks which combos the HWP editor forwards", async () => {
  const { host } = await openWindow("?module=DOC&win=w-keys");
  const bindings = host.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS;
  const forwarded = bindings.filter((b) => b.forwardFromEditor).map((b) => b.keys).sort((a, b) => a.localeCompare(b));
  assert.deepEqual(forwarded, ["Ctrl+K", "Ctrl+N", "Ctrl+O", "Ctrl+S", "Ctrl+W"]);
  for (const b of bindings.filter((x) => ["Ctrl+N", "Ctrl+W"].includes(x.keys))) assert.equal(b.desktopOnly, true);
  assert.ok(forwarded.every((k) => k.startsWith("Ctrl+")), "plain typing is never forwarded");
});

test("a forwarded combo runs the same action as the keyboard; unknown or plain keys are ignored", async () => {
  const { host } = await openWindow("?module=HWP&win=w-fwd");
  const catalog = host.__EASYOFFICE_RIBBON_CATALOG__;
  const ran = [];
  const session = host.__EASYOFFICE_CREATE_WORKSPACE_SESSION__({
    document: { getElementById: () => null, querySelectorAll: () => [], activeElement: null, addEventListener() {} },
    shortcutBindings: catalog.WORKSPACE_SHORTCUT_BINDINGS,
    getActiveModule: () => "HWP",
    getHwpHost: () => null,
    captureWorkspaceSnapshot: () => ({}),
    restoreWorkspaceSnapshot: async () => {},
    runExclusive: (fn) => Promise.resolve().then(fn),
    getWorkspacePanels: () => null,
    getSheetController: () => null,
    isDesktopShell: () => true,
  });
  const actions = {
    "studio-save": () => ran.push("save"), "file-open": () => ran.push("open"), "file-new": () => ran.push("new"),
    palette: () => ran.push("palette"), "window-close": () => ran.push("close"),
  };
  for (const combo of ["Ctrl+S", "Ctrl+O", "Ctrl+N", "Ctrl+K", "Ctrl+W"]) {
    assert.equal(session.runForwardedShortcut(combo, actions), true, combo);
  }
  assert.deepEqual(ran, ["save", "open", "new", "palette", "close"]);
  for (const combo of ["Ctrl+C", "Ctrl+Z", "A", "가", "Ctrl+Alt+Delete", "", undefined, 42]) {
    assert.equal(session.runForwardedShortcut(combo, actions), false, String(combo));
  }
  assert.equal(ran.length, 5);
});

test("desktop-only combos are not forwarded without the desktop shell", async () => {
  const { host } = await openWindow("?module=HWP&win=w-fwd2");
  const catalog = host.__EASYOFFICE_RIBBON_CATALOG__;
  const session = host.__EASYOFFICE_CREATE_WORKSPACE_SESSION__({
    document: { getElementById: () => null, querySelectorAll: () => [], activeElement: null, addEventListener() {} },
    shortcutBindings: catalog.WORKSPACE_SHORTCUT_BINDINGS,
    getActiveModule: () => "HWP", getHwpHost: () => null, captureWorkspaceSnapshot: () => ({}),
    restoreWorkspaceSnapshot: async () => {}, runExclusive: (fn) => Promise.resolve().then(fn),
    getWorkspacePanels: () => null, getSheetController: () => null, isDesktopShell: () => false,
  });
  const ran = [];
  assert.equal(session.runForwardedShortcut("Ctrl+W", { "window-close": () => ran.push("close") }), false);
  assert.equal(session.runForwardedShortcut("Ctrl+N", { "file-new": () => ran.push("new") }), false);
  assert.deepEqual(ran, []);
});

test("Ctrl+N and Ctrl+W work from the page itself in the desktop shell", async () => {
  const { shell, sandbox, document, advanceTimers } = await openWindow("?module=DOC&win=w-keys2", {}, { localStorage: createMemoryStorage() });
  sandbox.dispatchWindowKeydown({ key: "n", ctrlKey: true });
  await flushAsyncTurns();
  assert.deepEqual(shell.named("open_document_window")[0].args, { kind: "DOC" }, "new document of the same kind");
  sandbox.dispatchWindowKeydown({ key: "w", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(shell.named("close_current_window").length, 1);
  typeIntoDocParagraph(document, "x");
  advanceTimers(3000);
  await flushAsyncTurns(10);
  sandbox.dispatchWindowKeydown({ key: "w", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(document.getElementById("close-confirm-dialog").open, true);
});

test("HWP autosave records are keyed by the window id", async () => {
  const env = { setTimeout, clearTimeout, btoa: (s) => Buffer.from(s, "binary").toString("base64") };
  loadUiModule(new URL("../crates/easyoffice-app/ui/hwp-session.js", import.meta.url), env);
  const hwpHost = { onChange() {}, onStatus() {}, getStatus: () => "idle" };
  const mk = (id) => env.__EASYOFFICE_CREATE_HWP_SESSION__({
    document: { getElementById: () => null, querySelectorAll: () => [] }, hwpHost, env,
    invokeIpc: async () => ({}), getActiveModule: () => "HWP", getDocumentId: () => id,
    store: { put: async () => {}, get: async () => null, remove: async () => {}, purgeStale: async () => 0 },
  });
  assert.equal(mk("w-1").getTabId(), "hwp-w-1");
  assert.equal(mk("w-2").getTabId(), "hwp-w-2");
});

test("macOS Command shortcuts map onto the Ctrl-based shortcut table and the platform hint shows the Command key", () => {
  const SESSION_JS = new URL("../crates/easyoffice-app/ui/workspace-session.js", import.meta.url);
  const env = loadUiModule(SESSION_JS, { navigator: { platform: "MacIntel" } });
  const { shortcutComboFromEvent, guessClientPlatform } = env.__EASYOFFICE_WORKSPACE_SESSION_UTILS__;
  const ev = (init) => ({ key: "", code: "", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...init });
  assert.equal(shortcutComboFromEvent(ev({ key: "s", metaKey: true })), "Ctrl+S");
  assert.equal(shortcutComboFromEvent(ev({ key: "z", metaKey: true, shiftKey: true })), "Ctrl+Shift+Z");
  assert.equal(shortcutComboFromEvent(ev({ key: "s", ctrlKey: true })), "Ctrl+S");
  assert.deepEqual(guessClientPlatform(), { platform: "macOS", shortcutModifier: "⌘" });
});
