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

function makeJob(name, startOffsetSec, durationSec, steps = []) {
  const base = Date.parse("2026-11-26T00:00:00Z");
  return {
    name,
    conclusion: "success",
    started_at: new Date(base + startOffsetSec * 1000).toISOString(),
    completed_at: new Date(base + (startOffsetSec + durationSec) * 1000).toISOString(),
    steps: steps.map(([stepName, sOffset, sDur]) => ({
      name: stepName,
      started_at: new Date(base + sOffset * 1000).toISOString(),
      completed_at: new Date(base + (sOffset + sDur) * 1000).toISOString(),
    })),
  };
}

test("computeTimingReport detects job, step, and span regressions while ignoring minor jitter", () => {
  const prevJobs = [
    makeJob("prepare", 0, 15),
    makeJob("build-windows-release", 15, 600, [
      ["Prepare build", 15, 200],
      ["Bundle NSIS", 215, 385],
    ]),
  ];
  const currJobs = [
    makeJob("prepare", 0, 20), // +5s: below threshold
    makeJob("build-windows-release", 20, 750, [
      ["Prepare build", 20, 210], // +10s (<15s and <=10%)
      ["Bundle NSIS", 230, 540], // +155s (>10% and >=15s)
    ]),
    makeJob("release-timings", 770, 5), // excluded from span
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
  assert.deepEqual(
    report.jobRegressions.map((r) => r.job),
    ["build-windows-release"],
  );
  assert.deepEqual(
    report.stepRegressions.map((r) => r.step),
    ["Bundle NSIS"],
  );
  const md = formatTimingMarkdown(report);
  assert.match(md, /12m 50s/);
  assert.match(md, /Bundle NSIS/);
});

test("computeTimingReport handles missing baseline run gracefully", () => {
  const report = computeTimingReport({
    currentRunId: 50,
    currentJobs: [makeJob("prepare", 0, 14)],
    previousRun: null,
    previousJobs: [],
  });
  assert.equal(report.previousRunId, null);
  assert.equal(report.spanRegression, false);
  assert.equal(formatDuration(report.currentSpanSec), "14s");
});

test("runReleaseTimingReport queries GitHub API and writes summary file", async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "timing-report-test-"));
  const summaryFile = path.join(tmpDir, "summary.md");
  try {
    const prevJobs = [makeJob("prepare", 0, 20)];
    const currJobs = [makeJob("prepare", 0, 25)];
    const mockFetch = async (url) => {
      if (url.endsWith("/actions/runs/99/jobs?per_page=100")) {
        return new Response(JSON.stringify({ jobs: currJobs }), { status: 200 });
      }
      if (url.includes("/actions/workflows/release.yml/runs")) {
        return new Response(JSON.stringify({ workflow_runs: [{ id: 99 }, { id: 88, head_branch: "v0.2.2" }] }), {
          status: 200,
        });
      }
      if (url.endsWith("/actions/runs/88/jobs?per_page=100")) {
        return new Response(JSON.stringify({ jobs: prevJobs }), { status: 200 });
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
    assert.match(fs.readFileSync(summaryFile, "utf8"), /Release Build Timing Report/);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
