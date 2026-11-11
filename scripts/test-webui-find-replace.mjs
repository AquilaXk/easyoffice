import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// The find / replace dialog in the document and sheet modules (find-replace.js): shortcuts, the
// match count, navigation, replace all as one undo step, Korean input and the sheet options.

const DOC_HTML =
  "<p>첫 문단 apple</p><p>둘째 <b>Ap</b>ple pie</p><table><tbody><tr><td>apple 표</td><td>한글 입력</td></tr></tbody></table><p>마지막 Apple</p>";
const $ = (document, id) => document.getElementById(id);

async function openDoc() {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  $(h.document, "doc-editor").innerHTML = DOC_HTML;
  return h;
}

const ctrl = (h, key) => h.sandbox.dispatchWindowKeydown({ key, ctrlKey: true });
const type = (h, id, value) => {
  const input = $(h.document, id);
  input.value = value;
  input.dispatchEvent({ type: "input", target: input });
};
// The "n / total" label when there is one, else the status line.
const count = (h) => $(h.document, "find-count").textContent || $(h.document, "find-status").textContent;
const editorText = (h) => $(h.document, "doc-editor").textContent;

test("Ctrl+F opens the find dialog (not the palette), Ctrl+F2 adds the replace row, Ctrl+K keeps the palette", async () => {
  const h = await openDoc();
  const dialog = $(h.document, "find-dialog");
  assert.equal(ctrl(h, "f").defaultPrevented, true);
  assert.equal(dialog.open, true);
  assert.equal($(h.document, "cmd-palette-modal").open, false);
  assert.equal($(h.document, "find-replace-row").hidden, true);
  assert.equal($(h.document, "find-dialog-title").textContent, "찾기");
  assert.equal($(h.document, "find-replace-buttons").hidden, true);
  ctrl(h, "F2");
  assert.equal($(h.document, "find-replace-buttons").hidden, false, "the replace button row follows the replace mode");
  assert.equal($(h.document, "find-replace-row").hidden, false);
  assert.equal($(h.document, "find-dialog-title").textContent, "찾아 바꾸기");
  assert.equal(ctrl(h, "h").defaultPrevented, false, "Ctrl+H is not taken (macOS hide, browser history)");
  ctrl(h, "k");
  assert.equal($(h.document, "cmd-palette-modal").open, true);
});

test("the shortcut table has no repeated combination and keeps print, find and replace to one action each", async () => {
  const h = await createWorkspaceHarness();
  const { WORKSPACE_SHORTCUT_BINDINGS: table } = h.host.__EASYOFFICE_RIBBON_CATALOG__;
  const keys = table.map((b) => b.keys);
  assert.equal(new Set(keys).size, keys.length);
  const action = (k) => table.find((b) => b.keys === k)?.action;
  assert.deepEqual([action("Ctrl+F"), action("Ctrl+F2"), action("Ctrl+K"), action("Ctrl+P")], ["find", "replace", "palette", "print"]);
  assert.equal(action("Ctrl+H"), undefined);
  assert.equal(action("Ctrl+L"), undefined, "Ctrl+L is the browser address bar");
});

test("the count follows the query across paragraphs, runs and a table; options narrow it", async () => {
  const h = await openDoc();
  ctrl(h, "f");
  type(h, "find-query", "apple");
  assert.equal(count(h), "1 / 4", "paragraph, a match split over two runs, table cell, last paragraph (ignore case)");
  $(h.document, "btn-find-next").click();
  assert.equal(count(h), "2 / 4");
  for (let i = 0; i < 3; i += 1) $(h.document, "btn-find-next").click();
  assert.equal(count(h), "1 / 4", "wraps around");
  $(h.document, "btn-find-prev").click();
  assert.equal(count(h), "4 / 4");
  $(h.document, "find-opt-case").checked = true;
  $(h.document, "find-opt-case").dispatchEvent({ type: "change", target: $(h.document, "find-opt-case") });
  assert.match(count(h), / \/ 2$/, "case sensitive: the paragraph and the cell");
  type(h, "find-query", "zzz");
  assert.equal(count(h), "일치하는 항목이 없습니다");
});

test("Korean: typing waits for the composition to end, then finds the syllables", async () => {
  const h = await openDoc();
  ctrl(h, "f");
  const query = $(h.document, "find-query");
  query.dispatchEvent({ type: "compositionstart", target: query });
  type(h, "find-query", "ㅎ");
  assert.equal(count(h), "", "no search while composing");
  type(h, "find-query", "한글");
  query.dispatchEvent({ type: "compositionend", target: query });
  assert.equal(count(h), "1 / 1");
});

