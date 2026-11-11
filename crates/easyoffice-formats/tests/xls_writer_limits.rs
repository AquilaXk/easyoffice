//! `.xls` writer limits: sheet names Excel accepts, records that fit their 16-bit length, and
//! text that merely starts with `=` stored as text rather than as a formula record.

use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, LegacyOle2Codec, LegacyXlsPayload, Ole2ContainerCodec,
    UniversalDocumentModel, UniversalFormatConverter,
};

fn sheet(name: &str) -> LegacyXlsPayload {
    LegacyXlsPayload {
        sheet_name: name.to_string(),
        cells: vec![(0, 0, "x".to_string())],
    }
}

/// Opcodes of every record in the workbook stream.
fn opcodes(xls: &[u8]) -> Vec<u16> {
    let data = Ole2ContainerCodec::unpack(xls)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Workbook")
        .unwrap()
        .data;
    let mut out = Vec::new();
    let mut at = 0;
    while at + 4 <= data.len() {
        out.push(u16::from_le_bytes([data[at], data[at + 1]]));
        at += 4 + usize::from(u16::from_le_bytes([data[at + 2], data[at + 3]]));
    }
    out
}

fn names(xls: &[u8]) -> Vec<String> {
    LegacyOle2Codec::unpack_xls_multi(xls)
        .unwrap()
        .into_iter()
        .map(|s| s.sheet_name)
        .collect()
}

#[test]
fn sheet_names_follow_the_excel_rules() {
    let long = "가".repeat(40);
    let emoji = format!("{}😀", "a".repeat(30));
    let input = [
        "예산:2026/1분기",
        "a[b]c*d?e\\f",
        long.as_str(),
        "'앞뒤'",
        "",
        "Data",
        "data",
        emoji.as_str(),
    ];
    let clean = LegacyOle2Codec::sanitize_sheet_names(&input);
    assert_eq!(clean[0], "예산_2026_1분기");
    assert_eq!(clean[1], "a_b_c_d_e_f");
    assert_eq!(clean[2], "가".repeat(31));
    assert_eq!(clean[3], "앞뒤");
    assert_eq!(clean[4], "Sheet5");
    assert_eq!(clean[5], "Data");
    assert_ne!(
        clean[6].to_lowercase(),
        "data",
        "case-insensitive duplicate gets a number"
    );
    assert!(clean[6].starts_with("data"));
    assert_eq!(
        clean[7],
        "a".repeat(30),
        "a surrogate pair is never cut in half"
    );
    for n in &clean {
        assert!(n.encode_utf16().count() <= 31, "{n}");
        assert!(!n.contains([':', '\\', '/', '?', '*', '[', ']']), "{n}");
    }
    let lower: std::collections::HashSet<_> = clean.iter().map(|n| n.to_lowercase()).collect();
    assert_eq!(lower.len(), clean.len(), "names are unique ignoring case");

    let sheets: Vec<_> = input.iter().map(|n| sheet(n)).collect();
    let xls = LegacyOle2Codec::pack_xls_multi_values(&sheets).unwrap();
    assert_eq!(
        names(&xls),
        clean,
        "the written workbook carries the cleaned names"
    );
}

#[test]
fn clean_names_are_left_alone() {
    let input = ["매출", "요약 2026", "Sheet1"];
    assert_eq!(LegacyOle2Codec::sanitize_sheet_names(&input), input);
}

#[test]
fn identical_long_names_stay_distinct_after_truncation() {
    let a = "나".repeat(35);
    let clean = LegacyOle2Codec::sanitize_sheet_names(&[a.as_str(), a.as_str(), a.as_str()]);
    assert_eq!(clean[0], "나".repeat(31));
    assert_eq!(clean[1], format!("{}_2", "나".repeat(29)));
    assert_eq!(clean[2], format!("{}_3", "나".repeat(29)));
}

#[test]
fn a_formula_that_cannot_fit_one_record_is_refused_not_truncated() {
    let long = format!("={}", "A".repeat(5_000));
    let payload = LegacyXlsPayload {
        sheet_name: "수식".to_string(),
        cells: vec![(0, 0, long)],
    };
    let err = LegacyOle2Codec::pack_xls(&payload).unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "{err}"
    );

    let max_fit = format!("={}", "A".repeat(4_000));
    let ok = LegacyXlsPayload {
        sheet_name: "수식".to_string(),
        cells: vec![(0, 0, max_fit.clone())],
    };
    let xls = LegacyOle2Codec::pack_xls(&ok).unwrap();
    assert_eq!(
        LegacyOle2Codec::unpack_xls(&xls).unwrap().cells[0].2,
        max_fit
    );
}

#[test]
fn document_text_starting_with_equals_is_stored_as_text_not_as_a_formula() {
    let model = UniversalDocumentModel {
        title: "문서".to_string(),
        paragraphs: vec!["=1+2".to_string(), "=== 제목 ===".to_string()],
        ..Default::default()
    };
    let xls = UniversalFormatConverter::encode(&model, DocumentFormat::Xls).unwrap();
    assert!(
        !opcodes(&xls).contains(&LegacyOle2Codec::BIFF8_FORMULA),
        "no FORMULA record for document text"
    );
    let cells = LegacyOle2Codec::unpack_xls(&xls).unwrap().cells;
    assert_eq!(cells[0].2, "=1+2");
    assert_eq!(cells[1].2, "=== 제목 ===");
}
