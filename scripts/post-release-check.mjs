#!/usr/bin/env node
/**
 * Verifies a GitHub Release (draft or published) against EasyOffice's fixed release asset set,
 * `latest.json` updater manifest (signatures, URLs, version claims), `SHA256SUMS.txt`, and public
 * `/releases/latest` pointer.
 *
 * Uses Node.js built-in modules only.
 */

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  MANIFEST_FILE,
  PLATFORMS,
  RELEASE_DOWNLOAD_BASE_URL,
  SUMS_FILE,
  releaseFileNames,
  signedVersion,
} from "./release-assets.mjs";

const MAX_PAYLOAD_BYTES = 2 * 1024 * 1024;
const HEX_SHA256_RE = /^[0-9a-f]{64}$/;
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;

function compareStrings(a, b) {
  if (a < b) return -1;
  return a > b ? 1 : 0;
}

export function expectedReleaseAssetNames() {
  return [...releaseFileNames(), SUMS_FILE].sort(compareStrings);
}

export function expectedManifestUrls(version) {
  const urls = {};
  for (const platform of Object.values(PLATFORMS)) {
    for (const asset of platform.assets) {
      if (!asset.signed || !Array.isArray(asset.updater)) continue;
      for (const key of asset.updater) {
        urls[key] = `${RELEASE_DOWNLOAD_BASE_URL}/v${version}/${asset.name}`;
      }
    }
  }
  return urls;
}

export function validateManifestPayload(rawText, expectedVersion) {
  if (typeof rawText !== "string" || rawText.length === 0) {
    throw new Error(`${MANIFEST_FILE} is empty`);
  }
  if (Buffer.byteLength(rawText, "utf8") > MAX_PAYLOAD_BYTES) {
    throw new Error(`${MANIFEST_FILE} exceeds ${MAX_PAYLOAD_BYTES} bytes`);
  }
  let manifest;
  try {
    manifest = JSON.parse(rawText);
  } catch (err) {
    throw new Error(`${MANIFEST_FILE} is invalid JSON: ${err.message}`);
  }
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
    throw new Error(`${MANIFEST_FILE} must be a JSON object`);
  }
  if (manifest.version !== expectedVersion) {
    throw new Error(`${MANIFEST_FILE} version is '${manifest.version}', expected '${expectedVersion}'`);
  }
  if (typeof manifest.pub_date !== "string" || Number.isNaN(Date.parse(manifest.pub_date))) {
    throw new Error(`${MANIFEST_FILE} has invalid pub_date '${manifest.pub_date}'`);
  }
  if (!manifest.platforms || typeof manifest.platforms !== "object" || Array.isArray(manifest.platforms)) {
    throw new Error(`${MANIFEST_FILE} is missing platforms map`);
  }

  const expected = expectedManifestUrls(expectedVersion);
  const actualKeys = Object.keys(manifest.platforms).sort(compareStrings);
  const expectedKeys = Object.keys(expected).sort(compareStrings);
  const missingKeys = expectedKeys.filter((k) => !actualKeys.includes(k));
  const extraKeys = actualKeys.filter((k) => !expectedKeys.includes(k));
  if (missingKeys.length || extraKeys.length) {
    const details = [];
    if (missingKeys.length) details.push(`missing keys: ${missingKeys.join(", ")}`);
    if (extraKeys.length) details.push(`unexpected keys: ${extraKeys.join(", ")}`);
    throw new Error(`${MANIFEST_FILE} platform keys mismatch (${details.join("; ")})`);
  }

  for (const key of expectedKeys) {
    const entry = manifest.platforms[key];
    if (!entry || typeof entry !== "object") {
      throw new Error(`${MANIFEST_FILE} platform '${key}' entry is invalid`);
    }
    if (entry.url !== expected[key]) {
      throw new Error(`${MANIFEST_FILE} platform '${key}' url is '${entry.url}', expected '${expected[key]}'`);
    }
    if (typeof entry.signature !== "string" || entry.signature.length === 0 || !BASE64_RE.test(entry.signature)) {
      throw new Error(`${MANIFEST_FILE} platform '${key}' has malformed base64 signature`);
    }
    const sigVer = signedVersion(entry.signature);
    if (sigVer !== expectedVersion) {
      throw new Error(`${MANIFEST_FILE} platform '${key}' signature targets '${sigVer ?? "(none)"}', expected '${expectedVersion}'`);
    }
  }
  return manifest;
}

