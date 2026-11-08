import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// Menu bar (PD-098, step 3a): rhwp-studio markup and behaviour, and every ribbon command of the
// catalog (140 ids over the five editing modules) is reachable from a menu item.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (name) => fs.readFileSync(fileURLToPath(new URL(name, UI_DIR)), "utf8");
const MODULES = ["DOC", "HWP", "SHEET", "SLIDE", "PDF"];

async function openModule(mod) {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => mod,
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const executed = [];
  const harness = await createWorkspaceHarness({
    search: `?module=${mod}&win=w-menu-${mod}`,
    testIpc: async (command, payload) => {
      if (command === "execute_ribbon_command") {
        try {
          executed.push(JSON.parse(payload).commandId);
        } catch {
          executed.push(String(payload));
        }
      }
      return offline[command](payload);
    },
  });
  await flushAsyncTurns(20);
  return { ...harness, executed, catalog: harness.sandbox.__EASYOFFICE_RIBBON_CATALOG__ };
}

// The ribbon strip is gone (PD-098 step 3b): `*-view-ribbon` (collapse/expand) stays in the catalog
// and the IPC contract but has no menu item, so every other id must be reachable from a menu.
const RIBBON_COLLAPSE_SUFFIX = "-view-ribbon";
const MENU_COMMANDS_PER_MODULE = 27;

const catalogIdsOf = (catalog, mod) =>
  Object.values(catalog.RIBBON_CATEGORY_TOOLS[mod]).flatMap((tools) => tools.map((t) => t.id));
const idsOf = (catalog, mod) => catalogIdsOf(catalog, mod).filter((id) => !id.endsWith(RIBBON_COLLAPSE_SUFFIX));

const enabledMenuItems = (document) =>
  document.querySelectorAll(".md-item[data-studio-menu-cmd]").filter((el) => el.getAttribute("aria-disabled") !== "true");

test("the catalog has 140 ribbon commands, five editing modules with 28 each", async () => {
  const { catalog } = await openModule("DOC");
  const all = catalog.allRibbonCommandIds();
  assert.equal(all.length, 140);
  assert.equal(new Set(all).size, 140);
  for (const mod of MODULES) {
    assert.equal(catalogIdsOf(catalog, mod).length, 28, mod);
    assert.equal(idsOf(catalog, mod).length, MENU_COMMANDS_PER_MODULE, mod);
  }
});

for (const mod of MODULES) {
  test(`${mod}: every ribbon command of the module has an enabled menu item`, async () => {
    const { document, catalog } = await openModule(mod);
    const reachable = new Set(enabledMenuItems(document).map((el) => catalog.resolveMenuCommand(el.dataset.studioMenuCmd, mod)).filter(Boolean));
    const missing = idsOf(catalog, mod).filter((id) => !reachable.has(id));
    assert.deepEqual(missing, [], `${mod}: ribbon commands without a menu item`);
  });

  test(`${mod}: clicking the menu items really executes each ribbon command`, async () => {
    const { document, catalog, executed } = await openModule(mod);
    const clicked = new Map();
    for (const item of enabledMenuItems(document)) {
      const id = catalog.resolveMenuCommand(item.dataset.studioMenuCmd, mod);
      if (id && !clicked.has(id)) clicked.set(id, item);
    }
    for (const item of clicked.values()) item.click();
    await flushAsyncTurns(4 * clicked.size);
    // `view-theme` runs the theme toggle itself (the menu item behaves like the ribbon button).
    const expected = idsOf(catalog, mod).filter((id) => !id.endsWith("-view-theme"));
    const missing = expected.filter((id) => !executed.includes(id));
    assert.deepEqual(missing, [], `${mod}: menu items that did not run their ribbon command`);
  });
}

test("module-specific items are disabled outside their module and common items stay enabled", async () => {
  const { document } = await openModule("SHEET");
  const state = (cmd) => document.querySelector(`.md-item[data-studio-menu-cmd="${cmd}"]`).getAttribute("aria-disabled");
  assert.equal(state("sheet-mod-pivot"), "false");
  assert.equal(state("doc-mod-toc"), "true");
  assert.equal(state("pdf-mod-rotate"), "true");
  assert.equal(state("data-sort"), "false");
  assert.equal(state("review-stats"), "false");
});

test("the menus follow rhwp-studio: ids, titles, dropdowns, sub-menus and separators", async () => {
  const { document } = await openModule("DOC");
  const bar = document.getElementById("menu-bar");
  assert.ok(bar, "#menu-bar");
  assert.deepEqual(
    document.querySelectorAll(".menu-item > .menu-title").map((el) => el.textContent),
    ["파일", "편집", "보기", "입력", "서식", "쪽", "표", "도구", "도움말"]
  );
  for (const menu of document.querySelectorAll(".menu-item")) {
    assert.ok(menu.querySelector(".menu-dropdown"), `${menu.dataset.menu} has a dropdown`);
    assert.ok(menu.querySelector(".md-item"), `${menu.dataset.menu} has items`);
  }
  for (const sub of document.querySelectorAll(".md-sub")) {
    assert.ok(sub.querySelector(".md-sub-panel"), "every .md-sub owns a .md-sub-panel");
    assert.ok(sub.querySelector(".md-arrow"), "and an arrow");
  }
  assert.ok(document.querySelectorAll(".md-sep").length >= 10);
  for (const label of document.querySelectorAll(".md-shortcut")) assert.ok(label.textContent.trim().length > 0);
});

