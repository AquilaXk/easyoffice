use easyoffice_formats::{
    pages_write_loss, DecodedContent, DocumentFormat, EasyOfficeError, RichBlock, RichDocument,
    RichListKind, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn task_para(text: &str, checked: bool, level: u8, bold: bool) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun {
            text: text.to_string(),
            bold,
            ..RichRun::default()
        }],
        list: Some(RichListKind::Task),
        task_checked: checked,
        list_level: level,
        ..RichParagraph::default()
    })
}

fn sample_task_doc() -> RichDocument {
    RichDocument {
        title: "체크리스트 점검".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                heading_level: 1,
                runs: vec![RichRun::plain("체크리스트 점검")],
                ..RichParagraph::default()
            }),
            task_para("요구사항 정의서 검토", true, 0, false),
            task_para("보안 경계 검증", false, 1, true),
            task_para("악의적 입력 차단 테스트", true, 2, false),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("일반 본문 문단")],
                ..RichParagraph::default()
            }),
            task_para("배포 전 최종 확인", false, 0, false),
        ],
        ..RichDocument::default()
    }
}

fn decode_doc(bytes: &[u8], hint: &str) -> (RichDocument, Vec<String>) {
    let out = UniversalFormatConverter::decode_rich(bytes, Some(hint)).expect("decode_rich");
    let DecodedContent::Document(doc) = out.content else {
        panic!("expected Document content for {hint}");
    };
    (doc, out.warnings)
}

fn assert_task_roundtrip(decoded: &RichDocument, max_level: u8) {
    let paras: Vec<&RichParagraph> = decoded
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(paras.len(), 6);
    assert_eq!(paras[0].heading_level, 1);
    assert_eq!(paras[0].list, None);

    assert_eq!(paras[1].list, Some(RichListKind::Task));
    assert!(paras[1].task_checked);
    assert_eq!(paras[1].list_level, 0);
    assert_eq!(paras[1].plain_text(), "요구사항 정의서 검토");

    assert_eq!(paras[2].list, Some(RichListKind::Task));
    assert!(!paras[2].task_checked);
    assert_eq!(paras[2].list_level, 1.min(max_level));
    assert_eq!(paras[2].plain_text(), "보안 경계 검증");
    assert!(paras[2].runs.iter().any(|r| r.bold));

    assert_eq!(paras[3].list, Some(RichListKind::Task));
    assert!(paras[3].task_checked);
    assert_eq!(paras[3].list_level, 2.min(max_level));
    assert_eq!(paras[3].plain_text(), "악의적 입력 차단 테스트");

    assert_eq!(paras[4].list, None);
    assert!(!paras[4].task_checked);
    assert_eq!(paras[4].plain_text(), "일반 본문 문단");

    assert_eq!(paras[5].list, Some(RichListKind::Task));
    assert!(!paras[5].task_checked);
    assert_eq!(paras[5].list_level, 0);
    assert_eq!(paras[5].plain_text(), "배포 전 최종 확인");
}

#[test]
fn docx_task_list_roundtrips_with_w14_checkbox_sdt_and_nesting() {
    let doc = sample_task_doc();
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None)
        .expect("encode docx");

    let entries =
        ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).expect("unpack docx");
    let doc_xml = entries
        .iter()
        .find(|e| e.path == "word/document.xml")
        .map(|e| std::str::from_utf8(&e.data).expect("utf8"))
        .expect("document.xml");
    assert!(
        doc_xml.contains("w14:checkbox"),
        "expected w14:checkbox SDT"
    );
    assert!(doc_xml.contains("w14:checked w14:val=\"1\""));
    assert!(doc_xml.contains("w14:checked w14:val=\"0\""));

    let (decoded, read_warnings) = decode_doc(&bytes, "checklist.docx");
    assert!(read_warnings.is_empty(), "read warnings: {read_warnings:?}");
    assert_task_roundtrip(&decoded, 8);
}

