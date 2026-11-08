#!/usr/bin/env node
/**
 * rhwp upstream pin guard (PD-076).
 *
 * rhwp is a read-only upstream submodule at third_party/rhwp. This guard fails closed when:
 *  - config/rhwp-upstream.json is malformed,
 *  - .gitmodules lists anything but the pinned URL / path / shallow entry,
 *  - the recorded gitlink (git ls-files -s) is missing, not a gitlink, or not the pinned commit,
 *  - the submodule work tree is checked out and its remote, HEAD, version or toolchain differ,
 *    or it contains any local modification (tracked or untracked),
 *  - the vendored WASM package (vendor/rhwp-core, PD-081) is missing, tampered with or unlisted.
 * With no checkout (the CI default) only the superproject records are verified.
 * Uses Node built-in modules only.
 */

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { checkVendorCore } from "./verify-rhwp-core.mjs";

export const CONFIG_REL = "config/rhwp-upstream.json";
export const SUBMODULE_PATH = "third_party/rhwp";
export const PINNED_URL = "https://github.com/edwardkim/rhwp.git";
const GITLINK_MODE = "160000";
const SHA_RE = /^[0-9a-f]{40}$/;
const SEMVER_RE = /^\d+\.\d+\.\d+$/;

// Fixed, admin-owned install locations are preferred over a PATH lookup so a writable PATH entry
// cannot substitute another `git`. GIT_EXECUTABLE (absolute path) lets CI or developers pin it.
const FIXED_GIT_CANDIDATES =
  process.platform === "win32"
    ? [
        String.raw`C:\Program Files\Git\cmd\git.exe`,
        String.raw`C:\Program Files\Git\bin\git.exe`,
        String.raw`C:\Program Files (x86)\Git\cmd\git.exe`,
      ]
    : ["/usr/bin/git", "/usr/local/bin/git", "/opt/homebrew/bin/git"];

function findOnSearchPath(name) {
  const exts = process.platform === "win32" ? [".exe", ".cmd"] : [""];
  for (const dir of (process.env.PATH || "").split(path.delimiter)) {
    if (!dir || !path.isAbsolute(dir)) continue;
    for (const ext of exts) {
      const candidate = path.join(dir, name + ext);
      if (fs.existsSync(candidate)) return candidate;
    }
  }
  return null;
}

let cachedGitExecutable = null;

/** Absolute path of the git executable: GIT_EXECUTABLE, then fixed install dirs, then search path. */
export function resolveGitExecutable() {
  if (cachedGitExecutable) return cachedGitExecutable;
  const pinned = process.env.GIT_EXECUTABLE;
  const candidates = pinned && path.isAbsolute(pinned) ? [pinned, ...FIXED_GIT_CANDIDATES] : FIXED_GIT_CANDIDATES;
  const found = candidates.find((candidate) => fs.existsSync(candidate)) || findOnSearchPath("git");
  if (!found) throw new Error("git executable not found (set GIT_EXECUTABLE to an absolute path)");
  cachedGitExecutable = found;
  return found;
}

function git(cwd, args) {
  return execFileSync(resolveGitExecutable(), args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
}

export function readConfig(root) {
  return JSON.parse(fs.readFileSync(path.join(root, CONFIG_REL), "utf8"));
}

export function checkConfig(config) {
  const errors = [];
  if (config.url !== PINNED_URL) errors.push(`${CONFIG_REL}: url must be ${PINNED_URL}`);
  if (config.path !== SUBMODULE_PATH) errors.push(`${CONFIG_REL}: path must be ${SUBMODULE_PATH}`);
  if (!SHA_RE.test(String(config.commit))) errors.push(`${CONFIG_REL}: commit must be a full 40-hex SHA`);
  if (!SEMVER_RE.test(String(config.version))) errors.push(`${CONFIG_REL}: version must be x.y.z`);
  if (!SEMVER_RE.test(String(config.rustToolchain))) errors.push(`${CONFIG_REL}: rustToolchain must be x.y.z`);
  if (!SEMVER_RE.test(String(config.wasmPackVersion))) errors.push(`${CONFIG_REL}: wasmPackVersion must be x.y.z`);
  const sparse = config.sparse;
  if (!Array.isArray(sparse) || sparse.length === 0 || !sparse.every((p) => typeof p === "string" && p.startsWith("/"))) {
    errors.push(`${CONFIG_REL}: sparse must be a non-empty list of root-anchored patterns`);
  }
  return errors;
}

/** Parses .gitmodules text into Map(name -> {key: value}). */
export function parseGitmodules(text) {
  const entries = new Map();
  let current = null;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#") || line.startsWith(";")) continue;
    const header = /^\[submodule\s+"([^"]+)"\]$/.exec(line);
    if (header) {
      current = {};
      entries.set(header[1], current);
      continue;
    }
    const kv = /^([A-Za-z][\w-]*)\s*=\s*(\S.*|)$/.exec(line);
    if (kv && current) current[kv[1].toLowerCase()] = kv[2].trim();
  }
  return entries;
}

