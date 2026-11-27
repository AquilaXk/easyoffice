import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// PD-375: the 글꼴 dialog's 한글 글꼴(T) is the East Asian face and 영어 글꼴(F) the Latin one. A run keeps them as `font` (Latin)
// and `eastfont` (only when it differs), the save carries `font` and `fontEastAsia`, and the editor draws "Latin, East Asian".

const byId = (h, id) => h.document.getElementById(id);
const fire = (el, type = "change") => el.dispatchEvent({ type, target: el, bubbles: true });
const savedRuns = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor).flatMap((b) => b.runs || []);
const apply = (h) => h.userClick(byId(h, "btn-doc-font-apply"));

async function openDialog(h, { whole = false } = {}) {
  const first = h.blocks[0].childNodes[0];
  const node = first.nodeType === 3 ? first : first.childNodes[0];
  h.selection.addRange(rangeOf(node, 0, node, whole ? node.textContent.length : 1));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  h.userClick(h.document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]'));
  await flushAsyncTurns(4);
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

test("a pair of faces is two run values, drawn Latin first, and a single face stays single", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  choose(h, "doc-font-latin", "Arial");
  choose(h, "doc-font-family", "바탕");
  apply(h);
  const run = savedRuns(h).find((r) => r.fontEastAsia);
  assert.deepEqual([run.font, run.fontEastAsia], ["Arial", "바탕"]);
  const span = h.editor.querySelector(".eo-run");
  assert.equal(span.style.fontFamily, '"Arial", "바탕"', "Latin text takes the English face, Korean the other");
  assert.equal(span.dataset.eoEastfont, "바탕");

  await openDialog(h, { whole: true });
  assert.deepEqual([byId(h, "doc-font-latin").value, byId(h, "doc-font-family").value], ["Arial", "바탕"], "the dialog shows both");
  choose(h, "doc-font-family", "Arial");
  apply(h);
  const single = savedRuns(h).find((r) => r.font === "Arial");
  assert.equal(single.fontEastAsia, undefined, "the same face is one font");
});

test("changing only 영어 글꼴 keeps the East Asian face as it was, and the style bar's font sets both", async () => {
  const h = await createRunHarness([p("가나다")]);
  const text = h.blocks[0].childNodes[0];
  h.runs.applyFormat(h.editor, { startContainer: text, startOffset: 0, endContainer: text, endOffset: 3 }, { font: "Calibri", eastfont: "맑은 고딕" });
  await openDialog(h, { whole: true });
  choose(h, "doc-font-latin", "Arial");
  apply(h);
  let run = savedRuns(h).find((r) => r.fontEastAsia);
  assert.deepEqual([run.font, run.fontEastAsia], ["Arial", "맑은 고딕"]);
  const node = h.editor.querySelector(".eo-run").childNodes[0];
  h.runs.applyFormat(h.editor, { startContainer: node, startOffset: 0, endContainer: node, endOffset: 3 }, { font: "Verdana" });
  run = savedRuns(h)[0];
  assert.deepEqual([run.font, run.fontEastAsia], ["Verdana", undefined], "a font named alone is for both");
});

test("a decoded run opens with both faces and the engine's names are checked", async () => {
  const h = await createRunHarness([p("가")]);
  assert.deepEqual(h.runs.fromModelRun({ font: "Calibri", fontEastAsia: "맑은 고딕" }), { font: "Calibri", eastfont: "맑은 고딕" });
  assert.deepEqual(h.runs.fromModelRun({ font: "Calibri", fontEastAsia: 'bad"name' }), { font: "Calibri" });
  assert.deepEqual(h.runs.fromModelRun({ fontEastAsia: "x".repeat(65) }), {});
});

test("기본값으로 설정 keeps the two faces apart in the defaults", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  choose(h, "doc-font-latin", "Arial");
  choose(h, "doc-font-family", "바탕");
  h.userClick(byId(h, "btn-doc-font-default"));
  assert.match(byId(h, "doc-font-default-summary").textContent, /Arial 바탕/);
  h.userClick(byId(h, "btn-doc-font-default-apply"));
  const defaults = JSON.parse(h.editor.dataset.eoDefaults);
  assert.deepEqual([defaults.font, defaults.eastAsianFont], ["Arial", "바탕"]);
  assert.equal(h.editor.style.fontFamily, '"Arial", "바탕"');
});
