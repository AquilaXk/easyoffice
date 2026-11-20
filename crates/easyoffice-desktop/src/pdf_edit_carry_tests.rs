//! Tests of what the result of an edit keeps of the original besides the pages, and of the
//! warnings for what it cannot keep (PD-266). The files are written by hand so that every entry
//! under test is exactly the one the test names.

use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::hand_written_with_xref;
use crate::pdf_edit_session::EditSession;
use crate::pdf_edit_wire::Built;
use std::sync::Arc;
use usvg::fontdb::Database;

fn engine() -> InProcessEngine {
    InProcessEngine::new(Arc::new(Database::new()))
}

/// Opens `file` for editing and writes all its pages again.
pub(crate) fn rebuilt(file: Vec<u8>) -> Built {
    let engine = engine();
    let session = EditSession::open(file, &engine).expect("the file opens");
    session.render(None, &engine).expect("the file is written")
}

/// A one-page document. `catalog` and `annots` are added to the catalog and to the page's
/// `/Annots`; `more` are the objects from number 5 on.
pub(crate) fn one_page(catalog: &str, annots: &str, more: &[String]) -> Vec<u8> {
    let content = "0 0 1 rg 10 10 80 80 re f";
    let mut objects = vec![
        (1, format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>")),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R \
                 /Annots [{annots}] >>"
            ),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ),
    ];
    objects.extend((5u32..).zip(more.iter().cloned()));
    hand_written_with_xref(&objects, "")
}

fn square(rect: &str) -> String {
    format!("<< /Type /Annot /Subtype /Square /Rect [{rect}] /C [1 0 0] >>")
}

#[test]
fn annotations_the_result_cannot_write_are_reported_by_kind() {
    let more = vec![
        square("10 10 50 50"),
        "<< /Type /Annot /Subtype /Line /Rect [10 60 80 90] /L [10 60 80 90] >>".to_string(),
        square("100 100 150 150"),
        "<< /Type /Annot /Subtype /Link /Rect [0 0 20 20] /A << /S /URI /URI (https://example.org/) >> >>"
            .to_string(),
    ];
    let built = rebuilt(one_page("", "5 0 R 6 0 R 7 0 R 8 0 R", &more));
    assert_eq!(
        built.warnings,
        ["지원하지 않는 주석 3개는 저장하지 못했습니다 (Square 2, Line 1)."]
    );
}

#[test]
fn a_file_without_losses_has_no_warnings() {
    let more = vec![
        "<< /Type /Annot /Subtype /Link /Rect [0 0 20 20] /A << /S /URI /URI (https://example.org/) >> >>"
            .to_string(),
    ];
    assert_eq!(
        rebuilt(one_page("", "5 0 R", &more)).warnings,
        Vec::<String>::new()
    );
}

#[test]
fn catalog_entries_that_are_not_written_are_reported() {
    let more = vec!["<< /Type /StructTreeRoot >>".to_string()];
    let catalog =
        "/StructTreeRoot 5 0 R /Names << /JavaScript << /Names [] >> >> /Collection << /View /T >>";
    let built = rebuilt(one_page(catalog, "", &more));
    assert_eq!(
        built.warnings,
        [
            "태그 구조 트리(접근성 정보)는 저장하지 않았습니다.",
            "문서 스크립트는 보안을 위해 저장하지 않았습니다.",
            "PDF 포트폴리오 구성은 저장하지 않았습니다."
        ]
    );
}

#[test]
fn a_signature_field_is_reported_and_not_written() {
    let more = vec![
        "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Sign) /Rect [10 10 90 40] >>".to_string(),
    ];
    let built = rebuilt(one_page(
        "/AcroForm << /Fields [5 0 R] /SigFlags 3 >>",
        "5 0 R",
        &more,
    ));
    assert_eq!(
        built.warnings,
        ["전자서명 등 지원하지 않는 양식 필드 1개는 저장하지 못했습니다."]
    );
}
