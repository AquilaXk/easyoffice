//! Readable `ToUnicode` maps for the exported PDF (PD-131).
//!
//! svg2pdf writes the text map of a font from the characters of the page SVG, so a private-use
//! character of a Hangul document (an enclosed number, a bullet) is copied and searched as that
//! private-use code. [`remap_private_use`] rewrites such entries to the standard text of
//! [`crate::pdf_pua`] in the finished file; the glyph that is drawn is not touched.
//!
//! Ported from the rhwp PDF renderer (`src/renderer/pdf.rs`, `remap_pua_in_tounicode`,
//! `remap_bfchar_block`, `utf16be_hex_to_string`, lines 953-1046; rhwp 0.8.7 at `1a76570`; MIT,
//! Copyright (c) 2025-2026 Edward Kim, see THIRD_PARTY_NOTICES.md). It works on the bytes because
//! the map is the writer's own plain text: a CMap stream has no filter and only `beginbfchar`
//! blocks (`<glyph> <utf-16 hex>` lines), and a replacement never grows (a shorter one is padded
//! with spaces inside the brackets, which PDF ignores in a hex string, ISO 32000-1 7.3.4.3), so no
//! `/Length` or cross-reference offset moves. Changes to the port: only text between `begincmap`
//! and `endcmap` is read (the rest of the file is never edited), every loop has a bound, and the
//! work is one allocation-free pass over the bytes using a 256-entry nibble lookup table and a
//! bitwise PUA prefix gate.

use crate::pdf_pua;

const BEGIN_CMAP: &[u8] = b"begincmap";
const END_CMAP: &[u8] = b"endcmap";
const BEGIN_BFCHAR: &[u8] = b"beginbfchar";
const END_BFCHAR: &[u8] = b"endbfchar";
/// Most UTF-16 units of one destination text that are read. A glyph stands for a character or a
/// short cluster; a longer entry is not one the writer made and is left alone.
const MAX_DST_UNITS: usize = 32;
/// Most destination entries read in one `beginbfchar` block. The writer ends a block after 100
/// lines; a block longer than this is skipped past, not scanned without end.
const MAX_BLOCK_ENTRIES: usize = 256;
/// The writer's digits are upper case.
const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

/// Compile-time 256-entry hex nibble lookup table (`0..=15` for valid hex digits, `0xFF` otherwise).
const HEX_NIBBLE: [u8; 256] = {
    let mut table = [0xFFu8; 256];
    let mut b = b'0';
    while b <= b'9' {
        table[b as usize] = b - b'0';
        b += 1;
    }
    let mut b = b'A';
    while b <= b'F' {
        table[b as usize] = b - b'A' + 10;
        b += 1;
    }
    let mut b = b'a';
    while b <= b'f' {
        table[b as usize] = b - b'a' + 10;
        b += 1;
    }
    table
};

/// Index of the first `needle` in `hay`.
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    let first = *needle.first()?;
    let mut at = 0;
    while let Some(rel) = hay[at..].iter().position(|&b| b == first) {
        let start = at + rel;
        if hay[start..].starts_with(needle) {
            return Some(start);
        }
        at = start + 1;
    }
    None
}

/// Calls `edit` with the bytes between each `begin` and the next `end` (the markers excluded), in
/// order, and adds up what it returns. A `begin` with no `end` after it stops the scan.
fn for_each_span(
    bytes: &mut [u8],
    begin: &[u8],
    end: &[u8],
    mut edit: impl FnMut(&mut [u8]) -> usize,
) -> usize {
    let (mut at, mut total) = (0, 0);
    while let Some(rel) = find(&bytes[at..], begin) {
        let start = at + rel + begin.len();
        let Some(len) = find(&bytes[start..], end) else {
            break;
        };
        total += edit(&mut bytes[start..start + len]);
        at = start + len + end.len();
    }
    total
}

/// Rewrites the private-use characters in every `ToUnicode` map of the PDF to their standard text
/// and returns how many map entries changed.
pub fn remap_private_use(pdf: &mut [u8]) -> usize {
    for_each_span(pdf, BEGIN_CMAP, END_CMAP, |cmap| {
        for_each_span(cmap, BEGIN_BFCHAR, END_BFCHAR, remap_block)
    })
}

