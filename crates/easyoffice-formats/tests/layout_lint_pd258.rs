//! PD-258: Layout lint unit/integration tests and release benchmark for documents and slide decks.

use std::time::Instant;

use easyoffice_formats::{
    deflate_zlib, lint_document, lint_legacy_slides, lint_slide_deck, CellMerge, IeeeCrc32,
    LayoutIssueKind, RichBlock, RichDocument, RichImage, RichPageFrame, RichPageMargins, RichPaper,
    RichParagraph, RichRun, RichSlide, RichSlideDeck, RichTable, SlideAspect, SlideBox,
    SlideElement, SlideMasterLayout, SlideOrientation, SlidePlaceholderRole, SlideShapeType,
    SlideTextAlign, SlideTransition, TableCellImage, MAX_LINT_ISSUES,
};

fn make_png(width: u32, height: u32) -> Vec<u8> {
    let chunk = |kind: &[u8; 4], data: &[u8]| {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    };
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let raw = vec![0u8; height as usize * (1 + width as usize * 3)];
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &deflate_zlib(&raw)));
    png.extend(chunk(b"IEND", &[]));
    png
}

fn plain_para(text: &str) -> RichParagraph {
    RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    }
}

fn textbox(id: &str, x: f64, y: f64, w: f64, h: f64, text: &str, pt: f64) -> SlideElement {
    SlideElement::TextBox {
        id: id.to_string(),
        rect: SlideBox { x, y, w, h },
        text: text.to_string(),
        font_size_pt: pt,
        bold: false,
        italic: false,
        underline: false,
        align: SlideTextAlign::Left,
        color: None,
        fill: None,
        role: None,
    }
}

fn rect_shape(id: &str, x: f64, y: f64, w: f64, h: f64, text: &str) -> SlideElement {
    SlideElement::Shape {
        id: id.to_string(),
        rect: SlideBox { x, y, w, h },
        shape_type: SlideShapeType::Rect,
        text: text.to_string(),
        fill: Some("DBEAFE".to_string()),
        stroke: Some("2563EB".to_string()),
        stroke_width_pt: 1.0,
        color: None,
        font_size_pt: 16.0,
        bold: false,
        italic: false,
        underline: false,
        align: SlideTextAlign::Center,
    }
}

