import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, typeIntoDocParagraph, createMemoryStorage } from "./test-support/modules.mjs";

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const INDEX_HTML = fs.readFileSync(fileURLToPath(new URL("index.html", UI_DIR)), "utf8");
const STYLES_CSS = fs.readFileSync(fileURLToPath(new URL("styles.css", UI_DIR)), "utf8");
const AUTOSAVE_KEY = "easyoffice.workspace.autosave.v1";

// Shortcuts a web page cannot (or must not) take over from the browser: new window/tab,
// close tab, reload, devtools, history, address bar, downloads, print, caret browsing.
// Ctrl+O is intentionally absent: file open (PD-075) owns it and the page may preventDefault it.
// Plain F5 (reload) is absent as well: PD-080 binds it only in the slide module outside text fields,
// and that exception is pinned by SLIDE_SCOPED_BROWSER_KEYS below.
const BROWSER_RESERVED_SHORTCUTS = new Set([
  "Ctrl+N", "Ctrl+T", "Ctrl+W", "Ctrl+R", "Ctrl+L", "Ctrl+D", "Ctrl+H", "Ctrl+J", "Ctrl+P",
  "Ctrl+E", "Ctrl+Tab", "Ctrl+F4", "Ctrl+F5", "Ctrl+Shift+N", "Ctrl+Shift+T",
  "Ctrl+Shift+W", "Ctrl+Shift+R", "Ctrl+Shift+J", "Ctrl+Shift+C", "Ctrl+Shift+I",
  "Ctrl+Shift+B", "Ctrl+Shift+Delete", "F6", "F7", "F11", "F12", "Alt+F", "Alt+D",
  "Alt+E", "Alt+F4", "Alt+Home", "Alt+Left", "Alt+Right",
]);

const SLIDE_SCOPED_BROWSER_KEYS = new Set(["F5"]);

function createFakeClipboard(initial = "") {
  const clip = {
    text: initial,
    writes: [],
    async writeText(t) {
      clip.text = String(t);
      clip.writes.push(clip.text);
    },
    async readText() {
      return clip.text;
    },
  };
  return clip;
}

function createSelectionHolder() {
  return {
    rangeCount: 0,
    anchorNode: null,
    isCollapsed: true,
    _range: null,
    getRangeAt() {
      return this._range;
    },
    removeAllRanges() {
      this.rangeCount = 0;
      this.anchorNode = null;
      this.isCollapsed = true;
      this._range = null;
    },
  };
}

function selectTextRange(selection, document, textNode, start, end) {
  const state = { start, end };
  const range = {
    commonAncestorContainer: textNode,
    startContainer: textNode,
    get collapsed() {
      return state.start === state.end;
    },
    toString: () => textNode.data.slice(state.start, state.end),
    cloneRange() {
      return range;
    },
    deleteContents() {
      textNode.data = textNode.data.slice(0, state.start) + textNode.data.slice(state.end);
      state.end = state.start;
    },
    insertNode(node) {
      const parent = textNode.parentElement;
      const tail = document.createTextNode(textNode.data.slice(state.start));
      textNode.data = textNode.data.slice(0, state.start);
      const idx = parent.childNodes.indexOf(textNode);
      parent.childNodes.splice(idx + 1, 0, node, tail);
      node.parentElement = parent;
      tail.parentElement = parent;
    },
  };
  selection._range = range;
  selection.rangeCount = 1;
  selection.anchorNode = textNode;
  selection.isCollapsed = start === end;
  return range;
}

function firstTextNode(el) {
  for (const child of el.childNodes) {
    if (child.nodeType === 3 && child.data.trim()) return child;
    if (child.nodeType === 1) {
      const nested = firstTextNode(child);
      if (nested) return nested;
    }
  }
  return null;
}

function normalizeShortcutLabel(catalog, raw) {
  const formatted = catalog.formatPlatformShortcut(String(raw).trim(), "Ctrl");
  if (formatted === "/" || formatted.includes(",")) return formatted;
  const parts = formatted.split("+");
  const key = parts.pop();
  const order = ["Ctrl", "Alt", "Shift"];
  parts.sort((a, b) => order.indexOf(a) - order.indexOf(b));
  return [...parts, key].join("+");
}

