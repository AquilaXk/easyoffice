import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// The document find source (find-doc.js): the block / text-node index over the editor, how a change
// reaches the index (one text node = one block), replacing in place, and the two highlight paths.

async function setup(html, extraEnv = {}) {
  const h = await createWorkspaceHarness();
  const { document } = h;
  const root = document.getElementById("doc-editor");
  root.innerHTML = html;
  const observers = [];
  class FakeObserver {
    constructor(cb) { this.cb = cb; observers.push(this); }
    observe() {}
    disconnect() { this.gone = true; }
  }
  const env = { MutationObserver: FakeObserver, ...extraEnv };
  const core = h.host.__EASYOFFICE_FIND_CORE__;
  const undoRuns = [];
  const source = h.host.__EASYOFFICE_CREATE_DOC_FIND_SOURCE__({ root, document, env, withUndo: (fn) => { undoRuns.push(1); return fn(); } });
  const index = core.createFindIndex();
  const matcher = (query) => core.buildMatcher({ query });
  return { h, root, source, index, core, matcher, observers, undoRuns, document };
}

const texts = (root) => root.querySelectorAll("p, td").map((el) => el.textContent);

test("blocks are paragraphs, cells and lines; a match can cross inline runs but not a block or a <br>", async () => {
  const t = await setup("<p>ab<b>cd</b>ef</p><p>ab</p><table><tbody><tr><td>cd</td><td>ef</td></tr></tbody></table><p>ab<br>cd</p>");
  t.source.sync(t.index);
  assert.equal(t.index.blockIds().length, 6, "paragraph, paragraph, two cells, two lines");
  t.index.search(t.matcher("efab"));
  assert.equal(t.index.total(), 0, "no match across two paragraphs");
  t.index.search(t.matcher("abcd"));
  assert.equal(t.index.total(), 1, "the first paragraph only: the line ended by <br> does not join its two halves");
  t.index.search(t.matcher("bcde"));
  assert.equal(t.index.total(), 1, "a match across runs of one paragraph");
  t.index.search(t.matcher("cd"));
  assert.equal(t.index.total(), 3, "inside the run, the cell and the line after <br>");
});

test("a text change re-reads one block; a structure change keeps the records of unchanged blocks", async () => {
  const t = await setup("<p>one two</p><p>three</p>");
  const reads = [];
  const real = t.index.updateBlock;
  t.index.updateBlock = (id, text) => { reads.push(text); return real(id, text); };
  t.source.sync(t.index);
  t.index.search(t.matcher("t"));
  assert.equal(t.index.total(), 2);
  const [first] = t.root.querySelectorAll("p");
  const textNode = first.childNodes[0];
  textNode.data = "one two two";
  t.source.watch(() => {});
  t.observers.at(-1).cb([{ type: "characterData", target: textNode }]);
  t.source.sync(t.index);
  assert.deepEqual(reads, ["one two two"], "only the changed paragraph is read again");
  t.index.search(t.matcher("t"));
  assert.equal(t.index.total(), 3);

  const ids = [...t.index.blockIds()];
  const added = t.document.createElement("p");
  added.appendChild(t.document.createTextNode("tt"));
  t.root.appendChild(added);
  t.observers.at(-1).cb([{ type: "childList", target: t.root }]);
  t.source.sync(t.index);
  assert.deepEqual(t.index.blockIds().slice(0, 2), ids, "records of unchanged blocks are the same objects");
  t.index.search(t.matcher("t"));
  assert.equal(t.index.total(), 5);
});

