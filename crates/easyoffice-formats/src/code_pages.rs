//! Windows code pages for legacy 8-bit text: the single-byte pages 874 and 1250..=1258, and the
//! double-byte pages 932 (Shift_JIS), 936 (GBK), 949 (UHC, see [`crate::cp949`]) and 950 (Big5).
//!
//! 참조 구현: encoding_rs 0.8.44 (`src/single_byte.rs`, `src/shift_jis.rs`, `src/gb18030.rs`,
//! `src/big5.rs`; (Apache-2.0 OR MIT) AND BSD-3-Clause, Copyright Mozilla Foundation). 표 데이터는
//! WHATWG Encoding Standard의 `index-windows-*`, `index-jis0208`, `index-gb18030`, `index-big5`
//! 이며 BSD-3-Clause(`LICENSE-WHATWG`). encoding_rs의 디코더를 이식하지 않고, 두 바이트 조합마다
//! encoding_rs가 돌려주는 글자를 `u16` 리틀 엔디언으로 평탄화한 표 4장을 `code_page_data/`에
//! 둔다. 조회는 `(lead - first_lead) * trail_span + trail_index` 직접 인덱싱이며 값 0은 미할당
//! 칸이다. 출처 고지는 `THIRD_PARTY_NOTICES.md`.
//!
//! 표 범위: `sbcs.bin` 10쪽 x 0x80..=0xFF, `sjis.bin` 리드 0x81..=0x9F·0xE0..=0xFC x 트레일
//! 0x40..=0x7E·0x80..=0xFC (사용자 정의 0xF0..=0xF9 행은 U+E000부터), `gbk.bin` 리드 0x81..=0xFE x
//! 트레일 0x40..=0x7E·0x80..=0xFE, `big5.bin` 리드 0x81..=0xFE x 트레일 0x40..=0x7E·0xA1..=0xFE.
//! Big5의 BMP 밖 글자(홍콩 보충 문자 중 U+10000 이상)와 두 글자로 풀리는 네 칸은 비워 두었다(윈도우
//! 950에 없다). GB18030의 네 바이트 열도 936에 없어 다루지 않는다.

use crate::cp949::decode_cp949_pair;

static SBCS: &[u8; SBCS_PAGES * SBCS_HIGH_BYTES * 2] = include_bytes!("code_page_data/sbcs.bin");
static SJIS: &[u8; SJIS_LEADS * SJIS_TRAILS * 2] = include_bytes!("code_page_data/sjis.bin");
static GBK: &[u8; GBK_LEADS * GBK_TRAILS * 2] = include_bytes!("code_page_data/gbk.bin");
static BIG5: &[u8; BIG5_LEADS * BIG5_TRAILS * 2] = include_bytes!("code_page_data/big5.bin");

/// Pages in `sbcs.bin`, in the order of [`CodePage::from_number`]: 874, then 1250..=1258.
const SBCS_PAGES: usize = 10;
/// Bytes 0x80..=0xFF of a single-byte page.
const SBCS_HIGH_BYTES: usize = 128;
const SJIS_LEADS: usize = 60;
const SJIS_TRAILS: usize = 188;
const GBK_LEADS: usize = 126;
const GBK_TRAILS: usize = 190;
const BIG5_LEADS: usize = 126;
const BIG5_TRAILS: usize = 157;
/// Trail bytes 0x40..=0x7E of Big5 come first in a row, then 0xA1..=0xFE.
const BIG5_LOW_TRAILS: usize = 63;

/// A code page the engine has a table for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodePage {
    /// 874 and 1250..=1258: the page's index in `sbcs.bin`.
    SingleByte(u8),
    /// 932
    ShiftJis,
    /// 936
    Gbk,
    /// 949
    Uhc,
    /// 950
    Big5,
}

