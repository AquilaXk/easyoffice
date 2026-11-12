import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { EXEMPT_PREFIXES, MARKDOWN_NAME_HASHES, NAME_HASHES, STANDARD_IDENTIFIERS_B64, ossNameHits, withoutAttributionHeader } from "./oss-names.mjs";

// Open-source project name guard (QA 2026-11-12). The words are built from character codes so this test
// does not contain the names it checks; digests of the same words are compared with the guard's lists.

const word = (...codes) => String.fromCharCode(...codes);
const SPREADSHEET_ENGINE = word(85, 110, 105, 118, 101, 114); // listed in NAME_HASHES
const PDF_CRATE = word(104, 97, 121, 114, 111); // listed in MARKDOWN_NAME_HASHES
const SPREADSHEET_PROGRAM = word(69, 120, 99, 101, 108);
const digest = (w) => createHash("sha256").update(w.toLowerCase()).digest("hex");

test("the guard lists store digests, not words", () => {
  assert.ok(NAME_HASHES.has(digest(SPREADSHEET_ENGINE)));
  assert.ok(MARKDOWN_NAME_HASHES.has(digest(PDF_CRATE)));
  for (const hash of [...NAME_HASHES, ...MARKDOWN_NAME_HASHES]) assert.match(hash, /^[0-9a-f]{64}$/);
});

test("a name is found as a whole word, case-insensitively, also inside a path or a Korean sentence", () => {
  assert.equal(ossNameHits("docs/a.md", `${SPREADSHEET_ENGINE.toUpperCase()} 구현을 참고했습니다.`).length, 1);
  assert.equal(ossNameHits("src/a.rs", `// ${SPREADSHEET_PROGRAM}는 순서를 거부합니다`).length, 1);
  assert.equal(ossNameHits("src/a.rs", `// see ${PDF_CRATE}-write for details`).length, 0, "crate names are rejected in Markdown only");
  assert.equal(ossNameHits("docs/a.md", `${PDF_CRATE}-write 크레이트`).length, 1);
});

test("a longer word that merely contains a name is not a hit", () => {
  assert.equal(ossNameHits("src/a.rs", `// ${SPREADSHEET_ENGINE}sal and ${SPREADSHEET_PROGRAM}lent`).length, 0);
});

test("hits report the line and a digest prefix, never the word", () => {
  const [hit] = ossNameHits("docs/a.md", `첫 줄\n둘째 줄 ${SPREADSHEET_ENGINE}`);
  assert.deepEqual(hit, { line: 2, hash: digest(SPREADSHEET_ENGINE).slice(0, 8) });
});

test("exempt paths and manifests carry names", () => {
  for (const prefix of EXEMPT_PREFIXES) assert.equal(ossNameHits(prefix, SPREADSHEET_ENGINE).length, 0, prefix);
  assert.equal(ossNameHits("crates/x/Cargo.toml", `${PDF_CRATE} = "0.9"`).length, 0);
  assert.equal(ossNameHits("crates/x/src/a.rs", SPREADSHEET_ENGINE).length, 1);
});

test("only the leading comment block of a file that states an adaptation is exempt", () => {
  const header = `//! Reference implementation: ${SPREADSHEET_ENGINE} (MIT).\n//! More.\n\nfn a() {}\n`;
  assert.equal(ossNameHits("src/a.rs", header).length, 0);
  assert.equal(ossNameHits("src/a.rs", `${header}// ${SPREADSHEET_ENGINE} again\n`).length, 1);
  assert.equal(ossNameHits("src/a.rs", `//! ${SPREADSHEET_ENGINE} is mentioned without stating an adaptation\n`).length, 1);
  assert.equal(withoutAttributionHeader(header).split("\n").length, header.split("\n").length, "line numbers stay");
});

test("standard identifiers of a format specification are not hits", () => {
  for (const b64 of STANDARD_IDENTIFIERS_B64) assert.equal(ossNameHits("src/a.rs", `"${Buffer.from(b64, "base64").toString("utf8")}"`).length, 0, b64);
  const company = word(65, 100, 111, 98, 101);
  assert.equal(ossNameHits("src/a.rs", `${company} Reader`).length, 1);
});

test("a Rust symbol path in a Markdown profile table is not prose", () => {
  assert.equal(ossNameHits("docs/a.md", `\`${PDF_CRATE}::render::page\``).length, 0);
  assert.equal(ossNameHits("docs/a.md", `${PDF_CRATE} 렌더`).length, 1);
});
