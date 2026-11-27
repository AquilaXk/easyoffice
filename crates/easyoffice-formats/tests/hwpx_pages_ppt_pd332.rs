//! PD-332: HWPX·EqEdit·Pages·PPT 충실도 및 손실 경고 통합 검증.

mod image_fixture;
mod pages_fixture;

use easyoffice_formats::pages_iwa::{decompress_iwa, split_records, IwaRecord};
use easyoffice_formats::pages_proto::{Message, WireValue};
use easyoffice_formats::{
    convert_eqedit_to_latex, pages_write_loss, ppt_legacy_slides_loss, ppt_save_loss, ChartKind,
    ChartLegendPos, DecodedContent, DocumentFormat, PagesWriteLoss, PptSaveLoss, RichBlock,
    RichDocument, RichImage, RichLineSpacing, RichListKind, RichListNumber, RichNote, RichNoteKind,
    RichNumberEnclosure, RichNumberFormat, RichPageMargins, RichParagraph, RichRevision,
    RichRevisionKind, RichRun, RichScript, RichSlide, RichSlideDeck, RichTable, SlideBox,
    SlideChartSeries, SlideElement, TableCellImage, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy, MAX_LINE_SPACING_TWIPS,
};
use image_fixture::{make_jpeg, make_png};
use pages_fixture::{cat, f_bytes, f_f32, f_ref, f_str, f_varint, pages_zip, DocBuilder};

fn unpack_zip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack zip")
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect()
}

fn zip_text<'a>(entries: &'a [(String, Vec<u8>)], path: &str) -> &'a str {
    entries
        .iter()
        .find(|(p, _)| p == path)
        .and_then(|(_, d)| std::str::from_utf8(d).ok())
        .unwrap_or_else(|| panic!("missing utf8 entry {path}"))
}

fn decode_rich_doc(bytes: &[u8], hint: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(hint))
        .expect("decode_rich")
        .content
    {
        DecodedContent::Document(doc) => doc,
        other => panic!("expected document for {hint}, got {other:?}"),
    }
}

fn fields_ascending(payload: &[u8]) -> bool {
    let Ok(message) = Message::parse(payload) else {
        return true;
    };
    let numbers: Vec<u32> = message.fields.iter().map(|f| f.number).collect();
    if numbers.windows(2).any(|w| w[0] > w[1]) {
        return false;
    }
    message.fields.iter().all(|f| match f.value {
        WireValue::Bytes(inner) if inner.iter().any(|b| *b < 0x20) => fields_ascending(inner),
        _ => true,
    })
}

fn iwa_records(bytes: &[u8]) -> Vec<IwaRecord> {
    let mut all = Vec::new();
    for (path, data) in unpack_zip(bytes) {
        if path.starts_with("Index/") && path.ends_with(".iwa") {
            let mut budget = usize::MAX;
            let stream = decompress_iwa(&data, &mut budget).expect("decompress iwa");
            all.extend(split_records(&stream).expect("split records"));
        }
    }
    all
}

#[test]
fn hwpx_approval_lines_only_emit_when_official_agency_is_non_blank() {
    let plain = RichDocument {
        title: "일반 문서".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("일반 본문")],
            ..RichParagraph::default()
        })],
        official_agency: None,
        ..RichDocument::default()
    };
    let plain_bytes =
        UniversalFormatConverter::encode_rich(&plain, DocumentFormat::Hwpx, None).expect("encode");
    let plain_entries = unpack_zip(&plain_bytes);
    let plain_hpf = zip_text(&plain_entries, "Contents/content.hpf");
    assert!(
        !plain_hpf.contains("name=\"approvalLines\""),
        "plain document must not emit approvalLines:\n{plain_hpf}"
    );

    let blank_agency = RichDocument {
        official_agency: Some("   ".to_string()),
        ..plain.clone()
    };
    let blank_bytes =
        UniversalFormatConverter::encode_rich(&blank_agency, DocumentFormat::Hwpx, None)
            .expect("encode");
    let blank_entries = unpack_zip(&blank_bytes);
    let blank_hpf = zip_text(&blank_entries, "Contents/content.hpf");
    assert!(
        !blank_hpf.contains("name=\"approvalLines\""),
        "whitespace agency must not emit approvalLines:\n{blank_hpf}"
    );

    let official = RichDocument {
        official_agency: Some("행정안전부".to_string()),
        ..plain
    };
    let official_bytes =
        UniversalFormatConverter::encode_rich(&official, DocumentFormat::Hwpx, None)
            .expect("encode");
    let official_entries = unpack_zip(&official_bytes);
    let official_hpf = zip_text(&official_entries, "Contents/content.hpf");
    assert!(
        official_hpf.contains(
            "<opf:meta name=\"approvalLines\" content=\"text\">기안|검토|결재</opf:meta>"
        ),
        "official agency document must emit default approvalLines:\n{official_hpf}"
    );
}

