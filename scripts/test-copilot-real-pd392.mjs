import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The parts of the pointer's real operation (PD-392) that do not need a layout: the history group that makes a turn one undo step, and the
// turn flow that splits a response between the real controls and the internal path. What the pointer does to real elements is measured in a
// real WebKit (`scripts/webkit-dom/copilot-real.mjs`).

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

function historyOf() {
  const loaded = {};
  loadUiModule(UI("sheet-refs.js"), loaded);
  loadUiModule(UI("sheet-delta.js"), loaded);
  loadUiModule(UI("command-history.js"), loaded);
  loadUiModule(UI("workspace-history.js"), loaded);
  return loaded.__EASYOFFICE_CREATE_WORKSPACE_HISTORY__({
    document: { getElementById: () => ({ textContent: "" }) },
    invokeIpc: async () => ({}),
    sheetDataStore: {},
    sheetStoreHelpers: {},
    ribbonIpcPayload: () => "{}",
    sheetHistory: {},
    getSheetController: () => ({}),
    getActiveSheetName: () => "S",
    captureWorkspaceSnapshot: () => ({}),
    restoreWorkspaceSnapshot: async () => {},
    enqueueWorkspaceHistoryOp: (fn) => fn(),
  });
}

test("the records the controls leave inside a group are not kept, the epoch still moves, and the one record after it is the one step", () => {
  const history = historyOf();
  const epoch = history.getEpoch();
  history.beginGroup();
  history.pushRecord({ kind: "snapshot", sheets: {} });
  history.pushRecord({ kind: "snapshot", sheets: {} });
  history.endGroup();
  assert.equal(history.sizes().undo, 0, "nothing of the group is on the stack");
  assert.equal(history.getEpoch(), epoch + 2, "the history still counts the changes");
  history.pushRecord({ kind: "snapshot", sheets: {} });
  assert.equal(history.sizes().undo, 1, "after the group a record is kept again, one for the turn");
});

function turnOf({ realRuns = [], performed = [] } = {}) {
  const env = {};
  loadUiModule(UI("copilot-real-turn.js"), env);
  const calls = [];
  const slots = { active: null };
  const turn = env.__EASYOFFICE_COPILOT_REAL_TURN__.createRealTurn({
    real: {
      run: (mod, ops) => {
        calls.push(["real", ops.map((o) => o.id)]);
        const answer = realRuns.shift();
        return { done: Promise.resolve(answer(ops)), stop() {}, discard() {} };
      },
    },
    performer: {
      perform: (mod, ops) => {
        calls.push(["perform", ops.map((o) => o.id)]);
        return { done: Promise.resolve(performed.shift()?.(ops) ?? { operations: ops, completed: true }), stop() {}, discard() {} };
      },
    },
    applyOperations: async (mod, mode, ops, turnId, options) => {
      calls.push(["internal", ops.map((o) => o.id), options.noCommit === true]);
      return { applied: true, locations: [{ label: "내부 " + ops.map((o) => o.id).join("") }] };
    },
    restoreSnapshot: async () => calls.push(["restore"]),
    beginGroup: () => calls.push(["begin"]),
    endGroup: () => calls.push(["end"]),
    commit: (snap, id) => calls.push(["commit", snap, id]),
    afterReal: (mod) => calls.push(["after", mod]),
    setActiveRun: (run) => {
      slots.active = run;
    },
    endRun: () => calls.push(["endRun"]),
  });
  return { turn, calls };
}

const ops = (...ids) => ids.map((id) => ({ id }));

test("a turn that the pointer does by hand commits one undo step after the group ends", async () => {
  const { turn, calls } = turnOf({ realRuns: [(list) => ({ completed: list.length, locations: [{ label: "손" }], rest: [], aborted: null })] });
  const res = await turn.apply("DOC", "edit", ops("a", "b"), "t1", [{ real: true, ops: ops("a", "b") }], "snap");
  assert.deepEqual(calls.map((c) => c[0]), ["begin", "real", "after", "end", "commit", "endRun"]);
  assert.deepEqual(calls.at(-2), ["commit", "snap", "t1"]);
  assert.equal(res.applied, true);
  assert.deepEqual(res.locations, [{ label: "손" }]);
});

test("a failed operation and everything after it go to the internal path in order, without a commit of their own", async () => {
  const { turn, calls } = turnOf({ realRuns: [(list) => ({ completed: 1, locations: [{ label: "손" }], rest: list.slice(1), aborted: null, failure: new Error("x") })] });
  const res = await turn.apply("SHEET", "edit", ops("a", "b", "c"), "t1", [{ real: true, ops: ops("a", "b") }, { real: false, ops: ops("c") }], "snap");
  assert.deepEqual(calls.filter((c) => c[0] === "internal"), [["internal", ["b", "c"], true]]);
  assert.equal(calls.filter((c) => c[0] === "commit").length, 1, "one step for the turn");
  assert.deepEqual(res.locations.map((l) => l.label), ["손", "내부 bc"]);
});

test("a stop ends the turn where it is, and a discard puts the turn's snapshot back", async () => {
  const stopped = turnOf({ realRuns: [() => ({ completed: 1, locations: [], rest: ops("b"), aborted: "stop" })] });
  const a = await stopped.turn.apply("DOC", "edit", ops("a", "b"), "t1", [{ real: true, ops: ops("a", "b") }], "snap");
  assert.equal(a.interrupted, true);
  assert.equal(stopped.calls.some((c) => c[0] === "internal"), false, "the rest is not applied");
  assert.equal(stopped.calls.some((c) => c[0] === "commit"), true, "what was done stays one step");
  const discarded = turnOf({ realRuns: [() => ({ completed: 1, locations: [], rest: ops("b"), aborted: "discard" })] });
  const b = await discarded.turn.apply("DOC", "edit", ops("a", "b"), "t1", [{ real: true, ops: ops("a", "b") }], "snap");
  assert.equal(b.discarded, true);
  assert.deepEqual(discarded.calls.filter((c) => c[0] === "restore" || c[0] === "commit").map((c) => c[0]), ["restore"]);
});

test("the segments that the pointer cannot do are performed as before and applied without a commit", async () => {
  const { turn, calls } = turnOf({ realRuns: [(list) => ({ completed: list.length, locations: [], rest: [], aborted: null })] });
  await turn.apply("SHEET", "edit", ops("a", "b"), "t1", [{ real: true, ops: ops("a") }, { real: false, ops: ops("b") }], "snap");
  assert.deepEqual(calls.map((c) => c[0]), ["begin", "real", "perform", "internal", "after", "end", "commit", "endRun"]);
});
