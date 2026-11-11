/**
 * Shared by scripts/bench-run.mjs and scripts/bench-profile.mjs: finds the release-mode unit-test
 * executable that holds a measurement harness.
 */

import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** The executable of the unit tests of a package, built in release mode. */
export function testBinary(args) {
  const out = spawnSync("cargo", ["test", "--release", "--no-run", "--message-format=json", ...args], {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 1 << 28,
    stdio: ["ignore", "pipe", "inherit"],
  });
  if (out.status !== 0) throw new Error(`cargo test --no-run ${args.join(" ")} failed`);
  const artifacts = out.stdout
    .split("\n")
    .filter((line) => line.startsWith("{"))
    .map((line) => JSON.parse(line))
    .filter((msg) => msg.reason === "compiler-artifact" && msg.profile?.test && msg.executable);
  // The unit tests of the library hold the harness; a binary crate's own unit tests are the fallback.
  const unit =
    artifacts.find((a) => a.target.kind.some((kind) => kind === "lib" || kind === "rlib")) ??
    artifacts.find((a) => a.target.kind.includes("bin"));
  if (!unit) throw new Error(`no unit-test executable for ${args.join(" ")}`);
  return unit.executable;
}
