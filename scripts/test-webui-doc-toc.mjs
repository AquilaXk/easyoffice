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

test("the save payload gives the block its roles, the headings their bookmarks and the entries links to them; a file with them opens as a block again that still goes to the headings", async () => {
  const h = await createRunHarness([heading(1, "개요"), heading(2, "배경"), p("본문")]);
  toc(h).applyToc(h.editor, 3);
  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  assert.deepEqual(blocks.map((b) => [b.runs.map((r) => r.text).join(""), b.toc, b.heading, b.bookmark, b.runs[0].link]), [
    ["목차", 0, undefined, undefined, undefined], ["개요", 1, undefined, undefined, "#_Toc1"], ["배경", 2, undefined, undefined, "#_Toc2"],
    ["개요", undefined, 1, 1, undefined], ["배경", undefined, 2, 2, undefined], ["본문", undefined, undefined, undefined, undefined],
  ]);

  const opened = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  await pickFile(opened.document, makeFixtureFile("목차.docx", { blocks }));
  const editor = byId(opened, "doc-editor");
  const nav = editor.querySelector("nav.eo-doc-toc");
  assert.ok(nav);
  assert.equal(nav.getAttribute("contenteditable"), "false");
  assert.deepEqual(Array.from(nav.querySelectorAll("p[data-eo-toc]"), (e) => [Number(e.dataset.eoToc), e.textContent]), [[0, "목차"], [1, "개요"], [2, "배경"]]);
  assert.deepEqual(Array.from(nav.querySelectorAll("a"), (a) => a.getAttribute("href")), ["#eo-h-1", "#eo-h-2"]);
  assert.deepEqual(Array.from(editor.querySelectorAll("h1, h2"), (el) => el.id), ["eo-h-1", "eo-h-2"]);
  assert.equal(editor.querySelectorAll("nav.eo-doc-toc").length, 1);
  const api = opened.host.__EASYOFFICE_DOC_TOC__;
  const entry = nav.querySelectorAll("p[data-eo-toc]")[2];
  assert.equal(api.headingFor(editor, entry), editor.querySelector("h2"), "the entry goes to the heading by its bookmark");
  const again = opened.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor);
  assert.deepEqual(again, blocks, "saving the opened file gives the same blocks");
  const res = api.updateToc(editor);
  assert.equal(res.changed, true);
  assert.equal(editor.querySelectorAll("nav.eo-doc-toc").length, 1, "update replaces the block that was opened");
  assert.equal(res.toc.dataset.tocLevels, "2", "a file keeps no setting: the deepest entry gives the levels");
});

test("a bookmark number past the engine's range, a foreign link and a heading without a bookmark are not sent", async () => {
  const h = await createRunHarness([heading(1, "개요")]);
  toc(h).applyToc(h.editor, 3);
  const head = h.editor.querySelector("h1");
  head.setAttribute("id", "eo-h-4294967296");
  const entry = navOf(h).querySelectorAll("p")[1];
  entry.querySelector("a").setAttribute("href", "#eo-h-4294967296");
  let blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  assert.equal(blocks[1].runs[0].link, undefined);
  assert.equal(blocks[2].bookmark, undefined);
  head.setAttribute("id", "eo-h-0");
  entry.querySelector("a").setAttribute("href", "https://example.com/");
  blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  assert.equal(blocks[1].runs[0].link, undefined, "an entry only links to a bookmark");
  assert.equal(blocks[2].bookmark, undefined);
  head.setAttribute("id", "eo-h-4294967295");
  assert.equal(h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor)[2].bookmark, 4294967295);
});

// A page layout that puts four lines on a page: a block is one line, the table of contents is one line per entry.
// It counts its passes so a test can see the numbers were measured again after the block was in place.
function fakePages(h, { active = true, linesPerPage = 4 } = {}) {
  const api = {
    passes: 0,
    isActive: () => active,
    layoutNow() {
      api.passes += 1;
    },
    pageCount: () => (active ? 99 : 0),
    headingPages() {
      let line = 0;
      const out = [];
      for (const el of h.editor.childNodes) {
        if (el.tagName === "NAV") line += el.querySelectorAll("p").length;
        else {
          if (/^H[1-6]$/.test(el.tagName)) out.push({ el, level: Number(el.tagName[1]), page: Math.floor(line / linesPerPage) + 1 });
          line += 1;
        }
      }
      return out;
    },
  };
  h.host.__EASYOFFICE_DOC_PAGES__ = api;
  return api;
}
const pageNumbers = (h) => entries(h).slice(1).map((e) => e.dataset.eoTocPage);
const sixHeadings = () => [heading(1, "가"), p("a"), heading(2, "나"), p("b"), heading(1, "다"), p("c"), heading(3, "라"), p("d")];

test("with page numbers each entry shows the page its heading is on once the block is in place, measured until the numbers settle", async () => {
  const h = await createRunHarness(sixHeadings());
  const api = fakePages(h);
  toc(h).applyToc(h.editor, 3, null, { pages: true, dots: true });
  // The block (title + 4 entries) opens the document, so every heading is 5 lines further down than without it.
  const expected = api.headingPages().map((x) => String(x.page));
  assert.deepEqual(pageNumbers(h), expected);
  assert.deepEqual(expected, ["2", "2", "3", "3"]);
  assert.ok(api.passes >= 2, "measured before and after the block went in");
  assert.ok(entries(h).slice(1).every((e) => e.dataset.eoTocDots === "true"));
  assert.equal(navOf(h).dataset.tocPages, "true");
  assert.equal(entries(h)[0].dataset.eoTocPage, undefined, "the title has no number");
});

