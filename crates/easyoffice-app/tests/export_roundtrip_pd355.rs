//! Real `EasyOfficeIpcRouter` end-to-end integration tests for PD-355:
//! DOC -> `.pages` -> `decode` roundtrip (headings, tables, margins, headers/footers),
//! BASE/BOARD/PDF export `sourceFormat` wiring, and AcroForm `fields` JSON extraction.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{base64_codec, StrictJsonValue};

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing key {key}"))
}

fn parse_obj(raw: &str) -> Vec<(String, StrictJsonValue)> {
    StrictJsonValue::parse(raw)
        .expect("valid json")
        .as_object()
        .expect("json object")
        .to_vec()
}

#[test]
fn doc_to_pages_convert_then_decode_roundtrips_headings_table_styles_and_margins() {
    let payload = r#"{
        "sourceFormat": "docx",
        "targetFormat": "pages",
        "title": "분기 실적",
        "pageSetup": {
            "margins": {"top": 1200, "bottom": 1300, "left": 1000, "right": 1100, "header": 500, "footer": 540},
            "paper": {"width": 16838, "height": 11906}
        },
        "header": [{"type": "paragraph", "align": "right", "runs": [{"text": "대외비 머리말"}]}],
        "footer": [{"type": "paragraph", "align": "center", "runs": [{"text": "꼬리말"}]}],
        "blocks": [
            {"type": "paragraph", "heading": 2, "runs": [{"text": "요약"}]},
            {"type": "paragraph", "rightIndentTwips": 640, "lineSpacingPercent": 150, "runs": [{"text": "본문 문단"}]},
            {
                "type": "table",
                "colWidthsTwips": [2000, 3200],
                "rows": [[{"text": "항목"}, {"text": "금액"}], [{"text": "개발"}, {"text": "500"}]],
                "cellStyles": [{
                    "row": 0,
                    "col": 0,
                    "fill": "E6EEF8",
                    "borders": {
                        "top": {"kind": "thick", "color": "1F4E79"},
                        "bottom": {"kind": "dashed", "color": "2E75B6"}
                    }
                }]
            }
        ]
    }"#;
    let convert_res =
        parse_obj(&EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap());
    assert_eq!(field(&convert_res, "status").as_str(), Some("OK"));
    assert_eq!(field(&convert_res, "targetFormat").as_str(), Some("pages"));
    let pages_b64 = field(&convert_res, "convertedBase64").as_str().unwrap();

    let decode_req = format!(
        r#"{{"action":"decode","fileName":"분기실적.pages","sourceBase64":"{pages_b64}"}}"#
    );
    let decode_res =
        parse_obj(&EasyOfficeIpcRouter::dispatch("convert_document_format", &decode_req).unwrap());
    assert_eq!(field(&decode_res, "format").as_str(), Some("pages"));
    assert_eq!(field(&decode_res, "module").as_str(), Some("DOC"));

    let blocks = field(&decode_res, "blocks").as_array().unwrap();
    let h2 = blocks[0].as_object().unwrap();
    assert_eq!(field(h2, "heading").as_u64(), Some(2));
    let p2 = blocks[1].as_object().unwrap();
    assert_eq!(field(p2, "rightIndentTwips").as_u64(), Some(640));
    assert_eq!(field(p2, "lineSpacingPercent").as_u64(), Some(150));
    let tbl = blocks[2].as_object().unwrap();
    let widths: Vec<u64> = field(tbl, "colWidthsTwips")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(widths, vec![2000, 3200]);
    assert!(!field(tbl, "cellStyles").as_array().unwrap().is_empty());
    let page_setup = field(&decode_res, "pageSetup").as_object().unwrap();
    let margins = field(page_setup, "margins").as_object().unwrap();
    assert_eq!(field(margins, "left").as_u64(), Some(1000));
    let paper = field(page_setup, "paper").as_object().unwrap();
    assert_eq!(field(paper, "width").as_u64(), Some(16838));
    assert_eq!(field(paper, "height").as_u64(), Some(11906));
    assert!(!field(&decode_res, "header").as_array().unwrap().is_empty());
    assert!(!field(&decode_res, "footer").as_array().unwrap().is_empty());
}

