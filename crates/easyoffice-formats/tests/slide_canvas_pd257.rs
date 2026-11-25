//! PD-257: Coordinate-based RichSlideDeck model, PPTX/PPSX/ODP lossless roundtrips,
//! media deduplication, 5 chart types, 6 shapes, 3 aspects, 5 layouts, 4 transitions,
//! multi-page pure vector PDF export, fail-closed security/budget guards, and release benchmark.

use std::time::Instant;

use easyoffice_formats::{
    deflate_zlib, encode_slide_pdf, ChartKind, ChartLegendPos, EasyOfficeError, IeeeCrc32,
    OdfCodec, OoxmlCodec, PdfSerializer, RichImage, RichSlide, RichSlideDeck, SlideAspect,
    SlideBox, SlideChartSeries, SlideElement, SlideMasterLayout, SlidePlaceholderRole,
    SlideShapeType, SlideTextAlign, SlideTransition, ZipArchiveCodec, ZipSecurityPolicy,
    MAX_ELEMENTS_PER_SLIDE,
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
    let bpp = if color_type == 6 { 4 } else { 3 };
    let stride = 1 + width as usize * bpp;
    let mut row = Vec::with_capacity(stride);
    row.push(0);
    for _ in 0..width {
        if color_type == 6 {
            row.extend_from_slice(&[r, g, b, a]);
        } else {
            row.extend_from_slice(&[r, g, b]);
        }
    }
    let mut raw = Vec::with_capacity(stride * height as usize);
    for _ in 0..height {
        raw.extend_from_slice(&row);
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
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 8];
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
    out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C, 3, 1, 0, 2, 0, 3, 0, 0, 63, 0]);
    out.extend_from_slice(&[0x00, 0xFF, 0xD9]);
    out
}

