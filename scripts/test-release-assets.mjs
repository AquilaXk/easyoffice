import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  allAssetNames,
  buildManifest,
  bundleArguments,
  DOWNLOAD_BASE_URL,
  MANIFEST_FILE,
  PLATFORMS,
  RELEASE_DOWNLOAD_BASE_URL,
  releaseFileNames,
  signedVersion,
  stageAssets,
  SUMS_FILE,
  writeChecksums,
  writeManifest,
} from "./release-assets.mjs";
import { readText } from "./test-support/read-text.mjs";

const read = (rel) => readText(new URL(`../${rel}`, import.meta.url));

/** A target directory laid out the way the Tauri CLI leaves it, with one file per asset. */
function fakeBundles(root, platformId, overrides = {}, { signedFor } = {}) {
  const platform = PLATFORMS[platformId];
  for (const asset of platform.assets) {
    const dir = path.join(root, platform.bundleRoot, asset.dir);
    fs.mkdirSync(dir, { recursive: true });
    for (const file of overrides[asset.dir] ?? [`EasyOffice_0.1.0_x${asset.extension}`]) {
      fs.writeFileSync(path.join(dir, file), `${platformId}:${asset.dir}:${file}`);
      if (signedFor !== undefined && asset.signed) fs.writeFileSync(path.join(dir, `${file}.sig`), fakeSignature(signedFor, file));
    }
  }
}

/** The text of a Tauri `.sig` file: base64 of a minisign signature whose trusted comment names the version. */
function fakeSignature(version, file = "x") {
  const text = [
    "untrusted comment: signature from tauri secret key",
    "RUQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
    `trusted comment: timestamp:1700000000\tfile:${file}${version === null ? "" : `\tversion:${version}`}`,
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
    "",
  ].join("\n");
  return Buffer.from(text).toString("base64");
}

/** A directory holding every platform's staged, signed files, like the publish job's download. */
function stagedRelease(version = "1.2.3") {
  const dir = tempDir();
  for (const id of Object.keys(PLATFORMS)) {
    const root = tempDir();
    fakeBundles(root, id, {}, { signedFor: version });
    stageAssets(id, root, dir);
  }
  return dir;
}

const tempDir = () => fs.mkdtempSync(path.join(os.tmpdir(), "release-assets-"));

test("the six installers have fixed names with no version in them", () => {
  assert.deepEqual(allAssetNames(), [
    "EasyOffice-linux-arm64.deb",
    "EasyOffice-linux-x64.AppImage",
    "EasyOffice-linux-x64.deb",
    "EasyOffice-linux-x64.rpm",
    "EasyOffice-macos-universal.dmg",
    "EasyOffice-windows-x64-setup.exe",
  ]);
  for (const name of allAssetNames()) assert.doesNotMatch(name, /\d+\.\d+/, name);
});

test("a signed release is the installers, the macOS updater archive, a signature per package and latest.json", () => {
  assert.deepEqual(releaseFileNames(), [
    "EasyOffice-linux-arm64.deb",
    "EasyOffice-linux-arm64.deb.sig",
    "EasyOffice-linux-x64.AppImage",
    "EasyOffice-linux-x64.AppImage.sig",
    "EasyOffice-linux-x64.deb",
    "EasyOffice-linux-x64.deb.sig",
    "EasyOffice-linux-x64.rpm",
    "EasyOffice-linux-x64.rpm.sig",
    "EasyOffice-macos-universal.app.tar.gz",
    "EasyOffice-macos-universal.app.tar.gz.sig",
    "EasyOffice-macos-universal.dmg",
    "EasyOffice-windows-x64-setup.exe",
    "EasyOffice-windows-x64-setup.exe.sig",
    "latest.json",
  ]);
});