/// The `<src> <dst>` pairs of one block: only each `dst` is rewritten.
fn remap_block(block: &mut [u8]) -> usize {
    let (mut at, mut is_dst, mut entries, mut changed) = (0, false, 0, 0);
    while entries < MAX_BLOCK_ENTRIES {
        let Some(open) = block[at..].iter().position(|&b| b == b'<') else {
            break;
        };
        let start = at + open + 1;
        let Some(len) = block[start..].iter().position(|&b| b == b'>') else {
            break;
        };
        let end = start + len;
        if is_dst {
            entries += 1;
            changed += usize::from(remap_entry(&mut block[start..end]));
        }
        is_dst = !is_dst;
        at = end + 1;
    }
    changed
}

/// UTF-16 units of the hex text (four digits each) and whether any unit can belong to a mapped PUA
/// code point (`0xF0xx` BMP or `0xDB80` high surrogate for `U+F0000..U+F03FF`/`U+F08xx`).
#[inline]
fn decode_units(hex: &[u8], units: &mut [u16; MAX_DST_UNITS]) -> Option<(usize, bool)> {
    if hex.is_empty() || (hex.len() & 3) != 0 || (hex.len() >> 2) > MAX_DST_UNITS {
        return None;
    }
    let count = hex.len() >> 2;
    let mut has_pua_candidate = false;
    for (at, unit) in units[..count].iter_mut().enumerate() {
        let base = at << 2;
        let n0 = HEX_NIBBLE[usize::from(hex[base])];
        let n1 = HEX_NIBBLE[usize::from(hex[base + 1])];
        let n2 = HEX_NIBBLE[usize::from(hex[base + 2])];
        let n3 = HEX_NIBBLE[usize::from(hex[base + 3])];
        if ((n0 | n1 | n2 | n3) & 0xF0) != 0 {
            return None;
        }
        let u = (u16::from(n0) << 12) | (u16::from(n1) << 8) | (u16::from(n2) << 4) | u16::from(n3);
        has_pua_candidate |= (u & 0xFF00) == 0xF000 || (u & 0xFFFC) == 0xDB80;
        *unit = u;
    }
    Some((count, has_pua_candidate))
}

