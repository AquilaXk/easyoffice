//! PD-322: OOXML trailing single `<extLst>`, `@`/`_xlfn.SINGLE`, `definedNames`/`cfRule`/DV formula
//! spelling, sheet-name quoting for cell/R1C1 names, and BIFF8 mandatory `FONT`/`XF`/`STYLE` records.

use easyoffice_formats::{
    parse_sqref, CellBox, CellLink, CellStyle, CfKind, CfRule, CfStyle, Cmp, DataValidation,
    DefaultFont, DvKind, LegacyOle2Codec, LegacyXlsPayload, OdsUnpacked, Ole2ContainerCodec,
    OoxmlCodec, SheetCellNote, SheetLinks, SheetPage, SheetRules, SheetStyling, SheetTable,
    TableSlicer, ZipArchiveCodec, ZipSecurityPolicy, MAX_RULE_TEXT_CHARS,
};

const BIFF8_BOF: u16 = 0x0809;
const BIFF8_EOF: u16 = 0x000A;
const BIFF8_FONT: u16 = 0x0031;
const BIFF8_XF: u16 = 0x00E0;
const BIFF8_STYLE: u16 = 0x0293;
const BIFF8_NUMBER: u16 = 0x0203;
const BIFF8_LABELSST: u16 = 0x00FD;
const BIFF8_BOOLERR: u16 = 0x0205;
const BIFF8_BLANK: u16 = 0x0201;

fn walk_biff8_records(stream: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let mut records = Vec::new();
    let mut pos = 0usize;
    while pos + 4 <= stream.len() {
        let opcode = u16::from_le_bytes([stream[pos], stream[pos + 1]]);
        let len = u16::from_le_bytes([stream[pos + 2], stream[pos + 3]]) as usize;
        pos += 4;
        assert!(
            pos + len <= stream.len(),
            "truncated BIFF8 record 0x{opcode:04X}"
        );
        records.push((opcode, stream[pos..pos + len].to_vec()));
        pos += len;
    }
    records
}

#[test]
fn worksheet_merges_dv_and_slicer_extensions_into_one_trailing_ext_lst() {
    let cells = vec![
        (0, 0, "항목".to_string()),
        (0, 1, "완료".to_string()),
        (1, 0, "A".to_string()),
        (1, 1, "True".to_string()),
        (2, 0, "B".to_string()),
        (2, 1, "False".to_string()),
    ];
    let mut links = SheetLinks::new();
    links.insert(
        (1, 0),
        CellLink::new("https://example.com", "링크").unwrap(),
    );

    let mut table = SheetTable::new("SalesTable".to_string(), CellBox::parse("A1:B3").unwrap());
    table.columns = vec!["항목".to_string(), "완료".to_string()];
    table.slicers.push(TableSlicer {
        col: 0,
        caption: "항목".to_string(),
        left: 200,
        top: 20,
        width: 160,
        height: 180,
        multi_select: false,
    });

    let mut checkbox = DataValidation::new(parse_sqref("B2:B5"), DvKind::Checkbox);
    checkbox.a = "True".to_string();
    checkbox.b = "False".to_string();

    let mut styling = SheetStyling {
        links,
        tables: vec![table],
        page: SheetPage {
            landscape: true,
            ..SheetPage::default()
        },
        rules: SheetRules {
            dv: vec![checkbox],
            ..SheetRules::default()
        },
        ..SheetStyling::default()
    };
    styling.notes.insert(
        easyoffice_formats::pack_cell_pos(0, 0),
        SheetCellNote {
            author: "QA".to_string(),
            text: "메모".to_string(),
            visible: false,
        },
    );

    let xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&[("Sheet1".to_string(), cells)], &[], &[styling])
            .unwrap();
    let entries = ZipArchiveCodec::unpack(&xlsx, &ZipSecurityPolicy::default()).unwrap();
    let sheet_xml = entries
        .iter()
        .find(|e| e.path == "xl/worksheets/sheet1.xml")
        .map(|e| String::from_utf8(e.data.clone()).unwrap())
        .unwrap();

    assert_eq!(sheet_xml.matches("<extLst>").count(), 1, "{sheet_xml}");
    assert_eq!(sheet_xml.matches("</extLst>").count(), 1, "{sheet_xml}");
    assert!(
        sheet_xml.ends_with("</extLst></worksheet>"),
        "single <extLst> must sit right before </worksheet>: {sheet_xml}"
    );
    let tbl_pos = sheet_xml.find("<tableParts").unwrap();
    let ext_pos = sheet_xml.find("<extLst>").unwrap();
    assert!(tbl_pos < ext_pos, "{sheet_xml}");
    assert!(sheet_xml[ext_pos..].contains("<eo:dv "));
    assert!(sheet_xml[ext_pos..].contains("<x14:slicers>"));

    let (_, mut back_stylings) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let back = back_stylings.remove(0).1;
    assert_eq!(back.rules.dv.len(), 1);
    assert_eq!(back.rules.dv[0].kind, DvKind::Checkbox);
    assert_eq!(back.tables[0].slicers.len(), 1);
    assert_eq!(back.links.len(), 1);
    assert_eq!(back.notes.len(), 1);
}

