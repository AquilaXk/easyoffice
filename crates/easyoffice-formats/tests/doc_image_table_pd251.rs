//! Multi-format lossless roundtrip, fallback preservation, fail-closed security budgets,
//! and `--release` benchmark for `PD-251`:
//! 1. Table column widths (`col_widths_twips`)
//! 2. Table cell shading (`CellFill`) & per-cell 4-edge borders (`TableCellBorders`)
//! 3. Table cell inline pictures (`cell_images`) + display size (`width_emu` / `height_emu`)

mod image_fixture;

use std::time::Instant;

use easyoffice_formats::{
    pages_write_loss, CellFill, CellMerge, DecodedContent, DocumentFormat, EasyOfficeError,
    RichBlock, RichDocument, RichImage, RichParagraph, RichRun, RichTable, TableBorderEdge,
    TableBorderKind, TableCellBorders, TableCellImage, TableCellStyleEntry,
    UniversalFormatConverter, ZipArchiveCodec, MAX_COL_WIDTH_TWIPS, MAX_IMAGES_PER_DOCUMENT,
    MAX_STYLED_TABLE_CELLS, MIN_COL_WIDTH_TWIPS,
};
use image_fixture::{make_jpeg, make_png};

fn para(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    })
}

fn edge(kind: TableBorderKind, rgb: u32) -> TableBorderEdge {
    TableBorderEdge::new(kind, rgb)
}

fn sample_rich_doc() -> (Vec<u8>, Vec<u8>, RichDocument) {
    let png = make_png(32, 24, 2, 3);
    let jpg = make_jpeg(48, 36);

    let mut top_img = RichImage::new(png.clone(), "상단 도표").unwrap();
    top_img.set_display_emu(2_743_200, 1_828_800).unwrap();

    let mut cell_png = RichImage::new(png.clone(), "셀 차트").unwrap();
    cell_png.set_display_emu(1_371_600, 914_400).unwrap();

    let mut cell_jpg = RichImage::new(jpg.clone(), "빈 셀 사진").unwrap();
    cell_jpg.set_display_emu(914_400, 914_400).unwrap();

    let mut table = RichTable::with_merges(
        vec![
            vec!["항목".into(), "값".into(), "비고".into()],
            vec!["매출".into(), "1200".into(), "상승".into()],
            vec!["합계".into(), "2400".into(), String::new()],
        ],
        vec![CellMerge {
            row: 0,
            col: 1,
            row_span: 1,
            col_span: 2,
        }],
    )
    .0;

    table.col_widths_twips = vec![2_160, 4_320, 2_880];
    table.cell_styles = vec![
        TableCellStyleEntry {
            row: 0,
            col: 0,
            fill: CellFill::from_hex("#E8F1FC").unwrap(),
            borders: TableCellBorders {
                top: edge(TableBorderKind::Thick, 0x1F4E79),
                right: edge(TableBorderKind::Thin, 0x1F4E79),
                bottom: edge(TableBorderKind::Double, 0x1F4E79),
                left: TableBorderEdge::NONE,
            },
        },
        TableCellStyleEntry {
            row: 1,
            col: 1,
            fill: CellFill::from_hex("#FFF4CC").unwrap(),
            borders: TableCellBorders {
                top: edge(TableBorderKind::Dashed, 0xD93025),
                right: edge(TableBorderKind::Dotted, 0x188038),
                bottom: TableBorderEdge::UNSPECIFIED,
                left: TableBorderEdge::UNSPECIFIED,
            },
        },
        TableCellStyleEntry {
            row: 2,
            col: 2,
            fill: CellFill::NONE,
            borders: TableCellBorders {
                top: TableBorderEdge::NONE,
                right: TableBorderEdge::NONE,
                bottom: TableBorderEdge::NONE,
                left: TableBorderEdge::NONE,
            },
        },
    ];
    table.cell_images = vec![
        TableCellImage {
            row: 1,
            col: 1,
            image: cell_png,
        },
        TableCellImage {
            row: 2,
            col: 2,
            image: cell_jpg,
        },
    ];

    let doc = RichDocument {
        page_frame: Default::default(),
        title: "표 스타일과 그림".into(),
        blocks: vec![
            para("문서 서두"),
            RichBlock::Image(top_img),
            RichBlock::Table(table),
            para("문서 말미"),
        ],
        official_agency: None,
        ..RichDocument::default()
    };
    (png, jpg, doc)
}

