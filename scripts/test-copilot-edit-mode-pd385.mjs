import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { reviewButton } from "./test-support/copilot-accept.mjs";
import { open, answer } from "./test-support/copilot-open.mjs";
import { applyCopilotEdits, parseApplyCopilotEditsParams } from "../apps/hwp-studio/src/embed/copilot-bridge.ts";

// PD-385: the copilot's edit way. An answer that edits what is there is previewed and ends in 바꾸기 / 아래에 삽입 / 버리기. An
// answer that only adds new content goes into the document at once as a draft and ends in 유지 / 버리기.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("copilot-edit-mode.js"), env);
const editMode = env.__EASYOFFICE_COPILOT_EDIT_MODE__;

const replaceOne = (index, text) => ({ op: "doc.replace_paragraphs", replacements: [{ index, text }] });
const buttons = (document) => Array.from(document.querySelectorAll("#copilot-thread .eo-copilot-review-btn")).map((b) => b.textContent.trim());
const click = async (h, action, turns = 12) => {
  h.userClick(reviewButton(h.document, action));
  await flushAsyncTurns(turns);
};
const paragraphTexts = (editor) => Array.from(editor.querySelectorAll("p")).filter((p) => !p.classList.contains("manuscript-notice")).map((p) => p.textContent);
const seed = (editor, ...texts) => {
  editor.innerHTML = texts.map((t) => `<p>${t}</p>`).join("");
};

// ---- 판정

test("classifyEditMode: only answers made of new content go in as a draft", () => {
  const empty = { isCellFilled: () => false };
  const filled = { isCellFilled: () => true };
  const cells = { op: "sheet.write", cells: [{ coord: "D2", raw: "1" }, { coord: "D3", raw: "2" }] };
  const cases = [
    ["DOC", [{ op: "doc.append", text: "새 문단" }], "draft"],
    ["HWP", [{ op: "doc.append", text: "새 문단" }], "draft"],
    ["DOC", [replaceOne(0, "x")], "review"],
    ["DOC", [{ op: "doc.replace_span", index: 0, search: "a", replace: "b" }], "review"],
    ["DOC", [{ op: "doc.scrub_pii" }], "review"],
    ["HWP", [{ op: "doc.replace_paragraphs", startIndex: 0, endIndex: 1, blocks: ["x"] }], "review"],
    ["SLIDE", [{ op: "slide.add", title: "t" }, { op: "slide.add_element", element: { kind: "textbox" } }], "draft"],
    ["SLIDE", [{ op: "slide.update", title: "t" }], "review"],
    ["BASE", [{ op: "base.add_records", records: [] }, { op: "base.add_field", name: "f" }], "draft"],
    ["BASE", [{ op: "base.update_records", records: [] }], "review"],
    ["BASE", [{ op: "base.set_view" }], "review"],
    ["BOARD", [{ op: "board.add_graph" }], "draft"],
    ["SHEET", [{ op: "sheet.format", range: "A1" }], "review"],
    ["DOC", [], "review"],
    ["PDF", [{ op: "doc.append" }], "review"],
    // One editing operation makes the whole answer a preview.
    ["DOC", [{ op: "doc.append", text: "새 문단" }, replaceOne(0, "x")], "review"],
    ["SLIDE", [{ op: "slide.add", title: "t" }, { op: "slide.update", title: "u" }], "review"],
  ];
  for (const [mod, operations, expected] of cases) {
    assert.equal(editMode.classifyEditMode(mod, operations, empty), expected, `${mod} ${JSON.stringify(operations)}`);
  }
  assert.equal(editMode.classifyEditMode("SHEET", [cells], empty), "draft", "cells that are all empty are new content");
  assert.equal(editMode.classifyEditMode("SHEET", [cells], filled), "review", "a filled cell is overwritten");
  assert.equal(editMode.classifyEditMode("SHEET", [cells]), "review", "without a look at the sheet a write counts as an edit");
  assert.equal(editMode.classifyEditMode("SHEET", [{ op: "sheet.write", cells: [] }], empty), "review");
  assert.equal(editMode.classifyEditMode("SHEET", [cells, { op: "sheet.format", range: "D2" }], empty), "review");
  assert.equal(editMode.classifyEditMode("DOC", [null], empty), "review");
});

