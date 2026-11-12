//! RTF `\'hh` bytes are decoded with the code page the document or the current font names
//! (RTF 1.9.1 `\ansicpgN`, `\fcharsetN`): the single-byte pages 874 and 1250..=1258 and the
//! double-byte pages 932, 936, 949 and 950 (PD-181). A byte that no table maps is shown as U+FFFD
//! and reported, never guessed.

use easyoffice_formats::{DecodedContent, RichBlock, UniversalFormatConverter};
use std::time::Instant;

fn read_bytes(rtf: &[u8]) -> (String, Vec<String>) {
    let read = UniversalFormatConverter::decode_rich(rtf, Some("rtf")).unwrap();
    let DecodedContent::Document(doc) = read.content else {
        panic!("document");
    };
    let text = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    (text, read.warnings)
}

fn read(rtf: &str) -> (String, Vec<String>) {
    read_bytes(rtf.as_bytes())
}

fn no_code_page_warning(warnings: &[String]) -> bool {
    warnings.iter().all(|w| !w.contains("코드 페이지"))
}

#[test]
fn double_byte_pages_decode_pairs() {
    let (shown, warnings) = read("{\\rtf1\\ansi\\ansicpg932 a\\'82\\'a0\\'83\\'5c\\'88\\'9fb}");
    assert_eq!(shown, "a\u{3042}\u{30BD}\u{4E9C}b");
    assert!(no_code_page_warning(&warnings), "{warnings:?}");

    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg936 \\'b0\\'a1\\'81\\'40}");
    assert_eq!(shown, "\u{554A}\u{4E02}");
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg950 \\'a4\\'40\\'a4\\'41}");
    assert_eq!(shown, "\u{4E00}\u{4E59}");
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg949 \\'b0\\'a1\\'b3\\'aa}");
    assert_eq!(shown, "가나");
}

#[test]
fn shift_jis_single_byte_katakana_and_ascii_in_the_trail_range() {
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg932 \\'b1\\'b2\\'df}");
    assert_eq!(shown, "\u{FF71}\u{FF72}\u{FF9F}");
    // 0x5C and 0x7B are trail bytes of a pair, written as escapes.
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg932 \\'83\\'5c\\'83\\'7b}");
    assert_eq!(shown, "\u{30BD}\u{30DC}");
}

#[test]
fn single_byte_pages_decode_their_letters() {
    let cases: [(&str, &str, &str); 8] = [
        ("874", "\\'a1\\'a2", "\u{0E01}\u{0E02}"),
        ("1250", "\\'e8\\'9e", "\u{010D}\u{017E}"),
        (
            "1251",
            "\\'cf\\'f0\\'e8\\'e2\\'e5\\'f2",
            "\u{041F}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}",
        ),
        ("1253", "\\'e1\\'e2", "\u{03B1}\u{03B2}"),
        ("1254", "\\'f0\\'fd", "\u{011F}\u{0131}"),
        ("1255", "\\'e0\\'e1", "\u{05D0}\u{05D1}"),
        ("1256", "\\'c7\\'c8", "\u{0627}\u{0628}"),
        ("1258", "\\'c3\\'e0", "\u{0102}\u{00E0}"),
    ];
    for (page, escapes, expected) in cases {
        let (shown, warnings) = read(&format!("{{\\rtf1\\ansi\\ansicpg{page} {escapes}}}"));
        assert_eq!(shown, expected, "{page}");
        assert!(no_code_page_warning(&warnings), "{page}: {warnings:?}");
    }
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg1257 \\'e4\\'fe}");
    assert_eq!(shown, "\u{00E4}\u{017E}");
}

#[test]
fn a_font_charset_overrides_the_document_code_page() {
    let rtf = concat!(
        "{\\rtf1\\ansi\\ansicpg1252\\deff0",
        "{\\fonttbl{\\f0\\fnil\\fcharset0 Arial;}{\\f1\\fnil\\fcharset128 MS Mincho;}",
        "{\\f2\\fnil\\fcharset129 Batang;}{\\f3\\fnil\\fcharset134 SimSun;}",
        "{\\f4\\fnil\\fcharset136 PMingLiU;}{\\f5\\fnil\\fcharset204 Arial;}}",
        "\\f0 caf\\'e9 \\f1\\'82\\'a0 \\f2\\'b0\\'a1 \\f3\\'b0\\'a1 \\f4\\'a4\\'40 \\f5\\'cf\\f0 \\'e9}"
    );
    let (shown, warnings) = read(rtf);
    assert_eq!(
        shown,
        "caf\u{e9} \u{3042} \u{AC00} \u{554A} \u{4E00} \u{41F}\u{e9}"
    );
    assert!(no_code_page_warning(&warnings), "{warnings:?}");
}