#[test]
fn base_board_and_pdf_exports_succeed_with_expected_ipc_source_formats() {
    let base_csv = r#"{
        "sourceFormat": "xlsx",
        "targetFormat": "csv",
        "title": "고객목록",
        "sheets": [{"name": "고객", "cells": [{"ref": "A1", "raw": "이름"}, {"ref": "B1", "raw": "홍길동"}]}]
    }"#;
    let base_res =
        parse_obj(&EasyOfficeIpcRouter::dispatch("convert_document_format", base_csv).unwrap());
    assert_eq!(field(&base_res, "targetFormat").as_str(), Some("csv"));
    let csv_bytes =
        base64_codec::decode(field(&base_res, "convertedBase64").as_str().unwrap()).unwrap();
    assert!(String::from_utf8_lossy(&csv_bytes).contains("홍길동"));

    // Invalid legacy `"base"` sourceFormat must be refused (regression guard for export-dialog.js).
    let bad_base = r#"{"sourceFormat":"base","targetFormat":"csv","title":"x","sheets":[]}"#;
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", bad_base).is_err());

    let board_pdf = r#"{
        "sourceFormat": "pptx",
        "targetFormat": "pdf",
        "title": "아키텍처보드",
        "aspect": "16:9",
        "slides": [{"title": "", "bullets": [], "notes": "", "layout": "blank", "transition": "none", "elements": [
            {"id": "n1", "kind": "textbox", "x": 40, "y": 40, "w": 200, "h": 80, "text": "노드 A"}
        ]}]
    }"#;
    let board_res =
        parse_obj(&EasyOfficeIpcRouter::dispatch("convert_document_format", board_pdf).unwrap());
    assert_eq!(field(&board_res, "targetFormat").as_str(), Some("pdf"));
    let pdf_bytes =
        base64_codec::decode(field(&board_res, "convertedBase64").as_str().unwrap()).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));

    let pdf_to_txt = r#"{
        "sourceFormat": "pdf",
        "targetFormat": "txt",
        "title": "공고문",
        "blocks": [{"type": "paragraph", "runs": [{"text": "본문 첫 줄"}]}],
        "pdf": {"rotationDegrees": 0}
    }"#;
    let txt_res =
        parse_obj(&EasyOfficeIpcRouter::dispatch("convert_document_format", pdf_to_txt).unwrap());
    let txt_bytes =
        base64_codec::decode(field(&txt_res, "convertedBase64").as_str().unwrap()).unwrap();
    assert!(String::from_utf8_lossy(&txt_bytes).contains("본문 첫 줄"));
}

#[test]
fn pdf_decode_includes_extracted_acroform_fields_array() {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let objs = [
        (1u32, "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>"),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>"),
        (4, "<< /Fields [5 0 R 6 0 R] >>"),
        (
            5,
            "<< /FT /Tx /T (contractNo) /TU (Contract No) /V (2026-099) >>",
        ),
        (6, "<< /FT /Btn /T (agreed) /V /Yes >>"),
    ];
    let mut offsets = Vec::new();
    for (id, body) in &objs {
        offsets.push((*id, pdf.len()));
        pdf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for (_, off) in &offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );

    let decode_req = format!(
        r#"{{"action":"decode","fileName":"form.pdf","sourceBase64":"{}"}}"#,
        base64_codec::encode(&pdf)
    );
    let res =
        parse_obj(&EasyOfficeIpcRouter::dispatch("convert_document_format", &decode_req).unwrap());
    let pdf_form = field(&res, "pdfForm").as_object().unwrap();
    assert_eq!(field(pdf_form, "hasAcroForm").as_bool(), Some(true));
    assert_eq!(field(pdf_form, "fieldCount").as_u64(), Some(2));
    let fields = field(pdf_form, "fields").as_array().unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(
        field(fields[0].as_object().unwrap(), "name").as_str(),
        Some("Contract No")
    );
    assert_eq!(
        field(fields[0].as_object().unwrap(), "value").as_str(),
        Some("2026-099")
    );
    assert_eq!(
        field(fields[1].as_object().unwrap(), "name").as_str(),
        Some("agreed")
    );
    assert_eq!(
        field(fields[1].as_object().unwrap(), "value").as_str(),
        Some("Yes")
    );
}
