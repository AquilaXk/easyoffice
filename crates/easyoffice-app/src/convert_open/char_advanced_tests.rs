//! The width scale, position and kerning of a run (`scalePercent`, `positionHalfPt`, `kerningHalfPt`, PD-372)
//! through the save and open requests.

use super::tests::{blocks_array, json_field, request};
use super::*;

fn save(target: &str, run: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        "{{\"targetFormat\":\"{target}\",\"blocks\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"x\"{run}}}]}}]}}"
    );
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)
        .map_err(|e| format!("{e:?}"))?;
    EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64"))
        .ok_or_else(|| "base64".to_string())
}

fn open(file: &str, bytes: &[u8]) -> String {
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), file, bytes, ""),
    )
    .unwrap();
    blocks_array(&json).to_string()
}

#[test]
fn the_three_keys_survive_save_and_open() {
    let run = ",\"sizeHalfPt\":24,\"scalePercent\":150,\"positionHalfPt\":-6,\"kerningHalfPt\":24";
    let docx = save("docx", run).unwrap();
    let blocks = open("a.docx", &docx);
    for pair in [
        "\"scalePercent\":150",
        "\"positionHalfPt\":-6",
        "\"kerningHalfPt\":24",
    ] {
        assert!(blocks.contains(pair), "{pair} in {blocks}");
    }
}

#[test]
fn documents_without_the_keys_open_and_save_as_before() {
    let docx = save("docx", ",\"bold\":true").unwrap();
    let blocks = open("a.docx", &docx);
    for key in ["scalePercent", "positionHalfPt", "kerningHalfPt"] {
        assert!(!blocks.contains(key), "{key} in {blocks}");
    }
    let docx = save("docx", ",\"scalePercent\":100,\"positionHalfPt\":0").unwrap();
    assert!(!open("b.docx", &docx).contains("scalePercent"));
}

#[test]
fn values_outside_the_bounds_or_not_integers_reject_the_save() {
    for bad in [
        ",\"scalePercent\":0",
        ",\"scalePercent\":601",
        ",\"scalePercent\":-1",
        ",\"scalePercent\":99999999999999999999",
        ",\"scalePercent\":1.5",
        ",\"scalePercent\":\"넓게\"",
        ",\"positionHalfPt\":3169",
        ",\"positionHalfPt\":-3169",
        ",\"positionHalfPt\":2147483648",
        ",\"positionHalfPt\":0.5",
        ",\"positionHalfPt\":\"올림\"",
        ",\"kerningHalfPt\":0",
        ",\"kerningHalfPt\":3277",
        ",\"kerningHalfPt\":-1",
        ",\"kerningHalfPt\":true",
        ",\"kerningHalfPt\":1e3",
    ] {
        assert!(save("docx", bad).is_err(), "{bad}");
    }
    for ok in [
        ",\"scalePercent\":1",
        ",\"scalePercent\":600",
        ",\"positionHalfPt\":3168",
        ",\"positionHalfPt\":-3168",
        ",\"kerningHalfPt\":1",
        ",\"kerningHalfPt\":3276",
    ] {
        assert!(save("docx", ok).is_ok(), "{ok}");
    }
}
