import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { createRunHarness, p, rangeOf, textNodeOf } from "./test-support/doc-run-harness.mjs";

// Document table of contents (doc-toc.js): the heading hierarchy, an idempotent update, the links to the
// headings, the limits, the dialog and menu items, and the save payload and reopened file that carry it.

const toc = (h) => h.host.__EASYOFFICE_DOC_TOC__;
const byId = (h, id) => h.document.getElementById(id);
const heading = (level, text) => ({ tag: "h" + level, children: [text] });
const navOf = (h) => h.editor.querySelector("nav.eo-doc-toc");
const entries = (h) => Array.from(navOf(h).querySelectorAll("p[data-eo-toc]"));
const shape = (h) => entries(h).map((e) => [Number(e.dataset.eoToc), e.textContent]);

test("entries keep their own heading level, skipped levels stay skipped, blank headings and deeper levels are left out", async () => {
  const h = await createRunHarness([heading(1, "개요"), heading(3, "건너뛴 세부"), heading(2, "배경"), heading(1, "   "), heading(4, "너무 깊음"), p("본문"), heading(1, "결론")]);
  const res = toc(h).applyToc(h.editor, 3);
  assert.equal(res.changed, true);
  assert.equal(res.count, 4);
  assert.deepEqual(shape(h), [[0, "목차"], [1, "개요"], [3, "건너뛴 세부"], [2, "배경"], [1, "결론"]]);
  assert.equal(navOf(h).dataset.tocLevels, "3");
  assert.equal(h.editor.childNodes[0], navOf(h), "without a caret the block opens the document");
  toc(h).applyToc(h.editor, 1);
  assert.deepEqual(shape(h), [[0, "목차"], [1, "개요"], [1, "결론"]]);
  toc(h).applyToc(h.editor, 9);
  assert.equal(navOf(h).dataset.tocLevels, "3", "a level outside 1-3 is the default");
});

test("the block goes before the block the caret is in, and a document without headings is refused", async () => {
  const h = await createRunHarness([p("머리말"), heading(1, "개요"), p("본문")]);
  const caret = h.editor.childNodes[2];
  toc(h).applyToc(h.editor, 3, rangeOf(textNodeOf(caret), 0, textNodeOf(caret), 0));
  assert.equal(h.editor.childNodes[2], navOf(h));
  const none = await createRunHarness([p("본문만")]);
  assert.deepEqual(toc(none).applyToc(none.editor, 3), { changed: false, error: "제목 없음" });
  assert.equal(navOf(none), null);
});

test("updating rebuilds the block in place: a second pass changes nothing, a new heading appears, and the title text stays", async () => {
  const h = await createRunHarness([heading(1, "개요"), heading(2, "배경")]);
  toc(h).applyToc(h.editor, 2);
  navOf(h).querySelector('p[data-eo-toc="0"]').textContent = "차례";
  const snapshot = () => JSON.stringify([shape(h), entries(h).map((e) => e.querySelector("a")?.getAttribute("href")), navOf(h).dataset.tocLevels]);
  const first = toc(h).updateToc(h.editor);
  const once = snapshot();
  toc(h).updateToc(h.editor);
  assert.equal(snapshot(), once, "idempotent");
  assert.equal(first.count, 2);
  assert.equal(h.editor.querySelectorAll("nav.eo-doc-toc").length, 1, "no second block");
  assert.equal(shape(h)[0][1], "차례");
  h.editor.appendChild(Object.assign(h.document.createElement("h2"), { textContent: "추가" }));
  toc(h).updateToc(h.editor);
  assert.deepEqual(shape(h).map(([, t]) => t), ["차례", "개요", "배경", "추가"]);
  const none = await createRunHarness([heading(1, "개요")]);
  assert.deepEqual(toc(none).updateToc(none.editor), { changed: false, error: "목차 없음" });
});

