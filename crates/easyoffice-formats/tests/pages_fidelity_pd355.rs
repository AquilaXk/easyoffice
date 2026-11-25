//! Roundtrip, cross-format export, and hostile-input fidelity tests for `.pages` (PD-355):
//! table column widths (`6006`), cell background fills (`6005`->`6004` + bit 5), RLE cell
//! borders (`6305`->`6306`), `h1..=h6` headings, right indents (`12.19`), landscape orientation
//! & margins/headers/footers, DOCX/RTF/HWPX->Pages conversion roundtrips, ascending protobuf
//! fields invariant, and fail-closed hostile budget/corruption guards.

use easyoffice_formats::pages_iwa::{decompress_iwa, split_records, IwaRecord};
use easyoffice_formats::pages_proto::{Message, WireValue};
use easyoffice_formats::{
    pages_write_loss, CellFill, DecodedContent, DocumentFormat, EasyOfficeError, PagesWriteLoss,
    RichAlignment, RichBinding, RichBindingKind, RichBlock, RichDocument, RichPageFrame,
    RichPageMargins, RichPaper, RichParagraph, RichRun, RichTable, TableBorderEdge,
    TableBorderKind, TableCellBorders, TableCellLink, TableCellStyleEntry,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

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
        page_frame: RichPageFrame::default(),
        title: "PD355충실도".to_string(),
        blocks,
        official_agency: None,
        ..RichDocument::default()
    }
}

fn write_pages(document: &RichDocument) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(document, DocumentFormat::Pages, None).unwrap()
}

fn read_pages(bytes: &[u8]) -> (RichDocument, Vec<String>) {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some("test.pages")).unwrap();
    assert_eq!(decoded.format, DocumentFormat::Pages);
    let DecodedContent::Document(document) = decoded.content else {
        panic!("expected document");
    };
    (document, decoded.warnings)
}

fn varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn f_varint(num: u32, v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    varint(u64::from(num) << 3, &mut out);
    varint(v, &mut out);
    out
}

fn f_bytes(num: u32, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    varint((u64::from(num) << 3) | 2, &mut out);
    varint(data.len() as u64, &mut out);
    out.extend_from_slice(data);
    out
}

fn f_ref(num: u32, id: u64) -> Vec<u8> {
    f_bytes(num, &f_varint(1, id))
}

fn fields_ascending(payload: &[u8]) -> bool {
    let Ok(message) = Message::parse(payload) else {
        return true;
    };
    let numbers: Vec<u32> = message.fields.iter().map(|f| f.number).collect();
    numbers.windows(2).all(|w| w[0] <= w[1])
        && message.fields.iter().all(|f| match f.value {
            WireValue::Bytes(raw) => fields_ascending(raw),
            _ => true,
        })
}

fn encode_records_stream(records: &[IwaRecord]) -> Vec<u8> {
    const BLOCK: usize = 64 * 1024;
    let mut stream = Vec::new();
    for rec in records {
        let mut info_fields = Vec::new();
        info_fields.extend(f_varint(1, rec.info.identifier));
        for (i, msg_info) in rec.info.messages.iter().enumerate() {
            let mut m = f_varint(1, u64::from(msg_info.type_id));
            let mut packed_ver = Vec::new();
            for &ver in &[1u64, 0, 5] {
                varint(ver, &mut packed_ver);
            }
            m.extend(f_bytes(2, &packed_ver));
            let plen = rec
                .payloads
                .get(i)
                .map_or(u64::from(msg_info.length), |p| p.len() as u64);
            m.extend(f_varint(3, plen));
            if !msg_info.object_references.is_empty() {
                let mut packed_refs = Vec::new();
                for &r in &msg_info.object_references {
                    varint(r, &mut packed_refs);
                }
                m.extend(f_bytes(5, &packed_refs));
            }
            info_fields.extend(f_bytes(2, &m));
        }
        varint(info_fields.len() as u64, &mut stream);
        stream.extend(info_fields);
        for p in &rec.payloads {
            stream.extend(p);
        }
    }
    let mut out = Vec::with_capacity(stream.len() + 64);
    for chunk in stream.chunks(BLOCK) {
        let mut block = Vec::with_capacity(chunk.len() + 8);
        varint(chunk.len() as u64, &mut block);
        let n = chunk.len() - 1;
        if n < 60 {
            block.push((n as u8) << 2);
        } else if n < 1 << 8 {
            block.push(60 << 2);
            block.push(n as u8);
        } else {
            block.push(61 << 2);
            block.extend_from_slice(&(n as u16).to_le_bytes());
        }
        block.extend_from_slice(chunk);
        out.push(0);
        out.extend_from_slice(&(block.len() as u32).to_le_bytes()[..3]);
        out.extend_from_slice(&block);
    }
    out
}

