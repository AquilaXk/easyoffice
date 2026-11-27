import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";
import { matchesSelectorList } from "./webui-css-selector.mjs";

// The harness keeps the matches of a selector until the tree changes (PD-370). A cached answer must always equal the
// answer of a fresh walk, so these tests change the tree in every way the harness offers and compare both.

const SELECTORS = [
  ".item",
  ".item.on",
  "li",
  "#box li",
  "#box > li.item",
  "[data-k]",
  '[data-k="a"]',
  ".item:not(.on)",
  ".item:not([data-k])",
  "ul li, .extra",
  "#box",
  "[aria-disabled]",
];

function freshWalk(root, selector) {
  const out = [];
  const walk = (parent) => {
    for (const child of parent.childNodes) {
      if (child.nodeType !== 1) continue;
      if (matchesSelectorList(child, selector, root)) out.push(child);
      walk(child);
    }
  };
  walk(root.documentElement ?? root);
  return out;
}

function assertSameAsFreshWalk(root, step) {
  for (const selector of SELECTORS) {
    const fresh = freshWalk(root, selector);
    assert.deepEqual(root.querySelectorAll(selector), fresh, `querySelectorAll(${selector}) after: ${step}`);
    assert.equal(root.querySelector(selector), fresh[0] ?? null, `querySelector(${selector}) after: ${step}`);
  }
}

async function openList() {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-query" });
  const box = document.createElement("ul");
  box.id = "box";
  for (let i = 0; i < 4; i += 1) {
    const li = document.createElement("li");
    li.className = i % 2 ? "item on" : "item";
    li.setAttribute("data-k", i % 2 ? "a" : "b");
    box.appendChild(li);
  }
  document.body.appendChild(box);
  return { document, box };
}

test("a query repeated on an unchanged tree gives the same elements in a new array each time", async () => {
  const { document } = await openList();
  const first = document.querySelectorAll(".item");
  assert.equal(first.length, 4);
  first.pop();
  assert.equal(document.querySelectorAll(".item").length, 4, "the caller's change to its array does not reach the cache");
  assert.equal(document.querySelector(".item"), document.querySelectorAll(".item")[0]);
  assert.equal(document.querySelector(".nothing-here"), null);
  assert.equal(document.querySelector(".nothing-here"), null);
});

test("every kind of change to the tree or to an attribute a selector reads is seen by the next query", async () => {
  const { document, box } = await openList();
  const items = () => box.childNodes.filter((n) => n.nodeType === 1);
  assertSameAsFreshWalk(document, "start");

  items()[0].classList.add("on");
  assertSameAsFreshWalk(document, "classList.add");
  items()[1].classList.remove("on");
  assertSameAsFreshWalk(document, "classList.remove");
  items()[2].classList.toggle("on");
  assertSameAsFreshWalk(document, "classList.toggle");
  items()[3].className = "extra";
  assertSameAsFreshWalk(document, "className");
  items()[0].setAttribute("data-k", "z");
  assertSameAsFreshWalk(document, "setAttribute value change");
  items()[1].removeAttribute("data-k");
  assertSameAsFreshWalk(document, "removeAttribute");
  items()[2].dataset.k = "a";
  assertSameAsFreshWalk(document, "dataset");
  items()[0].setAttribute("aria-disabled", "true");
  assertSameAsFreshWalk(document, "attribute added");
  items()[0].id = "first";
  assertSameAsFreshWalk(document, "id");

  const added = document.createElement("li");
  added.className = "item";
  box.appendChild(added);
  assertSameAsFreshWalk(document, "appendChild");
  const before = document.createElement("li");
  before.className = "item on";
  box.insertBefore(before, items()[0]);
  assertSameAsFreshWalk(document, "insertBefore");
  box.removeChild(items()[1]);
  assertSameAsFreshWalk(document, "removeChild");
  items()[0].remove();
  assertSameAsFreshWalk(document, "remove");

  const clone = box.cloneNode(true);
  document.body.appendChild(clone);
  assertSameAsFreshWalk(document, "attached deep clone");
  clone.remove();
  assertSameAsFreshWalk(document, "detached clone");

  box.replaceChildren(...items().slice(0, 2));
  assertSameAsFreshWalk(document, "replaceChildren");
  box.innerHTML = '<li class="item on" data-k="a"></li><li class="item"></li><li class="extra"></li>';
  assertSameAsFreshWalk(document, "innerHTML");
  box.textContent = "";
  assertSameAsFreshWalk(document, "textContent");
});

