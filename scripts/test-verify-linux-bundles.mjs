import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { LINUX_DEB_DEPENDS, LINUX_DESKTOP_TEMPLATE, LINUX_RPM_DEPENDS, PDF_FONT_FILES, REQUIRED_MIME_TYPES } from "./desktop-file-associations.mjs";
import {
  checkDebControl,
  checkDesktopEntry,
  checkPayload,
  checkRpmDesktopMatchesDeb,
  checkRpmPackage,
  parseControl,
  parseRpmDump,
  signatureProblems,
} from "./verify-linux-bundles.mjs";
import { readText } from "./test-support/read-text.mjs";

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

test("the payload must hold the binary, a desktop file, the third-party notices and the PDF export font", () => {
  const fonts = PDF_FONT_FILES.map((font) => `./usr/lib/EasyOffice/${font}`);
  const good = ["./usr/bin/EasyOffice", "./usr/lib/EasyOffice/THIRD_PARTY_NOTICES.md", "./usr/share/applications/EasyOffice.desktop", ...fonts];
  assert.deepEqual(checkPayload(good), []);
  const problems = checkPayload(["./usr/bin/EasyOffice"]).join("\n");
  assert.match(problems, /THIRD_PARTY_NOTICES\.md/);
  assert.match(problems, /\.desktop/);
  assert.match(problems, /pdf-fonts\/NotoSansKR-Regular\.ttf/);
  assert.match(problems, /pdf-fonts\/NotoSansKR-OFL\.txt/);
  assert.match(checkPayload(good.slice(0, 4)).join("\n"), /pdf-fonts\/NotoSansKR-OFL\.txt/);
});

test("an rpm must be built for the architecture and require the runtime libraries", () => {
  const requires = LINUX_RPM_DEPENDS.join("\n") + "\nrpmlib(CompressedFileNames) <= 3.0.4-1\n";
  assert.deepEqual(checkRpmPackage({ name: "easyoffice", arch: "x86_64", requires }, "x86_64"), []);
  const problems = checkRpmPackage({ name: "easyoffice", arch: "aarch64", requires: "libgtk-3.so.0()(64bit)\n" }, "x86_64").join("\n");
  assert.match(problems, /architecture is aarch64, expected x86_64/);
  assert.match(problems, /Requires must list libwebkit2gtk-4\.1\.so\.0\(\)\(64bit\)/);
  assert.doesNotMatch(problems, /libgtk-3/);
});

const shellDir = new URL("../crates/easyoffice-desktop/", import.meta.url);
// A Windows checkout has CRLF line ends; the bundler is only ever run on Linux, where .gitattributes keeps LF.
const template = readText(new URL(LINUX_DESKTOP_TEMPLATE, shellDir));

/** What the bundler's Handlebars renders for the template: `{{x}}` filled in, `{{#if}}` blocks kept. */
const render = (text) =>
  text
    .split("\n")
    .filter((line) => !/^\{\{[#/]/.test(line))
    .join("\n")
    .replaceAll("{{name}}", "EasyOffice")
    .replaceAll("{{comment}}", "EasyOffice offline office workspace")
    .replaceAll("{{exec}}", "EasyOffice")
    .replaceAll("{{icon}}", "EasyOffice");

test("the desktop entry template renders an entry the package check accepts", () => {
  const rendered = render(template);
  assert.deepEqual(checkDesktopEntry(rendered), []);
  assert.match(rendered, /^Exec=EasyOffice %F$/m);
  assert.match(rendered, /^Categories=Office;$/m);
});

test("the template lists exactly the registered MIME types and no template variable is left over", () => {
  const rendered = render(template);
  const line = rendered.split("\n").find((l) => l.startsWith("MimeType="));
  assert.deepEqual(line.slice("MimeType=".length).split(";").filter(Boolean), [...REQUIRED_MIME_TYPES.values()]);
  assert.doesNotMatch(rendered, /\{\{/);
});

test("the deb, rpm and AppImage bundles all take the template (the AppImage reuses the deb setting)", () => {
  const conf = JSON.parse(fs.readFileSync(new URL("tauri.conf.json", shellDir), "utf8"));
  assert.equal(conf.bundle.linux.deb.desktopTemplate, LINUX_DESKTOP_TEMPLATE);
  assert.equal(conf.bundle.linux.rpm.desktopTemplate, LINUX_DESKTOP_TEMPLATE);
});

const sha256 = (text) => crypto.createHash("sha256").update(text).digest("hex");
const dump = (digest) =>
  `/usr/bin/EasyOffice 100 1700000000 ${"0".repeat(64)} 0100755 root root 0 0 0 X\n` +
  `/usr/share/applications/EasyOffice.desktop 300 1700000000 ${digest} 0100644 root root 0 0 0 X\n` +
  "/usr/share 0 1700000000 0000000000000000000000000000000000000000000000000000000000000000 040755 root root 0 0 0 X\n";

test("rpm --dump lines give the path and the file digest", () => {
  const entries = parseRpmDump(dump("ab".repeat(32)));
  assert.deepEqual(entries.map((e) => e.path), ["/usr/bin/EasyOffice", "/usr/share/applications/EasyOffice.desktop", "/usr/share"]);
  assert.equal(entries[1].digest, "ab".repeat(32));
});

test("the rpm .desktop file must hash like the checked deb one", () => {
  const text = render(template);
  assert.deepEqual(checkRpmDesktopMatchesDeb(parseRpmDump(dump(sha256(text))), text), []);
  assert.match(checkRpmDesktopMatchesDeb(parseRpmDump(dump(sha256("other"))), text).join("\n"), /differs from the deb's \.desktop/);
  assert.match(checkRpmDesktopMatchesDeb(parseRpmDump(dump("abc123")), text).join("\n"), /not SHA-256/);
  assert.match(checkRpmDesktopMatchesDeb(parseRpmDump(dump(sha256(text))), undefined).join("\n"), /no checked deb/);
  assert.deepEqual(checkRpmDesktopMatchesDeb(parseRpmDump("/usr/bin/EasyOffice 1 1 aa 0100755 root root 0 0 0 X\n"), text), []);
});

test("a release package needs a non-empty signature file next to it", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "signed-bundle-"));
  const deb = path.join(dir, "EasyOffice_0.1.0_amd64.deb");
  fs.writeFileSync(deb, "deb");
  assert.match(signatureProblems(deb).join("\n"), /EasyOffice_0\.1\.0_amd64\.deb\.sig is missing or empty/);
  fs.writeFileSync(`${deb}.sig`, "");
  assert.equal(signatureProblems(deb).length, 1, "an empty signature does not count");
  fs.writeFileSync(`${deb}.sig`, "c2ln");
  assert.deepEqual(signatureProblems(deb), []);
});