/// The two bytes of a table cell as a character; `0` marks an unassigned cell.
#[inline]
fn cell(table: &[u8], index: usize) -> Option<char> {
    let unit = u16::from_le_bytes([table[2 * index], table[2 * index + 1]]);
    (unit != 0)
        .then(|| char::from_u32(u32::from(unit)))
        .flatten()
}

impl CodePage {
    /// The page for a Windows code page number (`\ansicpgN`), if the engine has its table.
    pub(crate) fn from_number(number: u32) -> Option<Self> {
        Some(match number {
            874 => Self::SingleByte(0),
            1250..=1258 => Self::SingleByte((number - 1249) as u8),
            932 => Self::ShiftJis,
            936 => Self::Gbk,
            949 => Self::Uhc,
            950 => Self::Big5,
            _ => return None,
        })
    }

    /// The page of an RTF `\fcharsetN` (RTF 1.9.1 font table), for the charsets that name one.
    /// ANSI (0), default (1), symbol (2) and the Mac and OEM charsets have no page of their own.
    pub(crate) fn from_rtf_charset(charset: u8) -> Option<Self> {
        let number = match charset {
            128 => 932,
            129 => 949,
            134 => 936,
            136 => 950,
            161 => 1253,
            162 => 1254,
            163 => 1258,
            177 => 1255,
            178 => 1256,
            186 => 1257,
            204 => 1251,
            222 => 874,
            238 => 1250,
            _ => return None,
        };
        Self::from_number(number)
    }

    /// Whether `byte` starts a two-byte character of this page.
    #[inline]
    pub(crate) fn is_lead(self, byte: u8) -> bool {
        match self {
            Self::SingleByte(_) => false,
            Self::ShiftJis => matches!(byte, 0x81..=0x9F | 0xE0..=0xFC),
            Self::Gbk | Self::Big5 | Self::Uhc => matches!(byte, 0x81..=0xFE),
        }
    }

    /// The character of a byte that stands alone: ASCII everywhere; 0x80..=0xFF through the
    /// page's table or, for 932, the half-width katakana 0xA1..=0xDF and for 936 the euro 0x80.
    /// 949 keeps the Latin-1 reading of a lone high byte that the RTF reader always had.
    pub(crate) fn decode_single(self, byte: u8) -> Option<char> {
        if byte < 0x80 {
            return Some(char::from(byte));
        }
        match self {
            Self::SingleByte(page) => cell(
                SBCS,
                usize::from(page) * SBCS_HIGH_BYTES + usize::from(byte - 0x80),
            ),
            Self::ShiftJis => match byte {
                0x80 => Some('\u{80}'),
                0xA1..=0xDF => char::from_u32(0xFF61 + u32::from(byte - 0xA1)),
                _ => None,
            },
            Self::Gbk => (byte == 0x80).then_some('\u{20AC}'),
            Self::Uhc => Some(char::from(byte)),
            Self::Big5 => None,
        }
    }

    /// The character of a lead byte and a trail byte, `None` when the pair is not assigned.
    #[inline]
    pub(crate) fn decode_pair(self, lead: u8, trail: u8) -> Option<char> {
        match self {
            Self::SingleByte(_) => None,
            Self::Uhc => decode_cp949_pair(lead, trail),
            Self::ShiftJis => {
                let row = match lead {
                    0x81..=0x9F => lead - 0x81,
                    0xE0..=0xFC => lead - 0xE0 + 31,
                    _ => return None,
                };
                let col = match trail {
                    0x40..=0x7E => trail - 0x40,
                    0x80..=0xFC => trail - 0x41,
                    _ => return None,
                };
                cell(SJIS, usize::from(row) * SJIS_TRAILS + usize::from(col))
            }
            Self::Gbk => {
                let col = match trail {
                    0x40..=0x7E => trail - 0x40,
                    0x80..=0xFE => trail - 0x41,
                    _ => return None,
                };
                let row = lead.checked_sub(0x81).filter(|r| *r < 126)?;
                cell(GBK, usize::from(row) * GBK_TRAILS + usize::from(col))
            }
            Self::Big5 => {
                let col = match trail {
                    0x40..=0x7E => usize::from(trail - 0x40),
                    0xA1..=0xFE => BIG5_LOW_TRAILS + usize::from(trail - 0xA1),
                    _ => return None,
                };
                let row = lead.checked_sub(0x81).filter(|r| *r < 126)?;
                cell(BIG5, usize::from(row) * BIG5_TRAILS + col)
            }
        }
    }

