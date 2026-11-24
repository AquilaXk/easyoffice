import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The pure part of the point mode of the formula editor (`sheet-formula-point.js`). The F4 and caret cases follow the cases of
// the reference's own tests (`reference-absolute.spec.ts`, `formula-editor-helpers.spec.ts`), with the offsets counted in the
// text with its `=`.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
for (const name of ["sheet-refs.js", "sheet-ref-tokens.js", "sheet-table-totals.js", "sheet-formula-point.js"]) loadUiModule(ui(name), env);
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

// A table `매출` over B2:E7: the header on row 2, data rows 3..6, a totals row on 7, columns 부문, 1분기, 분기 합계, a.b.
const TABLE = { name: "매출", columns: ["부문", "1분기", "분기 합계", "a.b"], header: true, totals: true };
const BOX = { r1: 2, c1: 2, r2: 7, c2: 5 };
const rect = (r1, c1, r2, c2) => ({ r1, c1, r2, c2, axis: null });
const refOf = (rg, at = null) => P.structuredRef(rg, TABLE, BOX, at);

test("a whole data column is `Table[Column]`, several are `Table[[A]:[B]]`, and a name with a sign takes double brackets", () => {
  assert.equal(refOf(rect(3, 3, 6, 3)), "매출[1분기]");
  assert.equal(refOf(rect(3, 4, 6, 4)), "매출[분기 합계]");
  assert.equal(refOf(rect(3, 5, 6, 5)), "매출[[a.b]]");
  assert.equal(refOf(rect(3, 2, 6, 4)), "매출[[부문]:[분기 합계]]");
});

test("a cell of the formula's own row is `[@Column]` inside the table and `Table[[#This Row],[Column]]` outside it", () => {
  assert.equal(refOf(rect(4, 3, 4, 3), { r: 4, c: 5 }), "[@1분기]", "the formula is in the table, same row");
  assert.equal(refOf(rect(4, 4, 4, 4), { r: 4, c: 5 }), "[@[분기 합계]]");
  assert.equal(refOf(rect(4, 5, 4, 5), { r: 4, c: 2 }), "[@[a.b]]");
  assert.equal(refOf(rect(4, 3, 4, 4), { r: 4, c: 2 }), "[@[1분기]:[분기 합계]]");
  assert.equal(refOf(rect(4, 3, 4, 3), { r: 4, c: 9 }), "매출[[#This Row],[1분기]]", "the formula is in the same row, outside the table");
  assert.equal(refOf(rect(4, 3, 4, 4), { r: 4, c: 9 }), "매출[[#This Row],[1분기]:[분기 합계]]");
});

test("what is partial or outside the table stays an A1 reference", () => {
  assert.equal(refOf(rect(4, 3, 4, 3), { r: 5, c: 9 }), null, "another row");
  assert.equal(refOf(rect(4, 3, 4, 3)), null, "no formula cell on this sheet");
  assert.equal(refOf(rect(3, 3, 5, 3)), null, "part of a column");
  assert.equal(refOf(rect(2, 3, 5, 3)), null, "header plus partial data stays A1");
  assert.equal(refOf(rect(4, 3, 7, 3)), null, "partial data plus totals stays A1");
  assert.equal(refOf(rect(3, 1, 6, 3)), null, "a cell outside the table");
  assert.equal(refOf({ r1: 1, c1: 3, r2: 1048576, c2: 3, axis: "col" }), null, "a whole sheet column");
});

test("headers, totals, all and combined row items produce standard structured references", () => {
  assert.equal(refOf(rect(2, 3, 2, 3)), "매출[[#Headers],[1분기]]");
  assert.equal(refOf(rect(2, 3, 2, 4)), "매출[[#Headers],[1분기]:[분기 합계]]");
  assert.equal(refOf(rect(2, 2, 2, 5)), "매출[#Headers]");
  assert.equal(refOf(rect(7, 3, 7, 3)), "매출[[#Totals],[1분기]]");
  assert.equal(refOf(rect(7, 3, 7, 3), { r: 7, c: 2 }), "매출[[#Totals],[1분기]]", "totals cell pointed from totals row keeps #Totals");
  assert.equal(refOf(rect(7, 2, 7, 5)), "매출[#Totals]");
  assert.equal(refOf(rect(2, 3, 7, 3)), "매출[[#All],[1분기]]");
  assert.equal(refOf(rect(2, 2, 7, 5)), "매출[#All]");
  assert.equal(refOf(rect(2, 3, 6, 3)), "매출[[#Headers],[#Data],[1분기]]");
  assert.equal(refOf(rect(3, 3, 7, 3)), "매출[[#Data],[#Totals],[1분기]]");
  assert.equal(refOf(rect(3, 2, 6, 5)), "매출[#Data]");
  assert.equal(refOf(rect(4, 2, 4, 5), { r: 4, c: 3 }), "[@]");
  assert.equal(refOf(rect(4, 2, 4, 5), { r: 4, c: 9 }), "매출[#This Row]");
});

