//! Slice 5 (PD-357): paragraph before/after spacing (0..=400pt = 0..=8000 twips) across DOCX/ODT/.pages,
//! and table-cell inner run formatting + paragraph alignment/line-spacing roundtrip in DOCX/ODT.

use easyoffice_formats::{
    pages_write_loss, CellFill, CellMerge, DecodedContent, DocumentFormat, EasyOfficeError,
    RichAlignment, RichBlock, RichCellParagraph, RichDocument, RichParagraph, RichRun, RichScript,
    RichTable, TableCellBorders, TableCellLink, TableCellParagraphs, TableCellStyleEntry,
    UniversalFormatConverter, ZipArchiveCodec, MAX_CELL_PARAGRAPH_ENTRIES, MAX_PARAGRAPHS_PER_CELL,
};

fn build_doc_with_spacing_and_rich_table() -> RichDocument {
    let mut table = RichTable::new(vec![
        vec!["합계 H2O".to_string(), "링크 셀".to_string()],
        vec!["병합 셀".to_string(), "".to_string()],
    ]);
    table.merges = vec![CellMerge {
        row: 1,
        col: 0,
        row_span: 1,
        col_span: 2,
    }];
    table.links = vec![TableCellLink {
        row: 0,
        col: 1,
        start: 0,
        end: "링크 셀".len() as u32,
        url: "https://example.org".to_string(),
    }];
    table.col_widths_twips = vec![2400, 3200];
    table.cell_styles = vec![TableCellStyleEntry {
        row: 0,
        col: 0,
        fill: CellFill::solid(0xE2E8F0),
        borders: TableCellBorders::default(),
    }];
    table.cell_paragraphs = vec![
        TableCellParagraphs {
            row: 0,
            col: 0,
            paragraphs: vec![RichCellParagraph {
                runs: vec![
                    RichRun {
                        text: "합계 ".to_string(),
                        bold: true,
                        italic: true,
                        underline: true,
                        strike: true,
                        color: Some("B91C1C".to_string()),
                        font: Some("맑은 고딕".to_string()),
                        size_half_pt: Some(26),
                        ..RichRun::default()
                    },
                    RichRun {
                        text: "H".to_string(),
                        ..RichRun::default()
                    },
                    RichRun {
                        text: "2".to_string(),
                        script: RichScript::Subscript,
                        ..RichRun::default()
                    },
                    RichRun {
                        text: "O".to_string(),
                        script: RichScript::Superscript,
                        ..RichRun::default()
                    },
                ],
                alignment: Some(RichAlignment::Right),
                line_spacing_percent: Some(150),
            }],
        },
        TableCellParagraphs {
            row: 1,
            col: 0,
            paragraphs: vec![RichCellParagraph {
                runs: vec![RichRun {
                    text: "병합 셀".to_string(),
                    bold: true,
                    color: Some("1D4ED8".to_string()),
                    ..RichRun::default()
                }],
                alignment: Some(RichAlignment::Center),
                line_spacing_percent: Some(200),
            }],
        },
    ];

    RichDocument {
        title: "단락간격_표셀서식".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("첫 문단")],
                space_before_twips: Some(240),
                space_after_twips: Some(360),
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("최대 간격 경계")],
                space_before_twips: Some(0),
                space_after_twips: Some(8000),
                ..RichParagraph::default()
            }),
            RichBlock::Table(table),
        ],
        ..RichDocument::default()
    }
}

fn decode_doc(bytes: &[u8], file_name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(file_name))
        .unwrap()
        .content
    {
        DecodedContent::Document(d) => d,
        other => panic!("expected document: {other:?}"),
    }
}

#[test]
fn paragraph_before_after_spacing_survives_docx_and_odt_and_pages_handles_loss_honestly() {
    let doc = build_doc_with_spacing_and_rich_table();
    for (fmt, name) in [
        (DocumentFormat::Docx, "spacing.docx"),
        (DocumentFormat::Odt, "spacing.odt"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).unwrap();
        let reopened = decode_doc(&bytes, name);
        let paras: Vec<&RichParagraph> = reopened
            .blocks
            .iter()
            .filter_map(|b| match b {
                RichBlock::Paragraph(p) => Some(p),
                _ => None,
            })
            .collect();
        assert!(paras.len() >= 2, "{fmt:?}: {paras:?}");
        assert_eq!(paras[0].space_before_twips, Some(240), "{fmt:?}");
        assert_eq!(paras[0].space_after_twips, Some(360), "{fmt:?}");
        assert_eq!(paras[1].space_after_twips, Some(8000), "{fmt:?}");
    }
    let pages_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None).unwrap();
    let pages_reopened = decode_doc(&pages_bytes, "spacing.pages");
    let first_text: String = match &pages_reopened.blocks[0] {
        RichBlock::Paragraph(p) => p.runs.iter().map(|r| r.text.as_str()).collect(),
        _ => String::new(),
    };
    assert_eq!(first_text, "첫 문단");
}

