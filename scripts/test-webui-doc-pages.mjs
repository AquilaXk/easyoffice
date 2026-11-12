import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { createRunHarness, p } from "./test-support/doc-run-harness.mjs";

// Pages of the document editor: the arithmetic on fake block heights (`doc-pages-core.js`), the module that
// measures, applies and reports it (`doc-pages.js`, on a fake layout), and the status line it feeds.

const require = createRequire(import.meta.url);
globalThis.__EASYOFFICE_HOST_ENV__ = undefined;
require("../crates/easyoffice-app/ui/doc-pages-core.js");
const core = globalThis.__EASYOFFICE_DOC_PAGES_CORE__;

const A4 = core.pageGeometry({ paper: "A4", padding: { top: 48, right: 56, bottom: 48, left: 56 } });
const LH = 27;
const whole = (h, extra = {}) => ({ h, gap: 10, lines: false, lh: 0, brk: false, keep: false, keepMin: 0, ...extra });
const text = (lines, extra = {}) => ({ h: lines * LH, gap: 10, lines: true, lh: LH, brk: false, keep: false, keepMin: 0, ...extra });

// What the browser does with a split paragraph: a line that touches a band moves below it; the others stay.
function simulateLines(unit, top, bands) {
  const ys = [];
  const total = Math.round(unit.h / unit.lh);
  let y = 0;
  for (let i = 0; i < total; i += 1) {
    for (const [s, e] of bands || []) if (y + unit.lh > s + 1e-6 && y < e - 1e-6) y = e;
    ys.push(top + y);
    y += unit.lh;
  }
  return { ys, bottom: top + y };
}

function assertInContent(y, h, label) {
  const k = Math.floor((y + 0.01) / A4.pitch);
  const o = y - k * A4.pitch;
  assert.ok(o >= A4.contentTop - 0.01 && o + h <= A4.contentBottom + 0.01, `${label}: line at ${y} (page ${k + 1}, offset ${o}) leaves the content area`);
}

test("geometry: A4 and Letter, portrait and landscape, content area from the padding", () => {
  assert.deepEqual([A4.width, A4.height, A4.gap, A4.pitch], [794, 1123, 10, 1133]);
  assert.deepEqual([A4.contentTop, A4.contentBottom, A4.contentHeight], [48, 1075, 1027]);
  const letter = core.pageGeometry({ paper: "Letter", padding: { top: 20, bottom: 30 } });
  assert.deepEqual([letter.width, letter.height, letter.contentBottom], [816, 1056, 1026]);
  const land = core.pageGeometry({ paper: "A4", landscape: true, padding: {} });
  assert.deepEqual([land.width, land.height], [1123, 794]);
  assert.equal(core.pageGeometry({ paper: "", padding: {} }).width, 794, "an unknown paper is A4");
});

test("whole blocks fill a page and the one that does not fit starts the next", () => {
  const units = Array.from({ length: 6 }, () => whole(300));
  const out = core.paginate(units, A4);
  assert.deepEqual(Array.from(out.page), [0, 0, 0, 1, 1, 1]);
  assert.equal(out.pageCount, 2);
  assert.equal(out.top[3], A4.pitch + A4.contentTop, "the pushed block starts at the content top of page 2");
  assert.ok(out.push[3] > 0 && out.push[0] === 0 || out.push[0] > 0);
  assert.equal(out.bands.every((b) => b === null), true);
});

test("a block taller than a page stays where it is instead of leaving a page empty", () => {
  const out = core.paginate([whole(200), whole(1500), whole(100)], A4);
  assert.equal(out.page[1], 0);
  assert.equal(out.endPage[1], 1);
  assert.equal(out.page[2], 1, "what follows starts after the content area that the tall block ended in");
});

