//! The default column of a sheet that says nothing is 72 px (8 characters of the 8 px digit of Malgun Gothic 11, rounded up
//! to a multiple of 8 pixels), for the window, the chart and picture anchors and the PDF alike (PD-305 and PD-306, which
//! replaced the 96 px of PD-302). The widths a file gives a column come back as they went, and the sheet-wide `defaultColWidth` and
//! `baseColWidth` of an XLSX file never change a width that is written down.

use std::collections::BTreeMap;

use easyoffice_formats::{
    col_left_px, read_sheet_styling, ChartAnchor, DocumentFormat, OdfCodec, OdsUnpacked,
    OoxmlCodec, SheetGrid, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy, DEFAULT_COL_PX,
    MAX_COLUMN_PX, MIN_COLUMN_PX,
};

fn cell() -> OdsUnpacked {
    ("Sheet1".to_string(), vec![(0, 0, "x".to_string())])
}

fn with_columns(columns: &[(u32, u32)]) -> SheetStyling {
    SheetStyling {
        columns: columns.iter().copied().collect(),
        ..SheetStyling::default()
    }
}

#[test]
fn the_default_column_is_the_72_px_of_a_sheet_that_says_nothing() {
    assert_eq!(DEFAULT_COL_PX, 72);
    assert_eq!(SheetStyling::default().default_col_px(), 72);
    assert_eq!(col_left_px(0, &BTreeMap::new(), 72), 0);
    assert_eq!(col_left_px(3, &BTreeMap::new(), 72), 216);
    // A custom column changes the columns after it by the difference to the default.
    assert_eq!(col_left_px(3, &BTreeMap::from([(1, 200)]), 72), 216 + 128);
}

#[test]
fn an_anchor_is_placed_on_the_default_columns() {
    let mut anchor = ChartAnchor {
        from_col: 2,
        to_col: 4,
        from_row: 1,
        to_row: 3,
        ..ChartAnchor::default()
    };
    anchor.sync_px_from_grid(&SheetGrid::of(&SheetStyling::default()));
    assert_eq!((anchor.left_px, anchor.width_px), (144, 144));
    // The other way: 216 px is the left edge of column 3 and 72 px the width of one column.
    let mut by_px = ChartAnchor {
        left_px: 216,
        top_px: 24,
        width_px: 72,
        height_px: 24,
        ..ChartAnchor::default()
    };
    by_px.sync_grid_from_px(&SheetGrid::of(&SheetStyling::default()));
    assert_eq!((by_px.from_col, by_px.col_off_emu), (3, 0));
    assert_eq!(by_px.to_col, 4);
}

/// The first spreadsheet program's default column (8.43 characters, 64 px), the Korean edition's (8.38 characters, 72 px, the
/// default of this crate), the old engine default (80 px), the old window default (96 px) and the two limits come back from both formats exactly.
#[test]
fn the_widths_written_down_come_back_as_they_were() {
    let columns = [
        (0, 64),
        (6, 72),
        (1, 80),
        (2, 96),
        (3, 200),
        (4, MIN_COLUMN_PX),
        (5, MAX_COLUMN_PX),
    ];
    let styling = with_columns(&columns);
    let xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&[cell()], &[], std::slice::from_ref(&styling)).unwrap();
    let ods = OdfCodec::pack_ods_multi_styled(&[cell()], std::slice::from_ref(&styling)).unwrap();
    for (bytes, format) in [(xlsx, DocumentFormat::Xlsx), (ods, DocumentFormat::Ods)] {
        let back = read_sheet_styling(&bytes, format).unwrap();
        assert_eq!(back[0].1.columns, styling.columns, "{format:?}");
    }
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

fn columns_of(bytes: &[u8]) -> BTreeMap<u32, u32> {
    read_sheet_styling(bytes, DocumentFormat::Xlsx).unwrap()[0]
        .1
        .columns
        .clone()
}

/// A file with a sheet-wide default width and some columns of their own: the columns keep their width through a save and
/// a second read, and the columns without one stay without (they are as wide as the sheet's default says, PD-305).
#[test]
fn a_sheet_default_width_does_not_change_the_widths_written_down() {
    let sheet = format!(
        r#"{SHEET_HEAD}<sheetFormatPr baseColWidth="10" defaultColWidth="20" defaultRowHeight="15"/><cols><col min="1" max="1" width="9.140625" customWidth="1"/><col min="3" max="4" width="30" customWidth="1"/></cols>{SHEET_TAIL}"#
    );
    let first = columns_of(&xlsx_with_sheet(&sheet));
    // The workbook's default font is the standard one (an 8 px digit), so 9.140625 characters are 73 px and 30 are 240 px.
    assert_eq!(first, BTreeMap::from([(0, 73), (2, 240), (3, 240)]));
    let styling = read_sheet_styling(&xlsx_with_sheet(&sheet), DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1;
    let saved = OoxmlCodec::pack_xlsx_multi_styled(&[cell()], &[], &[styling]).unwrap();
    assert_eq!(columns_of(&saved), first);
}

/// A width, a range or a default width a hostile file makes up is read as no width or clamped to the limits.
#[test]
fn hostile_widths_are_clamped_or_ignored() {
    for cols in [
        r#"<col min="1" max="16384" width="1e300" customWidth="1"/>"#,
        r#"<col min="1" max="3" width="NaN" customWidth="1"/>"#,
        r#"<col min="1" max="3" width="-5" customWidth="1"/>"#,
        r#"<col min="1" max="3" width="0" customWidth="1"/>"#,
        r#"<col min="9" max="2" width="30" customWidth="1"/>"#,
        r#"<col min="0" max="2" width="30" customWidth="1"/>"#,
        r#"<col min="1" max="99999999999" width="30" customWidth="1"/>"#,
    ] {
        let sheet = format!(
            r#"{SHEET_HEAD}<sheetFormatPr baseColWidth="-1" defaultColWidth="1e308"/><cols>{cols}</cols>{SHEET_TAIL}"#
        );
        let columns = columns_of(&xlsx_with_sheet(&sheet));
        assert!(
            columns
                .iter()
                .all(|(&col, &px)| col < 16_384 && (MIN_COLUMN_PX..=MAX_COLUMN_PX).contains(&px)),
            "{cols}: {columns:?}"
        );
    }
}
