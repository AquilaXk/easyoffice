//! Pages minimal writer (PD-162): structure rules of the package, read-back through our own reader
//! (a circular check: it proves the writer and the reader agree, not that Pages opens the file),
//! limits and the loss counts behind the save warnings.

use easyoffice_formats::pages_iwa::{decompress_iwa, snappy_decompress, split_records, IwaRecord};
use easyoffice_formats::pages_proto::{Message, WireValue};
use easyoffice_formats::{
    pages_write_loss, DecodedContent, DocumentFormat, EasyOfficeError, RichAlignment, RichBlock,
    RichDocument, RichListKind, RichParagraph, RichRun, RichTable, UniversalDocumentModel,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};
use std::collections::BTreeSet;

const MAX_BLOCK: usize = 64 * 1024;
/// Body style of the writer: 12 pt (24 half-points), black. A read-back run that sets neither
/// carries these, because the reader resolves the paragraph style into the run.
const BODY_HALF_POINTS: u16 = 24;
const BODY_COLOR: &str = "000000";

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
        title: "쓰기".to_string(),
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

fn formatted_document() -> RichDocument {
    let bold = RichRun {
        bold: true,
        ..run("굵게")
    };
    let styled = RichRun {
        italic: true,
        underline: true,
        size_half_pt: Some(28),
        ..run("기울임·밑줄·14pt")
    };
    let red = RichRun {
        color: Some("FF0000".to_string()),
        ..run("빨강")
    };
    let aligned = |alignment, text: &str| {
        RichBlock::Paragraph(RichParagraph {
            alignment: Some(alignment),
            ..para(vec![run(text)])
        })
    };
    let heading = |level, text: &str| {
        RichBlock::Paragraph(RichParagraph {
            heading_level: level,
            ..para(vec![run(text)])
        })
    };
    doc(vec![
        heading(1, "제목"),
        RichBlock::Paragraph(para(vec![bold, run(" 보통 "), styled, run(" "), red])),
        heading(2, "소제목"),
        aligned(RichAlignment::Center, "가운데"),
        aligned(RichAlignment::Right, "오른쪽"),
        aligned(RichAlignment::Justify, "양쪽 맞춤"),
        aligned(RichAlignment::Left, "왼쪽"),
        RichBlock::Paragraph(para(Vec::new())),
        heading(6, "여섯째 제목"),
        RichBlock::Paragraph(para(vec![run("끝")])),
    ])
}

#[test]
fn text_headings_alignment_and_run_formats_come_back() {
    let source = formatted_document();
    let (back, _) = read_back(&write(&source));
    assert_eq!(back.blocks.len(), source.blocks.len());
    for (want, got) in source.blocks.iter().zip(&back.blocks) {
        let (RichBlock::Paragraph(want), RichBlock::Paragraph(got)) = (want, got) else {
            panic!("only paragraphs were written");
        };
        assert_eq!(got.plain_text(), want.plain_text());
        assert_eq!(
            got.heading_level,
            want.heading_level,
            "{}",
            got.plain_text()
        );
        if want.heading_level == 0 {
            assert_eq!(got.alignment, want.alignment, "{}", got.plain_text());
            for (w, g) in want.runs.iter().zip(&got.runs) {
                assert_eq!(g.text, w.text);
                assert_eq!(
                    (g.bold, g.italic, g.underline),
                    (w.bold, w.italic, w.underline)
                );
                assert_eq!(g.size_half_pt, w.size_half_pt.or(Some(BODY_HALF_POINTS)));
                assert_eq!(g.color.as_deref(), w.color.as_deref().or(Some(BODY_COLOR)));
            }
            assert_eq!(got.runs.len(), want.runs.len(), "{}", got.plain_text());
        }
    }
    assert_eq!(back.title, "제목");
}

#[test]
fn text_with_line_breaks_and_control_characters_is_cleaned_like_the_reader_does() {
    let messy = para(vec![run("첫 줄\n둘째 줄\r\u{0}셋째\u{FFFC}줄\u{4}끝")]);
    let (back, _) = read_back(&write(&doc(vec![RichBlock::Paragraph(messy)])));
    assert_eq!(back.blocks.len(), 1);
    let RichBlock::Paragraph(got) = &back.blocks[0] else {
        panic!("paragraph");
    };
    assert_eq!(got.plain_text(), "첫 줄 둘째 줄 셋째줄끝");
}