#[test]
fn hwpx_all_deleted_paragraph_keeps_empty_run_and_header_always_defines_numbering() {
    let rev_del = RichRevision {
        id: 1,
        kind: RichRevisionKind::Delete,
        author: "편집자".to_string(),
        date: "2026-11-24T00:00:00Z".to_string(),
    };
    let doc = RichDocument {
        title: "삭제 추적과 기본 번호".to_string(),
        track_changes: true,
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("첫 문단")],
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun {
                    text: "모두 삭제된 링크".to_string(),
                    link: Some("https://example.org/deleted".to_string()),
                    revision: Some(rev_del),
                    ..RichRun::default()
                }],
                ..RichParagraph::default()
            }),
        ],
        ..RichDocument::default()
    };

    let bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).expect("encode");
    let entries = unpack_zip(&bytes);
    let header = zip_text(&entries, "Contents/header.xml");
    let section = zip_text(&entries, "Contents/section0.xml");

    assert!(
        header.contains("<hh:numberings itemCnt=\"1\"><hh:numbering id=\"1\""),
        "header must always include default numberings:\n{header}"
    );
    assert!(
        section.contains("outlineShapeIDRef=\"1\""),
        "secPr must reference 1-based outline numbering:\n{section}"
    );
    assert!(
        !section.contains("merged=\"0\"></hp:p>"),
        "deleted-only paragraph must not produce zero runs:\n{section}"
    );
    assert!(
        section.contains("<hp:run charPrIDRef=\"0\"><hp:t/></hp:run>"),
        "deleted-only paragraph must fall back to empty hp:run:\n{section}"
    );
    assert!(
        !section.contains("https://example.org/deleted"),
        "deleted run link must not leak:\n{section}"
    );
}

