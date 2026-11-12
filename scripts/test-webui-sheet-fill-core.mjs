import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
for (const name of ["sheet-numfmt.js", "sheet-paste-core.js", "sheet-fill-core.js"]) loadUiModule(ui(name), env);
const core = env.__EASYOFFICE_SHEET_FILL_CORE__;

const cellOf = (v, extra = {}) => ({ value: v, formula: "", ...extra });
const refOf = (r, c) => String.fromCodePoint(64 + c) + r;
const shift = (f, dr, dc) => f + "@" + dr + "," + dc;

// Fills a column of values (A1..An) and answers the values written, ordered by row (or column).
function fillColumn(cells, direction, count, mode = "auto") {
  const source = { r1: 10, c1: 3, r2: 9 + cells.length, c2: 3 };
  const result = core.planFill({
    source: direction === "up" || direction === "down" ? source : { r1: 10, c1: 5, r2: 10, c2: 4 + cells.length },
    direction,
    count,
    mode,
    cellAt: (r, c) => cells[direction === "up" || direction === "down" ? r - 10 : c - 5],
    shiftFormula: shift,
    refOf,
  });
  return result;
}

const values = (result) => result.edits.map((e) => e.value);

// The cases below are the reference implementation's tests of its fill tools, with the cells as the
// text this editor stores (a number is its text).
test("a number series continues the step of two cells and runs backwards upwards", () => {
  assert.deepEqual(values(fillColumn(["2", "4"].map((v) => cellOf(v)), "down", 3)), ["6", "8", "10"]);
  const up = fillColumn(["10", "12"].map((v) => cellOf(v)), "up", 2);
  assert.deepEqual(up.edits.map((e) => e.ref), ["C9", "C8"]);
  assert.deepEqual(values(up), ["8", "6"], "the cell next to the source gets the next value away from it");
});

test("three or more numbers continue by regression or, when the ratio is equal, by the ratio", () => {
  assert.deepEqual(values(fillColumn(["1", "2", "3"].map((v) => cellOf(v)), "down", 2)), ["4", "5"]);
  assert.deepEqual(values(fillColumn(["2", "4", "8"].map((v) => cellOf(v)), "down", 2)), ["16", "32"]);
  assert.deepEqual(values(fillColumn(["1", "2", "4", "5"].map((v) => cellOf(v)), "down", 1)), ["6.5"], "the least squares line");
  assert.deepEqual(values(fillColumn(["0.1", "0.2"].map((v) => cellOf(v)), "down", 2)), ["0.3", "0.4"], "no float noise");
});

test("one number is copied, two kinds of fill swap with the modifier", () => {
  const one = fillColumn([cellOf("7")], "down", 3);
  assert.equal(one.applyType, "copy");
  assert.deepEqual(values(one), ["7", "7", "7"]);
  const swapped = fillColumn([cellOf("7")], "down", 3, "toggle");
  assert.equal(swapped.applyType, "series");
  assert.deepEqual(values(swapped), ["8", "9", "10"]);
  const two = fillColumn(["1", "2"].map((v) => cellOf(v)), "down", 4, "copy");
  assert.deepEqual(values(two), ["1", "2", "1", "2"]);
  const text = fillColumn([cellOf("가")], "down", 2, "toggle");
  assert.deepEqual(values(text), ["가", "가"], "text has no series to swap to");
});

