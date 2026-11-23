//! HWP targets are written by the shell's Hangul core, never by the engine (PD-178).

use easyoffice_app::EasyOfficeIpcRouter;

const STRUCTURED: &str = r#"{"sourceFormat":"docx","targetFormat":"hwp","title":"보고서","blocks":[{"type":"paragraph","runs":[{"text":"본문 하나"}]}]}"#;

#[test]
fn hwp_target_without_the_shell_writer_is_refused() {
    let err = EasyOfficeIpcRouter::dispatch("convert_document_format", STRUCTURED)
        .expect_err("the engine's private container must not be handed out as .hwp");
    assert!(err.to_string().contains("HWP"), "{err}");
    let legacy = EasyOfficeIpcRouter::dispatch("convert_document_format", "docx->hwp|제목|본문");
    assert!(legacy.is_err(), "the legacy body path is refused too");
}

#[test]
fn hwt_stays_available_and_reports_the_limit() {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", "docx->hwt|제목|본문")
        .expect("hwt");
    assert!(json.contains("\"compatibility\":\"limited\""), "{json}");
    assert!(!json.contains("\"compatibilityWarning\":null"), "{json}");
}

#[test]
fn hwt_with_shell_writer_routes_through_writer_and_reports_standard() {
    use easyoffice_app::ShellWriters;
    use easyoffice_formats::{DocumentFormat, UniversalDocumentModel, UniversalFormatConverter};

    let hwp_stub = UniversalFormatConverter::encode(
        &UniversalDocumentModel {
            title: "서식".to_string(),
            paragraphs: vec!["본문 하나".to_string()],
            ..UniversalDocumentModel::default()
        },
        DocumentFormat::Hwt,
    )
    .expect("stub hwt");
    let captured = std::cell::RefCell::new(Vec::new());
    let writer = |hwpx: &[u8]| -> Result<Vec<u8>, String> {
        captured.borrow_mut().push(hwpx.to_vec());
        Ok(hwp_stub.clone())
    };
    let writers = ShellWriters {
        hwp: Some(&writer),
        ..ShellWriters::default()
    };
    let payload = r#"{"sourceFormat":"docx","targetFormat":"hwt","title":"서식","blocks":[{"type":"paragraph","runs":[{"text":"본문 하나"}]}]}"#;
    let json = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "convert_document_format",
        payload,
        "",
        writers,
    )
    .expect("hwt with shell writer");
    assert_eq!(captured.borrow().len(), 1, "shell writer must be invoked");
    assert!(!captured.borrow()[0].is_empty());
    assert!(json.contains("\"compatibility\":\"standard\""), "{json}");
    assert!(json.contains("\"compatibilityWarning\":null"), "{json}");

    let source_docx = UniversalFormatConverter::encode(
        &UniversalDocumentModel {
            title: "원본".to_string(),
            paragraphs: vec!["문단".to_string()],
            ..UniversalDocumentModel::default()
        },
        DocumentFormat::Docx,
    )
    .expect("encode source docx");
    let source_b64 = EasyOfficeIpcRouter::encode_base64(&source_docx);
    let convert_payload = format!(
        r#"{{"sourceBase64":"{source_b64}","fileName":"in.docx","action":"convert","targetFormat":"hwt"}}"#
    );
    let convert_json = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "convert_document_format",
        &convert_payload,
        "",
        writers,
    )
    .expect("sourceBase64 -> hwt with shell writer");
    assert_eq!(
        captured.borrow().len(),
        2,
        "sourceBase64 conversion to hwt must invoke shell writer"
    );
    assert!(
        convert_json.contains("\"compatibility\":\"standard\""),
        "{convert_json}"
    );
    assert!(
        convert_json.contains("\"compatibilityWarning\":null"),
        "{convert_json}"
    );
}
