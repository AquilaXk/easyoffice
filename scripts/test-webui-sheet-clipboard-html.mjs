import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// Cells pasted from the HTML of a spreadsheet program or another table: merges, formats, numbers, text marks, empty
// cells, the bounds. The fixtures are written after the layout such programs export (a style sheet with
// classes, `x:` attributes on the cells). The markers are the identifiers `scripts/oss-names.mjs` lists, in lower case.

const NBSP = "\u00a0";
const NS_O = "urn:schemas-microsoft-com:office:office";
const NS_X = "urn:schemas-microsoft-com:office:excel";

const STYLE = `
td { padding-top:1px; color:windowtext; font-size:11.0pt; font-weight:400; font-family:"맑은 고딕", monospace; mso-number-format:General; text-align:general; vertical-align:bottom; border:none; white-space:nowrap; }
.xl65 { font-weight:700; text-align:center; background:yellow; }
.xl66 { mso-number-format:"0\\.00"; }
.xl67 { mso-number-format:"0\\.0%"; }
.xl68 { border:.5pt solid windowtext; white-space:normal; }
.xl69 { color:#FF0000; font-style:italic; text-decoration:underline; font-size:14.0pt; }
`;

const SHEET_HTML = `<html xmlns:o="${NS_O}" xmlns:x="${NS_X}"><head><meta name=ProgId content=excel.sheet><style>${STYLE}</style></head><body>
<table border=0 cellpadding=0 cellspacing=0 style='border-collapse:collapse;table-layout:fixed'>
<col width=87 style='width:65pt'><col width=64 span=2 style='width:48pt'>
<tr height=20 style='height:15.0pt'><td class=xl65>이름</td><td class=xl65>점수</td><td class=xl65>비율</td></tr>
<tr><td>가나</td><td class=xl66 align=right x:num="3.14159">3.14</td><td class=xl67 align=right x:num="0.125">12.5%</td></tr>
<tr><td colspan=2 class=xl68 style='mso-ignore:colspan'>넘침 글</td><td></td></tr>
<tr><td colspan=2 rowspan=2 class=xl68>병합</td><td class=xl69>x</td></tr>
<tr><td>y</td></tr>
<tr><td x:str="=1+1">=1+1</td><td x:bool="TRUE">TRUE</td><td x:err="#DIV/0!">#DIV/0!</td></tr>
<tr><td>${NBSP}</td><td class=xl65>${NBSP}</td><td>줄1<br style='mso-data-placement:same-cell'>줄2</td></tr>
<tr><td x:num="3" x:fmla="=SUM(A1:A2)">3</td><td><span style='mso-spacerun:yes'>${NBSP}${NBSP}</span>7</td><td>  가운데   공백
 줄바꿈  </td></tr>
</table></body></html>`;

const MODEL = {
  sheets: [{ name: "표", cells: [{ ref: "A1", raw: "옛값" }, { ref: "B1", raw: "옛값2", style: { bold: true } }, { ref: "C2", raw: "남음" }, { ref: "F1", raw: "옛F" }] }],
};

