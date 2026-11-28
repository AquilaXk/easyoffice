//! PD-258: Headless multi-page PNG screenshot CLI tests for `DOCX`, `HWPX`, `PPTX`, `XLSX`, and `PDF`.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use easyoffice_formats::{
    deflate_zlib, ChartKind, ChartLegendPos, DocumentFormat, IeeeCrc32, OoxmlCodec, RichBlock,
    RichDocument, RichImage, RichParagraph, RichRun, RichSlide, RichSlideDeck, RichTable,
    SheetStyling, SlideAspect, SlideBox, SlideChartSeries, SlideElement, SlideMasterLayout,
    SlideOrientation, SlideShapeType, SlideTextAlign, SlideTransition, UniversalFormatConverter,
};

fn hand_written_with_xref(objects: &[(u32, String)], trailer_extra: &str) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let max_id = objects.iter().map(|(id, _)| *id).max().unwrap_or(0);
    let mut offsets = vec![0usize; max_id as usize + 1];
    for (id, body) in objects {
        offsets[*id as usize] = pdf.len();
        let _ = writeln!(&mut pdf, "{id} 0 obj\n{body}\nendobj");
    }
    let startxref = pdf.len();
    let _ = writeln!(&mut pdf, "xref\n0 {}", max_id + 1);
    let _ = writeln!(&mut pdf, "0000000000 65535 f ");
    for off in offsets.iter().skip(1) {
        let _ = writeln!(&mut pdf, "{off:010} 00000 n ");
    }
    let _ = write!(
        &mut pdf,
        "trailer\n<< /Size {} /Root 1 0 R {trailer_extra}>>\nstartxref\n{startxref}\n%%EOF\n",
        max_id + 1
    );
    pdf
}

const BIN: &str = env!("CARGO_BIN_EXE_EasyOffice");
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn run_cli(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("spawn EasyOffice binary")
}

fn make_png_bytes(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
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
    let mut row = Vec::with_capacity(1 + width as usize * 3);
    row.push(0);
    for _ in 0..width {
        row.extend_from_slice(&[r, g, b]);
    }
    let mut raw = Vec::with_capacity(row.len() * height as usize);
    for _ in 0..height {
        raw.extend_from_slice(&row);
    }
    let mut png = Vec::from(PNG_MAGIC);
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &deflate_zlib(&raw)));
    png.extend(chunk(b"IEND", &[]));
    png
}

fn decode_png_metrics(path: &Path) -> (u32, u32, usize, usize, Vec<u8>) {
    let bytes = fs::read(path).expect("read PNG file");
    assert!(
        bytes.starts_with(&PNG_MAGIC),
        "not a PNG: {}",
        path.display()
    );
    let pixmap = tiny_skia::Pixmap::decode_png(&bytes).expect("valid PNG decode");
    let w = pixmap.width();
    let h = pixmap.height();
    let total_non_white = pixmap
        .pixels()
        .iter()
        .filter(|p| (p.red(), p.green(), p.blue()) != (255, 255, 255))
        .count();
    let bottom_start = (h as usize * 3) / 4 * (w as usize);
    let bottom_non_white = pixmap.pixels()[bottom_start..]
        .iter()
        .filter(|p| (p.red(), p.green(), p.blue()) != (255, 255, 255))
        .count();
    (w, h, total_non_white, bottom_non_white, bytes)
}

fn parse_stdout_json(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout.clone()).expect("UTF-8 stdout");
    let line = stdout.trim();
    assert!(!line.contains('\n'), "stdout must be 1 line JSON: {line}");
    serde_json::from_str(line).expect("valid JSON summary")
}

fn plain_para(text: &str, heading: u8, page_break_before: bool) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        heading_level: heading,
        page_break_before,
        ..RichParagraph::default()
    })
}