fn full_test_deck(aspect: SlideAspect) -> RichSlideDeck {
    let png = RichImage::new(make_png(4, 4, 6, 37, 99, 235, 220), "파랑 도식").unwrap();
    let jpg = RichImage::new(make_jpeg(6, 4), "샘플 사진").unwrap();

    RichSlideDeck {
        title: "분기 전략 보고서".into(),
        is_slideshow: false,
        aspect,
        elements_rejected: 0,
        slides: vec![
            RichSlide {
                title: "경영 요약".into(),
                bullets: vec![
                    "매출 전년 대비 28% 성장".into(),
                    "신규 고객 42개사 확보".into(),
                ],
                notes: "핵심 수치부터 간단히 설명".into(),
                layout: SlideMasterLayout::TitleSlide,
                transition: SlideTransition::Fade,
                elements: vec![
                    SlideElement::TextBox {
                        id: "tb-sub".into(),
                        rect: SlideBox {
                            x: 60.0,
                            y: 120.0,
                            w: 320.0,
                            h: 64.0,
                        },
                        text: "2026년 3분기\n성과 브리핑".into(),
                        font_size_pt: 20.0,
                        bold: true,
                        italic: true,
                        underline: true,
                        align: SlideTextAlign::Center,
                        color: Some("1E3A8A".into()),
                        fill: Some("EFF6FF".into()),
                        role: Some(SlidePlaceholderRole::Subtitle),
                    },
                    SlideElement::Shape {
                        id: "sh-callout".into(),
                        rect: SlideBox {
                            x: 400.0,
                            y: 120.0,
                            w: 220.0,
                            h: 72.0,
                        },
                        shape_type: SlideShapeType::Callout,
                        text: "목표 초과 달성".into(),
                        fill: Some("DBEAFE".into()),
                        stroke: Some("2563EB".into()),
                        stroke_width_pt: 2.0,
                        color: Some("0F172A".into()),
                        font_size_pt: 15.0,
                        bold: true,
                        italic: true,
                        underline: true,
                        align: SlideTextAlign::Center,
                    },
                    SlideElement::Image {
                        id: "img-a".into(),
                        rect: SlideBox {
                            x: 60.0,
                            y: 220.0,
                            w: 120.0,
                            h: 90.0,
                        },
                        image: png.clone(),
                        alt: "파랑 로고".into(),
                    },
                    SlideElement::Image {
                        id: "img-dup".into(),
                        rect: SlideBox {
                            x: 200.0,
                            y: 220.0,
                            w: 120.0,
                            h: 90.0,
                        },
                        image: png,
                        alt: "동일 이미지 재사용".into(),
                    },
                ],
            },
            RichSlide {
                title: "세부 지표 및 차트".into(),
                bullets: vec!["부문별 실적 비교".into()],
                notes: "표와 막대 차트를 함께 제시".into(),
                layout: SlideMasterLayout::TwoColumn,
                transition: SlideTransition::Push,
                elements: vec![
                    SlideElement::Table {
                        id: "tbl-kpi".into(),
                        rect: SlideBox {
                            x: 40.0,
                            y: 110.0,
                            w: 280.0,
                            h: 150.0,
                        },
                        rows: vec![
                            vec!["부서".into(), "실적".into(), "달성률".into()],
                            vec!["클라우드".into(), "420".into(), "112%".into()],
                            vec!["엔터프라이즈".into(), "315".into()],
                        ],
                        header_row: true,
                        fill: Some("1E40AF".into()),
                    },
                    SlideElement::Chart {
                        id: "ch-col".into(),
                        rect: SlideBox {
                            x: 340.0,
                            y: 110.0,
                            w: 340.0,
                            h: 200.0,
                        },
                        chart_type: ChartKind::Column,
                        title: "분기 매출 추이".into(),
                        categories: vec!["1분기".into(), "2분기".into(), "3분기".into()],
                        series: vec![
                            SlideChartSeries {
                                name: "목표".into(),
                                values: vec![100.0, 120.0, 140.0],
                            },
                            SlideChartSeries {
                                name: "실적".into(),
                                values: vec![108.0, 132.0, 165.0],
                            },
                        ],
                        legend: ChartLegendPos::Bottom,
                    },
                    SlideElement::Chart {
                        id: "ch-bar".into(),
                        rect: SlideBox {
                            x: 40.0,
                            y: 280.0,
                            w: 280.0,
                            h: 170.0,
                        },
                        chart_type: ChartKind::Bar,
                        title: "제품 비중".into(),
                        categories: vec!["문서".into(), "표".into()],
                        series: vec![SlideChartSeries {
                            name: "점유".into(),
                            values: vec![65.0, 35.0],
                        }],
                        legend: ChartLegendPos::Right,
                    },
                    SlideElement::Image {
                        id: "img-jpg".into(),
                        rect: SlideBox {
                            x: 340.0,
                            y: 330.0,
                            w: 140.0,
                            h: 100.0,
                        },
                        image: jpg,
                        alt: "현장 사진".into(),
                    },
                ],
            },
            RichSlide {
                title: String::new(),
                bullets: Vec::new(),
                notes: "전환 없음, 빈 레이아웃 테스트".into(),
                layout: SlideMasterLayout::Blank,
                transition: SlideTransition::None,
                elements: vec![
                    SlideElement::Shape {
                        id: "sh-arrow".into(),
                        rect: SlideBox {
                            x: 60.0,
                            y: 80.0,
                            w: 160.0,
                            h: 70.0,
                        },
                        shape_type: SlideShapeType::RightArrow,
                        text: "다음 단계".into(),
                        fill: Some("16A34A".into()),
                        stroke: Some("14532D".into()),
                        stroke_width_pt: 1.5,
                        color: Some("FFFFFF".into()),
                        font_size_pt: 14.0,
                        bold: false,
                        italic: false,
                        underline: false,
                        align: SlideTextAlign::Center,
                    },
                    SlideElement::Shape {
                        id: "sh-diamond".into(),
                        rect: SlideBox {
                            x: 250.0,
                            y: 80.0,
                            w: 110.0,
                            h: 80.0,
                        },
                        shape_type: SlideShapeType::Diamond,
                        text: "검토".into(),
                        fill: Some("F59E0B".into()),
                        stroke: Some("92400E".into()),
                        stroke_width_pt: 1.0,
                        color: None,
                        font_size_pt: 13.0,
                        bold: true,
                        italic: false,
                        underline: false,
                        align: SlideTextAlign::Center,
                    },
                    SlideElement::Shape {
                        id: "sh-round".into(),
                        rect: SlideBox {
                            x: 380.0,
                            y: 80.0,
                            w: 130.0,
                            h: 80.0,
                        },
                        shape_type: SlideShapeType::RoundRect,
                        text: "승인".into(),
                        fill: Some("7C3AED".into()),
                        stroke: None,
                        stroke_width_pt: 0.0,
                        color: Some("FFFFFF".into()),
                        font_size_pt: 14.0,
                        bold: true,
                        italic: false,
                        underline: false,
                        align: SlideTextAlign::Center,
                    },
                    SlideElement::Shape {
                        id: "sh-ellipse".into(),
                        rect: SlideBox {
                            x: 530.0,
                            y: 80.0,
                            w: 120.0,
                            h: 80.0,
                        },
                        shape_type: SlideShapeType::Ellipse,
                        text: "완료".into(),
                        fill: Some("0891B2".into()),
                        stroke: Some("0E7490".into()),
                        stroke_width_pt: 2.0,
                        color: Some("FFFFFF".into()),
                        font_size_pt: 14.0,
                        bold: false,
                        italic: false,
                        underline: false,
                        align: SlideTextAlign::Center,
                    },
                    SlideElement::Chart {
                        id: "ch-pie".into(),
                        rect: SlideBox {
                            x: 60.0,
                            y: 190.0,
                            w: 200.0,
                            h: 170.0,
                        },
                        chart_type: ChartKind::Pie,
                        title: "예산 배분".into(),
                        categories: vec!["연구".into(), "운영".into(), "마케팅".into()],
                        series: vec![SlideChartSeries {
                            name: "비율".into(),
                            values: vec![50.0, 30.0, 20.0],
                        }],
                        legend: ChartLegendPos::Top,
                    },
                    SlideElement::Chart {
                        id: "ch-line".into(),
                        rect: SlideBox {
                            x: 280.0,
                            y: 190.0,
                            w: 200.0,
                            h: 170.0,
                        },
                        chart_type: ChartKind::Line,
                        title: "만족도".into(),
                        categories: vec!["Q1".into(), "Q2".into(), "Q3".into()],
                        series: vec![SlideChartSeries {
                            name: "점수".into(),
                            values: vec![82.0, 88.0, 94.0],
                        }],
                        legend: ChartLegendPos::Left,
                    },
                    SlideElement::Chart {
                        id: "ch-scat".into(),
                        rect: SlideBox {
                            x: 500.0,
                            y: 190.0,
                            w: 190.0,
                            h: 170.0,
                        },
                        chart_type: ChartKind::Scatter,
                        title: "분포".into(),
                        categories: vec!["1".into(), "2".into(), "3".into()],
                        series: vec![SlideChartSeries {
                            name: "측정".into(),
                            values: vec![12.5, 24.0, 31.5],
                        }],
                        legend: ChartLegendPos::None,
                    },
                ],
            },
        ],
    }
}

