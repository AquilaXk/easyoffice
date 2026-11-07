#!/usr/bin/env node
/**
 * Commit message guard for pull requests: the commit messages in `base..head`, the PR title and
 * the PR body must not carry prohibited tool names, external brand names or attribution trailers
 * (the same list as the repository hygiene guard). Squash merges copy commit messages into main, so
 * this stops them before they land.
 *
 * Usage: node scripts/check-commit-messages.mjs <base-sha> <head-sha>
 * The PR title and body are read from PR_TITLE and PR_BODY when set.
 */

import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { PROHIBITED_PATTERNS } from "./prohibited-terms.mjs";

const RECORD_SEPARATOR = "\u001e";
const SHA = /^[0-9a-f]{7,40}$/;

/** Problems in one labelled text: one entry per prohibited pattern found. */
export function findProhibited(label, text) {
  return PROHIBITED_PATTERNS.filter((pattern) => pattern.test(text)).map(
    (pattern) => `${label}: prohibited content (${pattern.source.length} chars)`
  );
}

/** Commit messages of `base..head`, newest first, as `{ sha, message }`. */
export function commitMessages(base, head, run = execFileSync) {
  if (!SHA.test(base) || !SHA.test(head)) throw new Error("base and head must be commit SHAs");
  const out = run("git", ["log", `--format=%H%n%B${RECORD_SEPARATOR}`, `${base}..${head}`], { encoding: "utf8" });
  return out
    .split(RECORD_SEPARATOR)
    .map((record) => record.trim())
    .filter(Boolean)
    .map((record) => {
      const newline = record.indexOf("\n");
      return newline < 0 ? { sha: record, message: "" } : { sha: record.slice(0, newline), message: record.slice(newline + 1) };
    });
}

export function checkPullRequest({ commits, title = "", body = "" }) {
  const problems = commits.flatMap(({ sha, message }) => findProhibited(`commit ${sha.slice(0, 8)}`, message));
  return [...problems, ...findProhibited("PR title", title), ...findProhibited("PR body", body)];
}

function main() {
  const [base, head] = process.argv.slice(2);
  const problems = checkPullRequest({
    commits: commitMessages(base, head),
    title: process.env.PR_TITLE ?? "",
    body: process.env.PR_BODY ?? "",
  });
  if (problems.length > 0) {
    console.error(`Commit message guard failed:\n  - ${problems.join("\n  - ")}`);
    process.exit(1);
  }
  console.log("Commit message guard passed.");
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