fn build_test_pptx() -> Vec<u8> {
    let img = RichImage::new(make_png_bytes(40, 30, 30, 100, 220), "표지 그림").unwrap();
    let deck = RichSlideDeck {
        title: "스크린샷 테스트 덱".into(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        orientation: SlideOrientation::Landscape,
        slides: vec![
            RichSlide {
                title: "1페이지 분기 성과 보고".into(),
                bullets: vec!["핵심 매출 성장".into(), "품질 지표 개선".into()],
                notes: String::new(),
                layout: SlideMasterLayout::TitleAndContent,
                transition: SlideTransition::Fade,
                ext: Default::default(),
                elements: vec![
                    SlideElement::Shape {
                        id: "sh1".into(),
                        rect: SlideBox {
                            x: 40.0,
                            y: 90.0,
                            w: 220.0,
                            h: 80.0,
                        },
                        shape_type: SlideShapeType::RoundRect,
                        text: "도형 강조 블록".into(),
                        fill: Some("1D4ED8".into()),
                        stroke: Some("1E3A8A".into()),
                        stroke_width_pt: 2.0,
                        color: Some("FFFFFF".into()),
                        font_size_pt: 16.0,
                        bold: true,
                        italic: false,
                        underline: false,
                        align: SlideTextAlign::Center,
                    },
                    SlideElement::Image {
                        id: "im1".into(),
                        rect: SlideBox {
                            x: 40.0,
                            y: 200.0,
                            w: 160.0,
                            h: 120.0,
                        },
                        image: img,
                        alt: "파란 이미지".into(),
                    },
                    SlideElement::Table {
                        id: "tb1".into(),
                        rect: SlideBox {
                            x: 240.0,
                            y: 200.0,
                            w: 240.0,
                            h: 110.0,
                        },
                        rows: vec![
                            vec!["부서".into(), "점수".into()],
                            vec!["플랫폼".into(), "98".into()],
                        ],
                        header_row: true,
                        fill: Some("DBEAFE".into()),
                    },
                    SlideElement::Chart {
                        id: "ch1".into(),
                        rect: SlideBox {
                            x: 520.0,
                            y: 120.0,
                            w: 380.0,
                            h: 260.0,
                        },
                        chart_type: ChartKind::Column,
                        title: "분기별 추이".into(),
                        categories: vec!["1Q".into(), "2Q".into(), "3Q".into()],
                        series: vec![SlideChartSeries {
                            name: "실적".into(),
                            values: vec![40.0, 75.0, 120.0],
                        }],
                        legend: ChartLegendPos::Bottom,
                    },
                ],
            },
            RichSlide {
                title: "2페이지 세부 아키텍처".into(),
                bullets: vec!["다중 쪽 렌더링 검증".into()],
                notes: String::new(),
                layout: SlideMasterLayout::TitleAndContent,
                transition: SlideTransition::Push,
                ext: Default::default(),
                elements: vec![SlideElement::Shape {
                    id: "sh2".into(),
                    rect: SlideBox {
                        x: 120.0,
                        y: 150.0,
                        w: 320.0,
                        h: 160.0,
                    },
                    shape_type: SlideShapeType::Ellipse,
                    text: "원형 노드".into(),
                    fill: Some("DC2626".into()),
                    stroke: None,
                    stroke_width_pt: 1.0,
                    color: Some("FFFFFF".into()),
                    font_size_pt: 20.0,
                    bold: true,
                    italic: false,
                    underline: false,
                    align: SlideTextAlign::Center,
                }],
            },
            RichSlide {
                title: "3페이지 결론 및 계획".into(),
                bullets: vec!["최종 점검 완료".into()],
                notes: String::new(),
                layout: SlideMasterLayout::TitleAndContent,
                transition: SlideTransition::Zoom,
                ext: Default::default(),
                elements: Vec::new(),
            },
        ],
        elements_rejected: 0,
    };
    UniversalFormatConverter::encode_rich_slides(&deck, DocumentFormat::Pptx).unwrap()
}

fn build_multipage_rich_doc(paragraphs_count: usize) -> RichDocument {
    let mut blocks = vec![
        plain_para("다중 쪽 문서 제목 (한글 및 English)", 1, false),
        RichBlock::Table(RichTable::new(vec![
            vec!["항목".into(), "수치".into()],
            vec!["매출".into(), "1,250".into()],
        ])),
    ];
    for i in 0..paragraphs_count {
        let brk = i == paragraphs_count / 2;
        blocks.push(plain_para(
            &format!("{i}번째 본문 문단입니다. 다중 쪽 레이아웃과 한글 글꼴 렌더링을 확인합니다."),
            0,
            brk,
        ));
    }
    RichDocument {
        title: "다중 쪽 테스트".into(),
        blocks,
        ..RichDocument::default()
    }
}

fn build_multipage_xlsx() -> Vec<u8> {
    let s1 = (
        "매출요약".to_string(),
        vec![
            (0, 0, "항목".to_string()),
            (0, 1, "값".to_string()),
            (1, 0, "1분기".to_string()),
            (1, 1, "100".to_string()),
            (2, 0, "2분기".to_string()),
            (2, 1, "250".to_string()),
            (3, 0, "합계".to_string()),
            (3, 1, "=SUM(B2:B3)".to_string()),
        ],
    );
    let s2 = (
        "비용내역".to_string(),
        vec![
            (0, 0, "부서".to_string()),
            (0, 1, "금액".to_string()),
            (1, 0, "개발팀".to_string()),
            (1, 1, "500".to_string()),
            (2, 0, "운영팀".to_string()),
            (2, 1, "320".to_string()),
        ],
    );
    let mut st1 = SheetStyling::default();
    st1.page.grid_lines = true;
    let mut st2 = SheetStyling::default();
    st2.page.grid_lines = true;
    OoxmlCodec::pack_xlsx_multi_styled(
        &[s1, s2],
        &[Default::default(), Default::default()],
        &[st1, st2],
    )
    .unwrap()
}

fn build_two_page_korean_and_bottom_pdf() -> Vec<u8> {
    // Page 1: Korean text at top + solid dark rectangle & label near bottom (y=30..65 on 612x792)
    // Page 2: Distinct text + shape at center
    let s1 = concat!(
        "BT /F1 28 Tf 50 720 Td <AC00B098B2E4B77CB9C8BC14> Tj ET\n",
        "q 0.1 0.2 0.7 rg 50 30 500 35 re f Q\n",
        "BT /F2 18 Tf 60 90 Td (Bottom Region Marker) Tj ET\n"
    );
    let s2 = "q 0.8 0.1 0.1 rg 150 300 300 180 re f Q\nBT /F2 22 Tf 160 520 Td (Second PDF Page) Tj ET\n";
    let objects = vec![
        (1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R /Resources << /Font << /F1 7 0 R /F2 10 0 R >> >> >>".to_string(),
        ),
        (
            4,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F2 10 0 R >> >> >>".to_string(),
        ),
        (5, format!("<< /Length {} >>\nstream\n{s1}endstream", s1.len())),
        (6, format!("<< /Length {} >>\nstream\n{s2}endstream", s2.len())),
        (
            7,
            "<< /Type /Font /Subtype /Type0 /BaseFont /HYGoThic-Medium-UniKS-UCS2-H /Encoding /UniKS-UCS2-H /DescendantFonts [8 0 R] >>".to_string(),
        ),
        (
            8,
            "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /HYGoThic-Medium /CIDSystemInfo << /Registry (Adobe) /Ordering (Korea1) /Supplement 1 >> /FontDescriptor 9 0 R /DW 1000 >>".to_string(),
        ),
        (
            9,
            "<< /Type /FontDescriptor /FontName /HYGoThic-Medium /Flags 4 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 880 /StemV 80 >>".to_string(),
        ),
        (10, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()),
    ];
    hand_written_with_xref(&objects, "")
}

#[test]
fn pptx_multipage_screenshot_renders_16x9_korean_charts_and_page_selection() {
    let dir = tempfile::tempdir().unwrap();
    let pptx_path = dir.path().join("deck.pptx");
    fs::write(&pptx_path, build_test_pptx()).unwrap();

    let out_all = dir.path().join("out_all");
    let out = run_cli(&[
        "--render-pages",
        pptx_path.to_str().unwrap(),
        "--out-dir",
        out_all.to_str().unwrap(),
        "--dpi",
        "144",
    ]);
    let json = parse_stdout_json(&out);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["format"], "pptx");
    assert_eq!(json["pageCount"], 3);
    assert_eq!(json["dpi"], 144);
    assert_eq!(json["renderedCount"], 3);
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(files[0]["fileName"], "page-0001.png");
    assert_eq!(files[1]["fileName"], "page-0002.png");
    assert_eq!(files[2]["fileName"], "page-0003.png");

    let (w1, h1, nw1, _, b1) = decode_png_metrics(&out_all.join("page-0001.png"));
    let (w2, h2, nw2, _, b2) = decode_png_metrics(&out_all.join("page-0002.png"));
    let (w3, h3, nw3, _, b3) = decode_png_metrics(&out_all.join("page-0003.png"));
    assert_eq!((w1, h1), (1920, 1080));
    assert_eq!((w2, h2), (1920, 1080));
    assert_eq!((w3, h3), (1920, 1080));
    assert!(nw1 > 10_000, "slide 1 non-white={nw1}");
    assert!(nw2 > 10_000, "slide 2 non-white={nw2}");
    assert!(nw3 > 1_000, "slide 3 non-white={nw3}");
    assert_ne!(b1, b2);
    assert_ne!(b2, b3);

    // Alias --screenshot + --pages 1,3 + --dpi 72
    let out_sub = dir.path().join("out_sub");
    let out2 = run_cli(&[
        "--screenshot",
        pptx_path.to_str().unwrap(),
        "--out-dir",
        out_sub.to_str().unwrap(),
        "--pages",
        "1,3",
        "--dpi",
        "72",
    ]);
    let json2 = parse_stdout_json(&out2);
    assert_eq!(json2["pageCount"], 3);
    assert_eq!(json2["renderedCount"], 2);
    assert_eq!(json2["dpi"], 72);
    let files2 = json2["files"].as_array().unwrap();
    assert_eq!(files2[0]["page"], 1);
    assert_eq!(files2[0]["fileName"], "page-0001.png");
    assert_eq!(files2[0]["widthPx"], 960);
    assert_eq!(files2[0]["heightPx"], 540);
    assert_eq!(files2[1]["page"], 3);
    assert_eq!(files2[1]["fileName"], "page-0003.png");
    assert!(!out_sub.join("page-0002.png").exists());
    let (sw, sh, snw, _, _) = decode_png_metrics(&out_sub.join("page-0001.png"));
    assert_eq!((sw, sh), (960, 540));
    assert!(snw > 2_500);
}

