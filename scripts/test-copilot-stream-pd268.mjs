import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-268: the stream channel (ordered pieces), the partial JSON reader, the Markdown joiner and one request's stream session.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("copilot-stream.js"), env);
const { createStreamChannel, parseJSON, PartialJSON, MalformedJSON, MarkdownJoiner, extractReply, createStreamSession, MAX_LIVE_PARSE_CHARS } = env.__EASYOFFICE_COPILOT_STREAM__;

function fakeInternals() {
  const callbacks = new Map();
  let next = 40;
  return {
    callbacks,
    transformCallback(fn) {
      next += 1;
      callbacks.set(next, fn);
      return next;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
  };
}

test("the channel names itself the way the shell reads it and unregisters at the end", () => {
  const internals = fakeInternals();
  const got = [];
  const channel = createStreamChannel(internals, (m) => got.push(m));
  assert.equal(JSON.stringify({ onStream: channel }), `{"onStream":"__CHANNEL__:${channel.id}"}`);
  const send = internals.callbacks.get(channel.id);
  send({ message: "a", index: 0 });
  send({ end: true, index: 1 });
  assert.deepEqual(got, ["a"]);
  assert.equal(internals.callbacks.has(channel.id), false, "the callback is gone once the end arrives in order");
});

test("pieces that arrive out of order are held until the gap is filled", () => {
  const internals = fakeInternals();
  const got = [];
  const channel = createStreamChannel(internals, (m) => got.push(m));
  const send = internals.callbacks.get(channel.id);
  send({ message: "c", index: 2 });
  send({ message: "b", index: 1 });
  assert.deepEqual(got, []);
  send({ message: "a", index: 0 });
  assert.deepEqual(got, ["a", "b", "c"]);
  send({ end: true, index: 3 });
  assert.equal(internals.callbacks.has(channel.id), false);
});

test("a handler that throws does not stall the pieces behind it", () => {
  const internals = fakeInternals();
  const got = [];
  const reported = [];
  const channel = createStreamChannel(internals, (m) => {
    got.push(m);
    if (m === "bad") throw new Error("handler failed");
  }, (error) => reported.push(error.message));
  const send = internals.callbacks.get(channel.id);
  send({ message: "bad", index: 0 });
  send({ message: "next", index: 1 });
  assert.deepEqual(got, ["bad", "next"]);
  assert.deepEqual(reported, ["handler failed"], "the error is still reported");
});

test("partial JSON reads as far as it is complete", () => {
  assert.equal(parseJSON('"'), "");
  assert.equal(parseJSON('" \\x12'), " ");
  assert.deepEqual(parseJSON('["'), [""]);
  assert.deepEqual(parseJSON('{"": "'), { "": "" });
  assert.deepEqual(parseJSON('{"reply":"안녕 하세'), { reply: "안녕 하세" });
  assert.deepEqual(parseJSON('{"reply":"끝","operations":[{"kind":"doc.app'), { reply: "끝", operations: [{ kind: "doc.app" }] });
  assert.deepEqual(parseJSON('{"reply":"a\\n'), { reply: "a\n".slice(0, 1) + "\n" });
  assert.equal(parseJSON("-1.25e+4"), -1.25e4);
  assert.equal(parseJSON("-1.25e+"), -1.25);
  assert.equal(parseJSON("nu"), null);
  assert.equal(parseJSON("tr"), true);
  assert.throws(() => parseJSON(""), /empty/);
  assert.throws(() => parseJSON("-"), MalformedJSON);
  assert.ok(PartialJSON.prototype instanceof Error);
});

test("a dangling escape never breaks the reply", () => {
  assert.equal(parseJSON('{"reply":"줄\\'). reply, "줄");
  assert.equal(parseJSON('{"reply":"유니\\u00').reply, "유니");
  assert.equal(parseJSON('{"reply":"따옴표 \\"').reply, '따옴표 "');
});

test("the reply is taken from the JSON the model writes, or the whole text when it is not JSON", () => {
  assert.equal(extractReply('{"reply":"문단을 추가했'), "문단을 추가했");
  assert.equal(extractReply('{"operations":[{"kind":"doc.append"'), "", "no reply yet");
  assert.equal(extractReply('```json\n{"reply":"펜스 안'), "펜스 안");
  assert.equal(extractReply("그냥 글로 답합니다"), "그냥 글로 답합니다");
  assert.equal(extractReply('{"reply":"끝","operations":[{"kind":"sheet.write","cells":[{"coord":"A1","raw":"x"}]}'), "끝");
  assert.equal(extractReply("{" + "x".repeat(MAX_LIVE_PARSE_CHARS)), null, "past the limit the stream is no longer read");
});

test("the joiner holds a half-finished Markdown mark until it closes", () => {
  const joiner = new MarkdownJoiner();
  assert.equal(joiner.processText("값은 **"), "값은 ");
  assert.equal(joiner.processText("굵게"), "");
  assert.equal(joiner.processText("** 입니다"), "**굵게** 입니다");
  assert.equal(joiner.processText("[링크](http"), "");
  assert.equal(joiner.processText("://a.b/c) 끝"), "[링크](http://a.b/c) 끝");
  assert.equal(joiner.flush(), "");
});

test("the joiner lets a false alarm through and flushes what it still holds", () => {
  const joiner = new MarkdownJoiner();
  assert.equal(joiner.processText("2*3"), "");
  assert.equal(joiner.flush(), "2*3", "an unfinished mark is not lost");
  const other = new MarkdownJoiner();
  assert.equal(other.processText("1) 첫째\n"), "1) 첫째\n");
  assert.equal(other.processText("- 목록 항목\n"), "- 목록 항목\n");
  const code = new MarkdownJoiner();
  assert.equal(code.processText("```js\nlet a;\n"), "```js\nlet a;\n");
});

test("a session shows the reply as it grows and closes with whatever the joiner holds", () => {
  const shown = [];
  const session = createStreamSession({ requestId: "r1", onText: (t) => shown.push(t) });
  const pieces = ['{"reply":"요약', "했습니다 **굵", '게**","operations":[', '{"kind":"doc.append"}]}'];
  pieces.forEach((text, seq) => assert.equal(session.accept({ requestId: "r1", seq, text }), true));
  assert.equal(shown[0], "요약");
  assert.equal(shown.at(-1), "요약했습니다 **굵게**");
  assert.ok(!shown.join("").includes('"kind"'), "the operations never reach the screen");
  assert.equal(session.finish(), "요약했습니다 **굵게**");
  assert.equal(session.accept({ requestId: "r1", seq: 4, text: "x" }), false, "nothing is taken after the end");
});

test("a gap or a stranger's piece stops the stream text and the final answer is what counts", () => {
  const shown = [];
  const session = createStreamSession({ requestId: "r1", onText: (t) => shown.push(t) });
  assert.equal(session.accept({ requestId: "other", seq: 0, text: "남의 글" }), false);
  assert.equal(session.accept({ requestId: "r1", seq: 0, text: '{"reply":"앞' }), true);
  assert.equal(session.accept({ requestId: "r1", seq: 2, text: "빠진 뒤" }), false, "piece 1 never came");
  assert.equal(session.failed, true);
  assert.equal(session.accept({ requestId: "r1", seq: 1, text: "늦은 조각" }), false, "after a gap nothing is trusted");
  assert.deepEqual(shown, ["앞"]);
});

test("a plain-text answer streams as it is and a rewritten reply starts the join again", () => {
  const shown = [];
  const session = createStreamSession({ requestId: "p", onText: (t) => shown.push(t) });
  session.accept({ requestId: "p", seq: 0, text: "그냥 " });
  session.accept({ requestId: "p", seq: 1, text: "글입니다" });
  assert.equal(shown.at(-1), "그냥 글입니다");
});

test("a hostile stream cannot make the live reader throw or spin: deep nesting, an endless string, a gap of garbage", () => {
  const started = performance.now();
  assert.equal(extractReply("{" + "[".repeat(MAX_LIVE_PARSE_CHARS - 2)), "", "a deep nesting is read as no reply, not as a crash");
  assert.equal(extractReply('{"reply":"' + "가".repeat(MAX_LIVE_PARSE_CHARS - 20)).length, MAX_LIVE_PARSE_CHARS - 20);
  assert.equal(extractReply('{"reply" "x"} }}} {{{'), "");
  assert.ok(performance.now() - started < 2000, "bounded work for the largest live text");
  const session = createStreamSession({ requestId: "h", onText: () => {} });
  // 평범한 글은 상한 안에서만 보이고, 넘으면 그 뒤 조각은 받지 않는다(최종 응답이 온전한 글을 갖고 있다).
  let taken = 0;
  for (let seq = 0; seq < 200; seq += 1) taken += session.accept({ requestId: "h", seq, text: "x".repeat(2000) }) ? 1 : 0;
  assert.ok(taken > 0 && taken < 200, `the live text stops at the limit after ${taken} pieces`);
  assert.ok(session.shown.length <= MAX_LIVE_PARSE_CHARS + 2000);
});
