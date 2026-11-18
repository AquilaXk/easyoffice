import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p, textNodeOf } from "./test-support/doc-run-harness.mjs";

const select = async (h, range) => {
  h.selection.addRange(range);
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
};
const caretIn = (h, block) => select(h, rangeOf(textNodeOf(block), 0, textNodeOf(block), 0));

test("DOC toolbar, format menu, and native menu expose official Korean '체크리스트' command", async () => {
  const h = await createRunHarness([p("본문")], { search: "?module=DOC&win=w-doc-checklist" });
  const btn = h.document.querySelector('#icon-toolbar .tb-btn[data-studio-cmd="para-checklist"]');
  assert.ok(btn, "toolbar checklist button exists");
  assert.equal(btn.getAttribute("title"), "체크리스트");
  assert.equal(btn.getAttribute("aria-label"), "체크리스트");
  assert.equal(btn.getAttribute("aria-pressed"), "false");

  const menuItem = h.document.querySelector('#menu-bar .md-item[data-studio-menu-cmd="para-checklist"]');
  assert.ok(menuItem, "format menu checklist item exists");
  assert.equal(menuItem.querySelector(".md-label")?.textContent, "체크리스트");
  assert.equal(typeof h.document.defaultView.__EASYOFFICE_NATIVE_MENU__, "function");
});

test("toggling checklist, clicking checkbox, Ctrl+Alt+Enter, Tab nesting, Undo/Redo, and save/open payload", async () => {
  const h = await createRunHarness([p("첫 번째 점검 항목"), p("두 번째 점검 항목")], {
    search: "?module=DOC&win=w-doc-checklist-ops",
  });
  const [first, second] = h.blocks;
  await caretIn(h, first);

  const btn = h.document.querySelector('#icon-toolbar .tb-btn[data-studio-cmd="para-checklist"]');
  h.userClick(btn);
  await flushAsyncTurns(10);

  const ul = h.editor.querySelector('ul.eo-task-list[data-eo-list="task"]');
  assert.ok(ul, "paragraph wrapped into task list ul");
  const li = ul.childNodes[0];
  assert.ok(li && li.tagName === "LI", "task list item created");
  assert.equal(li.dataset.eoChecked, "false");
  assert.equal(btn.getAttribute("aria-pressed"), "true");

  // Toggle checked via standard checklist shortcut Ctrl+Alt+Enter
  await caretIn(h, li);
  const notPrevented = h.editor.dispatchEvent({
    type: "keydown",
    key: "Enter",
    ctrlKey: true,
    altKey: true,
    shiftKey: false,
    metaKey: false,
  });
  await flushAsyncTurns(10);
  assert.equal(notPrevented, false);
  assert.equal(li.dataset.eoChecked, "true", "Ctrl+Alt+Enter toggles checked state on");

  // Nest via Tab
  h.editor.dispatchEvent({
    type: "keydown",
    key: "Tab",
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
  });
  await flushAsyncTurns(10);
  assert.equal(li.dataset.eoListLevel, "1", "Tab indents task item to level 1");

  // Save payload captures list="task", checked=true, listLevel=1
  const savePayload = h.host.__EASYOFFICE_SAVE_PAYLOAD__;
  const blocks = savePayload.collectDocBlocks(h.editor, {});
  assert.equal(blocks.length, 2);
  assert.equal(blocks[0].list, "task");
  assert.equal(blocks[0].checked, true);
  assert.equal(blocks[0].listLevel, 1);
  assert.equal(blocks[0].runs[0].text, "첫 번째 점검 항목");

  // Clicking on the text area (past 22px gutter) leaves checked state untouched
  h.editor.dispatchEvent({ type: "click", target: li, clientX: 48, clientY: 8 });
  await flushAsyncTurns(10);
  assert.equal(li.dataset.eoChecked, "true", "clicking text area does not toggle checkbox");

  // Click in left checkbox gutter toggles unchecked
  h.editor.dispatchEvent({ type: "click", target: li, clientX: 6, clientY: 8 });
  await flushAsyncTurns(10);
  assert.equal(li.dataset.eoChecked, "false", "clicking checkbox gutter unchecks item");

  // Undo restores checked=true
  h.document.defaultView.__EASYOFFICE_NATIVE_MENU__("edit:undo");
  await flushAsyncTurns(10);
  const restoredLi = h.editor.querySelector('ul[data-eo-list="task"] > li');
  assert.equal(restoredLi?.dataset.eoChecked, "true", "undo restores checked=true");

  // Plain bullet paragraph adjacent to checklist does NOT merge into checklist ul
  const liveSecond = h.editor.querySelector("p");
  await caretIn(h, liveSecond);
  h.document.querySelector('.md-item[data-studio-menu-cmd="para-bullet"]').click();
  await flushAsyncTurns(10);
  const ulsBefore = h.editor.querySelectorAll("ul");
  assert.equal(ulsBefore.length, 2, "task ul and plain bullet ul stay separate");
  assert.equal(ulsBefore[0].dataset.eoList, "task");
  assert.equal(ulsBefore[1].dataset.eoList, undefined);

  // Switching task item to plain bullet removes data-eo-list="task" and data-eo-checked and joins plain bullet ul
  await caretIn(h, ulsBefore[0].querySelector("li"));
  h.document.querySelector('.md-item[data-studio-menu-cmd="para-bullet"]').click();
  await flushAsyncTurns(10);
  const ulsAfter = h.editor.querySelectorAll("ul");
  assert.equal(ulsAfter.length, 1, "merged into one plain bullet ul");
  assert.equal(ulsAfter[0].dataset.eoList, undefined);
  assert.equal(ulsAfter[0].querySelector("li")?.dataset.eoChecked, undefined);
});