#[test]
fn hwpx_and_docx_multipage_screenshots_render_all_pages_non_blank() {
    let dir = tempfile::tempdir().unwrap();
    let doc = build_multipage_rich_doc(48);

    // 1. HWPX
    let hwpx_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
    let hwpx_path = dir.path().join("multi.hwpx");
    fs::write(&hwpx_path, &hwpx_bytes).unwrap();
    let out_hwpx = dir.path().join("out_hwpx");
    let json_hwpx = parse_stdout_json(&run_cli(&[
        "--render-pages",
        hwpx_path.to_str().unwrap(),
        "--out-dir",
        out_hwpx.to_str().unwrap(),
    ]));
    assert_eq!(json_hwpx["format"], "hwpx");
    let hwpx_pages = json_hwpx["pageCount"].as_u64().unwrap();
    assert!(
        hwpx_pages >= 2,
        "expected >= 2 HWPX pages, got {hwpx_pages}"
    );
    assert_eq!(json_hwpx["renderedCount"], hwpx_pages);
    let (hw1, hh1, hnw1, _, hb1) = decode_png_metrics(&out_hwpx.join("page-0001.png"));
    let (hw2, hh2, hnw2, _, hb2) = decode_png_metrics(&out_hwpx.join("page-0002.png"));
    assert!(hw1 > 1000 && hh1 > hw1);
    assert_eq!((hw1, hh1), (hw2, hh2));
    assert!(hnw1 > 2_000 && hnw2 > 1_000);
    assert_ne!(hb1, hb2);

    // 2. DOCX
    let docx_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let docx_path = dir.path().join("multi.docx");
    fs::write(&docx_path, &docx_bytes).unwrap();
    let out_docx = dir.path().join("out_docx");
    let json_docx = parse_stdout_json(&run_cli(&[
        "--render-pages",
        docx_path.to_str().unwrap(),
        "--out-dir",
        out_docx.to_str().unwrap(),
    ]));
    assert_eq!(json_docx["format"], "docx");
    let docx_pages = json_docx["pageCount"].as_u64().unwrap();
    assert!(
        docx_pages >= 2,
        "expected >= 2 DOCX pages, got {docx_pages}"
    );
    let (_, _, dnw1, _, db1) = decode_png_metrics(&out_docx.join("page-0001.png"));
    let (_, _, dnw2, _, db2) = decode_png_metrics(&out_docx.join("page-0002.png"));
    assert!(dnw1 > 2_000 && dnw2 > 1_000);
    assert_ne!(db1, db2);
}