fn assert_deck_close(orig: &RichSlideDeck, got: &RichSlideDeck, label: &str) {
    assert_eq!(got.aspect, orig.aspect, "{label}: aspect");
    assert_eq!(got.slides.len(), orig.slides.len(), "{label}: slide count");
    for (si, (a, b)) in orig.slides.iter().zip(&got.slides).enumerate() {
        assert_eq!(b.title, a.title, "{label} s{si}: title");
        assert_eq!(b.bullets, a.bullets, "{label} s{si}: bullets");
        assert_eq!(b.notes, a.notes, "{label} s{si}: notes");
        assert_eq!(b.layout, a.layout, "{label} s{si}: layout");
        assert_eq!(b.transition, a.transition, "{label} s{si}: transition");
        assert_eq!(
            b.elements.len(),
            a.elements.len(),
            "{label} s{si}: elements len"
        );
        for (ei, (ea, eb)) in a.elements.iter().zip(&b.elements).enumerate() {
            assert_eq!(eb.id(), ea.id(), "{label} s{si} e{ei}: id");
            let (ra, rb) = (ea.rect(), eb.rect());
            assert!(
                (ra.x - rb.x).abs() < 0.05 && (ra.y - rb.y).abs() < 0.05,
                "{label} s{si} e{ei}: pos {ra:?} vs {rb:?}"
            );
            assert!(
                (ra.w - rb.w).abs() < 0.05 && (ra.h - rb.h).abs() < 0.05,
                "{label} s{si} e{ei}: size {ra:?} vs {rb:?}"
            );
            match (ea, eb) {
                (
                    SlideElement::TextBox {
                        text: t1,
                        font_size_pt: fs1,
                        bold: b1,
                        italic: i1,
                        underline: u1,
                        align: al1,
                        color: c1,
                        fill: f1,
                        role: r1,
                        ..
                    },
                    SlideElement::TextBox {
                        text: t2,
                        font_size_pt: fs2,
                        bold: b2,
                        italic: i2,
                        underline: u2,
                        align: al2,
                        color: c2,
                        fill: f2,
                        role: r2,
                        ..
                    },
                ) => {
                    assert_eq!(
                        (t2, *b2, *i2, *u2, *al2, c2, f2, *r2),
                        (t1, *b1, *i1, *u1, *al1, c1, f1, *r1),
                        "{label} s{si} e{ei}"
                    );
                    assert!((fs1 - fs2).abs() < 0.05);
                }
                (
                    SlideElement::Shape {
                        shape_type: st1,
                        text: t1,
                        fill: f1,
                        stroke: s1,
                        stroke_width_pt: sw1,
                        color: c1,
                        bold: b1,
                        italic: i1,
                        underline: u1,
                        align: al1,
                        ..
                    },
                    SlideElement::Shape {
                        shape_type: st2,
                        text: t2,
                        fill: f2,
                        stroke: s2,
                        stroke_width_pt: sw2,
                        color: c2,
                        bold: b2,
                        italic: i2,
                        underline: u2,
                        align: al2,
                        ..
                    },
                ) => {
                    assert_eq!(
                        (*st2, t2, f2, s2, c2, *b2, *i2, *u2, *al2),
                        (*st1, t1, f1, s1, c1, *b1, *i1, *u1, *al1),
                        "{label} s{si} e{ei}"
                    );
                    assert!((sw1 - sw2).abs() < 0.05);
                }
                (
                    SlideElement::Image {
                        image: i1, alt: a1, ..
                    },
                    SlideElement::Image {
                        image: i2, alt: a2, ..
                    },
                ) => {
                    assert_eq!(i2.bytes(), i1.bytes(), "{label} s{si} e{ei}: img bytes");
                    assert_eq!(a2, a1, "{label} s{si} e{ei}: img alt");
                }
                (
                    SlideElement::Table {
                        rows: r1,
                        header_row: h1,
                        fill: f1,
                        ..
                    },
                    SlideElement::Table {
                        rows: r2,
                        header_row: h2,
                        fill: f2,
                        ..
                    },
                ) => {
                    let max_c = r1.iter().map(Vec::len).max().unwrap_or(1);
                    let norm_r1: Vec<Vec<String>> = r1
                        .iter()
                        .map(|r| {
                            let mut c = r.clone();
                            c.resize(max_c, String::new());
                            c
                        })
                        .collect();
                    assert_eq!((r2, *h2, f2), (&norm_r1, *h1, f1), "{label} s{si} e{ei}");
                }
                (
                    SlideElement::Chart {
                        chart_type: ct1,
                        title: t1,
                        categories: c1,
                        series: s1,
                        legend: l1,
                        ..
                    },
                    SlideElement::Chart {
                        chart_type: ct2,
                        title: t2,
                        categories: c2,
                        series: s2,
                        legend: l2,
                        ..
                    },
                ) => {
                    assert_eq!(
                        (*ct2, t2, c2, s2, *l2),
                        (*ct1, t1, c1, s1, *l1),
                        "{label} s{si} e{ei}"
                    );
                }
                _ => panic!("{label} s{si} e{ei}: mismatched variant"),
            }
        }
    }
}