test("replace all is one undo step and keeps the run formatting", async () => {
  const h = await openDoc();
  const before = editorText(h);
  ctrl(h, "F2");
  type(h, "find-query", "apple");
  type(h, "find-replacement", "배");
  $(h.document, "btn-find-replace-all").click();
  await flushAsyncTurns(8);
  assert.equal(editorText(h).toLowerCase().includes("apple"), false);
  assert.equal(editorText(h).split("배").length - 1, 4);
  assert.match(count(h), /^4개를 바꿨습니다/);
  assert.ok($(h.document, "doc-editor").querySelector("b"), "the bold run is still there");
  ctrl(h, "z");
  await flushAsyncTurns(12);
  assert.equal(editorText(h), before, "one Ctrl+Z restores every replacement");
});

test("replace one replaces the current match and moves to the next", async () => {
  const h = await openDoc();
  ctrl(h, "F2");
  type(h, "find-query", "apple");
  type(h, "find-replacement", "pear");
  $(h.document, "btn-find-replace").click();
  await flushAsyncTurns(6);
  assert.equal(editorText(h).split("pear").length - 1, 1);
  assert.equal(count(h), "1 / 3", "the next match is current and the replaced one is gone from the count");
});

test("Enter in the query goes to the next match; Escape closes and clears; the dialog keeps its keys to itself", async () => {
  const h = await openDoc();
  ctrl(h, "f");
  type(h, "find-query", "apple");
  const query = $(h.document, "find-query");
  let leaked = 0;
  h.document.body.addEventListener("keydown", () => { leaked += 1; });
  query.dispatchEvent({ type: "keydown", key: "Enter" });
  assert.equal(count(h), "2 / 4");
  query.dispatchEvent({ type: "keydown", key: "Enter", shiftKey: true });
  assert.equal(count(h), "1 / 4", "Shift+Enter goes back");
  query.dispatchEvent({ type: "keydown", key: "z", ctrlKey: true });
  assert.equal(leaked, 0, "page shortcuts like undo do not run while the dialog has the focus");
  query.dispatchEvent({ type: "keydown", key: "Escape" });
  assert.equal($(h.document, "find-dialog").open, false);
});

