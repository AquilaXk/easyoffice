#!/usr/bin/env node
// Checks the Quick Look extensions inside a built EasyOffice.app (PD-140):
//   node scripts/verify-quicklook-bundle.mjs <EasyOffice.app> [--archs arm64,x86_64]
// Each extension must be present with the identity, extension point, supported types and version the
// app expects, be signed with the sandbox entitlements, and contain the requested CPU architectures.
// The supported types must be UTIs the app itself exports, so the two lists cannot drift apart.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { EXTENSIONS, MIN_MACOS, byCodeUnits } from "./quicklook-config.mjs";

const SANDBOX_KEYS = ["com.apple.security.app-sandbox", "com.apple.security.files.user-selected.read-only"];

/** Problems of one extension's Info.plist (as JSON) against the app's. Empty when it is right. */
export function checkExtensionInfo(info, ext, appInfo) {
  const problems = [];
  const expect = (what, actual, wanted) => {
    if (actual !== wanted) problems.push(`${ext.appexName}: ${what} is ${JSON.stringify(actual)}, expected ${JSON.stringify(wanted)}`);
  };
  const extension = info?.NSExtension ?? {};
  expect("CFBundleIdentifier", info?.CFBundleIdentifier, ext.bundleId);
  expect("CFBundleExecutable", info?.CFBundleExecutable, ext.moduleName);
  expect("CFBundlePackageType", info?.CFBundlePackageType, "XPC!");
  expect("NSExtensionPointIdentifier", extension.NSExtensionPointIdentifier, ext.pointIdentifier);
  expect("NSExtensionPrincipalClass", extension.NSExtensionPrincipalClass, ext.principalClass);
  expect("LSMinimumSystemVersion", info?.LSMinimumSystemVersion, MIN_MACOS);
  expect("CFBundleShortVersionString", info?.CFBundleShortVersionString, appInfo?.CFBundleShortVersionString);
  const supported = extension.NSExtensionAttributes?.QLSupportedContentTypes;
  const exported = new Set((appInfo?.UTExportedTypeDeclarations ?? []).map((type) => type.UTTypeIdentifier));
  if (!Array.isArray(supported) || supported.length === 0) {
    problems.push(`${ext.appexName}: QLSupportedContentTypes is empty`);
  } else {
    for (const type of supported) {
      if (!exported.has(type)) problems.push(`${ext.appexName}: supports '${type}', which the app does not export`);
    }
  }
  return problems;
}

function capture(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8" });
  return { status: result.status, stdout: result.stdout ?? "", stderr: result.stderr ?? "" };
}

function plistJson(path) {
  const result = capture("plutil", ["-convert", "json", "-o", "-", path]);
  if (result.status !== 0) throw new Error(`plutil could not read ${path}: ${result.stderr}`);
  return JSON.parse(result.stdout);
}

function verifyApp(app, archs) {
  const problems = [];
  const appInfo = plistJson(join(app, "Contents/Info.plist"));
  for (const ext of EXTENSIONS) {
    const appex = join(app, "Contents/PlugIns", ext.appexName);
    const executable = join(appex, "Contents/MacOS", ext.moduleName);
    if (!existsSync(executable)) {
      problems.push(`${ext.appexName}: missing ${executable}`);
      continue;
    }
    problems.push(...checkExtensionInfo(plistJson(join(appex, "Contents/Info.plist")), ext, appInfo));
    const found = capture("lipo", ["-archs", executable]).stdout.trim().split(/\s+/).sort(byCodeUnits);
    if (found.join(" ") !== [...archs].sort(byCodeUnits).join(" ")) {
      problems.push(`${ext.appexName}: architectures are [${found}], expected [${archs}]`);
    }
    const signature = capture("codesign", ["--verify", "--strict", appex]);
    if (signature.status !== 0) problems.push(`${ext.appexName}: codesign --verify failed: ${signature.stderr.trim()}`);
    const entitlements = capture("codesign", ["-d", "--entitlements", "-", appex]);
    for (const key of SANDBOX_KEYS) {
      if (!(entitlements.stdout + entitlements.stderr).includes(key)) problems.push(`${ext.appexName}: not signed with ${key}`);
    }
  }
  return problems;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.platform !== "darwin") {
    console.log("[quicklook] non-macOS platform; skipping the bundle check");
    process.exit(0);
  }
  const app = process.argv[2];
  if (!app) {
    console.error("usage: verify-quicklook-bundle.mjs <EasyOffice.app> [--archs arm64,x86_64]");
    process.exit(2);
  }
  const at = process.argv.indexOf("--archs");
  const archs = at > 0 ? process.argv[at + 1].split(",") : [process.arch === "arm64" ? "arm64" : "x86_64"];
  const problems = verifyApp(app, archs);
  if (problems.length > 0) {
    for (const problem of problems) console.error(`[quicklook] ${problem}`);
    process.exit(1);
  }
  console.log(`[quicklook] ${EXTENSIONS.length} extensions in ${app} are in order (${archs.join(", ")})`);
}
