/**
 * Checks the vendored svg2pdf patch (PD-101) against the copy a new rhwp commit pins.
 *
 * rhwp builds PDFs with a vendored svg2pdf (`vendor/svg2pdf`, wired by `[patch.crates-io]` in its
 * Cargo.toml). Our root Cargo.toml and `vendor/svg2pdf` are copies of that pair, so moving the rhwp
 * pin must prove they still match. Trees are compared by git blob ids (binary safe, no checkout).
 * Node built-in modules only.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { cargoVersion } from "./verify-rhwp-upstream.mjs";

export const SVG2PDF_DIR = "vendor/svg2pdf";
const OID_SHORT = 10;

/** Git blob object id of `data`. */
export function blobOid(data) {
  const head = Buffer.from(`blob ${data.length}\0`);
  return crypto.createHash("sha1").update(head).update(data).digest("hex");
}

/** Normalized `svg2pdf = ...` line of the `[patch.crates-io]` section, or null. */
export function patchEntry(cargoToml) {
  const section = /^\[patch\.crates-io\][^\n]*\n([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(cargoToml ?? "");
  const entry = section ? /^svg2pdf\s*=\s*(.+)$/m.exec(section[1]) : null;
  return entry ? entry[1].trim().replace(/\s+/g, " ") : null;
}

/** Map of relative path -> blob id for every file under `<root>/vendor/svg2pdf`. */
export function localTree(root) {
  const base = path.join(root, SVG2PDF_DIR);
  const tree = new Map();
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const abs = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(abs);
      else if (entry.isFile()) tree.set(path.relative(base, abs).split(path.sep).join("/"), blobOid(fs.readFileSync(abs)));
    }
  };
  if (fs.existsSync(base)) walk(base);
  return tree;
}

/** Differences from the local tree to the upstream tree, each list sorted by path. */
export function diffTrees(local, upstream) {
  const changed = [];
  const added = [];
  const removed = [];
  for (const [rel, oid] of upstream) {
    if (!local.has(rel)) added.push(rel);
    else if (local.get(rel) !== oid) changed.push(rel);
  }
  for (const rel of local.keys()) if (!upstream.has(rel)) removed.push(rel);
  const byPath = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
  return { changed: changed.sort(byPath), added: added.sort(byPath), removed: removed.sort(byPath) };
}

const short = (oid) => oid.slice(0, OID_SHORT);

function describeDiff(diff, local, upstream) {
  return [
    ...diff.changed.map((rel) => `changed ${rel} (${short(local.get(rel))} -> ${short(upstream.get(rel))})`),
    ...diff.added.map((rel) => `added ${rel} (${short(upstream.get(rel))})`),
    ...diff.removed.map((rel) => `removed ${rel} (${short(local.get(rel))})`),
  ];
}

/**
 * Plans the svg2pdf check. `problems` block the update; with `sync` a file drift becomes a
 * `sync` action (writes, removes and backups) instead of a problem. A differing `[patch]` entry
 * is never synced because it lives in our root Cargo.toml.
 */
export function planSvg2pdf({ root, remote, url, commit, sync }) {
  const problems = [];
  const upstreamPatch = patchEntry(remote.readFileAt(url, commit, "Cargo.toml"));
  const localPatch = patchEntry(fs.readFileSync(path.join(root, "Cargo.toml"), "utf8"));
  if (upstreamPatch !== localPatch) {
    problems.push(
      `root Cargo.toml [patch.crates-io] svg2pdf is '${localPatch}' but upstream has '${upstreamPatch}'; update it by hand`,
    );
  }
  const local = localTree(root);
  const upstream = remote.listTreeAt(url, commit, SVG2PDF_DIR);
  const diff = diffTrees(local, upstream);
  const drift = diff.changed.length + diff.added.length + diff.removed.length > 0;
  if (!drift) return { problems, sync: null, drift };

  const localVersion = cargoVersion(fs.readFileSync(path.join(root, SVG2PDF_DIR, "Cargo.toml"), "utf8"));
  const upstreamVersion = cargoVersion(remote.readFileAt(url, commit, `${SVG2PDF_DIR}/Cargo.toml`) ?? "");
  const lines = describeDiff(diff, local, upstream);
  if (!sync) {
    problems.push(
      `${SVG2PDF_DIR} differs from the copy rhwp pins (version ${localVersion} -> ${upstreamVersion}): ${lines.join("; ")}. `
        + "Run again with --sync-svg2pdf to copy the upstream files",
    );
    return { problems, sync: null, drift };
  }
  const writes = [...diff.changed, ...diff.added].map((rel) => ({ rel, data: remote.readBlobAt(url, commit, `${SVG2PDF_DIR}/${rel}`) }));
  const backups = new Map([...diff.changed, ...diff.removed].map((rel) => [rel, fs.readFileSync(path.join(root, SVG2PDF_DIR, rel))]));
  for (const rel of diff.added) backups.set(rel, null);
  return { problems, sync: { writes, removes: diff.removed, backups, lines, localVersion, upstreamVersion }, drift };
}

/** Writes the sync action into `<root>/vendor/svg2pdf`. */
export function applySvg2pdf(root, sync) {
  const base = path.join(root, SVG2PDF_DIR);
  for (const { rel, data } of sync.writes) {
    fs.mkdirSync(path.dirname(path.join(base, rel)), { recursive: true });
    fs.writeFileSync(path.join(base, rel), data);
  }
  for (const rel of sync.removes) fs.rmSync(path.join(base, rel), { force: true });
}

/** Puts every file the sync action touched back to its previous content (or removes it). */
export function restoreSvg2pdf(root, sync) {
  const base = path.join(root, SVG2PDF_DIR);
  for (const [rel, data] of sync.backups) {
    if (data === null) fs.rmSync(path.join(base, rel), { force: true });
    else {
      fs.mkdirSync(path.dirname(path.join(base, rel)), { recursive: true });
      fs.writeFileSync(path.join(base, rel), data);
    }
  }
}
