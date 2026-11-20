import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { REDUCED_MOTION, FULL_MOTION, acceptProposal, rejectProposal, reviewButton } from "./test-support/copilot-accept.mjs";

// PD-268: the page side of the copilot's second round: a preview before anything is written (삭제·삽입 표시, 수락·거절),
// the pointer and typing that apply an accepted change, stop and hands-on, the streamed reply, and the question's byte limit.

async function open({ run, module = "DOC", motion = false, streaming = false, extra = {} }) {
  const seen = [];
  const clock = { now: 0 };
  const testIpc = async (command, payload, options) => {
    if (command === "run_smart_assistant") {
      const parsed = JSON.parse(payload);
      seen.push(parsed);
      if (parsed.action === "copilot_run") return run(parsed, options);
      if (parsed.action === "copilot_cancel") return { status: "ok", cancelled: true };
    }
    const cat = harness.sandbox.__EASYOFFICE_RIBBON_CATALOG__;
    const offline = harness.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
      getActiveModule: () => module,
      ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
      ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
      contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
      moduleSpecs: cat.MODULE_SPECS,
      localRibbonState: cat.createDefaultRibbonState(),
    });
    return offline[command](payload);
  };
  testIpc.streaming = streaming;
  const harness = await createWorkspaceHarness({
    search: `?module=${module}&win=w-copilot-268-${module}`,
    matchMedia: motion ? FULL_MOTION : REDUCED_MOTION,
    // The animation reads time from `performance.now`; the harness's timers are virtual, so the clock is advanced with them.
    hostEnvExtras: { performance: { now: () => clock.now }, ...extra },
    testIpc,
  });
  const { document, userClick, advanceTimers } = harness;
  await flushAsyncTurns(12);
  userClick(document.getElementById("btn-copilot-toggle"));
  userClick(document.getElementById("btn-copilot-model"));
  const ollama = Array.from(document.getElementById("copilot-model-menu").querySelectorAll('button[role="menuitemradio"]')).find((b) => b.dataset.quickKind === "ollama");
  userClick(ollama);
  await flushAsyncTurns(6);
  const input = document.getElementById("copilot-input");
  const send = async (text, turns = 14) => {
    input.value = text;
    input.dispatchEvent({ type: "input", target: input });
    userClick(document.getElementById("btn-copilot-send"));
    await flushAsyncTurns(turns);
  };
  // Moves the virtual clock and the timers on together, letting promises settle between frames.
  const play = async (ms, step = 16) => {
    for (let t = 0; t < ms; t += step) {
      clock.now += step;
      advanceTimers(step);
      await flushAsyncTurns(2);
    }
  };
  const assistants = () => document.querySelectorAll("#copilot-thread .eo-copilot-msg--assistant");
  const editor = document.getElementById("doc-editor");
  return { ...harness, seen, send, play, assistants, editor, input };
}

const replaceOp = (index, text) => ({ op: "doc.replace_paragraphs", replacements: [{ index, text }] });
const answer = (operations, extra = {}) => ({ status: "ok", mode: "edit", reply: "다듬었습니다", updatedInPlace: true, operations, locations: [], ...extra });

function firstParagraph(editor, text) {
  const p = editor.querySelector("p");
  p.textContent = text;
  return p;
}

test("an edit answer shows what it would change and writes nothing until 수락", async () => {
  const { document, send, assistants, editor } = await open({ run: () => answer([replaceOp(0, "이번 분기 매출은 크게 늘었다")]) });
  const p = firstParagraph(editor, "이번 분기 매출은 늘었다");
  await send("문장을 다듬어 주세요");
  assert.equal(p.textContent, "이번 분기 매출은 늘었다", "the document is untouched");
  const msg = assistants()[0];
  assert.ok(reviewButton(document, "accept") && reviewButton(document, "reject"));
  assert.deepEqual(Array.from(msg.querySelectorAll(".eo-copilot-review-btn")).map((b) => b.textContent.trim()), ["수락", "거절"]);
  const change = msg.querySelector(".eo-copilot-change");
  assert.equal(change.querySelector(".eo-copilot-change-label").textContent, "문단 1");
  assert.equal(change.querySelector(".eo-copilot-change-kind").textContent, "변경");
  const after = change.querySelector('[data-side="after"]');
  assert.equal(after.querySelector("ins.eo-copilot-ins").textContent.trim(), "크게");
  assert.equal(after.querySelector("del"), null);
  assert.equal(msg.querySelector(".eo-copilot-undo-btn"), null, "nothing to undo yet");
});

