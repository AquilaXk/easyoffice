#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const ROOT = process.cwd();
const FORMAT_SWIFT_PATH = path.join(ROOT, "Sources/EasyOfficeCore/DocumentFormat.swift");
const PACKAGE_SCRIPT_PATH = path.join(ROOT, "scripts/package-macos-release.sh");
const DIST_PLIST_PATH = path.join(ROOT, "dist/EasyOffice.app/Contents/Info.plist");

const STANDARD_UTI_BY_EXTENSION = new Map([
  ["docx", "org.openxmlformats.wordprocessingml.document"],
  ["xlsx", "org.openxmlformats.spreadsheetml.sheet"],
  ["pptx", "org.openxmlformats.presentationml.presentation"],
  ["csv", "public.comma-separated-values-text"],
  ["tsv", "public.tab-separated-values-text"],
  ["rtf", "public.rtf"],
  ["md", "net.daringfireball.markdown"],
  ["txt", "public.plain-text"],
]);

function extractSupportedExtensionsFromSwift() {
  const src = fs.readFileSync(FORMAT_SWIFT_PATH, "utf8");
  const enumStart = src.indexOf("public enum DocumentFormat:");
  if (enumStart < 0) {
    throw new TypeError("Could not locate DocumentFormat enum in DocumentFormat.swift");
  }
  const bodySlice = src.slice(enumStart, src.indexOf("public var id:", enumStart));
  const extensions = [];
  for (const rawLine of bodySlice.split("\n")) {
    const trimmed = rawLine.trim();
    if (trimmed.startsWith("case ")) {
      const ext = trimmed.slice(5).trim();
      if (ext.length > 0) {
        extensions.push(ext);
      }
    }
  }
  return extensions;
}

function readInfoPlistXml() {
  if (fs.existsSync(DIST_PLIST_PATH)) {
    return fs.readFileSync(DIST_PLIST_PATH, "utf8");
  }
  const scriptSrc = fs.readFileSync(PACKAGE_SCRIPT_PATH, "utf8");
  const startMarker = '<?xml version="1.0" encoding="UTF-8"?>';
  const endMarker = "</plist>";
  const startIdx = scriptSrc.indexOf(startMarker);
  const endIdx = scriptSrc.indexOf(endMarker, startIdx);
  if (startIdx < 0 || endIdx < 0) {
    throw new TypeError("Could not locate Info.plist XML block in scripts/package-macos-release.sh");
  }
  return scriptSrc.slice(startIdx, endIdx + endMarker.length);
}

function extractStringArrayAfterKey(dictBlock, keyName) {
  const keyTag = `<key>${keyName}</key>`;
  const keyIdx = dictBlock.indexOf(keyTag);
  if (keyIdx < 0) {
    return null;
  }
  const arrayStart = dictBlock.indexOf("<array>", keyIdx);
  const arrayEnd = dictBlock.indexOf("</array>", arrayStart);
  if (arrayStart < 0 || arrayEnd < 0) {
    return [];
  }
  const arraySlice = dictBlock.slice(arrayStart, arrayEnd);
  const values = [];
  for (const rawLine of arraySlice.split("\n")) {
    const trimmed = rawLine.trim();
    if (trimmed.startsWith("<string>") && trimmed.endsWith("</string>")) {
      values.push(trimmed.slice(8, -9).trim());
    }
  }
  return values;
}

function extractSingleStringAfterKey(dictBlock, keyName) {
  const keyTag = `<key>${keyName}</key>`;
  const keyIdx = dictBlock.indexOf(keyTag);
  if (keyIdx < 0) {
    return "";
  }
  const strStart = dictBlock.indexOf("<string>", keyIdx);
  const strEnd = dictBlock.indexOf("</string>", strStart);
  if (strStart < 0 || strEnd < 0) {
    return "";
  }
  return dictBlock.slice(strStart + 8, strEnd).trim();
}

function parseDocumentTypeEntries(plistXml) {
  const docTypesKey = "<key>CFBundleDocumentTypes</key>";
  const keyIdx = plistXml.indexOf(docTypesKey);
  if (keyIdx < 0) {
    throw new TypeError("CFBundleDocumentTypes key missing from Info.plist");
  }
  const outerArrayStart = plistXml.indexOf("<array>", keyIdx);
  const outerArrayEnd = plistXml.lastIndexOf("</array>");
  if (outerArrayStart < 0 || outerArrayEnd <= outerArrayStart) {
    throw new TypeError("CFBundleDocumentTypes array missing from Info.plist");
  }
  const body = plistXml.slice(outerArrayStart + 7, outerArrayEnd);
  const entries = [];
  let cursor = 0;
  while (cursor < body.length) {
    const dictStart = body.indexOf("<dict>", cursor);
    if (dictStart < 0) {
      break;
    }
    const dictEnd = body.indexOf("</dict>", dictStart);
    if (dictEnd < 0) {
      throw new TypeError("Unclosed <dict> in CFBundleDocumentTypes");
    }
    const dictBlock = body.slice(dictStart + 6, dictEnd);
    entries.push({
      CFBundleTypeName: extractSingleStringAfterKey(dictBlock, "CFBundleTypeName"),
      CFBundleTypeRole: extractSingleStringAfterKey(dictBlock, "CFBundleTypeRole"),
      CFBundleTypeExtensions: extractStringArrayAfterKey(dictBlock, "CFBundleTypeExtensions") || [],
      LSItemContentTypes: extractStringArrayAfterKey(dictBlock, "LSItemContentTypes"),
    });
    cursor = dictEnd + 7;
  }
  return entries;
}

function collectEntryExtensions(entry, unshadowedCovered) {
  if (entry.CFBundleTypeRole !== "Editor") {
    throw new TypeError(`Document type "${entry.CFBundleTypeName}" must declare Editor role`);
  }
  const exts = entry.CFBundleTypeExtensions;
  const utis = entry.LSItemContentTypes;
  if (!utis || utis.length === 0) {
    for (const ext of exts) {
      unshadowedCovered.add(ext);
    }
    return;
  }
  const utiSet = new Set(utis);
  for (const ext of exts) {
    const expectedUti = STANDARD_UTI_BY_EXTENSION.get(ext);
    if (!expectedUti || !utiSet.has(expectedUti)) {
      throw new TypeError(
        `Extension ".${ext}" in "${entry.CFBundleTypeName}" is shadowed by LSItemContentTypes without a matching system UTI`
      );
    }
    unshadowedCovered.add(ext);
  }
}

function verifyInfoPlistRegistration() {
  const plistXml = readInfoPlistXml();
  const docTypes = parseDocumentTypeEntries(plistXml);
  if (docTypes.length === 0) {
    throw new TypeError("CFBundleDocumentTypes must be a non-empty array");
  }

  const expectedExtensions = extractSupportedExtensionsFromSwift();
  if (expectedExtensions.length !== 20) {
    throw new TypeError(`Expected 20 DocumentFormat cases, found ${expectedExtensions.length}`);
  }

  const unshadowedCovered = new Set();
  for (const entry of docTypes) {
    collectEntryExtensions(entry, unshadowedCovered);
  }

  const missing = expectedExtensions.filter((ext) => !unshadowedCovered.has(ext));
  if (missing.length > 0) {
    throw new TypeError(
      `Info.plist LaunchServices registration is missing or shadowing extensions: ${missing.join(", ")}`
    );
  }

  console.log(
    `macOS Info.plist verification passed (${expectedExtensions.length}/20 extensions registered without UTI shadowing across ${docTypes.length} document type groups).`
  );
}

verifyInfoPlistRegistration();
