//! Byte widths of characters and the full-width/half-width conversions behind the byte text
//! functions (`LENB`, `LEFTB`, `MIDB`, `RIGHTB`, `FINDB`, `SEARCHB`, `REPLACEB`, `ASC`, `DBCS`; PD-205).
//!
//! Fixed rule (QA decision, a Korean environment): Hangul, Han, kana and the full-width forms are
//! 2 bytes, every other character is 1 byte, whatever the system language. The ranges are
//! [`WIDE_RANGES`] and [`SUPPLEMENTARY_RANGES`]. The reference implementation counts every
//! UTF-16 unit above 255 as 2 bytes (`packages/engine-formula/src/engine/utils/char-kit.ts`) and
//! converts only the ASCII block in `ASC`/`DBCS` (`src/functions/text/{asc,dbcs}/index.ts`), commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.
//! (see `THIRD_PARTY_NOTICES.md`); the table, the kana and symbol pairs, and the split rule below
//! are written for this engine.

/// Inclusive code point ranges below U+10000 that count 2 bytes.
const WIDE_RANGES: [(u32, u32); 15] = [
    (0x1100, 0x11FF), // Hangul Jamo
    (0x2E80, 0x2FDF), // CJK radicals, Kangxi radicals
    (0x3000, 0x303E), // CJK symbols and punctuation
    (0x3040, 0x30FF), // Hiragana, Katakana
    (0x3130, 0x318F), // Hangul compatibility Jamo
    (0x31F0, 0x31FF), // Katakana phonetic extensions
    (0x3200, 0x33FF), // enclosed CJK letters, CJK compatibility
    (0x3400, 0x4DBF), // CJK unified ideographs extension A
    (0x4E00, 0x9FFF), // CJK unified ideographs
    (0xA960, 0xA97F), // Hangul Jamo extended A
    (0xAC00, 0xD7A3), // Hangul syllables
    (0xD7B0, 0xD7FF), // Hangul Jamo extended B
    (0xF900, 0xFAFF), // CJK compatibility ideographs
    (0xFF01, 0xFF60), // full-width forms
    (0xFFE0, 0xFFE6), // full-width symbols
];

/// Inclusive ranges above U+FFFF that count 2 bytes (ideograph extensions B to G).
const SUPPLEMENTARY_RANGES: [(u32, u32); 2] = [(0x2_0000, 0x2_FA1F), (0x3_0000, 0x3_134F)];

/// One bit per code point below U+10000, set for the 2-byte ranges: a width lookup is one load.
static BMP_WIDE: [u64; 1024] = bmp_wide_bits();

const fn bmp_wide_bits() -> [u64; 1024] {
    let mut bits = [0u64; 1024];
    let mut r = 0;
    while r < WIDE_RANGES.len() {
        let (mut cp, last) = WIDE_RANGES[r];
        while cp <= last {
            bits[(cp >> 6) as usize] |= 1 << (cp & 63);
            cp += 1;
        }
        r += 1;
    }
    bits
}

/// Bytes of one character: 2 for the wide ranges, else 1.
#[inline]
pub(crate) fn char_bytes(c: char) -> usize {
    let cp = c as u32;
    if cp < 0x1100 {
        1
    } else if cp < 0x1_0000 {
        1 + ((BMP_WIDE[(cp >> 6) as usize] >> (cp & 63)) & 1) as usize
    } else {
        1 + usize::from(
            SUPPLEMENTARY_RANGES
                .iter()
                .any(|&(lo, hi)| (lo..=hi).contains(&cp)),
        )
    }
}

/// Bytes of the whole text (`LENB`).
pub(crate) fn text_bytes(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().map(char_bytes).sum()
    }
}

/// The characters that lie in the byte span `[from, to)` of `s`. A 2-byte character that the span
/// cuts in half becomes one space, so the result is exactly `to - from` bytes wide.
pub(crate) fn slice_bytes(s: &str, from: usize, to: usize) -> String {
    let mut out = String::new();
    let mut at = 0usize;
    for c in s.chars() {
        let (start, end) = (at, at + char_bytes(c));
        at = end;
        if end <= from {
            continue;
        }
        if start >= to {
            break;
        }
        out.push(if start >= from && end <= to { c } else { ' ' });
    }
    out
}

/// Half-width katakana U+FF61 to U+FF9F and the full-width character each one converts to.
const HALF_KANA_FULL: [char; 63] = [
    '\u{3002}', '\u{300C}', '\u{300D}', '\u{3001}', '\u{30FB}', '\u{30F2}', '\u{30A1}', '\u{30A3}',
    '\u{30A5}', '\u{30A7}', '\u{30A9}', '\u{30E3}', '\u{30E5}', '\u{30E7}', '\u{30C3}', '\u{30FC}',
    '\u{30A2}', '\u{30A4}', '\u{30A6}', '\u{30A8}', '\u{30AA}', '\u{30AB}', '\u{30AD}', '\u{30AF}',
    '\u{30B1}', '\u{30B3}', '\u{30B5}', '\u{30B7}', '\u{30B9}', '\u{30BB}', '\u{30BD}', '\u{30BF}',
    '\u{30C1}', '\u{30C4}', '\u{30C6}', '\u{30C8}', '\u{30CA}', '\u{30CB}', '\u{30CC}', '\u{30CD}',
    '\u{30CE}', '\u{30CF}', '\u{30D2}', '\u{30D5}', '\u{30D8}', '\u{30DB}', '\u{30DE}', '\u{30DF}',
    '\u{30E0}', '\u{30E1}', '\u{30E2}', '\u{30E4}', '\u{30E6}', '\u{30E8}', '\u{30E9}', '\u{30EA}',
    '\u{30EB}', '\u{30EC}', '\u{30ED}', '\u{30EF}', '\u{30F3}', '\u{309B}', '\u{309C}',
];

