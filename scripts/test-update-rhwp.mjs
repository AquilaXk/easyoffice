// Tests for scripts/update-rhwp.mjs. The remote and the git/checkout side effects are injected
// fakes, so nothing touches the network or a real checkout.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { normalizedSha256Text } from "./hwp-studio-overrides.mjs";
import { applyLocalPatches, blobOid, loadLocalPatches, localTree, patchEntry } from "./rhwp-svg2pdf.mjs";
import { commitFromLsRemote, parseArgs, runUpdate } from "./update-rhwp.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OLD_COMMIT = "1a76570e833917d15817415a53c09ad61ab3203f";
const NEW_COMMIT = "2b87681f944a28e26928e463026b4c4d4e2d4f40";
const PEELED = "3c98792fa55b39e37039e5c5e5d5e5d5e5d5e5d5";
const CONFIG_REL = "config/rhwp-upstream.json";
const MANIFEST_REL = "config/rhwp-studio-overrides.json";
const MAIN_TS = "export const main = 1;\n";
const ASSET = "<svg/>\n";
const PATCH = '[patch.crates-io]\nsvg2pdf = { path = "vendor/svg2pdf" }\n';
const SHELL_REL = "crates/easyoffice-desktop/Cargo.toml";
const LOCK_REL = "Cargo.lock";
const shellManifest = (version) =>
  `[package]\nname = "easyoffice-desktop"\n\n[dependencies]\nsvg2pdf = { version = "0.13" }\nrhwp = { version = "${version}", default-features = false }\n`;
const lockFile = (version) =>
  `[[package]]\nname = "rhwp-extra"\nversion = "${version}"\n\n[[package]]\nname = "rhwp"\nversion = "${version}"\ndependencies = [\n "aes",\n]\n`;
const SVG2PDF = {
  "vendor/svg2pdf/Cargo.toml": '[package]\nname = "svg2pdf"\nversion = "0.13.0"\n',
  "vendor/svg2pdf/src/lib.rs": "pub fn a() {}\n",
  "vendor/svg2pdf/src/render/path.rs": "pub fn b() {}\n",
};

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
  fs.writeFileSync(path.join(root, "Cargo.toml"), `[workspace]\nmembers = []\n\n${PATCH}`);
  for (const [rel, text] of Object.entries(SVG2PDF)) {
    fs.mkdirSync(path.dirname(path.join(root, rel)), { recursive: true });
    fs.writeFileSync(path.join(root, rel), text);
  }
  fs.writeFileSync(path.join(root, CONFIG_REL), stringify(config));
  fs.writeFileSync(path.join(root, MANIFEST_REL), stringify(manifest));
  fs.mkdirSync(path.join(root, path.dirname(SHELL_REL)), { recursive: true });
  fs.writeFileSync(path.join(root, SHELL_REL), shellManifest(config.version));
  fs.writeFileSync(path.join(root, LOCK_REL), lockFile(config.version));
  return root;
}

function snapshot(root) {
  return [CONFIG_REL, MANIFEST_REL, SHELL_REL, LOCK_REL].map((rel) => fs.readFileSync(path.join(root, rel), "utf8"));
}

function readJson(root, rel) {
  return JSON.parse(fs.readFileSync(path.join(root, rel), "utf8"));
}

