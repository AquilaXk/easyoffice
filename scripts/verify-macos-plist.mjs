#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

const ROOT = process.cwd();
const FORMAT_SWIFT_PATH = path.join(ROOT, "Sources/EasyOfficeCore/DocumentFormat.swift");
const PACKAGE_SCRIPT_PATH = path.join(ROOT, "scripts/package-macos-release.sh");

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
    throw new Error("Could not locate DocumentFormat enum in DocumentFormat.swift");
  }
  const bodySlice = src.slice(enumStart, src.indexOf("public var id:", enumStart));
  const matches = [...bodySlice.matchAll(/^\s*case\s+([a-z0-9]+)\s*$/gm)];
  return matches.map((m) => m[1]);
}

function verifyPlistFile(plistPath) {
  if (process.platform !== "darwin") {
    console.log("Skipping plutil verification on non-macOS host.");
    return;
  }

  execFileSync("plutil", ["-lint", plistPath], { stdio: "pipe" });
  const jsonRaw = execFileSync("plutil", ["-convert", "json", "-o", "-", plistPath], {
    encoding: "utf8",
  });
  const plist = JSON.parse(jsonRaw);
  const docTypes = plist.CFBundleDocumentTypes;
  if (!Array.isArray(docTypes) || docTypes.length === 0) {
    throw new Error("CFBundleDocumentTypes must be a non-empty array");
  }

  const expectedExtensions = extractSupportedExtensionsFromSwift();
  if (expectedExtensions.length !== 20) {
    throw new Error(`Expected 20 DocumentFormat cases, found ${expectedExtensions.length}`);
  }

  const unshadowedCovered = new Set();
  for (const entry of docTypes) {
    if (entry.CFBundleTypeRole !== "Editor") {
      throw new Error(`Document type "${entry.CFBundleTypeName}" must declare Editor role`);
    }
    const exts = Array.isArray(entry.CFBundleTypeExtensions) ? entry.CFBundleTypeExtensions : [];
    const utis = Array.isArray(entry.LSItemContentTypes) ? entry.LSItemContentTypes : null;

    if (utis && utis.length > 0) {
      const utiSet = new Set(utis);
      for (const ext of exts) {
        const expectedUti = STANDARD_UTI_BY_EXTENSION.get(ext);
        if (!expectedUti || !utiSet.has(expectedUti)) {
          throw new Error(
            `Extension ".${ext}" in "${entry.CFBundleTypeName}" is shadowed by LSItemContentTypes without a matching system UTI`
          );
        }
        unshadowedCovered.add(ext);
      }
    } else {
      for (const ext of exts) {
        unshadowedCovered.add(ext);
      }
    }
  }

  const missing = expectedExtensions.filter((ext) => !unshadowedCovered.has(ext));
  if (missing.length > 0) {
    throw new Error(
      `Info.plist LaunchServices registration is missing or shadowing extensions: ${missing.join(", ")}`
    );
  }

  console.log(
    `macOS Info.plist verification passed (${expectedExtensions.length}/20 extensions registered without UTI shadowing across ${docTypes.length} document type groups).`
  );
}

const explicitPlist = process.argv[2];
if (explicitPlist) {
  verifyPlistFile(path.resolve(ROOT, explicitPlist));
} else {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "easyoffice-plist-"));
  const generatedPlist = path.join(tmpDir, "Info.plist");
  try {
    execFileSync("bash", [PACKAGE_SCRIPT_PATH, "1.0.0-verify", "--emit-plist-only", generatedPlist], {
      cwd: ROOT,
      stdio: "pipe",
    });
    verifyPlistFile(generatedPlist);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}