#[test]
fn table_cell_runs_alignment_line_spacing_and_merges_roundtrip_in_docx_and_odt() {
    let doc = build_doc_with_spacing_and_rich_table();
    for (fmt, name) in [
        (DocumentFormat::Docx, "cells.docx"),
        (DocumentFormat::Odt, "cells.odt"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).unwrap();
        let reopened = decode_doc(&bytes, name);
        let table = reopened
            .blocks
            .iter()
            .find_map(|b| match b {
                RichBlock::Table(t) => Some(t),
                _ => None,
            })
            .expect("table present");

        assert_eq!(table.merges.len(), 1, "{fmt:?}");
        assert_eq!(table.merges[0].col_span, 2, "{fmt:?}");
        assert!(!table.links.is_empty(), "{fmt:?} keeps cell link");
        assert!(!table.cell_styles.is_empty(), "{fmt:?} keeps cell shading");

        let c00 = table
            .cell_paragraphs
            .iter()
            .find(|c| c.row == 0 && c.col == 0)
            .unwrap_or_else(|| panic!("{fmt:?} missing cell (0,0) paragraphs"));
        let p0 = &c00.paragraphs[0];
        assert_eq!(p0.alignment, Some(RichAlignment::Right), "{fmt:?}");
        assert_eq!(p0.line_spacing_percent, Some(150), "{fmt:?}");
        assert!(
            p0.runs
                .iter()
                .any(|r| r.bold && r.italic && r.underline && r.strike),
            "{fmt:?}: {:?}",
            p0.runs
        );
        assert!(
            p0.runs.iter().any(|r| r.color.as_deref() == Some("B91C1C")
                && r.size_half_pt == Some(26)
                && r.font.as_deref() == Some("맑은 고딕")),
            "{fmt:?}: {:?}",
            p0.runs
        );
        assert!(
            p0.runs
                .iter()
                .any(|r| r.script == RichScript::Subscript && r.text == "2"),
            "{fmt:?}: {:?}",
            p0.runs
        );
        assert!(
            p0.runs
                .iter()
                .any(|r| r.script == RichScript::Superscript && r.text == "O"),
            "{fmt:?}: {:?}",
            p0.runs
        );

        let c10 = table
            .cell_paragraphs
            .iter()
            .find(|c| c.row == 1 && c.col == 0)
            .unwrap_or_else(|| panic!("{fmt:?} missing merged cell (1,0) paragraphs"));
        assert_eq!(
            c10.paragraphs[0].alignment,
            Some(RichAlignment::Center),
            "{fmt:?}"
        );
        assert_eq!(c10.paragraphs[0].line_spacing_percent, Some(200), "{fmt:?}");
    }
}

