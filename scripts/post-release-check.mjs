#!/usr/bin/env node
/**
 * Verifies a GitHub Release (draft or published) against EasyOffice's fixed release asset set,
 * `latest.json` updater manifest (signatures, URLs, version claims), `SHA256SUMS.txt` (including
 * cryptographic match with `latest.json`), and public `/releases/latest` pointer. It also downloads every
 * `.sig` asset (a few hundred bytes each) to check the signatures in `latest.json` against them and the
 * `.sig` digests against `SHA256SUMS.txt`, and hashes the two smallest packages (never over 64 MiB, the
 * stream is cut at the size the release declares) to check them against `SHA256SUMS.txt`.
 *
 * Uses Node.js built-in modules only.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  MANIFEST_FILE,
  PLATFORMS,
  RELEASE_DOWNLOAD_BASE_URL,
  SIGNATURE_SUFFIX,
  SUMS_FILE,
  releaseFileNames,
  signedVersion,
} from "./release-assets.mjs";
import { escapeWorkflowCommand } from "./workflow-command.mjs";

export const MAX_PAYLOAD_BYTES = 2 * 1024 * 1024;
/** A `.sig` file is a few hundred bytes of base64. */
export const MAX_SIGNATURE_BYTES = 16 * 1024;
/** Packages whose hash is checked against `SHA256SUMS.txt`: the smallest ones, none over this size. */
export const SPOT_CHECK_COUNT = 2;
export const MAX_SPOT_CHECK_BYTES = 64 * 1024 * 1024;
const MAX_RELEASE_PAGES = 20;
const HEX_SHA256_RE = /^[0-9a-f]{64}$/;
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;
export const defaultSleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export class TransientVerificationError extends Error {}

export async function retryTransient(
  fn,
  { maxAttempts = 4, retryDelayMs = 2000, sleepImpl = defaultSleep } = {},
) {
  if (!Number.isInteger(maxAttempts) || maxAttempts < 1) {
    throw new Error(`invalid maxAttempts '${maxAttempts}'`);
  }
  if (typeof retryDelayMs !== "number" || !Number.isFinite(retryDelayMs) || retryDelayMs < 0) {
    throw new Error(`invalid retryDelayMs '${retryDelayMs}'`);
  }
  let lastError;
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    try {
      return await fn(attempt);
    } catch (err) {
      lastError = err;
      if (!(err instanceof TransientVerificationError) || attempt >= maxAttempts) {
        throw err;
      }
      if (retryDelayMs > 0) {
        await sleepImpl(retryDelayMs * attempt);
      }
    }
  }
  throw lastError;
}

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

export function verifyReleaseMetadata({ release, expectedDraft, expectedVersion }) {
  if (!release || typeof release !== "object") {
    throw new Error("release payload is missing");
  }
  const expectedTag = `v${expectedVersion}`;
  if (release.tag_name !== expectedTag) {
    throw new Error(`release tag is '${release.tag_name}', expected '${expectedTag}'`);
  }
  const expectPrerelease = expectedVersion.includes("-");
  if (Boolean(release.prerelease) !== expectPrerelease) {
    throw new Error(
      `release ${expectedTag} prerelease=${Boolean(release.prerelease)}, expected prerelease=${expectPrerelease}`,
    );
  }
  if (typeof expectedDraft === "boolean" && Boolean(release.draft) !== expectedDraft) {
    throw new Error(`release ${expectedTag} draft=${Boolean(release.draft)}, expected draft=${expectedDraft}`);
  }

  const assets = Array.isArray(release.assets) ? release.assets : [];
  const assetByName = new Map();
  for (const asset of assets) {
    if (!asset || typeof asset.name !== "string") {
      throw new Error(`release ${expectedTag} contains malformed asset entry`);
    }
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
    if (asset.state !== "uploaded") {
      throw new Error(`release ${expectedTag} asset '${name}' is incomplete (state=${asset.state})`);
    }
    if (!Number.isSafeInteger(asset.size) || asset.size <= 0) {
      throw new Error(`release ${expectedTag} asset '${name}' has invalid size ${asset.size}`);
    }
  }

  return { expectedTag, expectedAssets, assetByName };
}

