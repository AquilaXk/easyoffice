import assert from "node:assert/strict";
import test from "node:test";
import { findServeReleaseViolations } from "./release-serve-rule.mjs";
import { readText } from "./test-support/read-text.mjs";

test("flags a published cli-serve archive", () => {
  const yml = 'ARCHIVE="EasyOffice-${VERSION}-windows-x64-cli-serve-preview.zip"';
  assert.ok(findServeReleaseViolations(yml).length >= 1);
});

test("flags --serve invocations and the CLI binary", () => {
  assert.equal(findServeReleaseViolations("run: app --serve 8787").length, 1);
  assert.ok(findServeReleaseViolations("cp target/release/easyoffice-app.exe dist/").length >= 1);
});

test("ignores comment lines", () => {
  assert.deepEqual(findServeReleaseViolations("# --serve is dev only\nrun: echo ok"), []);
});

test("accepts installer-only assets", () => {
  const yml = '"release-assets/EasyOffice-${VERSION}-windows-x64-setup.exe"\n"release-assets/SHA256SUMS-windows-x64.txt"';
  assert.deepEqual(findServeReleaseViolations(yml), []);
});

test("the real release workflow is clean", () => {
  const text = readText(new URL("../.github/workflows/release.yml", import.meta.url));
  assert.deepEqual(findServeReleaseViolations(text), []);
});
