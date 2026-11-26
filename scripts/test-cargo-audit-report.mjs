import assert from "node:assert/strict";
import test from "node:test";
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