test("resolveStructured parses valid manual structured references and rejects invalid ones", () => {
  const esc = {
    name: "t_esc",
    columns: ["a.b", "@x", "a[1]", "it's", "p#q"],
    header: true,
    totals: true,
  };
  const tables = [
    { ...TABLE, range: "B2:E7" },
    { ...esc, range: "G2:K6" },
  ];
  const boxOf = (t) => {
    const [a, b] = t.range.split(":");
    const pa = /^([A-Z]+)(\d+)$/.exec(a);
    const pb = /^([A-Z]+)(\d+)$/.exec(b);
    const colNum = (s) => [...s].reduce((n, ch) => n * 26 + ch.charCodeAt(0) - 64, 0);
    return { r1: Number(pa[2]), c1: colNum(pa[1]), r2: Number(pb[2]), c2: colNum(pb[1]) };
  };
  const r = (tok, at = null) => P.resolveStructured(tok, tables, boxOf, at);

  assert.deepEqual(r("매출[[#Headers],[1분기]]"), { r1: 2, c1: 3, r2: 2, c2: 3, axis: null });
  assert.deepEqual(r("매출[#Headers]"), { r1: 2, c1: 2, r2: 2, c2: 5, axis: null });
  assert.deepEqual(r("매출[#Totals]"), { r1: 7, c1: 2, r2: 7, c2: 5, axis: null });
  assert.deepEqual(r("매출[#All]"), { r1: 2, c1: 2, r2: 7, c2: 5, axis: null });
  assert.deepEqual(r("매출[#Data]"), { r1: 3, c1: 2, r2: 6, c2: 5, axis: null });
  assert.deepEqual(r("매출[[#Headers],[#Data],[1분기]]"), { r1: 2, c1: 3, r2: 6, c2: 3, axis: null });
  assert.deepEqual(r("매출[[#Data],[#Totals],[a.b]:[1분기]]"), { r1: 3, c1: 3, r2: 7, c2: 5, axis: null });
  assert.deepEqual(r("[@[분기 합계]]", { r: 4, c: 2 }), { r1: 4, c1: 4, r2: 4, c2: 4, axis: null });
  assert.deepEqual(r("[@1분기]", { r: 4, c: 2 }), { r1: 4, c1: 3, r2: 4, c2: 3, axis: null });
  assert.deepEqual(r("[@]", { r: 4, c: 5 }), { r1: 4, c1: 2, r2: 4, c2: 5, axis: null });
  assert.deepEqual(r("매출[[#This Row],[1분기]]", { r: 4, c: 9 }), { r1: 4, c1: 3, r2: 4, c2: 3, axis: null });
  assert.deepEqual(r("t_esc[[@x]]"), { r1: 3, c1: 8, r2: 5, c2: 8, axis: null });
  assert.deepEqual(r("t_esc[[a'[1']]]"), { r1: 3, c1: 9, r2: 5, c2: 9, axis: null });
  assert.deepEqual(r("t_esc[it''s]"), { r1: 3, c1: 10, r2: 5, c2: 10, axis: null });
  assert.deepEqual(r("t_esc[p'#q]"), { r1: 3, c1: 11, r2: 5, c2: 11, axis: null });

  assert.equal(r("매출[[#Headers],[#Totals]]"), null);
  assert.equal(r("매출[#Nope]"), null);
  assert.equal(r("매출[없는열]"), null);
  assert.equal(r("[@1분기]", { r: 2, c: 3 }), null, "header row cannot resolve @");
  assert.equal(r("[@1분기]", { r: 4, c: 9 }), null, "unqualified @ requires caret inside table");
  assert.equal(r("[Book]"), null, "unqualified non-structured bracket ignored");
  assert.equal(P.resolveStructured("t[#Totals]", [{ ...TABLE, name: "t", range: "B2:E7", totals: false }], boxOf, null), null);
});