#[test]
fn slide_lint_detects_stage_text_table_and_bullet_overflows() {
    let deck = RichSlideDeck {
        title: "넘침 검증".to_string(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        orientation: SlideOrientation::Landscape,
        slides: vec![RichSlide {
            title: "슬라이드1".to_string(),
            bullets: (1..=11).map(|i| format!("항목 {i}")).collect(),
            notes: String::new(),
            layout: SlideMasterLayout::TitleAndContent,
            transition: SlideTransition::Fade,
            ext: Default::default(),
            elements: vec![
                // Outside 960x540 stage right edge
                rect_shape("out-right", 850.0, 20.0, 130.0, 60.0, "정상"),
                // Wrapped Korean text > 3 lines in 40pt height
                textbox(
                    "tall-text",
                    40.0,
                    100.0,
                    140.0,
                    40.0,
                    "가나다라마바사아자차카타파하가나다라마바사아자차",
                    18.0,
                ),
                // Table 5 rows * 16 = 80 > 50 + 4
                SlideElement::Table {
                    id: "tbl-tall".to_string(),
                    rect: SlideBox {
                        x: 240.0,
                        y: 100.0,
                        w: 240.0,
                        h: 50.0,
                    },
                    rows: vec![
                        vec!["1".into()],
                        vec!["2".into()],
                        vec!["3".into()],
                        vec!["4".into()],
                        vec!["5".into()],
                    ],
                    header_row: true,
                    fill: None,
                },
            ],
        }],
        elements_rejected: 0,
    };

    let report = lint_slide_deck(&deck);
    assert_eq!(report.overflow_count, 4, "{:?}", report.issues);
    assert_eq!(report.overlap_count, 0);
    assert_eq!(report.empty_placeholder_count, 0);
    assert!(report
        .issues
        .iter()
        .any(|i| i.kind == LayoutIssueKind::Overflow && i.target_id == "out-right"));
    assert!(report
        .issues
        .iter()
        .any(|i| i.kind == LayoutIssueKind::Overflow && i.target_id == "tall-text"));
    assert!(report
        .issues
        .iter()
        .any(|i| i.kind == LayoutIssueKind::Overflow && i.target_id == "tbl-tall"));
    assert!(report
        .issues
        .iter()
        .any(|i| i.kind == LayoutIssueKind::Overflow && i.target_id == "slide-1-body"));
}

#[test]
fn slide_lint_detects_element_overlap_and_skips_full_background_shape() {
    let deck = RichSlideDeck {
        title: "겹침 검증".to_string(),
        is_slideshow: false,
        aspect: SlideAspect::Standard4x3, // 720 x 540 = 388,800 pt^2
        orientation: SlideOrientation::Landscape,
        slides: vec![RichSlide {
            title: "제목".to_string(),
            bullets: Vec::new(),
            notes: String::new(),
            layout: SlideMasterLayout::TitleAndContent,
            transition: SlideTransition::Fade,
            ext: Default::default(),
            elements: vec![
                // Covers 100% of stage area -> background shape excluded from overlap
                rect_shape("bg-full", 0.0, 0.0, 720.0, 540.0, ""),
                // Pair overlapping by 40 x 30 = 1200 pt^2
                rect_shape("box-a", 50.0, 80.0, 120.0, 80.0, "A"),
                rect_shape("box-b", 130.0, 130.0, 120.0, 80.0, "B"),
                // Grazing touch <= 1pt should NOT trigger overlap
                rect_shape("box-c", 250.5, 130.0, 100.0, 80.0, "C"),
            ],
        }],
        elements_rejected: 0,
    };

    let report = lint_slide_deck(&deck);
    assert_eq!(report.overlap_count, 1, "{:?}", report.issues);
    assert_eq!(report.issues[0].kind, LayoutIssueKind::Overlap);
    assert_eq!(report.issues[0].page_or_slide, 1);
    assert_eq!(report.issues[0].block_index, None);
    assert_eq!(report.issues[0].target_id, "box-a");
    assert!(report.issues[0].message.contains("box-b"));
    assert!(report.issues[0].message.contains("pt²"));
}

#[test]
fn slide_lint_detects_empty_placeholders_by_layout_and_textbox_content() {
    let deck = RichSlideDeck {
        title: "빈 영역 검증".to_string(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        orientation: SlideOrientation::Landscape,
        slides: vec![
            // Slide 1: TitleSlide without title, plus empty/prompt/template textboxes
            RichSlide {
                title: "   ".to_string(),
                bullets: Vec::new(),
                notes: String::new(),
                layout: SlideMasterLayout::TitleSlide,
                transition: SlideTransition::Fade,
                ext: Default::default(),
                elements: vec![
                    textbox("tb-empty", 40.0, 80.0, 300.0, 40.0, "   ", 18.0),
                    textbox(
                        "tb-prompt-ko",
                        40.0,
                        140.0,
                        300.0,
                        40.0,
                        "제목을 입력하려면 클릭하십시오",
                        18.0,
                    ),
                    textbox(
                        "tb-prompt-en",
                        40.0,
                        200.0,
                        300.0,
                        40.0,
                        "Click to add subtitle",
                        18.0,
                    ),
                    textbox(
                        "tb-tpl",
                        40.0,
                        260.0,
                        300.0,
                        40.0,
                        "작성자: {{author_name}}",
                        18.0,
                    ),
                ],
            },
            // Slide 2: TitleAndContent with empty title and no bullets/elements
            RichSlide {
                title: String::new(),
                bullets: vec!["   ".to_string()],
                notes: String::new(),
                layout: SlideMasterLayout::TitleAndContent,
                transition: SlideTransition::Fade,
                ext: Default::default(),
                elements: Vec::new(),
            },
            // Slide 3: Blank layout with nothing -> 0 issues
            RichSlide {
                title: String::new(),
                bullets: Vec::new(),
                notes: String::new(),
                layout: SlideMasterLayout::Blank,
                transition: SlideTransition::None,
                ext: Default::default(),
                elements: Vec::new(),
            },
            // Slide 4: Clean slide with Role::Title textbox supplying title -> 0 issues
            RichSlide {
                title: String::new(),
                bullets: Vec::new(),
                notes: String::new(),
                layout: SlideMasterLayout::SectionHeader,
                transition: SlideTransition::Fade,
                ext: Default::default(),
                elements: vec![SlideElement::TextBox {
                    id: "role-title".to_string(),
                    rect: SlideBox {
                        x: 40.0,
                        y: 40.0,
                        w: 400.0,
                        h: 50.0,
                    },
                    text: "섹션 제목".to_string(),
                    font_size_pt: 24.0,
                    bold: true,
                    italic: false,
                    underline: false,
                    align: SlideTextAlign::Left,
                    color: None,
                    fill: None,
                    role: Some(SlidePlaceholderRole::Title),
                }],
            },
        ],
        elements_rejected: 0,
    };

    let report = lint_slide_deck(&deck);
    assert_eq!(report.empty_placeholder_count, 7, "{:?}", report.issues);
    assert_eq!(report.overflow_count, 0);
    assert_eq!(report.overlap_count, 0);
    let ids: Vec<(&str, usize)> = report
        .issues
        .iter()
        .map(|i| (i.target_id.as_str(), i.page_or_slide))
        .collect();
    assert!(ids.contains(&("slide-1-title", 1)));
    assert!(ids.contains(&("tb-empty", 1)));
    assert!(ids.contains(&("tb-prompt-ko", 1)));
    assert!(ids.contains(&("tb-prompt-en", 1)));
    assert!(ids.contains(&("tb-tpl", 1)));
    assert!(ids.contains(&("slide-2-title", 2)));
    assert!(ids.contains(&("slide-2-body", 2)));
}

#[test]
fn legacy_slides_lint_reports_empty_and_bullet_overflow() {
    let slides = vec![
        (
            "정상 제목".to_string(),
            vec!["요약".to_string()],
            String::new(),
        ),
        (String::new(), Vec::new(), String::new()),
        (
            "장문 제목".to_string(),
            vec!["가".repeat(501)],
            String::new(),
        ),
    ];
    let report = lint_legacy_slides(&slides);
    assert_eq!(report.empty_placeholder_count, 2, "{:?}", report.issues);
    assert_eq!(report.overflow_count, 1, "{:?}", report.issues);
    assert_eq!(report.overlap_count, 0);
}

#[test]
fn document_lint_detects_overflow_overlap_and_empty_placeholders() {
    let mut big_img = RichImage::new(make_png(100, 100), "대형 그림").unwrap();
    // 500 pt wide > A4 content width with 20mm margins (595.3 - 113.4 = 481.9 pt)
    // Wait: RichImage::set_display_emu clamps at MAX_DISPLAY_WIDTH_EMU = 451.3 pt, so use narrower margins!
    big_img.set_display_emu(5_731_510, 2_540_000).unwrap();

    let cell_img = RichImage::new(make_png(40, 40), "셀 그림").unwrap();
    let mut overflow_tbl = RichTable::new(vec![
        vec!["항목".to_string(), "{{missing_field}}".to_string()],
        vec!["값".to_string(), "100".to_string()],
    ]);
    // Sum = 5000 + 5000 = 10000 twips = 500 pt > content_w_pt (~395.3 pt with 100pt margins)
    overflow_tbl.col_widths_twips = vec![5000, 5000];
    // Give cell image wider display than column width (even 100pt > 50pt or let's make col 1 = 1000 twips = 50pt)
    let mut wide_cell_img = cell_img;
    wide_cell_img
        .set_display_emu(12_700 * 120, 12_700 * 40)
        .unwrap(); // 120pt wide
    overflow_tbl.col_widths_twips = vec![8000, 1000]; // 400pt + 50pt = 450pt (> 395.3pt), col 1 is 50pt (< 120pt)
    overflow_tbl.cell_images.push(TableCellImage {
        row: 1,
        col: 0,
        image: wide_cell_img.clone(),
    });
    overflow_tbl.cell_images.push(TableCellImage {
        row: 1,
        col: 1,
        image: wide_cell_img,
    });
    // Overlapping merge pair inside table
    overflow_tbl.merges = vec![
        CellMerge {
            row: 0,
            col: 0,
            row_span: 2,
            col_span: 1,
        },
        CellMerge {
            row: 1,
            col: 0,
            row_span: 1,
            col_span: 2,
        },
    ];

    let empty_tbl = RichTable::new(vec![vec!["".to_string(), "   ".to_string()]]);

    let mut empty_h = plain_para("   ");
    empty_h.heading_level = 1;

    let long_token = format!("{{{{{}}}}}", "a".repeat(65)); // > 64 chars -> NOT flagged
    let valid_token = plain_para("수신자: {{recipient}}");
    let ignored_token = plain_para(&format!("정상: {long_token} / {{{{   }}}}"));

    let doc = RichDocument {
        title: "문서 린트".to_string(),
        blocks: vec![
            RichBlock::Paragraph(empty_h),
            RichBlock::Paragraph(valid_token),
            RichBlock::Paragraph(ignored_token),
            RichBlock::Image(big_img),
            RichBlock::Table(overflow_tbl),
            RichBlock::Table(empty_tbl),
        ],
        page_frame: RichPageFrame {
            header: vec![plain_para("상단 머리글")],
            footer: vec![plain_para("하단 꼬리말")],
            paper: Some(RichPaper::A4),
            margins: Some(RichPageMargins {
                top: 1000,
                bottom: 1000,
                header: 1400, // header > top -> Overlap
                footer: 1400, // footer > bottom -> Overlap
                left: 2000,   // 100 pt
                right: 2000,  // 100 pt => content_w_pt = 595.3 - 200 = 395.3 pt
            }),
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };

    let report = lint_document(&doc);
    // Expected:
    // Overflow: block-3 (image 451.3pt > 395.3pt), table-4 (sum 450pt > 395.3pt), table-4 cell image at (1,1) (120pt > 50pt) => 3
    // Overlap: header (> top), footer (> bottom), table-4 merges => 3
    // EmptyPlaceholder: block-0 (empty heading), block-1 ({{recipient}}), table-4 ({{missing_field}}), table-5 (empty table) => 4
    assert_eq!(report.overflow_count, 3, "{:?}", report.issues);
    assert_eq!(report.overlap_count, 3, "{:?}", report.issues);
    assert_eq!(report.empty_placeholder_count, 4, "{:?}", report.issues);
    assert!(!report.truncated);
    assert!(report
        .issues
        .iter()
        .any(|i| i.block_index == Some(0) && i.kind == LayoutIssueKind::EmptyPlaceholder));
    assert!(report
        .issues
        .iter()
        .any(|i| i.block_index == Some(3) && i.kind == LayoutIssueKind::Overflow));
    assert!(report
        .issues
        .iter()
        .any(|i| i.block_index.is_none() && i.target_id == "header"));
    assert!(report
        .issues
        .iter()
        .any(|i| i.block_index.is_none() && i.target_id == "footer"));
}

#[test]
fn lint_caps_hostile_report_at_max_lint_issues_and_marks_truncated() {
    let mut blocks = Vec::with_capacity(1_200);
    for _ in 0..1_200 {
        let mut h = plain_para("");
        h.heading_level = 2;
        blocks.push(RichBlock::Paragraph(h));
    }
    let doc = RichDocument {
        title: "적대적 대량 이슈".to_string(),
        blocks,
        ..RichDocument::default()
    };
    let report = lint_document(&doc);
    assert!(report.truncated);
    assert_eq!(report.issues.len(), MAX_LINT_ISSUES);
    assert_eq!(report.empty_placeholder_count, MAX_LINT_ISSUES);

    // Hostile slide with 100 overlapping boxes (4,950 pairs) stops cleanly at MAX_LINT_ISSUES
    let mut elements = Vec::with_capacity(100);
    for i in 0..100 {
        elements.push(rect_shape(&format!("el-{i}"), 40.0, 40.0, 80.0, 80.0, "X"));
    }
    let deck = RichSlideDeck {
        title: "적대적 슬라이드".to_string(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        orientation: SlideOrientation::Landscape,
        slides: vec![RichSlide {
            title: "제목".to_string(),
            bullets: Vec::new(),
            notes: String::new(),
            layout: SlideMasterLayout::TitleAndContent,
            transition: SlideTransition::Fade,
            ext: Default::default(),
            elements,
        }],
        elements_rejected: 0,
    };
    let s_report = lint_slide_deck(&deck);
    assert!(s_report.truncated);
    assert_eq!(s_report.issues.len(), MAX_LINT_ISSUES);
    assert_eq!(s_report.overlap_count, MAX_LINT_ISSUES);
}

#[test]
#[ignore]
fn benchmark_layout_lint_release() {
    // 1. 50 slides * 80 non-overlapping elements = 4,000 elements
    let mut slides = Vec::with_capacity(50);
    for s_idx in 0..50 {
        let mut elements = Vec::with_capacity(80);
        for row in 0..8 {
            for col in 0..10 {
                let id = format!("s{s_idx}-r{row}-c{col}");
                elements.push(textbox(
                    &id,
                    10.0 + col as f64 * 92.0,
                    40.0 + row as f64 * 58.0,
                    84.0,
                    48.0,
                    "정상 항목 텍스트",
                    12.0,
                ));
            }
        }
        slides.push(RichSlide {
            title: format!("슬라이드 {}", s_idx + 1),
            bullets: Vec::new(),
            notes: String::new(),
            layout: SlideMasterLayout::TitleAndContent,
            transition: SlideTransition::Fade,
            ext: Default::default(),
            elements,
        });
    }
    let deck = RichSlideDeck {
        title: "벤치마크 덱".to_string(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        orientation: SlideOrientation::Landscape,
        slides,
        elements_rejected: 0,
    };

    // 2. 10,000-block RichDocument
    let mut blocks = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        if i % 50 == 0 {
            let mut tbl = RichTable::new(vec![
                vec!["항목".into(), "값".into()],
                vec![format!("줄 {i}"), "100".into()],
            ]);
            tbl.col_widths_twips = vec![2000, 2000];
            blocks.push(RichBlock::Table(tbl));
        } else {
            blocks.push(RichBlock::Paragraph(plain_para(
                "성능 측정을 위한 일반 본문 문단입니다. 레이아웃 검사를 수행합니다.",
            )));
        }
    }
    let doc = RichDocument {
        title: "벤치마크 문서".to_string(),
        blocks,
        ..RichDocument::default()
    };

    let iters = 20;
    let t0 = Instant::now();
    for _ in 0..iters {
        let r = lint_slide_deck(&deck);
        assert_eq!(r.issues.len(), 0);
    }
    let slide_ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(iters);

    let t1 = Instant::now();
    for _ in 0..iters {
        let r = lint_document(&doc);
        assert_eq!(r.issues.len(), 0);
    }
    let doc_ms = t1.elapsed().as_secs_f64() * 1000.0 / f64::from(iters);

    println!(
        "BENCH PD-258 layout_lint: slides(50x80=4000 els)={slide_ms:.3}ms/iter, doc(10000 blocks)={doc_ms:.3}ms/iter"
    );
}
