// Tests for scripts/update-rhwp.mjs. The remote and the git/checkout side effects are injected
// fakes, so nothing touches the network or a real checkout.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { normalizedSha256Text } from "./hwp-studio-overrides.mjs";
import { commitFromLsRemote, parseArgs, runUpdate } from "./update-rhwp.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OLD_COMMIT = "1a76570e833917d15817415a53c09ad61ab3203f";
const NEW_COMMIT = "2b87681f944a28e26928e463026b4c4d4e2d4f40";
const PEELED = "3c98792fa55b39e37039e5c5e5d5e5d5e5d5e5d5";
const CONFIG_REL = "config/rhwp-upstream.json";
const MANIFEST_REL = "config/rhwp-studio-overrides.json";
const MAIN_TS = "export const main = 1;\n";
const ASSET = "<svg/>\n";

function stringify(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

const roots = [];
test.after(() => {
  for (const root of roots) fs.rmSync(root, { recursive: true, force: true });
});

function makeRoot() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "eo-update-rhwp-"));
  roots.push(root);
  fs.mkdirSync(path.join(root, "config"));
  const config = JSON.parse(fs.readFileSync(path.join(REPO, CONFIG_REL), "utf8"));
  const manifest = {
    schemaVersion: 1,
    upstream: {
      version: config.version,
      commit: config.commit,
      counterparts: { main: normalizedSha256Text(MAIN_TS) },
      assets: { "public/a.svg": normalizedSha256Text(ASSET) },
    },
    overrides: [],
    assetOverrides: [],
  };
  fs.writeFileSync(path.join(root, CONFIG_REL), stringify(config));
  fs.writeFileSync(path.join(root, MANIFEST_REL), stringify(manifest));
  return root;
}

function snapshot(root) {
  return [CONFIG_REL, MANIFEST_REL].map((rel) => fs.readFileSync(path.join(root, rel), "utf8"));
}

function readJson(root, rel) {
  return JSON.parse(fs.readFileSync(path.join(root, rel), "utf8"));
}

/** Fake remote: tags map to ls-remote output, files map `${commit}:${path}` to text. */
function makeRemote({ tags = { "v0.9.0": `${NEW_COMMIT}\trefs/tags/v0.9.0` }, files = {} } = {}) {
  const base = {
    "Cargo.toml": '[package]\nname = "rhwp"\nversion = "0.9.0"\n',
    "rust-toolchain.toml": '[toolchain]\nchannel = "1.94.0"\n',
    "rhwp-studio/src/main.ts": MAIN_TS,
    "rhwp-studio/public/a.svg": ASSET,
  };
  const content = { ...base, ...files };
  return {
    lsRemote: (_url, tag) => tags[tag] ?? "",
    readFileAt: (_url, commit, rel) => (commit === NEW_COMMIT ? content[rel] ?? null : null),
  };
}

function makeEnv(root, remote) {
  const calls = [];
  const logs = [];
  return {
    calls,
    logs,
    root,
    remote,
    log: (message) => logs.push(message),
    setGitlink: (sha) => calls.push(`gitlink ${sha}`),
    checkout: () => calls.push("checkout"),
  };
}

test("accepts only explicit vX.Y.Z tags", () => {
  assert.equal(parseArgs(["v0.9.0"]).tag, "v0.9.0");
  assert.deepEqual(parseArgs(["v0.9.0", "--dry-run", "--accept-overrides"]), { tag: "v0.9.0", dryRun: true, acceptOverrides: true });
  const rejected = [
    [], ["main"], ["latest"], ["0.9.0"], ["v0.9"], ["v0.9.0-rc.1"], ["v0.9.0+build"], [NEW_COMMIT],
    ["v1.2.3", "v1.2.4"], ["v1.2.3", "--force"], ["--dry-run"], ["v12345.0.0"], ["v1.2.3\nmain"], ["-vv1.2.3"],
  ];
  for (const args of rejected) assert.throws(() => parseArgs(args), /usage|Unexpected argument/, JSON.stringify(args));
});

test("resolves the tag to a commit and prefers the peeled commit of an annotated tag", () => {
  assert.equal(commitFromLsRemote(`${NEW_COMMIT}\trefs/tags/v0.9.0\n`, "v0.9.0"), NEW_COMMIT);
  assert.equal(
    commitFromLsRemote(`${PEELED}\trefs/tags/v0.9.0\n${NEW_COMMIT}\trefs/tags/v0.9.0^{}\n`, "v0.9.0"),
    NEW_COMMIT,
  );
  assert.throws(() => commitFromLsRemote("", "v0.9.0"), /not found/);
  assert.throws(() => commitFromLsRemote(`${NEW_COMMIT}\trefs/tags/v0.9.1\n`, "v0.9.0"), /not found/);
  assert.throws(() => commitFromLsRemote("deadbeef\trefs/tags/v0.9.0\n", "v0.9.0"), /not found/);
});

test("a missing tag fails and changes nothing", () => {
  const root = makeRoot();
  const before = snapshot(root);
  const env = makeEnv(root, makeRemote({ tags: {} }));
  assert.throws(() => runUpdate(["v0.9.0"], env), /not found/);
  assert.deepEqual(snapshot(root), before);
  assert.deepEqual(env.calls, []);
});

