import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  MANIFEST_FILE,
  PLATFORMS,
  RELEASE_DOWNLOAD_BASE_URL,
  SUMS_FILE,
  releaseFileNames,
} from "./release-assets.mjs";
import {
  MAX_PAYLOAD_BYTES,
  expectedManifestUrls,
  expectedReleaseAssetNames,
  fetchGitHubAssetDigest,
  fetchGitHubAssetText,
  formatAssetSummaryMarkdown,
  parseCliArgs,
  readResponseTextBounded,
  runPostReleaseCheckCli,
  selectSpotCheckAssets,
  signatureAssetNames,
  validateChecksumsPayload,
  validateManifestPayload,
  verifyReleaseOnGitHub,
  verifyReleaseRecord,
} from "./post-release-check.mjs";

function makeSig(version) {
  const comment = `trusted comment: timestamp:1700000000\tfile:app\tversion:${version}`;
  return Buffer.from(
    `untrusted comment: minisign signature\nRWQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==\n${comment}\nAAAA\n`,
    "utf8",
  ).toString("base64");
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

function makeChecksumsText(manifestText, names = releaseFileNames()) {
  const manifestHash = crypto.createHash("sha256").update(manifestText, "utf8").digest("hex");
  const digest = "a".repeat(64);
  return `${names.map((name) => `${name === MANIFEST_FILE ? manifestHash : digest}  ${name}`).join("\n")}\n`;
}

function makeRelease({ tag = "v0.2.3", draft = false, prerelease = false, overrideAssets = null } = {}) {
  const names = overrideAssets ?? expectedReleaseAssetNames();
  return {
    tag_name: tag,
    draft,
    prerelease,
    assets: names.map((name, idx) => ({
      name,
      state: "uploaded",
      size: 1024 + idx,
      url: `https://api.github.com/repos/AquilaXk/easyoffice/releases/assets/${idx + 1}`,
    })),
  };
}

/**
 * A release with every file it holds: `bodyOf(name)` is what the download of an asset answers, and `sums` is the
 * `SHA256SUMS.txt` of exactly those bodies (a `.sig` carries the manifest signature and a line break).
 */
function makeServedRelease({ tag = "v0.2.3", draft = false, prerelease = false } = {}) {
  const version = tag.slice(1);
  const release = makeRelease({ tag, draft, prerelease });
  const manifest = makeManifestText(version);
  const sig = makeSig(version);
  const bodies = new Map();
  for (const asset of release.assets) {
    if (asset.name === MANIFEST_FILE) bodies.set(asset.name, manifest);
    else if (asset.name.endsWith(".sig")) bodies.set(asset.name, `${sig}\n`);
    else if (asset.name !== SUMS_FILE) bodies.set(asset.name, Buffer.alloc(asset.size, 7));
  }
  const lines = releaseFileNames().map(
    (name) => `${crypto.createHash("sha256").update(bodies.get(name)).digest("hex")}  ${name}`,
  );
  const sums = `${lines.join("\n")}\n`;
  bodies.set(SUMS_FILE, sums);
  return { release, manifest, sums, sig, bodies, bodyOf: (name) => bodies.get(name) };
}

const assetNameOf = (release, url) => release.assets[Number(url.split("/").pop()) - 1].name;

test("valid published release passes manifest, checksum, asset, and latest pointer checks", () => {
  const release = makeRelease({ tag: "v0.2.3", draft: false });
  const manifestText = makeManifestText("0.2.3");
  const summary = verifyReleaseRecord({
    release,
    manifestText,
    sumsText: makeChecksumsText(manifestText),
    latestRelease: { tag_name: "v0.2.3" },
    expectedDraft: false,
    expectedVersion: "0.2.3",
  });
  assert.equal(summary.tag, "v0.2.3");
  assert.equal(summary.assetCount, 18);
  assert.equal(summary.checksumCount, 17);
  assert.equal(summary.manifestKeys, Object.keys(expectedManifestUrls("0.2.3")).length);
});

test("supports prerelease versions when marked prerelease and rejects prerelease mismatch", () => {
  const manifestText = makeManifestText("0.2.4-rc.1");
  const sumsText = makeChecksumsText(manifestText);
  const rc = makeRelease({ tag: "v0.2.4-rc.1", draft: false, prerelease: true });
  const summary = verifyReleaseRecord({
    release: rc,
    manifestText,
    sumsText,
    latestRelease: undefined,
    expectedDraft: false,
    expectedVersion: "0.2.4-rc.1",
  });
  assert.equal(summary.prerelease, true);

  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ tag: "v0.2.4-rc.1", draft: false, prerelease: false }),
        manifestText,
        sumsText,
        expectedDraft: false,
        expectedVersion: "0.2.4-rc.1",
      }),
    /prerelease=false, expected prerelease=true/,
  );

  const stableManifest = makeManifestText("0.2.3");
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ tag: "v0.2.3", draft: false, prerelease: true }),
        manifestText: stableManifest,
        sumsText: makeChecksumsText(stableManifest),
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /prerelease=true, expected prerelease=false/,
  );
});

