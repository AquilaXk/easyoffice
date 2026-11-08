import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import {
  checkDesktopBundleConfig,
  LINUX_DEB_DEPENDS,
  LINUX_DESKTOP_TEMPLATE,
  LINUX_RPM_DEPENDS,
  NOTICES_FILE,
  PDF_FONT_FILES,
  REQUIRED_ASSOCIATIONS,
  REQUIRED_MIME_TYPES,
  REQUIRED_UTIS,
} from "./desktop-file-associations.mjs";

const confUrl = new URL("../crates/easyoffice-desktop/tauri.conf.json", import.meta.url);
const load = () => JSON.parse(fs.readFileSync(confUrl, "utf8"));

test("the committed desktop bundle config satisfies the macOS, Windows and Linux contract", () => {
  assert.deepEqual(checkDesktopBundleConfig(load()), []);
});

test("every required extension is registered under its role", () => {
  const registered = new Map();
  for (const a of load().bundle.fileAssociations) for (const e of a.ext) registered.set(e, a.role);
  for (const [ext, role] of REQUIRED_ASSOCIATIONS) assert.equal(registered.get(ext), role, ext);
});

test("every required extension declares its Linux MIME type", () => {
  const mimeTypes = new Map();
  for (const a of load().bundle.fileAssociations) for (const e of a.ext) mimeTypes.set(e, a.mimeType);
  assert.deepEqual(Object.fromEntries(mimeTypes), Object.fromEntries(REQUIRED_MIME_TYPES));
});

test("the HWP formats declare only the exported UTI of this app", () => {
  const associations = load().bundle.fileAssociations;
  for (const [ext, uti] of REQUIRED_UTIS) {
    const assoc = associations.find((a) => a.ext.includes(ext));
    assert.deepEqual(assoc.contentTypes, [uti.exported], ext);
    assert.deepEqual(assoc.exportedType, { identifier: uti.exported, conformsTo: uti.conformsTo }, ext);
    assert.equal(assoc.rank, "Alternate", ext);
  }
});

test("a missing or mismatched UTI declaration is reported", () => {
  const conf = load();
  const hwp = conf.bundle.fileAssociations.find((a) => a.ext.includes("hwp"));
  const hwpx = conf.bundle.fileAssociations.find((a) => a.ext.includes("hwpx"));
  hwp.contentTypes = [];
  hwpx.exportedType.conformsTo = ["public.data"];
  const problems = checkDesktopBundleConfig(conf).join("\n");
  assert.match(problems, /'\.hwp' must list contentTypes 'com\.aquilaxk\.easyoffice\.hwp'/);
  assert.match(problems, /'com\.aquilaxk\.easyoffice\.hwpx' must conform to public\.data, public\.zip-archive/);
  delete hwp.exportedType;
  assert.match(checkDesktopBundleConfig(conf).join("\n"), /'\.hwp' must export the type/);
});

test("a missing extension, a wrong role or a pushy handler rank is reported", () => {
  const conf = load();
  conf.bundle.fileAssociations = conf.bundle.fileAssociations.filter((a) => !a.ext.includes("hwpx"));
  conf.bundle.fileAssociations.find((a) => a.ext.includes("pdf")).role = "Editor";
  conf.bundle.fileAssociations.find((a) => a.ext.includes("docx")).rank = "Owner";
  const problems = checkDesktopBundleConfig(conf).join("\n");
  assert.match(problems, /'\.hwpx' is missing/);
  assert.match(problems, /'\.pdf' must use role Viewer/);
  assert.match(problems, /rank Alternate/);
});

test("macOS bundle settings are required and a committed signing identity is rejected", () => {
  const conf = load();
  delete conf.bundle.macOS.minimumSystemVersion;
  conf.bundle.macOS.signingIdentity = "Developer ID Application: Someone";
  conf.bundle.targets = ["nsis"];
  conf.bundle.icon = conf.bundle.icon.filter((i) => !i.endsWith(".icns"));
  const problems = checkDesktopBundleConfig(conf).join("\n");
  assert.match(problems, /minimumSystemVersion/);
  assert.match(problems, /signingIdentity must not be committed/);
  assert.match(problems, /targets must include 'app'/);
  assert.match(problems, /icon.icns/);
});

test("Linux packages must declare the WebKitGTK runtime libraries and the bundle targets", () => {
  const conf = load();
  assert.deepEqual(conf.bundle.linux.deb.depends, LINUX_DEB_DEPENDS);
  assert.deepEqual(conf.bundle.linux.rpm.depends, LINUX_RPM_DEPENDS);
  conf.bundle.linux.deb.depends = ["libgtk-3-0"];
  delete conf.bundle.linux.deb.desktopTemplate;
  delete conf.bundle.linux.rpm;
  conf.bundle.targets = ["nsis", "app", "dmg"];
  const problems = checkDesktopBundleConfig(conf).join("\n");
  assert.match(problems, /bundle\.linux\.deb\.depends must list 'libwebkit2gtk-4\.1-0'/);
  assert.match(problems, /bundle\.linux\.rpm\.depends must list 'libxdo\.so\.3\(\)\(64bit\)'/);
  assert.match(problems, /targets must include 'appimage'/);
  assert.match(problems, /bundle\.linux\.deb\.desktopTemplate must be '\.\/linux\/easyoffice\.desktop'/);
  assert.match(problems, /bundle\.linux\.rpm\.desktopTemplate must be/);
  assert.equal(load().bundle.linux.rpm.desktopTemplate, LINUX_DESKTOP_TEMPLATE);
});

test("a missing MIME type on a file association is reported", () => {
  const conf = load();
  delete conf.bundle.fileAssociations.find((a) => a.ext.includes("docx")).mimeType;
  conf.bundle.fileAssociations.find((a) => a.ext.includes("pdf")).mimeType = "text/plain";
  const problems = checkDesktopBundleConfig(conf).join("\n");
  assert.match(problems, /'\.docx' must declare mimeType application\/vnd\.openxmlformats-officedocument\.wordprocessingml\.document/);
  assert.match(problems, /'\.pdf' must declare mimeType application\/pdf/);
});

test("the PDF export font and its license must be shipped as bundle resources", () => {
  const conf = load();
  for (const installed of PDF_FONT_FILES) {
    assert.ok(Object.values(conf.bundle.resources).includes(installed), installed);
  }
  const key = Object.keys(conf.bundle.resources).find((source) => source.endsWith("NotoSansKR-Regular.ttf"));
  delete conf.bundle.resources[key];
  assert.match(checkDesktopBundleConfig(conf).join("\n"), /bundle\.resources must ship 'pdf-fonts\/NotoSansKR-Regular\.ttf'/);
});

test("the third-party notice file must be shipped as a bundle resource", () => {
  const conf = load();
  assert.equal(conf.bundle.resources[`../../${NOTICES_FILE}`], NOTICES_FILE);
  delete conf.bundle.resources;
  assert.match(checkDesktopBundleConfig(conf).join("\n"), /bundle\.resources must ship 'THIRD_PARTY_NOTICES\.md'/);
});