test("아래에 삽입 is offered for text operations of DOC and HWP only", () => {
  const span = { op: "doc.replace_span", index: 0, search: "a", replace: "b" };
  assert.equal(editMode.canInsertBelow("DOC", [span]), true);
  assert.equal(editMode.canInsertBelow("HWP", [replaceOne(0, "x")]), true);
  assert.equal(editMode.canInsertBelow("DOC", [span, { op: "doc.append", text: "끝" }]), true);
  assert.equal(editMode.canInsertBelow("DOC", [span, { op: "doc.scrub_pii" }]), false, "a non-text operation in the answer");
  assert.equal(editMode.canInsertBelow("DOC", [{ op: "doc.append", text: "끝" }]), false, "nothing is replaced");
  for (const mod of ["SHEET", "SLIDE", "BASE", "BOARD", "PDF"]) {
    assert.equal(editMode.canInsertBelow(mod, [span]), false, mod);
  }
  assert.deepEqual(editMode.reviewActions({ state: "pending", insertBelow: true }).map((a) => a[1]), ["바꾸기", "아래에 삽입", "버리기"]);
  assert.deepEqual(editMode.reviewActions({ state: "pending", insertBelow: false }).map((a) => a[1]), ["바꾸기", "버리기"]);
  assert.deepEqual(editMode.reviewActions({ state: "draft" }).map((a) => a[1]), ["유지", "버리기"]);
  assert.deepEqual(editMode.reviewActions({ state: "kept" }), []);
});

// ---- 아래에 삽입으로의 연산 변환

const reader = (...texts) => ({ count: texts.length, text: (index) => texts[index] ?? null });

test("toInsertBelowOperations turns replacements into inserts under the old paragraphs and leaves other operations", () => {
  const ops = editMode.toInsertBelowOperations([
    { op: "doc.replace_span", index: 1, search: "둘째", replace: "다시 쓴 둘째" },
    replaceOne(0, "새 첫째"),
    replaceOne(9, "문서 밖 문단"),
    { op: "doc.replace_paragraphs", startIndex: 1, endIndex: 2, blocks: ["범위 새 글 1", "범위 새 글 2"] },
    { op: "doc.append", text: "끝" },
  ], reader("첫째", "둘째 문단", "셋째"));
  assert.deepEqual(ops, [
    {
      op: "doc.insert_below",
      inserts: [
        { after: 1, plain: true, blocks: ["다시 쓴 둘째 문단"] },
        { after: 0, blocks: ["새 첫째"] },
        { after: 2, blocks: ["범위 새 글 1", "범위 새 글 2"] },
      ],
    },
    { op: "doc.append", blocks: ["문서 밖 문단"] },
    { op: "doc.append", text: "끝" },
  ]);
  assert.throws(() => editMode.toInsertBelowOperations([{ op: "doc.replace_span", index: 0, search: "없음", replace: "x" }], reader("첫째")), /유일하게 찾을 수 없습니다/);
  assert.throws(() => editMode.toInsertBelowOperations([{ op: "doc.replace_span", index: 5, search: "a", replace: "x" }], reader("첫째")), /범위를 벗어났습니다/);
  assert.throws(() => editMode.toInsertBelowOperations([{ op: "doc.replace_span", index: 0, search: "a", replace: "x" }], { count: 1, text: () => null }), /읽지 못했습니다/);
});

test("a range with no end goes under the last paragraph; an empty document gets an append", () => {
  assert.deepEqual(editMode.toInsertBelowOperations([{ op: "doc.replace_paragraphs", blocks: ["전체"] }], reader("가", "나")), [
    { op: "doc.insert_below", inserts: [{ after: 1, blocks: ["전체"] }] },
  ]);
  assert.deepEqual(editMode.toInsertBelowOperations([{ op: "doc.replace_paragraphs", blocks: ["전체"] }], reader()), [{ op: "doc.append", blocks: ["전체"] }]);
});

// ---- DOC: 바꾸기, 아래에 삽입, 버리기

