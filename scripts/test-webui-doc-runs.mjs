import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p, span, textNodeOf, flatRuns, dataOf, fireChange, click } from "./test-support/doc-run-harness.mjs";

test("changing the size of part of a paragraph affects only those characters", async () => {
  const h = await createRunHarness([p("안녕하세요 반갑습니다"), p("다른 문단")]);
  const [first, second] = h.blocks;
  h.selection.addRange(rangeOf(textNodeOf(first), 2, textNodeOf(first), 5));

  fireChange(h.document, "doc-font-size-input", "14");
  await flushAsyncTurns(10);

  assert.deepEqual(flatRuns(first), [
    { text: "안녕" },
    { text: "하세요", eoSize: "28" },
    { text: " 반갑습니다" },
  ]);
  assert.equal(first.childNodes[1].style.fontSize, "14pt");
  assert.equal(h.editor.style.fontSize, "", "the whole document must not be resized");
  assert.deepEqual(flatRuns(second), [{ text: "다른 문단" }], "other paragraphs are untouched");
});

test("size, bold and color stack on the same selection into one merged run", async () => {
  const h = await createRunHarness([p("안녕하세요 반갑습니다")]);
  const [first] = h.blocks;
  h.selection.addRange(rangeOf(textNodeOf(first), 2, textNodeOf(first), 5));

  fireChange(h.document, "doc-font-size-input", "14");
  await click(h, "btn-stylebar-bold");
  fireChange(h.document, "doc-textcolor-input", "#ff0000");
  await flushAsyncTurns(10);

  const kids = first.childNodes;
  assert.equal(kids.length, 3, "one run between the two plain parts");
  assert.equal(kids[0].data, "안녕");
  assert.equal(kids[2].data, " 반갑습니다");
  assert.deepEqual(dataOf(kids[1]), { eoSize: "28", eoBold: "true", eoColor: "FF0000" });
  assert.equal(kids[1].textContent, "하세요");
  assert.equal(kids[1].style.fontWeight, "bold");
  assert.equal(kids[1].style.color, "#FF0000");
});

test("font family, highlight, italic, underline and strikethrough apply to the selection only", async () => {
  const h = await createRunHarness([p("가나다라마바")]);
  const [first] = h.blocks;
  h.selection.addRange(rangeOf(textNodeOf(first), 1, textNodeOf(first), 3));

  const select = h.document.getElementById("doc-font-select");
  select.value = "함초롬바탕";
  select.dispatchEvent({ type: "change", target: select });
  await click(h, "btn-stylebar-italic");
  await click(h, "btn-stylebar-underline");
  await click(h, "btn-stylebar-strike");
  fireChange(h.document, "doc-highlight-input", "#ffff00");
  await flushAsyncTurns(10);

  const run = first.childNodes[1];
  assert.equal(run.textContent, "나다");
  assert.deepEqual(dataOf(run), {
    eoFont: "함초롬바탕",
    eoHighlight: "FFFF00",
    eoItalic: "true",
    eoUnderline: "true",
    eoStrike: "true",
  });
  assert.equal(run.style.textDecoration, "underline line-through");
  assert.equal(first.childNodes[0].data, "가");
  assert.equal(first.childNodes[2].data, "라마바");
});

test("a selection spanning paragraphs and table cells formats each selected piece", async () => {
  const h = await createRunHarness([
    p("AAAA"),
    p("BBBB"),
    { tag: "table", children: [{ tag: "tbody", children: [{ tag: "tr", children: [{ tag: "td", children: ["CCCC"] }, { tag: "td", children: ["DDDD"] }] }] }] },
  ]);
  const [p1, p2, table] = h.blocks;
  const [c1, c2] = table.querySelectorAll("td");
  const range = rangeOf(textNodeOf(p1), 2, textNodeOf(c1), 2);

  const res = h.runs.applyFormat(h.editor, range, { bold: true, size: 40 });
  assert.equal(res.changed, true);

  assert.deepEqual(flatRuns(p1), [{ text: "AA" }, { text: "AA", eoSize: "40", eoBold: "true" }]);
  assert.deepEqual(flatRuns(p2), [{ text: "BBBB", eoSize: "40", eoBold: "true" }]);
  assert.deepEqual(flatRuns(c1), [{ text: "CC", eoSize: "40", eoBold: "true" }, { text: "CC" }]);
  assert.deepEqual(flatRuns(c2), [{ text: "DDDD" }], "text after the selection is untouched");
  assert.equal(res.range.startContainer, p1.childNodes[1].childNodes[0]);
});