/** The signed packages of the release with the updater keys each one serves. */
function signedAssets() {
  return Object.values(PLATFORMS)
    .flatMap((platform) => platform.assets)
    .filter((asset) => asset.signed && Array.isArray(asset.updater))
    .map((asset) => ({ name: asset.name, sigName: asset.name + SIGNATURE_SUFFIX, updater: asset.updater }));
}

export const signatureAssetNames = () => signedAssets().map((asset) => asset.sigName).sort(compareStrings);

const sha256Hex = (text) => crypto.createHash("sha256").update(text, "utf8").digest("hex");

/**
 * `latest.json` must carry, for every updater key, the text of the `.sig` asset of its package (as
 * `release-assets.mjs` writes it: the file without line breaks), and each `.sig` must be the file
 * `SHA256SUMS.txt` lists. `signatureTexts` maps a `.sig` asset name to its downloaded text.
 */
export function verifyManifestSignatures({ manifest, signatureTexts, checksums }) {
  for (const { name, sigName, updater } of signedAssets()) {
    const text = signatureTexts.get(sigName);
    if (typeof text !== "string" || text.length === 0) {
      throw new Error(`${sigName} was not downloaded or is empty`);
    }
    const digest = sha256Hex(text);
    if (digest !== checksums.get(sigName)) {
      throw new Error(`${SUMS_FILE} digest for ${sigName} (${checksums.get(sigName)}) does not match downloaded ${sigName} (${digest})`);
    }
    const signature = text.replace(/[\r\n]/g, "");
    for (const key of updater) {
      if (manifest.platforms[key]?.signature !== signature) {
        throw new Error(`${MANIFEST_FILE} platform '${key}' signature does not match ${sigName} of ${name}`);
      }
    }
  }
}

/**
 * The packages to download and hash: the smallest ones by their size in the release, at most `count`,
 * none larger than `maxBytes` (a download is the cost of the check). Ties go by name, so every run
 * picks the same files.
 */
export function selectSpotCheckAssets(assetByName, { count = SPOT_CHECK_COUNT, maxBytes = MAX_SPOT_CHECK_BYTES } = {}) {
  const packages = Object.values(PLATFORMS).flatMap((platform) => platform.assets.map((asset) => asset.name));
  return packages
    .map((name) => assetByName.get(name))
    .filter((asset) => asset && asset.size <= maxBytes)
    .sort((a, b) => a.size - b.size || compareStrings(a.name, b.name))
    .slice(0, count)
    .map((asset) => asset.name);
}

/** `spotChecks` maps a package name to `{ sha256, bytes }` of what was downloaded. */
export function verifySpotChecks({ assetByName, checksums, spotChecks }) {
  for (const [name, { sha256, bytes }] of spotChecks) {
    const declared = assetByName.get(name)?.size;
    if (bytes !== declared) {
      throw new Error(`downloaded '${name}' is ${bytes} bytes, the release declares ${declared}`);
    }
    if (sha256 !== checksums.get(name)) {
      throw new Error(`${SUMS_FILE} digest for ${name} (${checksums.get(name)}) does not match downloaded ${name} (${sha256})`);
    }
  }
}

