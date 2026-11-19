//! PD-258: IPC `"action":"inspect"` and CLI `--inspect <FILE>` layoutLint readback tests.

use std::fs;
use std::process::Command;

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    base64_codec, deflate_zlib, DocumentFormat, IeeeCrc32, RichBlock, RichDocument, RichImage,
    RichPageFrame, RichPageMargins, RichPaper, RichParagraph, RichRun, RichSlide, RichSlideDeck,
    RichTable, SlideAspect, SlideBox, SlideElement, SlideMasterLayout, SlideShapeType,
    SlideTextAlign, SlideTransition, StrictJsonValue, UniversalDocumentModel,
    UniversalFormatConverter, MAX_LINT_ISSUES,
};

fn parse_obj(json: &str) -> Vec<(String, StrictJsonValue)> {
    match StrictJsonValue::parse(json).expect("valid JSON") {
        StrictJsonValue::Object(entries) => entries,
        other => panic!("expected root JSON object, got {other:?}"),
    }
}

fn field<'a>(entries: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(entries, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

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

#[test]
fn ipc_inspect_document_includes_layout_lint_issues_and_preserves_existing_fields() {
    let mut img = RichImage::new(make_png(100, 100), "차트 이미지").unwrap();
    img.set_display_emu(5_731_510, 2_540_000).unwrap();

    let h1 = RichParagraph {
        runs: vec![RichRun::plain("1장 개요")],
        heading_level: 1,
        ..RichParagraph::default()
    };

    let mut tbl = RichTable::new(vec![
        vec!["항목".into(), "{{placeholder}}".into()],
        vec!["합계".into(), "=SUM(ABOVE)".into()],
    ]);
    tbl.col_widths_twips = vec![5000, 5000];

    let doc = RichDocument {
        title: "린트 문서".into(),
        blocks: vec![
            RichBlock::Paragraph(h1),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("고객: {{customer_name}}")],
                ..RichParagraph::default()
            }),
            RichBlock::Image(img),
            RichBlock::Table(tbl),
        ],
        page_frame: RichPageFrame {
            header: vec![RichParagraph {
                runs: vec![RichRun::plain("머리말")],
                ..RichParagraph::default()
            }],
            paper: Some(RichPaper::A4),
            margins: Some(RichPageMargins {
                top: 1000,
                bottom: 1000,
                header: 1400,
                footer: 500,
                left: 2000,
                right: 2000,
            }),
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };

    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let req = format!(
        r#"{{"action":"inspect","fileName":"lint-doc.docx","sourceBase64":"{}"}}"#,
        base64_codec::encode(&docx)
    );
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap();
    let root = parse_obj(&resp);

    // Existing readback fields preserved
    assert_eq!(field(&root, "kind").as_str(), Some("document"));
    let _stats = field(&root, "statistics").as_object().unwrap();
    let _checks = field(&root, "checks").as_object().unwrap();

    let lint = field(&root, "layoutLint").as_object().unwrap();
    assert_eq!(field(lint, "truncated").as_bool(), Some(false));
    assert!(field(lint, "overflowCount").as_u64().unwrap() >= 1);
    assert!(field(lint, "overlapCount").as_u64().unwrap() >= 1);
    assert!(field(lint, "emptyPlaceholderCount").as_u64().unwrap() >= 2);
    let issues = field(lint, "issues").as_array().unwrap();
    assert_eq!(
        field(lint, "issueCount").as_u64(),
        Some(issues.len() as u64)
    );
    assert!(issues.iter().any(|item| {
        let obj = item.as_object().unwrap();
        field(obj, "kind").as_str() == Some("emptyPlaceholder")
            && field(obj, "targetId").as_str() == Some("block-1")
            && field(obj, "blockIndex").as_u64() == Some(1)
    }));
}