    /// Decodes whole bytes; a byte or pair that is not assigned becomes U+FFFD.
    pub(crate) fn decode_bytes_lossy(self, bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() + bytes.len() / 2);
        let mut i = 0usize;
        while i < bytes.len() {
            let byte = bytes[i];
            if self.is_lead(byte) {
                if let Some(ch) = bytes
                    .get(i + 1)
                    .and_then(|trail| self.decode_pair(byte, *trail))
                {
                    out.push(ch);
                    i += 2;
                    continue;
                }
            }
            out.push(self.decode_single(byte).unwrap_or('\u{FFFD}'));
            i += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(number: u32) -> CodePage {
        CodePage::from_number(number).unwrap()
    }

    #[test]
    fn table_sizes_match_their_row_and_column_counts() {
        assert_eq!(SBCS.len(), 2560);
        assert_eq!(SJIS.len(), 22_560);
        assert_eq!(GBK.len(), 47_880);
        assert_eq!(BIG5.len(), 39_564);
    }

    #[test]
    fn pages_are_found_by_number_and_by_rtf_charset() {
        for number in [
            874, 932, 936, 949, 950, 1250, 1251, 1252, 1253, 1254, 1255, 1256, 1257, 1258,
        ] {
            assert!(CodePage::from_number(number).is_some(), "{number}");
        }
        for number in [0, 437, 850, 1249, 1259, 1361, 10000, 65001, u32::MAX] {
            assert!(CodePage::from_number(number).is_none(), "{number}");
        }
        assert_eq!(CodePage::from_rtf_charset(128), Some(CodePage::ShiftJis));
        assert_eq!(CodePage::from_rtf_charset(129), Some(CodePage::Uhc));
        assert_eq!(CodePage::from_rtf_charset(134), Some(CodePage::Gbk));
        assert_eq!(CodePage::from_rtf_charset(136), Some(CodePage::Big5));
        assert_eq!(CodePage::from_rtf_charset(204), Some(page(1251)));
        for charset in [0, 1, 2, 77, 130, 254, 255] {
            assert_eq!(CodePage::from_rtf_charset(charset), None, "{charset}");
        }
    }

    #[test]
    fn single_byte_pages_decode_their_high_half() {
        let cases = [
            (874, 0xA1, '\u{0E01}'),
            (1250, 0xE8, '\u{010D}'),
            (1251, 0xC0, '\u{0410}'),
            (1252, 0x80, '\u{20AC}'),
            (1252, 0xE9, 'é'),
            (1253, 0xE1, '\u{03B1}'),
            (1254, 0xF0, '\u{011F}'),
            (1255, 0xE0, '\u{05D0}'),
            (1256, 0xC7, '\u{0627}'),
            (1257, 0xE4, 'ä'),
            (1258, 0xC3, '\u{0102}'),
        ];
        for (number, byte, expected) in cases {
            assert_eq!(page(number).decode_single(byte), Some(expected), "{number}");
        }
        assert_eq!(page(1252).decode_single(b'A'), Some('A'));
        // Unassigned cells stay unassigned.
        assert_eq!(page(874).decode_single(0xDB), None);
        assert_eq!(page(1257).decode_single(0xA1), None);
    }

    #[test]
    fn shift_jis_decodes_pairs_katakana_and_user_defined_rows() {
        let sjis = page(932);
        assert_eq!(sjis.decode_pair(0x82, 0xA0), Some('\u{3042}'));
        assert_eq!(sjis.decode_pair(0x83, 0x5C), Some('\u{30BD}'));
        assert_eq!(sjis.decode_pair(0x88, 0x9F), Some('\u{4E9C}'));
        assert_eq!(sjis.decode_pair(0x81, 0x40), Some('\u{3000}'));
        assert_eq!(sjis.decode_pair(0xF0, 0x40), Some('\u{E000}'));
        assert_eq!(sjis.decode_single(0xB1), Some('\u{FF71}'));
        assert_eq!(sjis.decode_single(0xDF), Some('\u{FF9F}'));
        assert_eq!(sjis.decode_single(0xA0), None);
        assert_eq!(sjis.decode_pair(0x82, 0x7F), None);
        assert_eq!(sjis.decode_pair(0x82, 0xFD), None);
        assert_eq!(sjis.decode_pair(0xA1, 0x40), None);
        assert!(
            sjis.is_lead(0x81) && sjis.is_lead(0x9F) && sjis.is_lead(0xE0) && sjis.is_lead(0xFC)
        );
        assert!(!sjis.is_lead(0xA0) && !sjis.is_lead(0xDF) && !sjis.is_lead(0x7F));
    }

    #[test]
    fn gbk_and_big5_decode_pairs() {
        let gbk = page(936);
        assert_eq!(gbk.decode_pair(0xB0, 0xA1), Some('\u{554A}'));
        assert_eq!(gbk.decode_pair(0x81, 0x40), Some('\u{4E02}'));
        assert_eq!(gbk.decode_pair(0xFE, 0xFE), Some('\u{E4C5}'));
        assert_eq!(gbk.decode_single(0x80), Some('\u{20AC}'));
        assert_eq!(gbk.decode_pair(0x81, 0x7F), None);
        assert_eq!(gbk.decode_pair(0x80, 0x40), None);
        assert_eq!(gbk.decode_pair(0xFF, 0x40), None);
        let big5 = page(950);
        assert_eq!(big5.decode_pair(0xA4, 0x40), Some('\u{4E00}'));
        assert_eq!(big5.decode_pair(0xA4, 0x41), Some('\u{4E59}'));
        assert_eq!(big5.decode_pair(0xA1, 0x40), Some('\u{3000}'));
        assert_eq!(big5.decode_pair(0xA4, 0x7F), None);
        assert_eq!(big5.decode_pair(0xA4, 0xA0), None);
        assert_eq!(big5.decode_pair(0xFF, 0x40), None);
        assert_eq!(big5.decode_single(0x80), None);
    }

    #[test]
    fn assigned_cell_counts_pin_the_tables() {
        let assigned = |table: &[u8]| {
            table
                .chunks_exact(2)
                .filter(|c| u16::from_le_bytes([c[0], c[1]]) != 0)
                .count()
        };
        assert_eq!(assigned(SJIS), 9_604);
        assert_eq!(assigned(GBK), 23_940);
        assert_eq!(assigned(BIG5), 16_877);
        assert_eq!(SBCS.len() / 2 - assigned(SBCS), 23);
    }

    #[test]
    fn whole_byte_strings_decode_with_replacement_for_bad_input() {
        let sjis = page(932);
        assert_eq!(
            sjis.decode_bytes_lossy(b"a\x82\xa0\xb1b"),
            "a\u{3042}\u{FF71}b"
        );
        // A lead byte at the end, and a lead followed by an invalid trail.
        assert_eq!(sjis.decode_bytes_lossy(b"a\x82"), "a\u{FFFD}");
        assert_eq!(sjis.decode_bytes_lossy(b"\x82\x20x"), "\u{FFFD} x");
        assert_eq!(page(1252).decode_bytes_lossy(b"caf\xe9"), "café");
        assert_eq!(
            page(950).decode_bytes_lossy(b"\xa4\x40\xa4"),
            "\u{4E00}\u{FFFD}"
        );
    }
}