test("adjacent identical runs merge and property-less or empty runs disappear", async () => {
  const h = await createRunHarness([p("0123456789")]);
  const [para] = h.blocks;
  const t = () => para.childNodes[0];

  h.runs.applyFormat(h.editor, rangeOf(t(), 0, t(), 4), { bold: true });
  const second = para.childNodes[1];
  h.runs.applyFormat(h.editor, rangeOf(second, 0, second, 3), { bold: true });
  assert.deepEqual(flatRuns(para), [
    { text: "0123456", eoBold: "true" },
    { text: "789" },
  ], "a bold stretch next to a bold run merges into it");

  h.runs.applyFormat(h.editor, rangeOf(para.childNodes[0].childNodes[0], 1, para.childNodes[0].childNodes[0], 3), { bold: true });
  assert.equal(para.childNodes.length, 2, "re-applying bold inside a bold run changes nothing");
  h.runs.applyFormat(h.editor, rangeOf(para.childNodes[1], 0, para.childNodes[1], 2), { bold: true });
  assert.equal(para.childNodes.length, 2);
  assert.equal(para.childNodes[0].textContent, "012345678", "neighbouring bold stretches merged into one run");

  h.runs.applyFormat(h.editor, rangeOf(para.childNodes[0].childNodes[0], 0, para.childNodes[0].childNodes[0], 1), { bold: null });
  assert.deepEqual(flatRuns(para), [
    { text: "0" },
    { text: "12345678", eoBold: "true" },
    { text: "9" },
  ]);

  const whole = rangeOf(para.childNodes[0], 0, para.childNodes[2], 1);
  h.runs.applyFormat(h.editor, whole, { bold: null });
  assert.equal(para.childNodes.length, 1, "all runs unwrapped and merged back into one text node");
  assert.equal(para.childNodes[0].data, "0123456789");
  assert.equal(para.querySelectorAll("span").length, 0);
});

test("legacy inline formatting is preserved around the formatted part", async () => {
  const h = await createRunHarness([p("앞", { tag: "strong", children: ["굵은글자"] }, "뒤")]);
  const [para] = h.blocks;
  const bold = para.childNodes[1];
  h.runs.applyFormat(h.editor, rangeOf(bold.childNodes[0], 1, bold.childNodes[0], 3), { size: 36, color: "#00ff00" });

  const tags = para.childNodes.map((n) => (n.nodeType === 3 ? "#text" : n.tagName.toLowerCase()));
  assert.deepEqual(tags, ["#text", "strong", "span", "strong", "#text"]);
  const mid = para.childNodes[2];
  assert.equal(mid.textContent, "은글");
  assert.deepEqual(dataOf(mid), { eoSize: "36", eoColor: "00FF00", eoBold: "true" }, "bold carried over from the lifted <strong>");
  assert.equal(para.childNodes[1].textContent, "굵");
  assert.equal(para.childNodes[3].textContent, "자");
});

test("invalid values are ignored and never reach style or dataset", async () => {
  const h = await createRunHarness([p("abcdef")]);
  const [para] = h.blocks;
  const t = textNodeOf(para);
  for (const patch of [
    { color: "red;background:url(javascript:1)" },
    { color: "url(x)" },
    { font: 'a"; x:y' },
    { size: 99999 },
    { size: 1 },
    { bold: "yes" },
    {},
  ]) {
    const res = h.runs.applyFormat(h.editor, rangeOf(t, 0, t, 3), patch);
    assert.equal(res.changed, false, JSON.stringify(patch));
  }
  assert.equal(para.querySelectorAll("span").length, 0);
});

test("each formatting action is one undo step and redo restores the runs", async () => {
  const h = await createRunHarness([p("안녕하세요 반갑습니다")]);
  const [first] = h.blocks;
  h.selection.addRange(rangeOf(textNodeOf(first), 2, textNodeOf(first), 5));
  fireChange(h.document, "doc-font-size-input", "14");
  await flushAsyncTurns(10);
  assert.equal(h.editor.querySelectorAll(".eo-run").length, 1);

  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(40);
  const restored = h.document.getElementById("doc-editor");
  assert.equal(restored.querySelectorAll(".eo-run").length, 0, "one undo removes the whole size change");
  assert.equal(restored.textContent, "안녕하세요 반갑습니다");

  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(40);
  const redone = h.document.getElementById("doc-editor");
  const run = redone.querySelector(".eo-run");
  assert.equal(run.textContent, "하세요");
  assert.equal(run.dataset.eoSize, "28");
  assert.equal(run.style.fontSize, "14pt", "run style is restored from the snapshot");
});

