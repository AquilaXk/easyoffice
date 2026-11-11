import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p, textNodeOf, click } from "./test-support/doc-run-harness.mjs";
import { readText } from "./test-support/read-text.mjs";

// Paragraph-level controls of the style bar (alignment, line spacing, lists, styles) act on the
// paragraphs the selection touches and the saved blocks carry the result.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const saved = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
const select = async (h, range) => {
  h.selection.addRange(range);
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10); // the toolbar follows the selection on a timer
};
const selectAcross = (h, first, last) => select(h, rangeOf(textNodeOf(first), 0, textNodeOf(last), 1));
const caretIn = (h, block) => select(h, rangeOf(textNodeOf(block), 0, textNodeOf(block), 0));
const feedback = (h) => h.document.getElementById("status-command-feedback").textContent;
const menu = (h, cmd) => h.document.querySelector(`.md-item[data-studio-menu-cmd="${cmd}"]`);
const tags = (h) => h.editor.childNodes.map((n) => n.tagName);

test("alignment buttons align the selected paragraphs and headings, not the editor root", async () => {
  const h = await createRunHarness([p("첫째"), { tag: "h2", children: ["제목"] }, p("셋째"), p("넷째")]);
  const [a, b, c, d] = h.blocks;
  await selectAcross(h, a, c);
  await click(h, "btn-align-center");
  for (const el of [a, b, c]) assert.equal(el.style.textAlign, "center");
  assert.equal(d.style.textAlign, "", "an unselected paragraph keeps its alignment");
  assert.equal(h.editor.style.textAlign, "", "the editor root is not aligned");
  assert.equal(h.document.getElementById("btn-align-center").classList.contains("active"), true);
  assert.deepEqual(saved(h).map((blk) => blk.align), ["center", "center", "center", undefined]);

  await caretIn(h, d);
  await click(h, "btn-align-right");
  assert.equal(d.style.textAlign, "right");
  assert.equal(saved(h)[3].align, "right");
});

test("alignment applies to list items", async () => {
  const h = await createRunHarness([{ tag: "ul", children: [{ tag: "li", children: ["가"] }, { tag: "li", children: ["나"] }] }]);
  const [one, two] = h.blocks[0].childNodes;
  await selectAcross(h, one, one);
  await click(h, "btn-align-right");
  assert.equal(one.style.textAlign, "right");
  assert.equal(two.style.textAlign, "");
  assert.deepEqual(saved(h).map((blk) => blk.align), ["right", undefined]);
});

test("distribute and divide alignment are kept per paragraph, saved as justify, and the user is told", async () => {
  const h = await createRunHarness([p("배분"), p("나눔"), p("그대로")]);
  const [a, b] = h.blocks;
  await selectAcross(h, a, a);
  await click(h, "btn-align-distribute");
  assert.equal(a.dataset.eoAlign, "distribute");
  assert.equal(a.style.textAlign, "justify");
  assert.match(feedback(h), /배분 정렬은 저장할 때 양쪽 정렬로/);
  await selectAcross(h, b, b);
  await click(h, "btn-align-divide");
  assert.equal(b.dataset.eoAlign, "divide");
  assert.deepEqual(saved(h).map((blk) => blk.align), ["justify", "justify", undefined]);

  await selectAcross(h, a, a);
  assert.equal(h.document.getElementById("btn-align-distribute").classList.contains("active"), true, "the button follows the paragraph");
  h.userClick(studioItem(h.document, "studio-save"));
  await flushAsyncTurns(15);
  assert.match(feedback(h), /배분·나눔 정렬 2개 문단은 양쪽 정렬로 저장/);

  await click(h, "btn-align-left");
  assert.equal(a.dataset.eoAlign, undefined, "a plain alignment clears the approximation mark");
});

test("alignment without a selection says what to do and changes nothing", async () => {
  const h = await createRunHarness([p("본문")]);
  await click(h, "btn-align-center");
  assert.equal(h.blocks[0].style.textAlign, "");
  assert.match(feedback(h), /정렬할 문단을 먼저 선택/);
});

test("line spacing applies to the selected paragraphs only and is the value that is saved", async () => {
  const h = await createRunHarness([p("가"), p("나"), p("다")]);
  const [a, b, c] = h.blocks;
  await selectAcross(h, a, b);
  await click(h, "btn-ls-up");
  assert.equal(a.style.lineHeight, "1.80");
  assert.equal(b.style.lineHeight, "1.80");
  assert.equal(c.style.lineHeight, "");
  assert.equal(h.editor.style.lineHeight, "", "the editor root keeps its own line height");
  assert.equal(h.document.getElementById("doc-linespacing-select").value, "180");
  assert.deepEqual(saved(h).map((blk) => blk.lineSpacingPercent), [180, 180, undefined]);

  await selectAcross(h, c, c);
  assert.equal(h.document.getElementById("doc-linespacing-select").value, "160", "an untouched paragraph shows the default");
  await selectAcross(h, a, a);
  assert.equal(h.document.getElementById("doc-linespacing-select").value, "180", "a paragraph shows its own value");

  const lsBox = h.document.getElementById("doc-linespacing-select");
  lsBox.value = "130";
  lsBox.dispatchEvent({ type: "change", target: lsBox });
  assert.equal(a.style.lineHeight, "1.30");
});

