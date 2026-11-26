import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  computeTimingReport,
  formatDuration,
  formatTimingMarkdown,
  runReleaseTimingReport,
} from "./release-timing-report.mjs";

function makeJob(name, startOffsetSec, durationSec, steps = [], { conclusion = "success", status = "completed" } = {}) {
  const base = Date.parse("2026-11-26T00:00:00Z");
  return {
    name,
    status,
    conclusion,
    started_at: new Date(base + startOffsetSec * 1000).toISOString(),
    completed_at: durationSec === null ? null : new Date(base + (startOffsetSec + durationSec) * 1000).toISOString(),
    steps: steps.map(([stepName, sOffset, sDur]) => ({
      name: stepName,
      started_at: new Date(base + sOffset * 1000).toISOString(),
      completed_at: new Date(base + (sOffset + sDur) * 1000).toISOString(),
    })),
  };
}

test("computeTimingReport excludes Report Release Timing & Regressions self job and detects regressions", () => {
  const prevJobs = [
    makeJob("Resolve Release and Check Tag Matches Cargo Workspace Version", 0, 15),
    makeJob("Build Windows x64 Installer & Release Artifacts", 15, 600, [
      ["Prepare build", 15, 200],
      ["Bundle | NSIS", 215, 385],
    ]),
  ];
  const currJobs = [
    makeJob("Resolve Release and Check Tag Matches Cargo Workspace Version", 0, 20), // +5s: below threshold
    makeJob("Build Windows x64 Installer & Release Artifacts", 20, 750, [
      ["Prepare build", 20, 210], // +10s (<15s and <=10%)
      ["Bundle | NSIS", 230, 540], // +155s (>10% and >=15s)
    ]),
    makeJob("Report Release Timing & Regressions", 770, null, [], { conclusion: null, status: "in_progress" }),
  ];

  const report = computeTimingReport({
    currentRunId: 200,
    currentJobs: currJobs,
    previousRun: { id: 100, head_branch: "v0.2.3" },
    previousJobs: prevJobs,
  });

  assert.equal(report.currentSpanSec, 770);
  assert.equal(report.previousSpanSec, 615);
  assert.equal(report.spanRegression, true);
  assert.ok(!report.jobRows.some((r) => r.name === "Report Release Timing & Regressions"));
  assert.deepEqual(
    report.jobRegressions.map((r) => r.job),
    ["Build Windows x64 Installer & Release Artifacts"],
  );
  assert.deepEqual(
    report.stepRegressions.map((r) => r.step),
    ["Bundle | NSIS"],
  );
  const md = formatTimingMarkdown(report);
  assert.match(md, /12m 50s/);
  assert.match(md, /Bundle \\\| NSIS/);
});

test("computeTimingReport handles missing baseline run gracefully", () => {
  const report = computeTimingReport({
    currentRunId: 50,
    currentJobs: [makeJob("Resolve Release and Check Tag Matches Cargo Workspace Version", 0, 14)],
    previousRun: null,
    previousJobs: [],
  });
  assert.equal(report.previousRunId, null);
  assert.equal(report.spanRegression, false);
  assert.equal(formatDuration(report.currentSpanSec), "14s");
});

test("runReleaseTimingReport skips prepare-only duplicate-tag runs when selecting baseline", async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "timing-report-test-"));
  const summaryFile = path.join(tmpDir, "summary.md");
  try {
    const skippedDuplicateTagJobs = [
      makeJob("Resolve Release and Check Tag Matches Cargo Workspace Version", 0, 12),
      makeJob("Build Windows x64 Installer & Release Artifacts", 0, 0, [], { conclusion: "skipped" }),
    ];
    const realPrevJobs = [
      makeJob("Resolve Release and Check Tag Matches Cargo Workspace Version", 0, 15),
      makeJob("Build Windows x64 Installer & Release Artifacts", 15, 500),
    ];
    const currJobs = [
      makeJob("Resolve Release and Check Tag Matches Cargo Workspace Version", 0, 16),
      makeJob("Build Windows x64 Installer & Release Artifacts", 16, 510),
      makeJob("Report Release Timing & Regressions", 526, null, [], { conclusion: null, status: "in_progress" }),
    ];
    const mockFetch = async (url) => {
      if (url.endsWith("/actions/runs/99/jobs?per_page=100")) {
        return new Response(JSON.stringify({ jobs: currJobs }), { status: 200 });
      }
      if (url.includes("/actions/workflows/release.yml/runs")) {
        return new Response(
          JSON.stringify({
            workflow_runs: [
              { id: 99 },
              { id: 95, head_branch: "v0.2.3" },
              { id: 88, head_branch: "v0.2.2" },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.endsWith("/actions/runs/95/jobs?per_page=100")) {
        return new Response(JSON.stringify({ jobs: skippedDuplicateTagJobs }), { status: 200 });
      }
      if (url.endsWith("/actions/runs/88/jobs?per_page=100")) {
        return new Response(JSON.stringify({ jobs: realPrevJobs }), { status: 200 });
      }
      return new Response("not found", { status: 404 });
    };

    const report = await runReleaseTimingReport({
      repo: "AquilaXk/easyoffice",
      runId: 99,
      token: "tok",
      summaryPath: summaryFile,
      fetchImpl: mockFetch,
    });
    assert.equal(report.previousRunId, 88);
    assert.equal(report.previousSpanSec, 515);
    assert.match(fs.readFileSync(summaryFile, "utf8"), /Release Build Timing Report/);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