test("an entry is a link to its heading; the entry of a reopened file finds the same heading by level and text", async () => {
  const h = await createRunHarness([heading(1, "개요"), heading(2, "배경"), heading(2, "배경"), heading(1, "결론")]);
  toc(h).applyToc(h.editor, 3);
  const [, , second, third] = entries(h);
  const link = second.querySelector("a");
  const target = h.editor.querySelectorAll("h2")[0];
  assert.equal(link.getAttribute("href"), "#" + target.id);
  assert.equal(toc(h).headingFor(h.editor, second), target);
  assert.equal(toc(h).headingFor(h.editor, third), h.editor.querySelectorAll("h2")[1]);
  // The same entries without their ids, as a file gives them.
  h.editor.querySelectorAll("h1, h2").forEach((el) => el.removeAttribute("id"));
  entries(h).forEach((e) => e.querySelector("a")?.setAttribute("href", "#"));
  assert.equal(toc(h).headingFor(h.editor, second), target);
  assert.equal(toc(h).headingFor(h.editor, third), h.editor.querySelectorAll("h2")[1]);
});

test("clicking an entry scrolls to its heading and puts the caret there; the title does not move anything", async () => {
  const h = await createRunHarness([heading(1, "개요"), heading(1, "결론")]);
  toc(h).applyToc(h.editor, 3);
  const scrolled = [];
  h.editor.querySelectorAll("h1").forEach((el) => {
    el.scrollIntoView = () => scrolled.push(el.textContent);
  });
  const link = entries(h)[2].querySelector("a");
  link.dispatchEvent({ type: "click", target: link }); // the harness stubs a created anchor's click() as a download
  assert.deepEqual(scrolled, ["결론"]);
  assert.equal(h.selection.getRangeAt(0).startContainer, h.editor.querySelectorAll("h1")[1]);
  h.userClick(entries(h)[0]);
  assert.deepEqual(scrolled, ["결론"]);
});

test("a document over the heading or title limit keeps what it had", async () => {
  const { MAX_HEADINGS, MAX_TITLE_CHARS } = toc(await createRunHarness([]));
  const many = await createRunHarness(Array.from({ length: MAX_HEADINGS + 1 }, (_, i) => heading(1, "제목 " + i)));
  assert.deepEqual(toc(many).applyToc(many.editor, 3), { changed: false, error: "제목 수 초과" });
  assert.equal(navOf(many), null);
  const exact = await createRunHarness(Array.from({ length: MAX_HEADINGS }, (_, i) => heading(1, "제목 " + i)));
  assert.equal(toc(exact).applyToc(exact.editor, 3).count, MAX_HEADINGS);
  const long = await createRunHarness([heading(1, "가".repeat(MAX_TITLE_CHARS + 1))]);
  assert.deepEqual(toc(long).applyToc(long.editor, 3), { changed: false, error: "제목 길이 초과" });
  const edge = await createRunHarness([heading(1, "가".repeat(MAX_TITLE_CHARS))]);
  assert.equal(toc(edge).applyToc(edge.editor, 3).changed, true);
  edge.editor.appendChild(Object.assign(edge.document.createElement("h1"), { textContent: "가".repeat(MAX_TITLE_CHARS + 1) }));
  const before = shape(edge);
  assert.equal(toc(edge).updateToc(edge.editor).error, "제목 길이 초과");
  assert.deepEqual(shape(edge), before, "a refused update leaves the block as it was");
});

