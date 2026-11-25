//! Integration, security, CLI, and performance tests for structured Readback & Data Checks (`PD-248`).

use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use easyoffice_app::cli_args::InspectArgs;
use easyoffice_app::inspect_cli::{run_inspect_cli, run_inspect_cli_with_limit};
use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    base64_codec, deflate_zlib, CellBox, CellLink, CellStyle, CellValueKind, Cmp, DataValidation,
    DocumentFormat, DvAlert, DvKind, IeeeCrc32, PdfSerializer, RichAlignment, RichBlock,
    RichDocument, RichImage, RichListKind, RichPageFrame, RichPageMargins, RichPaper,
    RichParagraph, RichRun, RichTable, SheetRules, SheetStyling, SheetTable, StrictJsonValue,
    TableSlicer, UniversalDocumentModel, UniversalFormatConverter, CUSTOM_BASE,
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
    let mut ihdr = Vec::new();
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

fn parse_obj(json: &str) -> Vec<(String, StrictJsonValue)> {
    match StrictJsonValue::parse(json).expect("valid JSON response") {
        StrictJsonValue::Object(entries) => entries,
        other => panic!("expected root JSON object, got {other:?}"),
    }
}

fn field<'a>(entries: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(entries, key)
        .unwrap_or_else(|| panic!("missing JSON field '{key}' in {entries:?}"))
}

