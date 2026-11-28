import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// 날짜 및 시간, 책갈피, 상호 참조, 캡션 삽입 and 필드 업데이트 of the word processor window, and how the saved payload and an opened
// document carry them (`doc-word-model.js`).

const saved = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
const api = (h) => h.host.__EASYOFFICE_DOC_WORD_FIELDS__;
const select = async (h, range) => {
  h.selection.addRange(range);
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
};
const caretAt = (h, node, offset) => select(h, rangeOf(node, offset, node, offset));
const run = async (h, cmd) => {
  h.userClick(studioItem(h.document, cmd));
  await flushAsyncTurns(8);
};
const byId = (h, id) => h.document.getElementById(id);
const change = (h, id, value) => {
  const el = byId(h, id);
  el.value = String(value);
  el.dispatchEvent({ type: "change", target: el, bubbles: true });
};
const feedback = (h) => byId(h, "status-command-feedback").textContent;
const click = (h, id) => h.userClick(byId(h, id));

test("a date or time picture shows the way the Word program shows it", async () => {
  const h = await createRunHarness([p("가")]);
  const at = new Date(2026, 10, 28, 15, 5, 9);
  const show = (picture) => api(h).formatPicture(picture, at);
  assert.equal(show("yyyy-MM-dd"), "2026-11-28");
  assert.equal(show("yyyy. M. d."), "2026. 11. 28.");
  assert.equal(show("yyyy'년' M'월' d'일' dddd"), "2026년 11월 28일 토요일");
  assert.equal(show("yy-MM-dd"), "26-11-28");
  assert.equal(show("AM/PM h:mm"), "오후 3:05");
  assert.equal(show("HH:mm:ss"), "15:05:09");
  assert.equal(show("AM/PM h:mm:ss"), "오후 3:05:09");
  assert.equal(show("'it''s' yyyy"), "its 2026");
});

test("날짜 및 시간 inserts the text as it is, or a field that the file keeps with its picture", async () => {
  const h = await createRunHarness([p("앞뒤")]);
  const text = h.blocks[0].firstChild;
  await caretAt(h, text, 1);
  await run(h, "insert-datetime");
  assert.equal(byId(h, "doc-datetime-dialog").open, true);
  const options = Array.from(byId(h, "doc-dt-format").querySelectorAll("option"));
  assert.equal(options.length, 9);
  assert.match(options[0].textContent, /^\d{4}-\d{2}-\d{2}$/);
  change(h, "doc-dt-format", "yyyy'년' M'월' d'일'");
  byId(h, "doc-dt-update").checked = true;
  click(h, "btn-doc-dt-ok");
  await flushAsyncTurns(6);
  const field = h.editor.querySelector("[data-eo-field-code]");
  assert.equal(field.dataset.eoFieldCode, `DATE \\@ "yyyy'년' M'월' d'일'"`);
  assert.match(field.textContent, /^\d{4}년 \d{1,2}월 \d{1,2}일$/);
  const [block] = saved(h);
  const coded = block.runs.find((r) => r.fieldCode);
  assert.equal(coded.fieldCode, field.dataset.eoFieldCode);
  assert.equal(coded.text, field.textContent);
  assert.equal(block.runs.map((r) => r.text).join(""), "앞" + field.textContent + "뒤");
});

test("without 자동으로 업데이트 the date is plain text", async () => {
  const h = await createRunHarness([p("앞뒤")]);
  await caretAt(h, h.blocks[0].firstChild, 1);
  await run(h, "insert-datetime");
  byId(h, "doc-dt-update").checked = false;
  click(h, "btn-doc-dt-ok");
  await flushAsyncTurns(6);
  assert.equal(h.editor.querySelector("[data-eo-field-code]"), null);
  assert.match(h.editor.textContent, /^앞\d{4}-\d{2}-\d{2}뒤$/);
});

