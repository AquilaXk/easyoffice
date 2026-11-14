import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { backoffMs, isDownloadFailure, runWithRetry } from "./retry-on-download-failure.mjs";
import { readText } from "./test-support/read-text.mjs";

test("bundler download errors are retryable, compile and test errors are not", () => {
  assert.ok(isDownloadFailure("Downloading https://github.com/x/AppRun-x86_64\nfailed to bundle project: `http status: 504`"));
  assert.ok(isDownloadFailure("failed to bundle project: error sending request for url (https://github.com/x)"));
  assert.ok(isDownloadFailure("failed to bundle project: `http status: 429`"));
  assert.ok(!isDownloadFailure("failed to bundle project: `http status: 404`"));
  assert.ok(!isDownloadFailure("error[E0308]: mismatched types\nerror: could not compile `easyoffice-desktop`"));
  assert.ok(!isDownloadFailure("http status: 504 in a test log without the bundle stage"));
  assert.ok(!isDownloadFailure("failed to bundle project: `wix toolset failed`\nhttp status: 504"));
});

test("backoff doubles per failed attempt", () => {
  assert.deepEqual([1, 2, 3].map((n) => backoffMs(n, 10)), [10, 20, 40]);
});

const counterScript = (dir, body) => {
  const file = path.join(dir, "cmd.mjs");
  fs.writeFileSync(
    file,
    `import fs from "node:fs";
const f = ${JSON.stringify(path.join(dir, "count"))};
const n = (fs.existsSync(f) ? Number(fs.readFileSync(f, "utf8")) : 0) + 1;
fs.writeFileSync(f, String(n));
${body}`,
  );
  return { file, attempts: () => Number(fs.readFileSync(path.join(dir, "count"), "utf8")) };
};

const withDir = async (fn) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "retry-test-"));
  try {
    await fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
};

test("retries a download failure until it succeeds", async () => {
  await withDir(async (dir) => {
    const cmd = counterScript(dir, `if (n < 3) { console.error("failed to bundle project: \`http status: 504\`"); process.exit(1); }`);
    assert.equal(await runWithRetry([process.execPath, cmd.file], { baseDelayMs: 1 }), 0);
    assert.equal(cmd.attempts(), 3);
  });
});

test("gives up after three attempts and keeps the exit code", async () => {
  await withDir(async (dir) => {
    const cmd = counterScript(dir, `console.error("failed to bundle project: \`http status: 504\`"); process.exit(7);`);
    assert.equal(await runWithRetry([process.execPath, cmd.file], { baseDelayMs: 1 }), 7);
    assert.equal(cmd.attempts(), 3);
  });
});

test("does not retry a compile failure", async () => {
  await withDir(async (dir) => {
    const cmd = counterScript(dir, `console.error("error: could not compile"); process.exit(101);`);
    assert.equal(await runWithRetry([process.execPath, cmd.file], { baseDelayMs: 1 }), 101);
    assert.equal(cmd.attempts(), 1);
  });
});

// Workflow structure (PD-118): every bundling build retries downloads and has the tool cache restored.
const read = (rel) => readText(new URL(`../${rel}`, import.meta.url));
const CLI_BUILD = /@tauri-apps\/cli@(\d+\.\d+\.\d+) build/g;

// CI runs the CLI through npx; the release builds run the copy installed from the committed lockfile.
const RELEASE_CLI = 'node "$GITHUB_WORKSPACE/scripts/tauri-cli/node_modules/@tauri-apps/cli/tauri.js"';

test("every Tauri build runs through the retry wrapper", () => {
  for (const [file, runner] of [[".github/workflows/ci.yml", "npx "], [".github/workflows/release.yml", RELEASE_CLI]]) {
    const lines = read(file).split("\n").filter((line) => line.includes(runner) && line.includes(" build "));
    assert.ok(lines.length > 0, `${file} has no bundling build`);
    for (const line of lines) {
      assert.ok(line.includes(`scripts/retry-on-download-failure.mjs" -- ${runner}`), `${file}: ${line.trim()}`);
    }
  }
});

test("the tool cache is restored for the CI Linux and Windows builds and every release build", () => {
  const ci = read(".github/workflows/ci.yml");
  assert.equal(ci.split("uses: ./.github/actions/cache-tauri-tools").length - 1, 2);
  assert.ok(read(".github/actions/prepare-desktop-build/action.yml").includes("uses: ./.github/actions/cache-tauri-tools"));
  const action = read(".github/actions/cache-tauri-tools/action.yml");
  assert.match(action, /uses: actions\/cache@[0-9a-f]{40} # v/);
  assert.ok(action.includes("~/.cache/tauri") && action.includes("~/AppData/Local/tauri"));
  assert.match(action, /key: tauri-tools-\$\{\{ runner\.os \}\}-\$\{\{ runner\.arch \}\}-cli-\$\{\{ inputs\.tauri-cli-version \}\}/);
});

test("the cached CLI version equals the version the builds run", () => {
  const versions = new Set();
  for (const file of [".github/workflows/ci.yml", ".github/workflows/release.yml"]) {
    for (const match of read(file).matchAll(CLI_BUILD)) versions.add(match[1]);
  }
  for (const file of [".github/workflows/ci.yml", ".github/actions/prepare-desktop-build/action.yml"]) {
    for (const match of read(file).matchAll(/tauri-cli-version: (\S+)/g)) versions.add(match[1]);
  }
  versions.add(JSON.parse(read("scripts/tauri-cli/package.json")).dependencies["@tauri-apps/cli"]);
  assert.equal(versions.size, 1, `Tauri CLI versions differ: ${[...versions].join(", ")}`);
});
