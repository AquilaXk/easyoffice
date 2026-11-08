#!/usr/bin/env node
/**
 * Release asset names and staging (PD-086, PD-103): the one place that says which Tauri bundle becomes which
 * release asset and which updater platform keys it serves. Asset names carry no version, so
 * `releases/latest/download/<name>` links stay valid from release to release (the README links use exactly
 * these names). A release built with signed updater artifacts also holds the `.sig` file of each updatable
 * package, the macOS updater archive and `latest.json`, the manifest the app's updater reads.
 *
 * Usage:
 *   node scripts/release-assets.mjs bundles <platform>            bundle types to pass to `tauri build --bundles`
 *   node scripts/release-assets.mjs stage <platform> <out-dir>    copy the built bundles and their signatures to <out-dir> under their fixed names
 *   node scripts/release-assets.mjs manifest <dir> <version>      write <dir>/latest.json from the staged signatures
 *   node scripts/release-assets.mjs checksums <dir>               check <dir> holds exactly the release files, write SHA256SUMS.txt
 * `stage` reads the bundles from the current directory (the repository root, where Cargo's `target/` is).
 *
 * Reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim) of the reference app's desktop
 * release workflow (.github/workflows, copy_release_asset and copy_updater_asset): one bundle per extension
 * is copied to a stable asset name, its `.sig` is copied next to it, an entry per updater platform key
 * (`url`, `signature`) goes into `latest.json` and one checksum file lists the files. Here it is a script
 * with a unit test instead of shell in the workflow; a missing or ambiguous bundle or signature fails the
 * build; the URLs name the release tag, and each signature must have been made for the release version
 * (the trusted comment `version:`), which the app's updater requires too.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

export const SUMS_FILE = "SHA256SUMS.txt";
export const MANIFEST_FILE = "latest.json";
export const SIGNATURE_SUFFIX = ".sig";
export const DOWNLOAD_BASE_URL = "https://github.com/AquilaXk/easyoffice/releases/latest/download";
/** `latest.json` points at the files of its own release, never at "latest". */
export const RELEASE_DOWNLOAD_BASE_URL = "https://github.com/AquilaXk/easyoffice/releases/download";
const VERSION_PATTERN = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;
const SIGNATURE_PATTERN = /^[A-Za-z0-9+/]+={0,2}$/;

const HOST_BUNDLE_ROOT = "target/release/bundle";

/**
 * Per platform: the Tauri bundle types to build, where the CLI leaves them, and for each release asset
 * the bundle sub-directory, the extension of the file in it and the fixed asset name. `signed` assets have
 * a `.sig` next to them in a release build and serve the updater platform keys in `updater` (the plugin
 * looks for `<os>-<arch>-<installer>`, then `<os>-<arch>`; the generic keys exist only where the platform
 * has a single installer). `updaterOnly` assets are not download links of the README.
 */