export function validateChecksumsPayload(rawText, expectedNames = releaseFileNames()) {
  if (typeof rawText !== "string" || rawText.length === 0) {
    throw new Error(`${SUMS_FILE} is empty`);
  }
  if (Buffer.byteLength(rawText, "utf8") > MAX_PAYLOAD_BYTES) {
    throw new Error(`${SUMS_FILE} exceeds ${MAX_PAYLOAD_BYTES} bytes`);
  }
  const lines = rawText.trimEnd().split(/\r?\n/).filter((l) => l.length > 0);
  const map = new Map();
  for (const line of lines) {
    const sep = line.indexOf("  ");
    if (sep <= 0) {
      throw new Error(`${SUMS_FILE} contains malformed checksum line: '${line}'`);
    }
    const digest = line.slice(0, sep);
    const fileName = line.slice(sep + 2);
    if (!HEX_SHA256_RE.test(digest)) {
      throw new Error(`${SUMS_FILE} has invalid SHA-256 digest for '${fileName}'`);
    }
    if (!fileName || fileName.includes("/") || fileName.includes("\\")) {
      throw new Error(`${SUMS_FILE} has invalid asset filename '${fileName}'`);
    }
    if (map.has(fileName)) {
      throw new Error(`${SUMS_FILE} lists duplicate entry for '${fileName}'`);
    }
    map.set(fileName, digest);
  }

  const missing = expectedNames.filter((name) => !map.has(name));
  const extra = [...map.keys()].filter((name) => !expectedNames.includes(name));
  if (missing.length || extra.length) {
    const parts = [];
    if (missing.length) parts.push(`missing: ${missing.join(", ")}`);
    if (extra.length) parts.push(`unexpected: ${extra.join(", ")}`);
    throw new Error(`${SUMS_FILE} does not match expected release files (${parts.join("; ")})`);
  }
  return map;
}

export function verifyReleaseRecord({
  release,
  manifestText,
  sumsText,
  latestRelease,
  expectedDraft,
  expectedVersion,
}) {
  if (!release || typeof release !== "object") {
    throw new Error("release payload is missing");
  }
  const expectedTag = `v${expectedVersion}`;
  if (release.tag_name !== expectedTag) {
    throw new Error(`release tag is '${release.tag_name}', expected '${expectedTag}'`);
  }
  if (release.prerelease) {
    throw new Error(`release ${expectedTag} must not be marked prerelease`);
  }
  if (typeof expectedDraft === "boolean" && Boolean(release.draft) !== expectedDraft) {
    throw new Error(`release ${expectedTag} draft=${Boolean(release.draft)}, expected draft=${expectedDraft}`);
  }

  const assets = Array.isArray(release.assets) ? release.assets : [];
  const assetByName = new Map();
  for (const asset of assets) {
    if (!asset || typeof asset.name !== "string") continue;
    if (assetByName.has(asset.name)) {
      throw new Error(`release ${expectedTag} contains duplicate asset '${asset.name}'`);
    }
    assetByName.set(asset.name, asset);
  }

  const expectedAssets = expectedReleaseAssetNames();
  const missing = expectedAssets.filter((name) => !assetByName.has(name));
  const extra = [...assetByName.keys()].filter((name) => !expectedAssets.includes(name));
  if (missing.length || extra.length) {
    const parts = [];
    if (missing.length) parts.push(`missing: ${missing.join(", ")}`);
    if (extra.length) parts.push(`unexpected: ${extra.join(", ")}`);
    throw new Error(`release ${expectedTag} asset set mismatch (${parts.join("; ")})`);
  }

  for (const name of expectedAssets) {
    const asset = assetByName.get(name);
    if (typeof asset.size !== "number" || asset.size <= 0) {
      throw new Error(`release ${expectedTag} asset '${name}' has invalid size ${asset.size}`);
    }
  }

  const manifest = validateManifestPayload(manifestText, expectedVersion);
  const checksums = validateChecksumsPayload(sumsText);

  if (!release.draft && latestRelease !== undefined) {
    if (!latestRelease || latestRelease.tag_name !== expectedTag) {
      throw new Error(`/releases/latest points to '${latestRelease?.tag_name ?? "(none)"}', expected '${expectedTag}'`);
    }
  }

  return {
    tag: expectedTag,
    draft: Boolean(release.draft),
    assetCount: expectedAssets.length,
    manifestKeys: Object.keys(manifest.platforms).length,
    checksumCount: checksums.size,
  };
}