test("the menu item asks for the levels, 확인 makes the block, the update item rebuilds it", async () => {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "DOC");
  await flushAsyncTurns(10);
  const editor = byId(h, "doc-editor");
  editor.replaceChildren();
  for (const [level, text] of [[1, "개요"], [2, "배경"], [3, "세부"]]) editor.appendChild(Object.assign(h.document.createElement("h" + level), { textContent: text }));
  h.userClick(commandItem(h.document, "doc-mod-toc"));
  await flushAsyncTurns(10);
  const dialog = byId(h, "doc-toc-dialog");
  assert.equal(dialog.open, true);
  assert.equal(byId(h, "doc-toc-levels").value, "3");
  assert.deepEqual(Array.from(byId(h, "doc-toc-levels").querySelectorAll("option"), (o) => o.value), ["1", "2", "3"]);
  byId(h, "doc-toc-levels").value = "2";
  byId(h, "btn-doc-toc-apply").click();
  await flushAsyncTurns(10);
  assert.equal(dialog.open, false);
  assert.equal(editor.querySelectorAll("nav.eo-doc-toc p[data-eo-toc]").length, 3, "the title and two entries");
  editor.appendChild(Object.assign(h.document.createElement("h2"), { textContent: "추가" }));
  h.userClick(commandItem(h.document, "doc-mod-toc-update"));
  await flushAsyncTurns(10);
  assert.equal(editor.querySelectorAll("nav.eo-doc-toc").length, 1);
  assert.equal(editor.querySelectorAll("nav.eo-doc-toc p[data-eo-toc]").length, 4);
  assert.equal(byId(h, "status-command-feedback").textContent, "목차 3항목");
});

test("the dialog shows its levels and one button row, and has no explanatory sentence", async () => {
  const h = await createWorkspaceHarness();
  const dialog = byId(h, "doc-toc-dialog");
  assert.equal(dialog.className, "tool-dialog");
  assert.equal(dialog.querySelector(".dialog-title span").textContent, "목차");
  assert.equal(dialog.querySelector(".dialog-label").textContent, "포함할 개요");
  assert.deepEqual(Array.from(dialog.querySelectorAll(".dialog-footer button"), (b) => b.textContent), ["확인", "취소"]);
  assert.ok(dialog.querySelector("#btn-doc-toc-apply").className.includes("dialog-btn-primary"));
  const texts = [...dialog.querySelectorAll(".dialog-title span, .dialog-label, option, .dialog-footer button")].map((el) => el.textContent.trim());
  assert.ok(texts.every((t) => t.length <= 6 && !/[.。]$/.test(t)), texts.join("|"));
});

test("the save payload gives the block's paragraphs their role, and a file with them opens as a block again", async () => {
  const h = await createRunHarness([heading(1, "개요"), heading(2, "배경"), p("본문")]);
  toc(h).applyToc(h.editor, 3);
  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  assert.deepEqual(blocks.map((b) => [b.runs.map((r) => r.text).join(""), b.toc, b.heading]), [
    ["목차", 0, undefined], ["개요", 1, undefined], ["배경", 2, undefined], ["개요", undefined, 1], ["배경", undefined, 2], ["본문", undefined, undefined],
  ]);
  assert.ok(blocks.slice(0, 3).every((b) => b.runs.every((r) => r.link === undefined)), "an entry's link is not a stored link");

  const opened = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  await pickFile(opened.document, makeFixtureFile("목차.docx", { blocks }));
  const editor = byId(opened, "doc-editor");
  const nav = editor.querySelector("nav.eo-doc-toc");
  assert.ok(nav);
  assert.equal(nav.getAttribute("contenteditable"), "false");
  assert.deepEqual(Array.from(nav.querySelectorAll("p[data-eo-toc]"), (e) => [Number(e.dataset.eoToc), e.textContent]), [[0, "목차"], [1, "개요"], [2, "배경"]]);
  assert.equal(editor.querySelectorAll("nav.eo-doc-toc").length, 1);
  const again = opened.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor);
  assert.deepEqual(again, blocks, "saving the opened file gives the same blocks");
  const res = opened.host.__EASYOFFICE_DOC_TOC__.updateToc(editor);
  assert.equal(res.changed, true);
  assert.equal(editor.querySelectorAll("nav.eo-doc-toc").length, 1, "update replaces the block that was opened");
  assert.equal(res.toc.dataset.tocLevels, "2", "a file keeps no setting: the deepest entry gives the levels");
});
