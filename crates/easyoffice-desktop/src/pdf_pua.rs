//! Readable text for the private-use characters of Hangul documents (PD-131).
//!
//! The Hangul word processor stores many symbols (enclosed numbers, bullets, ruling pieces) as
//! private-use code points that only its own fonts draw. The PDF text map should name the
//! standard character instead, so a search for "①" finds the glyph and a screen reader does not
//! read a private-use code. This table decides only the text of the map; which glyph is drawn is
//! not touched (the fallback faces are chosen by the original code point).
//!
//! Ported from the rhwp PDF renderer, which builds the same text surface for its extracted text:
//! `src/renderer/composer.rs` `text_surface_replacement` / `pua_to_display_text` (lines
//! 4416-4452), the verified-symbol table of the `src/renderer/` module beside `composer.rs` (its
//! file name is the word processor vendor's, so it is not written here; lines 12-84) and the glyph
//! substitution table `src/renderer/layout/paragraph_layout.rs` `map_pua_bullet_char` (lines
//! 11018-11145) (rhwp 0.8.7 at `1a76570`; MIT, Copyright (c) 2025-2026 Edward Kim, see
//! THIRD_PARTY_NOTICES.md).
//! Differences: the three sources are one binary-searched table (a unit test checks the order),
//! the BMP `0xF000..=0xF0FF` range is pre-filtered by a compile-time 256-bit `[u64; 4]` bitmap,
//! the enclosed numbers are computed from their range, and the HWP 5.0 raw form of the symbols
//! (`0xA000 | n`, indistinguishable from real Yi syllables without the parser) is not mapped: the
//! renderer already turns the raw codes that its parser table lists (`src/parser/tags.rs`, the 66
//! values measured against the word processor's own output) into the codes of this table before
//! it writes the SVG (`src/renderer/composer.rs`, the display-text step), and a raw code outside
//! that table has no known meaning to map. The 14 sample documents (153 pages) draw none (PD-131).
//! An unknown private-use code is left as it is; a meaning is never guessed.

/// Enclosed numbers ① to ⑳ (U+2460..U+2473, three UTF-8 bytes each).
const ENCLOSED: &str = "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳";
const ENCLOSED_BYTES: usize = 3;
/// Numbers inside an inverted square, U+F02CE..U+F02E1: the plain number.
const INVERTED_SQUARE_FIRST: u32 = 0xF02CE;
const INVERTED_SQUARE_NUMBERS: [&str; 20] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17",
    "18", "19", "20",
];
/// Numbers inside a square, U+F02B1..U+F02C4: ① to ⑳ (the editor draws a square, the text only
/// needs to say "an enclosed number"). U+F02B0 is a square 0 and U+F02C5 a square 12, which are
/// not in the continuous run.
const SQUARE_FIRST: u32 = 0xF02B1;
const SQUARE_LAST: u32 = 0xF02C4;
const SQUARE_ZERO: u32 = 0xF02B0;
const SQUARE_TWELVE: u32 = 0xF02C5;
const MIN_MAPPED_CODE: u32 = 0xF022;
const MAX_MAPPED_CODE: u32 = 0xF0855;

/// Symbols checked against the word processor's own PDF output, then the symbol-font bullets
/// (U+F020..U+F0FF) and the book-bracket and ruling pieces. Sorted by code point.
#[rustfmt::skip]
const TABLE: &[(u32, &str)] = &[
    (0xF022, "✂"), (0xF036, "⌛"), (0xF045, "☜"), (0xF046, "☞"), (0xF047, "☝"), (0xF048, "☟"),
    (0xF04A, "☺"), (0xF04E, "☠"), (0xF052, "☼"), (0xF054, "❄"), (0xF058, "✠"), (0xF059, "✡"),
    (0xF06C, "●"), (0xF06D, "●"), (0xF06E, "■"), (0xF06F, "□"), (0xF070, "□"), (0xF071, "□"),
    (0xF072, "□"), (0xF073, "⬧"), (0xF074, "⧫"), (0xF075, "◆"), (0xF076, "❖"), (0xF077, "⬥"),
    (0xF09E, "·"), (0xF09F, "•"), (0xF0A0, "·"), (0xF0A1, "⚪"), (0xF0A2, "○"), (0xF0A3, "○"),
    (0xF0A4, "◉"), (0xF0A5, "◎"), (0xF0A7, "▪"), (0xF0A8, "◻"), (0xF0AA, "✦"), (0xF0AB, "★"),
    (0xF0AC, "✶"), (0xF0AD, "✴"), (0xF0AE, "✹"), (0xF0E8, "➔"), (0xF0EF, "⇦"), (0xF0F0, "⇨"),
    (0xF0F1, "⇧"), (0xF0F2, "⇩"), (0xF0FB, "✗"), (0xF0FC, "✔"), (0xF0FD, "☒"), (0xF0FE, "☑"),
    (0xF003B, "↓"), (0xF0090, "✺"), (0xF00DA, "▸"), (0xF012B, "(인)"),
    (0xF0288, "⓪"), (0xF0289, "①"), (0xF028A, "②"), (0xF028C, "④"), (0xF028D, "⑤"),
    (0xF028E, "⑥"), (0xF028F, "⑦"), (0xF0290, "⑧"), (0xF0291, "⑨"),
    (0xF02EC, "◇"), (0xF02EF, "·"), (0xF02FB, "▸"), (0xF02FC, "►"), (0xF031C, "■"),
    (0xF03A0, "↵"), (0xF03A7, "⊟"), (0xF03A8, "⊞"), (0xF03C5, "□"), (0xF03DA, "□"),
    (0xF03EF, "한"), (0xF03F0, "글"), (0xF03F1, "과"), (0xF03F2, "컴"), (0xF03F3, "퓨"),
    (0xF03F4, "터"), (0xF03FF, "□"),
    (0xF0806, "┌"), (0xF0807, "┬"), (0xF0808, "┐"), (0xF080C, "└"), (0xF080E, "┘"),
    (0xF080F, "━"), (0xF0810, "│"), (0xF0811, "┌"), (0xF0817, "└"), (0xF081A, "─"),
    (0xF081C, "┈"), (0xF0827, "═"), (0xF0832, "═"), (0xF0848, "━"), (0xF0854, "《"),
    (0xF0855, "》"),
];