export function formatBytes(bytes) {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${bytes} B`;
}

export function formatAssetSummaryMarkdown(release) {
  const rows = [...(release.assets ?? [])]
    .sort((a, b) => compareStrings(a.name, b.name))
    .map((asset) => `| \`${asset.name}\` | ${formatBytes(asset.size)} |`);
  return [
    `### Release Asset Verification (${release.tag_name}, ${release.draft ? "draft" : "public"})`,
    "",
    "| Asset | Size |",
    "| --- | ---: |",
    ...rows,
    "",
  ].join("\n");
}

function githubHeaders(token, accept = "application/vnd.github+json") {
  const headers = {
    Accept: accept,
    "User-Agent": "easyoffice-release-verifier",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (token) headers.Authorization = `Bearer ${token}`;
  return headers;
}

export async function fetchGitHubJson(url, token, fetchImpl = fetch) {
  const res = await fetchImpl(url, { headers: githubHeaders(token) });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`GET ${url} failed (${res.status}): ${body.slice(0, 300)}`);
  }
  const text = await res.text();
  if (Buffer.byteLength(text, "utf8") > MAX_PAYLOAD_BYTES) {
    throw new Error(`response from ${url} exceeds ${MAX_PAYLOAD_BYTES} bytes`);
  }
  return JSON.parse(text);
}

export async function fetchGitHubAssetText(asset, token, fetchImpl = fetch) {
  if (!asset?.url) throw new Error(`asset '${asset?.name ?? "(unknown)"}' has no API url`);
  const first = await fetchImpl(asset.url, {
    headers: githubHeaders(token, "application/octet-stream"),
    redirect: "manual",
  });
  let finalRes = first;
  if (first.status >= 300 && first.status < 400) {
    const location = first.headers.get("location");
    if (!location) throw new Error(`asset '${asset.name}' redirect missing Location header`);
    finalRes = await fetchImpl(location, {
      headers: { "User-Agent": "easyoffice-release-verifier" },
    });
  }
  if (!finalRes.ok) {
    throw new Error(`downloading '${asset.name}' failed (${finalRes.status})`);
  }
  const text = await finalRes.text();
  if (Buffer.byteLength(text, "utf8") > MAX_PAYLOAD_BYTES) {
    throw new Error(`asset '${asset.name}' exceeds ${MAX_PAYLOAD_BYTES} bytes`);
  }
  return text;
}

