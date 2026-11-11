import assert from "node:assert/strict";
import test from "node:test";
import { findHtmlSinkViolations } from "./html-sink-rule.mjs";

test("allows clearing innerHTML and reading markup", () => {
  assert.deepEqual(findHtmlSinkViolations('el.innerHTML = "";\nel.innerHTML=\'\';'), []);
  assert.deepEqual(findHtmlSinkViolations("const h = el.innerHTML; if (a.innerHTML === b) {}"), []);
  assert.deepEqual(findHtmlSinkViolations("const o = el.outerHTML;"), []);
});

test("flags innerHTML assignment on one line and across lines", () => {
  assert.equal(findHtmlSinkViolations("el.innerHTML = html;").length, 1);
  const split = findHtmlSinkViolations("x();\nel.innerHTML =\n  html;");
  assert.equal(split.length, 1);
  assert.match(split[0], /^line 2:/);
  assert.equal(findHtmlSinkViolations("el.innerHTML\n  += html;").length, 1);
  assert.equal(findHtmlSinkViolations("el.innerHTML =\n  `<b>${x}</b>`;").length, 1);
});

test("flags outerHTML assignment and insertAdjacentHTML", () => {
  assert.equal(findHtmlSinkViolations("el.outerHTML = html;").length, 1);
  assert.equal(findHtmlSinkViolations("el.insertAdjacentHTML('beforeend', html);").length, 1);
  assert.equal(findHtmlSinkViolations("el\n  .insertAdjacentHTML\n  (pos, html);").length, 1);
});

test("ignores comments and string text", () => {
  assert.deepEqual(findHtmlSinkViolations("// el.innerHTML = x;\n/* a.outerHTML = b */"), []);
  assert.deepEqual(findHtmlSinkViolations('const s = "el.innerHTML = x";'), []);
});
