//! PD-328: Korean font priority (`w:eastAsia` / `style:font-name-asian`), line-spacing
//! rounding + explicit `100%` + fixed/minimum conversion, `CT_Settings` element order,
//! landscape `<w:tblGrid>` widths, table-cell paragraph resets across DOCX/ODT/RTF, and `.doc`
//! save-loss warnings.

use easyoffice_formats::{
    doc_save_loss, DecodedContent, DocumentFormat, RichBinding, RichBindingKind, RichBlock,
    RichComment, RichDocument, RichPageFrame, RichPageMargins, RichPaper, RichParagraph,
    RichRevision, RichRevisionKind, RichRun, RichTable, RtfCodec, TableCellLink,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn unpack(bytes: &[u8]) -> Vec<(String, String)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack zip")
        .into_iter()
        .map(|entry| {
            (
                entry.path,
                String::from_utf8_lossy(&entry.data).into_owned(),
            )
        })
        .collect()
}

fn part<'a>(parts: &'a [(String, String)], path: &str) -> &'a str {
    parts
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, s)| s.as_str())
        .unwrap_or_else(|| panic!("missing part {path}"))
}

fn repack_with(bytes: &[u8], replacements: &[(&str, String)]) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let updated: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|entry| {
            if let Some((_, next)) = replacements.iter().find(|(p, _)| *p == entry.path) {
                (entry.path, next.as_bytes().to_vec())
            } else {
                (entry.path, entry.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&updated).expect("repack")
}

fn decode_doc(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode_rich")
        .content
    {
        DecodedContent::Document(doc) => doc,
        other => panic!("expected document for {name}, got {other:?}"),
    }
}

fn minimal_docx(document_xml: &str) -> Vec<u8> {
    let base_doc = RichDocument {
        title: "테스트".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("placeholder")],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    let bytes = UniversalFormatConverter::encode_rich(&base_doc, DocumentFormat::Docx, None)
        .expect("encode base docx");
    repack_with(&bytes, &[("word/document.xml", document_xml.to_string())])
}

fn minimal_odt(content_xml: &str) -> Vec<u8> {
    let base_doc = RichDocument {
        title: "테스트".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("placeholder")],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    let bytes = UniversalFormatConverter::encode_rich(&base_doc, DocumentFormat::Odt, None)
        .expect("encode base odt");
    repack_with(&bytes, &[("content.xml", content_xml.to_string())])
}

#[test]
fn docx_reads_east_asia_font_before_ascii_and_keeps_default_runs_none() {
    let xml = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
        r#"<w:p>"#,
        r#"<w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="맑은 고딕" w:cs="Arial"/></w:rPr><w:t>동아시아</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:rFonts w:ascii="Georgia" w:hAnsi="Georgia"/></w:rPr><w:t>영문</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:rFonts w:cs="Tahoma"/></w:rPr><w:t>복합</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:rFonts w:eastAsia="" w:ascii="Arial"/></w:rPr><w:t>빈동아시아</w:t></w:r>"#,
        r#"<w:r><w:t>기본</w:t></w:r>"#,
        r#"</w:p><w:sectPr/></w:body></w:document>"#,
    );
    let doc = decode_doc(&minimal_docx(xml), "fonts.docx");
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    let fonts: Vec<Option<&str>> = p.runs.iter().map(|r| r.font.as_deref()).collect();
    assert_eq!(
        fonts,
        vec![
            Some("맑은 고딕"),
            Some("Georgia"),
            Some("Tahoma"),
            Some("Arial"),
            None,
        ]
    );
}