test("the size box shows a blank mixed state across several sizes and the value otherwise", async () => {
  const h = await createRunHarness([p("가나다라마바")]);
  const [para] = h.blocks;
  const sizeInput = h.document.getElementById("doc-font-size-input");
  h.runs.applyFormat(h.editor, rangeOf(textNodeOf(para), 2, textNodeOf(para), 4), { size: 28 });

  h.selection.addRange(rangeOf(para.childNodes[0], 0, para.childNodes[2], 2));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  assert.equal(sizeInput.value, "");
  assert.equal(sizeInput.dataset.mixed, "true");

  h.selection.addRange(rangeOf(para.childNodes[1].childNodes[0], 0, para.childNodes[1].childNodes[0], 2));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  assert.equal(sizeInput.value, "14.0");
  assert.equal(sizeInput.dataset.mixed, "false");

  h.selection.addRange(rangeOf(para.childNodes[0], 0, para.childNodes[0], 2));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  assert.equal(sizeInput.value, "11.0", "text without a run size shows the document default");
});

test("bold state is mixed when only part of the selection is bold and toggling bolds everything", async () => {
  const h = await createRunHarness([p("가나다라")]);
  const [para] = h.blocks;
  h.runs.applyFormat(h.editor, rangeOf(textNodeOf(para), 0, textNodeOf(para), 2), { bold: true });
  h.selection.addRange(rangeOf(para.childNodes[0].childNodes[0], 0, para.childNodes[1], 2));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  const boldBtn = h.document.getElementById("btn-stylebar-bold");
  assert.equal(boldBtn.getAttribute("aria-pressed"), "mixed");

  await click(h, "btn-stylebar-bold");
  assert.equal(h.editor.querySelectorAll(".eo-run").length, 1);
  assert.equal(h.editor.querySelector(".eo-run").textContent, "가나다라");
  assert.equal(h.document.getElementById("btn-stylebar-bold").getAttribute("aria-pressed"), "true");

  await click(h, "btn-stylebar-bold");
  assert.equal(h.editor.querySelectorAll(".eo-run").length, 0, "second toggle removes bold from all");
});

test("a caret inside a word formats the word; a caret at a boundary sets a pending format", async () => {
  const h = await createRunHarness([p("hello world")]);
  const [para] = h.blocks;
  h.selection.addRange(rangeOf(textNodeOf(para), 8, textNodeOf(para), 8));
  await click(h, "btn-stylebar-bold");
  assert.deepEqual(flatRuns(para), [{ text: "hello " }, { text: "world", eoBold: "true" }]);

  const first = para.childNodes[0];
  h.selection.addRange(rangeOf(first, 5, first, 5));
  fireChange(h.document, "doc-font-size-input", "20");
  await flushAsyncTurns(10);
  assert.equal(para.querySelectorAll(".eo-run").length, 1, "nothing formatted yet: pending only");
});

