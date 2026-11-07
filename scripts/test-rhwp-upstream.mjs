// Tests for the rhwp upstream pin guard (scripts/verify-rhwp-upstream.mjs) and its wiring into
// the repository hygiene guard. Uses temporary git repositories only; no network access.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  parseGitmodules,
  checkGitmodules,
  checkConfig,
  verifyUpstream,
  sparseCheckoutPlan,
  resolveGitExecutable,
} from "./verify-rhwp-upstream.mjs";
import { REQUIRED_VENDOR_FILES, checkVendorCore } from "./verify-rhwp-core.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
const URL_OK = "https://github.com/edwardkim/rhwp.git";
const COMMIT = "1a76570e833917d15817415a53c09ad61ab3203f";
const OTHER = "2b87681f944a28e26928e463026b4c4d4e2d4f40";

function git(cwd, ...args) {
  return execFileSync(resolveGitExecutable(), args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
}

function gitmodulesText(url = URL_OK) {
  return `[submodule "third_party/rhwp"]\n\tpath = third_party/rhwp\n\turl = ${url}\n\tshallow = true\n`;
}

function makeRepo({ commit = COMMIT, url = URL_OK, config = {}, gitlink = commit } = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "eo-rhwp-"));
  git(dir, "init", "-q");
  fs.mkdirSync(path.join(dir, "config"));
  const base = JSON.parse(fs.readFileSync(path.join(REPO, "config/rhwp-upstream.json"), "utf8"));
  const cfg = { ...base, commit, ...config };
  fs.writeFileSync(path.join(dir, "config/rhwp-upstream.json"), JSON.stringify(cfg));
  fs.writeFileSync(path.join(dir, ".gitmodules"), gitmodulesText(url));
  git(dir, "update-index", "--add", "--cacheinfo", `160000,${gitlink},third_party/rhwp`);
  fs.cpSync(path.join(REPO, "vendor/rhwp-core"), path.join(dir, "vendor/rhwp-core"), { recursive: true });
  return dir;
}

function cleanup(dir) {
  fs.rmSync(dir, { recursive: true, force: true });
}

test("the real repository passes without a checkout", () => {
  assert.deepEqual(verifyUpstream(REPO), []);
});

test("gitmodules parsing accepts only the pinned entry", () => {
  const cfg = JSON.parse(fs.readFileSync(path.join(REPO, "config/rhwp-upstream.json"), "utf8"));
  assert.deepEqual(checkGitmodules(gitmodulesText(), cfg), []);
  assert.ok(checkGitmodules(gitmodulesText("https://example.com/x/rhwp.git"), cfg).length > 0);
  assert.ok(checkGitmodules(gitmodulesText().replace("shallow = true", "shallow = false"), cfg).length > 0);
  assert.ok(checkGitmodules(`${gitmodulesText()}[submodule "extra"]\n\tpath = extra\n\turl = ${URL_OK}\n`, cfg).length > 0);
  assert.ok(checkGitmodules("", cfg).length > 0);
  assert.equal(parseGitmodules(gitmodulesText()).get("third_party/rhwp").url, URL_OK);
});

test("config sanity is enforced", () => {
  const cfg = JSON.parse(fs.readFileSync(path.join(REPO, "config/rhwp-upstream.json"), "utf8"));
  assert.deepEqual(checkConfig(cfg), []);
  assert.ok(checkConfig({ ...cfg, commit: "abc" }).length > 0);
  assert.ok(checkConfig({ ...cfg, version: "latest" }).length > 0);
  assert.ok(checkConfig({ ...cfg, sparse: [] }).length > 0);
  assert.ok(checkConfig({ ...cfg, wasmPackVersion: "latest" }).length > 0);
  assert.ok(checkConfig({ ...cfg, url: "https://example.com/rhwp.git" }).length > 0);
});

test("a temporary repository with the pinned gitlink passes", () => {
  const dir = makeRepo();
  try {
    assert.deepEqual(verifyUpstream(dir), []);
  } finally {
    cleanup(dir);
  }
});

test("a changed gitlink commit fails", () => {
  const dir = makeRepo({ gitlink: OTHER });
  try {
    const errs = verifyUpstream(dir);
    assert.ok(errs.some((e) => e.includes("gitlink")), errs.join("\n"));
  } finally {
    cleanup(dir);
  }
});

test("a missing gitlink fails", () => {
  const dir = makeRepo();
  git(dir, "update-index", "--force-remove", "third_party/rhwp");
  try {
    assert.ok(verifyUpstream(dir).some((e) => e.includes("gitlink")));
  } finally {
    cleanup(dir);
  }
});

