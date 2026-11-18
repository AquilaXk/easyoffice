//! An XLS workbook's number formats (the `XF` format index and the `FORMAT` records) reach the
//! formatter (PD-235).
//!
//! Reference implementation: the files are spreadsheet files from the Apache POI test data
//! (`test-data/spreadsheet/`, Apache-2.0, commit 47dcff769bb395034dddfd59c4609659b6dbc6a4, see
//! `THIRD_PARTY_NOTICES.md`): `FormatKM.xls` keeps the text its program showed next to each
//! formatted value, so the formatter is checked against that program's own output.

use easyoffice_formats::{resolve_code, LegacyOle2Codec, SheetStyling};
use easyoffice_sheet::format_cell_value;

type Cells = Vec<(u32, u32, String)>;

fn open(name: &str) -> (Cells, SheetStyling) {
    let path = format!(
        "{}/tests/data/xls_formats/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).expect("fixture");
    let (mut sheets, mut stylings) = LegacyOle2Codec::unpack_xls_with_styling(&bytes).unwrap();
    (sheets.remove(0).1, stylings.remove(0).1)
}

fn shown(cells: &Cells, styling: &SheetStyling, row: u32, col: u32) -> Option<String> {
    let raw = &cells.iter().find(|(r, c, _)| (*r, *c) == (row, col))?.2;
    let code = resolve_code(styling.cells.get(&(row, col))?.num_fmt, &styling.num_fmts)?;
    format_cell_value(code, raw).map(|f| f.text)
}

#[test]
fn dates_in_the_user_defined_and_built_in_formats() {
    let (cells, styling) = open("DateFormats.xls");
    let column: Vec<_> = (0..5).map(|r| shown(&cells, &styling, r, 1)).collect();
    assert_eq!(
        column,
        [
            "10-Aug-07",
            "8/10/07",
            "2007-08-10",
            "Friday, August 10, 2007",
            "10/08/07"
        ]
        .map(|s| Some(s.to_string()))
    );
}

#[test]
fn a_number_with_one_digit_after_the_point() {
    let (cells, styling) = open("SimpleWithDataFormat.xls");
    assert_eq!(shown(&cells, &styling, 0, 0).as_deref(), Some("1.3"));
}

#[test]
fn thousands_and_millions_codes_match_the_text_the_program_stored() {
    let (cells, styling) = open("FormatKM.xls");
    let text = |row: u32, col: u32| {
        cells
            .iter()
            .find(|(r, c, _)| (*r, *c) == (row, col))
            .map(|(_, _, v)| v.clone())
    };
    let mut compared = 0;
    for row in 1..60 {
        for (code_col, expected_col) in [(1, 2), (3, 4)] {
            let (Some(raw), Some(expected)) = (text(row, code_col), text(row, expected_col)) else {
                continue;
            };
            if raw.starts_with('=') {
                continue;
            }
            assert_eq!(
                shown(&cells, &styling, row, code_col),
                Some(expected.clone()),
                "row {row} column {code_col}: {raw}"
            );
            compared += 1;
        }
    }
    assert!(compared >= 20, "{compared}");
}