#[test]
fn docx_odt_and_hwpx_roundtrip_table_widths_styles_borders_cell_images_and_display_sizes() {
    let (png, jpg, doc) = sample_rich_doc();

    for fmt in [
        DocumentFormat::Docx,
        DocumentFormat::Odt,
        DocumentFormat::Hwpx,
    ] {
        let encoded = UniversalFormatConverter::encode_rich(&doc, fmt, None)
            .unwrap_or_else(|e| panic!("encode {fmt:?}: {e}"));
        let decoded = UniversalFormatConverter::decode_rich(&encoded, Some(fmt.extension()))
            .unwrap_or_else(|e| panic!("decode {fmt:?}: {e}"));
        assert!(
            decoded.warnings.is_empty(),
            "{fmt:?} unexpected warnings: {:?}",
            decoded.warnings
        );
        let DecodedContent::Document(back) = decoded.content else {
            panic!("expected Document for {fmt:?}");
        };

        assert_eq!(back.blocks.len(), 4, "{fmt:?} block count");
        let RichBlock::Image(back_top) = &back.blocks[1] else {
            panic!("{fmt:?} expected top image");
        };
        assert_eq!(back_top.bytes(), png.as_slice(), "{fmt:?} top png bytes");
        assert_eq!(back_top.alt(), "상단 도표", "{fmt:?} top alt");
        let (tw, th) = back_top.display_emu();
        assert!(
            tw.abs_diff(2_743_200) <= 640 && th.abs_diff(1_828_800) <= 640,
            "{fmt:?} top display emu mismatch: ({tw}, {th})"
        );

        let RichBlock::Table(tbl) = &back.blocks[2] else {
            panic!("{fmt:?} expected table");
        };
        assert_eq!(
            tbl.col_widths_twips,
            vec![2_160, 4_320, 2_880],
            "{fmt:?} col_widths_twips"
        );
        assert_eq!(
            tbl.merges,
            vec![CellMerge {
                row: 0,
                col: 1,
                row_span: 1,
                col_span: 2
            }],
            "{fmt:?} merges"
        );

        let styles = tbl.validated_cell_styles();
        assert_eq!(styles.len(), 3, "{fmt:?} styled cells: {styles:?}");
        assert_eq!(styles[0].row, 0);
        assert_eq!(styles[0].col, 0);
        assert_eq!(styles[0].fill.rgb(), Some(0xE8F1FC), "{fmt:?} (0,0) fill");
        assert_eq!(
            styles[0].borders,
            TableCellBorders {
                top: edge(TableBorderKind::Thick, 0x1F4E79),
                right: edge(TableBorderKind::Thin, 0x1F4E79),
                bottom: edge(TableBorderKind::Double, 0x1F4E79),
                left: TableBorderEdge::NONE,
            },
            "{fmt:?} (0,0) borders"
        );
        assert_eq!((styles[1].row, styles[1].col), (1, 1));
        assert_eq!(styles[1].fill.rgb(), Some(0xFFF4CC), "{fmt:?} (1,1) fill");
        assert_eq!(
            styles[1].borders,
            TableCellBorders {
                top: edge(TableBorderKind::Dashed, 0xD93025),
                right: edge(TableBorderKind::Dotted, 0x188038),
                bottom: TableBorderEdge::UNSPECIFIED,
                left: TableBorderEdge::UNSPECIFIED,
            },
            "{fmt:?} (1,1) borders"
        );
        assert_eq!((styles[2].row, styles[2].col), (2, 2));
        assert!(styles[2].fill.is_none());
        assert_eq!(
            styles[2].borders,
            TableCellBorders {
                top: TableBorderEdge::NONE,
                right: TableBorderEdge::NONE,
                bottom: TableBorderEdge::NONE,
                left: TableBorderEdge::NONE,
            },
            "{fmt:?} (2,2) borders"
        );

        let imgs = tbl.validated_cell_images();
        assert_eq!(imgs.len(), 2, "{fmt:?} cell_images count");
        assert_eq!((imgs[0].row, imgs[0].col), (1, 1));
        assert_eq!(imgs[0].image.bytes(), png.as_slice());
        assert_eq!(imgs[0].image.alt(), "셀 차트");
        let (cw, ch) = imgs[0].image.display_emu();
        assert!(
            cw.abs_diff(1_371_600) <= 640 && ch.abs_diff(914_400) <= 640,
            "{fmt:?} (1,1) display emu mismatch: ({cw}, {ch})"
        );
        assert_eq!((imgs[1].row, imgs[1].col), (2, 2));
        assert_eq!(imgs[1].image.bytes(), jpg.as_slice());
        assert_eq!(imgs[1].image.alt(), "빈 셀 사진");
        assert_eq!(tbl.rows[2][2], "", "{fmt:?} empty cell stays empty");
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn count_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

#[test]
fn pages_and_pdf_fallback_preserve_cell_images_and_report_style_loss() {
    let (_png, jpg, doc) = sample_rich_doc();

    assert!(pages_write_loss(&doc).formats > 0);
    let pages_bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None)
        .expect("encode Pages");
    let decoded =
        UniversalFormatConverter::decode_rich(&pages_bytes, Some("pages")).expect("decode Pages");
    let DecodedContent::Document(back) = decoded.content else {
        panic!("expected Document");
    };
    let top_images = back
        .blocks
        .iter()
        .filter(|b| matches!(b, RichBlock::Image(_)))
        .count();
    let cell_images: usize = back
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Table(t) => Some(t.cell_images.len()),
            _ => None,
        })
        .sum();
    assert_eq!(top_images + cell_images, 3, "Pages preserves all 3 images");

    let pdf_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pdf, None).expect("encode PDF");
    assert!(pdf_bytes.starts_with(b"%PDF-"));
    assert_eq!(
        count_occurrences(&pdf_bytes, b"/Subtype /Image"),
        3,
        "PDF embeds top + 2 cell Image XObjects"
    );
    assert!(contains(&pdf_bytes, b"/Filter /DCTDecode"));
    assert!(contains(&pdf_bytes, b"/Filter /FlateDecode"));
    assert!(contains(&pdf_bytes, &jpg));
}

