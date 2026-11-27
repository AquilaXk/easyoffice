import assert from "node:assert/strict";
import test from "node:test";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { reviewButton } from "./test-support/copilot-accept.mjs";
import { open, answer } from "./test-support/copilot-open.mjs";

// Review fixes of the copilot's document edits (PD-386): a span or a paragraph replacement keeps the table and the list it is in,
// the text that was already in the document is never read again as tags or entities, and a freeze keeps its columns.

const TABLE = "<table><tbody><tr><td>Q1</td><td>Q2</td></tr><tr><td>Q3</td><td>합계</td></tr></tbody></table>";
const LIST = "<ul><li>사과</li><li>배</li><li>귤</li></ul>";

const blocks = (editor) => Array.from(editor.children).filter((el) => !el.classList.contains("manuscript-notice"));
const cellTexts = (el) => Array.from(el.querySelectorAll("td")).map((td) => td.textContent);
const itemTexts = (el) => Array.from(el.querySelectorAll("li")).map((li) => li.textContent);
const span = (index, search, replace) => ({ op: "doc.replace_span", index, search, replace });
const replaceOne = (index, text) => ({ op: "doc.replace_paragraphs", replacements: [{ index, text }] });

async function propose(operations, html, { track = false } = {}) {
  const h = await open({ run: () => answer(operations) });
  h.editor.innerHTML = html;
  if (track) h.editor.dataset.eoTrackChanges = "true";
  await h.send("고쳐 주세요");
  return h;
}
const press = async (h, action) => {
  h.userClick(reviewButton(h.document, action));
  await flushAsyncTurns(14);
};
const errorText = (h) => h.assistants()[0].textContent;

// ---- 표와 목록 안의 구간

test("replace_span in a table changes the one cell and keeps the table", async () => {
  const h = await propose([span(0, "Q3", "Q4")], TABLE);
  await press(h, "accept");
  const table = blocks(h.editor)[0];
  assert.equal(table.querySelectorAll("td").length, 4, "the cells are still there");
  assert.deepEqual(cellTexts(table), ["Q1", "Q2", "Q4", "합계"]);
});

test("replace_span in a list changes the one item and keeps the list", async () => {
  const h = await propose([span(0, "배", "포도")], LIST);
  await press(h, "accept");
  const list = blocks(h.editor)[0];
  assert.equal(list.tagName.toLowerCase(), "ul");
  assert.deepEqual(itemTexts(list), ["사과", "포도", "귤"]);
});

test("a span across two cells or two items is refused and nothing changes", async () => {
  const table = await propose([span(0, "Q1Q2", "X")], TABLE);
  await press(table, "accept");
  assert.deepEqual(cellTexts(blocks(table.editor)[0]), ["Q1", "Q2", "Q3", "합계"]);
  assert.match(errorText(table), /여러 칸/);
  const list = await propose([span(0, "사과배", "X")], LIST);
  await press(list, "accept");
  assert.deepEqual(itemTexts(blocks(list.editor)[0]), ["사과", "배", "귤"]);
});

test("replace_paragraphs on a list rebuilds the items from its lines, on a table it is refused", async () => {
  const list = await propose([replaceOne(0, "- 가\n- 나\n2. 다")], LIST);
  await press(list, "accept");
  assert.deepEqual(itemTexts(blocks(list.editor)[0]), ["가", "나", "다"]);

  const table = await propose([replaceOne(0, "표를 글로 바꿈")], TABLE);
  await press(table, "accept");
  assert.equal(blocks(table.editor)[0].tagName.toLowerCase(), "table", "the table is still a table");
  assert.deepEqual(cellTexts(blocks(table.editor)[0]), ["Q1", "Q2", "Q3", "합계"]);
  assert.match(errorText(table), /표 문단은 글로 바꿀 수 없습니다/);
});

test("with track changes on, a list replacement leaves the old list marked deleted and a new marked list under it", async () => {
  const h = await propose([replaceOne(0, "- 가\n- 나")], LIST, { track: true });
  await press(h, "accept");
  const [oldList, newList] = blocks(h.editor);
  assert.equal(oldList.querySelectorAll("li del").length, 3, "the old items are marked deleted inside their li");
  assert.equal(newList.tagName.toLowerCase(), "ul");
  assert.equal(newList.querySelectorAll("li ins").length, 2);
  assert.equal(newList.textContent, "가나");
});