function collectIndexHtmlShortcutLabels() {
  const labels = [];
  for (const m of INDEX_HTML.matchAll(/<span class="md-shortcut">([^<]*)<\/span>/g)) {
    labels.push({ where: "menu", raw: m[1] });
  }
  const looksLikeShortcut = /(Ctrl|Alt|Shift|Cmd|⌘|⌥|⇧|⌃|^F\d+$|^Insert$|^[A-Z/]$)/;
  for (const m of INDEX_HTML.matchAll(/title="[^"]*\(([^()"]*)\)"/g)) {
    for (const part of m[1].split(" / ")) {
      if (looksLikeShortcut.test(part.trim())) labels.push({ where: "tooltip", raw: part });
    }
  }
  return labels;
}

test("index.html menu and tooltip shortcut labels map 1:1 onto registered handlers and never advertise browser-reserved keys", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const catalog = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  assert.ok(Array.isArray(catalog.WORKSPACE_SHORTCUT_BINDINGS), "shortcut registry must be exported");
  const bound = new Set(catalog.WORKSPACE_SHORTCUT_BINDINGS.map((b) => b.keys));
  for (const b of catalog.WORKSPACE_SHORTCUT_BINDINGS) {
    // Only the desktop shell (which owns its windows) binds new/close window; see PD-088.
    assert.ok(!BROWSER_RESERVED_SHORTCUTS.has(b.keys) || b.desktopOnly === true, "registry must not bind reserved key " + b.keys);
    if (SLIDE_SCOPED_BROWSER_KEYS.has(b.keys)) {
      assert.equal(b.module, "SLIDE", b.keys + " may only be taken over inside the slide module");
      assert.equal(b.bareKey, true, b.keys + " must never fire from a text field");
    }
  }
  const labels = collectIndexHtmlShortcutLabels();
  assert.ok(labels.length >= 10);
  for (const { where, raw } of labels) {
    const norm = normalizeShortcutLabel(catalog, raw);
    assert.ok(!norm.includes(","), `${where} label "${raw}" advertises an unsupported chord sequence`);
    const desktopOnly = catalog.WORKSPACE_SHORTCUT_BINDINGS.some((b) => b.keys === norm && b.desktopOnly === true);
    assert.ok(!BROWSER_RESERVED_SHORTCUTS.has(norm) || desktopOnly, `${where} label "${raw}" is browser-reserved`);
    assert.ok(bound.has(norm), `${where} label "${raw}" (${norm}) has no registered handler`);
  }
});

test("ribbon tool strip and command palette only display shortcuts that the keydown handler really binds", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  const catalog = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const bound = new Set(catalog.WORKSPACE_SHORTCUT_BINDINGS.map((b) => b.keys));
  for (const mod of ["DOC", "SHEET", "SLIDE", "PDF"]) {
    switchModule(document, mod);
    await flushAsyncTurns();
    for (const tab of document.querySelectorAll("#ribbon-tabs .ribbon-btn")) {
      userClick(tab);
      await flushAsyncTurns();
      for (const small of document.querySelectorAll("#ribbon-tool-strip small")) {
        const label = small.textContent.replace(/^\(|\)$/g, "");
        assert.ok(bound.has(normalizeShortcutLabel(catalog, label)), `${mod} ribbon label ${label} is unbound`);
      }
    }
  }
  userClick(document.getElementById("btn-command-palette"));
  const input = document.getElementById("cmd-search-input");
  for (const q of ["", "pdf", "시트", "검토", "성능"]) {
    input.value = q;
    input.dispatchEvent({ type: "input" });
    for (const sc of document.querySelectorAll("#cmd-palette-list .cp-item-shortcut")) {
      assert.ok(bound.has(normalizeShortcutLabel(catalog, sc.textContent)), `palette label ${sc.textContent} is unbound`);
    }
  }
});

