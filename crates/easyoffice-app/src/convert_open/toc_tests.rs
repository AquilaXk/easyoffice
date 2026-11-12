//! Table-of-contents paragraphs, bookmarks and their links through the save and open requests.

use super::tests::{json_field, request};
use super::*;

#[test]
fn a_table_of_contents_survives_odt_save_and_reopen_and_a_bad_level_rejects_the_save() {
    let payload = concat!(
        "{\"targetFormat\":\"odt\",\"blocks\":[",
        "{\"type\":\"paragraph\",\"toc\":0,\"runs\":[{\"text\":\"목차\"}]},",
        "{\"type\":\"paragraph\",\"toc\":2,\"runs\":[{\"text\":\"세부\"}]},",
        "{\"type\":\"paragraph\",\"heading\":2,\"runs\":[{\"text\":\"세부\"}]}]}"
    );
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let bytes = EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "목차.odt", &bytes, ""),
    )
    .unwrap();
    assert!(json.contains("\"toc\":0"), "{json}");
    assert!(json.contains("\"toc\":2"), "{json}");
    assert!(json.contains("\"heading\":2"), "{json}");
    let bad = payload.replace("\"toc\":2", "\"toc\":10");
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &bad).is_err());
}

#[test]
fn a_toc_bookmark_and_its_links_survive_docx_save_and_reopen_and_bad_ones_reject_the_save() {
    let payload = concat!(
        "{\"targetFormat\":\"docx\",\"blocks\":[",
        "{\"type\":\"paragraph\",\"toc\":1,\"runs\":[{\"text\":\"개요\",\"link\":\"#_Toc7\"}]},",
        "{\"type\":\"paragraph\",\"heading\":1,\"bookmark\":7,\"runs\":[{\"text\":\"개요\"}]}]}"
    );
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let bytes = EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "목차.docx", &bytes, ""),
    )
    .unwrap();
    assert!(json.contains("\"link\":\"#_Toc7\""), "{json}");
    assert!(json.contains("\"bookmark\":7"), "{json}");
    for (from, to) in [
        ("\"bookmark\":7", "\"bookmark\":0"),
        ("\"bookmark\":7", "\"bookmark\":4294967296"),
        ("#_Toc7", "#top"),
        ("#_Toc7", "#_Toc07"),
    ] {
        let bad = payload.replacen(from, to, 1);
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &bad).is_err(),
            "{to}"
        );
    }
}

#[test]
fn a_toc_page_and_its_fill_survive_docx_save_and_reopen_and_bad_pages_reject_the_save() {
    let payload = concat!(
        "{\"targetFormat\":\"docx\",\"blocks\":[",
        "{\"type\":\"paragraph\",\"toc\":1,\"tocPage\":4,\"tocDots\":true,\"runs\":[{\"text\":\"개요\",\"link\":\"#_Toc7\"}]},",
        "{\"type\":\"paragraph\",\"heading\":1,\"bookmark\":7,\"runs\":[{\"text\":\"개요\"}]}]}"
    );
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let bytes = EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "목차.docx", &bytes, ""),
    )
    .unwrap();
    assert!(json.contains("\"tocPage\":4,\"tocDots\":true"), "{json}");
    assert!(
        json.contains("\"text\":\"개요\",\"link\":\"#_Toc7\""),
        "{json}"
    );
    for (from, to) in [
        ("\"tocPage\":4", "\"tocPage\":0"),
        ("\"tocPage\":4", "\"tocPage\":10000000"),
        ("\"toc\":1,\"tocPage\":4", "\"toc\":0,\"tocPage\":4"),
        ("\"tocDots\":true", "\"tocDots\":1"),
    ] {
        let bad = payload.replacen(from, to, 1);
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &bad).is_err(),
            "{to}"
        );
    }
}
