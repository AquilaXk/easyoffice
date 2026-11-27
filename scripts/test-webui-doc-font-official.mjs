import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// The 글꼴 dialog follows the word processor's official dialog: the 배율 box is a field with its presets, the value
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

test("배율 is a field you can type into, and its presets are the field's own list", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h);
  const scale = byId(h, "doc-font-scale");
  assert.equal(scale.getAttribute("type"), "text", "you can type into it");
  assert.equal(scale.classList.contains("dialog-input"), true, "it is the dialog's own field, with no custom arrow");
  const presets = h.document.getElementById(scale.getAttribute("list"));
  assert.equal(presets.tagName, "DATALIST");
  assert.deepEqual([...presets.querySelectorAll("option")].map((o) => o.value), ["200%", "150%", "100%", "90%", "80%", "66%", "50%", "33%"]);
  scale.value = "150%";
  fire(scale);
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.deepEqual(withKey(h, "scalePercent"), [150]);
});

test("the arrow keys step through a list box and put the row in the field", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, "font");
  const size = byId(h, "doc-font-size");
  const press = (key) => size.dispatchEvent({ type: "keydown", key, bubbles: true, preventDefault() {}, stopPropagation() {} });
  assert.equal(size.value, "10", "the list starts on the field's row");
  press("ArrowDown");
  assert.equal(size.value, "10.5");
  press("ArrowDown");
  assert.equal(size.value, "11");
  press("ArrowUp");
  assert.equal(size.value, "10.5");
  assert.equal(size.getAttribute("aria-activedescendant"), byId(h, "doc-font-size-list").children[3].id);
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

test("글꼴 tab has the official items with their access keys", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, "font");
  const labels = [...h.document.querySelectorAll("#doc-font-pane-font label")].map((l) => l.textContent.trim());
  for (const label of ["한글 글꼴(T):", "글꼴 스타일(Y):", "크기(S):", "영어 글꼴(F):", "글꼴 색(C):", "밑줄 스타일(U):", "밑줄 색(I):", "강조점(·):",
    "취소선(K)", "작은 대문자(M)", "이중 취소선(L)", "모두 대문자(A)", "위 첨자(P)", "숨김(H)", "아래 첨자(B)"]) {
    assert.ok(labels.includes(label), `${label} in ${labels}`);
  }
  assert.deepEqual([...h.document.querySelectorAll("#doc-font-dialog legend")].map((l) => l.textContent).slice(0, 1), ["효과"]);
  const sizes = [...byId(h, "doc-font-size-list").querySelectorAll(".goto-bookmark-item")].map((row) => row.textContent);
  assert.deepEqual(sizes, ["8", "9", "10", "10.5", "11", "12", "14", "16", "18", "20", "22", "24", "26", "28", "36", "48", "72"]);
});

test("크기 and 글꼴 스타일 are fields with list boxes: a row fills the field and typing highlights the row", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, "font");
  const size = byId(h, "doc-font-size");
  const rows = [...byId(h, "doc-font-size-list").querySelectorAll(".goto-bookmark-item")];
  h.userClick(rows[5]);
  assert.equal(size.value, "12", "a row fills the field");
  size.value = "2";
  fire(size, "input");
  assert.equal(rows.findIndex((r) => r.getAttribute("aria-selected") === "true"), rows.findIndex((r) => r.textContent.startsWith("2")), "the first row that starts with the text is highlighted");
  assert.equal(rows.filter((r) => r.classList.contains("selected")).length, 1, "the highlighted row carries the list's selected class");
  h.userClick(byId(h, "btn-doc-font-apply"));
  assert.deepEqual(savedRuns(h).filter((r) => r.sizeHalfPt).map((r) => r.sizeHalfPt), [4], "2 pt is 4 half-points");
});

test("글꼴 색 and 밑줄 스타일 write the run's color and underline", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, "font");
  const toggle = h.document.querySelector("#doc-font-color-host [data-color-toggle]");
  assert.equal(toggle.textContent.includes("자동"), true, "no color shows 자동");
  h.userClick(toggle);
  h.userClick(h.document.querySelector('#doc-font-color-host .sb-hl-swatch[aria-label="#ff0000"]'));
  assert.equal(h.document.querySelector("#doc-font-color-host .dialog-swatch-text").textContent, "#FF0000");
  const style = byId(h, "doc-font-underline-style");
  assert.deepEqual([...style.querySelectorAll("option")].map((o) => o.textContent), ["(없음)", "한 줄", "단어만", "이중 선", "굵은 선", "점선", "굵은 점선", "파선", "굵은 파선",
    "긴 파선", "굵은 긴 파선", "1점 쇄선", "굵은 1점 쇄선", "2점 쇄선", "굵은 2점 쇄선", "물결선", "굵은 물결선", "이중 물결선"]);
  style.value = "0";
  fire(style);
  h.userClick(byId(h, "btn-doc-font-apply"));
  const run = savedRuns(h).find((r) => r.color);
  assert.equal(run.color, "FF0000");
  assert.equal(run.underline, true);
  await openDialog(h, "font");
  assert.equal(h.document.querySelector("#doc-font-color-host .dialog-swatch-text").textContent, "#FF0000", "the dialog shows the run's color");
  assert.equal(byId(h, "doc-font-underline-style").value, "0");
});

test("기본값으로 설정 asks once and makes the font and size the document's defaults", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, "font");
  const confirm = byId(h, "doc-font-default-dialog");
  const size = byId(h, "doc-font-size");
  size.value = "12";
  fire(size, "input");
  const family = byId(h, "doc-font-family");
  const option = h.document.createElement("option");
  option.value = "Arial";
  option.textContent = "Arial";
  family.appendChild(option);
  family.value = "Arial";
  fire(family);
  h.userClick(byId(h, "btn-doc-font-default"));
  assert.equal(confirm.open, true, "the prompt opens");
  assert.match(byId(h, "doc-font-default-summary").textContent, /12pt/);
  assert.deepEqual([...confirm.querySelectorAll('input[type="radio"]')].map((r) => r.parentNode.textContent.trim()), ["이 문서에만(T)"], "only what the model keeps");
  h.userClick(confirm.querySelector("[data-dialog-close].dialog-btn"));
  assert.equal(h.editor.dataset.eoDefaults, undefined, "취소 sets nothing");
  confirm.open = false;
  h.userClick(byId(h, "btn-doc-font-default"));
  h.userClick(byId(h, "btn-doc-font-default-apply"));
  const defaults = JSON.parse(h.editor.dataset.eoDefaults);
  assert.equal(defaults.sizeHalfPoints, 24);
  assert.deepEqual([defaults.font, defaults.eastAsianFont], ["Arial", "Arial"]);
  assert.equal(h.editor.style.fontFamily, '"Arial"');
  assert.equal(defaults.line240ths, 259, "the rest are a new document's defaults");
  assert.equal(h.editor.style.fontSize, "12pt", "an unstyled paragraph draws with it");
  assert.equal(byId(h, "doc-font-dialog").open, false, "the font dialog closes");
});

test("the items the engine does not keep yet are visible and off", async () => {
  const h = await createRunHarness([p("가나다")]);
  await openDialog(h, "font");
  for (const id of ["btn-doc-font-effects"]) {
    assert.equal(byId(h, id).disabled, true, id);
  }
});