test("each platform builds the bundle types its assets come from", () => {
  assert.equal(bundleArguments("macos-universal"), "app,dmg", "the app bundle is what the updater archive is made from");
  assert.equal(bundleArguments("windows-x64"), "nsis");
  assert.equal(bundleArguments("linux-x64"), "appimage,deb,rpm");
  assert.equal(bundleArguments("linux-arm64"), "deb");
  assert.throws(() => bundleArguments("linux-riscv"), /unknown platform/);
});

test("staging copies each bundle and its signature under the fixed names", () => {
  const root = tempDir();
  const out = path.join(root, "dist");
  fakeBundles(root, "linux-x64", { deb: ["EasyOffice_0.1.0_amd64.deb"], rpm: ["EasyOffice-0.1.0-1.x86_64.rpm"] }, { signedFor: "0.1.0" });
  assert.deepEqual(stageAssets("linux-x64", root, out), [
    "EasyOffice-linux-x64.AppImage",
    "EasyOffice-linux-x64.AppImage.sig",
    "EasyOffice-linux-x64.deb",
    "EasyOffice-linux-x64.deb.sig",
    "EasyOffice-linux-x64.rpm",
    "EasyOffice-linux-x64.rpm.sig",
  ]);
  assert.equal(fs.readFileSync(path.join(out, "EasyOffice-linux-x64.deb"), "utf8"), "linux-x64:deb:EasyOffice_0.1.0_amd64.deb");
  assert.equal(signedVersion(fs.readFileSync(path.join(out, "EasyOffice-linux-x64.deb.sig"), "utf8")), "0.1.0");
});

test("macOS stages the dmg and the signed updater archive; the dmg itself is not signed", () => {
  const root = tempDir();
  const out = path.join(root, "dist");
  fakeBundles(root, "macos-universal", { macos: ["EasyOffice.app.tar.gz"] }, { signedFor: "0.1.0" });
  assert.deepEqual(stageAssets("macos-universal", root, out), [
    "EasyOffice-macos-universal.dmg",
    "EasyOffice-macos-universal.app.tar.gz",
    "EasyOffice-macos-universal.app.tar.gz.sig",
  ]);
});

test("staging fails when a signed bundle has no signature or an empty one", () => {
  const unsigned = tempDir();
  fakeBundles(unsigned, "windows-x64");
  assert.throws(() => stageAssets("windows-x64", unsigned, path.join(unsigned, "dist")), /EasyOffice-windows-x64-setup\.exe has no signature .*createUpdaterArtifacts/);
  const empty = tempDir();
  fakeBundles(empty, "windows-x64", {}, { signedFor: "0.1.0" });
  const dir = path.join(empty, PLATFORMS["windows-x64"].bundleRoot, "nsis");
  fs.writeFileSync(path.join(dir, "EasyOffice_0.1.0_x.exe.sig"), "");
  assert.throws(() => stageAssets("windows-x64", empty, path.join(empty, "dist")), /has no signature/);
  assert.equal(fs.existsSync(path.join(empty, "dist")), false, "nothing is staged from a release that is not fully signed");
});

test("staging fails when a bundle is missing or ambiguous", () => {
  const missing = tempDir();
  fakeBundles(missing, "linux-x64", { rpm: [] });
  assert.throws(() => stageAssets("linux-x64", missing, path.join(missing, "dist")), /expected exactly one \.rpm .* found 0/);

  const twice = tempDir();
  fakeBundles(twice, "macos-universal", { dmg: ["EasyOffice_0.1.0_universal.dmg", "other.dmg"] });
  assert.throws(() => stageAssets("macos-universal", twice, path.join(twice, "dist")), /found 2/);
});

