#!/usr/bin/env node
/**
 * Daily / on-demand guard verifying that GitHub `/releases/latest` matches the highest published
 * stable release tag (`vMAJOR.MINOR.PATCH`) and passes full release asset, updater manifest, and
 * SHA-256 checksum checks. Read-only: fails loudly via CI annotations and never creates issues.
 *
 * Uses Node.js built-in modules only.
 */

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { MANIFEST_FILE, SUMS_FILE } from "./release-assets.mjs";
import {
  TransientVerificationError,
  defaultSleep,
  fetchGitHubAssetText,
  fetchGitHubJson,
  fetchGitHubPaginatedJson,
  formatAssetSummaryMarkdown,
  retryTransient,
  verifyReleaseRecord,
} from "./post-release-check.mjs";
import { escapeWorkflowCommand } from "./workflow-command.mjs";

const STABLE_SEMVER_TAG_RE = /^v(\d+)\.(\d+)\.(\d+)$/;

export function parseSemverTag(tagName) {
  if (typeof tagName !== "string") return null;
  const match = STABLE_SEMVER_TAG_RE.exec(tagName);
  if (!match) return null;
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

export function compareSemverDesc(aTag, bTag) {
  const a = parseSemverTag(aTag);
  const b = parseSemverTag(bTag);
  if (!a || !b) return 0;
  for (let i = 0; i < 3; i += 1) {
    if (a[i] !== b[i]) return b[i] - a[i];
  }
  return 0;
}

export function findExpectedLatestRelease(releases) {
  if (!Array.isArray(releases)) {
    throw new Error("releases response must be an array");
  }
  const candidates = releases.filter((r) => r && !r.draft && !r.prerelease && parseSemverTag(r.tag_name) !== null);
  if (candidates.length === 0) {
    throw new Error("no published stable release matching vMAJOR.MINOR.PATCH was found");
  }
  candidates.sort((a, b) => compareSemverDesc(a.tag_name, b.tag_name));
  return candidates[0];
}

function assertLatestReleaseMatchesExpected(releases, latestRelease) {
  const expectedLatest = findExpectedLatestRelease(releases);
  if (!latestRelease || typeof latestRelease.tag_name !== "string") {
    throw new Error("/releases/latest returned no release");
  }
  if (latestRelease.draft || latestRelease.prerelease) {
    throw new Error(`/releases/latest '${latestRelease.tag_name}' cannot be draft or prerelease`);
  }
  if (latestRelease.tag_name !== expectedLatest.tag_name) {
    throw new TransientVerificationError(
      `/releases/latest points to '${latestRelease.tag_name}', expected highest published stable release '${expectedLatest.tag_name}'`,
    );
  }
  return expectedLatest;
}

export function evaluateLatestState({ releases, latestRelease, manifestText, sumsText }) {
  const expectedLatest = assertLatestReleaseMatchesExpected(releases, latestRelease);
  const result = verifyReleaseRecord({
    release: latestRelease,
    manifestText,
    sumsText,
    latestRelease,
    expectedDraft: false,
    expectedVersion: latestRelease.tag_name.slice(1),
  });
  return { expectedTag: expectedLatest.tag_name, verification: result };
}

export async function runLatestGuard({
  repo = process.env.GITHUB_REPOSITORY || "",
  token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN || "",
  summaryPath = process.env.GITHUB_STEP_SUMMARY || "",
  fetchImpl = fetch,
  maxAttempts = 4,
  retryDelayMs = 2000,
  sleepImpl = defaultSleep,
} = {}) {
  if (!/^[^/\s]+\/[^/\s]+$/.test(repo)) {
    throw new Error(`invalid repository '${repo}'`);
  }
  const apiBase = `https://api.github.com/repos/${repo}`;

  const { evaluated, latestRelease } = await retryTransient(
    async () => {
      const [releases, latestRelease] = await Promise.all([
        fetchGitHubPaginatedJson(`${apiBase}/releases?per_page=100`, token, fetchImpl),
        fetchGitHubJson(`${apiBase}/releases/latest`, token, fetchImpl),
      ]);
      assertLatestReleaseMatchesExpected(releases, latestRelease);

      const assetsByName = new Map((latestRelease?.assets ?? []).map((a) => [a.name, a]));
      const manifestAsset = assetsByName.get(MANIFEST_FILE);
      const sumsAsset = assetsByName.get(SUMS_FILE);
      if (!manifestAsset) throw new Error(`/releases/latest (${latestRelease?.tag_name}) is missing ${MANIFEST_FILE}`);
      if (!sumsAsset) throw new Error(`/releases/latest (${latestRelease?.tag_name}) is missing ${SUMS_FILE}`);

      const [manifestText, sumsText] = await Promise.all([
        fetchGitHubAssetText(manifestAsset, token, fetchImpl),
        fetchGitHubAssetText(sumsAsset, token, fetchImpl),
      ]);

      return {
        evaluated: evaluateLatestState({ releases, latestRelease, manifestText, sumsText }),
        latestRelease,
      };
    },
    { maxAttempts, retryDelayMs, sleepImpl },
  );

  if (summaryPath) {
    const md = [
      `## Latest Release Guard (${evaluated.expectedTag})`,
      "",
      `- **Latest tag**: \`${evaluated.expectedTag}\` (matches highest published stable tag)`,
      `- **Verified assets**: ${evaluated.verification.assetCount} files, ${evaluated.verification.manifestKeys} updater targets, ${evaluated.verification.checksumCount} SHA-256 entries`,
      "",
      formatAssetSummaryMarkdown(latestRelease),
    ].join("\n");
    fs.appendFileSync(summaryPath, `${md}\n`);
  }
  return evaluated;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  runLatestGuard()
    .then((res) => {
      console.log(
        `Latest release guard passed for ${res.expectedTag}: ${res.verification.assetCount} assets verified.`,
      );
    })
    .catch((err) => {
      console.error(`::error::${escapeWorkflowCommand(err?.message ?? err)}`);
      process.exit(1);
    });
}
