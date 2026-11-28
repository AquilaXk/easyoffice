import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// 대/소문자 바꾸기 (Shift+F3 included) of the word processor window, 대문자 > 첫 글자만 대문자 of the Pages window, and the
// Pages window's ⇧⌘K for 주석 추가 with the copilot as a toolbar button (PD-400).

const select = async (h, range) => {
  h.selection.addRange(range);
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
};
const run = async (h, cmd) => {
  h.userClick(studioItem(h.document, cmd));
  await flushAsyncTurns(8);
};
const byId = (h, id) => h.document.getElementById(id);
const openPages = async (content) => {
  const h = await createRunHarness(content);
  h.editor.dataset.eoDocFlavor = "pages";
  h.document.defaultView.__EASYOFFICE_ACTIVATE_MODULE__("DOC");
  await flushAsyncTurns(10);
  return h;
};
const wholeOf = (h, block) => rangeOf(block.firstChild, 0, block.lastChild, block.lastChild.textContent.length);
// The editor's own children: an undo rebuilds the blocks, so the harness's `blocks` go stale.
const textOf = (h) => h.editor.childNodes.map((b) => b.textContent);

test("the conversions follow the Word program's sentence, word and toggle rules", async () => {
  const h = await createRunHarness([p("x")]);
  const { convert, freshState } = h.host.__EASYOFFICE_DOC_TEXT_CASE__;
  const as = (mode, text) => convert(mode, text, freshState(), "en");
  assert.equal(as("sentence", "hELLO wORLD. tHIS is NASA! ok? yes"), "Hello world. This is nasa! Ok? Yes");
  assert.equal(as("sentence", 'he said. "go away." now'), 'He said. "Go away." Now');
  assert.equal(as("sentence", "pi is 3.14 and more"), "Pi is 3.14 and more", "a full stop inside a number ends no sentence");
  assert.equal(as("lower", "ABC Def"), "abc def");
  assert.equal(as("upper", "abc Def ß"), "ABC DEF SS");
  assert.equal(as("title", "don't STOP the-end 2nd"), "Don't STOP The-End 2nd");
  assert.equal(as("toggle", "aBc 가나"), "AbC 가나");
  assert.equal(convert("upper", "i", freshState(), "tr"), "İ", "the language decides the dotted capital I");
  const state = freshState();
  assert.equal(convert("title", "hel", state, "en") + convert("title", "lo wor", state, "en") + convert("title", "ld", state, "en"), "Hello World", "a word split over two runs is one word");
});

test("대/소문자 바꾸기 changes the characters of the selection, keeps the run formats and is one undo step", async () => {
  const h = await createRunHarness([p("hello big WORLD. next one"), p("second paragraph")]);
  const [first] = h.blocks;
  h.runs.applyFormat(h.editor, rangeOf(first.firstChild, 6, first.firstChild, 9), { bold: true });
  await select(h, rangeOf(first.firstChild, 0, h.blocks[1].lastChild, h.blocks[1].lastChild.textContent.length));
  const saved = () => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  await run(h, "text-case-upper");
  assert.deepEqual(textOf(h), ["HELLO BIG WORLD. NEXT ONE", "SECOND PARAGRAPH"]);
  assert.ok(saved()[0].runs.some((r) => r.bold && r.text === "BIG"), "the bold run is still bold");
  await run(h, "text-case-sentence");
  assert.deepEqual(textOf(h), ["Hello big world. Next one", "Second paragraph"], "each paragraph starts a sentence");
  await run(h, "text-case-title");
  assert.deepEqual(textOf(h), ["Hello Big World. Next One", "Second Paragraph"]);
  await run(h, "text-case-toggle");
  assert.deepEqual(textOf(h), ["hELLO bIG wORLD. nEXT oNE", "sECOND pARAGRAPH"]);
  await run(h, "text-case-lower");
  assert.deepEqual(textOf(h), ["hello big world. next one", "second paragraph"]);
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(8);
  assert.deepEqual(textOf(h), ["hELLO bIG wORLD. nEXT oNE", "sECOND pARAGRAPH"], "one undo for one command");
});

test("a caret without a selection converts the word at the caret, and nothing selected says so", async () => {
  const h = await createRunHarness([p("alpha beta gamma")]);
  const text = h.blocks[0].firstChild;
  await select(h, rangeOf(text, 8, text, 8));
  await run(h, "text-case-upper");
  assert.deepEqual(textOf(h), ["alpha BETA gamma"]);
  const none = { currentRange: () => null, withUndo: () => assert.fail("no undo step without text") };
  h.host.__EASYOFFICE_DOC_WORD_COMMANDS__.run("text-case-lower", { editor: h.editor, document: h.document, runs: none, module: "DOC" });
  assert.equal(byId(h, "status-command-feedback").textContent, "대/소문자를 바꿀 글을 먼저 선택하세요");
});