test("rejects missing release asset, incomplete upload state, or zero-byte upload", () => {
  const manifestText = makeManifestText("0.2.3");
  const sumsText = makeChecksumsText(manifestText);

  const fewer = expectedReleaseAssetNames().filter((n) => n !== "EasyOffice-macos-arm64.dmg");
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ overrideAssets: fewer }),
        manifestText,
        sumsText,
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /missing: EasyOffice-macos-arm64\.dmg/,
  );

  const starter = makeRelease();
  starter.assets[0].state = "starter";
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: starter,
        manifestText,
        sumsText,
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /incomplete \(state=starter\)/,
  );

  const missingState = makeRelease();
  delete missingState.assets[0].state;
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: missingState,
        manifestText,
        sumsText,
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /incomplete \(state=undefined\)/,
  );

  const nullState = makeRelease();
  nullState.assets[0].state = null;
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: nullState,
        manifestText,
        sumsText,
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /incomplete \(state=null\)/,
  );

  const zeroByte = makeRelease();
  zeroByte.assets[0].size = 0;
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: zeroByte,
        manifestText,
        sumsText,
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /invalid size 0/,
  );

  for (const badSize of [Number.NaN, Number.POSITIVE_INFINITY, 12.5, -1]) {
    const badSizeRelease = makeRelease();
    badSizeRelease.assets[0].size = badSize;
    assert.throws(
      () =>
        verifyReleaseRecord({
          release: badSizeRelease,
          manifestText,
          sumsText,
          latestRelease: { tag_name: "v0.2.3" },
          expectedDraft: false,
          expectedVersion: "0.2.3",
        }),
      /invalid size/,
    );
  }

  const nullAssetRelease = makeRelease();
  nullAssetRelease.assets[0] = null;
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: nullAssetRelease,
        manifestText,
        sumsText,
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /malformed asset entry/,
  );
});

test("rejects manifest signature targeting wrong version, wrong URL, or mismatched SHA256SUMS digest", () => {
  assert.throws(
    () => validateManifestPayload(makeManifestText("0.2.3", "0.2.2"), "0.2.3"),
    /signature targets '0\.2\.2', expected '0\.2\.3'/,
  );

  const parsed = JSON.parse(makeManifestText("0.2.3"));
  parsed.platforms["windows-x86_64"].url =
    "https://github.com/AquilaXk/easyoffice/releases/download/v0.2.2/EasyOffice-windows-x64-setup.exe";
  assert.throws(
    () => validateManifestPayload(JSON.stringify(parsed), "0.2.3"),
    /windows-x86_64.*url is/,
  );

  const manifestText = makeManifestText("0.2.3");
  const staleSums = makeChecksumsText(makeManifestText("0.2.2"));
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ tag: "v0.2.3", draft: false }),
        manifestText,
        sumsText: staleSums,
        latestRelease: { tag_name: "v0.2.3" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /SHA256SUMS\.txt digest for latest\.json .* does not match downloaded latest\.json/,
  );
});

test("rejects corrupted, missing, or duplicate SHA256SUMS entries", () => {
  const manifestText = makeManifestText("0.2.3");
  assert.throws(() => validateChecksumsPayload(""), /empty/);
  assert.throws(() => validateChecksumsPayload("not-a-hash  file.exe\n"), /invalid SHA-256 digest/);

  const subset = releaseFileNames().slice(1);
  assert.throws(() => validateChecksumsPayload(makeChecksumsText(manifestText, subset)), /missing:/);

  const dup = `${makeChecksumsText(manifestText)}${"b".repeat(64)}  ${MANIFEST_FILE}\n`;
  assert.throws(() => validateChecksumsPayload(dup), /duplicate entry/);
});

test("rejects draft mismatch and stale /releases/latest pointer", () => {
  const manifestText = makeManifestText("0.2.3");
  const sumsText = makeChecksumsText(manifestText);
  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ draft: true }),
        manifestText,
        sumsText,
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /draft=true, expected draft=false/,
  );

  assert.throws(
    () =>
      verifyReleaseRecord({
        release: makeRelease({ tag: "v0.2.3", draft: false }),
        manifestText,
        sumsText,
        latestRelease: { tag_name: "v0.2.2" },
        expectedDraft: false,
        expectedVersion: "0.2.3",
      }),
    /\/releases\/latest points to 'v0\.2\.2', expected 'v0\.2\.3'/,
  );
});

