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
FN:9,(anonymous_0)
FN:9,(anonymous_0)
FNDA:2,first
FNDA:1,(anonymous_0)
FNDA:0,(anonymous_0)
FNF:3
FNH:2
BRDA:3,0,0,2
BRDA:5,1,0,-
BRF:2
BRH:1
DA:3,2
DA:4,2
DA:9,0
LH:2
LF:3
end_of_record
SF:scripts/only-one.mjs
FN:1,run
FNDA:4,run
FNF:1
FNH:1
BRF:0
BRH:0
DA:1,4
LH:1
LF:1
end_of_record
`;

const SHARD_TWO = `TN:
SF:scripts/b.mjs
FN:1,other
FNDA:0,other
FNF:1
FNH:0
BRF:0
BRH:0
DA:1,0
LH:0
LF:1
end_of_record
SF:scripts/a.mjs
FN:3,first
FN:9,(anonymous_0)
FN:9,(anonymous_0)
FNDA:5,first
FNDA:0,(anonymous_0)
FNDA:3,(anonymous_0)
FNF:3
FNH:2
BRDA:3,0,0,1
BRDA:5,1,0,7
BRF:2
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

test("a file that two shards loaded gets the sum of every hit count, and the totals follow", () => {
  const lines = record(mergeLcov([SHARD_ONE, SHARD_TWO]), "scripts/a.mjs");
  assert.deepEqual(lines.filter((l) => l.startsWith("FN:")), ["FN:3,first", "FN:9,(anonymous_0)", "FN:9,(anonymous_0)"]);
  assert.deepEqual(lines.filter((l) => l.startsWith("FNDA:")), ["FNDA:7,first", "FNDA:1,(anonymous_0)", "FNDA:3,(anonymous_0)"], "functions of one name keep their own hits");
  assert.deepEqual(lines.filter((l) => /^(FNF|FNH):/.test(l)), ["FNF:3", "FNH:3"]);
  assert.deepEqual(lines.filter((l) => l.startsWith("BRDA:")), ["BRDA:3,0,0,3", "BRDA:5,1,0,7"], "a branch the first shard never reached (-) takes the hits of the second");
  assert.deepEqual(lines.filter((l) => /^(BRF|BRH):/.test(l)), ["BRF:2", "BRH:2"]);
  assert.deepEqual(lines.filter((l) => l.startsWith("DA:")), ["DA:3,3", "DA:4,2", "DA:9,3"]);
  assert.deepEqual(lines.filter((l) => /^(LH|LF):/.test(l)), ["LH:3", "LF:3"]);
});

test("a file that one shard loaded stays as it was, and the files come in name order", () => {
  const merged = mergeLcov([SHARD_TWO, SHARD_ONE]);
  assert.deepEqual(merged.split("\n").filter((l) => l.startsWith("SF:")), ["SF:scripts/a.mjs", "SF:scripts/b.mjs", "SF:scripts/only-one.mjs"]);
  assert.deepEqual(record(merged, "scripts/only-one.mjs").filter((l) => /^(FNDA|DA|LH|LF):/.test(l)), ["FNDA:4,run", "DA:1,4", "LH:1", "LF:1"]);
  assert.equal(mergeLcov([SHARD_ONE, SHARD_TWO]), mergeLcov([SHARD_TWO, SHARD_ONE]), "the order of the shards does not matter");
});

test("one report merged alone keeps its content, and the merge of merges equals the merge of all", () => {
  const alone = mergeLcov([SHARD_ONE]);
  assert.equal(mergeLcov([alone]), alone);
  const both = mergeLcov([SHARD_ONE, SHARD_TWO]);
  assert.equal(mergeLcov([mergeLcov([SHARD_ONE]), mergeLcov([SHARD_TWO])]), both);
  assert.equal(mergeLcov([""]), "", "an empty report has no files");
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
