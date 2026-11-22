import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// A double click on the edge of a column header fits the column to its widest cell, or every selected column when the
// pressed one is among them: hidden columns, hidden rows and merged cells are left out, a number counts as it is shown,
// an empty column goes back to the default width, and the fit is one undo step.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(ui("sheet-col-fit.js"), env);
const { columnsToFit, PADDING_PX, BUTTON_PX } = env.__EASYOFFICE_SHEET_COL_FIT__;
const create = env.__EASYOFFICE_CREATE_SHEET_COL_FIT__;

const packRef = (ref) => {
  const m = /^([A-Z]+)(\d+)$/.exec(ref);
  let col = 0;
  for (const ch of m[1]) col = col * 26 + ch.charCodeAt(0) - 64;
  return Number(m[2]) * 32768 + col;
};
// 8px a character, 9px in bold.
const measure = (font, family, text) => text.length * (font.includes("600") ? 9 : 8);

function fitOf(map, extra = {}) {
  return create({
    getMap: () => map,
    packRef,
    measure,
    isColHidden: () => false,
    isRowHidden: () => false,
    mergeAt: () => null,
    headerCells: () => false,
    formatGeneral: (t) => t,
    defaultWidth: 96,
    minWidth: 36,
    maxWidth: 720,
    ...extra,
  });
}
const textW = (text) => [...text].length * 8 + PADDING_PX;
const E = (value, style, extra = {}) => ({ value, formula: "", style, ...extra });
const widthOf = async (fit, col, resolve) => (await fit.widthsFor([col], resolve)).widths.find((w) => w.col === col)?.width;

test("a column is as wide as its widest cell and the cell's padding", async () => {
  const map = { A1: E("가나다"), A2: E("가나다라마바사"), A3: E("짧음"), B1: E("x") };
  const fit = fitOf(map);
  assert.equal(await widthOf(fit, 1), 7 * 8 + PADDING_PX);
  assert.equal(await widthOf(fit, 2), 36, "narrower than the default, but not under the grid's least");
});

test("the font of a cell counts: a bold or a larger text is wider", async () => {
  const bold = Object.freeze({ fontWeight: "600" });
  const fit = fitOf({ A1: E("가나다", bold), A2: E("가나다라") });
  assert.equal(await widthOf(fit, 1), 4 * 8 + PADDING_PX, "four plain characters beat three bold ones");
  assert.equal(await widthOf(fitOf({ A1: E("가나다라", bold), A2: E("가나다라") }), 1), 4 * 9 + PADDING_PX);
  const seen = [];
  await fitOf({ A1: E("가", { fontSize: "20px", fontFamily: "Noto Sans" }) }, { measure: (font, family, text) => (seen.push([font, family]), 5) }).widthsFor([1]);
  assert.deepEqual(seen, [["normal 400 20px", "Noto Sans"]]);
});

test("a cell with line breaks counts by its longest line, and a wrapped cell counts as one line", async () => {
  const fit = fitOf({ A1: E("한\n두 줄이 더 긴 줄\n셋"), B1: E("줄바꿈 켜진 칸의 글자", { wrap: true }) });
  assert.equal(await widthOf(fit, 1), "두 줄이 더 긴 줄".length * 8 + PADDING_PX);
  assert.equal(await widthOf(fit, 2), "줄바꿈 켜진 칸의 글자".length * 8 + PADDING_PX, "the column widens to the text; wrap decides only when it is narrower");
});

test("hidden columns, hidden rows and merged cells are left out; an empty column goes back to the default width", async () => {
  const map = { A1: E("일"), A2: E("보이지 않는 아주 긴 행의 글자"), A3: E("병합된 셀의 아주 아주 긴 글자"), B1: E("") };
  const fit = fitOf(map, { isRowHidden: (r) => r === 2, mergeAt: (r, c) => (r === 3 && c === 1 ? { r1: 3, c1: 1, r2: 3, c2: 2 } : null), isColHidden: (c) => c === 3 });
  const { widths: out } = await fit.widthsFor([1, 2, 3]);
  assert.deepEqual(out, [{ col: 1, width: 36 }, { col: 2, width: 96 }], "column 3 is hidden: not listed; column 2 has no content");
});