#[test]
fn table_col_widths_fills_and_borders_roundtrip_with_zero_loss() {
    let mut tbl = RichTable::with_merges_and_links(
        vec![
            vec!["구분".into(), "금액".into(), "비고".into()],
            vec!["1분기".into(), "".into(), "링크셀".into()],
            vec!["합계".into(), "100".into(), "".into()],
        ],
        Vec::new(),
        vec![TableCellLink {
            row: 1,
            col: 2,
            start: 0,
            end: "링크".len() as u32,
            url: "https://example.org/q1".into(),
        }],
    )
    .0;
    tbl.col_widths_twips = vec![1800, 2400, 3120];
    tbl.cell_styles = vec![
        TableCellStyleEntry {
            row: 0,
            col: 0,
            fill: CellFill::solid(0xE6EEF8),
            borders: TableCellBorders {
                top: TableBorderEdge::new(TableBorderKind::Thick, 0x1F4E79),
                right: TableBorderEdge::new(TableBorderKind::Thin, 0x808080),
                bottom: TableBorderEdge::new(TableBorderKind::Dashed, 0x2E75B6),
                left: TableBorderEdge::new(TableBorderKind::Thick, 0x1F4E79),
            },
        },
        // Empty cell (1, 1) carrying both background fill and dotted/none borders.
        TableCellStyleEntry {
            row: 1,
            col: 1,
            fill: CellFill::solid(0xFFF2CC),
            borders: TableCellBorders {
                top: TableBorderEdge::NONE,
                right: TableBorderEdge::new(TableBorderKind::Dotted, 0xBF9000),
                bottom: TableBorderEdge::new(TableBorderKind::Thin, 0x808080),
                left: TableBorderEdge::NONE,
            },
        },
        TableCellStyleEntry {
            row: 1,
            col: 2,
            fill: CellFill::solid(0xE2EFDA),
            borders: TableCellBorders {
                top: TableBorderEdge::new(TableBorderKind::Thin, 0x548235),
                ..TableCellBorders::default()
            },
        },
    ];

    let source = doc(vec![RichBlock::Table(tbl.clone())]);
    assert_eq!(pages_write_loss(&source), PagesWriteLoss::default());

    let bytes = write_pages(&source);
    for e in ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap() {
        if e.path.starts_with("Index/") && e.path.ends_with(".iwa") {
            let mut budget = usize::MAX;
            let stream = decompress_iwa(&e.data, &mut budget).unwrap();
            for rec in split_records(&stream).unwrap() {
                for p in &rec.payloads {
                    assert!(
                        fields_ascending(p),
                        "out-of-order fields in type {}",
                        rec.info.messages[0].type_id
                    );
                }
            }
        }
    }

    let (back, _) = read_pages(&bytes);
    let RichBlock::Table(out) = &back.blocks[0] else {
        panic!("expected table");
    };
    assert_eq!(out.rows, tbl.rows);
    assert_eq!(out.links, tbl.links);
    assert_eq!(out.col_widths_twips, vec![1800, 2400, 3120]);
    assert_eq!(out.cell_styles.len(), 3);
    assert_eq!(out.cell_styles[0], tbl.cell_styles[0]);
    assert_eq!(out.cell_styles[1], tbl.cell_styles[1]);
    assert_eq!(out.cell_styles[2], tbl.cell_styles[2]);

    // Double borders are approximated as Thick and therefore count 1 format loss.
    let mut double_tbl = RichTable::from(vec![vec!["x".to_string()]]);
    double_tbl.cell_styles.push(TableCellStyleEntry {
        row: 0,
        col: 0,
        fill: CellFill::NONE,
        borders: TableCellBorders {
            bottom: TableBorderEdge::new(TableBorderKind::Double, 0x000000),
            ..TableCellBorders::default()
        },
    });
    assert_eq!(
        pages_write_loss(&doc(vec![RichBlock::Table(double_tbl)])).formats,
        1
    );
}

