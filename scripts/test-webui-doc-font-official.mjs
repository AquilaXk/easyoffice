import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// The 글꼴 dialog follows the word processor's official dialog: the 배율 box is an editable combo with its presets, the value
// fields carry their unit (`2pt`), the access keys (Alt+letter) move the focus, and the preview has a frame and a note line.

const byId = (h, id) => h.document.getElementById(id);
const fire = (el, type = "change") => el.dispatchEvent({ type, target: el, bubbles: true });
const savedRuns = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor).flatMap((b) => b.runs || []);
const withKey = (h, key) => savedRuns(h).filter((r) => r[key] !== undefined).map((r) => r[key]);

async function openDialog(h, tab = "advanced") {
  const node = h.blocks[0].childNodes[0];
  h.selection.addRange(rangeOf(node, 0, node, 1));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]'));
  await flushAsyncTurns(4);
  h.userClick(byId(h, `doc-font-tab-${tab}`));
}

const keydown = (h, init) => byId(h, "doc-font-dialog").dispatchEvent({ type: "keydown", bubbles: true, preventDefault() {}, ...init });

test("배율 is an editable combo box with the official presets", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  const scale = byId(h, "doc-font-scale");
  assert.equal(scale.getAttribute("role"), "combobox");
  assert.equal(scale.getAttribute("type"), "text", "you can type into it");
  const list = h.document.querySelector("#doc-font-dialog .dialog-combo-list");
  assert.deepEqual([...list.querySelectorAll("li")].map((li) => li.textContent), ["200%", "150%", "100%", "90%", "80%", "66%", "50%", "33%"]);
  const arrow = h.document.querySelector("#doc-font-dialog .dialog-combo-arrow");
  arrow.dispatchEvent({ type: "mousedown", target: arrow, bubbles: true, preventDefault() {} });
  assert.equal(list.hidden, false, "the arrow opens the list");
  h.userClick(list.querySelectorAll("li")[1]);
  assert.equal(scale.value, "150%", "a row fills the field");
  assert.equal(list.hidden, true);
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.deepEqual(withKey(h, "scalePercent"), [150]);
});

test("the arrow keys step through the 배율 presets and Enter picks one", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  const scale = byId(h, "doc-font-scale");
  const press = (key) => scale.dispatchEvent({ type: "keydown", key, bubbles: true, preventDefault() {}, stopPropagation() {} });
  press("ArrowDown");
  const list = h.document.querySelector("#doc-font-dialog .dialog-combo-list");
  assert.equal(list.hidden, false);
  press("ArrowDown");
  press("ArrowDown");
  press("Enter");
  assert.equal(scale.value, "80%", "the first press opens on 100 %, the next two step to 90 % and 80 %");
});

test("a bare number in a value field takes its unit when the field is left", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  const kind = byId(h, "doc-font-spacing-kind");
  kind.value = "expanded";
  fire(kind);
  const value = byId(h, "doc-font-spacing-value");
  value.value = "2";
  fire(value);
  assert.equal(value.value, "2pt");
  value.value = "1,5";
  fire(value);
  assert.equal(value.value, "1.5pt");
  value.value = "abc";
  fire(value);
  assert.equal(value.value, "abc", "a text that is no number stays as typed");
  value.value = "2pt";
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.deepEqual(withKey(h, "letterSpacingTwips"), [40], "the unit is accepted when applying");
});

test("Alt+letter moves the focus to the labelled control of the visible tab", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  keydown(h, { altKey: true, code: "KeyC", key: "ç" });
  assert.equal(h.document.activeElement, byId(h, "doc-font-scale"), "배율(C), by the physical key (Option+C types a symbol on a Mac)");
  let clicks = 0;
  byId(h, "doc-font-kerning").addEventListener("click", () => { clicks += 1; });
  keydown(h, { altKey: true, code: "KeyK", key: "k" });
  assert.equal(clicks, 1, "a check box is clicked, so it flips");
  keydown(h, { altKey: true, code: "KeyO", key: "o" });
  assert.equal(h.document.activeElement, byId(h, "doc-font-kerning-value"), "포인트 이상(O)");
});

test("the preview has a framed group and the line under it", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  assert.match(byId(h, "doc-font-sample-note").textContent, /프린터/);
  assert.ok(byId(h, "doc-font-sample").closest("fieldset"), "the sample sits in the 미리 보기 group");
});
