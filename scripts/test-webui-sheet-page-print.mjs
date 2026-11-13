import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

// The pages of a printed sheet (sheet-page-print.js, PD-213): the print area, the paper and margins, the scale, the repeated
// rows and columns, the grid lines and the headings, each cut from the grid's own sizes.

class FakeStyleSheet {
  constructor() { this.cssRules = []; }
  insertRule(text, at) {
    this.cssRules.splice(at, 0, { cssText: text, style: { props: {}, setProperty(k, v) { this.props[k] = v; } } });
    return at;
  }
}

async function setup(cells) {
  const h = await createWorkspaceHarness();
  const printed = [];
  h.host.CSSStyleSheet = FakeStyleSheet;
  h.host.requestAnimationFrame = (cb) => cb();
  h.host.getComputedStyle = () => ({ getPropertyValue: (p) => (p === "font-size" ? "11pt" : "") });
  h.host.print = () => { printed.push(h.document.getElementById("eo-print-root")); };
  h.document.adoptedStyleSheets = [];
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  await pickFile(h.document, makeFixtureFile("인쇄.xlsx", { sheets: [{ name: "Sheet1", cells }] }));
  await flushAsyncTurns(12);
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const print = async () => {
    h.document.defaultView.__EASYOFFICE_NATIVE_MENU__("file:print");
    await flushAsyncTurns(8);
    return printed.at(-1);
  };
  const rules = () => h.document.adoptedStyleSheets.flatMap((sheet) => sheet.cssRules.map((r) => r.cssText));
  const feedback = () => h.document.getElementById("status-command-feedback").textContent;
  return { h, layout, print, rules, feedback, printed, done: () => h.sandbox.dispatchWindowEvent("afterprint") };
}

const column = (rows, ref = "A") => Array.from({ length: rows }, (_, i) => ({ ref: ref + (i + 1), raw: "행" + (i + 1) }));
const texts = (page) => page.querySelectorAll("td").map((td) => td.textContent);

test("a print area prints only its cells, on the paper and margins of the setup", async () => {
  const t = await setup([...column(6), ...column(6, "B"), { ref: "J10", raw: "먼 칸" }]);
  t.layout.setPage("Sheet1", { printArea: ["A2:B3"], paper: 8, landscape: true, margins: { top: 500, bottom: 500, left: 1000, right: 500, header: 300, footer: 300 } });
  const root = await t.print();
  const pages = root.querySelectorAll(".eo-print-sheet-page");
  assert.equal(pages.length, 1);
  assert.deepEqual(texts(pages[0]), ["행2", "행2", "행3", "행3"]);
  assert.ok(t.rules().includes("@page { size: 420mm 297mm; margin: 0; }"), "A3 turned");
  assert.equal(pages[0].style.padding, "12.7mm 12.7mm 12.7mm 25.4mm".replace(/12.7mm 12.7mm 12.7mm 25.4mm/, "12.7mm 12.7mm 12.7mm 25.4mm"));
  assert.equal(t.layout.pageOf("Sheet1").printArea[0], "A2:B3");
});

test("several print areas print one after another, each cut on its own", async () => {
  const t = await setup([...column(8), ...column(8, "B")]);
  t.layout.setPage("Sheet1", { printArea: ["A1:A2", "B7:B8"] });
  const pages = (await t.print()).querySelectorAll(".eo-print-sheet-page");
  assert.deepEqual(pages.map(texts), [["행1", "행2"], ["행7", "행8"]]);
});

test("the rows of the title are repeated on every page after the one they sit on, and pages go down before over", async () => {
  const t = await setup(column(100));
  t.layout.setPage("Sheet1", { titleRows: [1, 2] });
  const pages = (await t.print()).querySelectorAll(".eo-print-sheet-page");
  // A4 with 10 mm margins is 1047 px high: 43 rows of the grid's 24 px on a page.
  assert.ok(pages.length >= 3, String(pages.length));
  assert.deepEqual(texts(pages[0]).slice(0, 3), ["행1", "행2", "행3"]);
  assert.equal(texts(pages[0]).length, 43, "43 rows fill the first page");
  assert.equal(texts(pages[1]).length, 43, "two title rows and 41 more");
  assert.deepEqual(texts(pages[1]).slice(0, 3), ["행1", "행2", "행44"]);
});

test("the title columns are repeated on the pages across", async () => {
  const cells = Array.from({ length: 20 }, (_, i) => ({ ref: String.fromCharCode(65 + i) + "1", raw: "열" + (i + 1) }));
  const t = await setup(cells);
  t.layout.setPage("Sheet1", { titleCols: [1, 1] });
  const pages = (await t.print()).querySelectorAll(".eo-print-sheet-page");
  assert.ok(pages.length >= 2, "20 columns of 80 px do not fit across 718 px");
  assert.equal(texts(pages[1])[0], "열1");
  assert.notEqual(texts(pages[1])[1], "열2");
});

