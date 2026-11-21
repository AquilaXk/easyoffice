// PD-283: a public release is never rebuilt or overwritten, and the publish job records the evidence for
// a 403 on the release write. The workflow's shell steps run here against a stub `gh`, so the branches
// that decide skip, refuse and write are exercised, not only matched as text.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { readText } from "./test-support/read-text.mjs";

const workflow = readText(new URL("../.github/workflows/release.yml", import.meta.url));
const hasBash = process.platform !== "win32";

/** The text of one job, from its key to the next job key. */
function job(key) {
  const start = workflow.indexOf(`\n  ${key}:\n`);
  assert.ok(start >= 0, `${key} job exists`);
  const next = workflow.slice(start + 1).search(/\n {2}[a-z][a-z-]*:\n/);
  return next < 0 ? workflow.slice(start) : workflow.slice(start, start + 1 + next);
}

/** The `run: |` script of the step with this name, dedented. */
function stepScript(jobText, name) {
  const at = jobText.indexOf(`- name: ${name}\n`);
  assert.ok(at >= 0, `step "${name}" exists`);
  const afterRun = jobText.slice(at).split("        run: |\n")[1];
  assert.ok(afterRun, `step "${name}" has a run script`);
  const lines = [];
  for (const line of afterRun.split("\n")) {
    if (line !== "" && !line.startsWith("          ")) break;
    lines.push(line.slice(10));
  }
  return lines.join("\n");
}

test("a public release stops a tag push before any build and never lets a build job or the publish job run", () => {
  const prepare = job("prepare");
  assert.match(prepare, /skip: \$\{\{ steps\.published\.outputs\.skip \}\}/);
  assert.match(prepare, /releases\/tags\/\$TAG/);
  for (const key of ["release-macos", "build-windows-release", "build-linux-release"]) {
    assert.match(job(key), /\n {4}if: needs\.prepare\.outputs\.skip != 'true'\n/, `${key} is skipped for a public release`);
  }
  assert.match(job("publish-release"), /\n {4}if: needs\.prepare\.outputs\.skip != 'true' && \(github\.event_name != 'workflow_dispatch' \|\| inputs\.publish\)\n/);
});

test("the publish job has no path that uploads over a published release", () => {
  const publish = job("publish-release");
  assert.doesNotMatch(publish, /TAG_SHA/, "no same-commit exception");
  const script = stepScript(publish, "Create or update the GitHub Release");
  const refuse = script.indexOf('if [ "$IS_DRAFT" != true ]');
  const upload = script.indexOf("gh release upload");
  assert.ok(refuse > 0 && refuse < upload, "the published check comes before the upload");
  assert.match(script.slice(refuse, upload), /already published[\s\S]*exit 1/);
  assert.equal(script.split("--clobber").length - 1, 1, "--clobber exists only on the draft upload");
});

test("the release write diagnostics never print the token", () => {
  const publish = job("publish-release");
  const recorded = stepScript(publish, "Record the token's release access and the workflow drift of the target");
  const write = stepScript(publish, "Create or update the GitHub Release");
  assert.ok(publish.indexOf("Record the token's release access") < publish.indexOf("- name: Create or update the GitHub Release"), "recorded before the first write");
  assert.match(recorded, /x-accepted-github-permissions/);
  assert.match(recorded, /x-github-request-id/);
  assert.match(write, /< x-accepted-github-permissions:\|< x-github-request-id:/);
  for (const script of [recorded, write]) {
    assert.doesNotMatch(script, /echo[^\n]*\$GH_TOKEN|printenv|\benv\b\s*(\||$)|set -x/);
  }
});

