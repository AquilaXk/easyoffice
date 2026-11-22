//! A new workbook, a CSV or TSV and an ODS without a font of its own have the default font of the Korean edition of the
//! spreadsheet program: Malgun Gothic 11 pt, an 8 px digit, a default column of 8.38 characters (72 px) (PD-306). A workbook
//! that has another Normal font keeps it through a save, and the columns of every such file keep their pixels through a save
//! and a second read. The font name and size are bounded as every font name and size is.

use std::collections::BTreeMap;

use easyoffice_formats::{
    read_sheet_styling, CellStyle, DefaultColWidth, DefaultFont, DocumentFormat, OdfCodec,
    OdsUnpacked, OoxmlCodec, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy, MAX_FONT_NAME_CHARS,
    STANDARD_DIGIT_PX, STANDARD_FONT_NAME, STANDARD_FONT_SIZE_PT,
};

fn cell() -> OdsUnpacked {
    ("Sheet1".to_string(), vec![(0, 0, "x".to_string())])
}

const SHEET_HEAD: &str = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#;
const SHEET_TAIL: &str = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c></row></sheetData></worksheet>"#;

/// A file whose sheet is `sheet` and whose `xl/styles.xml` is `styles`.
fn xlsx_with(styles: &str, sheet: &str) -> Vec<u8> {
    let base = OoxmlCodec::pack_xlsx_multi(&[cell()]).unwrap();
    let entries = ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default()).unwrap();
    let replaced: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| match e.path.as_str() {
            "xl/worksheets/sheet1.xml" => (e.path, sheet.as_bytes().to_vec()),
            "xl/styles.xml" => (e.path, styles.as_bytes().to_vec()),
            _ => (e.path, e.data),
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&replaced).unwrap()
}

/// A stylesheet whose Normal font is `<sz val="{size}"/><name val="{name}"/>`, and one more font and cell format of the
/// same face in bold and at 14 pt.
fn styles_of(size: &str, name: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
            r#"<fonts count="2"><font><sz val="{size}"/><name val="{name}"/></font>"#,
            r#"<font><b/><sz val="14"/><name val="{name}"/></font></fonts>"#,
            r#"<cellXfs count="2"><xf fontId="0"/><xf fontId="1"/></cellXfs></styleSheet>"#
        ),
        size = size,
        name = name
    )
}

fn sheet_of(sheet_format: &str, cols: &str) -> String {
    format!("{SHEET_HEAD}{sheet_format}<cols>{cols}</cols>{SHEET_TAIL}")
}

fn styling_of(bytes: &[u8]) -> SheetStyling {
    read_sheet_styling(bytes, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.into_iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data).unwrap()
}

fn saved(styling: SheetStyling) -> Vec<u8> {
    OoxmlCodec::pack_xlsx_multi_styled(&[cell()], &[], &[styling]).unwrap()
}

/// The first `<font>` of a written `styles.xml`: the Normal font.
fn normal_font(bytes: &[u8]) -> String {
    let styles = part(bytes, "xl/styles.xml");
    let start = styles.find("<fonts").unwrap();
    let from = &styles[start..];
    let open = from.find("<font>").unwrap();
    let end = from[open..].find("</font>").unwrap() + "</font>".len();
    from[open..open + end].to_string()
}

#[test]
fn the_standard_default_font_is_the_korean_editions() {
    assert_eq!(STANDARD_FONT_NAME, "맑은 고딕");
    assert_eq!(STANDARD_FONT_SIZE_PT, 11.0);
    assert_eq!(STANDARD_DIGIT_PX, 8);
    let new = SheetStyling::default();
    assert_eq!((new.default_col_px(), new.digit_px()), (72, 8));
    assert_eq!(new.default_font, None);
}

