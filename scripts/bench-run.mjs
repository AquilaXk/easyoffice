#!/usr/bin/env node
/**
 * Runs the measurement harnesses behind docs/performance.md. Not part of CI: it builds three test
 * binaries in release mode and runs each case in a fresh process N times under `/usr/bin/time -l`
 * (macOS), then prints the median time and the median peak RSS per case.
 *
 * Usage: node scripts/bench-run.mjs [--runs 5] [--suite hwp|synthetic|engine|docpdf|quicklook|all]
 *          [--work <dir>] [--json <file>] [--only <text in the case id>] [--docs <file,file>]
 *          [--ref-bin <path>] [--ref-skia-bin <path>]
 *   --ref-bin / --ref-skia-bin  a binary of the reference app's document paths (built apart from this
 *              repository) that takes `<case> <input>` and prints the same `BENCH ...` line; the
 *              in-repo `ref-repro` cases run without it.
 *
 * Needs the rhwp checkout (`node scripts/checkout-rhwp.mjs`) for the sample documents. Cases are
 * defined in crates/easyoffice-desktop/src/perf_bench.rs, crates/easyoffice-app/src/perf_bench.rs and
 * apps/quicklook/rust/src/bench_tests.rs. Run the quicklook suite with its own CARGO_TARGET_DIR
 * (that crate is a separate workspace).
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { testBinary } from "./bench-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SAMPLES = path.join(root, "third_party/rhwp/rhwp-studio/public/samples");
const BLANK = path.join(root, "third_party/rhwp/saved/blank2010.hwp");

/** The Hangul documents measured: small, medium, many pages, table heavy and picture heavy. */
const HWP_INPUTS = [
  { label: "작은 것", file: path.join(SAMPLES, "shift-return.hwp") },
  { label: "빈 문서", file: BLANK },
  { label: "보통(6쪽)", file: path.join(SAMPLES, "biz_plan.hwp") },
  { label: "HWPX 표(10쪽)", file: path.join(SAMPLES, "form-002.hwpx") },
  { label: "긴 문서(77쪽)", file: path.join(SAMPLES, "kps-ai.hwp") },
  { label: "표 많음(115쪽)", file: path.join(SAMPLES, "issue1949_giant_cell_nested_tables_perf.hwp") },
  { label: "그림 많음", file: path.join(SAMPLES, "basic/KTX.hwp") },
  { label: "그림 많음 2", file: path.join(SAMPLES, "number-bullet.hwp") },
];

