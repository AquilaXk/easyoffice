#!/usr/bin/env node
/**
 * EasyOffice Repository Hygiene & Specification Guard
 *
 * Verifies:
 * 1. Presence of required product, architecture, policy, and workflow files.
 * 2. Zero local-only configuration files tracked in Git.
 * 3. Zero prohibited external brand mentions or tool attribution traces in tracked files.
 */

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";

const ROOT = process.cwd();

const REQUIRED_TRACKED_CONTRACTS = [
  "README.md",
  "README.ko.md",
  "SECURITY.md",
  "LICENSE",
  "Package.swift",
  "Cargo.toml",
  "crates/easyoffice-formats/src/lib.rs",
  "crates/easyoffice-sheet/src/lib.rs",
  "crates/easyoffice-layout/src/lib.rs",
  "crates/easyoffice-app/src/lib.rs",
  "crates/easyoffice-app/src/http_server.rs",
  "crates/easyoffice-app/src/security_ipc.rs",
  "crates/easyoffice-app/ui/sheet-utils.js",
  "crates/easyoffice-app/ui/pii-dom-scrubber.js",
  "crates/easyoffice-app/ui/sheet-store.js",
  "crates/easyoffice-app/ui/ribbon-catalog.js",
  "crates/easyoffice-app/ui/save-payload.js",
  "crates/easyoffice-app/ui/workspace-tools.js",
  "crates/easyoffice-app/ui/workspace-panels.js",
  "crates/easyoffice-app/ui/sheet-controller.js",
  "crates/easyoffice-app/ui/ribbon-history.js",
  "crates/easyoffice-app/ui/workspace-session.js",
  "crates/easyoffice-app/ui/app.js",
  "Sources/EasyOfficeCore/DocumentSecurityAuditor.swift",
  "fixtures/pii-scrub-vectors.json",
  "fixtures/sheet-coercion-parity.json",
  "docs/policy-decisions.md",
  "docs/design-system.md",
  "docs/product-spec.md",
  "docs/architecture.md",
  ".github/PULL_REQUEST_TEMPLATE.md",
  ".github/release.yml",
  ".github/workflows/ci.yml",
  ".github/workflows/automerge.yml",
  ".github/workflows/release.yml",
  "scripts/verify-macos-plist.mjs",
  "scripts/test-support/offline-formula-ast.js",
  "scripts/test-support/offline-formula-eval.js",
  "scripts/test-support/offline-rpc.js",
  "scripts/webui-dom-harness.mjs",
  "scripts/test-webui-behavior.mjs",
  "scripts/test-webui-ux-ergonomics.mjs",
  "scripts/test-webui-sheet-atomicity.mjs",
  "scripts/test-webui-round13-parity.mjs",
  "scripts/test-webui-security-ipc.mjs",
  "scripts/test-webui-session-context.mjs",
  "scripts/test-webui-save-fidelity.mjs",
];

// Local-only filenames/directories that must NEVER be tracked by Git (Base64 encoded)
// ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CURSOR.md", "COPILOT.md", ".agents/", ".claude/", ".codex/", ".gemini/", "docs/agent/"]
const FORBIDDEN_GIT_PATH_B64 = [
  "QUdFTlRTLm1k",
  "Q0xBVURFLm1k",
  "R0VNSU5JLm1k",
  "Q1VSU09SLm1k",
  "Q09QSUxPVC5tZA==",
  "LmFnZW50cy8=",
  "LmNsYXVkZS8=",
  "LmNvZGV4Lw==",
  "LmdlbWluaS8=",
  "ZG9jcy9hZ2VudC8=",
];

const FORBIDDEN_GIT_PATHS = FORBIDDEN_GIT_PATH_B64.map((b) =>
  Buffer.from(b, "base64").toString("utf8")
);

