import assert from "node:assert/strict";
import fs from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { REQUIRED_UTIS, bundledContentTypes } from "./desktop-file-associations.mjs";
import {
  EXTENSIONS, MIN_MACOS, byCodeUnits, appVersion, bundleFilesEntries, entitlements, ffiHeader, quicklookRoot, repoRoot, rustRoot,
} from "./quicklook-config.mjs";
import { checkExtensionInfo } from "./verify-quicklook-bundle.mjs";

const read = (path) => fs.readFileSync(path, "utf8");
const conf = () => JSON.parse(read(join(repoRoot, "crates/easyoffice-desktop/tauri.conf.json")));
const ours = [...REQUIRED_UTIS.values()].map((uti) => uti.exported).sort(byCodeUnits);
const supported = [...REQUIRED_UTIS.values()].flatMap((uti) => [uti.exported, uti.external]).sort(byCodeUnits);

/** `<key>name</key>` followed by a string value or an array of strings (the plist subset used here). */
function plistValue(text, key) {
  const match = text.match(new RegExp(`<key>${key}</key>\\s*(?:<string>([^<]*)</string>|<array>([\\s\\S]*?)</array>|<(true|false)/>)`));
  assert.ok(match, `${key} is missing`);
  if (match[1] !== undefined) return match[1];
  if (match[3] !== undefined) return match[3] === "true";
  return [...match[2].matchAll(/<string>([^<]*)<\/string>/g)].map((m) => m[1]);
}

test("the desktop shell bundles exactly the extensions the build stages", () => {
  assert.deepEqual(conf().bundle.macOS.files, bundleFilesEntries());
});

test("each extension's Info.plist matches the table the build and the verifier use", () => {
  for (const ext of EXTENSIONS) {
    const plist = read(ext.infoPlist);
    assert.equal(plistValue(plist, "CFBundleIdentifier"), ext.bundleId);
    assert.equal(plistValue(plist, "CFBundleExecutable"), ext.moduleName);
    assert.equal(plistValue(plist, "NSExtensionPointIdentifier"), ext.pointIdentifier);
    assert.equal(plistValue(plist, "NSExtensionPrincipalClass"), ext.principalClass);
    assert.equal(plistValue(plist, "LSMinimumSystemVersion"), MIN_MACOS);
    assert.equal(plistValue(plist, "CFBundleShortVersionString"), "@VERSION@");
    assert.equal(plistValue(plist, "CFBundleVersion"), "@VERSION@");
    assert.match(plistValue(plist, "CFBundleIdentifier"), /^com\.aquilaxk\.easyoffice\./);
  }
});

test("the extensions support only the UTIs the app exports or lists for HWP and HWPX", () => {
  for (const ext of EXTENSIONS) {
    assert.deepEqual([...plistValue(read(ext.infoPlist), "QLSupportedContentTypes")].sort(byCodeUnits), supported, ext.kind);
  }
  const exported = conf().bundle.fileAssociations.flatMap((a) => (a.exportedType ? [a.exportedType.identifier] : []));
  for (const uti of ours) assert.ok(exported.includes(uti), `${uti} is exported by the app`);
  const listed = conf().bundle.fileAssociations.flatMap(bundledContentTypes);
  for (const uti of supported) assert.ok(listed.includes(uti), `${uti} is in the document types the bundler writes`);
});

test("the principal class is a class of the extension's sources, in its module", () => {
  for (const ext of EXTENSIONS) {
    const [module, cls] = ext.principalClass.split(".");
    assert.equal(module, ext.moduleName);
    assert.ok(ext.sources.some((source) => new RegExp(`final class ${cls}\\b`).test(read(source))), cls);
    assert.equal(plistValue(read(ext.infoPlist), "CFBundlePackageType"), "XPC!");
  }
});

test("the extensions are sandboxed with read access to the opened file and nothing else", () => {
  const keys = [...read(entitlements).matchAll(/<key>([^<]+)<\/key>\s*<true\/>/g)].map((m) => m[1]).sort(byCodeUnits);
  assert.deepEqual(keys, ["com.apple.security.app-sandbox", "com.apple.security.files.user-selected.read-only"]);
  assert.equal((read(entitlements).match(/<key>/g) ?? []).length, 2);
});

