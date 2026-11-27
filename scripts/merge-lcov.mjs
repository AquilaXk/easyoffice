import fs from "node:fs";
import { pathToFileURL } from "node:url";

// Merges the lcov reports of the Node test shards (`node --test --test-shard=i/n`) into one report, so the static
// analysis job reads a single `coverage/js.lcov` as before (PD-370).
//
// Lines: a file that several shards loaded gets the sum of its DA hit counts.
// Branches: V8 reports a block only where its count differs from the enclosing one, so which blocks a report lists
// (and the BRDA block numbers) depends on what the shard ran, and two reports cannot be joined by block number. The
// blocks of one line are joined by their order on the line instead and their hits are summed. Measured against one
// unsharded run of the same suites the branch totals differ by under 2%.
// Functions: FN and FNDA lines are left out. They have the same problem and nothing reads them (the analysis uses
// line hits and branches).

const compareText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const compareNumber = (a, b) => a - b;

/** A BRDA hit count is a number, or `-` for a block the code never reached; both add up as numbers. */
function branchHits(text) {
  return text === "-" ? 0 : Number(text);
}

function addLineHits(record, line, hits) {
  record.lines.set(line, (record.lines.get(line) ?? 0) + hits);
}

/** The blocks of a line come in block-number order; the n-th block of this report joins the n-th of the others. */
function addBranchHits(record, line, orderedHits) {
  const merged = record.branches.get(line) ?? [];
  orderedHits.forEach((hits, index) => {
    merged[index] = (merged[index] ?? 0) + hits;
  });
  record.branches.set(line, merged);
}

function closeRecord(record, pendingBranches) {
  for (const [line, blocks] of pendingBranches) {
    addBranchHits(
      record,
      line,
      blocks.sort((a, b) => a.block - b.block).map((b) => b.hits),
    );
  }
}

/** Reads an lcov text into `files` (`Map<source file, { lines, branches }>`), adding to files already there. */
export function parseLcovInto(files, text) {
  let record = null;
  let pendingBranches = new Map();
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith("SF:")) {
      const name = line.slice(3);
      record = files.get(name) ?? { lines: new Map(), branches: new Map() };
      files.set(name, record);
      pendingBranches = new Map();
    } else if (line === "end_of_record") {
      if (record) closeRecord(record, pendingBranches);
      record = null;
    } else if (record && line.startsWith("DA:")) {
      const [lineNo, hits] = line.slice(3).split(",");
      addLineHits(record, Number(lineNo), Number(hits));
    } else if (record && line.startsWith("BRDA:")) {
      const [lineNo, block, , taken] = line.slice(5).split(",");
      const blocks = pendingBranches.get(Number(lineNo)) ?? [];
      blocks.push({ block: Number(block), hits: branchHits(taken) });
      pendingBranches.set(Number(lineNo), blocks);
    }
  }
  return files;
}

function formatBranches(record, out) {
  const lineNumbers = [...record.branches.keys()].sort(compareNumber);
  let total = 0;
  let hit = 0;
  for (const lineNo of lineNumbers) {
    for (const hits of record.branches.get(lineNo)) {
      out.push(`BRDA:${lineNo},${total},0,${hits}`);
      total += 1;
      if (hits > 0) hit += 1;
    }
  }
  out.push(`BRF:${total}`, `BRH:${hit}`);
}

function formatLines(record, out) {
  const lineNumbers = [...record.lines.keys()].sort(compareNumber);
  let hit = 0;
  for (const lineNo of lineNumbers) {
    const hits = record.lines.get(lineNo);
    if (hits > 0) hit += 1;
    out.push(`DA:${lineNo},${hits}`);
  }
  out.push(`LH:${hit}`, `LF:${lineNumbers.length}`);
}

/** Merges the texts of lcov reports into one lcov text, files in name order. */
export function mergeLcov(texts) {
  const files = new Map();
  for (const text of texts) parseLcovInto(files, text);
  const out = [];
  for (const name of [...files.keys()].sort(compareText)) {
    const record = files.get(name);
    out.push("TN:", `SF:${name}`);
    formatBranches(record, out);
    formatLines(record, out);
    out.push("end_of_record");
  }
  return out.length > 0 ? out.join("\n") + "\n" : "";
}

function main(argv) {
  const [output, ...inputs] = argv;
  if (!output || inputs.length === 0) {
    console.error("usage: node scripts/merge-lcov.mjs <output.lcov> <shard.lcov>...");
    return 2;
  }
  const merged = mergeLcov(inputs.map((file) => fs.readFileSync(file, "utf8")));
  if (merged === "") {
    console.error("the shard reports hold no source file; refusing to write an empty coverage report");
    return 1;
  }
  fs.writeFileSync(output, merged);
  return 0;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) process.exitCode = main(process.argv.slice(2));
