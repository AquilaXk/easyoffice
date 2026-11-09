//! Open -> edit -> save round trips for the structured decode path (`decode_rich`), plus the
//! security guards (ZIP bomb, path traversal, XXE) on that path.

use easyoffice_formats::{
    DecodedContent, DecodedDocument, DocumentFormat, EasyOfficeError, OoxmlCodec, RichAlignment,
    RichBlock, RichDocument, RichListKind, RichParagraph, RichRun, UniversalDocumentModel,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn run(text: &str, bold: bool, italic: bool, underline: bool, strike: bool) -> RichRun {
    RichRun {
        text: text.to_string(),
        bold,
        italic,
        underline,
        strike,
        ..RichRun::default()
    }
}

fn para(
    runs: Vec<RichRun>,
    alignment: Option<RichAlignment>,
    heading_level: u8,
    list: Option<RichListKind>,
) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs,
        alignment,
        heading_level,
        list,
        ..RichParagraph::default()
    })
}

fn table(rows: &[&[&str]]) -> RichBlock {
    RichBlock::Table(
        rows.iter()
            .map(|r| r.iter().map(ToString::to_string).collect())
            .collect::<Vec<Vec<String>>>()
            .into(),
    )
}

fn sample_blocks(with_ordered: bool, with_justify: bool) -> Vec<RichBlock> {
    let mut blocks = vec![
        para(
            vec![run("개요", false, false, false, false)],
            Some(RichAlignment::Center),
            1,
            None,
        ),
        para(
            vec![
                run("앞 ", false, false, false, false),
                run("굵게", true, false, false, false),
                run("기울임", false, true, false, false),
                run("밑줄", false, false, true, false),
                run("취소선", false, false, false, true),
                run("둘다", true, true, false, false),
            ],
            Some(RichAlignment::Right),
            0,
            None,
        ),
        table(&[&["부서", "금액"], &["기획", "1,000"]]),
        para(
            vec![run("표 뒤 문단", false, false, false, false)],
            None,
            0,
            None,
        ),
        para(
            vec![run("항목 하나", false, false, false, false)],
            None,
            0,
            Some(RichListKind::Bullet),
        ),
        para(
            vec![run("소제목", false, false, false, false)],
            Some(RichAlignment::Left),
            2,
            None,
        ),
    ];
    if with_ordered {
        blocks.push(para(
            vec![run("첫째", false, false, false, false)],
            None,
            0,
            Some(RichListKind::Ordered),
        ));
    }
    if with_justify {
        blocks.push(para(
            vec![run("양쪽 맞춤", false, false, false, false)],
            Some(RichAlignment::Justify),
            0,
            None,
        ));
    }
    blocks
}

fn document_of(decoded: &DecodedDocument) -> RichDocument {
    match &decoded.content {
        DecodedContent::Document(doc) => doc.clone(),
        other => panic!("document expected, got {other:?}"),
    }
}

/// decode -> model -> encode -> decode must reproduce the first decode.
fn assert_document_roundtrip(format: DocumentFormat, name: &str, blocks: Vec<RichBlock>) {
    let doc = RichDocument {
        title: "라운드트립".to_string(),
        blocks,
        official_agency: None,
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, format, None).unwrap();
    let first = UniversalFormatConverter::decode_rich(&bytes, Some(name)).unwrap();
    assert_eq!(first.format, format, "{name}");
    let first_doc = document_of(&first);
    assert_eq!(first_doc.blocks, doc.blocks, "{name}: decoded blocks");

    let bytes2 = UniversalFormatConverter::encode_rich(&first_doc, format, None).unwrap();
    let second = UniversalFormatConverter::decode_rich(&bytes2, Some(name)).unwrap();
    assert_eq!(document_of(&second).blocks, first_doc.blocks, "{name}: 2nd");
}

#[test]
fn docx_roundtrip_keeps_runs_alignment_headings_lists_and_tables_in_place() {
    assert_document_roundtrip(
        DocumentFormat::Docx,
        "보고서.docx",
        sample_blocks(true, true),
    );
}

