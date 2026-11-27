//! The packed OpenType features and the line-grid switch of a run (`openType`, `snapToGrid`, PD-374) through the save and
//! open requests.

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
fn the_packed_features_and_the_grid_switch_survive_save_and_open() {
    // ligatures 3, number spacing 2, number form 1, contextual alternates, style sets 1 and 20.
    let packed = 3 | 2 << 4 | 1 << 6 | 1 << 8 | (1 | 1 << 19) << 9;
    let run = format!(",\"openType\":{packed},\"snapToGrid\":false");
    let docx = save("docx", &run).unwrap();
    let blocks = open("a.docx", &docx);
    for pair in [
        format!("\"openType\":{packed}"),
        "\"snapToGrid\":false".to_string(),
    ] {
        assert!(blocks.contains(&pair), "{pair} in {blocks}");
    }
}

#[test]
fn documents_without_the_keys_open_and_save_as_before() {
    let docx = save("docx", ",\"bold\":true,\"snapToGrid\":true").unwrap();
    let blocks = open("a.docx", &docx);
    for key in ["openType", "snapToGrid"] {
        assert!(!blocks.contains(key), "{key} in {blocks}");
    }
}

#[test]
fn values_outside_the_bounds_or_of_the_wrong_kind_reject_the_save() {
    let too_big = (1u64 << 29) + 1;
    for bad in [
        ",\"openType\":0".to_string(),
        ",\"openType\":-1".to_string(),
        format!(",\"openType\":{too_big}"),
        ",\"openType\":99999999999999999999".to_string(),
        ",\"openType\":1.5".to_string(),
        ",\"openType\":\"standard\"".to_string(),
        ",\"openType\":true".to_string(),
        // number spacing 3 and number form 3 are not in the enumerations.
        format!(",\"openType\":{}", 3 << 4),
        format!(",\"openType\":{}", 3 << 6),
        ",\"snapToGrid\":0".to_string(),
        ",\"snapToGrid\":\"false\"".to_string(),
    ] {
        assert!(save("docx", &bad).is_err(), "{bad}");
    }
    for ok in [
        ",\"openType\":1".to_string(),
        // the largest valid value: every ligature bit, number spacing 2, number form 2, alternates, all 20 sets.
        format!(
            ",\"openType\":{}",
            15 | 2 << 4 | 2 << 6 | 1 << 8 | 0xF_FFFF << 9
        ),
        ",\"snapToGrid\":false".to_string(),
    ] {
        assert!(save("docx", &ok).is_ok(), "{ok}");
    }
}

// The packed text effects and the underline colour (`textFx`, `underlineColor`) of the 글꼴 tab.

#[test]
fn the_packed_effects_and_the_underline_color_survive_save_and_open() {
    // double strike, small capitals, emphasis 3 (circle), underline style 14 (wave).
    let packed = 1 | 2 | (3 << 4) | (14 << 7);
    let run = format!(",\"underline\":true,\"textFx\":{packed},\"underlineColor\":\"ff0000\"");
    let docx = save("docx", &run).unwrap();
    let blocks = open("a.docx", &docx);
    for pair in [
        format!("\"textFx\":{packed}"),
        "\"underlineColor\":\"FF0000\"".to_string(),
    ] {
        assert!(blocks.contains(&pair), "{pair} in {blocks}");
    }
    let plain = open("b.docx", &save("docx", ",\"underline\":true").unwrap());
    for key in ["textFx", "underlineColor"] {
        assert!(!plain.contains(key), "{key} in {plain}");
    }
}

#[test]
fn effect_values_outside_the_bounds_or_of_the_wrong_kind_reject_the_save() {
    for bad in [
        ",\"textFx\":0".to_string(),
        ",\"textFx\":-1".to_string(),
        format!(",\"textFx\":{}", 1 << 12),
        format!(",\"textFx\":{}", 5 << 4),
        format!(",\"textFx\":{}", 17 << 7),
        ",\"textFx\":1.5".to_string(),
        ",\"textFx\":\"double\"".to_string(),
        ",\"textFx\":99999999999999999999".to_string(),
        ",\"underlineColor\":\"red\"".to_string(),
        ",\"underlineColor\":\"1234567\"".to_string(),
        ",\"underlineColor\":12".to_string(),
    ] {
        assert!(save("docx", &bad).is_err(), "{bad}");
    }
    for ok in [
        ",\"textFx\":1".to_string(),
        format!(",\"textFx\":{}", 15 | (4 << 4) | (16 << 7)),
        ",\"underlineColor\":\"00FF00\"".to_string(),
        ",\"underlineColor\":\"\"".to_string(),
    ] {
        assert!(save("docx", &ok).is_ok(), "{ok}");
    }
}

// The East Asian font of a run (`fontEastAsia`, next to `font` for the Latin one).

#[test]
fn the_east_asian_font_survives_save_and_open_and_one_font_stays_one() {
    let docx = save(
        "docx",
        ",\"font\":\"Calibri\",\"fontEastAsia\":\"맑은 고딕\"",
    )
    .unwrap();
    let blocks = open("a.docx", &docx);
    assert!(blocks.contains("\"font\":\"Calibri\""), "{blocks}");
    assert!(
        blocks.contains("\"fontEastAsia\":\"맑은 고딕\""),
        "{blocks}"
    );
    let one = open("b.docx", &save("docx", ",\"font\":\"Arial\"").unwrap());
    assert!(!one.contains("fontEastAsia"), "{one}");
    let same = open(
        "c.docx",
        &save("docx", ",\"font\":\"Arial\",\"fontEastAsia\":\"Arial\"").unwrap(),
    );
    assert!(
        !same.contains("fontEastAsia"),
        "the same face is one font: {same}"
    );
}

#[test]
fn a_bad_east_asian_font_name_rejects_the_save() {
    let long = format!(",\"fontEastAsia\":\"{}\"", "가".repeat(65));
    for bad in [
        long.as_str(),
        ",\"fontEastAsia\":\"a\\u0007b\"",
        ",\"fontEastAsia\":12",
        ",\"fontEastAsia\":true",
    ] {
        assert!(save("docx", bad).is_err(), "{bad}");
    }
    for ok in [",\"fontEastAsia\":\"\"", ",\"fontEastAsia\":\"바탕\""] {
        assert!(save("docx", ok).is_ok(), "{ok}");
    }
}