test("text that holds a number counts up, keeps its width and its text around the number", () => {
  assert.deepEqual(values(fillColumn([cellOf("항목1")], "down", 3)), ["항목2", "항목3", "항목4"]);
  assert.deepEqual(values(fillColumn([cellOf("item009")], "down", 3)), ["item010", "item011", "item012"]);
  assert.deepEqual(values(fillColumn([cellOf("A-001")], "down", 2)), ["A-002", "A-003"]);
  assert.deepEqual(values(fillColumn([cellOf("A2B3C009D")], "down", 4)), ["A2B3C010D", "A2B3C011D", "A2B3C012D", "A2B3C013D"]);
  assert.deepEqual(values(fillColumn([cellOf("제1조")], "down", 2)), ["제2조", "제3조"]);
  assert.deepEqual(
    values(fillColumn(["A-001", "A-003", "A-010"].map((v) => cellOf(v)), "down", 4)),
    ["A-001", "A-003", "A-010", "A-001"],
    "no equal step: the run is copied"
  );
  assert.deepEqual(
    values(fillColumn(["A2B3C003D", "A2B3C002D", "A2B3C001D", "A2B3C000D", "A2B3C001D", "A2B3C002D"].map((v) => cellOf(v)), "down", 4)),
    ["A2B3C003D", "A2B3C004D", "A2B3C005D", "A2B3C006D"]
  );
  assert.deepEqual(values(fillColumn([cellOf("item003")], "up", 5)), ["item002", "item001", "item000", "item001", "item002"], "a count below zero turns back");
  const left = fillColumn(["A-003", "A-004"].map((v) => cellOf(v)), "left", 2);
  assert.deepEqual(left.edits.map((e) => e.ref), ["D10", "C10"]);
  assert.deepEqual(values(left), ["A-002", "A-001"]);
  assert.deepEqual(values(fillColumn([cellOf("항목1")], "down", 2, "copy")), ["항목1", "항목1"]);
});

test("weekdays and months of the Korean lists cycle in both directions", () => {
  assert.deepEqual(values(fillColumn([cellOf("월")], "down", 7)), ["화", "수", "목", "금", "토", "일", "월"]);
  assert.deepEqual(values(fillColumn([cellOf("토요일")], "down", 2)), ["일요일", "월요일"]);
  assert.deepEqual(values(fillColumn([cellOf("12월")], "down", 2)), ["1월", "2월"], "a month is not a counter");
  assert.deepEqual(values(fillColumn(["11월", "12월"].map((v) => cellOf(v)), "down", 3)), ["1월", "2월", "3월"]);
  assert.deepEqual(values(fillColumn([cellOf("월")], "up", 3)), ["일", "토", "금"]);
  assert.deepEqual(values(fillColumn(["월", "수"].map((v) => cellOf(v)), "down", 2)), ["금", "일"], "a step of two");
  assert.deepEqual(values(fillColumn(["Mon", "Wed", "Thu"].map((v) => cellOf(v)), "down", 2)), ["Mon", "Wed"], "no equal step: copy");
  assert.deepEqual(values(fillColumn([cellOf("Dec")], "down", 2)), ["Jan", "Feb"]);
  assert.deepEqual(values(fillColumn([cellOf("May")], "down", 2)), ["Jun", "Jul"], "the first list that holds the text");
  assert.deepEqual(values(fillColumn([cellOf("월")], "down", 2, "copy")), ["월", "월"]);
});

test("dates (a number with a date format) continue by the day, or by the step of the source", () => {
  const date = (v) => cellOf(v, { style: { numFmt: "yyyy-mm-dd" } });
  const one = fillColumn([date("46000")], "down", 3);
  assert.equal(one.applyType, "series", "a single date is not a single number");
  assert.deepEqual(values(one), ["46001", "46002", "46003"]);
  assert.deepEqual(values(fillColumn([date("46000"), date("46007")], "down", 2)), ["46014", "46021"]);
  assert.deepEqual(values(fillColumn([date("46000")], "up", 2)), ["45999", "45998"]);
  assert.deepEqual(values(fillColumn([cellOf("46000", { style: { numFmt: "@" } })], "down", 2)), ["46001", "46002"], "a text-format number is text that holds a number");
});

test("formulas are copied with their relative references moved by the distance", () => {
  const result = fillColumn([{ value: "", formula: "=A1+1" }], "down", 2);
  assert.deepEqual(result.edits.map((e) => [e.ref, e.formula, e.value]), [
    ["C11", "=A1+1@1,0", ""],
    ["C12", "=A1+1@2,0", ""],
  ]);
  const left = fillColumn([{ value: "", formula: "=A1" }], "left", 1);
  assert.equal(left.edits[0].formula, "=A1@0,-1");
  const mixed = fillColumn([cellOf("1"), { value: "", formula: "=B1" }], "down", 4);
  assert.deepEqual(mixed.edits.map((e) => e.formula), ["", "=B1@2,0", "", "=B1@4,0"], "a series run and a formula run beside each other");
  assert.deepEqual(values(mixed).filter(Boolean), ["2", "3"]);
});

