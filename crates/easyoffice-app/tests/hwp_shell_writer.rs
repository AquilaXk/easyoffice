//! HWP targets are written by the shell's Hangul core, never by the engine (PD-172).

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