test("the document preview is an overlay outside the editor, so saving and undo never see it", async () => {
  const { document, send, editor } = await open({ run: () => answer([replaceOp(0, "새 문장 입니다")]) });
  firstParagraph(editor, "옛 문장 입니다");
  const nodesBefore = editor.childNodes.length;
  await send("고쳐 주세요");
  const box = document.querySelector(".eo-copilot-diff-block");
  assert.ok(box, "the preview overlay is drawn over the paragraph");
  assert.equal(box.parentElement, editor.parentElement);
  assert.equal(editor.contains(box), false);
  assert.equal(editor.childNodes.length, nodesBefore);
  assert.ok(box.querySelector("ins.eo-copilot-ins") && box.querySelector("del.eo-copilot-del"));
  assert.equal(box.getAttribute("aria-hidden"), "true");
});

test("거절 leaves the document as it was and clears the overlay", async () => {
  const { document, userClick, send, assistants, editor } = await open({ run: () => answer([replaceOp(0, "바뀐 문장")]) });
  const p = firstParagraph(editor, "원래 문장");
  await send("고쳐 주세요");
  assert.ok(await rejectProposal(document, userClick));
  assert.equal(p.textContent, "원래 문장");
  assert.equal(document.querySelector(".eo-copilot-diff-block"), null);
  assert.equal(reviewButton(document, "accept"), undefined, "the buttons are gone once decided");
  assert.match(assistants()[0].textContent, /거절함/);
  assert.equal(assistants()[0].querySelector(".eo-copilot-undo-btn"), null);
});

test("수락 writes the change and one 실행 취소 takes it back", async () => {
  const { document, userClick, send, assistants, editor } = await open({ run: () => answer([replaceOp(0, "바뀐 문장")]) });
  const p = firstParagraph(editor, "원래 문장");
  await send("고쳐 주세요");
  await acceptProposal(document, userClick);
  assert.equal(p.textContent, "바뀐 문장");
  assert.equal(document.querySelector(".eo-copilot-diff-block"), null, "the overlay goes away");
  const undo = assistants()[0].querySelector(".eo-copilot-undo-btn");
  assert.ok(undo);
  userClick(undo);
  await flushAsyncTurns(12);
  assert.equal(editor.querySelector("p").textContent, "원래 문장");
});

test("a paragraph the user edited after the preview is not overwritten", async () => {
  const { document, userClick, send, editor } = await open({ run: () => answer([replaceOp(0, "바뀐 문장")]) });
  const p = firstParagraph(editor, "원래 문장");
  await send("고쳐 주세요");
  p.textContent = "직접 고친 문장";
  await acceptProposal(document, userClick);
  assert.equal(p.textContent, "직접 고친 문장");
  assert.equal(document.getElementById("status-command-feedback").textContent, "문서가 바뀌어 적용할 수 없습니다");
  assert.equal(reviewButton(document, "accept"), undefined);
});

test("a new question drops a change that is still waiting", async () => {
  const { document, send, editor, assistants } = await open({ run: (req) => (req.prompt === "고쳐 주세요" ? answer([replaceOp(0, "바뀐 문장")]) : { status: "ok", mode: "edit", reply: "답", operations: [] }) });
  const p = firstParagraph(editor, "원래 문장");
  await send("고쳐 주세요");
  assert.ok(document.querySelector(".eo-copilot-diff-block"));
  await send("다른 질문");
  assert.equal(document.querySelector(".eo-copilot-diff-block"), null);
  assert.equal(p.textContent, "원래 문장");
  assert.match(assistants()[0].textContent, /거절함/);
});

test("with reduced motion no pointer is drawn and the change is applied at once", async () => {
  const { document, userClick, send, editor } = await open({ run: () => answer([replaceOp(0, "바뀐 문장")]) });
  firstParagraph(editor, "원래 문장");
  await send("고쳐 주세요");
  await acceptProposal(document, userClick);
  assert.equal(document.getElementById("eo-copilot-cursor-layer"), null);
  assert.equal(editor.querySelector("p").textContent, "바뀐 문장");
});

