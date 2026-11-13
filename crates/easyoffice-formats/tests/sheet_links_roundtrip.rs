//! Cell hyperlinks survive XLSX and ODS writing and reading, and a hostile file's links are turned away
//! (PD-218).

use easyoffice_formats::{
    CellLink, EasyOfficeError, OdfCodec, OdsUnpacked, OoxmlCodec, SheetLinks, SheetStyling,
    ZipArchiveCodec, ZipSecurityPolicy, MAX_LINKS_PER_SHEET,
};

fn sheets() -> Vec<OdsUnpacked> {
    vec![
        (
            "Sheet1".to_string(),
            vec![
                (0, 0, "홈페이지".to_string()),
                (1, 0, "둘째 시트로".to_string()),
                (2, 0, "메일".to_string()),
                (3, 0, "합계 이름".to_string()),
            ],
        ),
        ("두 번째".to_string(), vec![(0, 0, "x".to_string())]),
    ]
}

fn links() -> SheetLinks {
    let mut links = SheetLinks::new();
    links.insert(
        (0, 0),
        CellLink::new("https://example.com/a?b=1&c=2", "열기 \"홈\"").unwrap(),
    );
    links.insert((1, 0), CellLink::new("#'두 번째'!A1", "").unwrap());
    links.insert((2, 0), CellLink::new("mailto:me@example.com", "").unwrap());
    links.insert((3, 0), CellLink::new("#Total", "").unwrap());
    links
}

fn stylings() -> Vec<SheetStyling> {
    vec![
        SheetStyling {
            links: links(),
            ..SheetStyling::default()
        },
        SheetStyling::default(),
    ]
}

fn entry(bytes: &[u8], path: &str) -> Option<String> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8(e.data).unwrap())
}

#[test]
fn xlsx_links_round_trip_with_their_relationships() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &stylings()).unwrap();
    let sheet = entry(&bytes, "xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains("<hyperlinks") && sheet.contains("location="));
    let rels = entry(&bytes, "xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    assert_eq!(rels.matches("TargetMode=\"External\"").count(), 2);
    assert!(entry(&bytes, "xl/worksheets/_rels/sheet2.xml.rels").is_none());
    let back = OoxmlCodec::unpack_xlsx_styling(&bytes).unwrap();
    assert_eq!(back[0].1.links, links());
    assert_eq!(back[0].1.links_rejected, 0);
    assert!(back[1].1.links.is_empty());
    // The writer sends the links in cell order, one hyperlink element per cell, before the page setup.
    assert!(sheet.find("<hyperlinks").unwrap() > sheet.find("</sheetData>").unwrap());
}

#[test]
fn a_foreign_file_with_unsafe_addresses_loses_only_those_links() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &stylings()).unwrap();
    let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    for e in &mut entries {
        if e.path == "xl/worksheets/_rels/sheet1.xml.rels" {
            let text = String::from_utf8(e.data.clone()).unwrap();
            e.data = text
                .replace("https://example.com/a?b=1&amp;c=2", "javascript:alert(1)")
                .into_bytes();
        }
    }
    let hostile = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    let back = OoxmlCodec::unpack_xlsx_styling(&hostile).unwrap();
    assert_eq!(back[0].1.links.len(), 3);
    assert!(!back[0].1.links.contains_key(&(0, 0)));
    assert_eq!(back[0].1.links_rejected, 1);
}

#[test]
fn a_hostile_link_count_is_cut_at_the_bound() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[]).unwrap();
    let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let mut block = String::from("<hyperlinks>");
    for i in 0..MAX_LINKS_PER_SHEET + 50 {
        block.push_str(&format!(
            "<hyperlink ref=\"A{}\" location=\"Sheet1!B{}\"/>",
            i % 1_000_000 + 1,
            i % 1_000_000 + 1
        ));
    }
    block.push_str("</hyperlinks>");
    for e in &mut entries {
        if e.path == "xl/worksheets/sheet1.xml" {
            let text = String::from_utf8(e.data.clone()).unwrap();
            e.data = text
                .replace("</worksheet>", &format!("{block}</worksheet>"))
                .into_bytes();
        }
    }
    let hostile = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    let back = OoxmlCodec::unpack_xlsx_styling(&hostile).unwrap();
    assert_eq!(back[0].1.links.len(), MAX_LINKS_PER_SHEET);
    assert_eq!(back[0].1.links_rejected, 50);
}

