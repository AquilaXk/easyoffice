use std::time::Instant;

use easyoffice_formats::{
    pages_write_loss, DecodedContent, DocumentFormat, EasyOfficeError, PdfSerializer, RichBlock,
    RichComment, RichDocument, RichNote, RichNoteKind, RichParagraph, RichRevision,
    RichRevisionKind, RichRun, RichScript, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy, MAX_ACTIVE_COMMENT_RANGES, MAX_COMMENTS_PER_DOCUMENT,
    MAX_NOTES_PER_DOCUMENT, MAX_NOTE_TEXT_BYTES, MAX_REVISIONS_PER_DOCUMENT,
};

fn decode_doc(bytes: &[u8], hint: &str) -> (RichDocument, Vec<String>) {
    let out = UniversalFormatConverter::decode_rich(bytes, Some(hint)).expect("decode_rich");
    let DecodedContent::Document(doc) = out.content else {
        panic!("expected document for {hint}");
    };
    (doc, out.warnings)
}

fn entry_utf8(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("missing ZIP entry {path}"));
    String::from_utf8(entry.data.clone()).expect("utf8")
}

fn sample_annotated_doc() -> RichDocument {
    let rev_ins = RichRevision {
        id: 1,
        kind: RichRevisionKind::Insert,
        author: "이작성".to_string(),
        date: "2026-11-19T09:00:00Z".to_string(),
    };
    let rev_del = RichRevision {
        id: 2,
        kind: RichRevisionKind::Delete,
        author: "이작성".to_string(),
        date: "2026-11-19T09:05:00Z".to_string(),
    };
    RichDocument {
        title: "주석·추적 검증".to_string(),
        track_changes: true,
        notes: vec![
            RichNote {
                id: 1,
                kind: RichNoteKind::Footnote,
                text: "첫 번째 각주\n둘째 줄".to_string(),
            },
            RichNote {
                id: 1,
                kind: RichNoteKind::Endnote,
                text: "첫 번째 미주 참고문헌".to_string(),
            },
        ],
        comments: vec![
            RichComment {
                id: 1,
                author: "김검토".to_string(),
                date: "2026-11-19T09:30:00Z".to_string(),
                text: "표현을 다듬으세요.".to_string(),
            },
            RichComment {
                id: 2,
                author: "박수정".to_string(),
                date: "2026-11-19T10:00:00Z".to_string(),
                text: "포인트 메모".to_string(),
            },
        ],
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![
                    RichRun::plain("본문 "),
                    RichRun {
                        text: "삽입된 내용".to_string(),
                        bold: true,
                        revision: Some(rev_ins),
                        ..RichRun::default()
                    },
                    RichRun::plain(" 그리고 "),
                    RichRun {
                        text: "삭제된 내용".to_string(),
                        revision: Some(rev_del),
                        ..RichRun::default()
                    },
                    RichRun {
                        script: RichScript::Superscript,
                        note_ref: Some((RichNoteKind::Footnote, 1)),
                        ..RichRun::default()
                    },
                    RichRun::plain(" 끝."),
                ],
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![
                    RichRun {
                        text: "범위 메모 대상".to_string(),
                        comment_id: Some(1),
                        ..RichRun::default()
                    },
                    RichRun::plain(" 이후 "),
                    RichRun {
                        comment_id: Some(2),
                        ..RichRun::default()
                    },
                    RichRun::plain("미주 앞"),
                    RichRun {
                        script: RichScript::Superscript,
                        note_ref: Some((RichNoteKind::Endnote, 1)),
                        ..RichRun::default()
                    },
                ],
                ..RichParagraph::default()
            }),
        ],
        ..RichDocument::default()
    }
}