#[test]
fn hwpx_roundtrip_keeps_runs_alignment_headings_lists_and_tables_in_place() {
    assert_document_roundtrip(
        DocumentFormat::Hwpx,
        "기안문.hwpx",
        sample_blocks(true, true),
    );
}

#[test]
fn odt_roundtrip_keeps_runs_alignment_headings_lists_and_tables_in_place() {
    assert_document_roundtrip(DocumentFormat::Odt, "문서.odt", sample_blocks(true, true));
}

#[test]
fn rtf_roundtrip_keeps_ordered_and_bullet_lists_and_alignments() {
    assert_document_roundtrip(DocumentFormat::Rtf, "문서.rtf", sample_blocks(true, true));
}

#[test]
fn hwpx_distribute_and_justify_alignments_decode_as_justify() {
    let header_xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<hh:head xmlns:hh=\"http://www.owpml.org/owpml/2011/head\" xmlns:hc=\"http://www.owpml.org/owpml/2011/core\">",
        "<hh:refList>",
        "<hh:paraProperties itemCnt=\"3\">",
        "<hh:paraPr id=\"0\"><hh:align horizontal=\"JUSTIFY\" vertical=\"BASELINE\"/></hh:paraPr>",
        "<hh:paraPr id=\"1\"><hh:align horizontal=\"DISTRIBUTE\" vertical=\"BASELINE\"/></hh:paraPr>",
        "<hh:paraPr id=\"2\"><hh:align horizontal=\"DISTRIBUTE_SPACE\" vertical=\"BASELINE\"/></hh:paraPr>",
        "</hh:paraProperties>",
        "</hh:refList></hh:head>"
    );
    let section_xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<hs:sec xmlns:hs=\"http://www.owpml.org/owpml/2011/section\" xmlns:hp=\"http://www.owpml.org/owpml/2011/paragraph\">",
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>양쪽</hp:t></hp:run></hp:p>",
        "<hp:p paraPrIDRef=\"1\"><hp:run charPrIDRef=\"0\"><hp:t>배분</hp:t></hp:run></hp:p>",
        "<hp:p paraPrIDRef=\"2\"><hp:run charPrIDRef=\"0\"><hp:t>공백배분</hp:t></hp:run></hp:p>",
        "</hs:sec>"
    );
    let bytes = ZipArchiveCodec::pack(&[
        ("mimetype".to_string(), b"application/hwp+zip".to_vec()),
        (
            "Contents/header.xml".to_string(),
            header_xml.as_bytes().to_vec(),
        ),
        (
            "Contents/section0.xml".to_string(),
            section_xml.as_bytes().to_vec(),
        ),
    ])
    .unwrap();
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("align.hwpx")).unwrap();
    let doc = document_of(&decoded);
    let aligns: Vec<Option<RichAlignment>> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.alignment),
            _ => None,
        })
        .collect();
    assert_eq!(
        aligns,
        vec![
            Some(RichAlignment::Justify),
            Some(RichAlignment::Justify),
            Some(RichAlignment::Justify),
        ]
    );
}

#[test]
fn non_finite_float_strings_are_preserved_as_text_cells_in_xls_xlsx_and_ods() {
    use easyoffice_formats::{LegacyOle2Codec, LegacyXlsPayload, OdfCodec};
    let cells = vec![
        (0, 0, "NaN".to_string()),
        (0, 1, "inf".to_string()),
        (0, 2, "-inf".to_string()),
        (0, 3, "Infinity".to_string()),
        (0, 4, "42.5".to_string()),
    ];
    let xls = LegacyOle2Codec::pack_xls(&LegacyXlsPayload {
        sheet_name: "S1".to_string(),
        cells: cells.clone(),
    })
    .unwrap();
    let unpacked_xls = LegacyOle2Codec::unpack_xls(&xls).unwrap();
    assert_eq!(unpacked_xls.cells, cells);

    let xlsx = OoxmlCodec::pack_xlsx_multi(&[(String::from("S1"), cells.clone())]).unwrap();
    let unpacked_xlsx = OoxmlCodec::unpack_xlsx_multi(&xlsx).unwrap();
    assert_eq!(unpacked_xlsx[0].1, cells);

    let ods = OdfCodec::pack_ods("S1", &cells).unwrap();
    let entries = ZipArchiveCodec::unpack(&ods, &ZipSecurityPolicy::default()).unwrap();
    let content_xml = entries
        .iter()
        .find(|e| e.path == "content.xml")
        .map(|e| std::str::from_utf8(&e.data).unwrap())
        .unwrap();
    assert!(
        !content_xml.contains("office:value=\"NaN\""),
        "{content_xml}"
    );
    assert!(
        !content_xml.contains("office:value=\"inf\""),
        "{content_xml}"
    );
    let unpacked_ods = OdfCodec::unpack_ods(&ods).unwrap();
    assert_eq!(unpacked_ods.1, cells);
}

