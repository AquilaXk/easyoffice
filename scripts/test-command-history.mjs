import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The undo and redo stacks (command-history.js) follow the CommandHistory of the reference editor
// (rhwp-studio `engine/history.ts`): what a step is, how many are kept, and which one leaves first.

const UI = (name) => fileURLToPath(new URL(`../crates/easyoffice-app/ui/${name}`, import.meta.url));
const REF_CORE = fileURLToPath(new URL("../third_party/rhwp/src/document_core/commands/document.rs", import.meta.url));
const MAX_STEPS = 5;
const MAX_WEIGHT = 100;

function makeHistory(options = {}) {
  const env = {};
  loadUiModule(UI("command-history.js"), env);
  return env.__EASYOFFICE_CREATE_COMMAND_HISTORY__({ maxSteps: MAX_STEPS, maxWeight: MAX_WEIGHT, weightOf: (c) => c.w ?? 1, ...options });
}
const step = (id, w = 1) => ({ id, bytes: w });
// The apply callback of a step that moves to the other stack as `inverse`.
const moveAs = (inverse) => (_command, commit) => commit(inverse);

test("a new step empties the redo stack (ref execute/recordWithoutExecute)", async () => {
  const h = makeHistory();
  h.record(step(1));
  h.record(step(2));
  await h.undo(moveAs(step("r2")));
  assert.deepEqual([h.sizes().undo, h.sizes().redo], [1, 1]);
  h.record(step(3));
  assert.deepEqual([h.sizes().undo, h.sizes().redo], [2, 0]);
  assert.equal(h.sizes().bytes, 2);
});

test("the oldest step leaves when the stack passes its step count", () => {
  const h = makeHistory();
  for (let id = 1; id <= MAX_STEPS + 3; id += 1) h.record(step(id));
  assert.equal(h.sizes().undo, MAX_STEPS);
  assert.equal(h.peekUndoTop().id, MAX_STEPS + 3);
});

test("the budget takes steps from the oldest side and never the newest one", () => {
  const h = makeHistory();
  for (let id = 1; id <= 4; id += 1) h.record(step(id, 40));
  assert.equal(h.sizes().undo, 2, "two steps of 40 fit under 100");
  assert.equal(h.peekUndoTop().id, 4);
  assert.equal(h.sizes().bytes, 80);
});

test("a step that is over the bound on its own ends the record (PD-221, a hard bound the reference does not have)", async () => {
  const h = makeHistory();
  h.record(step(1));
  await h.undo(moveAs(step("r1")));
  assert.equal(h.record(step(2, MAX_WEIGHT + 1)), false);
  assert.deepEqual(h.sizes(), { undo: 0, redo: 0, bytes: 0 });
});

test("an undo that makes the stacks heavier takes the furthest redo step first, then the oldest undo step (ref enforceSnapshotBudgetAfterUndo)", async () => {
  const h = makeHistory();
  h.record(step("u1", 30));
  h.record(step("u2", 30));
  h.record(step("u3", 30));
  await h.undo(moveAs(step("r3", 30)));
  await h.undo(moveAs(step("r2", 30)));
  assert.equal(h.sizes().bytes, 90);
  // The third undo moves a 60 byte inverse: 30 + 60 + 30 + 30 is over 100, so a redo step goes, not the undo step.
  await h.undo(moveAs(step("r1", 60)));
  assert.deepEqual([h.sizes().undo, h.sizes().redo], [0, 2]);
  assert.equal(h.peekRedoTop().id, "r1");
  assert.equal(h.sizes().bytes, 90, "the furthest redo step (r3) left");
});

test("a redo stays within the bound by dropping the oldest step", async () => {
  const h = makeHistory();
  h.record(step("u1", 50));
  h.record(step("u2", 40));
  await h.undo(moveAs(step("r2", 10)));
  await h.redo(moveAs(step("u2b", 70)));
  assert.ok(h.sizes().bytes <= MAX_WEIGHT);
  assert.equal(h.peekUndoTop().id, "u2b");
});

test("a step the caller refuses stays where it is", async () => {
  const h = makeHistory();
  h.record(step(1));
  assert.equal(await h.undo(async () => false), false);
  assert.deepEqual(h.sizes(), { undo: 1, redo: 0, bytes: 1 });
});

test("a step that throws before it moves stays where it is, so the same undo can be tried again", async () => {
  const h = makeHistory();
  h.record(step(1));
  h.record(step(2));
  await assert.rejects(() => h.undo(async () => { throw new Error("engine refused"); }), /engine refused/);
  assert.deepEqual(h.sizes(), { undo: 2, redo: 0, bytes: 2 });
  assert.equal(h.peekUndoTop().id, 2);
  assert.equal(await h.undo(moveAs(step("r2"))), true);
  assert.deepEqual([h.sizes().undo, h.sizes().redo], [1, 1]);
});

test("a step that throws after it moved keeps the stacks as they were moved", async () => {
  const h = makeHistory();
  h.record(step(1));
  await assert.rejects(() => h.undo(async (_command, commit) => { commit(step("r1")); throw new Error("late"); }), /late/);
  assert.deepEqual([h.sizes().undo, h.sizes().redo], [0, 1]);
});

test("an empty stack answers null and canUndo/canRedo follow the stacks", async () => {
  const h = makeHistory();
  assert.equal(await h.undo(moveAs(step("x"))), null);
  assert.equal(await h.redo(moveAs(step("x"))), null);
  h.record(step(1));
  assert.deepEqual([h.canUndo(), h.canRedo()], [true, false]);
  await h.undo(moveAs(step("r1")));
  assert.deepEqual([h.canUndo(), h.canRedo()], [false, true]);
});

test("the running weight equals the weight of the steps on both stacks after any order of calls", async () => {
  const h = makeHistory();
  let seed = 12345;
  const next = (n) => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed % n;
  };
  const sum = () => h.sizes().bytes;
  for (let i = 0; i < 400; i += 1) {
    const pick = next(4);
    if (pick === 0) h.record(step(i, 1 + next(45)));
    else if (pick === 1) await h.undo(moveAs(step("r" + i, 1 + next(45))));
    else if (pick === 2) await h.redo(moveAs(step("u" + i, 1 + next(45))));
    else if (next(5) === 0) await h.undo(async () => { throw new Error("x"); }).catch(() => {});
    assert.ok(sum() <= MAX_WEIGHT, "bound at call " + i);
    assert.ok(sum() >= 0);
  }
  while (h.canUndo()) await h.undo(moveAs(step("z", 1)));
  while (h.canRedo()) await h.redo(moveAs(step("z", 1)));
  while (h.canUndo()) await h.undo(moveAs(step("z", 1)));
  // Every remaining step is a redo step of weight 1: the running sum must match them one by one.
  assert.equal(h.sizes().bytes, h.sizes().redo + h.sizes().undo);
});

test("the workspace stacks keep as many steps as the reference core keeps snapshots", { skip: !fs.existsSync(REF_CORE) && "third_party/rhwp is not checked out" }, () => {
  const env = {};
  loadUiModule(UI("command-history.js"), env);
  loadUiModule(UI("sheet-refs.js"), env);
  loadUiModule(UI("sheet-delta.js"), env);
  loadUiModule(UI("workspace-history.js"), env);
  const declared = /pub const MAX_SNAPSHOTS: usize = (\d+);/.exec(fs.readFileSync(REF_CORE, "utf8"));
  assert.ok(declared, "MAX_SNAPSHOTS is declared in the pinned core");
  assert.equal(env.__EASYOFFICE_HISTORY_LIMITS__.MAX_HISTORY_STEPS, Number(declared[1]));
});