test("forced breaks start a page, but not when the block is already first on its page", () => {
  const out = core.paginate([whole(100), whole(100, { brk: true }), whole(100, { brk: true })], A4);
  assert.deepEqual(Array.from(out.page), [0, 1, 2]);
  assert.equal(out.top[1] % A4.pitch, A4.contentTop);
  const first = core.paginate([whole(100, { brk: true, gap: 48 }), whole(100)], A4);
  assert.deepEqual(Array.from(first.page), [0, 0], "a break before the first block makes no empty page");
  assert.equal(first.push[0], 0);
});

test("a paragraph splits at the line that crosses the page bottom, and every line ends up inside a content area", () => {
  const filler = whole(940);
  const para = text(14);
  const out = core.paginate([filler, para], A4);
  const bands = out.bands[1];
  assert.equal(bands.length, 1);
  assert.equal(out.page[1], 0);
  assert.equal(out.endPage[1], 1);
  const { ys, bottom } = simulateLines(para, out.top[1], bands);
  ys.forEach((y, i) => assertInContent(y, LH, "line " + i));
  assert.ok(Math.abs(bottom - out.bottom[1]) < 0.01, "the planned bottom is where the lines end");
  const [start, end] = bands[0];
  assert.ok(Math.abs(out.top[1] + end - (A4.pitch + A4.contentTop)) < 0.01, "the band ends at the next page's content top");
  assert.ok(Math.abs(out.top[1] + start - A4.contentBottom) < 0.01, "the band starts at the content bottom");
});

test("a paragraph over several pages gets one band per page boundary", () => {
  const para = text(120);
  const out = core.paginate([para], A4);
  assert.equal(out.bands[0].length, 3);
  const { ys } = simulateLines(para, out.top[0], out.bands[0]);
  ys.forEach((y, i) => assertInContent(y, LH, "line " + i));
  assert.equal(out.pageCount, 4);
});

test("orphan and widow control keep two lines on each side of a split", () => {
  // one line fits on the page, so the whole paragraph moves
  const orphan = core.paginate([whole(1027 - LH - 15, { gap: 48 }), text(6)], A4);
  assert.equal(orphan.bands[1], null);
  assert.equal(orphan.page[1], 1);
  // all but one line fit: a second line goes along to the next page
  const widow = core.paginate([whole(847, { gap: 48 }), text(7)], A4);
  const { ys } = simulateLines(text(7), widow.top[1], widow.bands[1]);
  assert.equal(ys.filter((y) => y >= A4.pitch).length, 2, "two lines on the second page");
  assert.equal(ys.filter((y) => y < A4.pitch).length, 5);
  // a paragraph already first on its page splits without moving
  const top = core.paginate([text(60, { gap: 48 })], A4);
  assert.equal(top.push[0], 0);
});

test("a heading stays with the first line of what follows", () => {
  const heading = whole(40, { keep: true, keepMin: LH });
  const fits = core.paginate([whole(900, { gap: 48 }), heading, text(5)], A4);
  assert.equal(fits.page[1], 0);
  const tight = core.paginate([whole(960, { gap: 48 }), heading, text(5)], A4);
  assert.equal(tight.page[1], 1, "the heading moves with its text");
});

test("a block that starts in the bottom margin or the gap moves to the next page's content top", () => {
  const out = core.paginate([whole(1017), whole(20, { gap: 30 })], A4);
  assert.equal(out.page[1], 1);
  assert.equal(out.top[1], A4.pitch + A4.contentTop);
});

test("a height measured for the same bands replaces the estimate; other bands keep the estimate", () => {
  const para = text(60);
  const estimate = core.paginate([para], A4);
  const sig = estimate.sig[0];
  const measured = core.paginate([{ ...para, hAct: estimate.height[0] + 5, actSig: sig }, whole(10)], A4);
  assert.equal(measured.height[0], estimate.height[0] + 5);
  assert.equal(measured.top[1] >= estimate.bottom[0] + 5, true);
  const stale = core.paginate([{ ...para, hAct: 5, actSig: "other" }], A4);
  assert.equal(stale.height[0], estimate.height[0]);
});

