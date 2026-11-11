/**
 * Authoring font policy for the Hangul editor overlay.
 *
 * Adapted from the reference app `apps/studio-host/src/core/font-authoring-policy.ts`
 * (MIT, Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md):
 * restricted commercial families (`휴먼*`, `HCI*`, `HY*`) are excluded from authoring dropdowns
 * and mapped to `'함초롬바탕'` (serif) or `'함초롬돋움'` (sans-serif) when applied in the toolbar.
 *
 * Low-level bit/word optimization:
 *   - 4x32-bit static ASCII bitmap skips whitespace, quotes (`"`, `'`), `-` and `_` in O(1).
 *   - ASCII letters fold to lower case via bitwise OR (`code | 0x20`) with zero locale or string allocation.
 *   - Prefix matching exits on the first kept character when it is neither `'휴'` (`0xd734`) nor `'h'` (`0x68`),
 *     and otherwise packs up to 8 post-prefix ASCII bytes into two 32-bit words (`hyHi`, `hyLo`).
 *   - Serif detection scans UTF-16 code units with a 32-bit packed Hangul bigram window (`명조`, `바탕`, `궁서`)
 *     and a two-word 24-bit rolling ASCII window (`serif`, `myeong`).
 */

const SANS_FALLBACK = '함초롬돋움';
const SERIF_FALLBACK = '함초롬바탕';

/** Bit 0 of `classifyAuthoringFontBits`: family is blocked for authoring. */
export const AUTHORING_FONT_BLOCKED_BIT = 1 << 0;
/** Bit 1 of `classifyAuthoringFontBits`: family belongs to the serif fallback class. */
export const AUTHORING_FONT_SERIF_BIT = 1 << 1;

// 128-bit ASCII skip table packed into four 32-bit words:
//   word 0 (0..31):   \t (9), \n (10), \v (11), \f (12), \r (13) -> 0x00003e00
//   word 1 (32..63):  ' ' (32), '"' (34), "'" (39), '-' (45)     -> 0x00002085
//   word 2 (64..95):  '_' (95)                                   -> 0x80000000
//   word 3 (96..127): none                                       -> 0x00000000
const SKIP_ASCII_WORDS = new Uint32Array([0x00003e00, 0x00002085, 0x80000000, 0x00000000]);

// Packed 32-bit UTF-16 bigrams ((first << 16) | second).
const BIGRAM_HYUMEON = 0xd734ba3c; // '휴먼'
const BIGRAM_MYEONGJO = 0xba85c870; // '명조'
const BIGRAM_BATANG = 0xbc14d0d5; // '바탕'
const BIGRAM_GUNGSEO = 0xad81c11c; // '궁서'

// Packed ASCII words after "hy" (indices 2..5 in hyHi, 6..9 in hyLo).
const HY2_MJ = 0x6d6a;
const HY2_GT = 0x6774;
const HY2_GP = 0x6770;
const HY2_SN = 0x736e;
const HY2_SM = 0x736d;
const HY_GOTH_HI = 0x676f7468; // "goth"
const HY_GOTH_LO16 = 0x6963; // "ic"
const HY_MYEO_HI = 0x6d79656f; // "myeo"
const HY_MYEO_LO16 = 0x6e67; // "ng"
const HY_GRAP_HI = 0x67726170; // "grap"
const HY_GRAP_LO24 = 0x686963; // "hic"
const HY_HEAD_HI = 0x68656164; // "head"
const HY_HEAD_LO = 0x6c696e65; // "line"

// Rolling ASCII windows (24-bit high + 24-bit low).
const WIN_SERIF_HI16 = 0x7365; // "se"
const WIN_SERIF_LO24 = 0x726966; // "rif"
const WIN_MYEONG_HI24 = 0x6d7965; // "mye"
const WIN_MYEONG_LO24 = 0x6f6e67; // "ong"

function isSkippedCodeUnit(code: number): boolean {
  if (code < 128) {
    return ((SKIP_ASCII_WORDS[code >>> 5] >>> (code & 31)) & 1) !== 0;
  }
  return (
    code === 0x00a0
    || (code >= 0x2000 && code <= 0x200a)
    || code === 0x2028
    || code === 0x2029
    || code === 0x202f
    || code === 0x205f
    || code === 0x3000
    || code === 0xfeff
    || code === 0x1680
  );
}

function foldAsciiLower(code: number): number {
  return code >= 0x41 && code <= 0x5a ? code | 0x20 : code;
}

function matchHyAsciiSuffix(count: number, hyAsciiMask: number, hyHi: number, hyLo: number): boolean {
  if (count >= 4 && (hyAsciiMask & 0x03) === 0x03) {
    const hy2 = hyHi >>> 16;
    if (hy2 === HY2_MJ || hy2 === HY2_GT || hy2 === HY2_GP || hy2 === HY2_SN || hy2 === HY2_SM) {
      return true;
    }
  }
  if (count >= 8 && (hyAsciiMask & 0x3f) === 0x3f) {
    const lo16 = hyLo >>> 16;
    if ((hyHi === HY_GOTH_HI && lo16 === HY_GOTH_LO16) || (hyHi === HY_MYEO_HI && lo16 === HY_MYEO_LO16)) {
      return true;
    }
  }
  if (count >= 9 && (hyAsciiMask & 0x7f) === 0x7f && hyHi === HY_GRAP_HI && (hyLo >>> 8) === HY_GRAP_LO24) {
    return true;
  }
  return count >= 10 && (hyAsciiMask & 0xff) === 0xff && hyHi === HY_HEAD_HI && hyLo === HY_HEAD_LO;
}

