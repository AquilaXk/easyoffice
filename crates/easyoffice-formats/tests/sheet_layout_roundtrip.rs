//! Merged ranges and hidden rows and columns survive an XLSX round trip.

use easyoffice_formats::{
    resolve_code, CellBox, CellStyle, EasyOfficeError, OoxmlCodec, SheetStyling, CUSTOM_BASE,
    MAX_HIDDEN_ROWS,
};
use std::sync::Arc;

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

#[test]
fn number_formats_merges_and_hidden_parts_share_one_sheet() {
    let mut styling = sample();
    let styled = |num_fmt| CellStyle {
        num_fmt,
        bold: num_fmt == CUSTOM_BASE,
        ..CellStyle::default()
    };
    styling.cells.insert((0, 0), styled(CUSTOM_BASE));
    styling.cells.insert((0, 1), styled(CUSTOM_BASE + 1));
    styling.cells.insert((4, 0), styled(CUSTOM_BASE));
    styling.num_fmts = Arc::new(vec!["\"₩\"#,##0".to_string(), "0.0%".to_string()]);
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![(0, 0, "1234.5".to_string()), (0, 1, "0.25".to_string())],
    )];
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling.clone()]).unwrap();
    let back = read(&bytes);
    assert_eq!(back.merges, styling.merges);
    assert_eq!(back.hidden_rows, styling.hidden_rows);
    assert_eq!(back.hidden_cols, styling.hidden_cols);
    let code = |at| resolve_code(back.cells[&at].num_fmt, &back.num_fmts).map(str::to_string);
    assert_eq!(code((0, 0)).as_deref(), Some("\"₩\"#,##0"));
    assert_eq!(code((0, 1)).as_deref(), Some("0.0%"));
    assert_eq!(code((4, 0)).as_deref(), Some("\"₩\"#,##0"));
    assert!(back.cells[&(0, 0)].bold);
    // A hidden row that holds a formatted cell keeps both.
    let mut again = styling;
    again.hidden_rows = vec![(0, 0)];
    let reread = read(&OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[again]).unwrap());
    assert_eq!(reread.hidden_rows, vec![(0, 0)]);
    assert_eq!(
        resolve_code(reread.cells[&(0, 0)].num_fmt, &reread.num_fmts),
        Some("\"₩\"#,##0")
    );
}
