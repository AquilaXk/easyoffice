//! The sheet-wide column width of an XLSX file (`sheetFormatPr` `defaultColWidth` and `baseColWidth`) is read, kept in
//! `SheetStyling::default_col_width` and written back, so a save no longer drops it (PD-303). A value a hostile file makes up
//! (not a number, not finite, out of the 0..=255 characters a column may have) is read as missing and never clamped.

use easyoffice_formats::{
    read_sheet_styling, DefaultColWidth, DocumentFormat, EasyOfficeError, OdfCodec, OdsUnpacked,
    OoxmlCodec, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy, MAX_COL_WIDTH_CHARS,
};

fn cell() -> OdsUnpacked {
    ("Sheet1".to_string(), vec![(0, 0, "x".to_string())])
}

const SHEET_HEAD: &str = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#;
const SHEET_TAIL: &str = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#;

fn xlsx_with_sheet(sheet: &str) -> Vec<u8> {
    let base = OoxmlCodec::pack_xlsx_multi(&[cell()]).unwrap();
    let entries = ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default()).unwrap();
    let replaced: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| match e.path.as_str() {
            "xl/worksheets/sheet1.xml" => (e.path, sheet.as_bytes().to_vec()),
            _ => (e.path, e.data),
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&replaced).unwrap()
}

fn with_format_pr(attrs: &str) -> Vec<u8> {
    xlsx_with_sheet(&format!(
        r#"{SHEET_HEAD}<sheetFormatPr {attrs}/>{SHEET_TAIL}"#
    ))
}

fn styling_of(bytes: &[u8]) -> SheetStyling {
    read_sheet_styling(bytes, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1
}

fn sheet_xml(bytes: &[u8]) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let sheet = entries
        .iter()
        .find(|e| e.path == "xl/worksheets/sheet1.xml")
        .unwrap();
    String::from_utf8(sheet.data.clone()).unwrap()
}

fn saved(styling: SheetStyling) -> Result<Vec<u8>, EasyOfficeError> {
    OoxmlCodec::pack_xlsx_multi_styled(&[cell()], &[], &[styling])
}

fn width(width: Option<f64>, base: Option<u32>) -> Option<DefaultColWidth> {
    Some(DefaultColWidth { width, base })
}

