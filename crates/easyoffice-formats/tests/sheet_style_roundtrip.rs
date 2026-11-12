//! Cell formatting and column widths survive XLSX/ODS save -> reopen (PD-089, #148).

use std::collections::BTreeMap;

use easyoffice_formats::{
    read_sheet_styling, CellStyle, DocumentFormat, OdfCodec, OdsUnpacked, OoxmlCodec, SheetStyling,
    XlsxCachedValue, XlsxFormulaCache, ZipArchiveCodec, ZipSecurityPolicy,
};

fn cell(text: &str) -> OdsUnpacked {
    ("Sheet1".to_string(), vec![(0, 0, text.to_string())])
}

fn styled(cells: Vec<((u32, u32), CellStyle)>, columns: Vec<(u32, u32)>) -> SheetStyling {
    SheetStyling {
        cells: cells.into_iter().collect(),
        columns: columns.into_iter().collect(),
        ..Default::default()
    }
}

fn flag(which: &str) -> CellStyle {
    let mut s = CellStyle::default();
    match which {
        "bold" => s.bold = true,
        "italic" => s.italic = true,
        "underline" => s.underline = true,
        "strike" => s.strike = true,
        _ => s.size_pt = Some(15.75),
    }
    s
}

fn pack(format: DocumentFormat, sheets: &[OdsUnpacked], styling: &[SheetStyling]) -> Vec<u8> {
    match format {
        DocumentFormat::Xlsx => OoxmlCodec::pack_xlsx_multi_styled(sheets, &[], styling).unwrap(),
        _ => OdfCodec::pack_ods_multi_styled(sheets, styling).unwrap(),
    }
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("missing part {path}"));
    String::from_utf8(entry.data.clone()).unwrap()
}

const BOTH: [DocumentFormat; 2] = [DocumentFormat::Xlsx, DocumentFormat::Ods];

#[test]
fn every_style_flag_and_size_round_trips_in_xlsx_and_ods() {
    for format in BOTH {
        for which in ["bold", "italic", "underline", "strike", "size"] {
            let sheets = vec![(
                "Sheet1".to_string(),
                vec![(0, 0, "가".to_string()), (1, 2, "12".to_string())],
            )];
            let st = styled(vec![((0, 0), flag(which)), ((1, 2), flag(which))], vec![]);
            let bytes = pack(format, &sheets, std::slice::from_ref(&st));
            let back = read_sheet_styling(&bytes, format).unwrap();
            assert_eq!(back.len(), 1, "{format:?} {which}");
            assert_eq!(back[0].0, "Sheet1");
            assert_eq!(back[0].1.cells, st.cells, "{format:?} {which}");
        }
    }
}

#[test]
fn combined_flags_and_blank_styled_cells_round_trip() {
    let all = CellStyle {
        bold: true,
        italic: true,
        underline: true,
        strike: true,
        size_pt: Some(9.0),
        ..CellStyle::default()
    };
    for format in BOTH {
        let st = styled(vec![((0, 0), all), ((3, 1), flag("bold"))], vec![]);
        // (3, 1) has no value: a bold-only empty cell must still be written and read back.
        let bytes = pack(format, &[cell("값")], std::slice::from_ref(&st));
        let back = read_sheet_styling(&bytes, format).unwrap();
        assert_eq!(back[0].1.cells, st.cells, "{format:?}");
    }
}