test("a table without a header or a totals row has its data from the first to the last row", () => {
  const bare = { name: "t", columns: ["x", "y"], header: false, totals: false };
  assert.equal(P.structuredRef(rect(2, 2, 7, 2), bare, BOX, null), "t[x]");
  assert.equal(P.structuredRef(rect(2, 2, 6, 2), bare, BOX, null), null);
});

test("a column name that is empty or missing gives no structured reference", () => {
  assert.equal(P.structuredRef(rect(3, 3, 6, 3), { ...TABLE, columns: ["부문", "", "c", "d"] }, BOX, null), null);
  assert.equal(P.structuredRef(rect(3, 5, 6, 5), { ...TABLE, columns: ["부문", "b"] }, BOX, null), null);
});

test("a structured reference is a span of the formula: the point replaces it, the colours count it, F4 and refSpans skip it", () => {
  const text = "=SUM(매출[1분기])+B3";
  const spans = P.colorSpans(text);
  assert.deepEqual(spans.map((s) => s.token), ["매출[1분기]", "B3"]);
  assert.equal(spans[0].ref, null);
  assert.deepEqual(P.refSpans(text).map((s) => s.token), ["B3"]);
  const ctx = P.pointContext(text, "=SUM(매출[1분기]".length);
  assert.deepEqual({ mode: ctx.mode, start: ctx.start, end: ctx.end }, { mode: "replace", start: 5, end: 12 });
  assert.equal(P.applyRef(text, ctx, "C3").text, "=SUM(C3)+B3");
  assert.equal(P.cycleRef(text, "=SUM(매출[1분기".length), null, "F4 has nothing to cycle in it");
  const unqualified = P.colorSpans("=[@Q1]*2");
  assert.deepEqual(unqualified.map((s) => s.token), ["[@Q1]"]);
  assert.equal(P.colorSpans('="매출[x]"').length, 0, "not inside a text literal");
});

test("3D sheet spans tokenize, color, quote, extract endpoints, and cycle F4 cleanly", () => {
  const quote = (n) => (/^\w+$/.test(n) && !/^\d/.test(n) ? n : "'" + n.replaceAll("'", "''") + "'");
  assert.deepEqual(P.sheetEndpoints("Sheet1:Sheet3!"), { start: "Sheet1", end: "Sheet3", whole: null });
  assert.deepEqual(P.sheetEndpoints("'Sheet 1:Sheet 3'!"), { start: "Sheet 1", end: "Sheet 3", whole: "Sheet 1:Sheet 3" });
  assert.deepEqual(P.sheetEndpoints("'Sheet 1':'Sheet 3'!"), { start: "Sheet 1", end: "Sheet 3", whole: null });
  assert.equal(P.quoteSheetSpan("Sheet1", "Sheet3", quote), "Sheet1:Sheet3!");
  assert.equal(P.quoteSheetSpan("1월", "3월", quote), "'1월:3월'!");
  assert.equal(P.quoteSheetSpan("Sheet2", "Sheet2", quote), "Sheet2!");

  const spans = P.refSpans("=SUM(Sheet1:Sheet3!A1:B2)+'1월:3월'!$C$4+'A 1':'B 2'!D5");
  assert.deepEqual(
    spans.map((s) => ({ token: s.token, sheet: s.sheet, ref: s.ref })),
    [
      { token: "Sheet1:Sheet3!A1:B2", sheet: "Sheet1:Sheet3!", ref: "A1:B2" },
      { token: "'1월:3월'!$C$4", sheet: "'1월:3월'!", ref: "$C$4" },
      { token: "'A 1':'B 2'!D5", sheet: "'A 1':'B 2'!", ref: "D5" },
    ]
  );

  // F4 at the range colon of a 3D reference toggles both range corners without misreading the sheet span colon
  const f = "=SUM(Sheet1:Sheet3!A1:B2)";
  const toggled = f4(f, "=SUM(Sheet1:Sheet3!A1".length);
  assert.equal(toggled.text, "=SUM(Sheet1:Sheet3!$A$1:$B$2)");
  assert.equal(toggled.caret, "=SUM(Sheet1:Sheet3!$A$1".length);
  assert.equal(f4("='1월:3월'!B2").text, "='1월:3월'!$B$2");
});
