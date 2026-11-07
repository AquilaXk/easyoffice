import assert from "node:assert/strict";
import test from "node:test";
import { findBareSorts, isSortRuleScoped, stripCommentsAndStrings } from "./sort-comparator-rule.mjs";

test("flags a sort call without a comparator", () => {
  assert.deepEqual(findBareSorts("const a = items.sort();"), [1]);
  assert.deepEqual(findBareSorts("x\n  .filter(Boolean)\n  .sort( )\n  .map(f)"), [3]);
  assert.deepEqual(findBareSorts("const k = Object.keys(o)?.sort();"), [1]);
  assert.deepEqual(findBareSorts("a.sort();\nb.sort();"), [1, 2]);
});

test("accepts sort calls that pass a comparator", () => {
  assert.deepEqual(findBareSorts("a.sort((x, y) => x - y);"), []);
  assert.deepEqual(findBareSorts("a.sort(byName);"), []);
  assert.deepEqual(findBareSorts("a.sort(\n  compare\n);"), []);
  assert.deepEqual(findBareSorts("const sort = 1; sorted.sort2(); resort();"), []);
});

test("ignores comments, strings, template text and regex literals", () => {
  const src = [
    "// never call .sort() here",
    "/* nor .sort()",
    "   across lines */",
    'const a = "text .sort() text";',
    "const b = '.sort()';",
    "const c = `template .sort() ${1 + 1} done`;",
    "const d = /['\"].sort()/.test(s);",
    String.raw`const e = "quote \" .sort() still string";`,
  ].join("\n");
  assert.deepEqual(findBareSorts(src), []);
});

test("still sees code inside template expressions and after tricky literals", () => {
  assert.deepEqual(findBareSorts("const m = `n: ${items.sort().length}`;"), [1]);
  assert.deepEqual(findBareSorts("const m = `a ${`inner ${ {x: 1}.x }`} b`; list.sort();"), [1]);
  assert.deepEqual(findBareSorts('const s = "a"; // note\nlist.sort();'), [2]);
  assert.deepEqual(findBareSorts("const half = total / 2; list.sort(); const r = n / 3;"), [1]);
});

test("stripping keeps line structure", () => {
  const src = "a\n/* x\ny */\nb `t\nu` c";
  assert.equal(stripCommentsAndStrings(src).split("\n").length, src.split("\n").length);
});

test("rule scope covers owned sources only", () => {
  assert.equal(isSortRuleScoped("crates/easyoffice-app/ui/sheet-grid.js"), true);
  assert.equal(isSortRuleScoped("scripts/check-repo-hygiene.mjs"), true);
  assert.equal(isSortRuleScoped("scripts/test-support/offline-rpc.js"), true);
  assert.equal(isSortRuleScoped("apps/hwp-studio/src/embed/rpc-router.ts"), true);
  assert.equal(isSortRuleScoped("apps/hwp-studio/vite.config.ts"), false);
  assert.equal(isSortRuleScoped("third_party/rhwp/rhwp-studio/src/a.ts"), false);
  assert.equal(isSortRuleScoped("vendor/rhwp-core/rhwp.js"), false);
  assert.equal(isSortRuleScoped("crates/easyoffice-app/ui/hwp-studio/assets/index.js"), false);
  assert.equal(isSortRuleScoped("scripts/node_modules/x/index.js"), false);
  assert.equal(isSortRuleScoped("scripts/readme.md"), false);
});