test("a DOC edit shows 바꾸기, 아래에 삽입 and 버리기 over a preview that leaves the document alone", async () => {
  const h = await open({ run: () => answer([replaceOne(1, "바뀐 둘째")]) });
  seed(h.editor, "첫째", "둘째", "셋째");
  await h.send("고쳐 주세요");
  assert.deepEqual(buttons(h.document), ["바꾸기", "아래에 삽입", "버리기"]);
  assert.ok(h.document.querySelector(".eo-copilot-diff-block"), "the preview is drawn");
  assert.deepEqual(paragraphTexts(h.editor), ["첫째", "둘째", "셋째"]);
});

test("아래에 삽입 keeps the original paragraph and puts the new text in a new paragraph right under it, and one undo takes it back", async () => {
  const h = await open({ run: () => answer([replaceOne(1, "바뀐 둘째")]) });
  seed(h.editor, "첫째", "둘째", "셋째");
  await h.send("고쳐 주세요");
  await click(h, "insert-below");
  assert.deepEqual(paragraphTexts(h.editor), ["첫째", "둘째", "바뀐 둘째", "셋째"]);
  assert.equal(h.document.querySelector(".eo-copilot-diff-block"), null, "the overlay goes away");
  assert.deepEqual(buttons(h.document), [], "decided");
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.deepEqual(paragraphTexts(h.editor), ["첫째", "둘째", "셋째"]);
});

test("아래에 삽입 of a span puts the whole paragraph with the replacement under the original, as plain text", async () => {
  const run = () => answer([{ op: "doc.replace_span", index: 0, search: "then x", replace: "then y" }]);
  const h = await open({ run });
  seed(h.editor, "if a&lt;b and c&gt;d then x", "끝");
  await h.send("고쳐 주세요");
  await click(h, "insert-below");
  assert.deepEqual(paragraphTexts(h.editor), ["if a<b and c>d then x", "if a<b and c>d then y", "끝"]);
});

test("아래에 삽입 of a range goes under the last paragraph of the range, and replacements in one answer stay on their old numbers", async () => {
  const range = await open({ run: () => answer([{ op: "doc.replace_paragraphs", startIndex: 1, endIndex: 2, blocks: [{ text: "새 글 A" }, { text: "새 글 B" }] }]) });
  seed(range.editor, "첫째", "둘째", "셋째", "넷째");
  await range.send("고쳐 주세요");
  await click(range, "insert-below");
  assert.deepEqual(paragraphTexts(range.editor), ["첫째", "둘째", "셋째", "새 글 A", "새 글 B", "넷째"]);

  const many = await open({ run: () => answer([replaceOne(0, "가 아래"), replaceOne(2, "다 아래")]) });
  seed(many.editor, "가", "나", "다");
  await many.send("고쳐 주세요");
  await click(many, "insert-below");
  assert.deepEqual(paragraphTexts(many.editor), ["가", "가 아래", "나", "다", "다 아래"]);
});

test("버리기 on a preview leaves the document as it was and says 버림", async () => {
  const h = await open({ run: () => answer([replaceOne(0, "바뀐 문장")]) });
  seed(h.editor, "원래 문장");
  await h.send("고쳐 주세요");
  await click(h, "reject", 6);
  assert.deepEqual(paragraphTexts(h.editor), ["원래 문장"]);
  assert.match(h.assistants()[0].textContent, /버림/);
  assert.doesNotMatch(h.assistants()[0].textContent, /거절/);
});

test("바꾸기 and 아래에 삽입 both refuse a document that changed since the preview", async () => {
  for (const action of ["accept", "insert-below"]) {
    const h = await open({ run: () => answer([replaceOne(0, "바뀐 문장")]) });
    seed(h.editor, "원래 문장");
    await h.send("고쳐 주세요");
    h.editor.querySelector("p").textContent = "직접 고친 문장";
    await click(h, action);
    assert.deepEqual(paragraphTexts(h.editor), ["직접 고친 문장"], action);
    assert.equal(h.document.getElementById("status-command-feedback").textContent, "문서가 바뀌어 적용할 수 없습니다", action);
    assert.equal(h.assistants()[0].querySelector(".eo-copilot-undo-btn"), null, action);
  }
});

