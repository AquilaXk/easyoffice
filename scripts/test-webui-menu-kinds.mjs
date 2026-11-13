import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { readText } from "./test-support/read-text.mjs";
import { itemTable } from "./test-support/rust-menu-table.mjs";

const byName = (a, b) => a.localeCompare(b);

// The menu row follows the window kind (PD-190): a document, spreadsheet, presentation or PDF window shows the menus
// and items that mean something there and no grayed leftovers; the Hangul window's fallback editor keeps the document
// row plus its own items. The native macOS bar has the same menus per kind (`native_menu/layout.rs`).

const REPO = new URL("../", import.meta.url);
const read = (rel) => readText(fileURLToPath(new URL(rel, REPO)));

const KINDS = ["DOC", "HWP", "SHEET", "SLIDE", "PDF"];
const TITLES = {
  DOC: ["파일", "편집", "보기", "입력", "서식", "쪽", "표", "도구", "도움말"],
  HWP: ["파일", "편집", "보기", "입력", "서식", "쪽", "표", "도구", "도움말"],
  SHEET: ["파일", "편집", "보기", "삽입", "서식", "시트", "데이터", "도구", "도움말"],
  SLIDE: ["파일", "편집", "보기", "삽입", "서식", "슬라이드", "슬라이드 쇼", "도구", "도움말"],
  PDF: ["파일", "편집", "보기", "주석", "쪽", "도구", "도움말"],
};

const shown = (el) => !el.closest("[hidden]");
const open = (mod) => createWorkspaceHarness({ search: `?module=${mod}&win=w-kind-${mod}` });
const shownMenus = (document) => document.querySelectorAll(".studio-menubar > .menu-item").filter(shown);
const titlesOf = (document) => shownMenus(document).map((menu) => menu.querySelector(".menu-title").textContent);
const commandsOf = (document, menuTitle) => {
  const menu = shownMenus(document).find((m) => m.querySelector(".menu-title").textContent === menuTitle);
  return menu.querySelectorAll(".md-item[data-studio-menu-cmd]").filter(shown).map((el) => el.dataset.studioMenuCmd);
};
const shownCommands = (document) => new Set(document.querySelectorAll(".md-item[data-studio-menu-cmd]").filter(shown).map((el) => el.dataset.studioMenuCmd));

for (const mod of KINDS) {
  test(`${mod}: the top-level menus are ${TITLES[mod].join(" · ")}`, async () => {
    const { document } = await open(mod);
    await flushAsyncTurns(10);
    assert.deepEqual(titlesOf(document), TITLES[mod]);
  });

  test(`${mod}: separators sit only between two shown items`, async () => {
    const { document } = await open(mod);
    await flushAsyncTurns(10);
    for (const menu of shownMenus(document)) {
      const rows = menu.querySelector(":scope > .menu-dropdown").children.filter((el) => !el.hidden);
      const name = menu.querySelector(".menu-title").textContent;
      assert.ok(!rows[0].classList.contains("md-sep"), `${mod} ${name} starts with a separator`);
      assert.ok(!rows.at(-1).classList.contains("md-sep"), `${mod} ${name} ends with a separator`);
      rows.forEach((row, i) => {
        if (i > 0) assert.ok(!(row.classList.contains("md-sep") && rows[i - 1].classList.contains("md-sep")), `${mod} ${name} repeats a separator`);
      });
    }
  });
}

test("a window shows only the items that mean something in it", async () => {
  const absent = {
    DOC: ["sheet-mod-pivot", "sheet-mod-chart", "sheet-mod-condfmt", "sheet-mod-freeze", "slide-mod-master", "slide-mod-presenter", "pdf-mod-rotate", "pdf-mod-highlight", "hwp-mod-field", "hwp-mod-header", "hwp-mod-border"],
    SHEET: ["para-shape", "para-numbering", "para-bullet", "indent-in", "indent-out", "layout-columns", "mod-header", "mod-footnote", "doc-mod-toc", "doc-mod-mailmerge", "doc-mod-tableformula", "table-add-row", "table-merge", "table-sum", "ctrl-marks", "para-marks", "doc-grid", "slide-mod-master", "pdf-mod-rotate", "hwp-mod-field"],
    SLIDE: ["para-shape", "para-numbering", "indent-in", "layout-columns", "mod-header", "doc-mod-toc", "table-add-row", "data-sort", "data-validation", "data-autosum", "data-fn-browser", "sheet-mod-pivot", "pdf-mod-rotate", "find", "replace", "clip-cut", "clip-paste", "ctrl-marks"],
    PDF: ["home-bold", "home-italic", "home-underline", "home-style", "para-shape", "insert-table", "insert-shape", "insert-chart", "insert-link", "layout-paper", "layout-orientation", "layout-columns", "layout-margins", "data-sort", "data-autosum", "mod-header", "doc-mod-toc", "sheet-mod-pivot", "slide-mod-master", "find", "replace", "clip-cut", "clip-copy", "clip-paste", "print", "review-track", "ctrl-marks", "para-marks", "doc-grid"],
  };
  for (const [mod, cmds] of Object.entries(absent)) {
    const { document } = await open(mod);
    await flushAsyncTurns(10);
    const have = shownCommands(document);
    assert.deepEqual(cmds.filter((cmd) => have.has(cmd)), [], `${mod} shows items of other kinds`);
  }
});

