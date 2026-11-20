//! PD-265: the `.eobase` / `.eoboard` edit packages through `convert_document_format`
//! (`encode_package` writes one, `decode` of a package file reads one back, hostile input is refused).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{deflate_zlib, IeeeCrc32, StrictJsonValue};

const IPC: &str = "convert_document_format";

fn png_1x1() -> Vec<u8> {
    let comp = deflate_zlib(&[0u8, 37, 99, 235, 255]);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let chunk = |kind: &[u8; 4], data: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(b"IHDR", &ihdr, &mut out);
    chunk(b"IDAT", &comp, &mut out);
    chunk(b"IEND", &[], &mut out);
    out
}

fn dispatch(payload: &str) -> Result<String, String> {
    EasyOfficeIpcRouter::dispatch(IPC, payload).map_err(|err| err.to_string())
}

fn parse(json: &str) -> Vec<(String, StrictJsonValue)> {
    match StrictJsonValue::parse(json).unwrap() {
        StrictJsonValue::Object(entries) => entries,
        other => panic!("not an object: {other:?}"),
    }
}

fn text<'a>(entries: &'a [(String, StrictJsonValue)], key: &str) -> &'a str {
    StrictJsonValue::get_str(entries, key).unwrap_or_else(|| panic!("missing {key}"))
}

fn encode_request(kind: &str, model: &str, media: &str) -> String {
    let model = EasyOfficeIpcRouter::escape_json_str(model);
    format!(r#"{{"action":"encode_package","kind":"{kind}","model":"{model}","media":[{media}]}}"#)
}

fn decode_request(file_name: &str, bytes_b64: &str) -> String {
    format!(r#"{{"action":"decode","fileName":"{file_name}","sourceBase64":"{bytes_b64}"}}"#)
}

#[test]
fn a_base_model_survives_encode_and_decode_unchanged() {
    let model = r#"{"version":1,"tables":[{"id":"tbl-1","name":"고객 \"표\"","records":[]}]}"#;
    let written = parse(&dispatch(&encode_request("base", model, "")).unwrap());
    assert_eq!(text(&written, "format"), "eobase");
    let read = parse(
        &dispatch(&decode_request(
            "고객.eobase",
            text(&written, "convertedBase64"),
        ))
        .unwrap(),
    );
    assert_eq!(text(&read, "module"), "BASE");
    assert_eq!(text(&read, "model"), model);
}

#[test]
fn a_board_keeps_its_picture_bytes() {
    let png = EasyOfficeIpcRouter::encode_base64(&png_1x1());
    let media = format!(r#"{{"path":"media/img-1.png","base64":"{png}"}}"#);
    let model = r#"{"nodes":[{"id":"n1","media":"media/img-1.png"}]}"#;
    let written = parse(&dispatch(&encode_request("board", model, &media)).unwrap());
    let read = parse(
        &dispatch(&decode_request(
            "보드.eoboard",
            text(&written, "convertedBase64"),
        ))
        .unwrap(),
    );
    assert_eq!(text(&read, "module"), "BOARD");
    let listed = StrictJsonValue::get_field(&read, "media")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(listed.len(), 1);
    let item = listed[0].as_object().unwrap();
    assert_eq!(text(item, "path"), "media/img-1.png");
    assert_eq!(text(item, "mediaType"), "image/png");
    assert_eq!(text(item, "base64"), png);
}

#[test]
fn a_request_outside_the_format_is_refused() {
    assert!(
        dispatch(&encode_request("table", "{}", "")).is_err(),
        "unknown kind"
    );
    assert!(
        dispatch(&encode_request("base", "[1]", "")).is_err(),
        "model is not an object"
    );
    assert!(
        dispatch(&encode_request("base", "{\"a\":1,\"a\":2}", "")).is_err(),
        "duplicate key"
    );
    let png = EasyOfficeIpcRouter::encode_base64(&png_1x1());
    let media = format!(r#"{{"path":"media/a.png","base64":"{png}"}}"#);
    assert!(
        dispatch(&encode_request("base", "{}", &media)).is_err(),
        "a BASE has no pictures"
    );
    let bad = r#"{"path":"media/a.png","base64":"AAAA"}"#;
    assert!(
        dispatch(&encode_request("board", "{}", bad)).is_err(),
        "not a picture"
    );
    let traversal = format!(r#"{{"path":"media/../a.png","base64":"{png}"}}"#);
    assert!(
        dispatch(&encode_request("board", "{}", &traversal)).is_err(),
        "traversal"
    );
}

#[test]
fn a_package_opened_under_the_wrong_name_or_with_a_wrong_body_is_refused() {
    let written = parse(&dispatch(&encode_request("base", "{}", "")).unwrap());
    let b64 = text(&written, "convertedBase64");
    assert!(
        dispatch(&decode_request("보드.eoboard", b64)).is_err(),
        "a base package named as a board"
    );
    let garbage = EasyOfficeIpcRouter::encode_base64(b"PK\x03\x04 not really a package");
    assert!(dispatch(&decode_request("a.eobase", &garbage)).is_err());
    assert!(dispatch(&decode_request("a.eobase", "")).is_err());
}

#[test]
fn a_package_cut_in_the_middle_is_refused() {
    let written = parse(&dispatch(&encode_request("base", "{\"a\":1}", "")).unwrap());
    let bytes = EasyOfficeIpcRouter::decode_base64(text(&written, "convertedBase64")).unwrap();
    let cut = EasyOfficeIpcRouter::encode_base64(&bytes[..bytes.len() / 2]);
    assert!(dispatch(&decode_request("a.eobase", &cut)).is_err());
}
