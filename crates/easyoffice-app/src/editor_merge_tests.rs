//! Table merges through the editor's structured open and save (PD-130): the decode response
//! carries `merges`, an unedited structured save writes them back, and hostile `merges` from the
//! page are re-validated (dropped and reported) or rejected.

use easyoffice_formats::{
    CellMerge, DecodedContent, DocumentFormat, RichBlock, RichDocument, RichTable,
    UniversalFormatConverter,
};

use crate::EasyOfficeIpcRouter;

fn merge(row: u32, col: u32, row_span: u32, col_span: u32) -> CellMerge {
    CellMerge {
        row,
        col,
        row_span,
        col_span,
    }
}

fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(ToString::to_string).collect())
        .collect()
}

/// 3x3: a two-column header, a block of rows 1-2 in column 2, and a 2x2 block at the bottom left.
/// Covered cells are empty, as the writers leave them.
fn merged_table() -> RichTable {
    let rows = grid(&[&["머리", "", "끝"], &["a", "", "세로"], &["", "", ""]]);
    let (table, dropped) = RichTable::with_merges(
        rows,
        vec![merge(0, 0, 1, 2), merge(1, 2, 2, 1), merge(1, 0, 2, 2)],
    );
    assert_eq!(dropped, 0);
    table
}

fn plain_table() -> RichTable {
    RichTable::new(grid(&[&["x", "y"], &["z", "w"]]))
}

fn encode(table: RichTable, format: DocumentFormat) -> Vec<u8> {
    let doc = RichDocument {
        title: "병합".into(),
        blocks: vec![RichBlock::Table(table)],
        official_agency: None,
    };
    UniversalFormatConverter::encode_rich(&doc, format, None).unwrap()
}

fn decode_request(file: &str, bytes: &[u8]) -> String {
    format!(
        "{{\"action\":\"decode\",\"fileName\":\"{file}\",\"sourceBase64\":\"{}\"}}",
        EasyOfficeIpcRouter::encode_base64(bytes)
    )
}

fn decoded_tables(bytes: &[u8], file: &str) -> Vec<RichTable> {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(file)).unwrap();
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    doc.blocks
        .into_iter()
        .filter_map(|b| match b {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn converted_bytes(response: &str) -> Vec<u8> {
    let start = response.find("\"convertedBase64\":\"").unwrap() + 19;
    let end = start + response[start..].find('"').unwrap();
    EasyOfficeIpcRouter::decode_base64(&response[start..end]).unwrap()
}

/// Save payload an editor sends after opening and not editing: the decode response's `blocks`.
fn unedited_save_payload(decode_response: &str, target: &str) -> String {
    let blocks = decode_response.find("\"blocks\":[").unwrap();
    let body = &decode_response[blocks..decode_response.len() - 1];
    format!("{{\"targetFormat\":\"{target}\",\"title\":\"병합\",{body}}}")
}

fn save(payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap()
}

#[test]
fn decode_lists_merges_of_a_merged_table_only() {
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &decode_request("m.docx", &encode(merged_table(), DocumentFormat::Docx)),
    )
    .unwrap();
    assert!(
        json.contains(concat!(
            "\"merges\":[{\"row\":0,\"col\":0,\"rowSpan\":1,\"colSpan\":2},",
            "{\"row\":1,\"col\":0,\"rowSpan\":2,\"colSpan\":2},",
            "{\"row\":1,\"col\":2,\"rowSpan\":2,\"colSpan\":1}]"
        )),
        "{json}"
    );
    let plain = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &decode_request("p.docx", &encode(plain_table(), DocumentFormat::Docx)),
    )
    .unwrap();
    assert!(!plain.contains("merges"), "{plain}");
}

#[test]
fn unedited_structured_save_keeps_merges_in_docx_odt_and_hwpx() {
    let expected = merged_table();
    for (format, file) in [
        (DocumentFormat::Docx, "m.docx"),
        (DocumentFormat::Odt, "m.odt"),
        (DocumentFormat::Hwpx, "m.hwpx"),
    ] {
        let source = encode(expected.clone(), format);
        assert_eq!(
            decoded_tables(&source, file),
            std::slice::from_ref(&expected),
            "{file}"
        );
        let opened = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &decode_request(file, &source),
        )
        .unwrap();
        let saved = save(&unedited_save_payload(&opened, format.extension()));
        assert!(saved.contains("\"warnings\":[]"), "{file}: {saved}");
        assert_eq!(
            decoded_tables(&converted_bytes(&saved), file),
            std::slice::from_ref(&expected),
            "{file}"
        );
    }
}

