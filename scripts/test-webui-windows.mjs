import assert from "node:assert/strict";
import fs from "node:fs";
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
    "close_current_window", "list_recent_documents", "clear_recent_documents", "open_recent_document", "report_menu_state", "end_close_prompt", "ack_close_prompt", "quit_app",
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
  assert.equal(document.getElementById("home-screen").hidden, false);
  const kinds = document.querySelectorAll("#home-screen [data-window-new]").map((b) => b.dataset.windowNew);
  assert.deepEqual(kinds, ["DOC", "HWP", "SHEET", "SLIDE", "PDF", "CONV"]);
  assert.ok(document.querySelector("#home-screen [data-file-open]"));
  const recent = document.querySelectorAll("#home-screen .home-recent-card[data-recent-id]");
  assert.deepEqual(recent.map((b) => b.querySelector(".home-recent-name").textContent.trim()), ["예산.xlsx", "계획.hwpx"]);
  assert.deepEqual(recent.map((b) => b.querySelector(".home-recent-path").textContent.trim()), ["스프레드시트", "한글 문서"]);
  assert.equal(document.title, "EasyOffice");
  recent[1].click();
  await flushAsyncTurns();
  assert.deepEqual(shell.named("open_recent_document")[0].args, { id: "b2" });
});

test("the start window says so when there are no recent files", async () => {
  const { document } = await openWindow("?module=START&win=main");
  assert.ok(document.querySelector("#home-screen .home-recent").classList.contains("is-empty"));
  assert.match(document.querySelector(".home-recent-hint").textContent, /아직 최근 문서가 없습니다/);
  assert.equal(document.querySelectorAll(".home-recent-card").length, 0);
});

test("recent cards are paged five at a time with placeholders and 이전/다음 buttons", async () => {
  const entries = Array.from({ length: 7 }, (_, i) => ({ id: "r" + i, name: `문서${i}.docx`, kind: "DOC" }));
  const { document, userClick } = await openWindow("?module=START&win=main", { list_recent_documents: () => entries });
  const cards = () => document.querySelectorAll(".home-recent-track .home-recent-card");
  const real = () => document.querySelectorAll(".home-recent-track .home-recent-card[data-recent-id]");
  const label = () => document.querySelector(".home-recent-page-label").textContent;
  assert.equal(cards().length, 5);
  assert.equal(real().length, 5);
  assert.equal(label(), "1 / 2");
  assert.equal(document.querySelector(".home-recent-page-prev").disabled, true);
  userClick(document.querySelector(".home-recent-page-next"));
  assert.equal(label(), "2 / 2");
  assert.equal(cards().length, 5, "the second page is padded with hidden placeholders");
  assert.deepEqual(real().map((c) => c.dataset.recentId), ["r5", "r6"]);
  assert.equal(document.querySelectorAll(".home-recent-card.is-placeholder").length, 3);
  assert.equal(document.querySelector(".home-recent-page-next").disabled, true);
  assert.equal(document.querySelector(".home-recent-card.is-placeholder").getAttribute("aria-hidden"), "true");
});

test("one page of recents hides the pagination controls", async () => {
  const { document } = await openWindow("?module=START&win=main", {
    list_recent_documents: () => [{ id: "a", name: "하나.docx", kind: "DOC" }],
  });
  assert.equal(document.querySelector(".home-recent-pagination").hidden, true);
});

