import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem, switchModule } from "./test-support/modules.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-398: slide find and replace, format painter, font dialog, table editing, overview, transitions, show settings,
// slide size and color schemes in the slide editor.

async function openSlide() {
  const h = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-slide-pd398" });
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  return h;
}

const $ = (document, id) => document.getElementById(id);
const ctrl = (h, key) => h.sandbox.dispatchWindowKeydown({ key, ctrlKey: true });
const type = (h, id, value) => {
  const input = $(h.document, id);
  input.value = value;
  input.dispatchEvent({ type: "input", target: input });
};

test("바꾸기 replaces text in text boxes, table cells, notes and picture alt text as one undo step", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "textbox", x: 80, y: 80, w: 300, h: 60, text: "사과 하나, 사과 둘" });
  canvas.addElement({ kind: "table", x: 80, y: 200, w: 400, h: 120, rows: [["사과", "배"], ["귤", "사과즙"]] });
  canvas.addElement({ kind: "chart", x: 500, y: 80, w: 300, h: 200, chartType: "column", title: "사과 판매" });
  const slide = canvas.currentSlide();
  slide.notes = "사과 메모";
  assert.equal(ctrl(h, "h").defaultPrevented, true);
  const dialog = $(h.document, "find-dialog");
  assert.equal(dialog.open, true);
  assert.equal($(h.document, "find-dialog-title").textContent, "바꾸기");
  assert.equal($(h.document, "find-replace-buttons").hidden, false, "the slide window has the replace row");
  type(h, "find-query", "사과");
  assert.match($(h.document, "find-count").textContent || $(h.document, "find-status").textContent, /1 \/ 6/, "chart title counts as a match: 2 + 2 cells + notes + chart");
  type(h, "find-replacement", "포도");
  $(h.document, "btn-find-replace-all").dispatchEvent({ type: "click" });
  await flushAsyncTurns(4);
  const els = canvas.getElements();
  assert.equal(els[0].text, "포도 하나, 포도 둘");
  assert.deepEqual(els[1].rows, [["포도", "배"], ["귤", "포도즙"]]);
  assert.equal(slide.notes, "포도 메모");
  assert.equal(els[2].title, "사과 판매", "the chart's text is found but not replaced");
  assert.match($(h.document, "find-status").textContent, /바꿨습니다/);
  assert.match($(h.document, "find-status").textContent, /차트·삽입 개체 1개는 건너뜀/);
  dialog.close();
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(6);
  assert.equal(canvas.getElements()[0].text, "사과 하나, 사과 둘", "one undo restores every replacement");
  assert.equal(canvas.currentSlide().notes, "사과 메모");
});

const act = (document, name) => {
  const item = document.querySelector(`#menu-bar [data-slide-act="${name}"]`);
  assert.ok(item, `missing menu item ${name}`);
  return item;
};

test("서식 복사 takes the look of an object and puts it on another one (Ctrl+Shift+C, Ctrl+Shift+V)", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "textbox", x: 40, y: 40, w: 200, h: 60, text: "원본", bold: true, italic: true, fontSizePt: 30, color: "FF0000", fill: "FFFF00", font: "Arial", dash: "dash", shadow: { color: "000000", alphaPct: 50, blurPt: 3, distPt: 2, angleDeg: 45 } });
  canvas.addElement({ kind: "textbox", x: 40, y: 200, w: 200, h: 60, text: "대상" });
  const [src, dst] = canvas.getElements().map((e) => e.id);
  canvas.selectElement(src);
  // Ctrl+Shift+C is a browser key, so only the desktop shell binds it; the menu item runs the same act.
  assert.equal(h.host.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS.find((b) => b.keys === "Ctrl+Shift+C" && b.module === "SLIDE").action, "slide-format-copy");
  assert.equal(h.host.__EASYOFFICE_SLIDE_RUN_ACT__("format-copy"), true);
  canvas.selectElement(dst);
  assert.equal(h.sandbox.dispatchWindowKeydown({ key: "V", ctrlKey: true, shiftKey: true }).defaultPrevented, true);
  const target = canvas.getElements().find((e) => e.id === dst);
  assert.deepEqual([target.bold, target.italic, target.fontSizePt, target.color, target.fill, target.font, target.dash], [true, true, 30, "FF0000", "FFFF00", "Arial", "dash"]);
  assert.equal(target.shadow.alphaPct, 50);
  assert.equal(target.text, "대상", "the text stays");
  assert.deepEqual(canvas.getElements().find((e) => e.id === src).fontSizePt, 30, "the source is unchanged");
});

