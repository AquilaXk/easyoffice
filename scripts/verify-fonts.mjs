#!/usr/bin/env node
/**
 * Bundled font guard (PD-081): assets/fonts holds only OFL 1.1 fonts.
 *
 * Fails closed when a font file is not listed in assets/fonts/fonts.json, a listed file is missing or
 * differs in size or SHA-256, an entry is not OFL-1.1, its license text is missing or is not the
 * SIL Open Font License 1.1, or FONTS.md does not mention a bundled file.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const FONTS_DIR_REL = "assets/fonts";
const OFL_RE = /SIL\s+OPEN\s+FONT\s+LICENSE[\s,]+Version\s+1\.1/i;
const SAFE_NAME_RE = /^[A-Za-z0-9_.-]+$/;
const SHA256_RE = /^[0-9a-f]{64}$/;
// A license that reserves a font name (OFL 1.1 section 3) means a Modified Version may not keep it, so the
// bundled file must be the official file or that file with WOFF2 compression only (OFL FAQ 2.2.1, PD-092).
const RFN_RE = /with\s+Reserved\s+Font\s+Names?\s+\S/i;
const CONVERSIONS = new Set(["none", "woff2-compression"]);

function checkReservedNameProvenance(name, font) {
  if (typeof font.reservedNameNotUsed === "string" && font.reservedNameNotUsed.trim() !== "") return [];
  const up = font.upstream;
  if (!up || typeof up !== "object") {
    return [`${name}: the license reserves a font name, so upstream (official source URL, SHA-256, conversion) is required`];
  }
  const errors = [];
  if (typeof up.url !== "string" || !up.url.startsWith("https://")) errors.push(`${name}: upstream.url must be an https URL`);
  if (!SHA256_RE.test(String(up.sha256))) errors.push(`${name}: upstream.sha256 is required`);
  if (!CONVERSIONS.has(up.conversion)) errors.push(`${name}: upstream.conversion must be "none" or "woff2-compression"`);
  else if (up.conversion === "none" && up.sha256 !== font.sha256) {
    errors.push(`${name}: upstream.conversion is "none" but the bundled SHA-256 differs from the official file`);
  }
  return errors;
}

export function checkFonts(root) {
  const dir = path.join(root, FONTS_DIR_REL);
  const errors = [];
  const manifestFile = path.join(dir, "fonts.json");
  if (!fs.existsSync(manifestFile)) return [`${FONTS_DIR_REL}/fonts.json is missing`];
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestFile, "utf8"));
  } catch (err) {
    return [`${FONTS_DIR_REL}/fonts.json cannot be parsed: ${err.message}`];
  }
  if (manifest.schemaVersion !== 1 || !Array.isArray(manifest.fonts) || manifest.fonts.length === 0) {
    return [`${FONTS_DIR_REL}/fonts.json: schemaVersion 1 with a non-empty fonts array is required`];
  }
  const doc = fs.existsSync(path.join(dir, "FONTS.md")) ? fs.readFileSync(path.join(dir, "FONTS.md"), "utf8") : null;
  if (doc === null) errors.push(`${FONTS_DIR_REL}/FONTS.md is missing`);
  const listed = new Set();
  for (const font of manifest.fonts) {
    const name = font?.file;
    if (typeof name !== "string" || !SAFE_NAME_RE.test(name) || !name.endsWith(".woff2")) {
      errors.push(`${FONTS_DIR_REL}/fonts.json: invalid font file name '${name}'`);
      continue;
    }
    listed.add(name);
    if (font.license !== "OFL-1.1") errors.push(`${name}: license must be OFL-1.1 (found '${font.license}')`);
    if (!SHA256_RE.test(String(font.sha256)) || !Number.isInteger(font.bytes)) errors.push(`${name}: bytes and sha256 are required`);
    const file = path.join(dir, name);
    if (!fs.existsSync(file)) errors.push(`${FONTS_DIR_REL}/${name} is missing`);
    else {
      const data = fs.readFileSync(file);
      if (data.length !== font.bytes) errors.push(`${FONTS_DIR_REL}/${name} is ${data.length} bytes, fonts.json records ${font.bytes}`);
      else if (crypto.createHash("sha256").update(data).digest("hex") !== font.sha256) errors.push(`${FONTS_DIR_REL}/${name} SHA-256 differs from fonts.json`);
    }
    const licenseRel = String(font.licenseFile ?? "");
    const licensePath = path.resolve(dir, licenseRel);
    if (!licenseRel.startsWith("licenses/") || !licensePath.startsWith(path.join(dir, "licenses") + path.sep)) {
      errors.push(`${name}: licenseFile must be under licenses/`);
    } else if (!fs.existsSync(licensePath)) errors.push(`${name}: license file ${licenseRel} is missing`);
    else {
      const licenseText = fs.readFileSync(licensePath, "utf8");
      if (!OFL_RE.test(licenseText)) errors.push(`${name}: ${licenseRel} is not the SIL Open Font License 1.1`);
      else if (RFN_RE.test(licenseText)) errors.push(...checkReservedNameProvenance(name, font));
    }
    if (doc !== null && !doc.includes(name)) errors.push(`${FONTS_DIR_REL}/FONTS.md does not list ${name}`);
  }
  for (const name of fs.readdirSync(dir)) {
    const isFont = /\.(woff2?|ttf|otf|ttc|eot)$/i.test(name);
    if (isFont && !listed.has(name)) errors.push(`${FONTS_DIR_REL}/${name} is not listed in fonts.json`);
  }
  const licensesDir = path.join(dir, "licenses");
  if (fs.existsSync(licensesDir)) {
    const used = new Set(manifest.fonts.map((f) => path.posix.basename(String(f.licenseFile ?? ""))));
    for (const name of fs.readdirSync(licensesDir)) {
      if (!used.has(name)) errors.push(`${FONTS_DIR_REL}/licenses/${name} is not referenced by fonts.json`);
    }
  }
  return errors;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const errors = checkFonts(root);
  if (errors.length > 0) {
    console.error("font verification failed:");
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }
  console.log("bundled fonts verified.");
}
