#!/usr/bin/env node
/**
 * Computes job/step durations and wall-clock span for a release workflow run and compares them
 * against the latest previous successful release run, emitting GitHub Actions warnings on
 * significant regressions (>10% and >=15s per step, >10% and >=30s per job/span).
 *
 * Uses Node.js built-in modules only.
 */

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { fetchGitHubJson } from "./post-release-check.mjs";

const SELF_JOB_NAMES = new Set(["release-timings", "Report Release Timing & Regressions"]);
const PREPARE_JOB_NAMES = new Set([
  "prepare",
  "Resolve Release and Check Tag Matches Cargo Workspace Version",
]);
const JOB_REGRESSION_MIN_SEC = 30;
const STEP_REGRESSION_MIN_SEC = 15;
const REGRESSION_RATIO = 0.10;
const MAX_CANDIDATE_RUNS = 5;

function escapeMarkdownCell(value) {
  return String(value ?? "").replace(/\r?\n/g, " ").replaceAll("|", "\\|");
}

function escapeWorkflowCommand(value) {
  return String(value ?? "").replaceAll("%", "%25").replaceAll("\r", "%0D").replaceAll("\n", "%0A");
}

export function elapsedSeconds(startedAt, completedAt) {
  if (!startedAt || !completedAt) return null;
  const start = Date.parse(startedAt);
  const end = Date.parse(completedAt);
  if (Number.isNaN(start) || Number.isNaN(end) || end < start) return null;
  return Math.round((end - start) / 1000);
}

export function formatDuration(seconds) {
  if (seconds === null || seconds === undefined) return "-";
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (mins === 0) return `${secs}s`;
  return `${mins}m ${String(secs).padStart(2, "0")}s`;
}

export function formatDelta(deltaSeconds, prevSeconds) {
  if (deltaSeconds === null || prevSeconds === null || prevSeconds <= 0) return "-";
  const sign = deltaSeconds >= 0 ? "+" : "-";
  const pct = Math.round((Math.abs(deltaSeconds) / prevSeconds) * 100);
  return `${sign}${formatDuration(Math.abs(deltaSeconds))} (${sign}${pct}%)`;
}

function summarizeJobs(jobs) {
  const filtered = (jobs ?? []).filter(
    (job) => job && !SELF_JOB_NAMES.has(job.name) && job.conclusion !== "skipped",
  );
  let minStart = Infinity;
  let maxEnd = -Infinity;
  const byName = new Map();

  for (const job of filtered) {
    const startMs = job.started_at ? Date.parse(job.started_at) : NaN;
    const endMs = job.completed_at ? Date.parse(job.completed_at) : NaN;
    if (!Number.isNaN(startMs)) minStart = Math.min(minStart, startMs);
    if (!Number.isNaN(endMs)) maxEnd = Math.max(maxEnd, endMs);

    const steps = new Map();
    for (const step of job.steps ?? []) {
      const dur = elapsedSeconds(step.started_at, step.completed_at);
      if (dur !== null && step.name) steps.set(step.name, dur);
    }
    byName.set(job.name, {
      name: job.name,
      conclusion: job.conclusion ?? job.status ?? "unknown",
      seconds: elapsedSeconds(job.started_at, job.completed_at),
      steps,
    });
  }

  const spanSeconds =
    Number.isFinite(minStart) && Number.isFinite(maxEnd) && maxEnd >= minStart
      ? Math.round((maxEnd - minStart) / 1000)
      : null;

  return { byName, spanSeconds };
}

export function isSubstantiveReleaseJobs(jobs) {
  const { byName } = summarizeJobs(jobs);
  for (const jobName of byName.keys()) {
    if (!PREPARE_JOB_NAMES.has(jobName)) return true;
  }
  return false;
}