#[test]
fn hwpx_table_headers_and_notes_roundtrip_without_duplicate_labels() {
    let doc = RichDocument {
        title: "표 머리글과 각주·미주".to_string(),
        notes: vec![
            RichNote {
                id: 1,
                kind: RichNoteKind::Footnote,
                text: "첫 각주 본문".to_string(),
            },
            RichNote {
                id: 1,
                kind: RichNoteKind::Endnote,
                text: "첫 미주 본문".to_string(),
            },
        ],
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![
                    RichRun::plain("본문"),
                    RichRun {
                        script: RichScript::Superscript,
                        note_ref: Some((RichNoteKind::Footnote, 1)),
                        ..RichRun::default()
                    },
                    RichRun::plain(" 이어짐"),
                    RichRun {
                        script: RichScript::Superscript,
                        note_ref: Some((RichNoteKind::Endnote, 1)),
                        ..RichRun::default()
                    },
                ],
                ..RichParagraph::default()
            }),
            RichBlock::Table(RichTable::new(vec![
                vec!["구분".to_string(), "값".to_string()],
                vec!["항목".to_string(), "100".to_string()],
            ])),
            RichBlock::Table(RichTable::new(vec![vec![
                "단일 행".to_string(),
                "유지".to_string(),
            ]])),
        ],
        ..RichDocument::default()
    };

    let bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).expect("encode");
    let entries = unpack_zip(&bytes);
    let section = zip_text(&entries, "Contents/section0.xml");

    assert!(
        section.contains("repeatHeader=\"1\" rowCnt=\"2\""),
        "multi-row table must set repeatHeader=1:\n{section}"
    );
    assert!(
        section.contains("repeatHeader=\"0\" rowCnt=\"1\""),
        "single-row table must set repeatHeader=0:\n{section}"
    );
    assert_eq!(
        section.matches("header=\"1\"").count(),
        2,
        "only row 0 cells of multi-row table should have header=1:\n{section}"
    );
    assert!(
        section.contains("<hp:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"TOP\"><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:ctrl><hp:autoNum numType=\"FOOTNOTE\"/></hp:ctrl><hp:t>첫 각주 본문</hp:t>"),
        "footnote must include subList attrs and autoNum:\n{section}"
    );
    assert!(
        section.contains("<hp:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"TOP\"><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:ctrl><hp:autoNum numType=\"ENDNOTE\"/></hp:ctrl><hp:t>첫 미주 본문</hp:t>"),
        "endnote must include subList attrs and autoNum:\n{section}"
    );

    let back = decode_rich_doc(&bytes, "notes_tbl.hwpx");
    let body_p = match &back.blocks[0] {
        RichBlock::Paragraph(p) => p,
        other => panic!("expected body para: {other:?}"),
    };
    assert_eq!(body_p.plain_text(), "본문1) 이어짐1)");
    let note_texts: Vec<String> = back
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect();
    assert!(
        note_texts.contains(&"1) 첫 각주 본문".to_string()),
        "{note_texts:?}"
    );
    assert!(
        note_texts.contains(&"1) 첫 미주 본문".to_string()),
        "{note_texts:?}"
    );
    assert!(
        !note_texts.iter().any(|t| t.contains("1) 1)")),
        "note label duplicated on roundtrip: {note_texts:?}"
    );
}

#[test]
fn hwpx_reads_non_percent_line_spacing_with_checked_arithmetic() {
    let base = RichDocument {
        title: "줄간격".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("기준")],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    let bytes =
        UniversalFormatConverter::encode_rich(&base, DocumentFormat::Hwpx, None).expect("encode");
    let entries = unpack_zip(&bytes);
    let base_header = zip_text(&entries, "Contents/header.xml");

    let cases = [
        // HWPUNIT values keep their rule and become twips (PD-366); a huge one is clamped.
        ("FIXED", "1800", Some(RichLineSpacing::Exact(360))),
        ("AT_LEAST", "2400", Some(RichLineSpacing::AtLeast(480))),
        ("BETWEEN_LINES", "600", Some(RichLineSpacing::Between(120))),
        ("BETWEEN_LINES", "0", Some(RichLineSpacing::Between(0))),
        ("FIXED", "300", Some(RichLineSpacing::Exact(60))),
        ("FIXED", "0", None),
        (
            "FIXED",
            "9223372036854775807",
            Some(RichLineSpacing::Exact(MAX_LINE_SPACING_TWIPS)),
        ),
        (
            "BETWEEN_LINES",
            "9223372036854775807",
            Some(RichLineSpacing::Between(MAX_LINE_SPACING_TWIPS)),
        ),
        ("FIXED", "-500", None),
        ("BETWEEN_LINES", "-500", None),
    ];

    for (ls_type, ls_val, expected) in cases {
        let header = base_header.replace(
            "<hh:lineSpacing type=\"PERCENT\" value=\"160\" unit=\"HWPUNIT\"/>",
            &format!("<hh:lineSpacing type=\"{ls_type}\" value=\"{ls_val}\" unit=\"HWPUNIT\"/>"),
        );
        let modified: Vec<(String, Vec<u8>)> = entries
            .iter()
            .map(|(p, d)| {
                if p == "Contents/header.xml" {
                    (p.clone(), header.as_bytes().to_vec())
                } else {
                    (p.clone(), d.clone())
                }
            })
            .collect();
        let repacked = ZipArchiveCodec::pack_compressed(&modified).expect("repack");
        let doc = decode_rich_doc(&repacked, "spacing.hwpx");
        let RichBlock::Paragraph(p) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(p.line_spacing, expected, "type={ls_type}, value={ls_val}");
    }
}