test("the spreadsheet puts sorting, validation, pivot and conditional formats in 데이터 and the frame in 시트", async () => {
  const { document } = await open("SHEET");
  await flushAsyncTurns(10);
  assert.deepEqual(commandsOf(document, "데이터"), ["data-sort", "data-validation", "sheet-mod-pivot", "sheet-mod-condfmt"]);
  assert.deepEqual(commandsOf(document, "시트"), ["sheet-mod-freeze"]);
  assert.deepEqual(commandsOf(document, "삽입").slice(-2), ["data-fn-browser", "data-autosum"]);
});

test("the presentation has 슬라이드 and 슬라이드 쇼 menus and the PDF has 주석 and 쪽", async () => {
  const slide = await open("SLIDE");
  await flushAsyncTurns(10);
  assert.deepEqual(commandsOf(slide.document, "슬라이드"), ["slide-mod-master", "slide-mod-snap", "layout-orientation", "layout-margins"]);
  assert.deepEqual(commandsOf(slide.document, "슬라이드 쇼"), ["slide-mod-transition", "slide-mod-presenter"]);
  const pdf = await open("PDF");
  await flushAsyncTurns(10);
  assert.deepEqual(commandsOf(pdf.document, "주석"), ["pdf-mod-highlight", "review-comment"]);
  assert.deepEqual(commandsOf(pdf.document, "쪽"), ["pdf-mod-rotate", "pdf-mod-splitmerge"]);
});

test("the document window keeps its menus and the Hangul window's fallback editor adds its own items", async () => {
  const doc = await open("DOC");
  await flushAsyncTurns(10);
  assert.deepEqual(commandsOf(doc.document, "표"), ["insert-table", "table-row-above", "table-add-row", "table-col-left", "table-col-right", "table-row-delete", "table-col-delete", "table-split", "table-merge", "table-sum", "doc-mod-tableformula", "data-autosum", "data-fn-browser", "data-sort", "data-validation"]);
  const hwp = await open("HWP");
  await flushAsyncTurns(10);
  assert.ok(commandsOf(hwp.document, "입력").includes("hwp-mod-field"));
  assert.ok(commandsOf(hwp.document, "쪽").includes("hwp-mod-manuscript"));
  assert.ok(commandsOf(hwp.document, "표").includes("hwp-mod-border"));
  assert.ok(!commandsOf(hwp.document, "입력").includes("doc-mod-toc"));
});

test("the start screen and the converter use the document menus", async () => {
  const { menuKindOf } = (await open("DOC")).document.defaultView.__EASYOFFICE_MENU_KINDS__;
  for (const [moduleCode, kind] of [["START", "DOC"], ["CONV", "DOC"], ["HWP_EMBED", "HWP"], ["SHEET", "SHEET"], ["SLIDE", "SLIDE"], ["PDF", "PDF"], [undefined, "DOC"]]) {
    assert.equal(menuKindOf(moduleCode), kind, String(moduleCode));
  }
});

test("switching the kind of an open window swaps the menus and closes a menu that went away", async () => {
  const { document } = await open("DOC");
  await flushAsyncTurns(10);
  const { applyMenuKind } = document.defaultView.__EASYOFFICE_MENU_KINDS__;
  const table = document.querySelector('.menu-item[data-menu="table"]');
  table.classList.add("open");
  applyMenuKind(document, "PDF");
  assert.deepEqual(titlesOf(document), TITLES.PDF);
  assert.ok(!table.classList.contains("open"), "the hidden 표 menu is closed");
  applyMenuKind(document, "SHEET");
  assert.deepEqual(titlesOf(document), TITLES.SHEET);
  applyMenuKind(document, "DOC");
  assert.deepEqual(titlesOf(document), TITLES.DOC);
});

