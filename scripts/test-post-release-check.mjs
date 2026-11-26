import assert from "node:assert/strict";
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
import {
  expectedManifestUrls,
  expectedReleaseAssetNames,
  formatAssetSummaryMarkdown,
  validateChecksumsPayload,
  validateManifestPayload,
  verifyReleaseOnGitHub,
  verifyReleaseRecord,
} from "./post-release-check.mjs";

function makeSig(version) {
  const comment = `trusted comment: timestamp:1700000000\tfile:app\tversion:${version}`;
  return Buffer.from(`untrusted comment: minisign signature\nRWQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==\n${comment}\nAAAA\n`, "utf8").toString("base64");
}

function makeManifestText(version = "0.2.3", sigVersion = version) {
  const sig = makeSig(sigVersion);
  const platforms = {};
  for (const platform of Object.values(PLATFORMS)) {
    for (const asset of platform.assets) {
      if (!asset.signed) continue;
      for (const key of asset.updater) {
        platforms[key] = {
          url: `${RELEASE_DOWNLOAD_BASE_URL}/v${version}/${asset.name}`,
          signature: sig,
        };
      }
    }
  }
  return JSON.stringify({
    version,
    notes: `EasyOffice v${version}`,
    pub_date: "2026-11-26T00:00:00.000Z",
    platforms,
  });
}

function makeChecksumsText(names = releaseFileNames()) {
  const digest = "a".repeat(64);
  return `${names.map((name) => `${digest}  ${name}`).join("\n")}\n`;
}

function makeRelease({ tag = "v0.2.3", draft = false, prerelease = false, overrideAssets = null } = {}) {
  const names = overrideAssets ?? expectedReleaseAssetNames();
  return {
    tag_name: tag,
    draft,
    prerelease,
    assets: names.map((name, idx) => ({
      name,
      size: 1024 + idx,
      url: `https://api.github.com/repos/AquilaXk/easyoffice/releases/assets/${idx + 1}`,
    })),
  };
}

test("valid published release passes manifest, checksum, asset, and latest pointer checks", () => {
  const release = makeRelease({ tag: "v0.2.3", draft: false });
  const summary = verifyReleaseRecord({
    release,
    manifestText: makeManifestText("0.2.3"),
    sumsText: makeChecksumsText(),
    latestRelease: { tag_name: "v0.2.3" },
    expectedDraft: false,
    expectedVersion: "0.2.3",
  });
  assert.equal(summary.tag, "v0.2.3");
  assert.equal(summary.assetCount, 18);
  assert.equal(summary.checksumCount, 17);
  assert.equal(summary.manifestKeys, Object.keys(expectedManifestUrls("0.2.3")).length);
});

test("rejects missing release asset or zero-byte upload", () => {
  const fewer = expectedReleaseAssetNames().filter((n) => n !== "EasyOffice-macos-arm64.dmg");
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ overrideAssets: fewer }),
        manifestText: makeManifestText("0.2.3"),
        sumsText: makeChecksumsText(),
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /missing: EasyOffice-macos-arm64\.dmg/,
  );

  const zeroByte = makeRelease();
  zeroByte.assets[0].size = 0;
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: zeroByte,
        manifestText: makeManifestText("0.2.3"),
        sumsText: makeChecksumsText(),
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /invalid size 0/,
  );
});

test("rejects manifest signature targeting wrong version or wrong asset URL", () => {
  assert.throws(
    () => validateManifestPayload(makeManifestText("0.2.3", "0.2.2"), "0.2.3"),
    /signature targets '0\.2\.2', expected '0\.2\.3'/,
  );

  const parsed = JSON.parse(makeManifestText("0.2.3"));
  parsed.platforms["windows-x86_64"].url = "https://github.com/AquilaXk/easyoffice/releases/download/v0.2.2/EasyOffice-windows-x64-setup.exe";
  assert.throws(
    () => validateManifestPayload(JSON.stringify(parsed), "0.2.3"),
    /windows-x86_64.*url is/,
  );
});

test("rejects corrupted, missing, or duplicate SHA256SUMS entries", () => {
  assert.throws(() => validateChecksumsPayload(""), /empty/);
  assert.throws(() => validateChecksumsPayload("not-a-hash  file.exe\n"), /invalid SHA-256 digest/);

  const subset = releaseFileNames().slice(1);
  assert.throws(() => validateChecksumsPayload(makeChecksumsText(subset)), /missing:/);

  const dup = `${makeChecksumsText()}${"b".repeat(64)}  ${MANIFEST_FILE}\n`;
  assert.throws(() => validateChecksumsPayload(dup), /duplicate entry/);
});

