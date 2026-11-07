// Tests for the hwp-studio overlay guards (PD-081): bundle static checks, override manifest and
// bundled font verification. Uses synthetic temporary directories; no browser, no network.
import test from "node:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { checkBundle, BUDGET_BYTES } from "./check-hwp-studio-bundle.mjs";
import { checkFonts } from "./verify-fonts.mjs";
import { checkManifestStructure, checkUpstreamBaseline, normalizedSha256, readManifest, verifyOverrides } from "./hwp-studio-overrides.mjs";
import { checkVendorCore } from "./verify-rhwp-core.mjs";
import { resolveGitExecutable } from "./verify-rhwp-upstream.mjs";
import { execFileSync, spawnSync } from "node:child_process";
import { stripTypeScriptTypes } from "node:module";
import { BEGIN, END, replaceSection, wasmDependencyClosure } from "./generate-wasm-notices.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
const CONFIG = JSON.parse(fs.readFileSync(path.join(REPO, "config/rhwp-upstream.json"), "utf8"));

function tmp(prefix) {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

function write(dir, rel, content) {
  const file = path.join(dir, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content);
}

const WASM_BYTES = Buffer.from([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0]);

function makeBundle(extra = {}) {
  const dir = tmp("eo-bundle-");
  write(dir, "index.html", '<!doctype html><html><head><script type="module" src="./assets/app.js"></script><link rel="stylesheet" href="./assets/app.css"></head><body></body></html>');
  write(dir, "assets/app.js", 'const ns = "http://www.w3.org/2000/svg"; export default ns;');
  write(dir, "assets/app.css", "body{margin:0}");
  write(dir, "assets/rhwp_bg-x.wasm", WASM_BYTES);
  for (const [rel, content] of Object.entries(extra)) write(dir, rel, content);
  const vendorWasm = path.join(dir, "..", `${path.basename(dir)}.wasm`);
  fs.writeFileSync(vendorWasm, WASM_BYTES);
  return { dir, vendorWasm };
}

function cleanup(...paths) {
  for (const p of paths) fs.rmSync(p, { recursive: true, force: true });
}

test("a clean synthetic bundle passes", () => {
  const { dir, vendorWasm } = makeBundle();
  try {
    const { errors, stats } = checkBundle(dir, { vendorWasm });
    assert.deepEqual(errors, []);
    assert.ok(stats.nonFontBytes > 0);
  } finally {
    cleanup(dir, vendorWasm);
  }
});

const FORBIDDEN_CASES = [
  ["external URL", { "assets/app.js": 'fetch("https://cdn.example.org/font.woff2")' }, /external URL/],
  ["eval", { "assets/app.js": "eval('1+1')" }, /eval\(\)/],
  ["new Function", { "assets/app.js": "const f = new Function('return 1')" }, /new Function/],
  ["Tauri reference", { "assets/app.js": "window.__TAURI_INTERNALS__" }, /__TAURI/],
  ["service worker", { "assets/app.js": "navigator.serviceWorker.register('/sw.js')" }, /service worker/],
  ["extension API", { "assets/app.js": "chrome.runtime.sendMessage({})" }, /extension API/],
  ["inline script", { "index.html": "<html><body><script>alert(1)</script></body></html>" }, /inline script/],
  ["inline style attribute", { "index.html": '<html><body><div style="color:red"></div></body></html>' }, /inline style attribute/],
  ["inline style element", { "index.html": "<html><body><style>a{}</style></body></html>" }, /inline <style>/],
  ["inline event handler", { "index.html": '<html><body><button onclick="x()"></button></body></html>' }, /inline event handler/],
  ["external script", { "index.html": '<html><body><script src="https://evil.example.org/a.js"></script></body></html>' }, /external/],
  ["service worker file", { "sw.js": "x" }, /forbidden file/],
  ["source map", { "assets/app.js.map": "{}" }, /forbidden file/],
];
for (const [name, files, pattern] of FORBIDDEN_CASES) {
  test(`bundle check rejects ${String(name)}`, () => {
    const { dir, vendorWasm } = makeBundle(files);
    try {
      const { errors } = checkBundle(dir, { vendorWasm });
      assert.ok(errors.some((e) => pattern.test(e)), errors.join("\n"));
    } finally {
      cleanup(dir, vendorWasm);
    }
  });
}

test("bundle check: eval-like identifiers and namespace URIs are not false positives", () => {
  const { dir, vendorWasm } = makeBundle({ "assets/app.js": "retrieval(1); medieval(2); obj.evaluate(3); const u = 'http://www.w3.org/1999/xlink';" });
  try {
    assert.deepEqual(checkBundle(dir, { vendorWasm }).errors, []);
  } finally {
    cleanup(dir, vendorWasm);
  }
});

test("bundle check requires the vendored wasm byte for byte", () => {
  const { dir, vendorWasm } = makeBundle();
  try {
    fs.writeFileSync(path.join(dir, "assets/rhwp_bg-x.wasm"), Buffer.concat([WASM_BYTES, Buffer.from([1])]));
    assert.ok(checkBundle(dir, { vendorWasm }).errors.some((e) => e.includes("differs from vendor")));
    fs.rmSync(path.join(dir, "assets/rhwp_bg-x.wasm"));
    assert.ok(checkBundle(dir, { vendorWasm }).errors.some((e) => e.includes("exactly one .wasm")));
  } finally {
    cleanup(dir, vendorWasm);
  }
});

test("bundle check enforces the size budget without counting fonts", () => {
  const { dir, vendorWasm } = makeBundle({ "fonts/big.woff2": Buffer.alloc(2048) });
  try {
    assert.deepEqual(checkBundle(dir, { vendorWasm, budget: 4096 }).errors, []);
    assert.ok(checkBundle(dir, { vendorWasm, budget: 50 }).errors.some((e) => e.includes("budget")));
    assert.ok(BUDGET_BYTES <= 20 * 1024 * 1024);
  } finally {
    cleanup(dir, vendorWasm);
  }
});

test("bundle check verifies the asset manifest and referenced files", () => {
  const { dir, vendorWasm } = makeBundle();
  try {
    const files = ["index.html", "assets/app.js", "assets/app.css", "assets/rhwp_bg-x.wasm"].map((rel) => {
      const data = fs.readFileSync(path.join(dir, rel));
      return { path: rel, bytes: data.length, sha256: crypto.createHash("sha256").update(data).digest("hex") };
    });
    write(dir, "asset-manifest.json", JSON.stringify({ schemaVersion: 1, files }));
    assert.deepEqual(checkBundle(dir, { vendorWasm }).errors, []);
    fs.appendFileSync(path.join(dir, "assets/app.css"), "a{}");
    assert.ok(checkBundle(dir, { vendorWasm }).errors.some((e) => e.includes("differs from asset-manifest.json")));
    write(dir, "assets/extra.js", "1");
    assert.ok(checkBundle(dir, { vendorWasm }).errors.some((e) => e.includes("not listed")));
    write(dir, "index.html", '<html><body><script type="module" src="./assets/missing.js"></script></body></html>');
    assert.ok(checkBundle(dir, { vendorWasm }).errors.some((e) => e.includes("references missing file")));
  } finally {
    cleanup(dir, vendorWasm);
  }
});

test("a missing bundle is reported", () => {
  const dir = tmp("eo-empty-");
  try {
    assert.ok(checkBundle(dir).errors[0].includes("index.html is missing"));
  } finally {
    cleanup(dir);
  }
});

// ---- override manifest ------------------------------------------------------------------------

function makeOverlay() {
  const dir = tmp("eo-overlay-");
  const manifest = {
    schemaVersion: 1,
    upstream: { version: CONFIG.version, commit: CONFIG.commit, counterparts: { "core/font-loader": "a".repeat(64) }, assets: { "index.html": "b".repeat(64) } },
    overrides: [{ id: "core/font-loader", strategy: "fork", reason: "test" }],
    assetOverrides: [{ path: "index.html", upstream: "index.html", strategy: "fork", reason: "test" }],
    verbatimAssets: [],
  };
  write(dir, "apps/hwp-studio/src/core/font-loader.ts", "export {};");
  write(dir, "apps/hwp-studio/index.html", "<html></html>");
  write(dir, "apps/hwp-studio/src/style.css", "");
  return { dir, manifest };
}

test("the real override manifest is consistent with the overlay", () => {
  assert.deepEqual(checkManifestStructure(REPO, readManifest(REPO)), []);
  assert.deepEqual(verifyOverrides(REPO, CONFIG), []);
});

test("manifest structure: unlisted overlay files, missing files and bad entries fail", () => {
  const { dir, manifest } = makeOverlay();
  try {
    assert.deepEqual(checkManifestStructure(dir, manifest), []);
    write(dir, "apps/hwp-studio/src/core/extra.ts", "export {};");
    assert.ok(checkManifestStructure(dir, manifest).some((e) => e.includes("extra.ts is not listed")));
    fs.rmSync(path.join(dir, "apps/hwp-studio/src/core/extra.ts"));
    fs.rmSync(path.join(dir, "apps/hwp-studio/src/core/font-loader.ts"));
    assert.ok(checkManifestStructure(dir, manifest).some((e) => e.includes("has no overlay file")));
    write(dir, "apps/hwp-studio/src/core/font-loader.ts", "export {};");
    const bad = structuredClone(manifest);
    bad.overrides[0].strategy = "rewrite";
    assert.ok(checkManifestStructure(dir, bad).some((e) => e.includes("invalid strategy")));
    bad.overrides[0] = { id: "../escape", strategy: "fork", reason: "x" };
    assert.ok(checkManifestStructure(dir, bad).some((e) => e.includes("invalid or duplicate")));
    const noReason = structuredClone(manifest);
    noReason.overrides[0].reason = " ";
    assert.ok(checkManifestStructure(dir, noReason).some((e) => e.includes("needs a reason")));
  } finally {
    cleanup(dir);
  }
});

test("upstream baseline: pin mismatch and missing counterpart hashes fail", () => {
  const { manifest, dir } = makeOverlay();
  try {
    assert.deepEqual(checkUpstreamBaseline(dir, manifest, CONFIG), []);
    const wrongPin = structuredClone(manifest);
    wrongPin.upstream.commit = "0".repeat(40);
    assert.ok(checkUpstreamBaseline(dir, wrongPin, CONFIG).some((e) => e.includes("differ from config")));
    const missing = structuredClone(manifest);
    delete missing.upstream.counterparts["core/font-loader"];
    assert.ok(checkUpstreamBaseline(dir, missing, CONFIG).some((e) => e.includes("missing 'core/font-loader'")));
  } finally {
    cleanup(dir);
  }
});

test("upstream baseline hashes line endings normalised and detect drift when checked out", { skip: !fs.existsSync(path.join(REPO, "third_party/rhwp/.git")) }, () => {
  const manifest = readManifest(REPO);
  assert.deepEqual(checkUpstreamBaseline(REPO, manifest, CONFIG), []);
  const drifted = structuredClone(manifest);
  drifted.upstream.counterparts["core/font-loader"] = "0".repeat(64);
  assert.ok(checkUpstreamBaseline(REPO, drifted, CONFIG).some((e) => e.includes("changed since its override was written")));
  const sample = path.join(REPO, "third_party/rhwp/rhwp-studio/src/core/font-loader.ts");
  assert.equal(normalizedSha256(sample), manifest.upstream.counterparts["core/font-loader"]);
});

// ---- fonts ------------------------------------------------------------------------------------

const OFL_TEXT = "Copyright 2020 Test\n\nSIL OPEN FONT LICENSE\nVersion 1.1 - 26 February 2007\n";

function makeFonts() {
  const root = tmp("eo-fonts-");
  const data = Buffer.from("font-bytes");
  write(root, "assets/fonts/Test-Regular.woff2", data);
  write(root, "assets/fonts/licenses/Test-OFL.txt", OFL_TEXT);
  write(root, "assets/fonts/FONTS.md", "| Test-Regular.woff2 |\n");
  const manifest = {
    schemaVersion: 1,
    fonts: [{ file: "Test-Regular.woff2", license: "OFL-1.1", licenseFile: "licenses/Test-OFL.txt", bytes: data.length, sha256: crypto.createHash("sha256").update(data).digest("hex") }],
  };
  write(root, "assets/fonts/fonts.json", JSON.stringify(manifest));
  return { root, manifest };
}

test("the bundled fonts are all OFL 1.1 and match their hashes", () => {
  assert.deepEqual(checkFonts(REPO), []);
  const manifest = JSON.parse(fs.readFileSync(path.join(REPO, "assets/fonts/fonts.json"), "utf8"));
  assert.ok(manifest.fonts.length >= 20);
  assert.ok(manifest.fonts.every((f) => f.license === "OFL-1.1"));
  assert.ok(!manifest.fonts.some((f) => /Cafe24|Happiness|LatinModern|SourceHan|hamchob|Arial|Malgun/i.test(f.file)));
});

test("font guard: unlisted, tampered, non-OFL and undocumented fonts fail", () => {
  const { root, manifest } = makeFonts();
  try {
    assert.deepEqual(checkFonts(root), []);
    write(root, "assets/fonts/Other-Regular.woff2", "x");
    assert.ok(checkFonts(root).some((e) => e.includes("Other-Regular.woff2 is not listed")));
    fs.rmSync(path.join(root, "assets/fonts/Other-Regular.woff2"));
    fs.appendFileSync(path.join(root, "assets/fonts/Test-Regular.woff2"), "z");
    assert.ok(checkFonts(root).some((e) => e.includes("bytes")));
    write(root, "assets/fonts/Test-Regular.woff2", "font-bytez");
    assert.ok(checkFonts(root).some((e) => e.includes("SHA-256")));
    write(root, "assets/fonts/Test-Regular.woff2", "font-bytes");
    write(root, "assets/fonts/licenses/Test-OFL.txt", "All rights reserved. Free for personal use.");
    assert.ok(checkFonts(root).some((e) => e.includes("not the SIL Open Font License")));
    write(root, "assets/fonts/licenses/Test-OFL.txt", OFL_TEXT);
    write(root, "assets/fonts/FONTS.md", "nothing\n");
    assert.ok(checkFonts(root).some((e) => e.includes("FONTS.md does not list")));
    write(root, "assets/fonts/FONTS.md", "| Test-Regular.woff2 |\n");
    const custom = structuredClone(manifest);
    custom.fonts[0].license = "custom-free";
    write(root, "assets/fonts/fonts.json", JSON.stringify(custom));
    assert.ok(checkFonts(root).some((e) => e.includes("must be OFL-1.1")));
    const escape = structuredClone(manifest);
    escape.fonts[0].licenseFile = "../../outside.txt";
    write(root, "assets/fonts/fonts.json", JSON.stringify(escape));
    assert.ok(checkFonts(root).some((e) => e.includes("licenseFile must be under licenses/")));
  } finally {
    cleanup(root);
  }
});

test("font guard: a reserved font name requires the official file or WOFF2 compression of it (PD-092)", () => {
  const { root, manifest } = makeFonts();
  const rfnText = "Copyright 2020 Test, with Reserved Font Name Test Sans.\n\nSIL OPEN FONT LICENSE\nVersion 1.1 - 26 February 2007\n";
  const withUpstream = (up) => {
    const m = structuredClone(manifest);
    if (up !== undefined) m.fonts[0].upstream = up;
    write(root, "assets/fonts/fonts.json", JSON.stringify(m));
    return checkFonts(root);
  };
  try {
    write(root, "assets/fonts/licenses/Test-OFL.txt", rfnText);
    assert.ok(withUpstream(undefined).some((e) => e.includes("upstream (official source URL")), "missing provenance fails");
    const sha = manifest.fonts[0].sha256;
    assert.deepEqual(withUpstream({ url: "https://example.org/Test.woff2", sha256: sha, conversion: "none" }), []);
    assert.ok(withUpstream({ url: "https://example.org/Test.woff2", sha256: "0".repeat(64), conversion: "none" }).some((e) => e.includes("differs from the official file")));
    assert.deepEqual(withUpstream({ url: "https://example.org/Test.ttf", sha256: "0".repeat(64), conversion: "woff2-compression" }), []);
    assert.ok(withUpstream({ url: "https://example.org/Test.ttf", sha256: sha, conversion: "subset" }).some((e) => e.includes("conversion must be")));
    assert.ok(withUpstream({ url: "http://example.org/Test.ttf", sha256: sha, conversion: "none" }).some((e) => e.includes("https URL")));
    const waived = structuredClone(manifest);
    waived.fonts[0].reservedNameNotUsed = "The reserved name is not used by this family.";
    write(root, "assets/fonts/fonts.json", JSON.stringify(waived));
    assert.deepEqual(checkFonts(root), []);
  } finally {
    cleanup(root);
  }
});

test("vendored WASM check is wired for the real repository", () => {
  assert.deepEqual(checkVendorCore(REPO, CONFIG), []);
});

// ---- repository hygiene integration -----------------------------------------------------------

function makeGitRepo() {
  const dir = tmp("eo-hygiene-");
  execFileSync(resolveGitExecutable(), ["init", "-q"], { cwd: dir });
  write(dir, ".gitignore", "/target*/\ncrates/easyoffice-desktop/gen/\ncrates/easyoffice-desktop/permissions/\napps/hwp-studio/node_modules/\ncrates/easyoffice-app/ui/hwp-studio/\n");
  return dir;
}

function runHygiene(dir) {
  return spawnSync(process.execPath, [path.join(REPO, "scripts/check-repo-hygiene.mjs")], { cwd: dir, encoding: "utf8" });
}

const BANNED = Buffer.from("c3ViYWdlbnQ=", "base64").toString("utf8");
const BROKEN_JS = `const = ;\n// ${BANNED}\n`;

test("hygiene skips ignored build output but still scans other untracked files", () => {
  const dir = makeGitRepo();
  try {
    for (const rel of [
      "target-wasm/debug/build.js",
      "crates/easyoffice-desktop/permissions/schema.js",
      "crates/easyoffice-desktop/gen/schemas/x.js",
      "apps/hwp-studio/node_modules/pkg/index.js",
      "crates/easyoffice-app/ui/hwp-studio/assets/app.js",
    ]) write(dir, rel, BROKEN_JS);
    write(dir, "tools-extra/broken.js", BROKEN_JS);
    const res = runHygiene(dir);
    assert.notEqual(res.status, 0);
    assert.match(res.stderr, /tools-extra\/broken\.js/);
    for (const ignored of ["target-wasm", "permissions/schema", "gen/schemas", "node_modules/pkg", "ui/hwp-studio/assets"]) {
      assert.doesNotMatch(res.stderr, new RegExp(ignored.replaceAll("/", String.raw`\/`)));
    }
  } finally {
    cleanup(dir);
  }
});

test("hygiene rejects a tracked generated editor bundle", () => {
  const dir = makeGitRepo();
  try {
    write(dir, "crates/easyoffice-app/ui/hwp-studio/index.html", "<html></html>");
    execFileSync(resolveGitExecutable(), ["add", "-f", "crates/easyoffice-app/ui/hwp-studio/index.html"], { cwd: dir });
    const res = runHygiene(dir);
    assert.match(res.stderr, /ui\/hwp-studio is a build output and must not be tracked/);
  } finally {
    cleanup(dir);
  }
});

test("hygiene scans overlay TypeScript for prohibited references", () => {
  const dir = makeGitRepo();
  try {
    write(dir, "apps/hwp-studio/src/note.ts", `// ${BANNED} notes\n`);
    const res = runHygiene(dir);
    assert.match(res.stderr, /apps\/hwp-studio\/src\/note\.ts/);
  } finally {
    cleanup(dir);
  }
});
// ---- third-party notices ----------------------------------------------------------------------

test("notices list every bundled font family, the bundled npm package and the generated crate section", () => {
  const notices = fs.readFileSync(path.join(REPO, "THIRD_PARTY_NOTICES.md"), "utf8");
  const fonts = JSON.parse(fs.readFileSync(path.join(REPO, "assets/fonts/fonts.json"), "utf8")).fonts;
  for (const family of new Set(fonts.map((f) => f.family))) assert.ok(notices.includes(`| ${family} |`), family);
  assert.ok(notices.includes("@noble/hashes"));
  const start = notices.indexOf(BEGIN);
  const end = notices.indexOf(END);
  assert.ok(start > 0 && end > start);
  const section = notices.slice(start, end);
  assert.match(section, /(?<!\d)\d+ third-party crates are linked into the module/);
  assert.ok(section.includes("| wasm-bindgen |"));
  assert.ok(section.includes("Permission is hereby granted, free of charge"));
  assert.ok(section.includes("Apache License"));
});

test("wasm dependency closure follows normal edges only and section replacement keeps the rest", () => {
  const pkg = (name, source) => ({ id: name, name, version: "1.0.0", source, license: "MIT", manifest_path: path.join(os.tmpdir(), "missing", name, "Cargo.toml") });
  const metadata = {
    packages: [pkg("rhwp", null), pkg("normal", "registry"), pkg("buildonly", "registry"), pkg("devonly", "registry"), pkg("transitive", "registry")],
    resolve: {
      nodes: [
        { id: "rhwp", deps: [
          { pkg: "normal", dep_kinds: [{ kind: null }] },
          { pkg: "buildonly", dep_kinds: [{ kind: "build" }] },
          { pkg: "devonly", dep_kinds: [{ kind: "dev" }] },
        ] },
        { id: "normal", deps: [{ pkg: "transitive", dep_kinds: [{ kind: null }] }] },
        { id: "buildonly", deps: [] }, { id: "devonly", deps: [] }, { id: "transitive", deps: [] },
      ],
    },
  };
  assert.deepEqual(wasmDependencyClosure(metadata).map((p) => p.name), ["normal", "rhwp", "transitive"]);
  assert.equal(replaceSection(`head\n${BEGIN}\nold\n${END}\ntail`, "new"), `head\n${BEGIN}\n\nnew\n${END}\ntail`);
  assert.throws(() => replaceSection("no markers", "x"));
});
// ---- embed parent origin ------------------------------------------------------------------------

test("the editor accepts the desktop shell's parent origin on every platform (PD-094)", async () => {
  const { isUsableParentOrigin } = await import("../apps/hwp-studio/src/embed/protocol.ts");
  for (const origin of ["http://tauri.localhost", "tauri://localhost", "http://127.0.0.1:18787", "https://example.org"]) {
    assert.equal(isUsableParentOrigin(origin), true, origin);
  }
  for (const origin of ["", "null", "file:///tmp/x.html", "data:text/html,x", "javascript:alert(1)", "not a url"]) {
    assert.equal(isUsableParentOrigin(origin), false, origin);
  }
});

// ---- print page rules -------------------------------------------------------------------------

test("print styles carry an @page size rule and a page box per page (#173)", async () => {
  const { buildPrintPageRules, createPrintPage } = await import("../apps/hwp-studio/src/command/print-pages.ts");
  const A4_PORTRAIT_PX = { width: 793.7, height: 1122.5 };
  const B5_LANDSCAPE_PX = { width: 944.882, height: 665.197 };
  const pages = [createPrintPage("<svg/>", A4_PORTRAIT_PX, 0), createPrintPage("<svg/>", B5_LANDSCAPE_PX, 1)];
  const rules = buildPrintPageRules(pages);
  assert.match(rules, /@page rhwp-print-page-1 \{ size: 210mm 297mm; margin: 0; \}/);
  assert.match(rules, /@page rhwp-print-page-2 \{ size: 250mm 176mm; margin: 0; \}/);
  assert.match(rules, /\.rhwp-print-page-1 \{ page: rhwp-print-page-1; width: 210mm; height: 297mm; \}/);
  assert.match(rules, /\.rhwp-print-page-2 \{ page: rhwp-print-page-2; width: 250mm; height: 176mm; \}/);
  assert.equal(buildPrintPageRules([]).trim(), "", "no pages, no rules");
});

// ---- password-protected HWP (PD-100) ---------------------------------------------------------
// The overlay's open funnel (apps/hwp-studio/src/main.ts) already tries a plain load first and only
// asks for a password after the engine reports an encrypted document. These tests run the real
// flow code (sliced out of main.ts, types stripped) with a mock engine and dialog, and guard that
// the password never reaches the shell, the embed port, storage or logs.

const OVERLAY_MAIN = fs.readFileSync(path.join(REPO, "apps/hwp-studio/src/main.ts"), "utf8");
const PASSWORD_REQUIRED = "비밀번호가 필요한 암호 문서";
const PASSWORD_REJECTED = "비밀번호가 일치하지 않거나 암호화 데이터가 손상되었습니다";

function openFlowSource() {
  const start = OVERLAY_MAIN.indexOf("class DocumentOpenCancelledError");
  const end = OVERLAY_MAIN.indexOf("/**\n * 문서 열기가 실패하면 빈 쪽 상태로");
  assert.ok(start >= 0 && end > start, "the open-flow markers exist");
  return stripTypeScriptTypes(OVERLAY_MAIN.slice(start, end));
}

// The sliced flow becomes a module whose factory receives the mock engine and dialog, so the real
// code runs unchanged without evaluating strings.
const OPEN_FLOW_MODULE = (() => {
  const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "eo-open-flow-")), "open-flow.mjs");
  fs.writeFileSync(
    file,
    `export function makeOpenFlow(wasm, showHwpPasswordDialog) {\n${openFlowSource()}\nreturn { open: loadDocumentForOpen, Cancelled: DocumentOpenCancelledError };\n}\n`
  );
  return import(pathToFileURL(file).href);
})();