test("the home screen keeps the reference app's structure and is the only visible part of a start window", async () => {
  const { document } = await openWindow("?module=START&win=main");
  const screen = document.getElementById("home-screen");
  assert.equal(screen.tagName, "SECTION");
  assert.equal(screen.getAttribute("aria-label"), "시작 화면");
  assert.equal(screen.parentElement.id, "app");
  assert.equal(screen.parentElement.childNodes[0], screen, "the screen comes first in the shell");
  assert.deepEqual(
    [".home-hero .home-title", ".home-hero .home-subtitle", ".home-hero .home-actions", ".home-recent .home-section-header .home-section-title", ".home-recent .home-recent-hint", ".home-recent .home-recent-track", ".home-drop-hint"].map((sel) => screen.querySelector(sel) !== null),
    new Array(7).fill(true)
  );
  assert.equal(screen.querySelector(".home-section-title").textContent, "최근 문서");
  for (const action of screen.querySelectorAll(".home-action")) {
    assert.ok(action.querySelector(".home-action-title").textContent.length > 0);
    assert.ok(action.querySelector(".home-action-description").textContent.length > 0);
    assert.ok(action.querySelector(".home-action-icon svg"), "every action has an SVG icon");
  }
  assert.equal(document.querySelectorAll(".start-card, #start-screen").length, 0, "the old start markup is gone");
});

test("a document window has no home screen", async () => {
  const { document } = await openWindow("?module=DOC&win=w-nohome");
  assert.equal(document.getElementById("home-screen"), null);
});

test("the recent-documents dialog lists the entries and opens the chosen one", async () => {
  const entries = [{ id: "a1", name: "예산.xlsx", kind: "SHEET" }, { id: "b2", name: "계획.hwpx", kind: "HWP" }];
  const { document, shell, userClick } = await openWindow("?module=DOC&win=w-recent-dialog", { list_recent_documents: () => entries });
  const trigger = document.getElementById("recent-dialog-open");
  assert.equal(trigger.hidden, false);
  userClick(trigger);
  await flushAsyncTurns();
  const dialog = document.querySelector(".recent-documents-dialog");
  assert.equal(dialog.getAttribute("role"), "dialog");
  const items = dialog.querySelectorAll(".recent-documents-item");
  assert.deepEqual(items.map((i) => i.querySelector(".recent-documents-name").textContent), ["예산.xlsx", "계획.hwpx"]);
  assert.equal(items[0].getAttribute("aria-selected"), "true");
  document.dispatchEvent({ type: "keydown", key: "ArrowDown", preventDefault() {}, stopPropagation() {} });
  assert.equal(items[1].getAttribute("aria-selected"), "true");
  document.dispatchEvent({ type: "keydown", key: "Enter", preventDefault() {}, stopPropagation() {} });
  await flushAsyncTurns();
  assert.equal(document.querySelector(".recent-documents-dialog"), null, "the dialog closes");
  assert.deepEqual(shell.named("open_recent_document")[0].args, { id: "b2" });
});

test("Escape and 취소 close the recent-documents dialog without opening anything", async () => {
  const entries = [{ id: "a1", name: "예산.xlsx", kind: "SHEET" }];
  const { document, shell, userClick } = await openWindow("?module=DOC&win=w-recent-esc", { list_recent_documents: () => entries });
  userClick(document.getElementById("recent-dialog-open"));
  await flushAsyncTurns();
  document.dispatchEvent({ type: "keydown", key: "Escape", preventDefault() {}, stopPropagation() {} });
  await flushAsyncTurns();
  assert.equal(document.querySelector(".recent-documents-dialog"), null);
  userClick(document.getElementById("recent-dialog-open"));
  await flushAsyncTurns();
  userClick(document.querySelectorAll(".recent-documents-dialog .dialog-btn")[2]);
  await flushAsyncTurns();
  assert.equal(document.querySelector(".recent-documents-dialog"), null);
  assert.equal(shell.named("open_recent_document").length, 0);
});