/// A new workbook says the Normal font as the Korean edition does; the column width dialog of that edition shows 8.38 (72 px)
/// because 8 characters of the 8 px digit and the 5 px of padding are 69 px, up to 72.
#[test]
fn a_new_workbook_is_written_with_malgun_gothic_11_and_reads_back_as_72_px() {
    let bytes = OoxmlCodec::pack_xlsx_multi(&[cell()]).unwrap();
    assert_eq!(
        normal_font(&bytes),
        "<font><sz val=\"11\"/><name val=\"맑은 고딕\"/></font>"
    );
    let read = styling_of(&bytes);
    assert_eq!((read.default_col_px(), read.default_font), (72, None));
    // The sheet says nothing about its default column, so the program works out its own: 72 px.
    assert!(!part(&bytes, "xl/worksheets/sheet1.xml").contains("defaultColWidth"));
    // A column of 72 px is 9 characters of the 8 px digit.
    let styling = SheetStyling {
        columns: BTreeMap::from([(0, 72), (2, 100)]),
        ..SheetStyling::default()
    };
    let sheet = part(&saved(styling.clone()), "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(r#"<col min="1" max="1" width="9" customWidth="1"/>"#),
        "{sheet}"
    );
    assert!(sheet.contains(r#"<col min="3" max="3" width="12.5" customWidth="1"/>"#));
    assert_eq!(styling_of(&saved(styling.clone())).columns, styling.columns);
}

/// A CSV or TSV sheet and an ODS without a font read as a styling with no default: the writer says the standard font.
#[test]
fn a_csv_and_an_ods_without_a_font_have_the_standard_default() {
    let ods = OdfCodec::pack_ods_multi_styled(&[cell()], &[SheetStyling::default()]).unwrap();
    let back = read_sheet_styling(&ods, DocumentFormat::Ods)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(back.default_col_px(), 72);
    assert_eq!(back.default_font, None);
    let xlsx = saved(back);
    assert_eq!(
        normal_font(&xlsx),
        "<font><sz val=\"11\"/><name val=\"맑은 고딕\"/></font>"
    );
}

/// Every Normal font keeps its name and size through a save, and the columns keep their pixels: the ones of their own and the
/// default one, whether the file gives `defaultColWidth`, `baseColWidth` or neither.
#[test]
fn the_normal_font_and_the_pixels_of_every_column_survive_a_save() {
    let fonts = [
        ("11", "Calibri", 7),
        ("11", "Arial", 8),
        ("12", "Arial", 9),
        ("10", "Times New Roman", 7),
        ("11", "Cambria", 8),
        ("10", "Verdana", 8),
        ("11", "Courier New", 9),
        ("11", "Malgun Gothic", 8),
        ("12", "맑은 고딕", 9),
        ("10", "맑은 고딕", 7),
        ("11", "Aptos Narrow", 7),
        ("11.5", "Tahoma", 8),
    ];
    let formats = [
        "",
        r#"<sheetFormatPr defaultColWidth="20.7109375" defaultRowHeight="15"/>"#,
        r#"<sheetFormatPr baseColWidth="10" defaultRowHeight="15"/>"#,
        r#"<sheetFormatPr baseColWidth="9" defaultColWidth="12.5"/>"#,
    ];
    let cols = r#"<col min="1" max="1" width="9.140625" customWidth="1"/><col min="3" max="4" width="30" customWidth="1"/><col min="6" max="6" width="1.5" customWidth="1"/>"#;
    for (size, name, digit) in fonts {
        for format in formats {
            let label = format!("{name} {size} {format}");
            let first_file = xlsx_with(&styles_of(size, name), &sheet_of(format, cols));
            let first = styling_of(&first_file);
            assert_eq!(first.digit_px(), digit, "{label}");
            let again_file = saved(first.clone());
            let second = styling_of(&again_file);
            assert_eq!(second.columns, first.columns, "{label}");
            assert_eq!(second.default_col_px(), first.default_col_px(), "{label}");
            assert_eq!(second.default_col_width, first.default_col_width, "{label}");
            assert_eq!(second.default_font, first.default_font, "{label}");
            // The file says the Normal font it was given and not another one.
            let font = normal_font(&again_file);
            let wanted = format!("<name val=\"{name}\"/></font>");
            assert!(font.ends_with(&wanted), "{label}: {font}");
            let wanted_size = format!("<sz val=\"{size}\"/>");
            assert!(font.contains(&wanted_size), "{label}: {font}");
            // A third round does not move anything either.
            assert_eq!(styling_of(&saved(second.clone())), second, "{label}");
        }
    }
}

/// A file with another Normal font than the standard one does not become a Malgun Gothic file.
#[test]
fn a_calibri_file_stays_a_calibri_file() {
    let file = xlsx_with(
        &styles_of("11", "Calibri"),
        &sheet_of(
            r#"<sheetFormatPr defaultColWidth="9.140625"/>"#,
            r#"<col min="2" max="2" width="13.7109375" customWidth="1"/>"#,
        ),
    );
    let read = styling_of(&file);
    assert_eq!(read.default_col_px(), 64);
    assert_eq!(read.columns, BTreeMap::from([(1, 96)]));
    let font = read.default_font.as_ref().unwrap();
    assert_eq!(
        (font.name(), font.size_pt(), font.digit_px()),
        ("Calibri", 11.0, 7)
    );
    let again = saved(read);
    assert_eq!(
        normal_font(&again),
        "<font><sz val=\"11\"/><name val=\"Calibri\"/></font>"
    );
    // The widths are written in the characters of the 7 px digit the file counts in, as they came.
    let sheet = part(&again, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(r#"width="13.7109375" customWidth="1""#),
        "{sheet}"
    );
    assert!(sheet.contains(r#"defaultColWidth="9.140625""#), "{sheet}");
}

/// A cell whose font is the Normal font is "not set", whatever the Normal font is: its size and face do not turn into an
/// own font of the cell that a save would have to write.
#[test]
fn a_cell_in_the_normal_font_keeps_no_font_of_its_own() {
    let sheet = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" s="0" t="inlineStr"><is><t>a</t></is></c><c r="B1" s="1" t="inlineStr"><is><t>b</t></is></c></row></sheetData></worksheet>"#;
    let file = xlsx_with(&styles_of("12", "Arial"), sheet);
    let read = styling_of(&file);
    assert_eq!(read.digit_px(), 9);
    // A1 has the Normal font (nothing to keep); B1 is bold at 14 pt in the same face (the face is the default's).
    assert!(!read.cells.contains_key(&(0, 0)));
    let b1: CellStyle = read.cells[&(0, 1)];
    assert!(
        (b1.bold, b1.size_pt, b1.font) == (true, Some(14.0), 0),
        "{b1:?}"
    );
    assert!(read.fonts.is_empty());
    let styles = part(&saved(read), "xl/styles.xml");
    assert!(
        styles.contains("<font><sz val=\"12\"/><name val=\"Arial\"/></font>"),
        "{styles}"
    );
    assert!(
        styles.contains("<font><b/><sz val=\"14\"/><name val=\"Arial\"/></font>"),
        "{styles}"
    );
}

/// A name or a size a hostile `styles.xml` gives is not a font: the file reads as Calibri 11 pt, and what a save writes is a
/// bounded font again.
#[test]
fn a_hostile_normal_font_reads_as_calibri_and_is_written_bounded() {
    let long = "F".repeat(MAX_FONT_NAME_CHARS + 1);
    let cases = [
        (long.as_str(), "11"),
        ("Bad;Font{}", "11"),
        ("Back\\slash", "11"),
        ("Arial", "NaN"),
        ("Arial", "inf"),
        ("Arial", "-1"),
        ("Arial", "0"),
        ("Arial", "409.5"),
        ("Arial", "1e308"),
        ("", "11"),
    ];
    for (name, size) in cases {
        let file = xlsx_with(&styles_of(size, name), &sheet_of("", ""));
        let read = styling_of(&file);
        let font = read.default_font.as_ref();
        let label = format!("{name:?} {size}");
        let expected_name = if name == "Arial" { "Arial" } else { "Calibri" };
        assert_eq!(font.map(|f| f.name()), Some(expected_name), "{label}");
        assert_eq!(font.map(|f| f.size_pt()), Some(11.0), "{label}");
        let written = normal_font(&saved(read));
        assert!(written.contains("<sz val=\"11\"/>"), "{label}: {written}");
        assert!(
            written.contains(&format!("<name val=\"{expected_name}\"/>")),
            "{label}: {written}"
        );
    }
    // The largest face name and size are fine; one more is not a font.
    let longest = "F".repeat(MAX_FONT_NAME_CHARS);
    assert!(DefaultFont::new(&longest, 409.0).is_some());
    assert!(DefaultFont::new(&format!("{longest}F"), 11.0).is_none());
    assert!(DefaultFont::new("Arial", 409.01).is_none());
    // A size the file writes with two decimals comes back as it went.
    let file = xlsx_with(&styles_of("10.25", "Arial"), &sheet_of("", ""));
    let font = styling_of(&file).default_font.unwrap();
    assert_eq!(font.size_pt(), 10.25);
    assert!(normal_font(&saved(styling_of(&file))).contains("<sz val=\"10.25\"/>"));
}

/// A workbook without `styles.xml` (or without fonts) has the first program's default font: its widths count in 7 px digits.
#[test]
fn a_workbook_without_fonts_counts_in_calibri_digits() {
    let styles = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"></styleSheet>"#;
    let file = xlsx_with(
        styles,
        &sheet_of("", r#"<col min="1" max="1" width="10" customWidth="1"/>"#),
    );
    let read = styling_of(&file);
    assert_eq!(read.columns, BTreeMap::from([(0, 70)]));
    assert_eq!(read.default_col_px(), 64);
    let again = styling_of(&saved(read.clone()));
    assert_eq!(again.columns, read.columns);
    assert_eq!(again.default_col_px(), 64);
}

/// The widths of the columns a file does not give do not depend on the font the file has beyond the digit: an 8 character
/// default column is `8 * digit + 5` pixels up to a multiple of 8.
#[test]
fn the_default_column_follows_the_digit_of_the_font() {
    let bare = sheet_of("", "");
    let px = |size: &str, name: &str| {
        styling_of(&xlsx_with(&styles_of(size, name), &bare)).default_col_px()
    };
    assert_eq!(px("11", "Calibri"), 64);
    assert_eq!(px("11", "맑은 고딕"), 72);
    assert_eq!(px("12", "맑은 고딕"), 80);
    assert_eq!(px("11", "Arial"), 72);
    // A default width a file gives is in the characters of its own digit.
    let given = sheet_of(r#"<sheetFormatPr defaultColWidth="9"/>"#, "");
    let px_given =
        |name: &str| styling_of(&xlsx_with(&styles_of("11", name), &given)).default_col_px();
    assert_eq!((px_given("Calibri"), px_given("맑은 고딕")), (63, 72));
    let kept = styling_of(&xlsx_with(&styles_of("11", "Calibri"), &given)).default_col_width;
    assert_eq!(
        kept,
        Some(DefaultColWidth {
            width: Some(9.0),
            base: None
        })
    );
}
