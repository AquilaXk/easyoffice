//! The frozen pane, the tab colour and the row and column groups survive an XLSX round trip and a
//! hostile file cannot widen them (PD-208).

use easyoffice_formats::{
    Freeze, OoxmlCodec, Outline, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
};

fn sample() -> SheetStyling {
    SheetStyling {
        freeze: Freeze::new(2, 1),
        tab_color: Some(0x3366CC),
        hidden_rows: vec![(40, 41)],
        outline: Outline {
            // Rows 3-9 in one group (level 1) with rows 5-6 in a second level and a folded group
            // of its own at rows 20-24; columns C-E form a group that is folded.
            rows: vec![(2, 3, 1), (4, 5, 2), (6, 8, 1), (19, 23, 1)],
            collapsed_rows: vec![(4, 5), (19, 23)],
            cols: vec![(2, 4, 1)],
            collapsed_cols: vec![(2, 4)],
            ..Outline::default()
        },
        ..SheetStyling::default()
    }
}

fn sheets() -> Vec<(String, Vec<(u32, u32, String)>)> {
    vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])]
}

fn pack(styling: SheetStyling) -> Vec<u8> {
    OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[styling]).unwrap()
}

fn read(bytes: &[u8]) -> SheetStyling {
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

/// `bytes` with the worksheet part replaced and, when given, a theme part added.
fn with_sheet(bytes: &[u8], sheet_xml: &str, theme: Option<&str>) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let mut out: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let data = if e.path == "xl/worksheets/sheet1.xml" {
                sheet_xml.as_bytes().to_vec()
            } else {
                e.data
            };
            (e.path, data)
        })
        .collect();
    if let Some(theme) = theme {
        out.push(("xl/theme/theme1.xml".to_string(), theme.as_bytes().to_vec()));
    }
    ZipArchiveCodec::pack(&out).unwrap()
}

#[test]
fn the_view_and_the_groups_come_back_unchanged() {
    let styling = sample();
    let back = read(&pack(styling.clone()));
    assert_eq!(back.freeze, styling.freeze);
    assert_eq!(back.tab_color, styling.tab_color);
    assert_eq!(back.outline, styling.outline);
    // Folded rows are not the rows the user hid.
    assert_eq!(back.hidden_rows, styling.hidden_rows);
}

#[test]
fn the_parts_are_written_in_schema_order_with_levels_and_marks() {
    let bytes = pack(sample());
    let xml = part(&bytes, "xl/worksheets/sheet1.xml");
    let at = |needle: &str| {
        xml.find(needle)
            .unwrap_or_else(|| panic!("{needle} in {xml}"))
    };
    assert!(at("<sheetPr><tabColor rgb=\"FF3366CC\"/></sheetPr>") < at("<sheetViews>"));
    assert!(at("<sheetViews>") < at("<sheetFormatPr defaultRowHeight=\"15\" outlineLevelRow=\"2\" outlineLevelCol=\"1\"/>"));
    assert!(at("<sheetFormatPr") < at("<cols>"));
    assert!(at("<cols>") < at("<sheetData>"));
    assert!(xml.contains(
        "<pane xSplit=\"1\" ySplit=\"2\" topLeftCell=\"B3\" activePane=\"bottomRight\" state=\"frozen\"/>"
    ));
    // The folded inner group of rows 5-6 hides them; row 7 sums them up.
    assert!(
        xml.contains("<row r=\"5\" hidden=\"1\" outlineLevel=\"2\">"),
        "{xml}"
    );
    assert!(
        xml.contains("<row r=\"7\" outlineLevel=\"1\" collapsed=\"1\">"),
        "{xml}"
    );
    // The folded group of rows 20-24 is summed up by row 25, which has no group level.
    assert!(xml.contains("<row r=\"25\" collapsed=\"1\">"), "{xml}");
    assert!(
        xml.contains("<col min=\"3\" max=\"5\" width=\"8.43\" hidden=\"1\" outlineLevel=\"1\"/>")
    );
    assert!(xml.contains("<col min=\"6\" max=\"6\" width=\"8.43\" collapsed=\"1\"/>"));
}

#[test]
fn a_group_without_a_folded_mark_keeps_its_rows_hidden_by_the_user() {
    let xml = r#"<worksheet><sheetData><row r="1" outlineLevel="1" hidden="1"/><row r="2" outlineLevel="1" hidden="1"/><row r="3"/></sheetData></worksheet>"#;
    let back = read(&with_sheet(&pack(SheetStyling::default()), xml, None));
    assert_eq!(back.outline.rows, vec![(0, 1, 1)]);
    assert!(back.outline.collapsed_rows.is_empty());
    assert_eq!(back.hidden_rows, vec![(0, 1)]);
    // With `collapsed` on the summary row the same rows are the group's, not the user's.
    let marked = xml.replace("<row r=\"3\"/>", "<row r=\"3\" collapsed=\"1\"/>");
    let back = read(&with_sheet(&pack(SheetStyling::default()), &marked, None));
    assert_eq!(back.outline.collapsed_rows, vec![(0, 1)]);
    assert!(back.hidden_rows.is_empty());
}