#[test]
fn eqedit_does_not_split_plain_words_with_font_prefixes_but_splits_commands_and_single_chars() {
    assert_eq!(convert_eqedit_to_latex("item").unwrap(), "item");
    assert_eq!(convert_eqedit_to_latex("ital").unwrap(), "ital");
    assert_eq!(convert_eqedit_to_latex("italic").unwrap(), "italic");
    assert_eq!(convert_eqedit_to_latex("rmax").unwrap(), "rmax");
    assert_eq!(convert_eqedit_to_latex("boldface").unwrap(), "boldface");

    assert_eq!(convert_eqedit_to_latex("rmA").unwrap(), "\\mathrm{A}");
    assert_eq!(convert_eqedit_to_latex("itx").unwrap(), "\\mathit{x}");
    assert_eq!(
        convert_eqedit_to_latex("boldsum").unwrap(),
        "\\mathbf{\\sum}"
    );
}

#[test]
fn pages_write_adds_table_geometry_uuid_defaults_and_cell_image_loss_warning() {
    let png = make_png(20, 10, 2, 3);
    let img = RichImage::new(png, "셀 그림").expect("valid png");
    let mut table = RichTable::new(vec![
        vec!["헤더1".to_string(), "헤더2".to_string()],
        vec!["값1".to_string(), "값2".to_string()],
    ]);
    table.cell_images = vec![TableCellImage {
        row: 0,
        col: 1,
        image: img,
    }];
    let mut hangul_paren = RichParagraph {
        list: Some(RichListKind::Ordered),
        list_level: 0,
        list_number: Some(RichListNumber {
            format: RichNumberFormat::HangulSyllable,
            enclosure: RichNumberEnclosure::Parentheses,
        }),
        runs: vec![RichRun::plain("가나다 괄호 목록")],
        ..RichParagraph::default()
    };
    let doc = RichDocument {
        title: "Pages 표 기하·손실".to_string(),
        blocks: vec![
            RichBlock::Table(table),
            RichBlock::Paragraph(hangul_paren.clone()),
        ],
        ..RichDocument::default()
    };

    let loss = pages_write_loss(&doc);
    assert_eq!(
        loss,
        PagesWriteLoss {
            elements: 0,
            formats: 2
        },
        "expected 1 for hoisted cell image + 1 for Hangul parentheses enclosure"
    );
    assert_eq!(loss.warnings(), vec!["Pages 저장에서 빠진 서식 2곳"]);

    hangul_paren.list_number = Some(RichListNumber {
        format: RichNumberFormat::HangulSyllable,
        enclosure: RichNumberEnclosure::Dot,
    });
    let doc_dot_only = RichDocument {
        blocks: vec![RichBlock::Paragraph(hangul_paren)],
        ..doc.clone()
    };
    assert_eq!(pages_write_loss(&doc_dot_only), PagesWriteLoss::default());

    let bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None).expect("encode");
    let all = iwa_records(&bytes);
    for rec in &all {
        for payload in &rec.payloads {
            assert!(
                fields_ascending(payload),
                "out-of-order fields on id={} type={}",
                rec.info.identifier,
                rec.info.messages[0].type_id
            );
        }
    }

    let info_rec = all
        .iter()
        .find(|r| r.info.messages[0].type_id == 6000)
        .expect("TST.TableInfoArchive");
    let info_msg = Message::parse(&info_rec.payloads[0]).expect("parse 6000");
    let drawable = info_msg
        .sub(1)
        .expect("sub 1")
        .expect("field 1 TSD.DrawableArchive");
    let geometry = drawable.sub(1).expect("sub 1").expect("drawable::GEOMETRY");
    let size = geometry.sub(2).expect("sub 2").expect("geometry::SIZE");
    let width_pt = size.float(1).expect("width_pt");
    let height_pt = size.float(2).expect("height_pt");
    assert!(width_pt > 400.0 && (height_pt - 44.0).abs() < 1e-3);

    let model_rec = all
        .iter()
        .find(|r| r.info.messages[0].type_id == 6001)
        .expect("TST.TableModelArchive");
    let model_msg = Message::parse(&model_rec.payloads[0]).expect("parse 6001");
    let expected_uuid = format!("00000000-0000-4000-8000-{:012x}", model_rec.info.identifier);
    assert_eq!(
        model_msg.string(1).expect("uuid").as_deref(),
        Some(expected_uuid.as_str())
    );
    assert_eq!(model_msg.varint(6), Some(2));
    assert_eq!(model_msg.varint(7), Some(2));
    assert_eq!(model_msg.float(8), Some(22.0));
    let default_col_w = model_msg.float(9).expect("default col width");
    assert!((default_col_w - (width_pt / 2.0).max(36.0)).abs() < 1e-3);
}