test("책갈피 over a selection is a pair of markers that the payload turns into a start and an end", async () => {
  const h = await createRunHarness([p("가나다라마")]);
  const text = h.blocks[0].firstChild;
  await select(h, rangeOf(text, 1, text, 4));
  await run(h, "insert-bookmark");
  assert.equal(byId(h, "doc-bookmark-dialog").open, true);
  byId(h, "doc-bm-name").value = "중요";
  click(h, "btn-doc-bm-add");
  await flushAsyncTurns(6);
  assert.deepEqual(Array.from(h.editor.querySelectorAll("[data-eo-bookmark-start], [data-eo-bookmark-end]"), (m) => m.dataset.eoBookmarkStart ?? "끝:" + m.dataset.eoBookmarkEnd), ["중요", "끝:중요"]);
  assert.equal(api(h).bookmarkText(h.editor, "중요"), "나다라");
  const runs = saved(h)[0].runs;
  assert.deepEqual(runs.map((r) => [r.text, r.bookmarkStart, r.bookmarkEnd]), [["가", undefined, undefined], ["", "중요", undefined], ["나다라", undefined, undefined], ["", undefined, "중요"], ["마", undefined, undefined]]);
});

test("책갈피 refuses a name Word refuses and a bookmark with nothing selected", async () => {
  const h = await createRunHarness([p("가나다")]);
  const text = h.blocks[0].firstChild;
  await caretAt(h, text, 1);
  assert.match(api(h).addBookmark({ document: h.document, editor: h.editor, runs: { currentRange: () => rangeOf(text, 1, text, 1), withUndo: (fn) => fn() } }, "좋은"), /먼저 선택/);
  await select(h, rangeOf(text, 0, text, 2));
  const env = { document: h.document, editor: h.editor, runs: { currentRange: () => rangeOf(text, 0, text, 2), withUndo: (fn) => fn() } };
  for (const bad of ["", "1번", "a b", "a-b", "_Toc12", "x".repeat(41)]) assert.match(api(h).addBookmark(env, bad), /책갈피 이름/, JSON.stringify(bad));
});

test("a bookmark with the same name moves, and 삭제 takes both markers away", async () => {
  const h = await createRunHarness([p("가나다라마")]);
  const text = h.blocks[0].firstChild;
  await select(h, rangeOf(text, 0, text, 2));
  await run(h, "insert-bookmark");
  byId(h, "doc-bm-name").value = "옮김";
  click(h, "btn-doc-bm-add");
  await flushAsyncTurns(6);
  assert.equal(api(h).visibleBookmarks(h.editor).length, 1);
  const again = h.editor.firstChild.childNodes.find((n) => n.nodeType === 3 && n.nodeValue === "다라마");
  await select(h, rangeOf(again, 0, again, 2));
  await run(h, "insert-bookmark");
  assert.deepEqual(Array.from(byId(h, "doc-bm-list").querySelectorAll("li"), (li) => li.textContent), ["옮김"]);
  byId(h, "doc-bm-name").value = "옮김";
  click(h, "btn-doc-bm-add");
  await flushAsyncTurns(6);
  assert.equal(api(h).bookmarkText(h.editor, "옮김"), "다라");
  await run(h, "insert-bookmark");
  byId(h, "doc-bm-name").value = "옮김";
  click(h, "btn-doc-bm-delete");
  assert.equal(h.editor.querySelectorAll("[data-eo-bookmark-start], [data-eo-bookmark-end]").length, 0);
});

test("캡션 삽입 puts a numbered caption paragraph under the picture and renumbers by order", async () => {
  const h = await createRunHarness([p("앞"), p("뒤")]);
  const target = h.blocks[0].firstChild;
  await caretAt(h, target, 1);
  await run(h, "insert-caption");
  assert.equal(byId(h, "doc-caption-dialog").open, true);
  assert.equal(byId(h, "doc-cap-text").value, "그림 1");
  byId(h, "doc-cap-text").value = "그림 1. 첫 그림";
  click(h, "btn-doc-cap-ok");
  await flushAsyncTurns(6);
  const [, first] = saved(h);
  assert.equal(first.caption, true);
  assert.deepEqual(first.runs.map((r) => [r.text, r.fieldCode]), [["그림 ", undefined], ["1", "SEQ 그림 \\* ARABIC"], [". 첫 그림", undefined]]);
  // A caption above the first one takes number 1 and the old first becomes 2 when the fields are updated.
  const caption = h.editor.querySelector("[data-eo-caption]");
  await caretAt(h, h.blocks[0].firstChild, 0);
  await run(h, "insert-caption");
  change(h, "doc-cap-pos", "above");
  click(h, "btn-doc-cap-ok");
  await flushAsyncTurns(6);
  const numbers = Array.from(h.editor.querySelectorAll("[data-eo-caption] [data-eo-field-code]"), (f) => f.textContent);
  assert.deepEqual(numbers, ["1", "2"]);
  assert.ok(h.editor.contains(caption));
});