#[test]
fn xlsx_file_and_session_readback_report_formulas_formats_slicers_spills_and_dv_violations() {
    let sheets = vec![
        (
            "매출".to_string(),
            vec![
                (0, 0, "품목".to_string()),
                (0, 1, "금액".to_string()),
                (1, 0, "123".to_string()),
                (1, 1, "20.5".to_string()),
                (2, 0, "노트북".to_string()),
                (2, 1, "22".to_string()),
                (3, 0, "합계".to_string()),
                (3, 1, "=SUM(B2:B3)".to_string()),
                (4, 0, "오류1".to_string()),
                (4, 1, "=1/0".to_string()),
                (5, 0, "오류2".to_string()),
                (5, 1, "=NO_SUCH_FN(1)".to_string()),
                (6, 0, "유효성위반".to_string()),
                (6, 1, "250".to_string()),
                (0, 3, "=SEQUENCE(2,2)".to_string()),
            ],
        ),
        ("보조".to_string(), vec![(0, 0, "정상".to_string())]),
    ];

    let mut st = SheetStyling {
        num_fmts: Arc::new(vec!["#,##0.00".to_string()]),
        ..SheetStyling::default()
    };
    st.set_cell_kind(1, 0, "123", CellValueKind::Text);
    st.cells.insert(
        (3, 1),
        CellStyle {
            num_fmt: CUSTOM_BASE,
            ..CellStyle::default()
        },
    );
    st.links.insert(
        (2, 0),
        CellLink {
            target: "https://example.com/item".to_string(),
            tip: String::new(),
        },
    );
    let mut table = SheetTable::new(
        "SalesTable",
        CellBox {
            row0: 0,
            col0: 0,
            row1: 2,
            col1: 1,
        },
    );
    table.columns = vec!["품목".to_string(), "금액".to_string()];
    table.style = Some("TableStyleMedium2".to_string());
    table.slicers.push(TableSlicer {
        col: 0,
        caption: "품목 필터".to_string(),
        left: 240,
        top: 20,
        width: 192,
        height: 220,
        multi_select: true,
    });
    st.tables.push(table);
    st.rules = SheetRules {
        dv: vec![DataValidation {
            ranges: vec![CellBox {
                row0: 6,
                col0: 1,
                row1: 6,
                col1: 1,
            }],
            kind: DvKind::Whole,
            cmp: Cmp::Between,
            a: "1".to_string(),
            b: "100".to_string(),
            allow_blank: true,
            show_dropdown: false,
            alert: DvAlert::Stop,
            show_error: true,
            error_title: "범위 초과".to_string(),
            error: "1~100 사이여야 합니다.".to_string(),
            show_prompt: false,
            prompt_title: String::new(),
            prompt: String::new(),
        }],
        ..SheetRules::default()
    };

    let xlsx_bytes = easyoffice_formats::OoxmlCodec::pack_xlsx_multi_styled(
        &sheets,
        &[Default::default(), Default::default()],
        &[st, SheetStyling::default()],
    )
    .expect("pack styled xlsx");
    let b64 = base64_codec::encode(&xlsx_bytes);

    let resp = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"action":"inspect","fileName":"매출점검.xlsx","sourceBase64":"{b64}","range":"'매출'!A1:E7"}}"#
        ),
    )
    .expect("inspect xlsx");
    let root = parse_obj(&resp);
    assert_eq!(field(&root, "format").as_str(), Some("xlsx"));
    assert_eq!(field(&root, "kind").as_str(), Some("workbook"));

    let sheet_arr = field(&root, "sheets").as_array().unwrap();
    assert_eq!(sheet_arr.len(), 2);
    let s0 = sheet_arr[0].as_object().unwrap();
    assert_eq!(field(s0, "name").as_str(), Some("매출"));
    assert_eq!(field(s0, "tables").as_array().unwrap().len(), 1);
    assert_eq!(field(s0, "slicers").as_array().unwrap().len(), 1);
    let err_counts = field(s0, "errorCounts").as_object().unwrap();
    assert_eq!(field(err_counts, "total").as_u64(), Some(2));
    let by_code = field(err_counts, "byCode").as_object().unwrap();
    assert_eq!(field(by_code, "#DIV/0!").as_u64(), Some(1));
    assert_eq!(field(by_code, "#NAME?").as_u64(), Some(1));

    let rr = field(&root, "rangeReadback").as_object().unwrap();
    assert_eq!(field(rr, "sheet").as_str(), Some("매출"));
    assert_eq!(field(rr, "range").as_str(), Some("A1:E7"));
    let cells = field(rr, "cells").as_array().unwrap();

    let a2 = cells
        .iter()
        .find_map(|v| {
            let o = v.as_object()?;
            (field(o, "cell").as_str() == Some("A2")).then_some(o)
        })
        .expect("A2 present");
    assert_eq!(field(a2, "explicitKind").as_str(), Some("s"));
    assert_eq!(field(a2, "valueType").as_str(), Some("text"));
    assert_eq!(field(a2, "display").as_str(), Some("123"));

    let b4 = cells
        .iter()
        .find_map(|v| {
            let o = v.as_object()?;
            (field(o, "cell").as_str() == Some("B4")).then_some(o)
        })
        .expect("B4 present");
    assert_eq!(field(b4, "formula").as_str(), Some("=SUM(B2:B3)"));
    assert_eq!(field(b4, "display").as_str(), Some("42.50"));
    assert_eq!(field(b4, "numFmt").as_str(), Some("#,##0.00"));

    let e2 = cells
        .iter()
        .find_map(|v| {
            let o = v.as_object()?;
            (field(o, "cell").as_str() == Some("E2")).then_some(o)
        })
        .expect("spilled cell E2 present");
    assert_eq!(field(e2, "spillAnchor").as_str(), Some("D1"));
    assert_eq!(field(e2, "display").as_str(), Some("4"));

    let checks = field(&root, "checks").as_object().unwrap();
    assert_eq!(field(checks, "totalErrorCells").as_u64(), Some(2));
    let dv_v = field(checks, "validationViolations").as_array().unwrap();
    assert_eq!(dv_v.len(), 1);
    let dv0 = dv_v[0].as_object().unwrap();
    assert_eq!(field(dv0, "cell").as_str(), Some("B7"));
    assert_eq!(field(dv0, "alert").as_str(), Some("stop"));

    // Live stateful RecalcSession inspection + owner isolation
    let opened = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        r#"{"sessionAction":"open","sheets":["Sheet1"]}"#,
        "win-owner-1",
    )
    .unwrap();
    let sid = field(&parse_obj(&opened), "session")
        .as_str()
        .unwrap()
        .to_string();
    EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        &format!(
            r#"{{"sessionAction":"apply","session":"{sid}","changes":[{{"sheet":"Sheet1","cell":"A1","value":"10"}},{{"sheet":"Sheet1","cell":"A2","value":"=A1*3"}},{{"sheet":"Sheet1","cell":"A3","value":"=1/0"}}]}}"#
        ),
        "win-owner-1",
    )
    .unwrap();
    let live_inspect = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        &format!(r#"{{"sessionAction":"inspect","session":"{sid}","range":"A1:A3"}}"#),
        "win-owner-1",
    )
    .unwrap();
    let live_root = parse_obj(&live_inspect);
    let live_checks = field(&live_root, "checks").as_object().unwrap();
    assert_eq!(field(live_checks, "totalFormulaCells").as_u64(), Some(2));
    assert_eq!(field(live_checks, "totalErrorCells").as_u64(), Some(1));

    let wrong_owner = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        &format!(r#"{{"sessionAction":"inspect","session":"{sid}"}}"#),
        "win-intruder",
    )
    .unwrap();
    assert_eq!(wrong_owner, "{\"expired\":true}");
}

