import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// 문서 속성, 줄 번호, 워터마크, 구역 나누기, 머리글 행 반복, 텍스트와 표의 변환 of the word processor window.

const select = async (h, range) => {
  h.selection.addRange(range);
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
};
const byId = (h, id) => h.document.getElementById(id);
const run = async (h, cmd) => {
  h.userClick(studioItem(h.document, cmd));
  await flushAsyncTurns(8);
};
const change = (h, id, value) => {
  const el = byId(h, id);
  el.value = String(value);
  el.dispatchEvent({ type: "change", target: el, bubbles: true });
};
const click = (h, id) => h.userClick(byId(h, id));
const payload = (h) => h.host.__EASYOFFICE_SAVE_DOC_META__.collectDocAnnotations(h.editor);
const meta = (h) => h.host.__EASYOFFICE_SAVE_DOC_META__.readWordMeta(h.editor.dataset.eoWordMeta);

test("문서 속성 keeps the title, subject, author, keywords and comments in the save payload, as one undo step", async () => {
  const h = await createRunHarness([p("본문")]);
  await run(h, "doc-properties");
  assert.equal(byId(h, "doc-properties-dialog").open, true);
  for (const [key, value] of [["title", "분기 보고서"], ["subject", "3분기"], ["author", "홍길동"], ["keywords", "매출, 비용"], ["comments", "초안"]]) byId(h, "doc-prop-" + key).value = value;
  click(h, "btn-doc-prop-ok");
  await flushAsyncTurns(6);
  assert.deepEqual(payload(h).properties, { title: "분기 보고서", subject: "3분기", author: "홍길동", keywords: "매출, 비용", comments: "초안" });
  await run(h, "doc-properties");
  assert.equal(byId(h, "doc-prop-author").value, "홍길동", "the dialog shows what the document has");
  h.userClick(byId(h, "doc-properties-dialog").querySelector("[data-dialog-close]"));
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(8);
  assert.equal(payload(h).properties, undefined);
});

test("a hostile property is cut and cleaned before it is kept", async () => {
  const h = await createRunHarness([p("본문")]);
  h.editor.dataset.eoWordMeta = JSON.stringify({ props: { author: "가".repeat(5000) + "\u0007", comments: "<b>x</b>\u0000", title: 5 }, lineNumbers: { start: 1, countBy: 0, distance: 0, restart: "never" }, watermark: "\u0007" });
  const out = payload(h);
  assert.equal(out.properties.author.length, 1024);
  assert.equal(out.properties.comments, "<b>x</b>");
  assert.equal(out.lineNumbers, undefined, "a number outside its bounds is not sent");
  assert.equal(out.watermark, undefined);
});

test("줄 번호 is a section setting with a start, a step, a distance and when it restarts", async () => {
  const h = await createRunHarness([p("본문")]);
  await run(h, "layout-linenumbers");
  assert.equal(byId(h, "doc-ln-on").checked, false);
  byId(h, "doc-ln-on").checked = true;
  byId(h, "doc-ln-start").value = "1";
  byId(h, "doc-ln-by").value = "5";
  byId(h, "doc-ln-distance").value = "10";
  change(h, "doc-ln-restart", "newSection");
  click(h, "btn-doc-ln-ok");
  await flushAsyncTurns(6);
  assert.deepEqual(payload(h).lineNumbers, { start: 1, countBy: 5, distance: 567, restart: "newSection" });
  await run(h, "layout-linenumbers");
  assert.equal(byId(h, "doc-ln-by").value, "5");
  byId(h, "doc-ln-on").checked = false;
  click(h, "btn-doc-ln-ok");
  await flushAsyncTurns(6);
  assert.equal(payload(h).lineNumbers, undefined);
});

test("줄 번호 refuses a step of zero", async () => {
  const h = await createRunHarness([p("본문")]);
  await run(h, "layout-linenumbers");
  byId(h, "doc-ln-on").checked = true;
  byId(h, "doc-ln-start").value = "1";
  byId(h, "doc-ln-by").value = "0";
  byId(h, "doc-ln-distance").value = "0";
  click(h, "btn-doc-ln-ok");
  await flushAsyncTurns(6);
  assert.equal(payload(h).lineNumbers, undefined);
  assert.equal(byId(h, "status-command-feedback").textContent, "줄 번호 값이 범위를 벗어났습니다");
});

test("워터마크 shows behind the page and goes into the save payload; 제거 takes it away", async () => {
  const h = await createRunHarness([p("본문")]);
  await run(h, "layout-watermark");
  byId(h, "doc-wm-text").value = "대외비";
  click(h, "btn-doc-wm-ok");
  await flushAsyncTurns(6);
  assert.equal(payload(h).watermark, "대외비");
  assert.equal(h.editor.dataset.eoWatermarkText, "대외비");
  await run(h, "layout-watermark");
  assert.equal(byId(h, "doc-wm-text").value, "대외비");
  click(h, "btn-doc-wm-remove");
  await flushAsyncTurns(6);
  assert.equal(payload(h).watermark, undefined);
  assert.equal(h.editor.dataset.eoWatermarkText, undefined);
});

