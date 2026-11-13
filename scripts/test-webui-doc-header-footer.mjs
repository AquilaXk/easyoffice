import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// The 머리말 and 꼬리말 commands (one button, menu item and palette entry each, like the reference toolbar): each puts one
// block in the body, the header first and the footer last; running it again updates that block instead of adding one

const toolbarButton = (document, cmd) => document.querySelector(`#icon-toolbar [data-studio-cmd="${cmd}"]`);

async function openDoc(win) {
  const harness = await createWorkspaceHarness({ search: `?module=DOC&win=${win}` });
  await flushAsyncTurns(6);
  const editor = harness.document.getElementById("doc-editor");
  editor.innerHTML = "<p>첫 문단</p><p>둘째 문단</p>";
  return { ...harness, editor };
}

test("the footer button adds one closing block after the body and a second press updates it", async () => {
  const { document, editor, userClick } = await openDoc("w-header-footer-buttons");
  editor.innerHTML = '<header class="eo-hwp-header"><p>문서 머리말 (표준 양식)</p></header><p>첫 문단</p><p>둘째 문단</p>';
  userClick(toolbarButton(document, "mod-footer"));
  await flushAsyncTurns();
  editor.querySelector(".eo-hwp-footer p").textContent = "고친 글";
  userClick(toolbarButton(document, "mod-footer"));
  await flushAsyncTurns();
  assert.equal(editor.querySelectorAll(".eo-hwp-footer").length, 1);
  const blocks = [...editor.children].filter((el) => !el.classList.contains("doc-page-layer"));
  assert.ok(blocks[0].classList.contains("eo-hwp-header"), "the header opens the document");
  assert.ok(blocks.at(-1).classList.contains("eo-hwp-footer"), "the footer closes the document");
  assert.equal(blocks.at(-1).textContent.trim(), "문서 꼬리말 (표준 양식)", "the second press rewrites the block");
});

test("the footer menu item runs the same command and one undo takes the footer back out", async () => {
  const { document, editor, sandbox, userClick } = await openDoc("w-header-footer-menu");
  assert.equal(studioItem(document, "mod-footer").querySelector(".md-label").textContent, "꼬리말");
  userClick(studioItem(document, "mod-footer"));
  await flushAsyncTurns();
  assert.equal(editor.querySelectorAll(".eo-hwp-footer").length, 1);
  assert.equal(document.getElementById("status-command-feedback").textContent, "꼬리말 적용 완료");
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(editor.querySelectorAll(".eo-hwp-footer").length, 0, "a single undo step");
  assert.equal(editor.querySelectorAll("p").length, 2, "the body is back as it was");
});

test("the command palette lists 머리말 and 꼬리말 in the document window and runs them", async () => {
  const { document, editor, userClick } = await openDoc("w-header-footer-palette");
  userClick(document.getElementById("btn-command-palette"));
  const search = document.getElementById("cmd-search-input");
  search.value = "꼬리말";
  search.dispatchEvent({ type: "input", target: search });
  await flushAsyncTurns();
  const rows = [...document.querySelectorAll("#cmd-palette-list li")].filter((li) => li.textContent.includes("꼬리말"));
  assert.equal(rows.length, 1);
  userClick(rows[0]);
  await flushAsyncTurns();
  assert.equal(editor.querySelectorAll(".eo-hwp-footer").length, 1);
});

test("the footer command is a document command: the other windows have neither the button nor an enabled menu item", async () => {
  const { document } = await openDoc("w-header-footer-modules");
  for (const mod of ["SHEET", "SLIDE", "PDF"]) {
    switchModule(document, mod);
    await flushAsyncTurns(6);
    assert.equal(studioItem(document, "mod-footer").getAttribute("aria-disabled"), "true", `${mod} menu item`);
    assert.equal(toolbarButton(document, "mod-footer").hidden, true, `${mod} button`);
  }
});