async function createOpenFlow({ loadDocument, loadDocumentWithPassword, dialog }) {
  const wasm = { loadDocument, loadDocumentWithPassword, getDocumentInfo: () => ({ pageCount: 1 }) };
  const { makeOpenFlow } = await OPEN_FLOW_MODULE;
  return makeOpenFlow(wasm, dialog);
}

const needsPassword = () => {
  throw new Error(PASSWORD_REQUIRED);
};

test("a document without a password opens without any dialog", async () => {
  let asked = 0;
  const flow = await createOpenFlow({
    loadDocument: () => ({ pageCount: 3 }),
    loadDocumentWithPassword: () => assert.fail("no password open"),
    dialog: async () => {
      asked += 1;
      return null;
    },
  });
  assert.deepEqual(await flow.open(new Uint8Array(1), "a.hwp"), { pageCount: 3 });
  assert.equal(asked, 0);
});

test("other load errors are not turned into a password prompt", async () => {
  const flow = await createOpenFlow({
    loadDocument: () => {
      throw new Error("손상된 파일");
    },
    loadDocumentWithPassword: () => assert.fail("no password open"),
    dialog: async () => assert.fail("no dialog"),
  });
  await assert.rejects(flow.open(new Uint8Array(1), "a.hwp"), /손상된 파일/);
});

