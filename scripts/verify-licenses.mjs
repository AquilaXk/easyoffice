#!/usr/bin/env node
/**
 * Automated dependency license policy verifier (PD-333).
 *
 * Enforces repository licensing rules across Cargo workspaces, npm lockfiles, vendored directories
 * (`vendor/`), and the generated WASM notice table in `THIRD_PARTY_NOTICES.md`:
 *  - Rejects copyleft licenses (GPL, LGPL, AGPL, etc.) unless an explicit permissive disjunct
 *    (`MIT OR ...` / `Apache-2.0 OR ...`) is offered and valid.
 *  - Allows `MPL-2.0` ONLY when linked unmodified from an official registry (`crates.io` or npm
 *    registry with integrity hash), kept out of core engine normal dependency closures, and
 *    documented in `THIRD_PARTY_NOTICES.md` with source URL and full MPL-2.0 license text (or as
 *    an unshipped `dev` build dependency in npm).
 *  - Fails closed on missing, empty, unknown, or malformed license metadata.
 *
 * Uses Node.js built-in modules only.
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const CRATES_IO_SOURCE = "registry+https://github.com/rust-lang/crates.io-index";
const DESKTOP_PACKAGE_NAME = "easyoffice-desktop";
const MPL_NOTICE_HEADING = "## Desktop shell MPL-2.0 crates (linked unmodified)";
const WASM_BEGIN = "<!-- BEGIN GENERATED WASM DEPENDENCY NOTICES -->";
const WASM_END = "<!-- END GENERATED WASM DEPENDENCY NOTICES -->";
const LICENSE_FILE_RE = /^(licen[cs]e|copying|unlicense)([-_.].*)?$/i;
const COPYLEFT_PREFIX_RE = /^(?:[AL]?GPL|GFDL|SSPL|EUPL|CPAL|OSL|CDDL|EPL|CC-BY-NC|CC-BY-SA|MPL-1\.)/i;
const MAX_SPDX_CHARS = 512;
const MAX_CARGO_METADATA_BYTES = 64 * 1024 * 1024;
const IGNORED_SCAN_DIRS = new Set([
  ".git",
  ".worktrees",
  "node_modules",
  "target",
  "third_party",
  "coverage",
  "dist",
  ".build",
  "release-build",
]);

export const PERMISSIVE_SPDX_IDS = new Set([
  "MIT",
  "MIT-0",
  "Apache-2.0",
  "BSD-1-Clause",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "0BSD",
  "ISC",
  "Zlib",
  "Unlicense",
  "CC0-1.0",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "BSL-1.0",
  "OFL-1.1",
]);

function tokenizeSpdx(rawExpr) {
  if (typeof rawExpr !== "string") {
    return { error: "missing license expression" };
  }
  const trimmed = rawExpr.trim();
  if (!trimmed) {
    return { error: "empty license expression" };
  }
  if (trimmed.length > MAX_SPDX_CHARS) {
    return { error: `license expression exceeds ${MAX_SPDX_CHARS} characters` };
  }
  if (/[^A-Za-z0-9.+()\s/-]/.test(trimmed)) {
    return { error: `invalid character in license expression '${trimmed}'` };
  }
  const normalized = trimmed.replaceAll("/", " OR ");
  const tokens = [];
  let i = 0;
  while (i < normalized.length) {
    const ch = normalized[i];
    if (/\s/.test(ch)) {
      i += 1;
      continue;
    }
    if (ch === "(" || ch === ")") {
      tokens.push({ type: ch, value: ch });
      i += 1;
      continue;
    }
    let end = i;
    while (end < normalized.length && !/[\s()]/.test(normalized[end])) {
      end += 1;
    }
    const word = normalized.slice(i, end);
    if (word === "OR" || word === "AND" || word === "WITH") {
      tokens.push({ type: word, value: word });
    } else if (/^[A-Za-z0-9][A-Za-z0-9.-]*\+?$/.test(word)) {
      tokens.push({ type: "ID", value: word });
    } else {
      return { error: `invalid SPDX token '${word}'` };
    }
    i = end;
  }
  if (tokens.length === 0) {
    return { error: "empty license expression" };
  }
  return { tokens };
}

function classifySingleLicense(baseId, exceptionId) {
  if (exceptionId !== null) {
    if (baseId === "Apache-2.0" && exceptionId === "LLVM-exception") {
      return { kind: "permissive" };
    }
    return { kind: "rejected", reason: `unsupported license exception '${baseId} WITH ${exceptionId}'` };
  }
  if (PERMISSIVE_SPDX_IDS.has(baseId)) {
    return { kind: "permissive" };
  }
  if (baseId === "MPL-2.0") {
    return { kind: "mpl" };
  }
  if (COPYLEFT_PREFIX_RE.test(baseId)) {
    return { kind: "rejected", reason: `copyleft license '${baseId}' is not permitted` };
  }
  return { kind: "rejected", reason: `unapproved or unknown license '${baseId}'` };
}

/**
 * Parses and classifies an SPDX 2.x expression (`MIT`, `Apache-2.0 WITH LLVM-exception`,
 * `MIT OR LGPL-2.1-or-later`, `MIT/Apache-2.0`, parenthesized groups) without guessing or silent
 * fallbacks.
 */
