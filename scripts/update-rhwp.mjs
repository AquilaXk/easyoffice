#!/usr/bin/env node
/**
 * Moves the rhwp upstream pin to an explicit release tag (PD-076, PD-081).
 *
 * Usage: node scripts/update-rhwp.mjs vX.Y.Z [--dry-run] [--accept-overrides]
 *   vX.Y.Z             explicit release tag only; branches, SHAs and `latest` are refused
 *   --dry-run          print the plan and change nothing
 *   --accept-overrides re-pin the hwp-studio override baseline (config/rhwp-studio-overrides.json)
 *                      after the overrides were reviewed against the new upstream files
 *
 * The tag is resolved on the remote to a commit, the new upstream files are read from that commit
 * without a checkout, and the whole plan is built first. Files are written only at the end: the pin
 * (config/rhwp-upstream.json), the override baseline, the gitlink and the sparse checkout. A failed
 * write restores the pin files and the gitlink. The vendored WASM is never rebuilt here.
 *
 * Reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim) of the reference app:
 * scripts/update-rhwp-upstream.mjs (explicit-tag argument, tag-to-commit resolution, version
 * alignment between tag and Cargo.toml, changed studio inputs report, restore on failure).
 * Node built-in modules only.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { normalizedSha256Text, MANIFEST_REL, studioFileRel } from "./hwp-studio-overrides.mjs";
import { CONFIG_REL, cargoVersion, checkConfig, readConfig, resolveGitExecutable } from "./verify-rhwp-upstream.mjs";

const TAG_RE = /^v(\d{1,4})\.(\d{1,4})\.(\d{1,4})$/;
const SHA_RE = /^[0-9a-f]{40}$/;
const GITLINK_MODE = "160000";
const GIT_TIMEOUT_MS = 120_000;
const MAX_FILE_BYTES = 4 * 1024 * 1024;
const MAX_LS_REMOTE_BYTES = 64 * 1024;
const TABLES = [
  ["counterparts", "overrides", "override"],
  ["assets", "assets", "asset"],
];

export function parseArgs(argv) {
  const opts = { tag: null, dryRun: false, acceptOverrides: false };
  for (const arg of argv) {
    if (arg === "--dry-run") opts.dryRun = true;
    else if (arg === "--accept-overrides") opts.acceptOverrides = true;
    else if (opts.tag === null && TAG_RE.test(arg)) opts.tag = arg;
    else throw new Error(`Unexpected argument '${arg.slice(0, 40)}' (only one explicit vX.Y.Z tag is accepted)`);
  }
  if (opts.tag === null) throw new Error("usage: update-rhwp.mjs vX.Y.Z [--dry-run] [--accept-overrides]");
  return opts;
}

/** Commit SHA for `tag` from `git ls-remote` output; an annotated tag's peeled commit wins. */
export function commitFromLsRemote(output, tag) {
  const refs = new Map();
  for (const line of output.split("\n")) {
    const m = /^([0-9a-f]{40})\t(refs\/tags\/[^\s]+)$/.exec(line.trim());
    if (m) refs.set(m[2], m[1]);
  }
  const sha = refs.get(`refs/tags/${tag}^{}`) ?? refs.get(`refs/tags/${tag}`);
  if (!sha || !SHA_RE.test(sha)) throw new Error(`tag ${tag} was not found on the upstream remote`);
  return sha;
}

function parseChannel(toolchainToml) {
  return /^\s*channel\s*=\s*"(\d+\.\d+\.\d+)"/m.exec(toolchainToml ?? "")?.[1] ?? null;
}

function git(cwd, args) {
  return execFileSync(resolveGitExecutable(), args, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    timeout: GIT_TIMEOUT_MS,
    maxBuffer: MAX_FILE_BYTES,
  });
}