test("a block without page numbers, without a layout or with the layout off has none, and no error", async () => {
  const off = await createRunHarness(sixHeadings());
  fakePages(off);
  assert.equal(toc(off).applyToc(off.editor, 3, null, { pages: false }).changed, true);
  assert.deepEqual(pageNumbers(off), [undefined, undefined, undefined, undefined]);
  const none = await createRunHarness(sixHeadings());
  delete none.host.__EASYOFFICE_DOC_PAGES__;
  assert.equal(toc(none).applyToc(none.editor, 3, null, { pages: true }).changed, true);
  assert.deepEqual(pageNumbers(none), [undefined, undefined, undefined, undefined]);
  const inactive = await createRunHarness(sixHeadings());
  fakePages(inactive, { active: false });
  assert.equal(toc(inactive).applyToc(inactive.editor, 3, null, { pages: true }).changed, true);
  assert.deepEqual(pageNumbers(inactive), [undefined, undefined, undefined, undefined]);
});

test("updating measures the pages again with the block's own settings, and a stable layout changes nothing", async () => {
  const h = await createRunHarness(sixHeadings());
  fakePages(h);
  toc(h).applyToc(h.editor, 3, null, { pages: true, dots: false });
  const before = pageNumbers(h);
  toc(h).updateToc(h.editor);
  assert.deepEqual(pageNumbers(h), before, "idempotent");
  assert.ok(entries(h).slice(1).every((e) => e.dataset.eoTocDots === "false"), "the fill stays blank");
  for (let i = 0; i < 4; i += 1) h.editor.insertBefore(Object.assign(h.document.createElement("p"), { textContent: "더" }), h.editor.childNodes[1]);
  toc(h).updateToc(h.editor);
  assert.notDeepEqual(pageNumbers(h), before, "the pages moved");
  assert.deepEqual(pageNumbers(h), fakePages(h) && h.host.__EASYOFFICE_DOC_PAGES__.headingPages().map((x) => String(x.page)));
});

test("the dialog has 쪽 번호 넣기 (on) and 채울 모양 (점선, off while the numbers are off) and passes them to the command", async () => {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "DOC");
  await flushAsyncTurns(10);
  const editor = byId(h, "doc-editor");
  editor.replaceChildren();
  for (const [level, text] of [[1, "개요"], [2, "배경"]]) editor.appendChild(Object.assign(h.document.createElement("h" + level), { textContent: text }));
  const api = fakePages({ editor, host: h.host });
  h.userClick(commandItem(h.document, "doc-mod-toc"));
  await flushAsyncTurns(10);
  const dialog = byId(h, "doc-toc-dialog");
  assert.equal(byId(h, "doc-toc-pages").checked, true);
  assert.equal(byId(h, "doc-toc-fill").value, "dot");
  assert.deepEqual(Array.from(byId(h, "doc-toc-fill").querySelectorAll("option"), (o) => o.textContent), ["점선", "없음"]);
  assert.deepEqual(Array.from(dialog.querySelectorAll(".dialog-label, .dialog-checkbox"), (el) => el.textContent.trim()), ["포함할 개요", "쪽 번호 넣기", "채울 모양"]);
  byId(h, "doc-toc-pages").checked = false;
  byId(h, "doc-toc-pages").dispatchEvent({ type: "change", target: byId(h, "doc-toc-pages") });
  assert.equal(byId(h, "doc-toc-fill").disabled, true);
  byId(h, "doc-toc-pages").checked = true;
  byId(h, "doc-toc-pages").dispatchEvent({ type: "change", target: byId(h, "doc-toc-pages") });
  assert.equal(byId(h, "doc-toc-fill").disabled, false);
  byId(h, "doc-toc-fill").value = "none";
  byId(h, "btn-doc-toc-apply").click();
  await flushAsyncTurns(10);
  assert.ok(api.passes >= 1);
  const nav = editor.querySelector("nav.eo-doc-toc");
  assert.deepEqual(Array.from(nav.querySelectorAll("p[data-eo-toc-page]"), (e) => e.dataset.eoTocDots), ["false", "false"]);
  // Open again: the dialog shows what the block has.
  h.userClick(commandItem(h.document, "doc-mod-toc"));
  await flushAsyncTurns(10);
  assert.equal(byId(h, "doc-toc-pages").checked, true);
  assert.equal(byId(h, "doc-toc-fill").value, "none");
});

test("the save payload carries each entry's page and fill, and a file with them opens with the numbers and keeps them on update", async () => {
  const h = await createRunHarness(sixHeadings());
  fakePages(h);
  toc(h).applyToc(h.editor, 2, null, { pages: true, dots: true });
  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  const entryBlocks = blocks.filter((b) => b.toc > 0);
  assert.deepEqual(entryBlocks.map((b) => [b.runs[0].text, b.tocPage, b.tocDots]), [["가", 2, true], ["나", 2, true], ["다", 3, true]]);
  assert.equal(blocks[0].tocPage, undefined, "the title has no page");
  assert.equal(blocks.at(-1).tocPage, undefined);

  const opened = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  await pickFile(opened.document, makeFixtureFile("목차.docx", { blocks }));
  const editor = byId(opened, "doc-editor");
  const shown = Array.from(editor.querySelectorAll("nav.eo-doc-toc p[data-eo-toc-page]"), (e) => [e.textContent, e.dataset.eoTocPage, e.dataset.eoTocDots]);
  assert.deepEqual(shown, [["가", "2", "true"], ["나", "2", "true"], ["다", "3", "true"]]);
  assert.deepEqual(opened.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor), blocks);
  fakePages({ editor, host: opened.host });
  const res = opened.host.__EASYOFFICE_DOC_TOC__.updateToc(editor);
  assert.equal(res.toc.dataset.tocPages, "true", "a file keeps no setting: the numbers it shows decide");
  assert.ok(Array.from(res.toc.querySelectorAll("p[data-eo-toc-page]")).length >= 3);
});