test("구역 나누기 asks the page layout to split the paragraph and mark where the next section begins", async () => {
  const h = await createRunHarness([p("앞뒤")]);
  const asked = [];
  h.host.__EASYOFFICE_DOC_PAGES__.insertSectionBreak = (kind) => (asked.push(kind), true);
  for (const [cmd, kind] of [["section-break-nextPage", "nextPage"], ["section-break-continuous", "continuous"], ["section-break-evenPage", "evenPage"], ["section-break-oddPage", "oddPage"]]) await run(h, cmd);
  assert.deepEqual(asked, ["nextPage", "continuous", "evenPage", "oddPage"]);
  assert.deepEqual(Array.from(h.document.querySelectorAll('.md-item[data-studio-menu-cmd^="section-break-"] .md-label'), (el) => el.textContent), ["다음 페이지", "연속", "짝수 페이지", "홀수 페이지"]);
  h.host.__EASYOFFICE_DOC_PAGES__.insertSectionBreak = () => false;
  await run(h, "section-break-nextPage");
  assert.equal(byId(h, "status-command-feedback").textContent, "구역 나누기를 넣을 문단 안에 커서를 놓으세요");
});

test("a paragraph that ends a section saves its break, and an unknown break is not saved", async () => {
  const h = await createRunHarness([p("가"), p("나"), p("다")]);
  h.blocks[0].dataset.eoSectionBreak = "oddPage";
  h.blocks[1].dataset.eoSectionBreak = "sideways";
  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  assert.deepEqual(blocks.map((b) => b.sectionBreak), ["oddPage", undefined, undefined]);
});

function tableOf(h, rows) {
  const html = `<table class="eo-table"><tbody>${rows.map((r) => `<tr>${r.map((c) => `<td>${c}</td>`).join("")}</tr>`).join("")}</tbody></table>`;
  h.editor.innerHTML = "<p>앞</p>" + html;
  return h.editor.querySelector("table");
}

test("머리글 행 반복 marks the rows from the top through the caret's row, and the payload carries the count", async () => {
  const h = await createRunHarness([p("앞")]);
  const table = tableOf(h, [["머리", "글"], ["값", "1"], ["값", "2"]]);
  const cell = table.querySelectorAll("tr")[1].querySelector("td");
  await select(h, rangeOf(cell.firstChild, 0, cell.firstChild, 0));
  await run(h, "table-header-repeat");
  assert.equal(table.dataset.eoHeaderRows, "2");
  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  assert.equal(blocks.find((b) => b.type === "table").headerRows, 2);
  await run(h, "table-header-repeat");
  assert.equal(table.dataset.eoHeaderRows, undefined);
  assert.equal(h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor).find((b) => b.type === "table").headerRows, undefined);
});

test("텍스트를 표로 변환 turns the selected paragraphs into a table by paragraph, comma or tab", async () => {
  const h = await createRunHarness([p("가,나,다"), p("라,마"), p("바")]);
  const first = h.blocks[0].firstChild;
  const last = h.blocks[2].firstChild;
  await select(h, rangeOf(first, 0, last, 1));
  await run(h, "table-from-text");
  assert.equal(byId(h, "doc-table-convert-dialog").open, true);
  assert.equal(byId(h, "doc-tc-title").textContent, "텍스트를 표로 변환");
  change(h, "doc-tc-sep", "comma");
  click(h, "btn-doc-tc-ok");
  await flushAsyncTurns(6);
  const rows = Array.from(h.editor.querySelectorAll("tr"), (tr) => Array.from(tr.querySelectorAll("td"), (td) => td.textContent));
  assert.deepEqual(rows, [["가", "나", "다"], ["라", "마", ""], ["바", "", ""]].map((r, i) => r.slice(0, i === 0 ? 3 : i === 1 ? 3 : 3)));
  assert.equal(h.editor.querySelectorAll("p").length, 0, "the paragraphs became the table");
});

test("텍스트를 표로 변환 by paragraph fills the columns row by row, and refuses an empty selection", async () => {
  const h = await createRunHarness([p("1"), p("2"), p("3"), p("4"), p("5")]);
  await select(h, rangeOf(h.blocks[0].firstChild, 0, h.blocks[4].firstChild, 1));
  await run(h, "table-from-text");
  byId(h, "doc-tc-cols").value = "2";
  click(h, "btn-doc-tc-ok");
  await flushAsyncTurns(6);
  assert.deepEqual(Array.from(h.editor.querySelectorAll("tr"), (tr) => Array.from(tr.querySelectorAll("td"), (td) => td.textContent)), [["1", "2"], ["3", "4"], ["5", ""]]);
  const empty = await createRunHarness([p("가")]);
  await run(empty, "table-from-text");
  click(empty, "btn-doc-tc-ok");
  assert.equal(byId(empty, "doc-tc-status").textContent, "표로 변환할 문단을 먼저 선택하세요");
});

test("표를 텍스트로 변환 writes a paragraph per row with the separator, or per cell", async () => {
  const h = await createRunHarness([p("앞")]);
  const table = tableOf(h, [["가", "나"], ["다", "라"]]);
  const cell = table.querySelector("td");
  await select(h, rangeOf(cell.firstChild, 0, cell.firstChild, 0));
  await run(h, "table-to-text");
  assert.equal(byId(h, "doc-tc-title").textContent, "표를 텍스트로 변환");
  assert.equal(byId(h, "doc-tc-cols-row").hidden, true);
  change(h, "doc-tc-sep", "other");
  assert.equal(byId(h, "doc-tc-other-row").hidden, false);
  byId(h, "doc-tc-other").value = ";";
  click(h, "btn-doc-tc-ok");
  await flushAsyncTurns(6);
  assert.equal(h.editor.querySelector("table"), null);
  assert.deepEqual(Array.from(h.editor.querySelectorAll("p"), (el) => el.textContent), ["앞", "가;나", "다;라"]);
});