test("목록 비우기 clears through the shell without asking, closes the dialog and empties the start screen", async () => {
  let entries = [{ id: "a1", name: "예산.xlsx", kind: "SHEET" }];
  const { document, shell, userClick } = await openWindow("?module=START&win=main", {
    list_recent_documents: () => entries,
    clear_recent_documents: () => { entries = []; return null; },
  });
  assert.equal(document.querySelectorAll("#home-screen .home-recent-card[data-recent-id]").length, 1);
  userClick(document.getElementById("recent-dialog-open"));
  await flushAsyncTurns();
  const footer = document.querySelector(".recent-documents-dialog .dialog-footer");
  const labels = footer.querySelectorAll("button").map((b) => b.textContent);
  assert.deepEqual(labels, ["목록 비우기", "열기", "취소"]);
  userClick(footer.querySelector(".recent-documents-clear"));
  await flushAsyncTurns();
  assert.equal(shell.named("clear_recent_documents").length, 1);
  assert.deepEqual(shell.named("clear_recent_documents")[0].args, {});
  assert.equal(document.querySelector(".recent-documents-dialog"), null, "the dialog closes");
  assert.equal(document.querySelectorAll("#home-screen .home-recent-card[data-recent-id]").length, 0);
  assert.equal(document.getElementById("recent-dialog-open").hidden, true);
  assert.equal(shell.named("open_recent_document").length, 0);
});

test("a failed 목록 비우기 keeps the dialog open and the button usable", async () => {
  const entries = [{ id: "a1", name: "예산.xlsx", kind: "SHEET" }];
  const { document, userClick } = await openWindow("?module=DOC&win=w-clear-fail", {
    list_recent_documents: () => entries,
    clear_recent_documents: () => { throw new Error("denied"); },
  });
  userClick(document.getElementById("recent-dialog-open"));
  await flushAsyncTurns();
  const clearBtn = document.querySelector(".recent-documents-clear");
  userClick(clearBtn);
  await flushAsyncTurns();
  assert.ok(document.querySelector(".recent-documents-dialog"));
  assert.equal(clearBtn.disabled, false);
});

test("opening the recent-documents dialog with an empty list shows the toast instead of a silent no-op", async () => {
  const { document, host, userClick } = await openWindow("?module=DOC&win=w-recent-none");
  userClick(document.querySelector("[data-recent-dialog-open]"));
  await flushAsyncTurns();
  assert.equal(document.querySelector(".recent-documents-dialog"), null);
  const toast = document.getElementById("shell-toast");
  assert.equal(toast.hidden, false);
  assert.equal(toast.textContent, "최근 문서가 없습니다.");
});