test("a correct password opens the document; the dialog only gets the file name", async () => {
  const dialogCalls = [];
  const opened = [];
  const flow = await createOpenFlow({
    loadDocument: needsPassword,
    loadDocumentWithPassword: (data, password, name) => opened.push([data.length, password, name]),
    dialog: async (...args) => {
      dialogCalls.push(args);
      return "s3cret";
    },
  });
  assert.deepEqual(await flow.open(new Uint8Array(4), "암호.hwp"), { pageCount: 1 });
  assert.deepEqual(dialogCalls, [["암호.hwp", undefined]]);
  assert.deepEqual(opened, [[4, "s3cret", "암호.hwp"]]);
});

test("a wrong password asks again with the retry text, then a right one opens", async () => {
  const messages = [];
  const answers = ["wrong", "right"];
  const flow = await createOpenFlow({
    loadDocument: needsPassword,
    loadDocumentWithPassword: (_data, password) => {
      if (password === "wrong") throw new Error(PASSWORD_REJECTED);
    },
    dialog: async (_name, message) => {
      messages.push(message);
      return answers.shift();
    },
  });
  assert.deepEqual(await flow.open(new Uint8Array(1), "a.hwp"), { pageCount: 1 });
  assert.deepEqual(messages, [undefined, "암호가 일치하지 않거나 문서가 손상되었습니다. 다시 입력하세요."]);
});