/// Compile-time 256-bit membership bitmap for `0xF000..=0xF0FF` derived from [`TABLE`].
const BMP_F0_BITMAP: [u64; 4] = {
    let mut bits = [0u64; 4];
    let mut i = 0;
    while i < TABLE.len() {
        let code = TABLE[i].0;
        if (code & 0xFFFF_FF00) == 0xF000 {
            let low = (code & 0xFF) as usize;
            bits[low >> 6] |= 1u64 << (low & 63);
        }
        i += 1;
    }
    bits
};

/// The standard text for a private-use character, or `None` when it has none (an ordinary
/// character or a private-use code whose meaning is not known).
#[inline]
pub fn replacement(c: char) -> Option<&'static str> {
    let code = c as u32;
    if !(MIN_MAPPED_CODE..=MAX_MAPPED_CODE).contains(&code) {
        return None;
    }
    if (code & 0xFFFF_FF00) == 0xF000 {
        let low = (code & 0xFF) as usize;
        if (BMP_F0_BITMAP[low >> 6] & (1u64 << (low & 63))) == 0 {
            return None;
        }
    } else if (code & 0xFFFF_F000) != 0xF0000 {
        return None;
    }
    match code {
        SQUARE_ZERO => Some("⓪"),
        SQUARE_FIRST..=SQUARE_LAST => enclosed((code - SQUARE_FIRST) as usize),
        SQUARE_TWELVE => enclosed(11),
        INVERTED_SQUARE_FIRST..=0xF02E1 => INVERTED_SQUARE_NUMBERS
            .get((code - INVERTED_SQUARE_FIRST) as usize)
            .copied(),
        _ => TABLE
            .binary_search_by_key(&code, |(key, _)| *key)
            .ok()
            .map(|at| TABLE[at].1),
    }
}

fn enclosed(index: usize) -> Option<&'static str> {
    ENCLOSED.get(index * ENCLOSED_BYTES..(index + 1) * ENCLOSED_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The binary search needs strictly ascending keys, and no entry may map a code to itself or
    /// to another private-use character (the map would not become more readable).
    #[test]
    fn the_table_is_sorted_and_maps_to_standard_text() {
        for pair in TABLE.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "U+{:X} before U+{:X}",
                pair[0].0,
                pair[1].0
            );
        }
        for (code, text) in TABLE {
            assert!(
                text.chars().all(|c| !is_private_use(c)),
                "U+{code:X} maps to private use"
            );
            assert!(
                (MIN_MAPPED_CODE..=MAX_MAPPED_CODE).contains(code),
                "U+{code:X} within bounds"
            );
        }
        assert_eq!(ENCLOSED.chars().count(), 20);
        assert_eq!(ENCLOSED.len(), 20 * ENCLOSED_BYTES);
    }

    fn is_private_use(c: char) -> bool {
        matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x10_0000..=0x10_FFFD)
    }

    #[test]
    fn enclosed_numbers_follow_the_reference_table() {
        assert_eq!(replacement('\u{F02B0}'), Some("⓪"));
        assert_eq!(replacement('\u{F02B1}'), Some("①"));
        assert_eq!(replacement('\u{F02C4}'), Some("⑳"));
        assert_eq!(replacement('\u{F02C5}'), Some("⑫"));
        assert_eq!(replacement('\u{F02CE}'), Some("1"));
        assert_eq!(replacement('\u{F02E1}'), Some("20"));
        assert_eq!(replacement('\u{F02E2}'), None);
        assert_eq!(replacement('\u{F0289}'), Some("①"));
        assert_eq!(replacement('\u{F012B}'), Some("(인)"));
        assert_eq!(replacement('\u{F06C}'), Some("●"));
        assert_eq!(replacement('\u{F0FE}'), Some("☑"));
    }

    #[test]
    fn unknown_private_use_and_ordinary_text_are_left_alone() {
        // A neighbour of a known code is not guessed, and the unmapped square-number gap stays.
        for c in [
            '\u{F03E0}',
            '\u{F028B}',
            '\u{F02C6}',
            '\u{E000}',
            'A',
            '가',
            '①',
        ] {
            assert_eq!(replacement(c), None, "{c:?}");
        }
    }
}