#[test]
fn an_astral_character_keeps_the_formatting_after_it() {
    let bold = RichRun {
        bold: true,
        ..run("굵게")
    };
    let (back, _) = read_back(&write(&doc(vec![RichBlock::Paragraph(para(vec![
        run("😀😀"),
        bold,
    ]))])));
    let RichBlock::Paragraph(got) = &back.blocks[0] else {
        panic!("paragraph");
    };
    assert_eq!(got.runs.len(), 2);
    assert!(!got.runs[0].bold && got.runs[1].bold);
    assert_eq!(got.runs[1].text, "굵게");
}

// -- package structure -----------------------------------------------------------------------

/// `(name, method)` of every local file header, in file order.
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

fn entry(bytes: &[u8], path: &str) -> Vec<u8> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("missing {path}"))
        .data
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

#[test]
fn every_zip_entry_is_stored_and_the_expected_parts_exist() {
    let entries = local_entries(&write(&formatted_document()));
    let names: BTreeSet<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    for part in [
        "Index/Document.iwa",
        "Index/DocumentMetadata.iwa",
        "Index/DocumentStylesheet.iwa",
        "Index/Metadata.iwa",
        "Metadata/Properties.plist",
        "Metadata/DocumentIdentifier",
    ] {
        assert!(names.contains(part), "missing {part}: {names:?}");
    }
    assert!(
        entries.iter().all(|(_, method)| *method == 0),
        "{entries:?}"
    );
}

#[test]
fn snappy_blocks_stay_within_64_kib_and_a_long_document_needs_several() {
    let paragraphs = (0..3_000)
        .map(|i| RichBlock::Paragraph(para(vec![run(&format!("{i} 가나다라마바사 ").repeat(8))])))
        .collect();
    let bytes = write(&doc(paragraphs));
    let iwa = entry(&bytes, "Index/Document.iwa");
    let (mut at, mut blocks, mut total) = (0usize, 0usize, 0usize);
    while at < iwa.len() {
        assert_eq!(iwa[at], 0, "chunk type");
        let len = usize::from(iwa[at + 1])
            | usize::from(iwa[at + 2]) << 8
            | usize::from(iwa[at + 3]) << 16;
        let block = snappy_decompress(&iwa[at + 4..at + 4 + len], MAX_BLOCK).unwrap();
        assert!(block.len() <= MAX_BLOCK);
        total += block.len();
        blocks += 1;
        at += 4 + len;
    }
    assert!(
        blocks >= 5 && total > 4 * MAX_BLOCK,
        "{blocks} blocks, {total} bytes"
    );
    let (back, _) = read_back(&bytes);
    assert_eq!(back.blocks.len(), 3_000);
}

/// Field numbers of a message in wire order, and whether every nested message that is plainly a
/// message (it holds control bytes, so it is no text) is in order too.
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
fn protobuf_fields_are_written_in_ascending_order() {
    let bytes = write(&formatted_document());
    let all = records(&bytes);
    assert!(all.len() > 40, "{} objects", all.len());
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
}

fn reference_ids(message: &Message, table: u32) -> BTreeSet<u64> {
    let mut ids = BTreeSet::new();
    if let Some(t) = message.sub(table).unwrap() {
        for entry in t.subs(1).unwrap() {
            if let Some(object) = entry.sub(2).unwrap() {
                ids.insert(object.varint(1).unwrap());
            }
        }
    }
    ids
}

#[test]
fn message_info_lists_the_strong_references_of_each_object() {
    let all = records(&write(&formatted_document()));
    let known: BTreeSet<u64> = all.iter().map(|r| r.info.identifier).collect();
    for record in &all {
        for id in &record.info.messages[0].object_references {
            assert!(
                known.contains(id),
                "object {} lists unknown {id}",
                record.info.identifier
            );
        }
    }
    let by_type = |t| payload_of(&all, t);
    let (stylesheet_id, _) = by_type(401)[0];
    let (body_id, body) = by_type(2001)[0];
    let body_info = all.iter().find(|r| r.info.identifier == body_id).unwrap();
    let listed: BTreeSet<u64> = body_info.info.messages[0]
        .object_references
        .iter()
        .copied()
        .collect();
    let message = Message::parse(body).unwrap();
    let mut owned = BTreeSet::new();
    for table in [5, 7, 8, 12] {
        owned.extend(reference_ids(&message, table));
    }
    assert!(owned.len() >= 6, "{owned:?}");
    assert_eq!(
        listed, owned,
        "a storage owns its styles, not its stylesheet"
    );
    assert!(!listed.contains(&stylesheet_id));
    let (_, root) = by_type(10000)[0];
    let root_info = all
        .iter()
        .find(|r| r.info.messages[0].type_id == 10000)
        .unwrap();
    assert!(
        root_info.info.messages[0]
            .object_references
            .contains(&body_id),
        "{root:?}"
    );
}

