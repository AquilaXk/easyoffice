//! Merged ranges and hidden rows and columns survive an XLSX round trip.

use easyoffice_formats::{CellBox, EasyOfficeError, OoxmlCodec, SheetStyling, MAX_HIDDEN_ROWS};

fn pack(styling: SheetStyling) -> Result<Vec<u8>, EasyOfficeError> {
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling])
}

fn read(bytes: &[u8]) -> SheetStyling {
    OoxmlCodec::unpack_xlsx_with_styling(bytes)
        .unwrap()
        .1
        .remove(0)
        .1
}

fn sample() -> SheetStyling {
    SheetStyling {
        merges: vec![
            CellBox::parse("A1:B2").unwrap(),
            CellBox::parse("D4:D9").unwrap(),
        ],
        hidden_rows: vec![(2, 3), (10, 10)],
        hidden_cols: vec![(1, 1), (4, 6)],
        columns: [(0, 120), (4, 80)].into(),
        ..SheetStyling::default()
    }
}

#[test]
fn layout_round_trips() {
    let bytes = pack(sample()).unwrap();
    let back = read(&bytes);
    assert_eq!(back.merges, sample().merges);
    assert_eq!(back.hidden_rows, sample().hidden_rows);
    assert_eq!(back.hidden_cols, sample().hidden_cols);
    assert_eq!(back.columns.get(&0), sample().columns.get(&0));
    assert_eq!(back.columns.get(&4), sample().columns.get(&4));
    // A second pass writes the same layout again.
    assert_eq!(read(&pack(back.clone()).unwrap()), back);
}

#[test]
fn the_part_uses_the_standard_elements_in_schema_order() {
    let bytes = pack(sample()).unwrap();
    let entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    let sheet = entries
        .iter()
        .find(|e| e.path == "xl/worksheets/sheet1.xml")
        .unwrap();
    let xml = String::from_utf8(sheet.data.clone()).unwrap();
    assert!(xml.contains(r#"<row r="3" hidden="1">"#), "{xml}");
    assert!(xml.contains(r#"<row r="11" hidden="1">"#), "{xml}");
    assert!(xml.contains(r#"hidden="1""#));
    let (cols, data, merges) = (
        xml.find("<cols>").unwrap(),
        xml.find("<sheetData>").unwrap(),
        xml.find("<mergeCells").unwrap(),
    );
    assert!(cols < data && data < merges);
    assert!(xml.contains(r#"<mergeCell ref="A1:B2"/>"#));
}

#[test]
fn layouts_beyond_the_bounds_are_refused_on_write() {
    let too_many = SheetStyling {
        hidden_rows: vec![(0, MAX_HIDDEN_ROWS as u32)],
        ..SheetStyling::default()
    };
    assert!(pack(too_many).is_err());
    let off_sheet = SheetStyling {
        merges: vec![CellBox {
            row0: 0,
            col0: 0,
            row1: 1_048_576,
            col1: 1,
        }],
        ..SheetStyling::default()
    };
    assert!(pack(off_sheet).is_err());
}