function parseArgs(argv) {
  const opts = { runs: 5, suite: "all", work: path.join(os.tmpdir(), "easyoffice-bench"), json: null, only: null, docs: [], refBin: null, refSkiaBin: null };
  for (let i = 0; i < argv.length; i++) {
    const next = () => argv[++i];
    if (argv[i] === "--runs") opts.runs = Number(next());
    else if (argv[i] === "--suite") opts.suite = next();
    else if (argv[i] === "--work") opts.work = next();
    else if (argv[i] === "--json") opts.json = next();
    else if (argv[i] === "--only") opts.only = next();
    else if (argv[i] === "--docs") opts.docs = next().split(",").filter(Boolean).map((file) => path.resolve(file));
    else if (argv[i] === "--ref-bin") opts.refBin = next();
    else if (argv[i] === "--ref-skia-bin") opts.refSkiaBin = next();
    else throw new Error(`unknown argument ${argv[i]}`);
  }
  return opts;
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

/** One process: its BENCH line and the peak RSS in bytes from `/usr/bin/time -l`. */
function runOnce(command, args, env) {
  const out = spawnSync("/usr/bin/time", ["-l", command, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
    maxBuffer: 1 << 26,
  });
  const text = `${out.stdout}\n${out.stderr}`;
  const line = text.split("\n").find((l) => l.startsWith("BENCH "));
  const rss = /(\d+)\s+maximum resident set size/.exec(text);
  // `time -l` prints "<real> real <user> user <sys> sys": CPU time does not grow when other work
  // keeps the cores busy, so it is reported next to the wall time.
  const cpu = /([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys/.exec(text);
  if (!line || !rss) throw new Error(`no result from ${command} ${args.join(" ")}\n${text.slice(-600)}`);
  const fields = Object.fromEntries(
    line
      .slice(6)
      .split(" ")
      .map((pair) => {
        const at = pair.indexOf("=");
        return [pair.slice(0, at), pair.slice(at + 1)];
      }),
  );
  const phases = Object.fromEntries(
    (fields.phases || "")
      .split(",")
      .filter(Boolean)
      .map((pair) => pair.split("=")),
  );
  return {
    totalMs: Number(fields.total_ms),
    outBytes: Number(fields.out_bytes),
    pages: Number(fields.pages),
    phases: Object.fromEntries(Object.entries(phases).map(([k, v]) => [k, Number(v)])),
    rssBytes: Number(rss[1]),
    cpuMs: cpu ? (Number(cpu[2]) + Number(cpu[3])) * 1000 : NaN,
  };
}

function measure(job, runs) {
  const samples = [];
  for (let i = 0; i < runs; i++) samples.push(runOnce(job.command, job.args, job.env));
  const phaseNames = Object.keys(samples[0].phases);
  return {
    id: job.id,
    tool: job.tool,
    case: job.case,
    input: job.input,
    label: job.label,
    runs,
    medianMs: median(samples.map((s) => s.totalMs)),
    minMs: Math.min(...samples.map((s) => s.totalMs)),
    maxMs: Math.max(...samples.map((s) => s.totalMs)),
    peakRssMb: median(samples.map((s) => s.rssBytes)) / 1048576,
    cpuMs: median(samples.map((s) => s.cpuMs)),
    outBytes: samples[0].outBytes,
    pages: samples[0].pages,
    phasesMs: Object.fromEntries(phaseNames.map((n) => [n, median(samples.map((s) => s.phases[n] ?? 0))])),
  };
}

function benchJob({ tool, binary, args, benchCase, input, label }) {
  const reproduction = benchCase.startsWith("ref:");
  const shownTool = reproduction ? "ref-repro" : tool;
  const shownCase = benchCase.replace(/^ref:/, "");
  return {
    id: `${shownTool}/${shownCase}/${path.basename(input)}`,
    tool: shownTool,
    case: shownCase,
    input: path.basename(input),
    label,
    command: binary,
    args,
    env: { EASYOFFICE_BENCH_CASE: benchCase, EASYOFFICE_BENCH_INPUT: input },
  };
}

function externalJob({ tool, binary, benchCase, input, label }) {
  return { id: `${tool}/${benchCase}/${path.basename(input)}`, tool, case: benchCase, input: path.basename(input), label, command: binary, args: [benchCase, input], env: {} };
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  const want = (suite) => opts.suite === "all" || opts.suite === suite;
  fs.mkdirSync(opts.work, { recursive: true });
  const inputs = HWP_INPUTS.filter((item) => {
    if (fs.existsSync(item.file)) return true;
    console.error(`skip ${item.file}: missing (run node scripts/checkout-rhwp.mjs)`);
    return false;
  });
  const jobs = [];

  if (want("hwp") || want("synthetic")) {
    const desktop = testBinary(["-p", "easyoffice-desktop"]);
    const args = ["perf_bench::bench_case", "--ignored", "--exact", "--nocapture"];
    const run = (benchCase, input, label) => benchJob({ tool: "eo", binary: desktop, args, benchCase, input, label });
    const synthetic = path.join(opts.work, "synthetic-300");
    if (want("synthetic")) {
      spawnSync(desktop, ["perf_bench::bench_dump_synthetic_pages", "--ignored", "--exact"], { env: { ...process.env, EASYOFFICE_BENCH_INPUT: synthetic }, stdio: "inherit" });
      jobs.push(run("pdf_svgs", synthetic, "300쪽 합성"), run("ref:pdf_svgs", synthetic, "300쪽 합성"));
      if (opts.refBin) jobs.push(externalJob({ tool: "ref", binary: opts.refBin, benchCase: "pdf_svgs", input: synthetic, label: "300쪽 합성" }));
    }
    if (want("hwp")) {
      for (const { label, file } of inputs) {
        jobs.push(
          run("save_verify", file, label),
          run("ref:save_verify", file, label),
          run("preview_embedded", file, label),
          run("preview_render", file, label),
          run("ref:preview", file, label),
          run("thumb_embed", file, label),
          run("pdf_doc", file, label),
          run("ref:pdf", file, label),
        );
        if (opts.refBin) {
          for (const benchCase of ["save_verify", "preview", "pdf"]) jobs.push(externalJob({ tool: "ref", binary: opts.refBin, benchCase, input: file, label }));
        }
      }
    }
  }

  if (want("quicklook")) {
    const ql = testBinary(["--manifest-path", "apps/quicklook/rust/Cargo.toml"]);
    for (const { label, file } of inputs) {
      for (const benchCase of ["ql_embedded", "ql_png", "ql_pdf"]) {
        jobs.push(benchJob({ tool: "eo", binary: ql, args: ["bench_tests::bench_case", "--ignored", "--exact", "--nocapture"], benchCase, input: file, label }));
      }
      if (opts.refSkiaBin) {
        for (const benchCase of ["ql_png", "ql_pdf"]) jobs.push(externalJob({ tool: "ref", binary: opts.refSkiaBin, benchCase, input: file, label }));
      }
    }
  }

  if (want("engine")) {
    const app = testBinary(["-p", "easyoffice-app"]);
    const args = ["perf_bench::bench_case", "--ignored", "--exact", "--nocapture"];
    const generated = path.join(opts.work, "engine-inputs");
    spawnSync(app, ["perf_bench::bench_generate_inputs", "--ignored", "--exact", "--nocapture"], { env: { ...process.env, EASYOFFICE_BENCH_OUT: generated }, stdio: "inherit" });
    for (const name of fs.readdirSync(generated).sort((a, b) => a.localeCompare(b))) {
      for (const benchCase of ["open", "save"]) {
        jobs.push(benchJob({ tool: "eo", binary: app, args, benchCase, input: path.join(generated, name), label: name }));
      }
    }
    for (const extension of ["docx", "hwpx", "xlsx", "pptx"]) jobs.push(benchJob({ tool: "eo", binary: app, args, benchCase: "encode", input: extension, label: `encode ${extension}` }));
    for (const rows of ["1000", "10000", "50000"]) jobs.push(benchJob({ tool: "eo", binary: app, args, benchCase: "recalc", input: rows, label: `${rows}행` }));
    jobs.push(benchJob({ tool: "eo", binary: app, args, benchCase: "layout", input: "100000", label: "100000회" }));
  }

  if (want("docpdf")) {
    // A rich document converted to PDF: the engine's writer and the HWPX + rhwp page layout (PD-166).
    // The inputs are the engine suite's generated documents, plus any `--docs` file.
    const app = testBinary(["-p", "easyoffice-app"]);
    const generated = path.join(opts.work, "engine-inputs");
    spawnSync(app, ["perf_bench::bench_generate_inputs", "--ignored", "--exact", "--nocapture"], { env: { ...process.env, EASYOFFICE_BENCH_OUT: generated }, stdio: "inherit" });
    const desktop = testBinary(["-p", "easyoffice-desktop"]);
    const args = ["perf_bench::bench_case", "--ignored", "--exact", "--nocapture"];
    const files = [...["docx", "odt", "rtf"].map((ext) => path.join(generated, `doc-large.${ext}`)), ...opts.docs];
    for (const file of files) {
      for (const benchCase of ["doc_pdf_engine", "doc_pdf_shell"]) jobs.push(benchJob({ tool: "eo", binary: desktop, args, benchCase, input: file, label: path.basename(file) }));
    }
  }

  const results = [];
  for (const job of jobs.filter((j) => !opts.only || j.id.includes(opts.only))) {
    try {
      const result = measure(job, opts.runs);
      results.push(result);
      const phases = Object.entries(result.phasesMs).map(([k, v]) => `${k}=${v.toFixed(1)}`).join(" ");
      console.log(`${job.id}\t${result.medianMs.toFixed(1)} ms\tcpu ${result.cpuMs.toFixed(0)} ms\t${result.peakRssMb.toFixed(0)} MB\t${result.outBytes} B\t${phases}`);
    } catch (err) {
      console.error(`FAILED ${job.id}: ${String(err.message).split("\n")[0]}`);
    }
  }
  if (opts.json) fs.writeFileSync(opts.json, JSON.stringify({ runs: opts.runs, os: `${os.type()} ${os.release()} ${os.arch()}`, cpus: os.cpus()[0]?.model, results }, null, 2));
}

main();
