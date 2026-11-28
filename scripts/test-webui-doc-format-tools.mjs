import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";
import { openPasteEditor } from "./test-support/paste-harness.mjs";

// 서식 복사, 모든 서식 지우기 and 붙여넣기 옵션 > 텍스트만 유지 of the word processor window.

const saved = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
const select = async (h, range) => {
  h.selection.addRange(range);
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
};
const contents = (block) => rangeOf(block, 0, block, block.childNodes.length);
const selectAll = (h, block) => select(h, contents(block));
const format = (h, block, patch) => h.runs.applyFormat(h.editor, contents(block), patch);
const firstText = (node) => (node.nodeType === 3 ? node : node.childNodes.map(firstText).find(Boolean));
const run = async (h, cmd) => {
  h.userClick(studioItem(h.document, cmd));
  await flushAsyncTurns(8);
};
const feedback = (h) => h.document.getElementById("status-command-feedback").textContent;
const tools = (h) => h.host.__EASYOFFICE_DOC_FORMAT_TOOLS__;

test("모든 서식 지우기 takes the character formats off the selection and sets the heading to the body style", async () => {
  const h = await createRunHarness([{ tag: "h2", children: ["제목 글"] }, p("본문")]);
  format(h, h.blocks[0], { bold: true, italic: true, color: "FF0000", size: 28 });
  assert.equal(saved(h)[0].runs[0].bold, true);
  await selectAll(h, h.blocks[0]);
  await run(h, "format-clear");
  const [block] = saved(h);
  assert.equal(block.heading, undefined, "the heading is the body style again");
  assert.deepEqual(block.runs.map((r) => [r.bold, r.italic, r.color, r.sizeHalfPt]), [[undefined, undefined, undefined, undefined]]);
  assert.equal(feedback(h), "서식 지우기 완료");
});

test("모든 서식 지우기 resets the alignment, indents and spacing of the paragraph and is one undo step", async () => {
  const h = await createRunHarness([p("가나다"), p("라마바")]);
  const [first, second] = h.blocks;
  first.style.textAlign = "center";
  first.style.paddingLeft = "32px";
  first.dataset.eoLeftTwips = "480";
  first.style.lineHeight = "2";
  first.dataset.eoLineSpacing = "200";
  await selectAll(h, first);
  await run(h, "format-clear");
  const el = h.editor.childNodes[0];
  assert.equal(el.style.textAlign, "");
  assert.equal(el.style.paddingLeft, "");
  assert.equal(el.style.lineHeight, "");
  assert.equal(el.dataset.eoLeftTwips, undefined);
  assert.equal(el.dataset.eoLineSpacing, undefined);
  assert.equal(second.style.textAlign, "", "the other paragraph is untouched");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(8);
  assert.equal(h.editor.childNodes[0].style.textAlign, "center", "one undo brings the whole format back");
});

test("모든 서식 지우기 with a caret clears the paragraph the caret is in", async () => {
  const h = await createRunHarness([p("가나다")]);
  format(h, h.blocks[0], { underline: true });
  assert.equal(saved(h)[0].runs[0].underline, true);
  const node = firstText(h.blocks[0]);
  await select(h, rangeOf(node, 1, node, 1));
  await run(h, "format-clear");
  assert.equal(saved(h)[0].runs[0].underline, undefined);
});

test("서식 복사 copies the format at the selection start and applies it to the next selection", async () => {
  const h = await createRunHarness([p("원본"), p("대상")]);
  const [source, target] = h.blocks;
  format(h, source, { bold: true, color: "00AA00", size: 30 });
  source.style.textAlign = "right";
  await selectAll(h, source);
  await run(h, "format-painter");
  assert.equal(tools(h).isArmed(), true);
  assert.equal(feedback(h), "서식을 적용할 글을 선택하세요");
  await selectAll(h, target);
  h.editor.dispatchEvent({ type: "mouseup", target: h.editor });
  await flushAsyncTurns(8);
  const [, applied] = saved(h);
  assert.deepEqual([applied.runs[0].bold, applied.runs[0].color, applied.runs[0].sizeHalfPt], [true, "00AA00", 30]);
  assert.equal(applied.align, "right");
  assert.equal(tools(h).isArmed(), false, "one use per copy");
});

test("서식 복사 is dropped by a second use of the command and by Esc", async () => {
  const h = await createRunHarness([p("원본")]);
  await selectAll(h, h.blocks[0]);
  await run(h, "format-painter");
  await run(h, "format-painter");
  assert.equal(tools(h).isArmed(), false);
  assert.equal(feedback(h), "서식 복사를 취소했습니다");
  await run(h, "format-painter");
  assert.equal(tools(h).isArmed(), true);
  h.editor.dispatchEvent({ type: "keydown", key: "Escape", target: h.editor });
  assert.equal(tools(h).isArmed(), false);
});

test("붙여넣기 옵션 > 텍스트만 유지 pastes the plain text of the clipboard, not its HTML", async () => {
  const sys = {
    items: [],
    async read() {
      return [{ types: ["text/html", "text/plain"], getType: async (type) => ({ size: 20, type, text: async () => (type === "text/html" ? "<p><b>굵은</b> 글</p>" : "굵은 글") }) }];
    },
    async readText() {
      return "굵은 글";
    },
  };
  const t = await openPasteEditor({ paragraphs: ["앞뒤"], clipboard: sys });
  const para = t.blocks()[0];
  t.select(para.firstChild, 1, para.firstChild, 1);
  t.userClick(studioItem(t.document, "paste-text-only"));
  await flushAsyncTurns(20);
  assert.equal(t.editor.textContent, "앞굵은 글뒤");
  assert.equal(t.editor.querySelector("b, strong"), null, "no bold of the clipboard's HTML");
  assert.equal(studioItem(t.document, "paste-text-only").closest(".md-sub").querySelector(":scope > .md-label").textContent, "붙여넣기 옵션");
});