fn plain_para(text: &str) -> RichParagraph {
    RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    }
}

#[test]
fn document_pages_and_hwpx_readback_extract_outline_equations_tables_and_integrity_checks() {
    let img = RichImage::new(make_png(4, 3), "").expect("tiny png");
    let mut h1 = plain_para("1장 개요");
    h1.heading_level = 1;
    h1.bookmark = Some(1);

    let mut empty_h2 = plain_para("   ");
    empty_h2.heading_level = 2;

    let mut eq_para = plain_para("근의 공식 $$x = \\frac{-b}{2a}$$ 적용");
    eq_para.alignment = Some(RichAlignment::Center);
    eq_para.list = Some(RichListKind::Ordered);
    eq_para.list_level = 2;

    let broken_link_para = RichParagraph {
        runs: vec![RichRun {
            text: "누락된 목차 이동".to_string(),
            link: Some("#_Toc99".to_string()),
            ..RichRun::default()
        }],
        ..RichParagraph::default()
    };

    let table = RichTable::new(vec![
        vec!["항목".to_string(), "수치".to_string()],
        vec!["A".to_string(), "15".to_string()],
        vec!["B".to_string(), "25".to_string()],
        vec!["합계".to_string(), "=SUM(ABOVE)".to_string()],
    ]);

    let doc = RichDocument {
        title: "검증보고서".to_string(),
        blocks: vec![
            RichBlock::Paragraph(h1),
            RichBlock::Paragraph(empty_h2),
            RichBlock::Paragraph(eq_para),
            RichBlock::Paragraph(broken_link_para),
            RichBlock::Table(table),
            RichBlock::Image(img),
        ],
        official_agency: None,
        page_frame: RichPageFrame {
            header: vec![plain_para("상단 머리글")],
            first_header: vec![plain_para("표지 머리글")],
            first_page_different: true,
            paper: Some(RichPaper::A4),
            margins: Some(RichPageMargins {
                top: 1440,
                bottom: 1440,
                header: 720,
                footer: 720,
                left: 1440,
                right: 1440,
            }),
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };

    for fmt in [
        DocumentFormat::Docx,
        DocumentFormat::Pages,
        DocumentFormat::Hwpx,
    ] {
        let encoded = UniversalFormatConverter::encode_rich(&doc, fmt, None).unwrap();
        let b64 = base64_codec::encode(&encoded);
        let resp = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &format!(
                r#"{{"action":"inspect","fileName":"report.{}","sourceBase64":"{b64}"}}"#,
                fmt.extension()
            ),
        )
        .unwrap_or_else(|e| panic!("inspect failed for {fmt:?}: {e}"));
        let root = parse_obj(&resp);
        assert_eq!(field(&root, "format").as_str(), Some(fmt.extension()));
        assert_eq!(field(&root, "kind").as_str(), Some("document"));

        let stats = field(&root, "statistics").as_object().unwrap();
        assert_eq!(field(stats, "tableCount").as_u64(), Some(1));
        assert_eq!(field(stats, "imageCount").as_u64(), Some(1));
        assert_eq!(field(stats, "equationCount").as_u64(), Some(1));

        let checks = field(&root, "checks").as_object().unwrap();
        assert_eq!(
            field(checks, "missingAltImages").as_array().unwrap().len(),
            1
        );
        if fmt == DocumentFormat::Docx {
            // DOCX preserves internal #_Toc99 links and empty heading paragraphs verbatim.
            assert_eq!(
                field(checks, "brokenInternalBookmarks")
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(field(checks, "tableFormulaCount").as_u64(), Some(1));
        }
    }
}

fn build_pdf_with_outlines_links_and_acroform() -> Vec<u8> {
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize];
    let push_obj = |buf: &mut Vec<u8>, offs: &mut Vec<usize>, id: usize, body: &str| {
        offs.push(buf.len());
        buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    let s1 = "BT /F1 12 Tf 72 720 Td (First Page Content) Tj ET";
    let s2 = "BT /F1 12 Tf 72 720 Td (   ) Tj ET";

    push_obj(
        &mut pdf,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R /Outlines 8 0 R /AcroForm << /Fields [10 0 R] >> >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        2,
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 612.0 792.0] >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        3,
        "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 7 0 R >> >> /Contents 5 0 R /Annots [11 0 R 12 0 R 10 0 R] >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        4,
        "<< /Type /Page /Parent 2 0 R /Rotate 90 /Resources << /Font << /F1 7 0 R >> >> /Contents 6 0 R >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        5,
        &format!("<< /Length {} >>\nstream\n{s1}\nendstream", s1.len()),
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        6,
        &format!("<< /Length {} >>\nstream\n{s2}\nendstream", s2.len()),
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        7,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        8,
        "<< /Type /Outlines /First 9 0 R /Last 9 0 R /Count 1 >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        9,
        "<< /Title (Overview Section) /Parent 8 0 R /Dest [3 0 R /Fit] >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        10,
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (applicant) /V (Kim) /Rect [72 650 220 670] >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        11,
        "<< /Type /Annot /Subtype /Link /Rect [72 700 180 720] /A << /S /URI /URI (https://example.com) >> >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        12,
        "<< /Type /Annot /Subtype /Highlight /Rect [72 715 160 730] >>",
    );

    let xref_pos = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for off in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    pdf
}

