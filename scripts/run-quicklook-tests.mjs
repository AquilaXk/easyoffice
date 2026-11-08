#!/usr/bin/env node
// Format, lint and test gate of the Quick Look Rust crate (PD-140), macOS only. It is not a
// `test-*.mjs` file on purpose: `node --test scripts/test-*.mjs` runs on every platform and must stay
// fast; this one compiles rhwp natively and needs the pinned upstream checkout
// (`node scripts/checkout-rhwp.mjs`).
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { MIN_MACOS, repoRoot, rustRoot } from "./quicklook-config.mjs";

if (process.platform !== "darwin") {
  console.log("[quicklook] non-macOS platform; skipping the Quick Look crate checks");
  process.exit(0);
}

const manifest = join(rustRoot, "Cargo.toml");
run("cargo", ["fmt", "--manifest-path", manifest, "--", "--check"]);
run("cargo", ["clippy", "--manifest-path", manifest, "--locked", "--all-targets", "--", "-D", "warnings"]);
// Release: the hostile-input tests render real documents, which takes minutes in a debug build.
run("cargo", ["test", "--manifest-path", manifest, "--locked", "--release"]);

function run(command, args) {
  console.log(`[quicklook] ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
    env: { ...process.env, MACOSX_DEPLOYMENT_TARGET: process.env.MACOSX_DEPLOYMENT_TARGET || MIN_MACOS },
  });
  if (result.status !== 0) throw new Error(`${command} failed with exit code ${result.status}`);
}