export function classifySpdx(rawExpr) {
  const tokenized = tokenizeSpdx(rawExpr);
  if (tokenized.error) {
    return { kind: "rejected", reason: tokenized.error };
  }
  const { tokens } = tokenized;
  let pos = 0;

  function peek() {
    return tokens[pos] ?? null;
  }

  function consume(expectedType) {
    const tok = tokens[pos];
    if (!tok || tok.type !== expectedType) return null;
    pos += 1;
    return tok;
  }

  function parseOr() {
    let left = parseAnd();
    while (consume("OR")) {
      const right = parseAnd();
      if (left.kind === "permissive" || right.kind === "permissive") {
        left = { kind: "permissive" };
      } else if (left.kind === "mpl" || right.kind === "mpl") {
        left = { kind: "mpl" };
      } else {
        left = { kind: "rejected", reason: left.reason || right.reason };
      }
    }
    return left;
  }

  function parseAnd() {
    let left = parseAtom();
    while (consume("AND")) {
      const right = parseAtom();
      if (left.kind === "rejected") {
        // preserve first rejection reason
      } else if (right.kind === "rejected") {
        left = right;
      } else if (left.kind === "mpl" || right.kind === "mpl") {
        left = { kind: "mpl" };
      } else {
        left = { kind: "permissive" };
      }
    }
    return left;
  }

  function parseAtom() {
    if (consume("(")) {
      const inner = parseOr();
      if (!consume(")")) {
        throw new Error("unclosed parenthesis in license expression");
      }
      return inner;
    }
    const base = consume("ID");
    if (!base) {
      const got = peek()?.value ?? "EOF";
      throw new Error(`expected SPDX identifier before '${got}'`);
    }
    let exc = null;
    if (consume("WITH")) {
      const excTok = consume("ID");
      if (!excTok) {
        throw new Error("expected exception identifier after 'WITH'");
      }
      exc = excTok.value;
    }
    return classifySingleLicense(base.value, exc);
  }

  try {
    const verdict = parseOr();
    if (pos !== tokens.length) {
      return { kind: "rejected", reason: `unexpected trailing token '${tokens[pos].value}'` };
    }
    return verdict;
  } catch (err) {
    return { kind: "rejected", reason: err.message };
  }
}

/**
 * Extracts documented unmodified MPL-2.0 packages and verifies full MPL-2.0 text presence from
 * `THIRD_PARTY_NOTICES.md`.
 */
