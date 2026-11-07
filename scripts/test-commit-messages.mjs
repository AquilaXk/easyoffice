import test from "node:test";
import assert from "node:assert/strict";
import { checkPullRequest, commitMessages, findProhibited } from "./check-commit-messages.mjs";
import { PROHIBITED_CONTENT_B64 } from "./prohibited-terms.mjs";

const decode = (b64) => Buffer.from(b64, "base64").toString("utf8");
const TRAILER = decode("Q28tQXV0aG9yZWQtQnk6") + " someone <a@b.c>";
const SESSION = decode("Q2xhdWRlLVNlc3Npb246") + " https://example.invalid/x";

test("clean commits, title and body pass", () => {
  const commits = [{ sha: "a".repeat(40), message: "[Feat] 기능 추가\n\n- 설명" }];
  assert.deepEqual(checkPullRequest({ commits, title: "[Feat] 기능", body: "## 요약" }), []);
});

test("attribution trailers in any commit, the title or the body are reported", () => {
  const commits = [
    { sha: "b".repeat(40), message: `[Fix] 수정\n\n${TRAILER}` },
    { sha: "c".repeat(40), message: `[Fix] 수정\n\n${SESSION}` },
  ];
  const problems = checkPullRequest({ commits, title: "ok", body: `본문\n${TRAILER}` });
  assert.equal(problems.length, 3);
  assert.ok(problems[0].startsWith("commit bbbbbbbb"));
  assert.ok(problems[1].startsWith("commit cccccccc"));
  assert.ok(problems[2].startsWith("PR body"));
  assert.ok(problems.every((p) => !p.includes(TRAILER)), "the report does not echo the term");
});

test("every shared prohibited term is caught case-insensitively", () => {
  for (const b64 of PROHIBITED_CONTENT_B64) {
    assert.equal(findProhibited("t", `x ${decode(b64).toUpperCase()} y`).length >= 1, true, b64);
  }
});

test("commit ranges are parsed from git log records and must be SHAs", () => {
  const fake = (cmd, args) => {
    assert.equal(cmd, "git");
    assert.equal(args.at(-1), "1111111..2222222");
    return `${"d".repeat(40)}\n첫 줄\n\n본문\n\u001e\n${"e".repeat(40)}\n둘째\n\u001e\n`;
  };
  assert.deepEqual(commitMessages("1111111", "2222222", fake), [
    { sha: "d".repeat(40), message: "첫 줄\n\n본문" },
    { sha: "e".repeat(40), message: "둘째" },
  ]);
  assert.throws(() => commitMessages("main", "HEAD", fake), /SHAs/);
});