fn assert_annotated_roundtrip(decoded: &RichDocument, fmt_label: &str) {
    assert!(decoded.track_changes, "{fmt_label}: track_changes");
    assert_eq!(decoded.notes.len(), 2, "{fmt_label}: notes");
    assert_eq!(
        decoded.notes[0],
        RichNote {
            id: 1,
            kind: RichNoteKind::Footnote,
            text: "첫 번째 각주\n둘째 줄".to_string(),
        },
        "{fmt_label}: footnote"
    );
    assert_eq!(
        decoded.notes[1],
        RichNote {
            id: 1,
            kind: RichNoteKind::Endnote,
            text: "첫 번째 미주 참고문헌".to_string(),
        },
        "{fmt_label}: endnote"
    );
    assert_eq!(decoded.comments.len(), 2, "{fmt_label}: comments");
    assert_eq!(decoded.comments[0].author, "김검토");
    assert_eq!(decoded.comments[0].text, "표현을 다듬으세요.");
    assert_eq!(decoded.comments[1].author, "박수정");
    assert_eq!(decoded.comments[1].text, "포인트 메모");

    assert_eq!(decoded.blocks.len(), 2, "{fmt_label}: block count");
    let RichBlock::Paragraph(p0) = &decoded.blocks[0] else {
        panic!("p0");
    };
    assert_eq!(p0.plain_text(), "본문 삽입된 내용 그리고  끝.");
    let ins_run = p0
        .runs
        .iter()
        .find(|r| r.text == "삽입된 내용")
        .expect("insert run");
    assert!(ins_run.bold);
    assert_eq!(
        ins_run
            .revision
            .as_ref()
            .map(|r| (r.kind, r.author.as_str())),
        Some((RichRevisionKind::Insert, "이작성"))
    );
    let del_run = p0
        .runs
        .iter()
        .find(|r| r.text == "삭제된 내용")
        .expect("delete run");
    assert_eq!(
        del_run
            .revision
            .as_ref()
            .map(|r| (r.kind, r.author.as_str())),
        Some((RichRevisionKind::Delete, "이작성"))
    );
    assert!(
        p0.runs
            .iter()
            .any(|r| r.note_ref == Some((RichNoteKind::Footnote, 1))),
        "{fmt_label}: footnote ref missing in {:?}",
        p0.runs
    );

    let RichBlock::Paragraph(p1) = &decoded.blocks[1] else {
        panic!("p1");
    };
    assert_eq!(p1.plain_text(), "범위 메모 대상 이후 미주 앞");
    assert!(p1
        .runs
        .iter()
        .any(|r| r.text == "범위 메모 대상" && r.comment_id == Some(1)));
    assert!(p1
        .runs
        .iter()
        .any(|r| r.text.is_empty() && r.comment_id == Some(2)));
    assert!(p1
        .runs
        .iter()
        .any(|r| r.note_ref == Some((RichNoteKind::Endnote, 1))));
}

