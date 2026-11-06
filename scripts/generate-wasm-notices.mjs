#!/usr/bin/env node
/**
 * Maintainer tool (offline, PD-081): regenerates the "Rust dependencies compiled into the rhwp
 * WebAssembly module" section of THIRD_PARTY_NOTICES.md.
 *
 *   cargo metadata --format-version 1 --filter-platform wasm32-unknown-unknown --offline \
 *     --manifest-path third_party/rhwp/Cargo.toml > metadata.json     (toolchain 1.93.1)
 *   node scripts/generate-wasm-notices.mjs metadata.json
 *
 * Only normal (non-dev, non-build) dependency edges reachable from the rhwp library are listed, which is
 * the set linked into rhwp_bg.wasm. License and NOTICE texts are read from each crate's source
 * directory; crates are grouped by identical license text so each distinct text appears once.
 * The generated block sits between the BEGIN/END markers; nothing else in the file is touched.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const BEGIN = "<!-- BEGIN GENERATED WASM DEPENDENCY NOTICES -->";
export const END = "<!-- END GENERATED WASM DEPENDENCY NOTICES -->";
const LICENSE_FILE_RE = /^(licen[cs]e|copying|unlicense)([-_.].*)?$/i;
const NOTICE_FILE_RE = /^notice([-_.].*)?$/i;

export function wasmDependencyClosure(metadata, rootName = "rhwp") {
  const byId = new Map(metadata.packages.map((p) => [p.id, p]));
  const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
  const root = metadata.packages.find((p) => p.name === rootName && !p.source);
  if (!root) throw new Error(`root package ${rootName} not found`);
  const seen = new Set([root.id]);
  const queue = [root.id];
  while (queue.length > 0) {
    const node = nodes.get(queue.pop());
    for (const dep of node?.deps ?? []) {
      if (!dep.dep_kinds.some((k) => k.kind === null)) continue; // normal edges only
      if (!seen.has(dep.pkg)) {
        seen.add(dep.pkg);
        queue.push(dep.pkg);
      }
    }
  }
  return [...seen].map((id) => byId.get(id)).sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

function readTexts(pkg, pattern) {
  const dir = path.dirname(pkg.manifest_path);
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && pattern.test(e.name))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((e) => fs.readFileSync(path.join(dir, e.name), "utf8").replaceAll("\r\n", "\n").trim());
}

export function renderSection(packages) {
  const external = packages.filter((p) => p.source);
  const groups = new Map();
  const notices = [];
  const lines = [];
  for (const pkg of external) {
    const texts = readTexts(pkg, LICENSE_FILE_RE);
    if (texts.length === 0) throw new Error(`${pkg.name} ${pkg.version}: no license file in its source (license: ${pkg.license})`);
    for (const text of texts) {
      const key = crypto.createHash("sha256").update(text).digest("hex");
      if (!groups.has(key)) groups.set(key, { text, crates: [] });
      const crates = groups.get(key).crates;
      const label = `${pkg.name} ${pkg.version}`;
      if (!crates.includes(label)) crates.push(label);
    }
    for (const text of readTexts(pkg, NOTICE_FILE_RE)) notices.push({ label: `${pkg.name} ${pkg.version}`, text });
  }
  lines.push(`${external.length} third-party crates are linked into the module (normal dependencies of rhwp for wasm32-unknown-unknown;`);
  lines.push("build-time and test-only crates are not shipped).", "");
  lines.push("| Crate | Version | License |", "|---|---|---|");
  for (const pkg of external) lines.push(`| ${pkg.name} | ${pkg.version} | ${pkg.license ?? "see license text"} |`);
  lines.push("", "### License texts", "");
  let index = 1;
  for (const { text, crates } of [...groups.values()].sort((a, b) => a.crates[0].localeCompare(b.crates[0]))) {
    lines.push(`#### Text ${index++}`, "", `Applies to: ${crates.join(", ")}`, "", "```", text, "```", "");
  }
  if (notices.length > 0) {
    lines.push("### NOTICE files (Apache-2.0, reproduced verbatim)", "");
    for (const { label, text } of notices) lines.push(`#### ${label}`, "", "```", text, "```", "");
  }
  return lines.join("\n");
}

export function replaceSection(document, section) {
  const start = document.indexOf(BEGIN);
  const end = document.indexOf(END);
  if (start < 0 || end < start) throw new Error("generated-section markers not found in THIRD_PARTY_NOTICES.md");
  return `${document.slice(0, start + BEGIN.length)}\n\n${section}\n${document.slice(end)}`;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const metadataFile = process.argv[2];
  if (!metadataFile) throw new Error("usage: generate-wasm-notices.mjs <cargo-metadata.json>");
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const metadata = JSON.parse(fs.readFileSync(metadataFile, "utf8"));
  const section = renderSection(wasmDependencyClosure(metadata));
  const target = path.join(root, "THIRD_PARTY_NOTICES.md");
  const text = fs.readFileSync(target, "utf8");
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const updated = replaceSection(text.replaceAll("\r\n", "\n"), section);
  fs.writeFileSync(target, eol === "\n" ? updated : updated.replaceAll("\n", eol));
  console.log("THIRD_PARTY_NOTICES.md updated");
}