test("checksums cover every release file, latest.json included, in one sorted file that sha256sum -c accepts", () => {
  const dir = tempDir();
  for (const name of releaseFileNames()) fs.writeFileSync(path.join(dir, name), `bytes of ${name}`);
  const lines = writeChecksums(dir);
  assert.equal(lines.length, 14);
  const written = fs.readFileSync(path.join(dir, SUMS_FILE), "utf8").trimEnd().split("\n");
  assert.deepEqual(written, lines);
  assert.deepEqual(written.map((l) => l.split("  ")[1]), releaseFileNames());
  assert.ok(written.some((l) => l.endsWith(`  ${MANIFEST_FILE}`)));
  const first = written[0].split("  ");
  assert.equal(first[0], crypto.createHash("sha256").update(`bytes of ${first[1]}`).digest("hex"));
});

test("checksums refuse a release with a missing or unexpected file", () => {
  const dir = tempDir();
  for (const name of releaseFileNames().slice(1)) fs.writeFileSync(path.join(dir, name), "x");
  fs.writeFileSync(path.join(dir, "EasyOffice-0.1.0-macos-universal.dmg"), "x");
  assert.throws(
    () => writeChecksums(dir),
    (err) => /missing: EasyOffice-linux-arm64\.deb/.test(err.message) && /unexpected: EasyOffice-0\.1\.0-macos-universal\.dmg/.test(err.message)
  );
});

test("the README links every asset through releases/latest/download", () => {
  const text = read("README.md");
  for (const name of allAssetNames()) {
    assert.ok(text.includes(`${DOWNLOAD_BASE_URL}/${name}`), `README.md must link ${name}`);
  }
});

test("latest.json lists every updater platform key with the release's own URL and the signature text", () => {
  const dir = stagedRelease("1.2.3");
  const date = new Date("2026-11-08T01:02:03.456Z");
  const manifest = writeManifest(dir, "1.2.3", date);
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(dir, MANIFEST_FILE), "utf8")), manifest);
  assert.deepEqual(Object.keys(manifest), ["version", "notes", "pub_date", "platforms"]);
  assert.equal(manifest.version, "1.2.3");
  assert.equal(manifest.pub_date, "2026-11-08T01:02:03.456Z");
  assert.deepEqual(Object.keys(manifest.platforms), [
    "darwin-aarch64",
    "darwin-aarch64-app",
    "darwin-x86_64",
    "darwin-x86_64-app",
    "linux-aarch64-deb",
    "linux-x86_64-appimage",
    "linux-x86_64-deb",
    "linux-x86_64-rpm",
    "windows-x86_64",
    "windows-x86_64-nsis",
  ]);
  const files = {
    "darwin-aarch64": "EasyOffice-macos-universal.app.tar.gz",
    "darwin-x86_64-app": "EasyOffice-macos-universal.app.tar.gz",
    "windows-x86_64": "EasyOffice-windows-x64-setup.exe",
    "windows-x86_64-nsis": "EasyOffice-windows-x64-setup.exe",
    "linux-x86_64-appimage": "EasyOffice-linux-x64.AppImage",
    "linux-x86_64-deb": "EasyOffice-linux-x64.deb",
    "linux-x86_64-rpm": "EasyOffice-linux-x64.rpm",
    "linux-aarch64-deb": "EasyOffice-linux-arm64.deb",
  };
  for (const [key, file] of Object.entries(files)) {
    const entry = manifest.platforms[key];
    assert.deepEqual(Object.keys(entry), ["url", "signature"], key);
    assert.equal(entry.url, `${RELEASE_DOWNLOAD_BASE_URL}/v1.2.3/${file}`, key);
    assert.equal(entry.signature, fs.readFileSync(path.join(dir, `${file}.sig`), "utf8"), key);
    assert.equal(signedVersion(entry.signature), "1.2.3");
  }
  assert.ok(RELEASE_DOWNLOAD_BASE_URL.startsWith("https://github.com/AquilaXk/easyoffice/releases/download"));
  assert.doesNotThrow(() => Date.parse(manifest.pub_date), "an RFC 3339 time the updater parses");
});

