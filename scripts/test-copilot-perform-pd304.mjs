import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadPerformParts, loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-304: which steps a slide change or a range replacement becomes, and where a stopped performance cuts the operations.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

function performer({ elements = [], registered = [] } = {}) {
  const env = { __EASYOFFICE_SHEET_UTILS__: { registerDocImage: (...args) => registered.push(args) } };
  for (const part of ["slide-svg.js", "slide-snap.js", "slide-fx.js", "slide-ext.js", "slide-group.js", "slide-stage-size.js", "slide-canvas.js"]) loadUiModule(UI(part), env);
  loadPerformParts(env);
  loadUiModule(UI("copilot-perform.js"), env);
  const deck = [{ aspect: "16:9", title: "첫 슬라이드", lower: "", elements }];
  return env.__EASYOFFICE_CREATE_COPILOT_PERFORMER__({ document: {}, slideDeckState: deck, getActiveSlideIndex: () => 0, getActiveSheetName: () => "Sheet1", blockText: (x) => String(x?.text ?? x) });
}

const box = (id, x, text) => ({ id, kind: "textbox", x, y: 100, w: 200, h: 60, text });
const kinds = (targets) => targets.map((t) => (t.kind === "slide-object" ? `${t.kind}:${t.change}:${t.element.id}` : t.kind));
const cutAt = (perf, ops, index, typed) => {
  const targets = perf.collectTargets("SLIDE", ops);
  return perf.cutOperations(ops, targets, { target: targets[index], typed });
};

test("a new slide is opened first, then its text, notes and objects follow in the order they are applied", () => {
  const ops = [{ op: "slide.add", title: "제목", bullets: ["가", "나"], notes: "노트", layout: "twoColumn", elements: [box("a", 10, "글"), { kind: "chart", x: 300, y: 100 }] }];
  assert.deepEqual(kinds(performer().collectTargets("SLIDE", ops)), ["slide-add", "slide-title", "slide-body", "slide-notes", "slide-object:add:a", "slide-object:add:el-2"]);
});

test("an update turns the layout, the transition and the changed objects into steps and leaves an unchanged object out", () => {
  const perf = performer({ elements: [box("keep", 10, "그대로"), box("move", 10, "옮김"), box("gone", 10, "지움")] });
  const ops = [{ op: "slide.update", index: 0, layout: "blank", transition: "zoom", elements: [box("keep", 10, "그대로"), box("move", 300, "옮김"), box("new", 50, "새 글")] }];
  assert.deepEqual(kinds(perf.collectTargets("SLIDE", ops)), ["slide-format", "slide-format", "slide-object:change:move", "slide-object:add:new", "slide-object:remove:gone"]);
});

test("a slide that does not exist and was not added by the same reply is left to the final write", () => {
  const ops = [{ op: "slide.update", index: 5, title: "없는 슬라이드" }, { op: "slide.add", title: "추가" }, { op: "slide.update", index: 1, title: "방금 추가한 슬라이드" }];
  const targets = performer().collectTargets("SLIDE", ops);
  assert.deepEqual(kinds(targets), ["slide-add", "slide-title", "slide-title"]);
  assert.deepEqual(targets.map((t) => t.fresh), [true, true, true]);
});

test("an image's bytes are not registered for the performance and stay in the written operation", () => {
  const registered = [];
  const perf = performer({ registered });
  const ops = [{ op: "slide.add_element", index: 0, element: { kind: "image", x: 10, y: 10, w: 100, h: 80, data: "AAAA", alt: "그림" } }];
  const targets = perf.collectTargets("SLIDE", ops);
  assert.deepEqual(kinds(targets), ["slide-object:add:el-1"]);
  assert.equal(registered.length, 0);
  assert.equal(perf.cutOperations(ops, targets, null)[0].element.data, "AAAA");
});