#[test]
fn xlsx_and_pdf_multipage_screenshots_render_sheets_korean_and_full_bottom() {
    let dir = tempfile::tempdir().unwrap();

    // 1. XLSX (2 sheets -> 2 PDF pages -> 2 PNGs)
    let xlsx_path = dir.path().join("book.xlsx");
    fs::write(&xlsx_path, build_multipage_xlsx()).unwrap();
    let out_xlsx = dir.path().join("out_xlsx");
    let json_xlsx = parse_stdout_json(&run_cli(&[
        "--render-pages",
        xlsx_path.to_str().unwrap(),
        "--out-dir",
        out_xlsx.to_str().unwrap(),
    ]));
    assert_eq!(json_xlsx["format"], "xlsx");
    assert_eq!(json_xlsx["pageCount"], 2);
    assert_eq!(json_xlsx["renderedCount"], 2);
    let (_, _, xnw1, _, xb1) = decode_png_metrics(&out_xlsx.join("page-0001.png"));
    let (_, _, xnw2, _, xb2) = decode_png_metrics(&out_xlsx.join("page-0002.png"));
    assert!(xnw1 > 2_000 && xnw2 > 2_000);
    assert_ne!(xb1, xb2);

    // 2. PDF (Korean CID + bottom content NOT clipped)
    let pdf_path = dir.path().join("full.pdf");
    fs::write(&pdf_path, build_two_page_korean_and_bottom_pdf()).unwrap();
    let out_pdf = dir.path().join("out_pdf");
    let json_pdf = parse_stdout_json(&run_cli(&[
        "--render-pages",
        pdf_path.to_str().unwrap(),
        "--out-dir",
        out_pdf.to_str().unwrap(),
        "--dpi",
        "144",
    ]));
    assert_eq!(json_pdf["format"], "pdf");
    assert_eq!(json_pdf["pageCount"], 2);
    assert_eq!(json_pdf["renderedCount"], 2);
    let (pw1, ph1, pnw1, p_bottom1, pb1) = decode_png_metrics(&out_pdf.join("page-0001.png"));
    let (pw2, ph2, pnw2, _, pb2) = decode_png_metrics(&out_pdf.join("page-0002.png"));
    assert_eq!((pw1, ph1), (1224, 1584)); // 612x792 * (144/72)
    assert_eq!((pw2, ph2), (1224, 1584));
    assert!(pnw1 > 5_000 && pnw2 > 5_000);
    assert!(
        p_bottom1 > 10_000,
        "bottom region must be rendered without clipping: {p_bottom1}"
    );
    assert_ne!(pb1, pb2);
}

