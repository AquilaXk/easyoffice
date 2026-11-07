import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { checkDesktopBundleConfig, REQUIRED_ASSOCIATIONS } from "./desktop-file-associations.mjs";

const confUrl = new URL("../crates/easyoffice-desktop/tauri.conf.json", import.meta.url);
const load = () => JSON.parse(fs.readFileSync(confUrl, "utf8"));

test("the committed desktop bundle config satisfies the macOS and Windows contract", () => {
  assert.deepEqual(checkDesktopBundleConfig(load()), []);
});

test("every required extension is registered under its role", () => {
  const registered = new Map();
  for (const a of load().bundle.fileAssociations) for (const e of a.ext) registered.set(e, a.role);
  for (const [ext, role] of REQUIRED_ASSOCIATIONS) assert.equal(registered.get(ext), role, ext);
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
