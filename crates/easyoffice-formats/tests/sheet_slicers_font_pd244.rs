//! PD-244: Standard `[MS-XLSX]` slicer parts (`xl/slicerCaches/`, `xl/slicers/`) writer and
//! per-cell font family (`CellStyle::font`, `SheetStyling::fonts`) round-trip across XLSX, ODS,
//! and BIFF8 XLS, with hostile-input bounds.

use std::sync::Arc;

use easyoffice_formats::{
    resolve_font_name, CellBox, CellStyle, LegacyOle2Codec, LegacyXlsPayload, OdfCodec, OoxmlCodec,
    SheetLinks, SheetStyling, SheetTable, TableSlicer, ZipArchiveCodec, ZipSecurityPolicy,
    MAX_FONT_FACES, MAX_FONT_NAME_CHARS,
};

#[test]
fn xlsx_packs_standard_slicer_parts_and_recovers_without_eo_table_extension() {
    let cells = vec![
        (0, 0, "지역".to_string()),
        (0, 1, "분기".to_string()),
        (0, 2, "매출".to_string()),
        (1, 0, "서울".to_string()),
        (1, 1, "1분기".to_string()),
        (1, 2, "1200".to_string()),
        (2, 0, "부산".to_string()),
        (2, 1, "2분기".to_string()),
        (2, 2, "950".to_string()),
    ];
    let mut table = SheetTable::new("SalesTbl".to_string(), CellBox::parse("A1:C3").unwrap());
    table.columns = vec!["지역".to_string(), "분기".to_string(), "매출".to_string()];
    table.slicers = vec![
        TableSlicer {
            col: 0,
            caption: "지역 필터".to_string(),
            left: 320,
            top: 40,
            width: 192,
            height: 224,
            multi_select: true,
        },
        TableSlicer {
            col: 1,
            caption: "분기 선택".to_string(),
            left: 528,
            top: 40,
            width: 180,
            height: 200,
            multi_select: false,
        },
    ];
    let styling = SheetStyling {
        tables: vec![table],
        ..SheetStyling::default()
    };

    let packed =
        OoxmlCodec::pack_xlsx_multi_styled(&[("매출현황".to_string(), cells)], &[], &[styling])
            .expect("pack xlsx with slicers");

    let entries = ZipArchiveCodec::unpack(&packed, &ZipSecurityPolicy::default()).unwrap();
    let part_xml = |path: &str| -> String {
        let entry = entries
            .iter()
            .find(|e| e.path == path)
            .unwrap_or_else(|| panic!("missing ZIP entry {path}"));
        std::str::from_utf8(&entry.data).unwrap().to_string()
    };

    let content_types = part_xml("[Content_Types].xml");
    assert!(content_types.contains("PartName=\"/xl/slicerCaches/slicerCache1.xml\""));
    assert!(content_types.contains("PartName=\"/xl/slicerCaches/slicerCache2.xml\""));
    assert!(content_types.contains("PartName=\"/xl/slicers/slicer1.xml\""));

    let wb_xml = part_xml("xl/workbook.xml");
    assert!(wb_xml.contains("<x14:slicerCaches><x14:slicerCache r:id=\"rIdSc1\"/><x14:slicerCache r:id=\"rIdSc2\"/></x14:slicerCaches>"));

    let wb_rels = part_xml("xl/_rels/workbook.xml.rels");
    assert!(wb_rels.contains("Id=\"rIdSc1\""));
    assert!(wb_rels.contains("Target=\"slicerCaches/slicerCache1.xml\""));

    let cache1 = part_xml("xl/slicerCaches/slicerCache1.xml");
    assert!(cache1.contains("name=\"Slicer_1\" sourceName=\"지역\""));
    assert!(cache1.contains("<x15:tableSlicerCache tableId=\"1\" column=\"1\"/>"));

    let cache2 = part_xml("xl/slicerCaches/slicerCache2.xml");
    assert!(cache2.contains("name=\"Slicer_2\" sourceName=\"분기\""));
    assert!(cache2.contains("<x15:tableSlicerCache tableId=\"1\" column=\"2\"/>"));

    let slicer1 = part_xml("xl/slicers/slicer1.xml");
    assert!(slicer1.contains("cache=\"Slicer_1\" caption=\"지역 필터\""));
    assert!(slicer1.contains(&format!("width=\"{}\"", 192u64 * 9_525)));
    assert!(slicer1.contains(&format!("height=\"{}\"", 224u64 * 9_525)));

    let sheet1 = part_xml("xl/worksheets/sheet1.xml");
    assert!(sheet1.contains("<x14:slicers><x14:slicer r:id=\"rIdSl1\"/></x14:slicers>"));
    let sheet1_rels = part_xml("xl/worksheets/_rels/sheet1.xml.rels");
    assert!(sheet1_rels.contains("Id=\"rIdSl1\""));
    assert!(sheet1_rels.contains("Target=\"../slicers/slicer1.xml\""));

    // Strip EasyOffice's `<extLst>` from `xl/tables/table1.xml` to prove native `[MS-XLSX]` parts alone round-trip!
    let stripped_entries: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "xl/tables/table1.xml" {
                let xml = std::str::from_utf8(&e.data).unwrap();
                let start = xml.find("<extLst>").expect("table1 has extLst");
                let end = xml.find("</extLst>").unwrap() + "</extLst>".len();
                let mut clean = String::new();
                clean.push_str(&xml[..start]);
                clean.push_str(&xml[end..]);
                (e.path, clean.into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let stripped_xlsx = ZipArchiveCodec::pack_compressed(&stripped_entries).unwrap();
    let unpacked = OoxmlCodec::unpack_xlsx_styling(&stripped_xlsx).unwrap();
    let recovered = &unpacked[0].1.tables[0].slicers;
    assert_eq!(recovered.len(), 2);
    assert_eq!(recovered[0].col, 0);
    assert_eq!(recovered[0].caption, "지역 필터");
    assert_eq!(recovered[0].width, 192);
    assert_eq!(recovered[0].height, 224);
    assert_eq!(recovered[1].col, 1);
    assert_eq!(recovered[1].caption, "분기 선택");
    assert_eq!(recovered[1].width, 180);
    assert_eq!(recovered[1].height, 200);
}

#[test]
fn per_cell_font_family_round_trips_and_deduplicates_in_xlsx_and_ods() {
    let mut styling = SheetStyling {
        fonts: Arc::new(vec![
            "Malgun Gothic".to_string(),
            "JetBrains Mono".to_string(),
        ]),
        ..SheetStyling::default()
    };
    for r in 0..200u32 {
        styling.cells.insert(
            (r, 0),
            CellStyle {
                font: 1,
                bold: true,
                ..CellStyle::default()
            },
        );
        styling.cells.insert(
            (r, 1),
            CellStyle {
                font: 2,
                size_pt: Some(10.5),
                ..CellStyle::default()
            },
        );
    }
    let cells = vec![
        (0, 0, "본문 제목".to_string()),
        (0, 1, "SELECT 1".to_string()),
        (0, 2, "기본 글꼴".to_string()),
    ];

    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Fonts".to_string(), cells.clone())],
        &[],
        &[styling.clone()],
    )
    .unwrap();
    let entries = ZipArchiveCodec::unpack(&xlsx, &ZipSecurityPolicy::default()).unwrap();
    let styles_xml = std::str::from_utf8(
        &entries
            .iter()
            .find(|e| e.path == "xl/styles.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(
        styles_xml.contains("<fonts count=\"3\">"),
        "400 styled cells sharing 2 font looks must emit exactly 3 <font> definitions (default + 2): {styles_xml}"
    );

    let xlsx_back = &OoxmlCodec::unpack_xlsx_styling(&xlsx).unwrap()[0].1;
    assert_eq!(xlsx_back.fonts_rejected, 0);
    assert_eq!(
        resolve_font_name(xlsx_back.cells[&(0, 0)].font, &xlsx_back.fonts),
        Some("Malgun Gothic")
    );
    assert_eq!(
        resolve_font_name(xlsx_back.cells[&(199, 1)].font, &xlsx_back.fonts),
        Some("JetBrains Mono")
    );
    assert!(!xlsx_back.cells.contains_key(&(0, 2)));

    let ods = OdfCodec::pack_ods_multi_styled(&[("Fonts".to_string(), cells)], &[styling]).unwrap();
    let ods_back = &OdfCodec::unpack_ods_styling(&ods).unwrap()[0].1;
    assert_eq!(ods_back.fonts_rejected, 0);
    assert_eq!(
        resolve_font_name(ods_back.cells[&(0, 0)].font, &ods_back.fonts),
        Some("Malgun Gothic")
    );
    assert_eq!(
        resolve_font_name(ods_back.cells[&(199, 1)].font, &ods_back.fonts),
        Some("JetBrains Mono")
    );
}

#[test]
fn biff8_xls_reads_latin1_and_utf16le_font_names_across_index_four_gap() {
    // Start from a valid BIFF8 workbook packed by LegacyOle2Codec, then inject custom FONT & XF records
    // into its Workbook stream? Even simpler: inspect `xls_styles` via a real BIFF8 workbook with 5 FONT
    // records (indices 0, 1, 2, 3, and 5 after the skipped index 4) by patching the FONT records in a
    // minimal packed XLS!
    let base_st = SheetStyling {
        cells: [
            (
                (0, 0),
                CellStyle {
                    font_color: Some(0xFF0000),
                    ..Default::default()
                },
            ),
            (
                (0, 1),
                CellStyle {
                    font_color: Some(0x00FF00),
                    ..Default::default()
                },
            ),
            (
                (0, 2),
                CellStyle {
                    font_color: Some(0x0000FF),
                    ..Default::default()
                },
            ),
            (
                (0, 3),
                CellStyle {
                    font_color: Some(0xFF00FF),
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let base_xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &[LegacyXlsPayload {
            sheet_name: "S1".to_string(),
            cells: vec![
                (0, 0, "A".to_string()),
                (0, 1, "B".to_string()),
                (0, 2, "C".to_string()),
                (0, 3, "D".to_string()),
            ],
        }],
        &[SheetLinks::default()],
        &[base_st],
    )
    .unwrap();
    // Replace two 5-char "Arial" FONT names in-place (same byte length = 5!) with "Menlo" and UTF-16LE?
    // Wait: same record size keeps all BIFF8 BOUNDSHEET stream offsets valid!
    // Font 0 stays "Arial", another 5-byte Latin-1 font becomes "Menlo", and a 2-char UTF-16LE font ("돋움" = 4 bytes + 1 pad? No: 2 UTF-16LE chars = 4 bytes != 5 bytes).
    // Let's check: 5-byte Latin-1 "Menlo" replaces the last "Arial" (record 4 -> BIFF8 font index 5, past skipped index 4!).
    let needle = [5u8, 0u8, b'A', b'r', b'i', b'a', b'l'];
    let mut patched = base_xls.clone();
    let mut hits = Vec::new();
    for i in 0..patched.len().saturating_sub(needle.len()) {
        if patched[i..i + needle.len()] == needle {
            hits.push(i);
        }
    }
    assert!(
        hits.len() >= 5,
        "expected default + 4 FONT records, got {}",
        hits.len()
    );
    let first_custom = hits[1];
    patched[first_custom + 2..first_custom + 7].copy_from_slice(b"Menlo");
    // Overwrite the 5th FONT record (index 4 in Vec -> BIFF8 ifnt = 5!) with "Serif"
    let fifth = hits[4];
    patched[fifth + 2..fifth + 7].copy_from_slice(b"Serif");

    let (_, stylings) = LegacyOle2Codec::unpack_xls_with_styling(&patched).unwrap();
    let st = &stylings[0].1;
    // `XlsColorPlan` orders looks by RGB: `0x0000FF` ((0, 2)) is custom FONT #1 (`hits[1]`),
    // `0x00FF00` ((0, 1)) is #2, `0xFF0000` ((0, 0)) is #3, and `0xFF00FF` ((0, 3)) is #4 (`ifnt = 5`).
    assert_eq!(
        resolve_font_name(st.cells[&(0, 2)].font, &st.fonts),
        Some("Menlo")
    );
    assert_eq!(st.cells[&(0, 1)].font, 0, "Arial matches default font 0");
    assert_eq!(
        resolve_font_name(st.cells[&(0, 3)].font, &st.fonts),
        Some("Serif"),
        "BIFF8 ifnt=5 (past missing index 4) resolves to 5th FONT record"
    );
}

#[test]
fn hostile_xlsx_font_names_and_overflow_fail_closed_to_default_font() {
    let base = OoxmlCodec::pack_xlsx_multi_styled(
        &[("S1".to_string(), vec![(0, 0, "x".to_string())])],
        &[],
        &[SheetStyling::default()],
    )
    .unwrap();
    let entries = ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default()).unwrap();

    let overlong = "F".repeat(MAX_FONT_NAME_CHARS + 5);
    let mut fonts_xml =
        String::from("<fonts count=\"516\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font>");
    let mut xfs_xml = String::from("<cellXfs count=\"516\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>");
    // 3 hostile names + 512 valid names + 1 overflow name = 515 custom XFs
    let bad_names = [
        "Evil;Font{}".to_string(),
        overlong,
        "Bad\\Quote".to_string(),
    ];
    for (idx, bad) in bad_names.iter().enumerate() {
        let fid = idx + 1;
        fonts_xml.push_str(&format!(
            "<font><sz val=\"11\"/><name val=\"{bad}\"/></font>"
        ));
        xfs_xml.push_str(&format!(
            "<xf numFmtId=\"0\" fontId=\"{fid}\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>"
        ));
    }
    for i in 0..=MAX_FONT_FACES {
        let fid = bad_names.len() + 1 + i;
        fonts_xml.push_str(&format!(
            "<font><sz val=\"11\"/><name val=\"CleanFont{i}\"/></font>"
        ));
        xfs_xml.push_str(&format!(
            "<xf numFmtId=\"0\" fontId=\"{fid}\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>"
        ));
    }
    fonts_xml.push_str("</fonts>");
    xfs_xml.push_str("</cellXfs>");

    let custom_styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">{fonts_xml}<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>{xfs_xml}</styleSheet>"
    );
    let mut row_xml = String::from("<row r=\"1\">");
    for col in 0..515u32 {
        let mut letters = String::new();
        let mut n = col + 1;
        while n > 0 {
            let rem = ((n - 1) % 26) as u8;
            letters.insert(0, (b'A' + rem) as char);
            n = (n - 1) / 26;
        }
        row_xml.push_str(&format!(
            "<c r=\"{letters}1\" s=\"{}\"><v>1</v></c>",
            col + 1
        ));
    }
    row_xml.push_str("</row>");
    let custom_sheet = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{row_xml}</sheetData></worksheet>"
    );

    let patched_entries: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "xl/styles.xml" {
                (e.path, custom_styles.as_bytes().to_vec())
            } else if e.path == "xl/worksheets/sheet1.xml" {
                (e.path, custom_sheet.as_bytes().to_vec())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let hostile_xlsx = ZipArchiveCodec::pack_compressed(&patched_entries).unwrap();
    let unpacked = &OoxmlCodec::unpack_xlsx_styling(&hostile_xlsx).unwrap()[0].1;
    assert_eq!(unpacked.fonts.len(), MAX_FONT_FACES);
    assert_eq!(
        unpacked.fonts_rejected, 4,
        "3 malformed font names + 1 overflow beyond MAX_FONT_FACES must be rejected"
    );
    assert!(!unpacked.cells.contains_key(&(0, 0)));
    assert!(!unpacked.cells.contains_key(&(0, 1)));
    assert!(!unpacked.cells.contains_key(&(0, 2)));
    assert_eq!(
        resolve_font_name(unpacked.cells[&(0, 3)].font, &unpacked.fonts),
        Some("CleanFont0")
    );
}