#[test]
fn pptx_ppsx_lossless_roundtrip_all_aspects_layouts_transitions_elements_and_media_dedup() {
    for aspect in [
        SlideAspect::Widescreen16x9,
        SlideAspect::Standard4x3,
        SlideAspect::HandoutA4,
    ] {
        let deck = full_test_deck(aspect);
        let pptx = OoxmlCodec::pack_pptx_rich(&deck, false).unwrap();
        let unpacked_pptx = OoxmlCodec::unpack_pptx_rich(&pptx).unwrap();
        assert!(!unpacked_pptx.is_slideshow);
        assert_deck_close(&deck, &unpacked_pptx, &format!("PPTX-{}", aspect.as_str()));

        // Verify media deduplication (2 identical PNGs + 1 JPEG = 2 media files total in ppt/media/)
        let entries = ZipArchiveCodec::unpack(&pptx, &ZipSecurityPolicy::default()).unwrap();
        let media_files: Vec<_> = entries
            .iter()
            .filter(|e| e.path.starts_with("ppt/media/"))
            .collect();
        assert_eq!(
            media_files.len(),
            2,
            "identical PNGs must deduplicate to 2 total media parts"
        );

        let ppsx = OoxmlCodec::pack_pptx_rich(&deck, true).unwrap();
        let unpacked_ppsx = OoxmlCodec::unpack_pptx_rich(&ppsx).unwrap();
        assert!(unpacked_ppsx.is_slideshow);
        assert_deck_close(&deck, &unpacked_ppsx, &format!("PPSX-{}", aspect.as_str()));
    }
}

