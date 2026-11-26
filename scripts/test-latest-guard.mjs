import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  MANIFEST_FILE,
  PLATFORMS,
  RELEASE_DOWNLOAD_BASE_URL,
  SUMS_FILE,
  releaseFileNames,
} from "./release-assets.mjs";
import { expectedReleaseAssetNames } from "./post-release-check.mjs";
import { evaluateLatestState, findExpectedLatestRelease, runLatestGuard } from "./latest-guard.mjs";

function makeSig(version) {
  const comment = `trusted comment: timestamp:1700000000\tfile:app\tversion:${version}`;
  return Buffer.from(`untrusted comment: sig\nRWQ=\n${comment}\nAAAA\n`, "utf8").toString("base64");
}

function makeManifest(version) {
  const sig = makeSig(version);
  const platforms = {};
  for (const platform of Object.values(PLATFORMS)) {
    for (const asset of platform.assets) {
      if (!asset.signed) continue;
      for (const key of asset.updater) {
        platforms[key] = { url: `${RELEASE_DOWNLOAD_BASE_URL}/v${version}/${asset.name}`, signature: sig };
      }
    }
  }
  return JSON.stringify({ version, notes: `EasyOffice v${version}`, pub_date: "2026-11-26T00:00:00Z", platforms });
}

function makeSums(manifestText) {
  const manifestHash = crypto.createHash("sha256").update(manifestText, "utf8").digest("hex");
  return `${releaseFileNames().map((n) => `${n === MANIFEST_FILE ? manifestHash : "c".repeat(64)}  ${n}`).join("\n")}\n`;
}

function makeRelease(tag, { draft = false, prerelease = false } = {}) {
  return {
    tag_name: tag,
    draft,
    prerelease,
    assets: expectedReleaseAssetNames().map((name, idx) => ({
      name,
      state: "uploaded",
      size: 2048 + idx,
      url: `https://api.github.com/repos/AquilaXk/easyoffice/releases/assets/${idx + 1}`,
    })),
  };
}

test("findExpectedLatestRelease ignores drafts and prereleases and sorts semver numerically", () => {
  const picked = findExpectedLatestRelease([
    makeRelease("v0.2.9"),
    makeRelease("v0.2.10"),
    makeRelease("v0.3.0", { draft: true }),
    makeRelease("v0.3.0-rc.1", { prerelease: true }),
  ]);
  assert.equal(picked.tag_name, "v0.2.10");
});

test("evaluateLatestState passes when /releases/latest matches highest published semver and assets", () => {
  const latest = makeRelease("v0.2.10");
  const releases = [makeRelease("v0.2.9"), latest, makeRelease("v0.3.0", { draft: true })];
  const manifestText = makeManifest("0.2.10");
  const res = evaluateLatestState({
    releases,
    latestRelease: latest,
    manifestText,
    sumsText: makeSums(manifestText),
  });
  assert.equal(res.expectedTag, "v0.2.10");
  assert.equal(res.verification.assetCount, 18);
});

test("evaluateLatestState fails when /releases/latest lags behind a newer published stable release", () => {
  const older = makeRelease("v0.2.2");
  const newer = makeRelease("v0.2.3");
  const manifestText = makeManifest("0.2.2");
  assert.throws(
    () =>
      evaluateLatestState({
        releases: [older, newer],
        latestRelease: older,
        manifestText,
        sumsText: makeSums(manifestText),
      }),
    /points to 'v0\.2\.2', expected highest published stable release 'v0\.2\.3'/,
  );
});

test("runLatestGuard paginates releases and verifies assets without modifying state", async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "latest-guard-test-"));
  const summaryFile = path.join(tmpDir, "summary.md");
  try {
    const older = makeRelease("v0.2.2");
    const latest = makeRelease("v0.2.3");
    const manifest = makeManifest("0.2.3");
    const sums = makeSums(manifest);

    const mockFetch = async (url, init = {}) => {
      if (init.method && init.method !== "GET") throw new Error(`unexpected mutating method ${init.method}`);
      if (url.endsWith("/releases?per_page=100")) {
        return new Response(JSON.stringify([older]), {
          status: 200,
          headers: { link: '<https://api.github.com/repos/AquilaXk/easyoffice/releases?per_page=100&page=2>; rel="next"' },
        });
      }
      if (url.endsWith("/releases?per_page=100&page=2")) {
        return new Response(JSON.stringify([latest]), { status: 200 });
      }
      if (url.endsWith("/releases/latest")) return new Response(JSON.stringify(latest), { status: 200 });
      if (url.includes("/releases/assets/")) {
        const idx = Number(url.split("/").pop()) - 1;
        const name = latest.assets[idx].name;
        if (name === MANIFEST_FILE) return new Response(manifest, { status: 200 });
        if (name === SUMS_FILE) return new Response(sums, { status: 200 });
      }
      return new Response("not found", { status: 404 });
    };

    const res = await runLatestGuard({
      repo: "AquilaXk/easyoffice",
      token: "tok",
      summaryPath: summaryFile,
      fetchImpl: mockFetch,
    });
    assert.equal(res.expectedTag, "v0.2.3");
    assert.match(fs.readFileSync(summaryFile, "utf8"), /Latest Release Guard \(v0\.2\.3\)/);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