test("the pointer goes to the paragraph, selects it and types the new text a letter at a time, then disappears", async () => {
  const { document, userClick, send, play, assistants, editor } = await open({ motion: true, run: () => answer([replaceOp(0, "새로 쓴 문장입니다")]) });
  const p = firstParagraph(editor, "옛 문장");
  await send("고쳐 주세요");
  assert.equal(document.getElementById("eo-copilot-cursor-layer"), null);
  userClick(reviewButton(document, "accept"));
  await flushAsyncTurns(6);
  const layer = document.getElementById("eo-copilot-cursor-layer");
  assert.ok(layer && layer.hidden === false, "the pointer shows");
  assert.equal(layer.querySelector(".eo-copilot-label").textContent, "코파일럿");
  assert.equal(document.getElementById("btn-copilot-send").getAttribute("aria-label"), "중지");

  await play(500);
  assert.ok(layer.querySelectorAll(".eo-copilot-selection").length > 0, "the old text is selected as if dragged over");
  const samples = new Set();
  for (let i = 0; i < 60; i += 1) {
    await play(32);
    samples.add(p.textContent);
  }
  assert.ok([...samples].some((t) => t.length > 0 && t.length < "새로 쓴 문장입니다".length), "the text grows letter by letter: " + [...samples].join("|"));
  await play(3000);
  assert.equal(p.textContent, "새로 쓴 문장입니다");
  assert.equal(document.getElementById("eo-copilot-cursor-layer"), null, "the pointer is gone when it is done");
  assert.equal(document.getElementById("btn-copilot-send").getAttribute("aria-label"), "보내기");
  const undo = assistants()[0].querySelector(".eo-copilot-undo-btn");
  userClick(undo);
  await flushAsyncTurns(12);
  assert.equal(editor.querySelector("p").textContent, "옛 문장", "the whole turn is one undo");
});