test("Ctrl+F opens the palette, '/' only fires outside text entry, and reserved browser keys are left untouched", async () => {
  const { document, sandbox } = await createWorkspaceHarness();
  const modal = document.getElementById("cmd-palette-modal");

  const ctrlF = sandbox.dispatchWindowKeydown({ key: "f", ctrlKey: true });
  assert.equal(ctrlF.defaultPrevented, true);
  assert.equal(modal.open, true);
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  assert.equal(modal.open, false);

  const docEditor = document.getElementById("doc-editor");
  docEditor.focus();
  const slashTyping = sandbox.dispatchWindowKeydown({ key: "/" });
  assert.equal(slashTyping.defaultPrevented, false);
  assert.equal(modal.open, false);
  const mTyping = sandbox.dispatchWindowKeydown({ key: "m" });
  assert.equal(mTyping.defaultPrevented, false);
  docEditor.blur();

  const slashIdle = sandbox.dispatchWindowKeydown({ key: "/" });
  assert.equal(slashIdle.defaultPrevented, true);
  assert.equal(modal.open, true);
  sandbox.dispatchWindowKeydown({ key: "Escape" });

  for (const combo of [
    { key: "R", ctrlKey: true, shiftKey: true },
    { key: "n", ctrlKey: true },
    { key: "t", ctrlKey: true },
    { key: "w", ctrlKey: true },
    { key: "J", ctrlKey: true, shiftKey: true },
  ]) {
    const ev = sandbox.dispatchWindowKeydown(combo);
    assert.equal(ev.defaultPrevented, false, "must not hijack " + JSON.stringify(combo));
  }

  const themeEv = sandbox.dispatchWindowKeydown({ key: "d", ctrlKey: true, altKey: true });
  await flushAsyncTurns();
  assert.equal(themeEv.defaultPrevented, true);
  assert.equal(document.documentElement.dataset.themeEffective, "dark");
});

test("workspace edits autosave to localStorage and are restored on the next load", async () => {
  const storage = createMemoryStorage();
  const first = await createWorkspaceHarness({ localStorage: storage });
  typeIntoDocParagraph(first.document, "자동 저장 복원 검증 문단");
  switchModule(first.document, "SHEET");
  await flushAsyncTurns();
  const b2 = first.document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "2000";
  b2.dispatchEvent({ type: "input", target: b2 });
  b2.dispatchEvent({ type: "keydown", key: "Enter", target: b2 });
  await flushAsyncTurns();
  b2.blur();
  await flushAsyncTurns();
  first.advanceTimers(5000);
  await flushAsyncTurns(10);

  const raw = storage.getItem(AUTOSAVE_KEY);
  assert.ok(raw, "autosave record must be written");
  const record = JSON.parse(raw);
  assert.equal(record.version, 1);
  assert.equal(record.dirty, true);
  assert.match(JSON.stringify(record.snapshot.docTabs), /자동 저장 복원 검증 문단/);

  const second = await createWorkspaceHarness({ localStorage: storage });
  await flushAsyncTurns(10);
  assert.match(second.document.getElementById("status-command-feedback").textContent, /복원/);
  assert.equal(second.document.getElementById("pane-sheet").hidden, false);
  assert.equal(
    second.document.querySelector('#pane-sheet td[data-cell="B2"]').textContent,
    "2000"
  );
  switchModule(second.document, "DOC");
  await flushAsyncTurns();
  assert.equal(
    second.document.getElementById("doc-editor").querySelector("p").textContent,
    "자동 저장 복원 검증 문단"
  );
});

test("unsafe or corrupted autosave records are ignored instead of being injected into the editor", async () => {
  const unsafe = {
    version: 1,
    dirty: true,
    snapshot: {
      activeModule: "DOC",
      docTabs: {
        DOC: { editorNodes: [{ t: "el", tag: "script", attrs: { src: "/x.js" }, data: {}, children: [] }], layoutStyle: {} },
        HWP: null,
      },
      sheets: {},
      sheetRowCounts: {},
      slides: [],
    },
  };
  for (const seed of [JSON.stringify(unsafe), "{not json"]) {
    const storage = createMemoryStorage({ [AUTOSAVE_KEY]: seed });
    const { document } = await createWorkspaceHarness({ localStorage: storage });
    await flushAsyncTurns(10);
    const editor = document.getElementById("doc-editor");
    assert.equal(editor.querySelectorAll("script").length, 0);
    assert.ok(editor.querySelector("h1"), "default document must remain");
  }
});