test("rejects draft mismatch and stale /releases/latest pointer", () => {
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ draft: true }),
        manifestText: makeManifestText("0.2.3"),
        sumsText: makeChecksumsText(),
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /draft=true, expected draft=false/,
  );

  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ tag: "v0.2.3", draft: false }),
        manifestText: makeManifestText("0.2.3"),
        sumsText: makeChecksumsText(),
        latestRelease: { tag_name: "v0.2.2" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /\/releases\/latest points to 'v0\.2\.2', expected 'v0\.2\.3'/,
  );
});

test("verifyReleaseOnGitHub strips auth header across CDN redirect and writes step summary", async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "post-release-test-"));
  const summaryFile = path.join(tmpDir, "summary.md");
  try {
    const release = makeRelease({ tag: "v0.2.3", draft: false });
    const manifest = makeManifestText("0.2.3");
    const sums = makeChecksumsText();
    const cdnAuthSeen = [];

    const mockFetch = async (url, init = {}) => {
      if (url === "https://api.github.com/repos/AquilaXk/easyoffice/releases/tags/v0.2.3") {
        return new Response(JSON.stringify(release), { status: 200 });
      }
      if (url === "https://api.github.com/repos/AquilaXk/easyoffice/releases/latest") {
        return new Response(JSON.stringify({ tag_name: "v0.2.3", draft: false }), { status: 200 });
      }
      if (url.startsWith("https://api.github.com/repos/AquilaXk/easyoffice/releases/assets/")) {
        assert.equal(init.headers.Authorization, "Bearer secret-tok");
        const id = Number(url.split("/").pop());
        const asset = release.assets[id - 1];
        return new Response(null, {
          status: 302,
          headers: { location: `https://objects.githubusercontent.com/dl/${asset.name}` },
        });
      }
      if (url.startsWith("https://objects.githubusercontent.com/dl/")) {
        cdnAuthSeen.push(init.headers?.Authorization ?? null);
        if (url.endsWith(`/${MANIFEST_FILE}`)) return new Response(manifest, { status: 200 });
        if (url.endsWith(`/${SUMS_FILE}`)) return new Response(sums, { status: 200 });
      }
      return new Response("unexpected url", { status: 404 });
    };

    const { result } = await verifyReleaseOnGitHub({
      repo: "AquilaXk/easyoffice",
      tag: "v0.2.3",
      expectedDraft: false,
      token: "secret-tok",
      summaryPath: summaryFile,
      fetchImpl: mockFetch,
    });
    assert.equal(result.tag, "v0.2.3");
    assert.deepEqual(cdnAuthSeen, [null, null]);
    const written = fs.readFileSync(summaryFile, "utf8");
    assert.match(written, /Release Asset Verification \(v0\.2\.3, public\)/);
    assert.match(formatAssetSummaryMarkdown(release), /EasyOffice-windows-x64-setup\.exe/);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test("verifyReleaseOnGitHub falls back to /releases list when verifying a draft release", async () => {
  const release = makeRelease({ tag: "v0.2.3", draft: true });
  const manifest = makeManifestText("0.2.3");
  const sums = makeChecksumsText();

  const mockFetch = async (url) => {
    if (url.endsWith("/releases/tags/v0.2.3")) return new Response("Not Found", { status: 404 });
    if (url.endsWith("/releases?per_page=30")) return new Response(JSON.stringify([release]), { status: 200 });
    if (url.includes("/releases/assets/")) {
      const idx = Number(url.split("/").pop()) - 1;
      const name = release.assets[idx].name;
      if (name === MANIFEST_FILE) return new Response(manifest, { status: 200 });
      if (name === SUMS_FILE) return new Response(sums, { status: 200 });
    }
    return new Response("unexpected url", { status: 404 });
  };

  const { result } = await verifyReleaseOnGitHub({
    repo: "AquilaXk/easyoffice",
    tag: "v0.2.3",
    expectedDraft: true,
    token: "draft-tok",
    summaryPath: "",
    fetchImpl: mockFetch,
  });
  assert.equal(result.draft, true);
  assert.equal(result.assetCount, 18);
});
