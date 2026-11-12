import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { build, createSelection, p, rangeOf, textNodeOf } from "./test-support/doc-run-harness.mjs";

// 표 만들기 (doc-table.js): the menu command puts a table at the caret, else at the end, also in a blank document.

const cellCounts = (table) => Array.from(table.querySelectorAll("tr"), (tr) => tr.querySelectorAll("td").length);
const tags = (editor) => Array.from(editor.children, (c) => c.tagName).join();

async function makeTable(h) {
  h.userClick(studioItem(h.document, "insert-table"));
  await flushAsyncTurns(10);
}

test("표 만들기 in a blank document ends the document with a 3x3 table and a paragraph to go on in", async () => {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "DOC");
  await flushAsyncTurns(10);
  const editor = h.document.getElementById("doc-editor");
  assert.equal(editor.querySelector("table"), null);
  const before = editor.children.length;
  await makeTable(h);
  const tables = editor.querySelectorAll("table.doc-inline-table");
  assert.equal(tables.length, 1);
  assert.deepEqual(cellCounts(tables[0]), [3, 3, 3]);
  assert.equal(editor.children.length, before + 2, tags(editor));
  assert.equal(editor.children[before].tagName, "TABLE", tags(editor));
  assert.equal(editor.children[before + 1].tagName, "P", tags(editor));
});

test("the table goes after the block the caret is in, and a second one is a second table that the save payload carries", async () => {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection });
  switchModule(h.document, "DOC");
  await flushAsyncTurns(10);
  const editor = h.document.getElementById("doc-editor");
  editor.replaceChildren();
  const [first] = [p("가"), p("나")].map((spec) => build(h.document, editor, spec));
  selection.addRange(rangeOf(textNodeOf(first), 1, textNodeOf(first), 1));
  await makeTable(h);
  assert.equal(tags(editor), "P,TABLE,P");
  assert.equal(editor.children[2].textContent, "나");
  await makeTable(h);
  assert.equal(editor.querySelectorAll("table").length, 2);
  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor);
  assert.equal(blocks.filter((b) => b.type === "table").length, 2);
});
