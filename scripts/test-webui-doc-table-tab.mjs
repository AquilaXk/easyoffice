import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { createSelection, p, rangeOf } from "./test-support/doc-run-harness.mjs";

// Tab in a table (doc-table-tab.js) follows the reference editor: next cell at its start, previous cell at its end,
// a new row after the last cell, spans skipped, and the way out of a nested table or of the first cell.

// Like `build` of the run harness, with the `attrs` of a spec set on the element.
function build(document, parent, spec) {
  const el = document.createElement(spec.tag);
  for (const [name, value] of Object.entries(spec.attrs || {})) el.setAttribute(name, value);
  for (const child of spec.children || []) {
    if (typeof child === "string") el.appendChild(document.createTextNode(child));
    else build(document, el, child);
  }
  parent.appendChild(el);
  return el;
}

const td = (...children) => ({ tag: "td", children: children.length ? children : [{ tag: "br" }] });
const rowOf = (...cells) => ({ tag: "tr", children: cells });
const text = (value) => ({ tag: "td", children: [value] });

async function openWith(tableSpec, { before = [p("앞")], after = [p("뒤")] } = {}) {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection });
  switchModule(h.document, "DOC");
  await flushAsyncTurns(10);
  const editor = h.document.getElementById("doc-editor");
  editor.replaceChildren();
  for (const spec of before) build(h.document, editor, spec);
  const table = build(h.document, editor, { tag: "table", children: [{ tag: "tbody", children: tableSpec }] });
  table.className = "doc-inline-table";
  for (const spec of after) build(h.document, editor, spec);
  const live = () => editor.querySelector("table");
  const cell = (r, c) => live().querySelectorAll("tr")[r].children[c];
  const caretAt = (node, offset = 0) => selection.addRange(rangeOf(node, offset, node, offset));
  const tab = (shiftKey = false, extra = {}) => h.document.getElementById("doc-editor").dispatchEvent({ type: "keydown", key: "Tab", shiftKey, ...extra });
  const caret = () => {
    const r = selection.getRangeAt();
    return { node: r.startContainer, offset: r.startOffset };
  };
  const undo = async () => {
    h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
    await flushAsyncTurns(20);
  };
  return { ...h, selection, editor, table, cell, caretAt, tab, caret, undo, rows: () => live().querySelectorAll("tr") };
}

const attr = (name, value) => ({ [name]: value });
const spanned = (value, attrs) => ({ tag: "td", attrs, children: [value] });

test("Tab goes to the start of the next cell and Shift+Tab to the end of the previous one", async () => {
  const t = await openWith([rowOf(text("가나"), text("다라")), rowOf(text("마바"), text("사아"))]);
  t.caretAt(t.cell(0, 0), 0);
  assert.equal(t.tab(), false, "the key is taken");
  assert.deepEqual(t.caret(), { node: t.cell(0, 1), offset: 0 }, "next cell, at its start");
  t.tab();
  assert.deepEqual(t.caret(), { node: t.cell(1, 0), offset: 0 }, "the next row starts after the last cell of the row");
  t.tab(true);
  assert.deepEqual(t.caret(), { node: t.cell(0, 1), offset: 1 }, "previous cell, at its end (one text node in the cell)");
});

test("the end of an empty cell is before its line break, and a block in a cell is entered", async () => {
  const t = await openWith([rowOf(td(), { tag: "td", children: [{ tag: "p", children: ["끝"] }] }, td())]);
  t.caretAt(t.cell(0, 2), 0);
  t.tab(true);
  assert.deepEqual(t.caret(), { node: t.cell(0, 1).children[0], offset: 1 }, "the end of the paragraph in the cell");
  t.tab(true);
  assert.deepEqual(t.caret(), { node: t.cell(0, 0), offset: 0 }, "an empty cell has the break after the caret");
  t.tab();
  assert.equal(t.caret().node, t.cell(0, 1).children[0], "Tab enters the block of the next cell");
});

test("a cell covered by a span is not a stop: the cell that owns it is, once", async () => {
  const t = await openWith([
    rowOf(spanned("A", attr("colspan", "2")), spanned("B", attr("rowspan", "2"))),
    rowOf(text("C"), text("D")),
  ]);
  // Reading order: A, B, C, D. The slot under A is C (row 1, col 0); B covers (0..1, col 2).
  t.caretAt(t.cell(0, 0), 0);
  t.tab();
  assert.equal(t.caret().node, t.cell(0, 1), "A -> B");
  t.tab();
  assert.equal(t.caret().node, t.cell(1, 0), "B -> C, not a covered slot");
  t.tab();
  assert.equal(t.caret().node, t.cell(1, 1), "C -> D");
  t.tab(true);
  t.tab(true);
  assert.equal(t.caret().node, t.cell(0, 1), "D -> C -> B going back");
});