test("random documents: every split line lands in a content area, the order is kept and the page count covers the end", () => {
  let seed = 12345;
  const rand = (n) => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed % n;
  };
  for (let round = 0; round < 60; round += 1) {
    const units = [];
    for (let i = 0; i < 40; i += 1) {
      const kind = rand(10);
      if (kind < 6) units.push(text(1 + rand(30), { gap: rand(2) * 16 }));
      else if (kind < 8) units.push(whole(20 + rand(500), { gap: rand(2) * 16 }));
      else if (kind === 8) units.push(whole(36, { keep: true, keepMin: LH, gap: 10 }));
      else units.push(text(1 + rand(5), { brk: true }));
    }
    const out = core.paginate(units, A4);
    let prev = 0;
    units.forEach((u, i) => {
      assert.ok(out.top[i] >= prev + u.gap - 0.01, "never above its natural place");
      assert.ok(out.bottom[i] >= out.top[i]);
      if (u.lines) simulateLines(u, out.top[i], out.bands[i]).ys.forEach((y) => assertInContent(y, LH, `round ${round} unit ${i}`));
      prev = out.bottom[i];
    });
    assert.ok(out.pageCount >= out.endPage[units.length - 1] + 1);
  }
});

test("the band shape is a polygon of one rectangle per band", () => {
  const { shape, height } = core.bandShape([[100, 190], [1217, 1300]]);
  assert.equal(shape, "polygon(0 100px,100% 100px,100% 190px,0 190px,0 1217px,100% 1217px,100% 1300px,0 1300px)");
  assert.equal(height, 1300);
});

// ---- the module on a fake layout -----------------------------------------------------------------

const NATURAL_GAP = 10;

// Blocks with fixed heights stacked like the browser does: a margin from the page layer replaces the natural gap.
function fakePage(heights, { tags = [], selection = null } = {}) {
  const raf = [];
  let reads = 0;
  const observers = [];
  const attrs = new Map();
  const make = (tag, h) => {
    const style = new Map();
    const el = {
      nodeType: 1, tagName: tag, h, hidden: false, className: "", dataset: {}, style: {
        setProperty: (k, v) => style.set(k, v), removeProperty: (k) => style.delete(k), get: (k) => style.get(k), has: (k) => style.has(k),
      },
      classList: { contains: (c) => el.className.split(" ").includes(c) },
      getAttribute: (k) => (k === "style" ? [...style].map(([a, b]) => a + ":" + b).join(";") : attrs.get(el)?.[k] ?? null),
      setAttribute(k, v) { attrs.set(el, { ...attrs.get(el), [k]: v }); },
      removeAttribute(k) { const a = { ...attrs.get(el) }; delete a[k]; attrs.set(el, a); },
      get parentElement() { return editor; },
      children: [], contains: (n) => n === el || el.children.includes(n),
      getBoundingClientRect: () => { reads += 1; return rectOf(el); },
    };
    return el;
  };
  const rectOf = (el) => {
    let bottom = 38;
    for (const child of editor.children) {
      if (child.tagName === "FOOTER" || child.hidden) continue;
      const pushed = child.getAttribute("data-eo-pg")?.includes("m") && child.style.has("--pg-mt");
      const top = bottom + (pushed ? Number.parseFloat(child.style.get("--pg-mt")) : NATURAL_GAP);
      bottom = top + child.h;
      if (child === el) return { top, bottom, height: child.h, width: 794 };
    }
    return { top: 0, bottom: 0, height: 0, width: 794 };
  };
  const editorStyle = new Map();
  const editor = {
    nodeType: 1, tagName: "ARTICLE", isConnected: true, offsetWidth: 794, dataset: {}, children: [], className: "",
    style: { setProperty: (k, v) => editorStyle.set(k, v), maxWidth: "" },
    classList: { contains: (c) => editor.className.split(" ").includes(c), toggle(c, on) { const set = new Set(editor.className.split(" ").filter(Boolean)); if (on) set.add(c); else set.delete(c); editor.className = [...set].join(" "); } },
    getBoundingClientRect: () => ({ top: 0, bottom: 0, height: 0, width: 794 }),
    getClientRects: () => [{}],
    addEventListener() {}, removeEventListener() {},
    contains: (n) => editor.children.includes(n) || n === editor,
    querySelector: (sel) => editor.children.find((c) => c.tagName === "FOOTER") ?? null,
    appendChild(c) { editor.children.push(c); c.children = c.children ?? []; return c; },
  };
  heights.forEach((h, i) => editor.children.push(make(tags[i] || "P", h)));
  const documentStub = {
    createElement: (tag) => Object.assign(make(tag.toUpperCase(), 0), { children: [], lastChild: null, setAttribute() {}, remove() {}, appendChild(c) { this.children.push(c); this.lastChild = c; } }),
    addEventListener() {},
  };
  const env = {
    requestAnimationFrame: (fn) => raf.push(fn),
    getComputedStyle: () => ({ fontSize: "16px", lineHeight: "26.4px", display: "block", paddingTop: "48px", paddingBottom: "48px", paddingLeft: "56px", paddingRight: "56px" }),
    MutationObserver: class { constructor(cb) { this.cb = cb; this.pending = []; observers.push(this); } observe() {} disconnect() {} takeRecords() { const p = this.pending; this.pending = []; return p; } },
    CSS: { supports: () => false },
    getSelection: () => selection,
  };
  return { editor, env, document: documentStub, observers, flush: () => raf.splice(0).forEach((fn) => fn()), reads: () => reads, resetReads: () => { reads = 0; }, touch: (el) => { observers[0].cb([{ type: "characterData", target: el }]); } };
}