#[test]
fn biff8_boolerr_and_formula_cached_bool_and_error_cells_decode() {
    use easyoffice_formats::{LegacyOle2Codec, Ole2ContainerCodec};
    let mut wb = Vec::new();
    let push_rec = |buf: &mut Vec<u8>, opcode: u16, data: &[u8]| {
        buf.extend_from_slice(&opcode.to_le_bytes());
        buf.extend_from_slice(&(data.len() as u16).to_le_bytes());
        buf.extend_from_slice(data);
    };
    // Workbook globals BOF
    push_rec(
        &mut wb,
        LegacyOle2Codec::BIFF8_BOF,
        &[0x00, 0x06, 0x05, 0x00],
    );
    // BoundSheet "Sheet1"
    push_rec(
        &mut wb,
        LegacyOle2Codec::BIFF8_BOUNDSHEET,
        &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, b'S', b'h', b'e', b'e', b't', b'1',
        ],
    );
    push_rec(&mut wb, LegacyOle2Codec::BIFF8_EOF, &[]);
    // Worksheet BOF
    push_rec(
        &mut wb,
        LegacyOle2Codec::BIFF8_BOF,
        &[0x00, 0x06, 0x10, 0x00],
    );
    // BOOLERR (0x0205) row=0, col=0: FALSE (b6=0, b7=0)
    push_rec(
        &mut wb,
        0x0205,
        &[0x00, 0x00, 0x00, 0x00, 0x0F, 0x00, 0x00, 0x00],
    );
    // BOOLERR (0x0205) row=0, col=1: TRUE (b6=0, b7=1)
    push_rec(
        &mut wb,
        0x0205,
        &[0x00, 0x00, 0x01, 0x00, 0x0F, 0x00, 0x00, 0x01],
    );
    // BOOLERR (0x0205) row=0, col=2: #DIV/0! (b6=1, b7=0x07)
    push_rec(
        &mut wb,
        0x0205,
        &[0x00, 0x00, 0x02, 0x00, 0x0F, 0x00, 0x01, 0x07],
    );
    // BOOLERR (0x0205) row=0, col=3: #NULL! (b6=1, b7=0x00)
    push_rec(
        &mut wb,
        0x0205,
        &[0x00, 0x00, 0x03, 0x00, 0x0F, 0x00, 0x01, 0x00],
    );
    // FORMULA (0x0006) row=1, col=0: cached boolean TRUE (byte6=1, byte8=1, byte12..14=0xFFFF)
    push_rec(
        &mut wb,
        LegacyOle2Codec::BIFF8_FORMULA,
        &[
            0x01, 0x00, 0x00, 0x00, 0x0F, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF,
            0x00, 0x00,
        ],
    );
    // FORMULA (0x0006) row=1, col=1: cached error #DIV/0! (byte6=2, byte8=0x07, byte12..14=0xFFFF)
    push_rec(
        &mut wb,
        LegacyOle2Codec::BIFF8_FORMULA,
        &[
            0x01, 0x00, 0x01, 0x00, 0x0F, 0x00, 0x02, 0x00, 0x07, 0x00, 0x00, 0x00, 0xFF, 0xFF,
            0x00, 0x00,
        ],
    );
    push_rec(&mut wb, LegacyOle2Codec::BIFF8_EOF, &[]);
    let ole = Ole2ContainerCodec::pack(&[("Workbook".to_string(), wb)]).unwrap();
    let unpacked = LegacyOle2Codec::unpack_xls(&ole).unwrap();
    assert_eq!(
        unpacked.cells,
        vec![
            (0, 0, "FALSE".to_string()),
            (0, 1, "TRUE".to_string()),
            (0, 2, "#DIV/0!".to_string()),
            (0, 3, "#NULL!".to_string()),
            (1, 0, "TRUE".to_string()),
            (1, 1, "#DIV/0!".to_string()),
        ]
    );
}

