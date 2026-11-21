(function (host) {
  "use strict";

  // Korean (and other) IME composition in the document editor and the undo step it leaves. Reference implementation:
  // rhwp-studio (https://github.com/edwardkim/rhwp, commit 1a76570e833917d15817415a53c09ad61ab3203f, MIT, Copyright (c)
  // 2025-2026 Edward Kim), `rhwp-studio/src/engine/input-handler-text.ts`: `onCompositionStart` (line 455),
  // `onCompositionEnd` (line 510) and the composing branch of `onInput` (lines 620-707). The rule is the reference's: while
  // a composition runs, each update changes the text and the history is not written ("Undo 스택에는 기록하지 않음",
  // line 635); `compositionend` records the composition once, and only when it left text (`finalLength > 0`, line 578),
  // so a cancelled composition leaves no step and does not end the redo steps.
  // The adapter differences: the reference keeps the text in a hidden textarea and replaces the composing range in its
  // document model on each update, so it records after the fact (`recordWithoutExecute`). The editor here is a
  // contenteditable and the browser changes the text itself, so the document as it was before the composition is
  // taken at `compositionstart` (before the first update changes anything) and pushed at `compositionend`. The ghost
  // `input` of the same text after `compositionend` (`_lastComposedText`, line 549 and 790) is a textarea artefact and
  // has no counterpart here: a later `input` is the same typing run and adds no step.
  //
  // A typing run is one undo step: the snapshot is taken at the first change after the run started, and the run ends when
  // anything else is pushed to the history (the epoch moves) or the editor loses or gets the focus.

  const SCRIPT_MODULES = new Set(["DOC", "HWP"]);

  const charCount = (text) => Array.from(String(text ?? "")).length;

  function createDocIme(deps) {
    const { getActiveModule, getDocTab, captureWorkspaceSnapshot, pushRawUndoSnapshot, getEpoch, markDocStale, computeStats, selectionIsRange } = deps;
    let composing = false;
    // The length in characters of the composing text after the last update (ref `compositionLength`).
    let compositionLength = 0;
    // The snapshot taken at `compositionstart`, pushed at `compositionend`.
    let pending = null;
    // The composition started over a selection: the browser replaces the selected text, so that is a change of its own.
    let replacedSelection = false;
    let typingEpoch = -1;

    const inDocument = () => SCRIPT_MODULES.has(getActiveModule());

    // The document as it was before this typing run, or null when the run has its snapshot already (or there is no
    // saved document state to go back to). The saved tab state, not the live DOM, is the state before: the live DOM may
    // already hold the first change when the snapshot is asked for.
    function takeRunSnapshot(force = false) {
      const mod = getActiveModule();
      const tab = SCRIPT_MODULES.has(mod) ? getDocTab(mod) : null;
      if (!tab || (!force && typingEpoch === getEpoch())) return null;
      const before = structuredClone(tab);
      const snapshot = captureWorkspaceSnapshot(false);
      snapshot.docTabs[mod] = before;
      return snapshot;
    }

    function recordRun(snapshot) {
      pushRawUndoSnapshot(snapshot);
      typingEpoch = getEpoch();
    }

    function resetRun() {
      typingEpoch = -1;
    }

    // Ref `onCompositionStart`: the flags start over, the composition has no text yet.
    function onCompositionStart() {
      composing = true;
      compositionLength = 0;
      replacedSelection = Boolean(selectionIsRange?.());
      // A selection that the composition replaces is recorded as a step of its own, also when this typing run has one: the
      // reference deletes the selection first, as its own command, and the composition is a second one.
      pending = takeRunSnapshot(replacedSelection);
    }

    // Ref `onCompositionEnd`: the flags are reset first, then the composition is recorded once if it left text. A composition
    // that replaced a selection is recorded whatever it left, so the replaced text can come back.
    function onCompositionEnd() {
      const finalLength = compositionLength;
      const snapshot = pending;
      const replaced = replacedSelection;
      abandon();
      if (snapshot && (finalLength > 0 || replaced)) recordRun(snapshot);
      computeStats();
    }

    // The composition is over without the end event (the focus went, another module showed): nothing is waiting for it.
    function abandon() {
      composing = false;
      compositionLength = 0;
      pending = null;
      replacedSelection = false;
    }

    // The editor loses the focus: a composition still running ends here, and the typing run ends.
    function onBlur() {
      if (composing) onCompositionEnd();
      resetRun();
    }

    // A typed character outside a composition: the run's snapshot, before the browser changes the text.
    function onBeforeInput(event) {
      if (composing || event?.isComposing) return;
      const snapshot = takeRunSnapshot();
      if (snapshot) recordRun(snapshot);
    }

    // Ref `onInput`: a composition update only follows the length of the composing text and writes no history.
    function onInput(event) {
      if (composing) {
        compositionLength = event?.inputType === "deleteCompositionText" ? 0 : charCount(event?.data);
        if (inDocument()) markDocStale();
        return;
      }
      if (inDocument()) {
        const snapshot = takeRunSnapshot();
        if (snapshot) recordRun(snapshot);
        markDocStale();
      }
      computeStats();
    }

    function bind(editor) {
      editor.addEventListener("focus", resetRun);
      editor.addEventListener("blur", onBlur);
      editor.addEventListener("compositionstart", onCompositionStart);
      editor.addEventListener("compositionend", onCompositionEnd);
      editor.addEventListener("beforeinput", onBeforeInput);
      editor.addEventListener("input", onInput);
    }

    return { bind, abandon, isComposing: () => composing && inDocument(), onCompositionStart, onCompositionEnd, onBeforeInput, onInput, resetRun };
  }

  host.__EASYOFFICE_CREATE_DOC_IME__ = createDocIme;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
