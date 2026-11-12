import test from "node:test";
import assert from "node:assert/strict";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// Shared Base64 byte conversion (`sheet-utils.js`): the webview's native `Uint8Array.fromBase64` /
// `toBase64` when present, a chunked / unrolled path otherwise. Round trips on both paths, and the
// timings of 1 MiB and 10 MiB conversions are printed for the record.

const SHEET_UTILS_JS = new URL("../crates/easyoffice-app/ui/sheet-utils.js", import.meta.url);
const MIB = 1048576;

const nodeAtob = (s) => Buffer.from(s, "base64").toString("latin1");
const nodeBtoa = (s) => Buffer.from(s, "latin1").toString("base64");

function loadUtils(env) {
  loadUiModule(SHEET_UTILS_JS, env);
  return env.__EASYOFFICE_SHEET_UTILS__;
}

function sample(size) {
  const bytes = new Uint8Array(size);
  for (let i = 0; i < size; i += 1) bytes[i] = (i * 31 + (i >> 8)) & 0xff;
  return bytes;
}

function ms(fn) {
  const start = performance.now();
  const out = fn();
  return [out, performance.now() - start];
}

test("fallback path (explicit atob/btoa) round trips odd sizes", () => {
  const utils = loadUtils({ atob: nodeAtob, btoa: nodeBtoa });
  for (const size of [0, 1, 2, 3, 4, 5, 0x8000, 0x8001, 100003]) {
    const bytes = sample(size);
    const text = utils.bytesToBase64(bytes, nodeBtoa);
    assert.equal(text, Buffer.from(bytes).toString("base64"));
    assert.deepEqual(utils.base64ToBytes(text, nodeAtob), bytes);
  }
});

test("native path matches the fallback when the runtime has fromBase64/toBase64", { skip: typeof Uint8Array.fromBase64 !== "function" }, () => {
  const utils = loadUtils({ atob: globalThis.atob, btoa: globalThis.btoa });
  const bytes = sample(100003);
  const text = utils.bytesToBase64(bytes);
  assert.equal(text, Buffer.from(bytes).toString("base64"));
  assert.deepEqual(utils.base64ToBytes(text), bytes);
});

test("timings: old per-character loop vs shared converter", () => {
  const utils = loadUtils({ atob: globalThis.atob, btoa: globalThis.btoa });
  const rows = [];
  for (const size of [MIB, 10 * MIB]) {
    const bytes = sample(size);
    const text = Buffer.from(bytes).toString("base64");
    const [, oldDecode] = ms(() => {
      const raw = globalThis.atob(text);
      const out = new Uint8Array(raw.length);
      for (let i = 0; i < raw.length; i += 1) out[i] = raw.codePointAt(i);
      return out;
    });
    const [, fallbackDecode] = ms(() => utils.base64ToBytes(text, nodeAtob));
    const [decoded, sharedDecode] = ms(() => utils.base64ToBytes(text));
    const [, fallbackEncode] = ms(() => utils.bytesToBase64(bytes, globalThis.btoa));
    const [encoded, sharedEncode] = ms(() => utils.bytesToBase64(bytes));
    assert.deepEqual(decoded, bytes);
    assert.equal(encoded, text);
    rows.push({ size: `${size / MIB} MiB`, oldDecode, fallbackDecode, sharedDecode, fallbackEncode, sharedEncode });
  }
  console.table(rows.map((r) => Object.fromEntries(Object.entries(r).map(([k, v]) => [k, typeof v === "number" ? `${v.toFixed(1)} ms` : v]))));
});
