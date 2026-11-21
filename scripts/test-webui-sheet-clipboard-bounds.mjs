import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// The HTML on the clipboard is untrusted: every pattern that reads it finishes in linear time on hostile input, every size is
// bounded and a block over a bound is refused whole with the reason, never cut. The merges and the cells one paste writes are
// bounded too, and the layout takes a pasted block's merges in one pass.

const BUDGET_MS = 200;
const within = (label, fn) => {
  const t0 = performance.now();
  const out = fn();
  const ms = performance.now() - t0;
  assert.ok(ms < BUDGET_MS, `${label} took ${ms.toFixed(0)} ms (limit ${BUDGET_MS})`);
  return out;
};

async function open() {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(h.document, makeFixtureFile("한도.xlsx", { sheets: [{ name: "한도", cells: [{ ref: "A1", raw: "가" }] }] }));
  await flushAsyncTurns(12);
  const win = h.document.defaultView;
  win.DOMParser = class {
    parseFromString(html) {
      const root = h.document.createElement("div");
      root.innerHTML = html;
      return { documentElement: root };
    }
  };
  return { h, win, api: win.__EASYOFFICE_SHEET_CLIP_HTML__, codec: win.__EASYOFFICE_SHEET_STYLE_CODEC__ };
}

const parseWith = (ctx, html) => ctx.api.parseClipboardHtml(html, { styleFromPayload: ctx.codec.styleFromPayload });

test("the size bound is 8 MiB and is checked before anything reads the HTML", async () => {
  const ctx = await open();
  assert.equal(ctx.api.LIMITS.HTML_CHARS, 8 * 1024 * 1024);
  const huge = "<td ".repeat(Math.ceil(ctx.api.LIMITS.HTML_CHARS / 4) + 1);
  assert.deepEqual(within("an over-size HTML", () => parseWith(ctx, huge)), { refused: "size" });
});

test("the markers of a spreadsheet program are looked for in the first 64 KiB only, without an open-ended pattern", async () => {
  const { api } = await open();
  const hostile = "<td ".repeat(2_000_000);
  assert.equal(within("the marker scan of 8 MB of open tags", () => api.htmlIsFromSpreadsheet(hostile)), false);
  assert.equal(within("the marker scan of 8 MB of class words", () => api.htmlIsFromSpreadsheet("<td class ".repeat(800_000))), false);
  const late = "x".repeat(70_000) + "<td class=xl65>";
  assert.equal(api.htmlIsFromSpreadsheet(late), false, "a marker past the first 64 KiB is not looked for");
  assert.equal(api.htmlIsFromSpreadsheet("<td class=xl65>"), true);
});

test("the rule scanner and the comment remover are single passes", async () => {
  const { api } = await open();
  within("a selector with no end", () => api.readStyleRules(["a".repeat(150_000) + "{" + "b".repeat(150_000)]));
  within("comment openers with no end", () => api.readStyleRules(["/*".repeat(250_000)]));
  within("braces", () => api.readStyleRules(["{".repeat(200_000)]));
  within("closing braces", () => api.readStyleRules(["}".repeat(200_000)]));
  within("an at-rule that never closes", () => api.readStyleRules(["@media x {" + "a{b:c}".repeat(50_000)]));
  const rules = api.readStyleRules(["/* a */ .x { color: #ff0000 } /* open"]);
  assert.deepEqual([...rules.keys()], [".x"], "an unterminated comment ends the text");
});

test("a declaration value longer than 256 characters is dropped, the others of its rule stay", async () => {
  const { api } = await open();
  const rules = api.readStyleRules([`.x { font-weight: 700; color: ${"z".repeat(300)}; text-align: center }`]);
  assert.deepEqual(Object.keys(rules.get(".x")), ["font-weight", "text-align"]);
});