#[test]
fn xlsx_boolean_cells_and_shared_formulas_decode() {
    let workbook = r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="S1" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets></workbook>"#;
    let sheet = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        r#"<row r="1"><c r="A1" t="b"><v>1</v></c><c r="B1" t="b"><v>0</v></c><c r="C1"><f t="shared" ref="C1:C2" si="0">A1+10</f><v>11</v></c></row>"#,
        r#"<row r="2"><c r="A2"><v>5</v></c><c r="C2"><f t="shared" si="0"/><v>15</v></c><c r="D2"><f t="shared" si="9"/><v>99</v></c></row>"#,
        r#"</sheetData></worksheet>"#
    );
    let bytes = ZipArchiveCodec::pack(&[
        ("xl/workbook.xml".to_string(), workbook.as_bytes().to_vec()),
        (
            "xl/worksheets/sheet1.xml".to_string(),
            sheet.as_bytes().to_vec(),
        ),
    ])
    .unwrap();
    let sheets = OoxmlCodec::unpack_xlsx_multi(&bytes).unwrap();
    let cells = &sheets[0].1;
    assert!(cells.contains(&(0, 0, "TRUE".to_string())), "{cells:?}");
    assert!(cells.contains(&(0, 1, "FALSE".to_string())), "{cells:?}");
    assert!(cells.contains(&(0, 2, "=A1+10".to_string())), "{cells:?}");
    assert!(cells.contains(&(1, 2, "=A2+10".to_string())), "{cells:?}");
    assert!(cells.contains(&(1, 3, "99".to_string())), "{cells:?}");
}

#[test]
fn odp_pack_emits_standard_draw_frame_text_box_and_presentation_notes() {
    use easyoffice_formats::OdfCodec;
    let slides = vec![(
        "제목 슬라이드".to_string(),
        vec!["첫 항목".to_string(), "둘째 항목".to_string()],
        "발표자 노트".to_string(),
    )];
    let bytes = OdfCodec::pack_odp("발표 문서", &slides).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let xml = entries
        .iter()
        .find(|e| e.path == "content.xml")
        .map(|e| std::str::from_utf8(&e.data).unwrap())
        .unwrap();
    assert!(xml.contains("<draw:frame draw:layer=\"layout\""), "{xml}");
    assert!(xml.contains("<draw:text-box>"), "{xml}");
    assert!(xml.contains("<presentation:notes><draw:frame"), "{xml}");
    let (title, unpacked) = OdfCodec::unpack_odp(&bytes).unwrap();
    assert_eq!(title, "발표 문서");
    assert_eq!(unpacked, slides);
}

#[test]
fn markdown_codec_parses_h5_and_h6_headings() {
    use easyoffice_formats::{CommonMarkCodec, MarkdownBlock};
    let blocks = CommonMarkCodec::parse("# H1\n##### H5 제목\n###### H6 제목\n").unwrap();
    assert!(blocks.contains(&MarkdownBlock::Heading {
        level: 5,
        text: "H5 제목".to_string()
    }));
    assert!(blocks.contains(&MarkdownBlock::Heading {
        level: 6,
        text: "H6 제목".to_string()
    }));
}

#[test]
fn docx_table_cell_with_line_breaks_and_tabs_roundtrips() {
    let blocks = vec![
        table(&[&["가\n나", "x\ty"]]),
        para(vec![run("끝", false, false, false, false)], None, 0, None),
    ];
    assert_document_roundtrip(DocumentFormat::Docx, "표.docx", blocks);
}