test("the sheet module finds cell text, replaces values (not formula results), as one undo step", async () => {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(
    h.document,
    makeFixtureFile("찾기.xlsx", {
      sheets: [{ name: "Sheet1", cells: [{ ref: "A1", raw: "사과 1" }, { ref: "A2", raw: "배" }, { ref: "B1", raw: "사과 2" }, { ref: "B2", raw: "10" }, { ref: "C2", raw: "=B2*2" }] }],
    }),
  );
  await flushAsyncTurns(12);
  const cellText = (ref) => h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`)?.textContent;
  ctrl(h, "F2");
  type(h, "find-query", "사과");
  assert.equal(count(h), "1 / 2");
  $(h.document, "btn-find-next").click();
  assert.equal(count(h), "2 / 2");
  type(h, "find-replacement", "귤");
  $(h.document, "btn-find-replace-all").click();
  await flushAsyncTurns(14);
  assert.deepEqual([cellText("A1"), cellText("B1"), cellText("A2")], ["귤 1", "귤 2", "배"]);
  ctrl(h, "z");
  await flushAsyncTurns(14);
  assert.deepEqual([cellText("A1"), cellText("B1")], ["사과 1", "사과 2"], "undo restores both cells in one step");

  type(h, "find-query", "20");
  assert.equal(count(h), "1 / 1", "a formula cell is found by the value it shows");
  type(h, "find-replacement", "7");
  $(h.document, "btn-find-replace-all").click();
  await flushAsyncTurns(14);
  assert.equal(cellText("C2"), "20", "a formula's result is not replaced");
  assert.match(count(h), /수식 셀 1개는 건너뜀/);
});

// The harness has no cascade, so this one resolves `display` from shell.css with a small specificity
// count: an element with `hidden` must lose to no rule that sets another display.
function specificity(selector) {
  const ids = (selector.match(/#[\w-]+/g) || []).length;
  const classes = (selector.match(/\.[\w-]+|\[[^\]]*\]|(?<!:):(?!:)(?!not)[\w-]+/g) || []).length;
  const tags = (selector.replace(/[#.][\w-]+|\[[^\]]*\]|:[\w-]+(\([^)]*\))?/g, " ").match(/(^|[\s>+~])[a-z][\w-]*/g) || []).length;
  return ids * 10000 + classes * 100 + tags;
}

test("the find dialog's hidden elements stay hidden under the stylesheet's display rules", async () => {
  const { matchesSelectorList } = await import("./webui-css-selector.mjs");
  const { readFileSync } = await import("node:fs");
  const css = readFileSync(new URL("../crates/easyoffice-app/ui/shell.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const rules = [...css.matchAll(/^([^@{}\n][^{}]*)\{([^}]*)\}/gm)]
    .map(([, selector, body], order) => ({ selector: selector.trim(), display: body.match(/(?:^|;)\s*display:\s*([^;]+)/)?.[1].trim(), order }))
    .filter((r) => r.display);
  const h = await createWorkspaceHarness();
  const { document } = h;
  for (const id of ["find-replace-row", "find-replace-buttons", "find-count"]) {
    const el = document.getElementById(id);
    assert.equal(el.hidden, true, id + " starts hidden");
    const hits = rules.filter((r) => r.selector.split(",").some((sel) => matchesSelectorList(el, sel.trim(), null)));
    const winner = hits.sort((a, b) => specificity(a.selector) - specificity(b.selector) || a.order - b.order).at(-1);
    assert.equal(winner?.display, "none", `${id} resolves to display:none, not "${winner?.display}" from ${winner?.selector}`);
  }
});

// Tool dialogs follow the reference's modal dialog (`dialog.ts`): the first focus is the primary button
// (never the title bar's close button), Esc closes, Enter outside a text field presses the primary button,
// every other key stays inside, and a click on the backdrop does nothing.
const keyEvent = (target, key, extra = {}) => ({
  type: "keydown", key, target, ctrlKey: false, calls: [],
  stopPropagation() { this.calls.push("stop"); },
  preventDefault() { this.calls.push("prevent"); },
  ...extra,
});
const toolDialogs = (h) => h.document.querySelectorAll("dialog.tool-dialog").filter((d) => !d.classList.contains("find-dialog"));

test("every tool dialog opens with the focus on its primary button or itself, never on the close button", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  assert.ok(toolDialogs(h).length >= 7);
  for (const dialog of toolDialogs(h)) {
    dialog.showModal();
    const active = h.document.activeElement;
    assert.equal(active.classList.contains("dialog-close"), false, dialog.id + " does not focus the close button");
    const primary = dialog.querySelector(".dialog-btn-primary");
    assert.equal(active.id, primary ? primary.id : dialog.id, dialog.id + " focus");
    dialog.close();
  }
});

test("a tool dialog takes Esc and Enter like the reference, keeps other keys inside, and only while it is open", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  const dialog = $(h.document, "pii-dialog");
  const primary = dialog.querySelector(".dialog-btn-primary");
  let pressed = 0;
  primary.addEventListener("click", () => { pressed += 1; });
  dialog.showModal();
  const enter = keyEvent(dialog.querySelector(".dialog-body"), "Enter");
  h.document.dispatchEvent(enter);
  assert.equal(pressed, 1, "Enter outside a text field presses the primary button");
  assert.deepEqual(enter.calls, ["stop", "prevent"]);
  const undo = keyEvent(dialog.querySelector(".dialog-body"), "z", { ctrlKey: true });
  h.document.dispatchEvent(undo);
  assert.deepEqual(undo.calls, ["stop", "prevent"], "Ctrl+Z is stopped before the editor's shortcuts");
  const field = h.document.createElement("input");
  dialog.querySelector(".dialog-body").appendChild(field);
  const typing = keyEvent(field, "a");
  h.document.dispatchEvent(typing);
  assert.deepEqual(typing.calls, ["stop"], "typing in a field is not prevented");
  const fieldEnter = keyEvent(field, "Enter");
  h.document.dispatchEvent(fieldEnter);
  assert.equal(pressed, 1, "Enter in a text field does not press the primary button");
  dialog.dispatchEvent({ type: "click", target: dialog });
  assert.equal(dialog.open, true, "a click on the backdrop does not close it");
  const esc = keyEvent(dialog, "Escape");
  h.document.dispatchEvent(esc);
  assert.equal(dialog.open, false, "Esc closes");
  dialog.dispatchEvent({ type: "close", target: dialog });
  const after = keyEvent(h.document.body, "x");
  h.document.dispatchEvent(after);
  assert.deepEqual(after.calls, [], "keys are not captured once the dialog is closed");
});

test("the dragged position of a tool dialog stays after it is closed; the find dialog is not dragged", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  const dialog = $(h.document, "pii-dialog");
  const wrap = dialog.querySelector(".dialog-wrap");
  const title = dialog.querySelector(".dialog-title");
  dialog.showModal();
  title.dispatchEvent({ type: "mousedown", button: 0, clientX: 10, clientY: 10 });
  h.document.dispatchEvent({ type: "mousemove", clientX: 60, clientY: 40 });
  h.document.dispatchEvent({ type: "mouseup" });
  assert.equal(wrap.style.transform, "translate(50px, 30px)");
  dialog.close();
  dialog.dispatchEvent({ type: "close", target: dialog });
  assert.equal(wrap.style.transform, "translate(50px, 30px)", "the position is kept");
  const findTitle = $(h.document, "find-dialog").querySelector(".dialog-title");
  findTitle.dispatchEvent({ type: "mousedown", button: 0, clientX: 10, clientY: 10 });
  h.document.dispatchEvent({ type: "mousemove", clientX: 60, clientY: 40 });
  assert.equal($(h.document, "find-dialog").querySelector(".dialog-wrap").style.transform || "", "");
});
