//! PD-256: Bidirectional XLSX floating images (`xl/drawings/drawing*.xml`, `xl/media/*`)
//! and cell notes (`xl/comments*.xml`, `xl/drawings/vmlDrawing*.vml`), unified drawing
//! coexistence with charts/tables/pivots, media deduplication, PDF rendering (Image XObjects +
//! note indicators), fail-closed hostile input guards, and release benchmark.

use std::collections::BTreeMap;
use std::time::Instant;

use easyoffice_formats::{
    deflate_zlib, encode_sheet_pdf, pack_cell_pos, CellBox, ChartAnchor, ChartKind, ChartLegendPos,
    ChartSeriesRef, EasyOfficeError, IeeeCrc32, OdsUnpacked, OoxmlCodec, PivotAggKind,
    PivotValueField, RichImage, SheetCellNote, SheetChart, SheetFloatingImage, SheetPivotTable,
    SheetStyling, SheetTable, ZipArchiveCodec, ZipSecurityPolicy, MAX_NOTES_PER_SHEET,
    MAX_SHEET_IMAGES_PER_SHEET, MAX_SHEET_NOTE_TEXT_BYTES,
};

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

fn make_png(width: u32, height: u32, color_type: u8, r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
    let bpp = match color_type {
        6 => 4usize,
        2 => 3usize,
        _ => 1usize,
    };
    let stride = 1 + width as usize * bpp;
    let mut scanline = Vec::with_capacity(stride);
    scanline.push(0);
    match color_type {
        6 => {
            for _ in 0..width {
                scanline.extend_from_slice(&[r, g, b, a]);
            }
        }
        2 => {
            for _ in 0..width {
                scanline.extend_from_slice(&[r, g, b]);
            }
        }
        _ => scanline.extend(std::iter::repeat_n(r, width as usize)),
    }
    let mut raw = Vec::with_capacity(stride * height as usize);
    for _ in 0..height {
        raw.extend_from_slice(&scanline);
    }
    let compressed = deflate_zlib(&raw);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, color_type, 0, 0, 0]);
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    bytes.extend_from_slice(&png_chunk(b"IDAT", &compressed));
    bytes.extend_from_slice(&png_chunk(b"IEND", &[]));
    bytes
}

fn make_jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    // SOF0 baseline RGB
    out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8]);
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
    // SOS
    out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C, 3, 1, 0, 2, 0, 3, 0, 0, 63, 0]);
    out.extend_from_slice(&[0x00, 0xFF, 0xD9]);
    out
}

fn make_palette_png(width: u32, height: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width as usize + 1) * height as usize);
    for _ in 0..height {
        raw.push(0);
        raw.extend(std::iter::repeat_n(0u8, width as usize));
    }
    let compressed = deflate_zlib(&raw);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 3, 0, 0, 0]);
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    bytes.extend_from_slice(&png_chunk(b"PLTE", &[10, 20, 30]));
    bytes.extend_from_slice(&png_chunk(b"IDAT", &compressed));
    bytes.extend_from_slice(&png_chunk(b"IEND", &[]));
    bytes
}

