import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// The pages of a printed document (doc-page-print.js, PD-213): each page is a window on the screen's layout with the
// header, footer and page number of the page; a paragraph over two pages is in both windows.

const GEO = { width: 794, height: 1123, pitch: 1133, padTop: 100, padBottom: 100, padLeft: 56, padRight: 56, contentTop: 100, contentHeight: 923 };

async function setup() {
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-pp" });
  await flushAsyncTurns(6);
  const editor = h.document.getElementById("doc-editor");
  editor.innerHTML = '<p id="a">첫째</p><p id="b">둘째가 두 쪽에 걸침</p><ol start="3"><li id="l1">하나</li><li id="l2">둘</li></ol><p id="c">마지막</p>';
  const print = h.host.__EASYOFFICE_DOC_PAGE_PRINT__;
  const [a, b, ol, c] = Array.from(editor.children);
  const unit = (el, top, page, endPage) => ({ el, top, page, endPage });
  const plan = {
    geo: GEO,
    pageCount: 3,
    units: [unit(a, 100, 0, 0), unit(b, 1000, 0, 1), unit(ol.children[0], 1233, 1, 1), unit(ol.children[1], 1260, 1, 1), unit(c, 2366, 2, 2)],
  };
  const rules = { classFor: (w, hh) => `paper-${w}x${hh}` };
  const build = (over = {}) => print.buildPages({ document: h.document, editor, plan, frame: null, rules, pageClass: "eo-print-page", ...over });
  return { h, editor, print, plan, build, unit };
}

test("every page is a box of the paper size with its units at the tops the layout gave them", async () => {
  const { build } = await setup();
  const pages = build();
  assert.equal(pages.length, 3);
  assert.ok(pages.every((p) => p.className === "eo-print-page paper-210.08x297.13 eo-print-doc-page"));
  const slots = (page) => Array.from(page.querySelectorAll(".eo-print-unit")).map((s) => [s.children[0].textContent, s.style.top]);
  // Window of page k starts at k * pitch + contentTop: 100, 1233, 2366.
  assert.deepEqual(slots(pages[0]), [["첫째", "0px"], ["둘째가 두 쪽에 걸침", "900px"]]);
  assert.deepEqual(slots(pages[1]), [["둘째가 두 쪽에 걸침", "-233px"], ["하나", "0px"], ["둘", "27px"]]);
  assert.deepEqual(slots(pages[2]), [["마지막", "0px"]]);
  const body = pages[0].querySelector(".eo-print-body");
  assert.equal(body.style.left, "56px");
  assert.equal(body.style.width, "682px");
  assert.equal(body.style.height, "923px");
});

test("a list item keeps its list: the number continues, and no clone repeats an id or keeps a page margin", async () => {
  const { build } = await setup();
  const pages = build();
  const shells = pages[1].querySelectorAll(".eo-print-unit > ol");
  assert.deepEqual(Array.from(shells).map((o) => o.start), [3, 4], "the items of an ordered list that starts at 3 are 3 and 4");
  assert.equal(pages.flatMap((p) => Array.from(p.querySelectorAll("[id]"))).length, 0);
  assert.equal(pages[0].querySelector(".eo-print-unit > p").style.marginTop, "0");
});

test("the header and footer of each page come from the frame with their numbers, and a page without a footer shows - n -", async () => {
  const { build, h, editor } = await setup();
  const seen = [];
  const frame = {
    printCopy: (kind, n, total) => {
      seen.push([kind, n, total]);
      if (kind === "footer") return null;
      const copy = h.document.createElement("div");
      copy.className = "doc-page-hf";
      copy.textContent = `머리 ${n}/${total}`;
      return copy;
    },
  };
  const pages = build({ frame });
  assert.deepEqual(seen.filter(([k]) => k === "header"), [["header", 1, 3], ["header", 2, 3], ["header", 3, 3]]);
  assert.equal(pages[1].querySelector(".doc-page-hf").textContent, "머리 2/3");
  assert.deepEqual(pages.map((p) => p.querySelector(".eo-print-pageno").textContent), ["- 1 -", "- 2 -", "- 3 -"]);
  void editor;
});

test("the real page frame prints its fields as text: the number of the page, the count and the file name", async () => {
  const { h, editor } = await setup();
  const footer = h.document.createElement("section");
  footer.dataset.eoHf = "footer";
  footer.innerHTML = '<p>쪽 <span class="eo-page-field" data-eo-field="page"></span>/<span class="eo-page-field" data-eo-field="pages"></span></p>';
  editor.appendChild(footer);
  const copy = h.host.__EASYOFFICE_DOC_PAGE_FRAME__.printCopy("footer", 2, 5);
  assert.equal(copy.textContent, "쪽 2/5");
  assert.equal(copy.querySelectorAll(".eo-page-field").length, 0);
  assert.equal(h.host.__EASYOFFICE_DOC_PAGE_FRAME__.printCopy("header", 1, 1), null, "no header, no copy");
});

test("a document past the page or clone limit is refused instead of built", async () => {
  const { build, plan, print, unit, editor } = await setup();
  assert.throws(() => build({ plan: { ...plan, pageCount: print.MAX_PRINT_PAGES + 1 } }), /너무 커서/);
  const many = Array.from({ length: 10 }, () => unit(editor.children[0], 100, 0, 30));
  const wide = { ...plan, pageCount: 31, units: Array.from({ length: 7000 }, (_, i) => many[i % 10]) };
  assert.throws(() => build({ plan: wide }), /너무 커서/);
  assert.equal(build({ plan: null }), null, "without a layout the caller prints the flow");
});
