#!/usr/bin/env node
/**
 * Builds the hwp-studio overlay (PD-081) into crates/easyoffice-app/ui/hwp-studio.
 *
 *   node scripts/build-hwp-studio.mjs [--skip-install] [--no-check]
 *
 * 1. verifies the rhwp pin, the vendored WASM, the override manifest and the bundled fonts,
 * 2. npm ci --ignore-scripts in apps/hwp-studio (lockfile only, no install scripts),
 * 3. vite build (WASM is the committed vendor/rhwp-core package; nothing is compiled),
 * 4. writes asset-manifest.json (path, bytes, SHA-256 per file) and runs the static bundle check.
 * Child processes are spawned by absolute path (node itself, npm-cli.js, vite's bin script).
 */

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { readConfig, verifyUpstream } from "./verify-rhwp-upstream.mjs";
import { verifyOverrides } from "./hwp-studio-overrides.mjs";
import { checkFonts } from "./verify-fonts.mjs";
import { ASSET_MANIFEST, DEFAULT_BUNDLE_DIR, checkBundle, listFiles, sha256 } from "./check-hwp-studio-bundle.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
const APP_DIR = path.join(REPO, "apps/hwp-studio");

function findNpmCli() {
  const nodeDir = path.dirname(process.execPath);
  const candidates = [
    process.env.npm_execpath && path.extname(process.env.npm_execpath) === ".js" ? process.env.npm_execpath : null,
    path.join(nodeDir, "node_modules/npm/bin/npm-cli.js"),
    path.join(nodeDir, "../lib/node_modules/npm/bin/npm-cli.js"),
  ].filter(Boolean);
  const found = candidates.find((candidate) => path.isAbsolute(candidate) && fs.existsSync(candidate));
  if (!found) throw new Error("npm-cli.js not found next to the node executable");
  return found;
}

function run(label, args) {
  console.log(`> ${label}`);
  const res = spawnSync(process.execPath, args, { cwd: APP_DIR, stdio: "inherit", env: { ...process.env, npm_config_audit: "false", npm_config_fund: "false" } });
  if (res.status !== 0) throw new Error(`${label} failed (exit ${res.status})`);
}

function failIf(errors, heading) {
  if (errors.length > 0) throw new Error(`${heading}:\n${errors.map((e) => "  - " + e).join("\n")}`);
}

function writeAssetManifest(dir) {
  const files = listFiles(dir)
    .filter((rel) => rel !== ASSET_MANIFEST)
    .map((rel) => {
      const data = fs.readFileSync(path.join(dir, rel));
      return { path: rel, bytes: data.length, sha256: sha256(data) };
    });
  fs.writeFileSync(path.join(dir, ASSET_MANIFEST), `${JSON.stringify({ schemaVersion: 1, files }, null, 2)}\n`);
  return files.length;
}

function main() {
  const args = new Set(process.argv.slice(2));
  const config = readConfig(REPO);
  failIf(verifyUpstream(REPO), "rhwp upstream verification failed");
  if (!fs.existsSync(path.join(REPO, config.path, ".git"))) {
    throw new Error(`${config.path} is not checked out; run node scripts/checkout-rhwp.mjs first`);
  }
  failIf(verifyOverrides(REPO, config), "override manifest verification failed");
  failIf(checkFonts(REPO), "font verification failed");
  if (!args.has("--skip-install")) run("npm ci --ignore-scripts", [findNpmCli(), "ci", "--ignore-scripts"]);
  run("vite build", [path.join(APP_DIR, "node_modules/vite/bin/vite.js"), "build"]);
  const count = writeAssetManifest(DEFAULT_BUNDLE_DIR);
  console.log(`wrote ${ASSET_MANIFEST} (${count} files)`);
  if (!args.has("--no-check")) {
    const { errors, stats } = checkBundle(DEFAULT_BUNDLE_DIR);
    failIf(errors, "bundle check failed");
    console.log(`bundle check passed: ${(stats.nonFontBytes / 1048576).toFixed(2)} MiB without fonts, ${(stats.fontBytes / 1048576).toFixed(2)} MiB fonts`);
  }
}

try {
  main();
} catch (err) {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
}
