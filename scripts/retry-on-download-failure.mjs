#!/usr/bin/env node
// Runs a command and runs it again only when the Tauri bundler failed while downloading its tools
// (PD-118). The bundler fetches NSIS, linuxdeploy and AppRun from GitHub releases on every cold build,
// and those downloads occasionally answer 5xx. Compile and test failures never match, so they fail at once.
//
// Usage: node scripts/retry-on-download-failure.mjs -- <command> [args...]
// The command runs through bash ("$@") so arguments reach it verbatim on every OS (Windows runners ship Git bash).
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

export const MAX_ATTEMPTS = 3;
export const DEFAULT_BASE_DELAY_MS = 15_000;
const OUTPUT_TAIL_BYTES = 256 * 1024;

// Only the bundle stage prints "failed to bundle project"; the cause after it must be a network error.
const DOWNLOAD_FAILURE =
  /failed to bundle project[^\n]*?(http status: (?:408|429|5\d\d)|error sending request|timed out|timeout|connection (?:reset|closed|refused|aborted)|dns error|unexpected eof)/i;

export function isDownloadFailure(output) {
  return DOWNLOAD_FAILURE.test(output);
}

// Wait before attempt n+1 (n starts at 1): base, 2 * base, ...
export function backoffMs(failedAttempt, baseMs = DEFAULT_BASE_DELAY_MS) {
  return baseMs * 2 ** (failedAttempt - 1);
}

function runOnce(command) {
  return new Promise((resolve, reject) => {
    const child = spawn("bash", ["-c", 'exec "$@"', "bash", ...command], { stdio: ["inherit", "pipe", "pipe"] });
    let tail = "";
    const forward = (stream, sink) =>
      stream.on("data", (chunk) => {
        sink.write(chunk);
        tail = (tail + chunk.toString("utf8")).slice(-OUTPUT_TAIL_BYTES);
      });
    forward(child.stdout, process.stdout);
    forward(child.stderr, process.stderr);
    child.on("error", reject);
    child.on("close", (code) => resolve({ code: code ?? 1, output: tail }));
  });
}

export async function runWithRetry(command, { baseDelayMs = DEFAULT_BASE_DELAY_MS, maxAttempts = MAX_ATTEMPTS } = {}) {
  for (let attempt = 1; ; attempt += 1) {
    const { code, output } = await runOnce(command);
    if (code === 0) return 0;
    if (attempt >= maxAttempts || !isDownloadFailure(output)) return code;
    const wait = backoffMs(attempt, baseDelayMs);
    console.error(`::warning::Tauri tool download failed (attempt ${attempt}/${maxAttempts}); retrying in ${wait / 1000}s`);
    await new Promise((done) => setTimeout(done, wait));
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const separator = process.argv.indexOf("--");
  const command = process.argv.slice(separator + 1);
  if (separator === -1 || command.length === 0) {
    console.error("usage: retry-on-download-failure.mjs -- <command> [args...]");
    process.exit(2);
  }
  const baseDelayMs = Number(process.env.RETRY_BASE_DELAY_MS ?? DEFAULT_BASE_DELAY_MS);
  process.exit(await runWithRetry(command, { baseDelayMs }));
}