#[test]
fn docx_and_odt_roundtrip_footnotes_endnotes_comments_and_revisions_losslessly() {
    let doc = sample_annotated_doc();
    let RichBlock::Paragraph(src_p0) = &doc.blocks[0] else {
        unreachable!();
    };
    assert_eq!(src_p0.plain_text(), "본문 삽입된 내용 그리고  끝.");

    // DOCX roundtrip (+ second pass ensuring no separator spaces accumulate)
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None)
        .expect("encode docx");
    let fn_xml = entry_utf8(&docx, "word/footnotes.xml");
    assert!(fn_xml.contains("<w:footnoteRef/>"), "{fn_xml}");
    assert!(fn_xml.contains("<w:br/>"), "{fn_xml}");
    let en_xml = entry_utf8(&docx, "word/endnotes.xml");
    assert!(en_xml.contains("<w:endnoteRef/>"), "{en_xml}");
    let cmt_xml = entry_utf8(&docx, "word/comments.xml");
    assert!(cmt_xml.contains("김검토"), "{cmt_xml}");
    let set_xml = entry_utf8(&docx, "word/settings.xml");
    assert!(set_xml.contains("<w:trackRevisions/>"), "{set_xml}");
    let doc_xml = entry_utf8(&docx, "word/document.xml");
    assert!(doc_xml.contains("<w:ins "), "{doc_xml}");
    assert!(doc_xml.contains("<w:del "), "{doc_xml}");
    assert!(doc_xml.contains("<w:delText "), "{doc_xml}");
    assert!(doc_xml.contains("<w:commentRangeStart "), "{doc_xml}");
    assert!(doc_xml.contains("<w:commentReference "), "{doc_xml}");

    let (docx_dec, docx_warn) = decode_doc(&docx, "notes.docx");
    assert!(docx_warn.is_empty(), "docx warnings: {docx_warn:?}");
    assert_annotated_roundtrip(&docx_dec, "docx-1");

    let docx2 = UniversalFormatConverter::encode_rich(&docx_dec, DocumentFormat::Docx, None)
        .expect("encode docx 2");
    let (docx_dec2, _) = decode_doc(&docx2, "notes2.docx");
    assert_annotated_roundtrip(&docx_dec2, "docx-2");

    // ODT roundtrip
    let odt =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).expect("encode odt");
    let content_xml = entry_utf8(&odt, "content.xml");
    assert!(content_xml.contains("xmlns:dc="), "{content_xml}");
    assert!(
        content_xml.contains("<text:tracked-changes text:track-changes=\"true\">"),
        "{content_xml}"
    );
    assert!(content_xml.contains("<text:note "), "{content_xml}");
    assert!(content_xml.contains("<office:annotation "), "{content_xml}");
    assert!(
        content_xml.contains("<office:annotation-end "),
        "{content_xml}"
    );

    let (odt_dec, odt_warn) = decode_doc(&odt, "notes.odt");
    assert!(odt_warn.is_empty(), "odt warnings: {odt_warn:?}");
    assert_annotated_roundtrip(&odt_dec, "odt-1");
}

#[test]
fn non_docx_odt_writers_skip_deleted_runs_and_render_notes_cleanly() {
    let doc = sample_annotated_doc();

    // Markdown
    let md_bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Markdown, None)
        .expect("encode md");
    let md = String::from_utf8(md_bytes).expect("md utf8");
    assert!(
        !md.contains("삭제된 내용"),
        "deleted text leaked to md:\n{md}"
    );
    assert!(md.contains("삽입된 내용"), "md:\n{md}");
    assert!(md.contains("[^fn1]"), "md:\n{md}");
    assert!(md.contains("[^en1]"), "md:\n{md}");
    assert!(md.contains("[^fn1]: 첫 번째 각주"), "md:\n{md}");
    assert!(md.contains("[^en1]: 첫 번째 미주 참고문헌"), "md:\n{md}");

    // TXT
    let txt_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Txt, None).expect("encode txt");
    let txt = String::from_utf8(txt_bytes).expect("txt utf8");
    assert!(
        !txt.contains("삭제된 내용"),
        "deleted text leaked to txt:\n{txt}"
    );
    assert!(txt.contains("[1]"), "txt:\n{txt}");
    assert!(txt.contains("[2]"), "txt:\n{txt}");
    assert!(txt.contains("첫 번째 미주 참고문헌"), "txt:\n{txt}");

    // PDF
    let pdf_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pdf, None).expect("encode pdf");
    let parsed_pdf = PdfSerializer::parse(&pdf_bytes).expect("parse pdf");
    let pdf_text: String = parsed_pdf
        .pages
        .iter()
        .map(|p| p.body_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !pdf_text.contains("삭제된 내용"),
        "deleted text leaked to pdf:\n{pdf_text}"
    );
    assert!(pdf_text.contains("[1]"), "pdf:\n{pdf_text}");
    assert!(
        pdf_text.contains("첫 번째 미주 참고문헌"),
        "pdf:\n{pdf_text}"
    );

    // HWPX
    let hwpx_bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None)
        .expect("encode hwpx");
    let sec0 = entry_utf8(&hwpx_bytes, "Contents/section0.xml");
    assert!(
        !sec0.contains("삭제된 내용"),
        "deleted text leaked to hwpx:\n{sec0}"
    );
    assert!(sec0.contains("<hp:footNote>"), "hwpx:\n{sec0}");
    assert!(sec0.contains("<hp:endNote>"), "hwpx:\n{sec0}");
    let (hwpx_dec, _) = decode_doc(&hwpx_bytes, "notes.hwpx");
    assert!(
        hwpx_dec.blocks.iter().any(
            |b| matches!(b, RichBlock::Paragraph(p) if p.plain_text().contains("첫 번째 각주"))
        ),
        "hwpx appendix missing note"
    );

    // RTF
    let rtf_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).expect("encode rtf");
    let (rtf_dec, _) = decode_doc(&rtf_bytes, "notes.rtf");
    let rtf_joined: String = rtf_dec
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rtf_joined.contains("삭제된 내용"), "rtf:\n{rtf_joined}");
    assert!(rtf_joined.contains("[1]"), "rtf:\n{rtf_joined}");
    assert!(
        rtf_joined.contains("첫 번째 미주 참고문헌"),
        "rtf:\n{rtf_joined}"
    );

    // Pages
    let loss = pages_write_loss(&doc);
    assert!(loss.elements > 0 && loss.formats > 0, "loss: {loss:?}");
    let pages_bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None)
        .expect("encode pages");
    let (pages_dec, _) = decode_doc(&pages_bytes, "notes.pages");
    let pages_joined: String = pages_dec
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !pages_joined.contains("삭제된 내용"),
        "pages:\n{pages_joined}"
    );
    assert!(pages_joined.contains("[1]"), "pages:\n{pages_joined}");
}