async function open(options) {
  const h = await createWorkspaceHarness(options);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(h.document, makeFixtureFile("표.xlsx", MODEL));
  await flushAsyncTurns(12);
  const win = h.document.defaultView;
  // The page parses with DOMParser; the test double builds the tree with the harness's own parser.
  win.DOMParser = class {
    parseFromString(html) {
      const root = h.document.createElement("div");
      root.innerHTML = html;
      return { documentElement: root };
    }
  };
  return h;
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const feedback = (d) => d.getElementById("status-command-feedback").textContent;

async function select(d, from, moves = []) {
  const start = cell(d, from);
  start.dispatchEvent({ type: "mousedown", target: start, button: 0, bubbles: true });
  start.dispatchEvent({ type: "mouseup", target: start, bubbles: true });
  for (const [k, n] of moves) for (let i = 0; i < n; i += 1) d.activeElement.dispatchEvent({ type: "keydown", key: k, target: d.activeElement, shiftKey: true });
  await flushAsyncTurns(2);
}

async function pasteHtml(d, html, text = "") {
  const target = d.activeElement;
  const data = { "text/html": html, "text/plain": text };
  target.dispatchEvent({ type: "paste", target, clipboardData: { getData: (t) => data[t] || "", setData() {} } });
  await flushAsyncTurns(20);
}

function parse(h, html) {
  const win = h.document.defaultView;
  return win.__EASYOFFICE_SHEET_CLIP_HTML__.parseClipboardHtml(html, { styleFromPayload: win.__EASYOFFICE_SHEET_STYLE_CODEC__.styleFromPayload });
}

test("the markers of a spreadsheet program's HTML are found, a plain table is not one", async () => {
  const h = await open();
  const api = h.document.defaultView.__EASYOFFICE_SHEET_CLIP_HTML__;
  assert.equal(api.htmlIsFromSpreadsheet(SHEET_HTML), true);
  assert.equal(api.htmlIsFromSpreadsheet("<table><tr><td class=xl65>a</td></tr></table>"), true, "the class of a cell, unquoted");
  assert.equal(api.htmlIsFromSpreadsheet('<table><tr><td class="xl7">a</td></tr></table>'), true, "and quoted");
  assert.equal(api.htmlIsFromSpreadsheet('<meta name=ProgId content="excel.sheet">'), true);
  assert.equal(api.htmlIsFromSpreadsheet("<table><tr><td>a</td></tr></table>"), false);
  assert.equal(api.htmlIsFromSpreadsheet(""), false);
});

test("the table becomes a block: values, the program's own numbers, text marks, empty cells and line breaks", async () => {
  const h = await open();
  const { block } = parse(h, SHEET_HTML);
  assert.equal(block.rich, true);
  assert.equal(block.rows, null);
  assert.equal(block.cols, 3);
  assert.equal(block.cells.length, 8);
  const v = (i, j) => block.cells[i][j]?.value;
  assert.deepEqual([v(0, 0), v(0, 1), v(0, 2)], ["이름", "점수", "비율"]);
  assert.deepEqual([v(1, 0), v(1, 1), v(1, 2)], ["가나", "3.14159", "0.125"], "x:num is the value, the shown text is the format's job");
  assert.deepEqual(block.cells[5].map((c) => [c.value, c.t]), [["=1+1", "s"], ["TRUE", undefined], ["#DIV/0!", undefined]], "a text mark pins a text that reads like a formula; a logical and an error keep their text");
  assert.equal(block.cells[6][0], null, "a cell of only a no-break space is empty");
  assert.equal(block.cells[2][2], null, "an empty cell");
  assert.ok(block.cells[6][1] && block.cells[6][1].value === "" && block.cells[6][1].style, "an empty cell with a fill keeps the fill");
  assert.equal(v(6, 2), "줄1\n줄2", "a break inside a cell");
  assert.equal(v(7, 0), "3", "a formula attribute gives its value only: its references belong to the copied place");
  assert.equal(block.cells[7][0].formula, "");
  assert.equal(v(7, 1), "7", "the filler spaces of the program's formats go");
  assert.equal(v(7, 2), "가운데 공백 줄바꿈", "white space collapses as in a browser");
});

test("merged ranges come as offsets, the covered cells keep the style, and text overflow is not a merge", async () => {
  const h = await open();
  const { block } = parse(h, SHEET_HTML);
  assert.deepEqual(block.merges, [{ r1: 3, c1: 0, r2: 4, c2: 1 }], "colspan=2 rowspan=2 only; mso-ignore:colspan is overflow");
  assert.equal(block.cells[3][0].value, "병합");
  assert.equal(block.cells[3][1].value, "");
  assert.equal(block.cells[4][1].style, block.cells[3][0].style, "a covered cell wears the merged cell's style");
  assert.equal(block.cells[4][2].value, "y", "the cell after a rowspan starts in the first free column");
  assert.equal(block.cells[3][2].value, "x");
});

test("fonts, colours, alignment, wrapping, borders and number formats come from the classes and the tag rule", async () => {
  const h = await open();
  const { block } = parse(h, SHEET_HTML);
  const head = block.cells[0][0].style;
  assert.equal(head.fontWeight, "600");
  assert.equal(head.textAlign, "center");
  assert.equal(head.fill, "ffff00", "a named colour");
  assert.equal(head.fontFamily, undefined, "the tag rule's font is the source's normal font: not carried");
  assert.equal(head.fontSize, "", "no size of its own");
  assert.equal(head.fontColor, undefined, "the automatic text colour is no colour");
  assert.equal(block.cells[1][0].style, undefined, "a cell with only the tag rule's look has no style: 400 is not bold, general is no alignment, bottom is the default");
  assert.equal(block.cells[1][1].style.numFmt, "0.00");
  assert.equal(block.cells[1][2].style.numFmt, "0.0%");
  const boxed = block.cells[3][0].style;
  assert.deepEqual({ ...boxed.border }, { r: "thin:000000", b: "thin:000000", l: "thin:000000" }, ".5pt solid is thin; the top edge is the shared one of the row above, which has none here, so it stays");
  assert.equal(boxed.wrap, true, "white-space:normal wraps");
  const fancy = block.cells[3][2].style;
  assert.equal(fancy.fontColor, "ff0000");
  assert.equal(fancy.fontStyle, "italic");
  assert.equal(fancy.textDecoration, "underline");
  assert.equal(fancy.fontSize, "18.67px", "14pt in the sheet's px: a size the cell's own class sets is carried");
  const named = parse(h, "<table><tr><td style='font-family:\"Noto Sans\", Arial;font-size:9pt'>f</td></tr></table>").block.cells[0][0].style;
  assert.equal(named.fontFamily, "Noto Sans", "the first family of the list, without quotes");
  assert.equal(named.fontSize, "12px");
  assert.equal(block.cells[1][1].style.textAlign, undefined, "align=right is an attribute the sheet does not need: numbers are right-aligned already");
});

test("a border two neighbours share is kept once; thick, dashed and double lines map to the sheet's line kinds", async () => {
  const h = await open();
  const html = `<style>.a{border-bottom:1.5pt solid #ff0000}.b{border-top:.5pt solid #00ff00;border-left:1px dashed black}.c{border:2pt double blue}</style>
    <table><tr><td class=a>1</td><td class=c>3</td></tr><tr><td class=b>2</td><td>4</td></tr></table>`;
  const { block } = parse(h, html);
  assert.equal(block.cells[0][0].style.border.b, "thick:ff0000");
  assert.equal(block.cells[1][0].style.border.t, undefined, "the border above is the shared edge: the lower cell drops its own");
  assert.equal(block.cells[1][0].style.border.l, "dashed:000000");
  assert.deepEqual({ ...block.cells[0][1].style.border }, { t: "double:0000ff", r: "double:0000ff", b: "double:0000ff", l: "double:0000ff" });
});

test("the number format codes of the program are decoded; a named format maps to its code; a default one is none", async () => {
  const h = await open();
  const api = h.document.defaultView.__EASYOFFICE_SHEET_CLIP_HTML__;
  assert.equal(api.decodeMsoNumberFormat('_\\(* \\#\\,\\#\\#0_\\)\\;_\\(* \\\\\\(\\#\\,\\#\\#0\\\\\\)\\;_\\(* \\0022-\\0022??_\\)'), '_(* #,##0_);_(* \\(#,##0\\);_(* "-"??_)');
  const html = `<style>.p{mso-number-format:Percent}.f{mso-number-format:Fixed}.g{mso-number-format:General}.t{mso-number-format:"\\@"}</style>
    <table><tr><td class=p x:num="0.5">50%</td><td class=f x:num="2">2.00</td><td class=g>1</td><td class=t>007</td></tr></table>`;
  const { block } = parse(h, html);
  assert.deepEqual(block.cells[0].map((c) => c.style?.numFmt), ["0%", "0.00", undefined, "@"]);
});

test("a style sheet in comment marks, selector lists, at-rules and an id rule is read; the inline style wins", async () => {
  const h = await open();
  const api = h.document.defaultView.__EASYOFFICE_SHEET_CLIP_HTML__;
  const rules = api.readStyleRules(["<!-- @page { margin:1in } td, th { color:#00ff00 } /* c */ .x { font-weight:700; } #y { background:#0000ff } -->"]);
  assert.deepEqual([...rules.keys()], ["td", "th", ".x", "#y"]);
  assert.equal(rules.get("th").color, "#00ff00");
  const { block } = parse(h, "<style>.x{font-weight:700;color:#ff0000}#y{background:#0000ff}</style><table><tr><td class=x style='color:#00ff00'>1</td><td id=y>2</td></tr></table>");
  assert.equal(block.cells[0][0].style.fontWeight, "600");
  assert.equal(block.cells[0][0].style.fontColor, "00ff00", "inline over class");
  assert.equal(block.cells[0][1].style.fill, "0000ff");
});

test("column widths come from the col elements, spans included", async () => {
  const h = await open();
  assert.deepEqual(parse(h, SHEET_HTML).block.widths, [87, 64, 64], "the width attribute, a span of two");
  assert.deepEqual(parse(h, "<table><col style='width:48pt'><tr><td>a</td></tr></table>").block.widths, [64], "or the style, 48pt is 64px");
});

test("two tables stack, a table in a cell is part of that cell, HTML without a cell gives no block", async () => {
  const h = await open();
  const { block } = parse(h, "<table><tr><td>a</td></tr></table><p>x</p><table><tr><td>b</td><td><table><tr><td>inner</td></tr></table></td></tr></table>");
  assert.deepEqual(block.cells.map((r) => r.map((c) => c?.value ?? null)), [["a", null], ["b", "inner"]]);
  assert.equal(parse(h, "<p>just text</p>").block, null);
  assert.equal(parse(h, "").block, null);
});

test("hostile HTML: markup in a cell is text only, a prototype name is a key, and scripts and images add nothing", async () => {
  const h = await open();
  const html = `<style>.__proto__{font-weight:700} __proto__{color:#ff0000} .constructor{background:#00ff00}</style>
    <table><tr><td class="__proto__ constructor" style="__proto__:x;constructor:y">a<script>alert(1)</script><img src=x onerror=alert(1)>b</td></tr></table>`;
  const { block } = parse(h, html);
  assert.equal(block.cells[0][0].value, "ab");
  assert.equal(block.cells[0][0].style.fontWeight, "600");
  assert.equal(({}).polluted, undefined);
  assert.equal(Object.prototype.fontWeight, undefined);
});

test("bounds: HTML over the size, a merge over the cell count, and far-apart cells whose rectangle is too big are refused whole", async () => {
  const h = await open();
  const api = h.document.defaultView.__EASYOFFICE_SHEET_CLIP_HTML__;
  assert.deepEqual(parse(h, "<table><tr><td>" + "x".repeat(api.LIMITS.HTML_CHARS) + "</td></tr></table>"), { refused: "size" });
  assert.deepEqual(parse(h, "<table><tr><td rowspan=100000 colspan=100>a</td></tr></table>"), { refused: "cells" });
  assert.deepEqual(parse(h, "<table><tr><td colspan=16384>a</td></tr></table>" + "<table>" + "<tr><td>b</td></tr>".repeat(100) + "</table>").refused, "cells", "16384 columns by 101 rows is a dense 1.6 million cells");
  const clamped = parse(h, "<table><tr><td colspan=99999>a</td></tr></table>").block;
  assert.equal(clamped.cols, 16384, "a span is cut at the grid's edge");
  const text = parse(h, "<table><tr><td>" + "y".repeat(40000) + "</td></tr></table>").block.cells[0][0].value;
  assert.equal(text.length, api.LIMITS.CELL_CHARS, "a cell holds 32,767 characters");
});

test("deeply nested markup in a cell does not overflow the stack", async () => {
  const h = await open();
  const deep = "<span>".repeat(3000) + "깊음" + "</span>".repeat(3000);
  assert.equal(parse(h, `<table><tr><td>${deep}</td></tr></table>`).block.cells[0][0].value, "깊음");
});

// ---- the page --------------------------------------------------------------------------------------

test("Ctrl+V of a table writes values, formats and merges in one undo step and replaces what was there, empty cells included", async () => {
  const h = await open();
  const d = h.document;
  await select(d, "A1");
  await pasteHtml(d, SHEET_HTML, "이름\t점수\t비율");
  assert.equal(cell(d, "A1").textContent, "이름");
  assert.equal(cell(d, "B1").textContent, "점수", "the old B1 is replaced, and its bold goes with its own style");
  assert.equal(cell(d, "A1").style.fontWeight, "600");
  assert.equal(cell(d, "B2").dataset.raw, "3.14159");
  assert.equal(cell(d, "B2").textContent, "3.14", "the format shows two decimals");
  assert.equal(cell(d, "C2").dataset.raw, "0.125");
  assert.equal(cell(d, "C2").textContent, "12.5%");
  assert.equal(cell(d, "C3").textContent, "", "the empty cell of the source empties the cell it lands on");
  assert.equal(cell(d, "A4").textContent, "병합");
  assert.deepEqual(d.defaultView.__EASYOFFICE_SHEET_LAYOUT__.mergesIn("표", 1, 1, 9, 3), [{ r1: 4, c1: 1, r2: 5, c2: 2 }]);
  assert.equal(cell(d, "A6").textContent, "=1+1", "text that reads like a formula stays text when the source marks it text");
  assert.equal(cell(d, "A6").dataset.formula ?? "", "");
  assert.equal(cell(d, "F1").textContent, "옛F", "cells outside the block are left alone");
  assert.equal(feedback(d), "붙여넣기: 24칸");
  h.userClick(studioItem(d, "studio-undo"));
  await flushAsyncTurns(12);
  assert.equal(cell(d, "A1").textContent, "옛값");
  assert.equal(cell(d, "B1").style.fontWeight, "600", "the old format comes back");
  assert.deepEqual(d.defaultView.__EASYOFFICE_SHEET_LAYOUT__.mergesIn("표", 1, 1, 9, 3), []);
});

test("the table is pasted at the selection and a program's HTML wins over a stale copy of the app that has the same text", async () => {
  const h = await open();
  const d = h.document;
  await select(d, "B1");
  const store = {};
  d.activeElement.dispatchEvent({ type: "copy", target: d.activeElement, clipboardData: { setData: (t, v) => (store[t] = v), getData: (t) => store[t] || "" } });
  assert.equal(store["text/plain"], "옛값2");
  await select(d, "E5");
  await pasteHtml(d, `<meta name=ProgId content=excel.sheet><table><tr><td class=xl65 style='font-weight:700'>옛값2</td></tr></table>`, "옛값2");
  assert.equal(cell(d, "E5").textContent, "옛값2");
  assert.equal(cell(d, "E5").style.fontWeight, "600");
  await select(d, "E7");
  await pasteHtml(d, "<table><tr><td>웹</td></tr></table>", "웹");
  assert.equal(cell(d, "E7").textContent, "웹");
});

test("HTML over the bound pastes the text and says so; a clipboard without a table pastes the text as before", async () => {
  const h = await open();
  const d = h.document;
  await select(d, "E1");
  await pasteHtml(d, "<table><tr><td rowspan=100000 colspan=100>a</td></tr></table>", "글자만");
  assert.equal(cell(d, "E1").textContent, "글자만");
  assert.equal(feedback(d), "붙여넣기 (칸이 많아 서식 없이 글자만): 1칸");
  await select(d, "E3");
  await pasteHtml(d, "<p>문단</p>", "문단");
  assert.equal(cell(d, "E3").textContent, "문단");
});

test("골라 붙이기 of a table: 값 leaves the formats, 서식 leaves the content", async () => {
  const h = await open();
  const d = h.document;
  const win = d.defaultView;
  await select(d, "E1");
  const rich = parse(h, SHEET_HTML).block;
  const clip = win.__EASYOFFICE_SHEET_PASTE_CORE__;
  const plan = (mode) => clip.planPaste({ snap: rich, mode, transpose: false, target: { r1: 1, c1: 5, r2: 1, c2: 5 }, getEntry: () => ({ value: "그대로", formula: "" }), shiftFormula: (f) => f, isFormulaStr: () => false, grid: { MAX_ROWS: 1048576, MAX_COLS: 16384, DEFAULT_COL_W: 96, refOf: (r, c) => "R" + r + "C" + c } });
  assert.ok(plan("values").edits.every((e) => !("style" in e)));
  assert.ok(plan("formats").edits.every((e) => e.value === "그대로" && "style" in e));
  assert.deepEqual(plan("widths").widths, { 5: 87, 6: 64, 7: 64 });
});

// ---- the toolbar and menu paste reads the system clipboard's HTML (`navigator.clipboard.read()`) ------------------

const item = (types) => ({ types: Object.keys(types), getType: async (t) => ({ size: types[t].length, text: async () => types[t] }) });
const toolbarPaste = async (h) => {
  h.userClick(h.document.querySelector('.tb-btn[data-studio-cmd="clip-paste"]'));
  await flushAsyncTurns(20);
};

test("the toolbar paste takes the table of the clipboard's HTML with its formats, as a paste event does", async () => {
  const clipboard = { read: async () => [item({ "text/html": SHEET_HTML, "text/plain": "이름\t점수\t비율" })], readText: async () => "이름\t점수\t비율" };
  const h = await open({ clipboard });
  const d = h.document;
  await select(d, "E1");
  await toolbarPaste(h);
  assert.equal(cell(d, "E1").textContent, "이름");
  assert.equal(cell(d, "E1").style.fontWeight, "600", "the bold of the table came with it");
  assert.equal(cell(d, "E4").textContent, "병합", "and the merged cell");
  assert.ok(h.host.__EASYOFFICE_SHEET_LAYOUT__.mergeAt("표", 4, 5), "with its merge");
});

test("a clipboard that refuses the read, has no read, or holds no table pastes the text as before", async () => {
  const denied = await open({ clipboard: { read: async () => { throw new Error("NotAllowedError"); }, readText: async () => "글자만" } });
  await select(denied.document, "E1");
  await toolbarPaste(denied);
  assert.equal(cell(denied.document, "E1").textContent, "글자만", "no permission: the text");

  const noRead = await open({ clipboard: { readText: async () => "읽기만" } });
  await select(noRead.document, "E1");
  await toolbarPaste(noRead);
  assert.equal(cell(noRead.document, "E1").textContent, "읽기만", "no read(): the text");

  const plainOnly = await open({ clipboard: { read: async () => [item({ "text/plain": "그냥" })], readText: async () => "그냥" } });
  await select(plainOnly.document, "E1");
  await toolbarPaste(plainOnly);
  assert.equal(cell(plainOnly.document, "E1").textContent, "그냥", "text only");

  const para = await open({ clipboard: { read: async () => [item({ "text/html": "<p>문단</p>", "text/plain": "문단" })], readText: async () => "문단" } });
  await select(para.document, "E1");
  await toolbarPaste(para);
  assert.equal(cell(para.document, "E1").textContent, "문단", "HTML with no table: the text");
});

test("a table over the bounds pastes the text and says so; a clipboard of an image does nothing", async () => {
  const over = await open({ clipboard: { read: async () => [item({ "text/html": "<table><tr><td rowspan=100000 colspan=100>a</td></tr></table>", "text/plain": "글자만" })], readText: async () => "글자만" } });
  await select(over.document, "E1");
  await toolbarPaste(over);
  assert.equal(cell(over.document, "E1").textContent, "글자만", "the bound refuses the table, not the text");

  const big = await open({ clipboard: { read: async () => [{ types: ["text/html", "text/plain"], getType: async (t) => (t === "text/html" ? { size: 40 * 1024 * 1024, text: async () => assert.fail("a table that size is not read") } : { size: 3, text: async () => "큰표" }) }], readText: async () => "큰표" } });
  await select(big.document, "E1");
  await toolbarPaste(big);
  assert.equal(cell(big.document, "E1").textContent, "큰표", "40 MB of HTML is not read, the text is pasted");

  const image = await open({ clipboard: { read: async () => [item({ "image/png": "x" })], readText: async () => "" } });
  await select(image.document, "E1");
  await toolbarPaste(image);
  assert.equal(cell(image.document, "E1").textContent, "");
});

test("골라 붙이기 reads the clipboard's table too: 값 leaves the formats of the table out", async () => {
  const clipboard = { read: async () => [item({ "text/html": SHEET_HTML, "text/plain": "이름" })], readText: async () => "이름" };
  const h = await open({ clipboard });
  const d = h.document;
  await select(d, "E1");
  h.userClick(d.querySelector('.md-item[data-sheet-paste="special"]'));
  d.getElementById("sheet-paste-kind-select").value = "values";
  h.userClick(d.getElementById("btn-sheet-paste-apply"));
  await flushAsyncTurns(20);
  assert.equal(cell(d, "E1").textContent, "이름");
  assert.notEqual(cell(d, "E1").style.fontWeight, "600", "values only");
  assert.equal(cell(d, "E4").textContent, "병합", "the whole table was read, not its first line");
});