#[test]
fn screenshot_cli_fails_closed_on_invalid_args_ranges_and_hostile_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let pdf_path = dir.path().join("two.pdf");
    fs::write(&pdf_path, build_two_page_korean_and_bottom_pdf()).unwrap();

    // Invalid DPI / pages / missing out-dir
    for bad_args in [
        vec!["--render-pages", pdf_path.to_str().unwrap()],
        vec!["--out-dir", dir.path().to_str().unwrap()],
        vec![
            "--render-pages",
            pdf_path.to_str().unwrap(),
            "--out-dir",
            dir.path().join("bad_dpi1").to_str().unwrap(),
            "--dpi",
            "10",
        ],
        vec![
            "--render-pages",
            pdf_path.to_str().unwrap(),
            "--out-dir",
            dir.path().join("bad_dpi2").to_str().unwrap(),
            "--dpi",
            "301",
        ],
        vec![
            "--render-pages",
            pdf_path.to_str().unwrap(),
            "--out-dir",
            dir.path().join("bad_pg0").to_str().unwrap(),
            "--pages",
            "0",
        ],
        vec![
            "--render-pages",
            pdf_path.to_str().unwrap(),
            "--out-dir",
            dir.path().join("bad_rev").to_str().unwrap(),
            "--pages",
            "2-1",
        ],
        vec![
            "--render-pages",
            pdf_path.to_str().unwrap(),
            "--out-dir",
            dir.path().join("bad_oob").to_str().unwrap(),
            "--pages",
            "3",
        ],
    ] {
        let res = run_cli(&bad_args);
        assert!(!res.status.success(), "expected failure for {bad_args:?}");
    }

    // Corrupt PDF fails closed
    let corrupt_path = dir.path().join("corrupt.pdf");
    fs::write(&corrupt_path, b"%PDF-1.7\ngarbage").unwrap();
    let out_corrupt = dir.path().join("out_corrupt");
    let res_corrupt = run_cli(&[
        "--render-pages",
        corrupt_path.to_str().unwrap(),
        "--out-dir",
        out_corrupt.to_str().unwrap(),
    ]);
    assert!(!res_corrupt.status.success());

    // Hidden / symlink-to-dotfile refused fail-closed
    let secret = dir.path().join(".env");
    fs::write(&secret, b"SECRET=1").unwrap();
    let out_sec = dir.path().join("out_sec");
    assert!(!run_cli(&[
        "--render-pages",
        secret.to_str().unwrap(),
        "--out-dir",
        out_sec.to_str().unwrap(),
    ])
    .status
    .success());

    #[cfg(unix)]
    {
        let sym = dir.path().join("innocent.pdf");
        std::os::unix::fs::symlink(&secret, &sym).unwrap();
        assert!(!run_cli(&[
            "--render-pages",
            sym.to_str().unwrap(),
            "--out-dir",
            out_sec.to_str().unwrap(),
        ])
        .status
        .success());

        // Pre-planted symlink in output dir must not be overwritten
        let out_sym = dir.path().join("out_sym");
        fs::create_dir_all(&out_sym).unwrap();
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, b"SAFE").unwrap();
        std::os::unix::fs::symlink(&victim, out_sym.join("page-0001.png")).unwrap();
        let res_sym = run_cli(&[
            "--render-pages",
            pdf_path.to_str().unwrap(),
            "--out-dir",
            out_sym.to_str().unwrap(),
            "--pages",
            "1",
        ]);
        assert!(!res_sym.status.success());
        assert_eq!(fs::read(&victim).unwrap(), b"SAFE");
    }

    // Also ensure page count > 200 without --pages is refused
    let mut big_objects = vec![(1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string())];
    let kids: Vec<String> = (3..204).map(|id| format!("{id} 0 R")).collect();
    big_objects.push((
        2,
        format!("<< /Type /Pages /Kids [{}] /Count 201 >>", kids.join(" ")),
    ));
    for id in 3..204 {
        big_objects.push((
            id,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string(),
        ));
    }
    let big_pdf_path = dir.path().join("too_many.pdf");
    fs::write(&big_pdf_path, hand_written_with_xref(&big_objects, "")).unwrap();
    let out_big = dir.path().join("out_big");
    let res_big = run_cli(&[
        "--render-pages",
        big_pdf_path.to_str().unwrap(),
        "--out-dir",
        out_big.to_str().unwrap(),
    ]);
    assert!(!res_big.status.success());

    // Keep PathBuf in scope/used
    let _: PathBuf = dir.path().to_path_buf();
}