#[test]
fn a_font_name_in_the_font_table_uses_its_own_charset() {
    let rtf = concat!(
        "{\\rtf1\\ansi\\ansicpg1252\\deff0",
        "{\\fonttbl{\\f0\\fnil\\fcharset0 Arial;}",
        "{\\f1\\fnil\\fcharset128 \\'82\\'6c\\'82\\'72 \\'96\\'be\\'92\\'a9;}}",
        "\\f1 x}"
    );
    let read = UniversalFormatConverter::decode_rich(rtf.as_bytes(), Some("rtf")).unwrap();
    let DecodedContent::Document(doc) = read.content else {
        panic!("document");
    };
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("paragraph");
    };
    assert_eq!(
        p.runs[0].font.as_deref(),
        Some("\u{FF2D}\u{FF33} \u{660E}\u{671D}")
    );
}

#[test]
fn bytes_that_no_table_maps_are_shown_as_replacement_and_counted() {
    // A lead byte with no trail, a lead with an invalid trail, and a single byte that is not a character.
    let (shown, warnings) = read("{\\rtf1\\ansi\\ansicpg932 a\\'82}");
    assert_eq!(shown, "a\u{FFFD}");
    assert!(
        warnings.iter().any(|w| w.contains("바이트 1개")),
        "{warnings:?}"
    );
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg932 \\'82\\'20x}");
    assert_eq!(shown, "\u{FFFD} x");
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg936 \\'ff\\'a1}");
    assert_eq!(shown, "\u{FFFD}\u{FFFD}");
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg1257 \\'a1}");
    assert_eq!(shown, "\u{FFFD}");
}

#[test]
fn a_page_without_a_table_is_reported_and_symbol_charsets_fall_back_to_the_document_page() {
    let (shown, warnings) = read("{\\rtf1\\ansi\\ansicpg437 a\\'80b}");
    assert_eq!(shown, "a\u{FFFD}b");
    assert!(warnings.iter().any(|w| w.contains("437")), "{warnings:?}");
    // Charset 2 (symbol) and 0 (ANSI) name no page: the document page applies.
    let rtf = "{\\rtf1\\ansi\\ansicpg1252{\\fonttbl{\\f0\\fcharset2 Symbol;}}\\f0 \\'e9}";
    let (shown, warnings) = read(rtf);
    assert_eq!(shown, "\u{e9}");
    assert!(no_code_page_warning(&warnings), "{warnings:?}");
}

#[test]
fn raw_eight_bit_bytes_use_the_ansicpg_page() {
    let (shown, _) = read_bytes(b"{\\rtf1\\ansi\\ansicpg1251 \xcf\xf0\xe8}");
    assert_eq!(shown, "\u{041F}\u{0440}\u{0438}");
    let (shown, _) = read_bytes(b"{\\rtf1\\ansi\\ansicpg932 a\x82\xa0\xb1}");
    assert_eq!(shown, "a\u{3042}\u{FF71}");
    let (shown, _) = read_bytes(b"{\\rtf1\\ansi\\ansicpg1252 caf\xe9}");
    assert_eq!(shown, "caf\u{e9}");
    // No page named, or the Korean one: CP949 as before.
    let (shown, _) = read_bytes(b"{\\rtf1\\ansi \xb0\xa1}");
    assert_eq!(shown, "\u{AC00}");
}

#[test]
fn hostile_code_page_numbers_and_charsets_are_inert() {
    for header in [
        "\\ansicpg99999999999999999999",
        "\\ansicpg-949",
        "\\ansicpg",
        "\\ansicpg0",
    ] {
        let (shown, warnings) = read(&format!("{{\\rtf1\\ansi{header} a\\'82\\'a0b}}"));
        assert_eq!(shown, "a\u{FFFD}\u{FFFD}b", "{header}");
        assert!(
            warnings.iter().any(|w| w.contains("코드 페이지")),
            "{header}: {warnings:?}"
        );
    }
    let rtf = "{\\rtf1\\ansi\\ansicpg1252{\\fonttbl{\\f0\\fcharset99999 A;}{\\f1\\fcharset-129 B;}}\\f0\\'e9\\f1\\'e9}";
    let (shown, warnings) = read(rtf);
    assert_eq!(shown, "\u{e9}\u{e9}");
    assert!(no_code_page_warning(&warnings), "{warnings:?}");
    // A sign or a short digit run is not a byte: the text stays as written.
    let (shown, _) = read("{\\rtf1\\ansi\\ansicpg932 \\'+f\\'8}");
    assert_eq!(shown, "+f8");
    let (shown, _) = read_bytes(b"{\\rtf1\\ansi\\ansicpg99999999999 \xe9}");
    assert!(!shown.is_empty());
}

#[test]
fn a_large_double_byte_body_decodes_in_linear_time() {
    let pairs = 200_000;
    let mut rtf = String::from("{\\rtf1\\ansi\\ansicpg932 ");
    for _ in 0..pairs {
        rtf.push_str("\\'82\\'a0");
    }
    rtf.push('}');
    let start = Instant::now();
    let (shown, warnings) = read(&rtf);
    let elapsed = start.elapsed();
    assert_eq!(shown.chars().count(), pairs);
    assert!(no_code_page_warning(&warnings), "{warnings:?}");
    assert!(elapsed.as_secs() < 20, "{elapsed:?}");
}
