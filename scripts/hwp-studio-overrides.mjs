/**
 * Override manifest for the hwp-studio overlay (PD-081).
 *
 * config/rhwp-studio-overrides.json lists every overlay file that replaces or extends an upstream
 * rhwp-studio file (hop's studio-host pattern). This module validates the manifest, keeps it in
 * sync with apps/hwp-studio/src, and pins the upstream baseline: each counterpart records the
 * SHA-256 of the upstream file the override was written against (line endings normalised), so an
 * rhwp bump that changes a forked file fails until the override is reviewed.
 * Node built-in modules only.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

export const MANIFEST_REL = "config/rhwp-studio-overrides.json";
export const OVERLAY_REL = "apps/hwp-studio";
export const UPSTREAM_STUDIO_REL = "third_party/rhwp/rhwp-studio";
const STRATEGIES = new Set(["extension", "fork", "contribution"]);
const SHA256_RE = /^[0-9a-f]{64}$/;
/** Overlay source files that are not upstream overrides. */
const OVERLAY_NON_OVERRIDE = new Set([
  "src/eo-overrides.css",
  "src/style.css",
  "src/vite-env.d.ts",
  // EasyOffice-only module (no upstream counterpart): shell shortcut forwarding over the embed port.
  "src/embed/shell-shortcuts.ts",
  // EasyOffice-only module: the 파일 menu as shell commands, shell theme and recent documents.
  "src/embed/shell-menu.ts",
  // EasyOffice-only module: page batches for the shell's print path (getPrintPages RPC).
  "src/embed/print-batch.ts",
  // EasyOffice-only module (hop port): the 보기 menu toggle for the toolbar labels.
  "src/ui/toolbar-labels.ts",
]);

export function normalizedSha256(file) {
  const text = fs.readFileSync(file, "utf8").replaceAll("\r\n", "\n");
  return crypto.createHash("sha256").update(text).digest("hex");
}

export function readManifest(root) {
  return JSON.parse(fs.readFileSync(path.join(root, MANIFEST_REL), "utf8"));
}

function isSafeRelative(value) {
  if (typeof value !== "string" || value.length === 0) return false;
  const segments = value.split(/[\\/]+/);
  return !path.isAbsolute(value) && !/^[A-Za-z]:/.test(value) && !segments.includes("..");
}

function checkEntries(manifest) {
  const errors = [];
  const seen = new Set();
  const lists = [
    ["overrides", "id"],
    ["assetOverrides", "path"],
  ];
  for (const [listName, key] of lists) {
    if (!Array.isArray(manifest[listName])) {
      errors.push(`${MANIFEST_REL}: ${listName} must be an array`);
      continue;
    }
    for (const entry of manifest[listName]) {
      const id = entry?.[key];
      if (!isSafeRelative(id) || seen.has(`${listName}:${id}`)) errors.push(`${MANIFEST_REL}: invalid or duplicate ${listName} ${key} '${id}'`);
      seen.add(`${listName}:${id}`);
      if (!STRATEGIES.has(entry?.strategy)) errors.push(`${MANIFEST_REL}: ${listName} '${id}' has an invalid strategy`);
      if (typeof entry?.reason !== "string" || entry.reason.trim() === "") errors.push(`${MANIFEST_REL}: ${listName} '${id}' needs a reason`);
    }
  }
  return errors;
}

function listOverlaySources(root) {
  const base = path.join(root, OVERLAY_REL, "src");
  const found = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else found.push(path.relative(path.join(root, OVERLAY_REL), full).split(path.sep).join("/"));
    }
  };
  if (fs.existsSync(base)) walk(base);
  return found;
}

