import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The pure part of the point mode of the formula editor (`sheet-formula-point.js`). The F4 and caret cases follow the cases of
// the reference's own tests (`reference-absolute.spec.ts`, `formula-editor-helpers.spec.ts`), with the offsets counted in the
// text with its `=`.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
for (const name of ["sheet-refs.js", "sheet-ref-tokens.js", "sheet-formula-point.js"]) loadUiModule(ui(name), env);
const P = env.__EASYOFFICE_SHEET_FORMULA_POINT__;

const f4 = (text, caret = text.length) => P.cycleRef(text, caret);

test("F4 cycles a reference through A1, $A$1, A$1, $A1 and back", () => {
  let text = "=A1";
  let caret = text.length;
  for (const expected of ["=$A$1", "=A$1", "=$A1", "=A1"]) {
    const next = f4(text, caret);
    assert.equal(next.text, expected);
    ({ text, caret } = next);
  }
});

test("F4 on a reference of another sheet cycles the cell part and keeps the caret in it", () => {
  assert.deepEqual(f4("=Sheet2!B5", "=Sheet2!B".length), { text: "=Sheet2!$B$5", start: 1, end: 12, caret: "=Sheet2!B".length });
  assert.equal(f4("='My Sheet'!B5").text, "='My Sheet'!$B$5");
});

test("F4 on a range cycles the end the caret is on, both next to the colon", () => {
  assert.equal(f4("=A1:B2", "=A".length).text, "=$A$1:B2");
  assert.equal(f4("=A1:B2", "=A1:".length + 1).text, "=A1:$B$2");
  const before = f4("=A1:B2", "=A1".length);
  const after = f4("=A1:B2", "=A1:".length);
  assert.equal(before.text, "=$A$1:$B$2");
  assert.equal(after.text, "=$A$1:$B$2");
  assert.equal(before.caret, "=$A$1".length);
  assert.equal(after.caret, "=$A$1:".length);
});

test("F4 keeps cycling both ends of a range at the colon, as the task lists A1:B2 in the same order", () => {
  let text = "=A1:B2";
  let caret = "=A1".length;
  for (const expected of ["=$A$1:$B$2", "=A$1:B$2", "=$A1:$B2", "=A1:B2"]) {
    const next = f4(text, caret);
    assert.equal(next.text, expected);
    ({ text, caret } = next);
  }
});

test("F4 on a whole column or row toggles the fixing of both ends", () => {
  assert.equal(f4("=A:A", "=A".length).text, "=$A:$A");
  assert.equal(f4("=$A:$A", "=$A".length).text, "=A:A");
  assert.equal(f4("=1:1", "=1".length).text, "=$1:$1");
  assert.equal(f4("=A:C").text, "=$A:$C");
});

test("F4 at the edges of a formula and with letters in lower case", () => {
  assert.equal(f4("=A1+SUM(B2)", "=A1".length).text, "=$A$1+SUM(B2)");
  assert.equal(f4("=SUM(1)+B2").text, "=SUM(1)+$B$2");
  assert.equal(f4("=a1").text, "=$A$1");
});

test("F4 does nothing off a cell reference", () => {
  assert.equal(f4("=SUM(1, 2)", "=SUM(".length), null);
  assert.equal(f4('="A1"', '="'.length + 1), null);
  assert.equal(f4("=namedRange", "=named".length), null);
  assert.equal(f4("=LOG10(2)", "=LOG1".length), null);
  assert.equal(f4(""), null);
  assert.equal(f4("=1", 1), null);
  assert.equal(f4("A1", 2), null, "text that is not a formula");
  assert.equal(f4("=A1", 0), null, "the caret before the =");
});

test("a reference is pointed after =, an operator, ( and , and nowhere else", () => {
  for (const text of ["=", "=1+", "=1-", "=2*", "=2/", "=2^", '="a"&', "=A1=", "=A1<", "=A1>", "=A1<>", "=A1>=", "=SUM(", "=SUM(A1,", "=SUM(A1, ", "=-"]) {
    assert.equal(P.pointContext(text, text.length).mode, "add", text);
  }
  for (const text of ["=1", "=SUM", "=SUM(1)", "=A1+B", "=A1 ", "=1%", "=1:", "A1+", ""]) {
    const ctx = P.pointContext(text, text.length);
    assert.notEqual(ctx.mode, "add", text);
  }
});

test("a reference is pointed in front of an operator or a bracket, not in front of a name or a number", () => {
  assert.equal(P.pointContext("=SUM(,2)", "=SUM(".length).mode, "add");
  assert.equal(P.pointContext("=+B2", "=".length).mode, "add");
  assert.equal(P.pointContext("=SUM(2)", "=SUM(".length).mode, "none");
  assert.equal(P.pointContext("=SUM(A1)", "=".length).mode, "none");
});

