import test from "node:test";
import assert from "node:assert/strict";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// Find / replace core (find-core.js): the matcher options, the offset table over text nodes, the
// per-block match index (an edit searches one block), and the budgets that stop a runaway query.

const core = loadUiModule(new URL("../crates/easyoffice-app/ui/find-core.js", import.meta.url), {}).__EASYOFFICE_FIND_CORE__;
const matcherOf = (query, options = {}) => {
  const m = core.buildMatcher({ query, ...options });
  assert.ok(m.ok, query);
  return m;
};
const spans = (text, m) => core.matchesOf(text, m).map(({ start, end }) => [start, end]);

test("the case option; the query is always literal", () => {
  assert.deepEqual(spans("Foo foo FOO", matcherOf("foo")), [[0, 3], [4, 7], [8, 11]]);
  assert.deepEqual(spans("Foo foo FOO", matcherOf("foo", { matchCase: true })), [[4, 7]]);
  assert.deepEqual(spans("a.b axb", matcherOf("a.b")), [[0, 3]], "a literal query is escaped");
  assert.deepEqual(spans("(x) x", matcherOf("(x)")), [[0, 3]]);
  assert.deepEqual(spans("a1 b22", matcherOf("[a-c]\\d+")), [], "no regular expressions");
});

test("Korean text: syllables, whole word with particles, and a decomposed query", () => {
  assert.deepEqual(spans("한글 문서와 한글문서 한글", matcherOf("한글")), [[0, 2], [7, 9], [12, 14]]);
  const decomposed = "한글";
  assert.equal(decomposed.normalize("NFC"), "한글");
  assert.deepEqual(spans("한글", matcherOf(decomposed)), [[0, 2]], "the query is composed before matching");
});

test("an oversized query is refused with a message, an empty one matches nothing", () => {
  const long = core.buildMatcher({ query: "a".repeat(core.MAX_QUERY_CHARS + 1) });
  assert.equal(long.ok, false);
  assert.match(long.error, /너무 깁니다/);
  assert.deepEqual(spans("abc", matcherOf("")), []);
});

test("edits across text nodes keep each run and cut the covered text", () => {
  const segments = ["Hello ", "wor", "ld and wor", "ld"];
  const text = segments.join("");
  const m = matcherOf("world");
  const edits = core.matchesOf(text, m).map(({ start, end }) => ({ start, end, value: "EARTH" }));
  assert.equal(edits.length, 2);
  const out = core.applyEdits(segments, edits);
  assert.equal(out.join(""), "Hello EARTH and EARTH");
  assert.deepEqual(out, ["Hello ", "EARTH", " and EARTH", ""], "the replacement lands in the run where the match starts");
  assert.deepEqual(core.applyEdits(["abc"], []), ["abc"]);
  assert.deepEqual(core.applyEdits(["가나다", "라마"], [{ start: 2, end: 4, value: "" }]), ["가나", "마"]);
  assert.deepEqual(core.applyEdits(["", "ab", ""], [{ start: 0, end: 2, value: "x" }]), ["", "x", ""]);
});

test("segmentAt: boundaries belong to the next run for a start and to the previous run for an end", () => {
  const starts = core.segmentStarts(["ab", "cd", "ef"]);
  assert.deepEqual(starts, [0, 2, 4]);
  assert.equal(core.segmentAt(starts, 2), 1);
  assert.equal(core.segmentAt(starts, 2, true), 0);
  assert.equal(core.segmentAt(starts, 6, true), 2);
});

// A matcher that counts how many texts a search ran over.
function spyMatcher(query) {
  const real = matcherOf(query);
  const seen = [];
  const re = { get lastIndex() { return real.re.lastIndex; }, set lastIndex(v) { real.re.lastIndex = v; }, exec(text) {
    if (real.re.lastIndex === 0) seen.push(text);
    return real.re.exec(text);
  } };
  return { matcher: { ...real, re }, seen };
}