/// Rewrites one destination text in place. Nothing changes (and `false` comes back) when it has no
/// known private-use character, is not valid UTF-16, or the standard text is longer than the code.
fn remap_entry(hex: &mut [u8]) -> bool {
    let mut units = [0u16; MAX_DST_UNITS];
    let Some((count, has_pua_candidate)) = decode_units(hex, &mut units) else {
        return false;
    };
    if !has_pua_candidate {
        return false;
    }
    let mut out = [0u16; MAX_DST_UNITS];
    let (mut written, mut changed) = (0, false);
    for decoded in char::decode_utf16(units[..count].iter().copied()) {
        let Ok(c) = decoded else { return false };
        let replacement = pdf_pua::replacement(c);
        changed |= replacement.is_some();
        let mut one = [0u8; 4];
        let text = replacement.unwrap_or_else(|| c.encode_utf8(&mut one));
        for unit in text.encode_utf16() {
            if written == count {
                return false;
            }
            out[written] = unit;
            written += 1;
        }
    }
    if !changed {
        return false;
    }
    for (at, &unit) in out[..written].iter().enumerate() {
        let base = at << 2;
        hex[base] = HEX_DIGITS[usize::from(unit >> 12)];
        hex[base + 1] = HEX_DIGITS[usize::from((unit >> 8) & 0xF)];
        hex[base + 2] = HEX_DIGITS[usize::from((unit >> 4) & 0xF)];
        hex[base + 3] = HEX_DIGITS[usize::from(unit & 0xF)];
    }
    hex[(written << 2)..].fill(b' ');
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmap(entries: &str) -> Vec<u8> {
        format!("<< /Length 9 >>\nstream\n/CIDInit begincmap\n1 beginbfchar\n{entries}endbfchar\nendcmap\nendstream")
            .into_bytes()
    }

    fn remapped(entries: &str) -> (String, usize) {
        let mut bytes = cmap(entries);
        let length = bytes.len();
        let count = remap_private_use(&mut bytes);
        assert_eq!(bytes.len(), length, "the file never changes size");
        (String::from_utf8(bytes).expect("ascii"), count)
    }

    #[test]
    fn a_private_use_entry_becomes_its_standard_text_and_keeps_its_length() {
        // U+F02B1 (surrogate pair DB80 DEB1), a BMP bullet U+F06C and a plain letter.
        let (out, count) = remapped("<0001> <DB80DEB1>\n<0002> <F06C>\n<0003> <0041>\n");
        assert_eq!(count, 2);
        assert!(out.contains("<0001> <2460    >\n"), "{out}");
        assert!(out.contains("<0002> <25CF>\n"), "{out}");
        assert!(out.contains("<0003> <0041>\n"), "{out}");
    }

    #[test]
    fn a_cluster_keeps_its_other_characters_and_a_longer_text_is_skipped() {
        // "A" + U+F02B1 stays one text of the same width; U+F02D7 ("10", two units) fits its
        // surrogate pair; "(인)" is three units and does not fit the pair of U+F012B.
        let (out, count) =
            remapped("<0001> <0041DB80DEB1>\n<0002> <DB80DED7>\n<0003> <DB80DD2B>\n");
        assert_eq!(count, 2, "{out}");
        assert!(out.contains("<0001> <00412460    >\n"), "{out}");
        assert!(out.contains("<0002> <00310030>\n"), "{out}");
        assert!(
            out.contains("<0003> <DB80DD2B>\n"),
            "(인) is 3 units, the pair is 2: {out}"
        );
    }

    #[test]
    fn unknown_private_use_and_damaged_entries_are_left_alone() {
        let entries = "<0001> <DB80DFE0>\n<0002> <D800>\n<0003> <12G4>\n<0004> <123>\n<0005> <>\n";
        let (out, count) = remapped(entries);
        assert_eq!(count, 0);
        assert!(out.contains(entries), "{out}");
    }

    /// Only text between `begincmap` and `endcmap` is read: the same lines elsewhere (a content
    /// stream, an unfinished map) are never edited.
    #[test]
    fn text_outside_a_map_is_never_edited() {
        let line = "1 beginbfchar\n<0001> <DB80DEB1>\nendbfchar\n";
        let mut outside = format!("stream\n{line}endstream\nbegincmap\n{line}").into_bytes();
        let before = outside.clone();
        assert_eq!(remap_private_use(&mut outside), 0, "no endcmap, no map");
        assert_eq!(outside, before);

        let mut inside = format!("begincmap\n{line}endcmap\nstream\n{line}endstream").into_bytes();
        assert_eq!(remap_private_use(&mut inside), 1);
        let text = String::from_utf8(inside).expect("ascii");
        assert_eq!(
            text.matches("<DB80DEB1>").count(),
            1,
            "the second copy is outside the map"
        );
    }

    #[test]
    fn two_maps_are_both_rewritten_and_the_pass_is_idempotent() {
        let mut bytes = cmap("<0001> <F06C>\n");
        bytes.extend(cmap("<0001> <DB80DEB2>\n"));
        assert_eq!(remap_private_use(&mut bytes), 2);
        assert_eq!(
            remap_private_use(&mut bytes),
            0,
            "a rewritten map has nothing left to do"
        );
    }

    /// A hostile map: more entries than a block may hold, an entry longer than any cluster, and a
    /// run of markers with no end. The scan ends, and only the first entries are rewritten.
    #[test]
    fn hostile_maps_are_bounded() {
        let mut entries = String::new();
        for _ in 0..MAX_BLOCK_ENTRIES + 10 {
            entries.push_str("<0001> <F06C>\n");
        }
        let (_, count) = remapped(&entries);
        assert_eq!(count, MAX_BLOCK_ENTRIES);

        let long = "F06C".repeat(MAX_DST_UNITS + 1);
        let (_, count) = remapped(&format!("<0001> <{long}>\n"));
        assert_eq!(count, 0, "an over-long destination is not read");

        let mut open = "begincmap beginbfchar <0001> <F06C> "
            .repeat(2_000)
            .into_bytes();
        let before = open.clone();
        assert_eq!(
            remap_private_use(&mut open),
            0,
            "nothing ends, nothing is edited"
        );
        assert_eq!(open, before);
    }
}