#[test]
fn slides_and_pdf_readback_report_structure_and_lint_checks() {
    let deck = UniversalDocumentModel {
        title: "발표점검".to_string(),
        paragraphs: Vec::new(),
        tables: Vec::new(),
        sheet_cells: Vec::new(),
        slides: vec![
            (
                "정상 슬라이드".to_string(),
                vec!["핵심 요약".to_string()],
                "발표자 메모".to_string(),
            ),
            (String::new(), Vec::new(), String::new()),
            (
                "과밀 슬라이드".to_string(),
                (1..=10).map(|n| format!("항목 {n}")).collect(),
                String::new(),
            ),
        ],
        official_agency: None,
    };
    let pptx = UniversalFormatConverter::encode(&deck, DocumentFormat::Pptx).unwrap();
    let pptx_resp = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"action":"inspect","fileName":"deck.pptx","sourceBase64":"{}"}}"#,
            base64_codec::encode(&pptx)
        ),
    )
    .unwrap();
    let pptx_root = parse_obj(&pptx_resp);
    assert_eq!(field(&pptx_root, "slideCount").as_u64(), Some(3));
    let s_checks = field(&pptx_root, "checks").as_object().unwrap();
    assert_eq!(
        field(s_checks, "untitledSlides").as_array().unwrap().len(),
        1
    );
    assert_eq!(field(s_checks, "emptySlides").as_array().unwrap().len(), 1);
    assert_eq!(field(s_checks, "denseSlides").as_array().unwrap().len(), 1);

    let pdf_bytes = build_pdf_with_outlines_links_and_acroform();
    let pdf_resp = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"action":"inspect","fileName":"contract.pdf","sourceBase64":"{}"}}"#,
            base64_codec::encode(&pdf_bytes)
        ),
    )
    .unwrap();
    let pdf_root = parse_obj(&pdf_resp);
    assert_eq!(field(&pdf_root, "pageCount").as_u64(), Some(2));
    assert_eq!(field(&pdf_root, "outlineCount").as_u64(), Some(1));
    assert_eq!(field(&pdf_root, "linkAnnotationCount").as_u64(), Some(1));
    assert_eq!(field(&pdf_root, "markupAnnotationCount").as_u64(), Some(1));
    assert_eq!(field(&pdf_root, "formFieldCount").as_u64(), Some(1));
    let pages = field(&pdf_root, "pages").as_array().unwrap();
    let p1 = pages[0].as_object().unwrap();
    assert_eq!(
        field(p1, "widthPt").to_scalar_string().as_deref(),
        Some("612.00")
    );
    assert_eq!(
        field(p1, "heightPt").to_scalar_string().as_deref(),
        Some("792.00")
    );
    let p_checks = field(&pdf_root, "checks").as_object().unwrap();
    assert_eq!(field(p_checks, "hasAcroForm").as_bool(), Some(true));
    assert_eq!(field(p_checks, "hasOutlines").as_bool(), Some(true));
    assert_eq!(field(p_checks, "rotatedPages").as_array().unwrap().len(), 1);
    assert_eq!(field(p_checks, "blankPages").as_array().unwrap().len(), 1);
}

