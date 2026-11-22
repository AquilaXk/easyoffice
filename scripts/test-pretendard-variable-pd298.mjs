// PD-298: Pretendard ships as one variable woff2 and the editor's per-weight catalog names map onto it.
import test from "node:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { readText } from "./test-support/read-text.mjs";
import { PRETENDARD_STATIC_WEIGHTS, PRETENDARD_VARIABLE_FILE, variableFaceFor } from "../apps/hwp-studio/src/core/pretendard-variable.ts";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const FONTS = path.join(REPO, "assets/fonts");
const SUPPLY = path.join(REPO, "third_party/rhwp/rhwp-studio/src/core/generated/font-rule-projections/webfont-supply.ts");
const VARIABLE_NAME = path.posix.basename(PRETENDARD_VARIABLE_FILE);

test("the nine weight names of the upstream catalog map to weights 100 to 900 on the one variable file", () => {
  assert.deepEqual(
    [...PRETENDARD_STATIC_WEIGHTS.entries()].map(([file, weight]) => [path.posix.basename(file, ".woff2"), weight]),
    [
      ["Pretendard-Thin", 100], ["Pretendard-ExtraLight", 200], ["Pretendard-Light", 300], ["Pretendard-Regular", 400], ["Pretendard-Medium", 500],
      ["Pretendard-SemiBold", 600], ["Pretendard-Bold", 700], ["Pretendard-ExtraBold", 800], ["Pretendard-Black", 900],
    ],
  );
  assert.deepEqual(variableFaceFor("fonts/Pretendard-SemiBold.woff2"), { file: PRETENDARD_VARIABLE_FILE, weight: "600" });
  assert.equal(variableFaceFor("fonts/NanumGothic-Bold.woff2"), null);
  assert.equal(variableFaceFor("fonts/Pretendard-Bold.woff2.evil"), null);
});

test("every Pretendard file the upstream catalog can request is mapped", { skip: !fs.existsSync(SUPPLY) && "rhwp checkout absent" }, () => {
  const requested = new Set(readText(SUPPLY).match(/fonts\/Pretendard-[A-Za-z]+\.woff2/g));
  assert.ok(requested.size >= 9, `expected the catalog to name the nine weights, found ${requested.size}`);
  for (const file of requested) assert.ok(variableFaceFor(file), `${file} has no variable mapping`);
});

test("assets/fonts holds the variable file and no static Pretendard file", () => {
  const files = fs.readdirSync(FONTS).filter((name) => /^Pretendard/.test(name) && name.endsWith(".woff2"));
  assert.deepEqual(files, [VARIABLE_NAME]);
});

test("fonts.json pins the variable file to the official release unchanged", () => {
  const manifest = JSON.parse(readText(path.join(FONTS, "fonts.json")));
  const entries = manifest.fonts.filter((font) => font.family === "Pretendard");
  assert.equal(entries.length, 1);
  const [entry] = entries;
  assert.equal(entry.file, VARIABLE_NAME);
  assert.equal(entry.upstream.conversion, "none");
  assert.equal(entry.upstream.sha256, entry.sha256);
  assert.match(entry.upstream.url, /^https:\/\/github\.com\/orioncactus\/pretendard\/releases\/download\/v\d+\.\d+\.\d+\//);
  assert.match(entry.upstream.archiveSha256, /^[0-9a-f]{64}$/);
  const bytes = fs.readFileSync(path.join(FONTS, VARIABLE_NAME));
  assert.equal(bytes.length, entry.bytes);
  assert.equal(crypto.createHash("sha256").update(bytes).digest("hex"), entry.sha256);
});

test("the font loader requests Pretendard through the variable mapping and keeps the weight descriptor", () => {
  const loader = readText(path.join(REPO, "apps/hwp-studio/src/core/font-loader.ts"));
  assert.match(loader, /from\s+'\.\/pretendard-variable'/);
  assert.match(loader, /variableFaceFor\(file\)/);
  assert.match(loader, /variableFaceFor\(entry\.file\)/);
  assert.match(loader, /weight:\s*variable\.weight/);
});