test("beforeunload guard is registered only while the workspace is dirty", async () => {
  const storage = createMemoryStorage();
  const { document, sandbox, userClick, advanceTimers } = await createWorkspaceHarness({ localStorage: storage });
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0);

  for (const mod of ["HWP", "SHEET", "SLIDE", "PDF", "CONV", "DOC"]) {
    switchModule(document, mod);
    await flushAsyncTurns();
    if (mod === "SHEET") {
      userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]'));
      await flushAsyncTurns();
      userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
      await flushAsyncTurns();
    }
    if (mod === "SLIDE") {
      userClick(document.querySelector('.slide-thumb[data-slide-idx="1"]'));
      await flushAsyncTurns();
    }
  }
  advanceTimers(5000);
  await flushAsyncTurns(10);
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0, "switching views must not mark dirty");

  typeIntoDocParagraph(document, "이탈 경고 확인용 수정");
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 1);
  advanceTimers(5000);
  await flushAsyncTurns(10);
  const ev = sandbox.dispatchWindowEvent("beforeunload");
  assert.equal(ev.defaultPrevented, true);
  assert.equal(ev.returnValue, "");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  advanceTimers(5000);
  await flushAsyncTurns(10);
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0, "undo back to baseline is clean");

  typeIntoDocParagraph(document, "저장 후에는 깨끗한 상태");
  advanceTimers(5000);
  await flushAsyncTurns(10);
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 1);
  userClick(document.getElementById("qat-save"));
  await flushAsyncTurns(10);
  advanceTimers(5000);
  await flushAsyncTurns(10);
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0, "explicit save clears dirty state");
});

test("document toolbars and menus are disabled outside DOC/HWP and the status bar drops other modules' text", async () => {
  const { document, userClick } = await createWorkspaceHarness();
  const styleBar = document.getElementById("doc-style-bar");
  const paraNumBtn = document.querySelector('#icon-toolbar [data-studio-cmd="para-numbering"]');
  const cutBtn = document.querySelector('#icon-toolbar [data-studio-cmd="clip-cut"]');
  const saveBtn = document.getElementById("qat-save");
  const paraMenu = document.querySelector('.md-item[data-studio-menu-cmd="para-numbering"]');
  const ctrlMarks = document.getElementById("btn-toggle-ctrl-marks");
  const sheetAgg = document.getElementById("status-sheet-agg");
  const docStatus = document.querySelector(".statusbar-left-group");

  assert.equal(styleBar.hidden, false);
  assert.equal(paraNumBtn.disabled, false);
  assert.equal(cutBtn.disabled, false);
  assert.equal(sheetAgg.hidden, true);

  switchModule(document, "SHEET");
  await flushAsyncTurns();
  assert.equal(styleBar.hidden, true);
  assert.equal(paraNumBtn.disabled, true);
  assert.equal(cutBtn.disabled, false, "cut/paste act on the selected cell range in SHEET");
  assert.equal(ctrlMarks.disabled, true);
  assert.equal(saveBtn.disabled, false);
  assert.equal(paraMenu.getAttribute("aria-disabled"), "true");
  assert.equal(sheetAgg.hidden, false);
  assert.equal(docStatus.hidden, true);

  const fb = document.getElementById("status-command-feedback");
  const before = fb.textContent;
  userClick(paraMenu);
  await flushAsyncTurns();
  assert.equal(fb.textContent, before);

  switchModule(document, "SLIDE");
  await flushAsyncTurns();
  assert.equal(sheetAgg.hidden, true, "sheet aggregate must not leak into SLIDE");

  switchModule(document, "HWP");
  await flushAsyncTurns();
  assert.equal(styleBar.hidden, false);
  assert.equal(paraNumBtn.disabled, false);
  assert.equal(paraMenu.getAttribute("aria-disabled"), "false");
  assert.equal(docStatus.hidden, false);
});

test("a window has no tab strip or module switcher; the File menu opens windows instead of switching", async () => {
  const { document } = await createWorkspaceHarness();
  assert.equal(document.getElementById("tab-strip"), null);
  assert.equal(document.querySelector(".module-switcher"), null);
  assert.doesNotMatch(STYLES_CSS, /\.tab-strip|\.tab-pill|\.mod-btn/, "no tab or module-button styling remains");
  assert.match(STYLES_CSS, /\.eo-doc-style-bar\[hidden\]/);
  assert.equal(document.querySelectorAll('.md-item[data-studio-menu-cmd^="module:"]').length, 0);
  const labels = document.querySelectorAll('.menu-item[data-menu="file"] .md-label').map((l) => l.textContent.trim());
  assert.ok(labels.includes("새 문서") && labels.includes("열기…") && labels.includes("닫기"), labels.join("|"));
});

