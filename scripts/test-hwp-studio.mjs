// Tests for the hwp-studio overlay guards (PD-081): bundle static checks, override manifest and
// bundled font verification. Uses synthetic temporary directories; no browser, no network.
import test from "node:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { checkBundle, BUDGET_BYTES } from "./check-hwp-studio-bundle.mjs";
import { checkFonts } from "./verify-fonts.mjs";
import { checkManifestStructure, checkUpstreamBaseline, normalizedSha256, readManifest, verifyOverrides } from "./hwp-studio-overrides.mjs";
import { checkVendorCore } from "./verify-rhwp-core.mjs";
import { resolveGitExecutable } from "./verify-rhwp-upstream.mjs";
import { execFileSync, spawnSync } from "node:child_process";
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
  test(`bundle check rejects ${name}`, () => {
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
