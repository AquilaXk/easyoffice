//! The page setup of a worksheet (paper, orientation, margins, scale, centring, grid lines and headings, print
//! area, repeated rows and columns) survives XLSX and ODS round trips and hostile files cannot widen it (PD-213).

use easyoffice_formats::{
    CellBox, OdfCodec, OoxmlCodec, PageMargins, PageScale, SheetPage, SheetStyling,
    ZipArchiveCodec, ZipSecurityPolicy, MAX_PRINT_RANGES, MAX_TITLE_ROWS, PAPER_A3,
};

fn full_page() -> SheetPage {
    SheetPage {
        landscape: true,
        paper: Some(PAPER_A3),
        margins: Some(PageMargins {
            left: 500,
            right: 600,
            top: 800,
            bottom: 900,
            header: 250,
            footer: 350,
        }),
        scale: PageScale::Percent(85),
        center_horizontal: true,
        center_vertical: true,
        grid_lines: true,
        headings: true,
        print_area: vec![
            CellBox {
                row0: 0,
                col0: 0,
                row1: 9,
                col1: 3,
            },
            CellBox::cell(14, 5),
        ],
        title_rows: Some((0, 1)),
        title_cols: Some((0, 0)),
    }
}

fn pack(names: &[&str], pages: Vec<SheetPage>) -> Vec<u8> {
    let sheets: Vec<_> = names
        .iter()
        .map(|n| ((*n).to_string(), vec![(0u32, 0u32, "1".to_string())]))
        .collect();
    let stylings: Vec<SheetStyling> = pages
        .into_iter()
        .map(|page| SheetStyling {
            page,
            ..SheetStyling::default()
        })
        .collect();
    OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &stylings).unwrap()
}

fn read(bytes: &[u8]) -> Vec<SheetPage> {
    OoxmlCodec::unpack_xlsx_with_styling(bytes)
        .unwrap()
        .1
        .into_iter()
        .map(|(_, s)| s.page)
        .collect()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    String::from_utf8(
        entries
            .iter()
            .find(|e| e.path == path)
            .unwrap()
            .data
            .clone(),
    )
    .unwrap()
}

fn replace_part(bytes: &[u8], path: &str, xml: &str) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let out: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == path {
                (e.path, xml.as_bytes().to_vec())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&out).unwrap()
}

#[test]
fn a_full_setup_round_trips_through_xlsx_and_a_second_write_is_the_same() {
    let bytes = pack(&["Sheet1"], vec![full_page()]);
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<printOptions horizontalCentered=\"1\" verticalCentered=\"1\" headings=\"1\" gridLines=\"1\"/>"), "{sheet}");
    assert!(sheet.contains("<pageMargins left=\"0.5\" right=\"0.6\" top=\"0.8\" bottom=\"0.9\" header=\"0.25\" footer=\"0.35\"/>"), "{sheet}");
    assert!(
        sheet.contains("<pageSetup paperSize=\"8\" scale=\"85\" orientation=\"landscape\"/>"),
        "{sheet}"
    );
    let book = part(&bytes, "xl/workbook.xml");
    assert!(book.contains("<definedName name=\"_xlnm.Print_Area\" localSheetId=\"0\">Sheet1!$A$1:$D$10,Sheet1!$F$15</definedName>"), "{book}");
    assert!(book.contains("<definedName name=\"_xlnm.Print_Titles\" localSheetId=\"0\">Sheet1!$A:$A,Sheet1!$1:$2</definedName>"), "{book}");
    assert_eq!(read(&bytes), vec![full_page()]);
    assert_eq!(read(&pack(&["Sheet1"], read(&bytes))), vec![full_page()]);
}

#[test]
fn fit_to_pages_is_a_sheet_property_and_zero_means_as_many_as_needed() {
    let page = SheetPage {
        scale: PageScale::Fit {
            width: 1,
            height: 0,
        },
        ..SheetPage::default()
    };
    let bytes = pack(&["Sheet1"], vec![page.clone()]);
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains("<sheetPr><pageSetUpPr fitToPage=\"1\"/></sheetPr>"),
        "{sheet}"
    );
    assert!(
        sheet.contains("<pageSetup fitToWidth=\"1\" fitToHeight=\"0\"/>"),
        "{sheet}"
    );
    assert_eq!(read(&bytes), vec![page]);
}