test("아래에 삽입 under a list makes a new list, under a table it is refused", async () => {
  const list = await propose([replaceOne(0, "- 가\n- 나")], LIST);
  await press(list, "insert-below");
  const [original, added] = blocks(list.editor);
  assert.deepEqual(itemTexts(original), ["사과", "배", "귤"], "the original list stays");
  assert.equal(added.tagName.toLowerCase(), "ul");
  assert.deepEqual(itemTexts(added), ["가", "나"]);

  const span2 = await propose([span(0, "배", "포도")], LIST);
  await press(span2, "insert-below");
  assert.deepEqual(itemTexts(blocks(span2.editor)[1]), ["사과", "포도", "귤"], "a span makes the whole list again with the one item changed");

  const table = await propose([replaceOne(0, "글")], TABLE);
  await press(table, "insert-below");
  assert.equal(blocks(table.editor).length, 1, "nothing is added");
  assert.match(errorText(table), /표 문단은 글로 바꿀 수 없어/);
});

// ---- 문서에 있던 글을 다시 읽지 않음

const paragraph = (html) => `<p>${html}</p>`;
const textOf = (editor) => blocks(editor).map((el) => el.textContent);

test("replace_span keeps what is already in the paragraph as it is, even if it looks like a tag", async () => {
  const h = await open({ run: () => answer([span(0, "then x", "then y")]) });
  h.editor.innerHTML = "<p></p>";
  h.editor.firstChild.textContent = "if a<b and c>d then x";
  await h.send("고쳐 주세요");
  await press(h, "accept");
  assert.deepEqual(textOf(h.editor), ["if a<b and c>d then y"]);
});

test("replace_span does not turn a literal &lt; into <", async () => {
  const h = await open({ run: () => answer([span(0, "then x", "then y")]) });
  h.editor.innerHTML = "<p></p>";
  h.editor.firstChild.textContent = "a &lt; b &amp; c then x";
  await h.send("고쳐 주세요");
  await press(h, "accept");
  assert.deepEqual(textOf(h.editor), ["a &lt; b &amp; c then y"]);
});

test("the text the model writes is still read for inline formatting", async () => {
  const h = await open({ run: () => answer([span(0, "중요", "<strong>핵심</strong>")]) });
  h.editor.innerHTML = paragraph("이것은 중요 문장");
  await h.send("고쳐 주세요");
  await press(h, "accept");
  const p = blocks(h.editor)[0];
  assert.equal(p.textContent, "이것은 핵심 문장");
  assert.ok(p.querySelector(".eo-run"), "the replacement keeps its bold run");
});

test("with track changes on, the paragraph's own text is not read again either", async () => {
  const h = await open({ run: () => answer([span(0, "then x", "then y")]) });
  h.editor.innerHTML = "<p></p>";
  h.editor.firstChild.textContent = "if a<b and c>d then x";
  h.editor.dataset.eoTrackChanges = "true";
  await h.send("고쳐 주세요");
  await press(h, "accept");
  const p = blocks(h.editor)[0];
  assert.equal(p.querySelector("del").textContent, "if a<b and c>d then x");
  assert.equal(p.querySelector("ins").textContent, "if a<b and c>d then y");
});

test("아래에 삽입 of a span writes the paragraph's text as text", async () => {
  const h = await open({ run: () => answer([span(0, "then x", "then y")]) });
  h.editor.innerHTML = "<p></p>";
  h.editor.firstChild.textContent = "if a<b and c>d then x &lt;";
  await h.send("고쳐 주세요");
  await press(h, "insert-below");
  assert.deepEqual(textOf(h.editor), ["if a<b and c>d then x &lt;", "if a<b and c>d then y &lt;"]);
});

// ---- 틀 고정의 열

test("a freeze with columns freezes them and a freeze of columns only is not an unfreeze", async () => {
  const h = await open({ module: "SHEET", run: () => answer([]) });
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const ops = h.host.__EASYOFFICE_COPILOT_SHEET_OPS__;
  const run = (op) => {
    const locations = [];
    ops.executeExtendedSheetOp("sheet.structure", { kind: "sheet.structure", action: "freeze", count: 1, ...op }, { sheetName: "Sheet1", sheetDataStore: {}, sheetRowCounts: {}, locations });
    return { freeze: layout.freezeOf("Sheet1"), label: locations[0].label };
  };
  assert.deepEqual(run({ index: 2, freezeCols: 1 }), { freeze: { rows: 2, cols: 1 }, label: "틀 고정 (2행 1열)" });
  assert.deepEqual(run({ index: 0, freezeCols: 3 }), { freeze: { rows: 0, cols: 3 }, label: "틀 고정 (0행 3열)" });
});