test("the widest allowed width ends the search, and the width never passes it", async () => {
  let calls = 0;
  const map = {};
  for (let r = 1; r <= 200; r += 1) map["A" + r] = E("긴".repeat(200 + r));
  const fit = fitOf(map, { measure: (font, family, text) => (calls += 1, text.length * 8) });
  assert.equal(await widthOf(fit, 1), 720);
  assert.ok(calls < 20, "once the column is as wide as it may be, the rest are not measured (" + calls + ")");
});

test("a text too short to beat the widest so far is not measured, a longer one that is wider always wins", async () => {
  let calls = 0;
  const map = { A1: E("가".repeat(40)) };
  for (let r = 2; r <= 100; r += 1) map["A" + r] = E("나다");
  map.A101 = E("W".repeat(41));
  const fit = fitOf(map, { measure: (font, family, text) => (calls += 1, text.length * 8) });
  assert.equal(await widthOf(fit, 1), 41 * 8 + PADDING_PX);
  assert.ok(calls < 10, "not one measurement per cell (" + calls + ")");
});

test("a number counts as the engine shows it; without an answer it counts as General", async () => {
  const map = { A1: E("45000", Object.freeze({ numFmt: "yyyy-mm-dd" })), B1: E("0.30000000000000004") };
  const asked = [];
  const answered = async (wanted) => (asked.push([...wanted.keys()]), new Map([["yyyy-mm-dd", new Map([["45000", { t: "2023-03-15" }]])]]));
  const fit = fitOf(map, { formatGeneral: (t) => (t.length > 10 ? "0.3" : t) });
  assert.equal(await widthOf(fit, 1, answered), 10 * 8 + PADDING_PX);
  assert.deepEqual(asked, [["yyyy-mm-dd"]]);
  assert.equal(await widthOf(fit, 2), 3 * 8 + PADDING_PX, "General shows 0.3");
  assert.equal(await widthOf(fit, 1), 5 * 8 + PADDING_PX, "no answer: the stored value");
  assert.equal(await widthOf(fitOf({ A1: E("007", Object.freeze({ numFmt: "@" })) }), 1, async () => assert.fail("a text format asks nothing")), 3 * 8 + PADDING_PX);
});

test("the drop-down button of a filter header cell adds its width", async () => {
  const fit = fitOf({ A1: E("제목입니다"), A2: E("값") }, { headerCells: (r, c) => r === 1 && c === 1 });
  assert.equal(await widthOf(fit, 1), 5 * 8 + BUTTON_PX + PADDING_PX);
});

test("a page that cannot measure gets no widths", async () => {
  assert.deepEqual(await fitOf({ A1: E("가") }, { measure: () => null }).widthsFor([1]), { refused: "measure" });
});

test("a fit that needs more measurements or more time than its budget is refused whole", async () => {
  const map = {};
  for (let i = 0; i < 300000; i += 1) map["A" + (i + 1)] = E("가".repeat(1 + (i % 50)) + i);
  let calls = 0;
  const grows = fitOf(map, { measure: () => (calls += 1, calls * 0.001) });
  assert.deepEqual(await grows.widthsFor([1]), { refused: "budget" });
  assert.ok(calls <= 200001, "the count ends the fit (" + calls + ")");
  let clock = 0;
  const many = {};
  for (let i = 0; i < 1000; i += 1) many["A" + (i + 1)] = E("글자" + i);
  const late = fitOf(many, { now: () => clock, measure: () => ((clock += 10), 1) });
  assert.deepEqual(await late.widthsFor([1]), { refused: "budget" }, "a slow canvas ends the fit by time");
});

test("the columns a double click fits: the selected ones when the pressed one is among them, else the pressed one", () => {
  assert.deepEqual(columnsToFit(3, { axis: "col", c1: 2, c2: 4 }), [2, 3, 4]);
  assert.deepEqual(columnsToFit(7, { axis: "col", c1: 2, c2: 4 }), [7]);
  assert.deepEqual(columnsToFit(3, { axis: "row", c1: 1, c2: 16384 }), [3], "a row selection does not widen the fit");
  assert.deepEqual(columnsToFit(3, { axis: null, c1: 1, c2: 5 }), [3], "nor does a cell range");
});