test("numbering and bullets build list structure, convert back, and are saved as list items", async () => {
  const h = await createRunHarness([p("가"), p("나"), p("다"), p("라")]);
  const [a, b, c] = h.blocks;
  await selectAcross(h, a, b);
  menu(h, "para-numbering").click();
  await flushAsyncTurns(10);
  const ol = h.editor.childNodes[0];
  assert.equal(ol.tagName, "OL");
  assert.deepEqual(ol.childNodes.map((li) => li.tagName), ["LI", "LI"]);
  assert.equal(ol.textContent, "가나", "no text prefix is inserted");
  assert.deepEqual(saved(h).map((blk) => blk.list), ["ordered", "ordered", undefined, undefined]);

  // The next paragraph joins the same list.
  await caretIn(h, c);
  menu(h, "para-numbering").click();
  await flushAsyncTurns(10);
  assert.deepEqual(tags(h), ["OL", "P"]);
  assert.equal(h.editor.childNodes[0].childNodes.length, 3);

  // Toggling off the middle item splits the list around a paragraph.
  await caretIn(h, h.editor.childNodes[0].childNodes[1]);
  menu(h, "para-numbering").click();
  await flushAsyncTurns(10);
  assert.deepEqual(tags(h), ["OL", "P", "OL", "P"]);
  assert.equal(h.editor.childNodes[1].textContent, "나");

  // The other kind turns the item into a bullet.
  await caretIn(h, h.editor.childNodes[0].childNodes[0]);
  menu(h, "para-bullet").click();
  await flushAsyncTurns(10);
  assert.deepEqual(tags(h), ["UL", "P", "OL", "P"]);
  assert.deepEqual(saved(h).map((blk) => blk.list), ["bullet", undefined, "ordered", undefined]);
});

test("list conversion keeps character runs, alignment and indentation", async () => {
  const h = await createRunHarness([p("앞 ", { tag: "strong", children: ["굵게"] })]);
  const [para] = h.blocks;
  para.style.textAlign = "center";
  para.style.paddingLeft = "16px";
  await selectAcross(h, para, para);
  menu(h, "para-bullet").click();
  await flushAsyncTurns(10);
  const li = h.editor.querySelector("li");
  assert.equal(li.style.textAlign, "center");
  assert.equal(li.style.paddingLeft, "16px");
  assert.equal(li.querySelector("strong").textContent, "굵게");
  assert.equal(saved(h)[0].list, "bullet");
  assert.equal(saved(h)[0].align, "center");
});

test("a list conversion is one undo step", async () => {
  const h = await createRunHarness([p("가"), p("나")]);
  await selectAcross(h, h.blocks[0], h.blocks[1]);
  menu(h, "para-bullet").click();
  await flushAsyncTurns(10);
  assert.equal(h.editor.querySelectorAll("li").length, 2);
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(h.editor.querySelectorAll("li").length, 0);
  assert.equal(h.editor.querySelectorAll("p").length, 2);
});

test("the style box turns paragraphs into outline headings and back", async () => {
  const h = await createRunHarness([p("가"), p("나"), p("다")]);
  const [a, b] = h.blocks;
  const style = h.document.getElementById("doc-style-select");
  await selectAcross(h, a, b);
  style.value = "개요 2";
  style.dispatchEvent({ type: "change", target: style });
  await flushAsyncTurns(10);
  assert.deepEqual(tags(h), ["H2", "H2", "P"]);
  assert.deepEqual(saved(h).map((blk) => blk.heading), [2, 2, undefined]);
  assert.equal(h.editor.childNodes[0].textContent, "가");

  await caretIn(h, h.editor.childNodes[0]);
  assert.equal(style.value, "개요 2", "the box shows the paragraph's style");
  style.value = "바탕글";
  style.dispatchEvent({ type: "change", target: style });
  await flushAsyncTurns(10);
  assert.deepEqual(tags(h), ["P", "H2", "P"]);
});

test("the style box without a selection keeps the shown style and says what to do", async () => {
  const h = await createRunHarness([p("본문")]);
  const style = h.document.getElementById("doc-style-select");
  style.value = "개요 1";
  style.dispatchEvent({ type: "change", target: style });
  await flushAsyncTurns(10);
  assert.equal(h.editor.childNodes[0].tagName, "P");
  assert.equal(style.value, "바탕글");
  assert.match(feedback(h), /스타일을 적용할 문단을 먼저 선택/);
});

