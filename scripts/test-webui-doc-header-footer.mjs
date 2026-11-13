import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// The 머리말 and 꼬리말 commands (PD-211): each enters the page's header or footer for editing in place (made on the
// first use, one undo step), like the reference editor. The header and footer are sections of the editor that are
// drawn on every page and saved as parts of their own, never as body paragraphs.

const toolbarButton = (document, cmd) => document.querySelector(`#icon-toolbar [data-studio-cmd="${cmd}"]`);
const section = (editor, kind) => editor.querySelector(`:scope > section[data-eo-hf="${kind}"]`);

async function openDoc(win) {
  const harness = await createWorkspaceHarness({ search: `?module=DOC&win=${win}` });
  await flushAsyncTurns(6);
  const editor = harness.document.getElementById("doc-editor");
  editor.innerHTML = "<p>첫 문단</p><p>둘째 문단</p>";
  harness.host.__EASYOFFICE_DOC_PAGES__.layoutNow();
  return { ...harness, editor };
}

test("the header button makes the header section once and edits it in place; the footer is a section of its own", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-buttons");
  userClick(toolbarButton(document, "mod-header"));
  await flushAsyncTurns();
  assert.equal(editor.querySelectorAll(':scope > section[data-eo-hf="header"]').length, 1);
  assert.equal(editor.dataset.eoHfEdit, "header", "the editor is in header edit state");
  assert.equal(editor.getAttribute("contenteditable"), "false", "the body is not editable meanwhile");
  assert.equal(section(editor, "header").getAttribute("contenteditable"), "true");
  assert.equal(document.querySelector("#icon-toolbar .tb-headerfooter-group").hidden, false);
  assert.match(document.querySelector("#icon-toolbar .tb-hf-label").textContent, /머리말/);
  assert.equal(editor.querySelectorAll("p").length, 3, "the header's paragraph is not a body paragraph of the flow");
  assert.equal(document.getElementById("status-command-feedback").textContent, "머리말 편집 중");
});

test("closing keeps a header with text and drops an empty one; the page layer draws the kept one on every sheet", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-close");
  const frame = host.__EASYOFFICE_DOC_PAGE_FRAME__;
  userClick(toolbarButton(document, "mod-header"));
  await flushAsyncTurns();
  document.querySelector('#icon-toolbar [data-hf-action="close"]').click();
  await flushAsyncTurns();
  assert.equal(section(editor, "header"), null, "an empty header is not kept");
  assert.equal(editor.getAttribute("contenteditable"), "true");
  userClick(toolbarButton(document, "mod-footer"));
  await flushAsyncTurns();
  section(editor, "footer").querySelector("p").textContent = "꼬리말 글";
  frame.endEdit();
  await flushAsyncTurns();
  assert.equal(frame.has("footer"), true);
  // The page layer is made by the page layout (a real layout in the browser); two sheets stand in for it here.
  const layer = document.createElement("footer");
  layer.className = "doc-page-layer";
  for (let i = 0; i < 2; i += 1) layer.appendChild(Object.assign(document.createElement("i"), { className: "doc-page-sheet" }));
  editor.appendChild(layer);
  frame.paint();
  const sheets = layer.querySelectorAll(".doc-page-sheet");
  assert.equal(sheets.length, 2);
  for (const sheet of sheets) {
    assert.equal(sheet.querySelector('.doc-page-hf[data-hf="footer"]').textContent, "꼬리말 글");
  }
  assert.equal(layer.hasAttribute("data-hf-footer"), true);
  assert.equal(sheets[0].querySelector('.doc-page-hf[data-hf="header"]'), null, "no header, no header copy");
});

test("the menu item runs the same command and one undo takes the header section back out", async () => {
  const { document, editor, host, sandbox, userClick } = await openDoc("w-hf-menu");
  assert.equal(studioItem(document, "mod-footer").querySelector(".md-label").textContent, "꼬리말");
  userClick(studioItem(document, "mod-footer"));
  await flushAsyncTurns();
  assert.ok(section(editor, "footer"));
  host.__EASYOFFICE_DOC_PAGE_FRAME__.endEdit();
  await flushAsyncTurns();
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(section(editor, "footer"), null, "a single undo step");
  assert.equal(editor.querySelectorAll("p").length, 2, "the body is back as it was");
});

test("a page-number field is inserted into the section being edited and saves as a field run beside the body", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-field");
  userClick(toolbarButton(document, "mod-footer"));
  await flushAsyncTurns();
  section(editor, "footer").querySelector("p").textContent = "쪽 ";
  document.querySelector('#icon-toolbar [data-hf-action="page-number"]').click();
  await flushAsyncTurns();
  assert.equal(section(editor, "footer").querySelectorAll(".eo-page-field").length, 1);
  host.__EASYOFFICE_DOC_PAGE_FRAME__.endEdit();
  const payload = host.__EASYOFFICE_SAVE_PAYLOAD__;
  const blocks = payload.collectDocBlocks(editor);
  assert.deepEqual(blocks.map((b) => b.runs.map((r) => r.text).join("")), ["첫 문단", "둘째 문단"], "the footer is not body text");
  const frame = payload.collectPageFrame(editor);
  assert.deepEqual(frame.footer[0].runs, [{ text: "쪽 " }, { text: "", field: "page" }]);
  assert.equal("header" in frame, false);
});

test("deleting removes the section as one undo step", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-delete");
  userClick(toolbarButton(document, "mod-header"));
  await flushAsyncTurns();
  section(editor, "header").querySelector("p").textContent = "지울 글";
  document.querySelector('#icon-toolbar [data-hf-action="delete"]').click();
  await flushAsyncTurns();
  assert.equal(section(editor, "header"), null);
  assert.equal(host.__EASYOFFICE_DOC_PAGE_FRAME__.isEditing(), null);
  assert.equal(document.querySelector("#icon-toolbar .tb-headerfooter-group").hidden, true);
});

test("the command palette lists 머리말 and 꼬리말 in the document window and runs them", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-palette");
  userClick(document.getElementById("btn-command-palette"));
  const search = document.getElementById("cmd-search-input");
  search.value = "꼬리말";
  search.dispatchEvent({ type: "input", target: search });
  await flushAsyncTurns();
  const rows = [...document.querySelectorAll("#cmd-palette-list li")].filter((li) => li.textContent.includes("꼬리말"));
  assert.equal(rows.length, 1);
  userClick(rows[0]);
  await flushAsyncTurns();
  assert.ok(section(editor, "footer"));
  assert.equal(editor.dataset.eoHfEdit, "footer");
});

test("a document opened with a header and footer shows them as sections and no old fake block is added", async () => {
  const { editor, host } = await openDoc("w-hf-open");
  const frame = host.__EASYOFFICE_DOC_PAGE_FRAME__;
  editor.innerHTML = '<p>본문</p><section class="eo-page-frame" data-eo-hf="header"><p>머리</p></section>';
  await flushAsyncTurns();
  assert.equal(frame.has("header"), true);
  assert.equal(frame.has("footer"), false);
  assert.equal(editor.querySelector(".eo-hwp-header"), null);
});

test("the footer command is a document command: the other windows have neither the button nor an enabled menu item", async () => {
  const { document } = await openDoc("w-hf-modules");
  for (const mod of ["SHEET", "SLIDE", "PDF"]) {
    switchModule(document, mod);
    await flushAsyncTurns(6);
    assert.equal(studioItem(document, "mod-footer").getAttribute("aria-disabled"), "true", `${mod} menu item`);
    assert.equal(toolbarButton(document, "mod-footer").hidden, true, `${mod} button`);
  }
});