#[test]
fn merges_are_ignored_by_formats_that_cannot_hold_them() {
    let opened = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &decode_request("m.docx", &encode(merged_table(), DocumentFormat::Docx)),
    )
    .unwrap();
    for target in ["md", "rtf", "txt", "pdf"] {
        let saved = save(&unedited_save_payload(&opened, target));
        assert!(saved.contains("\"status\":\"OK\""), "{target}");
    }
}

fn table_payload(merges: &str) -> String {
    format!(
        concat!(
            "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"table\",",
            "\"rows\":[[\"a\",\"\",\"\"],[\"\",\"\",\"\"],[\"\",\"\",\"\"]],\"merges\":[{}]}}]}}"
        ),
        merges
    )
}

fn merge_json(row: u64, col: u64, row_span: u64, col_span: u64) -> String {
    format!("{{\"row\":{row},\"col\":{col},\"rowSpan\":{row_span},\"colSpan\":{col_span}}}")
}

#[test]
fn hostile_merges_are_revalidated_dropped_and_reported() {
    let u32_max = u64::from(u32::MAX);
    let merges = [
        merge_json(0, 0, 2, 2),
        merge_json(1, 1, 2, 2),
        merge_json(2, 2, 1, 1),
        merge_json(0, 2, 0, 1),
        merge_json(5, 5, 1, 2),
        merge_json(0, 0, u32_max, u32_max),
        merge_json(u64::from(u32::MAX), 0, 2, 2),
        merge_json(0, 0, u64::from(u32::MAX) + 7, 1),
    ]
    .join(",");
    let saved = save(&table_payload(&merges));
    assert!(saved.contains("\"warnings\":[\""), "{saved}");
    assert!(saved.contains("병합"), "{saved}");
    let tables = decoded_tables(&converted_bytes(&saved), "t.docx");
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].merges, [merge(0, 0, 2, 2)]);
}

#[test]
fn malformed_merge_entries_reject_the_save() {
    for bad in [
        "5".to_string(),
        "\"x\"".to_string(),
        "{\"row\":0,\"col\":0,\"rowSpan\":2}".to_string(),
        "{\"row\":-1,\"col\":0,\"rowSpan\":2,\"colSpan\":2}".to_string(),
        "{\"row\":0.5,\"col\":0,\"rowSpan\":2,\"colSpan\":2}".to_string(),
        "{\"row\":\"0\",\"col\":0,\"rowSpan\":2,\"colSpan\":2}".to_string(),
        "{\"row\":1e30,\"col\":0,\"rowSpan\":2,\"colSpan\":2}".to_string(),
    ] {
        let result = EasyOfficeIpcRouter::dispatch("convert_document_format", &table_payload(&bad));
        assert!(result.is_err(), "{bad}");
    }
    let not_array = "{\"targetFormat\":\"docx\",\"blocks\":[{\"type\":\"table\",\"rows\":[[\"a\"]],\"merges\":{}}]}";
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", not_array).is_err());
}

#[test]
fn a_flood_of_merges_stops_at_the_payload_budget() {
    let one = merge_json(0, 0, 1, 1);
    let merges = vec![one; 500_001].join(",");
    let result = EasyOfficeIpcRouter::dispatch("convert_document_format", &table_payload(&merges));
    assert!(result.is_err());
}

#[test]
fn many_large_overlapping_merges_finish_quickly() {
    let rows: Vec<String> = (0..600)
        .map(|_| format!("[{}]", vec!["\"\""; 600].join(",")))
        .collect();
    let merges = vec![merge_json(0, 0, 600, 600); 20_000].join(",");
    let payload = format!(
        "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"table\",\"rows\":[{}],\"merges\":[{merges}]}}]}}",
        rows.join(",")
    );
    let started = std::time::Instant::now();
    let saved = save(&payload);
    assert!(started.elapsed().as_secs() < 20, "{:?}", started.elapsed());
    assert!(saved.contains("병합"), "overlaps are reported");
}
