import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";
import { findIntakeViolations } from "./intake-rules.mjs";
import { readText } from "./test-support/read-text.mjs";

test("allows the bug report form, its config and CONTRIBUTING", () => {
  const paths = [".github/ISSUE_TEMPLATE/bug_report.yml", ".github/ISSUE_TEMPLATE/config.yml", "CONTRIBUTING.md"];
  assert.deepEqual(findIntakeViolations(paths), []);
});

test("rejects other issue forms, pull request templates and label configuration", () => {
  const bad = [
    ".github/ISSUE_TEMPLATE/feature_request.yml",
    ".github/PULL_REQUEST_TEMPLATE.md",
    "docs/pull_request_template.md",
    ".github/PULL_REQUEST_TEMPLATE/default.md",
    ".github/labels.yml",
    ".github/labeler.yaml",
    ".github/ISSUE_TEMPLATE.md",
  ];
  assert.equal(findIntakeViolations(bad).length, bad.length);
});

test("the bug report form is bug-only, keeps blank issues off and the two required checks", () => {
  const form = readText(new URL("../.github/ISSUE_TEMPLATE/bug_report.yml", import.meta.url));
  assert.match(form, /labels: \["bug"\]/);
  assert.equal((form.match(/required: true/g) ?? []).length >= 9, true);
  const config = readText(new URL("../.github/ISSUE_TEMPLATE/config.yml", import.meta.url));
  assert.match(config, /^blank_issues_enabled: false$/m);
  assert.match(config, /security\/advisories\/new/);
});

test("the tracked tree follows the intake rules", () => {
  const tracked = execFileSync("git", ["ls-files"], { encoding: "utf8" }).split("\n").filter(Boolean);
  assert.deepEqual(findIntakeViolations(tracked), []);
});
