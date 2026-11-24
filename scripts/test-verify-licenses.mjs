import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  classifySpdx,
  parseMplNotices,
  verifyCargoMetadata,
  verifyNpmLockfile,
  verifyRepositoryLicenses,
  verifyVendoredAndWasm,
} from "./verify-licenses.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index";

function validNoticeDoc(crateName = "crate-mpl", version = "1.2.3", bodyExtra = "") {
  return [
    "# Third-Party Notices",
    "",
    "<!-- BEGIN GENERATED WASM DEPENDENCY NOTICES -->",
    "| Crate | Version | License |",
    "|---|---|---|",
    "| crate-wasm | 0.1.0 | MIT OR Apache-2.0 |",
    "<!-- END GENERATED WASM DEPENDENCY NOTICES -->",
    "",
    "## Desktop shell MPL-2.0 crates (linked unmodified)",
    "",
    "| Crate | Version | Copyright holder | Pulled in by | Source |",
    "| --- | --- | --- | --- | --- |",
    `| \`${crateName}\` | ${version} | Author Example | shell | https://crates.io/crates/${crateName}/${version} |`,
    bodyExtra,
    "```text",
    "Public License Version 2.0",
    "1. Definitions",
    "Covered Software",
    "Secondary License",
    "Exhibit A - Source Code Form License Notice",
    "```",
  ].join("\n");
}

function makeMetadata({
  extraPkg = null,
  fromEngine = false,
} = {}) {
  const packages = [
    { id: "easyoffice-app 0.2.3", name: "easyoffice-app", version: "0.2.3", license: "Apache-2.0", source: null, manifest_path: "/ws/crates/easyoffice-app/Cargo.toml" },
    { id: "easyoffice-desktop 0.2.3", name: "easyoffice-desktop", version: "0.2.3", license: "Apache-2.0", source: null, manifest_path: "/ws/crates/easyoffice-desktop/Cargo.toml" },
    ...(extraPkg ? [extraPkg] : []),
  ];
  const appDeps = fromEngine && extraPkg ? [{ pkg: extraPkg.id, dep_kinds: [{ kind: null }] }] : [];
  const desktopDeps = !fromEngine && extraPkg ? [{ pkg: extraPkg.id, dep_kinds: [{ kind: null }] }] : [];
  return {
    packages,
    workspace_members: ["easyoffice-app 0.2.3", "easyoffice-desktop 0.2.3"],
    resolve: {
      nodes: [
        { id: "easyoffice-app 0.2.3", deps: appDeps },
        { id: "easyoffice-desktop 0.2.3", deps: desktopDeps },
      ],
    },
  };
}

test("SPDX classifier accepts permissive expressions and rejects copyleft or malformed ones", () => {
  for (const expr of [
    "MIT",
    "Apache-2.0",
    "MIT OR Apache-2.0",
    "MIT/Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "MIT OR Apache-2.0 OR LGPL-2.1-or-later",
    "MIT OR Apache-2.0 OR WTFPL",
    "(Apache-2.0 OR MIT) AND BSD-3-Clause",
    "Unicode-3.0",
    "Unlicense OR MIT",
    "0BSD OR MIT OR Apache-2.0",
    "Zlib",
  ]) {
    assert.equal(classifySpdx(expr).kind, "permissive", expr);
  }

  assert.equal(classifySpdx("MPL-2.0").kind, "mpl");
  assert.equal(classifySpdx("MIT AND MPL-2.0").kind, "mpl");
  assert.equal(classifySpdx("MPL-2.0 OR Apache-2.0").kind, "permissive");

  for (const bad of [
    "",
    null,
    undefined,
    "GPL-2.0-only",
    "GPL-3.0-or-later",
    "LGPL-2.1-only",
    "AGPL-3.0-only",
    "MIT AND GPL-3.0-only",
    "GPL-2.0-or-later OR AGPL-3.0-only",
    "MPL-1.1",
    "CC-BY-NC-4.0",
    "CC-BY-SA-4.0",
    "SSPL-1.0",
    "Apache-2.0 WITH Custom-exception",
    "UNKNOWN-LIC",
    "(MIT OR Apache-2.0",
    "MIT OR",
    "MIT; rm -rf",
  ]) {
    assert.equal(classifySpdx(bad).kind, "rejected", String(bad));
  }
});

