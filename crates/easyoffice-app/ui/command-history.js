(function (host) {
  "use strict";

  // The two stacks of undo steps with the rules that keep them bounded. Reference implementation: `CommandHistory` of
  // rhwp-studio (https://github.com/edwardkim/rhwp, commit 1a76570e833917d15817415a53c09ad61ab3203f,
  // `rhwp-studio/src/engine/history.ts` lines 43-91 and 184-248, MIT, Copyright (c) 2025-2026 Edward Kim), moved into
  // plain JavaScript with the same method order and the same eviction order:
  //   - a new step empties the redo stack, then the oldest step leaves when the stack is over its step count (`record`);
  //   - then the budget is enforced from the oldest step on, and the newest step always stays (`enforceBudget`, ref
  //     `enforceSnapshotBudget`);
  //   - an undo makes the stacks heavier (the step moves to the redo stack as a bigger snapshot), so there the
  //     furthest redo step leaves first, then the oldest undo step (`enforceBudgetAfterUndo`);
  //   - an undo or redo moves the step to the other stack once it is applied (`undo`/`redo`, ref lines 184-241).
  // What is not moved:
  //   - the drop of a step that fails while it is applied (ref lines 195-203). The reference keeps a step whose command
  //     answers `retainOnFailure`; every step here does, because the failure is a request the engine refused and the
  //     same undo can be tried again;
  //   - the merge of neighbouring commands (`mergeWith`; a typing run here is one step through the typing epoch of
  //     `doc-ime.js`) and `isNoOp` (a change that changes nothing never reaches the stack), because a step here is a
  //     whole snapshot or a sheet delta, not a command that can be asked.
  // The reference counts WASM snapshot ids; here a step weighs its bytes, and the byte bound is a hard bound (PD-221):
  // it also holds the newest step, which the reference never drops.

  function createCommandHistory({ maxSteps, maxWeight, weightOf }) {
    const undoStack = [];
    const redoStack = [];
    // The weight of both stacks as a running sum, so a push does not weigh every step again (the reference scans its
    // stacks on each edit and skips the scan for steps that hold no snapshot).
    let live = 0;

    const weigh = (command) => (command.bytes ??= weightOf(command));

    function enforceBudget() {
      while (live > maxWeight && undoStack.length > 1) live -= weigh(undoStack.shift());
    }

    function enforceBudgetAfterUndo() {
      while (live > maxWeight && redoStack.length > 1) live -= weigh(redoStack.shift());
      enforceBudget();
    }

    // The bound that holds whatever the order above left: oldest step first, the redo stack after the undo stack.
    function enforceHardBound() {
      while (live > maxWeight && undoStack.length + redoStack.length > 0) {
        live -= weigh(undoStack.length > 0 ? undoStack.shift() : redoStack.shift());
      }
    }

    function clear() {
      undoStack.length = 0;
      redoStack.length = 0;
      live = 0;
    }

    // Ref `recordWithoutExecute`: the change is already in the document, only the way back is recorded. A step that is
    // over the byte bound on its own ends the record (PD-221).
    function record(command) {
      const weight = weigh(command);
      if (weight > maxWeight) {
        clear();
        return false;
      }
      for (const dropped of redoStack) live -= dropped.bytes;
      redoStack.length = 0;
      undoStack.push(command);
      live += weight;
      if (undoStack.length > maxSteps) live -= weigh(undoStack.shift());
      enforceBudget();
      return true;
    }

    // Ref `undo`/`redo`. `apply(command, commit)` does the step; `commit(inverse)` moves it to the other stack at the point
    // the caller chooses, `apply` answers `false` for a step that is refused (it stays where it is). A throw before the
    // commit leaves the step where it is, so the same step can be tried again; after the commit the stacks stay as moved.
    async function step(isUndo, apply) {
      const source = isUndo ? undoStack : redoStack;
      const dest = isUndo ? redoStack : undoStack;
      const command = source.at(-1);
      if (!command) return null;
      const commit = (inverse) => {
        source.pop();
        dest.push(inverse);
        live += weigh(inverse) - weigh(command);
        if (isUndo) enforceBudgetAfterUndo();
        enforceHardBound();
      };
      return (await apply(command, commit)) !== false;
    }

    return {
      record,
      clear,
      undo: (apply) => step(true, apply),
      redo: (apply) => step(false, apply),
      peekUndoTop: () => undoStack.at(-1) ?? null,
      peekRedoTop: () => redoStack.at(-1) ?? null,
      canUndo: () => undoStack.length > 0,
      canRedo: () => redoStack.length > 0,
      sizes: () => ({ undo: undoStack.length, redo: redoStack.length, bytes: live }),
    };
  }

  host.__EASYOFFICE_CREATE_COMMAND_HISTORY__ = createCommandHistory;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