test("an answer that mixes an edit with new content is previewed as a whole", async () => {
  const h = await open({ run: () => answer([replaceOne(0, "바뀐 문장"), { op: "doc.append", text: "덧붙인 문단" }]) });
  seed(h.editor, "원래 문장");
  await h.send("고쳐 주세요");
  assert.deepEqual(paragraphTexts(h.editor), ["원래 문장"], "nothing is written yet");
  assert.deepEqual(buttons(h.document), ["바꾸기", "아래에 삽입", "버리기"]);
  await click(h, "insert-below");
  assert.deepEqual(paragraphTexts(h.editor), ["원래 문장", "바뀐 문장", "덧붙인 문단"]);
});

test("a DOC operation that is not text hides 아래에 삽입", async () => {
  const h = await open({ run: () => answer([replaceOne(0, "바뀐 문장"), { op: "doc.scrub_pii" }]) });
  seed(h.editor, "원래 문장");
  await h.send("고쳐 주세요");
  assert.deepEqual(buttons(h.document), ["바꾸기", "버리기"]);
});

// ---- 초안: 바로 넣고, 유지 또는 버리기

test("new content goes into the document at once as a draft, with 유지 and 버리기 and no preview", async () => {
  const h = await open({ run: () => answer([{ op: "doc.append", text: "새로 쓴 문단" }]) });
  seed(h.editor, "원래 문장");
  await h.send("문단을 추가해 주세요");
  assert.deepEqual(paragraphTexts(h.editor), ["원래 문장", "새로 쓴 문단"]);
  assert.equal(h.document.querySelector(".eo-copilot-diff-block"), null, "no preview");
  assert.equal(h.document.querySelector(".eo-copilot-changes"), null, "no change list");
  assert.deepEqual(buttons(h.document), ["유지", "버리기"]);
  assert.equal(h.assistants()[0].querySelector(".eo-copilot-undo-btn"), null, "the undo chip comes after the decision");
});

test("버리기 on a draft puts the document back, 유지 keeps it and leaves the 실행 취소 chip", async () => {
  const dropped = await open({ run: () => answer([{ op: "doc.append", text: "새로 쓴 문단" }]) });
  seed(dropped.editor, "원래 문장");
  const before = dropped.editor.innerHTML;
  await dropped.send("문단을 추가해 주세요");
  assert.notEqual(dropped.editor.innerHTML, before);
  await click(dropped, "discard");
  assert.equal(dropped.editor.innerHTML, before, "back as it was");
  assert.deepEqual(buttons(dropped.document), []);
  assert.match(dropped.assistants()[0].textContent, /버림/);
  assert.equal(dropped.assistants()[0].querySelector(".eo-copilot-undo-btn"), null);

  const kept = await open({ run: () => answer([{ op: "doc.append", text: "새로 쓴 문단" }]) });
  seed(kept.editor, "원래 문장");
  await kept.send("문단을 추가해 주세요");
  await click(kept, "keep");
  assert.deepEqual(paragraphTexts(kept.editor), ["원래 문장", "새로 쓴 문단"]);
  assert.deepEqual(buttons(kept.document), []);
  const undo = kept.assistants()[0].querySelector(".eo-copilot-undo-btn");
  assert.ok(undo, "the undo chip is there as before");
  kept.userClick(undo);
  await flushAsyncTurns(12);
  assert.deepEqual(paragraphTexts(kept.editor), ["원래 문장"]);
});

test("a draft that the user edited over cannot be discarded, and the next question keeps it", async () => {
  const h = await open({ run: (req) => (req.prompt === "추가" ? answer([{ op: "doc.append", text: "초안 문단" }]) : { status: "ok", mode: "edit", reply: "답", operations: [] }) });
  seed(h.editor, "원래 문장");
  await h.send("추가");
  const p = h.editor.querySelector("p");
  h.editor.dispatchEvent({ type: "focus", target: h.editor });
  p.textContent = p.textContent + " 직접 고침";
  h.editor.dispatchEvent({ type: "input", target: p });
  h.document.dispatchEvent({ type: "keyup" });
  await flushAsyncTurns(4);
  assert.equal(reviewButton(h.document, "discard").disabled, true, "the draft cannot be taken back over the user's edit");
  await h.send("다른 질문");
  assert.deepEqual(buttons(h.document), [], "a new question settles the draft");
  assert.deepEqual(paragraphTexts(h.editor), ["원래 문장 직접 고침", "초안 문단"]);
});