fn package_metadata(bytes: &[u8]) -> Vec<u8> {
    let all = records(bytes);
    payload_of(&all, 11006)[0].1.to_vec()
}

#[test]
fn package_metadata_names_every_component_and_declares_roots_in_the_short_form() {
    let bytes = write(&formatted_document());
    let all = records(&bytes);
    let raw = package_metadata(&bytes);
    let meta = Message::parse(&raw).unwrap();
    for field in [5, 7, 8] {
        assert!(
            meta.bytes(field).is_some() || meta.varint(field).is_some(),
            "field {field}"
        );
    }
    let highest = meta.varint(1).unwrap();
    assert!(
        all.iter()
            .all(|r| r.info.identifier <= highest || r.info.identifier == 2),
        "new identifiers must start above field 1"
    );
    let max_object = all
        .iter()
        .map(|r| r.info.identifier)
        .filter(|id| *id != 71)
        .max()
        .unwrap();
    assert!(highest >= max_object);

    let components = meta.subs(3).unwrap();
    assert_eq!(components.len(), 3);
    let mut stylesheet = 0;
    for c in &components {
        for field in [1, 10, 12] {
            assert!(c.varint(field).is_some(), "component field {field}");
        }
        assert!(c.string(2).unwrap().is_some());
        if c.string(2).unwrap().as_deref() == Some("DocumentStylesheet") {
            stylesheet = c.varint(1).unwrap();
        }
    }
    assert_ne!(stylesheet, 0);
    let document = components
        .iter()
        .find(|c| c.string(2).unwrap().as_deref() == Some("Document"))
        .unwrap();
    let declared = document.subs(6).unwrap();
    let root_form = declared
        .iter()
        .find(|d| d.varint(1) == Some(stylesheet) && d.varint(2).is_none());
    assert!(
        root_form.is_some(),
        "the stylesheet root is declared as `{{component}}`"
    );
    assert!(declared
        .iter()
        .any(|d| d.varint(1) == Some(stylesheet) && d.varint(2).is_some()));
    for d in &declared {
        assert_ne!(
            d.varint(2),
            Some(stylesheet),
            "a root is never declared the long way"
        );
        assert!(d.varint(1) != Some(1) && d.varint(1) != Some(71));
    }
}

#[test]
fn colors_carry_every_field_and_variations_carry_no_name() {
    let all = records(&write(&formatted_document()));
    let mut colors = 0;
    let mut variations = 0;
    for ty in [2021u32, 2022, 2024] {
        for (id, payload) in payload_of(&all, ty) {
            let message = Message::parse(payload).unwrap();
            let header = message.sub(1).unwrap().unwrap();
            if header.varint(4) == Some(1) {
                variations += 1;
                assert!(
                    header.string(1).unwrap().is_none() && header.string(2).unwrap().is_none(),
                    "variation {id} is named"
                );
                assert!(
                    header.sub(3).unwrap().is_some(),
                    "variation {id} has no parent"
                );
            } else {
                assert!(
                    header.string(1).unwrap().is_some() && header.string(2).unwrap().is_some(),
                    "style {id} is unnamed"
                );
            }
            // A column style's field 11.7 is a reference, not a colour.
            if let (true, Some(bag)) = (ty != 2024, message.sub(11).unwrap()) {
                if let Some(color) = bag.sub(7).unwrap() {
                    colors += 1;
                    for field in [1, 3, 4, 5, 6, 12] {
                        assert!(
                            color.fields.iter().any(|f| f.number == field),
                            "color of {id} lacks field {field}"
                        );
                    }
                }
            }
        }
    }
    assert!(
        colors >= 3 && variations >= 6,
        "{colors} colors, {variations} variations"
    );
}

#[test]
fn identity_is_new_for_every_package() {
    let source = formatted_document();
    let (a, b) = (write(&source), write(&source));
    let ids = (
        entry(&a, "Metadata/DocumentIdentifier"),
        entry(&b, "Metadata/DocumentIdentifier"),
    );
    assert_eq!(ids.0.len(), 36);
    assert_ne!(ids.0, ids.1);
    let plist = entry(&a, "Metadata/Properties.plist");
    assert!(plist.starts_with(b"bplist00"));
    assert!(plist.windows(36).any(|w| w == ids.0.as_slice()));
}

// -- limits, flat model, loss counts ---------------------------------------------------------

