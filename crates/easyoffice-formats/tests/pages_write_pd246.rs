//! Extended `.pages` package writer tests (PD-246): tables (multi-tile, merges, cell hyperlinks),
//! PNG/JPEG pictures (`Data/` deduplication, `DATA_INFOS`, alt text, display dimensions), bullet
//! and 9-level numbered lists, body hyperlinks, extended run/paragraph/column styles, page setup
//! (paper, margins, duplex facing pages), 3-zone & first/even headers/footers, fail-closed limits
//! and encode/decode performance measurement.

mod image_fixture;

use easyoffice_formats::pages_iwa::{decompress_iwa, split_records, IwaRecord};
use easyoffice_formats::pages_proto::{Message, WireValue};
use easyoffice_formats::{
    pages_write_loss, CellMerge, DecodedContent, DocumentFormat, EasyOfficeError, PagesWriteLoss,
    RichAlignment, RichBinding, RichBindingKind, RichBlock, RichColumns, RichDocument, RichField,
    RichImage, RichListKind, RichListNumber, RichNumberEnclosure, RichNumberFormat, RichPageFrame,
    RichPageMargins, RichPaper, RichParagraph, RichRun, RichScript, RichTable, TableCellLink,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy, MAX_IMAGES_PER_DOCUMENT,
};
use image_fixture::{make_jpeg, make_png};
use std::time::Instant;

fn run(text: &str) -> RichRun {
    RichRun::plain(text)
}

fn para(runs: Vec<RichRun>) -> RichParagraph {
    RichParagraph {
        runs,
        ..RichParagraph::default()
    }
}

fn doc(blocks: Vec<RichBlock>) -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "확장쓰기".to_string(),
        blocks,
        official_agency: None,
    }
}

fn write(doc: &RichDocument) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, DocumentFormat::Pages, None).unwrap()
}

fn read_back(bytes: &[u8]) -> (RichDocument, Vec<String>) {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some("a.pages")).unwrap();
    assert_eq!(decoded.format, DocumentFormat::Pages);
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("expected a document");
    };
    (doc, decoded.warnings)
}

fn local_entries(zip: &[u8]) -> Vec<(String, u16)> {
    let le16 = |at: usize| usize::from(u16::from_le_bytes([zip[at], zip[at + 1]]));
    let le32 = |at: usize| u32::from_le_bytes([zip[at], zip[at + 1], zip[at + 2], zip[at + 3]]);
    let mut at = 0;
    let mut out = Vec::new();
    while at + 30 <= zip.len() && le32(at) == 0x0403_4B50 {
        let method = le16(at + 8) as u16;
        let compressed = le32(at + 18) as usize;
        let (name_len, extra_len) = (le16(at + 26), le16(at + 28));
        let name = String::from_utf8(zip[at + 30..at + 30 + name_len].to_vec()).unwrap();
        out.push((name, method));
        at += 30 + name_len + extra_len + compressed;
    }
    out
}

fn records(bytes: &[u8]) -> Vec<IwaRecord> {
    let mut all = Vec::new();
    for e in ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap() {
        if e.path.starts_with("Index/") && e.path.ends_with(".iwa") {
            let mut budget = usize::MAX;
            let stream = decompress_iwa(&e.data, &mut budget).unwrap();
            all.extend(split_records(&stream).unwrap());
        }
    }
    all
}