test("a number and a border value are read in linear time whatever their length", async () => {
  const { api } = await open();
  assert.equal(within("pxOf on a long digit run", () => api.pxOf("1".repeat(200_000) + "x")), null);
  assert.equal(within("pxOf on a long fraction", () => api.pxOf("1." + "1".repeat(200_000) + "x")), null);
  assert.equal(api.pxOf(".5pt"), 0.5 * (4 / 3));
  assert.equal(api.pxOf("12px"), 12);
  assert.equal(within("edgeOf on blanks", () => api.edgeOf(" ".repeat(200_000) + "(")), "thin:000000");
  assert.equal(within("edgeOf on open parentheses", () => api.edgeOf("(".repeat(100_000) + " solid")), "thin:000000");
  assert.equal(api.edgeOf("1.5pt double rgb(255, 0, 0)"), "double:ff0000");
  assert.equal(api.edgeOf(".5pt dashed windowtext"), "dashed:000000");
});

test("a style attribute and a whole table of hostile styles are read in time", async () => {
  const ctx = await open();
  const long = " ".repeat(100_000);
  within("blanks in a style attribute", () => parseWith(ctx, `<table><tr><td style="color:${long}red;font-weight:${long}">a</td></tr></table>`));
  within("many declarations", () => parseWith(ctx, `<table><tr><td style="${"a:b;".repeat(40_000)}">a</td></tr></table>`));
});

test("style sheets over the bounds are refused whole, with the reason", async () => {
  const ctx = await open();
  const table = "<table><tr><td class=a>x</td></tr></table>";
  const big = `<style>${".a{color:red}\n".repeat(Math.ceil(ctx.api.LIMITS.STYLE_CHARS / 13) + 10)}</style>`;
  assert.deepEqual(parseWith(ctx, big + table), { refused: "style" }, "the style text over its total");
  const many = Array.from({ length: ctx.api.LIMITS.STYLE_ELEMENTS + 1 }, () => "<style>.a{color:red}</style>").join("");
  assert.deepEqual(parseWith(ctx, many + table), { refused: "style" }, "too many style elements");
  const rules = `<style>${Array.from({ length: ctx.api.LIMITS.RULES + 1 }, (_, i) => `.c${i}{color:red}`).join("")}</style>`;
  assert.deepEqual(parseWith(ctx, rules + table), { refused: "style" }, "too many rules");
  const wide = `<style>.a{${Array.from({ length: ctx.api.LIMITS.DECLARATIONS + 1 }, (_, i) => `p${i}:v`).join(";")}}</style>`;
  assert.deepEqual(parseWith(ctx, wide + table), { refused: "style" }, "too many properties in one rule");
  assert.deepEqual(parseWith(ctx, `<table><tr><td style="${Array.from({ length: 65 }, (_, i) => `p${i}:v`).join(";")}">x</td></tr></table>`), { refused: "style" }, "or in one style attribute");
  assert.ok(parseWith(ctx, "<style>.a{color:red}</style>" + table).block, "within the bounds");
});

test("more merges than a sheet holds are refused whole", async () => {
  const ctx = await open();
  const max = ctx.win.__EASYOFFICE_SHEET_LAYOUT__.MAX_MERGES;
  const rows = "<tr><td colspan=2>x</td></tr>".repeat(max + 1);
  const out = parseWith(ctx, `<table>${rows}</table>`);
  assert.deepEqual(out, { refused: "merges" });
});

test("a paste of a block that would write more merges or cells than the bound is refused", async () => {
  const { win } = await open();
  const core = win.__EASYOFFICE_SHEET_PASTE_CORE__;
  const max = win.__EASYOFFICE_SHEET_LAYOUT__.MAX_MERGES;
  const grid = { MAX_ROWS: 1048576, MAX_COLS: 16384, DEFAULT_COL_W: 96, refOf: (r, c) => r + ":" + c };
  const one = { rows: null, rich: true, c1: 0, cols: 2, cells: [[{ value: "a", formula: "" }, null]], widths: [], merges: [{ r1: 0, c1: 0, r2: 0, c2: 1 }] };
  const target = { r1: 1, c1: 1, r2: max + 1, c2: 2 };
  assert.deepEqual(within("the merge plan", () => core.planMerges({ snap: one, mode: "all", transpose: false, target, grid })), { refused: "merges" });
  assert.equal(core.refusalOf({ snap: one, mode: "all", transpose: false, target }), "merges");
  const cell = { rows: null, c1: 0, cols: 1, cells: [[{ value: "a", formula: "" }]], widths: [] };
  assert.equal(core.refusalOf({ snap: cell, mode: "all", transpose: false, target: { r1: 1, c1: 1, r2: 2_000_001, c2: 1 } }), "cells");
  assert.equal(core.refusalOf({ snap: cell, mode: "all", transpose: false, target: { r1: 1, c1: 1, r2: 2_000_000, c2: 1 } }), null);
  assert.equal(within("a whole-sheet target", () => core.refusalOf({ snap: cell, mode: "all", transpose: false, target: { r1: 1, c1: 1, r2: 1048576, c2: 16384 } })), "cells");
  assert.equal(core.refusalOf({ snap: one, mode: "values", transpose: false, target: { r1: 1, c1: 1, r2: max + 1, c2: 2 } }), null, "merges only count for the modes that write them");
});