const MARKDOWN_SOURCE: &str = "# 보고서\n\n일반 **굵게** *기울임* <u>밑줄</u> ~~취소~~ 문단\\\n둘째 줄\n\n- 항목 하나\n- 항목 둘\n\n1. 첫째\n2. 둘째\n\n| 부서 | 금액 |\n| --- | --- |\n| 기획 | 1,000 |\n\n## 소제목\n";

#[test]
fn markdown_decode_extracts_formatting_lists_and_tables_then_roundtrips() {
    let first =
        UniversalFormatConverter::decode_rich(MARKDOWN_SOURCE.as_bytes(), Some("a.md")).unwrap();
    assert_eq!(first.format, DocumentFormat::Markdown);
    let doc = document_of(&first);
    let RichBlock::Paragraph(heading) = &doc.blocks[0] else {
        panic!("heading expected");
    };
    assert_eq!(
        (heading.heading_level, heading.plain_text()),
        (1, "보고서".to_string())
    );
    let RichBlock::Paragraph(body) = &doc.blocks[1] else {
        panic!("paragraph expected");
    };
    assert_eq!(
        body.runs,
        vec![
            run("일반 ", false, false, false, false),
            run("굵게", true, false, false, false),
            run(" ", false, false, false, false),
            run("기울임", false, true, false, false),
            run(" ", false, false, false, false),
            run("밑줄", false, false, true, false),
            run(" ", false, false, false, false),
            run("취소", false, false, false, true),
            run(" 문단\n둘째 줄", false, false, false, false),
        ]
    );
    let lists: Vec<_> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => p.list.map(|l| (l, p.plain_text())),
            RichBlock::Table(_) | RichBlock::Image(_) => None,
        })
        .collect();
    assert_eq!(
        lists,
        vec![
            (RichListKind::Bullet, "항목 하나".to_string()),
            (RichListKind::Bullet, "항목 둘".to_string()),
            (RichListKind::Ordered, "첫째".to_string()),
            (RichListKind::Ordered, "둘째".to_string()),
        ]
    );
    assert!(doc
        .blocks
        .contains(&table(&[&["부서", "금액"], &["기획", "1,000"]])));

    let bytes2 =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Markdown, None).unwrap();
    let second = UniversalFormatConverter::decode_rich(&bytes2, Some("a.md")).unwrap();
    assert_eq!(document_of(&second).blocks, doc.blocks);
}

#[test]
fn markdown_escaped_markers_stay_literal_text() {
    let first = UniversalFormatConverter::decode_rich(
        "문장 \\*별표\\* 와 snake_case_name 그리고 2 * 3\n".as_bytes(),
        Some("a.md"),
    )
    .unwrap();
    let doc = document_of(&first);
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("paragraph expected");
    };
    assert_eq!(
        p.plain_text(),
        "문장 *별표* 와 snake_case_name 그리고 2 * 3"
    );
    assert!(p.runs.iter().all(|r| !r.bold && !r.italic));
}

#[test]
fn txt_lines_become_paragraphs() {
    let first = UniversalFormatConverter::decode_rich(
        "첫 줄\r\n\r\n둘째 줄\n".as_bytes(),
        Some("메모.txt"),
    )
    .unwrap();
    assert_eq!(first.format, DocumentFormat::Txt);
    let texts: Vec<String> = document_of(&first)
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => p.plain_text(),
            RichBlock::Table(_) | RichBlock::Image(_) => String::new(),
        })
        .collect();
    assert_eq!(texts, vec!["첫 줄".to_string(), "둘째 줄".to_string()]);
}