// ---- the page --------------------------------------------------------------------------------------

const MODEL = {
  sheets: [
    {
      name: "맞춤",
      cells: [
        { ref: "A1", raw: "열 하나의 아주 긴 글" },
        { ref: "B1", raw: "둘" },
        { ref: "B2", raw: "열 둘에 쓴 글자가 조금 길다" },
        { ref: "C1", raw: "셋째 열의 중간 길이 글" },
      ],
      columns: [{ index: 1, widthPx: 300 }],
    },
  ],
};

async function open() {
  const h = await createWorkspaceHarness();
  const d = h.document;
  const win = d.defaultView;
  // The page measures on a canvas; the test double is 8px a character, 9px in bold.
  const make = d.createElement.bind(d);
  d.createElement = (tag) => {
    const el = make(tag);
    if (tag === "canvas") el.getContext = () => ({ font: "", measureText(t) { return { width: [...t].length * (this.font.includes("600") ? 9 : 8) }; } });
    return el;
  };
  win.getComputedStyle = () => ({ fontFamily: "sans-serif", fontSize: "13px" });
  switchModule(d, "SHEET");
  await flushAsyncTurns();
  await pickFile(d, makeFixtureFile("맞춤.xlsx", MODEL));
  await flushAsyncTurns(12);
  return h;
}

const head = (d, name) => d.querySelector(`#pane-sheet th[data-col="${name}"]`);
const handle = (d, name) => d.querySelector(`#pane-sheet .col-resizer[data-col-resizer="${name}"]`);
const widthPx = (d, name) => Number.parseInt(head(d, name).style.width, 10);
const doubleClick = async (d, name) => {
  const el = handle(d, name);
  el.dispatchEvent({ type: "dblclick", target: el, bubbles: true, button: 0 });
  await flushAsyncTurns(15);
};

test("a double click on the edge of a column header fits that column; one undo step gives the width back", async () => {
  const h = await open();
  const d = h.document;
  assert.equal(widthPx(d, "A"), 300);
  await doubleClick(d, "A");
  assert.equal(widthPx(d, "A"), textW("열 하나의 아주 긴 글"), "12 characters of 8px and the padding");
  assert.equal(d.getElementById("status-command-feedback").textContent, "열 너비 자동 맞춤: 1열");
  h.userClick(studioItem(d, "studio-undo"));
  await flushAsyncTurns(10);
  assert.equal(widthPx(d, "A"), 300);
});

test("with columns selected, the double click fits all of them when the pressed one is among them, only the pressed one otherwise", async () => {
  const h = await open();
  const d = h.document;
  const click = (el, extra = {}) => {
    el.dispatchEvent({ type: "mousedown", target: el, button: 0, bubbles: true, ...extra });
    el.dispatchEvent({ type: "mouseup", target: el, bubbles: true });
  };
  click(head(d, "A"));
  click(head(d, "B"), { shiftKey: true });
  await flushAsyncTurns(3);
  await doubleClick(d, "B");
  assert.equal(widthPx(d, "A"), textW("열 하나의 아주 긴 글"));
  assert.equal(widthPx(d, "B"), textW("열 둘에 쓴 글자가 조금 길다"));
  assert.equal(widthPx(d, "C"), 64, "C is not in the selection");
  await doubleClick(d, "C");
  assert.equal(widthPx(d, "C"), textW("셋째 열의 중간 길이 글"));
  assert.equal(widthPx(d, "A"), textW("열 하나의 아주 긴 글"), "and the others stay");
});

test("a column that already has the fitted width changes nothing and makes no undo step", async () => {
  const h = await open();
  const d = h.document;
  await doubleClick(d, "A");
  const fb = d.getElementById("status-command-feedback");
  fb.textContent = "";
  await doubleClick(d, "A");
  assert.equal(fb.textContent, "", "nothing to do");
});