test("a wrong .gitmodules URL fails", () => {
  const dir = makeRepo({ url: "https://github.com/someone-else/rhwp.git" });
  try {
    assert.ok(verifyUpstream(dir).some((e) => e.includes(".gitmodules")));
  } finally {
    cleanup(dir);
  }
});

test("a version mismatch with the checked-out sources fails", () => {
  const dir = makeRepo({ config: { version: "0.9.0" } });
  // Simulate the checked-out upstream with a nested repository whose HEAD matches the pin.
  const sub = path.join(dir, "third_party/rhwp");
  fs.mkdirSync(sub, { recursive: true });
  git(sub, "init", "-q");
  git(sub, "remote", "add", "origin", URL_OK);
  fs.writeFileSync(path.join(sub, "Cargo.toml"), '[package]\nname = "rhwp"\nversion = "0.8.7"\n');
  git(sub, "add", ".");
  git(sub, "-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-q", "-m", "x");
  const head = git(sub, "rev-parse", "HEAD").trim();
  git(dir, "update-index", "--add", "--cacheinfo", `160000,${head},third_party/rhwp`);
  const cfgPath = path.join(dir, "config/rhwp-upstream.json");
  const provPath = path.join(dir, "vendor/rhwp-core/PROVENANCE.json");
  const writeCfg = (next) => {
    fs.writeFileSync(cfgPath, JSON.stringify(next));
    const prov = JSON.parse(fs.readFileSync(provPath, "utf8"));
    fs.writeFileSync(provPath, JSON.stringify({ ...prov, commit: next.commit }));
  };
  const cfg = JSON.parse(fs.readFileSync(cfgPath, "utf8"));
  writeCfg({ ...cfg, commit: head });
  try {
    const errs = verifyUpstream(dir);
    assert.ok(errs.some((e) => e.includes("version")), errs.join("\n"));
    writeCfg({ ...cfg, commit: head, version: "0.8.7" });
    assert.deepEqual(verifyUpstream(dir), []);
    // A modified work tree fails closed.
    fs.appendFileSync(path.join(sub, "Cargo.toml"), "# edit\n");
    assert.ok(verifyUpstream(dir).some((e) => e.includes("modified")));
    git(sub, "checkout", "--", "Cargo.toml");
    fs.writeFileSync(path.join(sub, "stray.txt"), "x");
    assert.ok(verifyUpstream(dir).some((e) => e.includes("modified")));
    fs.rmSync(path.join(sub, "stray.txt"));
    // HEAD drifting away from the pin fails too.
    writeCfg({ ...cfg, commit: OTHER, version: "0.8.7" });
    git(dir, "update-index", "--add", "--cacheinfo", `160000,${OTHER},third_party/rhwp`);
    assert.ok(verifyUpstream(dir).some((e) => e.includes("HEAD")));
  } finally {
    cleanup(dir);
  }
});

test("the sparse plan covers the pinned list and excludes AI-tool files", () => {
  const cfg = JSON.parse(fs.readFileSync(path.join(REPO, "config/rhwp-upstream.json"), "utf8"));
  const plan = sparseCheckoutPlan(cfg);
  assert.ok(plan.sparse.includes("/rhwp-studio/"));
  assert.ok(plan.sparse.every((p) => p.startsWith("/")));
  assert.ok(!plan.sparse.some((p) => /mydocs|samples|^\/pdf\/|^\/tools\/?$|CLAUDE|AGENTS|\.mcp/i.test(p)));
  assert.ok(plan.submoduleArgs.includes("--depth"));
});

test("hygiene still fails when a forbidden file is tracked at the repository root", () => {
  const dir = makeRepo();
  const name = Buffer.from("Q0xBVURFLm1k", "base64").toString("utf8");
  fs.writeFileSync(path.join(dir, name), "notes\n");
  git(dir, "add", name);
  try {
    const res = spawnSync(process.execPath, [path.join(REPO, "scripts/check-repo-hygiene.mjs")], {
      cwd: dir,
      encoding: "utf8",
    });
    assert.notEqual(res.status, 0);
    assert.match(res.stderr, new RegExp(`Forbidden local-only file is tracked by Git: ${name}`));
  } finally {
    cleanup(dir);
  }
});

test("hygiene reports a tampered gitlink through the verifier", () => {
  const dir = makeRepo({ gitlink: OTHER });
  try {
    const res = spawnSync(process.execPath, [path.join(REPO, "scripts/check-repo-hygiene.mjs")], {
      cwd: dir,
      encoding: "utf8",
    });
    assert.notEqual(res.status, 0);
    assert.match(res.stderr, /rhwp upstream: .*gitlink/);
  } finally {
    cleanup(dir);
  }
});