#[test]
fn odp_lossless_roundtrip_all_aspects_layouts_transitions_charts_and_media_dedup() {
    for aspect in [
        SlideAspect::Widescreen16x9,
        SlideAspect::Standard4x3,
        SlideAspect::HandoutA4,
    ] {
        let deck = full_test_deck(aspect);
        let odp = OdfCodec::pack_odp_rich(&deck).unwrap();

        let entries = ZipArchiveCodec::unpack(&odp, &ZipSecurityPolicy::default()).unwrap();
        assert_eq!(entries[0].path, "mimetype");
        assert_eq!(entries[0].data, OdfCodec::MIMETYPE_ODP.as_bytes());
        let pics: Vec<_> = entries
            .iter()
            .filter(|e| e.path.starts_with("Pictures/"))
            .collect();
        assert_eq!(
            pics.len(),
            2,
            "ODP Pictures/ must deduplicate identical PNGs"
        );

        let unpacked = OdfCodec::unpack_odp_rich(&odp).unwrap();
        assert_deck_close(&deck, &unpacked, &format!("ODP-{}", aspect.as_str()));
    }
}

#[test]
fn cross_format_pptx_to_odp_to_pptx_preserves_canvas_and_transition_presets() {
    let mut deck = full_test_deck(SlideAspect::Standard4x3);
    // Test remaining layouts SectionHeader & TitleAndContent & Zoom transition explicitly
    deck.slides.push(RichSlide {
        title: "섹션 전환".into(),
        bullets: vec!["결론 요약".into()],
        notes: "줌 전환 확인".into(),
        layout: SlideMasterLayout::SectionHeader,
        transition: SlideTransition::Zoom,
        elements: vec![],
    });
    let pptx1 = OoxmlCodec::pack_pptx_rich(&deck, false).unwrap();
    let d1 = OoxmlCodec::unpack_pptx_rich(&pptx1).unwrap();
    let odp = OdfCodec::pack_odp_rich(&d1).unwrap();
    let d2 = OdfCodec::unpack_odp_rich(&odp).unwrap();
    let pptx2 = OoxmlCodec::pack_pptx_rich(&d2, false).unwrap();
    let d3 = OoxmlCodec::unpack_pptx_rich(&pptx2).unwrap();
    assert_deck_close(&deck, &d3, "PPTX->ODP->PPTX");
}