fn payload_of(records: &[IwaRecord], type_id: u32) -> Vec<(u64, &[u8])> {
    records
        .iter()
        .filter(|r| r.info.messages[0].type_id == type_id)
        .map(|r| (r.info.identifier, r.payloads[0].as_slice()))
        .collect()
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

#[test]
fn tables_merges_and_cell_hyperlinks_roundtrip_even_across_multiple_tiles() {
    let mut rows: Vec<Vec<String>> = (0..300)
        .map(|r| {
            vec![
                format!("행 {r}"),
                if r % 2 == 0 {
                    "반복값".to_string()
                } else {
                    String::new()
                },
                format!("값 {}", r * 10),
                "링크 😀 안내문".to_string(),
            ]
        })
        .collect();
    rows[0][1] = "병합 머리글".to_string();
    rows[0][2] = String::new();
    let link_cell = &rows[299][3];
    let target_sub = "안내문";
    let start_b = link_cell.find(target_sub).unwrap() as u32;
    let end_b = start_b + target_sub.len() as u32;
    let (tbl, dropped_m, dropped_l) = RichTable::with_merges_and_links(
        rows.clone(),
        vec![CellMerge {
            row: 0,
            col: 1,
            row_span: 2,
            col_span: 2,
        }],
        vec![TableCellLink {
            row: 299,
            col: 3,
            start: start_b,
            end: end_b,
            url: "https://example.com/guide".to_string(),
        }],
    );
    assert_eq!((dropped_m, dropped_l), (0, 0));

    let source = doc(vec![
        RichBlock::Paragraph(para(vec![run("표 앞")])),
        RichBlock::Table(tbl.clone()),
        RichBlock::Paragraph(para(vec![run("표 뒤")])),
    ]);
    assert_eq!(pages_write_loss(&source), PagesWriteLoss::default());

    let bytes = write(&source);
    let all = records(&bytes);
    assert_eq!(
        payload_of(&all, 6002).len(),
        2,
        "300 rows span 2 tiles of 256"
    );
    let (back, warnings) = read_back(&bytes);
    assert!(
        !warnings.iter().any(|w| w.starts_with("가져오지 않은 요소")),
        "{warnings:?}"
    );
    assert_eq!(back.blocks.len(), 3);
    let RichBlock::Table(got_tbl) = &back.blocks[1] else {
        panic!("expected table at block 1");
    };
    assert_eq!(got_tbl.rows, tbl.rows);
    assert_eq!(got_tbl.merges, tbl.merges);
    assert_eq!(got_tbl.links, tbl.links);
}

#[test]
fn png_and_jpeg_images_deduplicate_data_entries_and_roundtrip_alt_and_display_size() {
    let png_bytes = make_png(24, 12, 2, 3);
    let jpg_bytes = make_jpeg(16, 10);
    let mut pic1 = RichImage::new(png_bytes.clone(), "첫 그림 설명").unwrap();
    pic1.set_display_emu(120 * 12_700, 60 * 12_700).unwrap();
    let pic2 = RichImage::new(png_bytes.clone(), "중복 PNG 다른 설명").unwrap();
    let pic3 = RichImage::new(jpg_bytes.clone(), "JPEG 사진").unwrap();

    let source = doc(vec![
        RichBlock::Paragraph(para(vec![run("그림 시작")])),
        RichBlock::Image(pic1),
        RichBlock::Image(pic2),
        RichBlock::Image(pic3),
    ]);
    assert_eq!(pages_write_loss(&source), PagesWriteLoss::default());

    let bytes = write(&source);
    let zip_entries = local_entries(&bytes);
    let data_entries: Vec<&str> = zip_entries
        .iter()
        .filter(|(n, _)| n.starts_with("Data/"))
        .map(|(n, method)| {
            assert_eq!(*method, 0, "Data/ entry must be STORED");
            n.as_str()
        })
        .collect();
    assert_eq!(
        data_entries,
        vec!["Data/image-1.png", "Data/image-2.jpg"],
        "identical PNG bytes share one Data/ entry"
    );

    let (back, warnings) = read_back(&bytes);
    assert!(
        !warnings.iter().any(|w| w.starts_with("가져오지 않은 요소")),
        "{warnings:?}"
    );
    assert_eq!(back.blocks.len(), 4);
    let RichBlock::Image(g1) = &back.blocks[1] else {
        panic!("image 1");
    };
    let RichBlock::Image(g2) = &back.blocks[2] else {
        panic!("image 2");
    };
    let RichBlock::Image(g3) = &back.blocks[3] else {
        panic!("image 3");
    };
    assert_eq!(g1.bytes(), png_bytes.as_slice());
    assert_eq!(g1.alt(), "첫 그림 설명");
    assert_eq!(g1.display_emu(), (120 * 12_700, 60 * 12_700));
    assert_eq!(g2.bytes(), png_bytes.as_slice());
    assert_eq!(g2.alt(), "중복 PNG 다른 설명");
    assert_eq!(g3.bytes(), jpg_bytes.as_slice());
    assert_eq!(g3.alt(), "JPEG 사진");
}

#[test]
fn bullet_and_multilevel_ordered_lists_and_links_and_extended_styles_roundtrip() {
    let bullet_l2 = RichParagraph {
        list: Some(RichListKind::Bullet),
        list_level: 2,
        ..para(vec![run("글머리 기호 3단계")])
    };
    let roman_paren = RichListNumber {
        format: RichNumberFormat::UpperRoman,
        enclosure: RichNumberEnclosure::Parentheses,
    };
    let ordered_roman = RichParagraph {
        list: Some(RichListKind::Ordered),
        list_level: 4,
        list_number: Some(roman_paren),
        ..para(vec![run("로마자 괄호 목록")])
    };
    let hangul_list = RichParagraph {
        list: Some(RichListKind::Ordered),
        list_level: 1,
        list_number: Some(RichListNumber {
            format: RichNumberFormat::HangulSyllable,
            enclosure: RichNumberEnclosure::Dot,
        }),
        ..para(vec![run("가나다 목록")])
    };
    let rich_para = RichParagraph {
        alignment: Some(RichAlignment::Justify),
        left_indent_twips: Some(720),
        first_line_indent_twips: Some(-360),
        line_spacing_percent: Some(150),
        columns: Some(RichColumns {
            count: 2,
            equal: true,
            gap_twips: 360,
        }),
        ..para(vec![
            run("앞 😀 "),
            RichRun {
                strike: true,
                font: Some("Courier New".to_string()),
                highlight: Some("FFFF00".to_string()),
                link: Some("https://example.org/kr".to_string()),
                ..run("강조 링크")
            },
            RichRun {
                script: RichScript::Superscript,
                ..run("위첨자")
            },
            RichRun {
                script: RichScript::Subscript,
                link: Some("mailto:qa@example.org".to_string()),
                ..run("아래첨자메일")
            },
        ])
    };
    let unequal_cols = RichParagraph {
        columns: Some(RichColumns {
            count: 3,
            equal: false,
            gap_twips: 240,
        }),
        ..para(vec![run("비균등 3단")])
    };

    let source = doc(vec![
        RichBlock::Paragraph(bullet_l2),
        RichBlock::Paragraph(ordered_roman),
        RichBlock::Paragraph(hangul_list),
        RichBlock::Paragraph(rich_para),
        RichBlock::Paragraph(unequal_cols),
    ]);
    assert_eq!(pages_write_loss(&source), PagesWriteLoss::default());

    let (back, warnings) = read_back(&write(&source));
    assert!(
        !warnings.iter().any(|w| w.starts_with("가져오지 않은 요소")),
        "{warnings:?}"
    );
    assert_eq!(back.blocks.len(), 5);

    let RichBlock::Paragraph(p0) = &back.blocks[0] else {
        panic!();
    };
    assert_eq!((p0.list, p0.list_level), (Some(RichListKind::Bullet), 2));

    let RichBlock::Paragraph(p1) = &back.blocks[1] else {
        panic!();
    };
    assert_eq!(
        (p1.list, p1.list_level, p1.list_number),
        (Some(RichListKind::Ordered), 4, Some(roman_paren))
    );

    let RichBlock::Paragraph(p2) = &back.blocks[2] else {
        panic!();
    };
    assert_eq!(
        p2.list_number.map(|n| n.format),
        Some(RichNumberFormat::HangulSyllable)
    );

    let RichBlock::Paragraph(p3) = &back.blocks[3] else {
        panic!();
    };
    assert_eq!(p3.alignment, Some(RichAlignment::Justify));
    assert_eq!(p3.left_indent_twips, Some(720));
    assert_eq!(p3.first_line_indent_twips, Some(-360));
    assert_eq!(p3.line_spacing_percent, Some(150));
    assert_eq!(
        p3.columns,
        Some(RichColumns {
            count: 2,
            equal: true,
            gap_twips: 360,
        })
    );
    assert_eq!(p3.runs.len(), 4);
    assert!(p3.runs[1].strike);
    assert_eq!(p3.runs[1].font.as_deref(), Some("Courier New"));
    assert_eq!(p3.runs[1].highlight.as_deref(), Some("FFFF00"));
    assert_eq!(p3.runs[1].link.as_deref(), Some("https://example.org/kr"));
    assert_eq!(p3.runs[2].script, RichScript::Superscript);
    assert_eq!(p3.runs[3].script, RichScript::Subscript);
    assert_eq!(p3.runs[3].link.as_deref(), Some("mailto:qa@example.org"));

    let RichBlock::Paragraph(p4) = &back.blocks[4] else {
        panic!();
    };
    assert_eq!(
        p4.columns,
        Some(RichColumns {
            count: 3,
            equal: false,
            gap_twips: 240,
        })
    );
}

#[test]
fn page_setup_duplex_margins_and_section_headers_footers_roundtrip_and_keep_invariants() {
    let custom_paper = RichPaper {
        width: 12_240,
        height: 15_840,
    };
    let custom_margins = RichPageMargins {
        top: 1_800,
        bottom: 1_600,
        header: 900,
        footer: 800,
        left: 1_500,
        right: 1_400,
    };
    let mut source = doc(vec![
        RichBlock::Paragraph(RichParagraph {
            heading_level: 1,
            ..para(vec![run("구역·머리글 시험")])
        }),
        RichBlock::Paragraph(para(vec![run("본문 내용")])),
    ]);
    source.page_frame = RichPageFrame {
        paper: Some(custom_paper),
        margins: Some(custom_margins),
        binding: RichBinding {
            kind: RichBindingKind::Duplex,
            gutter: 0,
        },
        first_page_different: true,
        even_odd_different: true,
        header: vec![
            RichParagraph {
                alignment: Some(RichAlignment::Left),
                ..para(vec![run("홀수 왼쪽")])
            },
            RichParagraph {
                alignment: Some(RichAlignment::Center),
                ..para(vec![RichRun {
                    bold: true,
                    ..run("홀수 가운데")
                }])
            },
            RichParagraph {
                alignment: Some(RichAlignment::Right),
                ..para(vec![run("홀수 오른쪽")])
            },
        ],
        footer: vec![RichParagraph {
            alignment: Some(RichAlignment::Right),
            ..para(vec![
                run("쪽 "),
                RichRun {
                    field: Some(RichField::PageNumber),
                    ..RichRun::default()
                },
                run(" / "),
                RichRun {
                    field: Some(RichField::TotalPages),
                    ..RichRun::default()
                },
            ])
        }],
        first_header: vec![RichParagraph {
            alignment: Some(RichAlignment::Center),
            ..para(vec![run("표지 머리글")])
        }],
        first_footer: Vec::new(),
        even_header: vec![RichParagraph {
            alignment: Some(RichAlignment::Left),
            ..para(vec![run("짝수 머리글")])
        }],
        even_footer: vec![RichParagraph {
            alignment: Some(RichAlignment::Left),
            ..para(vec![run("짝수 바닥글")])
        }],
    };
    assert_eq!(pages_write_loss(&source), PagesWriteLoss::default());

    let bytes = write(&source);
    let all = records(&bytes);
    let (first_storage_id, _) = payload_of(&all, 2001)[0];
    let root_info = all
        .iter()
        .find(|r| r.info.messages[0].type_id == 10000)
        .unwrap();
    assert!(
        root_info.info.messages[0]
            .object_references
            .contains(&first_storage_id),
        "body storage stays the first 2001 record in Document.iwa"
    );
    for record in &all {
        for payload in &record.payloads {
            assert!(
                fields_ascending(payload),
                "object {} (type {}) has out-of-order fields",
                record.info.identifier,
                record.info.messages[0].type_id
            );
        }
    }

    let (back, _) = read_back(&bytes);
    let f = &back.page_frame;
    assert_eq!(f.paper, Some(custom_paper));
    assert_eq!(f.margins, Some(custom_margins));
    assert_eq!(f.binding.kind, RichBindingKind::Duplex);
    assert!(f.first_page_different && f.even_odd_different);
    assert_eq!(f.header.len(), 3);
    assert_eq!(
        (f.header[0].alignment, f.header[0].plain_text().as_str()),
        (Some(RichAlignment::Left), "홀수 왼쪽")
    );
    assert_eq!(
        (f.header[1].alignment, f.header[1].plain_text().as_str()),
        (Some(RichAlignment::Center), "홀수 가운데")
    );
    assert!(f.header[1].runs[0].bold);
    assert_eq!(
        (f.header[2].alignment, f.header[2].plain_text().as_str()),
        (Some(RichAlignment::Right), "홀수 오른쪽")
    );
    assert_eq!(f.footer.len(), 1);
    let fields: Vec<Option<RichField>> = f.footer[0].runs.iter().map(|r| r.field).collect();
    assert_eq!(
        fields,
        vec![
            None,
            Some(RichField::PageNumber),
            None,
            Some(RichField::TotalPages),
        ]
    );
    assert_eq!(f.first_header[0].plain_text(), "표지 머리글");
    assert!(f.first_footer.is_empty());
    assert_eq!(f.even_header[0].plain_text(), "짝수 머리글");
    assert_eq!(f.even_footer[0].plain_text(), "짝수 바닥글");
}

#[test]
fn oversized_tables_and_excess_images_fail_closed() {
    let wide_table = RichTable::from(vec![vec!["x".to_string(); 257]]);
    let err_wide = UniversalFormatConverter::encode_rich(
        &doc(vec![RichBlock::Table(wide_table)]),
        DocumentFormat::Pages,
        None,
    );
    assert!(
        matches!(err_wide, Err(EasyOfficeError::ResourceBudgetExceeded(_))),
        "{err_wide:?}"
    );

    let png = RichImage::new(make_png(2, 2, 2, 3), "p").unwrap();
    let too_many_pics: Vec<RichBlock> = (0..=MAX_IMAGES_PER_DOCUMENT)
        .map(|_| RichBlock::Image(png.clone()))
        .collect();
    let err_pics =
        UniversalFormatConverter::encode_rich(&doc(too_many_pics), DocumentFormat::Pages, None);
    assert!(
        matches!(err_pics, Err(EasyOfficeError::ResourceBudgetExceeded(_))),
        "{err_pics:?}"
    );
}

#[test]
fn pages_rich_package_benchmark_measures_encode_and_readback() {
    let png = RichImage::new(make_png(16, 16, 2, 3), "로고").unwrap();
    let (tbl, _, _) = RichTable::with_merges_and_links(
        (0..40)
            .map(|r| {
                vec![
                    format!("항목 {r}"),
                    "반복".to_string(),
                    "링크 셀".to_string(),
                ]
            })
            .collect(),
        vec![CellMerge {
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 2,
        }],
        vec![TableCellLink {
            row: 1,
            col: 2,
            start: 0,
            end: "링크".len() as u32,
            url: "https://example.org".to_string(),
        }],
    );
    let mut source = doc(vec![
        RichBlock::Paragraph(RichParagraph {
            heading_level: 1,
            ..para(vec![run("성능 측정 문서")])
        }),
        RichBlock::Paragraph(RichParagraph {
            list: Some(RichListKind::Ordered),
            list_level: 1,
            ..para(vec![RichRun {
                link: Some("https://example.org/item".to_string()),
                ..run("목록 링크 항목")
            }])
        }),
        RichBlock::Image(png),
        RichBlock::Table(tbl),
    ]);
    source.page_frame.header = vec![RichParagraph {
        alignment: Some(RichAlignment::Center),
        ..para(vec![run("머리글")])
    }];

    let iters = 25u32;
    let t0 = Instant::now();
    let mut last = Vec::new();
    for _ in 0..iters {
        last = write(&source);
    }
    let encode_us = t0.elapsed().as_micros() as f64 / f64::from(iters);
    let t1 = Instant::now();
    for _ in 0..iters {
        let _ = read_back(&last);
    }
    let decode_us = t1.elapsed().as_micros() as f64 / f64::from(iters);
    println!(
        "pages_rich_package_benchmark: size={}B encode_avg={encode_us:.2}us decode_avg={decode_us:.2}us",
        last.len()
    );
    assert!(!last.is_empty());
}