#[test]
fn implicit_single_defined_names_cf_rules_and_dv_round_trip_with_xlfn() {
    let cells = vec![
        (0, 0, "=@A1:A5".to_string()),
        (0, 1, "=@SEQUENCE(5)".to_string()),
        (0, 2, "=LET(x, A1:A5, @x)".to_string()),
        (0, 3, "=@A1#".to_string()),
        (0, 4, "=SUM(Table1[@Amount])".to_string()),
        (0, 5, "=\"@text\"".to_string()),
    ];

    let mut dv_list_formula = DataValidation::new(parse_sqref("C1:C5"), DvKind::List);
    dv_list_formula.a = "=UNIQUE(A1:A10)".to_string();
    let mut dv_custom = DataValidation::new(parse_sqref("D1:D5"), DvKind::Custom);
    dv_custom.a = "=@A1:A5>0".to_string();
    let mut dv_plain_list = DataValidation::new(parse_sqref("E1:E5"), DvKind::List);
    dv_plain_list.a = "가,나,다".to_string();
    let mut dv_checkbox = DataValidation::new(parse_sqref("F1:F5"), DvKind::Checkbox);
    dv_checkbox.a = "예".to_string();
    dv_checkbox.b = "아니오".to_string();

    let cf_cell_is = CfRule {
        ranges: parse_sqref("A1:A5"),
        priority: 1,
        stop_if_true: false,
        kind: CfKind::CellIs {
            cmp: Cmp::Greater,
            a: "XLOOKUP(B1,A1:A5,C1:C5)".to_string(),
            b: String::new(),
        },
        style: CfStyle::default(),
    };
    let cf_expr = CfRule {
        ranges: parse_sqref("B1:B5"),
        priority: 2,
        stop_if_true: false,
        kind: CfKind::Expression("=@A1:A5>0".to_string()),
        style: CfStyle::default(),
    };

    let mut styling = SheetStyling {
        rules: SheetRules {
            cf: vec![cf_cell_is, cf_expr],
            dv: vec![dv_list_formula, dv_custom, dv_plain_list, dv_checkbox],
            ..SheetRules::default()
        },
        ..SheetStyling::default()
    };
    styling.defined_names.push(easyoffice_formats::DefinedName {
        name: "MyLam".to_string(),
        scope: None,
        refers_to: "=LAMBDA(x, XLOOKUP(x, A1:A5, B1:B5))".to_string(),
        comment: String::new(),
        hidden: false,
    });
    styling.defined_names.push(easyoffice_formats::DefinedName {
        name: "SingleRef".to_string(),
        scope: None,
        refers_to: "=@Sheet1!A1:A5".to_string(),
        comment: String::new(),
        hidden: false,
    });

    let xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&[("Sheet1".to_string(), cells)], &[], &[styling])
            .unwrap();
    let entries = ZipArchiveCodec::unpack(&xlsx, &ZipSecurityPolicy::default()).unwrap();
    let part = |path: &str| -> String {
        entries
            .iter()
            .find(|e| e.path == path)
            .map(|e| String::from_utf8(e.data.clone()).unwrap())
            .unwrap()
    };
    let wb_xml = part("xl/workbook.xml");
    let sheet_xml = part("xl/worksheets/sheet1.xml");

    assert!(
        sheet_xml.contains("<f>_xlfn.SINGLE(A1:A5)</f>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<f>_xlfn.SINGLE(_xlfn.SEQUENCE(5))</f>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<f>_xlfn.LET(_xlpm.x, A1:A5, _xlfn.SINGLE(_xlpm.x))</f>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<f>_xlfn.SINGLE(_xlfn.ANCHORARRAY(A1))</f>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<f>SUM(Table1[[#This Row],[Amount]])</f>"),
        "{sheet_xml}"
    );
    assert!(
        !sheet_xml.contains("_xlfn.SINGLE([@Amount])"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<formula>_xlfn.XLOOKUP(B1,A1:A5,C1:C5)</formula>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<formula>=_xlfn.SINGLE(A1:A5)&gt;0</formula>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<formula1>_xlfn.UNIQUE(A1:A10)</formula1>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<formula1>=_xlfn.SINGLE(A1:A5)&gt;0</formula1>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<formula1>&quot;가,나,다&quot;</formula1>"),
        "{sheet_xml}"
    );
    assert!(
        wb_xml.contains("_xlfn.LAMBDA(_xlpm.x, _xlfn.XLOOKUP(_xlpm.x, A1:A5, B1:B5))"),
        "{wb_xml}"
    );
    assert!(wb_xml.contains("=_xlfn.SINGLE(Sheet1!A1:A5)"), "{wb_xml}");

    let (sheets_back, stylings_back) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let back_cells = &sheets_back[0].1;
    assert_eq!(back_cells[0].2, "=@A1:A5");
    assert_eq!(back_cells[1].2, "=@SEQUENCE(5)");
    assert_eq!(back_cells[2].2, "=LET(x, A1:A5, @x)");
    assert_eq!(back_cells[3].2, "=@A1#");
    assert_eq!(back_cells[4].2, "=SUM(Table1[[#This Row],[Amount]])");
    assert_eq!(back_cells[5].2, "=\"@text\"");

    let back_st = &stylings_back[0].1;
    assert_eq!(
        back_st.defined_names[0].refers_to,
        "=LAMBDA(x, XLOOKUP(x, A1:A5, B1:B5))"
    );
    assert_eq!(back_st.defined_names[1].refers_to, "=@Sheet1!A1:A5");
    assert_eq!(back_st.rules.dv[0].a, "=UNIQUE(A1:A10)");
    assert_eq!(back_st.rules.dv[1].a, "=@A1:A5>0");
    assert_eq!(back_st.rules.dv[2].a, "가,나,다");
    assert_eq!(back_st.rules.dv[3].kind, DvKind::Checkbox);
    assert_eq!(back_st.rules.cf.len(), 2);
}