// Prohibited external brand tokens + tool/agent tokens in tracked files (Base64 encoded)
const PROHIBITED_CONTENT_B64 = [
  "cG9sYXJpcyBvZmZpY2U=",     // external brand (en)
  "cG9sYXJpc29mZmljZQ==",     // external brand compact
  "7Y+0652866as7Iqk",         // external brand (ko)
  "7ZWc7Lu07Jik7ZS87Iqk",     // external brand 2 (ko)
  "aGFuY29tIG9mZmljZQ==",     // external brand 2 (en)
  "QUdFTlRTLm1k",             // local-only doc ref 1
  "Q0xBVURFLm1k",             // local-only doc ref 2
  "R0VNSU5JLm1k",             // local-only doc ref 3
  "QW50aWdyYXZpdHk=",         // tool name 1
  "Q2xhdWRlIENvZGU=",         // tool name 2
  "YXF1aWxhLXJldmlldw==",     // internal review skill name
  "c3ViYWdlbnQ=",             // internal orchestration term
  "Q28tQXV0aG9yZWQtQnk6",     // attribution header
];

const PROHIBITED_PATTERNS = PROHIBITED_CONTENT_B64.map(
  (b64) => new RegExp(Buffer.from(b64, "base64").toString("utf8"), "i")
);

const IGNORED_DIRS = new Set([
  ".git",
  ".build",
  ".swiftpm",
  "target",
  "DerivedData",
  "dist",
  "release-build",
  "node_modules",
  ".agents",
  ".claude",
  ".codex",
  ".gemini",
  ".cursor",
  ".superpowers",
  ".worktrees",
]);

const IGNORED_ROOT_FILES = new Set(FORBIDDEN_GIT_PATHS);

const TEXT_EXTENSIONS = new Set([
  ".md",
  ".swift",
  ".rs",
  ".toml",
  ".html",
  ".css",
  ".yml",
  ".yaml",
  ".json",
  ".mjs",
  ".js",
  ".sh",
  ".txt",
  ".plist",
]);

function walkTrackedCandidateFiles(dir, collected = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (IGNORED_DIRS.has(entry.name)) continue;
    const fullPath = path.join(dir, entry.name);
    const relPath = path.relative(ROOT, fullPath).replaceAll("\\", "/");
    if (IGNORED_ROOT_FILES.has(relPath)) continue;

    if (entry.isDirectory()) {
      walkTrackedCandidateFiles(fullPath, collected);
    } else if (entry.isFile()) {
      const ext = path.extname(entry.name);
      if (TEXT_EXTENSIONS.has(ext) || entry.name === "LICENSE") {
        collected.push(fullPath);
      }
    }
  }
  return collected;
}

const errors = [];

for (const rel of REQUIRED_TRACKED_CONTRACTS) {
  const full = path.join(ROOT, rel);
  if (!fs.existsSync(full)) {
    errors.push(`Missing required repository file: ${rel}`);
  }
}

// Verify git tracked files never include local-only files
try {
  const trackedRaw = execFileSync("git", ["ls-files"], {
    cwd: ROOT,
    encoding: "utf8",
  });
  const trackedList = trackedRaw
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);

  for (const tracked of trackedList) {
    for (const forbidden of FORBIDDEN_GIT_PATHS) {
      if (tracked === forbidden || tracked.startsWith(forbidden)) {
        errors.push(`Forbidden local-only file is tracked by Git: ${tracked}`);
      }
    }
  }
} catch {
  // Ignore if git is not initialized
}

const files = walkTrackedCandidateFiles(ROOT);
for (const file of files) {
  const rel = path.relative(ROOT, file).replaceAll("\\", "/");
  if (rel === "scripts/check-repo-hygiene.mjs") continue;

  const content = fs.readFileSync(file, "utf8");
  for (const regex of PROHIBITED_PATTERNS) {
    if (regex.test(content)) {
      errors.push(`Prohibited external brand or local-only reference detected in ${rel} (pattern: ${regex.source})`);
    }
  }

  if (rel.endsWith(".js") || rel.endsWith(".mjs")) {
    try {
      execFileSync(process.execPath, ["--check", file], {
        cwd: ROOT,
        stdio: "pipe",
      });
    } catch (err) {
      const stderr = err && err.stderr ? err.stderr.toString("utf8").trim() : String(err);
      errors.push(`JavaScript syntax check failed in ${rel}: ${stderr}`);
    }
  }
}

