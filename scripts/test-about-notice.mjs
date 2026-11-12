import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { MIME_EXCEPTIONS, NOTICE_EXCEPTIONS, PROHIBITED_PATTERNS, UTI_EXCEPTIONS, allowedMimeTokens, allowedNotices, allowedUtiTokens, prohibitedPatternsIn } from "./prohibited-terms.mjs";
import { readText } from "./test-support/read-text.mjs";

// About dialog notice and license table (PD-122) and the narrow hygiene exception that carries the notice.

const read = (rel) => readText(fileURLToPath(new URL(`../${rel}`, import.meta.url)));
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

const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const sortedTokens = (text) => [...licenseTokens(text)].sort((a, b) => a.localeCompare(b));

test("every license row of the About dialog is backed by a line of THIRD_PARTY_NOTICES.md that names it and its license", () => {
  const notices = read("THIRD_PARTY_NOTICES.md");
  const wasmTable = new Map([...notices.matchAll(/^\| ([\w-]+) \| [\d.]+ \| (.+?) \|$/gm)].map((m) => [m[1], m[2]]));
  const lines = notices.split("\n");
  const rows = [...index.matchAll(/<tr><td>([^<]+)<\/td><td>([^<]+)<\/td><\/tr>/g)];
  assert.ok(rows.length >= 10, "the license table is present");
  for (const [, names, license] of rows) {
    for (const name of names.split(" / ")) {
      if (wasmTable.has(name)) {
        assert.deepEqual(sortedTokens(wasmTable.get(name)), sortedTokens(license), name);
        continue;
      }
      const named = new RegExp(`(?<![\\w-])${escapeRegExp(name)}(?![\\w-])`);
      const backed = lines.some((line) => named.test(line) && [...licenseTokens(license)].every((token) => line.includes(token)));
      assert.ok(backed, `${name} (${license}) has a line in THIRD_PARTY_NOTICES.md with its name and license`);
    }
  }
});

test("the About dialog points to THIRD_PARTY_NOTICES.md as a label and carries no diagnostics line", () => {
  assert.match(index, /<div class="about-license-note">전체 목록: THIRD_PARTY_NOTICES\.md<\/div>/);
  const about = index.slice(index.indexOf('<dialog id="about-dialog"'), index.indexOf("</dialog>", index.indexOf('<dialog id="about-dialog"')));
  assert.doesNotMatch(about, /about-diagnostics|platform-pill|status-module|status-format-spec|ime-badge|status-engine/);
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

test("only the two exact UTI tokens pass, only in the files that list them, and never in commit messages", () => {
  const [{ files }] = UTI_EXCEPTIONS;
  const shortName = ["han", "com"].join("");
  const [hwp, hwpx] = allowedUtiTokens(files[0]);
  assert.equal(hwp, `com.${shortName}.hwp`);
  assert.equal(hwpx, `com.${shortName}.hwpx`);
  assert.deepEqual(allowedUtiTokens("docs/other.md"), []);
  for (const rel of files) {
    assert.equal(prohibitedPatternsIn(rel, `<string>${hwp}</string><string>${hwpx}</string>`).length, 0, rel);
    for (const text of [`${hwp}x2`, `${hwp}z`, `${hwpx}.bak`, `com.${shortName}.hwpml`, `com.${shortName}.other`, `${hwp} ${shortName}`]) {
      assert.ok(prohibitedPatternsIn(rel, text).length > 0, `in an allowed file: ${text}`);
    }
  }
  for (const text of [hwp, hwpx]) {
    assert.ok(prohibitedPatternsIn("docs/other.md", text).length > 0, `in another file: ${text}`);
    assert.ok(prohibitedPatternsIn("scripts/test-about-notice.mjs", text).length > 0, `in the test: ${text}`);
    assert.ok(PROHIBITED_PATTERNS.some((pattern) => pattern.test(text)), "commit messages get no exception");
  }
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