test("opening, hover switching and closing work like rhwp-studio's menu bar", async () => {
  const { document, sandbox, userClick } = await openModule("DOC");
  const titles = document.querySelectorAll(".menu-item > .menu-title");
  const [file, edit] = titles;
  userClick(file);
  assert.ok(file.parentElement.classList.contains("open"));
  edit.parentElement.dispatchEvent({ type: "mouseenter" });
  assert.ok(edit.parentElement.classList.contains("open"), "hover moves to the next menu while one is open");
  assert.ok(!file.parentElement.classList.contains("open"));
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  assert.equal(document.querySelectorAll(".menu-item.open").length, 0, "Escape closes the menu");
  userClick(edit);
  userClick(edit);
  assert.equal(document.querySelectorAll(".menu-item.open").length, 0, "a second click on the title closes it");
});

test("a single key runs the open menu's item with that shortcut label", async () => {
  const { document, sandbox, userClick } = await openModule("DOC");
  const tool = document.querySelector('.menu-item[data-menu="tool"] > .menu-title');
  userClick(tool);
  const modal = document.getElementById("cmd-palette-modal");
  assert.equal(modal.open, false);
  const ev = sandbox.dispatchWindowKeydown({ key: "/" });
  await flushAsyncTurns();
  assert.equal(ev.defaultPrevented, true);
  assert.equal(modal.open, true, "the '/' item (command search) ran");
});

test("Ctrl+Alt+O opens the recent-documents dialog and the menu advertises it", async () => {
  const { document, sandbox } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-ctrl-alt-o",
    shellCommands: {
      list_recent_documents: () => [{ id: "a1", name: "예산.xlsx", kind: "SHEET" }],
      open_document_window: () => null, open_file_in_new_window: () => null, take_pending_open: () => null,
      report_window_state: () => null, close_current_window: () => null, open_recent_document: () => null,
    },
  });
  await flushAsyncTurns(20);
  const item = document.getElementById("recent-dialog-open");
  assert.equal(item.querySelector(".md-shortcut").textContent, "Ctrl+Alt+O");
  const bound = sandbox.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS.find((b) => b.keys === "Ctrl+Alt+O");
  assert.equal(bound.action, "recent-dialog");
  const ev = sandbox.dispatchWindowKeydown({ key: "o", ctrlKey: true, altKey: true });
  await flushAsyncTurns(6);
  assert.equal(ev.defaultPrevented, true);
  assert.ok(document.querySelector(".recent-documents-dialog"), "the dialog is open");
});

test("menu-bar.css keeps rhwp-studio's selectors and names its source", () => {
  const css = read("menu-bar.css");
  assert.match(css, /Adapted from rhwp-studio/);
  for (const sel of [".menu-item.open > .menu-title", ".menu-dropdown", ".md-item", ".md-shortcut", ".md-sep", ".md-sub-panel", ".md-arrow"]) {
    assert.ok(css.includes(sel), sel);
  }
});

test("menu-bar.css and toolbar.css name rhwp-studio as their source and are in the notices", () => {
  const notices = fs.readFileSync(new URL("../THIRD_PARTY_NOTICES.md", import.meta.url), "utf8");
  for (const name of ["menu-bar.css", "toolbar.css"]) {
    assert.match(read(name), /Adapted from rhwp-studio/);
    assert.ok(read(name).includes("Edward Kim"));
    assert.ok(notices.includes(`ui/${name}`), `${name} is in the notices`);
  }
});

// A label must name what its command does. The app has no format painter, no find-and-replace and no
// object properties dialog, so no menu item or toolbar button may be called that; `home-style` is the
// ribbon's "스타일 적용" and `layout-paper` the paper size toggle ("편집 용지").
test("menu and toolbar labels name what their command does", async () => {
  const { document } = await openModule("DOC");
  const labelsOf = (cmd) =>
    document.querySelectorAll(`.md-item[data-studio-menu-cmd="${cmd}"] .md-label`).map((el) => el.textContent.trim());
  const buttonsOf = (cmd) => document.querySelectorAll(`#icon-toolbar [data-studio-cmd="${cmd}"]`);
  assert.deepEqual(labelsOf("home-style"), ["스타일 적용"]);
  assert.deepEqual(buttonsOf("home-style").map((el) => [el.getAttribute("title"), el.getAttribute("aria-label")]), [["스타일 적용", "스타일 적용"]]);
  assert.deepEqual(labelsOf("layout-paper"), ["편집 용지", "편집 용지"]);
  assert.equal(buttonsOf("layout-paper").length, 0, "no toolbar button called object properties");
  const named = [
    ...document.querySelectorAll(".md-label").map((el) => el.textContent),
    ...document.querySelectorAll("#icon-toolbar [title]").map((el) => el.getAttribute("title")),
  ];
  for (const promised of ["모양 복사", "찾아 바꾸기", "개체 속성", "글자 모양"]) {
    assert.ok(!named.some((text) => text.includes(promised)), `${promised} is not a feature of the app`);
  }
});