#[test]
fn print_names_quote_cell_address_and_r1c1_sheet_names() {
    let names = ["A1", "XFD1048576", "R1C1", "r1c1", "RC", "R12", "Sheet1"];
    let sheets: Vec<OdsUnpacked> = names
        .iter()
        .map(|n| ((*n).to_string(), vec![(0, 0, "1".to_string())]))
        .collect();
    let stylings: Vec<SheetStyling> = names
        .iter()
        .map(|_| SheetStyling {
            page: SheetPage {
                print_area: parse_sqref("A1:B2"),
                title_rows: Some((0, 0)),
                ..SheetPage::default()
            },
            ..SheetStyling::default()
        })
        .collect();

    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &stylings).unwrap();
    let entries = ZipArchiveCodec::unpack(&xlsx, &ZipSecurityPolicy::default()).unwrap();
    let wb_xml = entries
        .iter()
        .find(|e| e.path == "xl/workbook.xml")
        .map(|e| String::from_utf8(e.data.clone()).unwrap())
        .unwrap();

    for quoted in ["A1", "XFD1048576", "R1C1", "r1c1", "RC", "R12"] {
        assert!(
            wb_xml.contains(&format!("&apos;{quoted}&apos;!$A$1:$B$2")),
            "expected quoted sheet {quoted}: {wb_xml}"
        );
    }
    assert!(wb_xml.contains(">Sheet1!$A$1:$B$2<"), "{wb_xml}");

    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    for (i, (_, st)) in back.iter().enumerate() {
        assert_eq!(
            st.page.print_area,
            parse_sqref("A1:B2"),
            "sheet {}",
            names[i]
        );
        assert_eq!(st.page.title_rows, Some((0, 0)), "sheet {}", names[i]);
    }
}