/** Structural checks: every override has an overlay file and every overlay source is listed. */
export function checkManifestStructure(root, manifest) {
  const errors = [];
  if (manifest.schemaVersion !== 1) errors.push(`${MANIFEST_REL}: schemaVersion must be 1`);
  errors.push(...checkEntries(manifest));
  if (errors.length > 0) return errors;
  const listed = new Set();
  for (const { id } of manifest.overrides) {
    const rel = `src/${id}.ts`;
    listed.add(rel);
    if (!fs.existsSync(path.join(root, OVERLAY_REL, rel))) errors.push(`${MANIFEST_REL}: override '${id}' has no overlay file ${OVERLAY_REL}/${rel}`);
  }
  for (const { path: assetPath } of manifest.assetOverrides) {
    if (!fs.existsSync(path.join(root, OVERLAY_REL, assetPath))) {
      errors.push(`${MANIFEST_REL}: asset override '${assetPath}' has no overlay file`);
    }
  }
  for (const rel of listOverlaySources(root)) {
    if (!listed.has(rel) && !OVERLAY_NON_OVERRIDE.has(rel)) errors.push(`${OVERLAY_REL}/${rel} is not listed in ${MANIFEST_REL}`);
  }
  return errors;
}

function upstreamPathFor(kind, id) {
  return path.join(UPSTREAM_STUDIO_REL, kind === "overrides" ? `src/${id}.ts` : id);
}

function checkUpstreamTables(manifest, config) {
  const errors = [];
  const upstream = manifest.upstream ?? {};
  if (upstream.commit !== config.commit || upstream.version !== config.version) {
    errors.push(`${MANIFEST_REL}: upstream commit/version differ from config/rhwp-upstream.json`);
  }
  const counterparts = upstream.counterparts ?? {};
  const assets = upstream.assets ?? {};
  errors.push(...checkUpstreamTableEntries(counterparts, assets), ...checkUpstreamCoverage(manifest, counterparts, assets));
  return errors;
}

function checkUpstreamTableEntries(counterparts, assets) {
  const errors = [];
  for (const [name, table] of [
    ["counterparts", counterparts],
    ["assets", assets],
  ]) {
    for (const [id, sha] of Object.entries(table)) {
      if (!SHA256_RE.test(String(sha))) errors.push(`${MANIFEST_REL}: upstream.${name}['${id}'] must be a SHA-256`);
      if (!isSafeRelative(id)) errors.push(`${MANIFEST_REL}: upstream.${name} has an unsafe path '${id}'`);
    }
  }
  return errors;
}

function checkUpstreamCoverage(manifest, counterparts, assets) {
  const errors = [];
  for (const { id } of manifest.overrides) {
    if (!(id in counterparts)) errors.push(`${MANIFEST_REL}: upstream.counterparts is missing '${id}'`);
  }
  for (const { path: assetPath, upstream: source } of manifest.assetOverrides) {
    if (!(source in assets)) errors.push(`${MANIFEST_REL}: upstream.assets is missing '${source}' (for ${assetPath})`);
  }
  for (const source of manifest.verbatimAssets ?? []) {
    if (!(source in assets)) errors.push(`${MANIFEST_REL}: upstream.assets is missing verbatim asset '${source}'`);
  }
  return errors;
}

function checkUpstreamFiles(root, counterparts, assets) {
  const errors = [];
  const checks = [
    ...Object.entries(counterparts).map(([id, sha]) => [upstreamPathFor("overrides", id), sha]),
    ...Object.entries(assets).map(([id, sha]) => [upstreamPathFor("assets", id), sha]),
  ];
  for (const [rel, sha] of checks) {
    const file = path.join(root, rel);
    const shown = rel.split(path.sep).join("/");
    if (!fs.existsSync(file)) errors.push(`upstream file ${shown} is missing`);
    else if (normalizedSha256(file) !== sha) {
      errors.push(`upstream ${shown} changed since its override was written (review ${MANIFEST_REL})`);
    }
  }
  return errors;
}

/** Baseline checks against the sparse upstream checkout (skipped when it is not present). */
export function checkUpstreamBaseline(root, manifest, config) {
  const errors = checkUpstreamTables(manifest, config);
  if (errors.length > 0) return errors;
  if (!fs.existsSync(path.join(root, config.path, ".git"))) return errors;
  const upstream = manifest.upstream ?? {};
  return checkUpstreamFiles(root, upstream.counterparts ?? {}, upstream.assets ?? {});
}

export function verifyOverrides(root, config) {
  let manifest;
  try {
    manifest = readManifest(root);
  } catch (err) {
    return [`${MANIFEST_REL} cannot be read: ${err.message}`];
  }
  const errors = checkManifestStructure(root, manifest);
  if (errors.length > 0) return errors;
  return checkUpstreamBaseline(root, manifest, config);
}