#[test]
fn validation_helpers_deduplicate_and_coalescing_never_merges_distinct_notes_comments_revisions() {
    let doc = RichDocument {
        notes: vec![
            RichNote {
                id: 1,
                kind: RichNoteKind::Footnote,
                text: "첫 값".to_string(),
            },
            RichNote {
                id: 1,
                kind: RichNoteKind::Footnote,
                text: "중복 무시".to_string(),
            },
            RichNote {
                id: 1,
                kind: RichNoteKind::Endnote,
                text: "긴 미주 ".to_string() + &"가".repeat(MAX_NOTE_TEXT_BYTES),
            },
        ],
        comments: vec![
            RichComment {
                id: 5,
                author: "작성자".repeat(100),
                date: "2026-11-19".repeat(20),
                text: "유지".to_string(),
            },
            RichComment {
                id: 5,
                author: "중복".to_string(),
                date: String::new(),
                text: "무시".to_string(),
            },
        ],
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![
                RichRun {
                    script: RichScript::Superscript,
                    note_ref: Some((RichNoteKind::Footnote, 1)),
                    ..RichRun::default()
                },
                RichRun {
                    script: RichScript::Superscript,
                    note_ref: Some((RichNoteKind::Endnote, 1)),
                    ..RichRun::default()
                },
                RichRun {
                    text: "A".to_string(),
                    comment_id: Some(5),
                    ..RichRun::default()
                },
                RichRun {
                    text: "B".to_string(),
                    comment_id: None,
                    ..RichRun::default()
                },
            ],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };

    let notes = doc.validated_notes();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].text, "첫 값");
    assert!(notes[1].text.len() <= MAX_NOTE_TEXT_BYTES);

    let comments = doc.validated_comments();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text, "유지");
    assert!(comments[0].author.len() <= easyoffice_formats::MAX_AUTHOR_BYTES);
    assert!(comments[0].date.len() <= easyoffice_formats::MAX_DATE_BYTES);

    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let (back, _) = decode_doc(&docx, "coalesce.docx");
    let RichBlock::Paragraph(p) = &back.blocks[0] else {
        panic!();
    };
    let note_runs: Vec<_> = p.runs.iter().filter(|r| r.note_ref.is_some()).collect();
    assert_eq!(note_runs.len(), 2, "note_ref runs must never merge");
    assert!(p
        .runs
        .iter()
        .any(|r| r.text == "A" && r.comment_id == Some(5)));
    assert!(p
        .runs
        .iter()
        .any(|r| r.text == "B" && r.comment_id.is_none()));
}

