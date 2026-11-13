use super::*;

fn parse(cell_xml: &str, shared: &[String]) -> Result<Vec<(u32, u32, String)>, EasyOfficeError> {
    let sheet = format!("<sheetData><row r=\"1\">{cell_xml}</row></sheetData>");
    let table = SharedStrings::from_strings(shared);
    scan_worksheet(&sheet, &table, &[], false, &mut DecodeBudget::new()).map(|scan| scan.cells)
}

#[test]
fn shared_string_index_resolves_and_corrupt_index_fails_closed() {
    let shared = vec!["가".to_string(), "나".to_string()];
    let ok = parse(r#"<c r="A1" t="s"><v>1</v></c>"#, &shared).unwrap();
    assert_eq!(ok, vec![(0, 0, "나".to_string())]);
    for bad in ["abc", "", "-1", "2", "99999999999999999999"] {
        let xml = format!(r#"<c r="A1" t="s"><v>{bad}</v></c>"#);
        assert!(
            matches!(
                parse(&xml, &shared),
                Err(EasyOfficeError::CorruptedContainer(_))
            ),
            "index {bad:?} must not fall back to another string"
        );
    }
}