#[test]
fn pages_reader_defaults_omitted_margins_keeps_explicit_zero_and_maps_point_line_spacing() {
    let mut b = DocBuilder::default();
    // Modes of the line spacing message: 1 minimum, 2 exact (PD-366), amounts in points.
    let spacing_min = cat(&[f_varint(1, 1), f_f32(2, 18.0)]);
    let spacing_exact = cat(&[f_varint(1, 2), f_f32(2, 3.0)]);
    let spacing_huge = cat(&[f_varint(1, 1), f_f32(2, 240.0)]);
    let p_min = b.add(
        2022,
        &cat(&[
            f_bytes(1, &f_str(2, "p-min")),
            f_bytes(12, &f_bytes(19, &spacing_min)),
        ]),
    );
    let p_exact = b.add(
        2022,
        &cat(&[
            f_bytes(1, &f_str(2, "p-exact")),
            f_bytes(12, &f_bytes(19, &spacing_exact)),
        ]),
    );
    let p_huge = b.add(
        2022,
        &cat(&[
            f_bytes(1, &f_str(2, "p-huge")),
            f_bytes(12, &f_bytes(19, &spacing_huge)),
        ]),
    );
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(
            "첫줄\n둘째\n셋째",
            &[(0, Some(p_min)), (3, Some(p_exact)), (6, Some(p_huge))],
            &[],
            &[],
            &[],
        ),
    );
    // Case A: margins 32..=35 omitted -> fallback to PAGES_DEFAULT (1134 twips).
    let root_omitted = cat(&[f_ref(4, st), f_f32(30, 595.28), f_f32(31, 841.89)]);
    b.add_with_id(1, 10000, &root_omitted);
    let zip_omitted = pages_zip(pages_fixture::iwa_frame(&b.records, true));
    let doc_omitted = decode_rich_doc(&zip_omitted, "omitted.pages");
    assert_eq!(
        doc_omitted.page_frame.margins,
        Some(RichPageMargins::PAGES_DEFAULT)
    );
    let spacings: Vec<Option<RichLineSpacing>> = doc_omitted
        .blocks
        .iter()
        .filter_map(|blk| match blk {
            RichBlock::Paragraph(p) => Some(p.line_spacing),
            _ => None,
        })
        .collect();
    assert_eq!(
        spacings,
        vec![
            Some(RichLineSpacing::AtLeast(360)),
            Some(RichLineSpacing::Exact(60)),
            Some(RichLineSpacing::AtLeast(4800)),
        ]
    );

    // Case B: explicit 0.0 margins -> preserved as 0.
    let mut b2 = DocBuilder::default();
    let st2 = b2.add(
        2001,
        &DocBuilder::storage_payload("본문", &[], &[], &[], &[]),
    );
    let root_zero = cat(&[
        f_ref(4, st2),
        f_f32(30, 595.28),
        f_f32(31, 841.89),
        f_f32(32, 0.0),
        f_f32(33, 0.0),
        f_f32(34, 0.0),
        f_f32(35, 0.0),
        f_f32(36, 0.0),
        f_f32(37, 0.0),
    ]);
    b2.add_with_id(1, 10000, &root_zero);
    let zip_zero = pages_zip(pages_fixture::iwa_frame(&b2.records, true));
    let doc_zero = decode_rich_doc(&zip_zero, "zero.pages");
    assert_eq!(
        doc_zero.page_frame.margins,
        Some(RichPageMargins {
            top: 0,
            bottom: 0,
            header: 0,
            footer: 0,
            left: 0,
            right: 0,
        })
    );
}