test("the 서식 복사 brush waits for the next click and Esc drops it", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "textbox", x: 40, y: 40, w: 200, h: 60, text: "원본", underline: true, color: "00AA00" });
  canvas.addElement({ kind: "textbox", x: 40, y: 200, w: 200, h: 60, text: "대상" });
  const [src, dst] = canvas.getElements().map((e) => e.id);
  canvas.selectElement(src);
  const brush = $(h.document, "btn-slide-format-painter");
  h.userClick(brush);
  assert.equal(brush.getAttribute("aria-pressed"), "true");
  h.sandbox.dispatchWindowKeydown({ key: "Escape" });
  assert.equal(brush.getAttribute("aria-pressed"), "false", "Esc drops the copied look");
  h.userClick(brush);
  canvas.selectElement(dst);
  $(h.document, "slide-canvas-layer").dispatchEvent({ type: "click", target: $(h.document, "slide-canvas-layer") });
  const target = canvas.getElements().find((e) => e.id === dst);
  assert.equal(target.underline, true);
  assert.equal(target.color, "00AA00");
  assert.equal(brush.getAttribute("aria-pressed"), "false");
});

test("서식 > 글꼴 opens the font dialog on the selected text and writes the face, size, color and style", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "textbox", x: 40, y: 40, w: 200, h: 60, text: "글꼴" });
  canvas.selectElement(canvas.getElements()[0].id);
  h.userClick(act(h.document, "font-dialog"));
  const dialog = $(h.document, "doc-font-dialog");
  assert.equal(dialog.open, true);
  assert.equal($(h.document, "doc-font-tab-advanced").hidden, true, "the 고급 tab has nothing to set on a slide");
  assert.equal($(h.document, "doc-font-strike").disabled, true);
  assert.equal($(h.document, "btn-doc-font-default").hidden, true);
  $(h.document, "doc-font-size").value = "40";
  $(h.document, "doc-font-style").value = "굵게 기울임꼴";
  $(h.document, "doc-font-latin").value = "Georgia";
  $(h.document, "btn-doc-font-apply").dispatchEvent({ type: "click" });
  const el = canvas.getElements()[0];
  assert.deepEqual([el.fontSizePt, el.bold, el.italic, el.font], [40, true, true, "Georgia"]);
  assert.equal(canvas.currentSlide().elements[0].font, "Georgia");
  const wire = h.host.__EASYOFFICE_SLIDE_CANVAS_CODEC__.serializeDeckPayload([canvas.currentSlide()], h.document, null);
  assert.equal(wire.slides[0].elements[0].font, "Georgia", "the face goes to the file engine");
});

test("보기 > 슬라이드 정렬 shows every slide as a thumbnail, a drag moves a slide and a double click opens it", async () => {
  const h = await openSlide();
  for (let i = 0; i < 3; i += 1) h.userClick($(h.document, "btn-add-slide"));
  await flushAsyncTurns(4);
  h.userClick(act(h.document, "view-sorter"));
  const sorter = $(h.document, "slide-sorter");
  assert.equal(sorter.hidden, false);
  assert.equal($(h.document, "slide-workspace").hidden, true);
  const cells = () => Array.from(sorter.querySelectorAll(".slide-sorter-cell"));
  assert.equal(cells().length, 4);
  assert.ok(cells()[0].querySelector(".slide-sorter-thumb .slide-show-frame"), "a cell draws the slide as the show does");
  const first = cells()[0];
  first.dispatchEvent({ type: "dragstart", target: first, dataTransfer: { setData() {} } });
  const last = cells()[3];
  last.dispatchEvent({ type: "drop", target: last, preventDefault() {} });
  await flushAsyncTurns(4);
  assert.equal(cells().length, 4, "the grid is drawn again after the move");
  cells()[2].dispatchEvent({ type: "dblclick", target: cells()[2] });
  assert.equal(sorter.hidden, true, "a double click shows the slide in the normal view");
  assert.equal($(h.document, "slide-workspace").hidden, false);
});