test("no reference is pointed inside a text or a quoted sheet name", () => {
  assert.equal(P.pointContext('="a+', '="a+'.length).mode, "none");
  assert.equal(P.pointContext('="a+"&', '="a+"&'.length).mode, "add");
  assert.equal(P.pointContext("='My+", "='My+".length).mode, "none");
});

test("the caret in or at the end of a reference replaces it, at its start does not", () => {
  const text = "=SUM(A1:B2+";
  assert.equal(P.pointContext(text, "=SUM(".length).mode, "none", "the reference writes before an operand only where none stands");
  const inside = P.pointContext(text, "=SUM(A1:B".length);
  assert.equal(inside.mode, "replace");
  assert.deepEqual([inside.start, inside.end], [5, 10]);
  assert.equal(P.pointContext(text, "=SUM(A1:B2".length).mode, "replace");
  assert.equal(P.pointContext(text, text.length).mode, "add");
  assert.equal(P.pointContext("=SUM(A1)", "=SUM(".length).mode, "none", "the start of a reference sits behind an operand");
});

test("a selected text is replaced by the reference when it follows an operator, and a selected reference is replaced", () => {
  const text = "=1+xx*2";
  const ctx = P.pointContext(text, "=1+".length, "=1+xx".length);
  assert.equal(ctx.mode, "add");
  assert.deepEqual([ctx.start, ctx.end], [3, 5]);
  assert.equal(P.applyRef(text, ctx, "C3").text, "=1+C3*2");
  const hit = P.pointContext("=A1+B2", 1, 3);
  assert.equal(hit.mode, "replace");
  assert.deepEqual([hit.start, hit.end], [1, 3]);
});

test("applyRef writes the reference and puts the caret after it", () => {
  const add = P.pointContext("=SUM(,2)", "=SUM(".length);
  assert.deepEqual(P.applyRef("=SUM(,2)", add, "B2"), { text: "=SUM(B2,2)", start: 5, end: 7, caret: 7 });
  const replace = P.pointContext("=A1+B2", "=A1".length);
  assert.deepEqual(P.applyRef("=A1+B2", replace, "C3:D4"), { text: "=C3:D4+B2", start: 1, end: 6, caret: 6 });
});

test("rangeText writes cells, ranges, whole columns and whole rows", () => {
  assert.equal(P.rangeText({ r1: 2, c1: 2, r2: 2, c2: 2 }), "B2");
  assert.equal(P.rangeText({ r1: 2, c1: 2, r2: 4, c2: 4 }), "B2:D4");
  assert.equal(P.rangeText({ r1: 1, c1: 2, r2: 1048576, c2: 4 }, "col"), "B:D");
  assert.equal(P.rangeText({ r1: 2, c1: 1, r2: 4, c2: 16384 }, "row"), "2:4");
});

test("rangeOf reads the cells of a reference text in any order and fixing", () => {
  assert.deepEqual(P.rangeOf("$B$2:A1"), { r1: 1, c1: 1, r2: 2, c2: 2, axis: null });
  assert.deepEqual(P.rangeOf("c3"), { r1: 3, c1: 3, r2: 3, c2: 3, axis: null });
  assert.deepEqual(P.rangeOf("B:A"), { r1: 1, c1: 1, r2: 1048576, c2: 2, axis: "col" });
  assert.deepEqual(P.rangeOf("3:2"), { r1: 2, c1: 1, r2: 3, c2: 16384, axis: "row" });
});

test("sheetOf reads a plain and a quoted sheet prefix", () => {
  assert.equal(P.sheetOf("Sheet2!"), "Sheet2");
  assert.equal(P.sheetOf("'My ''Sheet'!"), "My 'Sheet");
});

test("references get the colours of the palette in the order they first appear and the same text keeps its colour", () => {
  const spans = P.refSpans("=A1+B2+A1+Sheet2!A1+C3+D4+E5+F6+G7");
  assert.deepEqual(spans.map((s) => s.token), ["A1", "B2", "A1", "Sheet2!A1", "C3", "D4", "E5", "F6", "G7"]);
  assert.deepEqual(P.colorIndexes(spans, 6), [0, 1, 0, 2, 3, 4, 5, 0, 1]);
});

test("refSpans skips text literals, structured references, names, and a literal the text leaves open", () => {
  assert.deepEqual(P.refSpans('="A1"&B2').map((s) => s.token), ["B2"]);
  assert.deepEqual(P.refSpans("=Table1[[#This Row],[Q1]]+LOG10(A1)+my_name1").map((s) => s.token), ["A1"]);
  assert.deepEqual(P.refSpans('=A1&"B2').map((s) => s.token), ["A1"], "an open literal hides what follows");
  assert.deepEqual(P.refSpans("A1+B2"), [], "text that is not a formula");
  const [span] = P.refSpans("=SUM('My Sheet'!$A$1:B2)");
  assert.deepEqual({ start: span.start, end: span.end, sheet: span.sheet, ref: span.ref }, { start: 5, end: 23, sheet: "'My Sheet'!", ref: "$A$1:B2" });
});