#[test]
fn odt_hwpx_markdown_and_rtf_roundtrip_task_lists() {
    let doc = sample_task_doc();
    for (fmt, hint, max_lvl) in [
        (DocumentFormat::Odt, "checklist.odt", 8u8),
        (DocumentFormat::Hwpx, "checklist.hwpx", 6u8),
        (DocumentFormat::Markdown, "checklist.md", 8u8),
        (DocumentFormat::Rtf, "checklist.rtf", 0u8),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None)
            .unwrap_or_else(|e| panic!("encode {fmt:?}: {e}"));
        if fmt == DocumentFormat::Markdown {
            let md = std::str::from_utf8(&bytes).expect("md utf8");
            assert!(md.contains("- [x] 요구사항 정의서 검토"), "md:\n{md}");
            assert!(md.contains("  - [ ] **보안 경계 검증**"), "md:\n{md}");
            assert!(
                md.contains("    - [x] 악의적 입력 차단 테스트"),
                "md:\n{md}"
            );
        }
        if fmt == DocumentFormat::Hwpx {
            let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
                .expect("unpack hwpx");
            let header_xml = entries
                .iter()
                .find(|e| e.path == "Contents/header.xml")
                .map(|e| std::str::from_utf8(&e.data).expect("utf8"))
                .expect("header.xml");
            assert!(
                header_xml.contains("checkable=\"1\""),
                "header:\n{header_xml}"
            );
            assert!(
                header_xml.contains("checked=\"1\""),
                "header:\n{header_xml}"
            );
        }
        let (decoded, read_warnings) = decode_doc(&bytes, hint);
        if fmt != DocumentFormat::Rtf {
            assert!(
                read_warnings.is_empty(),
                "{fmt:?} read warnings: {read_warnings:?}"
            );
        }
        assert_task_roundtrip(&decoded, max_lvl);
    }
}

#[test]
fn plain_text_pdf_and_pages_handle_task_lists_honestly() {
    let doc = sample_task_doc();
    let txt_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Txt, None).expect("encode txt");
    let txt = std::str::from_utf8(&txt_bytes).expect("txt utf8");
    assert!(txt.contains("☑ 요구사항 정의서 검토"), "txt:\n{txt}");
    assert!(txt.contains("☐ 보안 경계 검증"), "txt:\n{txt}");

    let pages_warnings = pages_write_loss(&doc).warnings();
    assert!(
        pages_warnings.iter().any(|w| w.contains("빠진 서식")),
        "expected honest Pages format loss warning, got {pages_warnings:?}"
    );
    let pages_bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None)
        .expect("encode pages");
    let (pages_decoded, _) = decode_doc(&pages_bytes, "checklist.pages");
    assert_eq!(pages_decoded.blocks.len(), 6);
}

#[test]
fn hostile_deeply_nested_docx_sdt_fails_closed() {
    let doc = sample_task_doc();
    let base_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).expect("base docx");
    let mut entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&base_bytes, &ZipSecurityPolicy::default())
            .expect("unpack")
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect();
    let mut deep_sdt = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p>",
    );
    for _ in 0..20 {
        deep_sdt.push_str("<w:sdt><w:sdtContent>");
    }
    deep_sdt.push_str("<w:r><w:t>x</w:t></w:r>");
    for _ in 0..20 {
        deep_sdt.push_str("</w:sdtContent></w:sdt>");
    }
    deep_sdt.push_str("</w:p></w:body></w:document>");
    for (path, data) in &mut entries {
        if path == "word/document.xml" {
            *data = deep_sdt.into_bytes();
            break;
        }
    }
    let hostile = ZipArchiveCodec::pack_compressed(&entries).expect("pack hostile");
    let err = UniversalFormatConverter::decode_rich(&hostile, Some("hostile.docx"))
        .expect_err("deeply nested w:sdt must fail closed");
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "expected ResourceBudgetExceeded, got {err:?}"
    );
}

#[test]
#[ignore]
fn task_list_multi_format_roundtrip_benchmark() {
    let mut doc = RichDocument {
        title: "체크리스트 벤치마크".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            heading_level: 1,
            runs: vec![RichRun::plain("체크리스트 벤치마크")],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    for i in 0..40 {
        doc.blocks.push(task_para(
            &format!("점검 항목 #{i} — 다중 포맷 무손실 왕복 검증"),
            i % 2 == 0,
            (i % 4) as u8,
            i % 3 == 0,
        ));
    }
    let rounds = 30usize;
    let start = std::time::Instant::now();
    for _ in 0..rounds {
        for (fmt, hint) in [
            (DocumentFormat::Docx, "bench.docx"),
            (DocumentFormat::Odt, "bench.odt"),
            (DocumentFormat::Hwpx, "bench.hwpx"),
            (DocumentFormat::Markdown, "bench.md"),
        ] {
            let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).expect("encode");
            let (decoded, _) = decode_doc(&bytes, hint);
            assert_eq!(decoded.blocks.len(), 41);
        }
    }
    let elapsed_us = start.elapsed().as_secs_f64() * 1_000_000.0 / rounds as f64;
    println!("PD-250 40-item 4-format (DOCX+ODT+HWPX+MD) roundtrip avg: {elapsed_us:.2} us");
}