#[test]
fn the_default_and_base_width_are_read_and_written_back() {
    let styling = styling_of(&with_format_pr(
        r#"baseColWidth="10" defaultColWidth="20.7109375" defaultRowHeight="15""#,
    ));
    assert_eq!(styling.default_col_width, width(Some(20.7109375), Some(10)));
    let again = saved(styling.clone()).unwrap();
    let xml = sheet_xml(&again);
    assert!(
        xml.contains(r#"baseColWidth="10" defaultColWidth="20.7109375""#),
        "{xml}"
    );
    assert_eq!(
        styling_of(&again).default_col_width,
        styling.default_col_width
    );
}

#[test]
fn either_value_alone_is_kept() {
    let only_default = styling_of(&with_format_pr(
        r#"defaultColWidth="9.140625" defaultRowHeight="15""#,
    ));
    assert_eq!(only_default.default_col_width, width(Some(9.140625), None));
    let xml = sheet_xml(&saved(only_default).unwrap());
    assert!(
        xml.contains(r#"defaultColWidth="9.140625""#) && !xml.contains("baseColWidth"),
        "{xml}"
    );

    let only_base = styling_of(&with_format_pr(
        r#"baseColWidth="10" defaultRowHeight="16""#,
    ));
    assert_eq!(only_base.default_col_width, width(None, Some(10)));
    let xml = sheet_xml(&saved(only_base).unwrap());
    assert!(
        xml.contains(r#"baseColWidth="10""#) && !xml.contains("defaultColWidth"),
        "{xml}"
    );
}

/// A file with neither says nothing, and the writer adds nothing for it.
#[test]
fn a_file_without_them_has_none_and_keeps_none() {
    let styling = styling_of(&with_format_pr(r#"defaultRowHeight="15""#));
    assert_eq!(styling.default_col_width, None);
    let xml = sheet_xml(&saved(styling).unwrap());
    assert!(!xml.contains("ColWidth"), "{xml}");
    assert_eq!(
        styling_of(&xlsx_with_sheet(&format!("{SHEET_HEAD}{SHEET_TAIL}"))).default_col_width,
        None
    );
}

/// The row default and the groups of the same element are still written beside them.
#[test]
fn the_width_is_written_beside_the_row_default_and_the_group_levels() {
    let sheet = format!(
        r#"{SHEET_HEAD}<sheetFormatPr baseColWidth="12" defaultColWidth="15" defaultRowHeight="21" customHeight="1" outlineLevelRow="1"/><sheetData><row r="1" outlineLevel="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#
    );
    let styling = styling_of(&xlsx_with_sheet(&sheet));
    assert_eq!(styling.default_col_width, width(Some(15.0), Some(12)));
    let again = styling_of(&saved(styling.clone()).unwrap());
    assert_eq!(again.default_col_width, styling.default_col_width);
    assert_eq!(again.default_row_height, styling.default_row_height);
    assert_eq!(again.outline, styling.outline);
}

/// A column that has a width of its own is not changed by the default (the widths written down are the file's).
#[test]
fn the_widths_of_the_columns_stay_as_the_file_wrote_them() {
    let sheet = format!(
        r#"{SHEET_HEAD}<sheetFormatPr defaultColWidth="20" defaultRowHeight="15"/><cols><col min="1" max="1" width="9.140625" customWidth="1"/><col min="3" max="3" width="30" customWidth="1"/></cols>{SHEET_TAIL}"#
    );
    let styling = styling_of(&xlsx_with_sheet(&sheet));
    let columns = styling.columns.clone();
    assert_eq!(
        columns.iter().map(|(&c, &w)| (c, w)).collect::<Vec<_>>(),
        vec![(0, 64), (2, 210)]
    );
    let again = styling_of(&saved(styling).unwrap());
    assert_eq!(again.columns, columns);
    assert_eq!(again.default_col_width, width(Some(20.0), None));
}

/// What a hostile file puts in the attributes is no width: the sheet reads as if it had none, and nothing is clamped.
#[test]
fn a_value_that_is_not_a_width_is_read_as_missing() {
    let long = "9".repeat(64);
    let bad_widths = [
        "NaN", "nan", "inf", "-inf", "Infinity", "-1", "0", "-0", "0.0", "255.01", "256", "1e3",
        "1e999", "1e-999", "abc", "", " ", "0x20", "20px", "2 0", "٢٠", &long,
    ];
    for text in bad_widths {
        let styling = styling_of(&with_format_pr(&format!(
            r#"defaultColWidth="{text}" defaultRowHeight="15""#
        )));
        assert_eq!(styling.default_col_width, None, "defaultColWidth={text:?}");
    }
    let bad_bases = [
        "-1",
        "256",
        "1000",
        "8.5",
        "NaN",
        "inf",
        "abc",
        "",
        "99999999999999999999",
        "+",
        "0x8",
        &long,
    ];
    for text in bad_bases {
        let styling = styling_of(&with_format_pr(&format!(
            r#"baseColWidth="{text}" defaultRowHeight="15""#
        )));
        assert_eq!(styling.default_col_width, None, "baseColWidth={text:?}");
    }
    // One good value next to a bad one: the good one stays.
    let mixed = styling_of(&with_format_pr(
        r#"baseColWidth="NaN" defaultColWidth="12" defaultRowHeight="15""#,
    ));
    assert_eq!(mixed.default_col_width, width(Some(12.0), None));
}

#[test]
fn the_limits_are_in_and_just_outside_are_out() {
    assert_eq!(MAX_COL_WIDTH_CHARS, 255.0);
    for (text, want) in [
        ("0.001", Some(0.001)),
        ("1", Some(1.0)),
        ("255", Some(255.0)),
        ("254.99609375", Some(254.99609375)),
    ] {
        let styling = styling_of(&with_format_pr(&format!(r#"defaultColWidth="{text}""#)));
        assert_eq!(
            styling.default_col_width,
            want.map(|w| DefaultColWidth {
                width: Some(w),
                base: None
            }),
            "{text}"
        );
    }
    for (text, want) in [
        ("0", Some(0)),
        ("8", Some(8)),
        ("255", Some(255)),
        ("256", None),
    ] {
        let styling = styling_of(&with_format_pr(&format!(r#"baseColWidth="{text}""#)));
        assert_eq!(
            styling.default_col_width,
            want.map(|b| DefaultColWidth {
                width: None,
                base: Some(b)
            }),
            "{text}"
        );
    }
}

/// The writer does not take a width the reader would refuse: a styling built by hand with one is refused whole.
#[test]
fn a_styling_with_a_bad_width_is_refused_by_the_writer() {
    let bad = [
        width(Some(f64::NAN), None),
        width(Some(f64::INFINITY), None),
        width(Some(f64::NEG_INFINITY), None),
        width(Some(0.0), None),
        width(Some(-1.0), None),
        width(Some(255.5), None),
        width(None, Some(256)),
        width(None, Some(u32::MAX)),
        width(Some(20.0), Some(256)),
    ];
    for default_col_width in bad {
        let styling = SheetStyling {
            default_col_width,
            ..SheetStyling::default()
        };
        let err = saved(styling).unwrap_err();
        assert!(
            matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
            "{default_col_width:?}: {err}"
        );
    }
    // A pair with nothing in it is no width.
    let empty = SheetStyling {
        default_col_width: width(None, None),
        ..SheetStyling::default()
    };
    assert!(saved(empty).is_err());
}

#[test]
fn a_styling_with_only_a_width_is_not_empty() {
    let styling = SheetStyling {
        default_col_width: width(Some(12.0), None),
        ..SheetStyling::default()
    };
    assert!(!styling.is_empty());
}

/// ODS has no sheet-wide width the page reads (the unstyled columns follow the table's default), so a save to ODS writes
/// none and reads none (the decision is in PD-303).
#[test]
fn ods_does_not_carry_it() {
    let styling = SheetStyling {
        default_col_width: width(Some(20.0), Some(10)),
        ..SheetStyling::default()
    };
    let ods = OdfCodec::pack_ods_multi_styled(&[cell()], std::slice::from_ref(&styling)).unwrap();
    let back = read_sheet_styling(&ods, DocumentFormat::Ods)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(back.default_col_width, None);
}