#[test]
fn ppt_save_loss_warns_on_tables_charts_images_and_notes_while_keeping_clean_empty() {
    let jpeg = make_jpeg(16, 12);
    let img = RichImage::new(jpeg, "슬라이드 그림").expect("valid jpeg");
    let rect = SlideBox {
        x: 40.0,
        y: 80.0,
        w: 300.0,
        h: 180.0,
    };
    let deck = RichSlideDeck {
        title: "발표 자료".to_string(),
        slides: vec![RichSlide {
            title: "개요".to_string(),
            bullets: vec!["핵심 요약".to_string()],
            notes: "발표자 참고 메모".to_string(),
            elements: vec![
                SlideElement::Table {
                    id: "t1".to_string(),
                    rect,
                    rows: vec![vec!["항목".to_string(), "수치".to_string()]],
                    header_row: true,
                    fill: None,
                },
                SlideElement::Chart {
                    id: "c1".to_string(),
                    rect,
                    chart_type: ChartKind::Bar,
                    title: "실적".to_string(),
                    categories: vec!["1분기".to_string()],
                    series: vec![SlideChartSeries {
                        name: "매출".to_string(),
                        values: vec![12.0],
                    }],
                    legend: ChartLegendPos::Right,
                },
                SlideElement::Image {
                    id: "i1".to_string(),
                    rect,
                    image: img,
                    alt: "로고".to_string(),
                },
            ],
            ..RichSlide::default()
        }],
        ..RichSlideDeck::default()
    };

    let loss = ppt_save_loss(&deck);
    assert_eq!(
        loss,
        PptSaveLoss {
            tables: 1,
            charts: 1,
            images: 1,
            notes: 1,
        }
    );
    assert!(!loss.is_empty());

    let (ppt_bytes, ppt_warnings) =
        UniversalFormatConverter::encode_rich_slides_with_warnings(&deck, DocumentFormat::Ppt)
            .expect("encode ppt");
    assert!(!ppt_bytes.is_empty());
    assert_eq!(ppt_warnings.len(), 4);
    assert!(ppt_warnings[0].contains("표 1개"));
    assert!(ppt_warnings[1].contains("차트 1개"));
    assert!(ppt_warnings[2].contains("그림 1개"));
    assert!(ppt_warnings[3].contains("발표자 노트 1개"));

    let (_, pptx_warnings) =
        UniversalFormatConverter::encode_rich_slides_with_warnings(&deck, DocumentFormat::Pptx)
            .expect("encode pptx");
    assert!(pptx_warnings.is_empty());

    let clean_deck = RichSlideDeck {
        title: "단순 슬라이드".to_string(),
        slides: vec![RichSlide {
            title: "제목".to_string(),
            bullets: vec!["본문".to_string()],
            ..RichSlide::default()
        }],
        ..RichSlideDeck::default()
    };
    assert!(ppt_save_loss(&clean_deck).is_empty());
    let (_, clean_warnings) = UniversalFormatConverter::encode_rich_slides_with_warnings(
        &clean_deck,
        DocumentFormat::Ppt,
    )
    .expect("encode clean ppt");
    assert!(clean_warnings.is_empty());

    let legacy_loss = ppt_legacy_slides_loss(&[
        (
            "1장".to_string(),
            vec!["내용".to_string()],
            "메모".to_string(),
        ),
        ("2장".to_string(), vec![], "   ".to_string()),
    ]);
    assert_eq!(legacy_loss.notes, 1);
    assert_eq!(legacy_loss.warnings().len(), 1);
}
