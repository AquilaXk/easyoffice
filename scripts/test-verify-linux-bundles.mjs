import assert from "node:assert/strict";
import test from "node:test";
import { LINUX_DEB_DEPENDS, LINUX_RPM_DEPENDS, REQUIRED_MIME_TYPES } from "./desktop-file-associations.mjs";
import { checkDebControl, checkDesktopEntry, checkPayload, checkRpmPackage, parseControl } from "./verify-linux-bundles.mjs";

const mimeLine = [...REQUIRED_MIME_TYPES.values()].join(";") + ";";
const desktop = (overrides = {}) => {
  const entry = {
    Type: "Application",
    Name: "EasyOffice",
    Exec: "EasyOffice %U",
    Icon: "EasyOffice",
    Categories: "Office;",
    MimeType: mimeLine,
    ...overrides,
  };
  return `[Desktop Entry]\n${Object.entries(entry).filter(([, v]) => v !== null).map(([k, v]) => `${k}=${v}`).join("\n")}\n`;
};

const control = `Package: easyoffice
Version: 0.1.0
Architecture: amd64
Maintainer: EasyOffice
Installed-Size: 61000
Depends: ${LINUX_DEB_DEPENDS.join(", ")}
Recommends: xdg-utils
Description: EasyOffice offline office workspace
 EasyOffice is an offline desktop office workspace
 for documents, HWP, spreadsheets, slides, and PDF.
`;

test("a package control file is parsed with its continuation lines", () => {
  const fields = parseControl(control);
  assert.equal(fields.get("Architecture"), "amd64");
  assert.match(fields.get("Description"), /offline office workspace\nEasyOffice is an offline/);
});

test("a deb with the right architecture and runtime dependencies passes", () => {
  assert.deepEqual(checkDebControl(parseControl(control), "amd64"), []);
});

test("a deb with another architecture, or missing or version-qualified dependencies, is judged by package name", () => {
  const stripped = control.replace("libxdo3", "libxdo3 (>= 1.0) | libxdo4").replace("libgtk-3-0, ", "");
  const problems = checkDebControl(parseControl(stripped), "arm64").join("\n");
  assert.match(problems, /Architecture is amd64, expected arm64/);
  assert.match(problems, /Depends must list libgtk-3-0/);
  assert.doesNotMatch(problems, /libxdo3/);
});

test("the desktop entry must carry the category, the file field code and every MIME type", () => {
  assert.deepEqual(checkDesktopEntry(desktop()), []);
  const problems = checkDesktopEntry(desktop({ Categories: "Utility;", Exec: "EasyOffice", MimeType: "application/pdf;" })).join("\n");
  assert.match(problems, /Categories must include Office/);
  assert.match(problems, /Exec must pass the opened files/);
  assert.match(problems, /MimeType must list application\/x-hwp/);
  assert.doesNotMatch(problems, /MimeType must list application\/pdf/);
  assert.match(checkDesktopEntry("Name=EasyOffice\n").join("\n"), /\[Desktop Entry\]/);
});

test("the payload must hold the binary, a desktop file and the third-party notices", () => {
  const good = ["./usr/bin/EasyOffice", "./usr/lib/EasyOffice/THIRD_PARTY_NOTICES.md", "./usr/share/applications/EasyOffice.desktop"];
  assert.deepEqual(checkPayload(good), []);
  const problems = checkPayload(["./usr/bin/EasyOffice"]).join("\n");
  assert.match(problems, /THIRD_PARTY_NOTICES\.md/);
  assert.match(problems, /\.desktop/);
});

test("an rpm must be built for the architecture and require the runtime libraries", () => {
  const requires = LINUX_RPM_DEPENDS.join("\n") + "\nrpmlib(CompressedFileNames) <= 3.0.4-1\n";
  assert.deepEqual(checkRpmPackage({ name: "easyoffice", arch: "x86_64", requires }, "x86_64"), []);
  const problems = checkRpmPackage({ name: "easyoffice", arch: "aarch64", requires: "libgtk-3.so.0()(64bit)\n" }, "x86_64").join("\n");
  assert.match(problems, /architecture is aarch64, expected x86_64/);
  assert.match(problems, /Requires must list libwebkit2gtk-4\.1\.so\.0\(\)\(64bit\)/);
  assert.doesNotMatch(problems, /libgtk-3/);
});