test("a node put under a parent by assigning parentElement (a hand-made range insert) is seen by the next query", async () => {
  const { document, box } = await openList();
  assertSameAsFreshWalk(document, "start");
  const extra = document.createElement("li");
  extra.className = "item on";
  box.childNodes.splice(1, 0, extra);
  extra.parentElement = box;
  assertSameAsFreshWalk(document, "childNodes.splice with parentElement");
  assert.ok(document.querySelectorAll(".item.on").includes(extra));
});

test("an element query on a subtree sees changes made inside that subtree and elsewhere", async () => {
  const { document, box } = await openList();
  const other = document.createElement("div");
  other.id = "other";
  document.body.appendChild(other);
  assert.equal(box.querySelectorAll(".item").length, 4);
  assert.equal(other.querySelectorAll(".item").length, 0);
  const moved = box.childNodes.find((n) => n.nodeType === 1);
  other.appendChild(moved);
  assert.equal(box.querySelectorAll(".item").length, 3);
  assert.equal(other.querySelectorAll(".item").length, 1);
  assertSameAsFreshWalk(box, "subtree root, after a move");
  assertSameAsFreshWalk(other, "second subtree root, after a move");
});

test("a tree outside the document is answered fresh after each change, and again once it is attached", async () => {
  const { document } = await openList();
  const detached = document.createElement("div");
  const inner = document.createElement("span");
  inner.className = "item";
  detached.appendChild(inner);
  assert.equal(detached.querySelectorAll(".item").length, 1);
  inner.className = "other";
  assert.equal(detached.querySelectorAll(".item").length, 0, "a class change in a detached tree");
  assert.equal(detached.querySelector(".other"), inner);
  const second = document.createElement("span");
  second.className = "item";
  detached.appendChild(second);
  assert.deepEqual(detached.querySelectorAll(".item"), [second], "an element added to a detached tree");
  assert.equal(document.querySelectorAll(".item").includes(second), false, "the document does not see it yet");
  document.body.appendChild(detached);
  assert.ok(document.querySelectorAll(".item").includes(second), "the document sees it once the tree is attached");
  assert.equal(document.querySelector("body > div > .item"), second);
  detached.remove();
  assert.equal(document.querySelectorAll(".item").includes(second), false, "and not any more once it is taken out");
  assert.equal(document.querySelector("body > div > .item"), null);
});

test("a selector first used after an attribute changed is answered from the changed attribute", async () => {
  const { document, box } = await openList();
  const li = box.childNodes.find((n) => n.nodeType === 1);
  li.setAttribute("title-for-test", "x");
  assert.equal(document.querySelectorAll("[title-for-test]").length, 1);
  li.setAttribute("title-for-test", "y");
  assert.equal(document.querySelectorAll('[title-for-test="y"]').length, 1);
  assert.equal(document.querySelectorAll('[title-for-test="x"]').length, 0);
  li.removeAttribute("title-for-test");
  assert.equal(document.querySelectorAll("[title-for-test]").length, 0);
});

test("a random series of changes keeps every cached answer equal to a fresh walk", async () => {
  const { document, box } = await openList();
  let seed = 20261127;
  const next = (n) => {
    // mulberry32: a small generator whose low bits are as good as its high ones.
    seed = (seed + 0x6d2b79f5) >>> 0;
    let t = seed;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return Math.floor((((t ^ (t >>> 14)) >>> 0) / 4294967296) * n);
  };
  const all = () => [box, ...freshWalk(box, "*")];
  const classes = ["item", "on", "extra", "item on", ""];
  for (let step = 0; step < 120; step += 1) {
    const pool = all();
    const el = pool[next(pool.length)];
    const op = next(8);
    let label = "";
    if (op === 0) {
      el.className = classes[next(classes.length)];
      label = "className";
    } else if (op === 1) {
      el.setAttribute("data-k", ["a", "b"][next(2)]);
      label = "data-k";
    } else if (op === 2) {
      el.removeAttribute("data-k");
      label = "remove data-k";
    } else if (op === 3 && el !== box) {
      el.remove();
      label = "remove";
    } else if (op === 4) {
      const child = document.createElement(next(2) ? "li" : "span");
      child.className = classes[next(classes.length)];
      el.appendChild(child);
      label = "append";
    } else if (op === 5 && el !== box && pool.length > 2) {
      const target = pool[next(pool.length)];
      if (!el.contains(target)) {
        target.appendChild(el);
        label = "move";
      }
    } else if (op === 6) {
      el.classList.toggle("on");
      label = "toggle";
    } else if (op === 7) {
      el.setAttribute("aria-disabled", next(2) ? "true" : "false");
      label = "aria-disabled";
    }
    // Ask every selector after each change: the answers of the step before are what the cache holds now.
    assertSameAsFreshWalk(document, `step ${step} ${label}`);
  }
  assertSameAsFreshWalk(document, "end");
});