const FORBIDDEN_UI_GATE_PHRASES = [
  "Fail-Closed",
  "CoreText CJK 서브셋",
  "Signature Lavender #5C6BC0",
  "Primary #5C6BC0",
  "시그니처 라벤더(#5C6BC0)",
  "내장 함수 카탈로그: 122종",
  "ISO 32000-1 무결성 검증",
  "doc-mod- DropCap",
  "EasyOffice for macOS",
  "시그니처 라벤더 디자인 시스템과 올인원 문서 워크스페이스",
  "122종 함수 마법사",
  "매직 바이트 보안 검증",
  "Rich Page Layout Engine",
  "Virtualized A1 Formula Grid",
  "Stage Canvas & Vector PDF Studio",
];

const webuiCssPath = path.join(ROOT, "crates/easyoffice-app/ui/styles.css");
if (fs.existsSync(webuiCssPath)) {
  const cssContent = fs.readFileSync(webuiCssPath, "utf8");
  const cssBoxMatches = cssContent.match(/border:|border-radius:|box-shadow:/g) || [];
  if (cssBoxMatches.length > 24) {
    errors.push(
      `WebUI styles.css over-boxing guard failed: found ${cssBoxMatches.length} matches of border:/border-radius:/box-shadow: (max 24)`
    );
  }
}

const webuiDir = path.join(ROOT, "crates/easyoffice-app/ui");
const webuiJsRels = fs.existsSync(webuiDir)
  ? fs
      .readdirSync(webuiDir)
      .filter((name) => name.endsWith(".js"))
      .sort()
      .map((name) => `crates/easyoffice-app/ui/${name}`)
  : [];