test("the index searches only the block that changed", () => {
  const index = core.createFindIndex();
  const blocks = Array.from({ length: 50 }, (_, i) => ({ id: "p" + i, text: "문단 " + i + " 찾기 대상" }));
  index.setBlocks(blocks);
  const { matcher, seen } = spyMatcher("찾기");
  assert.deepEqual(index.search(matcher), { total: 50, complete: true });
  assert.equal(seen.length, 50, "a new query searches every block once");
  seen.length = 0;
  index.updateBlock("p7", "바뀐 문단 찾기 찾기");
  assert.equal(index.search(matcher).total, 51);
  assert.deepEqual(seen, ["바뀐 문단 찾기 찾기"], "typing in one paragraph searches that paragraph");
  seen.length = 0;
  index.setBlocks([...blocks.slice(0, 20), { id: "new", text: "찾기" }, ...blocks.slice(20)].map((b) => ({ ...b, text: b.id === "p7" ? "바뀐 문단 찾기 찾기" : b.text })));
  index.search(matcher);
  assert.deepEqual(seen, ["찾기"], "a structure change keeps the matches of unchanged blocks");
  seen.length = 0;
  index.search(matcher);
  assert.equal(seen.length, 0, "nothing changed, nothing searched");
});

test("at(), neighbour() and wrap-around over several blocks", () => {
  const index = core.createFindIndex();
  index.setBlocks([{ id: "a", text: "x x" }, { id: "b", text: "none" }, { id: "c", text: "x" }]);
  index.search(matcherOf("x"));
  assert.equal(index.total(), 3);
  assert.deepEqual([0, 1, 2].map((n) => [index.at(n).id, index.at(n).start]), [["a", 0], ["a", 2], ["c", 0]]);
  assert.equal(index.at(3), null);
  assert.equal(index.neighbour("a", 1, true), 1, "next from after the first hit");
  assert.equal(index.neighbour("a", 3, true), 2, "skips a block without hits");
  assert.equal(index.neighbour("c", 1, true), 0, "wraps to the start");
  assert.equal(index.neighbour("a", 0, false), 2, "wraps back to the last");
  assert.equal(index.neighbour("c", 0, false), 1);
  assert.equal(index.neighbour("missing", 0, true), 0);
});

test("budgets: the match cap and the time budget stop a search instead of running away", () => {
  const index = core.createFindIndex();
  index.setBlocks([{ id: "big", text: "a".repeat(core.MAX_MATCHES + 50) }, { id: "next", text: "a" }]);
  const capped = index.search(matcherOf("a"));
  assert.deepEqual(capped, { total: core.MAX_MATCHES, complete: false });

  let clock = 0;
  const slow = core.createFindIndex(() => (clock += core.SEARCH_BUDGET_MS));
  slow.setBlocks([{ id: "1", text: "a" }, { id: "2", text: "a" }, { id: "3", text: "a" }]);
  const partial = slow.search(matcherOf("a"));
  assert.equal(partial.complete, false);
  assert.ok(partial.total < 3, "blocks past the budget stay stale for the next call");
});

test("whole-cell matching keeps only a block whose entire text is the query (case option still applies)", () => {
  const whole = (query, matchCase = false) => core.buildMatcher({ query, matchCase, wholeCell: true });
  assert.equal(core.matchesOf("사과", whole("사과")).length, 1);
  assert.equal(core.matchesOf("사과 파이", whole("사과")).length, 0);
  assert.equal(core.matchesOf("Apple", whole("apple")).length, 1, "case-insensitive by default");
  assert.equal(core.matchesOf("Apple", whole("apple", true)).length, 0);
  assert.equal(core.matchesOf("a.c", whole("a.c")).length, 1, "the query stays literal text");
  assert.equal(core.matchesOf("abc", whole("a.c")).length, 0);
  assert.notEqual(whole("x").key, core.buildMatcher({ query: "x" }).key, "the option changes the index key");
});