#[test]
fn more_paragraphs_than_the_reader_accepts_are_refused() {
    let blocks = (0..200_001)
        .map(|_| RichBlock::Paragraph(para(vec![run("x")])))
        .collect();
    let result = UniversalFormatConverter::encode_rich(&doc(blocks), DocumentFormat::Pages, None);
    assert!(
        matches!(result, Err(EasyOfficeError::ResourceBudgetExceeded(_))),
        "{result:?}"
    );
}

#[test]
fn a_flat_model_is_refused_and_a_document_converts() {
    let model = UniversalDocumentModel {
        title: "t".into(),
        paragraphs: vec!["a".into()],
        ..UniversalDocumentModel::default()
    };
    assert!(UniversalFormatConverter::encode(&model, DocumentFormat::Pages).is_err());
    let docx =
        UniversalFormatConverter::encode_rich(&formatted_document(), DocumentFormat::Docx, None)
            .unwrap();
    let pages =
        UniversalFormatConverter::convert(&docx, Some("docx"), DocumentFormat::Pages).unwrap();
    let (back, _) = read_back(&pages);
    assert!(back
        .blocks
        .iter()
        .any(|b| matches!(b, RichBlock::Paragraph(p) if p.plain_text() == "가운데")));
}

#[test]
fn what_cannot_be_written_is_counted_for_the_warning() {
    let table = RichTable::from(vec![vec!["a".to_string()]]);
    let list = RichParagraph {
        list: Some(RichListKind::Bullet),
        ..para(vec![run("항목")])
    };
    let styled = RichParagraph {
        left_indent_twips: Some(720),
        ..para(vec![RichRun {
            strike: true,
            font: Some("Arial".into()),
            ..run("서식")
        }])
    };
    let source = doc(vec![
        RichBlock::Paragraph(para(vec![run("본문")])),
        RichBlock::Table(table.clone()),
        RichBlock::Table(table),
        RichBlock::Paragraph(list),
        RichBlock::Paragraph(styled),
    ]);
    let loss = pages_write_loss(&source);
    assert_eq!(loss.elements, 3, "two tables and a list item");
    assert_eq!(loss.formats, 2, "one run and one paragraph");
    let (back, _) = read_back(&write(&source));
    let texts: Vec<String> = back
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        ["본문", "항목", "서식"],
        "tables are left out, list and styled text stay"
    );
    assert_eq!(pages_write_loss(&formatted_document()).elements, 0);
}

/// QA samples for opening in the app (it cannot be done by the tests):
/// `EASYOFFICE_PAGES_WRITE_SAMPLES=<dir> cargo test -p easyoffice-formats --test pages_write -- --ignored`.
#[test]
#[ignore = "writes sample files for a manual check in the app"]
fn write_qa_samples() {
    let dir = std::env::var("EASYOFFICE_PAGES_WRITE_SAMPLES").expect("set the sample directory");
    let look = |text: &str, f: fn(&mut RichRun)| {
        let mut r = run(text);
        f(&mut r);
        r
    };
    let formats = doc(vec![
        RichBlock::Paragraph(RichParagraph {
            heading_level: 1,
            ..para(vec![run("서식 시험")])
        }),
        RichBlock::Paragraph(para(vec![
            look("굵게 ", |r| r.bold = true),
            look("기울임 ", |r| r.italic = true),
            look("밑줄 ", |r| r.underline = true),
            look("굵은 기울임 밑줄 ", |r| {
                r.bold = true;
                r.italic = true;
                r.underline = true;
            }),
            look("20pt ", |r| r.size_half_pt = Some(40)),
            look("빨강 ", |r| r.color = Some("D32F2F".into())),
            look("파랑 14pt 굵게", |r| {
                r.color = Some("1565C0".into());
                r.size_half_pt = Some(28);
                r.bold = true;
            }),
        ])),
        RichBlock::Paragraph(para(vec![
            run("이모지 😀😀 뒤의 "),
            look("굵은 글자", |r| r.bold = true),
            run(" 그리고 일반 글자."),
        ])),
    ]);
    let long = doc((0..1_200)
        .map(|i| {
            RichBlock::Paragraph(para(vec![
                look(&format!("{}. ", i + 1), |r| r.bold = true),
                run("긴 문서의 문단입니다. 여러 개의 압축 블록으로 나뉘어 저장되는지 확인합니다. "),
                run("The quick brown fox jumps over the lazy dog."),
            ]))
        })
        .collect());
    for (name, d) in [
        ("pages-write-1-basic.pages", formatted_document()),
        ("pages-write-2-formats.pages", formats),
        ("pages-write-3-long.pages", long),
    ] {
        std::fs::write(format!("{dir}/{name}"), write(&d)).unwrap();
    }
}
