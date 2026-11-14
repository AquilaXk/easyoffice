//! The frozen pane, the tab colour and the row and column groups survive an XLSX round trip and a
//! hostile file cannot widen them (PD-208).

use easyoffice_formats::{
    Freeze, OdfCodec, OoxmlCodec, Outline, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
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

type Sheets = Vec<(String, Vec<(u32, u32, String)>)>;

fn sheets() -> Sheets {
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

// ---- ODS -------------------------------------------------------------------------------------------

fn pack_ods(styling: SheetStyling) -> Vec<u8> {
    OdfCodec::pack_ods_multi_styled(&sheets(), &[styling]).unwrap()
}

fn read_ods(bytes: &[u8]) -> SheetStyling {
    OdfCodec::unpack_ods_with_styling(bytes)
        .unwrap()
        .1
        .remove(0)
        .1
}

fn ods_content(bytes: &[u8]) -> String {
    part(bytes, "content.xml")
}

#[test]
fn the_view_and_the_groups_come_back_from_an_ods_file() {
    let styling = sample();
    let bytes = pack_ods(styling.clone());
    let back = read_ods(&bytes);
    assert_eq!(back.freeze, styling.freeze);
    assert_eq!(back.tab_color, styling.tab_color);
    assert_eq!(back.outline, styling.outline);
    let xml = ods_content(&bytes);
    assert!(xml.contains("tableooo:tab-color=\"#3366cc\""), "{xml}");
    assert!(xml.contains("<table:table table:name=\"Sheet1\" table:style-name=\"ta1\">"));
    // The inner group of rows 5-6 and the group of rows 20-24 are folded, the outer one is not.
    assert!(
        xml.contains("<table:table-row-group table:display=\"true\">"),
        "{xml}"
    );
    assert_eq!(
        xml.matches("<table:table-row-group table:display=\"false\">")
            .count(),
        2
    );
    assert!(xml.contains("<table:table-column-group table:display=\"false\">"));
    let settings = part(&bytes, "settings.xml");
    assert!(
        settings.contains("HorizontalSplitMode\" config:type=\"short\">2<"),
        "{settings}"
    );
    assert!(part(&bytes, "META-INF/manifest.xml").contains("settings.xml"));
}

#[test]
fn an_ods_file_without_a_view_has_no_settings_part_and_reads_none() {
    let bytes = pack_ods(SheetStyling::default());
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    assert!(entries.iter().all(|e| e.path != "settings.xml"));
    let back = read_ods(&bytes);
    assert!(back.freeze.is_none() && back.tab_color.is_none() && back.outline.is_empty());
}

/// Namespace of the table extension attributes, in two parts (see `ods_view.rs`).
const TABLEOOO: &str = concat!("http://open", "office.org/2009/table");

/// `content.xml` of an ODS file replaced by `table_xml` inside one table.
fn ods_with_table(table_attrs: &str, table_xml: &str, styles: &str) -> Vec<u8> {
    let bytes = pack_ods(SheetStyling::default());
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let content = format!(
        "<?xml version=\"1.0\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:tableooo=\"{TABLEOOO}\"><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:spreadsheet><table:table table:name=\"S\" {table_attrs}>{table_xml}</table:table></office:spreadsheet></office:body></office:document-content>"
    );
    let parts: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let data = if e.path == "content.xml" {
                content.clone().into_bytes()
            } else {
                e.data
            };
            (e.path, data)
        })
        .collect();
    ZipArchiveCodec::pack(&parts).unwrap()
}

