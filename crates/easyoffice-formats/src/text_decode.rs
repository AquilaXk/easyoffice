//! Encoding detection for plain-text based inputs (CSV, TSV, TXT, Markdown).
//!
//! 판정 순서(PD-158): UTF-8 BOM -> UTF-16 BOM(LE/BE) -> BOM 없는 유효 UTF-8 -> CP949.
//! BOM 없는 UTF-16은 추측하지 않는다(ASCII 위주 UTF-16은 NUL 바이트가 섞인 유효 UTF-8과 구분되지
//! 않으며, 한국어 스프레드시트의 "유니코드 텍스트"는 항상 BOM을 쓴다). 어떤 단계에서든 잘못된
//! 바이트열은 U+FFFD로 덮지 않고 거부한다(Fail-Closed).

use std::borrow::Cow;

use crate::cp949::decode_cp949_strict;
use crate::utf16_swar::{decode_utf16be_swar, decode_utf16le_swar};
use crate::EasyOfficeError;

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16BE_BOM: [u8; 2] = [0xFE, 0xFF];

fn invalid(label: &str, detail: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidUtf8(format!("{label} {detail}"))
}

fn decode_utf16be(bytes: &[u8], label: &str) -> Result<String, EasyOfficeError> {
    if bytes.len() % 2 != 0 {
        return Err(invalid(label, "UTF-16BE 바이트 길이가 짝수가 아닙니다."));
    }
    decode_utf16be_swar(bytes)
        .map_err(|_| invalid(label, "UTF-16BE에 짝이 없는 서러게이트가 있습니다."))
}

fn decode_utf16le(bytes: &[u8], label: &str) -> Result<String, EasyOfficeError> {
    if bytes.len() % 2 != 0 {
        return Err(invalid(label, "UTF-16LE 바이트 길이가 짝수가 아닙니다."));
    }
    decode_utf16le_swar(bytes)
        .map_err(|_| invalid(label, "UTF-16LE에 짝이 없는 서러게이트가 있습니다."))
}

