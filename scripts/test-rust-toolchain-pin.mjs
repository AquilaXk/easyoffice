import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { readText } from "./test-support/read-text.mjs";

const repo = (path) => new URL(`../${path}`, import.meta.url);
const SETUP = "./.github/actions/setup-rust";
const version = readText(repo(".github/rust-toolchain-version")).trim();

/** Splits a workflow into its jobs: [name, text]. */
function jobsOf(text) {
  const body = text.slice(text.indexOf("\njobs:\n") + 1);
  return body.split(/\n(?=  [a-z][\w-]*:\n)/).slice(1).map((job) => [job.match(/^ {2}([\w-]+):/)[1], job]);
}

const rustJobs = (text) =>
  jobsOf(text).filter(([, job]) => /\bcargo\b|build-quicklook-macos|prepare-desktop-build/.test(job));

test("the pinned version is an exact x.y.z", () => {
  assert.match(version, /^\d+\.\d+\.\d+$/);
});

test("the version is written in one file only", () => {
  const files = [
    ".github/workflows/ci.yml",
    ".github/workflows/release.yml",
    ".github/actions/setup-rust/action.yml",
    ".github/actions/prepare-desktop-build/action.yml",
  ];
  for (const file of files) {
    assert.ok(!readText(repo(file)).includes(version), `${file} must read the version file, not repeat it`);
  }
  assert.ok(readText(repo(".github/actions/setup-rust/action.yml")).includes(".github/rust-toolchain-version"));
  assert.ok(!fs.existsSync(repo("rust-toolchain.toml")), "no root rust-toolchain.toml (local development stays free)");
});

test("the setup action installs the pinned toolchain and exports it to later steps", () => {
  const action = readText(repo(".github/actions/setup-rust/action.yml"));
  assert.match(action, /rustup toolchain install "\$VERSION" --profile minimal --component clippy,rustfmt/);
  assert.match(action, /rustup default "\$VERSION"/);
  assert.match(action, /RUSTUP_TOOLCHAIN=\$VERSION" >> "\$GITHUB_ENV"/);
});

for (const file of [".github/workflows/ci.yml", ".github/workflows/release.yml"]) {
  test(`every Rust job in ${file} sets up the pinned toolchain before running cargo`, () => {
    const jobs = rustJobs(readText(repo(file)));
    assert.ok(jobs.length >= 1);
    for (const [name, job] of jobs) {
      const setup = Math.max(job.indexOf(SETUP), job.indexOf("./.github/actions/prepare-desktop-build"));
      assert.ok(setup >= 0, `${file} job ${name} does not set up the pinned Rust toolchain`);
      const firstUse = job.search(/run:.*\bcargo\b|^\s+cargo\b|build-quicklook-macos/m);
      if (firstUse >= 0) assert.ok(setup < firstUse, `${file} job ${name} runs cargo before the toolchain setup`);
    }
  });
}

test("the shared release action sets up the pinned toolchain before its cargo steps", () => {
  const action = readText(repo(".github/actions/prepare-desktop-build/action.yml"));
  assert.ok(action.indexOf(SETUP) >= 0);
  assert.ok(action.indexOf(SETUP) < action.indexOf("cargo test"));
});

test("no step installs or switches a Rust toolchain on its own", () => {
  for (const file of [".github/workflows/ci.yml", ".github/workflows/release.yml", ".github/actions/prepare-desktop-build/action.yml"]) {
    assert.doesNotMatch(readText(repo(file)), /rustup (toolchain|default|update|target)|dtolnay\/rust-toolchain/, file);
  }
});

test("CI Rust caches are keyed by the pinned version", () => {
  const ci = readText(repo(".github/workflows/ci.yml"));
  const caches = ci.match(/Swatinem\/rust-cache@[^\n]*\n(?: {8}with:\n(?: {10}[^\n]*\n)+)?/g) ?? [];
  assert.ok(caches.length >= 4);
  for (const cache of caches) assert.match(cache, /key: rust-\$\{\{ steps\.rust\.outputs\.version \}\}/);
});
