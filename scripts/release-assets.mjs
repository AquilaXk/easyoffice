#!/usr/bin/env node
/**
 * Release asset names and staging (PD-086): the one place that says which Tauri bundle becomes which
 * release asset. Asset names carry no version, so `releases/latest/download/<name>` links stay valid
 * from release to release (the README links use exactly these names).
 *
 * Usage:
 *   node scripts/release-assets.mjs bundles <platform>           bundle types to pass to `tauri build --bundles`
 *   node scripts/release-assets.mjs stage <platform> <out-dir>   copy the built bundles to <out-dir> under their fixed names
 *   node scripts/release-assets.mjs checksums <dir>              check <dir> holds exactly the release assets, write SHA256SUMS.txt
 * `stage` reads the bundles from the current directory (the repository root, where Cargo's `target/` is).
 *
 * Reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim) of the reference app's desktop
 * release workflow (.github/workflows, copy_release_asset): one bundle per extension is copied to a stable
 * asset name and one checksum file lists the assets. Here it is a script with a unit test instead of
 * shell in the workflow, and a missing or ambiguous bundle fails the build.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

export const SUMS_FILE = "SHA256SUMS.txt";
export const DOWNLOAD_BASE_URL = "https://github.com/AquilaXk/easyoffice/releases/latest/download";

const HOST_BUNDLE_ROOT = "target/release/bundle";

/**
 * Per platform: the Tauri bundle types to build, where the CLI leaves them, and for each release asset
 * the bundle sub-directory, the extension of the file in it and the fixed asset name.
 */
export const PLATFORMS = {
  "macos-universal": {
    tauriBundles: ["app", "dmg"],
    bundleRoot: "target/universal-apple-darwin/release/bundle",
    assets: [{ dir: "dmg", extension: ".dmg", name: "EasyOffice-macos-universal.dmg" }],
  },
  "windows-x64": {
    tauriBundles: ["nsis"],
    bundleRoot: HOST_BUNDLE_ROOT,
    assets: [{ dir: "nsis", extension: ".exe", name: "EasyOffice-windows-x64-setup.exe" }],
  },
  "linux-x64": {
    tauriBundles: ["appimage", "deb", "rpm"],
    bundleRoot: HOST_BUNDLE_ROOT,
    assets: [
      { dir: "appimage", extension: ".AppImage", name: "EasyOffice-linux-x64.AppImage" },
      { dir: "deb", extension: ".deb", name: "EasyOffice-linux-x64.deb" },
      { dir: "rpm", extension: ".rpm", name: "EasyOffice-linux-x64.rpm" },
    ],
  },
  "linux-arm64": {
    tauriBundles: ["deb"],
    bundleRoot: HOST_BUNDLE_ROOT,
    assets: [{ dir: "deb", extension: ".deb", name: "EasyOffice-linux-arm64.deb" }],
  },
};

function compareNames(a, b) {
  if (a < b) return -1;
  return a > b ? 1 : 0;
}

function platformOf(id) {
  const platform = PLATFORMS[id];
  if (!platform) throw new Error(`unknown platform '${id}' (expected one of ${Object.keys(PLATFORMS).join(", ")})`);
  return platform;
}

/** Value for `tauri build --bundles`. */
export function bundleArguments(id) {
  return platformOf(id).tauriBundles.join(",");
}

/** Every release asset name, sorted. */
export function allAssetNames() {
  return Object.values(PLATFORMS)
    .flatMap((platform) => platform.assets.map((asset) => asset.name))
    .sort(compareNames);
}

function findBundle(root, platform, asset) {
  const dir = path.join(root, platform.bundleRoot, asset.dir);
  const found = fs.existsSync(dir)
    ? fs
        .readdirSync(dir, { withFileTypes: true })
        .filter((entry) => entry.isFile() && entry.name.endsWith(asset.extension))
        .map((entry) => entry.name)
    : [];
  if (found.length !== 1) {
    throw new Error(`expected exactly one ${asset.extension} in ${dir}, found ${found.length}${found.length ? ` (${found.join(", ")})` : ""}`);
  }
  return path.join(dir, found[0]);
}

/** The built bundle file of each release asset of one platform, as `{ asset, file }`. */
export function locateBundles(id, root) {
  const platform = platformOf(id);
  return platform.assets.map((asset) => ({ asset, file: findBundle(root, platform, asset) }));
}

/** Copies the built bundles of one platform to `outDir` under their fixed names; returns those names. */
export function stageAssets(id, root, outDir) {
  const bundles = locateBundles(id, root);
  fs.mkdirSync(outDir, { recursive: true });
  for (const { asset, file } of bundles) fs.copyFileSync(file, path.join(outDir, asset.name));
  return bundles.map(({ asset }) => asset.name);
}

/** Writes SHA256SUMS.txt for exactly the release assets in `dir`; returns its lines. */
export function writeChecksums(dir) {
  const expected = allAssetNames();
  const present = fs.readdirSync(dir).filter((name) => name !== SUMS_FILE).sort(compareNames);
  const missing = expected.filter((name) => !present.includes(name));
  const unexpected = present.filter((name) => !expected.includes(name));
  if (missing.length || unexpected.length) {
    const parts = [];
    if (missing.length) parts.push(`missing: ${missing.join(", ")}`);
    if (unexpected.length) parts.push(`unexpected: ${unexpected.join(", ")}`);
    throw new Error(`release assets in ${dir} are not the expected set (${parts.join("; ")})`);
  }
  const lines = expected.map((name) => {
    const digest = crypto.createHash("sha256").update(fs.readFileSync(path.join(dir, name))).digest("hex");
    return `${digest}  ${name}`;
  });
  fs.writeFileSync(path.join(dir, SUMS_FILE), `${lines.join("\n")}\n`);
  return lines;
}

function main(argv) {
  const [command, first, second] = argv;
  if (command === "bundles" && first) {
    console.log(bundleArguments(first));
  } else if (command === "stage" && first && second) {
    for (const name of stageAssets(first, process.cwd(), second)) console.log(`staged ${name}`);
  } else if (command === "checksums" && first) {
    for (const line of writeChecksums(first)) console.log(line);
  } else {
    console.error("usage: release-assets.mjs bundles <platform> | stage <platform> <out-dir> | checksums <dir>");
    process.exit(2);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    console.error(`::error::${err.message}`);
    process.exit(1);
  }
}