#[test]
fn floating_images_only_round_trip_and_dedup_across_sheets() {
    let png_bytes = make_png(4, 3, 6, 12, 80, 200, 180);
    let jpg_bytes = make_jpeg(6, 4);
    let sheets: Vec<OdsUnpacked> = vec![
        ("Sheet1".to_string(), vec![(0, 0, "Header".to_string())]),
        ("Sheet2".to_string(), vec![(0, 0, "Copy".to_string())]),
    ];

    let img1 = SheetFloatingImage {
        id: "pic-1".to_string(),
        name: "회사 로고".to_string(),
        alt: "파란 로고".to_string(),
        image: RichImage::new(png_bytes.clone(), "파란 로고").unwrap(),
        anchor: ChartAnchor {
            from_col: 1,
            from_row: 1,
            to_col: 4,
            to_row: 6,
            left_px: 80,
            top_px: 24,
            width_px: 240,
            height_px: 120,
            ..ChartAnchor::default()
        },
    };
    let img2 = SheetFloatingImage {
        id: "pic-2".to_string(),
        name: "사진".to_string(),
        alt: "샘플 JPEG".to_string(),
        image: RichImage::new(jpg_bytes.clone(), "샘플 JPEG").unwrap(),
        anchor: ChartAnchor {
            from_col: 5,
            from_row: 1,
            to_col: 7,
            to_row: 4,
            left_px: 400,
            top_px: 24,
            width_px: 160,
            height_px: 72,
            ..ChartAnchor::default()
        },
    };
    let img_dup = SheetFloatingImage {
        id: "pic-3".to_string(),
        name: "로고 사본".to_string(),
        alt: "재사용 로고".to_string(),
        image: RichImage::new(png_bytes.clone(), "재사용 로고").unwrap(),
        anchor: ChartAnchor {
            left_px: 120,
            top_px: 48,
            width_px: 64,
            height_px: 48,
            ..ChartAnchor::default()
        },
    };

    let st1 = SheetStyling {
        images: vec![img1, img2],
        ..Default::default()
    };
    let st2 = SheetStyling {
        images: vec![img_dup],
        ..Default::default()
    };

    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[st1, st2]).unwrap();
    let entries = ZipArchiveCodec::unpack(&xlsx, &ZipSecurityPolicy::default()).unwrap();

    // Shared PNG must be deduplicated to a single media part (`xl/media/image1.png`)
    let media_paths: Vec<&str> = entries
        .iter()
        .filter(|e| e.path.starts_with("xl/media/"))
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(
        media_paths,
        vec!["xl/media/image1.png", "xl/media/image2.jpg"]
    );

    let ct = std::str::from_utf8(
        &entries
            .iter()
            .find(|e| e.path == "[Content_Types].xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(ct.contains(r#"Extension="png" ContentType="image/png""#));
    assert!(ct.contains(r#"Extension="jpg" ContentType="image/jpeg""#));
    assert_eq!(ct.matches(r#"Extension="png""#).count(), 1);

    let (_, out_st) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    assert_eq!(out_st[0].1.images_rejected, 0);
    assert_eq!(out_st[0].1.images.len(), 2);
    assert_eq!(out_st[0].1.images[0].name, "회사 로고");
    assert_eq!(out_st[0].1.images[0].alt, "파란 로고");
    assert_eq!(out_st[0].1.images[0].image.bytes(), png_bytes.as_slice());
    assert_eq!(out_st[0].1.images[0].anchor.from_col, 1);
    assert_eq!(out_st[0].1.images[0].anchor.to_col, 4);
    assert_eq!(out_st[0].1.images[1].image.bytes(), jpg_bytes.as_slice());

    assert_eq!(out_st[1].1.images.len(), 1);
    assert_eq!(out_st[1].1.images[0].name, "로고 사본");
    assert_eq!(out_st[1].1.images[0].alt, "재사용 로고");
    assert_eq!(out_st[1].1.images[0].image.bytes(), png_bytes.as_slice());
    assert_eq!(out_st[1].1.images[0].anchor.width_px, 64);
    assert_eq!(out_st[1].1.images[0].anchor.height_px, 48);
}

#[test]
fn cell_notes_round_trip_idempotent_author_prefix_and_visibility() {
    let sheets: Vec<OdsUnpacked> = vec![(
        "메모시트".to_string(),
        vec![
            (0, 0, "항목".to_string()),
            (1, 1, "100".to_string()),
            (2, 3, "200".to_string()),
        ],
    )];

    let mut notes = BTreeMap::new();
    notes.insert(
        pack_cell_pos(1, 1),
        SheetCellNote {
            author: "작성자".to_string(),
            text: "첫째 줄\n둘째 줄 & <확인>".to_string(),
            visible: true,
        },
    );
    notes.insert(
        pack_cell_pos(2, 3),
        SheetCellNote {
            author: "".to_string(),
            text: "작성자 없는\n메모".to_string(),
            visible: false,
        },
    );
    notes.insert(
        pack_cell_pos(4, 2),
        SheetCellNote {
            author: "검토자".to_string(),
            text: "검토자:\n이미 접두사가 있는 본문".to_string(),
            visible: false,
        },
    );

    let st = SheetStyling {
        notes,
        ..Default::default()
    };

    // Round-trip 1
    let xlsx1 = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[st]).unwrap();
    let (_, out1) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx1).unwrap();
    let st1 = &out1[0].1;
    assert_eq!(st1.notes_rejected, 0);
    assert_eq!(st1.notes.len(), 3);
    assert_eq!(
        st1.notes.get(&pack_cell_pos(1, 1)).unwrap(),
        &SheetCellNote {
            author: "작성자".to_string(),
            text: "첫째 줄\n둘째 줄 & <확인>".to_string(),
            visible: true,
        }
    );
    assert_eq!(
        st1.notes.get(&pack_cell_pos(2, 3)).unwrap(),
        &SheetCellNote {
            author: "".to_string(),
            text: "작성자 없는\n메모".to_string(),
            visible: false,
        }
    );
    assert_eq!(
        st1.notes.get(&pack_cell_pos(4, 2)).unwrap(),
        &SheetCellNote {
            author: "검토자".to_string(),
            text: "검토자:\n이미 접두사가 있는 본문".to_string(),
            visible: false,
        }
    );

    // Round-trip 2: author prefix must NOT duplicate
    let xlsx2 =
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(st1)).unwrap();
    let (_, out2) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx2).unwrap();
    assert_eq!(out2[0].1.notes, st1.notes);
}

#[test]
fn unified_sheet_with_chart_image_note_table_and_pivot_preserves_ooxml_order() {
    let sheets: Vec<OdsUnpacked> = vec![(
        "통합".to_string(),
        vec![
            (0, 0, "분류".to_string()),
            (0, 1, "값".to_string()),
            (1, 0, "A".to_string()),
            (1, 1, "10".to_string()),
            (2, 0, "B".to_string()),
            (2, 1, "20".to_string()),
        ],
    )];

    let mut notes = BTreeMap::new();
    notes.insert(
        pack_cell_pos(1, 1),
        SheetCellNote {
            author: "담당".to_string(),
            text: "실적 확인".to_string(),
            visible: true,
        },
    );

    let styling = SheetStyling {
        tables: vec![SheetTable::new(
            "SalesTbl",
            CellBox::parse("A1:B3").unwrap(),
        )],
        charts: vec![SheetChart {
            id: "ch-1".to_string(),
            kind: ChartKind::Column,
            title: "매출".to_string(),
            data_range: "A1:B3".to_string(),
            switch_row_col: false,
            legend: ChartLegendPos::Right,
            anchor: ChartAnchor {
                from_col: 3,
                from_row: 0,
                to_col: 8,
                to_row: 10,
                ..ChartAnchor::default()
            },
            series: vec![ChartSeriesRef {
                name: "값".to_string(),
                name_ref: Some("통합!$B$1".to_string()),
                categories_ref: Some("통합!$A$2:$A$3".to_string()),
                values_ref: "통합!$B$2:$B$3".to_string(),
            }],
        }],
        images: vec![SheetFloatingImage {
            id: "pic-1".to_string(),
            name: "배너".to_string(),
            alt: "배너 이미지".to_string(),
            image: RichImage::new(make_png(4, 4, 2, 10, 20, 30, 255), "배너 이미지").unwrap(),
            anchor: ChartAnchor {
                from_col: 3,
                from_row: 12,
                to_col: 7,
                to_row: 18,
                width_px: 320,
                height_px: 144,
                ..ChartAnchor::default()
            },
        }],
        pivots: vec![SheetPivotTable {
            name: "PV1".to_string(),
            cache_id: 1,
            source_sheet: "통합".to_string(),
            source_range: "A1:B3".to_string(),
            source_table: None,
            location: CellBox::parse("A6:B9").unwrap(),
            row_fields: vec![0],
            col_fields: Vec::new(),
            filter_fields: Vec::new(),
            value_fields: vec![PivotValueField {
                col: 1,
                agg: PivotAggKind::Sum,
                name: "합계".to_string(),
            }],
            filters: Vec::new(),
            field_names: Vec::new(),
            has_header: true,
            slicers: Vec::new(),
        }],
        notes,
        ..Default::default()
    };

    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling]).unwrap();
    let entries = ZipArchiveCodec::unpack(&xlsx, &ZipSecurityPolicy::default()).unwrap();

    let sheet1 = std::str::from_utf8(
        &entries
            .iter()
            .find(|e| e.path == "xl/worksheets/sheet1.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    // CT_Worksheet must have at most ONE <drawing> before <legacyDrawing> before <tableParts>
    assert_eq!(sheet1.matches("<drawing ").count(), 1, "{sheet1}");
    assert_eq!(sheet1.matches("<legacyDrawing ").count(), 1, "{sheet1}");
    let d_pos = sheet1.find("<drawing ").unwrap();
    let l_pos = sheet1.find("<legacyDrawing ").unwrap();
    let t_pos = sheet1.find("<tableParts").unwrap();
    assert!(d_pos < l_pos && l_pos < t_pos, "{sheet1}");

    let drawing1 = std::str::from_utf8(
        &entries
            .iter()
            .find(|e| e.path == "xl/drawings/drawing1.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(drawing1.contains("<c:chart"));
    assert!(drawing1.contains("<xdr:pic>"));

    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let st = &back[0].1;
    assert_eq!(st.charts.len(), 1);
    assert_eq!(st.images.len(), 1);
    assert_eq!(st.notes.len(), 1);
    assert_eq!(st.tables.len(), 1);
    assert_eq!(st.pivots.len(), 1);
}

#[test]
fn sheet_pdf_embeds_images_with_smask_fallback_and_red_note_triangles() {
    let sheets: Vec<OdsUnpacked> = vec![(
        "PDF".to_string(),
        vec![(0, 0, "항목".to_string()), (0, 1, "값".to_string())],
    )];
    let mut notes = BTreeMap::new();
    notes.insert(
        pack_cell_pos(0, 1),
        SheetCellNote {
            author: "작성자".to_string(),
            text: "메모 내용".to_string(),
            visible: false,
        },
    );
    let styling = SheetStyling {
        images: vec![
            SheetFloatingImage {
                id: "pic-alpha".to_string(),
                name: "RGBA".to_string(),
                alt: "알파 PNG".to_string(),
                image: RichImage::new(make_png(2, 2, 6, 40, 120, 220, 128), "알파 PNG").unwrap(),
                anchor: ChartAnchor {
                    from_col: 2,
                    from_row: 0,
                    to_col: 4,
                    to_row: 4,
                    width_px: 160,
                    height_px: 96,
                    ..ChartAnchor::default()
                },
            },
            SheetFloatingImage {
                id: "pic-pal".to_string(),
                name: "Palette".to_string(),
                alt: "대체 텍스트".to_string(),
                image: RichImage::new(make_palette_png(2, 2), "대체 텍스트").unwrap(),
                anchor: ChartAnchor {
                    from_col: 2,
                    from_row: 5,
                    to_col: 4,
                    to_row: 8,
                    width_px: 160,
                    height_px: 72,
                    ..ChartAnchor::default()
                },
            },
        ],
        notes,
        ..Default::default()
    };

    let pdf = encode_sheet_pdf("미디어 PDF", &sheets, &[styling], &[]).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4"));
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(
        pdf_text.contains("/Subtype /Image"),
        "missing image XObject"
    );
    assert!(pdf_text.contains("/SMask "), "missing alpha SMask");
    assert!(pdf_text.contains("/Im1 Do"), "missing image draw operator");
    assert!(
        pdf_text.contains("0.82 0.14 0.14 rg"),
        "missing red triangle note indicator"
    );
    assert!(
        pdf_text.contains("re S Q"),
        "missing fallback box for palette PNG"
    );
}

#[test]
fn hostile_inputs_and_budgets_fail_closed() {
    let sheets: Vec<OdsUnpacked> = vec![("S1".to_string(), vec![(0, 0, "A".to_string())])];

    // 1. Over MAX_SHEET_IMAGES_PER_SHEET (65)
    let png = RichImage::new(make_png(2, 2, 2, 1, 2, 3, 255), "").unwrap();
    let mut over_imgs = SheetStyling::default();
    for i in 0..=MAX_SHEET_IMAGES_PER_SHEET {
        over_imgs.images.push(SheetFloatingImage {
            id: format!("pic-{i}"),
            name: format!("img-{i}"),
            alt: String::new(),
            image: png.clone(),
            anchor: ChartAnchor::default(),
        });
    }
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_imgs]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. Over MAX_NOTES_PER_SHEET (4,097)
    let mut over_notes = SheetStyling::default();
    for i in 0..=MAX_NOTES_PER_SHEET {
        over_notes.notes.insert(
            pack_cell_pos(i as u32, 0),
            SheetCellNote {
                author: "A".to_string(),
                text: "n".to_string(),
                visible: false,
            },
        );
    }
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_notes]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 3. Over MAX_SHEET_NOTE_TEXT_BYTES (40 KiB)
    let mut over_text = SheetStyling::default();
    over_text.notes.insert(
        pack_cell_pos(0, 0),
        SheetCellNote {
            author: "A".to_string(),
            text: "x".repeat(40 * 1024),
            visible: false,
        },
    );
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[over_text]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 4. Path traversal (`../../secret.png`) + bad PNG CRC rejected cleanly
    let mut bad_crc_png = make_png(2, 2, 2, 1, 2, 3, 255);
    let last = bad_crc_png.len() - 1;
    bad_crc_png[last] ^= 0xFF;
    let pkg = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>ok</t></is></c></row></sheetData><drawing r:id="rIdD1"/></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdD1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#.to_vec()),
        ("xl/drawings/drawing1.xml".to_string(), concat!(
            r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
            r#"<xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>2</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="1" name="Traversal"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rId1"/></xdr:blipFill><xdr:spPr/></xdr:pic></xdr:twoCellAnchor>"#,
            r#"<xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>2</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="BadCrc"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rId2"/></xdr:blipFill><xdr:spPr/></xdr:pic></xdr:twoCellAnchor>"#,
            r#"</xdr:wsDr>"#
        ).as_bytes().to_vec()),
        ("xl/drawings/_rels/drawing1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../../secret.png"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/bad.png"/></Relationships>"#.to_vec()),
        ("secret.png".to_string(), make_png(2, 2, 2, 1, 2, 3, 255)),
        ("xl/media/bad.png".to_string(), bad_crc_png),
    ]).unwrap();
    let (_, out) = OoxmlCodec::unpack_xlsx_with_styling(&pkg).unwrap();
    assert_eq!(out[0].1.images.len(), 0);
    assert_eq!(out[0].1.images_rejected, 2);

    // 5. Hostile XML event bomb in comments1.xml -> ResourceBudgetExceeded
    let mut bomb = String::from(
        r#"<?xml version="1.0"?><comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>A</author></authors><commentList>"#,
    );
    for _ in 0..70_000 {
        bomb.push_str("<x/>");
    }
    bomb.push_str("</commentList></comments>");
    let bomb_pkg = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdC1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/></Relationships>"#.to_vec()),
        ("xl/comments1.xml".to_string(), bomb.into_bytes()),
    ]).unwrap();
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&bomb_pkg),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 6. Hostile 40 KiB note text in comments1.xml -> ResourceBudgetExceeded
    let huge_note = format!(
        r#"<?xml version="1.0"?><comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>A</author></authors><commentList><comment ref="A1" authorId="0"><text><t>{}</t></text></comment></commentList></comments>"#,
        "z".repeat(MAX_SHEET_NOTE_TEXT_BYTES + 8192)
    );
    let huge_pkg = ZipArchiveCodec::pack_compressed(&[
        ("xl/workbook.xml".to_string(), br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec()),
        ("xl/_rels/workbook.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec()),
        ("xl/worksheets/sheet1.xml".to_string(), br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#.to_vec()),
        ("xl/worksheets/_rels/sheet1.xml.rels".to_string(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdC1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/></Relationships>"#.to_vec()),
        ("xl/comments1.xml".to_string(), huge_note.into_bytes()),
    ]).unwrap();
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&huge_pkg),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
#[ignore = "release performance benchmark"]
fn bench_pd256_media_and_notes_roundtrip() {
    let shared_png = RichImage::new(make_png(16, 16, 6, 30, 110, 220, 255), "공유 로고").unwrap();
    let alt_jpg = RichImage::new(make_jpeg(32, 24), "사진").unwrap();
    let mut cells = Vec::with_capacity(1_000);
    let mut notes = BTreeMap::new();
    for i in 0..1_000u32 {
        let r = i / 10;
        let c = i % 10;
        cells.push((r, c, format!("val-{i}")));
        notes.insert(
            pack_cell_pos(r, c),
            SheetCellNote {
                author: format!("Author{}", i % 4),
                text: format!("메모 본문 #{i}"),
                visible: i % 10 == 0,
            },
        );
    }
    let mut images = Vec::with_capacity(32);
    for i in 0..32u32 {
        images.push(SheetFloatingImage {
            id: format!("pic-{}", i + 1),
            name: format!("그림 {}", i + 1),
            alt: "벤치마크 그림".to_string(),
            image: if i % 4 == 0 {
                alt_jpg.clone()
            } else {
                shared_png.clone()
            },
            anchor: ChartAnchor {
                from_col: (i % 8) + 12,
                from_row: i * 2,
                to_col: (i % 8) + 14,
                to_row: i * 2 + 4,
                width_px: 160,
                height_px: 96,
                ..ChartAnchor::default()
            },
        });
    }
    let sheets = vec![("Bench".to_string(), cells)];
    let st = SheetStyling {
        images,
        notes,
        ..Default::default()
    };

    let t_pack = Instant::now();
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[st]).unwrap();
    let pack_ms = t_pack.elapsed().as_secs_f64() * 1_000.0;

    let t_unpack = Instant::now();
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let unpack_ms = t_unpack.elapsed().as_secs_f64() * 1_000.0;
    assert_eq!(back[0].1.images.len(), 32);
    assert_eq!(back[0].1.notes.len(), 1_000);

    let t_lookup = Instant::now();
    let mut hits = 0usize;
    for r in 0..100u32 {
        for c in 0..10u32 {
            if back[0].1.notes.contains_key(&pack_cell_pos(r, c)) {
                hits += 1;
            }
        }
    }
    let lookup_us = t_lookup.elapsed().as_secs_f64() * 1_000_000.0;
    assert_eq!(hits, 1_000);

    println!(
        "PD-256 Bench (1,000 notes + 32 shared floating images): pack={pack_ms:.2}ms ({} B), unpack={unpack_ms:.2}ms, 1000x pack_cell_pos_lookup={lookup_us:.2}us",
        xlsx.len()
    );
}
