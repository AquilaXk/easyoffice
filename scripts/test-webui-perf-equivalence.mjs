import test from "node:test";
import assert from "node:assert/strict";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The WebUI speed-ups keep their results: each case runs the previous algorithm (kept here as the
// reference) and the shipped one on the same inputs.

const UI = new URL("../crates/easyoffice-app/ui/", import.meta.url);

// ---- sheet workbook fingerprint (file open's "was this edited" check) -------------------------

function referenceFingerprint(snap) {
  const compareCodeUnits = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
  const derived = new Set(["docNum", "docAvg", "docCount"]);
  const canonicalizeObject = (value, key) => {
    const out = {};
    for (const k of Object.keys(value).sort(compareCodeUnits)) {
      if (key === "data" && derived.has(k)) continue;
      const c = canonicalizeContent(value[k], k);
      if (c !== undefined) out[k] = c;
    }
    return Object.keys(out).length > 0 ? out : undefined;
  };
  const canonicalizeContent = (value, key) => {
    if (Array.isArray(value)) return value.length > 0 ? value.map((item) => canonicalizeContent(item, "") ?? null) : undefined;
    if (value && typeof value === "object") return canonicalizeObject(value, key);
    return value === "" || value === null || value === false ? undefined : value;
  };
  return JSON.stringify(canonicalizeContent([snap.sheets, snap.sheetRowCounts, snap.sheetColumnWidths, snap.sheetRules, snap.sheetFrozen], ""));
}

function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

const STYLES = [undefined, undefined, {}, { bold: false }, { fontWeight: "700" }, { fontStyle: "italic", fontSize: "15px" }, { sizePt: 12 }];
const VALUES = ["", "", "7", "1.5", "가나", "-3"];
const FORMULAS = ["", "", "", "=A1+1", "=SUM(A1:A3)"];

function randomEntry(next) {
  const entry = { value: VALUES[Math.floor(next() * VALUES.length)], formula: FORMULAS[Math.floor(next() * FORMULAS.length)] };
  const style = STYLES[Math.floor(next() * STYLES.length)];
  if (style) entry.style = structuredClone(style);
  return entry;
}

function randomStore(next) {
  const sheets = {};
  for (const name of ["Sheet1", "Lists"]) {
    const map = {};
    const cells = 5 + Math.floor(next() * 30);
    for (let i = 0; i < cells; i += 1) map[`${"ABCDE"[Math.floor(next() * 5)]}${1 + Math.floor(next() * 12)}`] = randomEntry(next);
    sheets[name] = map;
  }
  return sheets;
}

function reordered(sheets, next) {
  const out = {};
  for (const name of Object.keys(sheets).reverse()) {
    const refs = Object.keys(sheets[name]).sort(() => next() - 0.5);
    out[name] = Object.fromEntries(refs.map((ref) => [ref, structuredClone(sheets[name][ref])]));
  }
  return out;
}

function perturbed(sheets, next) {
  const out = structuredClone(sheets);
  const map = out[Object.keys(out)[Math.floor(next() * 2)]];
  const refs = Object.keys(map);
  const ref = refs[Math.floor(next() * refs.length)];
  switch (Math.floor(next() * 7)) {
    case 0: map[ref] = { ...map[ref], value: map[ref].value + "1" }; break;
    case 1: map[ref] = { ...map[ref], formula: map[ref].formula === "" ? "=B2" : "" }; break;
    case 2: map[ref] = { ...map[ref], style: map[ref].style ? undefined : { fontWeight: "700" } }; break;
    case 3: delete map[ref]; break;
    case 4: map[`Z${1 + Math.floor(next() * 5)}`] = randomEntry(next); break;
    case 5: map[`Y${1 + Math.floor(next() * 5)}`] = { value: "", formula: "" }; break;
    default: map[ref] = { value: map[ref].value, formula: map[ref].formula }; break;
  }
  return out;
}

test("the sheet fingerprint tells edited stores from untouched ones exactly as the canonical-copy version did", () => {
  const env = {};
  loadUiModule(new URL("workspace-session.js", UI), env);
  const fingerprint = (sheets, extra = {}) => env.__EASYOFFICE_WORKSPACE_SESSION_UTILS__.moduleFingerprint({ sheets, sheetRowCounts: { Sheet1: 12 }, sheetColumnWidths: {}, sheetRules: {}, sheetFrozen: {}, ...extra }, "SHEET");
  const reference = (sheets, extra = {}) => referenceFingerprint({ sheets, sheetRowCounts: { Sheet1: 12 }, sheetColumnWidths: {}, sheetRules: {}, sheetFrozen: {}, ...extra });
  const next = rng(20261112);
  let differing = 0;
  for (let round = 0; round < 400; round += 1) {
    const a = randomStore(next);
    const b = round % 2 === 0 ? reordered(a, next) : perturbed(a, next);
    const sameBefore = reference(a) === reference(b);
    assert.equal(fingerprint(a) === fingerprint(b), sameBefore, `round ${round}: ${JSON.stringify(a)} vs ${JSON.stringify(b)}`);
    if (!sameBefore) differing += 1;
  }
  assert.ok(differing > 60, "the perturbations changed content often enough to test both outcomes: " + differing);
  const same = randomStore(next);
  assert.equal(fingerprint(same, { sheetRowCounts: { Sheet1: 13 } }) === fingerprint(same), reference(same, { sheetRowCounts: { Sheet1: 13 } }) === reference(same), "row counts still count");
  assert.notEqual(fingerprint(same, { sheetRowCounts: { Sheet1: 13 } }), fingerprint(same));
});

test("contentFingerprint keeps exact equivalence with canonical serialization and skips JSON stringifying sheets", () => {
  const env = {};
  loadUiModule(new URL("workspace-session.js", UI), env);
  const { contentFingerprint } = env.__EASYOFFICE_WORKSPACE_SESSION_UTILS__;
  const baseSnap = (sheets, extra = {}) => ({
    docTabs: { DOC: { p: "본문" }, HWP: null },
    sheets,
    sheetRowCounts: { Sheet1: 12 },
    sheetColumnWidths: {},
    sheetRules: {},
    sheetLayout: {},
    definedNames: {},
    sheetFrozen: {},
    sheetOrder: ["Sheet1", "Lists"],
    slides: [{ title: "제목" }],
    pdfStudio: null,
    pdfRotationDeg: 0,
    ...extra,
  });
  const next = rng(20261114);
  let differing = 0;
  for (let round = 0; round < 400; round += 1) {
    const a = randomStore(next);
    const b = round % 2 === 0 ? reordered(a, next) : perturbed(a, next);
    const sameBefore = referenceFingerprint(baseSnap(a)) === referenceFingerprint(baseSnap(b));
    assert.equal(
      contentFingerprint(baseSnap(a), null) === contentFingerprint(baseSnap(b), null),
      sameBefore,
      `round ${round}`,
    );
    if (!sameBefore) differing += 1;
  }
  assert.ok(differing > 60);
  const emptyA = baseSnap({ Sheet1: { A1: { value: "", formula: "" } } });
  const emptyB = baseSnap({});
  assert.equal(contentFingerprint(emptyA, null), contentFingerprint(emptyB, null));
  assert.notEqual(contentFingerprint(emptyB, { lock: true }), contentFingerprint(emptyB, null));
  assert.notEqual(contentFingerprint(baseSnap({}, { docTabs: { DOC: { p: "수정" } } }), null), contentFingerprint(emptyB, null));
});

// ---- formatted-number normalization ------------------------------------------------------------

function referenceNormalize(raw) {
  const GROUPED = /^[+-]?\d{1,3}(,\d{3})+(\.\d+)?$/;
  const CURRENCY = /^([+-]?)\s*[₩$€£¥]\s*([+-]?\d{1,3}(?:,\d{3})*(?:\.\d+)?|[+-]?\d+(?:\.\d+)?)$/;
  const PERCENT = /^([+-]?\d{1,3}(?:,\d{3})*(?:\.\d+)?|[+-]?\d+(?:\.\d+)?)\s*%$/;
  const trimmed = String(raw || "").trim();
  if (GROUPED.test(trimmed)) return trimmed.replaceAll(",", "");
  const cur = CURRENCY.exec(trimmed);
  if (cur) return (cur[1] === "-" || cur[2].startsWith("-") ? "-" : "") + cur[2].replace(/^[+-]/, "").replaceAll(",", "");
  const pct = PERCENT.exec(trimmed);
  if (pct) return pct[1].replaceAll(",", "");
  return trimmed;
}

test("number normalization gives the same text as the three-pattern version on generated and fixed inputs", () => {
  const env = {};
  loadUiModule(new URL("sheet-utils.js", UI), env);
  const normalize = env.__EASYOFFICE_SHEET_UTILS__.normalizeFormattedNumericCell;
  const fixed = ["", "  ", "12", "-3.5", "1,234", "1,234,567.89", "₩1,000", "$ -2.50", "-€3", "45%", "1,000 %", "12abc", "1,23", ",", "%", "₩", "가나", "1e5", " 7 ", null, undefined, 0, 12];
  const pieces = ["0", "1", "9", "12", "345", ",", ".", "-", "+", "%", "$", "₩", "€", "£", "¥", " ", "a", "가"];
  const next = rng(77);
  const generated = Array.from({ length: 5000 }, () => Array.from({ length: 1 + Math.floor(next() * 8) }, () => pieces[Math.floor(next() * pieces.length)]).join(""));
  for (const input of [...fixed, ...generated]) assert.equal(normalize(input), referenceNormalize(input), JSON.stringify(input));
});
