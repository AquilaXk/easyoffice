// Two builds of the engine measured against each other (docs/performance.md, "PD-397").
//
//   node scripts/bench-ab.mjs <case> <input> <repeat> <old test binary> <new test binary> [rounds]
//   node scripts/bench-ab.mjs open  /tmp/easyoffice-bench/engine-inputs/slidesext-large.pptx 12 app-before app-after 3
//   node scripts/bench-ab.mjs sheetpdf x 12 app-before app-after 3
//
// The binaries are the unit-test executables of `easyoffice-app` (`target/release/deps/easyoffice_app-*`, copied
// before the second build overwrites the first). They run in turns, `rounds` times each, and every run repeats the case
// `repeat` times inside its process. A busy machine only adds time, so the minimum of all runs is the figure; the median
// shows how much the machine moved. B/A under 1 is faster. `case` and `input` are those of `bench-run.mjs`.
import { spawnSync } from "node:child_process";

const [benchCase, input, repeat, binA, binB, roundsArg] = process.argv.slice(2);
if (!benchCase || !input || !repeat || !binA || !binB) {
  console.error("usage: node scripts/bench-ab.mjs <case> <input> <repeat> <old binary> <new binary> [rounds]");
  process.exit(2);
}
const rounds = Number(roundsArg || 3);
const samples = { A: {}, B: {} };

for (let round = 0; round < rounds; round += 1) {
  for (const [side, binary] of [["A", binA], ["B", binB]]) {
    const out = spawnSync(binary, ["perf_bench::bench_case", "--ignored", "--exact", "--nocapture"], {
      env: { ...process.env, EASYOFFICE_BENCH_CASE: benchCase, EASYOFFICE_BENCH_INPUT: input, EASYOFFICE_BENCH_REPEAT: repeat },
      encoding: "utf8",
      maxBuffer: 1 << 26,
    });
    for (const line of out.stdout.split("\n").filter((l) => l.startsWith("BENCH "))) {
      const fields = Object.fromEntries(line.slice(6).split(" ").map((pair) => [pair.slice(0, pair.indexOf("=")), pair.slice(pair.indexOf("=") + 1)]));
      const phases = Object.fromEntries((fields.phases || "").split(",").filter(Boolean).map((pair) => pair.split("=")));
      (samples[side].total_ms ??= []).push(Number(fields.total_ms));
      for (const [phase, value] of Object.entries(phases)) (samples[side][phase] ??= []).push(Number(value));
    }
  }
}
if (!samples.A.total_ms || !samples.B.total_ms) {
  console.error("no BENCH line: the case or the input is not one of perf_bench.rs");
  process.exit(1);
}

const sorted = (values) => [...values].sort((a, b) => a - b);
const show = (values) => `${sorted(values)[0].toFixed(1).padStart(8)} / ${sorted(values)[values.length >> 1].toFixed(1).padStart(8)}`;
console.log(`${benchCase} ${input.split("/").pop()}  (min / median ms over ${samples.A.total_ms.length} runs per side)`);
for (const key of Object.keys(samples.A)) {
  const ratio = Math.min(...samples.B[key]) / Math.min(...samples.A[key]);
  console.log(`  ${key.padEnd(14)} A ${show(samples.A[key])}   B ${show(samples.B[key])}   B/A ${ratio.toFixed(3)}`);
}
