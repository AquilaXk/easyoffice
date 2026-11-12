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
import { prohibitedPatternsIn } from "./prohibited-terms.mjs";
import { execFileSync } from "node:child_process";
import { readConfig, resolveGitExecutable, verifyUpstream } from "./verify-rhwp-upstream.mjs";
import { verifyOverrides } from "./hwp-studio-overrides.mjs";
import { checkFonts } from "./verify-fonts.mjs";
import { findBareSorts, isSortRuleScoped } from "./sort-comparator-rule.mjs";
import { findHtmlSinkViolations } from "./html-sink-rule.mjs";
import { findServeReleaseViolations } from "./release-serve-rule.mjs";
import { checkDesktopBundleConfig } from "./desktop-file-associations.mjs";

const ROOT = process.cwd();

const REQUIRED_TRACKED_CONTRACTS = [
  "README.md",
  "README.ko.md",
  "SECURITY.md",
  "LICENSE",
  "Cargo.toml",
  "crates/easyoffice-formats/src/lib.rs",
  "crates/easyoffice-sheet/src/lib.rs",
  "crates/easyoffice-layout/src/lib.rs",
  "crates/easyoffice-app/src/lib.rs",
  "crates/easyoffice-app/src/http_server.rs",
  "crates/easyoffice-app/src/security_ipc.rs",
  "crates/easyoffice-app/ui/sheet-utils.js",
  "crates/easyoffice-app/ui/pii-dom-scrubber.js",
  "crates/easyoffice-app/ui/sheet-rules.js",
  "crates/easyoffice-app/ui/sheet-store.js",
  "crates/easyoffice-app/ui/ribbon-catalog.js",
  "crates/easyoffice-app/ui/save-payload.js",
  "crates/easyoffice-app/ui/desktop-bridge.js",
  "crates/easyoffice-app/ui/custom-select.js",
  "crates/easyoffice-app/ui/color-picker.js",
  "crates/easyoffice-app/ui/workspace-scrub.js",
  "crates/easyoffice-app/ui/workspace-tools.js",
  "crates/easyoffice-app/ui/workspace-panels.js",
  "crates/easyoffice-app/ui/sheet-grid.js",
  "crates/easyoffice-app/ui/sheet-clipboard.js",
  "crates/easyoffice-app/ui/sheet-input.js",
  "crates/easyoffice-app/ui/sheet-ops.js",
  "crates/easyoffice-app/ui/sheet-dialog-kit.js",
  "crates/easyoffice-app/ui/sheet-pivot-ui.js",
  "crates/easyoffice-app/ui/sheet-rules-ui.js",
  "crates/easyoffice-app/ui/sheet-format.js",
  "crates/easyoffice-app/ui/sheet-controller.js",
  "crates/easyoffice-app/ui/ribbon-history.js",
  "crates/easyoffice-app/ui/workspace-session.js",
  "crates/easyoffice-app/ui/file-open.js",
  "crates/easyoffice-app/ui/pdf-edit.js",
  "crates/easyoffice-app/ui/icons.js",
  "crates/easyoffice-app/ui/window-bridge.js",
  "crates/easyoffice-app/ui/ipc-transport.js",
  "crates/easyoffice-app/ui/menu-keyboard.js",
  "crates/easyoffice-app/ui/shell.css",
  "crates/easyoffice-app/ui/tokens.css",
  "crates/easyoffice-app/ui/content.css",
  "crates/easyoffice-app/ui/theme.js",
  "crates/easyoffice-app/ui/home.css",
  "crates/easyoffice-app/ui/menu-bar.css",
  "crates/easyoffice-app/ui/toolbar.css",
  "crates/easyoffice-app/ui/home-screen.js",
  "crates/easyoffice-app/ui/recent-documents-dialog.js",
  "crates/easyoffice-app/ui/update-notice.js",
  "crates/easyoffice-app/ui/slide-studio.js",
  "crates/easyoffice-app/ui/doc-runs.js",
  "crates/easyoffice-app/ui/doc-runs-ui.js",
  "crates/easyoffice-app/ui/hwp-host.js",
  "crates/easyoffice-app/ui/hwp-print.js",
  "crates/easyoffice-app/ui/office-print.js",
  "crates/easyoffice-app/ui/find-core.js",
  "crates/easyoffice-app/ui/find-doc.js",
  "crates/easyoffice-app/ui/find-sheet.js",
  "crates/easyoffice-app/ui/find-replace.js",
  "crates/easyoffice-app/ui/hwp-session.js",
  "crates/easyoffice-app/ui/shell-chrome.js",
  "crates/easyoffice-app/ui/app.js",
  "crates/easyoffice-app/src/hwp_bundle.rs",
  "crates/easyoffice-app/build.rs",
  "crates/easyoffice-desktop/Cargo.toml",
  "crates/easyoffice-desktop/build.rs",
  "crates/easyoffice-desktop/src/main.rs",
  "crates/easyoffice-desktop/src/app_commands.rs",
  "crates/easyoffice-desktop/src/windows.rs",
  "crates/easyoffice-desktop/src/window_commands.rs",
  "crates/easyoffice-desktop/src/recent.rs",
  "crates/easyoffice-desktop/tauri.conf.json",
  "crates/easyoffice-desktop/linux/easyoffice.desktop",
  "crates/easyoffice-desktop/capabilities/main-window.json",
  "crates/easyoffice-desktop/icons/icon.ico",
  "crates/easyoffice-desktop/icons/icon.icns",
  "scripts/generate-desktop-icons.mjs",
  "fixtures/pii-scrub-vectors.json",
  "fixtures/sheet-coercion-parity.json",
  "docs/policy-decisions.md",
  "docs/design-system.md",
  "docs/product-spec.md",
  "docs/architecture.md",
  ".github/release.yml",
  ".github/workflows/ci.yml",
  ".github/workflows/release.yml",
  ".github/actions/install-linux-deps/action.yml",
  ".github/actions/prepare-desktop-build/action.yml",
  ".github/actions/cache-tauri-tools/action.yml",
  "scripts/retry-on-download-failure.mjs",
  "scripts/test-retry-on-download-failure.mjs",
  "scripts/desktop-file-associations.mjs",
  "scripts/test-desktop-file-associations.mjs",
  "scripts/release-assets.mjs",
  "scripts/test-release-assets.mjs",
  "scripts/verify-linux-bundles.mjs",
  "scripts/test-verify-linux-bundles.mjs",
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
  "scripts/test-webui-desktop-bridge.mjs",
  "scripts/test-webui-file-open.mjs",
  "scripts/test-webui-slide-show.mjs",
  "scripts/test-webui-hwp-host.mjs",
  "scripts/test-webui-hwp-print.mjs",
  "scripts/test-webui-home-preview.mjs",
  "scripts/test-webui-hwp-session.mjs",
  "scripts/test-webui-windows.mjs",
  "scripts/test-webui-shell-chrome.mjs",
  "scripts/test-webui-hwp-shortcuts.mjs",
  "scripts/test-support/modules.mjs",
  "apps/hwp-studio/src/embed/shell-shortcuts.ts",
  ".gitmodules",
  "config/rhwp-upstream.json",
  "scripts/verify-rhwp-upstream.mjs",
  "scripts/checkout-rhwp.mjs",
  "scripts/test-rhwp-upstream.mjs",
  "scripts/update-rhwp.mjs",
  "scripts/test-update-rhwp.mjs",
  "THIRD_PARTY_NOTICES.md",
  "docs/rhwp-integration-design.md",
  "scripts/test-webui-doc-runs.mjs",
  "scripts/test-webui-sheet-editing.mjs",
  "vendor/rhwp-core/PROVENANCE.json",
  "vendor/rhwp-core/rhwp_bg.wasm",
  "scripts/verify-rhwp-core.mjs",
  "scripts/vendor-rhwp-core.mjs",
  "config/rhwp-studio-overrides.json",
  "apps/hwp-studio/package.json",
  "apps/hwp-studio/package-lock.json",
  "apps/hwp-studio/vite.config.ts",
  "apps/hwp-studio/src/main.ts",
  "assets/fonts/fonts.json",
  "assets/fonts/FONTS.md",
  "scripts/hwp-studio-overrides.mjs",
  "scripts/verify-fonts.mjs",
  "scripts/build-hwp-studio.mjs",
  "scripts/check-hwp-studio-bundle.mjs",
  "scripts/hwp-studio-harness.mjs",
  "scripts/hwp-roundtrip-cdp.mjs",
  "scripts/test-hwp-studio.mjs",
  "scripts/html-sink-rule.mjs",
  "scripts/test-html-sink-rule.mjs",
  "scripts/sort-comparator-rule.mjs",
  "scripts/test-sort-comparator-rule.mjs",
  "scripts/release-serve-rule.mjs",
  "scripts/test-release-serve-rule.mjs",
  "scripts/test-webui-sheet-format.mjs",
  "scripts/test-webui-sheet-styles.mjs",
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


const IGNORED_DIRS = new Set([
  ".git",
  ".build",
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

// Root-relative paths excluded from the content and JS syntax scans (PD-076, PD-081). None of them is
// hand-written EasyOffice source, and each is pinned another way instead:
//  - third_party/rhwp: read-only upstream submodule (scripts/verify-rhwp-upstream.mjs),
//  - vendor/rhwp-core: prebuilt WASM package, size and SHA-256 per file (scripts/verify-rhwp-core.mjs),
//  - crates/easyoffice-app/ui/hwp-studio: generated editor bundle, never tracked (checked below) and
//    statically checked after every build (scripts/check-hwp-studio-bundle.mjs).
// Forbidden local-only files are still rejected anywhere by the tracked-file check.
const IGNORED_ROOT_PATHS = new Set(["third_party/rhwp", "vendor/rhwp-core", "crates/easyoffice-app/ui/hwp-studio"]);

const TEXT_EXTENSIONS = new Set([
  ".md",
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
  ".ts",
]);

/**
 * Candidate files are what Git would track: tracked files plus untracked files that are not ignored.
 * Ignored build output (target*, generated bundles, node_modules, Tauri gen/permissions) is therefore
 * never scanned, while a new file is checked before it is added. Returns null without a work tree.
 */
function listGitCandidateFiles() {
  try {
    const raw = execFileSync(resolveGitExecutable(), ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], {
      cwd: ROOT,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      stdio: ["ignore", "pipe", "ignore"],
    });
    const collected = [];
    for (const rel of new Set(raw.split("\0").filter(Boolean))) {
      const segments = rel.split("/");
      if (segments.some((s) => IGNORED_DIRS.has(s))) continue;
      if (IGNORED_ROOT_FILES.has(rel)) continue;
      if ([...IGNORED_ROOT_PATHS].some((p) => rel === p || rel.startsWith(`${p}/`))) continue;
      const full = path.join(ROOT, rel);
      let stat;
      try {
        stat = fs.statSync(full);
      } catch {
        continue; // tracked but deleted in the work tree
      }
      if (!stat.isFile()) continue; // gitlinks (submodules) are directories
      const ext = path.extname(rel);
      if (TEXT_EXTENSIONS.has(ext) || path.basename(rel) === "LICENSE") collected.push(full);
    }
    return collected;
  } catch {
    return null;
  }
}

function walkTrackedCandidateFiles(dir, collected = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (IGNORED_DIRS.has(entry.name)) continue;
    const fullPath = path.join(dir, entry.name);
    const relPath = path.relative(ROOT, fullPath).replaceAll("\\", "/");
    if (IGNORED_ROOT_FILES.has(relPath) || IGNORED_ROOT_PATHS.has(relPath)) continue;

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

/** Default sort order (UTF-16 code units) written as an explicit comparator. */
function compareCodeUnits(a, b) {
  if (a < b) return -1;
  return a > b ? 1 : 0;
}

const errors = [];

// PD-086: `--serve` is a developer/CI mode; the release workflow must not publish it.
{
  const releaseWorkflow = path.join(ROOT, ".github/workflows/release.yml");
  if (!fs.existsSync(releaseWorkflow)) {
    errors.push("Release workflow .github/workflows/release.yml is missing (cannot verify PD-086).");
  } else {
    for (const violation of findServeReleaseViolations(fs.readFileSync(releaseWorkflow, "utf8"))) {
      errors.push(`release.yml must not publish the --serve CLI (PD-086): ${violation}`);
    }
  }
}

for (const rel of REQUIRED_TRACKED_CONTRACTS) {
  const full = path.join(ROOT, rel);
  if (!fs.existsSync(full)) {
    errors.push(`Missing required repository file: ${rel}`);
  }
}

// Verify git tracked files never include local-only files
try {
  const trackedRaw = execFileSync(resolveGitExecutable(), ["ls-files"], {
    cwd: ROOT,
    encoding: "utf8",
  });
  const trackedList = trackedRaw
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);

  for (const tracked of trackedList) {
    for (const forbidden of FORBIDDEN_GIT_PATHS) {
      if (tracked === forbidden || tracked.startsWith(forbidden) || tracked.includes(`/${forbidden}`)) {
        errors.push(`Forbidden local-only file is tracked by Git: ${tracked}`);
      }
    }
  }
} catch {
  // Ignore if git is not initialized
}

for (const upstreamError of verifyUpstream(ROOT)) {
  errors.push(`rhwp upstream: ${upstreamError}`);
}

// PD-081: hwp-studio overlay inputs. The override manifest and the bundled fonts are checked here
// (structure and hashes; upstream baselines only when the sparse checkout exists); the generated
// editor bundle must never be committed.
try {
  for (const overlayError of verifyOverrides(ROOT, readConfig(ROOT))) errors.push(`hwp-studio overrides: ${overlayError}`);
} catch (err) {
  errors.push(`hwp-studio overrides: ${err.message}`);
}
for (const fontError of checkFonts(ROOT)) errors.push(`bundled fonts: ${fontError}`);
try {
  const generated = execFileSync(resolveGitExecutable(), ["ls-files", "--", "crates/easyoffice-app/ui/hwp-studio"], {
    cwd: ROOT,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  }).trim();
  if (generated) errors.push("crates/easyoffice-app/ui/hwp-studio is a build output and must not be tracked by Git");
} catch {
  // not a git work tree
}

const files = listGitCandidateFiles() ?? walkTrackedCandidateFiles(ROOT);
for (const file of files) {
  const rel = path.relative(ROOT, file).replaceAll("\\", "/");
  if (rel === "scripts/check-repo-hygiene.mjs") continue;

  const content = fs.readFileSync(file, "utf8");
  for (const regex of prohibitedPatternsIn(rel, content)) {
    errors.push(`Prohibited external brand or local-only reference detected in ${rel} (pattern: ${regex.source})`);
  }

  if (isSortRuleScoped(rel)) {
    for (const line of findBareSorts(content)) {
      errors.push(`.sort() without a comparator in ${rel}:${line} (pass an explicit compare function)`);
    }
  }

  if (rel.endsWith(".js") || rel.endsWith(".mjs")) {
    try {
      execFileSync(process.execPath, ["--check", file], {
        cwd: ROOT,
        stdio: "pipe",
      });
    } catch (err) {
      const stderr = err?.stderr ? err.stderr.toString("utf8").trim() : String(err);
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
    for (const violation of findHtmlSinkViolations(uiText)) {
      errors.push(`Prohibited HTML sink in ${uiRel} (${violation})`);
    }
    if (/\.setAttribute\(\s*["']style["']/i.test(uiText)) {
      errors.push(`Prohibited setAttribute("style", ...) detected in ${uiRel} (violates CSP default-src 'self')`);
    }
  }
}

// Sources under these roots stay below the 1,000-line ceiling (Rust and the hwp-studio overlay TypeScript).
const LINE_CEILING_ROOTS = [
  "crates/easyoffice-sheet/src/",
  "crates/easyoffice-formats/src/",
  "crates/easyoffice-layout/src/",
  "crates/easyoffice-app/src/",
  "crates/easyoffice-desktop/src/",
  "apps/hwp-studio/src/",
  "apps/quicklook/rust/src/",
];
const LINE_CEILING_EXTENSIONS = [".rs", ".ts"];

for (const file of files) {
  const rel = path.relative(ROOT, file).replaceAll("\\", "/");
  if (
    LINE_CEILING_ROOTS.some((root) => rel.startsWith(root)) &&
    LINE_CEILING_EXTENSIONS.some((extension) => rel.endsWith(extension))
  ) {
    const sourceText = fs.readFileSync(file, "utf8");
    const lineCount = sourceText.split("\n").length - (sourceText.endsWith("\n") ? 1 : 0);
    if (lineCount >= 1000) {
      errors.push(`Source file ${rel} exceeds 1,000-line ceiling (${lineCount} lines)`);
    }
  }
}

const DESKTOP_SHELL_MANIFEST = "crates/easyoffice-desktop/Cargo.toml";
const DESKTOP_SHELL_CONFIG = "crates/easyoffice-desktop/tauri.conf.json";
// PD-083: the embedded editor needs WebAssembly compilation (`wasm-unsafe-eval`, never `unsafe-eval`),
// `data:`/`blob:` images and a same-origin frame; there is still no inline script/style allowance.
const DESKTOP_SHELL_CSP = {
  "default-src": "'self'",
  "script-src": "'self' 'wasm-unsafe-eval'",
  "img-src": "'self' data: blob:",
  "frame-src": "'self'",
  "connect-src": "'self' ipc: http://ipc.localhost",
};
// The app commands live in one Rust list (shared by build.rs and the crate); the capability file may
// grant exactly those (PD-088: engine IPC, save, and the narrow window commands; none takes a path).
const APP_COMMANDS_FILE = "crates/easyoffice-desktop/src/app_commands.rs";
const appCommandsText = fs.existsSync(path.join(ROOT, APP_COMMANDS_FILE))
  ? fs.readFileSync(path.join(ROOT, APP_COMMANDS_FILE), "utf8")
  : "";
const DESKTOP_SHELL_PERMISSIONS = new Set(
  [...appCommandsText.matchAll(/^[ \t]*"([a-z_]+)",$/gm)].map((m) => `allow-${m[1].replaceAll("_", "-")}`)
);

// Desktop shell guard (PD-074): embedded assets only, the WebUI CSP plus the shell IPC origin,
// file associations limited to formats the window model opens, and capabilities limited to the
// app commands (no core/shell/fs/http/dialog permissions reach the WebView).
// Extensions `WindowKind::for_extension` opens (crates/easyoffice-desktop/src/windows.rs).
const DESKTOP_OPENABLE_EXTENSIONS = [
  "docx", "doc", "odt", "rtf", "md", "markdown", "txt", "hwp", "hwpx", "hwt", "hwtx",
  "xlsx", "xls", "ods", "csv", "tsv", "pptx", "ppt", "ppsx", "odp", "pdf",
];
const desktopConfigFull = path.join(ROOT, DESKTOP_SHELL_CONFIG);
if (fs.existsSync(desktopConfigFull)) {
  let conf = null;
  try {
    conf = JSON.parse(fs.readFileSync(desktopConfigFull, "utf8"));
  } catch (err) {
    errors.push(`${DESKTOP_SHELL_CONFIG} is not valid JSON: ${err.message}`);
  }
  if (conf) {
    const security = conf.app?.security || {};
    if (JSON.stringify(security.csp) !== JSON.stringify(DESKTOP_SHELL_CSP)) {
      errors.push(`${DESKTOP_SHELL_CONFIG} CSP must be exactly ${JSON.stringify(DESKTOP_SHELL_CSP)}`);
    }
    if (security.dangerousDisableAssetCspModification || conf.app?.withGlobalTauri) {
      errors.push(`${DESKTOP_SHELL_CONFIG} must not disable CSP injection or expose the global shell API`);
    }
    if (conf.build?.devUrl || conf.build?.frontendDist !== "../easyoffice-app/ui") {
      errors.push(`${DESKTOP_SHELL_CONFIG} must load only the embedded ../easyoffice-app/ui assets (no devUrl)`);
    }
    for (const problem of checkDesktopBundleConfig(conf)) {
      errors.push(`${DESKTOP_SHELL_CONFIG}: ${problem}`);
    }
    const openable = new Set(DESKTOP_OPENABLE_EXTENSIONS);
    for (const assoc of Array.isArray(conf.bundle?.fileAssociations) ? conf.bundle.fileAssociations : []) {
      for (const ext of Array.isArray(assoc?.ext) ? assoc.ext : []) {
        if (!openable.has(String(ext).toLowerCase())) {
          errors.push(`${DESKTOP_SHELL_CONFIG} registers a file association for '.${ext}', which the desktop window model cannot open`);
        }
      }
    }
  }
  const capDir = path.join(ROOT, "crates/easyoffice-desktop/capabilities");
  for (const name of fs.existsSync(capDir) ? fs.readdirSync(capDir) : []) {
    const capRel = `crates/easyoffice-desktop/capabilities/${name}`;
    let cap = null;
    try {
      cap = JSON.parse(fs.readFileSync(path.join(capDir, name), "utf8"));
    } catch (err) {
      errors.push(`${capRel} is not valid JSON: ${err.message}`);
      continue;
    }
    if (cap.remote) errors.push(`${capRel} must not grant IPC to remote URLs`);
    for (const perm of cap.permissions || []) {
      const id = typeof perm === "string" ? perm : perm?.identifier;
      if (!DESKTOP_SHELL_PERMISSIONS.has(id)) {
        errors.push(`${capRel} grants permission '${id}' beyond the EasyOffice app commands (${APP_COMMANDS_FILE})`);
      }
    }
    const granted = new Set((cap.permissions || []).map((perm) => (typeof perm === "string" ? perm : perm?.identifier)));
    for (const wanted of DESKTOP_SHELL_PERMISSIONS) {
      if (!granted.has(wanted)) errors.push(`${capRel} does not grant '${wanted}' although it is an app command`);
    }
    if (DESKTOP_SHELL_PERMISSIONS.size < 9) errors.push(`${APP_COMMANDS_FILE} must list the app commands (found ${DESKTOP_SHELL_PERMISSIONS.size})`);
  }
}
const POST_185_RUST_APIS = [".is_multiple_of(", ".as_chunks(", ".as_chunks::<", ".floor_char_boundary("];
for (const file of files) {
  const rel = path.relative(ROOT, file).replaceAll("\\", "/");
  if (rel.startsWith("crates/") && rel.endsWith("/Cargo.toml")) {
    const tomlText = fs.readFileSync(file, "utf8");
    if (rel === DESKTOP_SHELL_MANIFEST) {
      // The desktop shell's floor is set by its Tauri v2 dependencies, not by the engine crates;
      // it must declare that floor explicitly and may never go below the workspace MSRV (PD-074).
      const declared = tomlText.match(/^rust-version\s*=\s*"(\d+)\.(\d+)"/m);
      if (!declared || Number(declared[1]) !== 1 || Number(declared[2]) < 85) {
        errors.push(`Crate manifest ${rel} must declare an explicit rust-version >= 1.85 (Tauri toolchain floor).`);
      }
    } else if (!tomlText.includes("rust-version.workspace = true")) {
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

const formatsLibPath = path.join(ROOT, "crates/easyoffice-formats/src/lib.rs");
const rustSheetFunctionsPath = path.join(ROOT, "crates/easyoffice-sheet/src/functions.rs");
const ribbonCatalogJsPath = path.join(ROOT, "crates/easyoffice-app/ui/ribbon-catalog.js");
const rustIpcLibPath = path.join(ROOT, "crates/easyoffice-app/src/lib.rs");

if (
  fs.existsSync(formatsLibPath) &&
  fs.existsSync(rustSheetFunctionsPath) &&
  fs.existsSync(ribbonCatalogJsPath) &&
  fs.existsSync(rustIpcLibPath)
) {
  // Sources are read with LF line endings so the line-anchored counts below match a CRLF checkout
  // (Windows release runners) the same way they match on Linux and macOS.
  const readSourceLf = (file) => fs.readFileSync(file, "utf8").replaceAll("\r\n", "\n");

  // Read/write formats: the DocumentFormat variants minus the experimental-write formats (Pages).
  const formatsLib = readSourceLf(formatsLibPath);
  const enumBody = (formatsLib.split("pub enum DocumentFormat {")[1] || "").split("\n}")[0];
  const variantCount = enumBody.split("\n").filter((line) => /^\s+[A-Z][A-Za-z0-9]*,$/.test(line)).length;
  const readOnlyCount = Number((formatsLib.match(/EXPERIMENTAL_WRITE:\s*\[DocumentFormat;\s*(\d+)\]/) || [])[1] || 0);
  const formatCount = variantCount - readOnlyCount;

  const rustFnMatch = readSourceLf(rustSheetFunctionsPath).match(/CANONICAL_122_FUNCTIONS:\s*\[&str;\s*122\]\s*=\s*\[([\s\S]*?)\];/);
  const functionCount = rustFnMatch ? (rustFnMatch[1].match(/"[A-Z0-9]+"/g) || []).length : 0;

  const ribbonCatalogJs = readSourceLf(ribbonCatalogJsPath);
  const commonSpecCount = (ribbonCatalogJs.match(/\{\s*suffix:\s*"/g) || []).length;
  const modSpecSection = ribbonCatalogJs.split("function buildRibbonCatalog")[0].split("MODULE_SPECIFIC_SPECS")[1] || "";
  const modSpecCount = (modSpecSection.match(/\{\s*id:\s*"/g) || []).length;
  const ribbonCommandCount = commonSpecCount * 5 + modSpecCount;

  const rustIpcLib = readSourceLf(rustIpcLibPath);
  const dispatchBlock = (rustIpcLib.split("pub fn dispatch(")[1] || "").split("fn handle_workspace_bootstrap")[0] || "";
  const ipcCommandCount = dispatchBlock
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith('"') && line.includes("=>")).length;

  if (formatCount !== 20) {
    errors.push(`Expected 20 read/write DocumentFormat variants in crates/easyoffice-formats/src/lib.rs, found ${formatCount}`);
  }
  if (functionCount !== 122) {
    errors.push(`Expected 122 built-in functions in crates/easyoffice-sheet/src/functions.rs, found ${functionCount}`);
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
    ".msi",
    ".nsis",
  ];
  // Docs may name the shell configuration only while it actually exists (PD-074).
  if (!fs.existsSync(path.join(ROOT, DESKTOP_SHELL_CONFIG))) {
    STALE_DOC_TOKENS.push("tauri.conf.json");
  }
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
