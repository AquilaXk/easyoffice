//! `r` on `<row>` and `<c>` is optional (ECMA-376 18.3.1.73, 18.3.1.4): a row without it follows the
//! previous row, a cell without it follows the previous cell of the row (PD-172). Before, every
//! such cell was read at (0, 0) and overwrote the others.

use easyoffice_formats::{OoxmlCodec, ZipArchiveCodec, ZipSecurityPolicy};

fn xlsx_with_sheet(sheet_data: &str) -> Vec<u8> {
    let base =
        OoxmlCodec::pack_xlsx_multi(&[("S".to_string(), vec![(0, 0, "x".to_string())])]).unwrap();
    let mut entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect();
    let sheet = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{sheet_data}</sheetData></worksheet>"
    );
    entries
        .iter_mut()
        .find(|(path, _)| path == "xl/worksheets/sheet1.xml")
        .unwrap()
        .1 = sheet.into_bytes();
    ZipArchiveCodec::pack(&entries).unwrap()
}

fn read(sheet_data: &str) -> Vec<(u32, u32, String)> {
    let cells = OoxmlCodec::unpack_xlsx_multi(&xlsx_with_sheet(sheet_data)).unwrap();
    cells.into_iter().next().unwrap().1
}

#[test]
fn cells_without_r_follow_the_previous_cell_and_rows_without_r_follow_the_previous_row() {
    let cells = read(concat!(
        "<row r=\"3\"><c t=\"inlineStr\"><is><t>a</t></is></c><c><v>2</v></c>",
        "<c r=\"E3\"><v>5</v></c><c><v>6</v></c></row>",
        "<row><c><v>7</v></c><c/><c t=\"inlineStr\"><is><t>x</t></is></c></row>"
    ));
    assert_eq!(
        cells,
        [
            (2, 0, "a".to_string()),
            (2, 1, "2".to_string()),
            (2, 4, "5".to_string()),
            (2, 5, "6".to_string()),
            (3, 0, "7".to_string()),
            (3, 2, "x".to_string()),
        ]
    );
}

#[test]
fn the_first_row_without_r_is_row_one_and_a_row_r_restarts_the_column() {
    let cells = read(concat!(
        "<row><c><v>1</v></c><c><v>2</v></c></row>",
        "<row r=\"5\"><c><v>3</v></c></row>",
        "<row><c><v>4</v></c></row>"
    ));
    assert_eq!(
        cells,
        [
            (0, 0, "1".to_string()),
            (0, 1, "2".to_string()),
            (4, 0, "3".to_string()),
            (5, 0, "4".to_string()),
        ]
    );
}

#[test]
fn a_position_past_the_sheet_limit_is_refused() {
    let bad = xlsx_with_sheet("<row r=\"1048577\"><c><v>1</v></c></row>");
    assert!(OoxmlCodec::unpack_xlsx_multi(&bad).is_err());
}
