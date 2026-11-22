// The Quick Look extensions of the macOS app (PD-140): one table shared by the build script, the
// bundle verifier and the config test, so a name cannot drift between them.
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const quicklookRoot = join(repoRoot, "apps/quicklook");
export const rustRoot = join(quicklookRoot, "rust");
export const ffiHeader = join(quicklookRoot, "Sources/Shared/EasyOfficeQuickLookFFI.h");
export const entitlements = join(quicklookRoot, "Resources/Extension.entitlements");
/** Where the build stages the extensions; `bundle.macOS.files` in tauri.conf.json copies them. */
export const stagingRoot = join(repoRoot, "target/quicklook");
export const plugInsDir = join(stagingRoot, "PlugIns");
export const staticLibName = "libeasyoffice_quicklook_ffi.a";
export const MIN_MACOS = "12.0";

/** Default sort order (UTF-16 code units) as an explicit comparator. */
export const byCodeUnits = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

const shared = join(quicklookRoot, "Sources/Shared/EasyOfficeQuickLookFFI.swift");

export const EXTENSIONS = [
  {
    kind: "preview",
    moduleName: "EasyOfficeQuickLookPreview",
    appexName: "EasyOfficeQuickLookPreview.appex",
    bundleId: "com.aquilaxk.easyoffice.quicklook.preview",
    pointIdentifier: "com.apple.quicklook.preview",
    principalClass: "EasyOfficeQuickLookPreview.HwpPreviewProvider",
    infoPlist: join(quicklookRoot, "Resources/Preview/Info.plist"),
    sources: [shared, join(quicklookRoot, "Sources/Preview/HwpPreviewProvider.swift")],
    frameworks: ["CoreFoundation", "CoreGraphics", "Foundation", "QuickLookUI", "UniformTypeIdentifiers", "OSLog"],
  },
  {
    kind: "thumbnail",
    moduleName: "EasyOfficeQuickLookThumbnail",
    appexName: "EasyOfficeQuickLookThumbnail.appex",
    bundleId: "com.aquilaxk.easyoffice.quicklook.thumbnail",
    pointIdentifier: "com.apple.quicklook.thumbnail",
    principalClass: "EasyOfficeQuickLookThumbnail.HwpThumbnailProvider",
    infoPlist: join(quicklookRoot, "Resources/Thumbnail/Info.plist"),
    sources: [shared, join(quicklookRoot, "Sources/Thumbnail/HwpThumbnailProvider.swift")],
    frameworks: ["CoreFoundation", "CoreGraphics", "Foundation", "ImageIO", "QuickLookThumbnailing", "UniformTypeIdentifiers", "OSLog"],
  },
];

export const RUST_TARGETS = { arm64: "aarch64-apple-darwin", x86_64: "x86_64-apple-darwin" };

/**
 * The Rust targets one Quick Look build covers (PD-297): `EASYOFFICE_QUICKLOOK_TARGETS` names them outright,
 * `--universal` is both (CI), `--arch arm64|x86_64` is one (the release builds one architecture per job, and
 * an arm64 host can build the Intel slice), and the host's own architecture is the default.
 */
export function quicklookTargets(argv, env = process.env, hostArch = process.arch) {
  if (env.EASYOFFICE_QUICKLOOK_TARGETS) return env.EASYOFFICE_QUICKLOOK_TARGETS.split(",");
  if (argv.includes("--universal")) return [RUST_TARGETS.arm64, RUST_TARGETS.x86_64];
  const at = argv.indexOf("--arch");
  if (at < 0) return [hostArch === "arm64" ? RUST_TARGETS.arm64 : RUST_TARGETS.x86_64];
  const target = Object.hasOwn(RUST_TARGETS, argv[at + 1]) ? RUST_TARGETS[argv[at + 1]] : undefined;
  if (!target) throw new Error(`--arch must be one of ${Object.keys(RUST_TARGETS).join(", ")}, not '${argv[at + 1] ?? ""}'`);
  return [target];
}

/** The Swift `-target` triple for a Rust macOS target. */
export function swiftTarget(rustTarget) {
  const arch = rustTarget === RUST_TARGETS.x86_64 ? "x86_64" : "arm64";
  return `${arch}-apple-macosx${MIN_MACOS}`;
}

/** The version of the Cargo workspace (the app's version), unless `override` names the release's. */
export function appVersion(override = process.env.EASYOFFICE_VERSION) {
  if (override) return override;
  const cargo = readFileSync(join(repoRoot, "Cargo.toml"), "utf8");
  const section = cargo.split(/^\[workspace\.package\]\s*$/m)[1] ?? "";
  const match = section.match(/^version\s*=\s*"(\d+\.\d+\.\d+)"/m);
  if (!match) throw new Error("Cargo.toml has no [workspace.package] version");
  return match[1];
}

/** The `.appex` file names the desktop shell bundles, in `bundle.macOS.files` form. */
export function bundleFilesEntries() {
  return Object.fromEntries(
    EXTENSIONS.map((ext) => [
      `PlugIns/${ext.appexName}`,
      `../../target/quicklook/PlugIns/${ext.appexName}`,
    ]),
  );
}