test("stopping keeps what was typed and one undo still takes everything back", async () => {
  const { document, userClick, send, play, assistants, editor } = await open({ motion: true, run: () => answer([replaceOp(0, "이 문장은 중간에 멈춥니다 끝까지 가지 않습니다")]) });
  const p = firstParagraph(editor, "옛 문장");
  await send("고쳐 주세요");
  userClick(reviewButton(document, "accept"));
  await flushAsyncTurns(6);
  await play(1100);
  const typed = p.textContent;
  assert.ok(typed.length > 0 && typed.length < 25, "typing is under way: " + typed);
  userClick(document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(14);
  const kept = p.textContent;
  assert.ok(kept.length > 0 && "이 문장은 중간에 멈춥니다 끝까지 가지 않습니다".startsWith(kept) && kept.length < 25, "what was typed stays: " + kept);
  assert.equal(document.getElementById("eo-copilot-cursor-layer"), null);
  assert.equal(document.getElementById("status-command-feedback").textContent, "코파일럿 편집을 중지했습니다");
  userClick(assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(editor.querySelector("p").textContent, "옛 문장");
});

test("a keystroke in the document stops the pointer where it is", async () => {
  const { document, userClick, send, play, editor } = await open({ motion: true, run: () => answer([replaceOp(0, "손을 대면 멈추는 문장입니다 길게 길게 이어집니다")]) });
  const p = firstParagraph(editor, "옛 문장");
  await send("고쳐 주세요");
  userClick(reviewButton(document, "accept"));
  await flushAsyncTurns(6);
  await play(1100);
  document.dispatchEvent({ type: "keydown", key: "a", target: editor });
  await flushAsyncTurns(14);
  const kept = p.textContent;
  assert.ok(kept.length > 0 && kept.length < 25, "the typed part is kept: " + kept);
  assert.equal(document.getElementById("eo-copilot-cursor-layer"), null);
  await play(2000);
  assert.equal(p.textContent, kept, "nothing more is typed after the hand was laid on");
});

test("an appended paragraph is typed into a new block that is replaced by the real one at the end", async () => {
  const doc = await open({ motion: true, run: () => answer([{ op: "doc.append", blocks: [{ text: "추가한 문단" }] }]) });
  await doc.send("문단을 추가해 주세요");
  doc.userClick(reviewButton(doc.document, "accept"));
  await flushAsyncTurns(6);
  await doc.play(300);
  const typing = Array.from(doc.editor.querySelectorAll("p")).filter((p) => "추가한 문단".startsWith(p.textContent) && p.textContent !== "");
  assert.equal(typing.length, 1, "one new block is being typed into");
  await doc.play(3000);
  assert.equal(doc.editor.textContent.split("추가한 문단").length, 2, "the typed block was replaced, not kept beside the real one");
  assert.equal(doc.document.getElementById("eo-copilot-cursor-layer"), null);
});

test("a sheet's cells are filled one after another and end as the written values", async () => {
  const sheet = await open({
    module: "SHEET",
    motion: true,
    run: () => answer([{ op: "sheet.write", cells: [{ coord: "A1", raw: "합계" }, { coord: "B1", raw: "마흔둘" }, { coord: "C1", raw: "=1+2" }] }]),
  });
  const cell = (coord) => sheet.document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
  await sheet.send("채워 주세요");
  assert.equal(cell("A1").textContent, "", "nothing is written before 수락");
  sheet.userClick(reviewButton(sheet.document, "accept"));
  await flushAsyncTurns(6);
  const layer = sheet.document.getElementById("eo-copilot-cursor-layer");
  assert.ok(layer, "the pointer shows over the sheet");
  const seen = new Set();
  for (let i = 0; i < 90; i += 1) {
    await sheet.play(32);
    seen.add(cell("A1").textContent + "|" + cell("B1").textContent);
  }
  assert.ok([...seen].some((t) => t.startsWith("합|") || t.startsWith("합계|마")), "the first cell is typed before the second: " + [...seen].join(" / "));
  await sheet.play(4000);
  assert.equal(cell("A1").textContent, "합계");
  assert.equal(cell("B1").textContent, "마흔둘");
  assert.equal(cell("C1").textContent, "3");
  assert.equal(sheet.document.getElementById("eo-copilot-cursor-layer"), null);
});

test("the reply streams in as the model writes and the operations never show", async () => {
  let releaseRun;
  const finished = new Promise((resolve) => { releaseRun = resolve; });
  const { document, send, assistants } = await open({
    streaming: true,
    run: async (req, options) => {
      assert.equal(req.stream, true);
      const text = '{"reply":"요약을 **굵게** 마쳤습니다","operations":[{"op":"doc.append","text":"비밀 연산"}]}';
      let seq = 0;
      for (let i = 0; i < text.length; i += 9) options.stream.onPiece({ requestId: req.requestId, seq: seq++, text: text.slice(i, i + 9) });
      await finished;
      return { status: "ok", mode: "edit", reply: "요약을 **굵게** 마쳤습니다", operations: [], locations: [] };
    },
  });
  await send("요약해 주세요", 8);
  const msg = assistants()[0];
  const live = msg.querySelector("[data-turn-reply]").textContent;
  assert.match(live, /^요약을 \*\*굵게\*\* 마쳤습니다$/, "the reply shows while the request is still running");
  assert.ok(!msg.textContent.includes("비밀 연산"), "operations are not shown before they are checked");
  assert.ok(msg.querySelector(".eo-copilot-loading"), "still waiting for the end");
  releaseRun();
  await flushAsyncTurns(10);
  assert.equal(assistants()[0].querySelector(".eo-copilot-loading"), null);
  assert.match(assistants()[0].textContent, /요약을 \*\*굵게\*\* 마쳤습니다/);
  assert.equal(document.getElementById("btn-copilot-send").getAttribute("aria-label"), "보내기");
});

test("a gap in the streamed pieces leaves the final answer to say what was written", async () => {
  const { send, assistants } = await open({
    streaming: true,
    run: async (req, options) => {
      options.stream.onPiece({ requestId: req.requestId, seq: 0, text: '{"reply":"앞부분' });
      options.stream.onPiece({ requestId: req.requestId, seq: 2, text: "빠진 뒤" });
      return { status: "ok", mode: "ask", reply: "온전한 답입니다", operations: [], locations: [] };
    },
  });
  await send("질문", 12);
  assert.match(assistants()[0].textContent, /온전한 답입니다/);
  assert.doesNotMatch(assistants()[0].textContent, /앞부분|빠진 뒤/);
});

test("the page asks for a stream only when the shell can stream", async () => {
  const off = await open({ streaming: false, run: () => ({ status: "ok", mode: "ask", reply: "답", operations: [] }) });
  await off.send("질문");
  assert.equal(off.seen.find((r) => r.action === "copilot_run").stream, undefined);
});

test("a question is cut at the engine's byte limit, not at a character count", async () => {
  const { send, seen, input } = await open({ run: () => ({ status: "ok", mode: "ask", reply: "답", operations: [] }) });
  await send("가".repeat(7000), 14);
  assert.ok(Buffer.byteLength(input.value) === 0 || Buffer.byteLength(input.value) <= 16384);
  const run = seen.find((r) => r.action === "copilot_run");
  assert.ok(Buffer.byteLength(run.prompt) <= 16384, `the prompt is ${Buffer.byteLength(run.prompt)} bytes`);
  assert.ok(run.prompt.length > 5000, "most of the text is kept: " + run.prompt.length);
  assert.equal(run.prompt.length, 5461);
});
