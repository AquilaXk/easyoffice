#!/usr/bin/env node
/**
 * Structure check of the built Linux packages (PD-086), run in CI and in the release after
 * `tauri build`. Nothing is installed and the app is never started:
 *   - AppImage: `--appimage-extract` unpacks it (the runtime's own option, the app does not run) and the
 *     extracted tree is inspected.
 *   - deb: control fields (architecture, runtime dependencies) and the unpacked payload.
 *   - rpm: architecture, requirements, the file list and the `.desktop` file's SHA-256 from the rpm's own
 *     file digests: the deb is built from the same template, so its already checked entry must hash the same.
 *     (The payload is not unpacked: `rpm2cpio` exits without a message on the Tauri-built rpm on the runner.)
 * Every package must carry the binary, a `.desktop` file and THIRD_PARTY_NOTICES.md (MIT notice condition);
 * the `.desktop` file must be in the Office menu category, pass the opened files to the app and list the
 * MIME type of every document format the app registers.
 *
 * Usage: node scripts/verify-linux-bundles.mjs <linux-x64|linux-arm64>   (run from the repository root)
 *
 * Reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim) of the reference app's desktop release
 * workflow (.github/workflows, "Verify Linux ABI baseline"): the AppImage is only extracted, never run.
 * Here the check is a script with a unit test, it also reads the deb and rpm metadata, and it checks the
 * desktop entry and the license notices. The glibc baseline scan of the reference is not taken over: both
 * runners build on Ubuntu 22.04, which fixes the baseline.
 */

import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  LINUX_DEB_DEPENDS,
  LINUX_DESKTOP_CATEGORY,
  LINUX_RPM_DEPENDS,
  NOTICES_FILE,
  REQUIRED_MIME_TYPES,
} from "./desktop-file-associations.mjs";
import { locateBundles } from "./release-assets.mjs";

const MAIN_BINARY = "usr/bin/EasyOffice";
const SHA256_HEX_LENGTH = 64;
const FILE_FIELD_CODE = /%[fFuU](\s|$)/;
const ARCHITECTURES = {
  "linux-x64": { deb: "amd64", rpm: "x86_64" },
  "linux-arm64": { deb: "arm64", rpm: "aarch64" },
};

/** `Key: value` fields of a Debian control file, continuation lines joined with a newline. */
export function parseControl(text) {
  const fields = new Map();
  let last = null;
  for (const line of text.split(/\r?\n/)) {
    if (/^[ \t]/.test(line) && last) {
      fields.set(last, `${fields.get(last)}\n${line.trim()}`);
    } else if (line.includes(":")) {
      const index = line.indexOf(":");
      last = line.slice(0, index).trim();
      fields.set(last, line.slice(index + 1).trim());
    }
  }
  return fields;
}

/** Package names a Depends line names, ignoring version constraints and alternatives. */
function dependencyNames(depends) {
  return depends
    .split(/[,|]/)
    .map((part) => part.trim().split(/[\s(]/)[0])
    .filter(Boolean);
}

export function checkDebControl(fields, architecture) {
  const problems = [];
  if (fields.get("Architecture") !== architecture) {
    problems.push(`Architecture is ${fields.get("Architecture")}, expected ${architecture}`);
  }
  const names = dependencyNames(fields.get("Depends") ?? "");
  for (const dependency of LINUX_DEB_DEPENDS) {
    if (!names.includes(dependency)) problems.push(`Depends must list ${dependency}`);
  }
  return problems;
}

export function checkRpmPackage({ arch, requires }, architecture) {
  const problems = [];
  if (arch !== architecture) problems.push(`architecture is ${arch}, expected ${architecture}`);
  const required = requires.split(/\r?\n/).map((line) => line.trim().split(/\s/)[0]);
  for (const dependency of LINUX_RPM_DEPENDS) {
    if (!required.includes(dependency)) problems.push(`Requires must list ${dependency}`);
  }
  return problems;
}

export function checkDesktopEntry(text) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === "[Desktop Entry]");
  if (start < 0) return ["the .desktop file has no [Desktop Entry] section"];
  const entry = new Map();
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith("[")) break;
    const index = line.indexOf("=");
    if (index > 0) entry.set(line.slice(0, index), line.slice(index + 1));
  }
  const problems = [];
  if (entry.get("Type") !== "Application") problems.push("Type must be Application");
  if (!FILE_FIELD_CODE.test(entry.get("Exec") ?? "")) {
    problems.push("Exec must pass the opened files to the app (%U, %u, %F or %f)");
  }
  if (!(entry.get("Categories") ?? "").split(";").includes(LINUX_DESKTOP_CATEGORY)) {
    problems.push(`Categories must include ${LINUX_DESKTOP_CATEGORY}`);
  }
  const mimeTypes = (entry.get("MimeType") ?? "").split(";");
  for (const mime of REQUIRED_MIME_TYPES.values()) {
    if (!mimeTypes.includes(mime)) problems.push(`MimeType must list ${mime}`);
  }
  return problems;
}

/** Entries of `rpm --dump`: `path size mtime digest mode owner group isconfig isdoc rdev symlink` per line. */
export function parseRpmDump(text) {
  return text
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => {
      const [entryPath, , , digest] = line.split(" ");
      return { path: entryPath, digest };
    });
}

