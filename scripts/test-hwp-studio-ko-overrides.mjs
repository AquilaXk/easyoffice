// Key-level ko catalog overrides: unit cases plus the real upstream catalog and built bundle.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { applyKoOverrides, readKoOverrides } from "./hwp-studio-ko-overrides.mjs";
import { listFiles } from "./check-hwp-studio-bundle.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const UPSTREAM_KO = path.join(REPO, "third_party/rhwp/rhwp-studio/src/i18n/locales/ko.ts");
const BUNDLE = path.join(REPO, "crates/easyoffice-app/ui/hwp-studio");
const OLD_TERM = "커맨드";
const OLD_PLACEHOLDER = "커맨드 검색";

test("applyKoOverrides replaces only the named value", () => {
  const code = 'const c = {\n  "a.b": "커맨드 검색...",\n  "a.c": "그대로",\n};\n';
  assert.equal(applyKoOverrides(code, { "a.b": "명령 검색..." }), 'const c = {\n  "a.b": "명령 검색...",\n  "a.c": "그대로",\n};\n');
});

test("applyKoOverrides rejects missing and duplicate keys", () => {
  assert.throws(() => applyKoOverrides('  "a": "x",\n', { b: "y" }), /matched 0 times/);
  assert.throws(() => applyKoOverrides('  "a": "x",\n  "a": "z",\n', { a: "y" }), /matched 2 times/);
});

test("overrides leave no 커맨드 in the upstream ko catalog", { skip: !fs.existsSync(UPSTREAM_KO) }, () => {
  const out = applyKoOverrides(fs.readFileSync(UPSTREAM_KO, "utf8"), readKoOverrides(REPO));
  assert.equal(out.includes(OLD_TERM), false);
  assert.equal(out.includes("토글"), false);
  assert.ok(out.includes("명령 검색..."));
  assert.ok(out.includes("글머리표 켜기/끄기"));
});

test("built bundle shows 명령 검색... and no 커맨드", { skip: !fs.existsSync(path.join(BUNDLE, "index.html")) }, () => {
  const text = listFiles(BUNDLE)
    .filter((rel) => /\.(js|html)$/.test(rel))
    .map((rel) => fs.readFileSync(path.join(BUNDLE, rel), "utf8"))
    .join("\n");
  assert.equal(text.includes(OLD_PLACEHOLDER), false);
  assert.ok(text.includes("명령 검색..."));
});
