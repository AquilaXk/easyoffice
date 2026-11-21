import test from "node:test";
import assert from "node:assert/strict";
import { clipboard, openPasteEditor, shape, shapes } from "./test-support/paste-harness.mjs";

// Paste into the document editor (doc-paste.js, doc-paste-model.js): the branches of the reference's onPaste, and what each
// kind of clipboard becomes in the editor's DOM.

const PNG_1x1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==";

const textOf = (node) => node.textContent;
const paragraphAt = (t, i = 0) => t.editor.childNodes.filter((n) => n.nodeType === 1)[i];

test("a paste is taken over from the browser, only in the document module", async () => {
  const t = await openPasteEditor({ paragraphs: ["abc"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  const delivered = await t.paste(clipboard({ text: "X" }));
  assert.equal(delivered, false, "preventDefault was called");
  const sheet = await openPasteEditor({ paragraphs: ["abc"], module: "SHEET" });
  sheet.caretAt(paragraphAt(sheet).firstChild, 1);
  assert.equal(await sheet.paste(clipboard({ text: "X" })), true, "other modules keep the browser's paste");
  assert.equal(textOf(paragraphAt(sheet)), "abc");
});

test("plain text: the first line joins the text before the caret, each line break is a paragraph, the last line joins the text after it", async () => {
  const t = await openPasteEditor({ paragraphs: ["앞뒤"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({ text: "가\r\n나\n\n다" }));
  assert.deepEqual(shapes(t.editor), ["p[앞가]", "p[나]", "p[<br>]", "p[다뒤]"]);
  assert.equal(t.feedback(), "붙여넣기 완료");
});

test("plain text inside a paragraph is one change, and one undo takes it back", async () => {
  const t = await openPasteEditor({ paragraphs: ["abcd"] });
  t.caretAt(paragraphAt(t).firstChild, 2);
  const before = t.steps();
  await t.paste(clipboard({ text: "XY" }));
  assert.equal(textOf(paragraphAt(t)), "abXYcd");
  assert.equal(t.steps(), before + 1);
  assert.deepEqual(t.caret().node.data, "abXYcd");
  assert.equal(t.caret().offset, 4, "the caret is after the pasted text");
  await t.undo();
  assert.deepEqual(shapes(t.editor), ["p[abcd]"]);
});

test("HTML: runs keep bold, italic, underline, size and colour; headings and alignment come across", async () => {
  const t = await openPasteEditor({ paragraphs: ["끝"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({
    html: '<h2 style="text-align:center">제목</h2><p>본문 <b>굵게</b> <i><span style="color:#ff0000">기울임</span></i> <span style="font-size:20pt;text-decoration:underline">큼</span></p>',
  }));
  const out = shapes(t.editor);
  assert.equal(out[0], "p[끝제목]", "the first block joins the caret's paragraph");
  assert.deepEqual(out.slice(1), ["p[본문 |span(b)[굵게]| |span(i,cFF0000)[기울임]| |span(u,s40)[큼]]"]);
  const empty = await openPasteEditor({ markup: "<p><br></p>" });
  empty.caretAt(paragraphAt(empty), 0);
  await empty.paste(clipboard({ html: '<h2 style="text-align:center">제목</h2><p style="text-align:right">본문</p>' }));
  assert.deepEqual(shapes(empty.editor), ["h2(al:center)[제목]", "p(al:right)[본문]"], "an empty paragraph takes the heading and the alignment");
});

test("the first paragraph of a paste joins the caret's text, the last one the text after it, the ones between stay whole", async () => {
  const t = await openPasteEditor({ paragraphs: ["앞뒤"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({ html: "<p>가</p><p>나</p><p>다</p>" }));
  assert.deepEqual(shapes(t.editor), ["p[앞가]", "p[나]", "p[다뒤]"]);
  assert.equal(t.caret().node.data, "다뒤");
  assert.equal(t.caret().offset, 1, "the caret sits at the join with the old text");
});

test("formatting inside the run at the caret does not leak into the pasted text", async () => {
  const t = await openPasteEditor({ markup: '<p><span class="eo-run" data-eo-bold="true" style="font-weight: bold">굵은글자</span></p>' });
  const bold = paragraphAt(t).firstChild;
  t.caretAt(bold.firstChild, 2);
  await t.paste(clipboard({ html: "<p>보통</p>" }));
  assert.equal(shape(paragraphAt(t)), "p[span(b)[굵은]|보통|span(b)[글자]]");
});

test("a paste at the end of an empty paragraph replaces its placeholder", async () => {
  const t = await openPasteEditor({ markup: "<p><br></p><p>다음</p>" });
  t.caretAt(paragraphAt(t), 0);
  await t.paste(clipboard({ html: "<p>내용</p>" }));
  assert.deepEqual(shapes(t.editor), ["p[내용]", "p[다음]"]);
});

test("links with a web or mail address stay; other addresses leave the text without a link", async () => {
  const t = await openPasteEditor({ paragraphs: ["x"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({ html: '<p><a href="https://example.com/a?b=1">웹</a> <a href="javascript:alert(1)">나쁨</a> <a href="mailto:a@b.c">메일</a> <a href="file:///etc/passwd">파일</a></p>' }));
  const para = paragraphAt(t);
  const links = para.childNodes.filter((n) => n.tagName === "A").map((a) => [a.getAttribute("href"), a.textContent]);
  assert.deepEqual(links, [["https://example.com/a?b=1", "웹"], ["mailto:a@b.c", "메일"]]);
  assert.equal(para.textContent, "x웹 나쁨 메일 파일");
});

test("lists: items keep their kind and level; a paste into an item continues that list", async () => {
  const t = await openPasteEditor({ paragraphs: ["앞"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({ html: "<p>머리</p><ul><li>하나<ul><li>안쪽</li></ul></li><li>둘</li></ul><ol><li>첫째</li></ol>" }));
  assert.deepEqual(shapes(t.editor).slice(1), ["ul[li[하나]|li(L1)[안쪽]|li[둘]]", "ol[li[첫째]]"]);
  const list = t.editor.childNodes.filter((n) => n.tagName === "UL")[0];
  t.caretAt(list.childNodes[0].firstChild, 1);
  await t.paste(clipboard({ html: "<p>a</p><p>b</p>" }));
  assert.equal(shape(list), "ul[li[하a]|li[b나]|li(L1)[안쪽]|li[둘]]", "pasted paragraphs are items of the list");
});

test("a table keeps its spans and goes between the two halves of the paragraph", async () => {
  const t = await openPasteEditor({ paragraphs: ["앞뒤"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({
    html: '<table><tr><td colspan="2">합</td></tr><tr><td rowspan="2">세로</td><td>b</td></tr><tr><td>c</td></tr></table>',
  }));
  assert.deepEqual(shapes(t.editor), [
    "p[앞]",
    "table[tbody[tr[td(cs2)[합]]|tr[td(rs2)[세로]|td[b]]|tr[td[c]]]]",
    "p[뒤]",
  ]);
  assert.equal(t.editor.childNodes.filter((n) => n.tagName === "TABLE")[0].className, "doc-inline-table");
});

test("a table pasted into a cell becomes the paragraphs of that cell's text, not a nested table", async () => {
  const t = await openPasteEditor({ markup: '<table class="doc-inline-table"><tbody><tr><td>가</td><td>나</td></tr></tbody></table><p>끝</p>' });
  const cell = t.editor.querySelector("td");
  t.caretAt(cell.firstChild, 1);
  await t.paste(clipboard({ html: "<table><tr><td>x</td><td>y</td></tr></table>" }));
  assert.equal(t.editor.querySelectorAll("table").length, 1);
  assert.equal(cell.textContent, "가x" + "y");
  assert.equal(shape(cell), "td[가x|<br>|y]".replace("|<br>|", "|<br>|"));
});

test("text pasted into a cell stays in the cell: lines become line breaks", async () => {
  const t = await openPasteEditor({ markup: '<table class="doc-inline-table"><tbody><tr><td>가나</td></tr></tbody></table>' });
  const cell = t.editor.querySelector("td");
  t.caretAt(cell.firstChild, 1);
  await t.paste(clipboard({ text: "1\n2" }));
  assert.equal(shape(cell), "td[가1|<br>|2나]");
  assert.equal(t.editor.querySelectorAll("tr").length, 1);
});

test("office HTML: conditional comments, style blocks, o:p and xml parts leave only the content", async () => {
  const t = await openPasteEditor({ paragraphs: ["x"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  const html = [
    '<html xmlns:o="urn:office"><head><style>p.MsoNormal{margin:0}</style><!--[if gte mso 9]><xml><o:OfficeDocumentSettings/></xml><![endif]--></head>',
    "<body lang=KO><!--StartFragment--><p class=MsoNormal><span lang=KO style='font-size:11.0pt'>안녕<o:p></o:p></span></p>",
    "<p class=MsoListParagraph><![if !supportLists]><span>·</span><![endif]>둘째 줄<o:p></o:p></p><!--EndFragment--></body></html>",
  ].join("\n");
  await t.paste(clipboard({ html }));
  assert.equal(t.editor.textContent, "x안녕둘째 줄");
  assert.deepEqual(shapes(t.editor), ["p[x|span(s22)[안녕]]", "p[둘째 줄]"]);
});

test("whitespace of the markup collapses, a line break stays, a <pre> keeps its lines", async () => {
  const t = await openPasteEditor({ paragraphs: [""] });
  t.caretAt(paragraphAt(t), 0);
  await t.paste(clipboard({ html: "<p>  a \n   b<br>c  </p><pre>x\n  y</pre>" }));
  assert.deepEqual(shapes(t.editor), ["p[a b|<br>|c]", "p[x|<br>|  y]"]);
});

test("a picture of the clipboard HTML (a PNG data address) is put in; the other pictures are left out", async () => {
  const t = await openPasteEditor({ paragraphs: ["앞뒤"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({
    html: `<p>가<img src="data:image/png;base64,${PNG_1x1}" alt="점">나<img src="https://example.com/a.png"><img src="file:///C:/a.png"><img src="data:image/svg+xml;base64,PHN2Zy8+"></p>`,
  }));
  const pictures = t.editor.querySelectorAll("img.doc-inline-image");
  assert.equal(pictures.length, 1);
  assert.equal(t.editor.querySelectorAll("img").length, 1, "no picture with an outside address");
  assert.equal(pictures[0].dataset.eoImageFormat, "png");
  assert.equal(t.editor.textContent, "앞가나뒤");
});

test("a picture file on the clipboard becomes a picture", async () => {
  const t = await openPasteEditor({ paragraphs: ["앞"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  const bytes = Buffer.from(PNG_1x1, "base64");
  const file = { type: "image/png", name: "a.png", arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.length) };
  await t.paste(clipboard({ html: "<p>무시</p>", text: "무시", files: [file] }));
  assert.equal(t.editor.querySelectorAll("img.doc-inline-image").length, 1);
  assert.ok(!t.editor.textContent.includes("무시"), "the file wins over the text of the same clipboard");
});

test("the pictures of an RTF clipboard fill the file:// pictures of its HTML", async () => {
  const t = await openPasteEditor({ paragraphs: ["앞"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  const png = Buffer.from(PNG_1x1, "base64").toString("hex");
  await t.paste(clipboard({ html: '<p><img src="file:///C:\\clip_image001.png"></p>', rtf: `{\\rtf1{\\pict\\pngblip ${png}}}` }));
  assert.equal(t.editor.querySelectorAll("img.doc-inline-image").length, 1);
});

test("a selection is replaced by the paste, across paragraphs, as one undo step", async () => {
  const t = await openPasteEditor({ paragraphs: ["하나둘", "셋넷", "다섯"] });
  const [first, , third] = t.blocks();
  t.select(first.firstChild, 2, third.firstChild, 1);
  const before = t.steps();
  await t.paste(clipboard({ text: "X" }));
  assert.deepEqual(shapes(t.editor), ["p[하나X섯]"]);
  assert.equal(t.steps(), before + 1);
  await t.undo();
  assert.deepEqual(shapes(t.editor), ["p[하나둘]", "p[셋넷]", "p[다섯]"]);
});

test("a selection over several cells is not deleted: the paste goes in at its start", async () => {
  const t = await openPasteEditor({ markup: '<table class="doc-inline-table"><tbody><tr><td>가</td><td>나</td></tr></tbody></table>' });
  const [a, b] = t.editor.querySelectorAll("td");
  t.select(a.firstChild, 0, b.firstChild, 1);
  await t.paste(clipboard({ text: "X" }));
  assert.equal(a.textContent, "X가");
  assert.equal(b.textContent, "나");
});

test("select all and paste keeps the header, footer and notes around the body", async () => {
  const t = await openPasteEditor({
    markup: '<section data-eo-hf="header"><p>머리</p></section><p>하나</p><p>둘</p><section data-eo-hf="footer"><p>꼬리</p></section><footer>끝주</footer>',
  });
  t.select(t.editor, 0, t.editor, t.editor.childNodes.length);
  await t.paste(clipboard({ text: "새글" }));
  assert.equal(t.editor.querySelectorAll("section").length, 2);
  assert.equal(t.editor.querySelectorAll("footer").length, 1);
  assert.equal(t.editor.querySelector('section[data-eo-hf="header"]').textContent, "머리");
  assert.ok(t.editor.textContent.includes("새글"));
  assert.ok(!t.editor.textContent.includes("하나") && !t.editor.textContent.includes("둘"));
});

test("a header takes the plain text only, as one line", async () => {
  const t = await openPasteEditor({ markup: '<section data-eo-hf="header"><p>머리</p></section><p>본문</p>' });
  const para = t.editor.querySelector("section").querySelector("p");
  t.caretAt(para.firstChild, 1);
  await t.paste(clipboard({ html: "<table><tr><td>표</td></tr></table><p>굵게</p>", text: "첫줄\n둘째줄" }));
  assert.equal(t.editor.querySelectorAll("table").length, 0);
  assert.equal(t.editor.querySelector("section").textContent, "머첫줄 둘째줄리");
  assert.equal(t.editor.querySelector("section").querySelectorAll("p").length, 1);
});

test("the caret goes after the pasted text and the document state is saved for the undo", async () => {
  const t = await openPasteEditor({ paragraphs: ["ab"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({ html: "<p>가</p><p>나</p>" }));
  assert.equal(t.caret().collapsed, true);
  assert.equal(t.caret().node.data, "나b");
  assert.equal(t.caret().offset, 1, "after the pasted 나, before the old b");
});

test("HTML with nothing to paste falls back to the plain text; an empty clipboard says so", async () => {
  const t = await openPasteEditor({ paragraphs: ["x"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({ html: "<script>1</script><style>p{}</style>", text: "글" }));
  assert.equal(textOf(paragraphAt(t)), "x글");
  await t.paste(clipboard({}));
  assert.equal(t.feedback(), "붙일 클립보드 내용이 없습니다");
});

test("an editor copy pastes back with its run formats", async () => {
  const t = await openPasteEditor({ paragraphs: ["x"] });
  t.caretAt(paragraphAt(t).firstChild, 1);
  await t.paste(clipboard({ html: '<p><span class="eo-run" data-eo-bold="true" data-eo-size="28" data-eo-color="FF0000" style="font-weight: bold; font-size: 14pt; color: #FF0000">복사</span></p>' }));
  assert.equal(shape(paragraphAt(t)), "p[x|span(b,s28,cFF0000)[복사]]");
});
