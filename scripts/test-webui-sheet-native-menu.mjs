import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { readText } from "./test-support/read-text.mjs";
import { itemTable } from "./test-support/rust-menu-table.mjs";

// The spreadsheet's number-format and row/column/merge items (PD-197) are in the native bar and the command search:
// a native item clicks the page's own menu item, so it runs the same command path, and the search lists them by label.

const REPO = new URL("../", import.meta.url);
const read = (rel) => readText(fileURLToPath(new URL(rel, REPO)));

function sheetItems() {
  const source = read("crates/easyoffice-desktop/src/native_menu.rs");
  const table = itemTable(source);
  return [...table.matchAll(/item\(\s*"([^"]+)",\s*"([^"]+)",\s*None,\s*"(sheet-(?:act|numfmt|paste)):([^"]+)",?\s*\)/g)].map((m) => ({ id: m[1], label: m[2], name: m[3], argument: m[4] }));
}

const selectorOf = ({ name, argument }) => `.md-item[data-${name}="${argument}"]`;

const open = async () => {
  const harness = await createWorkspaceHarness({ search: "?module=SHEET&win=w-sheet-native" });
  await flushAsyncTurns(10);
  return harness;
};

test("the table holds the 11 number-format items, the 35 row, column, hide, merge, sort, filter, freeze, sheet tab, group, page setup, print area and name items and 골라 붙이기", () => {
  const items = sheetItems();
  assert.equal(items.filter((i) => i.name === "sheet-numfmt").length, 11);
  assert.equal(items.filter((i) => i.name === "sheet-act").length, 35);
  assert.deepEqual(items.filter((i) => i.name === "sheet-paste").map((i) => [i.id, i.label, i.argument]), [["edit:paste-special", "골라 붙이기…", "special"]]);
});

test("each native item clicks the page's menu item that carries its label", async () => {
  const { document } = await open();
  const clicked = [];
  const items = sheetItems();
  for (const item of items) {
    const el = document.querySelector(selectorOf(item));
    assert.ok(el, `${item.id} has a page item`);
    assert.equal(el.querySelector(".md-label").textContent, item.label, item.id);
    el.addEventListener("click", () => clicked.push(item.id));
  }
  for (const item of items) assert.equal(document.defaultView.__EASYOFFICE_NATIVE_MENU__(item.id), true, item.id);
  assert.deepEqual(clicked, items.map((i) => i.id));
});

test("the spreadsheet window reports none of the sheet items as disabled, as the page grays none of them", async () => {
  const reports = [];
  const { document } = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-sheet-state",
    platform: "MacIntel",
    shellCommands: { report_menu_state: (args) => { reports.push(args); return null; }, report_window_state: () => null, take_pending_open: () => null, list_recent_documents: () => [] },
  });
  await flushAsyncTurns(20);
  const last = reports.at(-1);
  assert.equal(last.kind, "SHEET");
  const ids = sheetItems().map((i) => i.id);
  assert.deepEqual(last.disabled.filter((id) => ids.includes(id)), []);
  for (const item of sheetItems()) assert.notEqual(document.querySelector(selectorOf(item)).getAttribute("aria-disabled"), "true", item.id);
});

const search = (document, text) => {
  const input = document.getElementById("cmd-search-input");
  input.value = text;
  input.dispatchEvent({ type: "input" });
  return [...document.querySelectorAll("#cmd-palette-list .cp-item")];
};

test("the command search finds the sheet items by label and runs the page item", async () => {
  const { document, userClick } = await open();
  userClick(document.getElementById("btn-command-palette"));
  for (const { label } of sheetItems()) {
    const found = search(document, label).map((li) => li.querySelector(".cp-item-label").textContent);
    assert.ok(found.includes(label), `${label} is searchable`);
  }
  let clicks = 0;
  document.querySelector('.md-item[data-sheet-act="merge"]').addEventListener("click", () => (clicks += 1));
  const [first] = search(document, "셀 병합");
  first.click();
  assert.equal(clicks, 1);
  assert.equal(document.getElementById("cmd-palette-modal").open, false);
});

test("the command search of a document window does not list the sheet items", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=DOC&win=w-doc-native" });
  await flushAsyncTurns(10);
  userClick(document.getElementById("btn-command-palette"));
  assert.deepEqual(search(document, "셀 병합"), []);
  assert.deepEqual(search(document, "행 위에 삽입"), []);
});