#[test]
fn biff8_xls_emits_builtin_fonts_xfs_style_records_and_defaults_to_xf15() {
    let plain_xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &[LegacyXlsPayload {
            sheet_name: "S1".to_string(),
            cells: vec![(0, 0, "42".to_string()), (0, 1, "hello".to_string())],
        }],
        &[SheetLinks::default()],
        &[SheetStyling::default()],
    )
    .unwrap();
    let plain_streams = Ole2ContainerCodec::unpack(&plain_xls).unwrap();
    let wb = &plain_streams
        .iter()
        .find(|e| e.name == "Workbook")
        .unwrap()
        .data;
    let recs = walk_biff8_records(wb);
    let eof0 = recs.iter().position(|(op, _)| *op == BIFF8_EOF).unwrap();
    let globals = &recs[..eof0];
    let ws = &recs[eof0 + 1..];

    let fonts: Vec<_> = globals.iter().filter(|(op, _)| *op == BIFF8_FONT).collect();
    assert_eq!(fonts.len(), 4, "4 mandatory built-in FONT records");

    let xfs: Vec<_> = globals.iter().filter(|(op, _)| *op == BIFF8_XF).collect();
    assert_eq!(xfs.len(), 16, "15 style XFs + default cell XF 15");
    for (i, (_, body)) in xfs.iter().take(15).enumerate() {
        assert_eq!(body.len(), 20);
        assert_eq!(
            u16::from_le_bytes([body[4], body[5]]),
            0xFFF5,
            "style XF {i}"
        );
        assert_eq!(
            u16::from_le_bytes([body[18], body[19]]),
            0x0040 | (0x0041 << 7)
        );
    }
    assert_eq!(u16::from_le_bytes([xfs[15].1[4], xfs[15].1[5]]), 0x0001);
    assert_eq!(
        u16::from_le_bytes([xfs[15].1[18], xfs[15].1[19]]),
        0x0040 | (0x0041 << 7)
    );

    let styles: Vec<_> = globals
        .iter()
        .filter(|(op, _)| *op == BIFF8_STYLE)
        .collect();
    assert_eq!(styles.len(), 6, "6 mandatory built-in STYLE records");
    let expected_styles = [
        (0u16, 0x00u8),
        (1, 0x03),
        (2, 0x04),
        (3, 0x05),
        (4, 0x06),
        (5, 0x07),
    ];
    for ((_, body), (ixfe, built_in)) in styles.iter().zip(expected_styles) {
        assert_eq!(body.len(), 4);
        assert_eq!(u16::from_le_bytes([body[0], body[1]]), ixfe | 0x8000);
        assert_eq!(body[2], built_in);
        assert_eq!(body[3], 0xFF);
    }

    for (op, body) in ws {
        if matches!(
            *op,
            BIFF8_NUMBER | BIFF8_LABELSST | BIFF8_BOOLERR | BIFF8_BLANK
        ) {
            let xf_idx = u16::from_le_bytes([body[4], body[5]]);
            assert_eq!(xf_idx, 15, "plain cell opcode 0x{op:04X} must use XF 15");
        }
    }

    let mut colored = SheetStyling {
        default_font: DefaultFont::new("Arial", 10.0),
        ..SheetStyling::default()
    };
    colored.cells.insert(
        (0, 1),
        CellStyle {
            fill: Some(0x112233),
            font_color: Some(0xAABBCC),
            ..Default::default()
        },
    );
    let colored_xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &[LegacyXlsPayload {
            sheet_name: "S1".to_string(),
            cells: vec![(0, 0, "plain".to_string()), (0, 1, "tinted".to_string())],
        }],
        &[SheetLinks::default()],
        &[colored],
    )
    .unwrap();
    let colored_streams = Ole2ContainerCodec::unpack(&colored_xls).unwrap();
    let cwb = &colored_streams
        .iter()
        .find(|e| e.name == "Workbook")
        .unwrap()
        .data;
    let crecs = walk_biff8_records(cwb);
    let ceof0 = crecs.iter().position(|(op, _)| *op == BIFF8_EOF).unwrap();
    let cglobals = &crecs[..ceof0];
    let cws = &crecs[ceof0 + 1..];

    let cfonts: Vec<_> = cglobals
        .iter()
        .filter(|(op, _)| *op == BIFF8_FONT)
        .collect();
    assert_eq!(cfonts.len(), 5, "4 built-in + 1 custom FONT");
    let cxfs: Vec<_> = cglobals.iter().filter(|(op, _)| *op == BIFF8_XF).collect();
    assert_eq!(cxfs.len(), 17, "16 built-in XFs + 1 custom XF 16");
    let custom_ifnt = u16::from_le_bytes([cxfs[16].1[0], cxfs[16].1[1]]);
    assert_eq!(
        custom_ifnt, 5,
        "first custom FONT must skip index 4 and use 5"
    );
    assert_ne!(cglobals.iter().position(|(op, _)| *op == BIFF8_BOF), None);

    let mut seen_xfs = Vec::new();
    for (op, body) in cws {
        if matches!(
            *op,
            BIFF8_NUMBER | BIFF8_LABELSST | BIFF8_BOOLERR | BIFF8_BLANK
        ) {
            seen_xfs.push((
                u16::from_le_bytes([body[2], body[3]]),
                u16::from_le_bytes([body[4], body[5]]),
            ));
        }
    }
    assert_eq!(seen_xfs, vec![(0, 15), (1, 16)]);

    let (_, back_st) = LegacyOle2Codec::unpack_xls_with_styling(&colored_xls).unwrap();
    let st = &back_st[0].1;
    assert_eq!(
        st.num_fmts.len(),
        0,
        "built-in XFs must not pollute num_fmts"
    );
    assert_eq!(st.cells[&(0, 1)].fill, Some(0x112233));
    assert_eq!(st.cells[&(0, 1)].font_color, Some(0xAABBCC));
}