test("화면 전환 sets the effect, its side and the advance on the slide, and the save payload carries them", async () => {
  const h = await openSlide();
  h.userClick(act(h.document, "transition-dialog"));
  assert.equal($(h.document, "slide-trans-dialog").open, true);
  $(h.document, "slide-trans-effect").value = "wipe";
  $(h.document, "slide-trans-effect").dispatchEvent({ type: "change", target: $(h.document, "slide-trans-effect") });
  $(h.document, "slide-trans-dir").value = "top";
  $(h.document, "slide-trans-click").checked = false;
  $(h.document, "slide-trans-timed").checked = true;
  $(h.document, "slide-trans-seconds").value = "2.5";
  h.userClick($(h.document, "btn-slide-trans-ok"));
  const slide = h.host.__EASYOFFICE_SLIDE_CANVAS__.currentSlide();
  assert.deepEqual([slide.transition, slide.transitionDir, slide.noClickAdvance, slide.advanceAfterMs], ["wipe", "top", true, 2500]);
  assert.match($(h.document, "slide-meta-status").textContent, /전환: 지우기/);
  const wire = h.host.__EASYOFFICE_SLIDE_CANVAS_CODEC__.serializeDeckPayload([slide], h.document, null).slides[0];
  assert.deepEqual([wire.transition, wire.transitionDir, wire.noClickAdvance, wire.advanceAfterMs], ["wipe", "top", true, 2500]);
  const hostile = h.host.__EASYOFFICE_SLIDE_EXT__.normalize({ transitionDir: "sideways", advanceAfterMs: 9e15, noClickAdvance: "yes" }, new Set());
  assert.deepEqual(hostile, { advanceAfterMs: 86400000 }, "an unknown side and a flag that is not true are dropped, the wait is capped");
  assert.equal(h.host.__EASYOFFICE_SLIDE_CANVAS_CODEC__.normalizeTransition("spin"), "none");
});

test("쇼 설정 sets the repeat flag and the manual advance on every slide and reads them back", async () => {
  const h = await openSlide();
  h.userClick($(h.document, "btn-add-slide"));
  h.userClick(act(h.document, "show-setup"));
  $(h.document, "slide-show-loop").checked = true;
  $(h.document, "slide-show-manual").checked = true;
  h.userClick($(h.document, "btn-slide-show-ok"));
  const codec = h.host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
  const canvasSlide = h.host.__EASYOFFICE_SLIDE_CANVAS__.currentSlide();
  const payload = codec.serializeDeckPayload([canvasSlide], h.document, null);
  assert.equal(payload.showLoop, true);
  assert.equal(payload.manualAdvance, true);
  const back = codec.hydrateDecodedDeck({ showLoop: true, manualAdvance: true, colorScheme: { name: "x", colors: Array(12).fill("123456") }, slides: [{ title: "a" }, { title: "b" }] }, () => ({}));
  assert.deepEqual(back.map((s) => [s.showLoop, s.manualAdvance, s.colorScheme.colors.length]), [[true, true, 12], [true, true, 12]]);
  assert.equal(h.host.__EASYOFFICE_SLIDE_EXT__.deckFields({ colorScheme: { colors: ["zz"] } }).colorScheme, undefined, "a scheme without twelve colors is dropped");
});

test("the auto advance cues the slide's own wait, stops at the last slide unless the show repeats, and a manual show has no timer", () => {
  const env = {};
  loadUiModule(new URL("../crates/easyoffice-app/ui/slide-show-timing.js", import.meta.url).pathname, env);
  const { waitFor, nextIndex } = env.__EASYOFFICE_SHOW_TIMING__;
  const slide = { advanceAfterMs: 3000 };
  assert.equal(waitFor(slide, { manual: false, loop: false, isLast: false }), 3000);
  assert.equal(waitFor(slide, { manual: true, loop: true, isLast: false }), 0, "a show set to manual ignores the waits");
  assert.equal(waitFor(slide, { manual: false, loop: false, isLast: true }), 0, "the last slide of a show that does not repeat has no timer");
  assert.equal(waitFor(slide, { manual: false, loop: true, isLast: true }), 3000, "a repeating show moves on from the last slide too");
  assert.equal(waitFor({ advanceAfterMs: 9e15 }, { manual: false, loop: false, isLast: false }), 86_400_000);
  assert.equal(waitFor({ advanceAfterMs: -5 }, { manual: false, loop: false, isLast: false }), 0);
  assert.equal(waitFor({}, { manual: false, loop: false, isLast: false }), 0);
  const shown = (from) => from; // no slide is hidden
  assert.equal(nextIndex(1, 3, false, shown), 2);
  assert.equal(nextIndex(2, 3, false, shown), -1);
  assert.equal(nextIndex(2, 3, true, shown), 0, "the show starts again after the last slide");
  const fired = [];
  const timers = [];
  const clock = env.__EASYOFFICE_CREATE_SHOW_TIMING__({ setTimeout: (fn, ms) => (timers.push([fn, ms]), timers.length), clearTimeout: (id) => fired.push("clear" + id) });
  assert.equal(clock.cue(slide, { manual: false, loop: false, isLast: false }, () => fired.push("fire")), true);
  assert.equal(timers[0][1], 3000);
  clock.cue({}, { manual: false, loop: false, isLast: false }, () => fired.push("fire"));
  assert.deepEqual(fired, ["clear1"], "cueing again cancels the earlier timer");
  assert.equal(clock.isCued(), false);
});