test("replace edits the text nodes in place, in one undo step, and the index sees the new text", async () => {
  const t = await setup("<p>ab<b>cd</b>ef ab</p><p>ab</p>");
  t.source.sync(t.index);
  t.index.search(t.matcher("ab"));
  const plan = t.index.blockIds().map((id) => ({ id, edits: t.index.matchesInBlock(id).map((m) => ({ start: m.start, end: m.end, value: "가" })) })).filter((p) => p.edits.length);
  const outcome = await t.source.replace(plan);
  assert.deepEqual(outcome, { replaced: 3, skipped: 0 });
  assert.deepEqual(texts(t.root), ["가cdef 가", "가"]);
  assert.equal(t.root.querySelector("b").textContent, "cd", "the bold run is untouched");
  assert.equal(t.undoRuns.length, 1);
  t.source.sync(t.index);
  assert.equal(t.index.search(t.matcher("ab")).total, 0);
  assert.equal(t.index.search(t.matcher("가")).total, 3);
});

test("a match over two runs is replaced into the first run and cut from the second", async () => {
  const t = await setup("<p>x<b>ab</b>cd y</p>");
  t.source.sync(t.index);
  t.index.search(t.matcher("abcd"));
  const [id] = t.index.blockIds();
  const m = t.index.matchesInBlock(id)[0];
  await t.source.replace([{ id, edits: [{ start: m.start, end: m.end, value: "Z" }] }]);
  assert.equal(t.root.querySelector("p").textContent, "xZ y");
  assert.equal(t.root.querySelector("b").textContent, "Z", "the replacement keeps the run it starts in");
});

test("with the Custom Highlight API the DOM is not touched and the current match has its own highlight", async () => {
  const sets = new Map();
  class Highlight { constructor(...ranges) { this.ranges = ranges; } }
  const t = await setup("<p>ab ab ab</p>", { CSS: { highlights: sets }, Highlight });
  const before = t.root.innerHTML;
  t.source.sync(t.index);
  t.index.search(t.matcher("ab"));
  const near = [0, 1, 2].map((n) => t.index.at(n));
  t.source.show(near, near[1]);
  assert.equal(sets.get("eo-find-match").ranges.length, 3);
  assert.equal(sets.get("eo-find-current").ranges.length, 1);
  const [range] = sets.get("eo-find-current").ranges;
  assert.deepEqual([range.startOffset, range.endOffset], [3, 5]);
  assert.equal(t.root.innerHTML, before, "no markup is inserted");
  t.source.clearHighlight();
  assert.equal(sets.size, 0);
});

test("without the Highlight API the current match is selected", async () => {
  const added = [];
  const selection = { removeAllRanges() { added.length = 0; }, addRange: (r) => added.push(r), anchorNode: null, toString: () => "" };
  const t = await setup("<p>ab ab</p>", { getSelection: () => selection });
  t.source.sync(t.index);
  t.index.search(t.matcher("ab"));
  t.source.show([], t.index.at(1));
  assert.equal(added.length, 1);
  assert.deepEqual([added[0].startOffset, added[0].endOffset], [3, 5]);
});

test("the search starts from the caret when it is inside a paragraph", async () => {
  const selection = { anchorNode: null, anchorOffset: 0 };
  const t = await setup("<p>ab</p><p>cd ab</p>", { getSelection: () => selection });
  t.source.sync(t.index);
  t.index.search(t.matcher("ab"));
  assert.equal(t.source.anchor(), null, "no caret in the editor");
  selection.anchorNode = t.root.querySelectorAll("p")[1].childNodes[0];
  selection.anchorOffset = 1;
  const anchor = t.source.anchor();
  assert.equal(anchor.offset, 1);
  assert.equal(t.index.at(t.index.neighbour(anchor.id, anchor.offset, true)).start, 3, "the next match after the caret");
});

test("dispose stops watching and clears the highlight", async () => {
  const sets = new Map();
  class Highlight { constructor(...ranges) { this.ranges = ranges; } }
  const t = await setup("<p>ab</p>", { CSS: { highlights: sets }, Highlight });
  t.source.watch(() => {});
  t.source.sync(t.index);
  t.index.search(t.matcher("ab"));
  t.source.show([t.index.at(0)], t.index.at(0));
  t.source.dispose();
  assert.equal(t.observers.at(-1).gone, true);
  assert.equal(sets.size, 0);
});