#[test]
fn each_sheet_keeps_its_own_setup_and_sheet_names_with_quotes_and_spaces_are_quoted() {
    let a = SheetPage {
        print_area: vec![CellBox {
            row0: 1,
            col0: 1,
            row1: 3,
            col1: 3,
        }],
        ..SheetPage::default()
    };
    let b = SheetPage {
        landscape: true,
        print_area: vec![CellBox::cell(0, 0)],
        title_rows: Some((0, 0)),
        ..SheetPage::default()
    };
    let bytes = pack(&["내 시트", "It's"], vec![a.clone(), b.clone()]);
    let book = part(&bytes, "xl/workbook.xml");
    assert!(
        book.contains("localSheetId=\"0\">&apos;내 시트&apos;!$B$2:$D$4<"),
        "{book}"
    );
    assert!(
        book.contains("localSheetId=\"1\">&apos;It&apos;&apos;s&apos;!$A$1<"),
        "{book}"
    );
    assert_eq!(read(&bytes), vec![a, b]);
}

#[test]
fn a_default_setup_writes_nothing() {
    let bytes = pack(&["Sheet1"], vec![SheetPage::default()]);
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        !sheet.contains("pageSetup")
            && !sheet.contains("pageMargins")
            && !sheet.contains("printOptions"),
        "{sheet}"
    );
    assert!(!part(&bytes, "xl/workbook.xml").contains("definedName"));
    assert_eq!(read(&bytes), vec![SheetPage::default()]);
}

#[test]
fn a_file_from_another_program_is_read_with_its_defaults() {
    let bytes = pack(&["Sheet1"], vec![SheetPage::default()]);
    let sheet = "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
        <sheetPr><pageSetUpPr fitToPage=\"true\"/></sheetPr><sheetData/>\
        <pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>\
        <pageSetup paperSize=\"9\" orientation=\"portrait\" fitToHeight=\"3\"/></worksheet>";
    let page = read(&replace_part(&bytes, "xl/worksheets/sheet1.xml", sheet)).remove(0);
    assert_eq!(
        page.scale,
        PageScale::Fit {
            width: 1,
            height: 3
        },
        "fitToWidth defaults to 1"
    );
    assert_eq!(page.margins, Some(PageMargins::NORMAL));
    assert_eq!(page.paper, Some(9));
    assert!(!page.landscape);
}

#[test]
fn hostile_values_are_left_out_not_followed() {
    let bytes = pack(&["Sheet1"], vec![SheetPage::default()]);
    let sheet = "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData/>\
        <pageMargins left=\"-1\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>\
        <pageSetup paperSize=\"60000\" scale=\"5000\" orientation=\"landscape\"/></worksheet>";
    let page = read(&replace_part(&bytes, "xl/worksheets/sheet1.xml", sheet)).remove(0);
    assert_eq!(page.margins, None, "a negative margin drops all of them");
    assert_eq!(page.paper, None);
    assert_eq!(
        page.scale,
        PageScale::Percent(100),
        "a scale out of 10 to 400 is the default"
    );
    assert!(page.landscape);
    let big = "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetPr><pageSetUpPr fitToPage=\"1\"/></sheetPr><sheetData/>\
        <pageSetup fitToWidth=\"60000\" scale=\"30\"/></worksheet>";
    assert_eq!(
        read(&replace_part(&bytes, "xl/worksheets/sheet1.xml", big))
            .remove(0)
            .scale,
        PageScale::Percent(30)
    );
}

