#!/usr/bin/env node
/**
 * Vendored rhwp WASM guard (PD-081).
 *
 * vendor/rhwp-core/ holds the prebuilt wasm-pack output of the pinned rhwp commit. CI never
 * rebuilds it; instead this guard fails closed when
 *  - PROVENANCE.json is missing or malformed, or disagrees with config/rhwp-upstream.json
 *    (commit, version, Rust toolchain, wasm-pack version),
 *  - a listed file is missing, has a different size or SHA-256,
 *  - the directory contains a file that PROVENANCE.json does not list.
 * Uses Node built-in modules only.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const VENDOR_DIR_REL = "vendor/rhwp-core";
export const PROVENANCE_REL = `${VENDOR_DIR_REL}/PROVENANCE.json`;
export const REQUIRED_VENDOR_FILES = ["rhwp_bg.wasm", "rhwp.js", "rhwp.d.ts", "rhwp_bg.wasm.d.ts", "package.json", "LICENSE"];
const SHA256_RE = /^[0-9a-f]{64}$/;
const SAFE_NAME_RE = /^[A-Za-z0-9_.-]+$/;

export function sha256File(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function checkProvenanceFields(provenance, config) {
  const errors = [];
  if (provenance.schemaVersion !== 1) errors.push(`${PROVENANCE_REL}: schemaVersion must be 1`);
  const pairs = [
    ["commit", config.commit],
    ["version", config.version],
    ["rustToolchain", config.rustToolchain],
    ["wasmPackVersion", config.wasmPackVersion],
  ];
  for (const [key, expected] of pairs) {
    if (provenance[key] !== expected) {
      errors.push(`${PROVENANCE_REL}: ${key} '${provenance[key]}' differs from config/rhwp-upstream.json '${expected}'`);
    }
  }
  const command = provenance.buildCommand;
  if (typeof command !== "string" || !command.includes("wasm-pack") || !command.includes("--target web")) {
    errors.push(`${PROVENANCE_REL}: buildCommand must record the wasm-pack invocation`);
  }
  return errors;
}

function checkArtifactEntries(artifacts) {
  const errors = [];
  if (!artifacts || typeof artifacts !== "object") return [`${PROVENANCE_REL}: artifacts must be an object`];
  for (const name of REQUIRED_VENDOR_FILES) {
    if (!(name in artifacts)) errors.push(`${PROVENANCE_REL}: artifact ${name} is not listed`);
  }
  for (const [name, entry] of Object.entries(artifacts)) {
    if (!SAFE_NAME_RE.test(name)) errors.push(`${PROVENANCE_REL}: unsafe artifact name '${name}'`);
    if (!Number.isInteger(entry?.bytes) || entry.bytes <= 0) errors.push(`${PROVENANCE_REL}: ${name} bytes must be a positive integer`);
    if (!SHA256_RE.test(String(entry?.sha256))) errors.push(`${PROVENANCE_REL}: ${name} sha256 must be 64 lowercase hex`);
  }
  return errors;
}

function checkArtifactFiles(dir, artifacts) {
  const errors = [];
  for (const [name, entry] of Object.entries(artifacts)) {
    if (!SAFE_NAME_RE.test(name)) continue;
    const file = path.join(dir, name);
    if (!fs.existsSync(file)) {
      errors.push(`${VENDOR_DIR_REL}/${name} is missing`);
      continue;
    }
    const size = fs.statSync(file).size;
    if (size !== entry.bytes) {
      errors.push(`${VENDOR_DIR_REL}/${name} is ${size} bytes, PROVENANCE.json records ${entry.bytes}`);
      continue;
    }
    if (sha256File(file) !== entry.sha256) errors.push(`${VENDOR_DIR_REL}/${name} SHA-256 differs from PROVENANCE.json`);
  }
  for (const name of fs.readdirSync(dir)) {
    if (name !== "PROVENANCE.json" && !(name in artifacts)) {
      errors.push(`${VENDOR_DIR_REL}/${name} is not listed in PROVENANCE.json`);
    }
  }
  return errors;
}

/** Returns a list of problems (empty when the vendored WASM package matches its provenance). */
export function checkVendorCore(root, config) {
  const dir = path.join(root, VENDOR_DIR_REL);
  const provenanceFile = path.join(root, PROVENANCE_REL);
  if (!fs.existsSync(provenanceFile)) return [`${PROVENANCE_REL} is missing`];
  let provenance;
  try {
    provenance = JSON.parse(fs.readFileSync(provenanceFile, "utf8"));
  } catch (err) {
    return [`${PROVENANCE_REL} cannot be parsed: ${err.message}`];
  }
  const errors = [...checkProvenanceFields(provenance, config), ...checkArtifactEntries(provenance.artifacts)];
  if (errors.length > 0) return errors;
  return checkArtifactFiles(dir, provenance.artifacts);
}

/** Builds the artifacts table (bytes + SHA-256) for a directory; used by scripts/vendor-rhwp-core.mjs. */
export function describeArtifacts(dir) {
  const artifacts = {};
  for (const name of REQUIRED_VENDOR_FILES) {
    const file = path.join(dir, name);
    artifacts[name] = { bytes: fs.statSync(file).size, sha256: sha256File(file) };
  }
  return artifacts;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = process.cwd();
  const config = JSON.parse(fs.readFileSync(path.join(root, "config/rhwp-upstream.json"), "utf8"));
  const errors = checkVendorCore(root, config);
  if (errors.length > 0) {
    console.error("rhwp vendored WASM verification failed:");
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }
  console.log(`rhwp vendored WASM verified (${REQUIRED_VENDOR_FILES.length} files).`);
}
