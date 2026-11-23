import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { stripTypeScriptTypes } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { openDemoWorkbook } from "./test-support/demo-fixtures.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { readText } from "./test-support/read-text.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
const SESSION_JS = new URL("../crates/easyoffice-app/ui/workspace-session.js", import.meta.url);
const SHEET_INPUT_JS = new URL("../crates/easyoffice-app/ui/sheet-input.js", import.meta.url);
const SHEET_POINT_JS = new URL("../crates/easyoffice-app/ui/sheet-point-ui.js", import.meta.url);

const ev = (init = {}) => ({
  key: "",
  code: "",
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  isComposing: false,
  ...init,
});

test("workspace-session latinKeyOf and shortcutComboFromEvent normalize Korean 2-beolsik and non-composing Process/Unidentified", () => {
  const env = loadUiModule(SESSION_JS, { navigator: { platform: "Win32" } });
  const { latinKeyOf, shortcutComboFromEvent } = env.__EASYOFFICE_WORKSPACE_SESSION_UTILS__;
  assert.equal(typeof latinKeyOf, "function");

  for (const [init, wantKey, wantCombo] of [
    [{ key: "ㄴ", code: "KeyS", ctrlKey: true }, "S", "Ctrl+S"],
    [{ key: "ㅋ", code: "KeyZ", ctrlKey: true, shiftKey: true }, "Z", "Ctrl+Shift+Z"],
    [{ key: "ㅠ", code: "KeyB", ctrlKey: true }, "B", "Ctrl+B"],
    [{ key: "ㅁ", code: "KeyA", ctrlKey: true }, "A", "Ctrl+A"],
    [{ key: "ㅐ", code: "KeyO", ctrlKey: true }, "O", "Ctrl+O"],
    [{ key: "ㅜ", code: "KeyN", altKey: true }, "N", "Alt+N"],
    [{ key: "Process", code: "KeyZ", ctrlKey: true, isComposing: false }, "Z", "Ctrl+Z"],
    [{ key: "Unidentified", code: "KeyS", metaKey: true, isComposing: false }, "S", "Ctrl+S"],
    [{ key: "k", code: "KeyV", ctrlKey: true }, "K", "Ctrl+K"],
    [{ key: "F2", code: "F2", ctrlKey: true }, "", "Ctrl+F2"],
    [{ key: "Enter", code: "Enter", ctrlKey: true }, "", "Ctrl+Enter"],
    [{ key: "/", code: "Slash", ctrlKey: true }, "/", "Ctrl+/"],
  ]) {
    const e = ev(init);
    assert.equal(latinKeyOf(e), wantKey, JSON.stringify(init));
    assert.equal(shortcutComboFromEvent(e), wantCombo, JSON.stringify(init));
  }

  for (const composing of [
    { key: "ㄴ", code: "KeyS", ctrlKey: true, isComposing: true },
    { key: "Process", code: "KeyZ", ctrlKey: true, isComposing: true },
  ]) {
    const e = ev(composing);
    assert.equal(latinKeyOf(e), "");
    assert.equal(shortcutComboFromEvent(e), "");
  }

  assert.equal(shortcutComboFromEvent(ev({ key: "Process", code: "", ctrlKey: true })), "");
  assert.equal(latinKeyOf(ev({ key: "ㅁ", code: "KeyA" })), "ㅁ", "unmodified Jamo stays Jamo");
  assert.equal(shortcutComboFromEvent(ev({ key: "ㅁ", code: "KeyA" })), "ㅁ");

  const altGr = ev({
    getModifierState: (m) => m === "AltGraph",
    ctrlKey: true,
    altKey: true,
    code: "KeyQ",
    key: "@",
  });
  assert.equal(latinKeyOf(altGr), "@");
  assert.equal(shortcutComboFromEvent(altGr), "Ctrl+Alt+@");

  const altGrNotMeta = ev({
    getModifierState: (m) => m === "AltGraph",
    metaKey: true,
    altKey: true,
    code: "KeyO",
    key: "ø",
  });
  assert.equal(latinKeyOf(altGrNotMeta), "O");
  assert.equal(shortcutComboFromEvent(altGrNotMeta), "Ctrl+Alt+O");
});

