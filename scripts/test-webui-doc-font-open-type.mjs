import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// PD-375: the 글꼴 dialog's 고급 tab has the OpenType 기능 group (합자, 숫자 간격, 숫자 형식, 스타일 세트, 상황별 대체 사용) and the
// 문서 눈금이 지정되어 있으면 줄 눈금에 맞춤 box. They are one packed run value (`openType`) and the `snapToGrid` switch in the save.

const byId = (h, id) => h.document.getElementById(id);
const fire = (el, type = "change") => el.dispatchEvent({ type, target: el, bubbles: true });
const savedRuns = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor).flatMap((b) => b.runs || []);
const apply = (h) => h.userClick(byId(h, "btn-doc-font-apply"));
const OT = (h) => h.host.__EASYOFFICE_DOC_OPEN_TYPE__;

async function openDialog(h, { whole = false } = {}) {
  const node = h.blocks[0].childNodes[0].nodeType === 3 ? h.blocks[0].childNodes[0] : h.blocks[0].childNodes[0].childNodes[0];
  h.selection.addRange(rangeOf(node, 0, node, whole ? node.textContent.length : 1));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]'));
  await flushAsyncTurns(4);
  h.userClick(byId(h, "doc-font-tab-advanced"));
}

function choose(h, id, value) {
  const el = byId(h, id);
  el.value = value;
  fire(el);
}

test("the OpenType group lists the official choices", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  const texts = (id) => [...byId(h, id).querySelectorAll("option")].map((o) => o.textContent);
  assert.deepEqual(texts("doc-font-ligatures"), ["없음", "표준만", "표준 및 상황에 맞는", "기본 및 사용자 지정", "모두"]);
  assert.deepEqual(texts("doc-font-num-spacing"), ["기본", "비례", "표 형식"]);
  assert.deepEqual(texts("doc-font-num-form"), ["기본", "기준선 맞춤", "기존 스타일 맞춤"]);
  assert.deepEqual(texts("doc-font-style-set"), ["기본", ...Array.from({ length: 20 }, (_, i) => String(i + 1))]);
  const labels = [...h.document.querySelectorAll("#doc-font-pane-advanced label")].map((l) => l.textContent.trim());
  for (const label of ["합자(L):", "숫자 간격(M):", "숫자 양식(F):", "스타일 모음(T):", "상황별 대체 사용(A)", "문서 눈금이 지정되어 있으면 줄 눈금에 맞춤(W)"]) {
    assert.ok(labels.includes(label), `${label} in ${labels}`);
  }
  assert.equal(byId(h, "doc-font-grid").checked, true, "a run snaps to the grid unless it says otherwise");
});

test("the choices are one packed run value that the editor draws with CSS and the save carries", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  choose(h, "doc-font-ligatures", "3");
  choose(h, "doc-font-num-spacing", "2");
  choose(h, "doc-font-num-form", "1");
  choose(h, "doc-font-style-set", "5");
  byId(h, "doc-font-alternates").checked = true;
  fire(byId(h, "doc-font-alternates"));
  apply(h);
  const packed = OT(h).pack({ ligatures: 3, spacing: 2, form: 1, set: 5, alternates: true });
  assert.deepEqual(savedRuns(h).filter((r) => r.openType).map((r) => r.openType), [packed]);
  const style = h.editor.querySelector(".eo-run").style;
  assert.equal(style.fontVariantLigatures, "common-ligatures contextual");
  assert.equal(style.fontVariantNumeric, "tabular-nums lining-nums");
  assert.equal(style.fontFeatureSettings, '"clig" 1, "ss05" 1');

  await openDialog(h);
  assert.deepEqual(["doc-font-ligatures", "doc-font-num-spacing", "doc-font-num-form", "doc-font-style-set"].map((id) => byId(h, id).value), ["3", "2", "1", "5"]);
  assert.equal(byId(h, "doc-font-alternates").checked, true);
  choose(h, "doc-font-ligatures", "0");
  choose(h, "doc-font-num-spacing", "0");
  choose(h, "doc-font-num-form", "0");
  choose(h, "doc-font-style-set", "0");
  byId(h, "doc-font-alternates").checked = false;
  fire(byId(h, "doc-font-alternates"));
  apply(h);
  assert.deepEqual(savedRuns(h).filter((r) => r.openType), [], "every choice back to its default takes the value off");
});

