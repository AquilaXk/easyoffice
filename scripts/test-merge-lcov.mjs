import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { mergeLcov } from "./merge-lcov.mjs";

const SCRIPT = fileURLToPath(new URL("./merge-lcov.mjs", import.meta.url));

const SHARD_ONE = `TN:
SF:scripts/a.mjs
FN:3,first
FNDA:2,first
FNF:1
FNH:1
BRDA:3,0,0,2
BRDA:5,1,0,-
BRDA:5,2,0,4
BRF:3
BRH:2
DA:3,2
DA:4,2
DA:9,0
LH:2
LF:3
end_of_record
SF:scripts/only-one.mjs
BRF:0
BRH:0
DA:1,4
LH:1
LF:1
end_of_record
`;

// The second shard lists other blocks of the file: the block numbers do not line up with the first shard's.
const SHARD_TWO = `TN:
SF:scripts/b.mjs
BRF:0
BRH:0
DA:1,0
LH:0
LF:1
end_of_record
SF:scripts/a.mjs
BRDA:3,0,0,1
BRDA:5,7,0,7
BRDA:8,8,0,0
BRF:3
BRH:2
DA:3,1
DA:4,0
DA:9,3
LH:2
LF:3
end_of_record
`;

function record(text, file) {
  const start = text.indexOf(`SF:${file}\n`);
  assert.ok(start >= 0, `${file} is in the report`);
  return text.slice(start, text.indexOf("end_of_record", start)).split("\n");
}

test("a file that two shards loaded gets the sum of its line hits, and the totals follow", () => {
  const lines = record(mergeLcov([SHARD_ONE, SHARD_TWO]), "scripts/a.mjs");
  assert.deepEqual(lines.filter((l) => l.startsWith("DA:")), ["DA:3,3", "DA:4,2", "DA:9,3"]);
  assert.deepEqual(lines.filter((l) => /^(LH|LF):/.test(l)), ["LH:3", "LF:3"]);
});

test("the blocks of a line are joined by their order on the line, whatever their block numbers", () => {
  const lines = record(mergeLcov([SHARD_ONE, SHARD_TWO]), "scripts/a.mjs");
  // Line 5: the first report has two blocks (a never-reached `-` and 4 hits), the second has one with 7 hits, so the
  // first block of the line gets 0 + 7 and the second 4. Line 8 only the second report has. The block numbers of the
  // output run on from 0 over the whole file.
  assert.deepEqual(lines.filter((l) => l.startsWith("BRDA:")), ["BRDA:3,0,0,3", "BRDA:5,1,0,7", "BRDA:5,2,0,4", "BRDA:8,3,0,0"]);
  assert.deepEqual(lines.filter((l) => /^(BRF|BRH):/.test(l)), ["BRF:4", "BRH:3"]);
});

test("function lines are left out, and a file that one shard loaded stays as it was, files in name order", () => {
  const merged = mergeLcov([SHARD_TWO, SHARD_ONE]);
  assert.doesNotMatch(merged, /^(FN|FNDA|FNF|FNH):/m);
  assert.deepEqual(merged.split("\n").filter((l) => l.startsWith("SF:")), ["SF:scripts/a.mjs", "SF:scripts/b.mjs", "SF:scripts/only-one.mjs"]);
  assert.deepEqual(record(merged, "scripts/only-one.mjs").filter((l) => /^(DA|LH|LF|BRF|BRH):/.test(l)), ["BRF:0", "BRH:0", "DA:1,4", "LH:1", "LF:1"]);
  assert.equal(mergeLcov([SHARD_ONE, SHARD_TWO]), mergeLcov([SHARD_TWO, SHARD_ONE]), "the order of the shards does not matter");
});

test("one report merged alone keeps its lines and branches, and an empty report has no files", () => {
  const alone = mergeLcov([SHARD_ONE]);
  assert.deepEqual(record(alone, "scripts/a.mjs").filter((l) => /^(BRDA|DA):/.test(l)), ["BRDA:3,0,0,2", "BRDA:5,1,0,0", "BRDA:5,2,0,4", "DA:3,2", "DA:4,2", "DA:9,0"]);
  assert.equal(mergeLcov([alone]), alone, "a merged report merges to itself");
  assert.equal(mergeLcov([""]), "");
});

test("the command writes the merged report, and refuses empty input and a missing argument", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "merge-lcov-"));
  try {
    const one = path.join(dir, "js-1.lcov");
    const two = path.join(dir, "js-2.lcov");
    const out = path.join(dir, "js.lcov");
    fs.writeFileSync(one, SHARD_ONE);
    fs.writeFileSync(two, SHARD_TWO);
    const ok = spawnSync(process.execPath, [SCRIPT, out, one, two], { encoding: "utf8" });
    assert.equal(ok.status, 0, ok.stderr);
    assert.equal(fs.readFileSync(out, "utf8"), mergeLcov([SHARD_ONE, SHARD_TWO]));

    const empty = path.join(dir, "empty.lcov");
    fs.writeFileSync(empty, "");
    const refused = spawnSync(process.execPath, [SCRIPT, path.join(dir, "none.lcov"), empty], { encoding: "utf8" });
    assert.equal(refused.status, 1);
    assert.equal(fs.existsSync(path.join(dir, "none.lcov")), false, "no empty report is written");

    const usage = spawnSync(process.execPath, [SCRIPT, out], { encoding: "utf8" });
    assert.equal(usage.status, 2);
    assert.match(usage.stderr, /usage/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
