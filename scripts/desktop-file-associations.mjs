/**
 * Static contract of the desktop bundle configuration (crates/easyoffice-desktop/tauri.conf.json).
 *
 * macOS and Linux builds can only run on their own CI, so what ends up in the app's Info.plist is pinned
 * here: the Tauri `bundle.fileAssociations` become `CFBundleDocumentTypes` (extensions, role, handler
 * rank), `bundle.macOS` sets the minimum system version, and the bundle must stay unsigned in the
 * repository (a signing identity is supplied through the environment at release time, PD-094).
 * On Linux the same associations become the `MimeType=` line of the `.desktop` file, so every extension
 * needs a MIME type, and the package dependencies and the shipped license notices are pinned too
 * (PD-086). `scripts/verify-linux-bundles.mjs` checks the built packages against the same constants.
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

/** MIME type the Linux `.desktop` file declares for each extension (`MimeType=`). */
export const REQUIRED_MIME_TYPES = new Map([
  ["hwp", "application/x-hwp"],
  ["hwpx", "application/vnd.hancom.hwpx"],
  ["docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"],
  ["odt", "application/vnd.oasis.opendocument.text"],
  ["xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"],
  ["ods", "application/vnd.oasis.opendocument.spreadsheet"],
  ["pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation"],
  ["odp", "application/vnd.oasis.opendocument.presentation"],
  ["pdf", "application/pdf"],
]);

/**
 * macOS UTIs of the two HWP formats (Info.plist `LSItemContentTypes` and `UTExportedTypeDeclarations`): the
 * type the app exports itself (types other software declares are not listed). Both formats are plain data (HWPX is also a zip archive).
 */
export const REQUIRED_UTIS = new Map([
  ["hwp", { exported: "com.aquilaxk.easyoffice.hwp", conformsTo: ["public.data"] }],
  ["hwpx", { exported: "com.aquilaxk.easyoffice.hwpx", conformsTo: ["public.data", "public.zip-archive"] }],
]);

/** Runtime libraries the Linux packages must declare (the WebKitGTK shell, its menus and the update check TLS link them). */
export const LINUX_DEB_DEPENDS = ["libwebkit2gtk-4.1-0", "libgtk-3-0", "libxdo3", "libssl3"];
/** RPM requirements are library capabilities, so they hold on every RPM distribution's package names. */
export const LINUX_RPM_DEPENDS = [
  "libwebkit2gtk-4.1.so.0()(64bit)",
  "libgtk-3.so.0()(64bit)",
  "libxdo.so.3()(64bit)",
  "libssl.so.3()(64bit)",
];

/**
 * The `.desktop` template of the deb and rpm bundles (the AppImage reuses the deb one). Tauri's default has
 * no file field code on `Exec`, so a double click would start the app without the document.
 */
export const LINUX_DESKTOP_TEMPLATE = "./linux/easyoffice.desktop";

/** The notice file every package must carry (MIT notice condition) and where the config reads it from. */
export const NOTICES_FILE = "THIRD_PARTY_NOTICES.md";
const NOTICES_SOURCE = `../../${NOTICES_FILE}`;

/**
 * The PDF export fonts every package must carry (PD-114): the bundled Korean face and its license, as
 * installed paths under the resource folder and where the config reads them from (the rhwp checkout).
 */
export const PDF_FONT_FILES = ["pdf-fonts/NotoSansKR-Regular.ttf", "pdf-fonts/NotoSansKR-OFL.txt"];
const PDF_FONT_SOURCE_DIR = "../../third_party/rhwp/ttfs/opensource/";

/** The XDG menu category the `.desktop` file must carry (Tauri maps `bundle.category` Productivity to it). */
export const LINUX_DESKTOP_CATEGORY = "Office";

/** Handler ranks that make LaunchServices prefer this app over the user's current default. */
const PUSHY_RANKS = new Set(["Owner", "Default"]);

const REQUIRED_TARGETS = ["nsis", "app", "dmg", "deb", "rpm", "appimage"];
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

function checkLinuxSettings(bundle) {
  const problems = [];
  const linux = bundle.linux ?? {};
  const wanted = [
    ["deb", LINUX_DEB_DEPENDS],
    ["rpm", LINUX_RPM_DEPENDS],
  ];
  for (const [kind, required] of wanted) {
    const depends = Array.isArray(linux[kind]?.depends) ? linux[kind].depends : [];
    for (const dependency of required) {
      if (!depends.includes(dependency)) problems.push(`bundle.linux.${kind}.depends must list '${dependency}'`);
    }
    if (linux[kind]?.desktopTemplate !== LINUX_DESKTOP_TEMPLATE) {
      problems.push(`bundle.linux.${kind}.desktopTemplate must be '${LINUX_DESKTOP_TEMPLATE}' (Exec needs the file field code)`);
    }
  }
  if (bundle.resources?.[NOTICES_SOURCE] !== NOTICES_FILE) {
    problems.push(`bundle.resources must ship '${NOTICES_FILE}' (map '${NOTICES_SOURCE}' to '${NOTICES_FILE}', MIT notice condition)`);
  }
  for (const installed of PDF_FONT_FILES) {
    const source = `${PDF_FONT_SOURCE_DIR}${installed.split("/").pop()}`;
    if (bundle.resources?.[source] !== installed) {
      problems.push(`bundle.resources must ship '${installed}' (map '${source}' to '${installed}', PDF export font)`);
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
    else if (registered.get(ext).role !== role) problems.push(`file association for '.${ext}' must use role ${role}`);
    else if (registered.get(ext).mimeType !== REQUIRED_MIME_TYPES.get(ext)) {
      problems.push(`file association for '.${ext}' must declare mimeType ${REQUIRED_MIME_TYPES.get(ext)} (Linux MimeType=)`);
    }
  }
  return problems;
}

function checkUtiDeclarations(associations) {
  const problems = [];
  for (const [ext, uti] of REQUIRED_UTIS) {
    const assoc = associations.find((a) => Array.isArray(a?.ext) && a.ext.includes(ext));
    if (!assoc) continue;
    const types = Array.isArray(assoc.contentTypes) ? assoc.contentTypes : [];
    for (const wanted of [uti.exported]) {
      if (!types.includes(wanted)) problems.push(`file association for '.${ext}' must list contentTypes '${wanted}' (macOS UTI)`);
    }
    const exported = assoc.exportedType;
    if (exported?.identifier !== uti.exported) {
      problems.push(`file association for '.${ext}' must export the type '${uti.exported}'`);
    } else if (JSON.stringify(exported.conformsTo) !== JSON.stringify(uti.conformsTo)) {
      problems.push(`exported type '${uti.exported}' must conform to ${uti.conformsTo.join(", ")}`);
    }
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
      registered.set(String(ext).toLowerCase(), { role: assoc.role, mimeType: assoc.mimeType });
    }
  }
  return [...problems, ...checkRequiredExtensions(registered), ...checkUtiDeclarations(associations)];
}

/** Returns a list of problems (empty when the configuration satisfies the contract). */
export function checkDesktopBundleConfig(conf) {
  const bundle = conf?.bundle ?? {};
  return [
    ...checkTargetsAndIcons(bundle),
    ...checkMacOsSettings(bundle.macOS ?? {}),
    ...checkLinuxSettings(bundle),
    ...checkFileAssociations(Array.isArray(bundle.fileAssociations) ? bundle.fileAssociations : []),
  ];
}