function pagesOn(page, extra = {}) {
  const createPages = globalThis.__EASYOFFICE_CREATE_DOC_PAGES__ ?? (require("../crates/easyoffice-app/ui/doc-pages.js"), globalThis.__EASYOFFICE_CREATE_DOC_PAGES__);
  const pages = createPages({ document: page.document, env: page.env, ...extra });
  pages.attach(page.editor);
  return pages;
}

test("the module measures the blocks, pushes the one that crosses a page, and answers the page questions", () => {
  const page = fakePage([300, 300, 300, 300, 300, 300], { tags: ["H1", "P", "P", "H2", "P", "P"] });
  const pages = pagesOn(page);
  const result = pages.layoutNow();
  assert.deepEqual(result, { pageCount: 2, currentPage: 1 });
  assert.equal(pages.pageCount(), 2);
  const [a, , , d] = page.editor.children;
  assert.equal(pages.pageOf(a), 1);
  assert.equal(pages.pageOf(d), 2);
  assert.deepEqual(pages.headingPages().map((h) => [h.level, h.page]), [[1, 1], [2, 2]]);
  assert.equal(d.getAttribute("data-eo-pg"), "m");
  assert.equal(Number.parseFloat(d.style.get("--pg-mt")), A4.pitch + A4.contentTop - (A4.contentTop + 3 * 300 + 2 * NATURAL_GAP), "the margin lands the block on the content top of page 2");
  assert.equal(page.editor.children.at(-1).tagName, "FOOTER", "the paper layer is the last child");
  assert.equal(page.editor.children.at(-1).children.length, 2, "one sheet per page");
});

test("a change to one block measures that block only and moves the boundary", () => {
  const page = fakePage(Array.from({ length: 40 }, () => 200));
  const pages = pagesOn(page);
  pages.layoutNow();
  const before = pages.pageCount();
  page.resetReads();
  page.touch(page.editor.children[20]);
  page.flush();
  assert.ok(page.reads() <= 4, "the dirty block, the one before it and the editor: " + page.reads());
  assert.equal(pages.pageCount(), before);
  page.editor.children[0].h = 900;
  page.touch(page.editor.children[0]);
  page.flush();
  assert.ok(pages.pageCount() > before, "a taller first block pushes the end of the document down");
  const notified = [];
  pages.onChange((state) => notified.push(state));
  page.editor.children[1].h = 3000;
  page.touch(page.editor.children[1]);
  page.flush();
  assert.equal(notified.length, 1);
  assert.equal(notified[0].pageCount, pages.pageCount());
});