test("a new slide and a new object are drafts: no 바꾸기, 버리기 removes them", async () => {
  const h = await open({ module: "SLIDE", run: () => answer([{ op: "slide.add", title: "새 슬라이드" }]) });
  const thumbs = () => h.document.querySelectorAll("#slide-thumb-list .slide-thumb").length;
  assert.equal(thumbs(), 1);
  await h.send("슬라이드를 추가해 주세요");
  assert.equal(thumbs(), 2, "the slide is in the deck as a draft");
  assert.deepEqual(buttons(h.document), ["유지", "버리기"]);
  await click(h, "discard");
  assert.equal(thumbs(), 1);
});

test("a slide edit is previewed with 바꾸기 and 버리기 only", async () => {
  const h = await open({ module: "SLIDE", run: () => answer([{ op: "slide.update", title: "바뀐 제목" }]) });
  await h.send("제목을 고쳐 주세요");
  assert.deepEqual(buttons(h.document), ["바꾸기", "버리기"]);
});

test("SHEET: writes into empty cells are a draft, writes over a filled cell are previewed without 아래에 삽입", async () => {
  const cell = (h, coord) => h.document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
  const fill = (h, coord, raw) => {
    const td = cell(h, coord);
    td.focus();
    td.textContent = raw;
    td.dispatchEvent({ type: "input", target: td });
    td.blur();
  };
  const emptyTarget = await open({ module: "SHEET", run: () => answer([{ op: "sheet.write", cells: [{ coord: "D2", raw: "새 값" }] }]) });
  fill(emptyTarget, "A1", "있던 값");
  await flushAsyncTurns(4);
  await emptyTarget.send("써 주세요");
  assert.equal(cell(emptyTarget, "D2").textContent, "새 값", "written at once");
  assert.deepEqual(buttons(emptyTarget.document), ["유지", "버리기"]);
  await click(emptyTarget, "discard");
  assert.equal(cell(emptyTarget, "D2").textContent, "");

  const overwrite = await open({ module: "SHEET", run: () => answer([{ op: "sheet.write", cells: [{ coord: "A1", raw: "덮은 값" }] }]) });
  fill(overwrite, "A1", "있던 값");
  await flushAsyncTurns(4);
  await overwrite.send("써 주세요");
  assert.equal(cell(overwrite, "A1").textContent, "있던 값", "nothing is written before 바꾸기");
  assert.deepEqual(buttons(overwrite.document), ["바꾸기", "버리기"]);
});

// ---- HWP

async function openHwpOps(paragraphs) {
  const h = await open({ module: "DOC", run: () => answer([]) });
  const calls = [];
  const hwpHost = {
    usesLegacy: () => false,
    getStatus: () => "ready",
    onChange: () => {},
    onStatus: () => {},
    call: async (method, params) => {
      calls.push({ method, params });
      return {
        documentGeneration: 1,
        pageCount: 1,
        totalParagraphs: paragraphs.length,
        paragraphs: paragraphs.map((text, index) => ({ index, section: 0, paragraph: index, text, hasControls: false })),
        selection: { hasSelection: false },
      };
    },
  };
  const ops = h.host.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__({ document: h.document, getHwpHost: () => hwpHost });
  await ops.refreshHwpSnapshot();
  return { ops, calls };
}

test("HWP: 아래에 삽입 sends one insert_after step per paragraph, the highest number first, and replaces nothing", async () => {
  const { ops, calls } = await openHwpOps(["첫째", "둘째", "셋째"]);
  const converted = editMode.toInsertBelowOperations([
    replaceOne(0, "첫째 아래\n한 줄 더"),
    { op: "doc.replace_span", index: 2, search: "셋째", replace: "셋째 고침" },
    { op: "doc.replace_paragraphs", startIndex: 0, endIndex: 0, blocks: ["같은 문단 아래 또"] },
  ], { count: 3, text: (index) => ["첫째", "둘째", "셋째"][index] });
  const res = await ops.applyHwpOperations(converted);
  assert.equal(res.applied, true);
  const edit = calls.find((c) => c.method === "applyCopilotEdits");
  assert.deepEqual(edit.params.steps, [
    { kind: "insert_after", index: 2, texts: ["셋째 고침"] },
    { kind: "insert_after", index: 0, texts: ["첫째 아래\n한 줄 더", "같은 문단 아래 또"] },
  ]);
  assert.equal(edit.params.expectedDocumentGeneration, 1, "the generation of the preview is checked");
});