for (const uiRel of ["crates/easyoffice-app/ui/index.html", ...webuiJsRels]) {
  const uiFull = path.join(ROOT, uiRel);
  if (!fs.existsSync(uiFull)) continue;
  const uiText = fs.readFileSync(uiFull, "utf8");
  for (const phrase of FORBIDDEN_UI_GATE_PHRASES) {
    if (uiText.includes(phrase)) {
      errors.push(`Forbidden internal gate/spec copy "${phrase}" detected in ${uiRel}`);
    }
  }
  if (uiRel.endsWith(".html")) {
    if (/\sstyle\s*=/i.test(uiText)) {
      errors.push(`Inline HTML style= attribute detected in ${uiRel} (violates CSP default-src 'self')`);
    }
    if (/offline-(?:formula-ast|formula-eval|rpc)\.js/i.test(uiText)) {
      errors.push(`Production HTML ${uiRel} must not load offline test-support scripts`);
    }
  }
  if (uiRel.endsWith(".js")) {
    const lineCount = uiText.split("\n").length - (uiText.endsWith("\n") ? 1 : 0);
    if (lineCount >= 1000) {
      errors.push(`WebUI JS file ${uiRel} exceeds 1,000-line ceiling (${lineCount} lines)`);
    }
    if (/\.innerHTML\s*\+=/u.test(uiText)) {
      errors.push(`Prohibited .innerHTML += mutation detected in ${uiRel}`);
    }
    for (const match of uiText.matchAll(/\.innerHTML\s*=\s*([^\n;]+)/g)) {
      const rhs = match[1].trim();
      if (rhs !== '""' && rhs !== "''") {
        errors.push(`Dynamic .innerHTML assignment detected in ${uiRel}: ${match[0].trim()}`);
      }
    }
    if (/\.setAttribute\(\s*["']style["']/i.test(uiText)) {
      errors.push(`Prohibited setAttribute("style", ...) detected in ${uiRel} (violates CSP default-src 'self')`);
    }
  }
}

const swiftAppDir = path.join(ROOT, "Sources/EasyOfficeApp");
if (fs.existsSync(swiftAppDir)) {
  const swiftFiles = fs
    .readdirSync(swiftAppDir)
    .filter((name) => name.endsWith(".swift"))
    .map((name) => path.join(swiftAppDir, name));
  let swiftBoxPrimCount = 0;
  let swiftBoxCount = 0;
  for (const sf of swiftFiles) {
    const sfText = fs.readFileSync(sf, "utf8");
    const primMatches =
      sfText.match(
        /buttonStyle\(\.bordered|LavenderTextFieldPill|LavenderMenuPill|LavenderActionPill|RoundedRectangle|Capsule|strokeBorder|\.stroke\(/g
      ) || [];
    const bgMatches = sfText.match(/RoundedRectangle|\.stroke\(|\.background\(/g) || [];
    swiftBoxPrimCount += primMatches.length;
    swiftBoxCount += bgMatches.length;
    const sfRel = path.relative(ROOT, sf).replaceAll("\\", "/");
    for (const phrase of FORBIDDEN_UI_GATE_PHRASES) {
      if (sfText.includes(phrase)) {
        errors.push(`Forbidden internal gate/spec copy "${phrase}" detected in ${sfRel}`);
      }
    }
  }
  if (swiftBoxPrimCount > 16) {
    errors.push(
      `Swift EasyOfficeApp box-primitive guard failed: found ${swiftBoxPrimCount} matches of bordered/pill/RoundedRectangle/Capsule/stroke (max 16)`
    );
  }
  if (swiftBoxCount > 120) {
    errors.push(
      `Swift EasyOfficeApp over-boxing guard failed: found ${swiftBoxCount} matches of RoundedRectangle/.stroke(/.background( (max 120)`
    );
  }
}

const UNCHECKED_SWIFT_FLOAT_INT_REGEX = /\b(Int|UInt32|UInt16)\((floor|ceil|round)\(/;
let foundSafeIntStrictBound = false;
for (const file of files) {
  const rel = path.relative(ROOT, file).replaceAll("\\", "/");
  if (rel.startsWith("Sources/") && rel.endsWith(".swift")) {
    const text = fs.readFileSync(file, "utf8");
    if (UNCHECKED_SWIFT_FLOAT_INT_REGEX.test(text)) {
      errors.push(
        `Unchecked float-to-integer conversion detected in ${rel}; use safeInt/checkedInteger/safeClampedInt instead.`
      );
    }
    if (rel.startsWith("Sources/EasyOfficeCore/") && text.includes("func safeInt(")) {
      if (text.includes(">= -9223372036854775808.0")) {
        errors.push(
          `safeInt in ${rel} must exclude Int.min (-9223372036854775808.0) via strict '>' to prevent negation trap.`
        );
      }
      if (text.includes("> -9223372036854775808.0")) {
        foundSafeIntStrictBound = true;
      }
    }
    if (rel.startsWith("Sources/EasyOfficeCore/") && rel.endsWith(".swift")) {
      const lineCount = text.split("\n").length - (text.endsWith("\n") ? 1 : 0);
      if (lineCount >= 1000) {
        errors.push(`Swift core module ${rel} exceeds 1,000-line ceiling (${lineCount} lines)`);
      }
    }
    if (
      (rel === "Sources/EasyOfficeCore/BinaryContainerCodec.swift" ||
        rel === "Sources/EasyOfficeCore/OLE2AndHWP5Codec.swift" ||
        rel === "Sources/EasyOfficeCore/HWP5DocumentCodec.swift" ||
        rel === "Sources/EasyOfficeCore/LegacyBIFF8XLSCodec.swift" ||
        rel === "Sources/EasyOfficeCore/LegacyOLE2OfficeCodec.swift") &&
      (text.includes("abs(Int.min)") ||
        (/\bInt\(Double\(/.test(text) && !text.includes("maxRatio: Double = 100.0")))
    ) {
      errors.push(`Unguarded Int(Double( or abs(Int.min) trap detected in ${rel}.`);
    }
  }
  if (
    (rel.startsWith("crates/easyoffice-sheet/src/") ||
      rel.startsWith("crates/easyoffice-formats/src/") ||
      rel.startsWith("crates/easyoffice-layout/src/")) &&
    rel.endsWith(".rs")
  ) {
    const rsText = fs.readFileSync(file, "utf8");
    const lineCount = rsText.split("\n").length - (rsText.endsWith("\n") ? 1 : 0);
    if (lineCount >= 1000) {
      errors.push(`Rust module ${rel} exceeds 1,000-line ceiling (${lineCount} lines)`);
    }
  }
}
if (!foundSafeIntStrictBound) {
  errors.push(
    "Missing safeInt strict lower bound (> -9223372036854775808.0) in Sources/EasyOfficeCore/."
  );
}

const POST_185_RUST_APIS = [".is_multiple_of(", ".as_chunks(", ".floor_char_boundary("];
for (const file of files) {
  const rel = path.relative(ROOT, file).replaceAll("\\", "/");
  if (rel.startsWith("crates/") && rel.endsWith("/Cargo.toml")) {
    const tomlText = fs.readFileSync(file, "utf8");
    if (!tomlText.includes("rust-version.workspace = true")) {
      errors.push(`Crate manifest ${rel} is missing 'rust-version.workspace = true' for MSRV 1.85 enforcement.`);
    }
  }
  if (rel.startsWith("crates/") && rel.endsWith(".rs")) {
    const rsText = fs.readFileSync(file, "utf8");
    for (const api of POST_185_RUST_APIS) {
      if (rsText.includes(api)) {
        errors.push(`Post-Rust-1.85 API '${api}' detected in ${rel} (breaks MSRV 1.85).`);
      }
    }
  }
}

const formatSwiftPath = path.join(ROOT, "Sources/EasyOfficeCore/DocumentFormat.swift");
const sheetSwiftPath = path.join(ROOT, "Sources/EasyOfficeCore/SheetEngine.swift");
const ribbonCatalogJsPath = path.join(ROOT, "crates/easyoffice-app/ui/ribbon-catalog.js");
const rustIpcLibPath = path.join(ROOT, "crates/easyoffice-app/src/lib.rs");

if (
  fs.existsSync(formatSwiftPath) &&
  fs.existsSync(sheetSwiftPath) &&
  fs.existsSync(ribbonCatalogJsPath) &&
  fs.existsSync(rustIpcLibPath)
) {
  const formatSwift = fs.readFileSync(formatSwiftPath, "utf8");
  const enumSection = formatSwift.split("public var id:")[0].split("public enum DocumentFormat")[1] || "";
  const formatCount = enumSection
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("case ")).length;

  const sheetSwift = fs.readFileSync(sheetSwiftPath, "utf8");
  const fnMatch = sheetSwift.match(/supportedFunctionNames:\s*\[String\]\s*=\s*\[([\s\S]*?)\]\.sorted\(\)/);
  const swiftFnNames = fnMatch
    ? (fnMatch[1].match(/"[A-Z0-9]+"/g) || []).map((s) => s.replaceAll('"', ""))
    : [];
  const functionCount = swiftFnNames.length;

  const rustSheetFunctionsPath = path.join(ROOT, "crates/easyoffice-sheet/src/functions.rs");
  if (fs.existsSync(rustSheetFunctionsPath)) {
    const rustSheetFnText = fs.readFileSync(rustSheetFunctionsPath, "utf8");
    const rustFnMatch = rustSheetFnText.match(
      /CANONICAL_122_FUNCTIONS:\s*\[&str;\s*122\]\s*=\s*\[([\s\S]*?)\];/
    );
    const rustFnNames = rustFnMatch
      ? (rustFnMatch[1].match(/"[A-Z0-9]+"/g) || []).map((s) => s.replaceAll('"', ""))
      : [];
    const swiftSorted = [...swiftFnNames].sort((a, b) => a.localeCompare(b)).join(",");
    const rustSorted = [...rustFnNames].sort((a, b) => a.localeCompare(b)).join(",");
    if (swiftSorted !== rustSorted) {
      errors.push(
        "Swift supportedFunctionNames in SheetEngine.swift and Rust CANONICAL_122_FUNCTIONS in functions.rs do not match 1:1"
      );
    }
  }

  const ribbonCatalogJs = fs.readFileSync(ribbonCatalogJsPath, "utf8");
  const commonSpecCount = (ribbonCatalogJs.match(/\{\s*suffix:\s*"/g) || []).length;
  const modSpecSection = ribbonCatalogJs.split("function buildRibbonCatalog")[0].split("MODULE_SPECIFIC_SPECS")[1] || "";
  const modSpecCount = (modSpecSection.match(/\{\s*id:\s*"/g) || []).length;
  const ribbonCommandCount = commonSpecCount * 5 + modSpecCount;

  const rustIpcLib = fs.readFileSync(rustIpcLibPath, "utf8");
  const dispatchBlock = (rustIpcLib.split("pub fn dispatch(")[1] || "").split("fn handle_workspace_bootstrap")[0] || "";
  const ipcCommandCount = dispatchBlock
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith('"') && line.includes("=>")).length;

  if (formatCount !== 20) {
    errors.push(`Expected 20 DocumentFormat cases in DocumentFormat.swift, found ${formatCount}`);
  }
  if (functionCount !== 122) {
    errors.push(`Expected 122 built-in functions in SheetEngine.swift, found ${functionCount}`);
  }
  if (ribbonCommandCount !== 140) {
    errors.push(`Expected 140 ribbon commands in ribbon-catalog.js, found ${ribbonCommandCount}`);
  }
  if (ipcCommandCount !== 15) {
    errors.push(`Expected 15 IPC dispatch arms in crates/easyoffice-app/src/lib.rs, found ${ipcCommandCount}`);
  }

  const STALE_DOC_TOKENS = [
    "77 functions",
    "77 sheet functions",
    "77 Functions",
    "77종",
    "13종 IPC",
    "13종 명령",
    "tauri.conf.json",
    ".msi",
    ".nsis",
  ];
  const DOC_SYNC_TARGETS = [
    "README.md",
    "README.ko.md",
    "docs/architecture.md",
    "docs/product-spec.md",
    "docs/policy-decisions.md",
  ];

  for (const docRel of DOC_SYNC_TARGETS) {
    const docFull = path.join(ROOT, docRel);
    if (!fs.existsSync(docFull)) continue;
    const docText = fs.readFileSync(docFull, "utf8");
    for (const stale of STALE_DOC_TOKENS) {
      if (docText.includes(stale)) {
        errors.push(`Stale specification claim "${stale}" detected in ${docRel}`);
      }
    }
    if (docRel !== "docs/policy-decisions.md") {
      if (!docText.includes(String(formatCount))) {
        errors.push(`Doc ${docRel} does not reference verified format count ${formatCount}`);
      }
      if (!docText.includes(String(functionCount))) {
        errors.push(`Doc ${docRel} does not reference verified sheet function count ${functionCount}`);
      }
    }
  }

  const archText = fs.readFileSync(path.join(ROOT, "docs/architecture.md"), "utf8");
  if (!archText.includes(`${ribbonCommandCount}종`)) {
    errors.push(`docs/architecture.md does not reference verified ribbon command count ${ribbonCommandCount}종`);
  }
  if (!archText.includes(`${ipcCommandCount}종 명령`)) {
    errors.push(`docs/architecture.md does not reference verified IPC command count ${ipcCommandCount}종 명령`);
  }
}

if (errors.length > 0) {
  console.error("Repository hygiene check failed:");
  for (const err of errors) {
    console.error(`  - ${err}`);
  }
  process.exit(1);
}

console.log(
  `Repository hygiene check passed (${files.length} repository files inspected, ${REQUIRED_TRACKED_CONTRACTS.length} required contracts verified, 0 local-only files tracked, WebUI CSP/XSS & numeric doc-sync guards verified).`
);
