//! Row heights and the `customHeight` flag survive an XLSX and an ODS round trip, and a file that names an unusable
//! height or too many of them is read without them (PD-278).

use easyoffice_formats::{
    OdfCodec, OoxmlCodec, RowHeight, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
    MAX_ROW_HEIGHTS,
};

fn height(points: &str, custom: bool) -> RowHeight {
    RowHeight::parse(points, custom).unwrap()
}

fn sample() -> SheetStyling {
    SheetStyling {
        row_heights: [
            (0, height("30", true)),
            (2, height("14.4", true)),
            (4, height("409", true)),
            (7, height("21.75", false)),
        ]
        .into(),
        ..SheetStyling::default()
    }
}

fn xlsx(styling: &SheetStyling) -> Vec<u8> {
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "a".to_string())])];
    OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(styling)).unwrap()
}

fn read_xlsx(bytes: &[u8]) -> SheetStyling {
    OoxmlCodec::unpack_xlsx_with_styling(bytes)
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

#[test]
fn xlsx_row_heights_round_trip_with_the_custom_flag() {
    let bytes = xlsx(&sample());
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(r#"<row r="1" ht="30" customHeight="1">"#),
        "{sheet}"
    );
    assert!(
        sheet.contains(r#"<row r="3" ht="14.4" customHeight="1">"#),
        "{sheet}"
    );
    // A height that follows the content is written without `customHeight`, in a row that has no cell.
    assert!(sheet.contains(r#"<row r="8" ht="21.75">"#), "{sheet}");
    let back = read_xlsx(&bytes);
    assert_eq!(back.row_heights, sample().row_heights);
    assert_eq!(read_xlsx(&xlsx(&back)).row_heights, back.row_heights);
}

#[test]
fn a_sheet_without_heights_writes_no_height_attribute() {
    let sheet = part(&xlsx(&SheetStyling::default()), "xl/worksheets/sheet1.xml");
    assert!(
        !sheet.contains(" ht=")
            && !sheet.contains("customHeight")
            && sheet.contains(r#"<sheetFormatPr defaultRowHeight="16.5"/>"#),
        "{sheet}"
    );
}

#[test]
fn xlsx_heights_that_are_not_finite_points_up_to_409_are_left_out() {
    let bytes = xlsx(&SheetStyling::default());
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    let rows = [
        ("1", "NaN"),
        ("2", "inf"),
        ("3", "-5"),
        ("4", "0"),
        ("5", "409.5"),
        ("6", "1e400"),
        ("7", "abc"),
        ("8", "20"),
    ]
    .map(|(r, ht)| format!(r#"<row r="{r}" ht="{ht}" customHeight="1"/>"#))
    .concat();
    let hostile = sheet
        .replace("<sheetData>", &format!("<sheetData>{rows}"))
        .replace(r#"<row r="1">"#, r#"<row r="9">"#);
    let back = read_xlsx(&replace_part(&bytes, "xl/worksheets/sheet1.xml", &hostile));
    assert_eq!(back.row_heights, [(7, height("20", true))].into());
}

#[test]
fn xlsx_stops_at_the_height_cap_without_failing_the_file() {
    let bytes = xlsx(&SheetStyling::default());
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    let rows: String = (2..=(MAX_ROW_HEIGHTS as u32 + 6))
        .map(|r| format!(r#"<row r="{r}" ht="20" customHeight="1"/>"#))
        .collect();
    let flood = sheet.replace("</sheetData>", &format!("{rows}</sheetData>"));
    let start = std::time::Instant::now();
    let back = read_xlsx(&replace_part(&bytes, "xl/worksheets/sheet1.xml", &flood));
    assert_eq!(back.row_heights.len(), MAX_ROW_HEIGHTS);
    assert!(start.elapsed().as_secs() < 10);
}

#[test]
fn heights_over_the_bounds_are_refused_on_write() {
    let tall = SheetStyling {
        row_heights: [(
            0,
            RowHeight {
                centipt: 40_901,
                custom: true,
            },
        )]
        .into(),
        ..SheetStyling::default()
    };
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "a".to_string())])];
    assert!(OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(&tall)).is_err());
    let off_sheet = SheetStyling {
        row_heights: [(1_048_576, height("20", true))].into(),
        ..SheetStyling::default()
    };
    assert!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(&off_sheet)).is_err()
    );
    let many = SheetStyling {
        row_heights: (0..=MAX_ROW_HEIGHTS as u32)
            .map(|r| (r, height("20", true)))
            .collect(),
        ..SheetStyling::default()
    };
    assert!(OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(&many)).is_err());
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

#[test]
fn ods_row_heights_round_trip_for_the_rows_a_user_set() {
    let bytes = ods(&sample());
    let content = part(&bytes, "content.xml");
    assert!(
        content.contains(r#"style:row-height="30pt" style:use-optimal-row-height="false""#),
        "{content}"
    );
    assert!(
        content.contains(r#"style:row-height="21.75pt" style:use-optimal-row-height="true""#),
        "{content}"
    );
    let back = read_ods(&bytes);
    // The row that follows its content has no height of its own in ODS: the program that reads it measures the row.
    let custom: Vec<_> = sample()
        .row_heights
        .into_iter()
        .filter(|(_, h)| h.custom)
        .collect();
    assert_eq!(back.row_heights, custom.into_iter().collect());
}

#[test]
fn ods_ignores_bad_lengths_and_repeated_bands() {
    let bytes = ods(&sample());
    let content = part(&bytes, "content.xml");
    let styles = concat!(
        r#"<style:style style:name="bad1" style:family="table-row"><style:table-row-properties style:row-height="NaNpt" style:use-optimal-row-height="false"/></style:style>"#,
        r#"<style:style style:name="bad2" style:family="table-row"><style:table-row-properties style:row-height="999pt" style:use-optimal-row-height="false"/></style:style>"#,
        r#"<style:style style:name="bad3" style:family="table-row"><style:table-row-properties style:row-height="-3mm" style:use-optimal-row-height="false"/></style:style>"#,
        r#"<style:style style:name="ok" style:family="table-row"><style:table-row-properties style:row-height="1cm" style:use-optimal-row-height="false"/></style:style>"#,
    );
    let rows = concat!(
        r#"<table:table-row table:style-name="bad1"/><table:table-row table:style-name="bad2"/>"#,
        r#"<table:table-row table:style-name="bad3"/><table:table-row table:style-name="ok"/>"#,
        r#"<table:table-row table:style-name="ok" table:number-rows-repeated="1048000"/>"#,
        r#"<table:table-row table:style-name="ok" table:number-rows-repeated="2"/>"#,
    );
    let hostile = content
        .replacen(
            "<office:automatic-styles>",
            &format!("<office:automatic-styles>{styles}"),
            1,
        )
        .replacen("</table:table>", &format!("{rows}</table:table>"), 1);
    let back = read_ods(&replace_part(&bytes, "content.xml", &hostile));
    // The sample's own rows (0, 2, 4), the one good row after the three bad ones (11) and the two-row band at the
    // end (1,048,012 and 1,048,013). The band of 1,048,000 rows is a default look, not a height, so it is left out.
    let rows: Vec<u32> = back.row_heights.keys().copied().collect();
    assert_eq!(rows, vec![0, 2, 4, 11, 1_048_012, 1_048_013]);
    assert!(back.row_heights.values().all(|h| h.custom));
    assert_eq!(back.row_heights[&11].centipt, 2_835);
}