test("stopping before a new slide is open drops it, and after that keeps it with what was written", () => {
  const ops = [{ op: "slide.add", title: "긴 제목입니다", bullets: ["본문"], layout: "blank", notes: "노트" }];
  assert.deepEqual(cutAt(performer(), ops, 0, 0), []);
  assert.deepEqual(cutAt(performer(), ops, 1, 4), [{ op: "slide.add", title: "긴 제목", layout: "blank" }]);
  assert.deepEqual(cutAt(performer(), ops, 1, 0), [{ op: "slide.add", layout: "blank" }], "an opened slide stays even when nothing was typed");
  assert.deepEqual(cutAt(performer(), ops, 2, 1), [{ op: "slide.add", title: "긴 제목입니다", bullets: ["본"], layout: "blank" }]);
});

test("stopping inside an object list keeps the finished steps, the typed part, and the old objects the rest would have changed", () => {
  const old = [box("keep", 10, "그대로"), box("move", 10, "옮김"), box("gone", 10, "지움")];
  const next = [box("keep", 10, "그대로"), box("move", 300, "옮김"), box("new", 50, "새로 쓰는 글")];
  const ops = [{ op: "slide.update", index: 0, elements: next }];
  const ids = (cut) => cut[0].elements.map((el) => [el.id, el.x, el.text]);
  const perf = () => performer({ elements: old });
  // steps: change move, add new, remove gone
  assert.equal(cutAt(perf(), ops, 0, 0).length, 0, "nothing was finished, so nothing is written");
  assert.deepEqual(ids(cutAt(perf(), ops, 1, 0)), [["keep", 10, "그대로"], ["move", 300, "옮김"], ["gone", 10, "지움"]], "the finished move stays, the unfinished add is left out");
  assert.deepEqual(ids(cutAt(perf(), ops, 1, 2)), [["keep", 10, "그대로"], ["move", 300, "옮김"], ["gone", 10, "지움"], ["new", 50, "새로"]]);
  assert.deepEqual(ids(cutAt(perf(), ops, 2, 0)), [["keep", 10, "그대로"], ["move", 300, "옮김"], ["gone", 10, "지움"], ["new", 50, "새로 쓰는 글"]], "a stop at the removal leaves the object it was going to remove");
});

test("a stopped single object keeps the typed part of its text and nothing when no letter was typed", () => {
  const ops = [{ op: "slide.add_element", index: 0, element: box("a", 10, "새 글상자") }];
  assert.equal(cutAt(performer(), ops, 0, 0).length, 0);
  assert.equal(cutAt(performer(), ops, 0, 2)[0].element.text, "새 ");
});

test("a range replacement types its blocks one after another and keeps the range when it is cut", () => {
  const perf = performer();
  const ops = [{ op: "doc.replace_paragraphs", startIndex: 1, endIndex: 2, blocks: [{ text: "첫 블록" }, { text: "둘째 블록" }] }];
  const targets = perf.collectTargets("DOC", ops);
  assert.deepEqual(targets.map((t) => [t.kind, t.entry, t.text, t.startIndex, t.endIndex]), [["doc-range", 0, "첫 블록", 1, 2], ["doc-range", 1, "둘째 블록", 1, 2]]);
  assert.deepEqual(perf.cutOperations(ops, targets, { target: targets[1], typed: 2 }), [{ op: "doc.replace_paragraphs", startIndex: 1, endIndex: 2, blocks: [{ text: "첫 블록" }, { text: "둘째" }] }]);
  assert.deepEqual(perf.cutOperations(ops, targets, { target: targets[0], typed: 0 }), [], "stopped while the range was being selected: nothing is replaced");
});

test("a replacement by paragraph number is still typed one paragraph at a time", () => {
  const targets = performer().collectTargets("DOC", [{ op: "doc.replace_paragraphs", replacements: [{ index: 0, text: "가" }, { index: 2, text: "나" }] }]);
  assert.deepEqual(targets.map((t) => t.kind), ["doc-replace", "doc-replace"]);
});
