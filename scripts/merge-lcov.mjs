import fs from "node:fs";
import { pathToFileURL } from "node:url";

// Merges the lcov reports of the Node test shards (`node --test --test-shard=i/n`) into one report, so the static
// analysis job reads a single `coverage/js.lcov` as before (PD-369). A file that two shards loaded gets the sum of
// its hit counts: line hits (DA), function hits (FNDA) and branch hits (BRDA). The block and branch numbers of
// BRDA come from the source text alone, so the same file has the same ones in every shard.

const compareText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const compareNumber = (a, b) => a - b;

function newRecord() {
  return { functions: new Map(), branches: new Map(), lines: new Map(), pendingNames: [], pendingHits: [] };
}

function addHits(map, key, hits) {
  map.set(key, (map.get(key) ?? 0) + hits);
}

/** A BRDA hit count is a number, or `-` for a branch the block never reached; both add up as numbers. */
function branchHits(text) {
  return text === "-" ? 0 : Number(text);
}

function readRecordLine(record, line) {
  const colon = line.indexOf(":");
  if (colon < 0) return;
  const tag = line.slice(0, colon);
  const body = line.slice(colon + 1);
  if (tag === "FN") {
    record.pendingNames.push(body);
  } else if (tag === "FNDA") {
    record.pendingHits.push(Number(body.slice(0, body.indexOf(","))));
  } else if (tag === "BRDA") {
    const [lineNo, block, branch, taken] = body.split(",");
    addHits(record.branches, `${lineNo},${block},${branch}`, branchHits(taken));
  } else if (tag === "DA") {
    const [lineNo, hits] = body.split(",");
    addHits(record.lines, Number(lineNo), Number(hits));
  }
}

/**
 * A function is named by `FN:<line>,<name>` and its hits come in the `FNDA` lines in the same order (two functions
 * of a file can share a name), so the pairs are joined when the record ends.
 */
function closeRecord(record) {
  const seen = new Map();
  record.pendingNames.forEach((name, index) => {
    // The same line and name can repeat (the arrow functions of one line); the n-th repeat of a text is the
    // n-th repeat of every other text of the same source.
    const nth = seen.get(name) ?? 0;
    seen.set(name, nth + 1);
    addHits(record.functions, `${name}#${nth}`, record.pendingHits[index] ?? 0);
  });
  record.pendingNames = [];
  record.pendingHits = [];
}

/** Reads lcov texts into `Map<source file, record>`, adding the hits of a file that several texts hold. */
export function parseLcovInto(files, text) {
  let record = null;
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith("SF:")) {
      const name = line.slice(3);
      record = files.get(name) ?? newRecord();
      files.set(name, record);
    } else if (line === "end_of_record") {
      if (record) closeRecord(record);
      record = null;
    } else if (record) {
      readRecordLine(record, line);
    }
  }
  return files;
}

function functionLine(key) {
  return Number(key.slice(0, key.indexOf(",")));
}

/** `<line>,<name>#<n>` back to the `<line>,<name>` an FN line carries. */
function functionLabel(key) {
  return key.slice(0, key.lastIndexOf("#"));
}

function formatFunctions(record, out) {
  const keys = [...record.functions.keys()].sort((a, b) => functionLine(a) - functionLine(b) || compareText(a, b));
  let hit = 0;
  for (const key of keys) out.push(`FN:${functionLabel(key)}`);
  for (const key of keys) {
    const hits = record.functions.get(key);
    if (hits > 0) hit += 1;
    const label = functionLabel(key);
    out.push(`FNDA:${hits},${label.slice(label.indexOf(",") + 1)}`);
  }
  out.push(`FNF:${keys.length}`, `FNH:${hit}`);
}

function formatBranches(record, out) {
  const keys = [...record.branches.keys()].sort((a, b) => {
    const [la, ba, ca] = a.split(",").map(Number);
    const [lb, bb, cb] = b.split(",").map(Number);
    return la - lb || ba - bb || ca - cb;
  });
  let hit = 0;
  for (const key of keys) {
    const hits = record.branches.get(key);
    if (hits > 0) hit += 1;
    out.push(`BRDA:${key},${hits}`);
  }
  out.push(`BRF:${keys.length}`, `BRH:${hit}`);
}

function formatLines(record, out) {
  const numbers = [...record.lines.keys()].sort(compareNumber);
  let hit = 0;
  for (const lineNo of numbers) {
    const hits = record.lines.get(lineNo);
    if (hits > 0) hit += 1;
    out.push(`DA:${lineNo},${hits}`);
  }
  out.push(`LH:${hit}`, `LF:${numbers.length}`);
}

/** Merges the texts of lcov reports into one lcov text, files in name order. */
export function mergeLcov(texts) {
  const files = new Map();
  for (const text of texts) parseLcovInto(files, text);
  const out = [];
  for (const name of [...files.keys()].sort(compareText)) {
    const record = files.get(name);
    out.push("TN:", `SF:${name}`);
    formatFunctions(record, out);
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