#[test]
fn odt_reads_asian_font_and_size_before_western_and_keeps_default_none() {
    let content = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<office:document-content "#,
        r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
        r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
        r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
        r#"xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" "#,
        r#"office:version="1.2">"#,
        r#"<office:automatic-styles>"#,
        r#"<style:style style:name="T1" style:family="text"><style:text-properties style:font-name="Liberation Serif" style:font-name-asian="함초롬바탕" fo:font-size="11pt" style:font-size-asian="14pt"/></style:style>"#,
        r#"<style:style style:name="T2" style:family="text"><style:text-properties style:font-family-asian="'바탕'" fo:font-family="Arial" fo:font-size="12pt"/></style:style>"#,
        r#"<style:style style:name="T3" style:family="text"><style:text-properties style:font-name="Consolas" fo:font-family="monospace"/></style:style>"#,
        r#"</office:automatic-styles>"#,
        r#"<office:body><office:text>"#,
        r#"<text:p><text:span text:style-name="T1">한글</text:span><text:span text:style-name="T2">패밀리</text:span><text:span text:style-name="T3">영문</text:span></text:p>"#,
        r#"<text:p>미지정</text:p>"#,
        r#"</office:text></office:body></office:document-content>"#,
    );
    let doc = decode_doc(&minimal_odt(content), "asian.odt");
    let RichBlock::Paragraph(p0) = &doc.blocks[0] else {
        panic!("expected paragraph 0");
    };
    assert_eq!(p0.runs[0].font.as_deref(), Some("함초롬바탕"));
    assert_eq!(p0.runs[0].size_half_pt, Some(28));
    assert_eq!(p0.runs[1].font.as_deref(), Some("바탕"));
    assert_eq!(p0.runs[1].size_half_pt, Some(24));
    assert_eq!(p0.runs[2].font.as_deref(), Some("Consolas"));

    let RichBlock::Paragraph(p1) = &doc.blocks[1] else {
        panic!("expected paragraph 1");
    };
    assert_eq!(p1.runs[0].font, None);
    assert_eq!(p1.runs[0].size_half_pt, None);
}