test("상호 참조 inserts a REF field with the text of the bookmark, and 필드 업데이트 follows a changed text", async () => {
  const h = await createRunHarness([p("가나다라마"), p("끝")]);
  const text = h.blocks[0].firstChild;
  await select(h, rangeOf(text, 1, text, 3));
  await run(h, "insert-bookmark");
  byId(h, "doc-bm-name").value = "참조대상";
  click(h, "btn-doc-bm-add");
  await flushAsyncTurns(6);
  const tail = h.blocks[1].firstChild;
  await caretAt(h, tail, 1);
  await run(h, "insert-xref");
  assert.equal(byId(h, "doc-xref-dialog").open, true);
  assert.deepEqual(Array.from(byId(h, "doc-xref-type").querySelectorAll("option"), (o) => o.textContent), ["책갈피", "제목"]);
  const items = Array.from(byId(h, "doc-xref-list").querySelectorAll("li"));
  assert.deepEqual(items.map((li) => li.textContent), ["참조대상"]);
  items[0].dispatchEvent({ type: "click", target: items[0], bubbles: true });
  click(h, "btn-doc-xref-insert");
  await flushAsyncTurns(6);
  const field = h.editor.querySelector('[data-eo-field-code^="REF"]');
  assert.equal(field.dataset.eoFieldCode, "REF 참조대상 \\h");
  assert.equal(field.textContent, "나다");
  const bookmarked = h.editor.querySelector("[data-eo-bookmark-start]").nextSibling;
  bookmarked.nodeValue = "바뀜";
  await run(h, "field-update");
  assert.equal(field.textContent, "바뀜");
  assert.equal(feedback(h), "필드 업데이트 완료");
  await run(h, "field-update");
  assert.equal(feedback(h), "업데이트할 필드가 없습니다");
});

test("a reference to a missing bookmark shows the Word program's error text", async () => {
  const h = await createRunHarness([p("가")]);
  const field = h.document.createElement("span");
  field.dataset.eoFieldCode = "REF 없는책갈피 \\h";
  field.textContent = "예전 값";
  h.editor.firstChild.appendChild(field);
  assert.equal(api(h).updateFields(h.editor), 1);
  assert.equal(field.textContent, "오류! 책갈피가 정의되어 있지 않습니다.");
});

test("an opened document shows its fields and bookmark markers, and they save as they came", async () => {
  const h = await createRunHarness([p("자리")]);
  const model = h.host.__EASYOFFICE_DOC_WORD_MODEL__;
  const base = (runs) => runs.map((r) => ({ t: "text", v: r.text }));
  const specs = model.runSpecs(
    [{ text: "앞 " }, { text: "시작", bookmarkStart: "표지" }, { text: "끝", bookmarkEnd: "표지" }, { text: "2026-11-28", fieldCode: "DATE \\@ \"yyyy-MM-dd\"" }],
    {},
    base
  );
  assert.deepEqual(
    specs.map((s) => s.attrs?.class ?? s.v),
    ["앞 ", "eo-bookmark-start", "시작", "끝", "eo-bookmark-end", "eo-field-code"]
  );
  assert.equal(specs[1].data.eoBookmarkStart, "표지");
  assert.equal(specs[5].data.eoFieldCode, 'DATE \\@ "yyyy-MM-dd"');
  const hostile = model.runSpecs([{ text: "x", fieldCode: "INCLUDETEXT c:\\x", bookmarkStart: "1 나쁨" }], {}, base);
  assert.deepEqual(hostile.map((s) => s.v), ["x"], "a field or bookmark the editor does not know is plain text");
});