test("Tab in the last cell adds a row after the last row of that cell, as one undo step, and goes to its first cell", async () => {
  const t = await openWith([rowOf(text("a"), text("b")), rowOf(text("c"), text("d"))]);
  t.caretAt(t.cell(1, 1), 0);
  assert.equal(t.tab(), false);
  assert.equal(t.rows().length, 3, "a row was added");
  assert.deepEqual(t.caret(), { node: t.rows()[2].children[0], offset: 0 }, "the new row's first cell, at its start");
  assert.equal(t.rows()[2].children.length, 2);
  await t.undo();
  assert.equal(t.rows().length, 2, "one undo takes the row back");
});

test("the row goes after the last row that the last cell spans, not after its first row", async () => {
  const t = await openWith([rowOf(text("a"), spanned("b", attr("rowspan", "2"))), rowOf()]);
  t.caretAt(t.cell(0, 1), 0);
  t.tab();
  assert.equal(t.rows().length, 3, "after the second row, which the cell covers");
  assert.equal(t.cell(0, 1).getAttribute("rowspan"), "2", "the span that ends at the old last row stays");
  assert.deepEqual(t.caret(), { node: t.rows()[2].children[0], offset: 0 });
});

test("Tab in the last cell of a table inside a cell adds no row and leaves the outer table to the block after it", async () => {
  const inner = { tag: "table", children: [{ tag: "tbody", children: [rowOf(text("x"), text("y"))] }] };
  const t = await openWith([rowOf({ tag: "td", children: [inner] }, text("z"))]);
  const innerCell = t.table.querySelectorAll("table")[0].querySelectorAll("td")[1];
  t.caretAt(innerCell, 0);
  t.tab();
  assert.equal(t.table.querySelectorAll("tr").length, 2, "only the inner row exists; the outer table got no row");
  assert.deepEqual(t.caret(), { node: t.editor.children[2], offset: 0 }, "the start of the block after the outer table");
});

test("Shift+Tab in the first cell goes to the block before the table, and stays when there is none", async () => {
  const t = await openWith([rowOf(text("a"), text("b"))]);
  t.caretAt(t.cell(0, 0), 0);
  t.tab(true);
  assert.deepEqual(t.caret(), { node: t.editor.children[0], offset: 1 }, "the end of the block before the table");
  const first = await openWith([rowOf(text("a"), text("b"))], { before: [] });
  first.caretAt(first.cell(0, 0), 0);
  first.tab(true);
  assert.deepEqual(first.caret(), { node: first.cell(0, 0), offset: 0 }, "nothing before the table: the caret stays");
});

test("the way out skips pictures and the page frame, and Tab with a modifier or in a composition is not taken", async () => {
  const inner = { tag: "table", children: [{ tag: "tbody", children: [rowOf(text("x"))] }] };
  const t = await openWith([rowOf({ tag: "td", children: [inner] })], {
    before: [],
    after: [{ tag: "img" }, { tag: "section", attrs: { "data-eo-hf": "header" }, children: [{ tag: "p", children: ["머리말"] }] }, p("본문")],
  });
  const innerCell = t.editor.querySelectorAll("table")[1].querySelector("td");
  t.caretAt(innerCell, 0);
  assert.equal(t.tab(false, { isComposing: true }), true, "a composition keeps its key");
  assert.equal(t.tab(false, { ctrlKey: true }), true);
  assert.equal(t.tab(false, { altKey: true }), true);
  t.tab();
  assert.deepEqual(t.caret(), { node: t.editor.children[3], offset: 0 }, "the first block of the body flow after the table");
  assert.equal(t.editor.children[3].textContent, "본문");
});

test("Tab outside a table is left to the paragraph editor without touching the table", async () => {
  const t = await openWith([rowOf(text("a"))]);
  t.caretAt(t.editor.children[0], 0);
  assert.equal(t.tab(true), true, "Shift+Tab outside a list or table is untouched");
  assert.equal(t.tab(), false, "plain Tab in a body paragraph inserts a tab character");
  assert.equal(t.editor.children[0].textContent, "\t앞");
  assert.equal(t.rows().length, 1);
});
