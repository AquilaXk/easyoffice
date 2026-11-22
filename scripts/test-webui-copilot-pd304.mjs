import assert from "node:assert/strict";
import test from "node:test";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { acceptProposal, reviewButton } from "./test-support/copilot-accept.mjs";
import { open, answer } from "./test-support/copilot-open.mjs";

// PD-304: the pointer and typing also play for a slide (a new slide, an object put down or moved, the layout, the notes)
// and for a range of paragraphs replaced at once. The change is shown first and written once at the end, as before.

const layerOf = (document) => document.getElementById("eo-copilot-cursor-layer");
const cards = (document) => Array.from(document.querySelectorAll("#slide-canvas-layer .slide-el"));
const visibleCards = (document) => cards(document).filter((card) => !card.classList.contains("eo-copilot-ghost"));
const thumbs = (document) => document.querySelectorAll("#slide-thumb-list .slide-thumb");
const zoneText = (document, id) => document.getElementById(id).textContent.trim();

async function slides(run, motion = true) {
  const h = await open({ module: "SLIDE", motion, run });
  // Plays the accepted change to its end (the harness's timers are virtual).
  h.runAccepted = async (ms = 9000) => {
    h.userClick(reviewButton(h.document, "accept"));
    await flushAsyncTurns(6);
    await h.play(ms);
  };
  h.select = async (index) => {
    thumbs(h.document)[index].click();
    await flushAsyncTurns(4);
  };
  return h;
}

const textbox = (text, extra = {}) => ({ kind: "textbox", x: 100, y: 140, w: 320, h: 80, text, ...extra });
const addElement = (element, extra = {}) => ({ op: "slide.add_element", element, ...extra });

// ---- 새 슬라이드

test("a new slide: the pointer presses 새 슬라이드, a blank slide opens, the title is typed, and one undo removes it", async () => {
  const h = await slides(() => answer([{ op: "slide.add", title: "새로 만든 슬라이드 제목", bullets: ["첫째 줄", "둘째 줄"], layout: "twoColumn" }]));
  await h.send("슬라이드를 추가해 주세요");
  assert.equal(thumbs(h.document).length, 1, "nothing is added before 수락");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  assert.ok(layerOf(h.document), "the pointer shows");
  let opened = false;
  const titles = new Set();
  for (let i = 0; i < 120; i += 1) {
    await h.play(32);
    if (thumbs(h.document).length === 2) {
      opened = true;
      assert.equal(h.document.getElementById("slide-stage-box").dataset.layout, "twoColumn", "the new slide shows its layout");
    }
    titles.add(zoneText(h.document, "slide-title-zone"));
  }
  assert.ok(opened, "a temporary thumbnail stands for the new slide");
  assert.ok([...titles].some((t) => t.length > 0 && t.length < "새로 만든 슬라이드 제목".length), "the title grows letter by letter: " + [...titles].join("|"));
  await h.play(6000);
  assert.equal(layerOf(h.document), null);
  assert.equal(thumbs(h.document).length, 2, "the real slide replaced the temporary thumbnail");
  assert.equal(zoneText(h.document, "slide-title-zone"), "새로 만든 슬라이드 제목");
  assert.equal(zoneText(h.document, "slide-lower-zone"), "첫째 줄\n둘째 줄");
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(thumbs(h.document).length, 1, "one undo takes the slide back");
});