export const PLATFORMS = {
  "macos-universal": {
    tauriBundles: ["app", "dmg"],
    bundleRoot: "target/universal-apple-darwin/release/bundle",
    assets: [
      { dir: "dmg", extension: ".dmg", name: "EasyOffice-macos-universal.dmg" },
      {
        dir: "macos",
        extension: ".app.tar.gz",
        name: "EasyOffice-macos-universal.app.tar.gz",
        signed: true,
        updaterOnly: true,
        updater: ["darwin-aarch64", "darwin-aarch64-app", "darwin-x86_64", "darwin-x86_64-app"],
      },
    ],
  },
  "windows-x64": {
    tauriBundles: ["nsis"],
    bundleRoot: HOST_BUNDLE_ROOT,
    assets: [
      {
        dir: "nsis",
        extension: ".exe",
        name: "EasyOffice-windows-x64-setup.exe",
        signed: true,
        updater: ["windows-x86_64", "windows-x86_64-nsis"],
      },
    ],
  },
  "linux-x64": {
    tauriBundles: ["appimage", "deb", "rpm"],
    bundleRoot: HOST_BUNDLE_ROOT,
    assets: [
      { dir: "appimage", extension: ".AppImage", name: "EasyOffice-linux-x64.AppImage", signed: true, updater: ["linux-x86_64-appimage"] },
      { dir: "deb", extension: ".deb", name: "EasyOffice-linux-x64.deb", signed: true, updater: ["linux-x86_64-deb"] },
      { dir: "rpm", extension: ".rpm", name: "EasyOffice-linux-x64.rpm", signed: true, updater: ["linux-x86_64-rpm"] },
    ],
  },
  "linux-arm64": {
    tauriBundles: ["deb"],
    bundleRoot: HOST_BUNDLE_ROOT,
    assets: [{ dir: "deb", extension: ".deb", name: "EasyOffice-linux-arm64.deb", signed: true, updater: ["linux-aarch64-deb"] }],
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

const everyAsset = () => Object.values(PLATFORMS).flatMap((platform) => platform.assets);

/** The installers a user downloads (the README links), sorted. */
export function allAssetNames() {
  return everyAsset()
    .filter((asset) => !asset.updaterOnly)
    .map((asset) => asset.name)
    .sort(compareNames);
}

/** Every file of a signed release, sorted: the assets, the updater archive, a `.sig` per signed asset and `latest.json`. */
export function releaseFileNames() {
  const assets = everyAsset();
  return [
    ...assets.map((asset) => asset.name),
    ...assets.filter((asset) => asset.signed).map((asset) => asset.name + SIGNATURE_SUFFIX),
    MANIFEST_FILE,
  ].sort(compareNames);
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

function signatureFile(file, label) {
  const signature = `${file}${SIGNATURE_SUFFIX}`;
  if (!fs.existsSync(signature) || fs.statSync(signature).size === 0) {
    throw new Error(`${label} has no signature (${path.basename(signature)}): build with the updater signing key (createUpdaterArtifacts)`);
  }
  return signature;
}

/**
 * Copies the built bundles of one platform to `outDir` under their fixed names, and the signature of each
 * signed one as `<name>.sig`; returns the names. A signed bundle without a signature fails the stage.
 */
export function stageAssets(id, root, outDir) {
  const bundles = locateBundles(id, root);
  const signatures = new Map(
    bundles.filter(({ asset }) => asset.signed).map(({ asset, file }) => [asset.name, signatureFile(file, asset.name)])
  );
  fs.mkdirSync(outDir, { recursive: true });
  const staged = [];
  for (const { asset, file } of bundles) {
    fs.copyFileSync(file, path.join(outDir, asset.name));
    staged.push(asset.name);
    if (signatures.has(asset.name)) {
      fs.copyFileSync(signatures.get(asset.name), path.join(outDir, asset.name + SIGNATURE_SUFFIX));
      staged.push(asset.name + SIGNATURE_SUFFIX);
    }
  }
  return staged;
}

/** The version a minisign signature (the base64 text of a `.sig` file) was made for: its trusted comment's `version:` field. */
export function signedVersion(signature) {
  const lines = Buffer.from(signature, "base64").toString("utf8").split(/\r?\n/);
  const trusted = lines.find((line) => line.startsWith("trusted comment: "));
  const field = trusted?.slice("trusted comment: ".length).split("\t").find((part) => part.startsWith("version:"));
  return field?.slice("version:".length) ?? null;
}

/**
 * The `latest.json` content for the signatures staged in `dir`: every updater platform key with the URL of
 * its asset in the release `v<version>` and the signature text. Every signed asset must be present, and each
 * signature must carry the release version (an older package cannot be listed under a newer number).
 */
export function buildManifest(dir, version, pubDate = new Date()) {
  if (!VERSION_PATTERN.test(version)) throw new Error(`'${version}' is not a release version like 1.2.3`);
  const platforms = {};
  for (const asset of everyAsset().filter((candidate) => candidate.signed)) {
    const file = path.join(dir, asset.name + SIGNATURE_SUFFIX);
    if (!fs.existsSync(file)) throw new Error(`the signature of ${asset.name} is missing in ${dir}`);
    const signature = fs.readFileSync(file, "utf8").replace(/[\r\n]/g, "");
    if (!SIGNATURE_PATTERN.test(signature)) throw new Error(`${path.basename(file)} is not a base64 signature`);
    const signed = signedVersion(signature);
    if (signed !== version) {
      throw new Error(`${path.basename(file)} was made for version ${signed ?? "(none)"}, not ${version}`);
    }
    for (const key of asset.updater) {
      platforms[key] = { url: `${RELEASE_DOWNLOAD_BASE_URL}/v${version}/${asset.name}`, signature };
    }
  }
  return {
    version,
    notes: `EasyOffice v${version}`,
    pub_date: pubDate.toISOString(),
    platforms: Object.fromEntries(Object.entries(platforms).sort(([a], [b]) => compareNames(a, b))),
  };
}

/** Writes `latest.json` into `dir`; returns its content. */
export function writeManifest(dir, version, pubDate) {
  const manifest = buildManifest(dir, version, pubDate);
  fs.writeFileSync(path.join(dir, MANIFEST_FILE), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

/** Writes SHA256SUMS.txt for exactly the release files in `dir` (`latest.json` included); returns its lines. */
export function writeChecksums(dir) {
  const expected = releaseFileNames();
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
  } else if (command === "manifest" && first && second) {
    const manifest = writeManifest(first, second);
    console.log(`wrote ${MANIFEST_FILE} for ${manifest.version}: ${Object.keys(manifest.platforms).join(", ")}`);
  } else if (command === "checksums" && first) {
    for (const line of writeChecksums(first)) console.log(line);
  } else {
    console.error("usage: release-assets.mjs bundles <platform> | stage <platform> <out-dir> | manifest <dir> <version> | checksums <dir>");
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