export function verifyReleaseRecord({
  release,
  manifestText,
  sumsText,
  latestRelease,
  expectedDraft,
  expectedVersion,
  signatureTexts,
  spotChecks,
}) {
  const { expectedTag, expectedAssets, assetByName } = verifyReleaseMetadata({
    release,
    expectedDraft,
    expectedVersion,
  });

  const manifest = validateManifestPayload(manifestText, expectedVersion);
  const checksums = validateChecksumsPayload(sumsText);
  const manifestDigest = sha256Hex(manifestText);
  const expectedManifestDigest = checksums.get(MANIFEST_FILE);
  if (manifestDigest !== expectedManifestDigest) {
    throw new Error(
      `${SUMS_FILE} digest for ${MANIFEST_FILE} (${expectedManifestDigest}) does not match downloaded ${MANIFEST_FILE} (${manifestDigest})`,
    );
  }
  if (signatureTexts !== undefined) verifyManifestSignatures({ manifest, signatureTexts, checksums });
  if (spotChecks !== undefined) verifySpotChecks({ assetByName, checksums, spotChecks });

  if (!release.draft && !release.prerelease && latestRelease !== undefined) {
    if (!latestRelease || latestRelease.tag_name !== expectedTag) {
      throw new TransientVerificationError(
        `/releases/latest points to '${latestRelease?.tag_name ?? "(none)"}', expected '${expectedTag}'`,
      );
    }
  }

  return {
    tag: expectedTag,
    draft: Boolean(release.draft),
    prerelease: Boolean(release.prerelease),
    assetCount: expectedAssets.length,
    manifestKeys: Object.keys(manifest.platforms).length,
    checksumCount: checksums.size,
    signatureCount: signatureTexts?.size ?? 0,
    spotChecked: spotChecks ? [...spotChecks.keys()].sort(compareStrings) : [],
  };
}