const HALF_KANA_FIRST: u32 = 0xFF61;
const HALF_VOICED_MARK: u32 = 0xFF9E;
const HALF_SEMI_VOICED_MARK: u32 = 0xFF9F;
/// U+FFE0 to U+FFE6 and the half-width characters they convert to.
const FULL_SYMBOLS: [char; 7] = [
    '\u{A2}', '\u{A3}', '\u{AC}', '\u{AF}', '\u{A6}', '\u{A5}', '\u{20A9}',
];
const ASCII_TO_FULL_SHIFT: u32 = 0xFEE0;

/// The base katakana and the mark of a voiced or semi-voiced full-width katakana (`ガ` is `カ` and
/// the voiced mark), as the code point of the base and the number of code points below it.
fn voiced_base(cp: u32) -> Option<(u32, u32)> {
    match cp {
        0x30AC | 0x30AE | 0x30B0 | 0x30B2 | 0x30B4 | 0x30B6 | 0x30B8 | 0x30BA | 0x30BC | 0x30BE
        | 0x30C0 | 0x30C2 | 0x30C5 | 0x30C7 | 0x30C9 | 0x30D0 | 0x30D3 | 0x30D6 | 0x30D9
        | 0x30DC => Some((cp - 1, HALF_VOICED_MARK)),
        0x30D1 | 0x30D4 | 0x30D7 | 0x30DA | 0x30DD => Some((cp - 2, HALF_SEMI_VOICED_MARK)),
        0x30F4 => Some((0x30A6, HALF_VOICED_MARK)),
        _ => None,
    }
}

fn half_of_full_kana(cp: u32) -> Option<char> {
    if !(0x3001..=0x30FC).contains(&cp) && cp != 0x309B && cp != 0x309C {
        return None;
    }
    HALF_KANA_FULL
        .iter()
        .position(|&f| f as u32 == cp)
        .and_then(|i| char::from_u32(HALF_KANA_FIRST + i as u32))
}

/// `ASC`: full-width to half-width. The ideographic space, the full-width ASCII block, the
/// full-width symbols and the katakana (a voiced one becomes two characters).
pub(crate) fn to_half_width(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let cp = c as u32;
        match cp {
            0x3000 => out.push(' '),
            0xFF01..=0xFF5E => out.push(char::from_u32(cp - ASCII_TO_FULL_SHIFT).unwrap_or(c)),
            0xFFE0..=0xFFE6 => out.push(FULL_SYMBOLS[(cp - 0xFFE0) as usize]),
            _ => {
                if let Some((base, mark)) = voiced_base(cp) {
                    out.extend(half_of_full_kana(base));
                    out.extend(char::from_u32(mark));
                } else {
                    out.push(half_of_full_kana(cp).unwrap_or(c));
                }
            }
        }
    }
    out
}

/// A half-width katakana with its voiced or semi-voiced mark as one full-width katakana.
fn compose_kana(base: u32, mark: u32) -> Option<char> {
    let full = HALF_KANA_FULL[(base - HALF_KANA_FIRST) as usize] as u32;
    let composed = match (mark, base) {
        // ウ + voiced mark
        (HALF_VOICED_MARK, 0xFF73) => 0x30F4,
        // カ..ト and ハ..ホ: the voiced form is the next code point (ツ is U+30C4, ヅ is U+30C5)
        (HALF_VOICED_MARK, 0xFF76..=0xFF84 | 0xFF8A..=0xFF8E) => full + 1,
        (HALF_SEMI_VOICED_MARK, 0xFF8A..=0xFF8E) => full + 2,
        _ => return None,
    };
    char::from_u32(composed)
}

/// `DBCS`: half-width to full-width, the inverse of [`to_half_width`]; a half-width katakana
/// followed by a voiced or semi-voiced mark becomes one voiced character.
pub(crate) fn to_full_width(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let cp = c as u32;
        match cp {
            0x20 => out.push('\u{3000}'),
            0x21..=0x7E => out.extend(char::from_u32(cp + ASCII_TO_FULL_SHIFT)),
            0xA2 | 0xA3 | 0xAC | 0xAF | 0xA6 | 0xA5 | 0x20A9 => {
                let at = FULL_SYMBOLS.iter().position(|&h| h == c).unwrap_or(0) as u32;
                out.extend(char::from_u32(0xFFE0 + at));
            }
            HALF_KANA_FIRST..=HALF_SEMI_VOICED_MARK => {
                let composed = chars.peek().and_then(|&next| compose_kana(cp, next as u32));
                if let Some(full) = composed {
                    chars.next();
                    out.push(full);
                } else {
                    out.push(HALF_KANA_FULL[(cp - HALF_KANA_FIRST) as usize]);
                }
            }
            _ => out.push(c),
        }
    }
    out
}