export function computeTimingReport({ currentRunId, currentJobs, previousRun = null, previousJobs = [] }) {
  const curr = summarizeJobs(currentJobs);
  const prev = previousRun ? summarizeJobs(previousJobs) : { byName: new Map(), spanSeconds: null };

  const jobRows = [];
  const jobRegressions = [];
  const stepRegressions = [];

  for (const [jobName, info] of [...curr.byName.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
    const prevInfo = prev.byName.get(jobName) ?? null;
    const prevSec = prevInfo?.seconds ?? null;
    const delta = info.seconds !== null && prevSec !== null ? info.seconds - prevSec : null;
    jobRows.push({
      name: jobName,
      conclusion: info.conclusion,
      currentSec: info.seconds,
      previousSec: prevSec,
      deltaSec: delta,
    });

    if (delta !== null && prevSec > 0 && delta >= JOB_REGRESSION_MIN_SEC && delta / prevSec > REGRESSION_RATIO) {
      jobRegressions.push({ job: jobName, currentSec: info.seconds, previousSec: prevSec, deltaSec: delta });
    }

    if (prevInfo) {
      for (const [stepName, stepSec] of info.steps.entries()) {
        const prevStepSec = prevInfo.steps.get(stepName);
        if (typeof prevStepSec !== "number" || prevStepSec <= 0) continue;
        const stepDelta = stepSec - prevStepSec;
        if (stepDelta >= STEP_REGRESSION_MIN_SEC && stepDelta / prevStepSec > REGRESSION_RATIO) {
          stepRegressions.push({
            job: jobName,
            step: stepName,
            currentSec: stepSec,
            previousSec: prevStepSec,
            deltaSec: stepDelta,
          });
        }
      }
    }
  }

  stepRegressions.sort((a, b) => b.deltaSec - a.deltaSec);
  const spanDeltaSec =
    curr.spanSeconds !== null && prev.spanSeconds !== null ? curr.spanSeconds - prev.spanSeconds : null;
  const spanRegression =
    spanDeltaSec !== null &&
    prev.spanSeconds > 0 &&
    spanDeltaSec >= JOB_REGRESSION_MIN_SEC &&
    spanDeltaSec / prev.spanSeconds > REGRESSION_RATIO;

  return {
    currentRunId,
    previousRunId: previousRun?.id ?? null,
    previousLabel: previousRun?.head_branch ?? (previousRun?.id ? String(previousRun.id) : null),
    currentSpanSec: curr.spanSeconds,
    previousSpanSec: prev.spanSeconds,
    spanDeltaSec,
    spanRegression,
    jobRows,
    jobRegressions,
    stepRegressions,
  };
}

export function formatTimingMarkdown(report) {
  const lines = ["## Release Build Timing Report", ""];
  if (report.previousRunId) {
    lines.push(
      `Baseline: run \`${report.previousRunId}\` (${escapeMarkdownCell(report.previousLabel ?? "previous")}) | Wall-clock span: **${formatDuration(report.currentSpanSec)}** (baseline **${formatDuration(report.previousSpanSec)}**, delta ${formatDelta(report.spanDeltaSec, report.previousSpanSec)})`,
    );
  } else {
    lines.push(
      `No previous successful release run available for baseline comparison. Wall-clock span: **${formatDuration(report.currentSpanSec)}**`,
    );
  }

  lines.push("", "| Job | Outcome | Duration | Baseline | Delta |", "| --- | --- | ---: | ---: | ---: |");
  for (const row of report.jobRows) {
    lines.push(
      `| \`${escapeMarkdownCell(row.name)}\` | ${escapeMarkdownCell(row.conclusion)} | ${formatDuration(row.currentSec)} | ${formatDuration(row.previousSec)} | ${formatDelta(row.deltaSec, row.previousSec)} |`,
    );
  }

  if (report.stepRegressions.length > 0) {
    lines.push("", "### Step Timing Regressions (>10% and >=15s)", "", "| Job | Step | Current | Baseline | Delta |", "| --- | --- | ---: | ---: | ---: |");
    for (const reg of report.stepRegressions.slice(0, 15)) {
      lines.push(
        `| \`${escapeMarkdownCell(reg.job)}\` | ${escapeMarkdownCell(reg.step)} | ${formatDuration(reg.currentSec)} | ${formatDuration(reg.previousSec)} | ${formatDelta(reg.deltaSec, reg.previousSec)} |`,
      );
    }
  }
  lines.push("");
  return lines.join("\n");
}

export async function runReleaseTimingReport({
  repo = process.env.GITHUB_REPOSITORY || "",
  runId = Number(process.env.GITHUB_RUN_ID || 0),
  token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN || "",
  summaryPath = process.env.GITHUB_STEP_SUMMARY || "",
  fetchImpl = fetch,
} = {}) {
  if (!/^[^/\s]+\/[^/\s]+$/.test(repo)) throw new Error(`invalid repository '${repo}'`);
  if (!Number.isInteger(runId) || runId <= 0) throw new Error(`invalid GITHUB_RUN_ID '${runId}'`);

  const apiBase = `https://api.github.com/repos/${repo}`;
  const [jobsResp, runsResp] = await Promise.all([
    fetchGitHubJson(`${apiBase}/actions/runs/${runId}/jobs?per_page=100`, token, fetchImpl),
    fetchGitHubJson(`${apiBase}/actions/workflows/release.yml/runs?status=success&per_page=20`, token, fetchImpl),
  ]);

  const candidates = (runsResp.workflow_runs ?? [])
    .filter((run) => run && run.id !== runId)
    .slice(0, MAX_CANDIDATE_RUNS);
  let previousRun = null;
  let previousJobs = [];
  for (const candidate of candidates) {
    const candidateJobsResp = await fetchGitHubJson(
      `${apiBase}/actions/runs/${candidate.id}/jobs?per_page=100`,
      token,
      fetchImpl,
    );
    const candidateJobs = candidateJobsResp.jobs ?? [];
    if (isSubstantiveReleaseJobs(candidateJobs)) {
      previousRun = candidate;
      previousJobs = candidateJobs;
      break;
    }
  }

  const report = computeTimingReport({
    currentRunId: runId,
    currentJobs: jobsResp.jobs ?? [],
    previousRun,
    previousJobs,
  });

  if (report.spanRegression) {
    console.warn(
      `::warning::Release wall-clock span regressed from ${formatDuration(report.previousSpanSec)} to ${formatDuration(report.currentSpanSec)} (${formatDelta(report.spanDeltaSec, report.previousSpanSec)})`,
    );
  }
  for (const reg of report.jobRegressions) {
    console.warn(
      `::warning::Release job '${escapeWorkflowCommand(reg.job)}' regressed from ${formatDuration(reg.previousSec)} to ${formatDuration(reg.currentSec)} (${formatDelta(reg.deltaSec, reg.previousSec)})`,
    );
  }
  for (const reg of report.stepRegressions.slice(0, 10)) {
    console.warn(
      `::warning::Step '${escapeWorkflowCommand(reg.step)}' in '${escapeWorkflowCommand(reg.job)}' regressed from ${formatDuration(reg.previousSec)} to ${formatDuration(reg.currentSec)} (${formatDelta(reg.deltaSec, reg.previousSec)})`,
    );
  }

  if (summaryPath) {
    fs.appendFileSync(summaryPath, `${formatTimingMarkdown(report)}\n`);
  }
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  runReleaseTimingReport()
    .then((report) => {
      console.log(
        `Release timing summary computed for run ${report.currentRunId} (${report.jobRows.length} jobs, span ${formatDuration(report.currentSpanSec)}).`,
      );
    })
    .catch((err) => {
      console.error(`::warning::Release timing report unavailable: ${escapeWorkflowCommand(err.message)}`);
      process.exit(0);
    });
}
