//! Round-trip and security-boundary tests for XLSX `<rPh>` / `<phoneticPr>` phonetic guides (PD-319).

use easyoffice_formats::{
    CellPhonetic, EasyOfficeError, OoxmlCodec, PhoneticAlign, PhoneticKind, PhoneticRun,
    SheetStyling, ZipArchiveCodec, MAX_PHONETIC_RUNS_PER_CELL,
};

fn build_raw_xlsx(shared_xml: Option<&str>, sheet_xml: &str) -> Vec<u8> {
    let wb = r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="S" sheetId="1"/></sheets></workbook>"#;
    let mut entries = vec![
        ("xl/workbook.xml".to_string(), wb.as_bytes().to_vec()),
        (
            "xl/worksheets/sheet1.xml".to_string(),
            sheet_xml.as_bytes().to_vec(),
        ),
    ];
    if let Some(sst) = shared_xml {
        entries.push(("xl/sharedStrings.xml".to_string(), sst.as_bytes().to_vec()));
    }
    ZipArchiveCodec::pack(&entries).expect("pack test xlsx")
}

fn entry_text(zip_bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(zip_bytes, &Default::default()).expect("unpack zip");
    let found = entries
        .into_iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("missing zip entry {path}"));
    String::from_utf8(found.data).expect("utf-8 entry")
}

#[test]
fn xlsx_shared_strings_and_inline_rph_roundtrip_all_kana_modes() {
    let sst = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="7" uniqueCount="7">"#,
        r#"<si><t>申し込む</t><rPh sb="0" eb="1"><t>もう</t></rPh><rPh sb="2" eb="3"><t>こ</t></rPh><phoneticPr fontId="1"/></si>"#,
        r#"<si><t>東京都</t><rPh sb="0" eb="3"><t>トウキョウト</t></rPh><phoneticPr fontId="0" type="Hiragana" alignment="distributed"/></si>"#,
        r#"<si><t>大阪府</t><rPh sb="0" eb="3"><t>ｵｵｻｶﾌ</t></rPh><phoneticPr fontId="0" type="halfwidthKatakana"/></si>"#,
        r#"<si><t>ｶﾞｷﾞ</t><rPh sb="0" eb="2"><t>ｶﾞ</t></rPh><phoneticPr fontId="0" type="fullwidthKatakana"/></si>"#,
        r#"<si><t>A_x0041_漢字</t><rPh sb="2" eb="4"><t>カン_x005A_ジ</t></rPh><phoneticPr fontId="0" type="noConversion" alignment="center"/></si>"#,
        r#"<si><t>𠮷野家</t><rPh sb="0" eb="2"><t>ヨシ</t></rPh><rPh sb="2" eb="4"><t>ノヤ</t></rPh><phoneticPr fontId="0"/></si>"#,
        r#"<si><t>佐藤</t></si>"#,
        r#"</sst>"#,
    );
    let sheet = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        r#"<row r="1">"#,
        r#"<c r="A1" t="s"><v>0</v></c>"#,
        r#"<c r="B1" t="s"><v>1</v></c>"#,
        r#"<c r="C1" t="s"><v>2</v></c>"#,
        r#"<c r="D1" t="s"><v>3</v></c>"#,
        r#"<c r="E1" t="s"><v>4</v></c>"#,
        r#"<c r="F1" t="s"><v>5</v></c>"#,
        r#"<c r="G1" t="inlineStr"><is><t>北海道</t><rPh sb="0" eb="3"><t>ホッカイドウ</t></rPh><phoneticPr fontId="0" type="Hiragana"/></is></c>"#,
        r#"<c r="H1" t="s"><v>6</v></c>"#,
        r#"</row>"#,
        r#"<row r="2">"#,
        // Same base text `"佐藤"` in A2 with phonetics vs H1 without phonetics must stay separated in SST!
        r#"<c r="A2" t="inlineStr"><is><t>佐藤</t><rPh sb="0" eb="2"><t>サトウ</t></rPh><phoneticPr fontId="0"/></is></c>"#,
        r#"</row>"#,
        r#"</sheetData></worksheet>"#,
    );

    let input = build_raw_xlsx(Some(sst), sheet);
    let (sheets, named_stylings) = OoxmlCodec::unpack_xlsx_with_styling(&input).expect("unpack");
    let stylings: Vec<SheetStyling> = named_stylings.into_iter().map(|(_, s)| s).collect();
    let (cells, st) = (&sheets[0].1, &stylings[0]);

    let ph_at = |r: u32, c: u32| -> &CellPhonetic {
        let raw = &cells
            .iter()
            .find(|&&(rr, cc, _)| rr == r && cc == c)
            .unwrap()
            .2;
        st.cell_phonetic(r, c, raw).unwrap()
    };

    assert_eq!(ph_at(0, 0).reading(), "モウシコム");
    assert_eq!(ph_at(0, 1).reading(), "とうきょうと");
    assert_eq!(ph_at(0, 1).align, PhoneticAlign::Distributed);
    assert_eq!(ph_at(0, 2).reading(), "ｵｵｻｶﾌ");
    assert_eq!(ph_at(0, 3).reading(), "ガギ");
    assert_eq!(ph_at(0, 4).reading(), "AAカンZジ");
    assert_eq!(ph_at(0, 4).align, PhoneticAlign::Center);
    assert_eq!(ph_at(0, 5).reading(), "ヨシノヤ");
    assert_eq!(ph_at(0, 6).reading(), "ほっかいどう");
    assert!(st.cell_phonetic(0, 7, "佐藤").is_none());
    assert_eq!(ph_at(1, 0).reading(), "サトウ");

    // Re-pack through pack_xlsx_multi_styled -> verify separate SST items and round-trip fidelity.
    let repacked = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &stylings).expect("repack");
    let sst_out = entry_text(&repacked, "xl/sharedStrings.xml");
    assert!(sst_out.contains("<rPh sb=\"0\" eb=\"1\"><t>もう</t></rPh>"));
    assert!(sst_out.contains("alignment=\"distributed\""));
    assert!(sst_out.contains("<si><t>佐藤</t></si>"));
    assert!(sst_out.contains("<si><t>佐藤</t><rPh sb=\"0\" eb=\"2\"><t>サトウ</t></rPh>"));

    let (sheets2, stylings2) = OoxmlCodec::unpack_xlsx_with_styling(&repacked).expect("reunpack");
    let st2 = &stylings2[0].1;
    let raw2 = |r: u32, c: u32| -> &str {
        &sheets2[0]
            .1
            .iter()
            .find(|&&(rr, cc, _)| rr == r && cc == c)
            .unwrap()
            .2
    };
    assert_eq!(
        st2.cell_phonetic(0, 0, raw2(0, 0)).unwrap().reading(),
        "モウシコム"
    );
    assert_eq!(
        st2.cell_phonetic(0, 1, raw2(0, 1)).unwrap().reading(),
        "とうきょうと"
    );
    assert!(st2.cell_phonetic(0, 7, raw2(0, 7)).is_none());
    assert_eq!(
        st2.cell_phonetic(1, 0, raw2(1, 0)).unwrap().reading(),
        "サトウ"
    );
}

