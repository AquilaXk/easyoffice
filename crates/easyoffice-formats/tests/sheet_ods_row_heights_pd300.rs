//! Row heights in ODS (PD-300): rows that carry the sheet's default height do not use up the row-height budget before a
//! row of its own height, and a default height that follows the font (`customHeight` off, what every XLSX read gives) is
//! not written to ODS.

use easyoffice_formats::{
    OdfCodec, OoxmlCodec, RowHeight, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
    MAX_ROW_HEIGHTS,
};

const ODS_ROWS: u32 = 1_048_576;

fn height(points: &str, custom: bool) -> RowHeight {
    RowHeight::parse(points, custom).unwrap()
}

fn ods(styling: &SheetStyling) -> Vec<u8> {
    let sheets = vec![("Data".to_string(), vec![(0, 0, "a".to_string())])];
    OdfCodec::pack_ods_multi_styled(&sheets, std::slice::from_ref(styling)).unwrap()
}

fn read_ods(bytes: &[u8]) -> SheetStyling {
    OdfCodec::unpack_ods_with_styling(bytes)
        .unwrap()
        .1
        .remove(0)
        .1
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

fn replace_part(bytes: &[u8], path: &str, text: &str) -> Vec<u8> {
    let parts: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                let data = if e.path == path {
                    text.as_bytes().to_vec()
                } else {
                    e.data
                };
                (e.path, data)
            })
            .collect();
    ZipArchiveCodec::pack_compressed(&parts).unwrap()
}

/// A file whose first row holds a value (so the table has automatic styles to add to) and whose rows after it are `rows`.
fn with_rows(rows: &str) -> SheetStyling {
    let base = ods(&SheetStyling {
        row_heights: [(0, height("20", true))].into(),
        ..SheetStyling::default()
    });
    let content = part(&base, "content.xml");
    let styles = [("dflt", "20pt"), ("own", "45pt"), ("other", "33pt")]
        .map(|(name, pt)| {
            format!(
                r#"<style:style style:name="{name}" style:family="table-row"><style:table-row-properties style:row-height="{pt}" style:use-optimal-row-height="false"/></style:style>"#
            )
        })
        .concat();
    let xml = content
        .replacen(
            "<office:automatic-styles>",
            &format!("<office:automatic-styles>{styles}"),
            1,
        )
        .replacen("</table:table>", &format!("{rows}</table:table>"), 1);
    read_ods(&replace_part(&base, "content.xml", &xml))
}

