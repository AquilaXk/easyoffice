import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createFakeClock, settle } from "./test-support/fake-clock.mjs";

// PD-268: the typing loop (letter by letter, a rest at the end of a sentence, stop keeps what was typed).

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("copilot-typing.js"), env);
const { createTypist, stopsOf, MAX_TOTAL_MS } = env.__EASYOFFICE_COPILOT_TYPING__;

function typist(clock, extra = {}) {
  return createTypist({ setTimeout: clock.setTimeout, clearTimeout: clock.clearTimeout, random: () => 0, ...extra });
}

test("a text is typed one letter per step and the steps carry the prefix so far", async () => {
  const clock = createFakeClock();
  const prefixes = [];
  const run = typist(clock).type("안녕하세요", (text) => prefixes.push(text));
  clock.drain();
  assert.deepEqual(await run, { typed: 5, completed: true });
  assert.deepEqual(prefixes, ["안", "안녕", "안녕하", "안녕하세", "안녕하세요"]);
});

test("the gap between letters is humanized around the type speed", () => {
  const clock = createFakeClock();
  const stamps = [];
  const t = createTypist({ setTimeout: clock.setTimeout, clearTimeout: clock.clearTimeout, random: () => 1, typeSpeed: 20 });
  void t.type("abc", () => stamps.push(clock.now()));
  clock.drain();
  // 원본 humanizer: round(random * speed / 2) + speed. random이 1이면 20 + 10 = 30ms다.
  assert.deepEqual(stamps, [30, 60, 90]);
});

test("a sentence end rests before the next letter", async () => {
  const clock = createFakeClock();
  const at = [];
  const t = createTypist({ setTimeout: clock.setTimeout, clearTimeout: clock.clearTimeout, random: () => 0, typeSpeed: 20, sentencePauseMs: 400 });
  const run = t.type("가. 나", () => at.push(clock.now()));
  clock.drain();
  await run;
  const gaps = at.map((x, i) => x - (at[i - 1] ?? 0));
  assert.ok(gaps[2] >= 400, `the step after the period waits: ${gaps}`);
  assert.ok(gaps[1] < 100, "the step after the space does not");
});

test("a long text takes several letters per step so the total stays under the limit", async () => {
  const clock = createFakeClock();
  const text = "가나다라마바사 ".repeat(400);
  const t = typist(clock, { sentencePauseMs: 0 });
  let steps = 0;
  const run = t.type(text, () => (steps += 1));
  clock.drain();
  assert.deepEqual(await run, { typed: text.length, completed: true });
  assert.ok(clock.now() <= MAX_TOTAL_MS + 2000, `took ${clock.now()}ms`);
  assert.ok(steps < text.length / 2);
});

test("stop ends where it is and keeps the typed length; finish writes the rest", async () => {
  let clock = createFakeClock();
  let t = typist(clock);
  const seen = [];
  let run = t.type("열두 글자의 문장입니다", (x) => seen.push(x));
  clock.advance(24 * 4);
  t.stop();
  const result = await run;
  assert.equal(result.completed, false);
  assert.equal(result.typed, seen.at(-1).length);
  assert.ok(result.typed > 0 && result.typed < 12);
  assert.equal(t.running, false);

  clock = createFakeClock();
  t = typist(clock);
  const all = [];
  run = t.type("끝까지", (x) => all.push(x));
  clock.advance(30);
  t.finish();
  assert.deepEqual(await run, { typed: 3, completed: true });
  assert.equal(all.at(-1), "끝까지");
});

test("pause holds the next step and resume goes on from the same place", async () => {
  const clock = createFakeClock();
  const blinks = [];
  const t = typist(clock, { onBlink: (b) => blinks.push(b) });
  const seen = [];
  const run = t.type("가나다라", (x) => seen.push(x));
  clock.advance(60);
  t.pause();
  clock.advance(2000);
  const frozen = seen.length;
  assert.ok(frozen < 4);
  assert.equal(t.paused, true);
  t.resume();
  clock.drain();
  assert.deepEqual(await run, { typed: 4, completed: true });
  assert.equal(seen.at(-1), "가나다라");
  assert.ok(blinks.includes(true) && blinks.includes(false), "the caret blinks while it rests");
});

test("an emoji or a pair of surrogates is never split", async () => {
  const clock = createFakeClock();
  const seen = [];
  const run = typist(clock).type("a😀b", (x) => seen.push(x));
  clock.drain();
  await run;
  assert.deepEqual(seen, ["a", "a😀", "a😀b"]);
  assert.deepEqual(stopsOf("a😀b"), [1, 3, 4]);
});

test("an empty text finishes at once and the literal caret and backtick characters are typed as they are", async () => {
  const clock = createFakeClock();
  const t = typist(clock);
  assert.deepEqual(await t.type("", () => {}), { typed: 0, completed: true });
  const seen = [];
  const run = t.type("x^2 `코드`", (x) => seen.push(x));
  clock.drain();
  await run;
  assert.equal(seen.at(-1), "x^2 `코드`");
  await settle();
});

test("starting a second text ends the first without losing what it typed", async () => {
  const clock = createFakeClock();
  const t = typist(clock);
  const first = t.type("첫째 글", () => {});
  clock.advance(30);
  const second = t.type("둘째", () => {});
  clock.drain();
  assert.equal((await first).completed, false);
  assert.equal((await second).completed, true);
});
