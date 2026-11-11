//! Opening CSV/TSV/TXT/Markdown saved by Korean spreadsheet and editor defaults: UTF-8 (BOM),
//! UTF-16 with BOM ("유니코드 텍스트") and CP949, with Fail-Closed rejection of anything else.

use easyoffice_formats::{DecodedContent, EasyOfficeError, UniversalFormatConverter};

fn utf16le_bom(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    out
}

fn sheet_cells(bytes: &[u8], name: &str) -> Vec<(u32, u32, String)> {
    let doc = UniversalFormatConverter::decode_rich(bytes, Some(name)).unwrap();
    let DecodedContent::Workbook(sheets) = doc.content else {
        panic!("workbook expected");
    };
    sheets.into_iter().next().unwrap().1
}

fn cp949_csv() -> Vec<u8> {
    // "이름,값\n漢字,※\n" in CP949
    vec![
        0xC0, 0xCC, 0xB8, 0xA7, b',', 0xB0, 0xAA, b'\n', 0xF9, 0xD3, 0xED, 0xAE, b',', 0xA1, 0xD8,
        b'\n',
    ]
}

fn expected_cells() -> Vec<(u32, u32, String)> {
    [(0, 0, "이름"), (0, 1, "값"), (1, 0, "漢字"), (1, 1, "※")]
        .into_iter()
        .map(|(r, c, v)| (r, c, v.to_string()))
        .collect()
}

#[test]
fn cp949_csv_opens_with_hanja_and_symbols() {
    assert_eq!(sheet_cells(&cp949_csv(), "a.csv"), expected_cells());
}

#[test]
fn utf16le_bom_tsv_and_csv_open() {
    let tsv = sheet_cells(&utf16le_bom("이름\t값\n漢字\t※\n"), "a.tsv");
    assert_eq!(tsv, expected_cells());
    let csv = sheet_cells(&utf16le_bom("이름,값\r\n漢字,※\r\n"), "a.csv");
    assert_eq!(csv, expected_cells());
}

#[test]
fn utf8_bom_csv_still_opens() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("이름,값\n漢字,※\n".as_bytes());
    assert_eq!(sheet_cells(&bytes, "a.csv"), expected_cells());
}

#[test]
fn cp949_txt_and_markdown_open_through_both_decode_paths() {
    let mut txt = vec![0xC1, 0xA6, 0xB8, 0xF1, b'\n', 0xF9, 0xD3, 0xED, 0xAE]; // "제목\n漢字"
    let doc = UniversalFormatConverter::decode_rich(&txt, Some("a.txt")).unwrap();
    assert!(matches!(doc.content, DecodedContent::Document(_)));
    txt.splice(0..0, *b"# ");
    let md = UniversalFormatConverter::decode_rich(&txt, Some("a.md")).unwrap();
    assert!(matches!(md.content, DecodedContent::Document(_)));
}

#[test]
fn undecodable_bytes_are_rejected_instead_of_replaced() {
    for bad in [
        vec![b'a', b',', 0xB0],       // truncated lead byte
        vec![0xFF, 0xFF, b'\n'],      // not valid in any supported encoding
        vec![0xFF, 0xFE, 0x41],       // odd-length UTF-16
        vec![0xFF, 0xFE, 0x3D, 0xD8], // lone surrogate
    ] {
        let err = UniversalFormatConverter::decode_rich(&bad, Some("a.csv")).unwrap_err();
        assert!(
            matches!(err, EasyOfficeError::InvalidUtf8(_)),
            "{bad:?} -> {err:?}"
        );
    }
}