#[test]
fn table_style_validation_and_hostile_budgets_fail_closed() {
    let png = make_png(8, 8, 2, 3);
    let img = RichImage::new(png, "b").unwrap();

    // 1. Out-of-bounds widths clamp; mismatched length returns None.
    let mut t = RichTable::new(vec![vec!["a".into(), "b".into()]]);
    t.col_widths_twips = vec![10, 999_999];
    assert_eq!(
        t.validated_col_widths(),
        Some(vec![MIN_COL_WIDTH_TWIPS, MAX_COL_WIDTH_TWIPS])
    );
    t.col_widths_twips = vec![1_440];
    assert_eq!(t.validated_col_widths(), None);

    // 2. Duplicate/out-of-bounds cell styles deduplicate last-wins and cap at MAX_STYLED_TABLE_CELLS.
    let mut big_styles = Vec::with_capacity(MAX_STYLED_TABLE_CELLS + 16);
    for _ in 0..=MAX_STYLED_TABLE_CELLS {
        big_styles.push(TableCellStyleEntry {
            row: 0,
            col: 0,
            fill: CellFill::from_hex("#112233").unwrap(),
            borders: TableCellBorders::default(),
        });
    }
    t.cell_styles = big_styles;
    assert_eq!(t.validated_cell_styles().len(), 1);
    t.cell_styles.clear();

    // 3. Exceeding MAX_IMAGES_PER_DOCUMENT via cell_images fails closed across writers.
    t.cell_images = (0..=MAX_IMAGES_PER_DOCUMENT)
        .map(|_| TableCellImage {
            row: 0,
            col: 0,
            image: img.clone(),
        })
        .collect();
    let hostile_doc = RichDocument {
        blocks: vec![RichBlock::Table(t)],
        ..RichDocument::default()
    };
    for fmt in [
        DocumentFormat::Docx,
        DocumentFormat::Odt,
        DocumentFormat::Hwpx,
    ] {
        let err = UniversalFormatConverter::encode_rich(&hostile_doc, fmt, None).unwrap_err();
        assert!(
            matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
            "{fmt:?} expected ResourceBudgetExceeded, got {err:?}"
        );
    }

    // 4. Hostile HWPX header with > MAX_HWPX_BORDER_FILLS (4096) fails closed.
    let mut bfs = String::new();
    for id in 3..=4_105 {
        bfs.push_str(&format!("<hh:borderFill id=\"{id}\"/>"));
    }
    let hostile_header = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hh:head xmlns:hh=\"http://www.owpml.org/owpml/2011/head\"><hh:refList><hh:borderFills itemCnt=\"4103\">{bfs}</hh:borderFills></hh:refList></hh:head>"
    );
    let content_hpf = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><opf:package xmlns:opf=\"http://www.idpf.org/2007/opf/\"><opf:manifest><opf:item id=\"header\" href=\"Contents/header.xml\" media-type=\"application/xml\"/></opf:manifest></opf:package>";
    let section = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hs:sec xmlns:hs=\"http://www.owpml.org/owpml/2011/section\" xmlns:hp=\"http://www.owpml.org/owpml/2011/paragraph\"><hp:p><hp:run><hp:t>x</hp:t></hp:run></hp:p></hs:sec>";
    let hwpx = ZipArchiveCodec::pack_compressed(&[
        ("mimetype".to_string(), b"application/hwp+zip".to_vec()),
        (
            "Contents/content.hpf".to_string(),
            content_hpf.as_bytes().to_vec(),
        ),
        (
            "Contents/header.xml".to_string(),
            hostile_header.into_bytes(),
        ),
        (
            "Contents/section0.xml".to_string(),
            section.as_bytes().to_vec(),
        ),
    ])
    .unwrap();
    let err = UniversalFormatConverter::decode_rich(&hwpx, Some("hwpx")).unwrap_err();
    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));

    // 5. Hostile ODT repeating a cell image column 300 times fails closed on ImageBudget.
    let odt_manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"><manifest:file-entry manifest:full-path=\"Pictures/p.png\" manifest:media-type=\"image/png\"/></manifest:manifest>";
    let odt_content = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><office:body><office:text><table:table><table:table-row><table:table-cell table:number-columns-repeated=\"300\"><text:p><draw:frame><draw:image xlink:href=\"Pictures/p.png\"/></draw:frame></text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>";
    let odt_png = make_png(8, 8, 2, 3);
    let odt = ZipArchiveCodec::pack_compressed(&[
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.text".to_vec(),
        ),
        (
            "META-INF/manifest.xml".to_string(),
            odt_manifest.as_bytes().to_vec(),
        ),
        ("content.xml".to_string(), odt_content.as_bytes().to_vec()),
        ("Pictures/p.png".to_string(), odt_png),
    ])
    .unwrap();
    let odt_err = UniversalFormatConverter::decode_rich(&odt, Some("odt")).unwrap_err();
    assert!(matches!(
        odt_err,
        EasyOfficeError::ResourceBudgetExceeded(_)
    ));
}