#[test]
fn headings_h1_to_h6_right_indent_and_landscape_margins_roundtrip() {
    let mut blocks = Vec::new();
    for lvl in 1..=6u8 {
        blocks.push(RichBlock::Paragraph(RichParagraph {
            heading_level: lvl,
            ..para(vec![run(&format!("제목 수준 {lvl}"))])
        }));
    }
    blocks.push(RichBlock::Paragraph(RichParagraph {
        alignment: Some(RichAlignment::Justify),
        left_indent_twips: Some(480),
        right_indent_twips: Some(640),
        first_line_indent_twips: Some(360),
        line_spacing_percent: Some(150),
        ..para(vec![run("좌우 들여쓰기와 양쪽 정렬 문단")])
    }));

    let mut source = doc(blocks);
    let landscape_paper = RichPaper {
        width: 16_838,
        height: 11_906,
    };
    let margins = RichPageMargins {
        left: 1_000,
        right: 1_100,
        top: 1_200,
        bottom: 1_300,
        header: 500,
        footer: 540,
    };
    source.page_frame = RichPageFrame {
        paper: Some(landscape_paper),
        margins: Some(margins),
        binding: RichBinding {
            kind: RichBindingKind::Duplex,
            gutter: 0,
        },
        header: vec![
            RichParagraph {
                alignment: Some(RichAlignment::Left),
                ..para(vec![run("왼쪽 머리말")])
            },
            RichParagraph {
                alignment: Some(RichAlignment::Right),
                ..para(vec![run("오른쪽 머리말")])
            },
        ],
        footer: vec![RichParagraph {
            alignment: Some(RichAlignment::Center),
            ..para(vec![run("바닥글 본문")])
        }],
        ..RichPageFrame::default()
    };
    assert_eq!(pages_write_loss(&source), PagesWriteLoss::default());

    let bytes = write_pages(&source);
    let (back, _) = read_pages(&bytes);
    for lvl in 1..=6u8 {
        let RichBlock::Paragraph(p) = &back.blocks[(lvl - 1) as usize] else {
            panic!("expected heading {lvl}");
        };
        assert_eq!(p.heading_level, lvl);
        assert_eq!(p.plain_text(), format!("제목 수준 {lvl}"));
    }
    let RichBlock::Paragraph(indented) = &back.blocks[6] else {
        panic!("expected body paragraph");
    };
    assert_eq!(indented.alignment, Some(RichAlignment::Justify));
    assert_eq!(indented.left_indent_twips, Some(480));
    assert_eq!(indented.right_indent_twips, Some(640));
    assert_eq!(indented.first_line_indent_twips, Some(360));
    assert_eq!(indented.line_spacing_percent, Some(150));

    assert_eq!(back.page_frame.paper, Some(landscape_paper));
    assert_eq!(back.page_frame.margins, Some(margins));
    assert_eq!(back.page_frame.header.len(), 2);
    assert_eq!(
        back.page_frame.header[0].alignment,
        Some(RichAlignment::Left)
    );
    assert_eq!(
        back.page_frame.header[1].alignment,
        Some(RichAlignment::Right)
    );
}

#[test]
fn cross_format_export_from_docx_hwpx_and_rtf_to_pages_preserves_structure() {
    let mut table = RichTable::from(vec![
        vec!["부서".to_string(), "예산".to_string()],
        vec!["개발".to_string(), "500".to_string()],
    ]);
    table.col_widths_twips = vec![2000, 3000];
    table.cell_styles.push(TableCellStyleEntry {
        row: 0,
        col: 0,
        fill: CellFill::solid(0xDDEBF7),
        borders: TableCellBorders {
            bottom: TableBorderEdge::new(TableBorderKind::Thin, 0x2F5597),
            ..TableCellBorders::default()
        },
    });
    let mut source = doc(vec![
        RichBlock::Paragraph(RichParagraph {
            heading_level: 2,
            right_indent_twips: Some(400),
            ..para(vec![run("분기 보고")])
        }),
        RichBlock::Table(table),
    ]);
    source.page_frame.margins = Some(RichPageMargins {
        left: 900,
        right: 900,
        top: 1000,
        bottom: 1000,
        header: 450,
        footer: 450,
    });

    for src_fmt in [
        DocumentFormat::Docx,
        DocumentFormat::Odt,
        DocumentFormat::Hwpx,
        DocumentFormat::Rtf,
    ] {
        let encoded_src = UniversalFormatConverter::encode_rich(&source, src_fmt, None).unwrap();
        let pages_bytes = UniversalFormatConverter::convert(
            &encoded_src,
            Some(src_fmt.extension()),
            DocumentFormat::Pages,
        )
        .unwrap();
        let (back, _) = read_pages(&pages_bytes);
        let has_heading = back.blocks.iter().any(|b| {
            matches!(
                b,
                RichBlock::Paragraph(p) if p.heading_level == 2 && p.plain_text() == "분기 보고"
            )
        });
        assert!(has_heading, "heading lost exporting from {src_fmt:?}");
        let out_tbl = back
            .blocks
            .iter()
            .find_map(|b| match b {
                RichBlock::Table(t) => Some(t),
                _ => None,
            })
            .expect("table preserved");
        assert_eq!(out_tbl.rows[1][0], "개발");
        if matches!(src_fmt, DocumentFormat::Docx | DocumentFormat::Odt) {
            assert_eq!(out_tbl.col_widths_twips, vec![2000, 3000], "{src_fmt:?}");
            assert!(
                !out_tbl.cell_styles.is_empty(),
                "cell styles lost exporting from {src_fmt:?}"
            );
        }
    }
}