test("readResponseTextBounded rejects oversized streams before buffering whole body", async () => {
  let chunksPulled = 0;
  const stream = new ReadableStream({
    pull(controller) {
      chunksPulled += 1;
      controller.enqueue(new Uint8Array(1024 * 1024));
    },
  });
  const res = new Response(stream, { status: 200 });
  await assert.rejects(
    () => readResponseTextBounded(res, "oversized test payload", MAX_PAYLOAD_BYTES),
    /exceeds 2097152 bytes/,
  );
  assert.ok(chunksPulled <= 4, `expected early stream abort, pulled ${chunksPulled} chunks`);
});

test("verifyReleaseOnGitHub strips auth header across CDN redirect, retries lag, and writes step summary", async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "post-release-test-"));
  const summaryFile = path.join(tmpDir, "summary.md");
  try {
    const served = makeServedRelease({ tag: "v0.2.3", draft: false });
    const { release } = served;
    const cdnAuthSeen = [];
    let latestCalls = 0;

    const mockFetch = async (url, init = {}) => {
      if (url === "https://api.github.com/repos/AquilaXk/easyoffice/releases/tags/v0.2.3") {
        return new Response(JSON.stringify(release), { status: 200 });
      }
      if (url === "https://api.github.com/repos/AquilaXk/easyoffice/releases/latest") {
        latestCalls += 1;
        const tag = latestCalls === 1 ? "v0.2.2" : "v0.2.3";
        return new Response(JSON.stringify({ tag_name: tag, draft: false }), { status: 200 });
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
        const body = served.bodyOf(decodeURIComponent(url.split("/").pop()));
        if (body !== undefined) return new Response(body, { status: 200 });
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
      retryDelayMs: 0,
    });
    assert.equal(result.tag, "v0.2.3");
    assert.equal(latestCalls, 2);
    assert.ok(cdnAuthSeen.length >= 2 + 7 + 2, `manifest, sums, 7 signatures and 2 packages were downloaded (${cdnAuthSeen.length})`);
    assert.ok(cdnAuthSeen.every((auth) => auth === null), "no CDN request carries the token");
    assert.equal(result.signatureCount, 7);
    assert.equal(result.spotChecked.length, 2);
    const written = fs.readFileSync(summaryFile, "utf8");
    assert.match(written, /Release Asset Verification \(v0\.2\.3, public\)/);
    assert.match(formatAssetSummaryMarkdown(release), /EasyOffice-windows-x64-setup\.exe/);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test("verifyReleaseOnGitHub paginates /releases list when verifying a draft release", async () => {
  const served = makeServedRelease({ tag: "v0.2.3", draft: true });
  const { release } = served;

  const mockFetch = async (url) => {
    if (url.endsWith("/releases/tags/v0.2.3")) return new Response("Not Found", { status: 404 });
    if (url.endsWith("/releases?per_page=100")) {
      return new Response(JSON.stringify([makeRelease({ tag: "v0.2.2", draft: false })]), {
        status: 200,
        headers: { link: '<https://api.github.com/repos/AquilaXk/easyoffice/releases?per_page=100&page=2>; rel="next"' },
      });
    }
    if (url.endsWith("/releases?per_page=100&page=2")) {
      return new Response(JSON.stringify([release]), { status: 200 });
    }
    if (url.includes("/releases/assets/")) {
      return new Response(served.bodyOf(assetNameOf(release, url)), { status: 200 });
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
    retryDelayMs: 0,
  });
  assert.equal(result.draft, true);
  assert.equal(result.assetCount, 18);
});

test("fetchGitHubAssetText rejects non-HTTPS, relative, malformed, and credentialed redirect Locations without following them", async () => {
  const asset = {
    name: MANIFEST_FILE,
    url: "https://api.github.com/repos/AquilaXk/easyoffice/releases/assets/1",
  };
  const badLocations = [
    { location: "", expected: /missing Location header/ },
    { location: "http://objects.githubusercontent.com/dl/latest.json", expected: /non-HTTPS Location URL/ },
    { location: "file:///etc/passwd", expected: /non-HTTPS Location URL/ },
    { location: "ftp://objects.githubusercontent.com/dl/latest.json", expected: /non-HTTPS Location URL/ },
    { location: "javascript:alert(1)", expected: /non-HTTPS Location URL/ },
    { location: "/relative/dl/latest.json", expected: /invalid Location URL/ },
    { location: "//objects.githubusercontent.com/dl/latest.json", expected: /invalid Location URL/ },
    { location: "not a valid url", expected: /invalid Location URL/ },
    { location: "https://", expected: /invalid Location URL/ },
    { location: "https://objects.github\nusercontent.com/dl/latest.json", expected: /invalid Location URL/, rawHeader: true },
    { location: "https://objects.githubusercontent.com/dl/latest\tjson", expected: /invalid Location URL/ },
    { location: "https://objects.githubusercontent.com/dl with space", expected: /invalid Location URL/ },
    { location: "https:objects.githubusercontent.com/dl/latest.json", expected: /malformed HTTPS Location URL/ },
    { location: "https:///objects.githubusercontent.com/dl/latest.json", expected: /malformed HTTPS Location URL/ },
    { location: "https:////objects.githubusercontent.com/dl/latest.json", expected: /malformed HTTPS Location URL/ },
    { location: "https://objects.githubusercontent.com\\foo", expected: /malformed HTTPS Location URL/ },
    { location: "https://user:pass@objects.githubusercontent.com/dl/latest.json", expected: /credentials in Location URL/ },
    { location: "https://@objects.githubusercontent.com/dl/latest.json", expected: /credentials in Location URL/ },
    { location: "https://objects.githubusercontent.com\\@evil.com/dl/latest.json", expected: /credentials in Location URL/ },
  ];

  for (const { location, expected, rawHeader } of badLocations) {
    const seenUrls = [];
    const mockFetch = async (url) => {
      seenUrls.push(url);
      if (rawHeader) {
        return { status: 302, headers: new Map([["location", location]]), body: null };
      }
      const headers = location === "" ? undefined : { location };
      return new Response(null, { status: 302, headers });
    };
    await assert.rejects(() => fetchGitHubAssetText(asset, "tok", mockFetch), expected);
    assert.deepEqual(seenUrls, [asset.url], `must not follow bad Location '${location}'`);
  }
});

test("fetchGitHubAssetText validates secondary redirects and caps excessive redirect chains", async () => {
  const asset = {
    name: SUMS_FILE,
    url: "https://api.github.com/repos/AquilaXk/easyoffice/releases/assets/2",
  };
  const seen = [];
  const downgradeFetch = async (url) => {
    seen.push(url);
    if (url === asset.url) {
      return new Response(null, {
        status: 302,
        headers: { location: "https://objects.githubusercontent.com/cdn-1" },
      });
    }
    if (url === "https://objects.githubusercontent.com/cdn-1") {
      return new Response(null, {
        status: 307,
        headers: { location: "http://insecure.example.com/payload" },
      });
    }
    throw new Error(`should not fetch '${url}'`);
  };
  await assert.rejects(() => fetchGitHubAssetText(asset, "tok", downgradeFetch), /non-HTTPS Location URL/);
  assert.deepEqual(seen, [asset.url, "https://objects.githubusercontent.com/cdn-1"]);

  let redirects = 0;
  const loopFetch = async () => {
    redirects += 1;
    return new Response(null, {
      status: 302,
      headers: { location: `https://objects.githubusercontent.com/cdn-${redirects}` },
    });
  };
  await assert.rejects(() => fetchGitHubAssetText(asset, "tok", loopFetch), /exceeded 3 redirects/);
  assert.equal(redirects, 4);
});

test("verifyReleaseOnGitHub fails closed without retrying when an asset redirect uses non-HTTPS Location", async () => {
  const release = makeRelease({ tag: "v0.2.3", draft: true });
  let releaseCalls = 0;
  const mockFetch = async (url) => {
    if (url.endsWith("/releases/tags/v0.2.3")) {
      releaseCalls += 1;
      return new Response(JSON.stringify(release), { status: 200 });
    }
    if (url.includes("/releases/assets/")) {
      return new Response(null, {
        status: 302,
        headers: { location: "http://objects.githubusercontent.com/dl/downgraded" },
      });
    }
    return new Response("unexpected url", { status: 404 });
  };
  await assert.rejects(
    () =>
      verifyReleaseOnGitHub({
        repo: "AquilaXk/easyoffice",
        tag: "v0.2.3",
        expectedDraft: true,
        token: "tok",
        summaryPath: "",
        fetchImpl: mockFetch,
        maxAttempts: 4,
        retryDelayMs: 0,
      }),
    /non-HTTPS Location URL/,
  );
  assert.equal(releaseCalls, 1);
});

test("fetchGitHubAssetText fails closed on non-HTTPS initial asset.url, strips token off api.github.com, and rejects non-redirect 304", async () => {
  let called = false;
  await assert.rejects(
    () =>
      fetchGitHubAssetText(
        { name: MANIFEST_FILE, url: "http://api.github.com/repos/AquilaXk/easyoffice/releases/assets/1" },
        "secret-tok",
        async () => {
          called = true;
          return new Response("ok", { status: 200 });
        },
      ),
    /non-HTTPS URL/,
  );
  assert.equal(called, false, "must reject non-HTTPS initial asset.url before fetch");

  let seenAuth = "unset";
  const nonApiText = await fetchGitHubAssetText(
    { name: SUMS_FILE, url: "https://objects.githubusercontent.com/direct/SHA256SUMS.txt" },
    "secret-tok",
    async (_url, init) => {
      seenAuth = init.headers?.Authorization ?? null;
      return new Response("ok", { status: 200 });
    },
  );
  assert.equal(nonApiText, "ok");
  assert.equal(seenAuth, null, "must not send Authorization header outside api.github.com");

  await assert.rejects(
    () =>
      fetchGitHubAssetText(
        { name: MANIFEST_FILE, url: "https://api.github.com/repos/AquilaXk/easyoffice/releases/assets/1" },
        "tok",
        async () => new Response(null, { status: 304 }),
      ),
    /failed \(304\)/,
  );
});

test("verifyReleaseOnGitHub rejects malformed Link pagination URLs containing control chars or triple slashes", async () => {
  for (const link of [
    '<https:///api.github.com/repos/AquilaXk/easyoffice/releases?page=2>; rel="next"',
    '<https://api.github.com/repos/AquilaXk/easyoffice/releases\t?page=2>; rel="next"',
  ]) {
    const mockFetch = async (url) => {
      if (url.endsWith("/releases/tags/v0.2.3")) return new Response("Not Found", { status: 404 });
      if (url.endsWith("/releases?per_page=100")) {
        return new Response(JSON.stringify([]), {
          status: 200,
          headers: { link },
        });
      }
      return new Response("unexpected", { status: 404 });
    };
    await assert.rejects(
      () =>
        verifyReleaseOnGitHub({
          repo: "AquilaXk/easyoffice",
          tag: "v0.2.3",
          expectedDraft: true,
          token: "tok",
          summaryPath: "",
          fetchImpl: mockFetch,
          retryDelayMs: 0,
        }),
      /refusing non-GitHub API pagination URL/,
    );
  }
});

test("parseCliArgs parses --draft, --expect-draft, and --expect-public into expectedDraft", () => {
  assert.deepEqual(
    parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--draft", "true"], {}),
    { repo: "AquilaXk/easyoffice", tag: "v0.2.3", expectedDraft: true },
  );
  assert.deepEqual(
    parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--draft", "false"], {}),
    { repo: "AquilaXk/easyoffice", tag: "v0.2.3", expectedDraft: false },
  );
  assert.deepEqual(
    parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--expect-draft"], {}),
    { repo: "AquilaXk/easyoffice", tag: "v0.2.3", expectedDraft: true },
  );
  assert.deepEqual(
    parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--expect-public"], {}),
    { repo: "AquilaXk/easyoffice", tag: "v0.2.3", expectedDraft: false },
  );
  assert.deepEqual(
    parseCliArgs(["--tag", "v0.2.3"], { GITHUB_REPOSITORY: "AquilaXk/easyoffice" }),
    { repo: "AquilaXk/easyoffice", tag: "v0.2.3", expectedDraft: undefined },
  );

  assert.deepEqual(
    parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--draft", "true", "--expect-draft"], {}),
    { repo: "AquilaXk/easyoffice", tag: "v0.2.3", expectedDraft: true },
  );

  assert.throws(() => parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--draft", "yes"], {}), /invalid --draft/);
  assert.throws(() => parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--draft"], {}), /incomplete argument/);
  assert.throws(() => parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--expect-draft", "--expect-public"], {}), /conflicting draft\/public/);
  assert.throws(() => parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--draft", "true", "--expect-public"], {}), /conflicting draft\/public/);
  assert.throws(() => parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--draft", "false", "--draft", "true"], {}), /conflicting draft\/public/);
  assert.throws(() => parseCliArgs(["--repo", "AquilaXk/easyoffice"], {}), /usage:/);
  assert.throws(() => parseCliArgs(["--tag", "v0.2.3"], {}), /usage:/);
  assert.throws(() => parseCliArgs(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", "--bogus"], {}), /unknown/);
});

test("CLI draft/public options propagate expectedDraft through runPostReleaseCheckCli and reject mismatches before /releases/latest", async () => {
  const servedDraft = makeServedRelease({ tag: "v0.2.3", draft: true });
  const servedPublic = makeServedRelease({ tag: "v0.2.3", draft: false });
  const draftRelease = servedDraft.release;
  const publicRelease = servedPublic.release;

  for (const flagArgs of [["--draft", "false"], ["--expect-public"]]) {
    let listCalled = false;
    const draftOnlyFetch = async (url) => {
      if (url.endsWith("/releases/tags/v0.2.3")) return new Response("Not Found", { status: 404 });
      if (url.includes("/releases?per_page=100")) {
        listCalled = true;
        return new Response(JSON.stringify([draftRelease]), { status: 200 });
      }
      return new Response("unexpected", { status: 404 });
    };
    await assert.rejects(
      () =>
        runPostReleaseCheckCli(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", ...flagArgs], {
          env: { GH_TOKEN: "tok" },
          summaryPath: "",
          fetchImpl: draftOnlyFetch,
          maxAttempts: 2,
          retryDelayMs: 0,
        }),
      /404/,
    );
    assert.equal(listCalled, false, `${flagArgs.join(" ")} must disable draft list fallback`);

    let tagAttempts = 0;
    const directDraftFetch = async (url) => {
      if (url.endsWith("/releases/tags/v0.2.3")) {
        tagAttempts += 1;
        return new Response(JSON.stringify(draftRelease), { status: 200 });
      }
      return new Response("unexpected", { status: 404 });
    };
    await assert.rejects(
      () =>
        runPostReleaseCheckCli(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", ...flagArgs], {
          env: { GH_TOKEN: "tok" },
          summaryPath: "",
          fetchImpl: directDraftFetch,
          maxAttempts: 3,
          retryDelayMs: 0,
        }),
      /draft=true, expected draft=false/,
    );
    assert.equal(tagAttempts, 1, `${flagArgs.join(" ")} draft mismatch must fail without retry`);
  }

  for (const flagArgs of [["--draft", "true"], ["--expect-draft"]]) {
    let tagAttempts = 0;
    let latestAttempts = 0;
    const publishedFetch = async (url) => {
      if (url.endsWith("/releases/tags/v0.2.3")) {
        tagAttempts += 1;
        return new Response(JSON.stringify(publicRelease), { status: 200 });
      }
      if (url.endsWith("/releases/latest")) {
        latestAttempts += 1;
        return new Response("Not Found", { status: 404 });
      }
      return new Response("unexpected", { status: 404 });
    };
    await assert.rejects(
      () =>
        runPostReleaseCheckCli(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", ...flagArgs], {
          env: { GH_TOKEN: "tok" },
          summaryPath: "",
          fetchImpl: publishedFetch,
          maxAttempts: 3,
          retryDelayMs: 0,
        }),
      /draft=false, expected draft=true/,
    );
    assert.equal(tagAttempts, 1, `${flagArgs.join(" ")} public mismatch must fail without retry`);
    assert.equal(latestAttempts, 0, `${flagArgs.join(" ")} must reject draft mismatch before querying /releases/latest`);

    const logs = [];
    const matchedDraft = await runPostReleaseCheckCli(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", ...flagArgs], {
      env: { GH_TOKEN: "tok" },
      summaryPath: "",
      log: (msg) => logs.push(msg),
      fetchImpl: async (url) => {
        if (url.endsWith("/releases/tags/v0.2.3")) return new Response("Not Found", { status: 404 });
        if (url.endsWith("/releases?per_page=100")) return new Response(JSON.stringify([draftRelease]), { status: 200 });
        if (url.includes("/releases/assets/")) {
          return new Response(servedDraft.bodyOf(assetNameOf(draftRelease, url)), { status: 200 });
        }
        return new Response("unexpected", { status: 404 });
      },
      retryDelayMs: 0,
    });
    assert.equal(matchedDraft.draft, true);
    assert.match(logs[0], /Verified release v0\.2\.3 \(draft=true\)/);
  }

  for (const flagArgs of [["--draft", "false"], ["--expect-public"]]) {
    const logs = [];
    const matchedPublic = await runPostReleaseCheckCli(["--repo", "AquilaXk/easyoffice", "--tag", "v0.2.3", ...flagArgs], {
      env: { GH_TOKEN: "tok" },
      summaryPath: "",
      log: (msg) => logs.push(msg),
      fetchImpl: async (url) => {
        if (url.endsWith("/releases/tags/v0.2.3")) return new Response(JSON.stringify(publicRelease), { status: 200 });
        if (url.endsWith("/releases/latest")) return new Response(JSON.stringify(publicRelease), { status: 200 });
        if (url.includes("/releases/assets/")) {
          return new Response(servedPublic.bodyOf(assetNameOf(publicRelease, url)), { status: 200 });
        }
        return new Response("unexpected", { status: 404 });
      },
      retryDelayMs: 0,
    });
    assert.equal(matchedPublic.draft, false);
    assert.match(logs[0], /Verified release v0\.2\.3 \(draft=false\)/);
  }

  const malformedAssetsRelease = makeRelease({ tag: "v0.2.3", draft: false });
  malformedAssetsRelease.assets[0] = null;
  await assert.rejects(
    () =>
      verifyReleaseOnGitHub({
        repo: "AquilaXk/easyoffice",
        tag: "v0.2.3",
        expectedDraft: false,
        token: "tok",
        summaryPath: "",
        fetchImpl: async (url) => {
          if (url.endsWith("/releases/tags/v0.2.3")) {
            return new Response(JSON.stringify(malformedAssetsRelease), { status: 200 });
          }
          return new Response("unexpected", { status: 404 });
        },
        retryDelayMs: 0,
      }),
    /malformed asset entry/,
  );
});

/** Runs the whole check against `served`, whose bodies a test may have changed before. */
function checkServed(served, token = "tok") {
  const { release } = served;
  return verifyReleaseOnGitHub({
    repo: "AquilaXk/easyoffice",
    tag: release.tag_name,
    expectedDraft: release.draft,
    token,
    summaryPath: "",
    retryDelayMs: 0,
    fetchImpl: async (url) => {
      if (url.endsWith(`/releases/tags/${release.tag_name}`)) return new Response(JSON.stringify(release), { status: 200 });
      if (url.endsWith("/releases/latest")) return new Response(JSON.stringify(release), { status: 200 });
      if (url.includes("/releases/assets/")) {
        const body = served.bodyOf(assetNameOf(release, url));
        return new Response(body, { status: 200, headers: { "content-length": String(Buffer.byteLength(body)) } });
      }
      return new Response("unexpected", { status: 404 });
    },
  });
}

/** The same served release with one file replaced, and the digest of that file in SHA256SUMS.txt kept or updated. */
function withBody(served, name, body, { updateSums }) {
  const bodies = new Map(served.bodies);
  bodies.set(name, body);
  if (updateSums) {
    const digest = crypto.createHash("sha256").update(body).digest("hex");
    const sums = served.sums
      .split("\n")
      .map((line) => (line.endsWith(`  ${name}`) ? `${digest}  ${name}` : line))
      .join("\n");
    bodies.set(SUMS_FILE, sums);
  }
  return { ...served, bodies, bodyOf: (n) => bodies.get(n) };
}

test("the release passes when every signature file matches latest.json and the two smallest packages match SHA256SUMS", async () => {
  const served = makeServedRelease();
  const { result } = await checkServed(served);
  assert.equal(result.signatureCount, signatureAssetNames().length);
  assert.deepEqual(result.spotChecked, selectSpotCheckAssets(new Map(served.release.assets.map((a) => [a.name, a]))).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)));
});

test("a .sig that differs from the signature in latest.json fails, even when SHA256SUMS lists that .sig", async () => {
  const served = makeServedRelease();
  const other = Buffer.from(makeSig("0.2.3").replace(/.$/, "A"), "utf8").toString("utf8");
  const sigName = signatureAssetNames()[0];
  const changed = withBody(served, sigName, `${other}\n`, { updateSums: true });
  await assert.rejects(() => checkServed(changed), /signature does not match .*\.sig of /);
});

test("a .sig changed after SHA256SUMS was written fails on its digest", async () => {
  const served = makeServedRelease();
  const sigName = signatureAssetNames()[1];
  const changed = withBody(served, sigName, `${served.sig}\n\n`, { updateSums: false });
  await assert.rejects(() => checkServed(changed), new RegExp(`digest for ${sigName.replace(/\./g, "\\.")}`));
});

test("a missing or empty .sig body fails", async () => {
  const served = makeServedRelease();
  const sigName = signatureAssetNames()[2];
  const changed = withBody(served, sigName, "", { updateSums: true });
  await assert.rejects(() => checkServed(changed), /is empty|has invalid size|was not downloaded or is empty/);
});

test("a package whose bytes differ from its SHA256SUMS digest fails the spot check", async () => {
  const served = makeServedRelease();
  const names = selectSpotCheckAssets(new Map(served.release.assets.map((a) => [a.name, a])));
  assert.equal(names.length, 2);
  const size = served.release.assets.find((a) => a.name === names[1]).size;
  const changed = withBody(served, names[1], Buffer.alloc(size, 9), { updateSums: false });
  await assert.rejects(() => checkServed(changed), new RegExp(`digest for ${names[1].replace(/\./g, "\\.")}`));
});

test("a spot-checked package that is longer or shorter than the release declares fails without hashing the surplus", async () => {
  const served = makeServedRelease();
  const names = selectSpotCheckAssets(new Map(served.release.assets.map((a) => [a.name, a])));
  const size = served.release.assets.find((a) => a.name === names[0]).size;
  await assert.rejects(
    () => checkServed(withBody(served, names[0], Buffer.alloc(size + 1, 7), { updateSums: true })),
    /is longer than/,
  );
  await assert.rejects(
    () => checkServed(withBody(served, names[0], Buffer.alloc(size - 1, 7), { updateSums: true })),
    /is \d+ bytes, the release declares/,
  );
});

test("the spot check takes the two smallest packages, in a stable order, and none over the size cap", () => {
  const assets = new Map(
    ["EasyOffice-linux-x64.deb", "EasyOffice-linux-x64.rpm", "EasyOffice-linux-x64.AppImage", "EasyOffice-windows-x64-setup.exe"].map((name, i) => [
      name,
      { name, size: [500, 500, 100, 900][i] },
    ]),
  );
  assert.deepEqual(selectSpotCheckAssets(assets), ["EasyOffice-linux-x64.AppImage", "EasyOffice-linux-x64.deb"]);
  assert.deepEqual(selectSpotCheckAssets(assets, { maxBytes: 400 }), ["EasyOffice-linux-x64.AppImage"]);
  assert.deepEqual(selectSpotCheckAssets(assets, { count: 3 }).length, 3);
  assert.deepEqual(selectSpotCheckAssets(new Map()), []);
});

test("fetchGitHubAssetDigest hashes the stream, stops at the size cap and refuses a declared length over it", async () => {
  const body = Buffer.alloc(3000, 5);
  const asset = { name: "EasyOffice-linux-x64.deb", size: 3000, url: "https://api.github.com/repos/AquilaXk/easyoffice/releases/assets/1" };
  const ok = await fetchGitHubAssetDigest(asset, "tok", async () => new Response(body, { status: 200 }));
  assert.deepEqual(ok, { sha256: crypto.createHash("sha256").update(body).digest("hex"), bytes: 3000 });

  let pulled = 0;
  const endless = new ReadableStream({
    pull(controller) {
      pulled += 1;
      controller.enqueue(new Uint8Array(1024));
    },
  });
  await assert.rejects(
    () => fetchGitHubAssetDigest({ ...asset, size: 4096 }, "tok", async () => new Response(endless, { status: 200 })),
    /is longer than 4096 bytes/,
  );
  assert.ok(pulled <= 8, `the download stopped early (${pulled} chunks)`);

  await assert.rejects(
    () =>
      fetchGitHubAssetDigest(asset, "tok", async () => new Response("x", { status: 200, headers: { "content-length": "999999999" } })),
    /is longer than 3000 bytes/,
  );
  await assert.rejects(
    () => fetchGitHubAssetDigest({ ...asset, size: 10 }, "tok", async () => new Response(Buffer.alloc(11), { status: 200 })),
    /is longer than 10 bytes/,
  );
});

test("verifyReleaseRecord compares signatures and spot checks only for what it is given", () => {
  const served = makeServedRelease();
  const base = { release: served.release, manifestText: served.manifest, sumsText: served.sums, expectedDraft: false, expectedVersion: "0.2.3", latestRelease: { tag_name: "v0.2.3" } };
  assert.equal(verifyReleaseRecord(base).signatureCount, 0);
  assert.throws(() => verifyReleaseRecord({ ...base, signatureTexts: new Map() }), /was not downloaded or is empty/);
});

test("post-release-check CLI escapes %, CR, and LF in ::error:: output", () => {
  const scriptPath = fileURLToPath(new URL("./post-release-check.mjs", import.meta.url));
  const res = spawnSync(
    process.execPath,
    [scriptPath, "--repo", "AquilaXk/bad%repo\r\n::warning::injected", "--tag", "v0.2.3"],
    { encoding: "utf8" },
  );
  assert.equal(res.status, 1);
  assert.match(res.stderr, /^::error::invalid repository 'AquilaXk\/bad%25repo%0D%0A::warning::injected'\r?\n$/);
});