test("workspace-session handleShortcutKeydown ignores active composition even on a matching key", () => {
  const doc = {
    body: { addEventListener() {} },
    documentElement: {},
    activeElement: null,
    getElementById: () => null,
    querySelectorAll: () => [],
  };
  const env = loadUiModule(SESSION_JS, { navigator: { platform: "Win32" }, document: doc });
  const session = env.__EASYOFFICE_CREATE_WORKSPACE_SESSION__({
    document: doc,
    shortcutBindings: [{ keys: "Ctrl+S", action: "studio-save" }],
    getActiveModule: () => "DOC",
    getRibbonModulePrefix: () => "document",
    setFeedback() {},
    noteActivity() {},
    getWorkspacePanels: () => ({}),
    resolveActiveRange: () => null,
    isDesktopShell: () => true,
  });
  let saved = 0;
  let prevented = 0;
  const actions = { "studio-save": () => { saved += 1; } };
  const composingEv = { ...ev({ key: "s", code: "KeyS", ctrlKey: true, isComposing: true }), preventDefault() { prevented += 1; } };
  assert.equal(session.handleShortcutKeydown(composingEv, actions), false);
  assert.equal(saved, 0);
  assert.equal(prevented, 0);

  const koreanEv = { ...ev({ key: "ㄴ", code: "KeyS", ctrlKey: true, isComposing: false }), preventDefault() { prevented += 1; } };
  assert.equal(session.handleShortcutKeydown(koreanEv, actions), true);
  assert.equal(saved, 1);
  assert.equal(prevented, 1);
});

async function openSheet() {
  const h = await createWorkspaceHarness();
  await openDemoWorkbook(h.document);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  return h;
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });

test("host sheet-input handles Ctrl+ㅁ (KeyA), Ctrl+Alt+ㅍ (KeyV), non-composing Ctrl+Process, and rejects active composition", async () => {
  const { document } = await openSheet();
  const b2 = cell(document, "B2");
  b2.focus();
  const inRange = () => [...document.querySelectorAll("#pane-sheet td.in-range")].map((t) => t.dataset.cell);

  const selPrevented = !key(b2, "ㅁ", { code: "KeyA", ctrlKey: true, isComposing: false });
  assert.equal(selPrevented, true);
  assert.ok(cell(document, "A1").classList.contains("in-range"), "Ctrl+ㅁ selects all cells");
  assert.ok(inRange().length >= 8 * 8, "Ctrl+ㅁ selects the used data region");

  key(b2, "ArrowDown");
  assert.equal(inRange().length, 0);
  const processPrevented = !key(b2, "Process", { code: "KeyA", ctrlKey: true, isComposing: false });
  assert.equal(processPrevented, true, "non-composing Ctrl+Process is not swallowed as cell edit typing");
  assert.equal(b2.classList.contains("editing"), false);
  assert.ok(inRange().length >= 8 * 8);

  key(b2, "ArrowDown");
  assert.equal(inRange().length, 0);
  key(b2, "Process", { code: "KeyA", ctrlKey: true, isComposing: true });
  assert.equal(inRange().length, 0, "active composition does not select all");

  const a1 = cell(document, "A1");
  a1.focus();
  const pastePrevented = !key(a1, "ㅍ", { code: "KeyV", ctrlKey: true, altKey: true, isComposing: false });
  assert.equal(pastePrevented, true);
  assert.equal(document.getElementById("sheet-paste-dialog").open, true, "Ctrl+Alt+ㅍ opens paste special dialog");
  document.getElementById("sheet-paste-dialog").close();
});