test("cancelling the dialog cancels the open without trying a password", async () => {
  const flow = await createOpenFlow({
    loadDocument: needsPassword,
    loadDocumentWithPassword: () => assert.fail("no password open"),
    dialog: async () => null,
  });
  await assert.rejects(flow.open(new Uint8Array(1), "a.hwp"), (error) => error.name === "DocumentOpenCancelledError");
});

test("unsupported encryption, DRM and corrupt data end with fixed Korean errors that never echo the password", async () => {
  const cases = [
    ["지원하지 않는 암호화 방식 (AES)", /지원하지 않는 암호화 방식의 문서입니다/],
    ["DRM 보호 문서", /DRM으로 보호된 문서는 지원하지 않습니다/],
    ["내부 오류 hunter2", /암호화된 문서를 열 수 없습니다/],
  ];
  const attempt = async ([engineMessage, expected]) => {
    const flow = await createOpenFlow({
      loadDocument: needsPassword,
      loadDocumentWithPassword: () => {
        throw new Error(engineMessage);
      },
      dialog: async () => "hunter2",
    });
    await assert.rejects(flow.open(new Uint8Array(1), "a.hwp"), (error) => expected.test(error.message) && !error.message.includes("hunter2"));
  };
  await Promise.all(cases.map(attempt));
});