export function formatBytes(bytes) {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${bytes} B`;
}

export function formatAssetSummaryMarkdown(release) {
  const rows = (Array.isArray(release?.assets) ? release.assets : [])
    .filter((asset) => asset && typeof asset.name === "string")
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

export async function readResponseTextBounded(res, label, maxBytes = MAX_PAYLOAD_BYTES) {
  const contentLength = res.headers?.get?.("content-length");
  if (contentLength && /^\d+$/.test(contentLength) && Number(contentLength) > maxBytes) {
    await res.body?.cancel?.().catch(() => {});
    throw new Error(`${label} exceeds ${maxBytes} bytes`);
  }
  if (res.body && typeof res.body.getReader === "function") {
    const reader = res.body.getReader();
    const chunks = [];
    let total = 0;
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        const buf = Buffer.from(value.buffer, value.byteOffset, value.byteLength);
        total += buf.length;
        if (total > maxBytes) {
          await reader.cancel().catch(() => {});
          throw new Error(`${label} exceeds ${maxBytes} bytes`);
        }
        chunks.push(buf);
      }
    } finally {
      reader.releaseLock();
    }
    return Buffer.concat(chunks, total).toString("utf8");
  }
  const text = await res.text();
  if (Buffer.byteLength(text, "utf8") > maxBytes) {
    throw new Error(`${label} exceeds ${maxBytes} bytes`);
  }
  return text;
}

function parseNextLink(linkHeader) {
  if (!linkHeader) return null;
  for (const part of linkHeader.split(",")) {
    const match = part.match(/<([^>]+)>\s*;\s*rel="next"/);
    if (match) {
      const nextUrl = match[1];
      let validated;
      try {
        validated = validateHttpsUrl(nextUrl, "pagination", { isRedirect: false });
      } catch {
        throw new Error(`refusing non-GitHub API pagination URL '${nextUrl}'`);
      }
      if (validated.origin !== "https://api.github.com") {
        throw new Error(`refusing non-GitHub API pagination URL '${nextUrl}'`);
      }
      return validated.url;
    }
  }
  return null;
}

export async function fetchGitHubJson(url, token, fetchImpl = fetch) {
  const res = await fetchImpl(url, { headers: githubHeaders(token) });
  if (!res.ok) {
    const body = await readResponseTextBounded(res, `error response from ${url}`, 8192).catch(() => "");
    const message = `GET ${url} failed (${res.status}): ${body.slice(0, 300)}`;
    if (res.status === 404 || res.status >= 500) {
      throw new TransientVerificationError(message);
    }
    throw new Error(message);
  }
  const text = await readResponseTextBounded(res, `response from ${url}`);
  return JSON.parse(text);
}

export async function fetchGitHubPaginatedJson(initialUrl, token, fetchImpl = fetch, maxPages = MAX_RELEASE_PAGES) {
  const items = [];
  let nextUrl = initialUrl;
  let page = 0;
  while (nextUrl) {
    page += 1;
    if (page > maxPages) {
      throw new Error(`pagination for ${initialUrl} exceeded ${maxPages} pages`);
    }
    const res = await fetchImpl(nextUrl, { headers: githubHeaders(token) });
    if (!res.ok) {
      const body = await readResponseTextBounded(res, `error response from ${nextUrl}`, 8192).catch(() => "");
      const message = `GET ${nextUrl} failed (${res.status}): ${body.slice(0, 300)}`;
      if (res.status === 404 || res.status >= 500) {
        throw new TransientVerificationError(message);
      }
      throw new Error(message);
    }
    const text = await readResponseTextBounded(res, `response from ${nextUrl}`);
    const parsed = JSON.parse(text);
    if (!Array.isArray(parsed)) {
      throw new Error(`paginated response from ${nextUrl} must be a JSON array`);
    }
    items.push(...parsed);
    nextUrl = parseNextLink(res.headers?.get?.("link") ?? "");
  }
  return items;
}

const MAX_ASSET_REDIRECTS = 3;
const REDIRECT_STATUSES = new Set([301, 302, 303, 307, 308]);

function validateHttpsUrl(rawUrl, assetName, { isRedirect = false } = {}) {
  const context = isRedirect ? "redirect" : "API url";
  const subject = isRedirect ? "Location URL" : "URL";
  const trimmed = typeof rawUrl === "string" ? rawUrl.trim() : "";
  if (!trimmed) {
    throw new Error(
      isRedirect
        ? `asset '${assetName}' redirect missing Location header`
        : `asset '${assetName}' has no API url`,
    );
  }
  if (/[\s\u0000-\u001f\u007f]/.test(trimmed)) {
    throw new Error(`asset '${assetName}' ${context} has invalid ${subject} '${trimmed}'`);
  }
  let parsed;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error(`asset '${assetName}' ${context} has invalid ${subject} '${trimmed}'`);
  }
  if (parsed.protocol !== "https:" || !parsed.hostname) {
    throw new Error(`asset '${assetName}' ${context} refuses non-HTTPS ${subject} '${trimmed}'`);
  }
  if (!/^https:\/\//i.test(trimmed)) {
    throw new Error(`asset '${assetName}' ${context} has malformed HTTPS ${subject} '${trimmed}'`);
  }
  if (parsed.username || parsed.password || /^https:\/\/[^/?#]*@/i.test(trimmed)) {
    throw new Error(`asset '${assetName}' ${context} refuses credentials in ${subject}`);
  }
  if (!/^https:\/\/[^/?#@\\]+(?:[/?#]|$)/i.test(trimmed)) {
    throw new Error(`asset '${assetName}' ${context} has malformed HTTPS ${subject} '${trimmed}'`);
  }
  return { url: trimmed, origin: parsed.origin };
}

/** The response of an asset download after its redirects (the token stays on api.github.com), not yet read. */
async function openGitHubAsset(asset, token, fetchImpl) {
  const assetName = asset?.name ?? "(unknown)";
  const initial = validateHttpsUrl(asset?.url, assetName, { isRedirect: false });
  let currentUrl = initial.url;
  let res;
  for (let redirects = 0; ; redirects += 1) {
    const headers =
      redirects === 0
        ? githubHeaders(initial.origin === "https://api.github.com" ? token : "", "application/octet-stream")
        : { "User-Agent": "easyoffice-release-verifier" };
    res = await fetchImpl(currentUrl, { headers, redirect: "manual" });
    if (!REDIRECT_STATUSES.has(res.status)) break;
    const location = res.headers?.get?.("location");
    await res.body?.cancel?.().catch(() => {});
    if (redirects >= MAX_ASSET_REDIRECTS) {
      throw new Error(`asset '${assetName}' exceeded ${MAX_ASSET_REDIRECTS} redirects`);
    }
    currentUrl = validateHttpsUrl(location, assetName, { isRedirect: true }).url;
  }
  if (!res.ok) {
    await res.body?.cancel?.().catch(() => {});
    const message = `downloading '${assetName}' failed (${res.status})`;
    if (res.status === 404 || res.status >= 500) {
      throw new TransientVerificationError(message);
    }
    throw new Error(message);
  }
  return res;
}

export async function fetchGitHubAssetText(asset, token, fetchImpl = fetch, maxBytes = MAX_PAYLOAD_BYTES) {
  const res = await openGitHubAsset(asset, token, fetchImpl);
  return readResponseTextBounded(res, `asset '${asset?.name ?? "(unknown)"}'`, maxBytes);
}

/**
 * SHA-256 and byte count of an asset, hashed while it streams in: nothing is kept, and the download
 * stops with an error as soon as it is longer than `maxBytes` or than the size the release declares.
 */
export async function fetchGitHubAssetDigest(asset, token, fetchImpl = fetch, maxBytes = MAX_SPOT_CHECK_BYTES) {
  const label = `asset '${asset?.name ?? "(unknown)"}'`;
  const limit = Math.min(maxBytes, Number.isSafeInteger(asset?.size) ? asset.size : maxBytes);
  const res = await openGitHubAsset(asset, token, fetchImpl);
  const contentLength = res.headers?.get?.("content-length");
  if (contentLength && /^\d+$/.test(contentLength) && Number(contentLength) > limit) {
    await res.body?.cancel?.().catch(() => {});
    throw new Error(`${label} is longer than ${limit} bytes`);
  }
  const hash = crypto.createHash("sha256");
  let bytes = 0;
  const reader = res.body?.getReader?.();
  if (!reader) throw new Error(`${label} has no body to hash`);
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > limit) {
        await reader.cancel().catch(() => {});
        throw new Error(`${label} is longer than ${limit} bytes`);
      }
      hash.update(value);
    }
  } finally {
    reader.releaseLock();
  }
  return { sha256: hash.digest("hex"), bytes };
}

async function findReleaseByTag({ apiBase, tag, allowDraftFallback, token, fetchImpl }) {
  const headers = githubHeaders(token);
  const tagUrl = `${apiBase}/releases/tags/${encodeURIComponent(tag)}`;
  const res = await fetchImpl(tagUrl, { headers });
  if (res.ok) {
    const text = await readResponseTextBounded(res, `response from ${tagUrl}`);
    return JSON.parse(text);
  }
  if (res.status === 404 && allowDraftFallback) {
    await res.body?.cancel?.().catch(() => {});
    const releases = await fetchGitHubPaginatedJson(`${apiBase}/releases?per_page=100`, token, fetchImpl);
    const matches = releases.filter((r) => r && r.tag_name === tag);
    if (matches.length > 1) {
      throw new Error(`multiple draft releases match tag '${tag}' (${matches.length})`);
    }
    if (matches.length === 1) return matches[0];
  }
  const body = await readResponseTextBounded(res, `error response from ${tagUrl}`, 8192).catch(() => "");
  const message = `GET ${tagUrl} failed (${res.status}): ${body.slice(0, 300)}`;
  if (res.status === 404 || res.status >= 500) {
    throw new TransientVerificationError(message);
  }
  throw new Error(message);
}

export async function verifyReleaseOnGitHub({
  repo,
  tag,
  expectedDraft,
  token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN || "",
  summaryPath = process.env.GITHUB_STEP_SUMMARY || "",
  fetchImpl = fetch,
  maxAttempts = 4,
  retryDelayMs = 2000,
  sleepImpl = defaultSleep,
}) {
  if (!/^[^/\s]+\/[^/\s]+$/.test(repo)) throw new Error(`invalid repository '${repo}'`);
  if (!/^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(tag)) throw new Error(`invalid release tag '${tag}'`);
  const expectedVersion = tag.slice(1);
  const apiBase = `https://api.github.com/repos/${repo}`;

  return retryTransient(
    async () => {
      const release = await findReleaseByTag({
        apiBase,
        tag,
        allowDraftFallback: expectedDraft !== false,
        token,
        fetchImpl,
      });
      const { assetByName } = verifyReleaseMetadata({
        release,
        expectedDraft,
        expectedVersion,
      });
      const manifestAsset = assetByName.get(MANIFEST_FILE);
      const sumsAsset = assetByName.get(SUMS_FILE);

      const signatureNames = signatureAssetNames();
      const spotNames = selectSpotCheckAssets(assetByName);
      const [manifestText, sumsText, signatureList, spotList] = await Promise.all([
        fetchGitHubAssetText(manifestAsset, token, fetchImpl),
        fetchGitHubAssetText(sumsAsset, token, fetchImpl),
        Promise.all(
          signatureNames.map((name) => fetchGitHubAssetText(assetByName.get(name), token, fetchImpl, MAX_SIGNATURE_BYTES)),
        ),
        Promise.all(spotNames.map((name) => fetchGitHubAssetDigest(assetByName.get(name), token, fetchImpl))),
      ]);
      const signatureTexts = new Map(signatureNames.map((name, i) => [name, signatureList[i]]));
      const spotChecks = new Map(spotNames.map((name, i) => [name, spotList[i]]));

      let latestRelease;
      if (!release.draft && !release.prerelease) {
        latestRelease = await fetchGitHubJson(`${apiBase}/releases/latest`, token, fetchImpl);
      }

      const result = verifyReleaseRecord({
        release,
        manifestText,
        sumsText,
        latestRelease,
        expectedDraft,
        expectedVersion,
        signatureTexts,
        spotChecks,
      });

      if (summaryPath) {
        fs.appendFileSync(summaryPath, `${formatAssetSummaryMarkdown(release)}\n`);
      }
      return { result, release };
    },
    { maxAttempts, retryDelayMs, sleepImpl },
  );
}