export function checkGitmodules(text, config) {
  const errors = [];
  const entries = parseGitmodules(text);
  for (const name of entries.keys()) {
    if (name !== config.path) errors.push(`.gitmodules: unexpected submodule '${name}' (only ${config.path} is allowed)`);
  }
  const entry = entries.get(config.path);
  if (!entry) {
    errors.push(`.gitmodules: missing entry for ${config.path}`);
    return errors;
  }
  if (entry.path !== config.path) errors.push(`.gitmodules: path must be ${config.path}`);
  if (entry.url !== config.url) errors.push(`.gitmodules: url '${entry.url}' differs from ${config.url}`);
  if (entry.shallow !== "true") errors.push(".gitmodules: shallow = true is required");
  return errors;
}

export function parseGitlink(lsFilesOutput) {
  const line = lsFilesOutput.split("\n").map((s) => s.trim()).find(Boolean);
  if (!line) return null;
  const m = /^(\d+)\s+([0-9a-f]+)\s+\d+\t(.+)$/.exec(line);
  return m ? { mode: m[1], sha: m[2], file: m[3] } : null;
}

export function checkGitlink(lsFilesOutput, config) {
  const link = parseGitlink(lsFilesOutput);
  if (!link) return [`gitlink ${config.path} is not recorded in the index`];
  if (link.mode !== GITLINK_MODE) return [`${config.path} must be a gitlink (mode ${GITLINK_MODE}), found ${link.mode}`];
  if (link.sha !== config.commit) {
    return [`gitlink commit ${link.sha} differs from ${CONFIG_REL} commit ${config.commit}`];
  }
  return [];
}

export function cargoVersion(text) {
  const pkg = /\[package\]([\s\S]*?)(?:\n\[|$)/.exec(text);
  const m = pkg ? /^version\s*=\s*"([^"]+)"/m.exec(pkg[1]) : null;
  return m ? m[1] : null;
}

function readIfExists(file) {
  return fs.existsSync(file) ? fs.readFileSync(file, "utf8") : null;
}

export function checkWorktree(root, config) {
  const dir = path.join(root, config.path);
  if (!fs.existsSync(path.join(dir, ".git"))) return [];
  const errors = [];
  try {
    const head = git(dir, ["rev-parse", "HEAD"]).trim();
    if (head !== config.commit) errors.push(`${config.path} HEAD ${head} differs from pinned ${config.commit}`);
    const porcelain = git(dir, ["status", "--porcelain", "--ignored=no"]).trim();
    if (porcelain) {
      errors.push(`${config.path} work tree is modified (upstream must stay read-only):\n${porcelain.split("\n").slice(0, 10).join("\n")}`);
    }
    const remote = git(dir, ["config", "--get", "remote.origin.url"]).trim();
    if (remote !== config.url) errors.push(`${config.path} origin '${remote}' differs from ${config.url}`);
  } catch (err) {
    errors.push(`${config.path} work tree cannot be inspected: ${String(err.stderr || err.message).trim()}`);
  }
  const cargo = readIfExists(path.join(dir, "Cargo.toml"));
  if (cargo !== null && cargoVersion(cargo) !== config.version) {
    errors.push(`${config.path}/Cargo.toml version ${cargoVersion(cargo)} differs from ${CONFIG_REL} version ${config.version}`);
  }
  const toolchain = readIfExists(path.join(dir, "rust-toolchain.toml"));
  const channel = toolchain ? /channel\s*=\s*"([^"]+)"/.exec(toolchain) : null;
  if (toolchain !== null && channel?.[1] !== config.rustToolchain) {
    errors.push(`${config.path}/rust-toolchain.toml channel ${channel?.[1]} differs from ${CONFIG_REL} rustToolchain ${config.rustToolchain}`);
  }
  return errors;
}

/** Arguments and sparse patterns used by scripts/checkout-rhwp.mjs. */
export function sparseCheckoutPlan(config) {
  return {
    submoduleArgs: ["--depth", "1", "--filter=blob:none"],
    sparse: [...config.sparse],
  };
}

/** `skipVendor` is for scripts/update-rhwp.mjs only: the vendored WASM is refreshed after a pin change. */
export function verifyUpstream(root, { skipVendor = false } = {}) {
  let config;
  try {
    config = readConfig(root);
  } catch (err) {
    return [`${CONFIG_REL} cannot be read: ${err.message}`];
  }
  const errors = checkConfig(config);
  if (errors.length > 0) return errors;
  const modules = readIfExists(path.join(root, ".gitmodules"));
  if (modules === null) errors.push(".gitmodules is missing");
  else errors.push(...checkGitmodules(modules, config));
  try {
    errors.push(...checkGitlink(git(root, ["ls-files", "-s", "--", config.path]), config));
  } catch (err) {
    errors.push(`git ls-files failed: ${String(err.stderr || err.message).trim()}`);
  }
  errors.push(...checkWorktree(root, config));
  if (!skipVendor) errors.push(...checkVendorCore(root, config));
  return errors;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const errors = verifyUpstream(process.cwd());
  if (errors.length > 0) {
    console.error("rhwp upstream verification failed:");
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }
  const cfg = readConfig(process.cwd());
  const checked = fs.existsSync(path.join(process.cwd(), cfg.path, ".git")) ? "work tree checked out" : "no checkout";
  console.log(`rhwp upstream verified (${cfg.version} @ ${cfg.commit.slice(0, 8)}, ${checked}).`);
}
