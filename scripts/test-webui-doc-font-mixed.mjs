import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// PD-374: the 글꼴 dialog on a mixed selection changes only what the user touched. The fonts of two runs stay apart (changing the
// Korean font keeps each run's Latin font), and the packed OpenType and effect values are patched bit by bit (`{ mask, value }`).

const byId = (h, id) => h.document.getElementById(id);
const fire = (el, type = "change") => el.dispatchEvent({ type, target: el, bubbles: true });
const savedRuns = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor).flatMap((b) => b.runs || []);
const apply = (h) => h.userClick(byId(h, "btn-doc-font-apply"));
const FX = (h) => h.host.__EASYOFFICE_DOC_TEXT_FX__;
const OT = (h) => h.host.__EASYOFFICE_DOC_OPEN_TYPE__;

// Two runs "가나" and "다라" with their own formats, and a selection over both.
async function twoRuns(formatA, formatB) {
  const h = await createRunHarness([p("가나다라")]);
  const text = () => h.blocks[0].childNodes[0];
  h.runs.applyFormat(h.editor, { startContainer: text(), startOffset: 0, endContainer: text(), endOffset: 2 }, formatA);
  const rest = [...h.blocks[0].childNodes].find((n) => n.nodeType === 3);
  h.runs.applyFormat(h.editor, { startContainer: rest, startOffset: 0, endContainer: rest, endOffset: rest.textContent.length }, formatB);
  const spans = [...h.blocks[0].querySelectorAll(".eo-run")];
  const first = spans[0].childNodes[0];
  const last = spans[spans.length - 1].childNodes[0];
  h.selection.addRange(rangeOf(first, 0, last, last.textContent.length));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]'));
  await flushAsyncTurns(4);
  return h;
}

function choose(h, id, value) {
  const select = byId(h, id);
  if (![...select.querySelectorAll("option")].some((o) => o.value === value)) {
    const option = h.document.createElement("option");
    option.value = value;
    option.textContent = value;
    select.appendChild(option);
  }
  select.value = value;
  fire(select);
}

const fontsOf = (h) => savedRuns(h).filter((r) => r.font || r.fontEastAsia).map((r) => [r.text, r.font, r.fontEastAsia]);

test("changing only the Korean font keeps each run's Latin font", async () => {
  const h = await twoRuns({ font: "Calibri", eastfont: "맑은 고딕" }, { font: "Arial", eastfont: "바탕" });
  assert.equal(byId(h, "doc-font-family").value, "", "the Korean fonts differ, so the box is blank");
  choose(h, "doc-font-family", "함초롬바탕");
  apply(h);
  assert.deepEqual(fontsOf(h), [["가나", "Calibri", "함초롬바탕"], ["다라", "Arial", "함초롬바탕"]]);
});

test("changing only the English font keeps each run's Korean font, even for a run with one font", async () => {
  const h = await twoRuns({ font: "Calibri", eastfont: "맑은 고딕" }, { font: "Arial" });
  choose(h, "doc-font-latin", "Verdana");
  apply(h);
  assert.deepEqual(fontsOf(h), [["가나", "Verdana", "맑은 고딕"], ["다라", "Verdana", "Arial"]], "the run with one font keeps it for its Korean text");
});

test("a font named alone, as the style bar does, is still for both", async () => {
  const h = await twoRuns({ font: "Calibri", eastfont: "맑은 고딕" }, { font: "Arial", eastfont: "바탕" });
  const node = h.editor.querySelector(".eo-run").childNodes[0];
  h.runs.applyFormat(h.editor, { startContainer: node, startOffset: 0, endContainer: node, endOffset: 2 }, { font: "Verdana" });
  assert.deepEqual(fontsOf(h).slice(0, 1), [["가나", "Verdana", undefined]]);
});

test("turning on one effect keeps the other bits of every run", async () => {
  const h = await twoRuns({ textfx: FX(await createRunHarness([p("가")])).pack({ smallCaps: true }) }, { textfx: 8 });
  const box = byId(h, "doc-font-dstrike");
  box.checked = true;
  fire(box);
  apply(h);
  const bits = savedRuns(h).filter((r) => r.textFx).map((r) => [r.text, FX(h).parts(r.textFx)]);
  assert.deepEqual(bits.map(([t, b]) => [t, b.doubleStrike, b.smallCaps, b.hidden]), [["가나", true, true, false], ["다라", true, false, true]]);
});

test("turning an effect off clears only that bit, and an untouched mixed emphasis stays", async () => {
  const fx = FX(await createRunHarness([p("가")]));
  const h = await twoRuns({ textfx: fx.pack({ hidden: true, emphasis: 1 }) }, { textfx: fx.pack({ hidden: true, emphasis: 3 }) });
  const box = byId(h, "doc-font-hidden");
  box.checked = false;
  fire(box);
  apply(h);
  assert.deepEqual(savedRuns(h).filter((r) => r.textFx).map((r) => [r.text, fx.parts(r.textFx).hidden, fx.parts(r.textFx).emphasis]), [["가나", false, 1], ["다라", false, 3]]);
});

test("one OpenType choice keeps the other features of each run", async () => {
  const ot = OT(await createRunHarness([p("가")]));
  const h = await twoRuns({ opentype: ot.pack({ ligatures: 1 }) }, { opentype: ot.pack({ form: 1, sets: 0b101 }) });
  const box = byId(h, "doc-font-alternates");
  box.checked = true;
  fire(box);
  apply(h);
  const got = savedRuns(h).filter((r) => r.openType).map((r) => [r.text, ot.unpack(r.openType)]);
  assert.deepEqual(got.map(([t, u]) => [t, u.ligatures, u.form, u.sets, u.alternates]), [["가나", 1, 0, 0, true], ["다라", 0, 1, 0b101, true]]);
});

test("the bit patch refuses a mask or value out of range and an untouched dialog sends none", async () => {
  const h = await createRunHarness([p("가나")]);
  for (const bad of [{ mask: 0, value: 1 }, { mask: -1, value: 1 }, { mask: 2 ** 12, value: 1 }, { mask: 1, value: 2 ** 20 }, { mask: 1.5, value: 1 }, { mask: "x", value: 1 }]) {
    assert.equal(h.runs.normalizePatch({ textfx: bad }), null, JSON.stringify(bad));
  }
  assert.deepEqual(h.runs.normalizePatch({ textfx: { mask: 3, value: 7 } }), { textfx: { mask: 3, value: 3 } }, "value is cut to the mask");
});