test("host sheet-input fallback handles keyCode 229 with modifiers without workspace-session utils", () => {
  const vpListeners = {};
  const td = {
    tagName: "TD",
    dataset: { cell: "B2" },
    classList: { add() {}, remove() {}, contains: () => false },
    closest: (sel) => (sel === "td[data-cell]" ? td : null),
  };
  const pane = { contains: (el) => el === td, querySelectorAll: () => [] };
  const vp = { addEventListener: (type, fn) => { vpListeners[type] = fn; }, clientWidth: 600 };
  let startedEdit = 0;
  let openedPaste = 0;
  const doc = {
    activeElement: td,
    addEventListener() {},
    getElementById: (id) => (id === "pane-sheet" ? pane : id === "sheet-viewport" ? vp : null),
    querySelector: () => td,
    querySelectorAll: () => [],
  };
  const env = loadUiModule(SHEET_INPUT_JS, {
    document: doc,
    __EASYOFFICE_SHEET_NAV__: { sheetStep: () => 0 },
    __EASYOFFICE_CREATE_SHEET_CLIPBOARD__: () => ({ shiftFormula: (f) => f, clearRange: async () => {} }),
    __EASYOFFICE_CREATE_SHEET_DIALOG_KIT__: () => ({}),
    __EASYOFFICE_CREATE_SHEET_PASTE_UI__: () => ({ open: () => { openedPaste += 1; } }),
    __EASYOFFICE_CREATE_SHEET_FILL_UI__: () => ({ updateHandle() {} }),
    __EASYOFFICE_CREATE_SHEET_POINT_UI__: () => ({ bind() {}, key: () => false, barKey: () => false, refresh() {}, editEnded() {} }),
    __EASYOFFICE_SHEET_EDIT_BOX__: { createEditBox: () => ({ clear() {}, sync() {} }) },
    __EASYOFFICE_CREATE_SHEET_SELECTION_CHROME__: () => ({ paintSelection() {}, refreshChrome() {} }),
    __EASYOFFICE_CREATE_SHEET_MOUSE__: () => ({ onMouseDown() {}, onMouseOver() {}, onMouseMove() {}, onMouseUp() {}, onDblClick() {} }),
  });
  const stateMap = new WeakMap([[td, { mode: null, userEdited: false }]]);
  const input = env.__EASYOFFICE_CREATE_SHEET_INPUT__({
    document: doc,
    grid: {
      getTdAt: () => td,
      refOf: (r, c) => String.fromCodePoint(64 + c) + r,
      parseRef: () => ({ row: 2, col: 2 }),
      rowCount: () => 20,
      colCount: () => 10,
      visibleRowSpan: () => 10,
      ensureVisible() {},
      rowHeight: () => 24,
    },
    sheetCellFocusState: stateMap,
    isFormulaStr: () => false,
    getSheetName: () => "Sheet1",
    getEntry: () => null,
    getExtent: () => ({ dataRows: 10, dataCols: 5 }),
    hasValue: () => false,
    startEdit: () => { startedEdit += 1; },
    onFormatKey() {},
  });
  input.bindSheetCellEvents(pane);
  const onKeydown = vpListeners.keydown;
  assert.equal(typeof onKeydown, "function");

  let prevented = 0;
  onKeydown({
    type: "keydown",
    target: td,
    key: "a",
    code: "KeyA",
    keyCode: 229,
    ctrlKey: true,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    isComposing: false,
    preventDefault() { prevented += 1; },
  });
  assert.equal(startedEdit, 0, "Ctrl + keyCode 229 while not composing does not start cell edit");
  assert.deepEqual(input.getSelection(), { ar: 1, ac: 1, cr: 10, cc: 5, axis: null, whole: true, span: null, tabFrom: null });
  assert.equal(prevented, 1);

  onKeydown({
    type: "keydown",
    target: td,
    key: "ㅍ",
    code: "KeyV",
    keyCode: 229,
    ctrlKey: true,
    metaKey: false,
    altKey: true,
    shiftKey: false,
    isComposing: false,
    preventDefault() { prevented += 1; },
  });
  assert.equal(openedPaste, 1, "standalone fallback opens paste special on Ctrl+Alt+ㅍ");
});

test("host sheet-point-ui cycles formula reference on Cmd+ㅅ (KeyT) in cell and standalone fallback", async () => {
  const { document } = await openSheet();
  const b9 = cell(document, "B9");
  b9.focus();
  for (const ch of "=C3") {
    key(b9, ch);
    b9.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: b9 });
    b9.textContent += ch;
    b9.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: b9 });
  }
  assert.equal(key(b9, "ㅅ", { code: "KeyT", metaKey: true, isComposing: false }), false, "Cmd+ㅅ cycles reference");
  assert.equal(b9.textContent, "=$C$3");
  assert.equal(key(b9, "Process", { code: "KeyT", metaKey: true, isComposing: false }), false);
  assert.equal(b9.textContent, "=C$3");
  assert.equal(key(b9, "ㅅ", { code: "KeyT", metaKey: true, isComposing: true }), true, "composing does not cycle");
  assert.equal(b9.textContent, "=C$3");

  // Standalone sheet-point-ui fallback without workspace-session utils
  const env = {};
  loadUiModule(new URL("../crates/easyoffice-app/ui/sheet-ref-tokens.js", import.meta.url), env);
  loadUiModule(new URL("../crates/easyoffice-app/ui/sheet-refs.js", import.meta.url), env);
  loadUiModule(new URL("../crates/easyoffice-app/ui/sheet-formula-point.js", import.meta.url), env);
  loadUiModule(SHEET_POINT_JS, env);
  const barListeners = {};
  const bar = {
    value: "=A1",
    selectionStart: 3,
    selectionEnd: 3,
    setSelectionRange() {},
    classList: { toggle() {}, add() {}, remove() {} },
    addEventListener(type, fn) { (barListeners[type] ??= []).push(fn); },
  };
  const doc = { activeElement: bar, getElementById: (id) => (id === "formula-input" ? bar : null) };
  const pointUi = env.__EASYOFFICE_CREATE_SHEET_POINT_UI__({
    document: doc,
    grid: { ensureVisible() {}, tdAt: () => null },
    feedback() {},
    getActiveCell: () => ({ r: 1, c: 1 }),
    getSheetName: () => "Sheet1",
    refOf: () => "A1",
    gridTdOf: () => null,
    stateOf: () => null,
    editable: () => false,
    tablesIn: () => [],
    setMode() {},
    afterWrite() {},
    stepFrom: () => ({ r: 1, c: 1 }),
    cancelEdit() {},
    resumeCell() {},
    finishCell() {},
    switchSheet: async () => {},
    selectTarget() {},
    restoreBar() {},
  });
  pointUi.bind();
  for (const fn of barListeners.input || []) fn();
  let prevented = 0;
  const taken = pointUi.barKey({
    key: "ㅅ",
    code: "KeyT",
    metaKey: true,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    isComposing: false,
    preventDefault() { prevented += 1; },
  });
  assert.equal(taken, true);
  assert.equal(prevented, 1);
  assert.equal(bar.value, "=$A$1");
});

