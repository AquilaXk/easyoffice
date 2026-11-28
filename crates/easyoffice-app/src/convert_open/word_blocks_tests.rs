//! The paragraph and block features of PD-401 (tab stops, borders, shading, drop cap, column look, text boxes, equations, styles
//! of the user) through the structured save and the open JSON of the shell.

use super::*;

fn save(payload: &str) -> Result<Vec<u8>, EasyOfficeError> {
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", payload)?;
    Ok(EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap())
}

fn reopen(bytes: &[u8]) -> String {
    EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "새 기능.docx", bytes, ""),
    )
    .unwrap()
}

const FRACTION: &str = "<m:oMath><m:f><m:num><m:r><m:t>1</m:t></m:r></m:num><m:den><m:r><m:t>2</m:t></m:r></m:den></m:f></m:oMath>";

fn payload() -> String {
    format!(
        concat!(
            "{{\"targetFormat\":\"docx\",\"title\":\"쓰기\",\"docColumns\":2,\"docColumnSeparator\":true,\"docColumnNarrow\":\"last\",",
            "\"styles\":[{{\"name\":\"본문 강조\",\"font\":\"함초롬바탕\",\"sizeHalfPt\":28,\"bold\":true,\"color\":\"1F4E79\",\"align\":\"center\",\"spaceAfterTwips\":240}}],",
            "\"blocks\":[",
            "{{\"type\":\"paragraph\",\"styleName\":\"본문 강조\",\"tabs\":[{{\"pos\":8000,\"align\":\"right\",\"leader\":\"dot\"}},{{\"pos\":2000,\"align\":\"center\"}}],",
            "\"border\":{{\"top\":{{\"style\":\"single\",\"eighths\":6,\"color\":\"FF0000\",\"space\":4}},\"bottom\":{{\"style\":\"double\",\"eighths\":12,\"space\":1}}}},",
            "\"shade\":\"FFFF00\",\"runs\":[{{\"text\":\"목차\\t1\"}}]}},",
            "{{\"type\":\"paragraph\",\"dropCap\":{{\"lines\":3,\"margin\":false}},\"runs\":[{{\"text\":\"가\"}}]}},",
            "{{\"type\":\"paragraph\",\"columnBreak\":true,\"runs\":[{{\"text\":\"둘째 단\"}}]}},",
            "{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"값 \"}},{{\"text\":\"12\",\"math\":\"{math}\"}}],",
            "\"textBoxes\":[{{\"widthEmu\":2143125,\"heightEmu\":714375,\"wrap\":\"square\",\"fill\":\"FFEEAA\",\"border\":true,",
            "\"paragraphs\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"상자 글\"}}]}}]}}]}}",
            "]}}"
        ),
        math = FRACTION
    )
}

#[test]
fn the_paragraph_and_block_features_survive_a_docx_save_and_the_open_json() {
    let json = reopen(&save(&payload()).unwrap());
    for expected in [
        "\"tabs\":[{\"pos\":2000,\"align\":\"center\",\"leader\":\"none\"},{\"pos\":8000,\"align\":\"right\",\"leader\":\"dot\"}]",
        "\"border\":{\"top\":{\"style\":\"single\",\"eighths\":6,\"space\":4,\"color\":\"FF0000\"},\"bottom\":{\"style\":\"double\",\"eighths\":12,\"space\":1}}",
        "\"shade\":\"FFFF00\"",
        "\"dropCap\":{\"lines\":3,\"margin\":false}",
        "\"columnBreak\":true",
        "\"columnLook\":{\"separator\":true,\"narrow\":\"last\"}",
        "\"textBoxes\":[{\"widthEmu\":2143125,\"heightEmu\":714375,\"wrap\":\"square\",\"border\":true,\"fill\":\"FFEEAA\",\"paragraphs\":[",
        "\"text\":\"상자 글\"",
        "\"styleName\":\"본문 강조\"",
        "\"styles\":[{\"name\":\"본문 강조\",\"font\":\"함초롬바탕\",\"sizeHalfPt\":28",
        "\"math\":\"<m:oMath><m:f>",
    ] {
        assert!(json.contains(expected), "{expected}: {json}");
    }
}