test("pending format applies to the next typed text only", async () => {
  const h = await createRunHarness([p("ab cd")]);
  const [para] = h.blocks;
  const t = textNodeOf(para);
  h.selection.addRange(rangeOf(t, 2, t, 2));
  await click(h, "btn-stylebar-bold");
  assert.equal(para.querySelectorAll(".eo-run").length, 0);

  const ev = { type: "beforeinput", inputType: "insertText", data: "X", isComposing: false, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
  h.editor._listeners.get("beforeinput")[0](ev);
  assert.equal(ev.defaultPrevented, true);
  assert.deepEqual(flatRuns(para), [{ text: "ab" }, { text: "X", eoBold: "true" }, { text: " cd" }]);
  assert.equal(para.textContent, "abX cd");
});

test("save payload carries run size, font, color and highlight and keeps neighbours plain", async () => {
  const h = await createRunHarness([p("앞 문장 강조 구간 뒤 문장"), p("다른 문단")]);
  const [para] = h.blocks;
  const t = textNodeOf(para);
  h.runs.applyFormat(h.editor, rangeOf(t, 5, t, 10), {
    size: 28, bold: true, color: "#ff0000", font: "함초롬바탕", highlight: "#ffff00",
  });

  h.userClick(studioItem(h.document, "studio-save"));
  await flushAsyncTurns(20);
  const payload = h.converts.at(-1);
  assert.equal(payload.blocks.length, 2);
  assert.deepEqual(payload.blocks[0].runs, [
    { text: "앞 문장 " },
    {
      text: "강조 구간", bold: true, sizeHalfPt: 28, font: "함초롬바탕", color: "FF0000", highlight: "FFFF00",
    },
    { text: " 뒤 문장" },
  ]);
  assert.deepEqual(payload.blocks[1].runs, [{ text: "다른 문단" }]);
});

test("autosave snapshots capture the runs and restoring them rebuilds the run styles", async () => {
  const storage = new Map();
  const h = await createRunHarness([p("가나다라마")], {
    localStorage: {
      getItem: (k) => (storage.has(k) ? storage.get(k) : null),
      setItem: (k, v) => storage.set(k, String(v)),
      removeItem: (k) => storage.delete(k),
    },
  });
  const [para] = h.blocks;
  h.runs.applyFormat(h.editor, rangeOf(textNodeOf(para), 1, textNodeOf(para), 3), { size: 36, font: "바탕", color: "#0000ff" });
  h.editor.dispatchEvent({ type: "input", target: h.editor });
  h.advanceTimers(5000);
  await flushAsyncTurns(30);

  const raw = [...storage.values()].find((v) => v.includes("eoSize"));
  assert.ok(raw, "the autosave record contains the run dataset");
  const record = JSON.parse(raw);
  const nodes = record.snapshot.docTabs.DOC.editorNodes;
  const run = nodes.flatMap((n) => n.children || []).find((c) => c.tag === "span");
  assert.equal(run.data.eoSize, "36");
  assert.equal(run.style.fontSize, "18pt");
  assert.equal(run.style.fontFamily, '"바탕"');
  assert.equal(run.style.color, "#0000FF");
});

test("indentation commands act on the selected paragraphs only and the ruler pin drags one paragraph", async () => {
  const h = await createRunHarness([p("첫째"), p("둘째"), p("셋째")]);
  const [a, b, c] = h.blocks;
  h.selection.addRange(rangeOf(textNodeOf(a), 0, textNodeOf(b), 1));
  h.document.querySelector('.md-item[data-studio-menu-cmd="indent-in"]').click();
  await flushAsyncTurns(10);
  assert.equal(a.style.paddingLeft, "16px");
  assert.equal(b.style.paddingLeft, "16px");
  assert.equal(c.style.paddingLeft, "", "the unselected paragraph keeps its indent");

  h.selection.addRange(rangeOf(textNodeOf(c), 0, textNodeOf(c), 0));
  const pin = h.document.getElementById("ruler-pin-left-margin");
  pin.dispatchEvent({ type: "pointerdown", clientX: 100, pointerId: 1 });
  pin.dispatchEvent({ type: "pointermove", clientX: 132, pointerId: 1 });
  pin.dispatchEvent({ type: "pointerup", clientX: 132, pointerId: 1 });
  await flushAsyncTurns(10);
  assert.equal(c.style.paddingLeft, "32px");
  assert.equal(a.style.paddingLeft, "16px");
  const first = h.document.getElementById("ruler-pin-first-indent");
  first.dispatchEvent({ type: "pointerdown", clientX: 0, pointerId: 1 });
  first.dispatchEvent({ type: "pointermove", clientX: 8, pointerId: 1 });
  first.dispatchEvent({ type: "pointerup", clientX: 8, pointerId: 1 });
  assert.equal(c.style.textIndent, "8px");
  assert.equal(a.style.textIndent, "");
});

test("without a selection formatting is refused with a hint instead of resizing the document", async () => {
  const h = await createRunHarness([p("본문")]);
  fireChange(h.document, "doc-font-size-input", "20");
  await flushAsyncTurns(10);
  assert.equal(h.editor.style.fontSize, "");
  assert.equal(h.editor.querySelectorAll(".eo-run").length, 0);
  assert.match(h.document.getElementById("status-command-feedback").textContent, /선택/);
});

test("class-bearing structural spans such as the heading title keep their identity", async () => {
  const h = await createRunHarness([
    { tag: "h1", children: [{ tag: "span", children: ["1"] }, { tag: "span", children: [" 선정절차 안내"] }] },
  ]);
  const [heading] = h.blocks;
  const [num, title] = heading.childNodes;
  num.className = "eo-h-num";
  title.className = "eo-h-title";
  const t = title.childNodes[0];
  h.runs.applyFormat(h.editor, rangeOf(t, 1, t, 5), { size: 40 });

  assert.equal(heading.childNodes.length, 2, "the title span is not split into clones");
  assert.deepEqual(flatRuns(title), [{ text: " " }, { text: "선정절차", eoSize: "40" }, { text: " 안내" }]);
});