import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadPerformParts, loadUiModule } from "./test-support/load-ui-module.mjs";

// The rollback of a failed operation list (PD-264): a failed restore is reported with the original error, and a restore that
// would drop the user's own edits (the history moved while the operations ran) is not done.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

function actions({ restore, epoch }) {
  const env = {};
  loadUiModule(UI("sheet-refs.js"), env);
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-diff.js"), env);
  loadUiModule(UI("copilot-typing.js"), env);
  loadUiModule(UI("copilot-cursor.js"), env);
  loadUiModule(UI("copilot-timeline.js"), env);
  loadUiModule(UI("copilot-preview.js"), env);
  loadPerformParts(env);
  loadUiModule(UI("copilot-perform.js"), env);
  loadUiModule(UI("copilot-edit-mode.js"), env);
  loadUiModule(UI("copilot-actions.js"), env);
  const calls = [];
  const instance = env.__EASYOFFICE_CREATE_COPILOT_ACTIONS__({
    document: {},
    sheetDataStore: {},
    sheetRowCounts: {},
    slideDeckState: [],
    getActiveModule: () => "SHEET",
    getActiveSheetName: () => "S",
    getActiveSlideIndex: () => 0,
    getSheetController: () => ({}),
    getWorkspacePanels: () => ({}),
    getPdfEdit: () => null,
    activatePane: async () => {},
    captureWorkspaceSnapshot: () => ({ snapshot: true }),
    restoreWorkspaceSnapshot: async (...args) => {
      calls.push(args[0]);
      return restore();
    },
    commitDeferredUndoSnapshot: () => {},
    getHistoryMutationEpoch: epoch,
    noteActivity: () => {},
    extractEditableDocumentText: () => "",
    localScrubPii: () => ({ totalMasked: 0, scrubbedText: "" }),
  });
  return { instance, calls };
}

const failing = [{ op: "shell.exec", cmd: "id" }];

test("a failed operation list restores the snapshot and throws the original error", async () => {
  const { instance, calls } = actions({ restore: async () => {}, epoch: () => 1 });
  await assert.rejects(instance.applyOperations("SHEET", "edit", failing, "t"), /지원하지 않는 코파일럿 작업/);
  assert.equal(calls.length, 1);
});

test("a restore that fails is reported with the original error", async () => {
  const { instance } = actions({ restore: async () => { throw new Error("복원 오류"); }, epoch: () => 1 });
  await assert.rejects(instance.applyOperations("SHEET", "edit", failing, "t"), (err) => {
    assert.match(err.message, /지원하지 않는 코파일럿 작업/);
    assert.match(err.message, /되돌리지 못했습니다/);
    assert.match(err.message, /복원 오류/);
    return true;
  });
});

test("when the history moved while the operations ran, nothing is restored over the user's edits", async () => {
  let epoch = 0;
  // The first reading is the one before the operations; the next is at the failure.
  const { instance, calls } = actions({ restore: async () => {}, epoch: () => (epoch += 1) });
  await assert.rejects(instance.applyOperations("SHEET", "edit", failing, "t"), /다른 편집/);
  assert.equal(calls.length, 0);
});