/** The rpm's `.desktop` must be byte for byte the deb's (same template), judged by the rpm's SHA-256 file digest. */
export function checkRpmDesktopMatchesDeb(entries, debDesktopText) {
  const entry = entries.find((candidate) => candidate.path.endsWith(".desktop"));
  if (!entry) return []; // checkPayload reports the missing file
  if (debDesktopText === undefined) return ["there is no checked deb .desktop file to compare the rpm's with"];
  if (entry.digest?.length !== SHA256_HEX_LENGTH) {
    return [`the rpm file digest of ${entry.path} is not SHA-256, so it cannot be compared with the deb's .desktop file`];
  }
  const expected = crypto.createHash("sha256").update(debDesktopText).digest("hex");
  return entry.digest === expected ? [] : [`${entry.path} differs from the deb's .desktop file`];
}

/** Paths (any leading `./` or `/`) a package unpacks to. */
export function checkPayload(paths) {
  const files = paths.map((entry) => entry.replace(/^\.?\//, ""));
  const problems = [];
  if (!files.includes(MAIN_BINARY)) problems.push(`the package must contain ${MAIN_BINARY}`);
  if (!files.some((file) => file.endsWith(`/${NOTICES_FILE}`))) problems.push(`the package must contain ${NOTICES_FILE}`);
  if (!files.some((file) => file.endsWith(".desktop"))) problems.push("the package must contain a .desktop file");
  return problems;
}

function walk(root, dir = root) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(root, full) : [path.relative(root, full)];
  });
}

const run = (command, args, options = {}) => execFileSync(command, args, { encoding: "utf8", ...options });

function desktopFile(root, files) {
  const relative = files.find((file) => file.endsWith(".desktop"));
  return relative ? fs.readFileSync(path.join(root, relative), "utf8") : undefined;
}

function desktopProblems(root, files) {
  const text = desktopFile(root, files);
  return text === undefined ? [] : checkDesktopEntry(text);
}

function inspectDeb(file, scratch, arch, context) {
  const root = path.join(scratch, "deb");
  fs.mkdirSync(root);
  run("dpkg-deb", ["--extract", file, root]);
  const files = walk(root);
  context.debDesktop = desktopFile(root, files);
  return [
    ...checkDebControl(parseControl(run("dpkg-deb", ["--field", file])), arch.deb),
    ...checkPayload(files),
    ...desktopProblems(root, files),
  ];
}

function inspectRpm(file, arch, context) {
  const query = (...args) => run("rpm", ["--query", "--package", ...args, file]);
  const entries = parseRpmDump(query("--dump"));
  return [
    ...checkRpmPackage({ arch: query("--queryformat", "%{ARCH}").trim(), requires: query("--requires") }, arch.rpm),
    ...checkPayload(entries.map((entry) => entry.path)),
    ...checkRpmDesktopMatchesDeb(entries, context.debDesktop),
  ];
}

function inspectAppImage(file, scratch) {
  const root = path.join(scratch, "appimage");
  fs.mkdirSync(root);
  fs.chmodSync(file, 0o755);
  run(path.resolve(file), ["--appimage-extract"], { cwd: root, stdio: "pipe" });
  const extracted = path.join(root, "squashfs-root");
  const files = walk(extracted);
  return [...checkPayload(files), ...desktopProblems(extracted, files)];
}

const INSPECTORS = {
  deb: inspectDeb,
  rpm: (file, _scratch, arch, context) => inspectRpm(file, arch, context),
  appimage: (file, scratch) => inspectAppImage(file, scratch),
};

// The deb comes before the rpm: the rpm's .desktop file is compared with the deb's.
const INSPECTION_ORDER = ["appimage", "deb", "rpm"];

/** Inspects the bundles of one platform under `root`; returns one message per problem. */
export function verifyBundles(platformId, root) {
  const arch = ARCHITECTURES[platformId];
  if (!arch) throw new Error(`unknown Linux platform '${platformId}' (expected ${Object.keys(ARCHITECTURES).join(" or ")})`);
  const scratch = fs.mkdtempSync(path.join(process.env.RUNNER_TEMP ?? os.tmpdir(), "linux-bundles-"));
  try {
    const context = {};
    const bundles = locateBundles(platformId, root).sort(
      (a, b) => INSPECTION_ORDER.indexOf(a.asset.dir) - INSPECTION_ORDER.indexOf(b.asset.dir)
    );
    return bundles.flatMap(({ asset, file }) => {
      const problems = INSPECTORS[asset.dir](file, scratch, arch, context);
      console.log(`${asset.dir} ${path.basename(file)}: ${problems.length ? `${problems.length} problem(s)` : "ok"}`);
      return problems.map((problem) => `${path.basename(file)}: ${problem}`);
    });
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const problems = verifyBundles(process.argv[2] ?? "", process.cwd());
    for (const problem of problems) console.error(`::error::${problem}`);
    process.exit(problems.length ? 1 : 0);
  } catch (err) {
    console.error(`::error::${err.message}`);
    process.exit(1);
  }
}
