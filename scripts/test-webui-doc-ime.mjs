import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { click, createRunHarness, p, rangeOf, textNodeOf } from "./test-support/doc-run-harness.mjs";

// IME composition and the undo step it leaves (doc-ime.js, rhwp-studio `onCompositionStart`/`onCompositionEnd`/`onInput`):
// nothing is recorded while the composition runs, and one step is recorded when it ends, if it left text.

async function open() {
  const h = await createWorkspaceHarness({});
  switchModule(h.document, "DOC");
  await flushAsyncTurns(10);
  const editor = h.document.getElementById("doc-editor");
  editor.replaceChildren();
  const para = h.document.createElement("p");
  para.appendChild(h.document.createTextNode("앞"));
  editor.appendChild(para);
  editor.focus();
  await flushAsyncTurns(10);
  const history = () => h.host.__EASYOFFICE_WORKSPACE_HISTORY__;
  const steps = () => history().sizes().undo;
  // The browser changes the text between `beforeinput` and `input`.
  const fire = (type, init = {}) => editor.dispatchEvent({ type, target: para, isComposing: false, ...init });
  const compose = (text) => {
    fire("beforeinput", { inputType: "insertCompositionText", data: text, isComposing: true });
    para.firstChild.data = "앞" + text;
    fire("input", { inputType: "insertCompositionText", data: text, isComposing: true });
  };
  const undo = async () => {
    h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
    await flushAsyncTurns(20);
  };
  const live = () => editor.querySelector("p").textContent;
  return { ...h, editor, para, steps, fire, compose, undo, live, history };
}

test("compositionstart, many inputs, compositionend: one undo step, written at the end", async () => {
  const t = await open();
  const before = t.steps();
  t.fire("compositionstart");
  t.compose("ㅎ");
  t.compose("하");
  t.compose("한");
  assert.equal(t.steps(), before, "nothing is recorded while the composition runs");
  t.fire("compositionend", { data: "한" });
  assert.equal(t.steps(), before + 1, "one step at the end");
  assert.equal(t.live(), "앞한");
  await t.undo();
  assert.equal(t.live(), "앞", "the whole composition goes back in one undo");
});

test("a cancelled composition (the composing text is deleted) leaves no step and keeps the redo steps", async () => {
  const t = await open();
  t.fire("compositionstart");
  t.compose("ㅎ");
  t.compose("하");
  t.fire("beforeinput", { inputType: "deleteCompositionText", isComposing: true });
  t.para.firstChild.data = "앞";
  t.fire("input", { inputType: "deleteCompositionText", data: null, isComposing: true });
  t.fire("compositionend", { data: "" });
  assert.equal(t.steps(), 0, "nothing to undo");
  assert.equal(t.history().sizes().redo, 0);
});

test("a second composition in the same typing run adds no step; focus leaving the editor ends the run", async () => {
  const t = await open();
  const before = t.steps();
  t.fire("compositionstart");
  t.compose("한");
  t.fire("compositionend", { data: "한" });
  t.fire("compositionstart");
  t.compose("한글");
  t.fire("compositionend", { data: "글" });
  assert.equal(t.steps(), before + 1, "both compositions are in one run");
  t.fire("blur");
  t.fire("focus");
  t.fire("compositionstart");
  t.compose("한글ㅈ");
  t.fire("compositionend", { data: "ㅈ" });
  assert.equal(t.steps(), before + 2, "a new run after the focus came back");
});

test("typing outside a composition still records its run once, before the text changes", async () => {
  const t = await open();
  const before = t.steps();
  t.fire("beforeinput", { inputType: "insertText", data: "a" });
  t.para.firstChild.data = "앞a";
  t.fire("input", { inputType: "insertText", data: "a" });
  t.fire("beforeinput", { inputType: "insertText", data: "b" });
  t.para.firstChild.data = "앞ab";
  t.fire("input", { inputType: "insertText", data: "b" });
  assert.equal(t.steps(), before + 1);
  await t.undo();
  assert.equal(t.live(), "앞");
});

test("the keys of a composition do not run the workspace shortcuts, and the state follows the events", async () => {
  const t = await open();
  assert.equal(t.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true, isComposing: true }).defaultPrevented, false);
  t.fire("compositionstart");
  const keyDuring = t.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  assert.equal(keyDuring.defaultPrevented, false, "the composition flag alone keeps the shortcut out");
  t.fire("compositionend", { data: "" });
});

test("the pending format of a caret waits while a composition runs, and applies to the next typed character", async () => {
  const h = await createRunHarness([p("ab cd")]);
  const [para] = h.blocks;
  const t = textNodeOf(para);
  h.selection.addRange(rangeOf(t, 2, t, 2));
  await click(h, "btn-stylebar-bold");
  const beforeInput = (init) => {
    const ev = { type: "beforeinput", isComposing: false, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...init };
    h.editor._listeners.get("beforeinput")[0](ev);
    return ev;
  };
  h.editor.dispatchEvent({ type: "compositionstart" });
  // Some engines send the first event of a composition with the flag still off: the shared state decides.
  assert.equal(beforeInput({ inputType: "insertText", data: "ㅎ" }).defaultPrevented, false, "the composition writes its own text");
  h.editor.dispatchEvent({ type: "compositionend" });
  assert.equal(beforeInput({ inputType: "insertText", data: "X" }).defaultPrevented, true, "the next typed character takes the format");
});