#[test]
fn docx_and_odt_line_spacing_rounding_explicit_100_and_fixed_lengths() {
    let docx_xml = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
        r#"<w:p><w:pPr><w:spacing w:line="259" w:lineRule="auto"/></w:pPr><w:r><w:t>108</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:t>100</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="360" w:lineRule="exact"/></w:pPr><w:r><w:t>exact150</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="60" w:lineRule="atLeast"/></w:pPr><w:r><w:t>clamp50</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="4800" w:lineRule="exact"/></w:pPr><w:r><w:t>clamp1000</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="9223372036854775807" w:lineRule="auto"/></w:pPr><w:r><w:t>huge_auto</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="2402" w:lineRule="auto"/></w:pPr><w:r><w:t>over_auto</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="-240" w:lineRule="auto"/></w:pPr><w:r><w:t>neg_auto</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="9223372036854775807" w:lineRule="exact"/></w:pPr><w:r><w:t>huge_exact</w:t></w:r></w:p>"#,
        r#"<w:sectPr/></w:body></w:document>"#,
    );
    let docx_doc = decode_doc(&minimal_docx(docx_xml), "spacing.docx");
    let docx_spacings: Vec<Option<u16>> = docx_doc
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => p.line_spacing_percent,
            _ => panic!("paragraph expected"),
        })
        .collect();
    assert_eq!(
        docx_spacings,
        vec![
            Some(108),
            Some(100),
            Some(150),
            Some(50),
            Some(1000),
            None,
            None,
            None,
            Some(1000),
        ]
    );

    let rt_doc = RichDocument {
        title: "줄간격".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("108")],
                line_spacing_percent: Some(108),
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("100")],
                line_spacing_percent: Some(100),
                ..RichParagraph::default()
            }),
        ],
        ..RichDocument::default()
    };
    let docx_bytes =
        UniversalFormatConverter::encode_rich(&rt_doc, DocumentFormat::Docx, None).expect("docx");
    let docx_parts = unpack(&docx_bytes);
    let doc_xml = part(&docx_parts, "word/document.xml");
    assert!(doc_xml.contains("w:line=\"259\""), "{doc_xml}");
    assert!(doc_xml.contains("w:line=\"240\""), "{doc_xml}");
    let docx_back = decode_doc(&docx_bytes, "rt.docx");
    assert_eq!(docx_back.blocks, rt_doc.blocks);

    let odt_content = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<office:document-content "#,
        r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
        r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
        r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
        r#"xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" "#,
        r#"office:version="1.2">"#,
        r#"<office:automatic-styles>"#,
        r#"<style:style style:name="P1" style:family="paragraph"><style:paragraph-properties fo:line-height="100%"/></style:style>"#,
        r#"<style:style style:name="P2" style:family="paragraph"><style:paragraph-properties fo:line-height="108%"/></style:style>"#,
        r#"<style:style style:name="P3" style:family="paragraph"><style:paragraph-properties fo:line-height="18pt"/></style:style>"#,
        r#"<style:style style:name="P4" style:family="paragraph"><style:paragraph-properties fo:line-height="12.7mm"/></style:style>"#,
        r#"<style:style style:name="P5" style:family="paragraph"><style:paragraph-properties fo:line-height="2.54cm"/></style:style>"#,
        r#"<style:style style:name="P6" style:family="paragraph"><style:paragraph-properties fo:line-height="0.2in"/></style:style>"#,
        r#"<style:style style:name="P7" style:family="paragraph"><style:paragraph-properties fo:line-height="1pt"/></style:style>"#,
        r#"<style:style style:name="P8" style:family="paragraph"><style:paragraph-properties fo:line-height="10in"/></style:style>"#,
        r#"<style:style style:name="P9" style:family="paragraph"><style:paragraph-properties fo:line-height="0%"/></style:style>"#,
        r#"<style:style style:name="P10" style:family="paragraph"><style:paragraph-properties fo:line-height="-12pt"/></style:style>"#,
        r#"<style:style style:name="P11" style:family="paragraph"><style:paragraph-properties fo:line-height="1001%"/></style:style>"#,
        r#"<style:style style:name="P12" style:family="paragraph"><style:paragraph-properties fo:line-height="99999999in"/></style:style>"#,
        r#"</office:automatic-styles>"#,
        r#"<office:body><office:text>"#,
        r#"<text:p text:style-name="P1">100</text:p>"#,
        r#"<text:p text:style-name="P2">108</text:p>"#,
        r#"<text:p text:style-name="P3">pt</text:p>"#,
        r#"<text:p text:style-name="P4">mm</text:p>"#,
        r#"<text:p text:style-name="P5">cm</text:p>"#,
        r#"<text:p text:style-name="P6">in</text:p>"#,
        r#"<text:p text:style-name="P7">min</text:p>"#,
        r#"<text:p text:style-name="P8">max</text:p>"#,
        r#"<text:p text:style-name="P9">zero</text:p>"#,
        r#"<text:p text:style-name="P10">neg</text:p>"#,
        r#"<text:p text:style-name="P11">over</text:p>"#,
        r#"<text:p text:style-name="P12">huge</text:p>"#,
        r#"</office:text></office:body></office:document-content>"#,
    );
    let odt_doc = decode_doc(&minimal_odt(odt_content), "spacing.odt");
    let odt_spacings: Vec<Option<u16>> = odt_doc
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => p.line_spacing_percent,
            _ => panic!("paragraph expected"),
        })
        .collect();
    assert_eq!(
        odt_spacings,
        vec![
            Some(100),
            Some(108),
            Some(150),
            Some(300),
            Some(600),
            Some(120),
            Some(50),
            Some(1000),
            None,
            None,
            None,
            None,
        ]
    );
    let odt_bytes =
        UniversalFormatConverter::encode_rich(&rt_doc, DocumentFormat::Odt, None).expect("odt");
    assert_eq!(decode_doc(&odt_bytes, "rt.odt").blocks, rt_doc.blocks);
}