test("the layout takes the merges of a block in one pass and one version step", async () => {
  const { win } = await open();
  const layout = win.__EASYOFFICE_SHEET_LAYOUT__;
  const list = Array.from({ length: 60000 }, (_, i) => ({ r1: i * 2 + 1, c1: 1, r2: i * 2 + 1, c2: 2 }));
  const before = layout.version();
  within("60,000 merges", () => layout.addMerges("한도", list));
  assert.equal(layout.version(), before + 1, "one touch");
  assert.equal(layout.mergesIn("한도", 1, 1, 10, 2).length, 5);
  layout.addMerges("한도", [{ r1: 5, c1: 5, r2: 5, c2: 5 }, { r1: 9, c1: 6, r2: 8, c2: 6 }]);
  assert.deepEqual(layout.mergesIn("한도", 5, 5, 5, 6).filter((m) => m.c1 >= 5), [], "a single cell or an upside-down rectangle is no merge");
  layout.addMerges("한도", Array.from({ length: layout.MAX_MERGES }, (_, i) => ({ r1: 200000 + i, c1: 4, r2: 200000 + i, c2: 5 })));
  assert.ok(layout.mergesIn("한도", 1, 1, 1048576, 16384).length <= layout.MAX_MERGES, "the sheet's bound holds");
});

// ---- the page --------------------------------------------------------------------------------------

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const feedback = (d) => d.getElementById("status-command-feedback").textContent;
const paste = async (d, html, text, target = d.activeElement) => {
  const data = { "text/html": html, "text/plain": text };
  target.dispatchEvent({ type: "paste", target, clipboardData: { getData: (t) => data[t] || "", setData() {} } });
  await flushAsyncTurns(15);
};

test("a 12 MB HTML on the clipboard pastes the text at once and says why the format was left", async () => {
  const { h } = await open();
  const d = h.document;
  const start = cell(d, "C3");
  start.dispatchEvent({ type: "mousedown", target: start, button: 0, bubbles: true });
  start.dispatchEvent({ type: "mouseup", target: start, bubbles: true });
  await flushAsyncTurns(2);
  const t0 = performance.now();
  await paste(d, "<td ".repeat(3_000_000), "글자");
  assert.ok(performance.now() - t0 < 3000, "no pattern runs over the HTML before the size check");
  assert.equal(cell(d, "C3").textContent, "글자");
  assert.match(feedback(d), /서식 없이 글자만/);
});

test("a paste that would write over two million cells is refused with the reason and changes nothing", async () => {
  const { h } = await open();
  const d = h.document;
  const head = (name) => d.querySelector(`#pane-sheet th[data-col="${name}"]`);
  head("A").dispatchEvent({ type: "mousedown", target: head("A"), button: 0, bubbles: true });
  head("A").dispatchEvent({ type: "mouseup", target: head("A"), bubbles: true });
  head("B").dispatchEvent({ type: "mousedown", target: head("B"), button: 0, bubbles: true, shiftKey: true });
  head("B").dispatchEvent({ type: "mouseup", target: head("B"), bubbles: true });
  await flushAsyncTurns(3);
  await paste(d, "", "x", cell(d, "A1"));
  assert.match(feedback(d), /칸이 너무 많/);
  assert.equal(cell(d, "A1").textContent, "가", "nothing was written");
});
