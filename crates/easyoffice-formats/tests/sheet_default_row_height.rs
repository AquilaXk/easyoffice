//! The default row height of a sheet (`sheetFormatPr defaultRowHeight customHeight` in XLSX, the row style of the band that
//! ends an ODS table) survives a round trip, and a file that names an unusable one is read without it (PD-286).

use easyoffice_formats::{
    OdfCodec, OoxmlCodec, RowHeight, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
};

const ODS_ROWS: u32 = 1_048_576;

fn height(points: &str, custom: bool) -> RowHeight {
    RowHeight::parse(points, custom).unwrap()
}

fn with_default(default: RowHeight) -> SheetStyling {
    SheetStyling {
        default_row_height: Some(default),
        row_heights: [(2, height("45", true))].into(),
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

#[test]
fn xlsx_writes_and_reads_the_default_row_height_with_the_custom_flag() {
    let custom = with_default(height("30", true));
    let bytes = xlsx(&custom);
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(r#"<sheetFormatPr defaultRowHeight="30" customHeight="1"/>"#),
        "{sheet}"
    );
    let back = read_xlsx(&bytes);
    assert_eq!(back.default_row_height, custom.default_row_height);
    assert_eq!(back.row_heights, custom.row_heights);
    assert_eq!(
        read_xlsx(&xlsx(&back)).default_row_height,
        back.default_row_height
    );

    // A default that follows the font keeps its value and stays without `customHeight`.
    let auto = with_default(height("14.4", false));
    let bytes = xlsx(&auto);
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(r#"<sheetFormatPr defaultRowHeight="14.4"/>"#),
        "{sheet}"
    );
    assert_eq!(
        read_xlsx(&bytes).default_row_height,
        auto.default_row_height
    );
}

#[test]
fn xlsx_keeps_the_default_beside_the_group_levels() {
    let mut styling = with_default(height("18", true));
    styling.outline.rows.push((1, 2, 2));
    let sheet = part(&xlsx(&styling), "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(
            r#"<sheetFormatPr defaultRowHeight="18" customHeight="1" outlineLevelRow="2"/>"#
        ),
        "{sheet}"
    );
}

#[test]
fn a_sheet_without_a_default_writes_no_format_element() {
    let sheet = part(&xlsx(&SheetStyling::default()), "xl/worksheets/sheet1.xml");
    assert!(!sheet.contains("sheetFormatPr"), "{sheet}");
    assert!(read_xlsx(&xlsx(&SheetStyling::default()))
        .default_row_height
        .is_none());
}

#[test]
fn xlsx_defaults_that_are_not_finite_points_up_to_409_are_left_out() {
    let bytes = xlsx(&SheetStyling::default());
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    let long = "1".repeat(4096);
    let hostile: Vec<String> = [
        "NaN",
        "inf",
        "-5",
        "0",
        "409.5",
        "1e400",
        "abc",
        "",
        long.as_str(),
    ]
    .iter()
    .map(|ht| format!(r#"<sheetFormatPr defaultRowHeight="{ht}" customHeight="1"/>"#))
    .collect();
    for element in hostile {
        let xml = sheet.replace("<sheetData>", &format!("{element}<sheetData>"));
        let back = read_xlsx(&replace_part(&bytes, "xl/worksheets/sheet1.xml", &xml));
        assert!(
            back.default_row_height.is_none(),
            "{}",
            &element[..element.len().min(80)]
        );
    }
    // The boundary values and a `customHeight` that is not a flag.
    for (element, want) in [
        (
            r#"<sheetFormatPr defaultRowHeight="409" customHeight="true"/>"#,
            Some(height("409", true)),
        ),
        (
            r#"<sheetFormatPr defaultRowHeight="0.01" customHeight="yes"/>"#,
            Some(height("0.01", false)),
        ),
    ] {
        let xml = sheet.replace("<sheetData>", &format!("{element}<sheetData>"));
        let back = read_xlsx(&replace_part(&bytes, "xl/worksheets/sheet1.xml", &xml));
        assert_eq!(back.default_row_height, want, "{element}");
    }
}

#[test]
fn a_default_over_the_bounds_is_refused_on_write() {
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "a".to_string())])];
    for centipt in [0, 40_901, u32::MAX] {
        let styling = SheetStyling {
            default_row_height: Some(RowHeight {
                centipt,
                custom: true,
            }),
            ..SheetStyling::default()
        };
        assert!(
            OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(&styling))
                .is_err(),
            "{centipt}"
        );
    }
}