#[test]
#[ignore]
fn bench_pd251_doc_table_style_and_cell_images_release() {
    let png = make_png(64, 48, 2, 3);
    let mut rows = Vec::new();
    let mut styles = Vec::new();
    let mut cell_images = Vec::new();
    for r in 0..12u32 {
        rows.push(vec![
            format!("항목 {r}"),
            format!("값 {}", r * 10),
            format!("설명 {r}"),
            String::new(),
        ]);
        styles.push(TableCellStyleEntry {
            row: r,
            col: r % 4,
            fill: CellFill::from_hex("#EEF4FA").unwrap(),
            borders: TableCellBorders {
                top: edge(TableBorderKind::Thin, 0x1F4E79),
                right: edge(TableBorderKind::Thick, 0x1F4E79),
                bottom: edge(TableBorderKind::Dashed, 0x4A5568),
                left: TableBorderEdge::NONE,
            },
        });
        if r < 4 {
            cell_images.push(TableCellImage {
                row: r,
                col: 3,
                image: RichImage::new(png.clone(), "미리보기").unwrap(),
            });
        }
    }
    let mut table = RichTable::new(rows);
    table.col_widths_twips = vec![2_160, 2_160, 2_880, 1_800];
    table.cell_styles = styles;
    table.cell_images = cell_images;
    let doc = RichDocument {
        blocks: vec![RichBlock::Table(table)],
        ..RichDocument::default()
    };

    let iters = 30usize;
    let start = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iters {
        for fmt in [
            DocumentFormat::Docx,
            DocumentFormat::Odt,
            DocumentFormat::Hwpx,
        ] {
            let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).unwrap();
            total_bytes += bytes.len();
            let decoded =
                UniversalFormatConverter::decode_rich(&bytes, Some(fmt.extension())).unwrap();
            let DecodedContent::Document(back) = decoded.content else {
                panic!();
            };
            let RichBlock::Table(bt) = &back.blocks[0] else {
                panic!();
            };
            assert_eq!(bt.cell_images.len(), 4);
        }
    }
    let elapsed = start.elapsed();
    let ops = iters * 3;
    let per_op_ms = elapsed.as_secs_f64() * 1000.0 / ops as f64;
    println!(
        "PD-251 3-format roundtrip: {ops} encode+decode ops in {:?} ({per_op_ms:.3} ms/op, total {total_bytes} encoded bytes)",
        elapsed
    );
}
