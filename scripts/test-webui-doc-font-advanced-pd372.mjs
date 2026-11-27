import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// 글꼴 dialog 고급 tab (PD-372): 배율, 위치 (표준·올림·내림) and 글꼴 커닝 (포인트 이상) write the run's scale (percent),
// position and kerning (half-points); 확인 applies only what changed and refuses values out of range.

const saved = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
const byId = (h, id) => h.document.getElementById(id);
const fire = (el, type = "change") => el.dispatchEvent({ type, target: el, bubbles: true });
const savedRuns = (h) => saved(h).flatMap((b) => b.runs || []);
const withKey = (h, key) => savedRuns(h).filter((r) => r[key] !== undefined).map((r) => r[key]);
const apply = (h) => h.userClick(byId(h, "btn-doc-font-apply"));

async function openDialog(h, block) {
  const first = block.childNodes[0];
  const node = first.nodeType === 3 ? first : first.childNodes[0];
  h.selection.addRange(rangeOf(node, 0, node, 1));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]'));
  await flushAsyncTurns(4);
  h.userClick(byId(h, "doc-font-tab-advanced"));
}

function setBox(h, id, value, type = "input") {
  const el = byId(h, id);
  el.value = value;
  fire(el, type);
}

test("고급 tab lists 배율, 위치 (표준·올림·내림) and 글꼴 커닝 under 문자 간격", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, h.blocks[0]);
  assert.equal(byId(h, "doc-font-scale").value, "100%", "text without a scale shows 100 % with the unit in the field");
  assert.deepEqual([...byId(h, "doc-font-position-kind").querySelectorAll("option")].map((o) => o.textContent), ["표준", "올림", "내림"]);
  assert.equal(byId(h, "doc-font-position-value").disabled, true, "표준 takes no value");
  assert.equal(byId(h, "doc-font-kerning").checked, false);
  assert.equal(byId(h, "doc-font-kerning-value").disabled, true, "kerning off takes no size");
  const labels = [...h.document.querySelectorAll("#doc-font-pane-advanced label")].map((l) => l.textContent.trim());
  for (const label of ["배율(C):", "간격(S):", "값(B):", "위치(P):", "값(Y):", "글꼴 커닝(K):", "포인트 이상(O)"]) assert.ok(labels.includes(label), `${label} in ${labels}`);
  assert.deepEqual([...h.document.querySelectorAll("#doc-font-dialog legend")].map((l) => l.textContent), ["효과", "문자 간격", "OpenType 기능", "미리 보기"]);
});

test("배율 writes the scale in percent and 100 takes it off", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  await openDialog(h, a);
  setBox(h, "doc-font-scale", "150");
  apply(h);
  assert.equal(byId(h, "doc-font-dialog").open, false);
  assert.deepEqual(withKey(h, "scalePercent"), [150]);
  assert.equal(h.editor.querySelector(".eo-run").dataset.eoScale, "150");

  await openDialog(h, a);
  assert.equal(byId(h, "doc-font-scale").value, "150%", "the dialog shows the selection's scale");
  setBox(h, "doc-font-scale", "600");
  apply(h);
  assert.deepEqual(withKey(h, "scalePercent"), [600]);

  await openDialog(h, a);
  setBox(h, "doc-font-scale", "100");
  apply(h);
  assert.deepEqual(withKey(h, "scalePercent"), [], "100 % is no scale");
});

test("위치 writes the raise or lowering in half-points; 표준 takes it off", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  await openDialog(h, a);
  setBox(h, "doc-font-position-kind", "raised", "change");
  assert.equal(byId(h, "doc-font-position-value").disabled, false);
  assert.equal(byId(h, "doc-font-position-value").value, "3pt", "올림 starts at 3 pt");
  apply(h);
  assert.deepEqual(withKey(h, "positionHalfPt"), [6]);
  assert.equal(h.editor.querySelector(".eo-run").style.verticalAlign, "3pt", "the editor raises the run");

  await openDialog(h, a);
  assert.deepEqual([byId(h, "doc-font-position-kind").value, byId(h, "doc-font-position-value").value], ["raised", "3pt"]);
  setBox(h, "doc-font-position-kind", "lowered", "change");
  setBox(h, "doc-font-position-value", "0,5");
  apply(h);
  assert.deepEqual(withKey(h, "positionHalfPt"), [-1], "내림 0.5 pt is one half-point down");
  assert.equal(h.editor.querySelector(".eo-run").style.verticalAlign, "-0.5pt");

  await openDialog(h, a);
  setBox(h, "doc-font-position-kind", "normal", "change");
  apply(h);
  assert.deepEqual(withKey(h, "positionHalfPt"), []);
});