fn replace_zip_entry(base_zip: &[u8], path: &str, new_xml: String) -> Vec<u8> {
    let mut entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(base_zip, &ZipSecurityPolicy::default())
            .expect("unpack")
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect();
    if let Some((_, data)) = entries.iter_mut().find(|(p, _)| p == path) {
        *data = new_xml.into_bytes();
    } else {
        entries.push((path.to_string(), new_xml.into_bytes()));
    }
    ZipArchiveCodec::pack_compressed(&entries).expect("pack")
}

#[test]
fn hostile_docx_and_odt_bounds_fail_closed() {
    let base_doc = sample_annotated_doc();
    let base_docx =
        UniversalFormatConverter::encode_rich(&base_doc, DocumentFormat::Docx, None).unwrap();
    let base_odt =
        UniversalFormatConverter::encode_rich(&base_doc, DocumentFormat::Odt, None).unwrap();

    // 1. DOCX > MAX_NOTES_PER_DOCUMENT
    let mut many_fn = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:footnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    );
    for id in 1..=(MAX_NOTES_PER_DOCUMENT + 1) {
        many_fn.push_str(&format!(
            "<w:footnote w:id=\"{id}\"><w:p><w:r><w:t>n</w:t></w:r></w:p></w:footnote>"
        ));
    }
    many_fn.push_str("</w:footnotes>");
    let hostile = replace_zip_entry(&base_docx, "word/footnotes.xml", many_fn);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&hostile, Some("fn.docx")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. DOCX > MAX_COMMENTS_PER_DOCUMENT
    let mut many_cmt = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:comments xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    );
    for id in 1..=(MAX_COMMENTS_PER_DOCUMENT + 1) {
        many_cmt.push_str(&format!(
            "<w:comment w:id=\"{id}\" w:author=\"a\" w:date=\"2026\"><w:p><w:r><w:t>c</w:t></w:r></w:p></w:comment>"
        ));
    }
    many_cmt.push_str("</w:comments>");
    let hostile = replace_zip_entry(&base_docx, "word/comments.xml", many_cmt);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&hostile, Some("cmt.docx")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 3. DOCX > MAX_REVISIONS_PER_DOCUMENT
    let mut many_ins = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p>",
    );
    for id in 1..=(MAX_REVISIONS_PER_DOCUMENT + 1) {
        many_ins.push_str(&format!(
            "<w:ins w:id=\"{id}\" w:author=\"a\" w:date=\"2026\"><w:r><w:t>x</w:t></w:r></w:ins>"
        ));
    }
    many_ins.push_str("</w:p></w:body></w:document>");
    let hostile = replace_zip_entry(&base_docx, "word/document.xml", many_ins);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&hostile, Some("rev.docx")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 4. DOCX > MAX_ACTIVE_COMMENT_RANGES
    let mut deep_cmt = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p>",
    );
    for id in 1..=(MAX_ACTIVE_COMMENT_RANGES + 1) {
        deep_cmt.push_str(&format!("<w:commentRangeStart w:id=\"{id}\"/>"));
    }
    deep_cmt.push_str("<w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>");
    let hostile = replace_zip_entry(&base_docx, "word/document.xml", deep_cmt);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&hostile, Some("depth.docx")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 5. ODT > MAX_NOTES_PER_DOCUMENT
    let mut odt_notes = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"><office:body><office:text><text:p>",
    );
    for id in 1..=(MAX_NOTES_PER_DOCUMENT + 1) {
        odt_notes.push_str(&format!(
            "<text:note text:id=\"ftn{id}\" text:note-class=\"footnote\"><text:note-citation>{id}</text:note-citation><text:note-body><text:p>x</text:p></text:note-body></text:note>"
        ));
    }
    odt_notes.push_str("</text:p></office:text></office:body></office:document-content>");
    let hostile = replace_zip_entry(&base_odt, "content.xml", odt_notes);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&hostile, Some("fn.odt")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 6. ODT > MAX_REVISIONS_PER_DOCUMENT
    let mut odt_revs = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><office:body><office:text><text:tracked-changes text:track-changes=\"true\">",
    );
    for id in 1..=(MAX_REVISIONS_PER_DOCUMENT + 1) {
        odt_revs.push_str(&format!(
            "<text:changed-region text:id=\"ct{id}\"><text:insertion><office:change-info><dc:creator>a</dc:creator><dc:date>2026</dc:date></office:change-info></text:insertion></text:changed-region>"
        ));
    }
    odt_revs.push_str("</text:tracked-changes><text:p>ok</text:p></office:text></office:body></office:document-content>");
    let hostile = replace_zip_entry(&base_odt, "content.xml", odt_revs);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&hostile, Some("rev.odt")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 7. ODT > MAX_ACTIVE_COMMENT_RANGES
    let mut odt_deep_cmt = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><office:body><office:text><text:p>",
    );
    for id in 1..=(MAX_ACTIVE_COMMENT_RANGES + 1) {
        odt_deep_cmt.push_str(&format!(
            "<office:annotation office:name=\"cmt{id}\"><dc:creator>a</dc:creator><dc:date>2026</dc:date><text:p>c</text:p></office:annotation>"
        ));
    }
    odt_deep_cmt.push_str("body");
    for id in 1..=(MAX_ACTIVE_COMMENT_RANGES + 1) {
        odt_deep_cmt.push_str(&format!("<office:annotation-end office:name=\"cmt{id}\"/>"));
    }
    odt_deep_cmt.push_str("</text:p></office:text></office:body></office:document-content>");
    let hostile = replace_zip_entry(&base_odt, "content.xml", odt_deep_cmt);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&hostile, Some("depth.odt")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