test("the caption flag, the section break and the header rows of an opened document ride on their elements", async () => {
  const h = await createRunHarness([p("자리")]);
  const model = h.host.__EASYOFFICE_DOC_WORD_MODEL__;
  const para = model.paragraph({ attrs: { class: "x" }, data: {} }, { caption: true, sectionBreak: "evenPage" });
  assert.deepEqual([para.attrs.class, para.data.eoCaption, para.data.eoSectionBreak], ["x eo-caption", "1", "evenPage"]);
  assert.equal(model.paragraph({ attrs: {}, data: {} }, { sectionBreak: "sideways" }).data.eoSectionBreak, undefined);
  assert.equal(model.table({ data: {} }, { headerRows: 2 }).data.eoHeaderRows, "2");
  assert.equal(model.table({ data: {} }, { headerRows: -1 }).data.eoHeaderRows, undefined);
});

const marker = (h, kind, name) => {
  const el = h.document.createElement("span");
  el.dataset[kind === "start" ? "eoBookmarkStart" : "eoBookmarkEnd"] = name;
  return el;
};
const refField = (h, name) => {
  const el = h.document.createElement("span");
  el.dataset.eoFieldCode = `REF ${name} \\h`;
  el.textContent = "x";
  return el;
};

test("nested references do not grow with the depth, and a result is cut at 4,096 characters", async () => {
  const h = await createRunHarness([p("a")]);
  const para = h.editor.firstChild;
  para.insertBefore(marker(h, "start", "b0"), para.firstChild);
  para.appendChild(marker(h, "end", "b0"));
  for (let i = 1; i <= 20; i += 1) {
    const next = h.document.createElement("p");
    next.appendChild(marker(h, "start", "b" + i));
    next.appendChild(refField(h, "b" + (i - 1)));
    next.appendChild(refField(h, "b" + (i - 1)));
    next.appendChild(marker(h, "end", "b" + i));
    h.editor.appendChild(next);
  }
  const started = Date.now();
  api(h).updateFields(h.editor);
  const last = h.editor.lastChild.querySelectorAll("[data-eo-field-code]")[1];
  assert.equal(last.textContent, "", "a reference does not carry the result of the references inside the text it names");
  assert.ok(Date.now() - started < 1000);
  const long = h.document.createElement("p");
  long.appendChild(marker(h, "start", "긴글"));
  long.appendChild(h.document.createTextNode("가".repeat(10000)));
  long.appendChild(marker(h, "end", "긴글"));
  h.editor.appendChild(long);
  assert.equal(api(h).bookmarkText(h.editor, "긴글").length, 4096);
});

test("the page accepts the bookmark names the engine accepts, combining marks and letter numbers included", async () => {
  const h = await createRunHarness([p("a")]);
  const model = h.host.__EASYOFFICE_DOC_WORD_MODEL__;
  for (const name of ["책ͅ", "Ⅳ장", "_표1", "가".repeat(40)]) assert.equal(model.isBookmarkName(name), true, name);
  for (const name of ["1장", "가".repeat(41), "_Toc1", "_GoBack", "a b", ""]) assert.equal(model.isBookmarkName(name), false, name);
});

test("a caption number inside a bookmark is part of what a reference to it shows", async () => {
  const h = await createRunHarness([p("그림 ")]);
  const para = h.editor.firstChild;
  para.insertBefore(marker(h, "start", "_Ref1"), para.firstChild);
  const seq = h.document.createElement("span");
  seq.dataset.eoFieldCode = "SEQ 그림";
  seq.textContent = "9";
  para.appendChild(seq);
  para.appendChild(marker(h, "end", "_Ref1"));
  const ref = refField(h, "_Ref1");
  para.appendChild(ref);
  api(h).updateFields(h.editor);
  assert.equal(ref.textContent, "그림 1");
});

test("many references are resolved from one pass over the document", async () => {
  const h = await createRunHarness([p("본문")]);
  const para = h.editor.firstChild;
  for (let i = 0; i < 400; i += 1) {
    para.appendChild(marker(h, "start", "m" + i));
    para.appendChild(h.document.createTextNode("글" + i));
    para.appendChild(marker(h, "end", "m" + i));
  }
  const refs = [];
  for (let i = 0; i < 400; i += 1) {
    refs.push(refField(h, "m" + i));
    para.appendChild(refs[i]);
  }
  const started = Date.now();
  assert.equal(api(h).updateFields(h.editor), 400);
  assert.ok(Date.now() - started < 1000);
  assert.deepEqual([refs[0].textContent, refs[399].textContent], ["글0", "글399"]);
});