#[test]
fn xlsx_roundtrip_keeps_every_sheet_and_formulas() {
    let sheets = vec![
        (
            "매출".to_string(),
            vec![
                (0, 0, "월".to_string()),
                (0, 1, "금액".to_string()),
                (1, 0, "1월".to_string()),
                (1, 1, "100".to_string()),
                (2, 0, "2월".to_string()),
                (2, 1, "250".to_string()),
                (3, 1, "=SUM(B2:B3)".to_string()),
            ],
        ),
        (
            "요약".to_string(),
            vec![
                (0, 0, "='매출'!B4*2".to_string()),
                (1, 2, "메모".to_string()),
            ],
        ),
    ];
    let bytes = OoxmlCodec::pack_xlsx_multi(&sheets).unwrap();
    let first = UniversalFormatConverter::decode_rich(&bytes, Some("통계.xlsx")).unwrap();
    assert_eq!(first.format, DocumentFormat::Xlsx);
    let DecodedContent::Workbook(decoded) = &first.content else {
        panic!("workbook expected");
    };
    assert_eq!(decoded.len(), 2);
    for ((name, cells), (want_name, want_cells)) in decoded.iter().zip(&sheets) {
        assert_eq!(name, want_name);
        let mut got = cells.clone();
        got.sort();
        let mut want = want_cells.clone();
        want.sort();
        assert_eq!(got, want);
    }
    let bytes2 = OoxmlCodec::pack_xlsx_multi(decoded).unwrap();
    let second = UniversalFormatConverter::decode_rich(&bytes2, Some("통계.xlsx")).unwrap();
    let (DecodedContent::Workbook(a), DecodedContent::Workbook(b)) =
        (&first.content, &second.content)
    else {
        panic!("workbook expected");
    };
    assert_eq!(a, b);
}

#[test]
fn csv_with_bom_decodes_without_bom_in_first_cell_and_roundtrips() {
    let mut raw = vec![0xEF, 0xBB, 0xBF];
    raw.extend_from_slice("이름,값\r\n가,1\r\n나,\"a,b\"\r\n".as_bytes());
    let first = UniversalFormatConverter::decode_rich(&raw, Some("데이터.csv")).unwrap();
    assert_eq!(first.format, DocumentFormat::Csv);
    let DecodedContent::Workbook(sheets) = &first.content else {
        panic!("workbook expected");
    };
    assert_eq!(sheets.len(), 1);
    let cells = &sheets[0].1;
    assert!(cells.contains(&(0, 0, "이름".to_string())), "{cells:?}");
    assert!(cells.contains(&(2, 1, "a,b".to_string())), "{cells:?}");

    let model = UniversalDocumentModel {
        title: "t".into(),
        sheet_cells: cells.clone(),
        ..UniversalDocumentModel::default()
    };
    let bytes2 = UniversalFormatConverter::encode(&model, DocumentFormat::Csv).unwrap();
    assert!(bytes2.starts_with(&[0xEF, 0xBB, 0xBF]));
    let second = UniversalFormatConverter::decode_rich(&bytes2, Some("데이터.csv")).unwrap();
    let DecodedContent::Workbook(again) = &second.content else {
        panic!("workbook expected");
    };
    assert_eq!(&again[0].1, cells);
}

#[test]
fn pptx_roundtrip_keeps_slide_titles_bullets_and_notes() {
    let slides = vec![
        (
            "개요".to_string(),
            vec!["첫째".to_string(), "둘째".to_string()],
            "발표자 노트".to_string(),
        ),
        ("결론".to_string(), vec!["끝".to_string()], String::new()),
    ];
    let model = UniversalDocumentModel {
        title: "발표".into(),
        slides: slides.clone(),
        ..UniversalDocumentModel::default()
    };
    let bytes = UniversalFormatConverter::encode(&model, DocumentFormat::Pptx).unwrap();
    let first = UniversalFormatConverter::decode_rich(&bytes, Some("발표.pptx")).unwrap();
    assert_eq!(first.format, DocumentFormat::Pptx);
    let DecodedContent::Slides(decoded) = &first.content else {
        panic!("slides expected");
    };
    assert_eq!(decoded, &slides);

    let model2 = UniversalDocumentModel {
        title: "발표".into(),
        slides: decoded.clone(),
        ..UniversalDocumentModel::default()
    };
    let bytes2 = UniversalFormatConverter::encode(&model2, DocumentFormat::Pptx).unwrap();
    let second = UniversalFormatConverter::decode_rich(&bytes2, Some("발표.pptx")).unwrap();
    assert!(matches!(&second.content, DecodedContent::Slides(s) if s == &slides));
}