#[test]
fn cli_inspect_executes_end_to_end_and_enforces_security_and_size_guards() {
    let dir = std::env::temp_dir().join(format!("eo-pd248-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();

    let model = UniversalDocumentModel {
        title: "CLI점검".to_string(),
        paragraphs: Vec::new(),
        tables: Vec::new(),
        sheet_cells: vec![
            (0, 0, "10".to_string()),
            (0, 1, "20".to_string()),
            (0, 2, "=A1+B1".to_string()),
        ],
        slides: Vec::new(),
        official_agency: None,
    };
    let xlsx = UniversalFormatConverter::encode(&model, DocumentFormat::Xlsx).unwrap();
    let xlsx_path = dir.join("cli-check.xlsx");
    fs::write(&xlsx_path, &xlsx).unwrap();

    let json = run_inspect_cli(&InspectArgs {
        path: xlsx_path.to_string_lossy().into_owned(),
        sheet: None,
        range: Some("A1:C1".to_string()),
        max_items: Some(10),
    })
    .expect("CLI inspect succeeds");
    let root = parse_obj(&json);
    let rr = field(&root, "rangeReadback").as_object().unwrap();
    assert_eq!(field(rr, "returnedCount").as_u64(), Some(3));

    // Real binary subprocess execution
    let bin = env!("CARGO_BIN_EXE_easyoffice-app");
    let output = Command::new(bin)
        .arg("--inspect")
        .arg(&xlsx_path)
        .arg("--range")
        .arg("C1")
        .output()
        .expect("spawn easyoffice-app --inspect");
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cli_root = parse_obj(stdout.trim());
    let cli_rr = field(&cli_root, "rangeReadback").as_object().unwrap();
    let cli_cells = field(cli_rr, "cells").as_array().unwrap();
    assert_eq!(cli_cells.len(), 1);
    assert_eq!(
        field(cli_cells[0].as_object().unwrap(), "display").as_str(),
        Some("30")
    );

    // Oversized file refused Fail-Closed
    let over = run_inspect_cli_with_limit(
        &InspectArgs {
            path: xlsx_path.to_string_lossy().into_owned(),
            sheet: None,
            range: None,
            max_items: None,
        },
        64,
    );
    assert!(over.is_err());

    // Directory refused Fail-Closed
    let dir_res = run_inspect_cli(&InspectArgs {
        path: dir.to_string_lossy().into_owned(),
        sheet: None,
        range: None,
        max_items: None,
    });
    assert!(dir_res.is_err());

    // Dotfile / symlink-to-dotfile refused Fail-Closed
    let secret = dir.join(".env");
    fs::write(&secret, b"SECRET=1").unwrap();
    assert!(run_inspect_cli(&InspectArgs {
        path: secret.to_string_lossy().into_owned(),
        sheet: None,
        range: None,
        max_items: None,
    })
    .is_err());

    #[cfg(unix)]
    {
        let link_path = dir.join("innocent.txt");
        let _ = fs::remove_file(&link_path);
        std::os::unix::fs::symlink(&secret, &link_path).unwrap();
        assert!(run_inspect_cli(&InspectArgs {
            path: link_path.to_string_lossy().into_owned(),
            sheet: None,
            range: None,
            max_items: None,
        })
        .is_err());
        let _ = fs::remove_file(&link_path);
    }

    let _ = fs::remove_file(&secret);
    let _ = fs::remove_file(&xlsx_path);
    let _ = fs::remove_dir(&dir);
}

#[test]
#[ignore]
fn readback_10k_cells_benchmark() {
    let mut cells = Vec::with_capacity(10_000);
    for r in 0..2_500u32 {
        let row1 = r + 1;
        cells.push(format!(
            r#"{{"sheet":"S1","cell":"A{row1}","value":"{}"}}"#,
            r + 1
        ));
        cells.push(format!(
            r#"{{"sheet":"S1","cell":"B{row1}","value":"{}"}}"#,
            (r + 1) * 2
        ));
        cells.push(format!(
            r#"{{"sheet":"S1","cell":"C{row1}","value":"=A{row1}+B{row1}"}}"#
        ));
        cells.push(format!(
            r#"{{"sheet":"S1","cell":"D{row1}","value":"=C{row1}*1.1"}}"#
        ));
    }
    let payload = format!(
        r#"{{"action":"inspect","sheets":["S1"],"range":"A1:D100","cells":[{}]}}"#,
        cells.join(",")
    );
    let iters = 10u32;
    let start = Instant::now();
    for _ in 0..iters {
        let out = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &payload).unwrap();
        assert!(out.contains("\"totalFormulaCells\":5000"));
    }
    let avg_ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(iters);
    println!("readback_10k_cells_benchmark: avg_ms={avg_ms:.2}");
}

#[test]
fn evaluate_sheet_reports_blocked_and_connect_error_buckets() {
    let payload = r#"{"action":"inspect","sheets":["S1"],"range":"A1:D1","cells":[{"sheet":"S1","cell":"A1","value":"=IMAGE(\"https://example.invalid/a.png\")"},{"sheet":"S1","cell":"B1","value":"=CALL(\"Kernel32\",\"GetTickCount\",\"J\")"},{"sheet":"S1","cell":"C1","value":"=REGISTER.ID(\"Kernel32\",\"GetTickCount\")"},{"sheet":"S1","cell":"D1","value":"=STOCKHISTORY(\"MSFT\",DATE(2026,1,1))"}]}"#;
    let out = EasyOfficeIpcRouter::dispatch("evaluate_sheet", payload).unwrap();
    let root = parse_obj(&out);
    let sheets = field(&root, "sheets").as_array().unwrap();
    let err_counts = field(sheets[0].as_object().unwrap(), "errorCounts")
        .as_object()
        .unwrap();
    assert_eq!(field(err_counts, "total").as_u64(), Some(4));
    let by_code = field(err_counts, "byCode").as_object().unwrap();
    assert_eq!(field(by_code, "#BLOCKED!").as_u64(), Some(3));
    assert_eq!(field(by_code, "#CONNECT!").as_u64(), Some(1));
}

fn build_pdf_with_nested_outlines() -> Vec<u8> {
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize];
    let push_obj = |buf: &mut Vec<u8>, offs: &mut Vec<usize>, id: usize, body: &str| {
        offs.push(buf.len());
        buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    };
    let s1 = "BT /F1 12 Tf 72 720 Td (Outline Body) Tj ET";
    push_obj(
        &mut pdf,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        2,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 595 842] >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        3,
        "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        4,
        &format!("<< /Length {} >>\nstream\n{s1}\nendstream", s1.len()),
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        5,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        6,
        "<< /Type /Outlines /First 7 0 R /Last 9 0 R /Count 3 >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        7,
        "<< /Title (1. Overview) /Parent 6 0 R /First 8 0 R /Last 8 0 R /Next 9 0 R /Dest [3 0 R /Fit] >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        8,
        "<< /Title (1.1 Scope) /Parent 7 0 R /Dest [3 0 R /Fit] >>",
    );
    push_obj(
        &mut pdf,
        &mut offsets,
        9,
        "<< /Title (2. Conclusion) /Parent 6 0 R /Prev 7 0 R /Dest [3 0 R /Fit] >>",
    );
    let xref_pos = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for off in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    pdf
}