#[test]
fn cell_values_and_formula_cache_are_unchanged_by_styling() {
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![
            (0, 0, "1".to_string()),
            (1, 0, "2".to_string()),
            (2, 0, "=SUM(A1:A2)".to_string()),
            (3, 0, "글".to_string()),
        ],
    )];
    let st = styled(
        vec![((2, 0), flag("bold")), ((3, 0), flag("italic"))],
        vec![(0, 120)],
    );
    let mut cache = XlsxFormulaCache::new();
    cache.insert((2, 0), XlsxCachedValue::Number(3.0));
    let xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[cache], std::slice::from_ref(&st)).unwrap();
    let sheet = part(&xlsx, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains("<c r=\"A3\" s=\"1\"><f>SUM(A1:A2)</f><v>3</v></c>"),
        "{sheet}"
    );
    assert_eq!(OoxmlCodec::unpack_xlsx_multi(&xlsx).unwrap(), sheets);
    let ods = OdfCodec::pack_ods_multi_styled(&sheets, &[st]).unwrap();
    assert_eq!(OdfCodec::unpack_ods_multi(&ods).unwrap(), sheets);
}

#[test]
fn a_thousand_cells_sharing_two_styles_write_three_cell_xfs() {
    let cells: Vec<(u32, u32, String)> = (0..1000).map(|r| (r, 0, format!("v{r}"))).collect();
    let styles: Vec<((u32, u32), CellStyle)> = (0..1000)
        .map(|r| {
            (
                (r, 0),
                if r % 2 == 0 {
                    flag("bold")
                } else {
                    flag("italic")
                },
            )
        })
        .collect();
    let sheets = vec![("Sheet1".to_string(), cells)];
    let xlsx = pack(
        DocumentFormat::Xlsx,
        &sheets,
        &[styled(styles.clone(), vec![])],
    );
    let xml = part(&xlsx, "xl/styles.xml");
    assert!(xml.contains("<cellXfs count=\"3\">"), "{xml}");
    assert!(xml.contains("<fonts count=\"3\">"), "{xml}");
    assert_eq!(
        xml.matches("<xf ").count(),
        4,
        "1 cellStyleXf + 3 cellXfs: {xml}"
    );
    let ods = pack(DocumentFormat::Ods, &sheets, &[styled(styles, vec![])]);
    let content = part(&ods, "content.xml");
    assert_eq!(
        content.matches("style:family=\"table-cell\"").count(),
        2,
        "{content}"
    );
}

#[test]
fn column_widths_round_trip_and_use_excel_character_units() {
    let widths = vec![(0, 96), (1, 96), (3, 200), (5, 36)];
    for format in BOTH {
        let st = styled(vec![], widths.clone());
        let bytes = pack(format, &[cell("x")], std::slice::from_ref(&st));
        let back = read_sheet_styling(&bytes, format).unwrap();
        assert_eq!(back[0].1.columns, st.columns, "{format:?}");
    }
    let xlsx = pack(
        DocumentFormat::Xlsx,
        &[cell("x")],
        &[styled(vec![], widths)],
    );
    let sheet = part(&xlsx, "xl/worksheets/sheet1.xml");
    // 96 px / 7 px per digit = 13.7109375 (truncated to 1/256); A and B share one <col>.
    assert!(
        sheet.contains("<col min=\"1\" max=\"2\" width=\"13.7109375\" customWidth=\"1\"/>"),
        "{sheet}"
    );
    assert!(sheet.contains("<col min=\"4\" max=\"4\" width=\"28.5703125\" customWidth=\"1\"/>"));
    assert!(
        sheet.find("<cols>").unwrap() < sheet.find("<sheetData>").unwrap(),
        "cols precede sheetData"
    );
}

#[test]
fn styles_xml_follows_the_ecma376_element_order() {
    let st = styled(vec![((0, 0), flag("bold"))], vec![]);
    let xml = part(
        &pack(DocumentFormat::Xlsx, &[cell("x")], &[st]),
        "xl/styles.xml",
    );
    let order = [
        "<fonts",
        "<fills",
        "<borders",
        "<cellStyleXfs",
        "<cellXfs",
        "<cellStyles",
    ];
    let positions: Vec<usize> = order.iter().map(|t| xml.find(t).unwrap()).collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "{positions:?} {xml}"
    );
    // Inside <font>: b, i, strike, u, sz, name.
    let font = &xml[xml.find("<font><b/>").unwrap()..];
    assert!(font.starts_with("<font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font>"));
    let all = CellStyle {
        bold: true,
        italic: true,
        underline: true,
        strike: true,
        size_pt: Some(12.5),
        ..CellStyle::default()
    };
    let xml = part(
        &pack(
            DocumentFormat::Xlsx,
            &[cell("x")],
            &[styled(vec![((0, 0), all)], vec![])],
        ),
        "xl/styles.xml",
    );
    assert!(
        xml.contains("<font><b/><i/><strike/><u/><sz val=\"12.5\"/><name val=\"Calibri\"/></font>"),
        "{xml}"
    );
}