/** Fake remote: tags map to ls-remote output, files map `${commit}:${path}` to text. */
function makeRemote({ tags = { "v0.9.0": `${NEW_COMMIT}\trefs/tags/v0.9.0` }, files = {} } = {}) {
  const base = {
    "Cargo.toml": `[package]\nname = "rhwp"\nversion = "0.9.0"\n\n${PATCH}`,
    ...SVG2PDF,
    "rust-toolchain.toml": '[toolchain]\nchannel = "1.94.0"\n',
    "rhwp-studio/src/main.ts": MAIN_TS,
    "rhwp-studio/public/a.svg": ASSET,
  };
  const content = { ...base, ...files };
  return {
    lsRemote: (_url, tag) => tags[tag] ?? "",
    readFileAt: (_url, commit, rel) => (commit === NEW_COMMIT ? content[rel] ?? null : null),
    listTreeAt: (_url, _commit, dir) =>
      new Map(
        Object.entries(content)
          .filter(([rel, text]) => rel.startsWith(`${dir}/`) && text !== undefined)
          .map(([rel, text]) => [rel.slice(dir.length + 1), blobOid(Buffer.from(text))]),
      ),
    readBlobAt: (_url, _commit, rel) => Buffer.from(content[rel]),
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
  assert.deepEqual(parseArgs(["v0.9.0", "--dry-run", "--accept-overrides"]), { tag: "v0.9.0", dryRun: true, acceptOverrides: true, syncSvg2pdf: false });
  assert.equal(parseArgs(["--sync-svg2pdf", "v0.9.0"]).syncSvg2pdf, true);
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

test("the shell's rhwp requirement and the Cargo.lock entry follow the pin, and nothing else in them changes", () => {
  const root = makeRoot();
  const old = readJson(root, CONFIG_REL).version;
  runUpdate(["v0.9.0"], makeEnv(root, makeRemote()));
  assert.equal(fs.readFileSync(path.join(root, SHELL_REL), "utf8"), shellManifest("0.9.0"));
  // Only the `rhwp` package changes, not another package whose name starts the same way.
  assert.equal(
    fs.readFileSync(path.join(root, LOCK_REL), "utf8"),
    lockFile(old).replace('name = "rhwp"\nversion = "' + old, 'name = "rhwp"\nversion = "0.9.0'),
  );
});

test("a shell manifest or lock file without rhwp refuses the update and changes nothing", () => {
  for (const [rel, text, pattern] of [
    [SHELL_REL, '[dependencies]\nsvg2pdf = "0.13"\n', /easyoffice-desktop\/Cargo\.toml has no rhwp dependency/],
    [LOCK_REL, '[[package]]\nname = "aes"\nversion = "0.8.4"\n', /Cargo\.lock has no rhwp package entry/],
  ]) {
    const root = makeRoot();
    fs.writeFileSync(path.join(root, rel), text);
    const before = snapshot(root);
    const env = makeEnv(root, makeRemote());
    assert.throws(() => runUpdate(["v0.9.0"], env), pattern);
    assert.deepEqual(snapshot(root), before);
    assert.deepEqual(env.calls, []);
  }
});

test("a pin that is current but a shell requirement that is stale is repaired, not skipped", () => {
  const root = makeRoot();
  const pinned = readJson(root, CONFIG_REL);
  const remote = makeRemote({
    tags: { "v0.8.7": `${NEW_COMMIT}\trefs/tags/v0.8.7` },
    files: { "Cargo.toml": `[package]\nname = "rhwp"\nversion = "0.8.7"\n\n${PATCH}`, "rust-toolchain.toml": `[toolchain]\nchannel = "${pinned.rustToolchain}"\n` },
  });
  fs.writeFileSync(path.join(root, CONFIG_REL), stringify({ ...pinned, commit: NEW_COMMIT }));
  const manifest = readJson(root, MANIFEST_REL);
  manifest.upstream.commit = NEW_COMMIT;
  fs.writeFileSync(path.join(root, MANIFEST_REL), stringify(manifest));
  fs.writeFileSync(path.join(root, SHELL_REL), shellManifest("0.8.6"));
  const env = makeEnv(root, remote);
  runUpdate(["v0.8.7"], env);
  assert.equal(fs.readFileSync(path.join(root, SHELL_REL), "utf8"), shellManifest("0.8.7"));
  assert.doesNotMatch(env.logs.join("\n"), /already pinned/);
});

test("the already pinned tag is a no-op", () => {
  const root = makeRoot();
  const files = {
    "Cargo.toml": `[package]\nname = "rhwp"\nversion = "0.8.7"\n\n${PATCH}`,
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

const svg2pdfFiles = (root) =>
  Object.fromEntries(Object.keys(SVG2PDF).map((rel) => [rel, fs.existsSync(path.join(root, rel)) ? fs.readFileSync(path.join(root, rel), "utf8") : null]));

test("svg2pdf: an identical vendored copy passes and the patch entry is read", () => {
  assert.equal(patchEntry(PATCH), '{ path = "vendor/svg2pdf" }');
  assert.equal(patchEntry('[package]\nname = "x"\n'), null);
  const root = makeRoot();
  runUpdate(["v0.9.0", "--dry-run"], makeEnv(root, makeRemote()));
});

test("svg2pdf: a differing file refuses the update and names the file, hashes and versions", () => {
  const root = makeRoot();
  const before = snapshot(root);
  const files = { "vendor/svg2pdf/src/lib.rs": "pub fn changed() {}\n", "vendor/svg2pdf/Cargo.toml": '[package]\nname = "svg2pdf"\nversion = "0.14.0"\n' };
  const env = makeEnv(root, makeRemote({ files }));
  assert.throws(() => runUpdate(["v0.9.0"], env), (err) => {
    assert.match(err.message, /changed src\/lib\.rs \([0-9a-f]{10} -> [0-9a-f]{10}\)/);
    assert.match(err.message, /version 0\.13\.0 -> 0\.14\.0/);
    assert.match(err.message, /--sync-svg2pdf/);
    return true;
  });
  assert.deepEqual(snapshot(root), before);
  assert.equal(svg2pdfFiles(root)["vendor/svg2pdf/src/lib.rs"], SVG2PDF["vendor/svg2pdf/src/lib.rs"]);
  assert.deepEqual(env.calls, []);
});

test("svg2pdf: a different [patch] entry is refused even with --sync-svg2pdf", () => {
  const root = makeRoot();
  const cargo = '[package]\nname = "rhwp"\nversion = "0.9.0"\n\n[patch.crates-io]\nsvg2pdf = { git = "https://example.invalid/svg2pdf" }\n';
  const env = makeEnv(root, makeRemote({ files: { "Cargo.toml": cargo } }));
  assert.throws(() => runUpdate(["v0.9.0", "--sync-svg2pdf"], env), /\[patch\.crates-io\] svg2pdf is/);
  assert.deepEqual(env.calls, []);
});

test("svg2pdf: --sync-svg2pdf copies changed and added files and removes the ones upstream dropped", () => {
  const root = makeRoot();
  const files = {
    "vendor/svg2pdf/src/lib.rs": "pub fn changed() {}\n",
    "vendor/svg2pdf/src/render/path.rs": undefined,
    "vendor/svg2pdf/src/new.rs": "pub fn n() {}\n",
  };
  const env = makeEnv(root, makeRemote({ files }));
  runUpdate(["v0.9.0", "--sync-svg2pdf"], env);
  assert.equal(fs.readFileSync(path.join(root, "vendor/svg2pdf/src/lib.rs"), "utf8"), "pub fn changed() {}\n");
  assert.equal(fs.readFileSync(path.join(root, "vendor/svg2pdf/src/new.rs"), "utf8"), "pub fn n() {}\n");
  assert.equal(fs.existsSync(path.join(root, "vendor/svg2pdf/src/render/path.rs")), false);
  assert.deepEqual(env.calls, [`gitlink ${NEW_COMMIT}`, "checkout"]);
});

test("svg2pdf: --dry-run with --sync-svg2pdf writes nothing; a failing checkout restores the vendored files", () => {
  const files = { "vendor/svg2pdf/src/lib.rs": "pub fn changed() {}\n", "vendor/svg2pdf/src/new.rs": "pub fn n() {}\n" };
  const dry = makeRoot();
  const beforeDry = svg2pdfFiles(dry);
  runUpdate(["v0.9.0", "--sync-svg2pdf", "--dry-run"], makeEnv(dry, makeRemote({ files })));
  assert.deepEqual(svg2pdfFiles(dry), beforeDry);
  assert.equal(fs.existsSync(path.join(dry, "vendor/svg2pdf/src/new.rs")), false);

  const root = makeRoot();
  const before = svg2pdfFiles(root);
  const env = makeEnv(root, makeRemote({ files }));
  env.checkout = () => {
    throw new Error("fetch failed");
  };
  assert.throws(() => runUpdate(["v0.9.0", "--sync-svg2pdf"], env), /restored.*fetch failed/);
  assert.deepEqual(svg2pdfFiles(root), before);
  assert.equal(fs.existsSync(path.join(root, "vendor/svg2pdf/src/new.rs")), false);
});

const LIB_REL = "vendor/svg2pdf/src/lib.rs";
const PATCHED_LIB = "pub fn patched() {}\n";

/** A root whose lib.rs carries a recorded local patch on top of rhwp's copy. */
function makePatchedRoot(record = {}) {
  const root = makeRoot();
  fs.writeFileSync(path.join(root, LIB_REL), PATCHED_LIB);
  const patch = { id: "PD-163", base: blobOid(Buffer.from(SVG2PDF[LIB_REL])), patched: blobOid(Buffer.from(PATCHED_LIB)), ...record };
  fs.writeFileSync(path.join(root, "config/svg2pdf-local-patches.json"), stringify({ schemaVersion: 1, files: { "src/lib.rs": patch } }));
  return root;
}

test("svg2pdf: a recorded local patch is not drift while rhwp's file is the one it was made against", () => {
  const root = makePatchedRoot();
  const env = makeEnv(root, makeRemote());
  runUpdate(["v0.9.0", "--dry-run"], env);
  assert.deepEqual(env.calls, []);
});

test("svg2pdf: a patched file that rhwp changed is a conflict, and --sync-svg2pdf does not overwrite it", () => {
  const root = makePatchedRoot();
  const env = makeEnv(root, makeRemote({ files: { [LIB_REL]: "pub fn upstream() {}\n" } }));
  for (const args of [["v0.9.0"], ["v0.9.0", "--sync-svg2pdf"]]) {
    assert.throws(() => runUpdate(args, env), /src\/lib\.rs carries PD-163 but rhwp changed it/);
  }
  assert.equal(fs.readFileSync(path.join(root, LIB_REL), "utf8"), PATCHED_LIB);
  assert.deepEqual(env.calls, []);
});

test("svg2pdf: a file edited without updating its record is still drift", () => {
  const root = makePatchedRoot({ patched: blobOid(Buffer.from("something else\n")) });
  const env = makeEnv(root, makeRemote());
  assert.throws(() => runUpdate(["v0.9.0"], env), /changed src\/lib\.rs/);
});

test("svg2pdf: the recorded patches of this repository match the vendored files", () => {
  const patches = loadLocalPatches(REPO);
  assert.ok(patches.size > 0);
  const local = localTree(REPO);
  for (const [rel, patch] of patches) {
    assert.equal(local.get(rel), patch.patched, `${rel}: update ${patch.id}'s record in config/svg2pdf-local-patches.json after editing the file`);
    assert.notEqual(patch.base, patch.patched, rel);
  }
  const diff = applyLocalPatches({ changed: [...patches.keys()], added: [], removed: [] }, local, new Map([...patches].map(([rel, patch]) => [rel, patch.base])), patches);
  assert.deepEqual(diff.diff.changed, []);
  assert.deepEqual(diff.conflicts, []);
});