#[test]
fn ipc_inspect_structured_and_binary_slide_deck_includes_layout_lint() {
    let req = r#"{
        "action": "inspect",
        "sourceFormat": "pptx",
        "aspect": "16:9",
        "slides": [
            {
                "title": "",
                "bullets": [],
                "notes": "",
                "layout": "titleSlide",
                "transition": "fade",
                "elements": [
                    {"kind":"shape","id":"sh-a","x":50,"y":60,"w":150,"h":80,"shapeType":"rect","text":"A"},
                    {"kind":"shape","id":"sh-b","x":120,"y":90,"w":150,"h":80,"shapeType":"rect","text":"B"},
                    {"kind":"textBox","id":"tb-out","x":900,"y":10,"w":100,"h":40,"text":"Click to add title","fontSizePt":18}
                ]
            }
        ]
    }"#;
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", req).unwrap();
    let root = parse_obj(&resp);
    let lint = field(&root, "layoutLint").as_object().unwrap();
    assert_eq!(field(lint, "overflowCount").as_u64(), Some(1));
    assert_eq!(field(lint, "overlapCount").as_u64(), Some(1));
    assert_eq!(field(lint, "emptyPlaceholderCount").as_u64(), Some(2));
    assert_eq!(field(lint, "issueCount").as_u64(), Some(4));
    assert_eq!(field(lint, "truncated").as_bool(), Some(false));
    let issues = field(lint, "issues").as_array().unwrap();
    assert_eq!(issues.len(), 4);
    for issue in issues {
        let obj = issue.as_object().unwrap();
        assert_eq!(field(obj, "pageOrSlide").as_u64(), Some(1));
        assert!(matches!(field(obj, "blockIndex"), StrictJsonValue::Null));
        assert!(!field(obj, "message").as_str().unwrap().is_empty());
    }

    // Also test legacy slides (.ppt) readback path (`write_slides_readback`)
    let legacy_ppt = UniversalFormatConverter::encode(
        &UniversalDocumentModel {
            title: "레거시".into(),
            slides: vec![(String::new(), Vec::new(), String::new())],
            ..UniversalDocumentModel::default()
        },
        DocumentFormat::Ppt,
    )
    .unwrap();
    let ppt_req = format!(
        r#"{{"action":"inspect","fileName":"legacy.ppt","sourceBase64":"{}"}}"#,
        base64_codec::encode(&legacy_ppt)
    );
    let ppt_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &ppt_req).unwrap();
    let ppt_root = parse_obj(&ppt_resp);
    let ppt_lint = field(&ppt_root, "layoutLint").as_object().unwrap();
    assert_eq!(field(ppt_lint, "emptyPlaceholderCount").as_u64(), Some(2));
}

#[test]
fn cli_inspect_reports_layout_lint_for_docx_and_pptx_and_enforces_cap() {
    let dir = std::env::temp_dir().join(format!("eo-pd258-ipc-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();

    let deck = RichSlideDeck {
        title: "CLI 슬라이드 린트".into(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        slides: vec![RichSlide {
            title: "제목".into(),
            bullets: Vec::new(),
            notes: String::new(),
            layout: SlideMasterLayout::TitleAndContent,
            transition: SlideTransition::Fade,
            elements: vec![
                SlideElement::Shape {
                    id: "s1".into(),
                    rect: SlideBox {
                        x: 40.0,
                        y: 60.0,
                        w: 160.0,
                        h: 80.0,
                    },
                    shape_type: SlideShapeType::Rect,
                    text: "첫째".into(),
                    fill: Some("DBEAFE".into()),
                    stroke: None,
                    stroke_width_pt: 1.0,
                    color: None,
                    font_size_pt: 16.0,
                    bold: false,
                    align: SlideTextAlign::Center,
                },
                SlideElement::Shape {
                    id: "s2".into(),
                    rect: SlideBox {
                        x: 120.0,
                        y: 80.0,
                        w: 160.0,
                        h: 80.0,
                    },
                    shape_type: SlideShapeType::Rect,
                    text: "둘째".into(),
                    fill: Some("FEE2E2".into()),
                    stroke: None,
                    stroke_width_pt: 1.0,
                    color: None,
                    font_size_pt: 16.0,
                    bold: false,
                    align: SlideTextAlign::Center,
                },
            ],
        }],
        elements_rejected: 0,
    };
    let pptx = UniversalFormatConverter::encode_rich_slides(&deck, DocumentFormat::Pptx).unwrap();
    let pptx_path = dir.join("lint-check.pptx");
    fs::write(&pptx_path, &pptx).unwrap();

    let bin = env!("CARGO_BIN_EXE_easyoffice-app");
    let output = Command::new(bin)
        .arg("--inspect")
        .arg(&pptx_path)
        .output()
        .expect("run easyoffice-app --inspect");
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let root = parse_obj(stdout.trim());
    let lint = field(&root, "layoutLint").as_object().unwrap();
    assert_eq!(field(lint, "overlapCount").as_u64(), Some(1));
    assert_eq!(field(lint, "issueCount").as_u64(), Some(1));

    // Hostile 1,100-heading structured inspect caps at MAX_LINT_ISSUES (1,024)
    let blocks: Vec<String> = (0..1_100)
        .map(|_| r#"{"type":"paragraph","heading":2,"text":""}"#.to_string())
        .collect();
    let hostile_req = format!(
        r#"{{"action":"inspect","sourceFormat":"docx","blocks":[{}]}}"#,
        blocks.join(",")
    );
    let hostile_resp =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &hostile_req).unwrap();
    let hostile_root = parse_obj(&hostile_resp);
    let hostile_lint = field(&hostile_root, "layoutLint").as_object().unwrap();
    assert_eq!(field(hostile_lint, "truncated").as_bool(), Some(true));
    assert_eq!(
        field(hostile_lint, "issueCount").as_u64(),
        Some(MAX_LINT_ISSUES as u64)
    );
    assert_eq!(
        field(hostile_lint, "issues").as_array().unwrap().len(),
        MAX_LINT_ISSUES
    );

    let _ = fs::remove_file(&pptx_path);
    let _ = fs::remove_dir(&dir);
}