#[test]
fn hostile_ods_groups_colours_and_splits_are_bounded() {
    // Nested to depth 9: only the first seven levels count.
    let mut deep = String::new();
    for _ in 0..9 {
        deep.push_str("<table:table-row-group>");
    }
    deep.push_str("<table:table-row/>");
    for _ in 0..9 {
        deep.push_str("</table:table-row-group>");
    }
    let back = read_ods(&ods_with_table("", &deep, ""));
    assert!(back.outline.rows.is_empty(), "{:?}", back.outline);
    // A group of a million repeated rows is over the bound and is not read.
    let big = "<table:table-row-group table:display=\"false\"><table:table-row table:number-rows-repeated=\"1000000\"/></table:table-row-group>";
    let back = read_ods(&ods_with_table("", big, ""));
    assert!(back.outline.rows.is_empty() && back.outline.collapsed_rows.is_empty());
    // A folded group is folded; a group inside a folded one is folded too.
    let nested = "<table:table-row-group table:display=\"false\"><table:table-row/><table:table-row-group><table:table-row/></table:table-row-group></table:table-row-group>";
    let back = read_ods(&ods_with_table("", nested, ""));
    assert_eq!(back.outline.rows, vec![(0, 0, 1), (1, 1, 2)]);
    assert_eq!(back.outline.collapsed_rows, vec![(0, 1)]);
    // A bad colour and a table style that is no colour are ignored.
    let style = "<style:style style:name=\"ta1\" style:family=\"table\"><style:table-properties tableooo:tab-color=\"#12\"/></style:style>";
    let back = read_ods(&ods_with_table(
        "table:style-name=\"ta1\"",
        "<table:table-row/>",
        style,
    ));
    assert_eq!(back.tab_color, None);
    let good = style.replace("#12", "#00AaFf");
    let back = read_ods(&ods_with_table(
        "table:style-name=\"ta1\"",
        "<table:table-row/>",
        &good,
    ));
    assert_eq!(back.tab_color, Some(0x00AAFF));
    // Columns in a group.
    let cols = "<table:table-column-group table:display=\"false\"><table:table-column table:number-columns-repeated=\"2\"/></table:table-column-group><table:table-row/>";
    let back = read_ods(&ods_with_table("", cols, ""));
    assert_eq!(back.outline.cols, vec![(0, 1, 1)]);
    assert_eq!(back.outline.collapsed_cols, vec![(0, 1)]);
}

#[test]
fn hostile_ods_splits_leave_nothing_frozen_and_a_bad_pane_is_refused_on_write() {
    let bytes = pack_ods(SheetStyling::default());
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let settings = |rows: &str, mode: &str| {
        format!(
            "<office:document-settings xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:config=\"urn:oasis:names:tc:opendocument:xmlns:config:1.0\"><office:settings><config:config-item-set config:name=\"ooo:view-settings\"><config:config-item-map-indexed config:name=\"Views\"><config:config-item-map-entry><config:config-item-map-named config:name=\"Tables\"><config:config-item-map-entry config:name=\"Sheet1\"><config:config-item config:name=\"VerticalSplitMode\" config:type=\"short\">{mode}</config:config-item><config:config-item config:name=\"VerticalSplitPosition\" config:type=\"int\">{rows}</config:config-item></config:config-item-map-entry></config:config-item-map-named></config:config-item-map-entry></config:config-item-map-indexed></config:config-item-set></office:settings></office:document-settings>"
        )
    };
    let with = |text: String| {
        let mut parts: Vec<(String, Vec<u8>)> = entries
            .iter()
            .map(|e| (e.path.clone(), e.data.clone()))
            .collect();
        parts.push(("settings.xml".into(), text.into_bytes()));
        read_ods(&ZipArchiveCodec::pack(&parts).unwrap())
    };
    assert_eq!(with(settings("3", "2")).freeze, Freeze::new(3, 0));
    for (rows, mode) in [
        ("3", "1"),
        ("1001", "2"),
        ("-4", "2"),
        ("99999999999", "2"),
        ("x", "2"),
    ] {
        assert_eq!(with(settings(rows, mode)).freeze, None, "{rows} {mode}");
    }
    let mut bad = sample();
    bad.outline.rows = vec![(0, 3, 9)];
    bad.outline.collapsed_rows.clear();
    assert!(OdfCodec::pack_ods_multi_styled(&sheets(), &[bad]).is_err());
}

/// Rows 3-9 are the outer group (level 1) and rows 5-6 an inner one (level 2); columns B-D and C the same.
/// `collapsed` is the outer stretch (2, 8), the inner one (4, 5), both or neither; `open_inner` leaves the inner group
/// open inside a folded outer one.
fn nested(collapsed: &[(u32, u32)], open_inner: bool) -> SheetStyling {
    let cols = |&(a, b): &(u32, u32)| if (a, b) == (4, 5) { (2, 2) } else { (1, 3) };
    SheetStyling {
        outline: Outline {
            rows: vec![(2, 3, 1), (4, 5, 2), (6, 8, 1)],
            cols: vec![(1, 1, 1), (2, 2, 2), (3, 3, 1)],
            collapsed_rows: collapsed.to_vec(),
            collapsed_cols: collapsed.iter().map(cols).collect(),
            open_rows: if open_inner { vec![(4, 5, 2)] } else { vec![] },
            open_cols: if open_inner { vec![(2, 2, 2)] } else { vec![] },
            ..Outline::default()
        },
        ..SheetStyling::default()
    }
}

