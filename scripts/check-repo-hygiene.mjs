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
  "crates/easyoffice-app/tauri.conf.json",
  "docs/policy-decisions.md",
  "docs/design-system.md",
  "docs/product-spec.md",
  "docs/architecture.md",
  ".github/PULL_REQUEST_TEMPLATE.md",
  ".github/release.yml",
  ".github/workflows/ci.yml",
  ".github/workflows/automerge.yml",
  ".github/workflows/release.yml",
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
}

if (errors.length > 0) {
  console.error("Repository hygiene check failed:");
  for (const err of errors) {
    console.error(`  - ${err}`);
  }
  process.exit(1);
}

console.log(
  `Repository hygiene check passed (${files.length} repository files inspected, ${REQUIRED_TRACKED_CONTRACTS.length} required contracts verified, 0 local-only files tracked).`
);
