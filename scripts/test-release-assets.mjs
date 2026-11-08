import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  allAssetNames,
  bundleArguments,
  DOWNLOAD_BASE_URL,
  PLATFORMS,
  stageAssets,
  SUMS_FILE,
  writeChecksums,
} from "./release-assets.mjs";

const read = (rel) => fs.readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");

/** A target directory laid out the way the Tauri CLI leaves it, with one file per asset. */
function fakeBundles(root, platformId, overrides = {}) {
  const platform = PLATFORMS[platformId];
  for (const asset of platform.assets) {
    const dir = path.join(root, platform.bundleRoot, asset.dir);
    fs.mkdirSync(dir, { recursive: true });
    for (const file of overrides[asset.dir] ?? [`EasyOffice_0.1.0_x${asset.extension}`]) {
      fs.writeFileSync(path.join(dir, file), `${platformId}:${asset.dir}:${file}`);
    }
  }
}

const tempDir = () => fs.mkdtempSync(path.join(os.tmpdir(), "release-assets-"));

test("the six fixed asset names are the whole release, with no version in them", () => {
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

test("each platform builds the bundle types its assets come from", () => {
  assert.equal(bundleArguments("macos-universal"), "app,dmg");
  assert.equal(bundleArguments("windows-x64"), "nsis");
  assert.equal(bundleArguments("linux-x64"), "appimage,deb,rpm");
  assert.equal(bundleArguments("linux-arm64"), "deb");
  assert.throws(() => bundleArguments("linux-riscv"), /unknown platform/);
});

test("staging copies each bundle under its fixed name", () => {
  const root = tempDir();
  const out = path.join(root, "dist");
  fakeBundles(root, "linux-x64", { deb: ["EasyOffice_0.1.0_amd64.deb"], rpm: ["EasyOffice-0.1.0-1.x86_64.rpm"] });
  assert.deepEqual(stageAssets("linux-x64", root, out), [
    "EasyOffice-linux-x64.AppImage",
    "EasyOffice-linux-x64.deb",
    "EasyOffice-linux-x64.rpm",
  ]);
  assert.equal(fs.readFileSync(path.join(out, "EasyOffice-linux-x64.deb"), "utf8"), "linux-x64:deb:EasyOffice_0.1.0_amd64.deb");
});

test("staging fails when a bundle is missing or ambiguous", () => {
  const missing = tempDir();
  fakeBundles(missing, "linux-x64", { rpm: [] });
  assert.throws(() => stageAssets("linux-x64", missing, path.join(missing, "dist")), /expected exactly one \.rpm .* found 0/);

  const twice = tempDir();
  fakeBundles(twice, "macos-universal", { dmg: ["EasyOffice_0.1.0_universal.dmg", "other.dmg"] });
  assert.throws(() => stageAssets("macos-universal", twice, path.join(twice, "dist")), /found 2/);
});

test("checksums cover every asset in one sorted file that sha256sum -c accepts", () => {
  const dir = tempDir();
  for (const name of allAssetNames()) fs.writeFileSync(path.join(dir, name), `bytes of ${name}`);
  const lines = writeChecksums(dir);
  assert.equal(lines.length, 6);
  const written = fs.readFileSync(path.join(dir, SUMS_FILE), "utf8").trimEnd().split("\n");
  assert.deepEqual(written, lines);
  assert.deepEqual(written.map((l) => l.split("  ")[1]), allAssetNames());
  const first = written[0].split("  ");
  assert.equal(first[0], crypto.createHash("sha256").update(`bytes of ${first[1]}`).digest("hex"));
});

test("checksums refuse a release with a missing or unexpected file", () => {
  const dir = tempDir();
  for (const name of allAssetNames().slice(1)) fs.writeFileSync(path.join(dir, name), "x");
  fs.writeFileSync(path.join(dir, "EasyOffice-0.1.0-macos-universal.dmg"), "x");
  assert.throws(
    () => writeChecksums(dir),
    (err) => /missing: EasyOffice-linux-arm64\.deb/.test(err.message) && /unexpected: EasyOffice-0\.1\.0-macos-universal\.dmg/.test(err.message)
  );
});

test("both READMEs link every asset through releases/latest/download", () => {
  for (const readme of ["README.md", "README.ko.md"]) {
    const text = read(readme);
    for (const name of allAssetNames()) {
      assert.ok(text.includes(`${DOWNLOAD_BASE_URL}/${name}`), `${readme} must link ${name}`);
    }
  }
});