test("the manifest and the checksums complete the release the publish job uploads", () => {
  const dir = stagedRelease("1.2.3");
  writeManifest(dir, "1.2.3");
  assert.equal(writeChecksums(dir).length, 14);
  const byName = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
  assert.deepEqual(fs.readdirSync(dir).sort(byName), [...releaseFileNames(), SUMS_FILE].sort(byName));
});

test("latest.json is refused for a version the signatures were not made for (no update loop, no old package under a new number)", () => {
  const dir = stagedRelease("1.2.3");
  assert.throws(() => buildManifest(dir, "1.2.4"), /was made for version 1\.2\.3, not 1\.2\.4/);
  assert.throws(() => buildManifest(dir, "v1.2.3"), /not a release version/);
  assert.throws(() => buildManifest(dir, "1.2"), /not a release version/);
  const unversioned = stagedRelease(null);
  assert.throws(() => buildManifest(unversioned, "1.2.3"), /\(none\)/);
});

test("latest.json is refused when a signature is missing or not a base64 signature", () => {
  const missing = stagedRelease("1.2.3");
  fs.rmSync(path.join(missing, "EasyOffice-linux-x64.rpm.sig"));
  assert.throws(() => buildManifest(missing, "1.2.3"), /signature of EasyOffice-linux-x64\.rpm is missing/);
  const broken = stagedRelease("1.2.3");
  fs.writeFileSync(path.join(broken, "EasyOffice-linux-x64.deb.sig"), "not base64!\n");
  assert.throws(() => buildManifest(broken, "1.2.3"), /not a base64 signature/);
});

test("a signature with a trailing newline is stored without it", () => {
  const dir = stagedRelease("1.2.3");
  const file = path.join(dir, "EasyOffice-linux-x64.deb.sig");
  fs.writeFileSync(file, `${fs.readFileSync(file, "utf8")}\r\n`);
  assert.equal(buildManifest(dir, "1.2.3").platforms["linux-x86_64-deb"].signature.includes("\n"), false);
});

const workflow = read(".github/workflows/release.yml");

test("every build job signs: the updater artifacts are switched on with the release version and the keys come from secrets", () => {
  const builds = workflow.split(/\n(?=  [a-z-]+:\n)/).filter((job) => job.includes("@tauri-apps/cli"));
  assert.equal(builds.length, 3, "macOS, Windows and Linux build with the Tauri CLI");
  for (const job of builds) {
    assert.match(job, /\\"bundle\\":\{\\"createUpdaterArtifacts\\":true\}/, "createUpdaterArtifacts is on in the release build");
    assert.match(job, /\\"version\\":\\"\$\{VERSION\}\\"/, "the release version overrides the bundle version");
    assert.match(job, /TAURI_SIGNING_PRIVATE_KEY: \$\{\{ secrets\.TAURI_SIGNING_PRIVATE_KEY \}\}/);
    assert.match(job, /TAURI_SIGNING_PRIVATE_KEY_PASSWORD: \$\{\{ secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD \}\}/);
  }
  assert.doesNotMatch(workflow.replaceAll(/\$\{\{ secrets\.TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)? \}\}/g, ""), /PRIVATE_KEY[^\n]*(echo|printf|cat)/, "a key is never echoed");
});

// The CLI that holds the signing key is the one the committed lockfile pins, installed without scripts; the
// release never fetches a package by name in a step that has the key.
test("the release builds run the lockfile's Tauri CLI, not a package fetched by name", () => {
  assert.doesNotMatch(workflow, /\bnpx\b/);
  const action = read(".github/actions/prepare-desktop-build/action.yml");
  assert.match(action, /npm ci --ignore-scripts [^\n]*--prefix scripts\/tauri-cli/);
  const manifest = JSON.parse(read("scripts/tauri-cli/package.json"));
  assert.match(manifest.dependencies["@tauri-apps/cli"], /^\d+\.\d+\.\d+$/, "an exact version");
  const lock = JSON.parse(read("scripts/tauri-cli/package-lock.json"));
  const cli = lock.packages["node_modules/@tauri-apps/cli"];
  assert.equal(cli.version, manifest.dependencies["@tauri-apps/cli"]);
  for (const [name, entry] of Object.entries(lock.packages)) {
    if (name) assert.match(entry.integrity ?? "", /^sha512-/, `${name} has an integrity hash`);
  }
});

