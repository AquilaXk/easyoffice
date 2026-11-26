//! Slice 5 (PD-357): paragraph before/after spacing (0..=400pt = 0..=8000 twips) across DOCX/ODT/.pages,
//! and table-cell inner run formatting + paragraph alignment/line-spacing roundtrip in DOCX/ODT.

use easyoffice_formats::{
    CellFill, CellMerge, DecodedContent, DocumentFormat, RichAlignment, RichBlock,
    RichCellParagraph, RichDocument, RichParagraph, RichRun, RichScript, RichTable,
    TableCellBorders, TableCellLink, TableCellParagraphs, TableCellStyleEntry,
    UniversalFormatConverter,
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
