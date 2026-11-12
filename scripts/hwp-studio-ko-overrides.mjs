// Key-level overrides of the upstream ko locale catalog. The vite build rewrites the value of each
// listed key in locales/ko.ts, so the locale file itself is never forked. A key that is missing or
// appears twice fails the build.
import fs from "node:fs";
import path from "node:path";

export const KO_OVERRIDES_PATH = "config/rhwp-studio-ko-overrides.json";

export function readKoOverrides(repo) {
  return JSON.parse(fs.readFileSync(path.join(repo, KO_OVERRIDES_PATH), "utf8"));
}

export function applyKoOverrides(code, overrides) {
  let out = code;
  for (const [key, value] of Object.entries(overrides)) {
    const line = new RegExp(`^(\\s*${JSON.stringify(key)}:\\s*)"(?:[^"\\\\]|\\\\.)*"`, "gm");
    const count = (out.match(line) ?? []).length;
    if (count !== 1) throw new Error(`ko override key ${key} matched ${count} times in the upstream catalog`);
    out = out.replace(line, (_all, head) => `${head}${JSON.stringify(value)}`);
  }
  return out;
}