#[test]
fn out_of_bounds_cell_paragraphs_are_safely_ignored() {
    let mut table = RichTable::new(vec![vec!["a".to_string()]]);
    table.cell_paragraphs = vec![
        TableCellParagraphs {
            row: 99,
            col: 0,
            paragraphs: vec![RichCellParagraph {
                runs: vec![RichRun {
                    text: "oob".to_string(),
                    bold: true,
                    ..RichRun::default()
                }],
                ..RichCellParagraph::default()
            }],
        },
        TableCellParagraphs {
            row: 0,
            col: 50,
            paragraphs: vec![RichCellParagraph {
                runs: vec![RichRun {
                    text: "oob2".to_string(),
                    italic: true,
                    ..RichRun::default()
                }],
                ..RichCellParagraph::default()
            }],
        },
    ];
    let doc = RichDocument {
        title: "oob".to_string(),
        blocks: vec![RichBlock::Table(table)],
        ..RichDocument::default()
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let reopened = decode_doc(&bytes, "oob.docx");
    let tbl = reopened
        .blocks
        .iter()
        .find_map(|b| match b {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert!(tbl.cell_paragraphs.is_empty());
}

fn pack_odt_content(content_xml: &str) -> Vec<u8> {
    let manifest = r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;
    ZipArchiveCodec::pack_compressed(&[
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.text".to_vec(),
        ),
        (
            "META-INF/manifest.xml".to_string(),
            manifest.as_bytes().to_vec(),
        ),
        ("content.xml".to_string(), content_xml.as_bytes().to_vec()),
    ])
    .unwrap()
}

#[test]
fn pages_write_loss_counts_rich_cell_paragraphs() {
    let mut formatted_tbl = RichTable::new(vec![vec!["셀".to_string()]]);
    formatted_tbl.cell_paragraphs = vec![TableCellParagraphs {
        row: 0,
        col: 0,
        paragraphs: vec![RichCellParagraph {
            runs: vec![RichRun {
                text: "셀".to_string(),
                bold: true,
                ..RichRun::default()
            }],
            alignment: Some(RichAlignment::Center),
            line_spacing_percent: Some(150),
        }],
    }];
    let doc_fmt = RichDocument {
        title: "pages_loss_rich".to_string(),
        blocks: vec![RichBlock::Table(formatted_tbl)],
        ..RichDocument::default()
    };
    assert_eq!(pages_write_loss(&doc_fmt).formats, 1);

    let mut plain_tbl = RichTable::new(vec![vec!["셀".to_string()]]);
    plain_tbl.cell_paragraphs = vec![TableCellParagraphs {
        row: 0,
        col: 0,
        paragraphs: vec![RichCellParagraph {
            runs: vec![RichRun::plain("셀")],
            ..RichCellParagraph::default()
        }],
    }];
    let doc_plain = RichDocument {
        title: "pages_loss_plain".to_string(),
        blocks: vec![RichBlock::Table(plain_tbl)],
        ..RichDocument::default()
    };
    assert_eq!(pages_write_loss(&doc_plain).formats, 0);
}

#[test]
fn odt_repeated_styled_cells_exceeding_budget_fail_closed() {
    let empty_paras = "<text:p text:style-name=\"CenterP\"/>".repeat(2_000);
    let rows_empty = format!(
        "<table:table-row><table:table-cell table:number-columns-repeated=\"1024\">{empty_paras}</table:table-cell></table:table-row>"
    )
    .repeat(10);
    let xml_empty = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:xlink="http://www.w3.org/1999/xlink">
  <office:automatic-styles>
    <style:style style:name="CenterP" style:family="paragraph"><style:paragraph-properties fo:text-align="center"/></style:style>
  </office:automatic-styles>
  <office:body><office:text><table:table>{rows_empty}</table:table></office:text></office:body>
</office:document-content>"#
    );
    let odt_empty = pack_odt_content(&xml_empty);
    let err_empty =
        UniversalFormatConverter::decode_rich(&odt_empty, Some("bomb1.odt")).unwrap_err();
    assert!(
        matches!(err_empty, EasyOfficeError::ResourceBudgetExceeded(_)),
        "expected ResourceBudgetExceeded for repeated empty styled paras, got {err_empty:?}"
    );

    let span_runs = "<text:a xlink:href=\"https://example.org/r\"><text:span text:style-name=\"BoldT\">a</text:span><text:span text:style-name=\"ItalicT\">b</text:span></text:a>".repeat(500);
    let rows_spans = format!(
        "<table:table-row><table:table-cell table:number-columns-repeated=\"1024\"><text:p>{span_runs}</text:p></table:table-cell></table:table-row>"
    )
    .repeat(5);
    let xml_spans = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:xlink="http://www.w3.org/1999/xlink">
  <office:automatic-styles>
    <style:style style:name="BoldT" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
    <style:style style:name="ItalicT" style:family="text"><style:text-properties fo:font-style="italic"/></style:style>
  </office:automatic-styles>
  <office:body><office:text><table:table>{rows_spans}</table:table></office:text></office:body>
</office:document-content>"#
    );
    let odt_spans = pack_odt_content(&xml_spans);
    let err_spans =
        UniversalFormatConverter::decode_rich(&odt_spans, Some("bomb2.odt")).unwrap_err();
    assert!(
        matches!(err_spans, EasyOfficeError::ResourceBudgetExceeded(_)),
        "expected ResourceBudgetExceeded for repeated styled/linked 1-byte spans, got {err_spans:?}"
    );
}

#[test]
fn odt_repeated_cells_past_max_entries_stop_gracefully_and_overflow_cell_keeps_plain_text() {
    let rows_cap = "<table:table-row><table:table-cell table:number-columns-repeated=\"1024\"><text:p><text:span text:style-name=\"BoldT\">x</text:span></text:p></table:table-cell></table:table-row>"
        .repeat(55);
    let xml_cap = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:automatic-styles>
    <style:style style:name="BoldT" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
  </office:automatic-styles>
  <office:body><office:text><table:table>{rows_cap}</table:table></office:text></office:body>
</office:document-content>"#
    );
    let odt_cap = pack_odt_content(&xml_cap);
    let doc_cap = decode_doc(&odt_cap, "cap.odt");
    let tbl_cap = doc_cap
        .blocks
        .iter()
        .find_map(|b| match b {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(tbl_cap.rows.len(), 55);
    assert_eq!(tbl_cap.cell_paragraphs.len(), MAX_CELL_PARAGRAPH_ENTRIES);

    let mut overflow_paras = String::new();
    for i in 0..=(MAX_PARAGRAPHS_PER_CELL + 4) {
        overflow_paras.push_str(&format!(
            "<text:p><text:span text:style-name=\"BoldT\">L{i}</text:span></text:p>"
        ));
    }
    let xml_overflow = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:automatic-styles>
    <style:style style:name="BoldT" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
  </office:automatic-styles>
  <office:body><office:text><table:table><table:table-row><table:table-cell>{overflow_paras}</table:table-cell></table:table-row></table:table></office:text></office:body>
</office:document-content>"#
    );
    let odt_overflow = pack_odt_content(&xml_overflow);
    let doc_overflow = decode_doc(&odt_overflow, "overflow.odt");
    let tbl_overflow = doc_overflow
        .blocks
        .iter()
        .find_map(|b| match b {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert!(tbl_overflow.cell_paragraphs.is_empty());
    let tail_marker = format!("L{}", MAX_PARAGRAPHS_PER_CELL + 4);
    assert!(
        tbl_overflow.rows[0][0].ends_with(&tail_marker),
        "expected plain text past MAX_PARAGRAPHS_PER_CELL to survive"
    );
}