test("keyboard navigation between menus skips the menus of other kinds", async () => {
  const { document } = await open("PDF");
  await flushAsyncTurns(10);
  const press = (el, key) => el.dispatchEvent({ type: "keydown", key, target: el });
  const titleOf = (name) => document.querySelector(`.menu-item[data-menu="${name}"] > .menu-title`);
  const view = titleOf("view");
  view.focus();
  press(view, "ArrowRight");
  assert.ok(document.activeElement === titleOf("annotate"), "보기 -> 주석 (no 입력)");
  press(document.activeElement, "ArrowRight");
  assert.ok(document.activeElement === titleOf("page"), "주석 -> 쪽 (no 서식)");
  press(document.activeElement, "ArrowRight");
  assert.ok(document.activeElement === titleOf("tool"), "쪽 -> 도구 (no 표)");
  press(document.activeElement, "End");
  assert.ok(document.activeElement === titleOf("help"));
});

// ---- the native bar has the same menus per kind ---------------------------------------------------

const TOKENS = { S: "-", NEW_DOCS: "@new-docs", THEME: "@theme", NUMFMT: "@numfmt", ALIGN: "@align", BORDER: "@border", PASTE: "@paste", SELECT_ALL: "@select-all", SERVICES: "@services", HIDE: "@hide", HIDE_OTHERS: "@hide-others", SHOW_ALL: "@show-all", MINIMIZE: "@minimize", CLOSE_WINDOW: "@close-window", QUIT_ID: "app:quit" };

function rustLayout() {
  const src = read("crates/easyoffice-desktop/src/native_menu/layout.rs");
  const menus = {};
  for (const m of src.matchAll(/const (\w+): MenuSpec = menu\(\s*"([^"]+)",\s*&\[([\s\S]*?)\],?\s*\);/g)) {
    const lines = m[3].split(",").map((t) => t.trim()).filter(Boolean).map((t) => (t.startsWith('"') ? t.slice(1, -1) : TOKENS[t] ?? assert.fail(`unknown token ${t}`)));
    menus[m[1]] = { title: m[2], lines };
  }
  const kinds = {};
  for (const m of src.matchAll(/const (DOC|HWP|SHEET|SLIDE|PDF): \[&MenuSpec; (\d+)\] = \[([\s\S]*?)\];/g)) {
    const names = [...m[3].matchAll(/&(\w+)/g)].map((x) => x[1]);
    assert.equal(names.length, Number(m[2]), `${m[1]} array length`);
    kinds[m[1]] = names.map((name) => menus[name] ?? assert.fail(`unknown menu ${name}`));
  }
  return kinds;
}