#[test]
fn pdf_decode_returns_structured_outline_nodes_and_empty_when_absent() {
    let nested_pdf = build_pdf_with_nested_outlines();
    let parsed_nodes = PdfSerializer::parse_outline_nodes(&nested_pdf).unwrap();
    let pairs: Vec<(&str, u8)> = parsed_nodes
        .iter()
        .map(|n| (n.title.as_str(), n.level))
        .collect();
    assert_eq!(
        pairs,
        vec![("1. Overview", 1), ("1.1 Scope", 2), ("2. Conclusion", 1)]
    );

    let dec_req = format!(
        r#"{{"action":"decode","fileName":"nested.pdf","sourceBase64":"{}"}}"#,
        base64_codec::encode(&nested_pdf)
    );
    let dec_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &dec_req).unwrap();
    let root = parse_obj(&dec_resp);
    let outlines = field(&root, "pdfOutlines").as_array().unwrap();
    assert_eq!(outlines.len(), 3);
    assert_eq!(
        field(outlines[1].as_object().unwrap(), "title").as_str(),
        Some("1.1 Scope")
    );
    assert_eq!(
        field(outlines[1].as_object().unwrap(), "level").as_u64(),
        Some(2)
    );

    // PDF without /Outlines returns empty pdfOutlines array
    let plain_doc = RichDocument {
        title: "북마크 없음".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문만 있음")],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    let plain_pdf =
        UniversalFormatConverter::encode_rich(&plain_doc, DocumentFormat::Pdf, None).unwrap();
    assert!(PdfSerializer::parse_outline_nodes(&plain_pdf)
        .unwrap()
        .is_empty());
    let plain_req = format!(
        r#"{{"action":"decode","fileName":"plain.pdf","sourceBase64":"{}"}}"#,
        base64_codec::encode(&plain_pdf)
    );
    let plain_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &plain_req).unwrap();
    let plain_root = parse_obj(&plain_resp);
    assert!(field(&plain_root, "pdfOutlines")
        .as_array()
        .unwrap()
        .is_empty());
}
