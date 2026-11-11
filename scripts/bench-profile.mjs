#!/usr/bin/env node
/**
 * Samples one measurement case with macOS `sample` and prints the busiest functions: self time and
 * inclusive time, as a share of the samples of busy threads (idle waits are left out, so threads add
 * up: a function on four worker threads can pass 100% of one thread's time). Function names are
 * demangled with `rustfilt` (`cargo install rustfilt`). Not part of CI.
 *
 * Usage: node scripts/bench-profile.mjs --suite desktop|app|quicklook --case <case> --input <input>
 *          [--repeat 12] [--seconds 6] [--top 15] [--incl 30] [--match <regex for the inclusive list>]
 *          [--keep <file>] [--from <kept sample file>: only summarize it]
 * `--repeat` repeats the case inside one process so that it lasts longer than the sampling window.
 * The quicklook suite needs its own CARGO_TARGET_DIR (separate workspace).
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync, execFileSync } from "node:child_process";
import { testBinary } from "./bench-support.mjs";

const SUITES = {
  desktop: { cargo: ["-p", "easyoffice-desktop"], test: "perf_bench::bench_case" },
  app: { cargo: ["-p", "easyoffice-app"], test: "perf_bench::bench_case" },
  quicklook: { cargo: ["--manifest-path", "apps/quicklook/rust/Cargo.toml"], test: "bench_tests::bench_case" },
};
const IDLE = /^(semaphore_timedwait_trap|__ulock_wait2?|__psynch_cvwait|__psynch_mutexwait|mach_msg2_trap|mach_msg_trap|kevent|kevent_id|__workq_kernreturn|_pthread_cond_wait|__semwait_signal|nanosleep|__wait4|read|select|poll)$/;

function parseArgs(argv) {
  const opts = { suite: "desktop", case: null, input: null, repeat: 12, seconds: 6, top: 15, incl: 30, match: "", keep: null, from: null };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i].replace(/^--/, "");
    if (!(key in opts)) throw new Error(`unknown argument ${argv[i]}`);
    opts[key] = typeof opts[key] === "number" ? Number(argv[i + 1]) : argv[i + 1];
  }
  if (!opts.from && (!SUITES[opts.suite] || !opts.case || !opts.input)) throw new Error("--suite, --case and --input are required");
  return opts;
}

const demangle = (symbols) =>
  execFileSync("rustfilt", { input: symbols.join("\n"), encoding: "utf8" })
    .split("\n")
    .map((name) => name.replace(/::h[0-9a-f]{16}$/, ""));
const library = (name) => name.replace(/-[0-9a-f]{16}$/, "");

function summarize(text, opts) {
  const at = text.indexOf("Sort by top of stack");
  const rows = [];
  for (const line of text.slice(at).split("\n").slice(1)) {
    const m = /^\s+(\S.*?)\s{2,}\(in (.+?)\)\s+(\d+)\s*$/.exec(line);
    if (m) rows.push({ symbol: m[1], lib: m[2], count: Number(m[3]) });
    else if (line.startsWith("Binary Images")) break;
  }
  const busy = rows.filter((row) => !IDLE.test(row.symbol));
  const total = busy.reduce((sum, row) => sum + row.count, 0);
  const names = demangle(busy.map((row) => row.symbol));
  const self = new Map();
  busy.forEach((row, i) => {
    const key = `${names[i]} [${library(row.lib)}]`;
    self.set(key, (self.get(key) ?? 0) + row.count);
  });
  console.log(`busy samples ${total}`);
  console.log("-- self time");
  for (const [key, count] of [...self].sort((a, b) => b[1] - a[1]).slice(0, opts.top)) {
    console.log(`${((100 * count) / total).toFixed(1).padStart(5)}%  ${String(count).padStart(6)}  ${key.slice(0, 200)}`);
  }
  // Inclusive time: walk the call graph and count each function once per stack.
  const nodes = [];
  for (const line of text.slice(text.indexOf("Call graph:"), at).split("\n").slice(1)) {
    const m = /^([ +!:|]*?)(\d+) (.+?)  \(in (.+?)\)/.exec(line);
    if (m) nodes.push({ depth: m[1].length, count: Number(m[2]), symbol: m[3].trim(), lib: m[4] });
  }
  const distinct = [...new Set(nodes.map((node) => node.symbol))];
  const readable = new Map(distinct.map((symbol, i) => [symbol, demangle(distinct)[i]]));
  const stack = [];
  const inclusive = new Map();
  for (const node of nodes) {
    while (stack.length && stack[stack.length - 1].depth >= node.depth) stack.pop();
    const name = readable.get(node.symbol);
    if (!IDLE.test(node.symbol) && !stack.some((frame) => frame.name === name)) {
      const key = `${name} [${library(node.lib)}]`;
      inclusive.set(key, (inclusive.get(key) ?? 0) + node.count);
    }
    stack.push({ depth: node.depth, name });
  }
  const pattern = opts.match ? new RegExp(opts.match) : null;
  console.log("-- inclusive time");
  for (const [key, count] of [...inclusive].filter(([key]) => !pattern || pattern.test(key)).sort((a, b) => b[1] - a[1]).slice(0, opts.incl)) {
    console.log(`${((100 * count) / total).toFixed(1).padStart(5)}%  ${String(count).padStart(6)}  ${key.slice(0, 200)}`);
  }
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.from) return summarize(fs.readFileSync(opts.from, "utf8"), opts);
  const suite = SUITES[opts.suite];
  const binary = testBinary(suite.cargo);
  const env = { ...process.env, EASYOFFICE_BENCH_CASE: opts.case, EASYOFFICE_BENCH_INPUT: opts.input, EASYOFFICE_BENCH_REPEAT: String(opts.repeat) };
  const child = spawn(binary, [suite.test, "--ignored", "--exact", "--nocapture"], { env, stdio: "ignore" });
  await new Promise((resolve) => setTimeout(resolve, 1500));
  const file = opts.keep ?? path.join(os.tmpdir(), `easyoffice-sample-${child.pid}.txt`);
  spawnSync("sample", [String(child.pid), String(opts.seconds), "-file", file], { stdio: "ignore" });
  child.kill();
  summarize(fs.readFileSync(file, "utf8"), opts);
  if (!opts.keep) fs.rmSync(file, { force: true });
}

main();
