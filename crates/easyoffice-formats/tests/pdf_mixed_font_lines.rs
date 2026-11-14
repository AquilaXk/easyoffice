//! A line with Latin-1 signs outside ASCII (the middle dot, accented letters) is written as several text
//! objects in two fonts and read back as the one line it was (external review M-15).

use easyoffice_formats::{PdfDocumentModel, PdfPageModel, PdfSerializer};

fn round_trip(body: &str) -> String {
    let model = PdfDocumentModel {
        title: "혼합".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: body.to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    };
    let bytes = PdfSerializer::serialize(&model);
    PdfSerializer::parse(&bytes).unwrap().pages[0]
        .body_text
        .clone()
}

#[test]
fn a_line_with_a_middle_dot_reads_back_as_one_line() {
    assert_eq!(round_trip("제1조(목적·정의)"), "제1조(목적·정의)");
    assert_eq!(round_trip("총무팀 · 대외비"), "총무팀 · 대외비");
    assert_eq!(round_trip("café 메뉴 résumé"), "café 메뉴 résumé");
}

#[test]
fn lines_stay_lines_and_only_mixed_ones_are_joined() {
    let body = "제1조(목적·정의)\n둘째 줄\ncafé 셋째";
    assert_eq!(round_trip(body), body);
}