#[test]
fn unstyled_workbooks_keep_the_single_default_format() {
    let xlsx = OoxmlCodec::pack_xlsx_multi(&[cell("x")]).unwrap();
    let xml = part(&xlsx, "xl/styles.xml");
    assert!(xml.contains("<cellXfs count=\"1\">"), "{xml}");
    assert!(!part(&xlsx, "xl/worksheets/sheet1.xml").contains("<cols>"));
    let sheet = part(&xlsx, "xl/worksheets/sheet1.xml");
    assert!(!sheet.contains(" s=\""), "{sheet}");
    assert!(read_sheet_styling(&xlsx, DocumentFormat::Xlsx).unwrap()[0]
        .1
        .is_empty());
}

#[test]
fn styled_files_open_with_the_repo_zip_decoder_and_only_valid_parts() {
    let st = styled(vec![((0, 0), flag("bold"))], vec![(0, 150)]);
    for format in BOTH {
        let bytes = pack(format, &[cell("x")], std::slice::from_ref(&st));
        let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
        assert!(entries.len() >= 3);
        assert_eq!(&bytes[..2], b"PK");
    }
}

#[test]
fn out_of_range_style_coordinates_are_rejected() {
    let st = styled(vec![((OoxmlCodec::MAX_XLSX_ROWS, 0), flag("bold"))], vec![]);
    assert!(OoxmlCodec::pack_xlsx_multi_styled(&[cell("x")], &[], &[st]).is_err());
    let st = styled(vec![], vec![(OoxmlCodec::MAX_XLSX_COLS, 100)]);
    assert!(OoxmlCodec::pack_xlsx_multi_styled(&[cell("x")], &[], &[st]).is_err());
}

#[test]
fn multi_sheet_styling_is_kept_per_sheet() {
    let sheets = vec![
        ("가".to_string(), vec![(0, 0, "1".to_string())]),
        ("나".to_string(), vec![(0, 0, "2".to_string())]),
    ];
    let a = styled(vec![((0, 0), flag("bold"))], vec![(0, 120)]);
    let b = styled(vec![((0, 0), flag("italic"))], vec![]);
    for format in BOTH {
        let back =
            read_sheet_styling(&pack(format, &sheets, &[a.clone(), b.clone()]), format).unwrap();
        let by_name: BTreeMap<_, _> = back.into_iter().collect();
        assert_eq!(by_name["가"], a, "{format:?}");
        assert_eq!(by_name["나"], b, "{format:?}");
    }
}