#[test]
fn an_xlsx_link_outside_the_grid_is_refused_on_writing() {
    let mut styling = SheetStyling::default();
    styling
        .links
        .insert((1_048_576, 0), CellLink::new("https://a.b", "").unwrap());
    let result = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[styling]);
    assert!(matches!(
        result,
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn ods_links_round_trip_and_the_workbook_location_uses_a_dot() {
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets(), &stylings()).unwrap();
    let content = entry(&bytes, "content.xml").unwrap();
    assert!(content.contains("xlink:href=\"https://example.com/a?b=1&amp;c=2\""));
    assert!(content.contains("xlink:href=\"#&apos;두 번째&apos;.A1\""));
    assert!(content.contains("office:title=\"열기 &quot;홈&quot;\""));
    let (_, back) = OdfCodec::unpack_ods_with_styling(&bytes).unwrap();
    assert_eq!(back[0].1.links, links());
    assert!(back[1].1.links.is_empty());
}

#[test]
fn ods_unsafe_and_banded_links_are_turned_away() {
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets(), &stylings()).unwrap();
    let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    for e in &mut entries {
        if e.path == "content.xml" {
            let text = String::from_utf8(e.data.clone()).unwrap();
            e.data = text
                .replace(
                    "xlink:href=\"mailto:me@example.com\"",
                    "xlink:href=\"file:///etc/passwd\"",
                )
                .replace(
                    "<table:table-row>",
                    "<table:table-row table:number-rows-repeated=\"5000\">",
                )
                .into_bytes();
        }
    }
    let hostile = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    let (_, back) = OdfCodec::unpack_ods_with_styling(&hostile).unwrap();
    assert!(back[0].1.links.is_empty());
    assert!(back[0].1.links_rejected >= 4);
}

#[test]
fn ods_keeps_links_on_empty_cells_and_formula_cells_without_a_value() {
    let sheets: Vec<OdsUnpacked> = vec![(
        "Sheet1".to_string(),
        vec![(0, 0, "=1+1".to_string()), (2, 0, "글자".to_string())],
    )];
    let mut links = SheetLinks::new();
    links.insert(
        (0, 0),
        CellLink::new("https://example.com/formula", "").unwrap(),
    );
    links.insert(
        (1, 0),
        CellLink::new("https://example.com/empty", "").unwrap(),
    );
    links.insert((2, 0), CellLink::new("#Sheet1!A1", "").unwrap());
    let styling = vec![SheetStyling {
        links: links.clone(),
        ..SheetStyling::default()
    }];
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets, &styling).unwrap();
    let (_, back) = OdfCodec::unpack_ods_with_styling(&bytes).unwrap();
    assert_eq!(back[0].1.links, links);
}

#[test]
fn a_hostile_sheet_of_repeated_ranges_reads_and_ties_the_walk_to_the_cells() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &[]).unwrap();
    let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let mut block = String::from("<hyperlinks>");
    for _ in 0..131_060 {
        block.push_str("<hyperlink ref=\"A1:CV100\" location=\"Sheet1!B1\"/>");
    }
    block.push_str("</hyperlinks>");
    for e in &mut entries {
        if e.path.starts_with("xl/worksheets/sheet") && e.path.ends_with(".xml") {
            let text = String::from_utf8(e.data.clone()).unwrap();
            e.data = text
                .replace("</worksheet>", &format!("{block}</worksheet>"))
                .into_bytes();
        }
    }
    let hostile = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    let back = OoxmlCodec::unpack_xlsx_styling(&hostile).unwrap();
    assert_eq!(back[0].1.links.len(), 10_000);
    assert!(
        back[0].1.links_rejected > 100_000,
        "the repeats are counted, not walked"
    );
    // The allowance of cells to visit belongs to the document: the first sheet used it, so the second one places none.
    assert_eq!(back[1].1.links.len(), 0);
}