test("a changed override baseline is reported and blocks the update", () => {
  const root = makeRoot();
  const before = snapshot(root);
  const remote = makeRemote({ files: { "rhwp-studio/src/main.ts": "export const main = 2;\n" } });
  const env = makeEnv(root, remote);
  assert.throws(() => runUpdate(["v0.9.0"], env), (err) => {
    assert.match(err.message, /override main/);
    assert.doesNotMatch(err.message, /asset public\/a\.svg/);
    assert.match(err.message, /--accept-overrides/);
    return true;
  });
  assert.deepEqual(snapshot(root), before);
  assert.deepEqual(env.calls, []);
});

test("--accept-overrides re-pins the baseline of the changed files", () => {
  const root = makeRoot();
  const changed = "export const main = 2;\n";
  const env = makeEnv(root, makeRemote({ files: { "rhwp-studio/src/main.ts": changed } }));
  runUpdate(["v0.9.0", "--accept-overrides"], env);
  const manifest = readJson(root, MANIFEST_REL);
  assert.equal(manifest.upstream.counterparts.main, normalizedSha256Text(changed));
  assert.equal(manifest.upstream.assets["public/a.svg"], normalizedSha256Text(ASSET));
  assert.equal(manifest.upstream.commit, NEW_COMMIT);
});

test("a baseline file missing upstream blocks even with --accept-overrides", () => {
  const root = makeRoot();
  const before = snapshot(root);
  const env = makeEnv(root, makeRemote({ files: { "rhwp-studio/public/a.svg": undefined } }));
  assert.throws(() => runUpdate(["v0.9.0", "--accept-overrides"], env), /missing at v0\.9\.0: asset public\/a\.svg/);
  assert.deepEqual(snapshot(root), before);
  assert.deepEqual(env.calls, []);
});

test("a tag that disagrees with the upstream Cargo.toml version is refused", () => {
  const root = makeRoot();
  const before = snapshot(root);
  const env = makeEnv(root, makeRemote({ files: { "Cargo.toml": '[package]\nname = "rhwp"\nversion = "0.8.9"\n' } }));
  assert.throws(() => runUpdate(["v0.9.0"], env), /does not match upstream Cargo\.toml version 0\.8\.9/);
  assert.deepEqual(snapshot(root), before);
});

test("--dry-run prints the plan and changes nothing", () => {
  const root = makeRoot();
  const before = snapshot(root);
  const env = makeEnv(root, makeRemote());
  runUpdate(["v0.9.0", "--dry-run"], env);
  assert.deepEqual(snapshot(root), before);
  assert.deepEqual(env.calls, []);
  assert.match(env.logs.join("\n"), new RegExp(`v0\\.9\\.0 \\(${NEW_COMMIT}\\)`));
});

test("a failing checkout restores the pin files and the gitlink", () => {
  const root = makeRoot();
  const before = snapshot(root);
  const env = makeEnv(root, makeRemote());
  env.checkout = () => {
    env.calls.push("checkout");
    throw new Error("fetch failed");
  };
  assert.throws(() => runUpdate(["v0.9.0"], env), /restored.*fetch failed/);
  assert.deepEqual(snapshot(root), before);
  assert.deepEqual(env.calls, [`gitlink ${NEW_COMMIT}`, "checkout", `gitlink ${OLD_COMMIT}`]);
});

test("updates the pin, the baseline commit, the gitlink and the checkout, then points to the WASM steps", () => {
  const root = makeRoot();
  const env = makeEnv(root, makeRemote());
  runUpdate(["v0.9.0"], env);
  const config = readJson(root, CONFIG_REL);
  assert.equal(config.commit, NEW_COMMIT);
  assert.equal(config.version, "0.9.0");
  assert.equal(config.rustToolchain, "1.94.0");
  assert.equal(config.wasmPackVersion, "0.14.0");
  const manifest = readJson(root, MANIFEST_REL);
  assert.deepEqual([manifest.upstream.commit, manifest.upstream.version], [NEW_COMMIT, "0.9.0"]);
  assert.deepEqual(env.calls, [`gitlink ${NEW_COMMIT}`, "checkout"]);
  const output = env.logs.join("\n");
  assert.match(output, /vendored WASM was NOT rebuilt/);
  assert.match(output, /vendor-rhwp-core\.mjs/);
});

test("the already pinned tag is a no-op", () => {
  const root = makeRoot();
  const files = {
    "Cargo.toml": '[package]\nname = "rhwp"\nversion = "0.8.7"\n',
    "rust-toolchain.toml": '[toolchain]\nchannel = "1.93.1"\n',
  };
  const remote = makeRemote({ tags: { "v0.8.7": `${NEW_COMMIT}\trefs/tags/v0.8.7` }, files });
  const config = readJson(root, CONFIG_REL);
  fs.writeFileSync(path.join(root, CONFIG_REL), stringify({ ...config, commit: NEW_COMMIT }));
  const manifest = readJson(root, MANIFEST_REL);
  manifest.upstream.commit = NEW_COMMIT;
  fs.writeFileSync(path.join(root, MANIFEST_REL), stringify(manifest));
  const pinned = snapshot(root);
  const env = makeEnv(root, remote);
  runUpdate(["v0.8.7"], env);
  assert.deepEqual(snapshot(root), pinned);
  assert.deepEqual(env.calls, []);
  assert.match(env.logs.join("\n"), /already pinned/);
});