/** Runs a workflow script in a temp dir with a stub `gh` first in PATH; returns the calls it saw. */
function run(script, { env, ghScript }) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "release-guard-"));
  try {
    fs.mkdirSync(path.join(dir, "bin"));
    fs.mkdirSync(path.join(dir, "release-assets"));
    fs.writeFileSync(path.join(dir, "release-assets", "a.txt"), "a");
    fs.writeFileSync(path.join(dir, "bin", "gh"), `#!/bin/bash\necho "$*" >> "$CALLS"\n${ghScript}\n`, { mode: 0o755 });
    fs.writeFileSync(path.join(dir, "script.sh"), script);
    const output = path.join(dir, "output");
    fs.writeFileSync(output, "");
    const result = spawnSync("bash", ["-e", path.join(dir, "script.sh")], {
      cwd: dir,
      encoding: "utf8",
      env: {
        PATH: `${path.join(dir, "bin")}:${process.env.PATH}`,
        CALLS: path.join(dir, "calls"),
        GITHUB_OUTPUT: output,
        RUNNER_TEMP: dir,
        GITHUB_REPOSITORY: "owner/repo",
        GH_TOKEN: "SECRET-TOKEN-VALUE",
        ...env,
      },
    });
    const read = (name) => (fs.existsSync(path.join(dir, name)) ? fs.readFileSync(path.join(dir, name), "utf8") : "");
    return { status: result.status, out: result.stdout + result.stderr, output: read("output"), calls: read("calls") };
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const lookupStub = {
  draft: 'if [[ "$*" == *releases/tags* ]]; then echo "gh: Not Found (HTTP 404)" >&2; exit 1; fi',
  published: 'if [[ "$*" == *releases/tags* ]]; then echo false; exit 0; fi',
  broken: 'if [[ "$*" == *releases/tags* ]]; then echo "gh: Server Error (HTTP 500)" >&2; exit 1; fi',
};
const prepareScript = hasBash ? stepScript(job("prepare"), "Stop when this version's release is already public") : "";
const lookup = (state, env) =>
  run(prepareScript, { ghScript: lookupStub[state], env: { TAG: "v0.2.0", ...env } });

test("prepare: a tag push on a public release skips the run, on a draft or a missing release it builds", { skip: !hasBash }, () => {
  const tagPush = { GITHUB_EVENT_NAME: "push", INPUT_PUBLISH: "" };
  const skipped = lookup("published", tagPush);
  assert.equal(skipped.status, 0);
  assert.match(skipped.output, /skip=true/);
  assert.match(skipped.out, /already published; nothing is rebuilt or uploaded/);
  const built = lookup("draft", tagPush);
  assert.equal(built.status, 0);
  assert.match(built.output, /skip=false/);
});

test("prepare: a manual publishing run on a public version fails at once, a trial build may go on", { skip: !hasBash }, () => {
  const refused = lookup("published", { GITHUB_EVENT_NAME: "workflow_dispatch", INPUT_PUBLISH: "true" });
  assert.notEqual(refused.status, 0);
  assert.match(refused.out, /already published; its assets are never replaced/);
  assert.doesNotMatch(refused.output, /skip=/);
  const trial = lookup("published", { GITHUB_EVENT_NAME: "workflow_dispatch", INPUT_PUBLISH: "false" });
  assert.equal(trial.status, 0);
  assert.match(trial.output, /skip=false/);
});

test("prepare: a lookup error other than 404 fails instead of reading as no release", { skip: !hasBash }, () => {
  const broken = lookup("broken", { GITHUB_EVENT_NAME: "push", INPUT_PUBLISH: "" });
  assert.notEqual(broken.status, 0);
  assert.match(broken.out, /Could not read the release of v0\.2\.0/);
});

const publishScript = hasBash ? stepScript(job("publish-release"), "Create or update the GitHub Release") : "";
const publishEnv = { TAG: "v0.2.0", VERSION: "0.2.0", DRAFT: "true", TARGET_SHA: "abc123" };
const view = (isDraft) => `if [[ "$1 $2" == "release view" && "$*" == *isDraft* ]]; then ${isDraft === null ? "exit 1" : `echo ${isDraft}`}; fi
if [[ "$1 $2" == "release view" && "$*" == *assets* ]]; then echo stale.bin; fi`;
// A failing write the way `GH_DEBUG=api` shows it, with the token in the request header.
const failingWrite = `if [[ "$1 $2" == "release edit" || "$1 $2" == "release create" ]]; then
  {
    echo "> Authorization: token SECRET-TOKEN-VALUE"
    echo "< HTTP/2.0 403 Forbidden"
    echo "< X-Accepted-GitHub-Permissions: contents=write"
    echo "< X-GitHub-Request-Id: ABCD:1234"
    echo "HTTP 403: Resource not accessible by integration"
  } >&2
  exit 1
fi`;

test("publish: a published release gets no write and no upload", { skip: !hasBash }, () => {
  const result = run(publishScript, { ghScript: view("false"), env: publishEnv });
  assert.notEqual(result.status, 0);
  assert.match(result.out, /already published; its assets are never replaced/);
  assert.doesNotMatch(result.calls, /release (edit|upload|create|delete-asset)/);
});

test("publish: a draft moves its target, drops stale files and replaces its assets; no release creates one", { skip: !hasBash }, () => {
  const draft = run(publishScript, { ghScript: view("true"), env: publishEnv });
  assert.equal(draft.status, 0, draft.out);
  assert.match(draft.calls, /release edit v0\.2\.0 --target abc123/);
  assert.match(draft.calls, /release delete-asset v0\.2\.0 stale\.bin/);
  assert.match(draft.calls, /release upload v0\.2\.0 release-assets\/a\.txt --clobber/);
  const created = run(publishScript, { ghScript: view(null), env: publishEnv });
  assert.equal(created.status, 0, created.out);
  assert.match(created.calls, /release create v0\.2\.0 release-assets\/a\.txt --target abc123 .*--draft/);
});

test("publish: a refused write keeps the status and permission headers and never the token", { skip: !hasBash }, () => {
  for (const state of ["true", null]) {
    const result = run(publishScript, { ghScript: `${view(state)}\n${failingWrite}`, env: publishEnv });
    assert.notEqual(result.status, 0);
    assert.match(result.out, /< HTTP\/2\.0 403 Forbidden/);
    assert.match(result.out, /< X-Accepted-GitHub-Permissions: contents=write/i);
    assert.match(result.out, /< X-GitHub-Request-Id: ABCD:1234/i);
    assert.match(result.out, /HTTP 403: Resource not accessible by integration/);
    assert.doesNotMatch(result.out, /SECRET-TOKEN-VALUE/);
  }
});