export function parseMplNotices(noticesText) {
  const start = noticesText.indexOf(MPL_NOTICE_HEADING);
  if (start < 0) {
    return { missingSection: true, hasFullLicenseText: false, entries: new Map(), malformedRows: [] };
  }
  const rest = noticesText.slice(start + MPL_NOTICE_HEADING.length);
  const nextHeading = rest.search(/\n##\s+/);
  const section = nextHeading >= 0 ? rest.slice(0, nextHeading) : rest;

  const hasFullLicenseText =
    section.includes("Public License Version 2.0") &&
    section.includes("1. Definitions") &&
    section.includes("Covered Software") &&
    section.includes("Secondary License") &&
    section.includes("Exhibit A - Source Code Form License Notice");

  const entries = new Map();
  const malformedRows = [];
  for (const rawLine of section.split("\n")) {
    const line = rawLine.trim();
    if (!line.startsWith("|")) continue;
    const cols = line.slice(1, -1).split("|").map((c) => c.trim());
    if (cols.length < 5) continue;
    const name = cols[0].replaceAll("`", "").trim();
    const version = cols[1].trim();
    const holder = cols[2].trim();
    const source = cols[4].trim();
    if (!name || name === "Crate" || /^-+$/.test(name)) continue;
    const expectedUrl = `https://crates.io/crates/${name}/${version}`;
    if (!version || !holder || !source.includes(expectedUrl)) {
      malformedRows.push(`THIRD_PARTY_NOTICES.md MPL-2.0 row '${name}@${version}': missing copyright holder or crates.io URL '${expectedUrl}'`);
      continue;
    }
    entries.set(`${name}@${version}`, { name, version, holder, source });
  }
  return { missingSection: false, hasFullLicenseText, entries, malformedRows };
}

function normalDepClosure(metadata, rootIds) {
  const nodes = new Map((metadata.resolve?.nodes ?? []).map((n) => [n.id, n]));
  const seen = new Set();
  const queue = [...rootIds];
  while (queue.length > 0) {
    const cur = queue.pop();
    const node = nodes.get(cur);
    if (!node) continue;
    for (const dep of node.deps ?? []) {
      const isNormal = !dep.dep_kinds || dep.dep_kinds.some((k) => k.kind === null);
      if (!isNormal) continue;
      if (!seen.has(dep.pkg)) {
        seen.add(dep.pkg);
        queue.push(dep.pkg);
      }
    }
  }
  return seen;
}

/**
 * Verifies one Cargo workspace's resolved metadata against license policy and MPL-2.0 notice duties.
 */
export function verifyCargoMetadata(metadata, notices, { repoRoot, label, allowDesktopMpl }) {
  const errors = [];
  if (!metadata || !Array.isArray(metadata.packages)) {
    return [`${label}: malformed cargo metadata output`];
  }
  const resolvedRoot = path.resolve(repoRoot);
  const rootPrefix = `${resolvedRoot}${path.sep}`;
  const packages = metadata.packages;
  const byId = new Map(packages.map((p) => [p.id, p]));
  const nonDesktopRoots = (metadata.workspace_members ?? []).filter((id) => byId.get(id)?.name !== DESKTOP_PACKAGE_NAME);
  const nonDesktopDeps = normalDepClosure(metadata, nonDesktopRoots);
  const seenMpl = new Set();

  for (const pkg of packages) {
    const verdict = classifySpdx(pkg.license);
    const idLabel = `${pkg.name}@${pkg.version}`;
    if (verdict.kind === "rejected") {
      errors.push(`${label}: ${idLabel} has rejected license '${pkg.license ?? "NONE"}' (${verdict.reason})`);
      continue;
    }
    if (!pkg.source && verdict.kind !== "permissive") {
      errors.push(`${label}: local or path-sourced package ${idLabel} must use a permissive license (found '${pkg.license}')`);
      continue;
    }
    if (verdict.kind === "mpl") {
      if (pkg.source !== CRATES_IO_SOURCE) {
        errors.push(`${label}: MPL-2.0 crate ${idLabel} must come unmodified from crates.io (found source '${pkg.source}')`);
      }
      if (typeof pkg.manifest_path === "string" && pkg.manifest_path.length > 0) {
        const resolvedManifest = path.resolve(pkg.manifest_path);
        if (resolvedManifest === resolvedRoot || resolvedManifest.startsWith(rootPrefix)) {
          errors.push(`${label}: MPL-2.0 crate ${idLabel} resolves inside repository tree (${pkg.manifest_path}); bundled or modified MPL code is forbidden`);
        }
      }
      if (nonDesktopDeps.has(pkg.id)) {
        errors.push(`${label}: MPL-2.0 crate ${idLabel} is reachable via normal dependencies of non-desktop workspace crates`);
      }
      if (!allowDesktopMpl) {
        errors.push(`${label}: MPL-2.0 crate ${idLabel} is not permitted in this workspace`);
      } else {
        if (notices.missingSection || !notices.hasFullLicenseText) {
          errors.push(`${label}: THIRD_PARTY_NOTICES.md must include '${MPL_NOTICE_HEADING}' with full MPL-2.0 license text for ${idLabel}`);
        }
        if (!notices.entries.has(idLabel)) {
          errors.push(`${label}: unmodified MPL-2.0 crate ${idLabel} is missing from THIRD_PARTY_NOTICES.md`);
        }
        seenMpl.add(idLabel);
      }
    }
  }

  if (allowDesktopMpl) {
    for (const err of notices.malformedRows ?? []) {
      errors.push(`${label}: ${err}`);
    }
    for (const key of notices.entries.keys()) {
      if (!seenMpl.has(key)) {
        errors.push(`${label}: THIRD_PARTY_NOTICES.md lists MPL-2.0 crate '${key}' which is not resolved in Cargo metadata`);
      }
    }
  }
  return errors;
}

/**
 * Verifies every package entry in an npm `package-lock.json` file.
 */
export function verifyNpmLockfile(relPath, lockObj, notices) {
  const errors = [];
  if (!lockObj || typeof lockObj !== "object" || !lockObj.packages || typeof lockObj.packages !== "object") {
    return [`${relPath}: malformed package-lock.json ('packages' object missing)`];
  }
  for (const [pkgPath, info] of Object.entries(lockObj.packages)) {
    if (pkgPath === "") continue;
    const pkgName = pkgPath.split("node_modules/").pop() || pkgPath;
    const version = info?.version ?? "?";
    const idLabel = `${pkgName}@${version}`;
    const verdict = classifySpdx(info?.license);
    if (verdict.kind === "rejected") {
      errors.push(`${relPath}: ${idLabel} has rejected license '${info?.license ?? "NONE"}' (${verdict.reason})`);
      continue;
    }
    if (verdict.kind === "mpl") {
      const resolved = String(info?.resolved ?? "");
      if (info?.link === true || resolved.startsWith("file:") || !info?.integrity) {
        errors.push(`${relPath}: MPL-2.0 package ${idLabel} must be an unmodified registry archive with integrity hash`);
      }
      if (info?.dev !== true) {
        if (notices.missingSection || !notices.hasFullLicenseText || !notices.entries.has(idLabel)) {
          errors.push(`${relPath}: shipped MPL-2.0 npm package ${idLabel} is missing a notice entry in THIRD_PARTY_NOTICES.md`);
        }
      }
    }
  }
  return errors;
}

function parseCargoLicenseField(cargoText) {
  let section = "";
  let pkgLicense = null;
  let pkgUsesWorkspace = false;
  let wsLicense = null;
  for (const rawLine of cargoText.split(/\r?\n/)) {
    const line = rawLine.replace(/#.*$/, "").trim();
    if (!line) continue;
    const secMatch = /^\[([^\]]+)\]$/.exec(line);
    if (secMatch) {
      section = secMatch[1].trim();
      continue;
    }
    const kv = /^license\s*=\s*(.+)$/.exec(line);
    if (!kv) continue;
    const val = kv[1].trim();
    if (section === "package") {
      const quoted = /^"([^"]+)"$/.exec(val);
      if (quoted) pkgLicense = quoted[1];
      else if (/workspace\s*=\s*true/.test(val)) pkgUsesWorkspace = true;
    } else if (section === "workspace.package") {
      const quoted = /^"([^"]+)"$/.exec(val);
      if (quoted) wsLicense = quoted[1];
    }
  }
  if (pkgLicense) return pkgLicense;
  if (pkgUsesWorkspace) return wsLicense;
  return null;
}

