import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { readText } from "./test-support/read-text.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function listYamlFiles(dirRel) {
  const full = path.join(REPO, dirRel);
  if (!fs.existsSync(full)) return [];
  const results = [];
  for (const entry of fs.readdirSync(full, { withFileTypes: true })) {
    const rel = path.posix.join(dirRel, entry.name);
    if (entry.isDirectory()) {
      const actionFile = path.posix.join(rel, "action.yml");
      if (fs.existsSync(path.join(REPO, actionFile))) results.push(actionFile);
    } else if (entry.isFile() && (entry.name.endsWith(".yml") || entry.name.endsWith(".yaml"))) {
      results.push(rel);
    }
  }
  return results.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

const workflows = listYamlFiles(".github/workflows");
const actions = listYamlFiles(".github/actions");
const allCiYaml = [...workflows, ...actions];

const REPO_PATH_RE =
  /(?<![A-Za-z0-9_./-])((?:scripts|\.github\/(?:workflows|actions)|apps\/[A-Za-z0-9_-]+|crates\/[A-Za-z0-9_-]+)\/[A-Za-z0-9_./@-]+\.(?:mjs|sh|ps1|json|yml|yaml|toml|rs))(?![A-Za-z0-9_/-])/g;

test("every external GitHub Action is pinned to a 40-char commit SHA with a version comment", () => {
  assert.ok(workflows.length >= 2);
  for (const rel of allCiYaml) {
    const lines = readText(path.join(REPO, rel)).split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      const match = lines[i].match(/^\s*(?:-\s*)?uses:\s*(.+?)\s*$/);
      if (!match) continue;
      const target = match[1];
      if (target.startsWith("./")) {
        const localAction = path.join(REPO, target.slice(2), "action.yml");
        assert.ok(fs.existsSync(localAction), `${rel}:${i + 1} local action '${target}' is missing action.yml`);
      } else {
        assert.match(
          target,
          /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+(?:\/[A-Za-z0-9_.-]+)*@[0-9a-f]{40}\s+#\s+v\S+/,
          `${rel}:${i + 1} action '${target}' must be pinned to a 40-char hex SHA with a # v... comment`,
        );
      }
    }
  }
});