test("글꼴 커닝 writes the smallest kerned size in half-points", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  await openDialog(h, a);
  byId(h, "doc-font-kerning").checked = true;
  fire(byId(h, "doc-font-kerning"));
  assert.equal(byId(h, "doc-font-kerning-value").value, "1pt", "checking it starts at 1 pt");
  setBox(h, "doc-font-kerning-value", "12");
  apply(h);
  assert.deepEqual(withKey(h, "kerningHalfPt"), [24]);
  assert.equal(h.editor.querySelector(".eo-run").style.fontKerning, "normal");

  await openDialog(h, a);
  assert.equal(byId(h, "doc-font-kerning").checked, true);
  assert.equal(byId(h, "doc-font-kerning-value").value, "12pt");
  byId(h, "doc-font-kerning").checked = false;
  fire(byId(h, "doc-font-kerning"));
  apply(h);
  assert.deepEqual(withKey(h, "kerningHalfPt"), [], "unchecking takes kerning off");
});

test("values out of range keep the dialog open on the 고급 tab with the reason", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, h.blocks[0]);
  const refuse = (what) => {
    apply(h);
    assert.equal(byId(h, "doc-font-dialog").open, true, `${what} is refused`);
    assert.match(byId(h, "doc-font-status").textContent, /범위/, what);
    assert.equal(byId(h, "doc-font-pane-advanced").hidden, false);
  };
  for (const bad of ["0", "601", "-5", "1.5", "abc", "", "1e3", "NaN"]) {
    setBox(h, "doc-font-scale", bad);
    refuse(`배율 "${bad}"`);
  }
  setBox(h, "doc-font-scale", "100");
  setBox(h, "doc-font-position-kind", "raised", "change");
  for (const bad of ["1585", "0", "0.4", "-3", "", "abc", "Infinity"]) {
    setBox(h, "doc-font-position-value", bad);
    refuse(`위치 "${bad}"`);
  }
  setBox(h, "doc-font-position-kind", "normal", "change");
  byId(h, "doc-font-kerning").checked = true;
  fire(byId(h, "doc-font-kerning"));
  for (const bad of ["1639", "0", "0.5", "-1", "", "abc", "1e999"]) {
    setBox(h, "doc-font-kerning-value", bad);
    refuse(`커닝 "${bad}"`);
  }
  assert.deepEqual(savedRuns(h).filter((r) => r.scalePercent || r.positionHalfPt || r.kerningHalfPt), []);
  setBox(h, "doc-font-kerning-value", "1638");
  setBox(h, "doc-font-scale", "1");
  setBox(h, "doc-font-position-kind", "lowered", "change");
  setBox(h, "doc-font-position-value", "1584");
  apply(h);
  const run = savedRuns(h).find((r) => r.kerningHalfPt);
  assert.deepEqual([run.scalePercent, run.positionHalfPt, run.kerningHalfPt], [1, -3168, 3276], "the largest values pass");
});

test("확인 without a change on the 고급 tab leaves the run's other settings alone", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  await openDialog(h, a);
  setBox(h, "doc-font-scale", "150");
  setBox(h, "doc-font-position-kind", "raised", "change");
  apply(h);
  await openDialog(h, a);
  byId(h, "doc-font-strike").checked = true;
  fire(byId(h, "doc-font-strike"));
  apply(h);
  const run = savedRuns(h).find((r) => r.strike);
  assert.equal(run.scalePercent, 150);
  assert.equal(run.positionHalfPt, 6);
  assert.equal(run.kerningHalfPt, undefined);
});