test("host command palette focuses #cmd-search-input on Ctrl+ㄹ (KeyF) and Ctrl+Process (KeyF)", async () => {
  const { document, sandbox } = await createWorkspaceHarness();
  document.getElementById("btn-command-palette").click();
  const modal = document.getElementById("cmd-palette-modal");
  const input = document.getElementById("cmd-search-input");
  assert.equal(modal.open, true);

  document.body.focus();
  assert.notEqual(document.activeElement, input);
  const res = sandbox.dispatchWindowKeydown({ key: "ㄹ", code: "KeyF", ctrlKey: true, isComposing: false });
  assert.equal(res.defaultPrevented, true);
  assert.equal(document.activeElement, input, "Ctrl+ㄹ focuses palette search input");

  document.body.focus();
  const resProc = sandbox.dispatchWindowKeydown({ key: "Process", code: "KeyF", ctrlKey: true, isComposing: false });
  assert.equal(resProc.defaultPrevented, true);
  assert.equal(document.activeElement, input, "Ctrl+Process KeyF focuses palette search input");

  document.body.focus();
  const resComp = sandbox.dispatchWindowKeydown({ key: "Process", code: "KeyF", ctrlKey: true, isComposing: true });
  assert.equal(resComp.defaultPrevented, false);
  assert.notEqual(document.activeElement, input);
});

const HAS_UPSTREAM = fs.existsSync(path.join(REPO, "third_party/rhwp/rhwp-studio/src/command/shortcut-map.ts"));

function buildTempHwpModules() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "eo-hwp-ime-"));
  const writeTs = (relOut, srcRel, transform = (s) => s) => {
    const raw = readText(path.join(REPO, srcRel));
    const js = stripTypeScriptTypes(transform(raw), { mode: "transform" });
    const out = path.join(dir, relOut);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.writeFileSync(out, js);
  };
  writeTs("upstream/engine/navigation-keymap.mjs", "third_party/rhwp/rhwp-studio/src/engine/navigation-keymap.ts");
  writeTs("upstream/command/document-shortcut-guard.mjs", "third_party/rhwp/rhwp-studio/src/command/document-shortcut-guard.ts");
  writeTs("upstream/command/shortcut-map.mjs", "third_party/rhwp/rhwp-studio/src/command/shortcut-map.ts", (s) =>
    s.replace("../engine/navigation-keymap.ts", "../engine/navigation-keymap.mjs")
  );
  writeTs("embed/shell-shortcuts.mjs", "apps/hwp-studio/src/embed/shell-shortcuts.ts");
  writeTs("command/shortcut-map.mjs", "apps/hwp-studio/src/command/shortcut-map.ts", (s) =>
    s
      .replaceAll("'@upstream/command/shortcut-map'", "'../upstream/command/shortcut-map.mjs'")
      .replaceAll("'@upstream/engine/navigation-keymap'", "'../upstream/engine/navigation-keymap.mjs'")
      .replaceAll("'../embed/shell-shortcuts.ts'", "'../embed/shell-shortcuts.mjs'")
  );
  writeTs("embed/global-shortcuts.mjs", "apps/hwp-studio/src/embed/global-shortcuts.ts", (s) =>
    s
      .replaceAll("'@/command/shortcut-map'", "'../command/shortcut-map.mjs'")
      .replaceAll("'@/command/document-shortcut-guard'", "'../upstream/command/document-shortcut-guard.mjs'")
      .replaceAll("'./shell-shortcuts.ts'", "'./shell-shortcuts.mjs'")
  );
  return dir;
}