#[test]
fn hostile_header_bucket_and_stroke_sidecar_budgets_fail_closed() {
    let mut tbl = RichTable::from(vec![vec!["셀".to_string()]]);
    tbl.col_widths_twips = vec![2000];
    tbl.cell_styles.push(TableCellStyleEntry {
        row: 0,
        col: 0,
        fill: CellFill::solid(0xABCDEF),
        borders: TableCellBorders {
            top: TableBorderEdge::new(TableBorderKind::Thin, 0x112233),
            ..TableCellBorders::default()
        },
    });
    let base_bytes = write_pages(&doc(vec![RichBlock::Table(tbl)]));
    let entries = ZipArchiveCodec::unpack(&base_bytes, &ZipSecurityPolicy::default()).unwrap();

    // Case 1: Overflow header bucket entries (> 256 * 16 = 4096) -> ResourceBudgetExceeded.
    let doc_entry = entries
        .iter()
        .find(|e| e.path == "Index/Document.iwa")
        .unwrap();
    let mut budget = usize::MAX;
    let stream = decompress_iwa(&doc_entry.data, &mut budget).unwrap();
    let base_records = split_records(&stream).unwrap();

    let mut header_bomb_records = base_records.clone();
    let bucket_rec = header_bomb_records
        .iter_mut()
        .find(|r| r.info.messages.first().map(|m| m.type_id) == Some(6006))
        .expect("header bucket record");
    let mut big_bucket = Vec::new();
    let one_entry = [f_varint(1, 0), {
        let mut out = Vec::new();
        varint((2u64 << 3) | 5, &mut out);
        out.extend_from_slice(&100.0f32.to_le_bytes());
        out
    }]
    .concat();
    let packed_entry = f_bytes(2, &one_entry);
    for _ in 0..4_100 {
        big_bucket.extend_from_slice(&packed_entry);
    }
    bucket_rec.payloads[0] = big_bucket;

    let mut repacked: Vec<(String, Vec<u8>)> = entries
        .iter()
        .map(|e| {
            (
                e.path.clone(),
                if e.path == "Index/Document.iwa" {
                    encode_records_stream(&header_bomb_records)
                } else {
                    e.data.clone()
                },
            )
        })
        .collect();
    let bad_zip = ZipArchiveCodec::pack(&repacked).unwrap();
    let err = UniversalFormatConverter::decode_rich(&bad_zip, Some("bad-header.pages"));
    assert!(
        matches!(err, Err(EasyOfficeError::ResourceBudgetExceeded(_))),
        "expected ResourceBudgetExceeded for header bucket bomb, got {err:?}"
    );

    // Case 2: Overflow stroke layer runs (> 250_000) -> ResourceBudgetExceeded.
    let mut stroke_bomb_records = base_records;
    let layer_rec = stroke_bomb_records
        .iter_mut()
        .find(|r| r.info.messages.first().map(|m| m.type_id) == Some(6306))
        .expect("stroke layer record");
    let single_run = [f_varint(1, 0), f_varint(2, 1)].concat();
    let wrapped_run = f_bytes(2, &single_run);
    let mut big_layer = f_varint(1, 0);
    big_layer.reserve(wrapped_run.len() * 250_005);
    for _ in 0..250_005 {
        big_layer.extend_from_slice(&wrapped_run);
    }
    layer_rec.payloads[0] = big_layer;
    for (p, data) in &mut repacked {
        if p == "Index/Document.iwa" {
            *data = encode_records_stream(&stroke_bomb_records);
        }
    }
    let bad_stroke_zip = ZipArchiveCodec::pack(&repacked).unwrap();
    let err2 = UniversalFormatConverter::decode_rich(&bad_stroke_zip, Some("bad-stroke.pages"));
    assert!(
        matches!(err2, Err(EasyOfficeError::ResourceBudgetExceeded(_))),
        "expected ResourceBudgetExceeded for stroke run bomb, got {err2:?}"
    );

    // Case 3: Corrupt/truncated sidecar & style object references never panic.
    let mut corrupt_records = split_records(&stream).unwrap();
    for rec in &mut corrupt_records {
        if rec.info.messages.first().map(|m| m.type_id) == Some(6004) {
            rec.payloads[0] = vec![0xFF, 0x00, 0x80];
        }
        if rec.info.messages.first().map(|m| m.type_id) == Some(6305) {
            rec.payloads[0] = f_ref(4, 999_999);
        }
    }
    for (p, data) in &mut repacked {
        if p == "Index/Document.iwa" {
            *data = encode_records_stream(&corrupt_records);
        }
    }
    let corrupt_zip = ZipArchiveCodec::pack(&repacked).unwrap();
    let res3 = UniversalFormatConverter::decode_rich(&corrupt_zip, Some("corrupt-style.pages"));
    assert!(
        res3.is_ok() || res3.is_err(),
        "corrupt style payload must decode gracefully or return typed error without panicking"
    );
}

