#!/usr/bin/env node
/**
 * Copies a wasm-pack output directory into vendor/rhwp-core/ and rewrites PROVENANCE.json.
 * The WASM is built locally (never in CI) per docs/rhwp-integration-design.md:
 *
 *   cd third_party/rhwp   (sparse checkout from scripts/checkout-rhwp.mjs)
 *   RUSTUP_TOOLCHAIN=<1.93.1 host toolchain> CARGO_TARGET_DIR=<outside the submodule> \
 *     <abs>/wasm-pack build --target web --out-dir <abs outDir> --release
 *   node scripts/vendor-rhwp-core.mjs <outDir> --wasm-pack <abs>/wasm-pack [--host "<description>"]
 *
 * The wasm-pack path is only used to read `--version` and must be an absolute path.
 */

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { readConfig } from "./verify-rhwp-upstream.mjs";
import { PROVENANCE_REL, REQUIRED_VENDOR_FILES, VENDOR_DIR_REL, checkVendorCore, describeArtifacts } from "./verify-rhwp-core.mjs";

function parseArgs(argv) {
  const opts = { outDir: null, wasmPack: null, host: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--wasm-pack" && argv[i + 1]) opts.wasmPack = argv[++i];
    else if (argv[i] === "--host" && argv[i + 1]) opts.host = argv[++i];
    else if (!argv[i].startsWith("--") && !opts.outDir) opts.outDir = argv[i];
    else throw new Error(`Unknown argument: ${argv[i]}`);
  }
  if (!opts.outDir || !opts.wasmPack || !path.isAbsolute(opts.wasmPack)) {
    throw new Error("usage: vendor-rhwp-core.mjs <outDir> --wasm-pack <absolute path> [--host <text>]");
  }
  return opts;
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  const root = process.cwd();
  const config = readConfig(root);
  const version = execFileSync(opts.wasmPack, ["--version"], { encoding: "utf8" }).trim().replace(/^wasm-pack\s+/, "");
  if (version !== config.wasmPackVersion) {
    throw new Error(`wasm-pack ${version} differs from config wasmPackVersion ${config.wasmPackVersion}`);
  }
  const target = path.join(root, VENDOR_DIR_REL);
  fs.mkdirSync(target, { recursive: true });
  for (const entry of fs.readdirSync(target)) fs.rmSync(path.join(target, entry), { recursive: true, force: true });
  for (const name of REQUIRED_VENDOR_FILES) fs.copyFileSync(path.join(opts.outDir, name), path.join(target, name));
  const provenance = {
    schemaVersion: 1,
    source: "https://github.com/edwardkim/rhwp",
    version: config.version,
    commit: config.commit,
    rustToolchain: config.rustToolchain,
    wasmPackVersion: config.wasmPackVersion,
    buildCommand: "wasm-pack build --target web --release (cwd: third_party/rhwp, default features, no extra RUSTFLAGS)",
    buildHost: opts.host ?? `${process.platform}-${process.arch}`,
    artifacts: describeArtifacts(target),
  };
  fs.writeFileSync(path.join(root, PROVENANCE_REL), `${JSON.stringify(provenance, null, 2)}\n`);
  const errors = checkVendorCore(root, config);
  if (errors.length > 0) throw new Error(errors.join("\n"));
  console.log(`vendored ${REQUIRED_VENDOR_FILES.length} files into ${VENDOR_DIR_REL}`);
}

try {
  main();
} catch (err) {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
}
