import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// PD-374: the 글꼴 tab's 이중 취소선, 작은 대문자, 모두 대문자, 숨김, 밑줄 스타일·색 and 강조점 are one packed run value (`textFx`)
// and the underline colour (`underlineColor`) in the save, and the editor draws them with CSS.

const byId = (h, id) => h.document.getElementById(id);
const fire = (el, type = "change") => el.dispatchEvent({ type, target: el, bubbles: true });
const savedRuns = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor).flatMap((b) => b.runs || []);
const apply = (h) => h.userClick(byId(h, "btn-doc-font-apply"));
const FX = (h) => h.host.__EASYOFFICE_DOC_TEXT_FX__;

async function openDialog(h, { whole = false } = {}) {
  const node = h.blocks[0].childNodes[0].nodeType === 3 ? h.blocks[0].childNodes[0] : h.blocks[0].childNodes[0].childNodes[0];
  h.selection.addRange(rangeOf(node, 0, node, whole ? node.textContent.length : 1));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]'));
  await flushAsyncTurns(4);
}

function check(h, id, on = true) {
  const el = byId(h, id);
  el.checked = on;
  fire(el);
}

test("the effect check boxes write one packed value and the editor draws it", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  check(h, "doc-font-smallcaps");
  check(h, "doc-font-hidden");
  const emphasis = byId(h, "doc-font-emphasis");
  emphasis.value = "3";
  fire(emphasis);
  apply(h);
  const packed = FX(h).pack({ smallCaps: true, hidden: true, emphasis: 3 });
  assert.deepEqual(savedRuns(h).filter((r) => r.textFx).map((r) => r.textFx), [packed]);
  const style = h.editor.querySelector(".eo-run").style;
  assert.equal(style.fontVariantCaps, "small-caps");
  assert.equal(style.opacity, "0.55");
  assert.equal(style.textEmphasisStyle, "open circle");
  await openDialog(h, { whole: true });
  assert.deepEqual([byId(h, "doc-font-smallcaps").checked, byId(h, "doc-font-hidden").checked, byId(h, "doc-font-emphasis").value], [true, true, "3"]);
  check(h, "doc-font-smallcaps", false);
  check(h, "doc-font-hidden", false);
  emphasis.value = "0";
  fire(emphasis);
  apply(h);
  assert.deepEqual(savedRuns(h).filter((r) => r.textFx), [], "every effect off takes the value off");
});

test("취소선 and 이중 취소선, 작은 대문자 and 모두 대문자 exclude each other", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  check(h, "doc-font-dstrike");
  check(h, "doc-font-strike");
  assert.equal(byId(h, "doc-font-dstrike").checked, false, "checking 취소선 unchecks 이중 취소선");
  check(h, "doc-font-dstrike");
  assert.equal(byId(h, "doc-font-strike").checked, false, "checking 이중 취소선 unchecks 취소선");
  check(h, "doc-font-smallcaps");
  check(h, "doc-font-allcaps");
  assert.equal(byId(h, "doc-font-smallcaps").checked, false);
  apply(h);
  const run = savedRuns(h).find((r) => r.textFx);
  assert.deepEqual(FX(h).parts(run.textFx), { doubleStrike: true, smallCaps: false, allCaps: true, hidden: false, emphasis: 0, underline: 0 });
  assert.equal(run.strike, undefined);
});

test("the underline style and color are the run's and the editor draws them", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  const style = byId(h, "doc-font-underline-style");
  style.value = "14";
  fire(style);
  h.userClick(h.document.querySelector("#doc-font-underline-color-host [data-color-toggle]"));
  h.userClick(h.document.querySelector('#doc-font-underline-color-host .sb-hl-swatch[aria-label="#0000ff"]'));
  apply(h);
  const run = savedRuns(h).find((r) => r.underline);
  assert.equal(run.textFx, FX(h).pack({ underline: 14 }));
  assert.equal(run.underlineColor, "0000FF");
  const css = h.editor.querySelector(".eo-run").style;
  assert.equal(css.textDecorationStyle, "wavy");
  assert.equal(css.textDecorationColor, "#0000FF");
  await openDialog(h, { whole: true });
  assert.equal(byId(h, "doc-font-underline-style").value, "14");
  assert.equal(h.document.querySelector("#doc-font-underline-color-host .dialog-swatch-text").textContent, "#0000FF");
  style.value = "none";
  fire(style);
  apply(h);
  assert.equal(savedRuns(h).filter((r) => r.underline).length, 0, "(없음) takes the underline off");
});

test("a part the user did not touch keeps what the file had", async () => {
  const h = await createRunHarness([p("가나다")]);
  const text = h.blocks[0].childNodes[0];
  h.runs.applyFormat(h.editor, { startContainer: text, startOffset: 0, endContainer: text, endOffset: 3 }, { textfx: FX(h).pack({ doubleStrike: true, emphasis: 2, underline: 3 }), underline: true });
  await openDialog(h, { whole: true });
  check(h, "doc-font-hidden");
  apply(h);
  assert.deepEqual(savedRuns(h).filter((r) => r.textFx).map((r) => FX(h).parts(r.textFx)), [{ doubleStrike: true, smallCaps: false, allCaps: false, hidden: true, emphasis: 2, underline: 3 }]);
});

test("a decoded run opens with its effects, and values the engine would refuse are dropped", async () => {
  const h = await createRunHarness([p("가")]);
  const good = FX(h).pack({ allCaps: true, underline: 16 });
  assert.deepEqual(h.runs.fromModelRun({ textFx: good, underlineColor: "ff0000" }), { textfx: good, ucolor: "FF0000" });
  for (const bad of [-1, 4096, 1.5, "x", Number.NaN, Infinity, {}, 5 << 4, 17 << 7, 2 ** 32 + 1]) {
    assert.deepEqual(h.runs.fromModelRun({ textFx: bad }), {}, String(bad));
  }
  assert.deepEqual(h.runs.fromModelRun({ underlineColor: "red" }), {});
  assert.deepEqual(h.runs.fromModelRun({ textFx: 0 }), {});
});

test("the packed value round-trips and the CSS of an unset or invalid value is empty", async () => {
  const h = await createRunHarness([p("가")]);
  const fx = FX(h);
  const all = { doubleStrike: true, smallCaps: true, allCaps: true, hidden: true, emphasis: 4, underline: 16 };
  assert.deepEqual(fx.parts(fx.pack(all)), all);
  assert.equal(fx.valid(fx.pack(all)), true);
  assert.equal(fx.valid(fx.MAX_PACKED), false, "all bits set has emphasis 7");
  const empty = fx.css({});
  assert.ok(Object.values(empty).every((v) => v === ""));
  assert.ok(Object.values(fx.css({ textfx: 99999 })).every((v) => v === ""));
  assert.equal(fx.css({ underline: true, textfx: fx.pack({ underline: 5 }) }).textDecorationThickness, "2px", "a heavy line is thicker");
  assert.equal(fx.css({ strike: false, textfx: fx.pack({ doubleStrike: true }) }).textDecorationStyle, "double");
});
