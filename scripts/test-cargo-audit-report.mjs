import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  aggregateCargoAuditReports,
  formatCargoAuditMarkdown,
  parseCargoAuditJson,
} from "./cargo-audit-report.mjs";

test("parseCargoAuditJson extracts vulnerabilities and warnings and formats summary table", () => {
  const sample = JSON.stringify({
    vulnerabilities: {
      list: [
        {
          advisory: { id: "RUSTSEC-2026-0001", title: "Example | pipe\nnewline issue" },
          versions: { patched: [">=1.2.3"] },
          package: { name: "example-crate", version: "1.0.0" },
        },
      ],
    },
    warnings: {
      unmaintained: [
        {
          kind: "unmaintained",
          advisory: { id: "RUSTSEC-2025-0099", title: "Unmaintained" },
          package: { name: "old-crate", version: "0.4.0" },
        },
      ],
    },
  });

  const parsed = parseCargoAuditJson(sample, "workspace");
  const agg = aggregateCargoAuditReports([parsed]);
  assert.equal(agg.vulnerabilityCount, 1);
  assert.equal(agg.warningCount, 1);
  const md = formatCargoAuditMarkdown(agg);
  assert.match(md, /RUSTSEC-2026-0001/);
  assert.match(md, /`example-crate`/);
  assert.match(md, /`old-crate`/);
  assert.match(md, /Example \\\| pipe newline issue/);
});

test("parseCargoAuditJson rejects empty, malformed, or incomplete input fail-closed", () => {
  assert.throws(() => parseCargoAuditJson("", "empty"), /empty/);
  assert.throws(() => parseCargoAuditJson("{broken", "bad"), /invalid JSON/);
  assert.throws(() => parseCargoAuditJson("[]", "arr"), /JSON object/);
  assert.throws(() => parseCargoAuditJson('{"error":"db fetch failed"}', "err"), /missing vulnerabilities\.list/);
});

test("cargo-audit-report CLI escapes %, CR, and LF in ::error:: output for failures and strict advisories", () => {
  const scriptPath = fileURLToPath(new URL("./cargo-audit-report.mjs", import.meta.url));

  const badArgRes = spawnSync(
    process.execPath,
    [scriptPath, "--bad%flag\r\n::warning::injected"],
    { encoding: "utf8" },
  );
  assert.equal(badArgRes.status, 1);
  assert.match(
    badArgRes.stderr,
    /^::error::unknown or incomplete argument '--bad%25flag%0D%0A::warning::injected'\r?\n$/,
  );

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "cargo-audit-test-"));
  try {
    const reportFile = path.join(tmpDir, "audit.json");
    fs.writeFileSync(
      reportFile,
      JSON.stringify({
        vulnerabilities: {
          list: [
            {
              advisory: { id: "RUSTSEC-2026-0002", title: "100% vuln\r\n::notice::forged" },
              versions: { patched: [">=1.0.1"] },
              package: { name: "bad-crate", version: "1.0.0" },
            },
          ],
        },
      }),
    );
    const strictRes = spawnSync(
      process.execPath,
      [scriptPath, "--strict", "--report", `ws%2=${reportFile}`],
      { encoding: "utf8", env: { ...process.env, GITHUB_STEP_SUMMARY: "" } },
    );
    assert.equal(strictRes.status, 1);
    assert.match(
      strictRes.stderr,
      /^::error::\[ws%252\] RUSTSEC-2026-0002 in bad-crate 1\.0\.0 \(patched: >=1\.0\.1\) — 100%25 vuln%0D%0A::notice::forged\r?\n$/,
    );
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
