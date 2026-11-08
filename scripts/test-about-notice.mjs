import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { MIME_EXCEPTIONS, NOTICE_EXCEPTIONS, PROHIBITED_PATTERNS, allowedMimeTokens, allowedNotices, prohibitedPatternsIn } from "./prohibited-terms.mjs";

// About dialog notice and license table (PD-122) and the narrow hygiene exception that carries the notice.

const read = (rel) => fs.readFileSync(fileURLToPath(new URL(`../${rel}`, import.meta.url)), "utf8");
const INDEX = "crates/easyoffice-app/ui/index.html";
const [{ textB64, files }] = NOTICE_EXCEPTIONS;
const NOTICE = Buffer.from(textB64, "base64").toString("utf8");
const COMPANY = PROHIBITED_PATTERNS.at(-1).source;
const index = read(INDEX);

test("the About dialog carries the required notice sentence exactly once, in a note block", () => {
  assert.equal(index.split(NOTICE).length - 1, 1);
  assert.match(index, new RegExp(`<p class="about-notice">${NOTICE.replace(/[().]/g, "\\$&")}</p>`));
});

const licenseTokens = (text) => new Set(text.split(/[\s()/]+/).filter((t) => t && t !== "OR" && t !== "AND"));

test("every license row of the About dialog is a crate of the WebAssembly notice table with the same license", () => {
  const notices = read("THIRD_PARTY_NOTICES.md");
  const table = new Map([...notices.matchAll(/^\| ([\w-]+) \| [\d.]+ \| (.+?) \|$/gm)].map((m) => [m[1], m[2]]));
  const rows = [...index.matchAll(/<tr><td>([^<]+)<\/td><td>([^<]+)<\/td><\/tr>/g)];
  assert.ok(rows.length >= 10, "the license table is present");
  for (const [, names, license] of rows) {
    for (const name of names.split(" / ")) {
      assert.ok(table.has(name), `${name} is listed in THIRD_PARTY_NOTICES.md`);
      assert.deepEqual([...licenseTokens(table.get(name))].sort((a, b) => a.localeCompare(b)), [...licenseTokens(license)].sort((a, b) => a.localeCompare(b)), name);
    }
  }
});

test("the exception is the exact sentence in the About page only", () => {
  assert.deepEqual(files, [INDEX]);
  assert.deepEqual(allowedNotices(INDEX), [NOTICE]);
  assert.deepEqual(allowedNotices("crates/easyoffice-app/ui/app.js"), []);
  assert.equal(prohibitedPatternsIn(INDEX, index).length, 0, "the shipped page passes");
});

test("the company name stays prohibited outside the exact sentence", () => {
  assert.ok(NOTICE.includes(COMPANY), "the notice names the company");
  assert.ok(prohibitedPatternsIn("docs/other.md", NOTICE).length > 0, "same sentence in another file");
  assert.ok(prohibitedPatternsIn(INDEX, `${NOTICE} ${COMPANY}`).length > 0, "name added elsewhere in the page");
  assert.ok(prohibitedPatternsIn(INDEX, NOTICE.replace(".hwp", ".hwpx")).length > 0, "reworded sentence in the page");
  assert.ok(prohibitedPatternsIn(INDEX, `${COMPANY}의 소식`).length > 0, "another sentence with the name");
});

test("a reverse-domain identifier of another company is rejected, this app's own is not", () => {
  const foreign = ["com.", "han", "com", ".hwp"].join("");
  assert.ok(prohibitedPatternsIn("crates/x.json", `"${foreign}"`).length > 0);
  assert.equal(prohibitedPatternsIn("crates/x.json", '"com.aquilaxk.easyoffice.hwp"').length, 0);
});

test("the short company name in English is rejected in prose, the registered MIME token passes only where it is used", () => {
  const shortName = ["han", "com"].join("");
  const [{ files }] = MIME_EXCEPTIONS;
  const mime = allowedMimeTokens(files[0])[0];
  assert.equal(mime, `application/vnd.${shortName}.hwpx`);
  assert.deepEqual(allowedMimeTokens("docs/other.md"), []);
  for (const rel of files) assert.equal(prohibitedPatternsIn(rel, `"mimeType": "${mime}"`).length, 0, rel);
  for (const text of [shortName, `the ${shortName.toUpperCase()} families`, `${mime} ${shortName}`]) {
    assert.ok(prohibitedPatternsIn(files[0], text).length > 0, `in an allowed file: ${text}`);
    assert.ok(prohibitedPatternsIn("docs/other.md", text).length > 0, text);
  }
  assert.ok(prohibitedPatternsIn("docs/other.md", mime).length > 0, "the token in another file");
  assert.ok(PROHIBITED_PATTERNS.some((pattern) => pattern.test(mime)), "commit messages get no exception");
  assert.ok(prohibitedPatternsIn("crates/x.json", `"com.${shortName}.hwp"`).length > 0, "reverse-domain id");
});

test("the short company name is rejected with a particle attached, in every file and in commit messages", () => {
  const shortName = Buffer.from("7ZWc7Lu0", "base64").toString("utf8");
  for (const text of [shortName, `${shortName}의 소식`, `이${shortName}이`]) {
    assert.ok(prohibitedPatternsIn("docs/other.md", text).length > 0, text);
    assert.ok(prohibitedPatternsIn(INDEX, text).length > 0, `no notice exception: ${text}`);
    assert.ok(PROHIBITED_PATTERNS.some((pattern) => pattern.test(text)), text);
  }
});
