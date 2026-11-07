/**
 * Static contract of the desktop bundle configuration (crates/easyoffice-desktop/tauri.conf.json).
 *
 * macOS builds can only run on macOS CI, so what ends up in the app's Info.plist is pinned here: the
 * Tauri `bundle.fileAssociations` become `CFBundleDocumentTypes` (extensions, role, handler rank),
 * `bundle.macOS` sets the minimum system version, and the bundle must stay unsigned in the repository
 * (a signing identity is supplied through the environment at release time, PD-094).
 */

/** Extensions the installed app must register with the OS, with the role each is registered under. */
export const REQUIRED_ASSOCIATIONS = new Map([
  ["hwp", "Editor"],
  ["hwpx", "Editor"],
  ["docx", "Editor"],
  ["odt", "Editor"],
  ["xlsx", "Editor"],
  ["ods", "Editor"],
  ["pptx", "Editor"],
  ["odp", "Editor"],
  ["pdf", "Viewer"],
]);

/** Handler ranks that make LaunchServices prefer this app over the user's current default. */
const PUSHY_RANKS = new Set(["Owner", "Default"]);

const REQUIRED_TARGETS = ["nsis", "app", "dmg"];
const REQUIRED_ICONS = ["icons/icon.icns", "icons/icon.ico"];
const FORBIDDEN_MAC_KEYS = ["signingIdentity", "providerShortName", "entitlements"];
const MIN_SYSTEM_VERSION_PATTERN = /^\d+\.\d+(\.\d+)?$/;

function checkTargetsAndIcons(bundle) {
  const problems = [];
  const targets = Array.isArray(bundle.targets) ? bundle.targets : [];
  const icons = Array.isArray(bundle.icon) ? bundle.icon : [];
  for (const target of REQUIRED_TARGETS) {
    if (!targets.includes(target)) problems.push(`bundle.targets must include '${target}'`);
  }
  for (const icon of REQUIRED_ICONS) {
    if (!icons.includes(icon)) problems.push(`bundle.icon must list '${icon}'`);
  }
  if (typeof bundle.category !== "string" || bundle.category === "") {
    problems.push("bundle.category must be set (Info.plist LSApplicationCategoryType)");
  }
  return problems;
}

function checkMacOsSettings(mac) {
  const problems = [];
  if (!MIN_SYSTEM_VERSION_PATTERN.test(String(mac.minimumSystemVersion ?? ""))) {
    problems.push("bundle.macOS.minimumSystemVersion must be set (e.g. \"11.0\")");
  }
  for (const key of FORBIDDEN_MAC_KEYS) {
    if (key in mac) {
      problems.push(`bundle.macOS.${key} must not be committed (signing is supplied through APPLE_* environment variables, PD-094)`);
    }
  }
  return problems;
}

function checkAssociationShape(assoc, names) {
  const problems = [];
  const label = JSON.stringify(assoc?.ext ?? assoc?.name ?? "?");
  if (!["Editor", "Viewer"].includes(assoc?.role)) {
    problems.push(`file association ${label} must declare role Editor or Viewer`);
  }
  if (PUSHY_RANKS.has(assoc?.rank ?? "Default")) {
    problems.push(`file association ${label} must set rank Alternate (do not take over other apps' default handlers)`);
  }
  if (typeof assoc?.name !== "string" || assoc.name === "" || names.has(assoc.name)) {
    problems.push(`file association ${label} needs a unique name`);
  }
  names.add(assoc?.name);
  return problems;
}

function checkRequiredExtensions(registered) {
  const problems = [];
  for (const [ext, role] of REQUIRED_ASSOCIATIONS) {
    if (!registered.has(ext)) problems.push(`file association for '.${ext}' is missing`);
    else if (registered.get(ext) !== role) problems.push(`file association for '.${ext}' must use role ${role}`);
  }
  return problems;
}

function checkFileAssociations(associations) {
  const problems = [];
  const registered = new Map();
  const names = new Set();
  for (const assoc of associations) {
    problems.push(...checkAssociationShape(assoc, names));
    for (const ext of Array.isArray(assoc?.ext) ? assoc.ext : []) {
      registered.set(String(ext).toLowerCase(), assoc.role);
    }
  }
  return [...problems, ...checkRequiredExtensions(registered)];
}

/** Returns a list of problems (empty when the configuration satisfies the contract). */
export function checkDesktopBundleConfig(conf) {
  const bundle = conf?.bundle ?? {};
  return [
    ...checkTargetsAndIcons(bundle),
    ...checkMacOsSettings(bundle.macOS ?? {}),
    ...checkFileAssociations(Array.isArray(bundle.fileAssociations) ? bundle.fileAssociations : []),
  ];
}