test("the current page follows the caret", () => {
  const selection = { focusNode: null, rangeCount: 0 };
  const page = fakePage([300, 300, 300, 300, 300, 300], { selection });
  const pages = pagesOn(page);
  pages.layoutNow();
  assert.equal(pages.currentPage(), 1);
  selection.focusNode = page.editor.children[5];
  assert.equal(pages.layoutNow().currentPage, 2);
  selection.focusNode = page.editor.children[1];
  assert.equal(pages.layoutNow().currentPage, 1);
});

test("forced breaks and a hidden or detached editor", () => {
  const page = fakePage([100, 100, 100]);
  page.editor.children[1].dataset.eoBreak = "page";
  const pages = pagesOn(page);
  assert.equal(pages.layoutNow().pageCount, 2);
  assert.equal(pages.pageOf(page.editor.children[1]), 2);
  assert.equal(pages.pageOf(page.editor.children[2]), 2);
  page.editor.getClientRects = () => [];
  page.editor.children[0].h = 5000;
  page.touch(page.editor.children[0]);
  assert.equal(pages.layoutNow().pageCount, 2, "nothing is measured while the editor is not shown");
  page.editor.getClientRects = () => [{}];
  assert.ok(pages.layoutNow().pageCount > 2, "the pending change is measured when it shows again");
});

test("the status line shows the current and total page from the page layout", async () => {
  const h = await createWorkspaceHarness();
  const pages = h.host.__EASYOFFICE_DOC_PAGES__;
  pages.pageCount = () => 7;
  pages.currentPage = () => 3;
  h.host.__EASYOFFICE_DOC_PAGES__.pageCount();
  const editor = h.document.getElementById("doc-editor");
  editor.dispatchEvent({ type: "input", target: editor });
  await flushAsyncTurns();
  const pos = h.document.getElementById("status-studio-pos").textContent;
  assert.match(pos, /^3\/7 쪽/);
  assert.match(h.document.getElementById("doc-page-zone-indicator").textContent, /^3 \/ 7 쪽/);
  assert.match(h.document.getElementById("status-doc-filename").textContent, /7쪽$/);
});

test("the editor has no fixed footer page number any more", async () => {
  const h = await createWorkspaceHarness();
  assert.equal(h.document.querySelector("footer.doc-page-footer"), null);
});

test("a page break mark reaches the save payload, comes back from an opened file, and the page layer's own mark is not kept in snapshots", async () => {
  const h = await createRunHarness([p("첫 쪽"), p("둘째 쪽"), p("같은 쪽")]);
  h.editor.childNodes[1].dataset.eoBreak = "page";
  h.editor.childNodes[2].dataset.eoPg = "m";
  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  assert.deepEqual(blocks.map((b) => [b.runs[0].text, b.pageBreakBefore]), [["첫 쪽", undefined], ["둘째 쪽", true], ["같은 쪽", undefined]]);
  const snapshot = h.host.__EASYOFFICE_SHEET_UTILS__.serializeDomChildren(h.editor);
  assert.equal(snapshot[1].data.eoBreak, "page");
  assert.equal("eoPg" in snapshot[2].data, false);

  const opened = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  await pickFile(opened.document, makeFixtureFile("쪽.docx", { blocks }));
  const editor = opened.document.getElementById("doc-editor");
  const marked = Array.from(editor.querySelectorAll("p[data-eo-break]"), (e) => e.textContent);
  assert.deepEqual(marked, ["둘째 쪽"]);
  assert.deepEqual(opened.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor), blocks);
});
