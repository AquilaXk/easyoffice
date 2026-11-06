#!/usr/bin/env node
/**
 * Static checks for the built hwp-studio bundle (PD-081).
 *
 * Fails closed on: external URLs, eval / new Function, Tauri references, service worker or PWA
 * leftovers, inline scripts / styles / handlers in HTML, a wasm that differs from the vendored one,
 * files that are not listed in asset-manifest.json (or differ from it), and a size over budget.
 * Usage: node scripts/check-hwp-studio-bundle.mjs [bundleDir]
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
export const DEFAULT_BUNDLE_DIR = path.join(REPO, "crates/easyoffice-app/ui/hwp-studio");
export const ASSET_MANIFEST = "asset-manifest.json";
/** Size budget for everything except fonts/ (the bundled fonts are accounted separately). */
export const BUDGET_BYTES = 20 * 1024 * 1024;

const TEXT_EXT = new Set([".html", ".js", ".css", ".json", ".txt"]);
// Namespace identifiers (not network locations) and one placeholder shown in an input field.
const ALLOWED_URLS = [/^http:\/\/www\.w3\.org\//, /^https:\/\/example\.com$/];
const URL_RE = /https?:\/\/[A-Za-z0-9][^\s"'`)\\<>]*/g;
const FORBIDDEN_PATTERNS = [
  [/(^|[^\w$.])eval\s*\(/, "eval()"],
  [/\bnew\s+Function\b/, "new Function"],
  [/__TAURI/, "__TAURI reference"],
  [/serviceWorker/, "service worker reference"],
  [/importScripts\s*\(/, "importScripts"],
  [/chrome\.runtime|chrome\.storage|chrome\.tabs/, "browser extension API"],
];
const FORBIDDEN_FILES = [/^sw\.js$/, /manifest\.webmanifest$/, /workbox/i, /canvaskit.*\.wasm$/i, /\.map$/];

export function sha256(buffer) {
  return crypto.createHash("sha256").update(buffer).digest("hex");
}

export function listFiles(dir) {
  const files = [];
  const walk = (current) => {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) walk(full);
      else files.push(path.relative(dir, full).split(path.sep).join("/"));
    }
  };
  walk(dir);
  return files.sort((a, b) => a.localeCompare(b));
}

function scanText(rel, text, errors) {
  for (const [re, label] of FORBIDDEN_PATTERNS) {
    if (re.test(text)) errors.push(`${rel}: contains ${label}`);
  }
  const bad = new Set();
  for (const url of text.match(URL_RE) ?? []) {
    if (!ALLOWED_URLS.some((allowed) => allowed.test(url))) bad.add(url);
  }
  for (const url of [...bad].sort((a, b) => a.localeCompare(b)).slice(0, 5)) errors.push(`${rel}: external URL ${url}`);
}

function scanHtml(rel, text, errors) {
  for (const m of text.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)) {
    if (!/\bsrc\s*=/.test(m[1]) || m[2].trim() !== "") errors.push(`${rel}: inline script`);
    const src = /\bsrc\s*=\s*["']([^"']+)["']/.exec(m[1])?.[1];
    if (src && /^(https?:)?\/\//i.test(src)) errors.push(`${rel}: external script ${src}`);
  }
  if (/<style\b/i.test(text)) errors.push(`${rel}: inline <style> element`);
  if (/\sstyle\s*=\s*["']/i.test(text)) errors.push(`${rel}: inline style attribute`);
  if (/\son[a-z]+\s*=\s*["']/i.test(text)) errors.push(`${rel}: inline event handler`);
  for (const m of text.matchAll(/<(?:link|img|script|iframe)\b[^>]*\b(?:href|src)\s*=\s*["']([^"']+)["']/gi)) {
    if (/^(https?:)?\/\//i.test(m[1])) errors.push(`${rel}: external reference ${m[1]}`);
  }
}

function checkReferencedAssets(dir, files, errors) {
  const html = fs.readFileSync(path.join(dir, "index.html"), "utf8");
  for (const m of html.matchAll(/\b(?:href|src)\s*=\s*["']([^"']+)["']/gi)) {
    const ref = m[1];
    if (/^(data:|#|about:)/.test(ref)) continue;
    const rel = path.posix.normalize(ref.replace(/^\.\//, "").split(/[?#]/)[0]);
    if (rel.startsWith("..") || !files.includes(rel)) errors.push(`index.html references missing file ${ref}`);
  }
}

function checkManifest(dir, files, errors) {
  const file = path.join(dir, ASSET_MANIFEST);
  if (!fs.existsSync(file)) return;
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(file, "utf8"));
  } catch (err) {
    errors.push(`${ASSET_MANIFEST} cannot be parsed: ${err.message}`);
    return;
  }
  const listed = new Map((manifest.files ?? []).map((entry) => [entry.path, entry]));
  for (const rel of files.filter((name) => name !== ASSET_MANIFEST)) {
    const entry = listed.get(rel);
    if (!entry) errors.push(`${rel} is not listed in ${ASSET_MANIFEST}`);
    else if (sha256(fs.readFileSync(path.join(dir, rel))) !== entry.sha256) errors.push(`${rel} differs from ${ASSET_MANIFEST}`);
  }
  for (const rel of listed.keys()) {
    if (!files.includes(rel)) errors.push(`${ASSET_MANIFEST} lists missing file ${rel}`);
  }
}

function checkWasm(dir, files, vendorWasm, errors) {
  const wasmFiles = files.filter((name) => name.endsWith(".wasm"));
  if (wasmFiles.length !== 1) {
    errors.push(`expected exactly one .wasm file, found ${wasmFiles.length}`);
    return;
  }
  if (vendorWasm && fs.existsSync(vendorWasm) && sha256(fs.readFileSync(path.join(dir, wasmFiles[0]))) !== sha256(fs.readFileSync(vendorWasm))) {
    errors.push(`${wasmFiles[0]} differs from vendor/rhwp-core/rhwp_bg.wasm`);
  }
}

/** Returns { errors, stats } for a built bundle directory. */
export function checkBundle(dir, { vendorWasm = path.join(REPO, "vendor/rhwp-core/rhwp_bg.wasm"), budget = BUDGET_BYTES } = {}) {
  const errors = [];
  if (!fs.existsSync(path.join(dir, "index.html"))) {
    return { errors: [`${dir}: index.html is missing (build first)`], stats: null };
  }
  const files = listFiles(dir);
  let nonFontBytes = 0;
  let fontBytes = 0;
  for (const rel of files) {
    const full = path.join(dir, rel);
    const size = fs.statSync(full).size;
    if (rel.startsWith("fonts/")) fontBytes += size;
    else nonFontBytes += size;
    if (FORBIDDEN_FILES.some((re) => re.test(path.posix.basename(rel)))) errors.push(`${rel}: forbidden file`);
    if (TEXT_EXT.has(path.extname(rel)) && rel !== ASSET_MANIFEST && !rel.startsWith("fonts/")) {
      const text = fs.readFileSync(full, "utf8");
      scanText(rel, text, errors);
      if (rel.endsWith(".html")) scanHtml(rel, text, errors);
    }
  }
  checkReferencedAssets(dir, files, errors);
  checkManifest(dir, files, errors);
  checkWasm(dir, files, vendorWasm, errors);
  if (nonFontBytes > budget) errors.push(`bundle is ${nonFontBytes} bytes without fonts, budget ${budget}`);
  return { errors, stats: { files: files.length, nonFontBytes, fontBytes } };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dir = path.resolve(process.argv[2] ?? DEFAULT_BUNDLE_DIR);
  const { errors, stats } = checkBundle(dir);
  if (stats) {
    const mb = (n) => (n / 1048576).toFixed(2);
    console.log(`hwp-studio bundle: ${stats.files} files, ${mb(stats.nonFontBytes)} MiB without fonts (budget ${mb(BUDGET_BYTES)} MiB), fonts ${mb(stats.fontBytes)} MiB`);
  }
  if (errors.length > 0) {
    console.error("hwp-studio bundle check failed:");
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }
  console.log("hwp-studio bundle check passed.");
}
