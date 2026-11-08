//! Table cells of Pages 14.4 (file format 26.4.0): number, percent and currency cells with the
//! thousands separator switched on and off, plus string, duration and error cells. The two
//! fixtures are real documents (metadata, previews and template images removed); the expected
//! display values are the ones shown by Pages itself for these documents (Korean screen, checked
//! cell by cell), with the format rule that produces each of them named next to the assertion.

use easyoffice_formats::{DecodedContent, DocumentFormat, RichBlock, UniversalFormatConverter};

type Grid = Vec<Vec<String>>;

fn first_table(name: &str) -> (Grid, Vec<String>) {
    let path = format!("{}/tests/fixtures/pages/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(path).expect("fixture readable");
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some(name)).expect("decode");
    assert_eq!(decoded.format, DocumentFormat::Pages);
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("expected a document");
    };
    let table = doc
        .blocks
        .into_iter()
        .find_map(|b| match b {
            RichBlock::Table(table) => Some(table.rows),
            RichBlock::Paragraph(_) | RichBlock::Image(_) => None,
        })
        .expect("a table");
    (table, decoded.warnings)
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| (*c).to_string()).collect()
}

#[test]
fn thousands_separator_switch_is_read_from_format_field_5() {
    let (grid, warnings) = first_table("eo-pages-thousands.pages");
    // Column 1 has the separator on, column 2 off (format field 5 = 1 / 0 in the format list).
    // Number, decimals 1: 1234.5 -> "1,234.5" / "1234.5".
    assert_eq!(grid[0][..2], row(&["1,234.5", "1234.5"])[..]);
    // Percent, decimals 1: the stored 0.125 is shown times 100 -> "12.5%" with or without the
    // separator (Pages screen).
    assert_eq!(grid[1][..2], row(&["12.5%", "12.5%"])[..]);
    // Currency KRW, decimals 0, symbol in front: 10000 -> "₩10,000" / "₩10000".
    assert_eq!(grid[2][..2], row(&["₩10,000", "₩10000"])[..]);
    assert!(
        warnings.iter().all(|w| !w.contains("읽을 수 없는 값")),
        "every cell is readable: {warnings:?}"
    );
    // Column C and rows 4-6 are empty.
    let empty = row(&["", "", ""]);
    assert_eq!(grid.len(), 6);
    assert_eq!(grid[0][2], "");
    assert!(grid[3..].iter().all(|r| *r == empty));
}

#[test]
fn string_duration_and_error_cells_are_read() {
    let (grid, warnings) = first_table("eo-pages-cellkinds.pages");
    // String cell: the text of the string list ("123" typed as text stays text).
    assert_eq!(grid[0][0], "123");
    // Duration cells: the value is seconds; the format shows hours..minutes (units 4..8) and
    // days..hours (units 2..4) in the compact style with Korean unit words: 5400 s = "1시간 30분",
    // 183600 s = "2일 3시간" (Pages screen).
    assert_eq!(grid[0][1], "1시간 30분");
    assert_eq!(grid[1][1], "2일 3시간");
    // Error cells show the cached error text exactly as the Pages screen does ("÷0!" is the
    // Korean division-by-zero error, PD-097 item 8).
    assert_eq!(grid[0][2], "÷0!");
    assert_eq!(grid[1][0], "#REF!");
    // Rich text cells keep working next to the new kinds.
    assert_eq!(grid[1][2], "안녕하세요");
    assert_eq!(grid[3][0], "병합 가로");
    assert_eq!(grid[3][2], "병합 세로");
    assert!(
        warnings.iter().all(|w| !w.contains("읽을 수 없는 값")),
        "every cell is readable: {warnings:?}"
    );
    // Merged cells keep their text in the top-left cell; the rest of a merge is empty
    // (A4:B4 "병합 가로", C4:C5 "병합 세로").
    assert_eq!(
        grid,
        vec![
            row(&["123", "1시간 30분", "÷0!"]),
            row(&["#REF!", "2일 3시간", "안녕하세요"]),
            row(&["", "", ""]),
            row(&["병합 가로", "", "병합 세로"]),
            row(&["", "", ""]),
            row(&["", "", ""]),
        ]
    );
}
