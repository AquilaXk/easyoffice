//! The HWP 5.0 target of a converted document (PD-178): rhwp writes the records, and its own
//! reader (the one the editor opens files with, not the engine's) gets the text back.

use super::*;
use easyoffice_app::{EasyOfficeIpcRouter, ShellWriters};
use rhwp::DocumentCore;

const PAYLOAD: &str = r#"{"sourceFormat":"docx","targetFormat":"hwp","title":"보고서","blocks":[{"type":"paragraph","heading":1,"runs":[{"text":"제목 줄"}]},{"type":"paragraph","runs":[{"text":"본문 하나"}]},{"type":"paragraph","runs":[{"text":"본문 둘 둘"}]},{"type":"table","rows":[[{"text":"가"},{"text":"나"}]]}]}"#;

fn convert(payload: &str) -> Result<String, easyoffice_formats::EasyOfficeError> {
    let save = |hwpx: &[u8]| hwpx_to_hwp(hwpx);
    let writers = ShellWriters {
        pdf: None,
        hwp: Some(&save),
    };
    EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "convert_document_format",
        payload,
        "",
        writers,
    )
}

fn converted_bytes(response: &str) -> Vec<u8> {
    let marker = "\"convertedBase64\":\"";
    let start = response.find(marker).unwrap() + marker.len();
    let b64 = &response[start..start + response[start..].find('"').unwrap()];
    EasyOfficeIpcRouter::decode_base64(b64).unwrap()
}

fn paragraph_texts(core: &DocumentCore) -> Vec<String> {
    let count = core.get_paragraph_count_native(0).unwrap();
    (0..count)
        .map(|index| core.get_text_range_native(0, index, 0, 1000).unwrap())
        .collect()
}

#[test]
fn the_conversion_is_an_hwp_file_rhwp_reads_back_with_every_paragraph() {
    let response = convert(PAYLOAD).expect("converted");
    assert!(
        response.contains("\"compatibility\":\"standard\""),
        "{response}"
    );
    assert!(
        response.contains("\"compatibilityWarning\":null"),
        "{response}"
    );
    assert!(
        response.contains("\"paragraphCount\":4,\"tableCount\":1"),
        "{response}"
    );
    let bytes = converted_bytes(&response);
    assert!(
        bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]),
        "OLE2 signature"
    );
    let core = DocumentCore::from_bytes(&bytes).expect("rhwp opens it");
    let texts = paragraph_texts(&core);
    for wanted in ["제목 줄", "본문 하나", "본문 둘 둘"] {
        assert!(texts.iter().any(|t| t == wanted), "{wanted:?} in {texts:?}");
    }
    let svg = core.render_page_svg_native(0).unwrap();
    for cell in ["가", "나"] {
        assert!(svg.contains(cell), "table cell {cell} is drawn");
    }
}

#[test]
fn bytes_that_are_no_hwpx_are_an_error_not_a_panic() {
    assert!(hwpx_to_hwp(b"not a document").is_err());
    assert!(hwpx_to_hwp(&[]).is_err());
    assert!(hwpx_to_hwp(&vec![0; crate::doc_pdf::MAX_HWPX_BYTES + 1]).is_err());
}