const ITEM_PATTERN = /item\(\s*"([^"]+)",\s*"([^"]+)",\s*(?:None|Some\("[^"]+"\)),\s*"([^"]+)",?\s*\)/g;

function rustItemTable() {
  return itemTable(read("crates/easyoffice-desktop/src/native_menu.rs"));
}

function rustItems() {
  return new Map([...rustItemTable().matchAll(ITEM_PATTERN)].map((m) => [m[1], m[3]]));
}

function rustLabels() {
  return new Map([...rustItemTable().matchAll(ITEM_PATTERN)].map((m) => [m[1], m[2]]));
}

function submenuLines(name = "NUMFMT") {
  const src = read("crates/easyoffice-desktop/src/native_menu/layout.rs");
  const body = src.match(new RegExp(`pub const ${name}_LINES: \\[&str; (\\d+)\\] = \\[([\\s\\S]*?)\\];`));
  const lines = body[2].split(",").map((t) => t.trim()).filter(Boolean).map((t) => (t === "SEPARATOR" ? "-" : t.slice(1, -1)));
  assert.equal(lines.length, Number(body[1]));
  return lines;
}

// The page commands of the standard edit group stay on every native bar (text fields need them) and are not compared.
const EDIT_GROUP = new Set(["studio-cmd:studio-undo", "studio-cmd:studio-redo", "studio-cmd:clip-cut", "studio-cmd:clip-copy", "studio-cmd:clip-paste", "file-save-as"]);
const PAGE_OWN = { "studio-save": "studio-save", "studio-save-as": "file-save-as", find: "find", replace: "replace", print: "print" };

const nativeActions = (kind, items) =>
  new Set(rustLayout()[kind].flatMap((menu) => menu.lines).map((id) => items.get(id)).filter((a) => a && (a.startsWith("studio-cmd:") || ["find", "replace", "print", "studio-save"].includes(a))).filter((a) => !EDIT_GROUP.has(a)));

test("the native bars list the same menus as the page for every kind", () => {
  const layout = rustLayout();
  assert.deepEqual(Object.keys(layout).sort(byName), [...KINDS].sort(byName));
  for (const kind of KINDS) {
    const titles = layout[kind].map((menu) => menu.title);
    assert.deepEqual(titles.filter((t) => t !== "EasyOffice" && t !== "윈도우"), TITLES[kind], kind);
    assert.equal(titles[0], "EasyOffice");
    assert.equal(titles.at(-2), "윈도우");
  }
});

test("the native bar of a document, sheet, slide or PDF window has the page's commands and nothing else", async () => {
  const items = rustItems();
  for (const kind of ["DOC", "SHEET", "SLIDE", "PDF"]) {
    const { document } = await open(kind);
    await flushAsyncTurns(10);
    const page = new Set([...shownCommands(document)].map((cmd) => PAGE_OWN[cmd] ?? `studio-cmd:${cmd}`).filter((a) => !EDIT_GROUP.has(a)));
    assert.deepEqual([...page].sort(byName), [...nativeActions(kind, items)].sort(byName), kind);
  }
});

test("the Hangul window's native bar has every command of its page menus", async () => {
  const { document } = await open("HWP");
  await flushAsyncTurns(10);
  const page = [...shownCommands(document)].map((cmd) => PAGE_OWN[cmd] ?? `studio-cmd:${cmd}`).filter((a) => !EDIT_GROUP.has(a));
  const native = nativeActions("HWP", rustItems());
  assert.deepEqual(page.filter((a) => !native.has(a)), []);
});

test("the page reports the menu kind to the shell with its menu state", () => {
  const chrome = read("crates/easyoffice-app/ui/shell-chrome.js");
  assert.match(chrome, /kind: host\.__EASYOFFICE_MENU_KINDS__\.menuKindOf\(active\)/);
  const kinds = read("crates/easyoffice-app/ui/menu-kinds.js");
  assert.match(kinds, /const KINDS = Object\.freeze\(\["DOC", "HWP", "SHEET", "SLIDE", "PDF"\]\)/);
  const rust = read("crates/easyoffice-desktop/src/native_menu/layout.rs");
  for (const kind of KINDS) assert.ok(rust.includes(`"${kind}" => Some(`), kind);
});

// The rows of a menu as the page shows them and as the native bar lists them: a label per item, "-" for a separator and
// "label > ..." for a submenu.
const pageRows = (container) =>
  container.children
    .filter((el) => !el.hidden)
    .map((el) => {
      if (el.classList.contains("md-sep")) return "-";
      const label = el.querySelector(":scope > .md-label").textContent;
      const panel = el.querySelector(":scope > .md-sub-panel");
      return panel ? `${label} > ${pageRows(panel).join(" | ")}` : label;
    });

function nativeRows(menu) {
  const labels = rustLabels();
  return menu.lines.map((line) => {
    const submenu = { "@numfmt": ["표시 형식", "NUMFMT"], "@align": ["맞춤", "ALIGN"], "@border": ["테두리", "BORDER"] }[line];
    if (submenu) return `${submenu[0]} > ${submenuLines(submenu[1]).map((id) => labels.get(id) ?? id).join(" | ")}`;
    return labels.get(line) ?? line;
  });
}

test("the spreadsheet's native 서식 and 시트 menus list the page's items in the page's order with its separators", async () => {
  const { document } = await open("SHEET");
  await flushAsyncTurns(10);
  const native = rustLayout().SHEET;
  for (const title of ["서식", "시트"]) {
    const pageMenu = shownMenus(document).find((m) => m.querySelector(".menu-title").textContent === title);
    const rows = pageRows(pageMenu.querySelector(":scope > .menu-dropdown"));
    const expected = nativeRows(native.find((menu) => menu.title === title));
    assert.ok(rows.length >= 8, `${title} rows were read`);
    assert.deepEqual(expected, rows, title);
  }
});