#[test]
fn xlsx_drops_stale_phonetics_when_base_text_changes_and_rejects_malformed_runs() {
    let mut styling = SheetStyling::default();
    let ph = CellPhonetic::new(
        "東京都".to_string(),
        vec![PhoneticRun {
            sb: 0,
            eb: 3,
            text: "トウキョウト".to_string(),
        }],
        PhoneticKind::FullwidthKatakana,
        PhoneticAlign::NoControl,
    )
    .unwrap();
    styling.set_cell_phonetic(0, 0, "東京都", ph.clone());
    // Mutating base text makes `cell_phonetic` yield `None` so writer never writes stale `<rPh>`.
    let changed_sheets = vec![("S".to_string(), vec![(0, 0, "大阪府".to_string())])];
    let out = OoxmlCodec::pack_xlsx_multi_styled(&changed_sheets, &[], &[styling]).unwrap();
    let sst_out = entry_text(&out, "xl/sharedStrings.xml");
    assert!(!sst_out.contains("<rPh"));
    assert!(sst_out.contains("大阪府"));

    // Surrogate-splitting, out-of-bounds, reversed, and overlapping runs are discarded cleanly.
    let bad = CellPhonetic::new(
        "𠮷野".to_string(),
        vec![
            PhoneticRun {
                sb: 1,
                eb: 2,
                text: "BAD".to_string(),
            },
            PhoneticRun {
                sb: 2,
                eb: 9,
                text: "OOB".to_string(),
            },
            PhoneticRun {
                sb: 2,
                eb: 2,
                text: "EMPTY".to_string(),
            },
            PhoneticRun {
                sb: 2,
                eb: 3,
                text: "ノ".to_string(),
            },
            PhoneticRun {
                sb: 0,
                eb: 2,
                text: "LATE".to_string(),
            },
        ],
        PhoneticKind::NoConversion,
        PhoneticAlign::NoControl,
    )
    .unwrap();
    assert_eq!(bad.runs.len(), 2);
    assert_eq!(bad.reading(), "LATEノ");
}

#[test]
fn hostile_rph_counts_and_fanout_budgets_fail_closed() {
    // 1. Missing sb/eb `<rPh/>` repeated beyond MAX_PHONETIC_RUNS_PER_CELL is rejected.
    let excess_runs: String = (0..=MAX_PHONETIC_RUNS_PER_CELL)
        .map(|_| "<rPh><t>x</t></rPh>")
        .collect();
    let sst = format!("<sst><si><t>漢字</t>{excess_runs}</si></sst>");
    let bytes = build_raw_xlsx(Some(&sst), "<worksheet><sheetData/></worksheet>");
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&bytes),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. Excess phonetic bytes inside one inlineStr cell fail closed cleanly.
    let huge_t = "ア".repeat(50_000); // 150,000 bytes > MAX_PHONETIC_TEXT_BYTES (131,068)
    let sheet_huge = format!(
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>漢</t><rPh sb=\"0\" eb=\"1\"><t>{huge_t}</t></rPh></is></c></row></sheetData></worksheet>"
    );
    let bytes_huge = build_raw_xlsx(None, &sheet_huge);
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&bytes_huge),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 3. Fan-out: many cells referencing an SST entry with large valid `<rPh>` hit DecodeBudget (256 MiB).
    let ph_chunk = "ア".repeat(40_000); // 120,000 UTF-8 bytes per cell (< 131,068 per cell)
    let sst_fan =
        format!("<sst><si><t>漢</t><rPh sb=\"0\" eb=\"1\"><t>{ph_chunk}</t></rPh></si></sst>");
    let cells: String = (1..=2_500)
        .map(|r| format!("<c r=\"A{r}\" t=\"s\"><v>0</v></c>"))
        .collect();
    let sheet_fan =
        format!("<worksheet><sheetData><row r=\"1\">{cells}</row></sheetData></worksheet>");
    let bytes_fan = build_raw_xlsx(Some(&sst_fan), &sheet_fan);
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&bytes_fan),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