/**
 * Verifies vendored packages under `vendor/` (permissive-only + license file) and the generated WASM
 * notice table in `THIRD_PARTY_NOTICES.md`.
 */
export function verifyVendoredAndWasm(repoRoot, noticesText) {
  const errors = [];
  const vendorRoot = path.join(repoRoot, "vendor");
  let vendorDirs = 0;
  if (fs.existsSync(vendorRoot)) {
    for (const entry of fs.readdirSync(vendorRoot, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      vendorDirs += 1;
      const dir = path.join(vendorRoot, entry.name);
      const pkgJson = path.join(dir, "package.json");
      const cargoToml = path.join(dir, "Cargo.toml");
      let license = null;
      if (fs.existsSync(pkgJson)) {
        try {
          license = JSON.parse(fs.readFileSync(pkgJson, "utf8")).license ?? null;
        } catch {
          errors.push(`vendor/${entry.name}/package.json: invalid JSON`);
          continue;
        }
      } else if (fs.existsSync(cargoToml)) {
        license = parseCargoLicenseField(fs.readFileSync(cargoToml, "utf8"));
      } else {
        errors.push(`vendor/${entry.name}: missing package.json or Cargo.toml manifest`);
        continue;
      }
      const verdict = classifySpdx(license);
      if (verdict.kind !== "permissive") {
        errors.push(`vendor/${entry.name}: vendored code must use a permissive license (found '${license ?? "NONE"}')`);
      }
      const hasLicenseFile = fs
        .readdirSync(dir, { withFileTypes: true })
        .some((f) => f.isFile() && LICENSE_FILE_RE.test(f.name));
      if (!hasLicenseFile) {
        errors.push(`vendor/${entry.name}: missing LICENSE/COPYING file`);
      }
    }
  }

  const begin = noticesText.indexOf(WASM_BEGIN);
  const end = noticesText.indexOf(WASM_END);
  let wasmRows = 0;
  if (begin < 0 || end < begin) {
    errors.push("THIRD_PARTY_NOTICES.md: generated WASM notice markers are missing or out of order");
  } else {
    const block = noticesText.slice(begin + WASM_BEGIN.length, end);
    for (const rawLine of block.split(/\r?\n/)) {
      const line = rawLine.trim();
      if (!line.startsWith("|")) continue;
      const cols = line.slice(1, -1).split("|").map((c) => c.trim());
      if (cols.length !== 3) continue;
      const [name, version, license] = cols;
      if (!name || name === "Crate" || /^-+$/.test(name)) continue;
      wasmRows += 1;
      const verdict = classifySpdx(license);
      if (verdict.kind !== "permissive") {
        errors.push(`THIRD_PARTY_NOTICES.md WASM table ${name}@${version}: expected permissive license, found '${license}'`);
      }
    }
    if (wasmRows === 0) {
      errors.push("THIRD_PARTY_NOTICES.md: generated WASM notice table contains zero crate entries");
    }
  }
  return { errors, vendorDirs, wasmRows };
}

function findNpmLockfiles(root) {
  const out = [];
  const stack = [root];
  while (stack.length > 0) {
    const dir = stack.pop();
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.isDirectory()) {
        if (!IGNORED_SCAN_DIRS.has(entry.name)) {
          stack.push(path.join(dir, entry.name));
        }
      } else if (entry.isFile() && entry.name === "package-lock.json") {
        out.push(path.relative(root, path.join(dir, entry.name)).replaceAll(path.sep, "/"));
      }
    }
  }
  return out.sort((a, b) => a.localeCompare(b));
}