test("controls without a model counterpart are gone and no dead style rules remain", async () => {
  const h = await createRunHarness([p("본문")]);
  assert.equal(h.document.getElementById("doc-font-lang"), null, "no per-language font slot in the run model");
  assert.equal(h.document.getElementById("btn-stylebar-charfx"), null, "no character effects in the run model");
  const values = Array.from(h.document.getElementById("doc-style-select").querySelectorAll("option")).map((o) => o.value);
  assert.deepEqual(values, ["바탕글", "공문서 바탕글", "개요 1", "개요 2", "개요 3"]);
  const css = ["styles.css", "toolbar.css", "content.css"].map((f) => readText(new URL(f, UI_DIR))).join("\n");
  assert.doesNotMatch(css, /data-(strike|textcolor|highlight|charfx)|doc-font-lang|sb-lang-select/);
});

test("a list item outside a list or list-item in a table cell is wrapped, never moved out of the editor", async () => {
  const h = await createRunHarness([
    { tag: "li", children: ["고아"] },
    p("뒤"),
    { tag: "table", children: [{ tag: "tbody", children: [{ tag: "tr", children: [{ tag: "td", children: [{ tag: "li", children: ["칸"] }] }] }] }] },
  ]);
  const [orphan, after, table] = h.blocks;
  await caretIn(h, orphan);
  menu(h, "para-numbering").click();
  await flushAsyncTurns(10);
  assert.equal(h.document.getElementById("doc-editor"), h.editor, "the editor stays in the document");
  assert.deepEqual(tags(h), ["OL", "P", "TABLE"]);
  assert.equal(h.editor.childNodes[0].textContent, "고아");
  assert.equal(after.textContent, "뒤");

  // Applying the same kind again takes the (now real) item out of its list.
  await caretIn(h, h.editor.childNodes[0].childNodes[0]);
  menu(h, "para-numbering").click();
  await flushAsyncTurns(10);
  assert.deepEqual(tags(h), ["P", "P", "TABLE"]);

  const cell = table.querySelector("td");
  await caretIn(h, cell.childNodes[0]);
  menu(h, "para-bullet").click();
  await flushAsyncTurns(10);
  assert.equal(h.document.getElementById("doc-editor"), h.editor);
  assert.deepEqual(cell.childNodes.map((n) => n.tagName), ["UL"]);
  assert.equal(cell.textContent, "칸");
  menu(h, "para-bullet").click();
  await flushAsyncTurns(10);
  assert.deepEqual(cell.childNodes.map((n) => n.tagName), ["P"], "toggling off inside the cell stays in the cell");
  assert.equal(h.editor.contains(cell), true);
});

test("changing a paragraph's tag keeps its own bold, color, id and language", async () => {
  const h = await createRunHarness([p("가")]);
  const [para] = h.blocks;
  para.style.fontWeight = "700";
  para.style.color = "#112233";
  para.style.fontStyle = "italic";
  para.setAttribute("id", "keep-me");
  para.setAttribute("lang", "ko");
  await selectAcross(h, para, para);
  const style = h.document.getElementById("doc-style-select");
  style.value = "개요 3";
  style.dispatchEvent({ type: "change", target: style });
  await flushAsyncTurns(10);
  const heading = h.editor.childNodes[0];
  assert.equal(heading.tagName, "H3");
  assert.equal(heading.style.fontWeight, "700");
  assert.equal(heading.style.fontStyle, "italic");
  assert.equal(heading.style.color, "#112233");
  assert.equal(heading.getAttribute("id"), "keep-me");
  assert.equal(heading.getAttribute("lang"), "ko");
  assert.equal(saved(h)[0].runs[0].bold, true, "the saved run still carries the block's bold");
});

test("a paragraph command that changes nothing leaves no undo step", async () => {
  const h = await createRunHarness([p("가"), p("나")]);
  const [a, b] = h.blocks;
  await selectAcross(h, a, a);
  await click(h, "btn-align-center");
  await selectAcross(h, b, b);
  await click(h, "btn-align-right");
  await click(h, "btn-align-right"); // already right: nothing to record
  assert.equal(b.style.textAlign, "right");
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(h.editor.childNodes[1].style.textAlign, "", "one undo reverts the single change");
  assert.equal(h.editor.childNodes[0].style.textAlign, "center", "the earlier paragraph keeps its alignment");
});

test("the line spacing list keeps one custom value", async () => {
  const h = await createRunHarness([p("가"), p("나")]);
  const [a, b] = h.blocks;
  const box = h.document.getElementById("doc-linespacing-select");
  const customs = () => Array.from(box.querySelectorAll("option")).filter((o) => !["100", "130", "150", "160", "180", "200", "300"].includes(o.value)).map((o) => o.value);
  a.style.lineHeight = "1.25";
  b.style.lineHeight = "1.75";
  await selectAcross(h, a, a);
  assert.deepEqual(customs(), ["125"]);
  await selectAcross(h, b, b);
  assert.deepEqual(customs(), ["175"], "the earlier custom entry is dropped");
  await selectAcross(h, a, a);
  await selectAcross(h, b, b);
  assert.equal(customs().length, 1);
});