#[test]
fn ods_writes_the_default_on_every_row_and_the_band_that_ends_the_table() {
    // A default that follows the font is not written (PD-300, `sheet_ods_row_heights_pd300.rs`).
    for default in [height("30", true), height("14.4", true)] {
        let styling = with_default(default);
        let bytes = ods(&styling);
        let content = part(&bytes, "content.xml");
        // The default row style is on the band that runs to the last row, and on the rows between the used ones.
        let band = format!(r#"table:number-rows-repeated="{}""#, ODS_ROWS - 3);
        assert!(content.contains(&band), "{content}");
        assert!(
            content.contains(&format!(
                r#"style:row-height="{}pt" style:use-optimal-row-height="{}""#,
                default.points_text(),
                !default.custom
            )),
            "{content}"
        );
        let back = read_ods(&bytes);
        assert_eq!(back.default_row_height, Some(default));
        // The row of its own height stays; the rows that carry the default are not heights of their own.
        assert_eq!(back.row_heights, styling.row_heights);
        assert_eq!(read_ods(&ods(&back)).default_row_height, Some(default));
    }
}

#[test]
fn ods_ignores_a_band_that_is_not_a_usable_default() {
    // One row with a height, so the file has automatic styles to add to.
    let own = SheetStyling {
        row_heights: [(0, height("20", true))].into(),
        ..SheetStyling::default()
    };
    let bytes = ods(&own);
    let content = part(&bytes, "content.xml");
    let style = |name: &str, props: &str| {
        format!(
            r#"<style:style style:name="{name}" style:family="table-row"><style:table-row-properties {props}/></style:style>"#
        )
    };
    let long = format!("{}pt", "9".repeat(2048));
    let styles = [
        style("nan", r#"style:row-height="NaNpt""#),
        style("huge", r#"style:row-height="999pt""#),
        style("neg", r#"style:row-height="-3mm""#),
        style("long", &format!(r#"style:row-height="{long}""#)),
        style("none", r#"style:use-optimal-row-height="true""#),
        style(
            "ok",
            r#"style:row-height="1cm" style:use-optimal-row-height="false""#,
        ),
    ]
    .concat();
    let with = |band: &str| {
        let xml = content
            .replacen(
                "<office:automatic-styles>",
                &format!("<office:automatic-styles>{styles}"),
                1,
            )
            .replacen("</table:table>", &format!("{band}</table:table>"), 1);
        read_ods(&replace_part(&bytes, "content.xml", &xml))
    };
    // The file has one row; a band from the second row to the last row is the default look when its style has a usable height.
    let to_end = |name: &str| {
        format!(
            r#"<table:table-row table:style-name="{name}" table:number-rows-repeated="{}"/>"#,
            ODS_ROWS - 1
        )
    };
    for name in ["nan", "huge", "neg", "long", "none", "missing"] {
        assert!(with(&to_end(name)).default_row_height.is_none(), "{name}");
    }
    assert_eq!(
        with(&to_end("ok")).default_row_height,
        Some(height("28.35", true))
    );
    // A band that stops short of the last row, one that runs past it and one with a count that is not a number are
    // not the default.
    for band in [
        format!(
            r#"<table:table-row table:style-name="ok" table:number-rows-repeated="{}"/>"#,
            ODS_ROWS - 2
        ),
        format!(
            r#"<table:table-row table:style-name="ok" table:number-rows-repeated="{}"/>"#,
            ODS_ROWS
        ),
        r#"<table:table-row table:style-name="ok" table:number-rows-repeated="99999999999999999999"/>"#
            .to_string(),
        r#"<table:table-row table:style-name="ok" table:number-rows-repeated="1025"/>"#.to_string(),
    ] {
        let back = with(&band);
        assert!(back.default_row_height.is_none(), "{band}");
        // Only the file's own row, and the one row a count that is not a number leaves.
        assert!(back.row_heights.len() <= 2, "{band}");
    }
}