test("every workflow declares top-level permissions and job timeouts", () => {
  for (const rel of workflows) {
    const text = readText(path.join(REPO, rel));
    assert.match(text, /^permissions:\s*$/m, `${rel} must declare explicit permissions`);
    const jobsIndex = text.indexOf("\njobs:\n");
    assert.ok(jobsIndex >= 0, `${rel} must define top-level jobs:`);
    const jobsSection = text.slice(jobsIndex + 1);
    const jobs = jobsSection.split(/\n(?= {2}[a-z][\w-]*:\s*(?:#.*)?\n)/).slice(1);
    assert.ok(jobs.length >= 1, `${rel} has no jobs`);
    for (const job of jobs) {
      const jobName = job.match(/^ {2}([\w-]+):/)[1];
      assert.match(job, /^\s{4}timeout-minutes:\s+\d+/m, `${rel} job '${jobName}' must set timeout-minutes`);
    }
  }
});

function extractStepBlock(lines, lineIndex) {
  let start = lineIndex;
  while (start > 0 && !/^\s*-\s+/.test(lines[start])) {
    start -= 1;
  }
  const baseIndent = lines[start].search(/\S/);
  let end = start + 1;
  while (end < lines.length) {
    const line = lines[end];
    if (line.trim() !== "") {
      const indent = line.search(/\S/);
      if (indent < baseIndent || (/^\s*-\s+/.test(line) && indent <= baseIndent)) break;
    }
    end += 1;
  }
  return lines.slice(start, end).join("\n");
}

test("every actions/checkout step sets persist-credentials: false within its own step", () => {
  for (const rel of allCiYaml) {
    const lines = readText(path.join(REPO, rel)).split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      if (!lines[i].includes("actions/checkout@")) continue;
      const stepBlock = extractStepBlock(lines, i);
      assert.match(
        stepBlock,
        /persist-credentials:\s*false/,
        `${rel}:${i + 1} actions/checkout must set persist-credentials: false`,
      );
    }
  }
});

test("all workflow and composite-action caches keep pull_request runs read-only", () => {
  let rustCacheCount = 0;
  let cacheSaveStepCount = 0;
  for (const rel of allCiYaml) {
    const lines = readText(path.join(REPO, rel)).split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      const line = lines[i];
      if (line.includes("Swatinem/rust-cache@")) {
        rustCacheCount += 1;
        const stepBlock = extractStepBlock(lines, i);
        assert.match(
          stepBlock,
          /save-if:\s*\$\{\{\s*github\.event_name\s*!=\s*'pull_request'\s*\}\}/,
          `${rel}:${i + 1} Swatinem/rust-cache must set save-if to keep pull_request runs read-only`,
        );
      } else if (/uses:\s*actions\/cache(?:\/save)?@/.test(line) && !line.includes("actions/cache/restore@")) {
        cacheSaveStepCount += 1;
        const stepBlock = extractStepBlock(lines, i);
        assert.match(
          stepBlock,
          /^\s+if:\s*.*github\.event_name\s*!=\s*'pull_request'/m,
          `${rel}:${i + 1} actions/cache save step must exclude pull_request runs`,
        );
      } else if (line.includes("actions/setup-node@") && rel !== ".github/actions/setup-node-ci/action.yml") {
        const stepBlock = extractStepBlock(lines, i);
        assert.doesNotMatch(
          stepBlock,
          /^\s+cache:/m,
          `${rel}:${i + 1} must use ./.github/actions/setup-node-ci rather than actions/setup-node built-in cache`,
        );
      }
    }
  }
  assert.ok(rustCacheCount >= 6, `expected >= 6 Swatinem/rust-cache steps, found ${rustCacheCount}`);
  assert.ok(cacheSaveStepCount >= 4, `expected >= 4 guarded cache write steps, found ${cacheSaveStepCount}`);
});

test("workflow job dependency and release-guard trigger contracts hold", () => {
  const ci = readText(path.join(REPO, ".github/workflows/ci.yml"));
  const ciJobs = ci.slice(ci.indexOf("\njobs:\n") + 1).split(/\n(?= {2}[a-z][\w-]*:\s*(?:#.*)?\n)/).slice(1);
  for (const job of ciJobs) {
    const name = job.match(/^ {2}([\w-]+):/)[1];
    if (name === "sonar") {
      assert.match(job, /^\s{4}needs:\s*\[checks,\s*checks-rust\]/m, "ci.yml sonar must wait on checks and checks-rust");
    } else {
      assert.doesNotMatch(job, /^\s{4}needs:/m, `ci.yml job '${name}' must run in parallel without needs:`);
    }
  }

  const latestGuard = readText(path.join(REPO, ".github/workflows/latest-guard.yml"));
  assert.match(
    latestGuard,
    /on:\s*\n\s+release:\s*\n\s+types:\s*\[published,\s*edited,\s*released\]/,
    "latest-guard.yml must trigger on release published, edited, and released",
  );

  const release = readText(path.join(REPO, ".github/workflows/release.yml"));
  const releaseJobs = release.slice(release.indexOf("\njobs:\n") + 1).split(/\n(?= {2}[a-z][\w-]*:\s*(?:#.*)?\n)/).slice(1);
  assert.equal(releaseJobs.length, 6, "release.yml must define the 6 release jobs");
  for (const job of releaseJobs) {
    const jobName = job.match(/^ {2}([\w-]+):/)[1];
    assert.match(
      job,
      /uses:\s*\.\/\.github\/actions\/assert-sha[\s\S]*?expected-sha:\s*\$\{\{\s*github\.sha\s*\}\}/,
      `release.yml job '${jobName}' must verify checked-out commit via ./.github/actions/assert-sha`,
    );
    const checkoutIdx = job.indexOf("actions/checkout@");
    const assertShaIdx = job.indexOf("./.github/actions/assert-sha");
    assert.ok(
      checkoutIdx >= 0 && assertShaIdx > checkoutIdx,
      `release.yml job '${jobName}' must run ./.github/actions/assert-sha after actions/checkout`,
    );
  }

  const timingsBlock = release.slice(release.indexOf("\n  release-timings:"));
  assert.match(
    timingsBlock,
    /^\s{4}if:\s*always\(\)\s*&&\s*!cancelled\(\)\s*&&\s*needs\.release-macos\.result\s*!=\s*'skipped'/m,
    "release.yml release-timings must guard against cancelled runs and skipped tag pushes",
  );
});

test("all literal repo paths, working directories, --prefix dirs, and hashFiles lockfiles exist", () => {
  for (const rel of allCiYaml) {
    const text = readText(path.join(REPO, rel));
    for (const match of text.matchAll(REPO_PATH_RE)) {
      const targetRel = match[1];
      if (/[$*{}]/.test(targetRel) || /\/(target|dist)\//.test(targetRel)) continue;
      assert.ok(fs.existsSync(path.join(REPO, targetRel)), `${rel} references missing path '${targetRel}'`);
    }
    for (const match of text.matchAll(/^\s+working-directory:\s*([A-Za-z0-9_./-]+)\s*$/gm)) {
      const dirRel = match[1];
      assert.ok(fs.existsSync(path.join(REPO, dirRel)), `${rel} references missing working-directory '${dirRel}'`);
    }
    for (const match of text.matchAll(/--prefix\s+([A-Za-z0-9_./-]+)/g)) {
      const prefixRel = match[1];
      assert.ok(fs.existsSync(path.join(REPO, prefixRel)), `${rel} references missing --prefix '${prefixRel}'`);
    }
    for (const match of text.matchAll(/hashFiles\(([^)]+)\)/g)) {
      for (const pathMatch of match[1].matchAll(/['"]([^'"]+)['"]/g)) {
        const lockRel = pathMatch[1];
        if (lockRel.includes("*")) continue;
        assert.ok(fs.existsSync(path.join(REPO, lockRel)), `${rel} hashFiles references missing file '${lockRel}'`);
      }
    }
  }
});