test("확인 applies to the range the dialog opened on when the modal collapses the selection (WebKit)", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  await openDialog(h, a);
  const text = a.childNodes[0].nodeType === 3 ? a.childNodes[0] : a.childNodes[0].childNodes[0];
  h.selection.removeAllRanges();
  h.selection.addRange(rangeOf(text, text.textContent.length, text, text.textContent.length));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(4);
  setBox(h, "doc-font-scale", "150");
  apply(h);
  assert.deepEqual(withKey(h, "scalePercent"), [150], "the character the dialog opened on is scaled");
});

test("확인 applies nothing when the range the dialog opened on left the editor (undo or open while it was up)", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  await openDialog(h, a);
  let mutations = 0;
  h.editor.addEventListener("eo-doc-mutated", () => { mutations += 1; });
  a.remove();
  setBox(h, "doc-font-scale", "150");
  apply(h);
  assert.equal(byId(h, "doc-font-dialog").open, false);
  assert.equal(mutations, 0, "the document is not marked changed");
  assert.equal(a.querySelector(".eo-run"), null, "the detached block is not formatted");
  assert.match(byId(h, "status-command-feedback").textContent, /먼저 선택/, "the user is told nothing was selected");
  assert.deepEqual(withKey(h, "scalePercent"), []);
});

test("a script keeps its own vertical position over a raise or lowering", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  const text = a.childNodes[0];
  const range = { startContainer: text, startOffset: 0, endContainer: text, endOffset: 3 };
  h.runs.applyFormat(h.editor, range, { superscript: true, position: 6, scale: 200 });
  const run = h.editor.querySelector(".eo-run");
  assert.equal(run.style.verticalAlign, "super");
  assert.equal(run.style.fontStretch, "200%", "the width scale is font-stretch on screen");
});

test("the run engine drops advanced values that are not integers inside the bounds", async () => {
  const h = await createRunHarness([p("가나다")]);
  const [a] = h.blocks;
  const text = a.childNodes[0];
  const range = { startContainer: text, startOffset: 0, endContainer: text, endOffset: 3 };
  for (const bad of [{ scale: 601 }, { scale: -3 }, { scale: 1.5 }, { scale: "NaN" }, { scale: Number.NaN }, { scale: Infinity }, { scale: {} },
    { position: 3169 }, { position: -3169 }, { position: 0.5 }, { position: 1e21 }, { position: [6] },
    { kerning: 3277 }, { kerning: -1 }, { kerning: 2.5 }, { kerning: true }]) {
    h.runs.applyFormat(h.editor, range, bad);
  }
  assert.deepEqual(savedRuns(h).filter((r) => r.scalePercent || r.positionHalfPt || r.kerningHalfPt), []);
  const fmt = h.runs.fromModelRun({ scalePercent: 99999, positionHalfPt: -99999, kerningHalfPt: 99999 });
  assert.deepEqual(fmt, {}, "a model run out of range keeps none");
  assert.deepEqual(h.runs.fromModelRun({ scalePercent: 150, positionHalfPt: -6, kerningHalfPt: 24 }), { scale: 150, position: -6, kerning: 24 });
});

test("a decoded run with the settings opens with them on its span", async () => {
  const h = await createRunHarness([p("가나다")]);
  const fmt = h.runs.fromModelRun({ text: "x", scalePercent: 150, positionHalfPt: 6, kerningHalfPt: 24 });
  const span = h.runs.createRun(h.document, fmt);
  assert.deepEqual([span.dataset.eoScale, span.dataset.eoPosition, span.dataset.eoKerning], ["150", "6", "24"]);
  assert.deepEqual(h.runs.readRunFormat(span), { scale: 150, position: 6, kerning: 24 });
  span.dataset.eoScale = "9999999999";
  span.dataset.eoPosition = "abc";
  span.dataset.eoKerning = "-4";
  assert.deepEqual(h.runs.readRunFormat(span), {}, "tampered data attributes read as none");
});