test("Shift+F3 cycles each-word, UPPERCASE and lowercase in the word processor window only", async () => {
  const h = await createRunHarness([p("one two three")]);
  const text = h.blocks[0].firstChild;
  const whole = () => select(h, wholeOf(h, h.blocks[0]));
  const press = async () => {
    h.sandbox.dispatchWindowKeydown({ key: "F3", shiftKey: true });
    await flushAsyncTurns(8);
  };
  await whole();
  await press();
  assert.deepEqual(textOf(h), ["One Two Three"]);
  await whole();
  await press();
  assert.deepEqual(textOf(h), ["ONE TWO THREE"]);
  await whole();
  await press();
  assert.deepEqual(textOf(h), ["one two three"]);
  assert.ok(text, "the paragraph is still there");
  const pages = await openPages([p("one two")]);
  await select(pages, wholeOf(pages, pages.blocks[0]));
  pages.sandbox.dispatchWindowKeydown({ key: "F3", shiftKey: true });
  await flushAsyncTurns(8);
  assert.deepEqual(textOf(pages), ["one two"], "the Pages window has no such key");
});

test("the Word format menu has 대/소문자 바꾸기 with the five choices, and the Pages 대문자 menu has 첫 글자만 대문자", async () => {
  const word = await createRunHarness([p("x")]);
  const labels = (h, title) => {
    const sub = h.document.querySelectorAll(".md-sub").find((el) => el.querySelector(":scope > .md-label")?.textContent === title && !el.closest("[hidden]") && !el.hidden);
    return sub ? sub.querySelectorAll(".md-item").map((el) => el.querySelector(".md-label").textContent) : null;
  };
  assert.deepEqual(labels(word, "대/소문자 바꾸기"), ["문장의 첫 글자를 대문자로", "소문자로", "대문자로", "각 단어의 첫 글자를 대문자로", "대/소문자 전환"]);
  const pages = await openPages([p("가나")]);
  assert.deepEqual(labels(pages, "대문자"), ["없음", "모두 대문자", "작은 대문자", "첫 글자만 대문자"]);
  assert.equal(labels(pages, "대/소문자 바꾸기"), null);
  const english = await openPages([p("alpha beta")]);
  await select(english, wholeOf(english, english.blocks[0]));
  await run(english, "text-case-title");
  assert.deepEqual(textOf(english), ["Alpha Beta"], "the Pages choice is the same command");
});

test("⇧⌘K is 주석 추가 in the Pages window and the copilot in the other windows; the Pages copilot is the toolbar button", async () => {
  const pages = await openPages([p("가나다라")]);
  const text = pages.blocks[0].firstChild;
  await select(pages, rangeOf(text, 0, text, 2));
  pages.sandbox.dispatchWindowKeydown({ key: "K", ctrlKey: true, shiftKey: true });
  await flushAsyncTurns(10);
  assert.equal(byId(pages, "doc-comment-dialog").open, true, "Ctrl+Shift+K opens 주석 추가");
  assert.notEqual(byId(pages, "btn-copilot-toggle").getAttribute("aria-pressed"), "true", "the copilot did not open");
  assert.equal(byId(pages, "btn-copilot-toggle").getAttribute("title"), "코파일럿", "the toolbar button names no key");
  const copilotItem = pages.document.querySelectorAll('.md-item[data-studio-menu-cmd="copilot-toggle"]').find((el) => !el.hidden && !el.closest("[hidden]"));
  assert.equal(copilotItem.querySelector(".md-shortcut"), null, "the menu item names no key");

  const word = await createRunHarness([p("가나다라")]);
  word.sandbox.dispatchWindowKeydown({ key: "K", ctrlKey: true, shiftKey: true });
  await flushAsyncTurns(10);
  assert.notEqual(byId(word, "doc-comment-dialog")?.open, true, "the word processor keeps the key for the copilot");
  assert.equal(byId(word, "btn-copilot-toggle").getAttribute("title"), "코파일럿 (Ctrl+Shift+K)");
});

test("no shortcut combination repeats within a window flow", async () => {
  const h = await createRunHarness([p("x")]);
  const table = h.host.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS;
  const flows = { word: (b) => !b.flavor, pages: (b) => (!b.flavor || b.flavor === "pages") && !b.notPages };
  for (const [flow, applies] of Object.entries(flows)) {
    const seen = new Map();
    for (const b of table.filter(applies)) {
      for (const mod of b.module ? [b.module] : b.modules || ["*"]) {
        const key = `${b.keys}|${mod}`;
        const wild = `${b.keys}|*`;
        assert.ok(!seen.has(key) && !(mod !== "*" && seen.has(wild)), `${flow}: ${b.keys} repeats in ${mod}`);
        seen.set(key, b.action);
      }
    }
  }
});
