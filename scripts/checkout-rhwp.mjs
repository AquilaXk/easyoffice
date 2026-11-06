#!/usr/bin/env node
/**
 * Materialises the rhwp upstream submodule (third_party/rhwp) as a shallow, blob-filtered,
 * sparse checkout limited to config/rhwp-upstream.json "sparse". The full upstream is about
 * 5.3 GB (docs, PDFs, samples, tools), so a plain `git submodule update --init` must not be used.
 *
 * Usage: node scripts/checkout-rhwp.mjs [--dry-run] [--source <url-or-path>]
 *   --dry-run  print the plan only
 *   --source   fetch from another location (mirror or local clone); the work tree origin stays
 *              the pinned URL. Filters are skipped for plain paths.
 * Nothing is fetched by CI by default (PD-076).
 */

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { readConfig, sparseCheckoutPlan, verifyUpstream } from "./verify-rhwp-upstream.mjs";

function run(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
}

function parseArgs(argv) {
  const opts = { dryRun: false, source: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--dry-run") opts.dryRun = true;
    else if (argv[i] === "--source" && argv[i + 1]) opts.source = argv[++i];
    else throw new Error(`Unknown argument: ${argv[i]}`);
  }
  return opts;
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  const root = process.cwd();
  const config = readConfig(root);
  const plan = sparseCheckoutPlan(config);
  const dir = path.join(root, config.path);
  const source = opts.source ?? config.url;
  const remote = /^[a-z]+:\/\//i.test(source);
  // A custom source (mirror or local clone) may not advertise filtering or fetch-by-SHA by default.
  const sourceOpts = opts.source ? ["-c", "uploadpack.allowFilter=true", "-c", "uploadpack.allowAnySHA1InWant=true"] : [];
  const fetchArgs = [...sourceOpts, "fetch", "--depth", "1", ...(remote ? [plan.submoduleArgs[2]] : []), source, config.commit];

  console.log(`rhwp ${config.version} @ ${config.commit}`);
  console.log(`sparse patterns:\n${plan.sparse.map((p) => `  ${p}`).join("\n")}`);
  if (opts.dryRun) {
    console.log(`would run: git ${fetchArgs.join(" ")} (in ${config.path})`);
    return;
  }

  run(root, ["submodule", "init", "--", config.path]);
  fs.mkdirSync(dir, { recursive: true });
  if (!fs.existsSync(path.join(dir, ".git"))) {
    run(dir, ["init", "-q"]);
    run(dir, ["remote", "add", "origin", config.url]);
  }
  run(dir, ["config", "remote.origin.url", config.url]);
  run(dir, ["sparse-checkout", "init", "--no-cone"]);
  run(dir, ["sparse-checkout", "set", "--no-cone", ...plan.sparse]);
  run(dir, fetchArgs);
  run(dir, ["checkout", "-q", "--detach", config.commit]);
  const head = run(dir, ["rev-parse", "HEAD"]).trim();
  if (head !== config.commit) throw new Error(`checked out ${head}, expected ${config.commit}`);

  const errors = verifyUpstream(root);
  if (errors.length > 0) throw new Error(`verification failed:\n${errors.join("\n")}`);
  console.log(`checked out ${config.path} at ${head.slice(0, 8)} (sparse, shallow).`);
}

try {
  main();
} catch (err) {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
}