#[test]
fn legacy_and_pdf_formats_decode_to_plain_paragraphs() {
    let model = UniversalDocumentModel {
        title: "구형".into(),
        paragraphs: vec!["본문 하나".into(), "본문 둘".into()],
        ..UniversalDocumentModel::default()
    };
    for (format, name) in [
        (DocumentFormat::Doc, "a.doc"),
        (DocumentFormat::Hwp, "a.hwp"),
        (DocumentFormat::Pdf, "a.pdf"),
        (DocumentFormat::Rtf, "a.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode(&model, format).unwrap();
        let decoded = UniversalFormatConverter::decode_rich(&bytes, Some(name)).unwrap();
        assert_eq!(decoded.format, format);
        let text: Vec<String> = document_of(&decoded)
            .blocks
            .iter()
            .filter_map(|b| match b {
                RichBlock::Paragraph(p) => Some(p.plain_text()),
                RichBlock::Table(_) | RichBlock::Image(_) => None,
            })
            .collect();
        assert!(text.contains(&"본문 하나".to_string()), "{name}: {text:?}");
        assert!(text.contains(&"본문 둘".to_string()), "{name}: {text:?}");
    }
}

#[test]
fn magic_bytes_win_over_a_misleading_extension() {
    let sheets = vec![("S".to_string(), vec![(0, 0, "1".to_string())])];
    let bytes = OoxmlCodec::pack_xlsx_multi(&sheets).unwrap();
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("가짜.docx")).unwrap();
    assert_eq!(decoded.format, DocumentFormat::Xlsx);
    assert!(matches!(decoded.content, DecodedContent::Workbook(_)));
}

#[test]
fn word_style_headings_and_numbered_lists_are_recognized() {
    let doc_xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>",
        "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>장 제목</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"3\"/></w:numPr></w:pPr><w:r><w:t>번호 항목</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"4\"/></w:numPr></w:pPr><w:r><w:t>점 항목</w:t></w:r></w:p>",
        "<w:p><w:r><w:rPr><w:b w:val=\"0\"/><w:u w:val=\"none\"/></w:rPr><w:t>꺼진 서식</w:t></w:r></w:p>",
        "<w:p><w:del><w:r><w:delText>삭제됨</w:delText></w:r></w:del><w:r><w:t>남음</w:t></w:r></w:p>",
        "<w:sectPr/></w:body></w:document>"
    );
    let styles = concat!(
        "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
        "<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/></w:style></w:styles>"
    );
    let numbering = concat!(
        "<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
        "<w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/></w:lvl></w:abstractNum>",
        "<w:abstractNum w:abstractNumId=\"1\"><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"bullet\"/></w:lvl></w:abstractNum>",
        "<w:num w:numId=\"3\"><w:abstractNumId w:val=\"0\"/></w:num>",
        "<w:num w:numId=\"4\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>"
    );
    let bytes = ZipArchiveCodec::pack(&[
        ("[Content_Types].xml".to_string(), b"<Types/>".to_vec()),
        ("word/document.xml".to_string(), doc_xml.as_bytes().to_vec()),
        ("word/styles.xml".to_string(), styles.as_bytes().to_vec()),
        (
            "word/numbering.xml".to_string(),
            numbering.as_bytes().to_vec(),
        ),
    ])
    .unwrap();
    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("w.docx")).unwrap();
    let doc = document_of(&decoded);
    let kinds: Vec<(u8, Option<RichListKind>, String)> = doc
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => (p.heading_level, p.list, p.plain_text()),
            RichBlock::Table(_) | RichBlock::Image(_) => (0, None, String::new()),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (2, None, "장 제목".to_string()),
            (0, Some(RichListKind::Ordered), "번호 항목".to_string()),
            (0, Some(RichListKind::Bullet), "점 항목".to_string()),
            (0, None, "꺼진 서식".to_string()),
            (0, None, "남음".to_string()),
        ]
    );
    let RichBlock::Paragraph(off) = &doc.blocks[3] else {
        panic!("paragraph expected");
    };
    assert!(!off.runs[0].bold && !off.runs[0].underline);
}

// ---------------------------------------------------------------- security