test("a color scheme moves the colors of the old scheme to the same slot of the new one", () => {
  const env = {};
  loadUiModule(new URL("../crates/easyoffice-app/ui/slide-colors.js", import.meta.url).pathname, env);
  const { SCHEMES, slotMap, recolorSlide } = env.__EASYOFFICE_SLIDE_COLORS_MATH__;
  const [base, office] = SCHEMES;
  const slide = {
    elements: [{ fill: base.colors[4], stroke: "#" + base.colors[5], color: "ABCDEF", outline: { color: base.colors[6], widthPt: 1 }, shadow: { color: base.colors[0] } }],
    background: { kind: "gradient", from: base.colors[3], to: "123456", angleDeg: 90 },
  };
  recolorSlide(slide, slotMap(base.colors, office.colors));
  assert.equal(slide.elements[0].fill, office.colors[4]);
  assert.equal(slide.elements[0].stroke, office.colors[5]);
  assert.equal(slide.elements[0].color, "ABCDEF", "a color in no slot stays");
  assert.equal(slide.elements[0].outline.color, office.colors[6]);
  assert.equal(slide.elements[0].shadow.color, office.colors[0], "black is black in both schemes");
  assert.equal(slide.background.from, office.colors[3]);
  assert.equal(slide.background.to, "123456");
  assert.ok(SCHEMES.every((s) => s.colors.length === 12 && s.colors.every((c) => /^[0-9A-F]{6}$/.test(c))));
});

test("표 편집 inserts and deletes rows and columns next to the clicked cell and refuses past the limits", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "table", x: 40, y: 40, w: 360, h: 150, rows: [["가", "나"], ["다", "라"]] });
  const id = canvas.getElements()[0].id;
  canvas.selectElement(id);
  const rowsNow = () => canvas.getElements()[0].rows;
  const cell = Array.from($(h.document, "slide-canvas-layer").querySelectorAll("td, th")).find((td) => td.dataset.rowIdx === "0" && td.dataset.colIdx === "1");
  assert.ok(cell, "the table is drawn with its cells");
  cell.dispatchEvent({ type: "click", target: cell });
  h.userClick(act(h.document, "table-row-below"));
  assert.deepEqual(rowsNow(), [["가", "나"], ["", ""], ["다", "라"]], "a row goes under the clicked cell's row");
  canvas.selectElement(id);
  h.userClick(act(h.document, "table-col-left"));
  assert.equal(rowsNow()[0].length, 3);
  h.userClick(act(h.document, "table-row-delete"));
  h.userClick(act(h.document, "table-col-delete"));
  assert.ok(rowsNow().every((r) => r.length === 2));
  const { edit, MAX_ROWS, MAX_COLS } = h.host.__EASYOFFICE_SLIDE_TABLE_MATH__;
  const full = Array.from({ length: MAX_ROWS }, () => Array(MAX_COLS).fill("x"));
  assert.equal(edit(full, { row: 0, col: 0 }, "row-above"), null);
  assert.equal(edit(full, { row: 0, col: 0 }, "col-right"), null);
  assert.equal(edit([["a"]], { row: 0, col: 0 }, "row-delete"), null, "the last row stays");
  assert.equal(edit([["a"]], { row: 0, col: 0 }, "col-delete"), null, "the last column stays");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(4);
  assert.ok(rowsNow().length >= 2);
});

test("the overview scale and grid follow the reference overview: a fifth of the window, at least 150 px", async () => {
  const h = await openSlide();
  const m = h.host.__EASYOFFICE_SLIDE_OVERVIEW_MATH__;
  assert.equal(m.thumbWidth(1000), 200);
  assert.equal(m.thumbWidth(600), 150);
  assert.equal(m.columnsFor(1000, 200), 4);
  assert.equal(m.columnsFor(100, 200), 1);
  assert.equal(m.stepTarget("ArrowDown", 1, 4, 10), 5);
  assert.equal(m.stepTarget("ArrowDown", 8, 4, 10), 9, "the last row stops at the last slide");
  assert.equal(m.stepTarget("ArrowLeft", 0, 4, 10), 0);
  assert.equal(m.stepTarget("x", 0, 4, 10), null);
});