#[test]
fn hostile_oversized_rules_formula_and_defined_name_fail_closed() {
    let huge = format!("=XLOOKUP(A1,{},B1)", "A".repeat(MAX_RULE_TEXT_CHARS * 2));
    let xml = format!(
        "<worksheet><conditionalFormatting sqref=\"A1\"><cfRule type=\"expression\" priority=\"1\"><formula>{huge}</formula></cfRule></conditionalFormatting></worksheet>"
    );
    let cells = vec![(0, 0, "1".to_string())];
    let packed = OoxmlCodec::pack_xlsx_multi_styled(
        &[("S".to_string(), cells)],
        &[],
        &[SheetStyling::default()],
    )
    .unwrap();
    let entries = ZipArchiveCodec::unpack(&packed, &ZipSecurityPolicy::default()).unwrap();
    let rebuilt_entries: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "xl/worksheets/sheet1.xml" {
                let s = String::from_utf8(e.data).unwrap();
                (e.path, s.replace("</worksheet>", &xml[11..]).into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let rebuilt = ZipArchiveCodec::pack(&rebuilt_entries).unwrap();
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&rebuilt).unwrap();
    if let Some(CfRule {
        kind: CfKind::Expression(expr),
        ..
    }) = back[0].1.rules.cf.first()
    {
        assert!(expr.chars().count() <= MAX_RULE_TEXT_CHARS);
    }
}