test("Edit menu cut/copy/paste act on the real selection and the system clipboard", async () => {
  const selection = createSelectionHolder();
  const clipboard = createFakeClipboard();
  const { document, sandbox, userClick } = await createWorkspaceHarness({ selection, clipboard });
  const docEditor = document.getElementById("doc-editor");
  const para = docEditor.querySelector("p");
  const textNode = firstTextNode(para);
  const original = textNode.data;
  const fb = document.getElementById("status-command-feedback");

  userClick(document.querySelector('.md-item[data-studio-menu-cmd="clip-cut"]'));
  await flushAsyncTurns();
  assert.equal(para.textContent, original, "cut without a selection must not erase the paragraph");
  assert.match(fb.textContent, /선택/);

  selectTextRange(selection, document, textNode, 2, 6);
  const picked = original.slice(2, 6);
  userClick(document.querySelector('.md-item[data-studio-menu-cmd="clip-copy"]'));
  await flushAsyncTurns();
  assert.equal(clipboard.text, picked);
  assert.equal(para.textContent, original);

  userClick(document.querySelector('.md-item[data-studio-menu-cmd="clip-cut"]'));
  await flushAsyncTurns();
  assert.equal(clipboard.text, picked);
  assert.equal(para.textContent, original.slice(0, 2) + original.slice(6));

  clipboard.text = "[붙임]";
  selectTextRange(selection, document, textNode, 0, 0);
  userClick(document.querySelector('.md-item[data-studio-menu-cmd="clip-paste"]'));
  await flushAsyncTurns();
  assert.equal(para.textContent, "[붙임]" + original.slice(0, 2) + original.slice(6));

  selection.removeAllRanges();
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(docEditor.querySelector("p").textContent, original);
});

test("bootstrap IPC failure shows an error with retry and never claims macOS on Windows", async () => {
  let failBootstrap = true;
  const { document, sandbox, userClick } = await createWorkspaceHarness({
    platform: "Win32",
    testIpc: async (command) => {
      if (command === "get_workspace_bootstrap") {
        if (failBootstrap) throw new Error("서버 연결 거부");
        return {
          platform: "Windows",
          shortcutModifier: "Ctrl",
          recentFormatFilters: ["전체"],
          openTabs: [{ id: "tab-doc", title: "보고서.docx", module: "DOC", isPinned: true }],
        };
      }
      if (command === "get_ribbon_category_commands") {
        return { commands: [{ id: "document-home-bold", title: "굵게", shortcut: "Cmd+B" }] };
      }
      return { status: "ok" };
    },
  });
  await flushAsyncTurns();
  const banner = document.getElementById("bootstrap-error");
  assert.ok(banner, "bootstrap error banner must exist");
  assert.equal(banner.hidden, false);
  assert.match(document.getElementById("bootstrap-error-text").textContent, /서버 연결 거부/);
  assert.doesNotMatch(document.getElementById("platform-pill").textContent, /macOS|⌘/);
  assert.equal(sandbox.__EASYOFFICE_RIBBON_CATALOG__ !== undefined, true);

  failBootstrap = false;
  userClick(document.getElementById("btn-bootstrap-retry"));
  await flushAsyncTurns(10);
  assert.equal(banner.hidden, true);
  assert.equal(document.title, "보고서.docx — EasyOffice".replace("보고서.docx", document.defaultView.__EASYOFFICE_WINDOW_BRIDGE__.instance.getDocumentName("DOC")));
  assert.equal(document.defaultView.__EASYOFFICE_WINDOW_BRIDGE__.instance.getDocumentName("DOC"), "보고서.docx");
  assert.ok(document.querySelector("#ribbon-tool-strip .ribbon-tool-btn"));
  assert.equal(document.getElementById("platform-pill").textContent, "Windows · Ctrl");
});