test("hygiene scans only skip third_party/rhwp and still inspect other files", () => {
  const dir = makeRepo();
  const banned = Buffer.from("c3ViYWdlbnQ=", "base64").toString("utf8");
  fs.mkdirSync(path.join(dir, "third_party/rhwp"), { recursive: true });
  fs.writeFileSync(path.join(dir, "third_party/rhwp/note.md"), `${banned}\n`);
  fs.mkdirSync(path.join(dir, "third_party/other"), { recursive: true });
  fs.writeFileSync(path.join(dir, "third_party/other/note.md"), `${banned}\n`);
  try {
    const res = spawnSync(process.execPath, [path.join(REPO, "scripts/check-repo-hygiene.mjs")], {
      cwd: dir,
      encoding: "utf8",
    });
    assert.match(res.stderr, /third_party\/other\/note\.md/);
    assert.doesNotMatch(res.stderr, /third_party\/rhwp\/note\.md/);
  } finally {
    cleanup(dir);
  }
});

test("the vendored WASM package matches its provenance", () => {
  const cfg = JSON.parse(fs.readFileSync(path.join(REPO, "config/rhwp-upstream.json"), "utf8"));
  assert.deepEqual(checkVendorCore(REPO, cfg), []);
  const prov = JSON.parse(fs.readFileSync(path.join(REPO, "vendor/rhwp-core/PROVENANCE.json"), "utf8"));
  assert.equal(prov.rustToolchain, "1.93.1");
  assert.equal(prov.wasmPackVersion, cfg.wasmPackVersion);
  for (const name of REQUIRED_VENDOR_FILES) assert.ok(prov.artifacts[name].bytes > 0, name);
});

test("tampered, truncated, missing and unlisted vendor files fail closed", () => {
  const dir = makeRepo();
  const cfg = JSON.parse(fs.readFileSync(path.join(dir, "config/rhwp-upstream.json"), "utf8"));
  const vendor = path.join(dir, "vendor/rhwp-core");
  try {
    fs.appendFileSync(path.join(vendor, "rhwp.js"), "\n// tampered\n");
    assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes("rhwp.js")));
    fs.copyFileSync(path.join(REPO, "vendor/rhwp-core/rhwp.js"), path.join(vendor, "rhwp.js"));
    assert.deepEqual(checkVendorCore(dir, cfg), []);
    const wasm = path.join(vendor, "rhwp_bg.wasm");
    const bytes = fs.readFileSync(wasm);
    bytes[bytes.length >> 1] ^= 0xff;
    fs.writeFileSync(wasm, bytes);
    assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes("SHA-256")));
    fs.writeFileSync(wasm, bytes.subarray(0, -1));
    assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes("bytes")));
    fs.rmSync(wasm);
    assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes("missing")));
    fs.copyFileSync(path.join(REPO, "vendor/rhwp-core/rhwp_bg.wasm"), wasm);
    fs.writeFileSync(path.join(vendor, "extra.js"), "x");
    assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes("extra.js")));
    fs.rmSync(path.join(vendor, "extra.js"));
    fs.rmSync(path.join(vendor, "PROVENANCE.json"));
    assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes("PROVENANCE.json is missing")));
  } finally {
    cleanup(dir);
  }
});

test("provenance must agree with config/rhwp-upstream.json", () => {
  const dir = makeRepo();
  const cfg = JSON.parse(fs.readFileSync(path.join(dir, "config/rhwp-upstream.json"), "utf8"));
  const file = path.join(dir, "vendor/rhwp-core/PROVENANCE.json");
  const prov = JSON.parse(fs.readFileSync(file, "utf8"));
  try {
    for (const key of ["commit", "version", "rustToolchain", "wasmPackVersion"]) {
      fs.writeFileSync(file, JSON.stringify({ ...prov, [key]: "9.9.9" }));
      assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes(key)), key);
    }
    fs.writeFileSync(file, JSON.stringify({ ...prov, artifacts: { ...prov.artifacts, "../evil.js": prov.artifacts["rhwp.js"] } }));
    assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes("unsafe artifact name")));
    fs.writeFileSync(file, "{ not json");
    assert.ok(checkVendorCore(dir, cfg).some((e) => e.includes("cannot be parsed")));
  } finally {
    cleanup(dir);
  }
});

test("verifyUpstream reports a tampered vendor file", () => {
  const dir = makeRepo();
  try {
    fs.appendFileSync(path.join(dir, "vendor/rhwp-core/rhwp.d.ts"), "// x\n");
    assert.ok(verifyUpstream(dir).some((e) => e.includes("rhwp.d.ts")));
  } finally {
    cleanup(dir);
  }
});