#[test]
fn table_cells_reset_body_paragraph_defaults_and_roundtrip_cleanly() {
    let mut table = RichTable::new(vec![
        vec!["항목".into(), "참고 링크".into()],
        vec!["값 1".into(), "값 2".into()],
    ]);
    table.links = vec![TableCellLink {
        row: 0,
        col: 1,
        start: 0,
        end: "참고 링크".len() as u32,
        url: "https://example.com".to_string(),
    }];
    let doc = RichDocument {
        title: "표 셀 리셋".to_string(),
        blocks: vec![RichBlock::Table(table.clone())],
        ..RichDocument::default()
    };

    let docx_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).expect("docx");
    let docx_parts = unpack(&docx_bytes);
    let docx_styles = part(&docx_parts, "word/styles.xml");
    assert!(
        docx_styles.contains(
            "<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/><w:jc w:val=\"left\"/></w:pPr><w:tblPr>"
        ),
        "{docx_styles}"
    );
    let docx_body = part(&docx_parts, "word/document.xml");
    let cell_ppr = "<w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/><w:jc w:val=\"left\"/></w:pPr>";
    assert_eq!(docx_body.matches(cell_ppr).count(), 4, "{docx_body}");
    assert_eq!(decode_doc(&docx_bytes, "tbl.docx").blocks, doc.blocks);

    let odt_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).expect("odt");
    let odt_parts = unpack(&odt_bytes);
    let odt_content = part(&odt_parts, "content.xml");
    assert!(
        odt_content.contains(
            "<style:style style:name=\"TableCellP\" style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"start\" fo:margin-bottom=\"0pt\" fo:line-height=\"100%\"/></style:style>"
        ),
        "{odt_content}"
    );
    assert_eq!(
        odt_content
            .matches("<text:p text:style-name=\"TableCellP\">")
            .count(),
        4,
        "{odt_content}"
    );
    assert_eq!(decode_doc(&odt_bytes, "tbl.odt").blocks, doc.blocks);

    let rtf_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).expect("rtf");
    let rtf_raw = String::from_utf8(rtf_bytes.clone()).expect("utf8");
    assert_eq!(
        rtf_raw
            .matches("\\pard\\intbl\\ql\\sa0\\sl240\\slmult1 ")
            .count(),
        2,
        "{rtf_raw}"
    );
    let rtf_back = decode_doc(&rtf_bytes, "tbl.rtf");
    let RichBlock::Table(rtf_tbl) = &rtf_back.blocks[0] else {
        panic!("expected table");
    };
    assert_eq!(rtf_tbl.rows, table.rows);
    let rtf_flat = RtfCodec::decode(&rtf_bytes).expect("flat rtf");
    assert_eq!(rtf_flat.tables, vec![table.rows]);
}

