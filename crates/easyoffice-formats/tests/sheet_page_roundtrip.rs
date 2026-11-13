//! The page setup of a worksheet (paper, orientation, margins, scale, centring, grid lines and headings, print
//! area, repeated rows and columns) survives XLSX and ODS round trips and hostile files cannot widen it (PD-213).

use easyoffice_formats::{
    CellBox, OoxmlCodec, PageMargins, PageScale, SheetPage, SheetStyling, ZipArchiveCodec,
    ZipSecurityPolicy, MAX_PRINT_RANGES, MAX_TITLE_ROWS, PAPER_A3,
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