test("the dialog entry in the recent submenu is hidden while the list is empty", async () => {
  const { document } = await openWindow("?module=DOC&win=w-recent-empty");
  assert.equal(document.getElementById("recent-dialog-open").hidden, true);
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

test("the shell's recents-changed signal reloads the recent lists without any argument (PD-127)", async () => {
  let list = [];
  const { document, host, shell } = await openWindow("?module=DOC&win=w-sync", {
    list_recent_documents: () => list,
  });
  assert.equal(typeof host.__EASYOFFICE_RECENTS_CHANGED__, "function");
  assert.equal(document.querySelectorAll("#recent-menu-list [data-recent-id]").length, 0);
  const before = shell.named("list_recent_documents").length;
  list = [{ id: "r9", name: "다른 창.docx", kind: "DOC" }];
  host.__EASYOFFICE_RECENTS_CHANGED__();
  await flushAsyncTurns();
  assert.ok(shell.named("list_recent_documents").length > before, "the list is asked from the shell again");
  assert.deepEqual(document.querySelectorAll("#recent-menu-list [data-recent-id]").map((b) => b.dataset.recentId), ["r9"]);
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
  userClick(document.querySelector("#home-screen [data-file-open]"));
  await flushAsyncTurns();
  assert.equal(shell.named("close_current_window").length, 1);
});

test("a cancelled open dialog keeps the start window", async () => {
  const { document, shell, userClick } = await openWindow("?module=START&win=main", {
    open_file_in_new_window: () => ({ status: "cancelled" }),
  });
  userClick(document.querySelector("#home-screen [data-file-open]"));
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
  assert.equal(document.getElementById("close-dialog-title").textContent, "저장 확인");
  assert.equal(document.getElementById("close-dialog-text").textContent, "제목 없음의 변경 내용을 저장할까요?", "the prompt names the document, as the reference app does");
  assert.equal(shell.named("ack_close_prompt").length, 1, "the shell hears that the prompt is shown");
  const secondWhileOpen = host.__EASYOFFICE_CLOSE_REQUEST__();
  await flushAsyncTurns();
  assert.equal(shell.named("ack_close_prompt").length, 2, "a repeated request joins the open prompt and confirms it again");
  userClick(document.getElementById("btn-close-cancel"));
  assert.equal(await secondWhileOpen, await first, "both requests get the one answer");
  assert.equal(dialog.open, false);
  assert.equal(shell.named("close_current_window").length, 0);
  assert.equal(shell.named("end_close_prompt").length, 1, "취소 ends the close prompt (and a quit from the menu)");

  const second = host.__EASYOFFICE_CLOSE_REQUEST__();
  await flushAsyncTurns();
  assert.equal(shell.named("ack_close_prompt").length, 3, "the next prompt is confirmed again");
  userClick(document.getElementById("btn-close-save"));
  await second;
  await flushAsyncTurns(10);
  assert.equal(shell.named("save_document_file").length, 1, "저장 runs the native save first");
  assert.equal(shell.named("close_current_window").length, 1);
  assert.equal(shell.named("end_close_prompt").length, 1, "a close that goes through does not end the prompt");

  typeIntoDocParagraph(document, "다시 편집");
  advanceTimers(3000);
  await flushAsyncTurns(10);
  const third = host.__EASYOFFICE_CLOSE_REQUEST__();
  await flushAsyncTurns();
  userClick(document.getElementById("btn-close-discard"));
  await third;
  assert.equal(shell.named("close_current_window").length, 2);
});

test("a save that throws keeps the window open and ends the close prompt", async () => {
  const { document, shell, host, userClick, advanceTimers } = await openWindow(
    "?module=DOC&win=w-throw-save",
    { save_document_file: () => { throw new Error("디스크 오류"); } },
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
  assert.equal(shell.named("end_close_prompt").length, 1);
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
  assert.equal(shell.named("end_close_prompt").length, 1, "a save that did not happen ends the close prompt");
});

test("a prompt the page opened itself is confirmed when the shell asks for a close", async () => {
  const { document, shell, host, userClick, advanceTimers } = await openWindow("?module=DOC&win=w-page-first", {}, { localStorage: createMemoryStorage() });
  typeIntoDocParagraph(document, "편집 내용");
  advanceTimers(3000);
  await flushAsyncTurns(10);
  const closeButton = document.querySelector("[data-window-close]");
  assert.ok(closeButton, "the page has its own close button");
  userClick(closeButton);
  await flushAsyncTurns();
  const dialog = document.getElementById("close-confirm-dialog");
  assert.equal(dialog.open, true);
  assert.equal(shell.named("ack_close_prompt").length, 1);
  // The shell asks as well (Cmd+Q, the window's close button): it has no record of this prompt yet.
  const fromShell = host.__EASYOFFICE_CLOSE_REQUEST__();
  await flushAsyncTurns();
  assert.equal(shell.named("ack_close_prompt").length, 2, "the shell's request is answered with a confirmation");
  assert.equal(dialog.open, true, "still one prompt");
  userClick(document.getElementById("btn-close-cancel"));
  await fromShell;
  assert.equal(shell.named("close_current_window").length, 0);
});

test("a window without unsaved changes closes without a prompt or a confirmation", async () => {
  const { shell, host } = await openWindow("?module=DOC&win=w-clean-close", {}, { localStorage: createMemoryStorage() });
  await host.__EASYOFFICE_CLOSE_REQUEST__();
  assert.equal(shell.named("close_current_window").length, 1);
  assert.equal(shell.named("ack_close_prompt").length, 0);
  assert.equal(shell.named("end_close_prompt").length, 0);
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
  assert.deepEqual(forwarded, ["Ctrl+Alt+O", "Ctrl+E", "Ctrl+K", "Ctrl+N", "Ctrl+O", "Ctrl+P", "Ctrl+Q", "Ctrl+S", "Ctrl+W"]);
  for (const b of bindings.filter((x) => ["Ctrl+N", "Ctrl+W", "Ctrl+Q"].includes(x.keys))) assert.equal(b.desktopOnly, true);
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

// The reference app binds PDF export to ⌘E through its macOS menu only and leaves Ctrl+E to the editor's
// delete on Windows and Linux (its tests pin that). The table says so with `macOnly`.
test("Ctrl+E exports the PDF on macOS only, in the Hangul module of the desktop shell", async () => {
  const entry = (await openWindow("?module=DOC&win=w-e")).host.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS.find((b) => b.keys === "Ctrl+E");
  assert.deepEqual(
    { action: entry.action, module: entry.module, desktopOnly: entry.desktopOnly, macOnly: entry.macOnly, forward: entry.forwardFromEditor },
    { action: "hwp-export-pdf", module: "HWP", desktopOnly: true, macOnly: true, forward: true },
  );
  for (const [platform, expected] of [["MacIntel", true], ["Win32", false], ["Linux x86_64", false]]) {
    const { host } = await openWindow("?module=HWP&win=w-e-" + platform.slice(0, 3), {}, { harness: { platform } });
    const session = host.__EASYOFFICE_CREATE_WORKSPACE_SESSION__({
      document: { getElementById: () => null, querySelectorAll: () => [], activeElement: null, addEventListener() {} },
      shortcutBindings: host.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS,
      getActiveModule: () => "HWP", getHwpHost: () => null, captureWorkspaceSnapshot: () => ({}),
      restoreWorkspaceSnapshot: async () => {}, runExclusive: (fn) => Promise.resolve().then(fn),
      getWorkspacePanels: () => null, getSheetController: () => null, isDesktopShell: () => true,
    });
    const ran = [];
    assert.equal(session.runForwardedShortcut("Ctrl+E", { "hwp-export-pdf": () => ran.push("pdf") }), expected, platform);
    assert.equal(session.isMacClient(), expected, platform);
    // The key list the editor frame is told to forward follows the same rule.
    const keys = session.forwardedKeys();
    assert.equal(keys.includes("Ctrl+E"), expected, platform);
    assert.ok(["Ctrl+P", "Ctrl+S", "Ctrl+Q"].every((k) => keys.includes(k)), platform);
    assert.ok(keys.length <= 16, "the editor takes at most 16 combos");
    assert.deepEqual(ran, expected ? ["pdf"] : [], platform);
  }
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

// Windows and Linux have no native 끝내기 item (PD-121): Ctrl+Q starts the shell's quit queue, which closes
// the windows one at a time through their own close prompts. The reference app's page menu has no such item.
test("Ctrl+Q starts the shell's quit queue, and the page menu has no 끝내기 item", async () => {
  const { shell, sandbox, document } = await openWindow("?module=DOC&win=w-quit", {}, { harness: { platform: "Win32" } });
  assert.equal(document.querySelector("[data-app-quit]"), null, "no item the reference app's menu lacks");
  sandbox.dispatchWindowKeydown({ key: "q", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(shell.named("quit_app").length, 1);
  assert.equal(shell.named("close_current_window").length, 0, "the queue, not this window's own close");
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

test("HWP save plumbing: a plain save overwrites the opened file, the close prompt's save marks the window as closing", async () => {
  const { host, document } = await openWindow("?module=HWP&win=w-save-plumbing");
  const saves = [];
  const tools = host.__EASYOFFICE_CREATE_WORKSPACE_TOOLS__({
    document, URL, invokeIpc: async () => ({}), getActiveModule: () => "HWP",
    getHwpHost: () => ({ isEmbedActive: () => true }),
    getHwpSession: () => ({ save: async (options) => { saves.push(options); return { ok: false, message: "x" }; } }),
    sheetController: { commitPendingFocusedSheetCell: async () => {} },
    enqueueWorkspaceHistoryOp: (op) => op(), getWindowBridge: () => null,
  }, {});
  await tools.saveActiveModuleDocument();
  await tools.saveActiveModuleDocument({ closing: true });
  assert.deepEqual(saves.map((o) => [o.inPlace, o.closing]), [[true, false], [true, true]]);
  const app = fs.readFileSync(new URL("../crates/easyoffice-app/ui/app.js", import.meta.url), "utf8");
  assert.match(app, /isDirty: \(\) => workspaceSession\.isDirty\(\),\s+save: \(\) => saveWorkspaceDocument\(\{ closing: true \}\),/, "the close prompt saves with closing");
  assert.match(app, /openRecent: \(id\) => windowBridge\.openRecent\(id\), save: \(\) => saveWorkspaceDocument\(\),/, "the 파일 menu saves in place");
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

test("the about dialog shows the app version from the bootstrap and ignores a malformed one", async () => {
  const { document } = await openWindow("?module=DOC&win=w-about");
  const version = document.getElementById("about-version");
  assert.equal(version.hidden, false);
  assert.equal(version.textContent, "EasyOffice 1.2.3");
  const api = document.defaultView.__EASYOFFICE_CREATE_SHELL_CHROME__({ document, hwpHost: { setShellContext: async () => null, onCommand() {} } });
  api.setAppVersion("<b>x</b>");
  assert.equal(version.textContent, "EasyOffice 1.2.3", "a version with markup is dropped");
});

test("home screen and dialog files carry their the reference app source header and are listed in the notices", async () => {
  const fs = await import("node:fs");
  const read = (rel) => fs.readFileSync(new URL(`../crates/easyoffice-app/ui/${rel}`, import.meta.url), "utf8");
  const notices = fs.readFileSync(new URL("../THIRD_PARTY_NOTICES.md", import.meta.url), "utf8");
  for (const [file, source] of [
    ["home-screen.js", "apps/studio-host/src/ui/home-screen.ts"],
    ["recent-documents-dialog.js", "apps/studio-host/src/ui/recent-documents-dialog.ts"],
    ["home.css", "apps/studio-host/src/styles/home-screen.css"],
  ]) {
    assert.match(read(file), /Adapted from the reference app/, `${file} names the reference app`);
    assert.ok(read(file).includes("Edward Kim"), `${file} carries the copyright`);
    assert.ok(notices.includes(`ui/${file}`) && notices.includes(source), `${file} is in the notices table`);
  }
});

// ---- --smoke steps ---------------------------------------------------------------------------------

function smokeBridge(role, shellOverrides = {}) {
  const env = { location: { search: "?module=HWP&win=w-hwp", protocol: "tauri:" }, __EASYOFFICE_SMOKE__: role };
  loadUiModule(WINDOW_BRIDGE_JS, env);
  const shellCalls = [];
  const bridge = env.__EASYOFFICE_WINDOW_BRIDGE__.createWindowBridge({
    document: { title: "", querySelectorAll: () => [], getElementById: () => null },
    env,
    desktopBridge: {
      isAvailable: () => true,
      windowCommand: async (key, args) => {
        shellCalls.push({ key, args });
        if (shellOverrides[key]) throw shellOverrides[key];
        return null;
      },
    },
  });
  const reports = [];
  const invokeIpc = async (command, payload) => { reports.push({ command, payload }); };
  return { bridge, reports, shellCalls, invokeIpc };
}

test("smoke: the first window reports each step in order and then opens the second window", async () => {
  const t = smokeBridge(true);
  const ran = [];
  const steps = {
    hwp: async () => { ran.push("hwp"); return "bytes=512"; },
    pdf: async () => { ran.push("pdf"); return 7; },
    print: async () => { ran.push("print"); return "pages=1 rules=2"; },
  };
  await t.bridge.runSmoke({ invokeIpc: t.invokeIpc, steps });
  assert.deepEqual(ran, ["hwp", "pdf", "print"]);
  assert.deepEqual(t.reports, [
    { command: "scrub_pii", payload: "EASYOFFICE_SMOKE_STEP_OK hwp bytes=512" },
    { command: "scrub_pii", payload: "EASYOFFICE_SMOKE_STEP_OK pdf 7" },
    { command: "scrub_pii", payload: "EASYOFFICE_SMOKE_STEP_OK print pages=1 rules=2" },
  ]);
  assert.deepEqual(t.shellCalls, [{ key: "openDocumentWindow", args: { kind: "DOC" } }]);
});

test("smoke: the first failing step is reported, stops the run and opens no second window", async () => {
  const t = smokeBridge(true);
  const ran = [];
  const steps = {
    hwp: async () => "ok",
    pdf: async () => { throw new Error("쪽 수(0)가 범위를 벗어납니다"); },
    print: async () => { ran.push("print"); return "never"; },
  };
  await t.bridge.runSmoke({ invokeIpc: t.invokeIpc, steps });
  assert.deepEqual(t.reports.map((r) => r.payload), [
    "EASYOFFICE_SMOKE_STEP_OK hwp ok",
    "EASYOFFICE_SMOKE_STEP_FAIL pdf 쪽 수(0)가 범위를 벗어납니다",
  ]);
  assert.deepEqual(ran, []);
  assert.deepEqual(t.shellCalls, []);
  const text = smokeBridge(true);
  await text.bridge.runSmoke({ invokeIpc: text.invokeIpc, steps: { hwp: () => Promise.reject("plain text") } });
  assert.equal(text.reports[0].payload, "EASYOFFICE_SMOKE_STEP_FAIL hwp plain text");
});

test("smoke: a second window that cannot be opened fails the window_loaded step", async () => {
  const t = smokeBridge(true, { openDocumentWindow: new Error("창 만들기 실패") });
  await t.bridge.runSmoke({ invokeIpc: t.invokeIpc, steps: {} });
  assert.deepEqual(t.reports.map((r) => r.payload), ["EASYOFFICE_SMOKE_STEP_FAIL window_loaded 두 번째 창: 창 만들기 실패"]);
});

test("smoke: the second window reports that it loaded and closes itself; a normal run does nothing", async () => {
  const second = smokeBridge("secondary");
  await second.bridge.runSmoke({ invokeIpc: second.invokeIpc, steps: { hwp: () => assert.fail("a second window runs no steps") } });
  assert.deepEqual(second.reports.map((r) => r.payload), ["EASYOFFICE_SMOKE_STEP_OK window_loaded HWP"]);
  assert.deepEqual(second.shellCalls.map((c) => c.key), ["closeCurrentWindow"]);
  const normal = smokeBridge(undefined);
  await normal.bridge.runSmoke({ invokeIpc: normal.invokeIpc, steps: { hwp: () => assert.fail("not a smoke run") } });
  assert.deepEqual([normal.reports, normal.shellCalls], [[], []]);
});

test("smoke: the page wiring keeps the step order and app.js stays under the line ceiling", () => {
  const app = fs.readFileSync(new URL("../crates/easyoffice-app/ui/app.js", import.meta.url), "utf8");
  const wiring = /const smokeSteps = \{ (.*) \};/.exec(app);
  assert.ok(wiring, "app.js defines the smoke steps");
  assert.deepEqual([...wiring[1].matchAll(/(\w+): \(\) =>/g)].map((m) => m[1]), ["hwp", "pdf", "print", "frame_ipc"]);
  assert.ok(app.split("\n").length < 1000);
});

test("the shell's notice (open failures, the drop hint) shows as the shared toast in every window", async () => {
  for (const search of ["?module=START&win=w-n1", "?module=DOC&win=w-n2"]) {
    const { document, host } = await openWindow(search, {}, { localStorage: createMemoryStorage() });
    assert.equal(typeof host.__EASYOFFICE_NOTICE__, "function");
    const toast = document.getElementById("shell-toast");
    assert.equal(toast.hidden, true);
    host.__EASYOFFICE_NOTICE__("지원하지 않는 형식입니다 (a.exe)");
    assert.equal(toast.hidden, false, search);
    assert.equal(toast.textContent, "지원하지 않는 형식입니다 (a.exe)");
  }
});