#[test]
fn docx_landscape_and_bound_tables_scale_tbl_grid_to_page_text_width() {
    let two_col = RichTable::new(vec![vec!["왼쪽".into(), "오른쪽".into()]]);
    let landscape_doc = RichDocument {
        title: "가로 표".to_string(),
        blocks: vec![RichBlock::Table(two_col.clone())],
        page_frame: RichPageFrame {
            paper: Some(RichPaper {
                width: 16_838,
                height: 11_906,
            }),
            margins: Some(RichPageMargins::DOC_DEFAULT),
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };
    let landscape_bytes =
        UniversalFormatConverter::encode_rich(&landscape_doc, DocumentFormat::Docx, None)
            .expect("landscape docx");
    let landscape_parts = unpack(&landscape_bytes);
    let landscape_xml = part(&landscape_parts, "word/document.xml");
    // 16838 - 1440 - 1440 = 13958; 13958 / 2 = 6979.
    assert_eq!(
        landscape_xml.matches("<w:gridCol w:w=\"6979\"/>").count(),
        2,
        "{landscape_xml}"
    );

    let three_col = RichTable::new(vec![vec!["A".into(), "B".into(), "C".into()]]);
    let duplex_doc = RichDocument {
        title: "제본 가로 표".to_string(),
        blocks: vec![RichBlock::Table(three_col)],
        page_frame: RichPageFrame {
            paper: Some(RichPaper {
                width: 16_838,
                height: 11_906,
            }),
            margins: Some(RichPageMargins {
                left: 1_000,
                right: 1_000,
                ..RichPageMargins::DOC_DEFAULT
            }),
            binding: RichBinding {
                kind: RichBindingKind::Duplex,
                gutter: 500,
            },
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };
    let duplex_bytes =
        UniversalFormatConverter::encode_rich(&duplex_doc, DocumentFormat::Docx, None)
            .expect("duplex docx");
    let duplex_parts = unpack(&duplex_bytes);
    let duplex_xml = part(&duplex_parts, "word/document.xml");
    // 16838 - 1000 - 1000 - 500 = 14338; 14338 / 3 = 4779.
    assert_eq!(
        duplex_xml.matches("<w:gridCol w:w=\"4779\"/>").count(),
        3,
        "{duplex_xml}"
    );

    let top_bound_doc = RichDocument {
        title: "상철 표".to_string(),
        blocks: vec![RichBlock::Table(two_col)],
        page_frame: RichPageFrame {
            paper: Some(RichPaper::A4),
            margins: Some(RichPageMargins::DOC_DEFAULT),
            binding: RichBinding {
                kind: RichBindingKind::Top,
                gutter: 1_000,
            },
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };
    let top_bytes =
        UniversalFormatConverter::encode_rich(&top_bound_doc, DocumentFormat::Docx, None)
            .expect("top docx");
    let top_parts = unpack(&top_bytes);
    let top_xml = part(&top_parts, "word/document.xml");
    // Top binding does not consume side width: (11906 - 2880) / 2 = 4513.
    assert_eq!(
        top_xml.matches("<w:gridCol w:w=\"4513\"/>").count(),
        2,
        "{top_xml}"
    );
}

#[test]
fn doc_save_loss_reports_comments_revisions_and_first_page_frames_for_doc() {
    let doc = RichDocument {
        title: "DOC 손실".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![
                RichRun {
                    text: "추가".to_string(),
                    comment_id: Some(1),
                    revision: Some(RichRevision {
                        id: 1,
                        kind: RichRevisionKind::Insert,
                        author: "작성자".to_string(),
                        date: "2026-11-23T08:00:00Z".to_string(),
                    }),
                    ..RichRun::default()
                },
                RichRun {
                    text: "삭제".to_string(),
                    comment_id: Some(2),
                    revision: Some(RichRevision {
                        id: 2,
                        kind: RichRevisionKind::Delete,
                        author: "작성자".to_string(),
                        date: "2026-11-23T08:05:00Z".to_string(),
                    }),
                    ..RichRun::default()
                },
            ],
            ..RichParagraph::default()
        })],
        comments: vec![
            RichComment {
                id: 1,
                author: "검토자".to_string(),
                date: "2026-11-23T09:00:00Z".to_string(),
                text: "첫 메모".to_string(),
            },
            RichComment {
                id: 2,
                author: "검토자".to_string(),
                date: "2026-11-23T09:01:00Z".to_string(),
                text: "둘째 메모".to_string(),
            },
        ],
        page_frame: RichPageFrame {
            first_page_different: true,
            first_header: vec![RichParagraph {
                runs: vec![RichRun::plain("첫 머리글")],
                ..RichParagraph::default()
            }],
            first_footer: vec![RichParagraph {
                runs: vec![RichRun::plain("첫 바닥글")],
                ..RichParagraph::default()
            }],
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };

    let loss = doc_save_loss(&doc, DocumentFormat::Doc);
    assert_eq!(loss.comments, 2);
    assert_eq!(loss.revisions, 2);
    assert_eq!(loss.first_page_frames, 2);
    let warnings = loss.warnings("DOC");
    assert_eq!(
        warnings,
        vec![
            "DOC 저장에서 빠진 메모 2개",
            "DOC 저장에서 빠진 변경 추적 2곳",
            "DOC 저장에서 빠진 첫 쪽 머리글·바닥글 2개",
        ]
    );
}

#[test]
fn right_indent_twips_roundtrips_across_docx_odt_hwpx_and_rtf() {
    let doc = RichDocument {
        title: "오른쪽 여백".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            left_indent_twips: Some(360),
            right_indent_twips: Some(720),
            first_line_indent_twips: Some(180),
            runs: vec![RichRun::plain("문단 오른쪽 들여쓰기 확인")],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    for (fmt, name) in [
        (DocumentFormat::Docx, "right.docx"),
        (DocumentFormat::Odt, "right.odt"),
        (DocumentFormat::Hwpx, "right.hwpx"),
        (DocumentFormat::Rtf, "right.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).expect(name);
        let decoded = decode_doc(&bytes, name);
        let RichBlock::Paragraph(p) = &decoded.blocks[0] else {
            panic!("expected paragraph for {name}");
        };
        assert_eq!(p.left_indent_twips, Some(360), "{name}");
        assert_eq!(p.right_indent_twips, Some(720), "{name}");
        assert_eq!(p.first_line_indent_twips, Some(180), "{name}");
    }
}