#[test]
fn hostile_defined_names_are_bounded() {
    let bytes = pack(&["Sheet1"], vec![SheetPage::default()]);
    let book = part(&bytes, "xl/workbook.xml");
    let many = (0..=MAX_PRINT_RANGES)
        .map(|i| format!("Sheet1!$A${}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let titles_rows = format!("Sheet1!$1:${}", MAX_TITLE_ROWS + 1);
    let titles_cols = format!("Sheet1!$A:${}", "BZ");
    for (names, expect_area, expect_rows, expect_cols) in [
        (format!("<definedNames><definedName name=\"_xlnm.Print_Area\" localSheetId=\"0\">{many}</definedName></definedNames>"), 0usize, None, None),
        (format!("<definedNames><definedName name=\"_xlnm.Print_Titles\" localSheetId=\"0\">{titles_rows},{titles_cols}</definedName></definedNames>"), 0, None, None),
        // a range on another sheet, a missing local sheet, a malformed range and a reversed one
        ("<definedNames><definedName name=\"_xlnm.Print_Area\" localSheetId=\"0\">Other!$A$1:$B$2,Sheet1!$ZZZZ$1,Sheet1!$C$5:$A$1</definedName></definedNames>".to_string(), 1, None, None),
        ("<definedNames><definedName name=\"_xlnm.Print_Area\">Sheet1!$A$1</definedName><definedName name=\"_xlnm.Print_Area\" localSheetId=\"7\">Sheet1!$A$1</definedName></definedNames>".to_string(), 0, None, None),
        ("<definedNames><definedName name=\"_xlnm.Print_Titles\" localSheetId=\"0\">Sheet1!$1:$3,Sheet1!$C:$D</definedName></definedNames>".to_string(), 0, Some((0, 2)), Some((2, 3))),
    ] {
        let patched = book.replace("</sheets>", &format!("</sheets>{names}"));
        let page = read(&replace_part(&bytes, "xl/workbook.xml", &patched)).remove(0);
        assert_eq!(page.print_area.len(), expect_area, "{names}");
        assert_eq!(page.title_rows, expect_rows, "{names}");
        assert_eq!(page.title_cols, expect_cols, "{names}");
    }
}

// ---- ODS

fn pack_ods(names: &[&str], pages: Vec<SheetPage>) -> Vec<u8> {
    let sheets: Vec<_> = names
        .iter()
        .map(|n| ((*n).to_string(), vec![(0u32, 0u32, "1".to_string())]))
        .collect();
    let stylings: Vec<SheetStyling> = pages
        .into_iter()
        .map(|page| SheetStyling {
            page,
            ..SheetStyling::default()
        })
        .collect();
    OdfCodec::pack_ods_multi_styled(&sheets, &stylings).unwrap()
}

fn read_ods(bytes: &[u8]) -> Vec<SheetPage> {
    OdfCodec::unpack_ods_with_styling(bytes)
        .unwrap()
        .1
        .into_iter()
        .map(|(_, s)| s.page)
        .collect()
}

/// What an ODS keeps of a page: all of it, the repeated lines when they start at the first row and column.
fn ods_kept(page: &SheetPage) -> SheetPage {
    page.clone()
}

#[test]
fn an_ods_keeps_the_layout_the_print_ranges_and_gives_each_sheet_its_own() {
    let a = full_page();
    let b = SheetPage {
        scale: PageScale::Fit {
            width: 2,
            height: 0,
        },
        paper: Some(1),
        ..SheetPage::default()
    };
    let c = SheetPage {
        print_area: vec![CellBox {
            row0: 2,
            col0: 2,
            row1: 4,
            col1: 4,
        }],
        ..SheetPage::default()
    };
    let bytes = pack_ods(
        &["Sheet1", "내 시트", "세 번째"],
        vec![a.clone(), b.clone(), c.clone()],
    );
    assert_eq!(read_ods(&bytes), vec![ods_kept(&a), b, c]);
    let content = part(&bytes, "content.xml");
    assert!(
        content.contains("table:print-ranges=\"Sheet1.A1:Sheet1.D10 Sheet1.F15:Sheet1.F15\""),
        "{content}"
    );
    assert!(part(&bytes, "styles.xml").contains("style:print-orientation=\"landscape\""));
    // The second write of what was read is the same file content.
    let again = pack_ods(
        &["Sheet1"],
        read_ods(&pack_ods(&["Sheet1"], vec![a.clone()])),
    );
    assert_eq!(read_ods(&again), vec![ods_kept(&a)]);
}

#[test]
fn a_default_ods_setup_writes_no_styles_part_and_equal_layouts_share_a_master_page() {
    let bytes = pack_ods(&["A"], vec![SheetPage::default()]);
    assert!(
        ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .iter()
            .all(|e| e.path != "styles.xml")
    );
    let landscape = SheetPage {
        landscape: true,
        ..SheetPage::default()
    };
    let bytes = pack_ods(&["A", "B"], vec![landscape.clone(), landscape]);
    assert_eq!(
        part(&bytes, "styles.xml")
            .matches("<style:master-page ")
            .count(),
        1
    );
}

#[test]
fn hostile_ods_values_are_left_out() {
    let bytes = pack_ods(
        &["Sheet1"],
        vec![SheetPage {
            landscape: true,
            ..SheetPage::default()
        }],
    );
    let styles = part(&bytes, "styles.xml")
        .replace("style:scale-to=\"100%\"", "style:scale-to=\"9000%\"")
        .replace("fo:margin-top=\"19.05mm\"", "fo:margin-top=\"99999in\"");
    let patched = replace_part(&bytes, "styles.xml", &styles);
    let page = read_ods(&patched).remove(0);
    assert_eq!(page.scale, PageScale::Percent(100));
    assert_eq!(page.margins, None, "a margin out of bounds drops them all");
    assert!(page.landscape);
    let many = (0..=MAX_PRINT_RANGES)
        .map(|i| format!("Sheet1.A{0}:Sheet1.A{0}", i + 1))
        .collect::<Vec<_>>()
        .join(" ");
    let content = part(&bytes, "content.xml").replace(
        "<table:table table:name=\"Sheet1\"",
        &format!("<table:table table:print-ranges=\"{many}\" table:name=\"Sheet1\""),
    );
    assert!(
        read_ods(&replace_part(&bytes, "content.xml", &content))[0]
            .print_area
            .is_empty(),
        "too many ranges are not read"
    );
    let odd = part(&bytes, "content.xml").replace("<table:table table:name=\"Sheet1\"", "<table:table table:print-ranges=\"Other.A1:Other.B2 junk Sheet1.ZZZZ9:Sheet1.A1\" table:name=\"Sheet1\"");
    assert_eq!(
        read_ods(&replace_part(&bytes, "content.xml", &odd))[0]
            .print_area
            .len(),
        1,
        "a range of another sheet reads by its cells, junk is dropped"
    );
}

#[test]
fn repeated_rows_and_columns_and_the_header_distances_round_trip_in_ods() {
    let page = SheetPage {
        paper: Some(9),
        margins: Some(PageMargins {
            left: 500,
            right: 600,
            top: 800,
            bottom: 900,
            header: 250,
            footer: 350,
        }),
        title_rows: Some((0, 2)),
        title_cols: Some((0, 1)),
        ..SheetPage::default()
    };
    let bytes = pack_ods(&["Sheet1"], vec![page.clone()]);
    let content = part(&bytes, "content.xml");
    assert!(
        content.contains("<table:table-header-rows>")
            && content.contains("<table:table-header-columns>"),
        "{content}"
    );
    assert!(part(&bytes, "styles.xml").contains("eo:header-distance=\"6.35mm\""));
    assert_eq!(read_ods(&bytes), vec![page.clone()]);
    assert_eq!(
        read_ods(&pack_ods(&["Sheet1"], read_ods(&bytes))),
        vec![page]
    );
}

#[test]
fn repeated_lines_that_start_later_are_read_from_a_file_and_over_the_bounds_are_not() {
    let bytes = pack_ods(
        &["Sheet1"],
        vec![SheetPage {
            landscape: true,
            ..SheetPage::default()
        }],
    );
    let content = part(&bytes, "content.xml");
    let with = |rows: &str, cols: &str| {
        content.replace("</table:table>", &format!("{rows}{cols}</table:table>"))
    };
    // rows 4-5 (after three plain rows with a repeat count), columns 3-4
    let foreign = with(
        "<table:table-row table:number-rows-repeated=\"3\"/><table:table-header-rows><table:table-row/><table:table-row/></table:table-header-rows>",
        "",
    );
    let page = read_ods(&replace_part(&bytes, "content.xml", &foreign)).remove(0);
    assert_eq!(
        page.title_rows,
        Some((4, 5)),
        "after the sheet's first row and three more"
    );
    let huge = with("<table:table-header-rows><table:table-row table:number-rows-repeated=\"5000\"/></table:table-header-rows>", "");
    assert_eq!(
        read_ods(&replace_part(&bytes, "content.xml", &huge))
            .remove(0)
            .title_rows,
        None,
        "5000 repeated rows are refused"
    );
    let wide = with("", "<table:table-header-columns><table:table-column table:number-columns-repeated=\"65\"/></table:table-header-columns>");
    assert_eq!(
        read_ods(&replace_part(&bytes, "content.xml", &wide))
            .remove(0)
            .title_cols,
        None
    );
    let groups = with(
        "<table:table-row-group><table:table-row/></table:table-row-group>",
        "",
    );
    assert_eq!(
        read_ods(&replace_part(&bytes, "content.xml", &groups))
            .remove(0)
            .title_rows,
        None,
        "a row group is not a header"
    );
}