test("runs of different kinds each continue on their own", () => {
  const result = fillColumn([cellOf("1"), cellOf("2"), cellOf("가")], "down", 6);
  assert.deepEqual(values(result), ["3", "4", "가", "5", "6", "가"]);
});

test("a source cell brings its format; the frozen-panes flag does not travel; an empty cell empties", () => {
  const bold = { fontWeight: "600", fontStyle: "", textDecoration: "", fontSize: "" };
  const frozen = { fontWeight: "600", fontStyle: "", textDecoration: "", fontSize: "", frozenPanes: true };
  const result = fillColumn([cellOf("1", { style: bold }), cellOf("", { style: frozen }), undefined], "down", 3, "copy");
  assert.equal(result.edits[0].style, bold);
  assert.deepEqual(result.edits[1].style, { fontWeight: "600", fontStyle: "", textDecoration: "", fontSize: "" });
  assert.deepEqual(result.edits[2], { ref: "C15", value: "", formula: "", style: null });
});

test("a fill past the limits is refused and writes nothing; odd numbers never turn into Infinity or NaN", () => {
  const big = fillColumn([cellOf("1"), cellOf("2")], "down", core.MAX_FILL_STEPS + 1);
  assert.deepEqual(big, { refused: "limit" });
  const wide = core.planFill({
    source: { r1: 1, c1: 1, r2: 1, c2: 11 },
    direction: "down",
    count: Math.floor(core.MAX_FILL_CELLS / 11) + 1,
    cellAt: () => cellOf("1"),
    shiftFormula: shift,
    refOf,
  });
  assert.deepEqual(wide, { refused: "limit" });
  const huge = fillColumn([cellOf("1e308"), cellOf("1.5e308")], "down", 2);
  assert.deepEqual(values(huge), ["1e308", "1.5e308"], "an overflowing series is copied");
  assert.deepEqual(values(fillColumn([cellOf("x" + "9".repeat(400))], "down", 2)), ["x" + "9".repeat(400), "x" + "9".repeat(400)]);
  assert.deepEqual(values(fillColumn([cellOf("x" + "9".repeat(15))], "down", 1)), ["x1000000000000000"]);
  assert.deepEqual(values(fillColumn([cellOf("Infinity"), cellOf("NaN")], "down", 2)), ["Infinity", "NaN"]);
  assert.deepEqual(fillColumn([cellOf("1")], "down", 0).edits, []);
  const ok = fillColumn([cellOf("1"), cellOf("2")], "down", 20000);
  assert.equal(ok.edits.length, 20000);
  assert.equal(ok.edits.at(-1).value, "20002");
});

test("the double click target follows the neighbouring column and stops above data in the source columns", () => {
  const data = new Map();
  const put = (r, c) => data.set(r + "," + c, true);
  for (let r = 1; r <= 8; r += 1) put(r, 1);
  put(1, 2);
  const has = (r, c) => data.has(r + "," + c);
  assert.deepEqual(core.detectFillEnd({ r1: 1, c1: 2, r2: 1, c2: 2 }, has, 1048576, 16384), { end: 8 }, "the column on the left decides");
  put(5, 2);
  assert.deepEqual(core.detectFillEnd({ r1: 1, c1: 2, r2: 1, c2: 2 }, has, 1048576, 16384), { end: 4 }, "it stops before data of the source column");
  assert.equal(core.detectFillEnd({ r1: 1, c1: 1, r2: 8, c2: 1 }, has, 1048576, 16384), null, "nothing beside the selection");
  data.delete("5,2");
  const right = new Map();
  for (let r = 1; r <= 4; r += 1) right.set(r + ",3", true);
  right.set("1,2", true);
  assert.deepEqual(core.detectFillEnd({ r1: 1, c1: 2, r2: 1, c2: 2 }, (r, c) => right.has(r + "," + c), 1048576, 16384), { end: 4 }, "else the column on the right");
  const tall = (r, c) => c === 1 && r <= core.MAX_FILL_STEPS * 3;
  assert.deepEqual(core.detectFillEnd({ r1: 1, c1: 2, r2: 1, c2: 2 }, tall, 1048576, 16384), { refused: "limit" });
  assert.equal(core.detectFillEnd({ r1: 1048576, c1: 2, r2: 1048576, c2: 2 }, () => true, 1048576, 16384), null, "at the last row");
});