test("stopping while a new slide's title is typed leaves a slide with what was typed", async () => {
  const h = await slides(() => answer([{ op: "slide.add", title: "멈추기 전까지만 쓰는 긴 슬라이드 제목입니다", bullets: ["본문은 쓰지 못합니다"] }]));
  await h.send("슬라이드를 추가해 주세요");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  await h.play(1400);
  const typed = zoneText(h.document, "slide-title-zone");
  assert.ok(typed.length > 0 && typed.length < 20, "typing is under way: " + typed);
  h.userClick(h.document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(14);
  assert.equal(layerOf(h.document), null);
  assert.equal(thumbs(h.document).length, 2);
  const kept = zoneText(h.document, "slide-title-zone");
  assert.ok(kept.length > 0 && "멈추기 전까지만 쓰는 긴 슬라이드 제목입니다".startsWith(kept), "the typed part stays: " + kept);
  assert.equal(zoneText(h.document, "slide-lower-zone"), "", "the body was never typed");
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(thumbs(h.document).length, 1);
});

// ---- 개체

test("a text box is drawn open with the pointer, selected, typed into, and ends as the one real object", async () => {
  const h = await slides(() => answer([addElement(textbox("글상자에 쓰는 내용입니다"))]));
  await h.send("글상자를 넣어 주세요");
  assert.equal(cards(h.document).length, 0);
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  const seen = new Set();
  let selected = false;
  for (let i = 0; i < 160; i += 1) {
    await h.play(32);
    const card = cards(h.document)[0];
    if (card) seen.add(card.querySelector(".slide-el-textbox").textContent);
    if (layerOf(h.document)?.querySelectorAll(".eo-copilot-selection").length > 0) selected = true;
  }
  assert.ok(selected, "the object is outlined as the pointer draws and selects it");
  assert.ok([...seen].some((t) => t.length > 0 && t.length < "글상자에 쓰는 내용입니다".length), "the text grows letter by letter: " + [...seen].join("|"));
  await h.play(6000);
  assert.equal(layerOf(h.document), null);
  assert.equal(cards(h.document).length, 1, "the temporary object is gone and the real one stands");
  assert.equal(cards(h.document)[0].querySelector(".slide-el-textbox").textContent, "글상자에 쓰는 내용입니다");
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(cards(h.document).length, 0, "one undo removes it");
});

test("a chart is drawn as a frame and appears whole", async () => {
  const h = await slides(() => answer([addElement({ kind: "chart", chartType: "column", x: 460, y: 240, w: 360, h: 190, title: "실적 요약" })]));
  await h.send("차트를 넣어 주세요");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  let temp = 0;
  for (let i = 0; i < 80; i += 1) {
    await h.play(32);
    temp = Math.max(temp, cards(h.document).filter((c) => c.dataset.kind === "chart").length);
  }
  assert.equal(temp, 1, "the chart shows while the pointer is still on it");
  await h.play(4000);
  assert.equal(cards(h.document).filter((c) => c.dataset.kind === "chart").length, 1);
  assert.equal(layerOf(h.document), null);
});

test("an object that is in the slide already is grabbed and moved, and the old one is hidden meanwhile", async () => {
  let call = 0;
  const h = await slides(() => {
    call += 1;
    if (call === 1) return answer([addElement(textbox("옮길 글상자", { x: 40, y: 60 }))]);
    return answer([{ op: "slide.update", elements: [{ id: "el-1", ...textbox("옮길 글상자", { x: 300, y: 200 }) }] }]);
  });
  await h.send("글상자를 넣어 주세요");
  await h.runAccepted();
  assert.equal(cards(h.document).length, 1);
  const left = () => cards(h.document)[0].style.left;
  const original = left();
  await h.send("오른쪽 아래로 옮겨 주세요");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  let hidden = false;
  const lefts = new Set();
  for (let i = 0; i < 100; i += 1) {
    await h.play(32);
    if (cards(h.document).some((c) => c.classList.contains("eo-copilot-ghost"))) hidden = true;
    lefts.add(visibleCards(h.document)[0]?.style.left);
  }
  assert.ok(hidden, "the original card is hidden while its copy moves");
  assert.ok(lefts.size > 2, "the copy glides through places in between: " + [...lefts].join(" "));
  await h.play(4000);
  assert.equal(cards(h.document).length, 1);
  assert.notEqual(left(), original);
  assert.equal(cards(h.document)[0].classList.contains("eo-copilot-ghost"), false);
  h.userClick(h.assistants()[1].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(left(), original, "one undo puts it back");
});

test("an object that the new list leaves out is selected and then removed", async () => {
  let call = 0;
  const h = await slides(() => {
    call += 1;
    if (call === 1) return answer([addElement(textbox("지울 글상자"))]);
    return answer([{ op: "slide.update", elements: [] }]);
  });
  await h.send("글상자를 넣어 주세요");
  await h.runAccepted();
  assert.equal(cards(h.document).length, 1);
  await h.send("글상자를 지워 주세요");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  await h.play(400);
  assert.ok(layerOf(h.document).querySelectorAll(".eo-copilot-selection").length > 0, "the object is selected first");
  assert.equal(visibleCards(h.document).length, 1, "and still there");
  await h.play(4000);
  assert.equal(cards(h.document).length, 0);
});

test("stopping in the middle of a text box's text keeps the box with the typed part", async () => {
  const text = "중간에 멈추면 쓴 만큼만 남는 글상자의 긴 내용입니다";
  const h = await slides(() => answer([addElement(textbox(text))]));
  await h.send("글상자를 넣어 주세요");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  await h.play(1500);
  const typing = cards(h.document)[0]?.querySelector(".slide-el-textbox").textContent;
  assert.ok(typing && typing.length > 0 && typing.length < text.length, "typing is under way: " + typing);
  h.userClick(h.document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(14);
  assert.equal(cards(h.document).length, 1);
  const kept = cards(h.document)[0].querySelector(".slide-el-textbox").textContent;
  assert.ok(kept.length > 0 && text.startsWith(kept) && kept.length < text.length, "the typed part stays: " + kept);
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(cards(h.document).length, 0);
});

test("stopping before the pointer has drawn the object leaves the slide as it was", async () => {
  const h = await slides(() => answer([addElement(textbox("그리기 전에 멈춥니다"))]));
  await h.send("글상자를 넣어 주세요");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  await h.play(100);
  h.userClick(h.document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(14);
  assert.equal(cards(h.document).length, 0);
  assert.equal(layerOf(h.document), null);
  assert.equal(h.assistants()[0].querySelector(".eo-copilot-undo-btn"), null, "nothing was written, so there is nothing to undo");
});

// ---- 서식과 노트

test("the layout is picked with the pointer, shown on the stage, and written at the end", async () => {
  const h = await slides(() => answer([{ op: "slide.update", layout: "twoColumn" }]));
  const stage = h.document.getElementById("slide-stage-box");
  const select = h.document.getElementById("slide-layout-select");
  const before = stage.dataset.layout;
  await h.send("두 개의 내용으로 바꿔 주세요");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  let shown = false;
  for (let i = 0; i < 60; i += 1) {
    await h.play(32);
    if (stage.dataset.layout === "twoColumn" && select.value === "twoColumn") shown = true;
  }
  assert.ok(shown, "the stage and the picker show the layout while the pointer is on it");
  await h.play(4000);
  assert.equal(layerOf(h.document), null);
  assert.equal(stage.dataset.layout, "twoColumn");
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(stage.dataset.layout, before);
});

test("the speaker notes are typed into their field and written at the end", async () => {
  const h = await slides(() => answer([{ op: "slide.update", notes: "발표할 때 강조할 요점입니다" }]));
  const notes = h.document.getElementById("slide-presenter-notes");
  await h.send("노트를 써 주세요");
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  const seen = new Set();
  for (let i = 0; i < 100; i += 1) {
    await h.play(32);
    seen.add(notes.value);
  }
  assert.ok([...seen].some((t) => t.length > 0 && t.length < "발표할 때 강조할 요점입니다".length), "the notes grow letter by letter: " + [...seen].join("|"));
  await h.play(4000);
  assert.equal(notes.value, "발표할 때 강조할 요점입니다");
  assert.equal(layerOf(h.document), null);
});

test("with reduced motion a slide change is applied at once and no pointer is drawn", async () => {
  const h = await slides(() => answer([{ op: "slide.add", title: "바로 적용", bullets: ["본문"] }, addElement(textbox("바로 놓임"), { slideIndex: 1 })]), false);
  await h.send("슬라이드를 추가해 주세요");
  await acceptProposal(h.document, h.userClick);
  assert.equal(layerOf(h.document), null);
  assert.equal(thumbs(h.document).length, 2);
  assert.equal(cards(h.document).length, 1);
});

test("an object on a slide the same reply added is drawn on that slide's blank stage", async () => {
  const h = await slides(() => answer([{ op: "slide.add", title: "표지" }, addElement(textbox("새 슬라이드의 글상자"), { slideIndex: 1 })]));
  await h.send("슬라이드와 글상자를 만들어 주세요");
  await h.runAccepted();
  assert.equal(layerOf(h.document), null);
  assert.equal(thumbs(h.document).length, 2);
  assert.equal(zoneText(h.document, "slide-title-zone"), "표지");
  assert.equal(cards(h.document).length, 1);
  assert.equal(cards(h.document)[0].querySelector(".slide-el-textbox").textContent, "새 슬라이드의 글상자");
});

// ---- 미리보기

test("the preview names a layout change, a new object list and the kind of a new object before anything is written", async () => {
  const h = await slides(() => answer([{ op: "slide.update", layout: "twoColumn", transition: "push", elements: [textbox("개체")] }, addElement({ kind: "chart" })]));
  await h.send("바꿔 주세요");
  const labels = Array.from(h.document.querySelectorAll("#copilot-thread .eo-copilot-change-label")).map((b) => b.textContent);
  assert.deepEqual(labels, ["슬라이드 1 레이아웃", "슬라이드 1 화면 전환", "슬라이드 1 개체", "개체 추가 (차트)"]);
  assert.equal(cards(h.document).length, 0);
  assert.ok(reviewButton(h.document, "accept") && reviewButton(h.document, "reject"));
});

test("an object moved by hand after the preview makes the change stale", async () => {
  const h = await slides(() => answer([{ op: "slide.update", layout: "blank" }]));
  await h.send("빈 화면으로 바꿔 주세요");
  const select = h.document.getElementById("slide-layout-select");
  select.value = "titleSlide";
  select.dispatchEvent({ type: "change", target: select });
  await flushAsyncTurns(4);
  h.document.getElementById("status-command-feedback").textContent = "";
  await acceptProposal(h.document, h.userClick);
  assert.equal(h.document.getElementById("status-command-feedback").textContent, "문서가 바뀌어 적용할 수 없습니다");
  assert.equal(h.document.getElementById("slide-stage-box").dataset.layout, "titleSlide");
});

// ---- 문서 범위 교체

const rangeOp = (extra, ...texts) => ({ op: "doc.replace_paragraphs", ...extra, blocks: texts.map((text) => ({ text })) });
const paragraphs = (editor) => Array.from(editor.querySelectorAll("p")).filter((p) => !p.classList.contains("manuscript-notice") && !p.classList.contains("eo-copilot-ghost")).map((p) => p.textContent);

async function range(op, motion = true) {
  const h = await open({ motion, run: () => answer([op]) });
  h.editor.innerHTML = "<p>첫째 문단</p><p>둘째 문단입니다</p><p>셋째 문단</p><p>넷째 문단</p>";
  await h.send("바꿔 주세요");
  return h;
}

test("a range of paragraphs is selected with the pointer first, then replaced by the typed blocks", async () => {
  const h = await range(rangeOp({ startIndex: 1, endIndex: 2 }, "새로 쓴 둘째 문단", "새로 쓴 셋째 문단"));
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  assert.ok(layerOf(h.document), "the pointer shows");
  let selected = 0;
  let hidden = false;
  const seen = new Set();
  for (let i = 0; i < 200; i += 1) {
    await h.play(32);
    selected = Math.max(selected, layerOf(h.document)?.querySelectorAll(".eo-copilot-selection").length ?? 0);
    if (Array.from(h.editor.querySelectorAll("p")).some((p) => p.classList.contains("eo-copilot-ghost"))) hidden = true;
    seen.add(paragraphs(h.editor).join("/"));
  }
  assert.ok(selected >= 2, "both old paragraphs are selected: " + selected);
  assert.ok(hidden, "the old range is hidden once it is typed over");
  assert.ok([...seen].some((s) => /^첫째 문단\/새로 쓴 둘째 문단\/(새로 쓴 셋째 문단\/)?넷째 문단$/.test(s) || /^첫째 문단\/새로/.test(s)), "new blocks appear in the place of the range: " + [...seen].slice(0, 6).join(" | "));
  await h.play(6000);
  assert.equal(layerOf(h.document), null);
  assert.deepEqual(paragraphs(h.editor), ["첫째 문단", "새로 쓴 둘째 문단", "새로 쓴 셋째 문단", "넷째 문단"]);
  assert.equal(h.editor.querySelectorAll(".eo-copilot-ghost").length, 0);
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.deepEqual(paragraphs(h.editor), ["첫째 문단", "둘째 문단입니다", "셋째 문단", "넷째 문단"], "one undo puts the range back");
});

test("stopping in the middle of a range keeps the blocks typed so far in place of the range", async () => {
  const h = await range(rangeOp({ startIndex: 1, endIndex: 2 }, "첫 블록은 끝까지 씁니다", "둘째 블록은 쓰다가 멈춥니다 길게 이어지는 문장"));
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  await h.play(2700);
  const typing = paragraphs(h.editor);
  assert.ok(typing.length >= 3, "typing has reached a new block: " + typing.join("/"));
  h.userClick(h.document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(14);
  const kept = paragraphs(h.editor);
  assert.equal(kept[0], "첫째 문단");
  assert.equal(kept.at(-1), "넷째 문단");
  assert.ok(kept.length >= 3 && !kept.includes("둘째 문단입니다") && !kept.includes("셋째 문단"), "the old range is gone: " + kept.join("/"));
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.deepEqual(paragraphs(h.editor), ["첫째 문단", "둘째 문단입니다", "셋째 문단", "넷째 문단"]);
});

test("stopping while the pointer is still selecting the range leaves the document untouched", async () => {
  const h = await range(rangeOp({ startIndex: 0, endIndex: 1 }, "쓰기 전에 멈춥니다"));
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  await h.play(150);
  h.userClick(h.document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(14);
  assert.deepEqual(paragraphs(h.editor), ["첫째 문단", "둘째 문단입니다", "셋째 문단", "넷째 문단"]);
  assert.equal(h.editor.querySelectorAll(".eo-copilot-ghost").length, 0);
  assert.equal(h.assistants()[0].querySelector(".eo-copilot-undo-btn"), null);
});

test("a replacement that names no range covers the whole document", async () => {
  const h = await range(rangeOp({}, "문서 전체를 바꾼 글"));
  h.userClick(reviewButton(h.document, "accept"));
  await flushAsyncTurns(6);
  await h.play(6000);
  assert.deepEqual(paragraphs(h.editor), ["문서 전체를 바꾼 글"]);
});

test("with reduced motion a range replacement is applied at once", async () => {
  const h = await range(rangeOp({ startIndex: 1, endIndex: 2 }, "바로 바뀜"), false);
  await acceptProposal(h.document, h.userClick);
  assert.equal(layerOf(h.document), null);
  assert.deepEqual(paragraphs(h.editor), ["첫째 문단", "바로 바뀜", "넷째 문단"]);
});