/** Real remote: `git ls-remote` plus blob reads from a throwaway blob-less bare repository. */
export function createGitRemote() {
  let scratch = null;
  const fetched = new Set();
  const repoFor = (url, commit) => {
    if (scratch === null) {
      scratch = fs.mkdtempSync(path.join(os.tmpdir(), "eo-rhwp-update-"));
      git(scratch, ["init", "--bare", "-q"]);
      git(scratch, ["remote", "add", "origin", url]);
    }
    if (!fetched.has(commit)) {
      git(scratch, ["fetch", "-q", "--depth", "1", "--filter=blob:none", "origin", commit]);
      fetched.add(commit);
    }
    return scratch;
  };
  return {
    lsRemote(url, tag) {
      const out = git(process.cwd(), ["ls-remote", "--tags", url, `refs/tags/${tag}`, `refs/tags/${tag}^{}`]);
      if (out.length > MAX_LS_REMOTE_BYTES) throw new Error("ls-remote output is unexpectedly large");
      return out;
    },
    /** File text at `commit:relPath`, or null when the commit has no such file. */
    readFileAt(url, commit, relPath) {
      const repo = repoFor(url, commit);
      if (git(repo, ["ls-tree", "--name-only", commit, "--", relPath]).trim() === "") return null;
      return git(repo, ["show", `${commit}:${relPath}`]);
    },
    close() {
      if (scratch !== null) fs.rmSync(scratch, { recursive: true, force: true });
      scratch = null;
    },
  };
}

function readBaseline(remote, config, commit, manifest) {
  const upstream = manifest.upstream ?? {};
  const next = { counterparts: {}, assets: {} };
  const changed = [];
  const missing = [];
  for (const [table, kind, label] of TABLES) {
    for (const [id, before] of Object.entries(upstream[table] ?? {})) {
      const rel = `rhwp-studio/${studioFileRel(kind, id)}`;
      const text = remote.readFileAt(config.url, commit, rel);
      if (text === null) {
        missing.push(`${label} ${id} (${rel})`);
        continue;
      }
      next[table][id] = normalizedSha256Text(text);
      if (next[table][id] !== before) changed.push(`${label} ${id}`);
    }
  }
  return { next, changed, missing };
}