test("a choice the user did not touch keeps what the file had, even when the lists cannot show it", async () => {
  const h = await createRunHarness([p("가나다")]);
  const text = h.blocks[0].childNodes[0];
  const several = OT(h).pack({ ligatures: 5, sets: 0b101 });
  h.runs.applyFormat(h.editor, { startContainer: text, startOffset: 0, endContainer: text, endOffset: 3 }, { opentype: several });
  await openDialog(h, { whole: true });
  choose(h, "doc-font-num-form", "2");
  apply(h);
  assert.deepEqual(savedRuns(h).filter((r) => r.openType).map((r) => r.openType), [OT(h).pack({ ligatures: 5, form: 2, sets: 0b101 })]);
});

test("the line-grid box writes snapToGrid false and checking it again takes it off", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  const grid = byId(h, "doc-font-grid");
  grid.checked = false;
  fire(grid);
  apply(h);
  assert.deepEqual(savedRuns(h).filter((r) => r.snapToGrid === false).length, 1);
  assert.equal(h.editor.querySelector(".eo-run").dataset.eoNogrid, "1");
  await openDialog(h);
  assert.equal(byId(h, "doc-font-grid").checked, false, "the dialog shows the run's switch");
  byId(h, "doc-font-grid").checked = true;
  fire(byId(h, "doc-font-grid"));
  apply(h);
  assert.equal(savedRuns(h).filter((r) => r.snapToGrid !== undefined).length, 0);
});

test("a decoded run opens with its features, and values the engine would refuse are dropped", async () => {
  const h = await createRunHarness([p("가")]);
  const good = OT(h).pack({ ligatures: 15, spacing: 1 });
  assert.deepEqual(h.runs.fromModelRun({ openType: good, snapToGrid: false }), { opentype: good, nogrid: 1 });
  for (const bad of [-1, 2 ** 29, 1.5, "x", "8", Number.NaN, Infinity, {}, { mask: 3, value: 3 }, [8], 3 << 4, 3 << 6, 2 ** 32 + 1]) {
    assert.deepEqual(h.runs.fromModelRun({ openType: bad }), {}, String(bad));
  }
  assert.deepEqual(h.runs.fromModelRun({ snapToGrid: true }), {});
  assert.deepEqual(h.runs.fromModelRun({ openType: 0 }), {});
});

test("the packed value round-trips and the CSS of an unset or invalid value is empty", async () => {
  const h = await createRunHarness([p("가")]);
  const ot = OT(h);
  const all = { ligatures: 15, spacing: 2, form: 2, alternates: true, sets: 0xfffff };
  assert.equal(ot.valid(ot.pack(all)), true, "the largest value the engine accepts");
  assert.equal(ot.valid(ot.MAX_PACKED), false, "all bits set has number spacing 3");
  assert.deepEqual(ot.unpack(ot.pack(all)), { ligatures: 15, spacing: 2, form: 2, alternates: true, set: 1, sets: 0xfffff, custom: true });
  assert.equal(ot.unpack(ot.pack({ ligatures: 1, set: 20 })).set, 20);
  assert.equal(ot.unpack(ot.pack({ ligatures: 5 })).custom, true, "a mix the list lacks is custom");
  for (const none of [0, -1, 2 ** 29, 3 << 4, Number.NaN]) {
    assert.deepEqual(ot.css(none), { fontVariantLigatures: "", fontVariantNumeric: "", fontFeatureSettings: "" }, String(none));
  }
});