#[test]
fn a_summary_above_the_group_is_kept() {
    let styling = SheetStyling {
        outline: Outline {
            rows: vec![(5, 8, 1)],
            collapsed_rows: vec![(5, 8)],
            summary_above: true,
            ..Outline::default()
        },
        ..SheetStyling::default()
    };
    let bytes = pack(styling.clone());
    let xml = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        xml.contains("<outlinePr summaryBelow=\"0\" summaryRight=\"1\"/>"),
        "{xml}"
    );
    assert!(xml.contains("<row r=\"5\" collapsed=\"1\">"), "{xml}");
    assert_eq!(read(&bytes).outline, styling.outline);
}

#[test]
fn a_theme_tab_colour_is_resolved_with_the_files_theme_and_tint() {
    let xml =
        r#"<worksheet><sheetPr><tabColor theme="4" tint="0.0"/></sheetPr><sheetData/></worksheet>"#;
    let theme = r#"<a:theme><a:themeElements><a:clrScheme name="x"><a:accent1><a:srgbClr val="123456"/></a:accent1></a:clrScheme></a:themeElements></a:theme>"#;
    let bytes = pack(SheetStyling::default());
    assert_eq!(
        read(&with_sheet(&bytes, xml, Some(theme))).tab_color,
        Some(0x123456)
    );
    // Without a theme part the Office palette stands in.
    assert_eq!(
        read(&with_sheet(&bytes, xml, None)).tab_color,
        Some(0x4472C4)
    );
    let darker = xml.replace("tint=\"0.0\"", "tint=\"-0.5\"");
    let dark = read(&with_sheet(&bytes, &darker, None)).tab_color.unwrap();
    assert!(dark != 0x4472C4 && (dark >> 16) < 0x44);
}

#[test]
fn hostile_values_in_a_file_are_ignored() {
    let xml = concat!(
        r#"<worksheet><sheetPr><tabColor rgb="nothex!!"/></sheetPr>"#,
        r#"<sheetViews><sheetView><pane xSplit="1e30" ySplit="99999" state="frozen"/></sheetView></sheetViews>"#,
        r#"<cols><col min="1" max="3" width="9" outlineLevel="8"/><col min="4" max="9999999" width="9" outlineLevel="1"/></cols>"#,
        r#"<sheetData><row r="1" outlineLevel="9"/><row r="2" outlineLevel="255"/><row r="3" outlineLevel="-1"/><row r="4" outlineLevel="x"/></sheetData></worksheet>"#,
    );
    let back = read(&with_sheet(&pack(SheetStyling::default()), xml, None));
    assert_eq!(back.freeze, None);
    assert_eq!(back.tab_color, None);
    assert!(back.outline.rows.is_empty());
    // The one column range is cut to the last column; its level stays within the bound.
    assert!(back
        .outline
        .cols
        .iter()
        .all(|&(_, e, l)| e < 16_384 && (1..=7).contains(&l)));
}

#[test]
fn rows_out_of_order_cannot_grow_the_group_list() {
    let mut xml = String::from("<worksheet><sheetData>");
    for row in (1..=2000).rev() {
        xml.push_str(&format!("<row r=\"{row}\" outlineLevel=\"1\"/>"));
    }
    xml.push_str("</sheetData></worksheet>");
    let back = read(&with_sheet(&pack(SheetStyling::default()), &xml, None));
    // Only the first row of a descending list is in order.
    assert_eq!(back.outline.rows, vec![(1999, 1999, 1)]);
}

#[test]
fn values_over_the_bounds_are_refused_on_write() {
    let sheets = sheets();
    let write =
        |styling: SheetStyling| OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling]);
    let mut deep = sample();
    deep.outline.rows = vec![(0, 3, 8)];
    deep.outline.collapsed_rows.clear();
    assert!(write(deep).is_err());
    let mut stray = sample();
    stray.outline.collapsed_rows = vec![(30, 31)];
    assert!(write(stray).is_err());
    let mut wide = sample();
    wide.freeze = Some(Freeze {
        rows: 5_000,
        cols: 0,
    });
    assert!(write(wide).is_err());
    let mut color = sample();
    color.tab_color = Some(0x1_000000);
    assert!(write(color).is_err());
    assert!(write(sample()).is_ok());
}