fn zip_with(entries: &[(&str, &str)]) -> Vec<u8> {
    let owned: Vec<(String, Vec<u8>)> = entries
        .iter()
        .map(|(p, d)| (p.to_string(), d.as_bytes().to_vec()))
        .collect();
    ZipArchiveCodec::pack(&owned).unwrap()
}

const XXE_XML: &str = "<?xml version=\"1.0\"?><!DOCTYPE d [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>&x;</w:t></w:r></w:p></w:body></w:document>";

#[test]
fn xxe_is_rejected_for_docx_xlsx_pptx_hwpx_and_odt_on_the_decode_path() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("x.docx", zip_with(&[("word/document.xml", XXE_XML)])),
        (
            "x.xlsx",
            zip_with(&[
                ("xl/workbook.xml", XXE_XML),
                ("xl/worksheets/sheet1.xml", "<a/>"),
            ]),
        ),
        (
            "x.pptx",
            zip_with(&[
                ("ppt/presentation.xml", XXE_XML),
                ("ppt/slides/slide1.xml", "<a/>"),
            ]),
        ),
        (
            "x.hwpx",
            zip_with(&[
                ("Contents/header.xml", "<a/>"),
                ("Contents/section0.xml", XXE_XML),
            ]),
        ),
        (
            "x.odt",
            zip_with(&[
                ("mimetype", "application/vnd.oasis.opendocument.text"),
                ("content.xml", XXE_XML),
            ]),
        ),
    ];
    for (name, bytes) in cases {
        let err = UniversalFormatConverter::decode_rich(&bytes, Some(name)).unwrap_err();
        assert!(
            matches!(err, EasyOfficeError::XxeOrExternalEntityDetected),
            "{name}: {err:?}"
        );
    }
}

#[test]
fn zip_bomb_is_rejected_on_the_decode_path() {
    let zeros = vec![b' '; 24 * 1024 * 1024];
    let bomb = ZipArchiveCodec::pack_deflate(&[("word/document.xml", zeros)]).unwrap();
    assert!(
        bomb.len() < 1024 * 1024,
        "fixture must be highly compressed"
    );
    let err = UniversalFormatConverter::decode_rich(&bomb, Some("bomb.docx")).unwrap_err();
    assert!(
        matches!(
            err,
            EasyOfficeError::DecompressionBombDetected { .. }
                | EasyOfficeError::ResourceBudgetExceeded(_)
                | EasyOfficeError::SecurityViolation(_)
        ),
        "{err:?}"
    );
}

#[test]
fn path_traversal_entry_is_rejected_on_the_decode_path() {
    let mut bytes = zip_with(&[
        ("xx/evil.xml", "<a/>"),
        ("word/document.xml", "<w:document/>"),
    ]);
    let needle = b"xx/evil.xml";
    let patched = b"../evil.xml";
    let mut hits = 0;
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            bytes[i..i + needle.len()].copy_from_slice(patched);
            hits += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    assert_eq!(hits, 2, "local header and central directory");
    assert!(ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).is_err());
    let err = UniversalFormatConverter::decode_rich(&bytes, Some("slip.docx")).unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::SecurityViolation(_)),
        "{err:?}"
    );
}

#[test]
fn corrupt_empty_and_unsupported_inputs_fail_cleanly() {
    assert!(UniversalFormatConverter::decode_rich(&[], Some("a.docx")).is_err());
    assert!(UniversalFormatConverter::decode_rich(b"not a zip at all", Some("a.docx")).is_err());
    assert!(UniversalFormatConverter::decode_rich(b"PK\x03\x04garbage", Some("a.xlsx")).is_err());
    assert!(UniversalFormatConverter::decode_rich(b"hello", Some("a.exe")).is_err());
    assert!(
        UniversalFormatConverter::decode_rich(&[0xFF, 0xFE, 0x00, 0x01], Some("a.txt")).is_err()
    );
    let truncated = {
        let doc = RichDocument::default();
        let mut b =
            UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
        b.truncate(b.len() / 2);
        b
    };
    assert!(UniversalFormatConverter::decode_rich(&truncated, Some("a.docx")).is_err());
}
