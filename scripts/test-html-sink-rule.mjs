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

test("flags the parsers and writers that take markup", () => {
  assert.equal(findHtmlSinkViolations("const d = new DOMParser().parseFromString(t, 'text/html');").length, 1);
  assert.equal(findHtmlSinkViolations("const f = range.createContextualFragment(html);").length, 1);
  assert.equal(findHtmlSinkViolations("document.write(html);").length, 1);
  assert.equal(findHtmlSinkViolations("win.document\n  .writeln(html);").length, 1);
  assert.equal(findHtmlSinkViolations("el.setHTMLUnsafe(html);").length, 1);
  assert.equal(findHtmlSinkViolations("Document.parseHTMLUnsafe(html);").length, 1);
});

test("flags a frame document and clears it only with an empty string", () => {
  assert.equal(findHtmlSinkViolations("frame.srcdoc = html;").length, 1);
  assert.equal(findHtmlSinkViolations("frame.srcdoc += html;").length, 1);
  assert.deepEqual(findHtmlSinkViolations('frame.srcdoc = "";'), []);
  assert.deepEqual(findHtmlSinkViolations("const s = frame.srcdoc;"), []);
  assert.equal(findHtmlSinkViolations("frame.setAttribute('srcdoc', html);").length, 1);
  assert.equal(findHtmlSinkViolations('frame.setAttribute( "SRCDOC", html);').length, 1);
  assert.deepEqual(findHtmlSinkViolations("frame.setAttribute('title', t);"), []);
});

test("flags a sink reached by a string key", () => {
  assert.equal(findHtmlSinkViolations('el["innerHTML"] = html;').length, 1);
  assert.equal(findHtmlSinkViolations("el[ 'outerHTML' ] = html;").length, 1);
  assert.equal(findHtmlSinkViolations('el["insertAdjacentHTML"]("beforeend", html);').length, 1);
  assert.equal(findHtmlSinkViolations("el[`srcdoc`] = html;").length, 1);
  assert.equal(findHtmlSinkViolations('el["innerHTML"] += html;').length, 1);
  assert.deepEqual(findHtmlSinkViolations('el["innerHTML"] = "";'), []);
  assert.deepEqual(findHtmlSinkViolations('const h = el["innerHTML"];'), []);
  assert.deepEqual(findHtmlSinkViolations('const v = el["value"]; map["srcdocs"] = 1;'), []);
});

test("reports the line of each violation in order", () => {
  const found = findHtmlSinkViolations("a();\nel.innerHTML = x;\nb();\nnew DOMParser().parseFromString(t, 'x');");
  assert.equal(found.length, 2);
  assert.match(found[0], /^line 2:/);
  assert.match(found[1], /^line 4:/);
});

test("a parse that stays is named by its file", () => {
  const code = "new DOMParser().parseFromString(text, 'image/svg+xml');";
  assert.equal(findHtmlSinkViolations(code, "crates/easyoffice-app/ui/other.js").length, 1);
  assert.deepEqual(findHtmlSinkViolations(code, "crates/easyoffice-app/ui/hwp-print.js"), []);
});

test("ignores the new sinks in comments and strings", () => {
  assert.deepEqual(findHtmlSinkViolations("// document.write(x)\n/* a.parseFromString(b) */"), []);
  assert.deepEqual(findHtmlSinkViolations('const s = "frame.srcdoc = x; el[\'innerHTML\']";'), []);
});
