import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// PD-401: tab stops, paragraph borders and shading, drop caps, columns, text boxes, equations and the styles of the user of the
// word processor window: the open and save sides, the commands and the bounds.

const byId = (h, id) => h.document.getElementById(id);
const run = async (h, cmd) => {
  h.userClick(studioItem(h.document, cmd));
  await flushAsyncTurns(8);
};
const click = (h, id) => h.userClick(byId(h, id));
// Picks a radio of a group by its value (`h.document.querySelectorAll` of the harness takes one attribute selector).
const choose = (h, name, value) => {
  for (const input of h.document.querySelectorAll("input[name=\"" + name + "\"]")) input.checked = input.value === value;
};
const blocksOf = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
const meta = (h) => h.host.__EASYOFFICE_SAVE_DOC_META__.collectDocAnnotations(h.editor);
const blocksApi = (h) => h.host.__EASYOFFICE_DOC_WORD_BLOCKS__;
const caretIn = async (h, block) => {
  const node = block.childNodes.find((n) => n.nodeType === 3) ?? block;
  h.selection.addRange(rangeOf(node, 0, node, 0));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
};

test("tab stops, borders and shading are read from a paragraph, sorted, bounded and checked", async () => {
  const h = await createRunHarness([p("a")]);
  const api = blocksApi(h);
  assert.deepEqual(api.decodeTabs("8000:right:dot;2000:center:none;2000:left:none;99999:left:none;5:bad:none;100:left:wavy"), [
    { pos: 2000, align: "center", leader: "none" },
    { pos: 8000, align: "right", leader: "dot" },
  ]);
  const many = Array.from({ length: 200 }, (_, i) => `${i * 10}:left:none`).join(";");
  assert.equal(api.decodeTabs(many).length, 64);
  assert.deepEqual(api.decodeBorders("top=single,6,FF0000,4;left=double,1,-,0;bottom=dotted,6,zzzzzz,0;right=dashed,12,-,99"), { top: { style: "single", eighths: 6, color: "FF0000", space: 4 } });
  const spec = api.paragraph(
    { attrs: {}, data: {}, style: {}, children: [] },
    { tabs: [{ pos: 1500, align: "decimal", leader: "dot" }], border: { top: { style: "double", eighths: 12, color: "0000FF", space: 2 } }, shade: "ffff00", dropCap: { lines: 3, margin: true }, columnBreak: true, styleName: "강조" },
  );
  assert.equal(spec.data.eoTabs, "1500:decimal:dot");
  assert.equal(spec.data.eoBorder, "top=double,12,0000FF,2");
  assert.equal(spec.data.eoShade, "FFFF00");
  assert.equal(spec.data.eoDropcap, "3,margin");
  assert.equal(spec.data.eoColBreak, "1");
  assert.equal(spec.data.eoStyle, "강조");
  assert.match(spec.style.borderTop, /^\d+px double #0000FF$/);
  assert.equal(spec.style.backgroundColor, "#FFFF00");
  const bad = api.paragraph({ attrs: {}, data: {}, style: {}, children: [] }, { dropCap: { lines: 1 }, shade: "red", border: { top: { style: "wavy", eighths: 4, space: 0 } } });
  assert.equal(bad.data.eoDropcap, undefined);
  assert.equal(bad.data.eoShade, undefined);
  assert.equal(bad.data.eoBorder, undefined);
});

test("a paragraph saves its tab stops, border, shading, drop cap, column break, style and text box", async () => {
  const h = await createRunHarness([p("목차"), p("가"), p("나다라")]);
  const [first, cap, third] = h.editor.children;
  first.dataset.eoTabs = "8000:right:dot";
  first.dataset.eoBorder = "top=single,6,FF0000,4";
  first.dataset.eoShade = "FFFF00";
  first.dataset.eoStyle = "본문 강조";
  cap.dataset.eoDropcap = "3,drop";
  third.dataset.eoColBreak = "1";
  const box = h.document.createElement("div");
  Object.assign(box.dataset, { eoTextbox: "1", eoTbWrap: "front", eoTbW: "2143125", eoTbH: "714375", eoTbBorder: "1", eoTbFill: "FFEEAA" });
  box.appendChild(h.document.createElement("p")).appendChild(h.document.createTextNode("상자 글"));
  third.appendChild(box);
  const [b0, b1, b2] = blocksOf(h);
  assert.deepEqual(b0.tabs, [{ pos: 8000, align: "right", leader: "dot" }]);
  assert.deepEqual(b0.border, { top: { style: "single", eighths: 6, space: 4, color: "FF0000" } });
  assert.equal(b0.shade, "FFFF00");
  assert.equal(b0.styleName, "본문 강조");
  assert.deepEqual(b1.dropCap, { lines: 3, margin: false });
  assert.equal(b2.columnBreak, true);
  assert.equal(b2.runs.map((r) => r.text).join(""), "나다라", "the text of the box is not the paragraph's");
  assert.deepEqual(b2.textBoxes, [{ widthEmu: 2143125, heightEmu: 714375, wrap: "front", border: true, fill: "FFEEAA", paragraphs: [{ type: "paragraph", runs: [{ text: "상자 글" }] }] }]);
  assert.equal(blocksOf(h).length, 3, "the box adds no body paragraph");
});

test("an opened text box and equation become elements that save back as they were", async () => {
  const h = await createRunHarness([p("a")]);
  const model = h.host.__EASYOFFICE_DOC_WORD_MODEL__;
  const omml = "<m:oMath><m:f><m:num><m:r><m:t>1</m:t></m:r></m:num><m:den><m:r><m:t>2</m:t></m:r></m:den></m:f></m:oMath>";
  const specs = model.runSpecs([{ text: "앞 " }, { text: "12", math: omml }], {}, (runs) => runs.map((r) => ({ t: "text", v: r.text })));
  assert.equal(specs[1].attrs.class, "eo-math");
  assert.equal(specs[1].data.eoOmml, omml);
  assert.equal(model.runSpecs([{ text: "x", math: "<script>" }], {}, (runs) => runs.map((r) => ({ t: "text", v: r.text }))).length, 1, "text that is not OMML is plain text");
  const para = h.editor.firstChild;
  const math = h.document.createElement("span");
  math.dataset.eoOmml = omml;
  math.dataset.eoMathText = "12";
  para.appendChild(math);
  assert.deepEqual(blocksOf(h)[0].runs.at(-1), { text: "12", math: omml });
  const text = blocksApi(h).paragraph(
    { attrs: {}, data: {}, style: {}, children: [] },
    { textBoxes: [{ widthEmu: 2143125, heightEmu: 714375, wrap: "square", border: true, fill: "FFEEAA", paragraphs: [{ type: "paragraph", runs: [{ text: "상자" }] }] }, { widthEmu: 5, heightEmu: 5, paragraphs: [] }] },
    (blocks) => blocks.map((b) => ({ t: "el", tag: "p", attrs: {}, data: {}, style: {}, children: [{ t: "text", v: b.runs[0].text }] })),
  );
  assert.equal(text.children.length, 1, "a box outside the size bounds is not shown");
  assert.equal(text.children[0].data.eoTbW, "2143125");
  assert.equal(text.children[0].style.backgroundColor, "#FFEEAA");
});

test("the equation is drawn as MathML from its OMML, and what is not OMML is not drawn", async () => {
  const h = await createRunHarness([p("a")]);
  const math = h.host.__EASYOFFICE_DOC_WORD_MATH__;
  const tree = math.parseOmml("<m:oMath><m:sSup><m:e><m:r><m:t>x&amp;y</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath>");
  assert.equal(tree.children[0].children[0].name, "sSup");
  assert.equal(tree.children[0].children[0].children[0].children[0].children[0].text, "x&y");
  assert.equal(math.parseOmml("<m:oMath><m:e></m:oMath>"), null, "unbalanced");
  assert.equal(math.parseOmml("<m:oMath>x<script>alert(1)</script></m:oMath>"), null, "an element that is not m: is not read");
  assert.equal(math.parseOmml("<m:oMath>" + "<m:e>".repeat(40) + "</m:e>".repeat(40) + "</m:oMath>"), null, "too deep");
  const made = [];
  const doc = { createElementNS: (ns, name) => { const el = { ns, name, children: [], attrs: {}, setAttribute(k, v) { this.attrs[k] = v; }, appendChild(c) { this.children.push(c); } }; made.push(el); return el; }, createTextNode: (text) => ({ text }) };
  const root = math.toMathML(doc, "<m:oMath><m:f><m:num><m:r><m:t>1</m:t></m:r></m:num><m:den><m:r><m:t>a+2</m:t></m:r></m:den></m:f></m:oMath>");
  assert.equal(root.name, "math");
  const names = made.map((e) => e.name);
  assert.ok(["mfrac", "mi", "mn", "mo", "mrow"].every((n) => names.includes(n)), names.join());
  assert.equal(math.toMathML(doc, "<w:p/>"), null);
  assert.equal(math.toMathML(doc, "<m:oMath><m:f></m:oMath>"), null);
});

test("columns: the presets, the dialog, the separator and one undo step", async () => {
  const h = await createRunHarness([p("a"), p("b")]);
  await run(h, "layout-columns-2");
  assert.equal(h.editor.style.columnCount, "2");
  assert.equal(meta(h).docColumnGap, undefined, "the default gap and look are not sent");
  await run(h, "layout-columns-right");
  assert.equal(h.editor.style.columnCount, "2");
  assert.equal(meta(h).docColumnNarrow, "last");
  await run(h, "layout-columns");
  assert.equal(byId(h, "doc-columns-dialog").open, true);
  byId(h, "doc-col-count").value = "3";
  byId(h, "doc-col-gap").value = "10";
  byId(h, "doc-col-sep").checked = true;
  click(h, "btn-doc-col-ok");
  await flushAsyncTurns(8);
  assert.equal(h.editor.style.columnCount, "3");
  assert.equal(h.editor.style.columnRule, "1px solid #000000");
  assert.deepEqual([meta(h).docColumnGap, meta(h).docColumnSeparator, meta(h).docColumnNarrow], [567, true, undefined]);
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(8);
  assert.equal(h.editor.style.columnCount, "2");
  await run(h, "layout-columns-1");
  assert.equal(h.editor.style.columnCount, "");
  assert.equal(h.editor.style.columnRule, "");
});

test("columns refuse a count or a gap outside the bounds", async () => {
  const h = await createRunHarness([p("a")]);
  await run(h, "layout-columns");
  byId(h, "doc-col-count").value = "99";
  click(h, "btn-doc-col-ok");
  assert.match(byId(h, "doc-col-status").textContent, /단 개수/);
  assert.equal(h.editor.style.columnCount, "");
  byId(h, "doc-col-count").value = "2";
  byId(h, "doc-col-gap").value = "9999";
  click(h, "btn-doc-col-ok");
  assert.match(byId(h, "doc-col-status").textContent, /간격/);
});

test("단 나누기 marks the paragraph at the caret and a second use takes it back", async () => {
  const h = await createRunHarness([p("a"), p("b")]);
  await caretIn(h, h.editor.children[1]);
  await run(h, "layout-column-break");
  assert.equal(h.editor.children[1].dataset.eoColBreak, "1");
  assert.equal(blocksOf(h)[1].columnBreak, true);
  await run(h, "layout-column-break");
  assert.equal(h.editor.children[1].dataset.eoColBreak, undefined);
});

test("탭 sets the stops of the paragraphs at the caret, as one undo step, and refuses a position outside the page", async () => {
  const h = await createRunHarness([p("가\t나")]);
  await caretIn(h, h.editor.firstChild);
  await run(h, "para-tabs");
  assert.equal(byId(h, "doc-tabs-dialog").open, true);
  byId(h, "doc-tab-pos").value = "999";
  click(h, "btn-doc-tab-set");
  assert.match(byId(h, "doc-tab-status").textContent, /558mm/);
  byId(h, "doc-tab-pos").value = "50";
  choose(h, "doc-tab-align", "right");
  choose(h, "doc-tab-leader", "dot");
  click(h, "btn-doc-tab-set");
  click(h, "btn-doc-tab-ok");
  await flushAsyncTurns(8);
  assert.equal(h.editor.firstChild.dataset.eoTabs, "2835:right:dot");
  assert.deepEqual(blocksOf(h)[0].tabs, [{ pos: 2835, align: "right", leader: "dot" }]);
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(8);
  assert.equal(h.editor.firstChild.dataset.eoTabs, undefined);
});

test("테두리 및 음영 puts the border and the fill on the paragraphs and takes them off", async () => {
  const h = await createRunHarness([p("테두리")]);
  await caretIn(h, h.editor.firstChild);
  await run(h, "format-borders");
  assert.equal(byId(h, "doc-para-border-dialog").open, true);
  h.document.querySelector("#doc-para-border-dialog [data-pb-preset=\"box\"]").dispatchEvent({ type: "click" });
  h.userClick(h.document.querySelector("#doc-para-border-dialog [data-pb-preset=\"box\"]"));
  h.userClick(h.document.querySelector("#doc-para-border-dialog [data-pb-style=\"double\"]"));
  byId(h, "doc-pb-shade-on").checked = true;
  byId(h, "doc-pb-shade").value = "#ffff00";
  click(h, "btn-doc-pb-ok");
  await flushAsyncTurns(8);
  const block = blocksOf(h)[0];
  assert.deepEqual(Object.keys(block.border).sort(), ["bottom", "left", "right", "top"]);
  assert.equal(block.border.top.style, "double");
  assert.equal(block.shade, "FFFF00");
  assert.match(h.editor.firstChild.style.borderTop, /double/);
  await run(h, "format-borders");
  h.userClick(h.document.querySelector("#doc-para-border-dialog [data-pb-preset=\"none\"]"));
  byId(h, "doc-pb-shade-on").checked = false;
  click(h, "btn-doc-pb-ok");
  await flushAsyncTurns(8);
  assert.equal(blocksOf(h)[0].border, undefined);
  assert.equal(blocksOf(h)[0].shade, undefined);
});

test("드롭 캡 splits the first letter into a paragraph of its own and 없음 puts it back", async () => {
  const h = await createRunHarness([p("가나다라")]);
  await caretIn(h, h.editor.firstChild);
  await run(h, "insert-dropcap-drop");
  assert.equal(h.editor.children.length, 2);
  assert.equal(h.editor.children[0].dataset.eoDropcap, "3,drop");
  assert.equal(h.editor.children[0].textContent, "가");
  assert.equal(h.editor.children[1].textContent, "나다라");
  assert.deepEqual(blocksOf(h)[0].dropCap, { lines: 3, margin: false });
  await run(h, "insert-dropcap-margin");
  assert.equal(h.editor.children[0].dataset.eoDropcap, "3,margin");
  await run(h, "insert-dropcap-none");
  assert.equal(h.editor.children.length, 1);
  assert.equal(h.editor.firstChild.textContent, "가나다라");
});

test("드롭 캡 옵션 takes the lines of the dialog and refuses a count outside 2 to 10", async () => {
  const h = await createRunHarness([p("가나다라")]);
  await caretIn(h, h.editor.firstChild);
  await run(h, "insert-dropcap-options");
  choose(h, "doc-dc-pos", "drop");
  byId(h, "doc-dc-lines").value = "11";
  click(h, "btn-doc-dc-ok");
  assert.match(byId(h, "doc-dc-status").textContent, /2에서 10/);
  byId(h, "doc-dc-lines").value = "5";
  click(h, "btn-doc-dc-ok");
  await flushAsyncTurns(8);
  assert.equal(h.editor.children[0].dataset.eoDropcap, "5,drop");
});

test("텍스트 상자 is inserted at the caret, saved with its paragraphs, and takes the wrap commands", async () => {
  const h = await createRunHarness([p("본문")]);
  await caretIn(h, h.editor.firstChild);
  await run(h, "insert-textbox");
  const box = h.editor.querySelector(".eo-textbox");
  assert.ok(box, "a box");
  assert.equal(box.dataset.eoTbWrap, "square");
  box.querySelector("p").childNodes.forEach((n) => n.remove?.());
  box.querySelector("p").appendChild(h.document.createTextNode("상자 글"));
  const node = box.querySelector("p").firstChild;
  h.selection.addRange(rangeOf(node, 0, node, 0));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  await run(h, "image-wrap-behind");
  assert.equal(box.dataset.eoTbWrap, "behind");
  await run(h, "image-wrap-inline");
  assert.equal(box.dataset.eoTbWrap, "behind", "a text box is never in the line");
  const saved = blocksOf(h)[0].textBoxes;
  assert.equal(saved.length, 1);
  assert.deepEqual([saved[0].wrap, saved[0].paragraphs[0].runs[0].text], ["behind", "상자 글"]);
});

test("스타일: a new style is listed, applied as direct formatting, changed everywhere and saved with the document", async () => {
  const h = await createRunHarness([p("제목 글"), p("다른 글")]);
  await caretIn(h, h.editor.firstChild);
  await run(h, "format-styles");
  assert.equal(byId(h, "doc-styles-dialog").open, true);
  click(h, "btn-doc-styles-new");
  byId(h, "doc-st-name").value = "본문 강조";
  byId(h, "doc-st-size").value = "14";
  byId(h, "doc-st-bold").checked = true;
  byId(h, "doc-st-align").value = "center";
  byId(h, "doc-st-after").value = "12";
  click(h, "btn-doc-st-ok");
  await flushAsyncTurns(8);
  assert.deepEqual(meta(h).styles, [{ name: "본문 강조", sizeHalfPt: 28, spaceAfterTwips: 240, align: "center", bold: true }]);
  const list = Array.from(byId(h, "doc-styles-list").querySelectorAll("option")).map((o) => o.value);
  assert.deepEqual(list.slice(-2), ["제목 6", "본문 강조"]);
  byId(h, "doc-styles-list").value = "본문 강조";
  click(h, "btn-doc-styles-apply");
  await flushAsyncTurns(8);
  assert.equal(h.editor.firstChild.dataset.eoStyle, "본문 강조");
  assert.equal(h.editor.firstChild.style.textAlign, "center");
  const first = blocksOf(h)[0];
  assert.equal(first.styleName, "본문 강조");
  assert.equal(first.align, "center");
  assert.equal(first.runs[0].bold, true);
  assert.equal(first.runs[0].sizeHalfPt, 28);
  assert.equal(blocksOf(h)[1].styleName, undefined);
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(8);
  assert.equal(h.editor.firstChild.dataset.eoStyle, undefined, "applying a style is one undo step");
});

test("스타일 refuses the names of the built-in styles, a repeated name and a value outside its bounds", async () => {
  const h = await createRunHarness([p("a")]);
  await caretIn(h, h.editor.firstChild);
  await run(h, "format-styles");
  for (const [name, size, expected] of [["제목 1", "", /기본 스타일/], ["Caption", "", /기본 스타일/], ["", "", /이름/], ["내 스타일", "9999", /글자 크기/]]) {
    click(h, "btn-doc-styles-new");
    byId(h, "doc-st-name").value = name;
    byId(h, "doc-st-size").value = size;
    click(h, "btn-doc-st-ok");
    assert.match(byId(h, "doc-st-status").textContent, expected, name);
  }
  assert.equal(meta(h).styles, undefined);
  byId(h, "doc-st-name").value = "내 스타일";
  byId(h, "doc-st-size").value = "";
  click(h, "btn-doc-st-ok");
  await flushAsyncTurns(6);
  click(h, "btn-doc-styles-new");
  byId(h, "doc-st-name").value = "내 스타일";
  click(h, "btn-doc-st-ok");
  assert.match(byId(h, "doc-st-status").textContent, /같은 이름/);
});

test("the settings in eoWordMeta are cleaned: hostile styles and columns are dropped", async () => {
  const h = await createRunHarness([p("a")]);
  h.editor.dataset.eoWordMeta = JSON.stringify({
    styles: [{ name: "제목 2" }, { name: "좋음", sizeHalfPt: 99999, color: "red", align: "wavy", bold: 1 }, { name: " 앞공백" }, { name: "좋음" }, ...Array.from({ length: 100 }, (_, i) => ({ name: "스타일" + i }))],
    columns: { gap: -5, sep: "yes", narrow: "middle" },
  });
  const out = meta(h);
  assert.equal(out.styles.length, 64);
  assert.deepEqual(out.styles[0], { name: "좋음" });
  assert.equal(out.docColumnGap, undefined);
});