#[ignore]
fn docx_and_odt_notes_track_roundtrip_benchmark() {
    let mut doc = sample_annotated_doc();
    for i in 2..=30u32 {
        doc.notes.push(RichNote {
            id: i,
            kind: RichNoteKind::Footnote,
            text: format!("각주 본문 #{i} — 다중 라인\n보충 설명"),
        });
        doc.comments.push(RichComment {
            id: i + 10,
            author: "검토자".to_string(),
            date: "2026-11-19T11:00:00Z".to_string(),
            text: format!("검토 메모 #{i}"),
        });
        doc.blocks.push(RichBlock::Paragraph(RichParagraph {
            runs: vec![
                RichRun {
                    text: format!("단락 {i} "),
                    comment_id: Some(i + 10),
                    ..RichRun::default()
                },
                RichRun {
                    text: "추가".to_string(),
                    revision: Some(RichRevision {
                        id: i * 2,
                        kind: RichRevisionKind::Insert,
                        author: "작성자".to_string(),
                        date: "2026-11-19T11:00:00Z".to_string(),
                    }),
                    ..RichRun::default()
                },
                RichRun {
                    text: "삭제".to_string(),
                    revision: Some(RichRevision {
                        id: i * 2 + 1,
                        kind: RichRevisionKind::Delete,
                        author: "작성자".to_string(),
                        date: "2026-11-19T11:01:00Z".to_string(),
                    }),
                    ..RichRun::default()
                },
                RichRun {
                    script: RichScript::Superscript,
                    note_ref: Some((RichNoteKind::Footnote, i)),
                    ..RichRun::default()
                },
            ],
            ..RichParagraph::default()
        }));
    }

    let rounds = 40usize;
    let start = Instant::now();
    for _ in 0..rounds {
        let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
        let (d_docx, _) = decode_doc(&docx, "bench.docx");
        assert_eq!(d_docx.notes.len(), doc.notes.len());

        let odt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).unwrap();
        let (d_odt, _) = decode_doc(&odt, "bench.odt");
        assert_eq!(d_odt.notes.len(), doc.notes.len());
    }
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let per_round_ms = elapsed_ms / rounds as f64;
    println!(
        "docx_and_odt_notes_track_roundtrip_benchmark: rounds={rounds}, total_ms={elapsed_ms:.2}, per_round_ms={per_round_ms:.3}"
    );
}