function stringify(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

/** Builds the whole update plan without touching the working tree. `problems` block applying it. */
export function planUpdate({ root, tag, acceptOverrides, remote }) {
  const config = readConfig(root);
  const configErrors = checkConfig(config);
  if (configErrors.length > 0) throw new Error(`current pin is invalid:\n${configErrors.join("\n")}`);
  const manifestText = fs.readFileSync(path.join(root, MANIFEST_REL), "utf8");
  const manifest = JSON.parse(manifestText);
  const configText = fs.readFileSync(path.join(root, CONFIG_REL), "utf8");

  const commit = commitFromLsRemote(remote.lsRemote(config.url, tag), tag);
  const version = tag.slice(1);
  const problems = [];
  const upstreamVersion = cargoVersion(remote.readFileAt(config.url, commit, "Cargo.toml") ?? "");
  if (upstreamVersion !== version) problems.push(`tag ${tag} does not match upstream Cargo.toml version ${upstreamVersion}`);
  const rustToolchain = parseChannel(remote.readFileAt(config.url, commit, "rust-toolchain.toml"));
  if (rustToolchain === null) problems.push("upstream rust-toolchain.toml has no channel");

  const baseline = readBaseline(remote, config, commit, manifest);
  if (baseline.missing.length > 0) problems.push(`upstream files are missing at ${tag}: ${baseline.missing.join(", ")}`);
  if (baseline.changed.length > 0 && !acceptOverrides) {
    problems.push(
      `forked upstream files changed since the overrides were written: ${baseline.changed.join(", ")}. `
        + `Re-apply the overrides in apps/hwp-studio, then run again with --accept-overrides`,
    );
  }

  const nextConfig = { ...config, commit, version, rustToolchain: rustToolchain ?? config.rustToolchain };
  const nextManifest = { ...manifest, upstream: { ...manifest.upstream, version, commit, ...baseline.next } };
  const files = [
    { rel: CONFIG_REL, before: configText, after: stringify(nextConfig) },
    { rel: MANIFEST_REL, before: manifestText, after: stringify(nextManifest) },
  ];
  return {
    tag, commit, version, previousCommit: config.commit, rustToolchain: nextConfig.rustToolchain,
    wasmPackVersion: config.wasmPackVersion, changedOverrides: baseline.changed, problems, files,
    unchanged: files.every((f) => f.before === f.after),
  };
}

/** Writes the plan. On failure the pin files and the gitlink are restored before the error is rethrown. */
export function applyPlan(root, plan, env) {
  const written = [];
  try {
    for (const file of plan.files) {
      fs.writeFileSync(path.join(root, file.rel), file.after);
      written.push(file);
    }
    env.setGitlink(plan.commit);
    env.checkout();
  } catch (err) {
    for (const file of written) fs.writeFileSync(path.join(root, file.rel), file.before);
    try {
      env.setGitlink(plan.previousCommit);
    } catch (restoreErr) {
      throw new AggregateError([err, restoreErr], "rhwp update failed and the gitlink could not be restored");
    }
    throw new Error(
      `rhwp update failed; pin files and gitlink were restored (run node scripts/checkout-rhwp.mjs to realign the work tree): ${err.message}`,
    );
  }
}

function describePlan(plan, dryRun) {
  const lines = [
    `${dryRun ? "Plan" : "Updating"}: rhwp ${plan.previousCommit.slice(0, 8)} -> ${plan.tag} (${plan.commit})`,
    `  ${CONFIG_REL}: commit, version ${plan.version}, rustToolchain ${plan.rustToolchain}`,
    `  ${MANIFEST_REL}: upstream baseline (${plan.changedOverrides.length} changed: ${plan.changedOverrides.join(", ") || "none"})`,
    "  gitlink third_party/rhwp and sparse checkout via scripts/checkout-rhwp.mjs",
  ];
  return lines.join("\n");
}

function nextSteps(plan) {
  return [
    "The vendored WASM was NOT rebuilt. Next:",
    `  1. cd third_party/rhwp && RUSTUP_TOOLCHAIN=${plan.rustToolchain} CARGO_TARGET_DIR=<outside the submodule> wasm-pack build --target web --out-dir <abs outDir> --release (wasm-pack ${plan.wasmPackVersion})`,
    "  2. node scripts/vendor-rhwp-core.mjs <outDir> --wasm-pack <abs path to wasm-pack>   (rewrites vendor/rhwp-core/PROVENANCE.json)",
    "  3. node scripts/verify-rhwp-upstream.mjs && node scripts/check-repo-hygiene.mjs",
    "  4. Re-check THIRD_PARTY_NOTICES.md and run the hwp-studio build and tests.",
  ].join("\n");
}

/** Runs the command; `env` supplies the remote and the side effects so tests can fake them. */
export function runUpdate(argv, env) {
  const opts = parseArgs(argv);
  const plan = planUpdate({ root: env.root, tag: opts.tag, acceptOverrides: opts.acceptOverrides, remote: env.remote });
  if (plan.unchanged && plan.problems.length === 0) {
    env.log(`rhwp is already pinned to ${plan.tag} (${plan.commit}); nothing to update.`);
    return;
  }
  env.log(describePlan(plan, opts.dryRun));
  if (plan.problems.length > 0) throw new Error(`rhwp update refused:\n- ${plan.problems.join("\n- ")}`);
  if (opts.dryRun) return;
  applyPlan(env.root, plan, env);
  env.log(`rhwp pin moved to ${plan.tag}.\n${nextSteps(plan)}`);
}

function realEnv(root, remote) {
  return {
    root,
    remote,
    log: (message) => console.log(message),
    setGitlink: (sha) => git(root, ["update-index", "--add", "--cacheinfo", `${GITLINK_MODE},${sha},third_party/rhwp`]),
    checkout: () =>
      execFileSync(process.execPath, [path.join(root, "scripts/checkout-rhwp.mjs"), "--skip-vendor-check"], { cwd: root, stdio: "inherit" }),
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const remote = createGitRemote();
  try {
    runUpdate(process.argv.slice(2), realEnv(process.cwd(), remote));
  } catch (err) {
    console.error(err instanceof Error ? err.message : String(err));
    process.exitCode = 1;
  } finally {
    remote.close();
  }
}