/** Fast O(1)-bounded prefix check (reads at most 10 normalized characters; exits on char 0 for non-'휴'/'h'). */
export function isAuthoringBlockedFontFamily(family: string | null | undefined): boolean {
  if (!family) return false;
  const len = family.length;
  let count = 0;
  let c0 = 0;
  let c1 = 0;
  let hyHi = 0;
  let hyLo = 0;
  let hyAsciiMask = 0;

  for (let i = 0; i < len; i++) {
    const raw = family.charCodeAt(i);
    if (isSkippedCodeUnit(raw)) continue;
    const c = foldAsciiLower(raw);

    if (count === 0) {
      if (c !== 0xd734 && c !== 0x68) return false;
      c0 = c;
      count = 1;
      continue;
    }
    if (count === 1) {
      c1 = c;
      count = 2;
      if (c0 === 0xd734) return (((c0 << 16) | c1) >>> 0) === BIGRAM_HYUMEON;
      if (c1 !== 0x63 && c1 !== 0x79) return false;
      continue;
    }
    if (count === 2) {
      if (c1 === 0x63) return c === 0x69;
      // c0 === 'h', c1 === 'y'
      if (((c - 0xac00) >>> 0) <= 0x2ba3) return true;
    }

    const rel = count - 2;
    if (c < 0x80) {
      hyAsciiMask |= 1 << rel;
      if (rel < 4) {
        hyHi = (hyHi | ((c & 0xff) << ((3 - rel) << 3))) >>> 0;
      } else {
        hyLo = (hyLo | ((c & 0xff) << ((7 - rel) << 3))) >>> 0;
      }
    } else {
      break;
    }
    count += 1;
    if (count === 10) break;
  }

  return c0 === 0x68 && c1 === 0x79 && matchHyAsciiSuffix(count, hyAsciiMask, hyHi, hyLo);
}

function hasSerifKeyword(family: string): boolean {
  const len = family.length;
  let prevU16 = 0;
  let hasPrev = false;
  let asciiWinHi = 0;
  let asciiWinLo = 0;
  let asciiRun = 0;

  for (let i = 0; i < len; i++) {
    const raw = family.charCodeAt(i);
    if (isSkippedCodeUnit(raw)) continue;
    const c = foldAsciiLower(raw);

    if (hasPrev) {
      const pair32 = ((prevU16 << 16) | c) >>> 0;
      if (pair32 === BIGRAM_MYEONGJO || pair32 === BIGRAM_BATANG || pair32 === BIGRAM_GUNGSEO) {
        return true;
      }
    }
    prevU16 = c;
    hasPrev = true;

    if (c < 0x80) {
      asciiWinHi = ((asciiWinHi << 8) | (asciiWinLo >>> 16)) & 0xffffff;
      asciiWinLo = ((asciiWinLo << 8) | c) & 0xffffff;
      asciiRun += 1;
      if (asciiRun >= 5 && (asciiWinHi & 0xffff) === WIN_SERIF_HI16 && asciiWinLo === WIN_SERIF_LO24) {
        return true;
      }
      if (asciiRun >= 6 && asciiWinHi === WIN_MYEONG_HI24 && asciiWinLo === WIN_MYEONG_LO24) {
        return true;
      }
    } else {
      asciiWinHi = 0;
      asciiWinLo = 0;
      asciiRun = 0;
    }
  }
  return false;
}

/** Returns bitmask of `AUTHORING_FONT_BLOCKED_BIT | AUTHORING_FONT_SERIF_BIT`. */
export function classifyAuthoringFontBits(family: string | null | undefined): number {
  if (!family) return 0;
  const blocked = isAuthoringBlockedFontFamily(family) ? AUTHORING_FONT_BLOCKED_BIT : 0;
  const serif = hasSerifKeyword(family) ? AUTHORING_FONT_SERIF_BIT : 0;
  return blocked | serif;
}

export function sanitizeAuthoringFontFamily(family: string): string {
  const trimmed = family.trim();
  if (!trimmed) return trimmed;
  if (!isAuthoringBlockedFontFamily(trimmed)) return trimmed;
  return hasSerifKeyword(trimmed) ? SERIF_FALLBACK : SANS_FALLBACK;
}

export function filterAuthoringFontFamilies(families: Iterable<string>): string[] {
  const out: string[] = [];
  for (const family of families) {
    const trimmed = family.trim();
    if (trimmed && !isAuthoringBlockedFontFamily(trimmed)) {
      out.push(trimmed);
    }
  }
  return out;
}
