import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-268: the word-level diff and the accept/reject review of a proposed change.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("copilot-diff.js"), env);
const { textTokens, textEdits, boundedEdits, diffSegments, createChangeReview, MAX_DIFF_CHARS } = env.__EASYOFFICE_COPILOT_DIFF__;

const apply = (before, edits) => {
  let out = "";
  let cursor = 0;
  for (const edit of edits) {
    out += before.slice(cursor, edit.from) + edit.text;
    cursor = edit.to;
  }
  return out + before.slice(cursor);
};

test("tokens are words, runs of space and single marks", () => {
  assert.deepEqual(textTokens("이번 분기, 매출은 12.5%"), ["이번", " ", "분기", ",", " ", "매출은", " ", "12", ".", "5", "%"]);
  assert.deepEqual(textTokens(""), []);
});

test("a rewrite replaces whole words, not letters", () => {
  const before = "이번 분기 매출은 전 분기보다 늘었다";
  const after = "이번 분기 매출은 전 분기보다 크게 늘었다";
  const edits = textEdits(before, after);
  assert.deepEqual(edits, [{ from: before.indexOf("늘었다"), text: "크게 ", to: before.indexOf("늘었다") }]);
  assert.equal(apply(before, edits), after);
  const swapped = textEdits("서버 라이선스 갱신", "서버 인증서 갱신");
  assert.equal(swapped.length, 1);
  assert.equal(swapped[0].text, "인증서");
});

test("two edits that only whitespace separates read as one", () => {
  const before = "가 나 다";
  const after = "라 마 다";
  const edits = textEdits(before, after);
  assert.equal(edits.length, 1);
  assert.equal(apply(before, edits), after);
});

test("applying the edits always gives the new text", () => {
  const pairs = [
    ["", "새 글"], ["지울 글", ""], ["같은 글", "같은 글"], ["a b c d e", "e d c b a"],
    ["첫째 문장입니다. 둘째 문장입니다.", "둘째 문장입니다. 첫째 문장입니다. 셋째."],
    ["tab\tand\nnewline", "tab and newline"],
  ];
  for (const [before, after] of pairs) assert.equal(apply(before, textEdits(before, after)), after, `${before} -> ${after}`);
  assert.deepEqual(textEdits("같은", "같은"), []);
});

test("segments join back into both texts", () => {
  const before = "합계 행을 추가하세요";
  const after = "합계와 평균 행을 추가하세요";
  const segments = diffSegments(before, after);
  assert.equal(segments.filter((s) => s.kind !== "insert").map((s) => s.text).join(""), before);
  assert.equal(segments.filter((s) => s.kind !== "delete").map((s) => s.text).join(""), after);
  assert.ok(segments.some((s) => s.kind === "insert"));
  assert.deepEqual(diffSegments("같다", "같다"), [{ kind: "equal", text: "같다" }]);
});

test("a text over the limit is one edit between the common ends, never a token walk", () => {
  const head = "앞".repeat(MAX_DIFF_CHARS);
  const tail = "뒤".repeat(10);
  const before = head + "가나다" + tail;
  const after = head + "라마바사" + tail;
  const started = performance.now();
  const edits = boundedEdits(before, after);
  assert.ok(performance.now() - started < 200, "no quadratic walk over the text");
  assert.deepEqual(edits, [{ from: head.length, text: "라마바사", to: head.length + 3 }]);
  assert.deepEqual(boundedEdits(before, before), []);
});

test("an edit never cuts a surrogate pair", () => {
  const before = "x".repeat(MAX_DIFF_CHARS + 1) + "😀";
  const after = "x".repeat(MAX_DIFF_CHARS + 1) + "😁";
  const [edit] = boundedEdits(before, after);
  assert.equal(apply(before, [edit]), after);
  assert.doesNotMatch(edit.text, /^[\udc00-\udfff]|[\ud800-\udbff]$/);
});

test("a hostile pair of long, unrelated texts finishes quickly", () => {
  const a = Array.from({ length: 6000 }, (_, i) => "가" + i).join(" ");
  const b = Array.from({ length: 6000 }, (_, i) => "나" + i).join(" ");
  const started = performance.now();
  const edits = boundedEdits(a, b);
  assert.ok(performance.now() - started < 1500);
  assert.equal(apply(a, edits), b);
});

test("review: accept applies and clears, reject withdraws and clears, a failure keeps the change", async () => {
  const calls = [];
  const review = createChangeReview({
    accept: async (change) => (calls.push(["accept", change.id]), { status: change.fail ? "failed" : "applied" }),
    withdraw: async (change) => (calls.push(["withdraw", change.id]), { status: "unchanged" }),
  });
  assert.equal(await review.accept(), null, "nothing is pending");
  review.set({ id: 1 });
  assert.deepEqual(await review.accept(), { status: "applied" });
  assert.equal(review.pending, null);
  review.set({ id: 2, fail: true });
  assert.deepEqual(await review.accept(), { status: "failed" });
  assert.equal(review.pending.id, 2, "a failed accept leaves the change to decide again");
  assert.deepEqual(await review.reject(), { status: "unchanged" });
  assert.equal(review.pending, null);
  assert.deepEqual(calls, [["accept", 1], ["accept", 2], ["withdraw", 2]]);
});