function setExpectedDraft(args, value) {
  if (args.expectedDraft !== undefined && args.expectedDraft !== value) {
    throw new Error("conflicting draft/public arguments");
  }
  args.expectedDraft = value;
}

export function parseCliArgs(argv, env = process.env) {
  const args = { repo: env?.GITHUB_REPOSITORY || "", tag: "", expectedDraft: undefined };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--repo" && argv[i + 1]) args.repo = argv[++i];
    else if (arg === "--tag" && argv[i + 1]) args.tag = argv[++i];
    else if (arg === "--expect-draft") setExpectedDraft(args, true);
    else if (arg === "--expect-public") setExpectedDraft(args, false);
    else if (arg === "--draft" && argv[i + 1]) {
      const val = argv[++i];
      if (val !== "true" && val !== "false") throw new Error(`invalid --draft '${val}' (expected true|false)`);
      setExpectedDraft(args, val === "true");
    }
    else throw new Error(`unknown or incomplete argument '${arg}'`);
  }
  if (!args.repo || !args.tag) {
    throw new Error(
      "usage: post-release-check.mjs --repo <owner/repo> --tag <vX.Y.Z> [--expect-draft|--expect-public|--draft <true|false>]",
    );
  }
  return args;
}

export async function runPostReleaseCheckCli(
  argv = process.argv.slice(2),
  {
    env = process.env,
    fetchImpl = fetch,
    summaryPath = env?.GITHUB_STEP_SUMMARY || "",
    maxAttempts = 4,
    retryDelayMs = 2000,
    sleepImpl = defaultSleep,
    log = console.log,
  } = {},
) {
  const opts = parseCliArgs(argv, env);
  const { result } = await verifyReleaseOnGitHub({
    repo: opts.repo,
    tag: opts.tag,
    expectedDraft: opts.expectedDraft,
    token: env?.GH_TOKEN || env?.GITHUB_TOKEN || "",
    summaryPath,
    fetchImpl,
    maxAttempts,
    retryDelayMs,
    sleepImpl,
  });
  log(
    `Verified release ${result.tag} (draft=${result.draft}): ${result.assetCount} assets, ${result.manifestKeys} updater keys, ${result.checksumCount} checksums, ${result.signatureCount} signatures matched, hashes of ${result.spotChecked.join(", ") || "no package"} matched.`,
  );
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  runPostReleaseCheckCli().catch((err) => {
    console.error(`::error::${escapeWorkflowCommand(err?.message ?? err)}`);
    process.exit(1);
  });
}
