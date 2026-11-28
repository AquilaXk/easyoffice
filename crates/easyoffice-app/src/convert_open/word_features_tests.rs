//! The Word-style features (PD-395) through the structured save and the open JSON of the shell.

use super::*;

fn save(payload: &str) -> Result<Vec<u8>, EasyOfficeError> {
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", payload)?;
    Ok(EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap())
}

fn reopen(bytes: &[u8]) -> String {
    EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "속성.docx", bytes, ""),
    )
    .unwrap()
}

const PAYLOAD: &str = concat!(
    "{\"targetFormat\":\"docx\",\"title\":\"쓰기\",",
    "\"properties\":{\"title\":\"분기 보고서\",\"author\":\"홍길동\",\"subject\":\"3분기\",\"keywords\":\"매출\",\"comments\":\"초안\"},",
    "\"lineNumbers\":{\"start\":1,\"countBy\":5,\"distance\":240,\"restart\":\"newSection\"},",
    "\"blocks\":[",
    "{\"type\":\"paragraph\",\"caption\":true,\"runs\":[{\"text\":\"그림 \"},{\"text\":\"1\",\"fieldCode\":\"SEQ 그림 \\\\* ARABIC\"}]},",
    "{\"type\":\"paragraph\",\"sectionBreak\":\"oddPage\",\"runs\":[{\"text\":\"앞 \"},{\"text\":\"표지\",\"bookmarkStart\":\"표지\",\"bookmarkEnd\":\"표지\"}]},",
    "{\"type\":\"table\",\"headerRows\":1,\"rows\":[[{\"text\":\"머리\"}],[{\"text\":\"몸\"}]]},",
    "{\"type\":\"paragraph\",\"runs\":[{\"text\":\"끝\"}]}]}"
);

#[test]
fn the_word_features_survive_a_docx_save_and_the_open_json() {
    let bytes = save(PAYLOAD).unwrap();
    let json = reopen(&bytes);
    for expected in [
        "\"caption\":true",
        "\"fieldCode\":\"SEQ 그림 \\\\* ARABIC\"",
        "\"sectionBreak\":\"oddPage\"",
        "\"bookmarkStart\":\"표지\"",
        "\"bookmarkEnd\":\"표지\"",
        "\"headerRows\":1",
        "\"lineNumbers\":{\"start\":1,\"countBy\":5,\"distance\":240,\"restart\":\"newSection\"}",
        "\"properties\":{\"author\":\"홍길동\",\"subject\":\"3분기\",\"keywords\":\"매출\",\"comments\":\"초안\"}",
    ] {
        assert!(json.contains(expected), "{expected}: {json}");
    }
    assert!(
        json.contains("\"title\":\"분기 보고서\""),
        "the title the user wrote is the file's: {json}"
    );
}

#[test]
fn a_value_the_formats_do_not_keep_rejects_the_save() {
    for (from, to) in [
        ("SEQ 그림", "INCLUDETEXT x"),
        ("\"표지\",\"bookmarkEnd\"", "\"1 나쁨\",\"bookmarkEnd\""),
        ("oddPage", "sideways"),
        ("\"countBy\":5", "\"countBy\":0"),
        ("\"restart\":\"newSection\"", "\"restart\":\"never\""),
    ] {
        let bad = PAYLOAD.replace(from, to);
        assert!(save(&bad).is_err(), "{to} must be refused");
    }
}