function readCargoMetadata(repoRoot, manifestRel, cargoBin) {
  const raw = execFileSync(
    cargoBin,
    ["metadata", "--format-version", "1", "--locked", "--manifest-path", manifestRel],
    { cwd: repoRoot, maxBuffer: MAX_CARGO_METADATA_BYTES, encoding: "utf8" },
  );
  return JSON.parse(raw);
}

/**
 * Runs all dependency license checks across the repository at `repoRoot`.
 */
export function verifyRepositoryLicenses(repoRoot, { runCargo = true, cargoBin = "cargo" } = {}) {
  const errors = [];
  const noticesPath = path.join(repoRoot, "THIRD_PARTY_NOTICES.md");
  if (!fs.existsSync(noticesPath)) {
    return { errors: ["THIRD_PARTY_NOTICES.md is missing"], summary: null };
  }
  const noticesText = fs.readFileSync(noticesPath, "utf8");
  const notices = parseMplNotices(noticesText);

  const vendorAndWasm = verifyVendoredAndWasm(repoRoot, noticesText);
  errors.push(...vendorAndWasm.errors);

  const lockfiles = findNpmLockfiles(repoRoot);
  if (lockfiles.length === 0) {
    errors.push("no package-lock.json files found to inspect");
  }
  let npmPackages = 0;
  for (const relPath of lockfiles) {
    try {
      const lockObj = JSON.parse(fs.readFileSync(path.join(repoRoot, relPath), "utf8"));
      npmPackages += Math.max(0, Object.keys(lockObj.packages ?? {}).filter((k) => k !== "").length);
      errors.push(...verifyNpmLockfile(relPath, lockObj, notices));
    } catch (err) {
      errors.push(`${relPath}: failed to read/parse package-lock.json (${err.message})`);
    }
  }

  let cargoPackages = 0;
  let cargoWorkspaces = 0;
  if (runCargo) {
    const targets = [
      { manifest: "Cargo.toml", allowDesktopMpl: true },
      { manifest: "apps/quicklook/rust/Cargo.toml", allowDesktopMpl: false },
    ];
    for (const target of targets) {
      try {
        const meta = readCargoMetadata(repoRoot, target.manifest, cargoBin);
        cargoWorkspaces += 1;
        cargoPackages += meta.packages?.length ?? 0;
        errors.push(...verifyCargoMetadata(meta, notices, { repoRoot, label: target.manifest, allowDesktopMpl: target.allowDesktopMpl }));
      } catch (err) {
        errors.push(`${target.manifest}: cargo metadata failed (${err.message})`);
      }
    }
  }

  return {
    errors,
    summary: {
      cargoWorkspaces,
      cargoPackages,
      npmLockfiles: lockfiles.length,
      npmPackages,
      vendorDirs: vendorAndWasm.vendorDirs,
      wasmRows: vendorAndWasm.wasmRows,
      mplNotices: notices.entries.size,
    },
  };
}

const isEntrypoint = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isEntrypoint) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const { errors, summary } = verifyRepositoryLicenses(root, { runCargo: !process.argv.includes("--no-cargo") });
  if (errors.length > 0) {
    console.error(["Dependency license check failed:", ...errors.map((e) => `  - ${e}`)].join("\n"));
    process.exitCode = 1;
  } else {
    console.log(
      `Dependency license check passed (cargo_workspaces=${summary.cargoWorkspaces}, cargo_packages=${summary.cargoPackages}, npm_lockfiles=${summary.npmLockfiles}, npm_packages=${summary.npmPackages}, vendored=${summary.vendorDirs}, wasm_notices=${summary.wasmRows}, mpl_notices=${summary.mplNotices}).`,
    );
  }
}
