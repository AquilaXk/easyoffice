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
    const jobsSection = text.slice(text.indexOf("\njobs:\n") + 1);
    const jobs = jobsSection.split(/\n(?= {2}[a-z][\w-]*:\s*(?:#.*)?\n)/).slice(1);
    assert.ok(jobs.length >= 1, `${rel} has no jobs`);
    for (const job of jobs) {
      const jobName = job.match(/^ {2}([\w-]+):/)[1];
      assert.match(job, /^\s{4}timeout-minutes:\s+\d+/m, `${rel} job '${jobName}' must set timeout-minutes`);
    }
  }
});

test("every actions/checkout step sets persist-credentials: false", () => {
  for (const rel of allCiYaml) {
    const text = readText(path.join(REPO, rel));
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      if (!lines[i].includes("actions/checkout@")) continue;
      const block = lines.slice(i, Math.min(i + 8, lines.length)).join("\n");
      assert.match(block, /persist-credentials:\s*false/, `${rel}:${i + 1} actions/checkout must set persist-credentials: false`);
    }
  }
});

test("all literal script paths, working directories, --prefix dirs, and hashFiles lockfiles exist", () => {
  for (const rel of allCiYaml) {
    const text = readText(path.join(REPO, rel));
    for (const match of text.matchAll(/(?:scripts\/[A-Za-z0-9_./-]+\.mjs)/g)) {
      const scriptRel = match[0];
      assert.ok(fs.existsSync(path.join(REPO, scriptRel)), `${rel} references missing script '${scriptRel}'`);
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
      for (const pathMatch of match[1].matchAll(/'([^']+)'/g)) {
        const lockRel = pathMatch[1];
        if (lockRel.includes("*")) continue;
        assert.ok(fs.existsSync(path.join(REPO, lockRel)), `${rel} hashFiles references missing file '${lockRel}'`);
      }
    }
  }
});