test("publishing is serialized by version across tag pushes and manual runs", () => {
  const publish = workflow.slice(workflow.indexOf("  publish-release:"));
  assert.match(publish, /concurrency:\s+group: release-publish-\$\{\{ needs\.prepare\.outputs\.version \}\}\s+cancel-in-progress: false/);
});

test("a run without the signing key fails before any build, publishing or not", () => {
  const prepare = workflow.slice(workflow.indexOf("  prepare:"), workflow.indexOf("  release-macos:"));
  assert.match(prepare, /TAURI_SIGNING_PRIVATE_KEY: \$\{\{ secrets\.TAURI_SIGNING_PRIVATE_KEY \}\}/);
  assert.match(prepare, /-z "\$\{TAURI_SIGNING_PRIVATE_KEY:-\}"/);
  assert.match(prepare, /TAURI_SIGNING_PRIVATE_KEY is required/);
  assert.match(prepare, /TAURI_SIGNING_PRIVATE_KEY_PASSWORD: \$\{\{ secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD \}\}/);
  assert.match(prepare, /-z "\$\{TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-\}"/);
  assert.match(prepare, /TAURI_SIGNING_PRIVATE_KEY_PASSWORD is required/);
  const jobs = workflow.slice(workflow.indexOf("  release-macos:"));
  assert.equal(jobs.split("needs: prepare").length - 1 + jobs.split("needs: [prepare,").length - 1, 4, "every other job waits for prepare");
});

test("the publish job writes latest.json before the checksums and uploads everything in release-assets", () => {
  const publish = workflow.slice(workflow.indexOf("  publish-release:"));
  const manifest = publish.indexOf("release-assets.mjs manifest release-assets");
  const checksums = publish.indexOf("release-assets.mjs checksums release-assets");
  const upload = publish.indexOf("FILES=(release-assets/*)");
  assert.ok(manifest > 0 && manifest < checksums && checksums < upload);
  assert.match(publish, /manifest release-assets "\$VERSION"/);
});

test("the tag, the app version and latest.json agree, so an updated app finds nothing newer", () => {
  assert.match(workflow, /Tag v\$TAG_VERSION does not match Cargo workspace version \$CARGO_VERSION/);
  assert.match(workflow, /--target "\$TARGET_SHA"/);
  assert.match(workflow, /VERSION: \$\{\{ needs\.prepare\.outputs\.version \}\}/);
  assert.match(workflow, /--prerelease/, "a pre-release version is flagged so /releases/latest never serves its latest.json");
});

test("a rerun moves a draft's target commit with its assets and never moves a published release's (PD-146)", () => {
  const publish = workflow.slice(workflow.indexOf("  publish-release:"));
  const draft = publish.indexOf('if [ "$IS_DRAFT" = true ]');
  const edit = publish.indexOf('gh release edit "$TAG" --target "$TARGET_SHA"');
  const prune = publish.indexOf("gh release delete-asset");
  const published = publish.indexOf("else", draft);
  const refuse = publish.indexOf('if [ "$TAG_SHA" != "$TARGET_SHA" ]');
  const upload = publish.indexOf('gh release upload "$TAG"');
  assert.ok(draft > 0 && draft < edit && edit < prune && prune < published, "a draft gets the new target, then loses stale files");
  assert.ok(published < refuse && refuse < upload, "a published release is checked against its tag before any upload");
  assert.match(publish, /\.isDraft/);
  assert.match(publish, /commits\/\$TAG/, "the published release's commit is read from its tag");
  assert.match(publish, /refusing to replace its assets/);
  assert.match(publish, /exit 1/);
});