test("the C header, the Swift status codes and the Rust crate agree", () => {
  const rust = read(join(rustRoot, "src/lib.rs"));
  const exported = [...rust.matchAll(/pub unsafe extern "C" fn (eo_ql_\w+)/g)].map((m) => m[1]).sort(byCodeUnits);
  const declared = [...read(ffiHeader).matchAll(/\b(eo_ql_\w+)\(/g)].map((m) => m[1]).sort(byCodeUnits);
  assert.deepEqual(declared, exported);
  assert.equal((rust.match(/#\[no_mangle\]/g) ?? []).length, exported.length);

  const rustCodes = Object.fromEntries(
    [...read(join(rustRoot, "src/status.rs")).matchAll(/pub const STATUS_(\w+): i32 = (\d+);/g)].map((m) => [m[1], Number(m[2])]),
  );
  const swift = read(join(quicklookRoot, "Sources/Shared/EasyOfficeQuickLookFFI.swift"));
  const swiftCodes = Object.fromEntries([...swift.matchAll(/case (\w+) = (\d+)/g)].map((m) => [m[1].replace(/[A-Z]/g, (c) => `_${c}`).toUpperCase(), Number(m[2])]));
  assert.deepEqual(swiftCodes, rustCodes);
});

test("the Rust crate unwinds into catch_unwind, links as a static library and has no network code", () => {
  const manifest = read(join(rustRoot, "Cargo.toml"));
  assert.match(manifest, /crate-type\s*=\s*\[[^\]]*"staticlib"/);
  assert.doesNotMatch(manifest, /panic\s*=\s*"abort"/, "a panic must stay catchable at the C boundary");
  const sources = fs.readdirSync(join(rustRoot, "src")).filter((f) => f.endsWith(".rs") && f !== "tests.rs").map((f) => read(join(rustRoot, "src", f))).join("\n");
  assert.match(sources, /catch_unwind/);
  assert.doesNotMatch(sources, /std::net|TcpStream|reqwest|ureq/);
  assert.match(manifest, /^\[workspace\]\s*$/m, "the crate is its own workspace, outside the CI --workspace jobs");
});

test("the workspace version feeds the extensions and a release version overrides it", () => {
  assert.match(appVersion(""), /^\d+\.\d+\.\d+$/);
  assert.equal(appVersion("1.2.3"), "1.2.3");
});

test("the verifier accepts a right extension and names each way it can be wrong", () => {
  const externals = [...REQUIRED_UTIS.values()].map((uti) => uti.external);
  const app = {
    CFBundleShortVersionString: "1.0.0",
    UTExportedTypeDeclarations: ours.map((id) => ({ UTTypeIdentifier: id })),
    CFBundleDocumentTypes: [{ LSItemContentTypes: externals }],
  };
  for (const ext of EXTENSIONS) {
    const info = {
      CFBundleIdentifier: ext.bundleId, CFBundleExecutable: ext.moduleName, CFBundlePackageType: "XPC!",
      LSMinimumSystemVersion: MIN_MACOS, CFBundleShortVersionString: "1.0.0",
      NSExtension: {
        NSExtensionPointIdentifier: ext.pointIdentifier, NSExtensionPrincipalClass: ext.principalClass,
        NSExtensionAttributes: { QLSupportedContentTypes: supported },
      },
    };
    assert.deepEqual(checkExtensionInfo(info, ext, app), []);
    const wrong = structuredClone(info);
    wrong.CFBundleShortVersionString = "0.9.0";
    wrong.NSExtension.NSExtensionAttributes.QLSupportedContentTypes = ["com.example.unknown"];
    wrong.NSExtension.NSExtensionPointIdentifier = "com.apple.quicklook.other";
    const problems = checkExtensionInfo(wrong, ext, app).join("\n");
    assert.match(problems, /CFBundleShortVersionString/);
    assert.match(problems, /com\.example\.unknown/);
    assert.match(problems, /NSExtensionPointIdentifier/);
    assert.ok(checkExtensionInfo({}, ext, app).length >= 5);
    const unlisted = structuredClone(info);
    unlisted.NSExtension.NSExtensionAttributes.QLSupportedContentTypes = supported;
    assert.match(checkExtensionInfo(unlisted, ext, { ...app, CFBundleDocumentTypes: [] }).join("\n"), /neither exports nor opens/);
  }
});

test("CI and the release build the extensions before the app and check the bundle after", () => {
  for (const [file, job] of [[".github/workflows/ci.yml", "verify"], [".github/workflows/release.yml", "release-macos"]]) {
    const text = read(join(repoRoot, file));
    const section = text.slice(text.indexOf(`  ${job}:`));
    const build = section.indexOf("build-quicklook-macos.mjs --universal");
    const tauri = section.indexOf("@tauri-apps/cli");
    const verify = section.indexOf("verify-quicklook-bundle.mjs");
    assert.ok(build > 0 && tauri > build, `${file}: the extensions are built before tauri build`);
    assert.ok(verify > tauri, `${file}: the bundle is checked after tauri build`);
  }
});
