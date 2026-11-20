//! CP949 (KS X 1001 + Unified Hangul Code) decoding for RTF and legacy text bytes.
//!
//! A thin adapter over the `encoding_rs` crate's `EUC_KR` (the WHATWG euc-kr index, which is
//! Windows code page 949; `(Apache-2.0 OR MIT) AND BSD-3-Clause`). This module keeps the three
//! decode policies the readers use: one pair, strict whole text, and lenient raw bytes whose
//! unmappable bytes become Latin-1.

use encoding_rs::{DecoderResult, EUC_KR};

/// First byte of a CP949 two-byte pair; anything below is ASCII or unassigned.
const FIRST_LEAD: u8 = 0x81;

/// Decodes a 2-byte CP949 (KS X 1001 / UHC) pair: Hangul, symbols, Compatibility Jamo or Hanja.
pub(crate) fn decode_cp949_pair(b1: u8, b2: u8) -> Option<char> {
    if b1 < FIRST_LEAD {
        return None;
    }
    let pair = [b1, b2];
    let text = EUC_KR.decode_without_bom_handling_and_without_replacement(&pair)?;
    let mut chars = text.chars();
    let ch = chars.next()?;
    chars.next().is_none().then_some(ch)
}

/// Decodes raw RTF input bytes: whole UTF-8 buffers stay intact; otherwise valid CP949 pairs and
/// ASCII decode as text, and a byte that starts no valid pair becomes its Latin-1 character (the
/// byte after it is then tried as a lead of its own).
pub(crate) fn decode_cp949_or_latin1_bytes(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = bytes;
    loop {
        let mut decoder = EUC_KR.new_decoder_without_bom_handling();
        out.reserve(
            decoder
                .max_utf8_buffer_length_without_replacement(rest.len())
                .unwrap_or(rest.len().saturating_mul(3)),
        );
        let (result, read) = decoder.decode_to_string_without_replacement(rest, &mut out, true);
        match result {
            DecoderResult::InputEmpty => return out,
            DecoderResult::OutputFull => {
                unreachable!("the output was reserved for the whole input")
            }
            DecoderResult::Malformed(bad, after) => {
                let start = read - usize::from(after) - usize::from(bad);
                out.push(char::from(rest[start]));
                rest = &rest[start + 1..];
            }
        }
    }
}