/// Decodes text-format input bytes to a string without a leading BOM. UTF-8 input is borrowed.
/// `label` names the format in error messages (for example `"CSV/TSV"`).
pub(crate) fn decode_text<'a>(
    bytes: &'a [u8],
    label: &str,
) -> Result<Cow<'a, str>, EasyOfficeError> {
    if let Some(rest) = bytes.strip_prefix(&UTF8_BOM) {
        return std::str::from_utf8(rest)
            .map(Cow::Borrowed)
            .map_err(|_| invalid(label, "UTF-8(BOM) 디코딩 오류"));
    }
    if let Some(rest) = bytes.strip_prefix(&UTF16LE_BOM) {
        return decode_utf16le(rest, label).map(Cow::Owned);
    }
    if let Some(rest) = bytes.strip_prefix(&UTF16BE_BOM) {
        return decode_utf16be(rest, label).map(Cow::Owned);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(Cow::Borrowed(text));
    }
    decode_cp949_strict(bytes).map(Cow::Owned).ok_or_else(|| {
        invalid(
            label,
            "UTF-8·UTF-16·CP949 어느 인코딩으로도 읽을 수 없습니다.",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(bytes: &[u8]) -> Result<String, EasyOfficeError> {
        decode_text(bytes, "CSV").map(Cow::into_owned)
    }

    #[test]
    fn utf8_with_and_without_bom_is_borrowed() {
        let plain = "a,b\n가,나".as_bytes();
        assert!(matches!(
            decode_text(plain, "CSV"),
            Ok(Cow::Borrowed("a,b\n가,나"))
        ));
        let mut bom = UTF8_BOM.to_vec();
        bom.extend_from_slice(plain);
        assert!(matches!(
            decode_text(&bom, "CSV"),
            Ok(Cow::Borrowed("a,b\n가,나"))
        ));
    }

    #[test]
    fn utf8_bom_with_invalid_body_is_rejected_not_reinterpreted() {
        assert!(text(&[0xEF, 0xBB, 0xBF, 0xB0, 0xA1]).is_err());
    }

    #[test]
    fn utf16_boms_decode_both_byte_orders() {
        assert_eq!(text(&[0xFF, 0xFE, 0x00, 0xAC, b',', 0x00]).unwrap(), "가,");
        assert_eq!(text(&[0xFE, 0xFF, 0xAC, 0x00, 0x00, b',']).unwrap(), "가,");
        assert_eq!(text(&[0xFF, 0xFE]).unwrap(), "");
    }

    #[test]
    fn utf16_surrogate_pairs_decode_and_lone_surrogates_are_rejected() {
        // U+1F600 = D83D DE00
        assert_eq!(
            text(&[0xFF, 0xFE, 0x3D, 0xD8, 0x00, 0xDE]).unwrap(),
            "\u{1F600}"
        );
        assert_eq!(
            text(&[0xFE, 0xFF, 0xD8, 0x3D, 0xDE, 0x00]).unwrap(),
            "\u{1F600}"
        );
        assert!(text(&[0xFF, 0xFE, 0x3D, 0xD8]).is_err());
        assert!(text(&[0xFF, 0xFE, 0x00, 0xDE, 0x3D, 0xD8]).is_err());
        assert!(text(&[0xFE, 0xFF, 0xD8, 0x3D, 0x00, 0x41]).is_err());
    }

    #[test]
    fn odd_length_utf16_is_rejected() {
        assert!(text(&[0xFF, 0xFE, 0x41]).is_err());
        assert!(text(&[0xFE, 0xFF, 0x00, 0x41, 0x00]).is_err());
    }

    #[test]
    fn bomless_non_utf8_decodes_as_cp949() {
        // "이름,값\n漢字,※"
        let bytes = [
            0xC0, 0xCC, 0xB8, 0xA7, b',', 0xB0, 0xAA, b'\n', 0xF9, 0xD3, 0xED, 0xAE, b',', 0xA1,
            0xD8,
        ];
        assert_eq!(text(&bytes).unwrap(), "이름,값\n漢字,※");
    }

    #[test]
    fn truncated_lead_byte_and_garbage_are_rejected() {
        assert!(text(&[b'a', 0xB0]).is_err());
        assert!(text(&[0xB0, 0xA1, 0xB0]).is_err());
        assert!(text(&[0xFF, 0xFF, 0xFF]).is_err());
        assert!(text(&[0x80]).is_err());
    }

    #[test]
    fn bomless_utf16_is_not_guessed() {
        // "A" in UTF-16LE without a BOM is valid UTF-8 (A + NUL) and stays as is.
        assert_eq!(text(&[b'A', 0x00]).unwrap(), "A\0");
    }

    #[test]
    fn utf8_replacement_character_in_input_is_preserved() {
        assert_eq!(text("a\u{FFFD}b".as_bytes()).unwrap(), "a\u{FFFD}b");
    }

    #[test]
    fn utf16be_swar_matches_char_decode_across_long_ascii_and_mixed_text() {
        let long_ascii = "AsciiColumn_0123456789,".repeat(15);
        let mixed = format!("{long_ascii}가나다라\u{1F680}끝{}", "Z".repeat(71));
        let mut be_bytes = UTF16BE_BOM.to_vec();
        for u in mixed.encode_utf16() {
            be_bytes.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(text(&be_bytes).unwrap(), mixed);

        // Tail lengths 0..=14 bytes of pure ASCII around the 8-byte SWAR stride and 64-byte batch limit.
        for n in 0..=140usize {
            let s = "A".repeat(n);
            let mut raw = Vec::with_capacity(n * 2);
            for b in s.bytes() {
                raw.push(0);
                raw.push(b);
            }
            assert_eq!(decode_utf16be_swar(&raw).unwrap(), s);
        }
    }
}
