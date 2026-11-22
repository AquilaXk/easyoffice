#!/usr/bin/env node
// Builds the Quick Look preview and thumbnail extensions (PD-140) and stages them under
// target/quicklook/PlugIns, where `bundle.macOS.files` of the desktop shell picks them up.
// Run it before `tauri build`. Other platforms: no-op.
//
//   node scripts/build-quicklook-macos.mjs                   host architecture only (local use)
//   node scripts/build-quicklook-macos.mjs --arch <arch>     one architecture, arm64 or x86_64 (release, PD-297)
//   node scripts/build-quicklook-macos.mjs --universal       arm64 + x86_64 in one binary (CI)
//
// Environment: APPLE_SIGNING_IDENTITY (default "-", ad-hoc, PD-094), EASYOFFICE_VERSION (the release
// version; default the workspace version), EASYOFFICE_QUICKLOOK_TARGETS (comma-separated Rust targets).
// Structure after the reference app's `scripts/build-quicklook-macos.mjs` (MIT, see
// THIRD_PARTY_NOTICES.md): Rust static library, then `swiftc` per extension, then `codesign` with the
// sandbox entitlements.
import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  EXTENSIONS, MIN_MACOS, appVersion, entitlements, ffiHeader, plugInsDir, quicklookTargets, repoRoot, rustRoot,
  staticLibName, stagingRoot, swiftTarget,
} from "./quicklook-config.mjs";

if (process.platform !== "darwin") {
  console.log("[quicklook] non-macOS platform; skipping the Quick Look extension build");
  process.exit(0);
}

const targets = quicklookTargets(process.argv.slice(2));
const version = appVersion();
const identity = process.env.APPLE_SIGNING_IDENTITY || "-";
const libDir = join(stagingRoot, "lib");

rmSync(stagingRoot, { recursive: true, force: true });
mkdirSync(plugInsDir, { recursive: true });
mkdirSync(libDir, { recursive: true });

const staticLibs = new Map(targets.map((target) => [target, buildRustStaticLibrary(target)]));
for (const ext of EXTENSIONS) buildExtension(ext);
console.log(`[quicklook] staged ${EXTENSIONS.length} extensions (${targets.join(", ")}) in ${plugInsDir}`);

function buildRustStaticLibrary(target) {
  run("cargo", ["build", "--manifest-path", join(rustRoot, "Cargo.toml"), "--locked", "--release", "--target", target]);
  const built = join(rustRoot, "target", target, "release", staticLibName);
  if (!existsSync(built)) throw new Error(`the Rust static library was not produced: ${built}`);
  const staged = join(libDir, `${target}-${staticLibName}`);
  copyFileSync(built, staged);
  return staged;
}

function buildExtension(ext) {
  const appexDir = join(plugInsDir, ext.appexName);
  const contentsDir = join(appexDir, "Contents");
  const macosDir = join(contentsDir, "MacOS");
  mkdirSync(macosDir, { recursive: true });
  writeFileSync(join(contentsDir, "Info.plist"), readFileSync(ext.infoPlist, "utf8").replaceAll("@VERSION@", version));

  const slices = targets.map((target) => {
    const output = join(libDir, `${ext.moduleName}-${target}`);
    run("xcrun", [
      "swiftc", "-target", swiftTarget(target), "-O", "-application-extension", "-parse-as-library",
      "-emit-executable", "-module-name", ext.moduleName, "-import-objc-header", ffiHeader, "-o", output,
      ...ext.sources, staticLibs.get(target),
      "-Xlinker", "-e", "-Xlinker", "_NSExtensionMain",
      // The static library holds all of rhwp; only what the three entry points reach is kept.
      "-Xlinker", "-dead_strip",
      ...ext.frameworks.flatMap((framework) => ["-framework", framework]),
    ]);
    // PD-293: nothing reads the symbol table at run time (the FFI catches panics with `catch_unwind`),
    // and the 16,000 symbols of the linked rhwp library are a fifth of each slice. The codesign below
    // runs after this and signs the stripped file.
    run("xcrun", ["strip", output]);
    return output;
  });
  const executable = join(macosDir, ext.moduleName);
  if (slices.length === 1) copyFileSync(slices[0], executable);
  else run("lipo", ["-create", "-output", executable, ...slices]);

  const timestamp = identity === "-" ? ["--timestamp=none"] : ["--timestamp"];
  run("codesign", ["--force", "--sign", identity, ...timestamp, "--options", "runtime", "--entitlements", entitlements, appexDir]);
}

function run(command, args) {
  console.log(`[quicklook] ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
    env: { ...process.env, MACOSX_DEPLOYMENT_TARGET: process.env.MACOSX_DEPLOYMENT_TARGET || MIN_MACOS },
  });
  if (result.status !== 0) throw new Error(`${command} failed with exit code ${result.status}`);
}