#[test]
fn reads_styles_written_by_other_applications() {
    // Default font 12 pt, a bold 12 pt format (size equals the default -> no explicit size),
    // an explicit `<b val="0"/>`, and a double underline.
    let styles = concat!(
        "<?xml version=\"1.0\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
        "<fonts count=\"4\"><font><sz val=\"12\"/><name val=\"Arial\"/></font>",
        "<font><b/><sz val=\"12\"/><name val=\"Arial\"/></font>",
        "<font><b val=\"0\"/><sz val=\"20\"/><name val=\"Arial\"/></font>",
        "<font><u val=\"double\"/><sz val=\"12\"/></font></fonts>",
        "<cellXfs count=\"4\"><xf fontId=\"0\"/><xf fontId=\"1\"><alignment/></xf><xf fontId=\"2\"/><xf fontId=\"3\"/></cellXfs></styleSheet>"
    );
    let sheet = concat!(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
        "<cols><col min=\"2\" max=\"3\" width=\"20\" customWidth=\"1\"/><col min=\"5\" max=\"5\" width=\"9\"/></cols>",
        "<sheetData><row r=\"1\"><c r=\"A1\" s=\"1\" t=\"n\"><v>1</v></c><c r=\"B1\" s=\"2\"/><c r=\"C1\" s=\"3\"><v>3</v></c><c r=\"D1\" s=\"0\"><v>4</v></c></row></sheetData></worksheet>"
    );
    let bytes = base_xlsx_with(styles, sheet);
    let back = &read_sheet_styling(&bytes, DocumentFormat::Xlsx).unwrap()[0].1;
    assert_eq!(back.cells.len(), 3, "{back:?}");
    assert_eq!(
        back.cells[&(0, 0)],
        CellStyle {
            bold: true,
            ..CellStyle::default()
        }
    );
    assert_eq!(
        back.cells[&(0, 1)],
        CellStyle {
            size_pt: Some(20.0),
            ..CellStyle::default()
        }
    );
    assert_eq!(
        back.cells[&(0, 2)],
        CellStyle {
            underline: true,
            ..CellStyle::default()
        }
    );
    // width 20 -> 140 px for B and C; the `<col>` without customWidth is ignored.
    assert_eq!(back.columns, BTreeMap::from([(1, 140), (2, 140)]));
}

fn base_xlsx_with(styles: &str, sheet: &str) -> Vec<u8> {
    let base = OoxmlCodec::pack_xlsx_multi(&[cell("x")]).unwrap();
    let entries = ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default()).unwrap();
    let replaced: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| match e.path.as_str() {
            "xl/styles.xml" => (e.path, styles.as_bytes().to_vec()),
            "xl/worksheets/sheet1.xml" => (e.path, sheet.as_bytes().to_vec()),
            _ => (e.path, e.data),
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&replaced).unwrap()
}

#[test]
fn reads_ods_styles_with_units_repeats_and_ignores_sheet_wide_fills() {
    let content = concat!(
        "<?xml version=\"1.0\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\">",
        "<office:automatic-styles>",
        "<style:style style:name=\"co9\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"2.54cm\"/></style:style>",
        "<style:style style:name=\"ce5\" style:family=\"table-cell\" style:parent-style-name=\"Default\"><style:text-properties fo:font-weight=\"700\" fo:font-size=\"12pt\" style:text-line-through-style=\"none\"/></style:style>",
        "</office:automatic-styles><office:body><office:spreadsheet><table:table table:name=\"S\">",
        "<table:table-column table:style-name=\"co9\" table:number-columns-repeated=\"2\"/>",
        "<table:table-row><table:table-cell table:style-name=\"ce5\" table:number-columns-repeated=\"2\" office:value-type=\"string\"><text:p>a</text:p></table:table-cell><table:table-cell table:style-name=\"ce5\" table:number-columns-repeated=\"16000\"/></table:table-row>",
        "</table:table></office:spreadsheet></office:body></office:document-content>"
    );
    let entries = vec![
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.spreadsheet".to_vec(),
        ),
        ("content.xml".to_string(), content.as_bytes().to_vec()),
    ];
    let bytes = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    let back = &read_sheet_styling(&bytes, DocumentFormat::Ods).unwrap()[0].1;
    let bold12 = CellStyle {
        bold: true,
        size_pt: Some(12.0),
        ..CellStyle::default()
    };
    assert_eq!(
        back.cells,
        BTreeMap::from([((0, 0), bold12), ((0, 1), bold12)])
    );
    assert_eq!(back.columns, BTreeMap::from([(0, 96), (1, 96)]));
}

#[test]
fn other_formats_have_no_styling() {
    assert!(read_sheet_styling(b"a,b", DocumentFormat::Csv)
        .unwrap()
        .is_empty());
}