#[test]
fn an_inner_group_keeps_its_own_fold_when_the_outer_group_folds_and_opens_again() {
    let cases = [
        ("outer open, inner folded", nested(&[(4, 5)], false)),
        ("outer folded, inner folded", nested(&[(2, 8)], false)),
        ("outer folded, inner open", nested(&[(2, 8)], true)),
        ("nothing folded", nested(&[], false)),
    ];
    for (what, case) in &cases {
        assert!(case.view_within_bounds(1_048_576, 16_384), "{what}");
        assert_eq!(
            read(&pack(case.clone())).outline,
            case.outline,
            "xlsx, {what}"
        );
        assert_eq!(
            read_ods(&pack_ods(case.clone())).outline,
            case.outline,
            "ods, {what}"
        );
    }
    // The file marks a group on its summary row only when the group is folded: the outer summary (row 10), and the inner
    // one (row 7) only when it is folded too.
    let marks = |case: &SheetStyling| {
        let xml = part(&pack(case.clone()), "xl/worksheets/sheet1.xml");
        ["<row r=\"7\"", "<row r=\"10\""].map(|row| {
            xml.find(row).is_some_and(|at| {
                xml[at..]
                    .split('>')
                    .next()
                    .unwrap()
                    .contains("collapsed=\"1\"")
            })
        })
    };
    assert_eq!(marks(&cases[1].1), [true, true]);
    assert_eq!(marks(&cases[2].1), [false, true]);
    assert_eq!(marks(&cases[0].1), [true, false]);
    // In ODS the open inner group is written open inside the folded outer one.
    let content = ods_content(&pack_ods(cases[2].1.clone()));
    assert!(
        content.contains("<table:table-row-group table:display=\"false\">")
            && content.contains("<table:table-row-group table:display=\"true\">"),
        "{content}"
    );
}

#[test]
fn open_groups_that_name_no_group_or_lie_outside_the_folded_rows_are_refused_on_write() {
    let write =
        |styling: SheetStyling| OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[styling]);
    for open in [
        // Not a whole stretch of its level, outside the folded rows, the wrong level, unsorted, repeated.
        vec![(4, 4, 2)],
        vec![(6, 8, 1)],
        vec![(4, 5, 1)],
        vec![(4, 5, 2), (2, 8, 1)],
        vec![(4, 5, 2), (4, 5, 2)],
    ] {
        let mut bad = nested(&[(2, 5)], false);
        bad.outline.open_rows = open.clone();
        assert!(write(bad.clone()).is_err(), "{open:?}");
        assert!(pack_ods_result(bad).is_err(), "{open:?}");
    }
    // The same rows as the folded ones are fine.
    let mut fine = nested(&[(2, 8)], false);
    fine.outline.open_rows = vec![(4, 5, 2)];
    assert!(write(fine).is_ok());
}

fn pack_ods_result(styling: SheetStyling) -> Result<Vec<u8>, easyoffice_formats::EasyOfficeError> {
    OdfCodec::pack_ods_multi_styled(&sheets(), &[styling])
}

#[test]
fn a_file_with_a_hostile_number_of_open_groups_stays_bounded() {
    // 30,000 one-row groups of level 2 inside one folded group, none of them marked: every one is an open group.
    let mut rows = vec![(0, 0, 1)];
    rows.extend((0..30_000u32).map(|i| (1 + i * 2, 1 + i * 2, 2)));
    let xml = {
        let mut x = String::from("<worksheet><sheetData>");
        for (first, _, level) in &rows {
            x.push_str(&format!(
                "<row r=\"{}\" hidden=\"1\" outlineLevel=\"{level}\"/>",
                first + 1
            ));
        }
        x.push_str("</sheetData></worksheet>");
        x
    };
    let started = std::time::Instant::now();
    let back = read(&with_sheet(&pack(SheetStyling::default()), &xml, None));
    assert!(back.outline.open_rows.len() <= 100_000);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}
