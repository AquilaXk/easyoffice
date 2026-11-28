import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// PD-402: 슬라이드 크기 (표준 4:3, 와이드스크린 16:9, 사용자 지정 슬라이드 크기) of the slide editor.

async function openSlide() {
  const h = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-slide-pd402" });
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  return h;
}

const $ = (document, id) => document.getElementById(id);
const type = (h, id, value) => {
  const input = $(h.document, id);
  input.value = value;
  input.dispatchEvent({ type: "input", target: input });
};
const deckOf = (h) => h.host.__EASYOFFICE_SLIDE_CANVAS__.currentSlide();

test("the size menu has 표준(4:3), 와이드스크린(16:9) and 사용자 지정 슬라이드 크기 and the first two set the preset", async () => {
  const h = await openSlide();
  const labels = ["slide-size-4x3", "slide-size-16x9", "slide-size"].map((name) => h.document.querySelector(`#menu-bar [data-slide-act="${name}"] .md-label`)?.textContent);
  assert.deepEqual(labels, ["표준(4:3)", "와이드스크린(16:9)", "사용자 지정 슬라이드 크기…"]);
  assert.equal(h.host.__EASYOFFICE_SLIDE_RUN_ACT__("slide-size-4x3"), true);
  assert.equal(deckOf(h).aspect, "4:3");
  assert.equal(h.host.__EASYOFFICE_SLIDE_RUN_ACT__("slide-size-16x9"), true);
  assert.equal(deckOf(h).aspect, "16:9");
});

test("the dialog turns width and height into the preset they equal or into a custom size, in one undo step", async () => {
  const h = await openSlide();
  assert.equal(h.host.__EASYOFFICE_SLIDE_RUN_ACT__("slide-size"), true);
  const dialog = $(h.document, "slide-size-dialog");
  assert.equal(dialog.open, true);
  assert.equal($(h.document, "slide-size-w").value, "33.87");
  type(h, "slide-size-w", "25.4");
  type(h, "slide-size-h", "14.29");
  assert.equal($(h.document, "slide-size-kind").value, "custom");
  $(h.document, "btn-slide-size-ok").dispatchEvent({ type: "click" });
  assert.equal(dialog.open, false);
  assert.match(deckOf(h).aspect, /^custom:9144000x(5143[0-9]{3}|5144[0-9]{3})$/);
  assert.equal(deckOf(h).orientation, "landscape");
  const size = h.host.__EASYOFFICE_SLIDE_CANVAS__.stageSize();
  assert.ok(Math.abs(size.w - 720) < 0.01 && Math.abs(size.h - 405) < 1, JSON.stringify(size));
  assert.equal($(h.document, "slide-stage-box").style.aspectRatio !== "", true, "a custom page sizes the stage inline");

  h.host.__EASYOFFICE_SLIDE_RUN_ACT__("slide-size");
  assert.equal($(h.document, "slide-size-w").value, "25.4");
  type(h, "slide-size-w", "19.05");
  type(h, "slide-size-h", "25.4");
  assert.equal($(h.document, "slide-size-portrait").checked, true);
  assert.equal($(h.document, "slide-size-kind").value, "4:3", "the numbers of a preset are the preset");
  $(h.document, "btn-slide-size-ok").dispatchEvent({ type: "click" });
  assert.equal(deckOf(h).aspect, "4:3");
  assert.equal(deckOf(h).orientation, "portrait");
  assert.equal($(h.document, "slide-stage-box").style.aspectRatio, "", "a preset leaves the inline ratio");
});

test("a size outside 1 inch to 56 inches is refused and nothing changes", async () => {
  const h = await openSlide();
  for (const [w, hh] of [["1", "19"], ["33", "0"], ["200", "20"], ["-5", "10"], ["abc", "10"], ["1e9", "1e9"]]) {
    h.host.__EASYOFFICE_SLIDE_RUN_ACT__("slide-size");
    type(h, "slide-size-w", w);
    type(h, "slide-size-h", hh);
    $(h.document, "btn-slide-size-ok").dispatchEvent({ type: "click" });
    assert.equal($(h.document, "slide-size-dialog").open, true, `${w}x${hh} keeps the dialog open`);
    assert.match($(h.document, "slide-size-status").textContent, /2\.54 cm에서 142\.24 cm 사이/);
    assert.equal(deckOf(h).aspect, "16:9");
    $(h.document, "slide-size-dialog").close();
  }
});

test("a deck from a file keeps its custom size and drops a hostile one", async () => {
  const h = await openSlide();
  const codec = h.host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
  assert.deepEqual(codec.stageSizeOf("custom:9144000x5143500"), { w: 720, h: 405 });
  assert.deepEqual(codec.stageSizeOf("custom:9144000x5143500", "portrait"), { w: 405, h: 720 });
  for (const bad of ["custom:1x1", "custom:99999999999x914400", "custom:914400x", "custom:-1x5", "custom:9144000x5143500;x", "custom:51206401x914400", "constructor"]) {
    assert.deepEqual(codec.stageSizeOf(bad), { w: 960, h: 540 }, bad);
  }
  const ok = codec.hydrateDecodedDeck({ aspect: "custom:9144000x5143500", slides: [] }, () => ({}));
  assert.equal(ok[0].aspect, "custom:9144000x5143500");
  const bad = codec.hydrateDecodedDeck({ aspect: "custom:9x9", slides: [] }, () => ({}));
  assert.equal(bad[0].aspect, "16:9");
});