test("a percentage scales the table and fit-to-pages finds the scale: never above 100%, 0 leaves a direction free", async () => {
  const t = await setup([...column(120), ...column(120, "B")]);
  t.layout.setPage("Sheet1", { scale: { percent: 50 } });
  let page = (await t.print()).querySelector(".eo-print-sheet-page");
  assert.equal(page.querySelector("table").style.transform, "scale(0.5)");
  t.done();
  t.layout.setPage("Sheet1", { scale: { fitWidth: 1, fitHeight: 1 } });
  const root = await t.print();
  assert.equal(root.querySelectorAll(".eo-print-sheet-page").length, 1, "the whole range on one page");
  page = root.querySelector(".eo-print-sheet-page");
  const s = Number(/scale\(([\d.]+)\)/.exec(page.querySelector("table").style.transform)[1]);
  assert.ok(s < 1 && s > 0.3, String(s));
  t.done();
  t.layout.setPage("Sheet1", { scale: { fitWidth: 1, fitHeight: 0 } });
  const small = await t.print();
  assert.equal(small.querySelector("table").style.transform, "", "a range that fits across needs no scale");
  const core = t.h.host.__EASYOFFICE_SHEET_PAGE_PRINT__;
  assert.equal(core.scaleOf({ scale: { fitWidth: 1, fitHeight: 0 } }, 2000, 100, 1000, 1000), 0.5);
  assert.equal(core.scaleOf({ scale: { fitWidth: 2, fitHeight: 2 } }, 100, 100, 1000, 1000), 1);
  assert.equal(core.scaleOf({ scale: { fitWidth: 1, fitHeight: 1 } }, 1e9, 1e9, 1, 1), 0.1, "never smaller than 10%");
  assert.equal(core.scaleOf(null, 1, 1, 1, 1), 1);
});

test("centring, grid lines and headings are in the page", async () => {
  const t = await setup([{ ref: "A1", raw: "가" }, { ref: "B1", raw: "나" }]);
  t.layout.setPage("Sheet1", { centerH: true, centerV: true, grid: true, headings: true });
  const page = (await t.print()).querySelector(".eo-print-sheet-page");
  assert.equal(page.style.justifyContent, "center");
  assert.equal(page.style.alignItems, "center");
  assert.match(page.querySelector("table").className, /eo-print-grid/);
  const heads = page.querySelectorAll("td.eo-print-head").map((td) => td.textContent);
  assert.deepEqual(heads, ["", "A", "B", "1"]);
  t.done();
  t.layout.setPage("Sheet1", null);
  const plain = (await t.print()).querySelector(".eo-print-sheet-page");
  assert.doesNotMatch(plain.querySelector("table").className, /eo-print-grid/, "no grid lines unless the setup asks");
  assert.equal(plain.querySelectorAll("td.eo-print-head").length, 0);
});

test("hidden rows and columns are not printed, and a range or page count past the caps is refused", async () => {
  const t = await setup([...column(5), ...column(5, "B")]);
  t.layout.setHidden("Sheet1", "row", 2, 3, true);
  t.layout.setHidden("Sheet1", "col", 2, 2, true);
  const page = (await t.print()).querySelector(".eo-print-sheet-page");
  assert.deepEqual(texts(page), ["행1", "행4", "행5"]);
  t.done();
  t.layout.setHidden("Sheet1", "row", 2, 3, false);
  t.layout.setHidden("Sheet1", "col", 2, 2, false);
  t.layout.setPage("Sheet1", { printArea: ["A1:XFD1048576"] });
  await t.print();
  assert.equal(t.printed.length, 1, "the page cap and the cell cap refuse a whole sheet");
  assert.match(t.feedback(), /너무 커서/);
  t.layout.setPage("Sheet1", { printArea: Array.from({ length: 64 }, () => "A1:A1000") });
  await t.print();
  assert.match(t.feedback(), /너무 커서|인쇄/);
});

test("the pure cut: groups fill the budget, a line wider than it stands alone, repeated lines cost room after the first group", async () => {
  const { h } = await setup([{ ref: "A1", raw: "x" }]);
  const { cut } = h.host.__EASYOFFICE_SHEET_PAGE_PRINT__;
  const sizes = Float64Array.from([10, 10, 10, 10, 10, 10, 10, 10]);
  assert.deepEqual(cut(sizes, 30).groups.map((g) => [g.first, g.last]), [[0, 2], [3, 5], [6, 7]]);
  assert.deepEqual(cut(sizes, 30, [0, 0]).groups.map((g) => [g.first, g.last, g.title]), [[0, 2, false], [3, 4, true], [5, 6, true], [7, 7, true]]);
  assert.deepEqual(cut(Float64Array.from([50, 10, 10]), 30).groups.map((g) => [g.first, g.last]), [[0, 0], [1, 2]]);
  assert.deepEqual(cut(Float64Array.from([0, 0, 0]), 30).groups.length, 1);
  assert.deepEqual(cut(new Float64Array(0), 30).groups, []);
});