/// Strict CP949 decoder for whole text files: any lone lead byte, truncated pair or unassigned
/// pair rejects the input (`None`) instead of substituting U+FFFD, so a mis-detected encoding is
/// never silently opened as corrupted text.
pub(crate) fn decode_cp949_strict(bytes: &[u8]) -> Option<String> {
    EUC_KR
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(std::borrow::Cow::into_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV-1a over `(lead, trail, codepoint-or-0)` for every `0x81..=0xFE` x `0x41..=0xFE` pair.
    /// The digest and the 17,048 assigned cells pin the whole mapping, so a crate update that
    /// changes the EUC-KR index fails here instead of silently changing how old files open.
    #[test]
    fn full_pair_space_matches_reference_index() {
        let mut fnv: u64 = 0xcbf2_9ce4_8422_2325;
        let mut assigned = 0usize;
        for b1 in 0x81u8..=0xFE {
            for b2 in 0x41u8..=0xFE {
                let cp = decode_cp949_pair(b1, b2).map_or(0, u32::from);
                assigned += usize::from(cp != 0);
                for x in [u64::from(b1), u64::from(b2), u64::from(cp)] {
                    fnv = (fnv ^ x).wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
        }
        assert_eq!(assigned, 17_048);
        assert_eq!(fnv, 0x0ce2_423b_8471_8f68);
    }

    #[test]
    fn table_boundary_cells_decode() {
        assert_eq!(decode_cp949_pair(0xA1, 0xA1), Some('\u{3000}'));
        assert_eq!(decode_cp949_pair(0xA1, 0xFE), Some('\u{FFE2}'));
        assert_eq!(decode_cp949_pair(0xA2, 0xE6), Some('€'));
        assert_eq!(decode_cp949_pair(0xA4, 0xA1), Some('ㄱ'));
        assert_eq!(decode_cp949_pair(0xB0, 0xA1), Some('가'));
        assert_eq!(decode_cp949_pair(0xC8, 0xFE), Some('힝'));
        assert_eq!(decode_cp949_pair(0xCA, 0xA1), Some('伽'));
        assert_eq!(decode_cp949_pair(0xFD, 0xFE), Some('詰'));
        assert_eq!(decode_cp949_pair(0x81, 0x41), Some('갂'));
        assert_eq!(decode_cp949_pair(0xC6, 0x52), Some('\u{D7A3}'));
    }

    #[test]
    fn unassigned_and_out_of_range_pairs_are_rejected() {
        assert_eq!(decode_cp949_pair(0xC9, 0xA1), None);
        assert_eq!(decode_cp949_pair(0xFE, 0xA1), None);
        assert_eq!(decode_cp949_pair(0x80, 0x41), None);
        assert_eq!(decode_cp949_pair(0xFF, 0xA1), None);
        assert_eq!(decode_cp949_pair(0xB0, 0xFF), None);
        assert_eq!(decode_cp949_pair(0xCA, 0x41), None);
    }

    #[test]
    fn strict_decode_handles_mixed_ascii_hangul_hanja_symbols() {
        // "A,가나 漢字 ※\n" in CP949.
        let bytes = [
            b'A', b',', 0xB0, 0xA1, 0xB3, 0xAA, b' ', 0xF9, 0xD3, 0xED, 0xAE, b' ', 0xA1, 0xD8,
            b'\n',
        ];
        assert_eq!(
            decode_cp949_strict(&bytes).as_deref(),
            Some("A,가나 漢字 ※\n")
        );
    }

    #[test]
    fn strict_decode_rejects_truncated_and_invalid_sequences() {
        assert_eq!(decode_cp949_strict(&[b'a', 0xB0]), None);
        assert_eq!(decode_cp949_strict(&[0xB0, 0x20]), None);
        assert_eq!(decode_cp949_strict(&[0xFF]), None);
        assert_eq!(decode_cp949_strict(&[0x80, 0x41]), None);
        assert_eq!(decode_cp949_strict(&[0xC9, 0xA1]), None);
        assert_eq!(decode_cp949_strict(b""), Some(String::new()));
    }

    #[test]
    fn cp949_pairs_overlapping_utf8_bit_patterns_are_not_misdecoded() {
        // [0xB0, 0xA1] ('가') forces non-UTF-8 fallback path.
        // [0xC8, 0xA1..=0xBF] matches valid 2-byte UTF-8 (110xxxxx 10xxxxxx), e.g. 0xC8 0xAD ('화').
        // [0xEC, 0xA1, 0xB0] matches valid 3-byte UTF-8 (1110xxxx 10xxxxxx 10xxxxxx) across
        // Hanja (0xEC, 0xA1) + Hangul (0xB0, 0xA1).
        let hanja = decode_cp949_pair(0xEC, 0xA1).unwrap();
        let hangul_2b = decode_cp949_pair(0xC8, 0xAD).unwrap();
        assert_eq!(hangul_2b, '화');

        let bytes_2b = [b'x', 0xB0, 0xA1, 0xC8, 0xAD, b'y'];
        assert_eq!(decode_cp949_or_latin1_bytes(&bytes_2b), "x가화y");

        let bytes_3b = [0xB0, 0xA1, 0xEC, 0xA1, 0xB0, 0xA1];
        assert_eq!(
            decode_cp949_or_latin1_bytes(&bytes_3b),
            format!("가{hanja}가")
        );

        // Unassigned CP949 pair [0xC9, 0xA1] overlaps 2-byte UTF-8 too, and must fall back
        // to Latin-1 bytes rather than synthetic UTF-8 U+0261.
        let unassigned = [0xB0, 0xA1, 0xC9, 0xA1];
        assert_eq!(
            decode_cp949_or_latin1_bytes(&unassigned),
            "가\u{00C9}\u{00A1}"
        );
    }
}
