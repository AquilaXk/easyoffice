import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createFakeClock, settle } from "./test-support/fake-clock.mjs";

// PD-304: the scene player of the copilot's pointer gestures (the action timer and the `at` positions of the sequence).

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("copilot-timeline.js"), env);
const { createTimer, calcNextTime, resolveSteps, createScenePlayer } = env.__EASYOFFICE_COPILOT_TIMELINE__;

const timingOf = (clock) => ({ now: clock.now, requestAnimationFrame: clock.requestAnimationFrame, cancelAnimationFrame: clock.cancelAnimationFrame });

// Plays frame by frame until `ready()` holds (a fixed span would hide a scene that ends a frame late).
async function playUntil(clock, ready, limitMs = 2000) {
  for (let t = 0; t < limitMs && !ready(); t += 16) {
    clock.advance(16);
    await settle();
  }
  assert.ok(ready(), "the moment never came");
}

test("an `at` is read as an absolute time, an offset from the previous end, or a place against the previous start", () => {
  assert.equal(calcNextTime(500, 120, 100), 120);
  assert.equal(calcNextTime(500, "+50", 100), 550);
  assert.equal(calcNextTime(500, "-200", 100), 300);
  assert.equal(calcNextTime(100, "-200", 0), 0, "never before the start");
  assert.equal(calcNextTime(500, "<", 100), 100);
  assert.equal(calcNextTime(500, "<30", 100), 130);
});

test("steps without an `at` follow the end of the step before, and `<` starts together with it", () => {
  const { timed, end } = resolveSteps([{ run() {} }, { at: "<", duration: 420 }, { run() {} }, { at: "+80", duration: 100 }, { at: 10, duration: 5 }]);
  assert.deepEqual(timed.map((t) => t.start), [0, 0, 420, 500, 10]);
  assert.equal(end, 600);
});

test("the timer runs actions in the order of their delays, also when they are added out of order, and not before their time", () => {
  const clock = createFakeClock();
  const timer = createTimer(timingOf(clock));
  const log = [];
  for (const delay of [100, 30, 100, 0]) timer.addAction({ delay, doAction: () => log.push(delay + "@" + clock.now()) });
  timer.start();
  clock.advance(15);
  assert.deepEqual(log, [], "the first frame is a frame after the start, the actions at 0 included");
  clock.advance(1);
  assert.deepEqual(log, ["0@16"]);
  clock.advance(16);
  assert.deepEqual(log, ["0@16", "30@32"]);
  clock.advance(80);
  assert.deepEqual(log, ["0@16", "30@32", "100@112", "100@112"]);
  assert.equal(clock.pending, 0, "an emptied timer asks for no more frames");
});

test("clearing the timer drops what is left, also from inside an action", () => {
  const clock = createFakeClock();
  const timer = createTimer(timingOf(clock));
  const log = [];
  timer.addAction({ delay: 20, doAction: () => { log.push("a"); timer.clear(); } });
  timer.addAction({ delay: 20, doAction: () => log.push("b") });
  timer.addAction({ delay: 500, doAction: () => log.push("c") });
  timer.start();
  clock.advance(1000);
  assert.deepEqual(log, ["a"]);
  assert.equal(clock.pending, 0);
});

test("a scene runs its steps at their times and ends when the last one, and the animation it started, has ended", async () => {
  const clock = createFakeClock();
  const player = createScenePlayer(timingOf(clock));
  const log = [];
  let finishAnimation = null;
  let done = null;
  const scene = player.play([
    { run: () => log.push("press@" + clock.now()) },
    { at: "<", duration: 200, run: () => new Promise((resolve) => { finishAnimation = resolve; log.push("draw@" + clock.now()); }) },
    { run: () => log.push("release@" + clock.now()) },
  ]);
  scene.then((value) => { done = value; });
  await playUntil(clock, () => log.includes("release@208"));
  assert.deepEqual(log, ["press@16", "draw@16", "release@208"]);
  await settle();
  assert.equal(done, null, "the animation has not ended yet");
  finishAnimation();
  await settle();
  assert.equal(done, true);
});

test("clearing a scene ends it with false and stops the steps that were left", async () => {
  const clock = createFakeClock();
  const player = createScenePlayer(timingOf(clock));
  const log = [];
  let done = null;
  player.play([{ run: () => log.push("press") }, { at: 300, run: () => log.push("release") }]).then((value) => { done = value; });
  await playUntil(clock, () => log.length === 1);
  player.clear();
  await settle();
  assert.equal(done, false);
  clock.advance(1000);
  assert.deepEqual(log, ["press"]);
  assert.equal(clock.pending, 0);
});

test("a new scene takes over from the one that is running, which ends with false", async () => {
  const clock = createFakeClock();
  const player = createScenePlayer(timingOf(clock));
  const results = [];
  player.play([{ duration: 1000 }]).then((value) => results.push("first:" + value));
  player.play([{ duration: 32 }]).then((value) => results.push("second:" + value));
  await playUntil(clock, () => results.length === 2);
  assert.deepEqual(results, ["first:false", "second:true"]);
});

test("an error thrown by a step, or rejected by its animation, rejects the scene and stops it", async () => {
  const clock = createFakeClock();
  const player = createScenePlayer(timingOf(clock));
  const log = [];
  const thrown = player.play([{ run() { throw new Error("step"); } }, { at: 200, run: () => log.push("late") }]);
  const outcome = thrown.then(() => "resolved", (error) => error.message);
  await playUntil(clock, () => clock.now() >= 16);
  assert.equal(await outcome, "step");
  clock.advance(1000);
  assert.deepEqual(log, []);

  const rejected = player.play([{ duration: 100, run: () => Promise.reject(new Error("animation")) }]);
  const outcome2 = rejected.then(() => "resolved", (error) => error.message);
  clock.advance(16);
  await settle();
  assert.equal(await outcome2, "animation");
});