#[test]
fn a_value_the_formats_do_not_keep_rejects_the_save() {
    let good = payload();
    for (from, to) in [
        ("\"pos\":8000", "\"pos\":99999"),
        ("\"align\":\"right\"", "\"align\":\"diagonal\""),
        ("\"eighths\":6", "\"eighths\":1"),
        ("\"space\":4", "\"space\":99"),
        ("\"lines\":3", "\"lines\":1"),
        ("\"widthEmu\":2143125", "\"widthEmu\":5"),
        (
            "\"docColumnNarrow\":\"last\"",
            "\"docColumnNarrow\":\"middle\"",
        ),
        ("\"name\":\"본문 강조\"", "\"name\":\"제목 1\""),
        ("<m:oMath><m:f>", "<w:p><m:f>"),
        ("\"shade\":\"FFFF00\"", "\"shade\":\"yellow\""),
    ] {
        let bad = good.replace(from, to);
        assert!(save(&bad).is_err(), "{to} must be refused");
    }
}

#[test]
fn a_text_box_inside_a_text_box_rejects_the_save() {
    let nested = payload().replace(
        "\"paragraphs\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"상자 글\"}]}]",
        "\"paragraphs\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"상자 글\"}],\"textBoxes\":[{\"widthEmu\":720000,\"heightEmu\":720000,\"paragraphs\":[]}]}]",
    );
    assert!(save(&nested).is_err());
}

#[test]
fn a_flood_of_tab_stops_equations_and_text_boxes_rejects_the_save() {
    let stops: String = (0..65)
        .map(|i| format!("{{\"pos\":{}}}", 100 + i))
        .collect::<Vec<_>>()
        .join(",");
    let flood = format!(
        "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"paragraph\",\"tabs\":[{stops}],\"runs\":[{{\"text\":\"x\"}}]}}]}}"
    );
    assert!(save(&flood).is_err());
    let boxes: String = (0..257)
        .map(|_| "{\"widthEmu\":720000,\"heightEmu\":720000,\"paragraphs\":[]}")
        .collect::<Vec<_>>()
        .join(",");
    let flood = format!(
        "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"paragraph\",\"textBoxes\":[{boxes}],\"runs\":[{{\"text\":\"x\"}}]}}]}}"
    );
    assert!(save(&flood).is_err());
}

fn math_payload(math: &str) -> String {
    format!(
        "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"x\",\"math\":\"{math}\"}}]}}]}}"
    )
}

#[test]
fn an_equation_that_closes_the_paragraph_or_the_body_rejects_the_save() {
    for broken in [
        "<m:oMath></m:oMath></w:p></w:body><m:oMath>",
        "<m:oMath><m:r></m:oMath></m:r>",
        "<m:oMath></m:oMath><m:oMath></m:oMath>",
    ] {
        assert!(save(&math_payload(broken)).is_err(), "{broken}");
    }
    let bytes = save(&math_payload("<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>")).unwrap();
    assert!(reopen(&bytes).contains("\"math\":\"<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>\""));
}

#[test]
fn a_style_name_is_taken_without_the_white_space_around_it_and_a_reserved_one_rejects_the_save() {
    let styles = |name: &str| {
        format!(
            "{{\"targetFormat\":\"docx\",\"styles\":[{{\"name\":\"{name}\",\"lineSpacingKind\":\"exact\",\"lineSpacingTwips\":400}}],\"blocks\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"x\"}}]}}]}}"
        )
    };
    let json = reopen(&save(&styles("  내 스타일  ")).unwrap());
    assert!(
        json.contains("\"styles\":[{\"name\":\"내 스타일\""),
        "{json}"
    );
    assert!(
        json.contains("\"lineSpacingKind\":\"exact\",\"lineSpacingTwips\":400"),
        "{json}"
    );
    for reserved in [
        "Title",
        "Outline 2",
        "Heading_1",
        "제목-1",
        "toc 3",
        "caption",
    ] {
        assert!(save(&styles(reserved)).is_err(), "{reserved}");
    }
}