test("Cargo metadata verifier rejects GPL, unannounced/modified/engine-linked MPL-2.0, and stale notices", () => {
  const notices = parseMplNotices(validNoticeDoc());
  assert.equal(notices.hasFullLicenseText, true);
  assert.equal(notices.malformedRows.length, 0);

  // Unmodified crates.io MPL-2.0 linked only by easyoffice-desktop passes when noticed.
  const okMeta = makeMetadata({
    extraPkg: { id: "crate-mpl 1.2.3", name: "crate-mpl", version: "1.2.3", license: "MPL-2.0", source: CRATES_IO, manifest_path: "/ext-cache/crate-mpl/Cargo.toml" },
  });
  assert.deepEqual(verifyCargoMetadata(okMeta, notices, { repoRoot: "/ws", label: "Cargo.toml", allowDesktopMpl: true }), []);

  // Any copyleft / unknown crate is rejected immediately.
  const gplMeta = makeMetadata({
    extraPkg: { id: "crate-gpl 0.1.0", name: "crate-gpl", version: "0.1.0", license: "GPL-3.0-only", source: CRATES_IO, manifest_path: "/ext-cache/crate-gpl/Cargo.toml" },
  });
  assert.match(
    verifyCargoMetadata(gplMeta, notices, { repoRoot: "/ws", label: "Cargo.toml", allowDesktopMpl: true }).join("\n"),
    /rejected license 'GPL-3\.0-only'/,
  );

  // Path-sourced / vendored / git-sourced MPL-2.0 fails closed.
  const pathMpl = makeMetadata({
    extraPkg: { id: "crate-mpl 1.2.3", name: "crate-mpl", version: "1.2.3", license: "MPL-2.0", source: null, manifest_path: "/ws/vendor/crate-mpl/Cargo.toml" },
  });
  assert.ok(verifyCargoMetadata(pathMpl, notices, { repoRoot: "/ws", label: "Cargo.toml", allowDesktopMpl: true }).length > 0);

  const inRepoMpl = makeMetadata({
    extraPkg: { id: "crate-mpl 1.2.3", name: "crate-mpl", version: "1.2.3", license: "MPL-2.0", source: CRATES_IO, manifest_path: "/ws/vendor/crate-mpl/Cargo.toml" },
  });
  assert.match(
    verifyCargoMetadata(inRepoMpl, notices, { repoRoot: "/ws", label: "Cargo.toml", allowDesktopMpl: true }).join("\n"),
    /inside repository tree/,
  );

  // Engine crate pulling an MPL-2.0 crate is rejected even if noticed for desktop.
  const engineMpl = makeMetadata({
    fromEngine: true,
    extraPkg: { id: "crate-mpl 1.2.3", name: "crate-mpl", version: "1.2.3", license: "MPL-2.0", source: CRATES_IO, manifest_path: "/ext-cache/crate-mpl/Cargo.toml" },
  });
  assert.match(
    verifyCargoMetadata(engineMpl, notices, { repoRoot: "/ws", label: "Cargo.toml", allowDesktopMpl: true }).join("\n"),
    /non-desktop workspace crates/,
  );

  // Disallowed in non-desktop workspace (allowDesktopMpl: false).
  assert.match(
    verifyCargoMetadata(okMeta, notices, { repoRoot: "/ws", label: "ext/Cargo.toml", allowDesktopMpl: false }).join("\n"),
    /not permitted in this workspace/,
  );

  // Missing or stale MPL-2.0 notice entries are rejected.
  const missingNotice = parseMplNotices(validNoticeDoc("other-mpl", "9.9.9"));
  const errs = verifyCargoMetadata(okMeta, missingNotice, { repoRoot: "/ws", label: "Cargo.toml", allowDesktopMpl: true });
  assert.ok(errs.some((e) => e.includes("missing from THIRD_PARTY_NOTICES.md")));
  assert.ok(errs.some((e) => e.includes("other-mpl@9.9.9")));
});

test("NPM lockfile verifier enforces registry integrity, notice duty for prod MPL-2.0, and GPL rejection", () => {
  const notices = parseMplNotices(validNoticeDoc("pkg-mpl-prod", "1.0.0"));
  const okLock = {
    packages: {
      "": {},
      "node_modules/pkg-mit": { version: "1.0.0", license: "MIT", integrity: "sha512-abc" },
      "node_modules/pkg-mpl-dev": { version: "2.0.0", license: "MPL-2.0", dev: true, integrity: "sha512-def", resolved: "https://registry.npmjs.org/pkg-mpl-dev/-/pkg-mpl-dev-2.0.0.tgz" },
    },
  };
  assert.deepEqual(verifyNpmLockfile("app/package-lock.json", okLock, notices), []);

  const badLock = {
    packages: {
      "": {},
      "node_modules/pkg-gpl": { version: "1.0.0", license: "GPL-3.0-or-later", dev: true, integrity: "sha512-x" },
      "node_modules/pkg-unnoticed-mpl": { version: "1.0.0", license: "MPL-2.0", integrity: "sha512-y" },
      "node_modules/pkg-linked-mpl": { version: "2.0.0", license: "MPL-2.0", dev: true, resolved: "file:../local", link: true },
    },
  };
  const errs = verifyNpmLockfile("app/package-lock.json", badLock, notices);
  assert.equal(errs.length, 3, errs.join("\n"));
});

test("Vendored & WASM table verifier rejects MPL/GPL in vendor/ or WASM notices and missing LICENSE files", () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "lic-check-"));
  try {
    const vDir = path.join(tmp, "vendor", "pkg-bad");
    fs.mkdirSync(vDir, { recursive: true });
    fs.writeFileSync(path.join(vDir, "Cargo.toml"), "[package]\nname = \"pkg-bad\"\nlicense = \"MPL-2.0\"\n");
    const badNotices = [
      "<!-- BEGIN GENERATED WASM DEPENDENCY NOTICES -->",
      "| Crate | Version | License |",
      "|---|---|---|",
      "| pkg-wasm | 0.1.0 | GPL-2.0-only |",
      "<!-- END GENERATED WASM DEPENDENCY NOTICES -->",
    ].join("\n");
    const res = verifyVendoredAndWasm(tmp, badNotices);
    assert.ok(res.errors.some((e) => e.includes("vendored code must use a permissive license")));
    assert.ok(res.errors.some((e) => e.includes("missing LICENSE/COPYING file")));
    assert.ok(res.errors.some((e) => e.includes("expected permissive license, found 'GPL-2.0-only'")));
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test("Repository lockfiles, vendored crates, MPL-2.0 notices, and WASM notices pass in CI without Cargo", () => {
  const { errors, summary } = verifyRepositoryLicenses(ROOT, { runCargo: false });
  assert.deepEqual(errors, [], errors.join("\n"));
  assert.ok(summary.npmLockfiles >= 2);
  assert.ok(summary.vendorDirs >= 3);
  assert.ok(summary.wasmRows > 0);
  assert.equal(summary.mplNotices, 5);
});