function paragraphWasm(initial) {
  const sections = [initial.slice()];
  let generation = 1;
  const wasm = {
    get pageCount() { return 1; },
    get documentGeneration() { return generation; },
    getSectionCount: () => sections.length,
    getParagraphCount: (s) => sections[s].length,
    getParagraphLength: (s, p) => sections[s][p].length,
    getTextRange: (s, p, off, cnt) => sections[s][p].slice(off, off + cnt),
    getControlTextPositions: () => [],
    replaceText: (s, p, off, len, next) => {
      const cur = sections[s][p];
      sections[s][p] = cur.slice(0, off) + next + cur.slice(off + len);
      return { ok: true };
    },
    insertText: (s, p, off, txt) => {
      const cur = sections[s][p];
      sections[s][p] = cur.slice(0, off) + txt + cur.slice(off);
    },
    deleteText: () => {},
    splitParagraph: (s, p, off) => {
      const cur = sections[s][p];
      sections[s].splice(p, 1, cur.slice(0, off), cur.slice(off));
    },
    mergeParagraph: () => {},
    getFieldInfoAt: () => ({ inField: false }),
  };
  const undoStack = [];
  const input = {
    getCursorPosition: () => ({ sectionIndex: 0, paragraphIndex: 0, charOffset: 0 }),
    getSelection: () => ({ start: { sectionIndex: 0, paragraphIndex: 0, charOffset: 0 }, end: { sectionIndex: 0, paragraphIndex: 0, charOffset: 0 } }),
    executeOperation: (desc) => {
      const before = structuredClone(sections);
      try {
        desc.operation(wasm);
        generation += 1;
        undoStack.push(before);
      } catch (err) {
        sections[0] = before[0];
        throw err;
      }
    },
  };
  return { sections, wasm, input, undoStack };
}

test("the editor's insert_after step leaves the paragraph as it is and adds a paragraph per line right under it, in one undo step", () => {
  const { sections, wasm, input, undoStack } = paragraphWasm(["첫째", "둘째", "셋째"]);
  const params = parseApplyCopilotEditsParams({
    expectedDocumentGeneration: 1,
    steps: [
      { kind: "insert_after", index: 2, texts: ["셋째 아래"] },
      { kind: "insert_after", index: 0, texts: ["첫째 아래 1\n첫째 아래 2", "첫째 아래 3"] },
    ],
  });
  const res = applyCopilotEdits(wasm, input, params);
  assert.equal(res.ok, true);
  assert.deepEqual(sections[0], ["첫째", "첫째 아래 1", "첫째 아래 2", "첫째 아래 3", "둘째", "셋째", "셋째 아래"]);
  assert.equal(undoStack.length, 1);
  assert.equal(res.totalParagraphsAfter, 7);
});

test("insert_after refuses a paragraph past the end, extra fields and a stale document without changing anything", () => {
  const { sections, wasm, input, undoStack } = paragraphWasm(["첫째"]);
  assert.throws(() => applyCopilotEdits(wasm, input, parseApplyCopilotEditsParams({ steps: [{ kind: "insert_after", index: 5, texts: ["x"] }] })), /범위를 벗어났습니다/);
  assert.throws(() => parseApplyCopilotEditsParams({ steps: [{ kind: "insert_after", index: 0, texts: ["x"], extra: 1 }] }), /unknown field/);
  assert.throws(() => parseApplyCopilotEditsParams({ steps: [{ kind: "insert_after", index: 0, texts: [] }] }), /non-empty array/);
  assert.throws(() => applyCopilotEdits(wasm, input, parseApplyCopilotEditsParams({ expectedDocumentGeneration: 9, steps: [{ kind: "insert_after", index: 0, texts: ["x"] }] })), /변경되어 적용을 중단/);
  assert.deepEqual(sections[0], ["첫째"]);
  assert.equal(undoStack.length, 0);
});
