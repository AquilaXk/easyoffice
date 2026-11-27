import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// The word processor's 글꼴 dialog (서식 > 글꼴…): 고급 tab 문자 간격 (간격 표준·넓게·좁게, 값 in pt) writes the run's
// spacing in twips, and 확인 applies only what the user changed.

const saved = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
const byId = (h, id) => h.document.getElementById(id);
const fire = (el, type = "change") => el.dispatchEvent({ type, target: el, bubbles: true });
const spacingRuns = (h) => saved(h).flatMap((b) => b.runs || []).filter((r) => r.letterSpacingTwips);

async function openDialog(h, block, { whole = false } = {}) {
  // The first character, whether the block still holds one text node or has been split into runs.
  const first = block.childNodes[0];
  const node = first.nodeType === 3 ? first : first.childNodes[0];
  h.selection.addRange(rangeOf(node, 0, node, whole ? node.textContent.length : 1));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]'));
  await flushAsyncTurns(4);
}

test("서식 > 글꼴… opens the dialog with the 글꼴 and 고급 tabs", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, h.blocks[0]);
  assert.equal(byId(h, "doc-font-dialog").open, true);
  assert.deepEqual([...h.document.querySelectorAll("#doc-font-dialog [data-font-tab]")].map((t) => t.textContent), ["글꼴", "고급"]);
  assert.equal(byId(h, "doc-font-pane-advanced").hidden, true);
  h.userClick(byId(h, "doc-font-tab-advanced"));
  assert.equal(byId(h, "doc-font-pane-advanced").hidden, false);
  assert.equal(byId(h, "doc-font-pane-font").hidden, true);
  assert.deepEqual([...byId(h, "doc-font-spacing-kind").querySelectorAll("option")].map((o) => o.textContent), ["표준", "넓게", "좁게"]);
  assert.equal(byId(h, "doc-font-spacing-value").disabled, true, "표준 takes no value");
});

test("고급 > 문자 간격: 넓게 and 좁게 write the spacing in twips, 표준 takes it off", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  const kind = byId(h, "doc-font-spacing-kind");
  const value = byId(h, "doc-font-spacing-value");

  await openDialog(h, a);
  kind.value = "expanded";
  fire(kind);
  assert.equal(value.disabled, false);
  assert.equal(value.value, "1pt", "넓게 starts at 1 pt");
  value.value = "3";
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.equal(byId(h, "doc-font-dialog").open, false);
  assert.deepEqual(spacingRuns(h).map((r) => r.letterSpacingTwips), [60], "3 pt is 60 twips");

  await openDialog(h, a);
  assert.deepEqual([kind.value, value.value], ["expanded", "3pt"], "the dialog shows the selection's spacing with its unit");
  kind.value = "condensed";
  fire(kind);
  value.value = "0,5";
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.deepEqual(spacingRuns(h).map((r) => r.letterSpacingTwips), [-10], "좁게 0.5 pt is -10 twips");

  await openDialog(h, a);
  kind.value = "normal";
  fire(kind);
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.deepEqual(spacingRuns(h), [], "표준 takes the spacing off");
});

test("a spacing value out of range keeps the dialog open on the 고급 tab with the reason", async () => {
  const h = await createRunHarness([p("가나다")]);
  const kind = byId(h, "doc-font-spacing-kind");
  const value = byId(h, "doc-font-spacing-value");
  await openDialog(h, h.blocks[0]);
  kind.value = "expanded";
  fire(kind);
  for (const bad of ["1585", "0", "-3", "", "abc"]) {
    value.value = bad;
    h.userClick(byId(h, "btn-doc-font-apply"));
    assert.equal(byId(h, "doc-font-dialog").open, true, `"${bad}" is refused`);
    assert.match(byId(h, "doc-font-status").textContent, /범위/);
    assert.equal(byId(h, "doc-font-pane-advanced").hidden, false);
  }
  assert.deepEqual(spacingRuns(h), []);
  value.value = "1584";
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.deepEqual(spacingRuns(h).map((r) => r.letterSpacingTwips), [31680], "1584 pt is the largest value");
});

test("확인 applies only what changed on the 글꼴 tab", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  await openDialog(h, a);
  const style = byId(h, "doc-font-style");
  assert.deepEqual([...byId(h, "doc-font-style-list").querySelectorAll(".goto-bookmark-item")].map((o) => o.textContent), ["보통", "기울임꼴", "굵게", "굵게 기울임꼴"], "the 글꼴 스타일 list box");
  assert.equal(style.value, "보통");
  style.value = "굵게 기울임꼴";
  fire(style);
  byId(h, "doc-font-strike").checked = true;
  fire(byId(h, "doc-font-strike"));
  h.userClick(byId(h, "btn-doc-font-apply"));
  const run = saved(h).flatMap((b) => b.runs || []).find((r) => r.bold);
  assert.ok(run, "the run is bold");
  assert.equal(run.italic, true);
  assert.equal(run.strike, true);
  assert.equal(run.letterSpacingTwips, undefined, "the spacing was not touched");
});

test("changing the font of a run that has a variant drops the variant (PD-368)", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  const text = a.childNodes[0];
  h.runs.applyFormat(h.editor, { startContainer: text, startOffset: 0, endContainer: text, endOffset: 3 }, { font: "Helvetica Neue Light", face: "HelveticaNeue-Light" });
  const run = () => h.editor.querySelector(".eo-run");
  assert.equal(run().dataset.eoFace, "HelveticaNeue-Light");
  await openDialog(h, a, { whole: true });
  const family = byId(h, "doc-font-family");
  const option = h.document.createElement("option");
  option.value = "Arial";
  option.textContent = "Arial";
  family.appendChild(option);
  family.value = "Arial";
  fire(family);
  const latin = byId(h, "doc-font-latin");
  latin.appendChild(option.cloneNode(true));
  latin.value = "Arial";
  fire(latin);
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.equal(run().dataset.eoFont, "Arial");
  assert.equal(run().dataset.eoFace, undefined, "the old face is gone");
  assert.equal(run().style.fontFamily, '"Arial"');
  const saved1 = saved(h).flatMap((b) => b.runs || [])[0];
  assert.deepEqual([saved1.font, saved1.fontFace], ["Arial", undefined]);
});