test("the password stays inside the open flow: no storage, logs, embed port or shell command carries it", () => {
  const start = OVERLAY_MAIN.indexOf("async function openPasswordProtectedDocument");
  const flow = OVERLAY_MAIN.slice(start, OVERLAY_MAIN.indexOf("async function loadDocumentForOpen"));
  assert.match(flow, /password = '';/, "the local reference is cleared after each attempt");
  assert.doesNotMatch(flow, /localStorage|sessionStorage|addRecentDoc|autosave|console\.|postMessage|emit\(/);
  const outside = [
    "apps/hwp-studio/src/embed/protocol.ts",
    "apps/hwp-studio/src/embed/rpc-router.ts",
    "apps/hwp-studio/src/embed/shell-menu.ts",
    "apps/hwp-studio/src/embed/shell-shortcuts.ts",
    "crates/easyoffice-app/ui/hwp-host.js",
    "crates/easyoffice-app/ui/hwp-session.js",
    "crates/easyoffice-desktop/src/main.rs",
  ];
  for (const rel of outside) {
    assert.doesNotMatch(fs.readFileSync(path.join(REPO, rel), "utf8"), /openWithPassword|loadDocumentWithPassword|hwp-password/, rel);
  }
});

test("the pinned engine exports the password open", () => {
  assert.match(fs.readFileSync(path.join(REPO, "vendor/rhwp-core/rhwp.d.ts"), "utf8"), /static openWithPassword\(data: Uint8Array, password: string\): HwpDocument/);
});

const BUILT_ASSETS = path.join(REPO, "crates/easyoffice-app/ui/hwp-studio/assets");

// The editor bundle is build output (not tracked); CI jobs that do not build it skip this check.
test("the built bundle carries the password flow", { skip: !fs.existsSync(BUILT_ASSETS) }, () => {
  const assets = BUILT_ASSETS;
  const bundle = fs.readdirSync(assets).filter((f) => f.endsWith(".js")).map((f) => fs.readFileSync(path.join(assets, f), "utf8")).join("\n");
  assert.ok(bundle.includes(PASSWORD_REQUIRED) && bundle.includes(PASSWORD_REJECTED), "the bundle contains the password flow");
});

// ---- print batches (print-pages-v1, PD-104) -----------------------------------------------------

test("print batches carry the paper size in mm, clamp at the document end and reject bad ranges", async () => {
  const { parsePrintBatchRange, renderPrintBatch, PRINT_BATCH_MAX_PAGES } = await import("../apps/hwp-studio/src/embed/print-batch.ts");
  const sizes = [{ width: 793.7, height: 1122.5 }, { width: 944.882, height: 665.197 }, { width: 793.7, height: 1122.5 }];
  const calls = [];
  const source = {
    pageCount: sizes.length,
    withPortableMetrics: (op) => { calls.push("metrics"); return op(); },
    renderPageSvgWithProfile: (page, profile) => `<svg data-page="${page}" data-profile="${profile}"/>`,
    getPageInfo: (page) => sizes[page],
  };
  const first = renderPrintBatch(source, 0, 2);
  assert.equal(first.pageCount, 3);
  assert.deepEqual(first.pages.map((p) => [p.widthMm, p.heightMm]), [[210, 297], [250, 176]]);
  assert.match(first.pages[0].svg, /data-profile="print"/);
  const last = renderPrintBatch(source, 2, PRINT_BATCH_MAX_PAGES);
  assert.equal(last.pages.length, 1, "the batch stops at the last page");
  assert.deepEqual(calls, ["metrics", "metrics"], "one portable-metrics transaction per batch");
  assert.throws(() => renderPrintBatch(source, 3, 1), /out of range/);
  assert.deepEqual(parsePrintBatchRange({ from: 4, count: 10 }), { from: 4, count: 10 });
  for (const bad of [{ from: -1 }, { from: 1.5 }, { count: 0 }, { count: PRINT_BATCH_MAX_PAGES + 1 }, { count: "3" }, { extra: 1 }]) {
    assert.throws(() => parsePrintBatchRange(bad), undefined, JSON.stringify(bad));
  }
});

test("the embed router routes getPrintPages and the editor advertises print-pages-v1", async () => {
  // The router imports the editor's path aliases, which plain Node cannot resolve: check its source.
  const router = fs.readFileSync(path.join(REPO, "apps/hwp-studio/src/embed/rpc-router.ts"), "utf8");
  assert.match(router, /case 'getPrintPages': \{\s+const range = parsePrintBatchRange\(params\);\s+return handlers\.getPrintPages\(range\.from, range\.count\);/);
  const main = fs.readFileSync(path.join(REPO, "apps/hwp-studio/src/main.ts"), "utf8");
  assert.match(main, /async getPrintPages\(from, count\) \{[\s\S]{0,200}renderPrintBatch\(wasm, from, count\)/);
  const { EMBED_CAPABILITIES } = await import("../apps/hwp-studio/src/embed/protocol.ts");
  assert.ok(EMBED_CAPABILITIES.includes("print-pages-v1"));
  const menu = fs.readFileSync(path.join(REPO, "apps/hwp-studio/index.html"), "utf8");
  assert.ok(menu.includes('data-shell-cmd="print"'), "the 파일 menu lists printing");
});

// ---- toolbar labels toggle (PD-105) -------------------------------------------------------------

function fakeLabelsHost({ stored = null, failRead = false, failWrite = false } = {}) {
  const store = new Map(stored === null ? [] : [["toolbar-labels", stored]]);
  const item = { active: null, attrs: {}, classList: { toggle(name, force) { item.active = force; } }, setAttribute(k, v) { item.attrs[k] = v; } };
  return {
    store, item,
    root: { dataset: {} },
    menuItems: () => [item],
    storage: () => ({
      getItem: (k) => {
        if (failRead) {
          throw new Error("denied");
        }
        return store.get(k) ?? null;
      },
      setItem: (k, v) => {
        if (failWrite) {
          throw new Error("full");
        }
        store.set(k, v);
      },
    }),
  };
}

test("the toolbar labels toggle round-trips through storage and defaults to shown", async () => {
  const { applyToolbarLabels, readToolbarLabels, toggleToolbarLabels } = await import("../apps/hwp-studio/src/ui/toolbar-labels.ts");
  const host = fakeLabelsHost();
  assert.equal(readToolbarLabels(host), true, "nothing stored: labels are shown");
  applyToolbarLabels(readToolbarLabels(host), host);
  assert.equal(host.root.dataset.toolbarLabels, "shown");
  assert.equal(host.item.attrs["aria-checked"], "true");
  assert.equal(toggleToolbarLabels(host), false);
  assert.equal(host.root.dataset.toolbarLabels, "hidden");
  assert.equal(host.store.get("toolbar-labels"), "hidden");
  assert.deepEqual([host.item.active, host.item.attrs["aria-checked"]], [false, "false"]);
  assert.equal(readToolbarLabels(host), false, "the stored choice is read back");
  assert.equal(toggleToolbarLabels(host), true);
  assert.equal(host.store.get("toolbar-labels"), "shown");
});

test("unreadable or unwritable storage falls back to shown and the toggle still works", async () => {
  const { readToolbarLabels, toggleToolbarLabels } = await import("../apps/hwp-studio/src/ui/toolbar-labels.ts");
  assert.equal(readToolbarLabels(fakeLabelsHost({ failRead: true })), true);
  assert.equal(readToolbarLabels(fakeLabelsHost({ stored: "garbage" })), true, "only the exact value hidden hides the labels");
  const host = fakeLabelsHost({ failWrite: true });
  assert.equal(toggleToolbarLabels(host), false, "the first toggle hides (the page starts as shown)");
  assert.equal(host.root.dataset.toolbarLabels, "hidden", "the window follows the choice although it could not be saved");
});

test("the toolbar labels item, command, CSS and manifest agree", () => {
  const html = fs.readFileSync(path.join(REPO, "apps/hwp-studio/index.html"), "utf8");
  assert.match(html, /role="menuitemcheckbox"[^>]*data-cmd="view:toolbar-labels"/);
  const main = fs.readFileSync(path.join(REPO, "apps/hwp-studio/src/main.ts"), "utf8");
  assert.ok(main.includes("registry.register(toolbarLabelsCommand);") && main.includes("initToolbarLabels();"));
  const css = fs.readFileSync(path.join(REPO, "apps/hwp-studio/src/eo-overrides.css"), "utf8");
  const block = css.slice(css.indexOf(':root[data-toolbar-labels="hidden"] #icon-toolbar .tb-label'));
  assert.ok(block.includes("#style-bar .sb-ribbon-label") && block.includes("height: 32px"));
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|rgb/.test(block), "the rules use no colours");
  const manifest = fs.readFileSync(path.join(REPO, "config/rhwp-studio-overrides.json"), "utf8");
  assert.ok(manifest.includes("view:toolbar-labels") && manifest.includes("src/ui/toolbar-labels.ts"));
  assert.ok(fs.readFileSync(path.join(REPO, "THIRD_PARTY_NOTICES.md"), "utf8").includes("src/ui/toolbar-labels.ts"));
});
