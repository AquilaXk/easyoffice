//! The JSON array stream reader scans each byte once, however the chunks cut the objects.

use super::*;

fn google_object(text: &str) -> String {
    format!(r#"{{"candidates":[{{"content":{{"parts":[{{"text":"{text}"}}]}}}}]}}"#)
}

#[test]
fn a_json_array_object_that_arrives_in_pieces_is_scanned_once() {
    let object = google_object(&"가\\\"}".repeat(20_000));
    let body = format!("[{object},{object}]");
    let mut reader = JsonArrayReader::default();
    let mut texts = 0;
    for chunk in body.as_bytes().chunks(64) {
        texts += reader.push(ProviderKind::Google, chunk).unwrap().len();
    }
    assert_eq!(texts, 2, "both objects came out");
    // Scanning again from the start of the object on every chunk would examine about
    // len/64 times len bytes in all.
    assert!(
        reader.examined <= 2 * body.len(),
        "{} bytes examined for a body of {}",
        reader.examined,
        body.len()
    );
}

#[test]
fn an_object_with_braces_in_strings_and_a_split_escape_still_ends_at_its_own_brace() {
    let body = format!("[{},{}]", google_object(r#"a}{\\\"}"#), google_object("b"));
    let mut reader = JsonArrayReader::default();
    let mut deltas = Vec::new();
    for byte in body.as_bytes() {
        deltas.extend(reader.push(ProviderKind::Google, std::slice::from_ref(byte)).unwrap());
    }
    assert_eq!(deltas.len(), 2);
}