fn block(style: &str, count: u32) -> String {
    format!(r#"<table:table-row table:style-name="{style}" table:number-rows-repeated="{count}"/>"#)
}

#[test]
fn rows_with_the_default_height_do_not_use_up_the_budget_before_a_row_of_its_own_height() {
    // Row 0 and 109,999 rows after it have the default height (more than the budget, in blocks that fill it exactly), then
    // one row of 45 pt, then the band to the last row.
    let first_free = 110_000;
    let rows = format!(
        "{}{}{}{}{}",
        (0..99).map(|_| block("dflt", 1000)).collect::<String>(),
        block("dflt", 999),
        (0..10).map(|_| block("dflt", 1000)).collect::<String>(),
        block("own", 1),
        block("dflt", ODS_ROWS - first_free - 1),
    );
    let styling = with_rows(&rows);
    assert_eq!(styling.default_row_height, Some(height("20", true)));
    assert_eq!(
        styling.row_heights.get(&first_free),
        Some(&height("45", true)),
        "the row after the 110,000 default rows keeps its height"
    );
    // Only the file's own row and that one (the first row has the default height, too, and is not a height of its own).
    assert_eq!(styling.row_heights.len(), 1, "{:?}", styling.row_heights);
}

#[test]
fn a_height_between_two_default_stretches_keeps_each_row_it_names() {
    let rows = format!(
        "{}{}{}{}{}",
        block("dflt", 1000),
        block("own", 3),
        block("dflt", 1000),
        block("other", 2),
        block("dflt", ODS_ROWS - 1 - 2005),
    );
    let styling = with_rows(&rows);
    assert_eq!(styling.default_row_height, Some(height("20", true)));
    let own = height("45", true);
    let other = height("33", true);
    let want: Vec<(u32, RowHeight)> = [1001, 1002, 1003, 2004, 2005]
        .into_iter()
        .zip([own, own, own, other, other])
        .collect();
    let got: Vec<(u32, RowHeight)> = styling.row_heights.iter().map(|(&r, &h)| (r, h)).collect();
    assert_eq!(got, want);
}

#[test]
fn hostile_alternating_heights_stay_inside_the_budget() {
    // 300,000 rows that alternate between the default and another height: far more runs than the budget allows.
    let pair = format!("{}{}", block("dflt", 1), block("own", 1));
    let rows = format!(
        "{}{}",
        pair.repeat(150_000),
        block("dflt", ODS_ROWS - 1 - 300_000)
    );
    let styling = with_rows(&rows);
    assert_eq!(styling.default_row_height, Some(height("20", true)));
    assert!(styling.row_heights.len() <= MAX_ROW_HEIGHTS);
    assert!(
        styling
            .row_heights
            .values()
            .all(|&h| h == height("45", true)),
        "a row of the default height is never a height of its own"
    );
    assert!(!styling.row_heights.is_empty());
}

#[test]
fn rows_of_one_height_without_a_default_still_stop_at_the_budget() {
    // No band to the last row: no default, so every row is a height of its own, up to the budget.
    let rows = (0..101).map(|_| block("own", 1000)).collect::<String>();
    let styling = with_rows(&rows);
    assert!(styling.default_row_height.is_none());
    assert!(styling.row_heights.len() <= MAX_ROW_HEIGHTS);
    assert!(styling.row_heights.len() + 1000 > MAX_ROW_HEIGHTS);
    assert_eq!(styling.row_heights.keys().next(), Some(&0));
}

fn xlsx_read_styling() -> SheetStyling {
    // What an XLSX read gives: the default row height follows the font (`customHeight` off).
    let sheet = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetFormatPr defaultRowHeight="15"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>a</t></is></c></row><row r="3" ht="40" customHeight="1"><c r="A3" t="inlineStr"><is><t>b</t></is></c></row></sheetData></worksheet>"#;
    let base = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Sheet1".to_string(), vec![(0, 0, "a".to_string())])],
        &[],
        &[SheetStyling::default()],
    )
    .unwrap();
    let bytes = replace_part(&base, "xl/worksheets/sheet1.xml", sheet);
    OoxmlCodec::unpack_xlsx_with_styling(&bytes)
        .unwrap()
        .1
        .remove(0)
        .1
}

#[test]
fn a_default_that_follows_the_font_is_not_written_to_ods() {
    let styling = xlsx_read_styling();
    assert_eq!(styling.default_row_height, Some(height("15", false)));
    let bytes = ods(&styling);
    let content = part(&bytes, "content.xml");
    let band = format!(r#"table:number-rows-repeated="{}""#, ODS_ROWS - 3);
    assert!(!content.contains(&band), "no band that ends the table");
    assert!(
        !content.contains(r#"style:row-height="15pt""#),
        "no row style for the default"
    );
    let back = read_ods(&bytes);
    assert!(back.default_row_height.is_none());
    assert_eq!(
        back.row_heights, styling.row_heights,
        "the row of 40 pt stays"
    );
}

#[test]
fn a_default_a_user_set_is_still_written_to_ods() {
    let styling = SheetStyling {
        default_row_height: Some(height("30", true)),
        row_heights: [(2, height("45", true))].into(),
        ..SheetStyling::default()
    };
    let content = part(&ods(&styling), "content.xml");
    assert!(content.contains(&format!(r#"table:number-rows-repeated="{}""#, ODS_ROWS - 3)));
    assert_eq!(
        read_ods(&ods(&styling)).default_row_height,
        styling.default_row_height
    );
}

#[test]
fn an_xlsx_round_trip_keeps_a_default_that_follows_the_font() {
    let styling = xlsx_read_styling();
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Sheet1".to_string(), vec![(0, 0, "a".to_string())])],
        &[],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let back = OoxmlCodec::unpack_xlsx_with_styling(&bytes)
        .unwrap()
        .1
        .remove(0)
        .1;
    assert_eq!(back.default_row_height, styling.default_row_height);
    assert_eq!(back.row_heights, styling.row_heights);
}