fn f_float(num: u32, value: f32) -> Vec<u8> {
    let mut out = Vec::new();
    varint((u64::from(num) << 3) | 5, &mut out);
    out.extend_from_slice(&value.to_le_bytes());
    out
}

#[test]
fn right_indent_float_and_web_edition_line_spacing_field_19_do_not_interfere() {
    let source = doc(vec![
        RichBlock::Paragraph(RichParagraph {
            right_indent_twips: Some(720),
            line_spacing_percent: Some(180),
            ..para(vec![run("오른쪽 들여쓰기 + 줄간격")])
        }),
        RichBlock::Paragraph(RichParagraph {
            right_indent_twips: Some(400),
            ..para(vec![run("오른쪽 들여쓰기 단독")])
        }),
    ]);
    let bytes = write_pages(&source);
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let stylesheet_entry = entries
        .iter()
        .find(|e| e.path == "Index/DocumentStylesheet.iwa")
        .unwrap();
    let mut budget = usize::MAX;
    let stream = decompress_iwa(&stylesheet_entry.data, &mut budget).unwrap();
    let mut records = split_records(&stream).unwrap();

    // Mutate one paragraph style so field 12 holds web-edition line spacing (submessage 19)
    // instead of Fixed32 right indent (float 19), verifying clean wire-type separation.
    let mut mutated_web_style = false;
    'outer: for rec in &mut records {
        for (msg_info, payload) in rec.info.messages.iter().zip(rec.payloads.iter_mut()) {
            if msg_info.type_id == 2022 {
                if let Ok(msg) = Message::parse(payload) {
                    if let Ok(Some(props)) = msg.sub(12) {
                        if props.float(19).is_some() && props.sub(13).ok().flatten().is_none() {
                            let spacing_msg = [f_varint(1, 0), f_float(2, 1.4)].concat();
                            let new_para_props = [f_bytes(19, &spacing_msg)].concat();
                            let mut rebuilt = Vec::new();
                            for f in &msg.fields {
                                if f.number == 12 {
                                    rebuilt.extend(f_bytes(12, &new_para_props));
                                } else if let WireValue::Varint(v) = f.value {
                                    rebuilt.extend(f_varint(f.number, v));
                                } else if let WireValue::Bytes(raw) = f.value {
                                    rebuilt.extend(f_bytes(f.number, raw));
                                }
                            }
                            *payload = rebuilt;
                            mutated_web_style = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    assert!(
        mutated_web_style,
        "expected standalone right_indent variation"
    );

    let repacked: Vec<(String, Vec<u8>)> = entries
        .iter()
        .map(|e| {
            (
                e.path.clone(),
                if e.path == "Index/DocumentStylesheet.iwa" {
                    encode_records_stream(&records)
                } else {
                    e.data.clone()
                },
            )
        })
        .collect();
    let (back, _) = read_pages(&ZipArchiveCodec::pack(&repacked).unwrap());
    let RichBlock::Paragraph(first) = &back.blocks[0] else {
        panic!("expected first paragraph");
    };
    assert_eq!(first.right_indent_twips, Some(720));
    assert_eq!(first.line_spacing_percent, Some(180));
    let RichBlock::Paragraph(second) = &back.blocks[1] else {
        panic!("expected second paragraph");
    };
    assert_eq!(second.right_indent_twips, None);
    assert_eq!(second.line_spacing_percent, Some(140));
}

#[test]
fn hostile_rle_stroke_expansion_and_shared_sidecar_amplification_fail_closed() {
    use easyoffice_formats::pages_iwa::IwaRecord;
    use easyoffice_formats::pages_proto::{ArchiveInfo, MessageInfo};

    let mut tbl = RichTable::from(vec![vec!["셀".to_string()]]);
    tbl.cell_styles.push(TableCellStyleEntry {
        row: 0,
        col: 0,
        fill: CellFill::NONE,
        borders: TableCellBorders {
            left: TableBorderEdge::new(TableBorderKind::Thin, 0x112233),
            ..TableCellBorders::default()
        },
    });
    let base_bytes = write_pages(&doc(vec![RichBlock::Table(tbl)]));
    let entries = ZipArchiveCodec::unpack(&base_bytes, &ZipSecurityPolicy::default()).unwrap();
    let doc_entry = entries
        .iter()
        .find(|e| e.path == "Index/Document.iwa")
        .unwrap();
    let mut budget = usize::MAX;
    let stream = decompress_iwa(&doc_entry.data, &mut budget).unwrap();
    let base_records = split_records(&stream).unwrap();

    let stroke_msg = [
        f_bytes(
            1,
            &[
                f_float(1, 0.0),
                f_float(2, 0.0),
                f_float(3, 0.0),
                f_float(4, 1.0),
            ]
            .concat(),
        ),
        f_float(2, 1.0),
        f_bytes(3, &f_varint(1, 0)),
    ]
    .concat();
    let solid_long_run = f_bytes(
        2,
        &[
            f_varint(1, 0),
            f_varint(2, 10_000),
            f_bytes(3, &stroke_msg),
            f_varint(4, 1),
        ]
        .concat(),
    );

    let pack_doc_iwa = |recs: &[IwaRecord]| {
        let repacked: Vec<(String, Vec<u8>)> = entries
            .iter()
            .map(|e| {
                (
                    e.path.clone(),
                    if e.path == "Index/Document.iwa" {
                        encode_records_stream(recs)
                    } else {
                        e.data.clone()
                    },
                )
            })
            .collect();
        ZipArchiveCodec::pack(&repacked).unwrap()
    };

    // Case 1: Single 10_000 x 1 table with 250 overlapping runs of length 10_000 (2_500_000
    // expanded cells, far below MAX_STROKE_RUNS = 250_000) must fail closed before looping.
    let mut single_table_dos = base_records.clone();
    for rec in &mut single_table_dos {
        match rec.info.messages.first().map(|m| m.type_id) {
            Some(6001) => {
                let msg = Message::parse(&rec.payloads[0]).unwrap();
                let mut rebuilt = [f_varint(6, 10_000), f_varint(7, 1)].concat();
                for f in &msg.fields {
                    if !matches!(f.number, 6 | 7) {
                        if let WireValue::Varint(v) = f.value {
                            rebuilt.extend(f_varint(f.number, v));
                        } else if let WireValue::Bytes(raw) = f.value {
                            rebuilt.extend(f_bytes(f.number, raw));
                        }
                    }
                }
                rec.payloads[0] = rebuilt;
            }
            Some(6306) => {
                let mut payload = f_varint(1, 0);
                for _ in 0..250 {
                    payload.extend_from_slice(&solid_long_run);
                }
                rec.payloads[0] = payload;
            }
            _ => {}
        }
    }
    let err1 =
        UniversalFormatConverter::decode_rich(&pack_doc_iwa(&single_table_dos), Some("a.pages"));
    assert!(
        matches!(&err1, Err(EasyOfficeError::ResourceBudgetExceeded(msg)) if msg.contains("테두리 확장 셀")),
        "expected ResourceBudgetExceeded for overlapping long RLE runs, got {err1:?}"
    );

    let mut tile_dos = base_records.clone();
    // Case 2: 400 tables (10_000 x 1 = 4_000_000 total cells <= MAX_TOTAL_CELLS) sharing one
    // STROKE_SIDECAR with 50 runs of length 10_000 (500_000 per table <= per-table limit,
    // 200_000_000 across doc) must trip TableBudget cumulative stroke-cell cap immediately.
    let mut multi_table_dos = base_records;
    let max_id = multi_table_dos
        .iter()
        .map(|r| r.info.identifier)
        .max()
        .unwrap_or(1_000);
    let mut next_id = max_id + 100;
    let mut store_id_opt = None;
    let mut sidecar_id_opt = None;
    let mut info_drawable_opt = None;
    for rec in &mut multi_table_dos {
        match rec.info.messages.first().map(|m| m.type_id) {
            Some(6000) => {
                let msg = Message::parse(&rec.payloads[0]).unwrap();
                info_drawable_opt = msg
                    .fields
                    .iter()
                    .find(|f| f.number == 1)
                    .and_then(|f| match f.value {
                        WireValue::Bytes(b) => Some(b.to_vec()),
                        _ => None,
                    });
            }
            Some(6001) => {
                let msg = Message::parse(&rec.payloads[0]).unwrap();
                store_id_opt =
                    msg.fields
                        .iter()
                        .find(|f| f.number == 4)
                        .and_then(|f| match f.value {
                            WireValue::Bytes(b) => Some(b.to_vec()),
                            _ => None,
                        });
                sidecar_id_opt =
                    msg.fields
                        .iter()
                        .find(|f| f.number == 49)
                        .and_then(|f| match f.value {
                            WireValue::Bytes(b) => Some(b.to_vec()),
                            _ => None,
                        });
                let mut rebuilt = [f_varint(6, 10_000), f_varint(7, 1)].concat();
                for f in &msg.fields {
                    if !matches!(f.number, 6 | 7) {
                        if let WireValue::Varint(v) = f.value {
                            rebuilt.extend(f_varint(f.number, v));
                        } else if let WireValue::Bytes(raw) = f.value {
                            rebuilt.extend(f_bytes(f.number, raw));
                        }
                    }
                }
                rec.payloads[0] = rebuilt;
            }
            Some(6306) => {
                let mut payload = f_varint(1, 0);
                for _ in 0..50 {
                    payload.extend_from_slice(&solid_long_run);
                }
                rec.payloads[0] = payload;
            }
            _ => {}
        }
    }
    let store_bytes = store_id_opt.expect("table data store field");
    let sidecar_bytes = sidecar_id_opt.expect("table sidecar ref");
    let info_drawable_bytes = info_drawable_opt.expect("table info drawable field");
    let body_storage_id = multi_table_dos
        .iter()
        .find(|r| r.info.messages.first().map(|m| m.type_id) == Some(10000))
        .and_then(|r| Message::parse(&r.payloads[0]).ok())
        .and_then(|m| m.sub(4).ok().flatten())
        .and_then(|s| s.varint(1))
        .unwrap();

    let mut extra_attachments = Vec::new();
    for _ in 0..399 {
        let model_id = next_id;
        let info_id = next_id + 1;
        let att_id = next_id + 2;
        next_id += 3;
        let mut model_payload = [f_varint(6, 10_000), f_varint(7, 1)].concat();
        model_payload.extend(f_bytes(4, &store_bytes));
        model_payload.extend(f_bytes(49, &sidecar_bytes));
        multi_table_dos.push(IwaRecord {
            info: ArchiveInfo {
                identifier: model_id,
                messages: vec![MessageInfo {
                    type_id: 6001,
                    length: model_payload.len() as u32,
                    object_references: Vec::new(),
                    data_references: Vec::new(),
                }],
            },
            payloads: vec![model_payload],
        });
        let info_payload = [f_bytes(1, &info_drawable_bytes), f_ref(2, model_id)].concat();
        multi_table_dos.push(IwaRecord {
            info: ArchiveInfo {
                identifier: info_id,
                messages: vec![MessageInfo {
                    type_id: 6000,
                    length: info_payload.len() as u32,
                    object_references: Vec::new(),
                    data_references: Vec::new(),
                }],
            },
            payloads: vec![info_payload],
        });
        let att_payload = f_ref(1, info_id);
        multi_table_dos.push(IwaRecord {
            info: ArchiveInfo {
                identifier: att_id,
                messages: vec![MessageInfo {
                    type_id: 2003,
                    length: att_payload.len() as u32,
                    object_references: Vec::new(),
                    data_references: Vec::new(),
                }],
            },
            payloads: vec![att_payload],
        });
        extra_attachments.push(att_id);
    }
    let body_rec = multi_table_dos
        .iter_mut()
        .find(|r| r.info.identifier == body_storage_id)
        .unwrap();
    let body_msg = Message::parse(&body_rec.payloads[0]).unwrap();
    let old_text = body_msg.string(3).ok().flatten().unwrap_or_default();
    let mut new_text = old_text;
    let mut att_entries = Vec::new();
    if let Ok(Some(att_tbl)) = body_msg.sub(9) {
        let _ = att_tbl.for_each_sub(1, |e| {
            let idx = e.varint(1).unwrap_or(0);
            let obj = e
                .sub(2)
                .ok()
                .flatten()
                .and_then(|s| s.varint(1))
                .unwrap_or(0);
            att_entries.push([f_varint(1, idx), f_ref(2, obj)].concat());
            Ok(())
        });
    }
    for att_id in extra_attachments {
        let char_idx = new_text.encode_utf16().count() as u64;
        new_text.push('\u{FFFC}');
        att_entries.push([f_varint(1, char_idx), f_ref(2, att_id)].concat());
    }
    let mut rebuilt_body = Vec::new();
    for f in &body_msg.fields {
        match f.number {
            3 => rebuilt_body.extend(f_bytes(3, new_text.as_bytes())),
            9 => {
                let packed: Vec<u8> = att_entries.iter().flat_map(|e| f_bytes(1, e)).collect();
                rebuilt_body.extend(f_bytes(9, &packed));
            }
            n => match f.value {
                WireValue::Varint(v) => rebuilt_body.extend(f_varint(n, v)),
                WireValue::Bytes(raw) => rebuilt_body.extend(f_bytes(n, raw)),
                _ => {}
            },
        }
    }
    body_rec.payloads[0] = rebuilt_body;

    let err2 =
        UniversalFormatConverter::decode_rich(&pack_doc_iwa(&multi_table_dos), Some("multi.pages"));
    assert!(
        matches!(&err2, Err(EasyOfficeError::ResourceBudgetExceeded(msg)) if msg.contains("테두리 확장 셀 총수")),
        "expected ResourceBudgetExceeded for shared sidecar multi-table DoS, got {err2:?}"
    );

    // Case 3: Multi-table shared sidecar with 5_500 zero-length runs across 400 small tables
    // (1 x 1 each) must trip TableBudget cumulative stroke-run cap instead of scanning 2M+ runs.
    let mut noop_multi_dos = multi_table_dos;
    let noop_run = f_bytes(2, &[f_varint(1, 0), f_varint(2, 0)].concat());
    for rec in &mut noop_multi_dos {
        match rec.info.messages.first().map(|m| m.type_id) {
            Some(6001) => {
                let msg = Message::parse(&rec.payloads[0]).unwrap();
                let mut rebuilt = [f_varint(6, 1), f_varint(7, 1)].concat();
                for f in &msg.fields {
                    if !matches!(f.number, 6 | 7) {
                        if let WireValue::Varint(v) = f.value {
                            rebuilt.extend(f_varint(f.number, v));
                        } else if let WireValue::Bytes(raw) = f.value {
                            rebuilt.extend(f_bytes(f.number, raw));
                        }
                    }
                }
                rec.payloads[0] = rebuilt;
            }
            Some(6306) => {
                let mut payload = f_varint(1, 0);
                for _ in 0..5_500 {
                    payload.extend_from_slice(&noop_run);
                }
                rec.payloads[0] = payload;
            }
            _ => {}
        }
    }
    let err3 = UniversalFormatConverter::decode_rich(
        &pack_doc_iwa(&noop_multi_dos),
        Some("noop-multi.pages"),
    );
    assert!(
        matches!(&err3, Err(EasyOfficeError::ResourceBudgetExceeded(msg)) if msg.contains("테두리 런 총수")),
        "expected ResourceBudgetExceeded for shared sidecar noop-run amplification, got {err3:?}"
    );

    // Case 4: Shared TABLE_TILE referenced by 1_000 TILES_ENTRIES x 20 TILE_ROWS x 256 cols
    // (5_120_000 tile cells > MAX_TILE_CELLS_PER_TABLE) must fail closed before looping.
    let mut tile_id = None;
    for rec in &mut tile_dos {
        if rec.info.messages.first().map(|m| m.type_id) == Some(6002) {
            tile_id = Some(rec.info.identifier);
            let row_payload = [
                f_varint(1, 0),
                f_bytes(6, &[]),
                f_bytes(7, &[0xFF, 0xFF].repeat(256)),
            ]
            .concat();
            let mut tile_payload = Vec::new();
            for _ in 0..20 {
                tile_payload.extend_from_slice(&f_bytes(5, &row_payload));
            }
            rec.payloads[0] = tile_payload;
            break;
        }
    }
    let tid = tile_id.expect("table tile object");
    let tile_entry = f_bytes(1, &[f_varint(1, 0), f_ref(2, tid)].concat());
    let mut tiles_msg = f_varint(2, 1);
    for _ in 0..1_000 {
        tiles_msg.extend_from_slice(&tile_entry);
    }
    for rec in &mut tile_dos {
        if rec.info.messages.first().map(|m| m.type_id) == Some(6001) {
            let msg = Message::parse(&rec.payloads[0]).unwrap();
            let mut rebuilt = [f_varint(6, 1), f_varint(7, 256)].concat();
            for f in &msg.fields {
                if f.number == 4 {
                    if let WireValue::Bytes(raw) = f.value {
                        let store = Message::parse(raw).unwrap();
                        let mut store_rebuilt = Vec::new();
                        for sf in &store.fields {
                            if sf.number == 3 {
                                store_rebuilt.extend(f_bytes(3, &tiles_msg));
                            } else if let WireValue::Varint(v) = sf.value {
                                store_rebuilt.extend(f_varint(sf.number, v));
                            } else if let WireValue::Bytes(sraw) = sf.value {
                                store_rebuilt.extend(f_bytes(sf.number, sraw));
                            }
                        }
                        rebuilt.extend(f_bytes(4, &store_rebuilt));
                    }
                } else if !matches!(f.number, 6 | 7) {
                    if let WireValue::Varint(v) = f.value {
                        rebuilt.extend(f_varint(f.number, v));
                    } else if let WireValue::Bytes(raw) = f.value {
                        rebuilt.extend(f_bytes(f.number, raw));
                    }
                }
            }
            rec.payloads[0] = rebuilt;
            break;
        }
    }
    let err4 =
        UniversalFormatConverter::decode_rich(&pack_doc_iwa(&tile_dos), Some("tile-dos.pages"));
    assert!(
        matches!(&err4, Err(EasyOfficeError::ResourceBudgetExceeded(msg)) if msg.contains("타일")),
        "expected ResourceBudgetExceeded for shared tile amplification, got {err4:?}"
    );
}