test("hwp-studio shortcut-map and global-shortcuts normalize Korean 2-beolsik and Process while protecting composing, Dvorak and AltGr", { skip: !HAS_UPSTREAM }, async () => {
  const dir = buildTempHwpModules();
  try {
    const { defaultShortcuts, matchShortcut } = await import(pathToFileURL(path.join(dir, "command/shortcut-map.mjs")).href);
    const { setupGlobalShortcuts } = await import(pathToFileURL(path.join(dir, "embed/global-shortcuts.mjs")).href);

    const kbd = (init) => ({ key: "", code: "", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, isComposing: false, ...init });
    assert.equal(matchShortcut(kbd({ key: "ㅠ", code: "KeyB", ctrlKey: true }), defaultShortcuts, "other"), "format:bold");
    assert.equal(matchShortcut(kbd({ key: "ㅋ", code: "KeyZ", ctrlKey: true }), defaultShortcuts, "other"), "edit:undo");
    assert.equal(matchShortcut(kbd({ key: "ㅋ", code: "KeyZ", ctrlKey: true, shiftKey: true }), defaultShortcuts, "other"), "edit:redo");
    assert.equal(matchShortcut(kbd({ key: "Process", code: "KeyB", ctrlKey: true, isComposing: false }), defaultShortcuts, "other"), "format:bold");
    assert.equal(matchShortcut(kbd({ key: "Dead", code: "KeyN", altKey: true }), defaultShortcuts, "mac"), "file:new-doc");
    assert.equal(matchShortcut(kbd({ key: "ㅠ", code: "KeyB", ctrlKey: true, isComposing: true }), defaultShortcuts, "other"), null);
    assert.equal(matchShortcut(kbd({ key: "k", code: "KeyV", ctrlKey: true }), defaultShortcuts, "other"), null, "Dvorak ASCII Ctrl+K does not fall back to KeyV");
    assert.equal(
      matchShortcut(kbd({ key: "ą", code: "KeyA", ctrlKey: true, altKey: true, getModifierState: (m) => m === "AltGraph" }), defaultShortcuts, "other"),
      null,
      "AltGr character input is not hijacked"
    );

    const listeners = [];
    const prevDoc = globalThis.document;
    globalThis.document = {
      addEventListener: (_type, fn) => listeners.push(fn),
      querySelector: () => null,
    };
    const dispatched = [];
    try {
      setupGlobalShortcuts({
        dispatcher: {
          dispatch: (id) => dispatched.push(id),
          dispatchWithResult: () => ({ ok: false }),
        },
        getInputHandler: () => null,
        getCanvasView: () => null,
      });
      const onKeydown = listeners[0];
      const fire = (init) => {
        let prevented = false;
        onKeydown({ ...kbd(init), target: null, preventDefault() { prevented = true; } });
        return prevented;
      };
      assert.equal(fire({ key: "ㅜ", code: "KeyN", altKey: true }), true);
      assert.equal(fire({ key: "Process", code: "KeyN", altKey: true, isComposing: false }), true);
      assert.equal(fire({ key: "ㅐ", code: "KeyO", ctrlKey: true }), true);
      assert.equal(fire({ key: "Process", code: "KeyO", ctrlKey: true, isComposing: false }), true);
      assert.equal(fire({ key: "Process", code: "KeyO", ctrlKey: true, isComposing: true }), false);
      assert.deepEqual(dispatched, ["file:new-doc", "file:new-doc", "file:open", "file:open"]);
    } finally {
      globalThis.document = prevDoc;
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

const BUILT_ASSETS = path.join(REPO, "crates/easyoffice-app/ui/hwp-studio/assets");
test("built hwp-studio bundle includes AltGraph and physicalLatinKey normalization", { skip: !fs.existsSync(BUILT_ASSETS) }, () => {
  const bundle = fs
    .readdirSync(BUILT_ASSETS)
    .filter((f) => f.endsWith(".js"))
    .sort((a, b) => a.localeCompare(b))
    .map((f) => readText(path.join(BUILT_ASSETS, f)))
    .join("\n");
  assert.ok(bundle.includes("AltGraph") && bundle.includes("Unidentified"), "built editor bundle contains PD-326 key normalization");
});