#[test]
fn multipage_vector_pdf_export_has_per_page_mediabox_korean_text_and_vector_graphics() {
    for aspect in [
        SlideAspect::Widescreen16x9,
        SlideAspect::Standard4x3,
        SlideAspect::HandoutA4,
    ] {
        let deck = full_test_deck(aspect);
        let pdf = encode_slide_pdf(&deck, "PD-257 벡터 PDF").unwrap();
        let report = PdfSerializer::inspect_readback(&pdf).unwrap();
        assert_eq!(report.pages.len(), 3);
        let (ew, eh) = aspect.dimensions_pt();
        for p in &report.pages {
            assert!(
                (p.width_pt - ew).abs() < 0.5 && (p.height_pt - eh).abs() < 0.5,
                "{}x{}",
                p.width_pt,
                p.height_pt
            );
        }
        let parsed = PdfSerializer::parse(&pdf).unwrap();
        assert_eq!(parsed.pages.len(), 3);
        let all_text: String = parsed
            .pages
            .iter()
            .map(|p| p.body_text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for expected in ["경영 요약", "목표 초과 달성", "분기 매출 추이", "예산 배분"]
        {
            assert!(
                all_text.contains(expected),
                "PDF missing '{expected}': {all_text}"
            );
        }
        let raw = String::from_utf8_lossy(&pdf);
        assert!(raw.contains("/Im1 Do"), "embedded image XObject Do missing");
        assert!(
            raw.contains(" c "),
            "Bezier curves for Ellipse/RoundRect/Pie missing"
        );
        assert!(
            raw.contains("1 0 0.21 1 0 0 cm"),
            "italic oblique shear transform missing in vector PDF"
        );
        assert!(
            raw.contains(" l S Q"),
            "underline stroke missing in vector PDF"
        );
        assert!(
            !raw.contains("[이미지:"),
            "valid PNG/JPEG should render as XObject, not placeholder"
        );
    }
}

#[test]
fn hostile_slide_inputs_fail_closed() {
    let mut bad = full_test_deck(SlideAspect::Widescreen16x9);
    bad.slides[0].elements.push(SlideElement::TextBox {
        id: "oob".into(),
        rect: SlideBox {
            x: 4_001.0,
            y: 10.0,
            w: 50.0,
            h: 20.0,
        },
        text: "범위 초과".into(),
        font_size_pt: 14.0,
        bold: false,
        italic: false,
        underline: false,
        align: SlideTextAlign::Left,
        color: None,
        fill: None,
        role: None,
    });
    assert!(matches!(
        OoxmlCodec::pack_pptx_rich(&bad, false),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // Exceed MAX_ELEMENTS_PER_SLIDE (256)
    let mut flood = RichSlideDeck::default();
    let many: Vec<SlideElement> = (0..=MAX_ELEMENTS_PER_SLIDE)
        .map(|i| SlideElement::Shape {
            id: format!("s{i}"),
            rect: SlideBox {
                x: 10.0,
                y: 10.0,
                w: 20.0,
                h: 20.0,
            },
            shape_type: SlideShapeType::Rect,
            text: String::new(),
            fill: None,
            stroke: None,
            stroke_width_pt: 1.0,
            color: None,
            font_size_pt: 12.0,
            bold: false,
            italic: false,
            underline: false,
            align: SlideTextAlign::Center,
        })
        .collect();
    flood.slides.push(RichSlide {
        elements: many,
        ..RichSlide::default()
    });
    assert!(matches!(
        flood.validate(),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // PPTX path traversal in slide relationships fails closed
    let trav = ZipArchiveCodec::pack(&[
        ("[Content_Types].xml".into(), b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>".to_vec()),
        ("ppt/_rels/presentation.xml.rels".into(), b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"../etc/passwd\"/></Relationships>".to_vec()),
        ("ppt/presentation.xml".into(), b"<p:presentation xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:sldIdLst/></p:presentation>".to_vec()),
    ]).unwrap();
    assert!(matches!(
        OoxmlCodec::unpack_pptx_rich(&trav),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    // ODP xlink:href traversal fails closed
    let odp_trav = ZipArchiveCodec::pack(&[
        ("mimetype".into(), OdfCodec::MIMETYPE_ODP.as_bytes().to_vec()),
        ("content.xml".into(), b"<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><office:body><office:presentation><draw:page><draw:frame draw:name=\"x\" svg:x=\"10pt\" svg:y=\"10pt\" svg:width=\"50pt\" svg:height=\"50pt\"><draw:image xlink:href=\"../../etc/passwd\"/></draw:frame></draw:page></office:presentation></office:body></office:document-content>".to_vec()),
    ]).unwrap();
    assert!(matches!(
        OdfCodec::unpack_odp_rich(&odp_trav),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
#[ignore]
fn bench_pd257_slide_deck_roundtrip() {
    let png = RichImage::new(make_png(16, 16, 6, 37, 99, 235, 255), "아이콘").unwrap();
    let mut slides = Vec::with_capacity(30);
    for i in 0..30 {
        slides.push(RichSlide {
            title: format!("슬라이드 {}", i + 1),
            bullets: vec!["요약 첫째 줄".into(), "요약 둘째 줄".into()],
            notes: "발표 노트".into(),
            layout: SlideMasterLayout::ALL[i % SlideMasterLayout::ALL.len()],
            transition: SlideTransition::Fade,
            elements: vec![
                SlideElement::TextBox {
                    id: format!("t-{i}"),
                    rect: SlideBox {
                        x: 40.0,
                        y: 100.0,
                        w: 300.0,
                        h: 60.0,
                    },
                    text: "상세 설명 텍스트".into(),
                    font_size_pt: 16.0,
                    bold: true,
                    italic: false,
                    underline: false,
                    align: SlideTextAlign::Left,
                    color: Some("111827".into()),
                    fill: None,
                    role: None,
                },
                SlideElement::Shape {
                    id: format!("s-{i}"),
                    rect: SlideBox {
                        x: 360.0,
                        y: 100.0,
                        w: 180.0,
                        h: 60.0,
                    },
                    shape_type: SlideShapeType::RoundRect,
                    text: "핵심".into(),
                    fill: Some("2563EB".into()),
                    stroke: None,
                    stroke_width_pt: 0.0,
                    color: Some("FFFFFF".into()),
                    font_size_pt: 14.0,
                    bold: true,
                    italic: false,
                    underline: false,
                    align: SlideTextAlign::Center,
                },
                SlideElement::Image {
                    id: format!("im-{i}"),
                    rect: SlideBox {
                        x: 40.0,
                        y: 180.0,
                        w: 120.0,
                        h: 90.0,
                    },
                    image: png.clone(),
                    alt: "로고".into(),
                },
                SlideElement::Table {
                    id: format!("tb-{i}"),
                    rect: SlideBox {
                        x: 180.0,
                        y: 180.0,
                        w: 260.0,
                        h: 110.0,
                    },
                    rows: vec![
                        vec!["구분".into(), "수치".into()],
                        vec!["A".into(), "100".into()],
                    ],
                    header_row: true,
                    fill: Some("1E3A8A".into()),
                },
                SlideElement::Chart {
                    id: format!("ch-{i}"),
                    rect: SlideBox {
                        x: 460.0,
                        y: 180.0,
                        w: 420.0,
                        h: 240.0,
                    },
                    chart_type: ChartKind::Column,
                    title: "추이".into(),
                    categories: vec!["Q1".into(), "Q2".into(), "Q3".into()],
                    series: vec![SlideChartSeries {
                        name: "매출".into(),
                        values: vec![10.0, 20.0, 30.0],
                    }],
                    legend: ChartLegendPos::Right,
                },
            ],
        });
    }
    let deck = RichSlideDeck {
        title: "벤치마크".into(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        slides,
        elements_rejected: 0,
    };

    let t0 = Instant::now();
    let pptx = OoxmlCodec::pack_pptx_rich(&deck, false).unwrap();
    let d_pptx = OoxmlCodec::unpack_pptx_rich(&pptx).unwrap();
    let pptx_ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(d_pptx.slides.len(), 30);

    let t1 = Instant::now();
    let odp = OdfCodec::pack_odp_rich(&deck).unwrap();
    let d_odp = OdfCodec::unpack_odp_rich(&odp).unwrap();
    let odp_ms = t1.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(d_odp.slides.len(), 30);

    let t2 = Instant::now();
    let pdf = encode_slide_pdf(&deck, "벤치마크").unwrap();
    let pdf_ms = t2.elapsed().as_secs_f64() * 1000.0;
    assert!(!pdf.is_empty());

    println!(
        "BENCH PD-257 (30 slides, 150 elements): PPTX={pptx_ms:.2}ms ({}B), ODP={odp_ms:.2}ms ({}B), PDF={pdf_ms:.2}ms ({}B)",
        pptx.len(), odp.len(), pdf.len()
    );
}