async function findReleaseByTag({ apiBase, tag, allowDraftFallback, token, fetchImpl }) {
  const headers = githubHeaders(token);
  const tagUrl = `${apiBase}/releases/tags/${encodeURIComponent(tag)}`;
  const res = await fetchImpl(tagUrl, { headers });
  if (res.ok) {
    const text = await res.text();
    if (Buffer.byteLength(text, "utf8") > MAX_PAYLOAD_BYTES) {
      throw new Error(`response from ${tagUrl} exceeds ${MAX_PAYLOAD_BYTES} bytes`);
    }
    return JSON.parse(text);
  }
  if (res.status === 404 && allowDraftFallback) {
    const releases = await fetchGitHubJson(`${apiBase}/releases?per_page=30`, token, fetchImpl);
    const found = Array.isArray(releases) ? releases.find((r) => r && r.tag_name === tag) : null;
    if (found) return found;
  }
  const body = await res.text().catch(() => "");
  throw new Error(`GET ${tagUrl} failed (${res.status}): ${body.slice(0, 300)}`);
}

export async function verifyReleaseOnGitHub({
  repo,
  tag,
  expectedDraft,
  token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN || "",
  summaryPath = process.env.GITHUB_STEP_SUMMARY || "",
  fetchImpl = fetch,
}) {
  if (!/^[^/\s]+\/[^/\s]+$/.test(repo)) throw new Error(`invalid repository '${repo}'`);
  if (!/^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(tag)) throw new Error(`invalid release tag '${tag}'`);
  const expectedVersion = tag.slice(1);
  const apiBase = `https://api.github.com/repos/${repo}`;

  const release = await findReleaseByTag({
    apiBase,
    tag,
    allowDraftFallback: expectedDraft !== false,
    token,
    fetchImpl,
  });
  const assetsByName = new Map((release.assets ?? []).map((a) => [a.name, a]));
  const manifestAsset = assetsByName.get(MANIFEST_FILE);
  const sumsAsset = assetsByName.get(SUMS_FILE);
  if (!manifestAsset) throw new Error(`release ${tag} is missing ${MANIFEST_FILE}`);
  if (!sumsAsset) throw new Error(`release ${tag} is missing ${SUMS_FILE}`);

  const [manifestText, sumsText] = await Promise.all([
    fetchGitHubAssetText(manifestAsset, token, fetchImpl),
    fetchGitHubAssetText(sumsAsset, token, fetchImpl),
  ]);

  let latestRelease;
  if (!release.draft) {
    latestRelease = await fetchGitHubJson(`${apiBase}/releases/latest`, token, fetchImpl);
  }

  const result = verifyReleaseRecord({
    release,
    manifestText,
    sumsText,
    latestRelease,
    expectedDraft,
    expectedVersion,
  });

  if (summaryPath) {
    fs.appendFileSync(summaryPath, `${formatAssetSummaryMarkdown(release)}\n`);
  }
  return { result, release };
}

function parseCliArgs(argv) {
  const args = { repo: process.env.GITHUB_REPOSITORY || "", tag: "", expectDraft: undefined };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--repo" && argv[i + 1]) args.repo = argv[++i];
    else if (arg === "--tag" && argv[i + 1]) args.tag = argv[++i];
    else if (arg === "--expect-draft") args.expectDraft = true;
    else if (arg === "--expect-public") args.expectDraft = false;
    else if (arg === "--draft" && argv[i + 1]) {
      const val = argv[++i];
      if (val !== "true" && val !== "false") throw new Error(`invalid --draft '${val}' (expected true|false)`);
      args.expectDraft = val === "true";
    }
    else throw new Error(`unknown or incomplete argument '${arg}'`);
  }
  if (!args.repo || !args.tag) {
    throw new Error("usage: post-release-check.mjs --repo <owner/repo> --tag <vX.Y.Z> [--expect-draft|--expect-public]");
  }
  return args;
}

async function main() {
  const opts = parseCliArgs(process.argv.slice(2));
  const { result } = await verifyReleaseOnGitHub(opts);
  console.log(
    `Verified release ${result.tag} (draft=${result.draft}): ${result.assetCount} assets, ${result.manifestKeys} updater keys, ${result.checksumCount} checksums.`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((err) => {
    console.error(`::error::${err.message}`);
    process.exit(1);
  });
}
